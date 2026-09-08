#[cfg(any(test, feature = "benchmark-helpers"))]
mod aggregate_diagnostics;
mod candidate;
mod host;
mod preconsensus;

pub use self::host::{
    HostBlock, HostCommit, HostCommittedState, HostDepositResult, HostExecution,
    HostExecutionPhase, HostExecutionResponse, HostNoteSeizureResult, HostTxResponse,
    HostWithdrawal,
};
#[cfg(any(test, feature = "fuzzing"))]
pub use self::preconsensus::decode_batch_item_for_fuzz;
pub use self::preconsensus::{
    ProposalArtifactSidecar, ProposalArtifactSidecarRecord, ProposalArtifactSidecarRecordEntry,
};
pub use candidate::{candidate_digest_from_hashes, sidecar_commitment, CandidateEnvelope};

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ark_groth16::PreparedVerifyingKey;
use async_trait::async_trait;
use cnidarium::{ArcStateDeltaExt, Snapshot, StateDelta, StateRead, StateWrite, Storage};
use cnidarium_component::Component;
use decaf377::{Bls12_377, Fq};
use ibc_types::core::connection::ChainId;
use jmt::RootHash;
use prost::bytes::Bytes;
use prost::Message as _;
#[cfg(any(test, feature = "benchmark-helpers"))]
use serde::{Deserialize, Serialize};
use shieldd_sdk_compact_block::{
    component::{CompactBlockManager, RoutingManager as _},
    StatePayload,
};
use shieldd_sdk_compliance::params::StateReadExt as _;
use shieldd_sdk_compliance::registry::ComplianceRegistryRead as _;
use shieldd_sdk_compliance::Compliance;
use shieldd_sdk_fee::component::{
    clear_block_fee_price_cache, FeeComponent, FeePay as _, StateReadExt as _, StateWriteExt as _,
};
use shieldd_sdk_fee::{Fee, Gas, GasPrices};
use shieldd_sdk_ibc::component::Ibc;
use shieldd_sdk_ibc::StateReadExt as _;
use shieldd_sdk_proof_aggregation::{
    aggregate_family, app_verify_accepted_join_projection_core, app_verify_family_code,
    app_verify_family_count_core, app_verify_join_acceptance_core, app_verify_plan_identity_core,
    app_verify_plan_ids_core, app_verify_plan_padding_core, app_verify_preflight_core,
    app_verify_reduce_core, pad_items_to_power_of_two, prepare_verify_inputs, srs_id,
    verify_shipping_family_aggregate, AggregateBundle, AggregateStatement,
    AppVerifyAcceptedJoinProjectionError, AppVerifyCallId, AppVerifyCallResult,
    AppVerifyExpectedCall, AppVerifyPlanError, AppVerifyPlannerIndexedExecutedRecord,
    AppVerifyPreflightError, AppVerifyReductionError, AppVerifyShippingCall, DevSrs,
    FamilyAggregate, ProofFamilyId, ShippingAggregateVerification, AGGREGATE_PROTOCOL_VERSION,
};
use shieldd_sdk_proof_params::{
    batch::{self, BatchItem, VerifiedBatchItem},
    DeployedProofKey,
};
use shieldd_sdk_proto::core::app::v1::TransactionsByHeightResponse;
use shieldd_sdk_proto::{DomainType, StateWriteProto as _};
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_sct::component::sct::Sct;
use shieldd_sdk_sct::component::source::SourceContext as _;
use shieldd_sdk_sct::component::tree::SctManager as _;
use shieldd_sdk_sct::component::tree::SctRead as _;
use shieldd_sdk_sct::component::StateReadExt as _;
use shieldd_sdk_sct::epoch::Epoch;
use shieldd_sdk_sct::{CommitmentSource, Nullifier};
use shieldd_sdk_shielded_pool::component::{
    note_reshape_check_stateless_and_extract, shielded_host_withdrawal_check_stateless_and_extract,
    shielded_ics20_withdrawal_check_stateless_and_extract, transfer_check_stateless_and_extract,
    NoteManager as _, ShieldedPool, StateReadExt as _, StateWriteExt as _,
};
use shieldd_sdk_shielded_pool::VolumeNullifier;
use shieldd_sdk_transaction::gas::GasCost as _;
use shieldd_sdk_transaction::{
    Action, FeeFunding, Transaction, TransactionBody, TransactionParameters,
};
use shieldd_sdk_txhash::TransactionContext;
use tendermint::abci::{self, Event};
use tendermint::v0_37::abci::{request, response};
use tendermint::{account, block, chain, AppHash, Hash, Time};
use tracing::{instrument, Instrument};

use crate::action_handler::transaction::{
    append_transaction_audit_effects, check_and_execute, check_historical_with_context,
    prepare_candidate_read, prepare_candidate_read_blocking, supports_parallel_prepare,
    verify_historical_nullifier_proof, HistoricalCheckContext, PreparedCandidateRead,
};
use crate::action_handler::AppActionHandler;
use crate::block_tx_indexing::BlockTxIndexingMode;
use crate::genesis::AppState;

use crate::params::AppParameters;
use crate::stateless_cache::{
    CacheEntry, HistoricalValidationStamp, StatelessCache, TxArtifact, VerifiedTxArtifact,
};
use crate::{metrics, ShielddHost};
use sha2::Digest as _;
#[cfg(feature = "benchmark-helpers")]
use shieldd_sdk_ibc::benchmarking::{record_inbound_stage, InboundStage};

pub mod state_key;

/// The inter-block state being written to by the application.
type InterBlockState = Arc<StateDelta<Snapshot>>;

/// The maximum size of a CometBFT block payload (1MB)
pub const MAX_BLOCK_TXS_PAYLOAD_BYTES: usize = 1024 * 1024;

/// The maximum size of a single individual transaction (96KB).
pub const MAX_TRANSACTION_SIZE_BYTES: usize = 96 * 1024;

/// The maximum number of transactions in one proposal candidate set.
pub const MAX_BLOCK_TX_COUNT: usize = 4_096;

/// Maximum number of body actions plus an optional fee-funding action.
pub const MAX_TRANSACTION_ACTION_COUNT: usize = 512;

/// Maximum number of proof-bound nullifiers in one transaction.
pub const MAX_TRANSACTION_NULLIFIER_COUNT: usize = 256;

/// The maximum number of proof-bound nullifiers in one block.
pub const MAX_BLOCK_NULLIFIER_COUNT: usize =
    shieldd_sdk_sct::component::tree::MAX_NULLIFIERS_PER_BLOCK;

/// The maximum size of the evidence portion of a block (30KB).
pub const MAX_EVIDENCE_SIZE_BYTES: usize = 30 * 1024;

fn extract_fee_funding_proof_item(
    fee_funding: &FeeFunding,
    context: &TransactionContext,
) -> Result<BatchItem> {
    transfer_check_stateless_and_extract(
        &fee_funding.transfer,
        context,
        shieldd_sdk_shielded_pool::TransferProofContext::FeeFunding,
    )
    .context("fee funding transfer stateless extraction failed")
}

const MAX_PADDED_PROOF_COUNT: usize = 32_768;
fn shipping_srs() -> Result<DevSrs> {
    // Insecure isolated integration only; never enable in production.
    #[cfg(feature = "orbis-dev-srs")]
    {
        return Ok(DevSrs::default());
    }
    #[cfg(all(not(feature = "orbis-dev-srs"), any(test, feature = "fuzzing")))]
    {
        return Ok(DevSrs::default());
    }
    #[cfg(not(any(test, feature = "fuzzing", feature = "orbis-dev-srs")))]
    {
        shieldd_sdk_proof_aggregation::load_active_production_srs()
    }
}

fn shipping_srs_for_id(requested_id: &[u8]) -> Result<DevSrs> {
    // Insecure isolated integration only; never enable in production.
    #[cfg(feature = "orbis-dev-srs")]
    {
        anyhow::ensure!(
            requested_id == shieldd_sdk_proof_aggregation::DEFAULT_DEV_SRS_ID.as_slice(),
            "Orbis integration SnarkPack SRS id mismatch"
        );
        return Ok(DevSrs::default());
    }
    #[cfg(all(not(feature = "orbis-dev-srs"), any(test, feature = "fuzzing")))]
    {
        anyhow::ensure!(
            requested_id == shieldd_sdk_proof_aggregation::DEFAULT_DEV_SRS_ID.as_slice(),
            "test/fuzz SnarkPack SRS id mismatch"
        );
        return Ok(DevSrs::default());
    }
    #[cfg(not(any(test, feature = "fuzzing", feature = "orbis-dev-srs")))]
    {
        shieldd_sdk_proof_aggregation::load_production_srs_for_id(requested_id)
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(not(any(test, feature = "benchmark-helpers")), allow(dead_code))]
struct AggregateDebugRow {
    tx_id: String,
    action_index: Option<usize>,
    family_local_index: usize,
    public_inputs: Vec<Fq>,
}

#[derive(Clone, Debug)]
struct AggregateDebugSegmentFamily {
    family_index: usize,
    family_id: ProofFamilyId,
    rows: Vec<AggregateDebugRow>,
}

fn action_family_id(action: &Action) -> Option<ProofFamilyId> {
    match action {
        Action::Transfer(_) => Some(ProofFamilyId::Transfer),
        Action::NoteReshape(note_reshape) => {
            Some(ProofFamilyId::NoteReshape(note_reshape.body.family_id))
        }
        Action::ShieldedIcs20Withdrawal(withdrawal) => Some(
            ProofFamilyId::ShieldedIcs20Withdrawal(withdrawal.body.family_id),
        ),
        Action::ShieldedHostWithdrawal(withdrawal) => Some(ProofFamilyId::ShieldedIcs20Withdrawal(
            withdrawal.body.family_id,
        )),
        _ => None,
    }
}

fn proof_verification_key_for_family(
    family_id: ProofFamilyId,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    match family_id {
        ProofFamilyId::Transfer => shieldd_sdk_proof_params::transfer_proof_verification_key(),
        ProofFamilyId::NoteReshape(family_id) => family_id.proof_verification_key(),
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => family_id.proof_verification_key(),
    }
}

fn deployed_key_for_family(family_id: ProofFamilyId) -> DeployedProofKey {
    match family_id {
        ProofFamilyId::Transfer => DeployedProofKey::Transfer,
        ProofFamilyId::NoteReshape(family_id) => family_id.deployed_proof_key(),
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => family_id.deployed_proof_key(),
    }
}

fn proof_family_label(family_id: ProofFamilyId) -> &'static str {
    match family_id {
        ProofFamilyId::Transfer => shieldd_sdk_shielded_pool::TRANSFER_PROOF_LABEL,
        ProofFamilyId::NoteReshape(family_id) => family_id.label(),
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => family_id.label(),
    }
}

fn proof_family_batch_verify_stage(family_id: ProofFamilyId) -> &'static str {
    match family_id {
        ProofFamilyId::Transfer => "transfer_batch_verify",
        ProofFamilyId::NoteReshape(_) => "note_reshape_batch_verify",
        ProofFamilyId::ShieldedIcs20Withdrawal(_) => "shielded_ics20_withdrawal_batch_verify",
    }
}

#[cfg(any(test, feature = "benchmark-helpers"))]
use aggregate_diagnostics::maybe_write_aggregate_debug_dump;

#[cfg(not(any(test, feature = "benchmark-helpers")))]
fn maybe_write_aggregate_debug_dump(
    _phase: &str,
    _segment_index: usize,
    _family_index: usize,
    _family_id: ProofFamilyId,
    _rows: &[AggregateDebugRow],
    _padded_public_inputs: &[Vec<Fq>],
    _aggregate: Option<&FamilyAggregate>,
) {
}
const AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES: u64 = 8 * 1024;
const AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER: usize = 24 * 1024;
#[cfg(any(test, feature = "benchmark-helpers"))]
const MAX_CONCURRENT_AGGREGATE_SEGMENTS: usize = 2;
const MAX_CONCURRENT_AGGREGATE_VERIFY_CALLS: usize = 4;

async fn drain_joinset_results<T: Send + 'static>(
    tasks: &mut tokio::task::JoinSet<Result<T>>,
    panic_context: &str,
) -> Result<Vec<T>> {
    let mut values = Vec::new();
    let mut first_error = None;
    while let Some(result) = tasks.join_next().await {
        match result
            .with_context(|| panic_context.to_owned())
            .and_then(|result| result)
        {
            Ok(value) => values.push(value),
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(values)
}

fn max_transaction_size_bytes() -> usize {
    #[cfg(any(test, feature = "benchmark-helpers"))]
    {
        return aggregate_diagnostics::max_transaction_size_bytes_override();
    }
    #[cfg(not(any(test, feature = "benchmark-helpers")))]
    {
        MAX_TRANSACTION_SIZE_BYTES
    }
}

fn truncate_prepare_candidates<T>(candidates: &mut Vec<T>) {
    candidates.truncate(MAX_BLOCK_TX_COUNT);
}

fn process_proposal_tx_count_allowed(tx_count: usize) -> bool {
    tx_count <= MAX_BLOCK_TX_COUNT
}

fn prepare_proposal_payload_limit(max_tx_bytes: i64) -> u64 {
    u64::try_from(max_tx_bytes)
        .unwrap_or(0)
        .min(MAX_BLOCK_TXS_PAYLOAD_BYTES as u64)
}

fn process_proposal_payload_size_allowed(payload_size: usize) -> bool {
    payload_size <= MAX_BLOCK_TXS_PAYLOAD_BYTES
}

fn block_nullifier_count_allowed(nullifier_count: usize) -> bool {
    nullifier_count <= MAX_BLOCK_NULLIFIER_COUNT
}

fn transaction_size_allowed(transaction_size: usize) -> bool {
    transaction_size <= max_transaction_size_bytes()
}

#[cfg(any(test, feature = "benchmark-helpers"))]
pub(crate) fn benchmark_zero_timestamp_allowed() -> bool {
    aggregate_diagnostics::zero_timestamp_allowed()
}

struct PrepareBlockLocalState {
    seen_nullifiers: BTreeSet<Nullifier>,
    seen_volume_nullifiers: BTreeSet<VolumeNullifier>,
    remaining_nullifier_capacity: usize,
}

impl Default for PrepareBlockLocalState {
    fn default() -> Self {
        Self {
            seen_nullifiers: BTreeSet::new(),
            seen_volume_nullifiers: BTreeSet::new(),
            remaining_nullifier_capacity: MAX_BLOCK_NULLIFIER_COUNT,
        }
    }
}

#[derive(Clone, Debug)]
#[cfg(any(test, feature = "benchmark-helpers"))]
struct BenchBlockContext {
    height: block::Height,
    time: Time,
    chain_id: chain::Id,
    proposer_address: account::Id,
    next_validators_hash: Hash,
    app_hash: AppHash,
}

#[derive(Clone)]
enum CandidateData {
    Decoded(Arc<Transaction>),
    ExtractedArtifact(Arc<TxArtifact>),
    VerifiedArtifact(Arc<VerifiedTxArtifact>),
}

#[derive(Clone)]
struct Candidate {
    hash: [u8; 32],
    bytes: Bytes,
    data: CandidateData,
}

impl Candidate {
    fn tx(&self) -> &Arc<Transaction> {
        match &self.data {
            CandidateData::Decoded(tx) => tx,
            CandidateData::ExtractedArtifact(artifact) => &artifact.tx,
            CandidateData::VerifiedArtifact(artifact) => artifact.tx(),
        }
    }

    fn artifact(&self) -> Option<Arc<TxArtifact>> {
        match &self.data {
            CandidateData::ExtractedArtifact(artifact) => Some(artifact.clone()),
            CandidateData::VerifiedArtifact(artifact) => Some(artifact.extracted()),
            CandidateData::Decoded(_) => None,
        }
    }

    fn verified_artifact(&self) -> Option<Arc<VerifiedTxArtifact>> {
        match &self.data {
            CandidateData::VerifiedArtifact(artifact) => Some(artifact.clone()),
            CandidateData::Decoded(_) | CandidateData::ExtractedArtifact(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg(any(test, feature = "benchmark-helpers"))]
pub struct ExecutionBlockProfile {
    pub block_tx_count: usize,
    pub begin_block_ms: f64,
    pub deliver_txs_wall_ms: f64,
    pub end_block_ms: f64,
    pub commit_ms: f64,
    pub execute_tx_ms: f64,
}

#[derive(Clone)]
struct AggregateExpectedVerifySegment {
    segment_index: usize,
    family_index: usize,
    family_id: ProofFamilyId,
    items: Vec<BatchItem>,
    debug_rows: Vec<AggregateDebugRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AggregateVerifyCallId {
    order_index: usize,
    segment_index: usize,
    family_index: usize,
    family_id: ProofFamilyId,
}

#[derive(Clone)]
struct AggregateVerifyCall {
    id: AggregateVerifyCallId,
    shipping_call: AppVerifyShippingCall,
    statement: AggregateStatement,
    aggregate: FamilyAggregate,
    srs: DevSrs,
    debug_rows: Vec<AggregateDebugRow>,
    padded_public_inputs: Vec<Vec<Fq>>,
    items: Vec<BatchItem>,
}

#[derive(Clone, Default)]
struct AggregateVerifyPlan {
    calls: Vec<AggregateVerifyCall>,
}

#[derive(Clone)]
struct AggregateVerifyCallOutcome {
    id: AggregateVerifyCallId,
    shipping_verification: ShippingAggregateVerification,
    items: Vec<BatchItem>,
}

fn aggregate_verify_app_call_id(id: AggregateVerifyCallId) -> AppVerifyCallId {
    AppVerifyCallId {
        order_index: id.order_index,
        segment_index: id.segment_index,
        family_index: id.family_index,
        family: app_verify_family_code(id.family_id),
    }
}

fn require_no_rejected_joined_calls(rejected_calls: Vec<AppVerifyCallId>) -> Result<()> {
    let rejected_count = rejected_calls.len();
    if !app_verify_join_acceptance_core(rejected_calls) {
        anyhow::bail!(
            "aggregate verification join retained {} rejected call(s) after reducer acceptance",
            rejected_count
        );
    }
    Ok(())
}

impl AggregateVerifyCallOutcome {
    fn result(&self) -> Result<AggregateVerifyCallResult> {
        let shipping_result = self.shipping_verification.shipping_result();
        anyhow::ensure!(
            shipping_result.result.id
                == AppVerifyCallId {
                    order_index: self.id.order_index,
                    segment_index: self.id.segment_index,
                    family_index: self.id.family_index,
                    family: app_verify_family_code(self.id.family_id),
                },
            "aggregate verification result identity does not match its planned call"
        );
        Ok(AggregateVerifyCallResult {
            id: self.id,
            accepted: shipping_result.result.accepted,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AggregateVerifyCallResult {
    id: AggregateVerifyCallId,
    accepted: bool,
}

#[derive(Clone, Debug, Default)]
struct AggregateVerifyReduction {
    rejected_calls: Vec<AggregateVerifyCallId>,
}

impl AggregateVerifyReduction {
    fn acceptance_result(&self) -> Result<()> {
        if self.rejected_calls.is_empty() {
            return Ok(());
        }

        let details = self
            .rejected_calls
            .iter()
            .map(|id| {
                format!(
                    "segment={} family_index={} family={:?}",
                    id.segment_index, id.family_index, id.family_id
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!("SnarkPack verification rejected aggregate bundle ({details})")
    }
}

#[derive(Clone, Debug)]
struct AggregateBundleFamilyEstimate {
    family_id: ProofFamilyId,
    real_count: u32,
    padded_count: u32,
    aggregate_proof_bytes: usize,
}

#[derive(Clone)]
pub(crate) struct CachedProposalAggregate {
    height: u64,
    included_tx_count: usize,
    proposal_txs_digest: [u8; 32],
    proposal_segment_tx_count: Option<usize>,
    bundle_tx_bytes: Option<Bytes>,
}

#[derive(Clone, Debug)]
pub struct CheckTxSharedContext {
    pub(crate) sct_base_position: shieldd_sdk_tct::Position,
    pub(crate) base_gas_prices: GasPrices,
    pub(crate) historical_check_context: Arc<HistoricalCheckContext>,
}

impl CheckTxSharedContext {
    pub async fn load(snapshot: &Snapshot) -> Result<Self> {
        let sct_base_position = snapshot
            .get_sct()
            .await
            .position()
            .expect("state commitment tree is not full");
        let base_gas_prices = snapshot.get_gas_prices().await?;
        let historical_check_context =
            Arc::new(HistoricalCheckContext::load_for_checktx(snapshot).await?);

        Ok(Self {
            sct_base_position,
            base_gas_prices,
            historical_check_context,
        })
    }

    fn gas_prices_for_fee(&self, fee: Fee) -> Result<GasPrices> {
        anyhow::ensure!(
            fee.asset_id() == *shieldd_sdk_asset::BASE_ASSET_ID,
            "only base-asset fees are supported, found {}",
            fee.asset_id(),
        );
        Ok(self.base_gas_prices)
    }
}

#[derive(Clone, Debug, Default)]
struct BlockSctAppendLog {
    base_position: Option<shieldd_sdk_tct::Position>,
    next_offset: u64,
    entries: Vec<(shieldd_sdk_tct::Position, StatePayload)>,
}

impl BlockSctAppendLog {
    async fn reserve_positions<S: shieldd_sdk_sct::component::tree::SctRead>(
        &mut self,
        state: &S,
        payloads: Vec<StatePayload>,
    ) -> Result<Vec<(shieldd_sdk_tct::Position, StatePayload)>> {
        #[cfg(feature = "benchmark-helpers")]
        let reserve_start = Instant::now();
        if payloads.is_empty() {
            return Ok(Vec::new());
        }

        let base_position = match self.base_position {
            Some(position) => position,
            None => {
                let position = state
                    .get_sct_position()
                    .await?
                    .expect("state commitment tree is not full");
                self.base_position = Some(position);
                position
            }
        };

        let used_in_block = base_position.commitment() as u64 + self.next_offset;
        anyhow::ensure!(
            used_in_block.saturating_add(payloads.len() as u64)
                <= shieldd_sdk_sct::component::tree::SCT_BLOCK_COMMITMENT_CAPACITY as u64,
            "SCT block commitment capacity exceeded"
        );
        let base_position_u64: u64 = base_position.into();
        let start = base_position_u64
            .checked_add(self.next_offset)
            .context("SCT position overflow while reserving block commitments")?;
        let mut positioned = Vec::with_capacity(payloads.len());
        for (offset, payload) in payloads.into_iter().enumerate() {
            let position = shieldd_sdk_tct::Position::from(start + offset as u64);
            positioned.push((position, payload));
        }
        self.next_offset += positioned.len() as u64;

        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::DeferredSctReserve, reserve_start.elapsed());

        Ok(positioned)
    }

    fn append_positioned(&mut self, entries: Vec<(shieldd_sdk_tct::Position, StatePayload)>) {
        self.entries.extend(entries);
    }

    fn take_entries(&mut self) -> Vec<(shieldd_sdk_tct::Position, StatePayload)> {
        self.base_position = None;
        self.next_offset = 0;
        std::mem::take(&mut self.entries)
    }

    fn clear(&mut self) {
        self.base_position = None;
        self.next_offset = 0;
        self.entries.clear();
    }
}

/// The Shieldd application, written as a bundle of [`Component`]s.
///
/// The [`App`] is not a [`Component`], but
/// it constructs the components and exposes a [`commit`](App::commit) that
/// commits the changes to the persistent storage and resets its subcomponents.
pub struct App {
    state: InterBlockState,
    committed_snapshot: Snapshot,
    snapshot_version: u64,
    block_tx_indexing_mode: BlockTxIndexingMode,
    deferred_block_transactions: Vec<shieldd_sdk_proto::core::transaction::v1::Transaction>,
    pending_sct_append_log: BlockSctAppendLog,
    checktx_shared_context: Option<Arc<CheckTxSharedContext>>,
    aggregate_retry_cache: Option<CachedProposalAggregate>,
    proposal_segment_tx_count: Option<usize>,
}

impl App {
    #[cfg(any(test, feature = "benchmark-helpers"))]
    async fn benchmark_block_context(&self) -> Result<BenchBlockContext> {
        let next_height = self.state.get_block_height().await?.saturating_add(1);
        let height = block::Height::try_from(next_height)
            .context("converting execution benchmark height")?;
        let current_time = self.state.get_current_block_timestamp().await?;
        let time = current_time
            .checked_add(Duration::from_secs(1))
            .unwrap_or(current_time);
        let chain_id = chain::Id::try_from(self.state.get_chain_id().await?)
            .context("parsing execution benchmark chain id")?;
        let base_snapshot = self.committed_snapshot.clone();
        let app_hash = AppHash::try_from(base_snapshot.root_hash().await?.0.to_vec())
            .context("converting execution benchmark app hash")?;

        Ok(BenchBlockContext {
            height,
            time,
            chain_id,
            proposer_address: account::Id::new([0u8; 20]),
            next_validators_hash: Hash::None,
            app_hash,
        })
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    fn begin_block_request_from_context(context: &BenchBlockContext) -> request::BeginBlock {
        request::BeginBlock {
            hash: Hash::None,
            header: block::Header {
                version: block::header::Version { block: 11, app: 1 },
                chain_id: context.chain_id.clone(),
                height: context.height,
                time: context.time,
                last_block_id: None,
                last_commit_hash: None,
                data_hash: None,
                validators_hash: context.next_validators_hash,
                next_validators_hash: context.next_validators_hash,
                consensus_hash: Hash::None,
                app_hash: context.app_hash.clone(),
                last_results_hash: None,
                evidence_hash: None,
                proposer_address: context.proposer_address,
            },
            last_commit_info: abci::types::CommitInfo {
                round: 0u8.into(),
                votes: Vec::new(),
            },
            byzantine_validators: Vec::new(),
        }
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    fn process_proposal_request_from_envelope(
        context: &BenchBlockContext,
        envelope: &CandidateEnvelope,
    ) -> request::ProcessProposal {
        let mut txs = envelope
            .txs
            .iter()
            .cloned()
            .map(Bytes::from)
            .collect::<Vec<_>>();
        if let Some(bundle_tx_bytes) = &envelope.aggregate_bundle_tx_bytes {
            txs.push(Bytes::from(bundle_tx_bytes.clone()));
        }

        request::ProcessProposal {
            txs,
            proposed_last_commit: None,
            misbehavior: Vec::new(),
            hash: Hash::None,
            height: context.height,
            time: context.time,
            next_validators_hash: context.next_validators_hash,
            proposer_address: context.proposer_address,
        }
    }

    fn ensure_user_tx_has_no_unsupported_internal_actions(tx: &Transaction) -> Result<()> {
        let _ = tx;
        Ok(())
    }

    pub(crate) fn ensure_user_tx_has_no_internal_actions(tx: &Transaction) -> Result<()> {
        Self::ensure_user_tx_has_no_unsupported_internal_actions(tx)?;
        anyhow::ensure!(
            !tx.contains_aggregate_bundle_action(),
            "Aggregate bundle actions are not permitted in user-submitted transactions"
        );
        Ok(())
    }

    fn proof_family_ids() -> Vec<ProofFamilyId> {
        let mut family_ids = vec![ProofFamilyId::Transfer];
        family_ids.extend(
            shieldd_sdk_shielded_pool::NOTE_RESHAPE_FAMILY_SPECS
                .into_iter()
                .map(|spec| ProofFamilyId::NoteReshape(spec.id)),
        );
        family_ids.extend(
            shieldd_sdk_shielded_pool::SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS
                .into_iter()
                .map(|spec| ProofFamilyId::ShieldedIcs20Withdrawal(spec.id)),
        );
        family_ids
    }

    fn total_proof_count(proof_items: &BTreeMap<ProofFamilyId, Vec<BatchItem>>) -> usize {
        proof_items.values().map(Vec::len).sum()
    }

    fn empty_proof_items() -> BTreeMap<ProofFamilyId, Vec<BatchItem>> {
        Self::proof_family_ids()
            .into_iter()
            .map(|family_id| (family_id, Vec::new()))
            .collect()
    }

    fn merge_artifact_proof_items(
        artifacts: &[Arc<TxArtifact>],
    ) -> BTreeMap<ProofFamilyId, Vec<BatchItem>> {
        let mut proof_items = Self::empty_proof_items();

        for artifact in artifacts {
            for (family_id, items) in &artifact.proof_items {
                proof_items
                    .get_mut(family_id)
                    .expect("proof family exists")
                    .extend(items.iter().cloned());
            }
        }

        proof_items
    }

    fn aggregate_debug_rows_for_family(
        artifacts: &[Arc<TxArtifact>],
        family_id: ProofFamilyId,
    ) -> Vec<AggregateDebugRow> {
        let mut rows = Vec::new();

        for artifact in artifacts {
            let Some(items) = artifact.proof_items.get(&family_id) else {
                continue;
            };
            if items.is_empty() {
                continue;
            }

            let action_indices = artifact
                .tx
                .actions()
                .enumerate()
                .filter_map(|(index, action)| {
                    (action_family_id(action) == Some(family_id)).then_some(index)
                })
                .collect::<Vec<_>>();

            for (family_local_index, item) in items.iter().enumerate() {
                rows.push(AggregateDebugRow {
                    tx_id: artifact.tx.id().to_string(),
                    action_index: action_indices.get(family_local_index).copied(),
                    family_local_index,
                    public_inputs: item.public_inputs.clone(),
                });
            }
        }

        rows
    }

    fn aggregate_debug_families(artifacts: &[Arc<TxArtifact>]) -> Vec<AggregateDebugSegmentFamily> {
        let mut segments = Vec::new();
        let proof_items = Self::merge_artifact_proof_items(artifacts);
        let mut family_index = 0usize;
        for family_id in Self::proof_family_ids() {
            let items = proof_items.get(&family_id).cloned().unwrap_or_default();
            if items.is_empty() {
                continue;
            }
            segments.push(AggregateDebugSegmentFamily {
                family_index,
                family_id,
                rows: Self::aggregate_debug_rows_for_family(artifacts, family_id),
            });
            family_index += 1;
        }

        segments
    }

    fn total_artifact_proof_count(artifacts: &[Arc<TxArtifact>]) -> usize {
        artifacts
            .iter()
            .map(|artifact| artifact.total_proof_count)
            .sum()
    }

    fn current_historical_validation_stamp(&self, tx: &Transaction) -> HistoricalValidationStamp {
        HistoricalValidationStamp {
            snapshot_version: self.snapshot_version,
            anchor: tx.anchor,
        }
    }

    fn proposal_txs_digest_from_hashes(tx_hashes: &[[u8; 32]]) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update((tx_hashes.len() as u64).to_le_bytes());
        for hash in tx_hashes {
            hasher.update(hash);
        }
        hasher.finalize().into()
    }

    fn prepare_proposal_filter_concurrency() -> usize {
        let default = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get().min(64))
            .unwrap_or(1)
            .max(1);

        #[cfg(any(test, feature = "benchmark-helpers"))]
        {
            return aggregate_diagnostics::prepare_proposal_filter_concurrency_override(default);
        }
        #[cfg(not(any(test, feature = "benchmark-helpers")))]
        {
            default
        }
    }

    fn apply_checktx_fee_with_context<S: cnidarium::StateWrite>(
        state: &mut S,
        gas_used: Gas,
        fee: Fee,
        context: &CheckTxSharedContext,
    ) -> Result<()> {
        let current_gas_prices = context.gas_prices_for_fee(fee)?;

        anyhow::ensure!(
            current_gas_prices.asset_id == fee.asset_id(),
            "unexpected mismatch between fee and queried gas prices (expected: {}, found: {})",
            fee.asset_id(),
            current_gas_prices.asset_id,
        );

        let base_fee = current_gas_prices.fee(&gas_used);

        anyhow::ensure!(
            fee.amount() >= base_fee.amount(),
            "fee must be greater than or equal to the transaction base price (supplied: {}, base: {})",
            fee.amount(),
            base_fee.amount(),
        );

        let tip = Fee(shieldd_sdk_asset::Value {
            amount: fee.amount() - base_fee.amount(),
            asset_id: fee.asset_id(),
        });

        state.record_proto(shieldd_sdk_proto::core::component::fee::v1::EventPaidFee {
            fee: Some(fee.into()),
            base_fee: Some(base_fee.into()),
            gas_used: Some(gas_used.into()),
            tip: Some(tip.into()),
        });

        state.raw_accumulate_base_fee_and_tip(base_fee, tip);
        Ok(())
    }

    fn record_artifact_reuse(stage: &'static str) {
        metrics::counter!(metrics::TX_ARTIFACT_REUSE_TOTAL, "stage" => stage).increment(1);
    }

    fn record_artifact_build(
        stage: &'static str,
        tx_count: usize,
        elapsed: Duration,
        success: bool,
    ) {
        let result = if success { "ok" } else { "err" };
        metrics::counter!(
            metrics::TX_ARTIFACT_BUILD_TOTAL,
            "stage" => stage,
            "result" => result
        )
        .increment(tx_count as u64);
        metrics::histogram!(
            metrics::TX_ARTIFACT_BUILD_DURATION,
            "stage" => stage,
            "result" => result
        )
        .record(elapsed);
    }

    fn handle_proof_verification_result<T>(context: &'static str, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                tracing::debug!(?error, context, "proof verification failed");
                Err(error)
            }
        }
    }

    async fn collect_consensus_proof_items_with_artifacts(
        txs: &[Arc<Transaction>],
    ) -> Result<(
        BTreeMap<ProofFamilyId, Vec<BatchItem>>,
        Vec<Arc<TxArtifact>>,
    )> {
        use cnidarium_component::ActionHandler as _;
        use shieldd_sdk_shielded_pool::component::Ics20Transfer;

        let mut proof_items = Self::empty_proof_items();
        let mut artifacts = Vec::with_capacity(txs.len());

        for tx in txs {
            Self::ensure_user_tx_has_no_internal_actions(tx)?;
            crate::action_handler::transaction::validate_transaction_envelope(tx)?;

            let context = tx.context();
            let mut tx_proof_items = Self::empty_proof_items();

            for action in tx.actions() {
                match action {
                    Action::Transfer(transfer) => {
                        let item = transfer_check_stateless_and_extract(
                            transfer,
                            &context,
                            shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
                        )
                        .context("transfer stateless extraction failed")?;
                        let family_id = action_family_id(&Action::Transfer(transfer.clone()))
                            .expect("transfer has a proof family");

                        let tx_family_items = tx_proof_items
                            .get_mut(&family_id)
                            .ok_or_else(|| anyhow::anyhow!("unsupported transfer proof family"))?;
                        let family_items = proof_items
                            .get_mut(&family_id)
                            .ok_or_else(|| anyhow::anyhow!("unsupported transfer proof family"))?;
                        tx_family_items.push(item.clone());
                        family_items.push(item);
                    }
                    Action::ShieldedIcs20Withdrawal(withdrawal) => {
                        let item = shielded_ics20_withdrawal_check_stateless_and_extract(
                            withdrawal, &context,
                        )
                        .context("shielded ICS-20 withdrawal stateless extraction failed")?;

                        let family_id =
                            action_family_id(&Action::ShieldedIcs20Withdrawal(withdrawal.clone()))
                                .expect("shielded ICS-20 withdrawal has a proof family");

                        tx_proof_items
                            .get_mut(&family_id)
                            .expect("shielded ICS-20 withdrawal family exists")
                            .push(item.clone());
                        proof_items
                            .get_mut(&family_id)
                            .expect("shielded ICS-20 withdrawal family exists")
                            .push(item);
                    }
                    Action::ShieldedHostWithdrawal(withdrawal) => {
                        let item = shielded_host_withdrawal_check_stateless_and_extract(
                            withdrawal, &context,
                        )
                        .context("shielded host withdrawal stateless extraction failed")?;

                        let family_id =
                            action_family_id(&Action::ShieldedHostWithdrawal(withdrawal.clone()))
                                .expect("shielded host withdrawal has a proof family");

                        tx_proof_items
                            .get_mut(&family_id)
                            .expect("shielded withdrawal family exists")
                            .push(item.clone());
                        proof_items
                            .get_mut(&family_id)
                            .expect("shielded withdrawal family exists")
                            .push(item);
                    }
                    Action::NoteReshape(note_reshape) => {
                        let item = note_reshape_check_stateless_and_extract(note_reshape, &context)
                            .context("note reshape stateless extraction failed")?;

                        let family_id =
                            action_family_id(&Action::NoteReshape(note_reshape.clone()))
                                .expect("note reshape has a proof family");

                        tx_proof_items
                            .get_mut(&family_id)
                            .expect("note reshape family exists")
                            .push(item.clone());
                        proof_items
                            .get_mut(&family_id)
                            .expect("note reshape family exists")
                            .push(item);
                    }
                    Action::IbcRelay(action) => {
                        action
                            .clone()
                            .with_handler::<Ics20Transfer, ShielddHost>()
                            .check_stateless(())
                            .await?
                    }
                    Action::ComplianceRegisterAsset(action) => action.check_stateless(()).await?,
                    Action::ComplianceRegisterUser(action) => action.check_stateless(()).await?,
                    Action::AggregateBundle(_) => {
                        anyhow::bail!("aggregate bundle actions are not permitted in user txs");
                    }
                }
            }
            if let Some(fee_funding) = &tx.transaction_body.fee_funding {
                let transfer = &fee_funding.transfer;

                let item = extract_fee_funding_proof_item(fee_funding, &context)?;

                let family_id = action_family_id(&Action::Transfer(transfer.clone()))
                    .expect("fee funding transfer has a proof family");

                tx_proof_items
                    .get_mut(&family_id)
                    .expect("fee funding transfer family exists")
                    .push(item.clone());
                proof_items
                    .get_mut(&family_id)
                    .expect("fee funding transfer family exists")
                    .push(item);
            }

            let mut anchor_pairs = HashSet::new();
            let mut spend_nullifiers = Vec::new();
            for action in tx.actions() {
                match action {
                    Action::Transfer(transfer) => {
                        anchor_pairs
                            .insert((transfer.body.compliance_anchor, transfer.body.asset_anchor));
                        spend_nullifiers
                            .extend(transfer.body.inputs.iter().map(|input| input.nullifier));
                    }
                    Action::ShieldedIcs20Withdrawal(withdrawal) => {
                        anchor_pairs.insert((
                            withdrawal.body.compliance_anchor,
                            withdrawal.body.asset_anchor,
                        ));
                        spend_nullifiers
                            .extend(withdrawal.body.inputs.iter().map(|input| input.nullifier));
                    }
                    Action::ShieldedHostWithdrawal(withdrawal) => {
                        anchor_pairs.insert((
                            withdrawal.body.compliance_anchor,
                            withdrawal.body.asset_anchor,
                        ));
                        spend_nullifiers
                            .extend(withdrawal.body.inputs.iter().map(|input| input.nullifier));
                    }
                    Action::NoteReshape(note_reshape) => {
                        spend_nullifiers
                            .extend(note_reshape.body.inputs.iter().map(|input| input.nullifier));
                    }
                    _ => {}
                }
            }
            if let Some(fee_funding) = &tx.transaction_body.fee_funding {
                anchor_pairs.insert((
                    fee_funding.transfer.body.compliance_anchor,
                    fee_funding.transfer.body.asset_anchor,
                ));
                spend_nullifiers.extend(
                    fee_funding
                        .transfer
                        .body
                        .inputs
                        .iter()
                        .map(|input| input.nullifier),
                );
            }

            let total_proof_count = Self::total_proof_count(&tx_proof_items);
            artifacts.push(Arc::new(TxArtifact {
                tx: tx.clone(),
                proof_items: tx_proof_items,
                spend_nullifiers,
                anchor_pairs: anchor_pairs.into_iter().collect(),
                total_proof_count,
                historical_validation: None,
            }));
        }

        Ok((proof_items, artifacts))
    }

    async fn build_tx_artifacts(txs: &[Arc<Transaction>]) -> Result<Vec<Arc<VerifiedTxArtifact>>> {
        if txs.is_empty() {
            return Ok(Vec::new());
        }

        let (proof_items, artifacts) =
            Self::collect_consensus_proof_items_with_artifacts(txs).await?;

        let capabilities = Self::independently_verify_proof_families(proof_items).await?;
        let artifacts = Self::attach_verified_capabilities(artifacts, capabilities)?;

        Ok(artifacts)
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    async fn build_tx_artifacts_extracted(
        txs: &[Arc<Transaction>],
    ) -> Result<Vec<Arc<TxArtifact>>> {
        if txs.is_empty() {
            return Ok(Vec::new());
        }

        let (_proof_items, artifacts) =
            Self::collect_consensus_proof_items_with_artifacts(txs).await?;
        Ok(artifacts)
    }

    async fn build_tx_artifacts_for_stage(
        stage: &'static str,
        txs: &[Arc<Transaction>],
    ) -> Result<Vec<Arc<VerifiedTxArtifact>>> {
        let start = Instant::now();
        let result = Self::build_tx_artifacts(txs).await;
        Self::record_artifact_build(stage, txs.len(), start.elapsed(), result.is_ok());
        result
    }

    async fn build_tx_artifact_for_stage(
        stage: &'static str,
        tx: Arc<Transaction>,
    ) -> Result<Arc<VerifiedTxArtifact>> {
        let mut artifacts =
            Self::build_tx_artifacts_for_stage(stage, std::slice::from_ref(&tx)).await?;
        artifacts
            .pop()
            .context("single verified transaction artifact missing")
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn build_tx_artifacts_extracted_for_stage_public(
        stage: &'static str,
        txs: &[Arc<Transaction>],
    ) -> Result<Vec<Arc<TxArtifact>>> {
        let start = Instant::now();
        let result = Self::build_tx_artifacts_extracted(txs).await;
        Self::record_artifact_build(stage, txs.len(), start.elapsed(), result.is_ok());
        let artifacts = result?;
        Ok(artifacts)
    }

    async fn verify_tx_artifacts_for_stage(
        stage: &'static str,
        artifacts: &[Arc<TxArtifact>],
    ) -> Result<Vec<Arc<VerifiedTxArtifact>>> {
        let start = Instant::now();
        let proof_items = Self::merge_artifact_proof_items(artifacts);
        let result = Self::independently_verify_proof_families(proof_items).await;
        Self::record_artifact_build(stage, artifacts.len(), start.elapsed(), result.is_ok());
        let capabilities = result?;
        let verified = Self::attach_verified_capabilities(artifacts.to_vec(), capabilities)?;
        Ok(verified)
    }

    /// Runs Groth16 batch verification across multiple pre-extracted artifacts in one call.
    /// Amortizes the MSM cost across all proofs in the slice.
    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn batch_verify_artifacts_for_bench(artifacts: &[Arc<TxArtifact>]) -> Result<()> {
        Self::verify_tx_artifacts_for_stage("bench_batch", artifacts).await?;
        Ok(())
    }

    async fn independently_verify_proof_families(
        proof_items: BTreeMap<ProofFamilyId, Vec<BatchItem>>,
    ) -> Result<BTreeMap<ProofFamilyId, VecDeque<VerifiedBatchItem>>> {
        let mut proof_items = proof_items;
        let mut tasks = tokio::task::JoinSet::new();

        for family_id in Self::proof_family_ids() {
            let Some(items) = proof_items.remove(&family_id) else {
                continue;
            };
            if items.is_empty() {
                continue;
            }

            tasks.spawn(async move {
                let family_label = proof_family_label(family_id);
                let batch_verify_stage = proof_family_batch_verify_stage(family_id);
                let key = deployed_key_for_family(family_id);
                let result = tokio::task::spawn_blocking(move || {
                    batch::verify_each_with_capabilities(
                        key,
                        items.into_iter().map(Arc::new).collect(),
                    )
                    .map_err(|error| {
                        anyhow::anyhow!("{family_label} independent verification failed: {error}")
                    })
                })
                .await
                .with_context(|| format!("{family_label} proof verification task panicked"))?;
                let capabilities =
                    Self::handle_proof_verification_result(batch_verify_stage, result)?;
                Ok::<_, anyhow::Error>((family_id, VecDeque::from(capabilities)))
            });
        }

        let verified =
            drain_joinset_results(&mut tasks, "independent proof verification task panicked")
                .await?
                .into_iter()
                .collect();
        Ok(verified)
    }

    fn attach_verified_capabilities(
        artifacts: Vec<Arc<TxArtifact>>,
        mut capabilities: BTreeMap<ProofFamilyId, VecDeque<VerifiedBatchItem>>,
    ) -> Result<Vec<Arc<VerifiedTxArtifact>>> {
        let verified = artifacts
            .into_iter()
            .map(|artifact| {
                VerifiedTxArtifact::take_family_capabilities(artifact, &mut capabilities)
                    .map(Arc::new)
            })
            .collect::<Result<Vec<_>>>()?;
        anyhow::ensure!(
            capabilities.values().all(VecDeque::is_empty),
            "verified proof capabilities remain after exact transaction-slot assignment"
        );
        Ok(verified)
    }

    fn max_prefix_len_for_payload_limit(
        prefix_payload_bytes: &[u64],
        max_payload_bytes: u64,
    ) -> usize {
        let mut len = 0usize;
        while len < prefix_payload_bytes.len() && prefix_payload_bytes[len] < max_payload_bytes {
            len += 1;
        }
        len
    }

    fn padded_proof_count(real_count: usize) -> Result<u32> {
        if real_count == 0 {
            return Ok(0);
        }

        let padded = real_count
            .checked_next_power_of_two()
            .context("padded proof count overflow")?;
        anyhow::ensure!(
            padded <= MAX_PADDED_PROOF_COUNT,
            "padded proof count {padded} exceeds maximum {MAX_PADDED_PROOF_COUNT}"
        );
        Ok(padded as u32)
    }

    fn estimated_aggregate_proof_bytes(family_id: ProofFamilyId) -> usize {
        match family_id {
            ProofFamilyId::Transfer
            | ProofFamilyId::NoteReshape(_)
            | ProofFamilyId::ShieldedIcs20Withdrawal(_) => AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
        }
    }

    fn aggregate_bundle_family_estimates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
    ) -> Result<Vec<AggregateBundleFamilyEstimate>> {
        let proof_items = Self::merge_artifact_proof_items(artifacts);
        let mut estimates = Vec::new();

        for family_id in Self::proof_family_ids() {
            let real_count = proof_items.get(&family_id).map(Vec::len).unwrap_or(0);
            if real_count == 0 {
                continue;
            }

            estimates.push(AggregateBundleFamilyEstimate {
                family_id,
                real_count: real_count as u32,
                padded_count: Self::padded_proof_count(real_count)?,
                aggregate_proof_bytes: Self::estimated_aggregate_proof_bytes(family_id),
            });
        }

        Ok(estimates)
    }

    fn estimate_aggregate_bundle_tx_size_bytes(
        chain_id: &str,
        family_estimates: &[AggregateBundleFamilyEstimate],
    ) -> usize {
        let bundle = AggregateBundle {
            version: AGGREGATE_PROTOCOL_VERSION,
            srs_id: vec![0; 32],
            families: family_estimates
                .iter()
                .map(|estimate| FamilyAggregate {
                    family_id: estimate.family_id,
                    real_count: estimate.real_count,
                    padded_count: estimate.padded_count,
                    aggregate_proof: vec![0; estimate.aggregate_proof_bytes],
                })
                .collect(),
        };

        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::AggregateBundle(bundle)],
                transaction_parameters: TransactionParameters {
                    expiry_height: 0,
                    chain_id: chain_id.to_owned(),
                    fee: Fee::default(),
                },
                fee_funding: None,
                memo: None,
                nullifier_window: None,
                historical_nullifier_proofs: Vec::new(),
            },
            binding_sig: [0; 64].into(),
            anchor: shieldd_sdk_tct::Root(shieldd_sdk_tct::structure::Hash::zero()),
        };

        tx.encode_to_vec().len()
    }

    fn select_prefix_len_with_bundle_budget(
        prefix_payload_bytes: &[u64],
        max_proposal_size_bytes: u64,
        safety_margin_bytes: u64,
        bundle_bytes: usize,
    ) -> usize {
        let usable_limit = max_proposal_size_bytes
            .saturating_sub(safety_margin_bytes)
            .saturating_sub(bundle_bytes as u64);
        Self::max_prefix_len_for_payload_limit(prefix_payload_bytes, usable_limit)
    }

    async fn build_aggregate_bundle_tx(&self, bundle: AggregateBundle) -> Result<Transaction> {
        let anchor = self.state.get_sct().await.root();
        let chain_id = self.state.get_chain_id().await?;
        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::AggregateBundle(bundle)],
                transaction_parameters: TransactionParameters {
                    expiry_height: 0,
                    chain_id,
                    fee: Fee::default(),
                },
                fee_funding: None,
                memo: None,
                nullifier_window: None,
                historical_nullifier_proofs: Vec::new(),
            },
            binding_sig: [0; 64].into(),
            anchor,
        };

        Ok(tx)
    }

    async fn build_family_aggregates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        segment_index: usize,
    ) -> Result<Vec<FamilyAggregate>> {
        let proof_items = Self::merge_artifact_proof_items(artifacts);
        if Self::total_artifact_proof_count(artifacts) == 0 {
            return Ok(Vec::new());
        }

        let srs = shipping_srs()?;

        let mut aggregate_tasks = Vec::new();
        let debug_entries = Self::aggregate_debug_families(artifacts);

        for family_id in Self::proof_family_ids() {
            let items = proof_items.get(&family_id).cloned().unwrap_or_default();
            if items.is_empty() {
                continue;
            }

            let real_count = items.len() as u32;

            let padded_items = pad_items_to_power_of_two(&items, MAX_PADDED_PROOF_COUNT)?;

            let padded_count = padded_items.len() as u32;
            let srs_for_task = srs.clone();
            let padded_public_inputs = padded_items
                .iter()
                .map(|item| item.public_inputs.clone())
                .collect::<Vec<_>>();
            let statement = AggregateStatement::new(
                AGGREGATE_PROTOCOL_VERSION,
                family_id,
                srs_id(&srs),
                proof_verification_key_for_family(family_id),
                real_count,
                &padded_public_inputs,
            )?;
            let debug_entry = debug_entries
                .iter()
                .find(|entry| entry.family_id == family_id)
                .cloned()
                .unwrap_or(AggregateDebugSegmentFamily {
                    family_index: 0,
                    family_id,
                    rows: Vec::new(),
                });
            maybe_write_aggregate_debug_dump(
                "aggregate",
                segment_index,
                debug_entry.family_index,
                family_id,
                &debug_entry.rows,
                &padded_public_inputs,
                None,
            );

            aggregate_tasks.push(tokio::task::spawn_blocking(
                move || -> Result<FamilyAggregate> {
                    let aggregate_proof = aggregate_family(
                        &statement,
                        proof_verification_key_for_family(family_id),
                        &padded_items,
                        &srs_for_task,
                    )?;

                    Ok(FamilyAggregate {
                        family_id,
                        real_count,
                        padded_count,
                        aggregate_proof,
                    })
                },
            ));
        }

        let mut families = Vec::new();
        let mut first_error = None;
        for task in aggregate_tasks {
            match task.await {
                Ok(Ok(family)) => {
                    families.push(family);
                }
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error =
                            Some(anyhow::anyhow!("aggregate family task panicked: {error}"));
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        Ok(families)
    }

    async fn build_segmented_family_aggregates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_count: usize,
    ) -> Result<(Vec<FamilyAggregate>, Vec<usize>)> {
        if artifacts.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut families = Vec::new();
        let mut segment_tx_counts = Vec::new();

        for (segment_index, artifact_segment) in artifacts.chunks(segment_tx_count).enumerate() {
            let segment_families =
                Self::build_family_aggregates_for_artifacts(artifact_segment, segment_index)
                    .await?;
            if !artifact_segment.is_empty() {
                segment_tx_counts.push(artifact_segment.len());
            }

            families.extend(segment_families);
        }

        Ok((families, segment_tx_counts))
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    async fn build_exact_segmented_family_aggregates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_counts: &[usize],
    ) -> Result<(Vec<FamilyAggregate>, Vec<usize>)> {
        if artifacts.is_empty() {
            anyhow::ensure!(
                segment_tx_counts.is_empty(),
                "empty artifacts must not provide segment counts"
            );
            return Ok((Vec::new(), Vec::new()));
        }

        anyhow::ensure!(
            !segment_tx_counts.is_empty(),
            "non-empty artifacts require at least one segment"
        );
        anyhow::ensure!(
            segment_tx_counts.iter().sum::<usize>() == artifacts.len(),
            "segment coverage mismatch: expected {}, got {}",
            artifacts.len(),
            segment_tx_counts.iter().sum::<usize>()
        );

        let mut families = Vec::new();

        let mut next_start = 0usize;
        let mut next_segment = 0usize;
        let mut segment_tasks = tokio::task::JoinSet::new();
        let mut ordered_segment_results = vec![None; segment_tx_counts.len()];
        let mut first_error = None;

        while next_segment < segment_tx_counts.len() || !segment_tasks.is_empty() {
            while next_segment < segment_tx_counts.len()
                && segment_tasks.len() < MAX_CONCURRENT_AGGREGATE_SEGMENTS
            {
                let segment_tx_count = segment_tx_counts[next_segment];
                anyhow::ensure!(segment_tx_count > 0, "segment_tx_counts must be positive");
                let end = next_start + segment_tx_count;
                let artifact_segment = artifacts[next_start..end].to_vec();
                let segment_index = next_segment;
                segment_tasks.spawn(async move {
                    let segment_families = Self::build_family_aggregates_for_artifacts(
                        &artifact_segment,
                        segment_index,
                    )
                    .await?;
                    Ok::<_, anyhow::Error>((segment_index, segment_families))
                });
                next_start = end;
                next_segment += 1;
            }

            let Some(result) = segment_tasks.join_next().await else {
                continue;
            };
            match result {
                Ok(Ok((segment_index, segment_families))) => {
                    ordered_segment_results[segment_index] = Some(segment_families);
                }
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error =
                            Some(anyhow::anyhow!("aggregate segment task panicked: {error}"));
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        for segment_result in ordered_segment_results {
            let segment_families = segment_result.context("missing aggregate segment result")?;

            families.extend(segment_families);
        }

        Ok((families, segment_tx_counts.to_vec()))
    }

    async fn build_aggregate_bundle_from_families(
        &self,
        families: Vec<FamilyAggregate>,
    ) -> Result<Option<Bytes>> {
        if families.is_empty() {
            return Ok(None);
        }

        let srs = shipping_srs()?;

        let bundle_tx = self
            .build_aggregate_bundle_tx(AggregateBundle {
                version: AGGREGATE_PROTOCOL_VERSION,
                srs_id: srs_id(&srs).to_vec(),
                families,
            })
            .await?;

        Ok(Some(Bytes::from(bundle_tx.encode_to_vec())))
    }

    pub(crate) fn ensure_aggregate_bundle_tx_shape(tx: &Transaction) -> Result<&AggregateBundle> {
        use crate::action_handler::transaction::stateless::valid_binding_signature;

        anyhow::ensure!(
            tx.is_aggregate_bundle_tx(),
            "aggregate bundle tx must contain exactly one aggregate bundle action"
        );
        anyhow::ensure!(
            tx.transaction_body.memo.is_none(),
            "aggregate bundle tx must not contain a memo"
        );
        anyhow::ensure!(
            tx.transaction_body.transaction_parameters.fee == Fee::default(),
            "aggregate bundle tx must have zero fee"
        );
        valid_binding_signature(tx)?;
        tx.aggregate_bundle_action()
            .context("aggregate bundle tx missing bundle action")
    }

    fn expected_aggregate_verify_segments(
        artifacts: &[Arc<TxArtifact>],
        segment_ranges: &[shieldd_sdk_proof_aggregation::AppVerifySegmentRange],
    ) -> Vec<AggregateExpectedVerifySegment> {
        let mut expected_segments = Vec::new();

        for segment_range in segment_ranges {
            let artifact_group = &artifacts[segment_range.start..segment_range.end];
            let proof_items = Self::merge_artifact_proof_items(artifact_group);
            let mut family_index = 0usize;
            for family_id in Self::proof_family_ids() {
                let items = proof_items.get(&family_id).cloned().unwrap_or_default();
                if items.is_empty() {
                    continue;
                }
                expected_segments.push(AggregateExpectedVerifySegment {
                    segment_index: segment_range.segment_index,
                    family_index,
                    family_id,
                    items,
                    debug_rows: Self::aggregate_debug_rows_for_family(artifact_group, family_id),
                });
                family_index += 1;
            }
        }

        expected_segments
    }

    fn validate_aggregate_verify_plan_inputs(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
        srs: &DevSrs,
    ) -> Result<Vec<shieldd_sdk_proof_aggregation::AppVerifySegmentRange>> {
        match app_verify_preflight_core(
            AGGREGATE_PROTOCOL_VERSION,
            bundle.version,
            Self::total_artifact_proof_count(artifacts),
            srs_id(srs).to_vec(),
            bundle.srs_id.clone(),
            artifacts.len(),
            segment_tx_counts.is_some(),
            segment_tx_counts.unwrap_or_default().to_vec(),
        ) {
            Ok(ranges) => return Ok(ranges),
            Err(AppVerifyPreflightError::BadVersion) => {
                anyhow::bail!("unsupported aggregate bundle version {}", bundle.version);
            }
            Err(AppVerifyPreflightError::EmptyProofSet) => {
                anyhow::bail!("aggregate bundle requires at least one proof");
            }
            Err(AppVerifyPreflightError::BadSrsLength) => {
                anyhow::bail!(
                    "aggregate bundle SRS id must be 32 bytes, got {}",
                    bundle.srs_id.len()
                );
            }
            Err(AppVerifyPreflightError::SrsMismatch) => {
                anyhow::bail!("aggregate bundle SRS id mismatch");
            }
            Err(AppVerifyPreflightError::SegmentCoverageOverflow) => {
                anyhow::bail!(
                    "aggregate segment coverage overflow while summing transaction counts"
                );
            }
            Err(AppVerifyPreflightError::SegmentCoverageMismatch) => {
                let covered_artifacts = segment_tx_counts
                    .unwrap_or_default()
                    .iter()
                    .fold(0usize, |covered, count| covered.saturating_add(*count));
                anyhow::bail!(
                    "aggregate segment coverage mismatch: expected {}, got {}",
                    artifacts.len(),
                    covered_artifacts
                );
            }
        }
    }

    fn plan_aggregate_bundle_verification(
        bundle: &AggregateBundle,
        expected_segments: Vec<AggregateExpectedVerifySegment>,
        srs: DevSrs,
    ) -> Result<AggregateVerifyPlan> {
        if let Err(AppVerifyPlanError::FamilyCountMismatch) =
            app_verify_family_count_core(expected_segments.len(), bundle.families.len())
        {
            anyhow::bail!(
                "aggregate bundle family count mismatch: expected {}, got {}",
                expected_segments.len(),
                bundle.families.len()
            );
        }

        let core_ids = app_verify_plan_ids_core(
            expected_segments
                .iter()
                .map(|expected| AppVerifyExpectedCall {
                    segment_index: expected.segment_index,
                    family_index: expected.family_index,
                    family: app_verify_family_code(expected.family_id),
                })
                .collect(),
        );
        let mut calls = Vec::with_capacity(expected_segments.len());
        for (order_index, expected_segment) in expected_segments.into_iter().enumerate() {
            let AggregateExpectedVerifySegment {
                segment_index,
                family_index,
                family_id,
                items,
                debug_rows,
            } = expected_segment;
            let aggregate = bundle
                .families
                .get(order_index)
                .cloned()
                .context("missing aggregate family")?;
            let core_id = core_ids[order_index];
            match app_verify_plan_identity_core(
                core_id,
                app_verify_family_code(aggregate.family_id),
                items.len(),
                aggregate.real_count,
            ) {
                Ok(_) => {}
                Err(AppVerifyPlanError::FamilyMismatch) => {
                    anyhow::bail!(
                        "aggregate family ordering mismatch: expected {:?}, got {:?}",
                        family_id,
                        aggregate.family_id
                    );
                }
                Err(AppVerifyPlanError::RealCountMismatch) => {
                    anyhow::bail!(
                        "aggregate real_count mismatch for {:?}: expected {}, got {}",
                        family_id,
                        items.len(),
                        aggregate.real_count
                    );
                }
                Err(AppVerifyPlanError::RealCountOverflow) => {
                    anyhow::bail!(
                        "aggregate real_count mismatch for {:?}: expected {}, got {}",
                        family_id,
                        items.len(),
                        aggregate.real_count
                    );
                }
                Err(AppVerifyPlanError::PaddedCountMismatch) => {
                    unreachable!("identity validation cannot report padding mismatch")
                }
                Err(AppVerifyPlanError::PaddedCountOverflow) => {
                    unreachable!("identity validation cannot report padding overflow")
                }
                Err(AppVerifyPlanError::FamilyCountMismatch) => {
                    unreachable!("identity validation cannot report family-count mismatch")
                }
            }

            let prepared_inputs = prepare_verify_inputs(&items, MAX_PADDED_PROOF_COUNT)?;
            let shipping_call = shieldd_sdk_proof_aggregation::AppVerifyShippingCall {
                id: core_id,
                bundle_family: app_verify_family_code(aggregate.family_id),
                expected_real_count: items.len(),
                bundle_real_count: aggregate.real_count,
                expected_padded_count: prepared_inputs.padded_count,
                bundle_padded_count: aggregate.padded_count,
            };
            match app_verify_plan_padding_core(
                shipping_call.id,
                shipping_call.expected_padded_count,
                shipping_call.bundle_padded_count,
            ) {
                Ok(_) => {}
                Err(AppVerifyPlanError::PaddedCountMismatch) => {
                    anyhow::bail!(
                        "aggregate padded_count mismatch for {:?}: expected {}, got {}",
                        family_id,
                        prepared_inputs.padded_count,
                        aggregate.padded_count
                    );
                }
                Err(AppVerifyPlanError::PaddedCountOverflow) => {
                    anyhow::bail!(
                        "aggregate padded_count mismatch for {:?}: expected {}, got {}",
                        family_id,
                        prepared_inputs.padded_count,
                        aggregate.padded_count
                    );
                }
                Err(AppVerifyPlanError::FamilyMismatch)
                | Err(AppVerifyPlanError::RealCountMismatch)
                | Err(AppVerifyPlanError::RealCountOverflow) => {
                    unreachable!("padding validation cannot report identity mismatch")
                }
                Err(AppVerifyPlanError::FamilyCountMismatch) => {
                    unreachable!("padding validation cannot report family-count mismatch")
                }
            }

            let statement = AggregateStatement::new(
                AGGREGATE_PROTOCOL_VERSION,
                family_id,
                srs_id(&srs),
                proof_verification_key_for_family(family_id),
                shipping_call.bundle_real_count,
                &prepared_inputs.padded_public_inputs,
            )?;
            calls.push(AggregateVerifyCall {
                id: AggregateVerifyCallId {
                    order_index,
                    segment_index,
                    family_index,
                    family_id,
                },
                shipping_call,
                statement,
                aggregate,
                srs: srs.clone(),
                debug_rows,
                padded_public_inputs: prepared_inputs.padded_public_inputs,
                items,
            });
        }

        Ok(AggregateVerifyPlan { calls })
    }

    fn execute_aggregate_verify_call(
        call: AggregateVerifyCall,
    ) -> Result<AggregateVerifyCallOutcome> {
        let shipping_verification = verify_shipping_family_aggregate(
            call.shipping_call,
            &call.statement,
            proof_verification_key_for_family(call.id.family_id),
            &call.aggregate.aggregate_proof,
            &call.srs,
        )?;
        Ok(AggregateVerifyCallOutcome {
            id: call.id,
            shipping_verification,
            items: call.items,
        })
    }

    fn reduce_aggregate_verify_outcomes(
        expected_call_ids: &[AggregateVerifyCallId],
        mut results: Vec<AggregateVerifyCallResult>,
    ) -> Result<AggregateVerifyReduction> {
        let expected_core = expected_call_ids
            .iter()
            .map(|id| AppVerifyCallId {
                order_index: id.order_index,
                segment_index: id.segment_index,
                family_index: id.family_index,
                family: app_verify_family_code(id.family_id),
            })
            .collect::<Vec<_>>();
        let result_core = results
            .iter()
            .map(|result| AppVerifyCallResult {
                id: AppVerifyCallId {
                    order_index: result.id.order_index,
                    segment_index: result.id.segment_index,
                    family_index: result.id.family_index,
                    family: app_verify_family_code(result.id.family_id),
                },
                accepted: result.accepted,
            })
            .collect::<Vec<_>>();
        let rejected_core = match app_verify_reduce_core(expected_core, result_core) {
            Ok(rejected) => rejected,
            Err(AppVerifyReductionError::OutcomeCountMismatch) => {
                anyhow::bail!(
                    "aggregate verification outcome count mismatch: expected {}, got {}",
                    expected_call_ids.len(),
                    results.len()
                );
            }
            Err(AppVerifyReductionError::OutcomeIdentityMismatch) => {
                results.sort_by_key(|result| result.id.order_index);
                let mismatch = expected_call_ids
                    .iter()
                    .zip(&results)
                    .find(|(expected, result)| {
                        let result_core = AppVerifyCallId {
                            order_index: result.id.order_index,
                            segment_index: result.id.segment_index,
                            family_index: result.id.family_index,
                            family: app_verify_family_code(result.id.family_id),
                        };
                        let expected_core = AppVerifyCallId {
                            order_index: expected.order_index,
                            segment_index: expected.segment_index,
                            family_index: expected.family_index,
                            family: app_verify_family_code(expected.family_id),
                        };
                        result_core != expected_core
                    })
                    .context("aggregate verification core reported an unlocatable mismatch")?;
                anyhow::bail!(
                    "aggregate verification outcome identity mismatch: expected {:?}, got {:?}",
                    mismatch.0,
                    mismatch.1.id
                );
            }
        };

        let rejected_calls = rejected_core
            .into_iter()
            .map(|core_id| {
                expected_call_ids
                    .iter()
                    .find(|id| {
                        id.order_index == core_id.order_index
                            && id.segment_index == core_id.segment_index
                            && id.family_index == core_id.family_index
                            && app_verify_family_code(id.family_id) == core_id.family
                    })
                    .copied()
                    .context("aggregate verification core returned an unknown rejected call")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(AggregateVerifyReduction { rejected_calls })
    }

    async fn verify_aggregate_bundle_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> Result<Vec<Arc<VerifiedTxArtifact>>> {
        match Self::verify_aggregate_bundle_for_artifacts_raw(artifacts, bundle, segment_tx_counts)
            .await
        {
            Ok(verified) => Ok(verified),
            Err(error) => {
                tracing::debug!(
                    ?error,
                    context = "aggregate_bundle_verify",
                    "aggregate verification failed"
                );
                Err(error)
            }
        }
    }

    async fn verify_aggregate_bundle_for_artifacts_raw(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> Result<Vec<Arc<VerifiedTxArtifact>>> {
        let srs = shipping_srs_for_id(&bundle.srs_id)?;
        let segment_ranges = Self::validate_aggregate_verify_plan_inputs(
            artifacts,
            bundle,
            segment_tx_counts,
            &srs,
        )?;

        let expected_segments =
            Self::expected_aggregate_verify_segments(artifacts, &segment_ranges);

        let plan_result = Self::plan_aggregate_bundle_verification(bundle, expected_segments, srs);

        let plan = plan_result?;

        let expected_call_ids = plan.calls.iter().map(|call| call.id).collect::<Vec<_>>();
        let mut pending_calls = VecDeque::from(plan.calls);
        let mut verify_tasks = tokio::task::JoinSet::new();
        let mut outcomes = Vec::with_capacity(pending_calls.len());
        let mut first_error = None;
        while !pending_calls.is_empty() || !verify_tasks.is_empty() {
            while verify_tasks.len() < MAX_CONCURRENT_AGGREGATE_VERIFY_CALLS {
                let Some(call) = pending_calls.pop_front() else {
                    break;
                };
                maybe_write_aggregate_debug_dump(
                    "verify",
                    call.id.segment_index,
                    call.id.family_index,
                    call.id.family_id,
                    &call.debug_rows,
                    &call.padded_public_inputs,
                    Some(&call.aggregate),
                );
                verify_tasks.spawn_blocking(move || Self::execute_aggregate_verify_call(call));
            }
            let Some(task) = verify_tasks.join_next().await else {
                continue;
            };
            match task {
                Ok(Ok(outcome)) => outcomes.push(outcome),
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!(
                            "aggregate verification task panicked: {error}"
                        ));
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        let expected_core_ids = expected_call_ids
            .iter()
            .copied()
            .map(aggregate_verify_app_call_id)
            .collect::<Vec<_>>();
        let joined_records = outcomes
            .into_iter()
            .map(|outcome| {
                let shipping_result = outcome.shipping_verification.shipping_result();
                let authenticated_id = shipping_result.input.call.id;
                let executed_id = shipping_result.result.id;
                let accepted = shipping_result.result.accepted;
                let observation = outcome.shipping_verification.shipping_observation();
                AppVerifyPlannerIndexedExecutedRecord {
                    planner_id: aggregate_verify_app_call_id(outcome.id),
                    authenticated_id,
                    executed_id,
                    accepted,
                    observation,
                    executed: outcome,
                }
            })
            .collect::<Vec<_>>();
        let joined_projection =
            match app_verify_accepted_join_projection_core(expected_core_ids, joined_records) {
                Ok(projection) => projection,
                Err(AppVerifyAcceptedJoinProjectionError::OutcomeCountMismatch {
                    expected,
                    actual,
                }) => {
                    anyhow::bail!(
                        "aggregate verification outcome count mismatch: expected {}, got {}",
                        expected,
                        actual
                    );
                }
                Err(AppVerifyAcceptedJoinProjectionError::FullIdentityMismatch { .. }) => {
                    anyhow::bail!(
                        "aggregate verification result identity does not match its planned call"
                    );
                }
                Err(AppVerifyAcceptedJoinProjectionError::OutcomeOrderMismatch { position }) => {
                    anyhow::bail!(
                        "aggregate verification outcome order mismatch at planner position {}",
                        position
                    );
                }
            };
        let rejected_calls = joined_projection.rejected_calls;
        let outcomes = joined_projection
            .records
            .into_iter()
            .map(|record| record.executed)
            .collect::<Vec<_>>();
        let results = outcomes
            .iter()
            .map(AggregateVerifyCallOutcome::result)
            .collect::<Result<Vec<_>>>()?;
        let reduction = Self::reduce_aggregate_verify_outcomes(&expected_call_ids, results)?;
        reduction.acceptance_result()?;
        require_no_rejected_joined_calls(rejected_calls)?;

        let mut capabilities = BTreeMap::<ProofFamilyId, VecDeque<VerifiedBatchItem>>::new();
        for outcome in outcomes {
            let family_id = outcome.id.family_id;
            let verified = outcome
                .shipping_verification
                .verified_statement_capabilities(
                    family_id,
                    deployed_key_for_family(family_id),
                    &outcome.items,
                )?;
            capabilities.entry(family_id).or_default().extend(verified);
        }
        Self::attach_verified_capabilities(artifacts.to_vec(), capabilities)
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn verify_aggregate_bundle_for_artifacts_raw_public(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> Result<()> {
        Self::verify_aggregate_bundle_for_artifacts_raw(artifacts, bundle, segment_tx_counts)
            .await
            .map(|_| ())
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn build_aggregate_bundle_tx_for_snapshot_public(
        snapshot: Snapshot,
        bundle: AggregateBundle,
    ) -> Result<Transaction> {
        Self::new(snapshot).build_aggregate_bundle_tx(bundle).await
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn process_candidate_envelope(
        &mut self,
        envelope: &CandidateEnvelope,
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<response::ProcessProposal> {
        let context = self.benchmark_block_context().await?;
        let proposal = Self::process_proposal_request_from_envelope(&context, envelope);
        let sidecar = ProposalArtifactSidecar::from_record(envelope.sidecar.clone());

        Ok(self
            .process_proposal(proposal, stateless_cache, Some(&sidecar), false)
            .await)
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn execute_validated_candidate_envelope_profiled(
        &mut self,
        envelope: &CandidateEnvelope,
        storage: Storage,
    ) -> Result<ExecutionBlockProfile> {
        let context = self.benchmark_block_context().await?;
        let begin_block = Self::begin_block_request_from_context(&context);
        let mut profile = ExecutionBlockProfile {
            block_tx_count: envelope.block_tx_count,
            ..Default::default()
        };

        let begin_block_start = Instant::now();
        let _events = self.begin_block(&begin_block).await;
        profile.begin_block_ms = begin_block_start.elapsed().as_secs_f64() * 1000.0;

        let decoded_txs = envelope
            .txs
            .iter()
            .enumerate()
            .map(|(index, tx_bytes)| {
                Transaction::decode_canonical(tx_bytes.as_slice())
                    .map(Arc::new)
                    .with_context(|| format!("decoding execution benchmark tx ordinal {index}"))
            })
            .collect::<Result<Vec<_>>>()?;

        let extracted_artifacts = Self::build_tx_artifacts_extracted(&decoded_txs).await?;
        let verified_artifacts = if extracted_artifacts
            .iter()
            .any(|artifact| artifact.total_proof_count != 0)
        {
            let bundle_bytes = envelope
                .aggregate_bundle_tx_bytes
                .as_deref()
                .context("validated candidate with proofs is missing aggregate bundle tx")?;
            let bundle_tx = Transaction::decode_canonical(bundle_bytes)
                .context("decoding validated candidate aggregate bundle tx")?;
            let bundle = Self::ensure_aggregate_bundle_tx_shape(&bundle_tx)?;
            Self::verify_aggregate_bundle_for_artifacts(
                &extracted_artifacts,
                bundle,
                Some(&envelope.segment_tx_counts),
            )
            .await?
        } else {
            extracted_artifacts
                .into_iter()
                .map(|artifact| VerifiedTxArtifact::new(artifact, Vec::new()).map(Arc::new))
                .collect::<Result<Vec<_>>>()?
        };

        let deliver_txs_start = Instant::now();
        for artifact in verified_artifacts {
            let execute_tx_start = Instant::now();
            let _events = self.execute_tx_checked_historical(artifact).await?;
            profile.execute_tx_ms += execute_tx_start.elapsed().as_secs_f64() * 1000.0;
        }
        profile.deliver_txs_wall_ms = deliver_txs_start.elapsed().as_secs_f64() * 1000.0;

        let end_block = request::EndBlock {
            height: i64::try_from(context.height.value())
                .context("converting execution benchmark end_block height")?,
        };
        let end_block_start = Instant::now();
        let _events = self.end_block(&end_block).await;
        profile.end_block_ms = end_block_start.elapsed().as_secs_f64() * 1000.0;

        let commit_start = Instant::now();
        let _root_hash = self.commit(storage).await;
        profile.commit_ms = commit_start.elapsed().as_secs_f64() * 1000.0;

        Ok(profile)
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn build_exact_segmented_aggregate_bundle_for_artifacts_public(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_counts: &[usize],
    ) -> Result<(AggregateBundle, Vec<usize>)> {
        let (families, segment_tx_counts) =
            Self::build_exact_segmented_family_aggregates_for_artifacts(
                artifacts,
                segment_tx_counts,
            )
            .await?;
        let srs = shipping_srs()?;
        Ok((
            AggregateBundle {
                version: AGGREGATE_PROTOCOL_VERSION,
                srs_id: srs_id(&srs).to_vec(),
                families,
            },
            segment_tx_counts,
        ))
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub fn candidate_envelope_from_prepared_proposal_public(
        prepared: &response::PrepareProposal,
        sidecar: &ProposalArtifactSidecar,
        source_builder_label: impl Into<String>,
    ) -> Result<CandidateEnvelope> {
        let user_tx_count = sidecar.chunk_tx_count;
        anyhow::ensure!(
            prepared.txs.len() == user_tx_count
                || prepared.txs.len() == user_tx_count.saturating_add(1),
            "prepared proposal must contain {user_tx_count} user transactions and at most one aggregate bundle, got {} entries",
            prepared.txs.len()
        );
        anyhow::ensure!(
            sidecar.segment_tx_counts.iter().sum::<usize>() == user_tx_count,
            "prepared proposal sidecar segments must cover all user transactions"
        );

        let txs = prepared.txs[..user_tx_count]
            .iter()
            .map(|tx| tx.to_vec())
            .collect::<Vec<_>>();
        let aggregate_bundle_tx_bytes = if prepared.txs.len() == user_tx_count + 1 {
            let bytes = prepared.txs[user_tx_count].to_vec();
            let bundle_tx = Transaction::decode_canonical(bytes.as_slice())
                .context("decoding prepared aggregate bundle transaction")?;
            Self::ensure_aggregate_bundle_tx_shape(&bundle_tx)
                .context("validating prepared aggregate bundle transaction")?;
            Some(bytes)
        } else {
            None
        };
        let tx_hashes = txs
            .iter()
            .map(|tx_bytes| sha2::Sha256::digest(tx_bytes).into())
            .collect::<Vec<[u8; 32]>>();

        Ok(CandidateEnvelope {
            txs,
            tx_hashes: tx_hashes.clone(),
            aggregate_bundle_tx_bytes,
            sidecar: sidecar.to_record(),
            segment_tx_counts: sidecar.segment_tx_counts.clone(),
            block_tx_count: user_tx_count,
            total_payload_bytes: prepared.txs[..user_tx_count].iter().map(Bytes::len).sum(),
            candidate_digest: candidate_digest_from_hashes(&tx_hashes),
            source_builder_label: source_builder_label.into(),
        })
    }

    fn ensure_unique_spend_nullifiers_from_artifacts(artifacts: &[Arc<TxArtifact>]) -> Result<()> {
        let mut seen = HashSet::new();
        for artifact in artifacts {
            for &nullifier in &artifact.spend_nullifiers {
                if !seen.insert(nullifier) {
                    anyhow::bail!("duplicate spend nullifier in proposal");
                }
            }
        }
        Ok(())
    }

    fn ensure_unique_volume_nullifiers_from_artifacts(artifacts: &[Arc<TxArtifact>]) -> Result<()> {
        let mut seen = HashSet::new();
        for artifact in artifacts {
            for action in artifact.tx.actions() {
                let payload = match action {
                    Action::Transfer(transfer) => {
                        anyhow::ensure!(
                            transfer.body.proof_context
                                == shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
                            "body transfer must use ordinary proof context"
                        );
                        Some(&transfer.body.volume_accumulator)
                    }
                    Action::ShieldedHostWithdrawal(withdrawal) => {
                        Some(&withdrawal.body.volume_accumulator)
                    }
                    Action::ShieldedIcs20Withdrawal(withdrawal) => {
                        Some(&withdrawal.body.volume_accumulator)
                    }
                    _ => None,
                };
                if let Some(payload) = payload {
                    anyhow::ensure!(
                        seen.insert(payload.scoped_nullifier()),
                        "duplicate daily volume nullifier in proposal"
                    );
                }
            }
            if let Some(fee_funding) = &artifact.tx.transaction_body.fee_funding {
                anyhow::ensure!(
                    fee_funding.transfer.body.proof_context
                        == shieldd_sdk_shielded_pool::TransferProofContext::FeeFunding,
                    "fee funding transfer must use fee-funding proof context"
                );
            }
        }
        Ok(())
    }

    async fn precheck_compliance_anchors_dedup_from_artifacts(
        &self,
        artifacts: &[Arc<TxArtifact>],
    ) -> Result<()> {
        let mut unique_pairs = HashSet::new();

        for artifact in artifacts {
            unique_pairs.extend(artifact.anchor_pairs.iter().copied());
        }

        for (compliance_anchor, asset_anchor) in unique_pairs {
            self.state
                .validate_compliance_anchors(&compliance_anchor, &asset_anchor)
                .await?;
        }

        Ok(())
    }

    async fn precheck_compliance_anchors_dedup(&self, txs: &[Arc<Transaction>]) -> Result<()> {
        let mut unique_pairs = HashSet::new();

        for tx in txs {
            for action in tx.actions() {
                match action {
                    Action::Transfer(transfer) => {
                        unique_pairs
                            .insert((transfer.body.compliance_anchor, transfer.body.asset_anchor));
                    }
                    Action::ShieldedIcs20Withdrawal(withdrawal) => {
                        unique_pairs.insert((
                            withdrawal.body.compliance_anchor,
                            withdrawal.body.asset_anchor,
                        ));
                    }
                    Action::ShieldedHostWithdrawal(withdrawal) => {
                        unique_pairs.insert((
                            withdrawal.body.compliance_anchor,
                            withdrawal.body.asset_anchor,
                        ));
                    }
                    _ => {}
                }
            }
        }

        for (compliance_anchor, asset_anchor) in unique_pairs {
            self.state
                .validate_compliance_anchors(&compliance_anchor, &asset_anchor)
                .await?;
        }

        Ok(())
    }

    async fn prepare_proposal_batched(
        &mut self,
        proposal_height: u64,
        txs: Vec<Bytes>,
        max_proposal_size_bytes: u64,
        stateless_cache: Option<&StatelessCache>,
        allow_oversized_proposal: bool,
    ) -> Result<(Vec<Bytes>, Option<ProposalArtifactSidecar>)> {
        let mut candidates = Vec::new();
        let mut proposal_size_bytes = 0u64;
        let mut assembly_attempts = 0;

        for tx_bytes in txs {
            let transaction_size = tx_bytes.len() as u64;
            let total_with_tx = proposal_size_bytes.saturating_add(transaction_size);

            if transaction_size > max_transaction_size_bytes() as u64 {
                continue;
            }
            if !allow_oversized_proposal && total_with_tx >= max_proposal_size_bytes {
                break;
            }

            let hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_ref()).into();
            if let Some(cache) = stateless_cache {
                match cache.get(&hash, tx_bytes.as_ref()) {
                    Some(CacheEntry::Invalid) => continue,
                    Some(CacheEntry::FullyVerified(artifact)) => {
                        Self::record_artifact_reuse("prepare_proposal");
                        proposal_size_bytes = total_with_tx;
                        candidates.push(Candidate {
                            bytes: tx_bytes,
                            hash,
                            data: CandidateData::VerifiedArtifact(artifact),
                        });
                        continue;
                    }
                    Some(CacheEntry::Extracted(artifact)) => {
                        Self::record_artifact_reuse("prepare_proposal");
                        proposal_size_bytes = total_with_tx;
                        candidates.push(Candidate {
                            bytes: tx_bytes,
                            hash,
                            data: CandidateData::ExtractedArtifact(artifact),
                        });
                        continue;
                    }
                    None => {}
                }
            }

            let tx = match Transaction::decode_canonical(tx_bytes.as_ref()) {
                Ok(tx) => Arc::new(tx),
                Err(_) => continue,
            };
            if Self::ensure_user_tx_has_no_internal_actions(&tx).is_err() {
                continue;
            }
            proposal_size_bytes = total_with_tx;

            candidates.push(Candidate {
                bytes: tx_bytes,
                hash,
                data: CandidateData::Decoded(tx),
            });
        }

        if candidates.is_empty() {
            return Ok((Vec::new(), None));
        }

        // Fast precheck: reject duplicate spends before heavier verification.

        let mut seen_nullifiers = HashSet::new();
        let mut seen_volume_nullifiers = HashSet::new();
        let mut deduped = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let mut tx_nullifiers = HashSet::new();
            let mut tx_volume_nullifiers = HashSet::new();
            let mut duplicate = false;

            for nullifier in candidate.tx().spent_nullifiers() {
                if !tx_nullifiers.insert(nullifier) || seen_nullifiers.contains(&nullifier) {
                    duplicate = true;
                    break;
                }
            }

            for action in candidate.tx().actions() {
                let payload = match action {
                    Action::Transfer(transfer) => Some(&transfer.body.volume_accumulator),
                    Action::ShieldedHostWithdrawal(withdrawal) => {
                        Some(&withdrawal.body.volume_accumulator)
                    }
                    Action::ShieldedIcs20Withdrawal(withdrawal) => {
                        Some(&withdrawal.body.volume_accumulator)
                    }
                    _ => None,
                };
                if let Some(payload) = payload {
                    let scoped = payload.scoped_nullifier();
                    if !tx_volume_nullifiers.insert(scoped)
                        || seen_volume_nullifiers.contains(&scoped)
                    {
                        duplicate = true;
                        break;
                    }
                }
            }

            if duplicate {
                continue;
            }
            if !block_nullifier_count_allowed(
                seen_nullifiers
                    .len()
                    .saturating_add(seen_volume_nullifiers.len())
                    .saturating_add(tx_nullifiers.len())
                    .saturating_add(tx_volume_nullifiers.len()),
            ) {
                break;
            }

            seen_nullifiers.extend(tx_nullifiers);
            seen_volume_nullifiers.extend(tx_volume_nullifiers);
            deduped.push(candidate);
        }

        let deduped_txs: Vec<Arc<Transaction>> = deduped.iter().map(|c| c.tx().clone()).collect();

        self.precheck_compliance_anchors_dedup(&deduped_txs).await?;

        let cache_miss_txs = deduped
            .iter()
            .filter_map(|candidate| match &candidate.data {
                CandidateData::Decoded(tx) => Some(tx.clone()),
                CandidateData::ExtractedArtifact(_) | CandidateData::VerifiedArtifact(_) => None,
            })
            .collect::<Vec<_>>();

        let extracted_cache_hits = deduped
            .iter()
            .filter_map(|candidate| match &candidate.data {
                CandidateData::ExtractedArtifact(artifact) => {
                    Some((candidate.bytes.clone(), artifact.clone()))
                }
                CandidateData::VerifiedArtifact(_) | CandidateData::Decoded(_) => None,
            })
            .collect::<Vec<_>>();

        if !cache_miss_txs.is_empty() {
            let miss_artifacts =
                Self::build_tx_artifacts_for_stage("prepare_proposal", &cache_miss_txs).await?;
            let mut miss_artifacts = miss_artifacts.into_iter();

            for candidate in &mut deduped {
                if matches!(candidate.data, CandidateData::Decoded(_)) {
                    let artifact = miss_artifacts
                        .next()
                        .expect("artifact count should match decoded candidates");
                    if let Some(cache) = stateless_cache {
                        cache.insert_fully_verified(candidate.bytes.as_ref(), artifact.clone())?;
                    }
                    candidate.data = CandidateData::VerifiedArtifact(artifact);
                }
            }
        }

        if !extracted_cache_hits.is_empty() {
            let extracted_artifacts = extracted_cache_hits
                .iter()
                .map(|(_, artifact)| artifact.clone())
                .collect::<Vec<_>>();
            let verified_artifacts = Self::verify_tx_artifacts_for_stage(
                "prepare_proposal_upgrade",
                &extracted_artifacts,
            )
            .await?;

            if let Some(cache) = stateless_cache {
                for ((raw_tx, _), artifact) in extracted_cache_hits.iter().zip(&verified_artifacts)
                {
                    cache.insert_fully_verified(raw_tx.as_ref(), artifact.clone())?;
                }
            }

            let mut verified_artifacts = verified_artifacts.into_iter();
            for candidate in &mut deduped {
                if matches!(candidate.data, CandidateData::ExtractedArtifact(_)) {
                    candidate.data = CandidateData::VerifiedArtifact(
                        verified_artifacts
                            .next()
                            .expect("verified artifact count must match extracted cache hits"),
                    );
                }
            }
        }

        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&self.state)).await?;
        let deduped_candidate_count = deduped.len();

        let included_candidates = if deduped_candidate_count > 1
            && deduped
                .iter()
                .all(|candidate| supports_parallel_prepare(candidate.tx()))
        {
            self.execute_prepare_candidates_parallel(deduped, historical_context.clone())
                .await?
        } else {
            let mut included_candidates = Vec::new();
            for candidate in deduped {
                if let Ok(_) = self
                    .execute_prepare_candidate(
                        candidate
                            .verified_artifact()
                            .expect("prepare candidate must be proof verified"),
                        &historical_context,
                    )
                    .await
                {
                    included_candidates.push(candidate);
                }
            }
            included_candidates
        };

        if self.block_tx_indexing_mode == BlockTxIndexingMode::DeferredBatch {
            self.flush_deferred_block_transactions().await?;
        }

        if included_candidates.is_empty() {
            return Ok((Vec::new(), None));
        }

        #[derive(Clone)]
        struct ProposalAssemblyResult {
            prefix_len: usize,
            bundle_tx_bytes: Option<Bytes>,

            sidecar: ProposalArtifactSidecar,
        }

        let included_prefix_payload_bytes = included_candidates
            .iter()
            .scan(0u64, |total, candidate| {
                *total = total.saturating_add(candidate.bytes.len() as u64);
                Some(*total)
            })
            .collect::<Vec<_>>();

        let max_payload_prefix_len = if allow_oversized_proposal {
            included_candidates.len()
        } else {
            Self::max_prefix_len_for_payload_limit(
                &included_prefix_payload_bytes,
                max_proposal_size_bytes,
            )
        };
        if max_payload_prefix_len == 0 {
            return Ok((Vec::new(), None));
        }

        let chain_id = self.state.get_chain_id().await?;
        let mut current_prefix_len = max_payload_prefix_len;
        let mut best_result: Option<ProposalAssemblyResult> = None;
        let mut fallback_used = false;

        while current_prefix_len > 0 && assembly_attempts < 2 {
            let selected_candidates = &included_candidates[..current_prefix_len];
            let selected_artifacts: Vec<Arc<TxArtifact>> = selected_candidates
                .iter()
                .map(|candidate| {
                    candidate
                        .artifact()
                        .expect("included proposal candidates should have artifacts")
                })
                .collect();

            let proposal_txs_digest = Self::proposal_txs_digest_from_hashes(
                &selected_candidates
                    .iter()
                    .map(|candidate| candidate.hash)
                    .collect::<Vec<_>>(),
            );

            if let Some(cached) = &self.aggregate_retry_cache {
                if cached.height == proposal_height
                    && cached.included_tx_count == current_prefix_len
                    && cached.proposal_txs_digest == proposal_txs_digest
                    && cached.proposal_segment_tx_count == self.proposal_segment_tx_count
                {
                    let sidecar = ProposalArtifactSidecar::build(
                        &selected_artifacts,
                        current_prefix_len,
                        Self::proposal_segment_counts(
                            current_prefix_len,
                            self.proposal_segment_tx_count,
                        ),
                    )?;

                    tracing::info!(
                        height = proposal_height,
                        included_tx_count = current_prefix_len,
                        proposal_segment_tx_count = self.proposal_segment_tx_count,
                        "prepare_proposal_aggregate_retry_cache_hit"
                    );
                    best_result = Some(ProposalAssemblyResult {
                        prefix_len: current_prefix_len,
                        bundle_tx_bytes: cached.bundle_tx_bytes.clone(),

                        sidecar,
                    });
                    break;
                }
            }

            if !allow_oversized_proposal {
                let family_estimates =
                    Self::aggregate_bundle_family_estimates_for_artifacts(&selected_artifacts)?;
                let estimated_bundle_bytes =
                    Self::estimate_aggregate_bundle_tx_size_bytes(&chain_id, &family_estimates);
                let estimated_prefix_len = Self::select_prefix_len_with_bundle_budget(
                    &included_prefix_payload_bytes,
                    max_proposal_size_bytes,
                    AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
                    estimated_bundle_bytes,
                )
                .min(current_prefix_len);

                if estimated_prefix_len == 0 {
                    return Ok((Vec::new(), None));
                }

                if estimated_prefix_len < current_prefix_len {
                    current_prefix_len = estimated_prefix_len;
                    continue;
                }
            }

            assembly_attempts += 1;

            let (families, segment_tx_counts) =
                if let Some(segment_tx_count) = self.proposal_segment_tx_count {
                    let (segment_families, segment_tx_counts) =
                        Self::build_segmented_family_aggregates_for_artifacts(
                            &selected_artifacts,
                            segment_tx_count,
                        )
                        .await?;
                    (segment_families, segment_tx_counts)
                } else {
                    let families =
                        Self::build_family_aggregates_for_artifacts(&selected_artifacts, 0).await?;
                    let segment_tx_counts = if !selected_artifacts.is_empty() {
                        vec![selected_artifacts.len()]
                    } else {
                        Vec::new()
                    };
                    (families, segment_tx_counts)
                };

            let bundle_result = self.build_aggregate_bundle_from_families(families).await;

            match bundle_result {
                Ok(bundle_tx_bytes) => {
                    let actual_bundle_bytes = bundle_tx_bytes
                        .as_ref()
                        .map(|bytes| bytes.len())
                        .unwrap_or(0);
                    let family_estimates =
                        Self::aggregate_bundle_family_estimates_for_artifacts(&selected_artifacts)?;
                    let estimated_bundle_bytes =
                        Self::estimate_aggregate_bundle_tx_size_bytes(&chain_id, &family_estimates);
                    let proposal_size_bytes = included_prefix_payload_bytes[current_prefix_len - 1]
                        .saturating_add(actual_bundle_bytes as u64);

                    tracing::info!(
                        attempt_index = assembly_attempts,
                        candidate_prefix_len = current_prefix_len,
                        payload_bytes_before_bundle =
                            included_prefix_payload_bytes[current_prefix_len - 1],
                        estimated_bundle_bytes,
                        actual_bundle_bytes,
                        proposal_size_bytes,
                        max_proposal_size_bytes,
                        oversize = !allow_oversized_proposal
                            && proposal_size_bytes >= max_proposal_size_bytes,
                        "prepare_proposal_assembly_attempt"
                    );

                    if !allow_oversized_proposal && proposal_size_bytes >= max_proposal_size_bytes {
                        if fallback_used {
                            tracing::warn!(
                                candidate_prefix_len = current_prefix_len,
                                actual_bundle_bytes,
                                proposal_size_bytes,
                                max_proposal_size_bytes,
                                estimate_miss_bytes =
                                    proposal_size_bytes.saturating_sub(max_proposal_size_bytes),
                                "prepare_proposal exact-size fallback still oversized"
                            );
                            break;
                        }

                        let mut fallback_prefix_len = Self::select_prefix_len_with_bundle_budget(
                            &included_prefix_payload_bytes,
                            max_proposal_size_bytes,
                            AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
                            actual_bundle_bytes,
                        )
                        .min(current_prefix_len.saturating_sub(1));

                        if fallback_prefix_len == 0 {
                            break;
                        }
                        if fallback_prefix_len >= current_prefix_len {
                            fallback_prefix_len = current_prefix_len.saturating_sub(1);
                        }

                        tracing::warn!(
                            attempt_index = assembly_attempts,
                            previous_prefix_len = current_prefix_len,
                            fallback_prefix_len,
                            actual_bundle_bytes,
                            estimate_miss_bytes =
                                proposal_size_bytes.saturating_sub(max_proposal_size_bytes),
                            "prepare_proposal exact-size fallback rebuild"
                        );
                        fallback_used = true;
                        current_prefix_len = fallback_prefix_len;
                    } else {
                        let sidecar = ProposalArtifactSidecar::build(
                            &selected_artifacts,
                            selected_candidates.len(),
                            segment_tx_counts,
                        )?;

                        let cached_bundle_tx_bytes = bundle_tx_bytes.clone();
                        best_result = Some(ProposalAssemblyResult {
                            prefix_len: current_prefix_len,
                            bundle_tx_bytes,

                            sidecar,
                        });
                        self.aggregate_retry_cache = Some(CachedProposalAggregate {
                            height: proposal_height,
                            included_tx_count: current_prefix_len,
                            proposal_txs_digest,
                            proposal_segment_tx_count: self.proposal_segment_tx_count,
                            bundle_tx_bytes: cached_bundle_tx_bytes,
                        });
                        break;
                    }
                }
                Err(err) if err.to_string().contains("padded proof count") => {
                    tracing::warn!(
                        attempt_index = assembly_attempts,
                        candidate_prefix_len = current_prefix_len,
                        error = %err,
                        "prepare_proposal padded proof count exceeded during assembly"
                    );
                    break;
                }
                Err(err) => return Err(err),
            }
        }

        if let Some(best_result) = best_result {
            let mut included_txs = included_candidates[..best_result.prefix_len]
                .iter()
                .map(|candidate| candidate.bytes.clone())
                .collect::<Vec<_>>();
            if let Some(bundle_tx_bytes) = best_result.bundle_tx_bytes {
                included_txs.push(bundle_tx_bytes);
            }
            return Ok((included_txs, Some(best_result.sidecar)));
        }

        Ok((Vec::new(), None))
    }

    /// Constructs a new application, using the provided [`Snapshot`].
    /// Callers should ensure that [`App::is_ready`]) returns `true`, but this is not enforced.
    #[instrument(skip_all)]
    pub fn new(snapshot: Snapshot) -> Self {
        tracing::debug!("initializing App instance");
        let snapshot_version = snapshot.version();

        // We perform the `Arc` wrapping of `State` here to ensure
        // there should be no unexpected copies elsewhere.
        let state = Arc::new(StateDelta::new(snapshot.clone()));

        Self {
            state,
            committed_snapshot: snapshot,
            snapshot_version,
            block_tx_indexing_mode: BlockTxIndexingMode::PerTx,
            deferred_block_transactions: Vec::new(),
            pending_sct_append_log: BlockSctAppendLog::default(),
            checktx_shared_context: None,
            aggregate_retry_cache: None,
            proposal_segment_tx_count: Some(200),
        }
    }

    pub fn set_block_tx_indexing_mode(&mut self, mode: BlockTxIndexingMode) {
        self.block_tx_indexing_mode = mode;
    }

    pub fn set_checktx_shared_context(&mut self, context: Arc<CheckTxSharedContext>) {
        self.checktx_shared_context = Some(context);
    }

    pub(crate) fn set_aggregate_retry_cache(&mut self, cache: Option<CachedProposalAggregate>) {
        self.aggregate_retry_cache = cache;
    }

    /// Override the proposer aggregate segment size. Production default is 128.
    pub fn set_proposal_segment_tx_count(&mut self, segment_tx_count: Option<usize>) {
        self.proposal_segment_tx_count = segment_tx_count;
    }

    fn proposal_segment_counts(tx_count: usize, segment_tx_count: Option<usize>) -> Vec<usize> {
        match segment_tx_count {
            Some(0) => Vec::new(),
            Some(segment_tx_count) => (0..tx_count)
                .step_by(segment_tx_count)
                .map(|start| (tx_count - start).min(segment_tx_count))
                .collect(),
            None if tx_count > 0 => vec![tx_count],
            None => Vec::new(),
        }
    }

    pub(crate) fn aggregate_retry_cache(&self) -> Option<CachedProposalAggregate> {
        self.aggregate_retry_cache.clone()
    }

    /// Returns whether the application is ready to start.
    #[instrument(skip_all, ret)]
    pub async fn is_ready(state: Snapshot) -> bool {
        if let Err(error) = shieldd_sdk_sct::nullifier_tree::verify_committed_roots(&state).await {
            tracing::error!(?error, "nullifier tree root check failed");
            return false;
        }
        if let Err(error) = state.verify_committed_sct_root().await {
            tracing::error!(?error, "SCT root check failed");
            return false;
        }
        if let Err(error) = state.verify_committed_tree_roots().await {
            tracing::error!(?error, "compliance tree root check failed");
            return false;
        }
        true
    }

    // StateDelta::apply only works when the StateDelta wraps an underlying
    // StateWrite.  But if we want to share the StateDelta with spawned tasks,
    // we usually can't wrap a StateWrite instance, which requires exclusive
    // access. This method "externally" applies the state delta to the
    // inter-block state.
    //
    // Invariant: `state_tx` and `self.state` are the only two references to the
    // inter-block state.
    fn apply(&mut self, state_tx: StateDelta<InterBlockState>) -> Vec<Event> {
        let (state2, mut cache) = state_tx.flatten();
        std::mem::drop(state2);
        // Now there is only one reference to the inter-block state: self.state

        let events = cache.take_events();
        cache.apply_to(
            Arc::get_mut(&mut self.state).expect("no other references to inter-block state"),
        );

        events
    }

    pub async fn init_chain(&mut self, app_state: &AppState) {
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should not be referenced elsewhere");
        match app_state {
            AppState::Content(genesis) => {
                crate::app_version::initialize_app_version(&mut state_tx);
                state_tx.put_chain_id(genesis.chain_id.clone());
                Sct::init_chain(&mut state_tx, Some(&genesis.sct_content)).await;
                // Compliance assets and users are admitted before issuance so
                // regulated genesis notes use their registered recovery capability.
                Compliance::init_chain(&mut state_tx, Some(&genesis.compliance_content)).await;
                ShieldedPool::init_chain(&mut state_tx, Some(&genesis.shielded_pool_content)).await;
                Ibc::init_chain(&mut state_tx, Some(&genesis.ibc_content)).await;
                FeeComponent::init_chain(&mut state_tx, Some(&genesis.fee_content)).await;

                state_tx
                    .finish_block()
                    .await
                    .expect("must be able to finish compact block");
            }
            AppState::Checkpoint(_) => {
                Compliance::init_chain(&mut state_tx, None).await;
                ShieldedPool::init_chain(&mut state_tx, None).await;
                Ibc::init_chain(&mut state_tx, None).await;
                FeeComponent::init_chain(&mut state_tx, None).await;
            }
        };

        // Note that `init_chain` can not emit any events, and we do not want to
        // work around this as it violates the design principle that events are changes
        // to initial data.
        //
        // This means that indexers are responsible for parsing genesis data and bootstrapping
        // their initial state before processing chronological events.
        //
        // See: https://github.com/mizufinance/shieldd/pull/4449#discussion_r1636868800

        state_tx.apply();
    }

    pub async fn prepare_proposal(
        &mut self,
        mut proposal: request::PrepareProposal,
        stateless_cache: Option<&StatelessCache>,
        allow_oversized_proposal: bool,
    ) -> (response::PrepareProposal, Option<ProposalArtifactSidecar>) {
        let num_candidate_txs = proposal.txs.len();
        truncate_prepare_candidates(&mut proposal.txs);
        tracing::debug!(
            "processing PrepareProposal, found {} candidate transactions",
            num_candidate_txs
        );

        // This is a node controlled parameter that is different from the homonymous
        // mempool's `max_tx_bytes`. Comet will send us raw proposals that exceed this
        // limit, presuming that a subset of those transactions will be shed.
        // More context in https://github.com/cometbft/cometbft/blob/v0.37.5/spec/abci/abci%2B%2B_app_requirements.md
        let max_proposal_size_bytes = prepare_proposal_payload_limit(proposal.max_tx_bytes);
        let (included_txs, sidecar) = match self
            .prepare_proposal_batched(
                proposal.height.value() as u64,
                proposal.txs,
                max_proposal_size_bytes,
                stateless_cache,
                allow_oversized_proposal,
            )
            .await
        {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(?e, "prepare_proposal failed, returning an empty proposal");
                (Vec::new(), None)
            }
        };

        // The evidence payload is validated by Comet, we can lean on three guarantees:
        // 1. The total payload is bound by `MAX_EVIDENCE_SIZE_BYTES`
        // 2. Expired evidence is filtered
        // 3. Evidence is valid.
        tracing::debug!(
            "finished processing PrepareProposal, including {}/{} candidate transactions",
            included_txs.len(),
            num_candidate_txs
        );

        (response::PrepareProposal { txs: included_txs }, sidecar)
    }

    #[instrument(skip_all, ret, level = "debug")]
    pub async fn process_proposal(
        &mut self,
        proposal: request::ProcessProposal,
        stateless_cache: Option<&StatelessCache>,
        synthetic_sidecar: Option<&ProposalArtifactSidecar>,
        allow_oversized_proposal: bool,
    ) -> response::ProcessProposal {
        tracing::debug!(
            height = proposal.height.value(),
            proposer = ?proposal.proposer_address,
            proposal_hash = ?proposal.hash,
            "processing proposal"
        );

        let proposal_height = proposal.height.value();
        let proposal_hash = proposal.hash.to_string();
        macro_rules! reject_process_proposal {
            ($reason:literal) => {{
                tracing::warn!(
                    height = proposal_height,
                    proposal_hash = %proposal_hash,
                    reason = $reason,
                    "process_proposal_reject_reason"
                );
                return response::ProcessProposal::Reject;
            }};
            ($reason:literal, $($field:tt)*) => {{
                tracing::warn!(
                    height = proposal_height,
                    proposal_hash = %proposal_hash,
                    reason = $reason,
                    $($field)*,
                    "process_proposal_reject_reason"
                );
                return response::ProcessProposal::Reject;
            }};
        }

        let mut evidence_buffer: Vec<u8> = Vec::with_capacity(MAX_EVIDENCE_SIZE_BYTES);
        let mut bytes_tracker = 0usize;

        for evidence in proposal.misbehavior {
            evidence_buffer.clear();
            let proto_evidence: tendermint_proto::v0_37::abci::Misbehavior = evidence.into();
            let evidence_size = match proto_evidence.encode(&mut evidence_buffer) {
                Ok(_) => evidence_buffer.len(),
                Err(_) => reject_process_proposal!("misbehavior_encode_failed"),
            };
            bytes_tracker = bytes_tracker.saturating_add(evidence_size);
            if bytes_tracker > MAX_EVIDENCE_SIZE_BYTES {
                reject_process_proposal!("misbehavior_bytes_exceeded", bytes_tracker);
            }
        }

        enum UserTxData {
            ExtractedArtifact(Arc<TxArtifact>),
            VerifiedArtifact(Arc<VerifiedTxArtifact>),
            Decoded(Arc<Transaction>),
        }

        struct UserTx {
            hash: [u8; 32],
            raw_tx: Bytes,
            data: UserTxData,
            cache_miss: bool,
            extracted_cache_hit: bool,
        }

        impl UserTx {
            fn artifact(&self) -> Option<Arc<TxArtifact>> {
                match &self.data {
                    UserTxData::ExtractedArtifact(artifact) => Some(artifact.clone()),
                    UserTxData::VerifiedArtifact(artifact) => Some(artifact.extracted()),
                    UserTxData::Decoded(_) => None,
                }
            }
        }

        let proposal_tx_count = proposal.txs.len();
        if !process_proposal_tx_count_allowed(proposal_tx_count) {
            reject_process_proposal!("tx_count_exceeded", proposal_tx_count);
        }
        let mut total_txs_payload_size = 0usize;
        let mut user_txs = Vec::with_capacity(proposal_tx_count);
        let mut bundle_tx: Option<Arc<Transaction>> = None;

        for (index, tx_bytes) in proposal.txs.into_iter().enumerate() {
            let tx_size = tx_bytes.len();
            if !allow_oversized_proposal && tx_size > max_transaction_size_bytes() {
                reject_process_proposal!("tx_size_exceeded", index, tx_size);
            }

            total_txs_payload_size = total_txs_payload_size.saturating_add(tx_size);
            if !allow_oversized_proposal
                && !process_proposal_payload_size_allowed(total_txs_payload_size)
            {
                reject_process_proposal!(
                    "total_txs_payload_exceeded",
                    index,
                    total_txs_payload_size
                );
            }

            let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_ref()).into();
            if let Some(cache) = stateless_cache {
                match cache.get(&tx_hash, tx_bytes.as_ref()) {
                    Some(CacheEntry::Invalid) => {
                        reject_process_proposal!("stateless_cache_invalid", tx_hash = %hex::encode(tx_hash));
                    }
                    Some(CacheEntry::FullyVerified(artifact)) => {
                        Self::record_artifact_reuse("process_proposal");

                        user_txs.push(UserTx {
                            hash: tx_hash,
                            raw_tx: tx_bytes.clone(),
                            data: UserTxData::VerifiedArtifact(artifact),
                            cache_miss: false,
                            extracted_cache_hit: false,
                        });
                        continue;
                    }
                    Some(CacheEntry::Extracted(artifact)) => {
                        Self::record_artifact_reuse("process_proposal");

                        user_txs.push(UserTx {
                            hash: tx_hash,
                            raw_tx: tx_bytes.clone(),
                            data: UserTxData::ExtractedArtifact(artifact),
                            cache_miss: false,
                            extracted_cache_hit: true,
                        });
                        continue;
                    }
                    None => {}
                }
            }

            let tx = match Transaction::decode_canonical(tx_bytes.as_ref()) {
                Ok(tx) => Arc::new(tx),
                Err(_) => reject_process_proposal!("tx_decode_failed", index),
            };

            if tx.is_aggregate_bundle_tx() {
                if index + 1 != proposal_tx_count {
                    reject_process_proposal!("aggregate_bundle_not_last", index, proposal_tx_count);
                }
                if let Err(error) = Self::ensure_aggregate_bundle_tx_shape(&tx) {
                    reject_process_proposal!("aggregate_bundle_bad_shape", index, error = %error);
                }
                if bundle_tx.replace(tx).is_some() {
                    reject_process_proposal!("multiple_aggregate_bundle_txs");
                }
                continue;
            }

            if tx.contains_aggregate_bundle_action()
                || Self::ensure_user_tx_has_no_internal_actions(&tx).is_err()
            {
                reject_process_proposal!("user_tx_contains_internal_actions", index);
            }

            user_txs.push(UserTx {
                hash: tx_hash,
                raw_tx: tx_bytes,
                data: UserTxData::Decoded(tx),
                cache_miss: true,
                extracted_cache_hit: false,
            });
        }

        if !user_txs.is_empty() {
            let mut sidecar_hits = Vec::new();
            let mut raw_miss_txs = Vec::new();

            for (index, user_tx) in user_txs.iter().enumerate() {
                match &user_tx.data {
                    UserTxData::ExtractedArtifact(_) | UserTxData::VerifiedArtifact(_) => {}
                    UserTxData::Decoded(tx) => {
                        if let Some(sidecar) = synthetic_sidecar {
                            if let Some(encoded_entry) = sidecar.entry_bytes(&user_tx.hash) {
                                sidecar_hits.push((index, encoded_entry, tx.clone()));
                                continue;
                            }
                        }
                        raw_miss_txs.push(tx.clone());
                    }
                }
            }

            if let Some(sidecar) = synthetic_sidecar {
                if !sidecar_hits.is_empty() {
                    for (index, encoded_entry, tx) in sidecar_hits {
                        let artifact = match sidecar.decode_artifact(
                            user_txs[index].hash,
                            tx,
                            encoded_entry.as_slice(),
                        ) {
                            Ok(artifact) => artifact,
                            Err(_) => reject_process_proposal!("sidecar_decode_failed", index),
                        };
                        user_txs[index].data = UserTxData::ExtractedArtifact(artifact);
                    }
                }
            }

            if !raw_miss_txs.is_empty() {
                let miss_artifacts =
                    match Self::build_tx_artifacts_for_stage("process_proposal", &raw_miss_txs)
                        .await
                    {
                        Ok(result) => result,
                        Err(_) => reject_process_proposal!("artifact_reconstruction_failed"),
                    };
                let mut miss_artifacts = miss_artifacts.into_iter();

                for user_tx in &mut user_txs {
                    if matches!(user_tx.data, UserTxData::Decoded(_)) {
                        let artifact = miss_artifacts
                            .next()
                            .expect("artifact count should match decoded proposal transactions");
                        user_tx.data = UserTxData::VerifiedArtifact(artifact);
                    }
                }
            }
        }

        let artifacts = user_txs
            .iter()
            .map(|user_tx| {
                user_tx
                    .artifact()
                    .expect("proposal user tx should have artifact after miss fill")
            })
            .collect::<Vec<_>>();
        let block_nullifier_count = artifacts
            .iter()
            .map(|artifact| {
                artifact.spend_nullifiers.len()
                    + artifact
                        .tx
                        .actions()
                        .filter(|action| matches!(action, Action::Transfer(_)))
                        .count()
            })
            .sum::<usize>();
        if !block_nullifier_count_allowed(block_nullifier_count) {
            reject_process_proposal!("block_nullifier_count_exceeded", block_nullifier_count);
        }

        if Self::ensure_unique_spend_nullifiers_from_artifacts(&artifacts).is_err() {
            reject_process_proposal!("duplicate_spend_nullifiers");
        }
        if Self::ensure_unique_volume_nullifiers_from_artifacts(&artifacts).is_err() {
            reject_process_proposal!("duplicate_volume_nullifiers");
        }

        if self
            .precheck_compliance_anchors_dedup_from_artifacts(&artifacts)
            .await
            .is_err()
        {
            reject_process_proposal!("anchor_recheck_failed");
        }

        let total_proofs = Self::total_artifact_proof_count(&artifacts);
        let mut aggregate_verify_task: Option<
            tokio::task::JoinHandle<anyhow::Result<Vec<Arc<VerifiedTxArtifact>>>>,
        > = None;
        match (total_proofs, bundle_tx.as_ref()) {
            (0, None) => {}
            (0, Some(_)) => reject_process_proposal!("bundle_present_with_zero_proofs"),
            (_, None) => reject_process_proposal!("bundle_missing_with_nonzero_proofs"),
            (_, Some(bundle_tx)) => {
                let bundle = match Self::ensure_aggregate_bundle_tx_shape(bundle_tx) {
                    Ok(bundle) => bundle,
                    Err(_) => reject_process_proposal!("bundle_shape_validation_failed"),
                };
                let artifacts = artifacts.clone();
                let bundle = bundle.clone();
                let segment_tx_counts =
                    synthetic_sidecar.map(|sidecar| sidecar.segment_tx_counts.clone());

                aggregate_verify_task = Some(tokio::task::spawn(async move {
                    Self::verify_aggregate_bundle_for_artifacts(
                        &artifacts,
                        &bundle,
                        segment_tx_counts.as_deref(),
                    )
                    .await
                }));
            }
        }

        let historical_context = match HistoricalCheckContext::load(Arc::as_ref(&self.state)).await
        {
            Ok(context) => context,
            Err(_) => reject_process_proposal!("historical_context_load_failed"),
        };

        let verified_artifacts = if let Some(aggregate_verify_task) = aggregate_verify_task {
            match aggregate_verify_task.await {
                Ok(Ok(verified)) => verified,
                _ => reject_process_proposal!("aggregate_verify_task_failed"),
            }
        } else {
            match artifacts
                .iter()
                .cloned()
                .map(|artifact| VerifiedTxArtifact::new(artifact, Vec::new()).map(Arc::new))
                .collect::<Result<Vec<_>>>()
            {
                Ok(verified) => verified,
                Err(_) => reject_process_proposal!("zero_proof_capability_construction_failed"),
            }
        };

        if let Some(cache) = stateless_cache {
            for (user_tx, artifact) in user_txs.iter().zip(&verified_artifacts) {
                if user_tx.cache_miss || user_tx.extracted_cache_hit {
                    if cache
                        .insert_fully_verified(user_tx.raw_tx.as_ref(), artifact.clone())
                        .is_err()
                    {
                        reject_process_proposal!("stateless_cache_artifact_binding_failed");
                    }
                }
            }
        }

        for artifact in verified_artifacts {
            match self
                .deliver_tx_with_verified_stateless(artifact, Some(&historical_context))
                .await
            {
                Ok(_) => {}
                Err(_) => reject_process_proposal!("stateful_replay_failed"),
            };
        }

        if self.block_tx_indexing_mode == BlockTxIndexingMode::DeferredBatch {
            if self.flush_deferred_block_transactions().await.is_err() {
                reject_process_proposal!("deferred_index_flush_failed");
            }
        }

        response::ProcessProposal::Accept
    }

    pub async fn begin_block(&mut self, begin_block: &request::BeginBlock) -> Vec<abci::Event> {
        self.pending_sct_append_log.clear();
        let mut state_tx = StateDelta::new(self.state.clone());

        clear_block_fee_price_cache(&mut state_tx);

        // Run each of the begin block handlers for each component, in sequence:
        let mut arc_state_tx = Arc::new(state_tx);
        Sct::begin_block(&mut arc_state_tx, begin_block).await;
        ShieldedPool::begin_block(&mut arc_state_tx, begin_block).await;
        Ibc::begin_block::<ShielddHost, StateDelta<Arc<StateDelta<cnidarium::Snapshot>>>>(
            &mut arc_state_tx,
            begin_block,
        )
        .await;
        FeeComponent::begin_block(&mut arc_state_tx, begin_block).await;

        let state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");

        self.apply(state_tx)
    }

    /// Verify and execute one transaction, reusing byte-bound cached proof results.
    pub async fn deliver_tx_bytes(
        &mut self,
        tx_bytes: &[u8],
        cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        anyhow::ensure!(
            transaction_size_allowed(tx_bytes.len()),
            "transaction size {} exceeds maximum {}",
            tx_bytes.len(),
            MAX_TRANSACTION_SIZE_BYTES
        );
        if let Some(cache) = cache {
            let hash: [u8; 32] = sha2::Sha256::digest(tx_bytes).into();
            let artifact = match cache.get(&hash, tx_bytes) {
                Some(CacheEntry::FullyVerified(artifact)) => {
                    Self::record_artifact_reuse("checktx");
                    Some(artifact)
                }
                Some(CacheEntry::Extracted(extracted)) => {
                    let mut verified = match Self::verify_tx_artifacts_for_stage(
                        "checktx_cache_upgrade",
                        std::slice::from_ref(&extracted),
                    )
                    .await
                    {
                        Ok(verified) => verified,
                        Err(error) => {
                            cache.insert_invalid(tx_bytes)?;
                            return Err(error);
                        }
                    };
                    let artifact = verified
                        .pop()
                        .context("verified cache-upgrade artifact missing")?;
                    cache.insert_fully_verified(tx_bytes, artifact.clone())?;
                    Some(artifact)
                }
                Some(CacheEntry::Invalid) => {
                    anyhow::bail!("transaction previously failed stateless checks")
                }
                None => None,
            };
            if let Some(artifact) = artifact {
                let skip_historical =
                    artifact.has_matching_historical_validation(self.snapshot_version);
                let events = if supports_parallel_prepare(artifact.tx())
                    && self.checktx_shared_context.is_some()
                {
                    self.execute_checktx_fast(artifact, skip_historical).await?
                } else {
                    self.deliver_tx_with_verified_stateless(artifact, None)
                        .await?
                };
                return Ok(events);
            }
        }

        let tx = Arc::new(Transaction::decode_canonical(tx_bytes).context("decoding transaction")?);
        Self::ensure_user_tx_has_no_internal_actions(&tx)?;
        let fast = supports_parallel_prepare(tx.as_ref()) && self.checktx_shared_context.is_some();
        let tx_for_extract = tx.clone();
        let handle = tokio::runtime::Handle::current();
        let span = tracing::Span::current();
        let stage = if cache.is_some() {
            "checktx"
        } else {
            "checktx_uncached"
        };
        let stateless = tokio::task::spawn_blocking(move || {
            span.in_scope(|| {
                handle.block_on(Self::build_tx_artifact_for_stage(stage, tx_for_extract))
            })
        });
        let prepared = if fast {
            let context = self
                .checktx_shared_context
                .as_ref()
                .expect("checked shared context")
                .historical_check_context
                .as_ref()
                .clone();
            let snapshot = Arc::new(self.committed_snapshot.clone());
            let tx = tx.clone();
            Some(tokio::spawn(
                async move { prepare_candidate_read(tx, snapshot, context, false).await }
                    .instrument(tracing::Span::current()),
            ))
        } else {
            None
        };
        let historical = if !fast {
            let state = self.state.clone();
            Some(tokio::spawn(
                async move { tx.check_historical(state).await }
                    .instrument(tracing::Span::current()),
            ))
        } else {
            None
        };

        // Stateless rejection wins before any prepared effects can be applied.
        let artifact_result = stateless.await.context("waiting for extraction task")?;
        if let Some(cache) = cache {
            match &artifact_result {
                Ok(artifact) => cache.insert_fully_verified(tx_bytes, artifact.clone())?,
                Err(_) => cache.insert_invalid(tx_bytes)?,
            }
        }
        let artifact = match artifact_result {
            Ok(artifact) => artifact,
            Err(error) => {
                if let Some(task) = prepared {
                    task.abort();
                }
                if let Some(task) = historical {
                    task.abort();
                }
                return Err(error).context("extract stateless failed");
            }
        };
        if let Some(prepared) = prepared {
            let prepared = prepared.await.context("waiting for prepared candidate")??;
            let stamp = self.current_historical_validation_stamp(artifact.tx());
            let artifact = artifact.with_historical_validation_owned(stamp);
            if let Some(cache) = cache {
                cache.insert_fully_verified(tx_bytes, artifact.clone())?;
            }
            let events = self.apply_prepared_checktx(artifact, prepared).await?;
            Ok(events)
        } else {
            historical
                .expect("standard path has a historical task")
                .await
                .context("waiting for historical checks")??;
            let stamp = self.current_historical_validation_stamp(artifact.tx());
            let artifact = artifact.with_historical_validation_owned(stamp);
            if let Some(cache) = cache {
                cache.insert_fully_verified(tx_bytes, artifact.clone())?;
            }
            let events = self.execute_tx_checked_historical(artifact).await?;
            Ok(events)
        }
    }

    async fn deliver_tx_with_verified_stateless(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        historical_context: Option<&HistoricalCheckContext>,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();

        match historical_context {
            Some(context) => {
                check_historical_with_context(Arc::as_ref(&tx), self.state.clone(), context)
                    .await
                    .context("check_stateful failed")?
            }
            None => tx
                .check_historical(self.state.clone())
                .await
                .context("check_stateful failed")?,
        }

        let events = self.execute_tx_checked_historical(artifact).await?;

        Ok(events)
    }

    async fn execute_prepare_candidate(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        historical_context: &HistoricalCheckContext,
    ) -> Result<Vec<abci::Event>> {
        if artifact.has_matching_historical_validation(self.snapshot_version) {
            return self.execute_tx_checked_historical(artifact).await;
        }

        self.deliver_tx_with_verified_stateless(artifact, Some(historical_context))
            .await
    }

    async fn execute_checktx_fast(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        skip_historical: bool,
    ) -> Result<Vec<abci::Event>> {
        let context = self
            .checktx_shared_context
            .as_ref()
            .map(|context| context.historical_check_context.as_ref().clone())
            .context("missing CheckTxSharedContext for fast CheckTx path")?;
        let tx = artifact.tx().clone();
        let snapshot = self.committed_snapshot.clone();
        let handle = tokio::runtime::Handle::current();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_candidate_read_blocking(tx, snapshot, context, skip_historical, handle)
        })
        .await
        .context("joining fast CheckTx prepare task")??;
        self.apply_prepared_checktx(artifact, prepared).await
    }

    async fn apply_prepared_checktx(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        prepared: PreparedCandidateRead,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();

        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");

        let mut deferred_transaction = None;
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                Self::append_block_transaction_to_state(&mut state_tx, height, proto_transaction)
                    .await
                    .context("storing transactions")?;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let _height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                deferred_transaction = Some(proto_transaction);
            }
        }

        let tx_id = tx.id();

        state_tx.put_current_source(Some(tx_id.clone()));

        let gas_used = tx.gas_cost();
        let fee = tx.transaction_body.transaction_parameters.fee;
        if let Some(context) = self.checktx_shared_context.as_ref() {
            Self::apply_checktx_fee_with_context(&mut state_tx, gas_used, fee, context)?;
        } else {
            state_tx.pay_fee(gas_used, fee).await?;
        }

        // CheckTx runs against an ephemeral per-transaction app fork. For the
        // supported fast path, committed-state nullifier checks have already
        // run in the read phase, and same-block conflict resolution is a
        // proposer/block concern. However, the fast path still builds an app
        // fork with concrete state for downstream consumers and tests, so the
        // fork should reflect the same semantic spend set as the slow path.
        for scoped in &prepared.volume_nullifiers {
            state_tx
                .record_volume_nullifier(scoped.day_start, scoped.nullifier)
                .await?;
        }

        state_tx
            .nullify_all(&prepared.spend_nullifiers, tx_id.clone().into())
            .await?;

        for nullifier in &prepared.spend_nullifiers {
            state_tx.record_proto(
                shieldd_sdk_shielded_pool::event::EventNullifierSpent {
                    nullifier: *nullifier,
                }
                .to_proto(),
            );
        }

        for payload in &prepared.sct_payloads {
            if let StatePayload::Note { note, .. } = payload {
                state_tx.record_proto(
                    shieldd_sdk_shielded_pool::event::EventNoteCreated {
                        note_commitment: note.note_commitment,
                    }
                    .to_proto(),
                );
            }
        }

        if let Some(context) = self.checktx_shared_context.as_ref() {
            let base_position_u64: u64 = context.sct_base_position.into();
            for (offset, payload) in prepared.sct_payloads.iter().enumerate() {
                let position = shieldd_sdk_tct::Position::from(base_position_u64 + offset as u64);
                state_tx.record_proto(shieldd_sdk_sct::event::commitment(
                    *payload.commitment(),
                    position,
                    payload.source().clone(),
                ));
            }
        } else {
            let positioned_sct_payloads = self
                .pending_sct_append_log
                .reserve_positions(&state_tx, prepared.sct_payloads.clone())
                .await
                .context("reserving deferred SCT positions")?;
            for (position, payload) in &positioned_sct_payloads {
                state_tx.record_proto(shieldd_sdk_sct::event::commitment(
                    *payload.commitment(),
                    *position,
                    payload.source().clone(),
                ));
            }
            self.pending_sct_append_log
                .append_positioned(positioned_sct_payloads);
        }

        state_tx.stage_routing_actions(prepared.routing_actions.clone());
        append_transaction_audit_effects(&mut state_tx, prepared.audit_effects.clone()).await?;

        let events = state_tx.apply().1;

        if let Some(transaction) = deferred_transaction {
            self.deferred_block_transactions.push(transaction);
        }

        Ok(events)
    }

    async fn apply_prepared_prepare_candidate(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        prepared: PreparedCandidateRead,
        block_state: &mut PrepareBlockLocalState,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();
        let proof_bound_nullifier_count = prepared
            .spend_nullifiers
            .len()
            .saturating_add(prepared.volume_nullifiers.len());
        anyhow::ensure!(
            proof_bound_nullifier_count <= block_state.remaining_nullifier_capacity,
            "proof-bound nullifier capacity exceeded by proposal"
        );
        for nullifier in &prepared.spend_nullifiers {
            anyhow::ensure!(
                !block_state.seen_nullifiers.contains(nullifier),
                "nullifier {} already spent earlier in this proposal",
                nullifier
            );
        }
        for scoped in &prepared.volume_nullifiers {
            anyhow::ensure!(
                !block_state.seen_volume_nullifiers.contains(scoped),
                "daily volume nullifier {} for day {} already spent earlier in this proposal",
                scoped.nullifier,
                scoped.day_start
            );
        }
        // Prepared candidate reads only consult committed state, so they intentionally
        // remain blind to same-block conflicts. Serial apply is the sole resolver for
        // duplicate nullifiers within a single proposal.

        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");

        let mut deferred_transaction = None;
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                Self::append_block_transaction_to_state(&mut state_tx, height, proto_transaction)
                    .await
                    .context("storing transactions")?;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let _height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                deferred_transaction = Some(proto_transaction);
            }
        }

        let tx_id = tx.id();

        state_tx.put_current_source(Some(tx_id.clone()));

        let gas_used = tx.gas_cost();
        let fee = tx.transaction_body.transaction_parameters.fee;
        state_tx.pay_fee(gas_used, fee).await?;

        for scoped in &prepared.volume_nullifiers {
            state_tx
                .record_volume_nullifier(scoped.day_start, scoped.nullifier)
                .await?;
        }
        for nullifier in &prepared.spend_nullifiers {
            state_tx.record_proto(
                shieldd_sdk_shielded_pool::event::EventNullifierSpent {
                    nullifier: *nullifier,
                }
                .to_proto(),
            );
        }

        for payload in &prepared.sct_payloads {
            if let StatePayload::Note { note, .. } = payload {
                state_tx.record_proto(
                    shieldd_sdk_shielded_pool::event::EventNoteCreated {
                        note_commitment: note.note_commitment,
                    }
                    .to_proto(),
                );
            }
        }

        let positioned_sct_payloads = self
            .pending_sct_append_log
            .reserve_positions(&state_tx, prepared.sct_payloads.clone())
            .await
            .context("reserving deferred SCT positions")?;
        for (position, payload) in &positioned_sct_payloads {
            state_tx.record_proto(shieldd_sdk_sct::event::commitment(
                *payload.commitment(),
                *position,
                payload.source().clone(),
            ));
        }
        self.pending_sct_append_log
            .append_positioned(positioned_sct_payloads);

        state_tx.stage_routing_actions(prepared.routing_actions.clone());
        append_transaction_audit_effects(&mut state_tx, prepared.audit_effects.clone()).await?;

        let events = state_tx.apply().1;

        if let Some(transaction) = deferred_transaction {
            self.deferred_block_transactions.push(transaction);
        }
        block_state.remaining_nullifier_capacity -= proof_bound_nullifier_count;
        block_state
            .seen_nullifiers
            .extend(prepared.spend_nullifiers.iter().copied());
        block_state
            .seen_volume_nullifiers
            .extend(prepared.volume_nullifiers.iter().copied());

        Ok(events)
    }

    async fn execute_prepare_candidates_parallel(
        &mut self,
        deduped: Vec<Candidate>,
        historical_context: HistoricalCheckContext,
    ) -> Result<Vec<Candidate>> {
        let concurrency = Self::prepare_proposal_filter_concurrency();
        if concurrency <= 1
            || !deduped
                .iter()
                .all(|candidate| supports_parallel_prepare(candidate.tx()))
        {
            return Ok(Vec::new());
        }

        let snapshot = Arc::new(self.committed_snapshot.clone());
        let mut tasks = tokio::task::JoinSet::new();
        let mut next_to_spawn = 0usize;

        let mut prepared_results = std::iter::repeat_with(|| None)
            .take(deduped.len())
            .collect::<Vec<_>>();

        while next_to_spawn < deduped.len() || !tasks.is_empty() {
            while next_to_spawn < deduped.len() && tasks.len() < concurrency {
                let tx = deduped[next_to_spawn].tx().clone();
                let snapshot = snapshot.clone();
                let context = historical_context.clone();
                let handle = tokio::runtime::Handle::current();
                let skip_historical = deduped[next_to_spawn].artifact().is_some_and(|artifact| {
                    artifact.has_matching_historical_validation(self.snapshot_version)
                });
                let index = next_to_spawn;
                tasks.spawn_blocking(move || {
                    let result = prepare_candidate_read_blocking(
                        tx,
                        Arc::as_ref(&snapshot).clone(),
                        context,
                        skip_historical,
                        handle,
                    );
                    (index, result)
                });

                next_to_spawn += 1;
            }

            if let Some(joined) = tasks.join_next().await {
                let (index, result) = match joined {
                    Ok(result) => result,
                    Err(error) => {
                        tracing::warn!(?error, "parallel prepare candidate task failed");
                        return Ok(Vec::new());
                    }
                };
                prepared_results[index] = Some(result);
            }
        }

        let durable_nullifier_count =
            shieldd_sdk_sct::nullifier_tree::current_leaf_count(Arc::as_ref(&self.state)).await?;
        let remaining_generation_capacity = shieldd_sdk_sct::indexed_nullifier_tree::CAPACITY
            .saturating_sub(durable_nullifier_count);
        let mut block_state = PrepareBlockLocalState {
            remaining_nullifier_capacity: usize::try_from(remaining_generation_capacity)
                .unwrap_or(usize::MAX)
                .min(MAX_BLOCK_NULLIFIER_COUNT),
            ..Default::default()
        };
        let mut included_candidates = Vec::new();
        for (candidate, prepared_result) in deduped.into_iter().zip(prepared_results.into_iter()) {
            let Some(prepared_result) = prepared_result else {
                tracing::warn!("missing prepared candidate result, falling back to exclusion");
                continue;
            };
            let prepared = match prepared_result {
                Ok(prepared) => prepared,
                Err(error) => {
                    tracing::debug!(?error, "parallel prepare candidate rejected");
                    continue;
                }
            };

            match self
                .apply_prepared_prepare_candidate(
                    candidate
                        .verified_artifact()
                        .expect("prepared candidate must be proof verified"),
                    prepared,
                    &mut block_state,
                )
                .await
            {
                Ok(_) => {
                    included_candidates.push(candidate);
                }
                Err(error) => {
                    tracing::debug!(?error, "serial apply rejected prepared candidate");
                }
            }
        }

        Ok(included_candidates)
    }

    async fn append_block_transaction_to_state<S>(
        state_tx: &mut S,
        height: u64,
        transaction: shieldd_sdk_proto::core::transaction::v1::Transaction,
    ) -> Result<()>
    where
        S: StateWrite + StateReadExt,
    {
        let mut transactions_response = state_tx.transactions_by_height(height).await?;

        transactions_response.transactions.push(transaction);

        let encoded = transactions_response.encode_to_vec();

        state_tx.nonverifiable_put_raw(
            state_key::cometbft_data::transactions_by_height(height).into(),
            encoded,
        );

        Ok(())
    }

    async fn materialize_pending_sct_append_log<S>(&mut self, state_tx: &mut S) -> Result<()>
    where
        S: StateWrite
            + shieldd_sdk_sct::component::tree::SctManager
            + shieldd_sdk_shielded_pool::component::NoteManager,
    {
        #[cfg(feature = "benchmark-helpers")]
        let materialize_start = Instant::now();
        let entries = self.pending_sct_append_log.take_entries();
        if entries.is_empty() {
            return Ok(());
        }

        let mut note_payloads = state_tx.pending_note_payloads();
        let mut rolled_up_payloads = state_tx.pending_rolled_up_payloads();
        let mut volume_accumulator_payloads = state_tx.pending_volume_accumulator_payloads();
        let mut last_position = None;
        let mut sct_entries = Vec::with_capacity(entries.len());

        for (position, payload) in entries {
            debug_assert!(
                last_position
                    .map(|previous| previous <= position)
                    .unwrap_or(true),
                "deferred SCT append log should already be position-sorted"
            );
            last_position = Some(position);

            let commitment = *payload.commitment();
            sct_entries.push((position, commitment));

            match payload {
                StatePayload::Note { source, note } => {
                    note_payloads.push_back((position, *note, source));
                }
                StatePayload::RolledUp { commitment, .. } => {
                    rolled_up_payloads.push_back((position, commitment));
                }
                StatePayload::VolumeAccumulator { source, payload } => {
                    volume_accumulator_payloads.push_back((position, *payload, source));
                }
            }
        }

        state_tx.finalize_sct_block_forget(sct_entries).await?;

        #[cfg(feature = "benchmark-helpers")]
        let pending_payload_start = Instant::now();
        state_tx.object_put(
            shieldd_sdk_shielded_pool::state_key::pending_notes(),
            note_payloads,
        );
        state_tx.object_put(
            shieldd_sdk_shielded_pool::state_key::pending_rolled_up_payloads(),
            rolled_up_payloads,
        );
        state_tx.object_put(
            shieldd_sdk_shielded_pool::state_key::pending_volume_accumulator_payloads(),
            volume_accumulator_payloads,
        );
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::DeferredSctPendingPayload,
            pending_payload_start.elapsed(),
        );
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::DeferredSctMaterialize,
            materialize_start.elapsed(),
        );

        Ok(())
    }

    async fn flush_deferred_block_transactions(&mut self) -> Result<()> {
        if self.block_tx_indexing_mode != BlockTxIndexingMode::DeferredBatch
            || self.deferred_block_transactions.is_empty()
        {
            return Ok(());
        }

        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");
        let height = state_tx.get_block_height().await?;
        let mut transactions_response = state_tx.transactions_by_height(height).await?;
        transactions_response
            .transactions
            .append(&mut self.deferred_block_transactions);
        state_tx.nonverifiable_put_raw(
            state_key::cometbft_data::transactions_by_height(height).into(),
            transactions_response.encode_to_vec(),
        );
        state_tx.apply();
        Ok(())
    }

    async fn execute_tx_checked_historical(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();

        // At this point, the stateful checks should have completed,
        // leaving us with exclusive access to the Arc<State>.

        let tx_id = tx.id();
        let state_arc_strong_count = Arc::strong_count(&self.state);
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "CheckTx could not begin state transaction after historical checks: tx_id={}, action_count={}, state_arc_strong_count={}",
                    tx_id,
                    tx.actions().count(),
                    state_arc_strong_count,
                )
            })?;

        // Index the transaction:

        let mut deferred_transaction = None;
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                Self::append_block_transaction_to_state(&mut state_tx, height, proto_transaction)
                    .await
                    .context("storing transactions")?;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let _height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                deferred_transaction = Some(proto_transaction);
            }
        }

        check_and_execute(Arc::as_ref(&artifact), &mut state_tx)
            .await
            .context("executing transaction")?;

        // At this point, we've completed execution successfully with no errors,
        // so we can apply the transaction to the State. Otherwise, we'd have
        // bubbled up an error and dropped the StateTransaction.

        let events = state_tx.apply().1;

        if let Some(transaction) = deferred_transaction {
            self.deferred_block_transactions.push(transaction);
        }

        Ok(events)
    }

    #[tracing::instrument(skip_all, fields(height = %end_block.height))]
    pub async fn end_block(&mut self, end_block: &request::EndBlock) -> Vec<abci::Event> {
        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions in end_block");
        let mut state_tx = StateDelta::new(self.state.clone());
        self.materialize_pending_sct_append_log(&mut state_tx)
            .await
            .expect("must be able to materialize deferred SCT payloads in end_block");

        tracing::debug!("running app components' `end_block` hooks");
        let mut arc_state_tx = Arc::new(state_tx);
        Sct::end_block(&mut arc_state_tx, end_block).await;
        ShieldedPool::end_block(&mut arc_state_tx, end_block).await;
        Ibc::end_block(&mut arc_state_tx, end_block).await;
        FeeComponent::end_block(&mut arc_state_tx, end_block).await;
        Compliance::end_block(&mut arc_state_tx, end_block).await;
        let mut state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");
        tracing::debug!("finished app components' `end_block` hooks");

        let current_height = state_tx
            .get_block_height()
            .await
            .expect("able to get block height in end_block");
        let current_epoch = state_tx
            .get_current_epoch()
            .await
            .expect("able to get current epoch in end_block");

        let is_end_epoch = current_epoch.is_scheduled_epoch_end(
            current_height,
            state_tx
                .get_epoch_duration_parameter()
                .await
                .expect("able to get epoch duration in end_block"),
        ) || state_tx.is_epoch_ending_early().await;

        if is_end_epoch {
            tracing::info!(%is_end_epoch, ?current_height, "ending epoch");

            let mut arc_state_tx = Arc::new(state_tx);

            Sct::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Sct component");
            Ibc::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on IBC component");
            ShieldedPool::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on shielded pool component");
            FeeComponent::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Fee component");

            let mut state_tx = Arc::try_unwrap(arc_state_tx)
                .expect("components did not retain copies of shared state");

            state_tx
                .finish_epoch()
                .await
                .expect("must be able to finish compact block");

            // set the epoch for the next block
            shieldd_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
                &mut state_tx,
                current_height + 1,
                Epoch {
                    index: current_epoch.index + 1,
                    start_height: current_height + 1,
                },
            );

            self.apply(state_tx)
        } else {
            // set the epoch for the next block
            shieldd_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
                &mut state_tx,
                current_height + 1,
                current_epoch,
            );

            state_tx
                .finish_block()
                .await
                .expect("must be able to finish compact block");

            self.apply(state_tx)
        }
    }

    /// Commits the application state to persistent storage,
    /// returning the new root hash and storage version.
    ///
    /// This method also resets `self` as if it were constructed
    /// as an empty state over top of the newly written storage.
    pub async fn commit(&mut self, storage: Storage) -> RootHash {
        self.state
            .ensure_nullifier_block_materialized()
            .expect("cannot commit an open nullifier block");

        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions before commit");

        // We need to extract the State we've built up to commit it.  Fill in a dummy state.
        let dummy_state = StateDelta::new(storage.latest_snapshot());
        let state = Arc::try_unwrap(std::mem::replace(&mut self.state, Arc::new(dummy_state)))
            .expect("we have exclusive ownership of the State at commit()");

        // Commit the pending writes, clearing the state.

        let jmt_root = storage
            .commit(state)
            .await
            .expect("must be able to successfully commit to storage");

        tracing::debug!(?jmt_root, "finished committing state");

        // Get the latest version of the state, now that we've committed it.

        let latest_snapshot = storage.latest_snapshot();
        self.snapshot_version = latest_snapshot.version();
        self.committed_snapshot = latest_snapshot.clone();
        self.state = Arc::new(StateDelta::new(latest_snapshot));
        self.pending_sct_append_log.clear();

        jmt_root
    }
}

#[async_trait]
pub trait StateReadExt: StateRead {
    async fn get_chain_id(&self) -> Result<String> {
        let raw_chain_id = self
            .get_raw(state_key::data::chain_id())
            .await?
            .expect("chain id is always set");

        Ok(String::from_utf8_lossy(&raw_chain_id).to_string())
    }

    /// Checks a provided chain_id against the chain state.
    ///
    /// Passes through if the provided chain_id is empty or matches, and
    /// otherwise errors.
    async fn check_chain_id(&self, provided: &str) -> Result<()> {
        let chain_id = self
            .get_chain_id()
            .await
            .context(format!("error getting chain id: '{provided}'"))?;
        if provided.is_empty() || provided == chain_id {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "provided chain_id {} does not match chain_id {}",
                provided,
                chain_id
            ))
        }
    }

    /// Gets the chain revision number, from the chain ID
    async fn get_revision_number(&self) -> Result<u64> {
        let cid_str = self.get_chain_id().await?;

        Ok(ChainId::from_string(&cid_str).version())
    }

    /// Returns the set of app parameters
    async fn get_app_params(&self) -> Result<AppParameters> {
        let chain_id = self.get_chain_id().await?;
        let compliance_params = self.get_compliance_params().await?;
        let ibc_params = self.get_ibc_params().await?;
        let fee_params = self.get_fee_params().await?;
        let sct_params = self.get_sct_params().await?;
        let shielded_pool_params = self.get_shielded_pool_params().await?;

        Ok(AppParameters {
            chain_id,
            compliance_params,
            fee_params,
            ibc_params,
            sct_params,
            shielded_pool_params,
        })
    }

    async fn transactions_by_height(
        &self,
        block_height: u64,
    ) -> Result<TransactionsByHeightResponse> {
        let transactions = match self
            .nonverifiable_get_raw(
                state_key::cometbft_data::transactions_by_height(block_height).as_bytes(),
            )
            .await?
        {
            Some(transactions) => transactions,
            None => TransactionsByHeightResponse {
                transactions: vec![],
                block_height,
            }
            .encode_to_vec(),
        };

        Ok(TransactionsByHeightResponse::decode(&transactions[..])?)
    }
}

impl<
        T: StateRead
            + shieldd_sdk_fee::component::StateReadExt
            + shieldd_sdk_sct::component::clock::EpochRead
            + shieldd_sdk_ibc::component::StateReadExt
            + ?Sized,
    > StateReadExt for T
{
}

#[async_trait]
pub trait StateWriteExt: StateWrite {
    /// Sets the chain ID.
    fn put_chain_id(&mut self, chain_id: String) {
        self.put_raw(state_key::data::chain_id().into(), chain_id.into_bytes());
    }

    /// Stores the transactions that occurred during a CometBFT block.
    /// This is used to create a durable transaction log for clients to retrieve;
    /// the CometBFT `get_block_by_height` RPC call will only return data for blocks
    /// since the last checkpoint, so we need to store the transactions separately.
    async fn put_block_transaction(
        &mut self,
        height: u64,
        transaction: shieldd_sdk_proto::core::transaction::v1::Transaction,
    ) -> Result<()> {
        // Extend the existing transactions with the new one.
        let mut transactions_response = self.transactions_by_height(height).await?;
        transactions_response.transactions = transactions_response
            .transactions
            .into_iter()
            .chain(std::iter::once(transaction))
            .collect();

        self.nonverifiable_put_raw(
            state_key::cometbft_data::transactions_by_height(height).into(),
            transactions_response.encode_to_vec(),
        );
        Ok(())
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}

#[cfg(test)]
mod tests {
    mod proof_acceptance_tests;

    use std::collections::BTreeMap;
    use std::ops::Deref;
    use std::sync::Arc;

    use anyhow::{anyhow, Context, Result};
    use ark_ff::Zero;
    use ark_serialize::CanonicalSerialize;
    use cnidarium::{ArcStateDeltaExt as _, StateDelta, StateRead, StateWrite, TempStorage};
    use decaf377::{Fq, Fr};
    use decaf377_rdsa as rdsa;
    use futures::StreamExt as _;
    use proptest::prelude::*;
    use prost::bytes::Bytes;
    use rand_core::OsRng;
    use sha2::Digest as _;
    use shieldd_sdk_asset::{asset, Value, BASE_ASSET_DENOM, BASE_ASSET_ID};
    use shieldd_sdk_compact_block::StatePayload;
    use shieldd_sdk_compliance::genesis::{GenesisUserRegistration, NativeAssetRegistration};
    use shieldd_sdk_compliance::registry::ComplianceRegistryWrite as _;
    use shieldd_sdk_compliance::structs::{
        OrbisCapabilityCertificate, UserRegistrationGrant, UserRegistrationGrantBody,
    };
    use shieldd_sdk_compliance::{
        derive_regulated_nullifier_key, AssetPolicy, ComplianceLeaf, MsgRegisterUser,
    };
    use shieldd_sdk_fee::Fee;
    use shieldd_sdk_keys::{test_keys, Address};
    use shieldd_sdk_mock_client::MockClient;
    use shieldd_sdk_mock_consensus::TestNode;
    use shieldd_sdk_num::Amount;
    #[cfg(feature = "orbis-dev-srs")]
    use shieldd_sdk_proof_aggregation::srs_id;
    use shieldd_sdk_proof_aggregation::{
        app_verify_family_code, AggregateBundle, AppVerifyCallId, DevSrs, FamilyAggregate,
        ProofFamilyId, AGGREGATE_PROTOCOL_VERSION, DEFAULT_DEV_SRS_ID,
    };
    use shieldd_sdk_proof_params::batch::BatchItem;
    use shieldd_sdk_proto::DomainType;
    use shieldd_sdk_sct::component::clock::{EpochManager as _, EpochRead as _};
    use shieldd_sdk_sct::component::tree::{SctManager as _, SctRead as _};
    use shieldd_sdk_sct::component::StateWriteExt as _;
    use shieldd_sdk_sct::epoch::Epoch;
    use shieldd_sdk_sct::nullifier_generation::{
        empty_history_head, NullifierWindow, PROTOCOL_VERSION,
    };
    use shieldd_sdk_sct::params::SctParameters;
    use shieldd_sdk_sct::{CommitmentSource, Nullifier};
    use shieldd_sdk_shielded_pool::component::NoteManager as _;
    use shieldd_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::build_transfer_action_and_public_without_proof;
    use shieldd_sdk_shielded_pool::{genesis::Allocation, ShieldedInputPlan, ShieldedOutputPlan};
    use shieldd_sdk_tct as tct;
    use shieldd_sdk_transaction::{
        memo::{MemoCiphertext, MemoPlaintext, MEMO_CIPHERTEXT_LEN_BYTES},
        plan::MemoPlan,
        Action, ActionPlan, Transaction, TransactionParameters, TransactionPlan,
    };
    use shieldd_sdk_txhash::AuthorizingData;
    use tendermint::v0_37::abci::{request, response};
    use tendermint::{account, block, Hash, Time};

    use super::PrepareBlockLocalState;
    use crate::action_handler::transaction::{
        prepare_candidate_read, prepare_candidate_read_blocking, supports_parallel_prepare,
        HistoricalCheckContext,
    };

    use crate::action_handler::AppActionHandler;
    use crate::app::CheckTxSharedContext;
    use crate::app::ProposalArtifactSidecar;
    use crate::app::{candidate_digest_from_hashes, CandidateEnvelope};
    use crate::genesis::{AppState, Content};
    use crate::server::consensus::{Consensus, ConsensusService};
    use crate::stateless_cache::{CacheEntry, StatelessCache, TxArtifact};
    use crate::SUBSTORE_PREFIXES;

    use super::{
        AggregateBundleFamilyEstimate, App, BlockSctAppendLog, BlockTxIndexingMode, StateReadExt,
        AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES, AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
    };

    fn test_nullifier_window() -> NullifierWindow {
        NullifierWindow {
            protocol_version: PROTOCOL_VERSION,
            current_generation: 0,
            recent_position_floor: 0,
            archived_generation_count: 0,
            archived_history_head: empty_history_head(),
        }
    }

    const SRS_ID_MISMATCH: &str = if cfg!(feature = "orbis-dev-srs") {
        "Orbis integration SnarkPack SRS id mismatch"
    } else {
        "test/fuzz SnarkPack SRS id mismatch"
    };

    #[cfg(feature = "orbis-dev-srs")]
    #[test]
    fn orbis_dev_srs_selects_only_the_insecure_integration_fixture() -> Result<()> {
        let srs = super::shipping_srs()?;
        assert!(!srs.is_registered());
        assert_eq!(srs_id(&srs), DEFAULT_DEV_SRS_ID);

        let selected = super::shipping_srs_for_id(&DEFAULT_DEV_SRS_ID)?;
        assert!(!selected.is_registered());
        assert_eq!(srs_id(&selected), DEFAULT_DEV_SRS_ID);

        let error = super::shipping_srs_for_id(&[0u8; 32])
            .expect_err("integration fixture must reject every other SRS id");
        assert!(error
            .to_string()
            .contains("Orbis integration SnarkPack SRS id mismatch"));

        Ok(())
    }

    fn rolled_up_payload(value: u64) -> StatePayload {
        StatePayload::RolledUp {
            source: CommitmentSource::transaction(),
            commitment: tct::StateCommitment(Fq::from(value)),
        }
    }

    #[tokio::test]
    async fn failed_transaction_drops_all_staged_effects() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut base_state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut base_state).await?;
        let mut state = Arc::new(base_state);
        let nullifier = Nullifier(Fq::from(71u64));
        let source = CommitmentSource::Transaction {
            id: Some([7u8; 32]),
        };
        let payload = shieldd_sdk_shielded_pool::NotePayload {
            note_commitment: tct::StateCommitment(Fq::from(72u64)),
            ..shieldd_sdk_shielded_pool::NotePayload::dummy()
        };
        let unrelated_effect_key = "fv/transaction/staged-effect".to_string();

        let execution_result: Result<()> = async {
            let mut state_tx = state
                .try_begin_transaction()
                .expect("test state must have unique ownership");
            state_tx.put_block_height(42);
            state_tx
                .nullify_all(std::slice::from_ref(&nullifier), source.clone())
                .await?;
            state_tx.add_note_payload(payload, source).await;
            state_tx.put_raw(unrelated_effect_key.clone(), vec![1u8]);

            assert_eq!(
                state_tx
                    .pending_nullifiers()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                vec![nullifier]
            );
            assert_eq!(state_tx.pending_note_payloads().len(), 1);
            assert_eq!(
                state_tx.get_raw(unrelated_effect_key.as_str()).await?,
                Some(vec![1u8])
            );

            Err(anyhow!("later action failed"))
        }
        .await;

        assert!(execution_result.is_err());
        assert!(state.pending_nullifiers().is_empty());
        assert!(state.pending_note_payloads().is_empty());
        assert!(!shieldd_sdk_sct::nullifier_tree::is_spent(Arc::as_ref(&state), nullifier).await?);
        assert_eq!(state.get_raw(unrelated_effect_key.as_str()).await?, None);

        Ok(())
    }

    #[test]
    fn proposal_tx_count_policy_is_fixed_at_boundary() {
        let mut candidates = vec![Bytes::new(); super::MAX_BLOCK_TX_COUNT + 1];
        super::truncate_prepare_candidates(&mut candidates);
        assert_eq!(candidates.len(), super::MAX_BLOCK_TX_COUNT);
        assert!(super::process_proposal_tx_count_allowed(
            super::MAX_BLOCK_TX_COUNT
        ));
        assert!(!super::process_proposal_tx_count_allowed(
            super::MAX_BLOCK_TX_COUNT + 1
        ));
    }

    #[test]
    fn proposal_payload_size_policy_is_fixed_at_boundary() {
        assert_eq!(super::prepare_proposal_payload_limit(-1), 0);
        assert_eq!(super::prepare_proposal_payload_limit(0), 0);
        assert_eq!(
            super::prepare_proposal_payload_limit(super::MAX_BLOCK_TXS_PAYLOAD_BYTES as i64),
            super::MAX_BLOCK_TXS_PAYLOAD_BYTES as u64
        );
        assert_eq!(
            super::prepare_proposal_payload_limit(super::MAX_BLOCK_TXS_PAYLOAD_BYTES as i64 + 1),
            super::MAX_BLOCK_TXS_PAYLOAD_BYTES as u64
        );
        assert!(super::process_proposal_payload_size_allowed(
            super::MAX_BLOCK_TXS_PAYLOAD_BYTES
        ));
        assert!(!super::process_proposal_payload_size_allowed(
            super::MAX_BLOCK_TXS_PAYLOAD_BYTES + 1
        ));
    }

    #[test]
    fn proposal_nullifier_count_policy_is_fixed_at_boundary() {
        assert!(super::block_nullifier_count_allowed(
            super::MAX_BLOCK_NULLIFIER_COUNT
        ));
        assert!(!super::block_nullifier_count_allowed(
            super::MAX_BLOCK_NULLIFIER_COUNT + 1
        ));
        assert!(!super::block_nullifier_count_allowed(usize::MAX));
    }

    #[test]
    fn proposal_transaction_size_policy_is_fixed_at_boundary() {
        assert!(super::transaction_size_allowed(
            super::MAX_TRANSACTION_SIZE_BYTES
        ));
        assert!(!super::transaction_size_allowed(
            super::MAX_TRANSACTION_SIZE_BYTES + 1
        ));
    }

    #[test]
    fn proof_worker_concurrency_is_bounded_for_all_hardware_sizes() {
        assert_eq!(App::proof_family_ids().len(), 4);
        assert_eq!(super::MAX_CONCURRENT_AGGREGATE_SEGMENTS, 2);
        assert_eq!(super::MAX_CONCURRENT_AGGREGATE_VERIFY_CALLS, 4);
        assert!(super::MAX_CONCURRENT_AGGREGATE_SEGMENTS * App::proof_family_ids().len() <= 8);
    }

    #[tokio::test]
    async fn structured_join_drain_waits_for_siblings_after_error() {
        let sibling_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sibling_finished_for_task = sibling_finished.clone();
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async { Err::<(), anyhow::Error>(anyhow!("injected early failure")) });
        tasks.spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(25));
            sibling_finished_for_task.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok::<(), anyhow::Error>(())
        });

        let result = super::drain_joinset_results(&mut tasks, "injected task panic").await;
        assert!(result.is_err());
        assert!(
            sibling_finished.load(std::sync::atomic::Ordering::SeqCst),
            "drain must await sibling work before returning the first error"
        );
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn oversized_checktx_bytes_reject_before_decode_or_cache() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut app = App::new(storage.latest_snapshot());
        let cache = StatelessCache::new();
        let maximum_transaction_size = super::MAX_TRANSACTION_SIZE_BYTES;
        let oversized = vec![
            0xff;
            maximum_transaction_size
                .checked_add(1)
                .context("maximum transaction size must fit in usize")?
        ];

        let error = app
            .deliver_tx_bytes(&oversized, Some(&cache))
            .await
            .expect_err("oversized CheckTx bytes must reject before decoding or cache admission");
        assert_eq!(
            error.to_string(),
            format!(
                "transaction size {} exceeds maximum {maximum_transaction_size}",
                oversized.len()
            ),
            "oversized CheckTx rejection must come from the pre-decode size guard"
        );

        Ok(())
    }

    #[tokio::test]
    async fn artifact_extraction_cannot_bypass_action_stateless_checks() -> Result<()> {
        let action_anchor = tct::Tree::default().root();
        let balance_commitment = shieldd_sdk_asset::Balance::default().commit(Fr::from(1u64));
        let inputs = (0..8)
            .map(|index| shieldd_sdk_shielded_pool::NoteReshapeInputBody {
                nullifier: Nullifier(Fq::from(10u64 + index)),
                rk: rdsa::VerificationKey::from(rdsa::SigningKey::<rdsa::SpendAuth>::from(
                    Fr::from(20u64 + index),
                )),
                encrypted_backref: shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                    [u8::try_from(index + 1).expect("small index"); 48],
                )
                .expect("fixed-size encrypted backref"),
                history_required: false,
            })
            .collect();
        let note_reshape = shieldd_sdk_shielded_pool::NoteReshape {
            body: shieldd_sdk_shielded_pool::NoteReshapeBody {
                family_id: shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne,
                anchor: action_anchor,
                balance_commitment,
                inputs,
                outputs: vec![shieldd_sdk_shielded_pool::NoteReshapeOutputBody {
                    note_payload: shieldd_sdk_shielded_pool::NotePayload {
                        note_commitment: tct::StateCommitment(Fq::from(30u64)),
                        ..shieldd_sdk_shielded_pool::NotePayload::dummy()
                    },
                    wrapped_memo_key: shieldd_sdk_keys::symmetric::WrappedMemoKey([31u8; 48]),
                    ovk_wrapped_key: shieldd_sdk_keys::symmetric::OvkWrappedKey([32u8; 48]),
                }],
                routing_tag: Default::default(),
                routing_parameter_set_id: Fq::from(0u64),
                asset_anchor: tct::StateCommitment(Fq::from(0u64)),
                compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
            },
            auth_sigs: vec![[0u8; 64].into(); 8],
            proof: shieldd_sdk_shielded_pool::NoteReshapeProof::default(),
        };
        let mut invalid_auth = Transaction {
            transaction_body: shieldd_sdk_transaction::TransactionBody {
                actions: vec![Action::NoteReshape(note_reshape)],
                memo: Some(MemoCiphertext([0u8; MEMO_CIPHERTEXT_LEN_BYTES])),
                nullifier_window: Some(test_nullifier_window()),
                ..Default::default()
            },
            anchor: action_anchor,
            ..Default::default()
        };
        let binding_signing_key = rdsa::SigningKey::<rdsa::Binding>::from(Fr::from(1u64));
        invalid_auth.binding_sig =
            binding_signing_key.sign_deterministic(invalid_auth.auth_hash().as_bytes());

        let mut mismatched_anchor = invalid_auth.clone();
        mismatched_anchor.anchor = tct::Root(tct::structure::Hash::new(Fq::from(987_654u64)));
        let error = match App::build_tx_artifacts_extracted_for_stage_public(
            "artifact_stateless_regression_anchor",
            &[Arc::new(mismatched_anchor)],
        )
        .await
        {
            Ok(_) => panic!("artifact extraction must enforce action/context anchor equality"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("body anchor does not match transaction anchor"),
            "unexpected anchor rejection: {error:#}"
        );

        let error = match App::build_tx_artifacts_extracted_for_stage_public(
            "artifact_stateless_regression_auth",
            &[Arc::new(invalid_auth)],
        )
        .await
        {
            Ok(_) => panic!("artifact extraction must verify spend authorization signatures"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("auth signature 0 failed to verify"),
            "unexpected authorization rejection: {error:#}"
        );

        Ok(())
    }

    #[test]
    fn fee_funding_extraction_rejects_identity_randomized_key() {
        let (mut transfer, _, context) = build_transfer_action_and_public_without_proof(true);
        transfer.body.proof_context = shieldd_sdk_shielded_pool::TransferProofContext::FeeFunding;
        transfer.body.volume_accumulator =
            shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding();
        let identity_sk = rdsa::SigningKey::<rdsa::SpendAuth>::from(Fr::from(0u64));
        transfer.body.inputs[0].rk = rdsa::VerificationKey::from(identity_sk.clone());
        let different_message = b"different fee funding authorization hash";
        assert_ne!(&different_message[..], context.effect_hash.as_ref());
        transfer.auth_sigs[0] = identity_sk.sign_deterministic(different_message);
        transfer.body.inputs[0]
            .rk
            .verify(context.effect_hash.as_ref(), &transfer.auth_sigs[0])
            .expect("the pinned RDSA primitive admits identity keys across messages");
        let fee_funding = shieldd_sdk_transaction::FeeFunding { transfer };

        let error = match super::extract_fee_funding_proof_item(&fee_funding, &context) {
            Ok(_) => panic!("fee funding must use the shared identity-RK rejection"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("randomized spend key 0 must not be identity"),
            "unexpected rejection reason: {error:#}"
        );
    }

    async fn delete_nv_prefix<S>(state: &mut S, prefix: &[u8]) -> Result<()>
    where
        S: StateRead + StateWrite + ?Sized,
    {
        let mut keys = Vec::new();
        {
            let stream = state.nonverifiable_prefix_raw(prefix);
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                let (key, _) = item?;
                keys.push(key);
            }
        }
        for key in keys {
            state.nonverifiable_delete(key);
        }
        Ok(())
    }

    async fn setup_test_txs(
        tx_count: usize,
    ) -> Result<(TempStorage, TestNode<ConsensusService>, Vec<Vec<u8>>)> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;

        let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
            raw_amount: 1_000_000u128.into(),
            raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
            address: test_keys::ADDRESS_0.to_owned(),
        })
        .take(tx_count)
        .collect();

        let app_state_bytes = serde_json::to_vec(&AppState::Content(Content {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            shielded_pool_content: shieldd_sdk_shielded_pool::genesis::Content {
                allocations,
                ..Default::default()
            },
            ..Default::default()
        }))?;

        let consensus = Consensus::new(storage.as_ref().clone());
        let initial_time = tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?;
        let mut test_node = TestNode::builder()
            .single_validator()
            .app_state(app_state_bytes)
            .with_initial_timestamp(initial_time)
            .init_chain(consensus)
            .await?;

        test_node.block().execute().await?;

        let client = Arc::new(
            MockClient::new(test_keys::SPEND_KEY.clone())
                .with_sync_to_storage(&storage)
                .await?,
        );

        let notes: Vec<_> = client
            .notes
            .values()
            .filter(|note| {
                note.asset_id() == *BASE_ASSET_ID
                    && note.address() == test_keys::ADDRESS_0.deref().clone()
            })
            .cloned()
            .take(tx_count)
            .collect();
        let mut txs = Vec::with_capacity(tx_count);
        for note in notes {
            let spend = ShieldedInputPlan::new(
                &mut OsRng,
                note.clone(),
                client
                    .position(note.commit())
                    .ok_or_else(|| anyhow!("note position was unknown to mock client"))?,
            );
            let send_amount = Amount::from(1u64);
            let change_amount = note.amount() - send_amount;
            let output = ShieldedOutputPlan::new(
                &mut OsRng,
                Value {
                    amount: send_amount,
                    asset_id: note.asset_id(),
                },
                test_keys::ADDRESS_1.deref().clone(),
            );
            let change = ShieldedOutputPlan::new(
                &mut OsRng,
                Value {
                    amount: change_amount,
                    asset_id: note.asset_id(),
                },
                note.address(),
            );

            let intent = shieldd_sdk_mock_client::TransactionIntent {
                actions: vec![shieldd_sdk_mock_client::TransferIntent {
                    spends: vec![spend.into()],
                    outputs: vec![output.into(), change.into()],
                    value_blinding: Fr::from(1u64),
                }
                .into()],
                memo: Some(MemoPlan::new(
                    &mut OsRng,
                    MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
                )),
                fee_funding: None,
                transaction_parameters: TransactionParameters {
                    chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                    ..Default::default()
                },
                nullifier_window: Some(test_nullifier_window()),
            };

            let tx = client
                .witness_auth_build(
                    &client
                        .complete_intent(intent, storage.latest_snapshot())
                        .await?,
                )
                .await?;
            txs.push(tx.encode_to_vec());
        }

        Ok((storage, test_node, txs))
    }

    #[tokio::test]
    async fn regulated_genesis_note_transfers_through_consensus_and_compact_block() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let authority_vk = rdsa::VerificationKey::from(test_keys::SPEND_KEY.spend_auth_key());
        let regulated_denom = "wregulated_usd";
        let regulated_asset_id = asset::REGISTRY.parse_unit(regulated_denom).id();
        let native_asset = NativeAssetRegistration {
            asset_id: regulated_asset_id,
            is_regulated: true,
            dk_pub: Some(decaf377::Element::GENERATOR.vartime_compress().0),
            registration_authority_vk: Some(authority_vk),
            seizure_authority_vk: Some(authority_vk),
            ring_pk: Some(decaf377::Element::GENERATOR.vartime_compress().0),
            ring_id: "test-ring".to_owned(),
            policy_id: "test-policy".to_owned(),
            permission: "read".to_owned(),
            resource: "document".to_owned(),
        };
        let policy = native_asset.asset_policy()?;
        let make_leaf = |address: Address| {
            let rnk_dh_pk = address.diversified_generator().clone();
            let rnk = derive_regulated_nullifier_key(
                test_keys::FULL_VIEWING_KEY.incoming(),
                &address,
                regulated_asset_id,
                decaf377::Element::GENERATOR,
                rnk_dh_pk,
            )?;
            ComplianceLeaf::registered_from_rnk(
                address,
                regulated_asset_id,
                decaf377::Element::GENERATOR,
                rnk_dh_pk,
                rnk,
            )
        };
        let genesis_leaf = make_leaf(test_keys::ADDRESS_0.deref().clone())?;
        let runtime_leaf = make_leaf(test_keys::ADDRESS_1.deref().clone())?;
        let app_state_bytes = serde_json::to_vec(&AppState::Content(Content {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            compliance_content: shieldd_sdk_compliance::genesis::Content {
                native_assets: vec![native_asset],
                user_registrations: vec![GenesisUserRegistration {
                    capability_certificate: OrbisCapabilityCertificate::sign_for_test(
                        TestNode::<()>::CHAIN_ID,
                        &genesis_leaf,
                        &policy,
                        decaf377::Fr::from(1u64),
                    )?,
                    leaf: genesis_leaf,
                }],
                ..Default::default()
            },
            shielded_pool_content: shieldd_sdk_shielded_pool::genesis::Content {
                allocations: vec![
                    Allocation {
                        raw_amount: 1_000_000u128.into(),
                        raw_denom: regulated_denom.to_string(),
                        address: test_keys::ADDRESS_0.deref().clone(),
                    },
                    Allocation {
                        raw_amount: 1_000_000u128.into(),
                        raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
                        address: test_keys::ADDRESS_0.deref().clone(),
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        }))?;

        let consensus = Consensus::new(storage.as_ref().clone());
        let mut test_node = TestNode::builder()
            .single_validator()
            .app_state(app_state_bytes)
            .with_initial_timestamp(tendermint::Time::parse_from_rfc3339(
                "2026-01-01T00:00:00Z",
            )?)
            .init_chain(consensus)
            .await?;
        test_node.block().execute().await?;

        let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
            .with_sync_to_storage(&storage)
            .await?;
        let grant_body = UserRegistrationGrantBody {
            leaf: runtime_leaf.clone(),
            policy_id: "test-policy".to_owned(),
            valid_until_unix: 4_102_444_800,
            nonce: vec![1u8; 16],
        };
        let registration = MsgRegisterUser {
            leaf: runtime_leaf.clone(),
            capability_certificate: Some(OrbisCapabilityCertificate::sign_for_test(
                TestNode::<()>::CHAIN_ID,
                &runtime_leaf,
                &policy,
                decaf377::Fr::from(1u64),
            )?),
            grant: Some(UserRegistrationGrant {
                signature: test_keys::SPEND_KEY
                    .spend_auth_key()
                    .sign(OsRng, &grant_body.signing_bytes()),
                body: grant_body,
            }),
        };
        let registration_plan = TransactionPlan {
            actions: vec![ActionPlan::from(registration)],
            memo: None,
            fee_funding: None,
            transaction_parameters: TransactionParameters {
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                ..Default::default()
            },
            nullifier_window: None,
        };
        let registration_tx = client.witness_auth_build(&registration_plan).await?;
        test_node
            .block()
            .with_data(vec![registration_tx.encode_to_vec()])
            .execute()
            .await?;
        client.sync_to_latest(storage.latest_snapshot()).await?;
        let note = client
            .notes
            .values()
            .find(|note| {
                note.asset_id() == regulated_asset_id
                    && note.address() == test_keys::ADDRESS_0.deref().clone()
            })
            .cloned()
            .context("regulated genesis note must be recoverable")?;
        let spent_commitment = note.commit();
        let position = client
            .position(note.commit())
            .context("regulated genesis note position must be known")?;
        let spend = ShieldedInputPlan::new(&mut OsRng, note.clone(), position);
        let fee_note = client
            .notes
            .values()
            .find(|note| {
                note.asset_id() == *BASE_ASSET_ID
                    && note.address() == test_keys::ADDRESS_0.deref().clone()
            })
            .cloned()
            .context("base genesis note must be recoverable for fee funding")?;
        let fee_position = client
            .position(fee_note.commit())
            .context("base genesis note position must be known")?;
        let fee_spend = ShieldedInputPlan::new(&mut OsRng, fee_note.clone(), fee_position);
        let fee_change = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: fee_note.amount(),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_0.deref().clone(),
        );
        let fee_funding_transfer = shieldd_sdk_mock_client::TransferIntent {
            spends: vec![fee_spend],
            outputs: vec![fee_change],
            value_blinding: Fr::from(2u64),
        };
        let send_amount = Amount::from(100u64);
        let output = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: send_amount,
                asset_id: regulated_asset_id,
            },
            test_keys::ADDRESS_1.deref().clone(),
        );
        let change = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: note.amount() - send_amount,
                asset_id: regulated_asset_id,
            },
            test_keys::ADDRESS_0.deref().clone(),
        );
        let intent = shieldd_sdk_mock_client::TransactionIntent {
            actions: vec![shieldd_sdk_mock_client::TransferIntent {
                spends: vec![spend],
                outputs: vec![output, change],
                value_blinding: Fr::from(1u64),
            }
            .into()],
            memo: Some(MemoPlan::new(
                &mut OsRng,
                MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
            )),
            fee_funding: Some(fee_funding_transfer),
            transaction_parameters: TransactionParameters {
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                ..Default::default()
            },
            nullifier_window: Some(test_nullifier_window()),
        };
        let plan = client
            .complete_intent(intent, storage.latest_snapshot())
            .await?;
        let tx_bytes = client.witness_auth_build(&plan).await?.encode_to_vec();

        let cache = StatelessCache::new();
        let mut mempool_app = App::new(storage.latest_snapshot());
        mempool_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        mempool_app
            .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
            .await?;

        test_node.block().execute().await?;
        let mut recheck_app = App::new(storage.latest_snapshot());
        recheck_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        recheck_app
            .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
            .await
            .context("regulated transfer must remain valid during next-block mempool recheck")?;

        let proposal = request::PrepareProposal {
            txs: vec![tx_bytes.into()],
            max_tx_bytes: 1024 * 1024,
            local_last_commit: None,
            misbehavior: Vec::new(),
            height: block::Height::from(4u32),
            time: Time::unix_epoch(),
            next_validators_hash: Hash::None,
            proposer_address: account::Id::new([0u8; 20]),
        };
        let prepared = test_node.prepare_proposal(proposal).await?;
        assert_eq!(
            prepared.txs.len(),
            2,
            "proposal must include the regulated transfer and aggregate bundle"
        );
        let verdict = test_node
            .process_proposal(request::ProcessProposal {
                txs: prepared.txs.clone(),
                proposed_last_commit: None,
                misbehavior: Vec::new(),
                hash: Hash::None,
                height: block::Height::from(4u32),
                time: Time::unix_epoch(),
                next_validators_hash: Hash::None,
                proposer_address: account::Id::new([0u8; 20]),
            })
            .await?;
        assert!(matches!(verdict, response::ProcessProposal::Accept));
        test_node
            .block()
            .with_data(
                prepared
                    .txs
                    .into_iter()
                    .map(|bytes| bytes.to_vec())
                    .collect(),
            )
            .execute()
            .await?;
        client.sync_to_latest(storage.latest_snapshot()).await?;
        assert!(
            client.spent_note(&spent_commitment),
            "committed regulated transfer nullifier must be visible in the compact block"
        );

        Ok(())
    }

    async fn candidate_envelope_from_fixture_txs(
        storage: &TempStorage,
        txs: &[Vec<u8>],
    ) -> Result<CandidateEnvelope> {
        let decoded = txs
            .iter()
            .enumerate()
            .map(|(index, tx_bytes)| {
                Transaction::decode(tx_bytes.as_slice())
                    .map(Arc::new)
                    .with_context(|| format!("decoding fixture tx ordinal {index}"))
            })
            .collect::<Result<Vec<_>>>()?;
        let verified_artifacts = App::build_tx_artifacts_for_stage("app_test", &decoded).await?;
        let artifacts = verified_artifacts
            .iter()
            .map(|artifact| artifact.extracted())
            .collect::<Vec<_>>();
        let segment_tx_counts = vec![decoded.len()];
        let (bundle, _segment_tx_counts) =
            App::build_exact_segmented_aggregate_bundle_for_artifacts_public(
                &artifacts,
                &segment_tx_counts,
            )
            .await?;
        let sidecar =
            ProposalArtifactSidecar::build(&artifacts, decoded.len(), segment_tx_counts.clone())?;
        let bundle_tx =
            App::build_aggregate_bundle_tx_for_snapshot_public(storage.latest_snapshot(), bundle)
                .await?;
        let tx_hashes = txs
            .iter()
            .map(|tx_bytes| sha2::Sha256::digest(tx_bytes).into())
            .collect::<Vec<[u8; 32]>>();

        Ok(CandidateEnvelope {
            txs: txs.to_vec(),
            tx_hashes: tx_hashes.clone(),
            aggregate_bundle_tx_bytes: Some(bundle_tx.encode_to_vec()),
            sidecar: sidecar.to_record(),
            segment_tx_counts,
            block_tx_count: txs.len(),
            total_payload_bytes: txs.iter().map(Vec::len).sum(),
            candidate_digest: candidate_digest_from_hashes(&tx_hashes),
            source_builder_label: "app_test".to_string(),
        })
    }

    async fn aggregate_fixture(
        tx_count: usize,
    ) -> Result<(
        TempStorage,
        Vec<Arc<TxArtifact>>,
        AggregateBundle,
        Transaction,
    )> {
        let (storage, _node, txs) = setup_test_txs(tx_count).await?;
        let decoded = txs
            .iter()
            .map(|tx_bytes| Transaction::decode(tx_bytes.as_slice()).map(Arc::new))
            .collect::<Result<Vec<_>, _>>()?;
        let verified_artifacts = App::build_tx_artifacts_for_stage("app_test", &decoded).await?;
        let artifacts = verified_artifacts
            .iter()
            .map(|artifact| artifact.extracted())
            .collect::<Vec<_>>();
        let segment_tx_counts = vec![decoded.len()];
        let (bundle, _) = App::build_exact_segmented_aggregate_bundle_for_artifacts_public(
            &artifacts,
            &segment_tx_counts,
        )
        .await?;
        let bundle_tx = App::build_aggregate_bundle_tx_for_snapshot_public(
            storage.latest_snapshot(),
            bundle.clone(),
        )
        .await?;

        Ok((storage, artifacts, bundle, bundle_tx))
    }

    fn aggregate_verify_test_item(family_id: ProofFamilyId, value: u64) -> BatchItem {
        let arity = super::proof_verification_key_for_family(family_id)
            .vk
            .gamma_abc_g1
            .len()
            - 1;
        BatchItem {
            proof: ark_groth16::Proof {
                a: Default::default(),
                b: Default::default(),
                c: Default::default(),
            },
            public_inputs: vec![Fq::from(value); arity],
        }
    }

    fn aggregate_verify_test_artifact(entries: Vec<(ProofFamilyId, BatchItem)>) -> Arc<TxArtifact> {
        let bundle = AggregateBundle {
            version: AGGREGATE_PROTOCOL_VERSION,
            srs_id: DEFAULT_DEV_SRS_ID.to_vec(),
            families: Vec::new(),
        };
        let total_proof_count = entries.len();
        let mut proof_items = BTreeMap::new();
        for (family_id, item) in entries {
            proof_items
                .entry(family_id)
                .or_insert_with(Vec::new)
                .push(item);
        }
        Arc::new(TxArtifact {
            tx: Arc::new(aggregate_bundle_shape_test_tx(bundle, 5)),
            proof_items,
            spend_nullifiers: Vec::new(),
            anchor_pairs: Vec::new(),
            total_proof_count,
            historical_validation: None,
        })
    }

    #[test]
    fn aggregate_expected_segments_preserve_segment_and_family_order() {
        let transfer = ProofFamilyId::Transfer;
        let note_reshape =
            ProofFamilyId::NoteReshape(shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne);
        let artifacts = vec![
            aggregate_verify_test_artifact(vec![
                (note_reshape, aggregate_verify_test_item(note_reshape, 11)),
                (transfer, aggregate_verify_test_item(transfer, 12)),
            ]),
            aggregate_verify_test_artifact(vec![
                (transfer, aggregate_verify_test_item(transfer, 21)),
                (note_reshape, aggregate_verify_test_item(note_reshape, 22)),
            ]),
        ];

        let segments = App::expected_aggregate_verify_segments(
            &artifacts,
            &[
                shieldd_sdk_proof_aggregation::AppVerifySegmentRange {
                    segment_index: 0,
                    start: 0,
                    end: 1,
                },
                shieldd_sdk_proof_aggregation::AppVerifySegmentRange {
                    segment_index: 1,
                    start: 1,
                    end: 2,
                },
            ],
        );
        let ids = segments
            .iter()
            .enumerate()
            .map(|(order_index, segment)| super::AggregateVerifyCallId {
                order_index,
                segment_index: segment.segment_index,
                family_index: segment.family_index,
                family_id: segment.family_id,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                super::AggregateVerifyCallId {
                    order_index: 0,
                    segment_index: 0,
                    family_index: 0,
                    family_id: transfer,
                },
                super::AggregateVerifyCallId {
                    order_index: 1,
                    segment_index: 0,
                    family_index: 1,
                    family_id: note_reshape,
                },
                super::AggregateVerifyCallId {
                    order_index: 2,
                    segment_index: 1,
                    family_index: 0,
                    family_id: transfer,
                },
                super::AggregateVerifyCallId {
                    order_index: 3,
                    segment_index: 1,
                    family_index: 1,
                    family_id: note_reshape,
                },
            ]
        );
        assert_eq!(segments[0].items[0].public_inputs[0], Fq::from(12u64));
        assert_eq!(segments[1].items[0].public_inputs[0], Fq::from(11u64));
        assert_eq!(segments[2].items[0].public_inputs[0], Fq::from(21u64));
        assert_eq!(segments[3].items[0].public_inputs[0], Fq::from(22u64));
    }

    #[test]
    fn aggregate_verify_planner_preserves_segment_order_and_checks_counts() -> Result<()> {
        let family_id = ProofFamilyId::Transfer;
        let expected_segments = vec![
            super::AggregateExpectedVerifySegment {
                segment_index: 0,
                family_index: 0,
                family_id,
                items: vec![aggregate_verify_test_item(family_id, 1)],
                debug_rows: Vec::new(),
            },
            super::AggregateExpectedVerifySegment {
                segment_index: 1,
                family_index: 0,
                family_id,
                items: vec![aggregate_verify_test_item(family_id, 2)],
                debug_rows: Vec::new(),
            },
        ];
        let bundle = AggregateBundle {
            version: AGGREGATE_PROTOCOL_VERSION,
            srs_id: DEFAULT_DEV_SRS_ID.to_vec(),
            families: vec![
                FamilyAggregate {
                    family_id,
                    real_count: 1,
                    padded_count: 1,
                    aggregate_proof: vec![1],
                },
                FamilyAggregate {
                    family_id,
                    real_count: 1,
                    padded_count: 1,
                    aggregate_proof: vec![2],
                },
            ],
        };

        let plan = App::plan_aggregate_bundle_verification(
            &bundle,
            expected_segments.clone(),
            shieldd_sdk_proof_aggregation::DevSrs::default(),
        )?;
        assert_eq!(
            plan.calls.iter().map(|call| call.id).collect::<Vec<_>>(),
            vec![
                super::AggregateVerifyCallId {
                    order_index: 0,
                    segment_index: 0,
                    family_index: 0,
                    family_id,
                },
                super::AggregateVerifyCallId {
                    order_index: 1,
                    segment_index: 1,
                    family_index: 0,
                    family_id,
                },
            ]
        );
        assert_eq!(plan.calls[0].padded_public_inputs[0][0], Fq::from(1u64));
        assert_eq!(plan.calls[1].padded_public_inputs[0][0], Fq::from(2u64));
        assert_eq!(
            plan.calls[0].shipping_call,
            shieldd_sdk_proof_aggregation::AppVerifyShippingCall {
                id: AppVerifyCallId {
                    order_index: 0,
                    segment_index: 0,
                    family_index: 0,
                    family: app_verify_family_code(family_id),
                },
                bundle_family: app_verify_family_code(family_id),
                expected_real_count: 1,
                bundle_real_count: 1,
                expected_padded_count: 1,
                bundle_padded_count: 1
            }
        );

        let mut missing_family = bundle.clone();
        missing_family.families.pop();
        let family_count_error = App::plan_aggregate_bundle_verification(
            &missing_family,
            expected_segments.clone(),
            shieldd_sdk_proof_aggregation::DevSrs::default(),
        )
        .err()
        .expect("missing family must reject");
        assert!(family_count_error
            .to_string()
            .contains("aggregate bundle family count mismatch"));

        let mut wrong_order = bundle.clone();
        wrong_order.families[0].family_id =
            ProofFamilyId::NoteReshape(shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne);
        let order_error = App::plan_aggregate_bundle_verification(
            &wrong_order,
            expected_segments.clone(),
            shieldd_sdk_proof_aggregation::DevSrs::default(),
        )
        .err()
        .expect("wrong family order must reject");
        assert!(order_error
            .to_string()
            .contains("aggregate family ordering mismatch"));

        let mut wrong_real_count = bundle.clone();
        wrong_real_count.families[0].real_count = 2;
        let real_count_error = App::plan_aggregate_bundle_verification(
            &wrong_real_count,
            expected_segments.clone(),
            shieldd_sdk_proof_aggregation::DevSrs::default(),
        )
        .err()
        .expect("wrong real count must reject");
        assert!(real_count_error
            .to_string()
            .contains("aggregate real_count mismatch"));

        let mut wrong_padded_count = bundle;
        wrong_padded_count.families[1].padded_count = 2;
        let padded_count_error = App::plan_aggregate_bundle_verification(
            &wrong_padded_count,
            expected_segments,
            shieldd_sdk_proof_aggregation::DevSrs::default(),
        )
        .err()
        .expect("wrong padded count must reject");
        assert!(padded_count_error
            .to_string()
            .contains("aggregate padded_count mismatch"));

        Ok(())
    }

    #[test]
    fn aggregate_verify_plan_header_rejects_incomplete_segment_coverage() {
        let bundle = AggregateBundle {
            version: AGGREGATE_PROTOCOL_VERSION,
            srs_id: DEFAULT_DEV_SRS_ID.to_vec(),
            families: Vec::new(),
        };
        let tx = aggregate_bundle_shape_test_tx(bundle.clone(), 5);
        let artifact = Arc::new(TxArtifact {
            tx: Arc::new(tx),
            proof_items: BTreeMap::new(),
            spend_nullifiers: Vec::new(),
            anchor_pairs: Vec::new(),
            total_proof_count: 1,
            historical_validation: None,
        });

        let error = App::validate_aggregate_verify_plan_inputs(
            &[artifact],
            &bundle,
            Some(&[0]),
            &DevSrs::default(),
        )
        .expect_err("incomplete segment coverage must reject");
        assert!(error
            .to_string()
            .contains("aggregate segment coverage mismatch"));
    }

    #[test]
    fn aggregate_verify_reducer_is_order_independent_and_rejects_exact_calls() -> Result<()> {
        let family_id = ProofFamilyId::Transfer;
        let expected = vec![
            super::AggregateVerifyCallId {
                order_index: 0,
                segment_index: 0,
                family_index: 0,
                family_id,
            },
            super::AggregateVerifyCallId {
                order_index: 1,
                segment_index: 1,
                family_index: 0,
                family_id,
            },
        ];

        let reduction = App::reduce_aggregate_verify_outcomes(
            &expected,
            vec![
                super::AggregateVerifyCallResult {
                    id: expected[1],
                    accepted: true,
                },
                super::AggregateVerifyCallResult {
                    id: expected[0],
                    accepted: true,
                },
            ],
        )?;
        reduction.acceptance_result()?;

        let rejected = App::reduce_aggregate_verify_outcomes(
            &expected,
            vec![
                super::AggregateVerifyCallResult {
                    id: expected[0],
                    accepted: true,
                },
                super::AggregateVerifyCallResult {
                    id: expected[1],
                    accepted: false,
                },
            ],
        )?;
        let rejection = rejected
            .acceptance_result()
            .expect_err("one rejected call must reject the bundle");
        assert!(rejection
            .to_string()
            .contains("segment=1 family_index=0 family=Transfer"));

        let duplicate_error = App::reduce_aggregate_verify_outcomes(
            &expected,
            vec![
                super::AggregateVerifyCallResult {
                    id: expected[0],
                    accepted: true,
                },
                super::AggregateVerifyCallResult {
                    id: expected[0],
                    accepted: true,
                },
            ],
        )
        .expect_err("duplicate outcomes must reject");
        assert!(duplicate_error
            .to_string()
            .contains("aggregate verification outcome identity mismatch"));

        let missing_error = App::reduce_aggregate_verify_outcomes(
            &expected,
            vec![super::AggregateVerifyCallResult {
                id: expected[0],
                accepted: true,
            }],
        )
        .expect_err("missing outcomes must reject");
        assert!(missing_error
            .to_string()
            .contains("aggregate verification outcome count mismatch"));

        Ok(())
    }

    #[test]
    fn aggregate_verify_join_rejection_guard_is_fail_closed() -> Result<()> {
        super::require_no_rejected_joined_calls(Vec::new())?;

        let rejected = AppVerifyCallId {
            order_index: 3,
            segment_index: 5,
            family_index: 7,
            family: app_verify_family_code(ProofFamilyId::Transfer),
        };
        let error = super::require_no_rejected_joined_calls(vec![rejected])
            .expect_err("a retained rejected join must fail after reducer acceptance");
        assert!(error
            .to_string()
            .contains("join retained 1 rejected call(s)"));
        Ok(())
    }

    #[tokio::test]
    async fn aggregate_bundle_rejects_wrong_real_count() -> Result<()> {
        let (_storage, artifacts, mut bundle, _bundle_tx) = aggregate_fixture(1).await?;
        App::verify_aggregate_bundle_for_artifacts_raw(&artifacts, &bundle, Some(&[1])).await?;
        bundle.families[0].real_count += 1;
        assert!(
            App::verify_aggregate_bundle_for_artifacts_raw(&artifacts, &bundle, Some(&[1]))
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn async_verifier_outcome_retains_its_exact_shipping_input() -> Result<()> {
        let (_storage, artifacts, bundle, _bundle_tx) = aggregate_fixture(1).await?;
        let ranges = App::validate_aggregate_verify_plan_inputs(
            &artifacts,
            &bundle,
            Some(&[1]),
            &DevSrs::default(),
        )?;
        let expected_segments = App::expected_aggregate_verify_segments(&artifacts, &ranges);
        let mut plan = App::plan_aggregate_bundle_verification(
            &bundle,
            expected_segments,
            shieldd_sdk_proof_aggregation::DevSrs::default(),
        )?;
        let call = plan.calls.remove(0);
        let expected_id = call.id;
        let expected_shipping_call = call.shipping_call;
        let expected_statement = call.statement.clone();
        let expected_wrapped_proof = call.aggregate.aggregate_proof.clone();
        let mut expected_padded_public_inputs =
            Vec::with_capacity(expected_statement.padded_public_inputs().len());
        for row in expected_statement.padded_public_inputs() {
            let mut serialized_row = Vec::with_capacity(row.len());
            for field in row {
                let mut bytes = Vec::new();
                field.serialize_compressed(&mut bytes)?;
                serialized_row.push(bytes);
            }
            expected_padded_public_inputs.push(serialized_row);
        }
        let expected_public_input_arity = u32::try_from(
            expected_padded_public_inputs
                .first()
                .context("aggregate statement must retain one padded public-input row")?
                .len(),
        )?;

        let outcome = App::execute_aggregate_verify_call(call)?;
        let shipping_result = outcome.shipping_verification.shipping_result();

        assert_eq!(outcome.id, expected_id);
        assert_eq!(shipping_result.input.call, expected_shipping_call);
        assert_eq!(
            shipping_result.input.protocol_version,
            AGGREGATE_PROTOCOL_VERSION
        );
        assert_eq!(
            shipping_result.input.family,
            app_verify_family_code(expected_id.family_id)
        );
        assert_eq!(shipping_result.input.srs_id, expected_statement.srs_id());
        assert_eq!(
            shipping_result.input.vk_digest,
            expected_statement.vk_digest()
        );
        assert_eq!(
            shipping_result.input.real_count,
            expected_statement.real_count()
        );
        assert_eq!(
            shipping_result.input.padded_count,
            expected_statement.padded_count()
        );
        assert_eq!(
            shipping_result.input.public_input_arity,
            expected_public_input_arity
        );
        assert_eq!(
            shipping_result.input.padded_public_inputs,
            expected_padded_public_inputs
        );
        assert_eq!(
            shipping_result.input.canonical_statement_bytes,
            expected_statement.canonical_bytes()
        );
        assert_eq!(
            shipping_result.input.statement_digest,
            expected_statement.statement_digest()
        );
        assert_eq!(
            shipping_result.input.wrapped_proof_bytes,
            expected_wrapped_proof
        );
        assert_eq!(
            shipping_result.input.challenge_context,
            expected_statement.challenge_context().as_bytes()
        );
        assert_eq!(
            shipping_result.result.accepted,
            outcome
                .shipping_verification
                .shipping_result()
                .result
                .accepted
        );
        assert_eq!(
            outcome.result()?.accepted,
            outcome
                .shipping_verification
                .shipping_result()
                .result
                .accepted
        );
        Ok(())
    }

    #[tokio::test]
    async fn latest_snapshot_supports_parallel_reads() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let snapshot = storage.latest_snapshot();
        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..4 {
            let snapshot = snapshot.clone();
            tasks.spawn(async move {
                let _ = snapshot.get_raw("parallel.snapshot.read").await?;
                Ok::<(), anyhow::Error>(())
            });
        }

        while let Some(result) = tasks.join_next().await {
            result??;
        }

        Ok(())
    }

    #[tokio::test]
    async fn prepare_candidate_read_supports_unregulated_fixture_txs() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(2).await?;
        let snapshot = Arc::new(storage.latest_snapshot());
        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

        for tx_bytes in txs {
            let tx = Arc::new(Transaction::decode(tx_bytes.as_slice())?);
            assert!(
                supports_parallel_prepare(Arc::as_ref(&tx)),
                "fixture tx should stay on the supported transfer fast path"
            );

            let prepared = prepare_candidate_read(
                tx.clone(),
                snapshot.clone(),
                historical_context.clone(),
                false,
            )
            .await?;

            assert_eq!(prepared.spend_nullifiers.len(), 2);
            assert_eq!(
                prepared.sct_payloads.len(),
                3,
                "fixture transfer should create receiver, change, and accumulator payloads",
            );
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prepare_candidate_read_blocking_matches_async_fast_path() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(2).await?;
        let snapshot = Arc::new(storage.latest_snapshot());
        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

        for tx_bytes in txs {
            let tx = Arc::new(Transaction::decode(tx_bytes.as_slice())?);
            assert!(supports_parallel_prepare(Arc::as_ref(&tx)));

            let prepared_async = prepare_candidate_read(
                tx.clone(),
                snapshot.clone(),
                historical_context.clone(),
                false,
            )
            .await?;
            let tx_for_blocking = tx;
            let snapshot_for_blocking = Arc::as_ref(&snapshot).clone();
            let context_for_blocking = historical_context.clone();
            let handle = tokio::runtime::Handle::current();
            let prepared_blocking = tokio::task::spawn_blocking(move || {
                prepare_candidate_read_blocking(
                    tx_for_blocking,
                    snapshot_for_blocking,
                    context_for_blocking,
                    false,
                    handle,
                )
            })
            .await??;

            assert_eq!(
                prepared_async.spend_nullifiers,
                prepared_blocking.spend_nullifiers
            );
            assert_eq!(
                prepared_async.sct_payloads.len(),
                prepared_blocking.sct_payloads.len()
            );
            assert_eq!(
                prepared_async
                    .sct_payloads
                    .iter()
                    .map(|payload| *payload.commitment())
                    .collect::<Vec<_>>(),
                prepared_blocking
                    .sct_payloads
                    .iter()
                    .map(|payload| *payload.commitment())
                    .collect::<Vec<_>>()
            );
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn checktx_fast_path_matches_standard_path() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx = Arc::new(Transaction::decode(
            txs.first().expect("fixture transaction").as_slice(),
        )?);
        let artifact = App::build_tx_artifact_for_stage("app_test", tx.clone()).await?;
        assert!(supports_parallel_prepare(Arc::as_ref(&tx)));
        let shared_context =
            Arc::new(CheckTxSharedContext::load(&storage.latest_snapshot()).await?);

        let mut standard_app = App::new(storage.latest_snapshot());
        tx.check_historical(standard_app.state.clone()).await?;
        let standard_events = standard_app
            .execute_tx_checked_historical(artifact.clone())
            .await?;

        let mut fast_app = App::new(storage.latest_snapshot());
        fast_app.set_checktx_shared_context(shared_context);
        let fast_events = fast_app.execute_checktx_fast(artifact, false).await?;

        let mut standard_rendered = standard_events
            .iter()
            .map(|event| format!("{event:?}"))
            .collect::<Vec<_>>();
        standard_rendered.sort();
        let mut fast_rendered = fast_events
            .iter()
            .map(|event| format!("{event:?}"))
            .collect::<Vec<_>>();
        fast_rendered.sort();
        assert_eq!(standard_rendered, fast_rendered);

        Ok(())
    }

    #[tokio::test]
    async fn process_candidate_envelope_accepts_valid_fixture() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(2).await?;
        let envelope = candidate_envelope_from_fixture_txs(&storage, &txs).await?;
        let mut app = App::new(storage.latest_snapshot());
        let starting_generation =
            shieldd_sdk_sct::nullifier_tree::generation_state(Arc::as_ref(&app.state)).await?;

        let verdict = app.process_candidate_envelope(&envelope, None).await?;
        assert!(matches!(verdict, response::ProcessProposal::Accept));
        assert_eq!(app.state.pending_nullifiers().len(), 4);
        assert!(!app.state.nullifier_block_is_materialized());
        assert_eq!(
            shieldd_sdk_sct::nullifier_tree::generation_state(Arc::as_ref(&app.state))
                .await?
                .current_root,
            starting_generation.current_root,
            "ProcessProposal should reuse delivery semantics without building a disposable tree",
        );

        Ok(())
    }

    #[tokio::test]
    async fn ensure_aggregate_bundle_tx_shape_rejects_memo_fee_and_extra_action() -> Result<()> {
        let (_storage, _artifacts, bundle, bundle_tx) = aggregate_fixture(1).await?;

        let mut with_memo = bundle_tx.clone();
        with_memo.transaction_body.memo = Some(MemoCiphertext([0; MEMO_CIPHERTEXT_LEN_BYTES]));
        let memo_error =
            App::ensure_aggregate_bundle_tx_shape(&with_memo).expect_err("memo must be rejected");
        assert!(memo_error
            .to_string()
            .contains("aggregate bundle tx must not contain a memo"));

        let mut with_fee = bundle_tx.clone();
        with_fee.transaction_body.transaction_parameters.fee =
            Fee::from_staking_token_amount(1u64.into());
        let fee_error =
            App::ensure_aggregate_bundle_tx_shape(&with_fee).expect_err("nonzero fee must fail");
        assert!(fee_error
            .to_string()
            .contains("aggregate bundle tx must have zero fee"));

        let mut with_extra_action = bundle_tx.clone();
        with_extra_action
            .transaction_body
            .actions
            .push(Action::AggregateBundle(bundle));
        let shape_error = App::ensure_aggregate_bundle_tx_shape(&with_extra_action)
            .expect_err("multiple actions must fail aggregate bundle shape validation");
        assert!(shape_error
            .to_string()
            .contains("aggregate bundle tx must contain exactly one aggregate bundle action"));

        Ok(())
    }

    fn aggregate_bundle_shape_test_tx(bundle: AggregateBundle, mode: u8) -> Transaction {
        let mut tx = Transaction {
            transaction_body: shieldd_sdk_transaction::TransactionBody {
                actions: vec![Action::AggregateBundle(bundle.clone())],
                transaction_parameters: TransactionParameters {
                    expiry_height: 0,
                    chain_id: "shieldd-test-chain".to_owned(),
                    fee: Fee::default(),
                },
                fee_funding: None,
                memo: None,
                nullifier_window: None,
                historical_nullifier_proofs: Vec::new(),
            },
            binding_sig: [0; 64].into(),
            anchor: shieldd_sdk_tct::Root(shieldd_sdk_tct::structure::Hash::zero()),
        };

        match mode % 5 {
            0 => tx.transaction_body.actions.clear(),
            1 => {
                tx.transaction_body.memo = Some(MemoCiphertext([0; MEMO_CIPHERTEXT_LEN_BYTES]));
            }
            2 => {
                tx.transaction_body.transaction_parameters.fee =
                    Fee::from_staking_token_amount(1u64.into());
            }
            3 => tx
                .transaction_body
                .actions
                .push(Action::AggregateBundle(bundle)),
            _ => {
                let binding_signing_key = rdsa::SigningKey::from(Fr::zero());
                let auth_hash = tx.transaction_body.auth_hash();
                tx.binding_sig = binding_signing_key.sign_deterministic(auth_hash.as_bytes());
            }
        }

        tx
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn ensure_aggregate_bundle_tx_shape_do_not_panic(
            mode in 0u8..5,
            version in any::<u32>(),
            srs_id in prop::collection::vec(any::<u8>(), 0usize..=64),
            aggregate_proof in prop::collection::vec(any::<u8>(), 0usize..=1024),
            real_count in any::<u32>(),
            padded_count in any::<u32>(),
        ) {
            let bundle = AggregateBundle {
                version,
                srs_id,
                families: vec![shieldd_sdk_proof_aggregation::FamilyAggregate {
                    family_id: ProofFamilyId::Transfer,
                    real_count,
                    padded_count,
                    aggregate_proof,
                }],
            };
            let tx = aggregate_bundle_shape_test_tx(bundle, mode);
            let result = App::ensure_aggregate_bundle_tx_shape(&tx);

            match mode % 5 {
                0 | 3 => prop_assert!(
                    result
                        .expect_err("aggregate action shape mutation must reject")
                        .to_string()
                        .contains("exactly one aggregate bundle action")
                ),
                1 => prop_assert!(
                    result
                        .expect_err("memo mutation must reject")
                        .to_string()
                        .contains("must not contain a memo")
                ),
                2 => prop_assert!(
                    result
                        .expect_err("fee mutation must reject")
                        .to_string()
                        .contains("must have zero fee")
                ),
                _ => {
                    let _ = result;
                }
            }
        }
    }

    #[tokio::test]
    async fn aggregate_bundle_verification_rejects_bad_version_srs_and_family_count() -> Result<()>
    {
        let (_storage, artifacts, bundle, _bundle_tx) = aggregate_fixture(1).await?;

        let mut bad_version = bundle.clone();
        bad_version.version += 1;
        let version_error =
            App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &bad_version, None)
                .await
                .expect_err("bad version must fail verification");
        assert!(version_error
            .to_string()
            .contains("unsupported aggregate bundle version"));

        let mut bad_srs = bundle.clone();
        bad_srs.srs_id[0] ^= 0x01;
        let srs_error =
            App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &bad_srs, None)
                .await
                .expect_err("bad SRS id must fail verification");
        assert!(srs_error.to_string().contains(SRS_ID_MISMATCH));

        let mut empty_families = bundle.clone();
        empty_families.families.clear();
        let empty_error = App::verify_aggregate_bundle_for_artifacts_raw_public(
            &artifacts,
            &empty_families,
            None,
        )
        .await
        .expect_err("empty family list must fail verification");
        assert!(empty_error
            .to_string()
            .contains("aggregate bundle family count mismatch"));

        let mut extra_family = bundle.clone();
        extra_family.families.push(extra_family.families[0].clone());
        let family_count_error =
            App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &extra_family, None)
                .await
                .expect_err("extra family entries must fail verification");
        assert!(family_count_error
            .to_string()
            .contains("aggregate bundle family count mismatch"));

        Ok(())
    }

    #[tokio::test]
    async fn aggregate_bundle_verification_rejects_bad_srs_id_before_srs_setup() -> Result<()> {
        let mut wrong_full_length_srs_id = DEFAULT_DEV_SRS_ID.to_vec();
        wrong_full_length_srs_id[0] ^= 0x01;

        for (srs_id, expected_error) in [
            (vec![0; 3], SRS_ID_MISMATCH),
            (wrong_full_length_srs_id, SRS_ID_MISMATCH),
        ] {
            let bundle = AggregateBundle {
                version: AGGREGATE_PROTOCOL_VERSION,
                srs_id,
                families: vec![FamilyAggregate {
                    family_id: ProofFamilyId::Transfer,
                    real_count: 1,
                    padded_count: 1,
                    aggregate_proof: vec![0xaa, 0xbb],
                }],
            };
            let tx = aggregate_bundle_shape_test_tx(bundle.clone(), 5);
            let artifact = Arc::new(TxArtifact {
                tx: Arc::new(tx),
                proof_items: BTreeMap::new(),
                spend_nullifiers: Vec::new(),
                anchor_pairs: Vec::new(),
                total_proof_count: 1,
                historical_validation: None,
            });

            let started = std::time::Instant::now();
            let result =
                App::verify_aggregate_bundle_for_artifacts_raw(&[artifact], &bundle, None).await;
            let elapsed = started.elapsed();
            let error = result.err().expect("bad SRS id must fail before SRS setup");

            assert!(error.to_string().contains(expected_error));
            assert!(
                elapsed < std::time::Duration::from_millis(500),
                "bad SRS id rejection took {elapsed:?}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn execute_validated_candidate_envelope_profiled_skips_proposal_validation() -> Result<()>
    {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let spent_nullifiers = Transaction::decode(txs[0].as_slice())?
            .spent_nullifiers()
            .collect::<Vec<_>>();
        let envelope = candidate_envelope_from_fixture_txs(&storage, &txs).await?;

        let mut preflight_app = App::new(storage.latest_snapshot());
        let verdict = preflight_app
            .process_candidate_envelope(&envelope, None)
            .await?;
        assert!(matches!(verdict, response::ProcessProposal::Accept));

        let mut execution_only = envelope.clone();
        execution_only.tx_hashes.clear();
        execution_only.candidate_digest = [0; 32];

        let mut app = App::new(storage.latest_snapshot());
        let profile = app
            .execute_validated_candidate_envelope_profiled(
                &execution_only,
                storage.as_ref().clone(),
            )
            .await?;
        assert_eq!(profile.block_tx_count, 1);
        assert!(profile.deliver_txs_wall_ms > 0.0);
        let committed = storage.latest_snapshot();
        for nullifier in spent_nullifiers {
            assert!(shieldd_sdk_sct::nullifier_tree::is_spent(&committed, nullifier).await?);
        }
        shieldd_sdk_sct::nullifier_tree::verify_committed_roots(&committed).await?;

        Ok(())
    }

    #[tokio::test]
    async fn checktx_shared_context_caches_historical_context_for_snapshot() -> Result<()> {
        let (storage, _node, _txs) = setup_test_txs(1).await?;
        let snapshot = storage.latest_snapshot();
        let shared_context = CheckTxSharedContext::load(&snapshot).await?;
        let direct_context = HistoricalCheckContext::load(&snapshot).await?;

        assert_eq!(
            shared_context.historical_check_context.chain_id,
            direct_context.chain_id
        );
        assert_eq!(
            shared_context.historical_check_context.block_height,
            direct_context.block_height
        );
        assert_eq!(
            shared_context.historical_check_context.block_timestamp,
            direct_context.block_timestamp
        );
        assert_eq!(
            shared_context
                .historical_check_context
                .discovery_grace_period_blocks,
            direct_context.discovery_grace_period_blocks
        );
        assert_eq!(
            shared_context
                .historical_check_context
                .previous_discovery_parameters,
            direct_context.previous_discovery_parameters
        );
        assert_eq!(
            shared_context
                .historical_check_context
                .current_discovery_parameters,
            direct_context.current_discovery_parameters
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn checktx_cache_hit_and_miss_match_for_supported_tx() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx_bytes = txs.first().expect("fixture transaction").clone();
        let cache = StatelessCache::new();
        let shared_context =
            Arc::new(CheckTxSharedContext::load(&storage.latest_snapshot()).await?);

        let mut miss_app = App::new(storage.latest_snapshot());
        miss_app.set_checktx_shared_context(shared_context.clone());
        let miss_events = miss_app.deliver_tx_bytes(&tx_bytes, Some(&cache)).await?;
        let hash: [u8; 32] = sha2::Sha256::digest(&tx_bytes).into();
        assert!(matches!(
            cache.get(&hash, &tx_bytes),
            Some(CacheEntry::FullyVerified(_))
        ));

        let mut hit_app = App::new(storage.latest_snapshot());
        hit_app.set_checktx_shared_context(shared_context);
        let hit_events = hit_app.deliver_tx_bytes(&tx_bytes, Some(&cache)).await?;
        assert!(matches!(
            cache.get(&hash, &tx_bytes),
            Some(CacheEntry::FullyVerified(_))
        ));
        assert_eq!(miss_events, hit_events);

        Ok(())
    }

    #[tokio::test]
    async fn prepared_reads_are_blind_to_same_block_nullifier_conflicts() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx = Arc::new(Transaction::decode(
            txs.first().expect("fixture transaction").as_slice(),
        )?);
        let artifact = App::build_tx_artifact_for_stage("app_test", tx.clone()).await?;
        let snapshot = Arc::new(storage.latest_snapshot());
        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

        let prepared_first = prepare_candidate_read(
            tx.clone(),
            snapshot.clone(),
            historical_context.clone(),
            false,
        )
        .await?;
        let prepared_second =
            prepare_candidate_read(tx.clone(), snapshot, historical_context, false).await?;

        anyhow::ensure!(
            !prepared_first.spend_nullifiers.is_empty(),
            "fixture tx should exercise committed nullifier checks"
        );
        anyhow::ensure!(
            !prepared_second.spend_nullifiers.is_empty(),
            "fixture tx should exercise committed nullifier checks"
        );

        let mut app = App::new(storage.latest_snapshot());
        let mut block_state = PrepareBlockLocalState::default();

        let first_nullifier_count = prepared_first
            .spend_nullifiers
            .len()
            .saturating_add(prepared_first.volume_nullifiers.len());
        app.apply_prepared_prepare_candidate(artifact.clone(), prepared_first, &mut block_state)
            .await?;
        assert_eq!(
            block_state.remaining_nullifier_capacity,
            super::MAX_BLOCK_NULLIFIER_COUNT - first_nullifier_count
        );
        let err = app
            .apply_prepared_prepare_candidate(artifact, prepared_second, &mut block_state)
            .await
            .expect_err("serial apply should resolve duplicate nullifiers in the same proposal");

        assert!(
            err.to_string()
                .contains("already spent earlier in this proposal"),
            "unexpected error: {err:#}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn batched_nullify_matches_repeated_nullify_and_preserves_pending_order() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let snapshot = storage.latest_snapshot();

        let nullifiers = vec![
            Nullifier(Fq::from(11u64)),
            Nullifier(Fq::from(12u64)),
            Nullifier(Fq::from(13u64)),
        ];
        let source = CommitmentSource::Transaction {
            id: Some([7u8; 32]),
        };

        let mut repeated = StateDelta::new(snapshot.clone());
        repeated.put_block_height(42);
        shieldd_sdk_sct::nullifier_tree::initialize(&mut repeated).await?;
        for nullifier in &nullifiers {
            repeated.nullify(*nullifier, source.clone()).await?;
        }

        let mut batched = StateDelta::new(snapshot);
        batched.put_block_height(42);
        shieldd_sdk_sct::nullifier_tree::initialize(&mut batched).await?;
        batched.nullify_all(&nullifiers, source).await?;

        assert_eq!(repeated.pending_nullifiers(), batched.pending_nullifiers());

        for nullifier in &nullifiers {
            assert_eq!(
                repeated.is_nullifier_spent(*nullifier).await?,
                batched.is_nullifier_spent(*nullifier).await?,
            );
        }

        repeated.materialize_nullifier_block().await?;
        batched.materialize_nullifier_block().await?;
        assert_eq!(
            shieldd_sdk_sct::nullifier_tree::generation_state(&repeated)
                .await?
                .current_root,
            shieldd_sdk_sct::nullifier_tree::generation_state(&batched)
                .await?
                .current_root,
        );

        Ok(())
    }

    #[tokio::test]
    async fn app_readiness_accepts_empty_pregenesis_state() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        assert!(App::is_ready(storage.latest_snapshot()).await);
        Ok(())
    }

    #[tokio::test]
    async fn app_readiness_fails_on_corrupted_nullifier_tree_nv() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::insert_batch(&mut state, [Nullifier(Fq::from(91u64))])
            .await?;
        storage.commit(state).await?;
        assert!(App::is_ready(storage.latest_snapshot()).await);

        let mut corrupt = StateDelta::new(storage.latest_snapshot());
        let tree = shieldd_sdk_sct::nullifier_tree::generation_state(&corrupt)
            .await?
            .current_tree;
        let mut stream = corrupt.nonverifiable_prefix_raw(
            &shieldd_sdk_sct::state_key::nullifier_generations::tree_node_prefix(tree),
        );
        let mut keys = Vec::new();
        while let Some(item) = stream.next().await {
            let (key, _) = item?;
            keys.push(key);
        }
        drop(stream);
        for key in keys {
            corrupt.nonverifiable_delete(key);
        }
        storage.commit(corrupt).await?;

        assert!(!App::is_ready(storage.latest_snapshot()).await);

        Ok(())
    }

    #[tokio::test]
    async fn app_readiness_fails_on_corrupted_sct_nv() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
        state.put_sct_params(SctParameters {
            epoch_duration: 10,
            sct_anchor_retention_blocks: 100,
        });
        state.put_block_height(1);
        state.put_block_timestamp(1, Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?);
        state.put_epoch_by_height(
            1,
            Epoch {
                index: 0,
                start_height: 0,
            },
        );

        let mut tree = tct::Tree::new();
        tree.insert(
            tct::Witness::Forget,
            tct::StateCommitment::try_from([11u8; 32])?,
        )?;
        let block_root = tree.end_block()?;
        state.write_sct(1, tree, block_root, None).await;
        storage.commit(state).await?;
        assert!(App::is_ready(storage.latest_snapshot()).await);

        let mut corrupt = StateDelta::new(storage.latest_snapshot());
        delete_nv_prefix(
            &mut corrupt,
            shieldd_sdk_sct::state_key::tree::incremental_prefix().as_bytes(),
        )
        .await?;
        storage.commit(corrupt).await?;

        assert!(!App::is_ready(storage.latest_snapshot()).await);

        Ok(())
    }

    #[tokio::test]
    async fn app_readiness_fails_on_corrupted_compliance_nv() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
        state
            .test_only_add_compliance_leaf(ComplianceLeaf::registered_for_test(
                Address::dummy(&mut rand::thread_rng()),
                asset::Id(Fq::from(123u64)),
            ))
            .await?;
        state
            .test_only_register_asset(
                asset::Id(Fq::from(456u64)),
                AssetPolicy::for_test(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
                true,
            )
            .await?;
        storage.commit(state).await?;
        assert!(App::is_ready(storage.latest_snapshot()).await);

        let mut corrupt = StateDelta::new(storage.latest_snapshot());
        delete_nv_prefix(
            &mut corrupt,
            shieldd_sdk_compliance::state_key::tree_storage::user_node_prefix().as_bytes(),
        )
        .await?;
        storage.commit(corrupt).await?;

        assert!(!App::is_ready(storage.latest_snapshot()).await);

        Ok(())
    }

    #[tokio::test]
    async fn deferred_sct_log_reserves_contiguous_positions() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let snapshot = storage.latest_snapshot();
        let mut log = BlockSctAppendLog::default();

        let first = log
            .reserve_positions(&snapshot, vec![rolled_up_payload(1), rolled_up_payload(2)])
            .await?;
        let second = log
            .reserve_positions(&snapshot, vec![rolled_up_payload(3)])
            .await?;

        assert_eq!(first[0].0, tct::Position::from(0u64));
        assert_eq!(first[1].0, tct::Position::from(1u64));
        assert_eq!(second[0].0, tct::Position::from(2u64));

        Ok(())
    }

    #[tokio::test]
    async fn deferred_sct_log_materializes_into_tree_and_pending_payloads() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut app = App::new(storage.latest_snapshot());
        let mut state_tx = StateDelta::new(app.state.clone());

        app.pending_sct_append_log.append_positioned(vec![
            (tct::Position::from(0u64), rolled_up_payload(10)),
            (tct::Position::from(1u64), rolled_up_payload(11)),
        ]);

        app.materialize_pending_sct_append_log(&mut state_tx)
            .await?;

        let pending = state_tx.pending_rolled_up_payloads();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].0, tct::Position::from(0u64));
        assert_eq!(pending[1].0, tct::Position::from(1u64));
        assert_eq!(
            state_tx.get_sct().await.position(),
            Some(tct::Position::from(2u64))
        );

        Ok(())
    }

    #[tokio::test]
    async fn deferred_sct_log_returns_error_on_position_drift() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut app = App::new(storage.latest_snapshot());
        let mut state_tx = StateDelta::new(app.state.clone());

        state_tx
            .add_sct_commitment(
                tct::StateCommitment(Fq::from(99u64)),
                CommitmentSource::transaction(),
            )
            .await?;
        app.pending_sct_append_log
            .append_positioned(vec![(tct::Position::from(0u64), rolled_up_payload(100))]);

        let err = app
            .materialize_pending_sct_append_log(&mut state_tx)
            .await
            .expect_err("position drift should return an explicit error");
        assert!(err.to_string().contains("position drifted"));

        Ok(())
    }

    #[tokio::test]
    async fn checktx_no_index_does_not_record_tx_log_entries_on_app_fork() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx_bytes = txs
            .into_iter()
            .next()
            .expect("fixture should return one tx");

        let mut app = App::new(storage.latest_snapshot());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        let cache = StatelessCache::new();
        app.deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
            .await?;

        let height = app.state.get_block_height().await?;
        let tx_log = app.state.transactions_by_height(height).await?;
        assert!(
            tx_log.transactions.is_empty(),
            "checktx app fork should not stage tx-log entries in NoIndex mode"
        );
        assert!(
            app.deferred_block_transactions.is_empty(),
            "NoIndex mode should not accumulate deferred tx-log entries"
        );

        Ok(())
    }

    #[tokio::test]
    async fn deferred_batch_persists_full_tx_log_by_block_end() -> Result<()> {
        let (storage, mut node, txs) = setup_test_txs(2).await?;
        let expected_hashes = txs
            .iter()
            .map(|tx| hex::encode(sha2::Sha256::digest(tx.as_slice())))
            .collect::<Vec<_>>();

        node.block().with_data(txs).execute().await?;

        let snapshot = storage.latest_snapshot();
        let height = snapshot.get_block_height().await?;
        let tx_log = snapshot.transactions_by_height(height).await?;
        assert_eq!(tx_log.transactions.len(), 2);

        let actual_hashes = tx_log
            .transactions
            .into_iter()
            .map(Transaction::try_from)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|tx| hex::encode(sha2::Sha256::digest(tx.encode_to_vec().as_slice())))
            .collect::<Vec<_>>();

        assert_eq!(actual_hashes, expected_hashes);

        Ok(())
    }

    #[tokio::test]
    async fn prepare_proposal_reuses_fully_verified_checktx_cache_entries() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx_bytes = txs
            .into_iter()
            .next()
            .expect("fixture should return one tx");
        let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
        let cache = StatelessCache::new();

        let mut mempool_app = App::new(storage.latest_snapshot());
        mempool_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        mempool_app
            .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
            .await?;

        let extracted = match cache.get(&tx_hash, &tx_bytes) {
            Some(CacheEntry::FullyVerified(artifact)) => artifact.extracted(),
            _ => anyhow::bail!("expected fully verified cache entry after CheckTx"),
        };
        assert!(!extracted.proof_items.is_empty());
        assert!(extracted.has_matching_historical_validation(storage.latest_snapshot().version()));
        assert_eq!(
            extracted
                .historical_validation
                .map(|stamp| stamp.snapshot_version),
            Some(storage.latest_snapshot().version()),
            "CheckTx should stamp the cache entry with the validated snapshot version"
        );

        let mut proposer = App::new(storage.latest_snapshot());
        proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        let proposal = request::PrepareProposal {
            txs: vec![tx_bytes.clone().into()],
            max_tx_bytes: 1024 * 1024,
            local_last_commit: None,
            misbehavior: Vec::new(),
            height: block::Height::from(1u32),
            time: Time::unix_epoch(),
            next_validators_hash: Hash::None,
            proposer_address: account::Id::new([0u8; 20]),
        };

        let (prepared, _) = proposer
            .prepare_proposal(proposal, Some(&cache), false)
            .await;
        assert_eq!(
            prepared.txs.len(),
            2,
            "proposal should include user tx plus aggregate bundle"
        );

        match cache.get(&tx_hash, &tx_bytes) {
            Some(CacheEntry::FullyVerified(_)) => {}
            _ => anyhow::bail!("expected fully verified cache entry after PrepareProposal"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn prepare_proposal_verifies_and_upgrades_extracted_cache_entry() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx_bytes = txs
            .into_iter()
            .next()
            .expect("fixture should return one tx");
        let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
        let cache = StatelessCache::new();

        let tx = Arc::new(Transaction::decode_canonical(tx_bytes.as_slice())?);
        let mut extracted = App::build_tx_artifacts_extracted_for_stage_public(
            "test_extracted_seed",
            std::slice::from_ref(&tx),
        )
        .await?;
        let extracted = extracted
            .pop()
            .context("single extracted transaction artifact missing")?;
        cache.insert_extracted(tx_bytes.as_slice(), extracted.clone())?;
        assert!(!extracted.proof_items.is_empty());

        let mut proposer = App::new(storage.latest_snapshot());
        proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        let proposal = request::PrepareProposal {
            txs: vec![tx_bytes.clone().into()],
            max_tx_bytes: 1024 * 1024,
            local_last_commit: None,
            misbehavior: Vec::new(),
            height: block::Height::from(1u32),
            time: Time::unix_epoch(),
            next_validators_hash: Hash::None,
            proposer_address: account::Id::new([0u8; 20]),
        };

        let (prepared, _) = proposer
            .prepare_proposal(proposal, Some(&cache), false)
            .await;
        assert_eq!(
            prepared.txs.len(),
            2,
            "proposal should include the user transaction and aggregate bundle"
        );

        match cache.get(&tx_hash, tx_bytes.as_slice()) {
            Some(CacheEntry::FullyVerified(_)) => {}
            _ => anyhow::bail!("expected fully verified cache entry after PrepareProposal"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn prepare_proposal_does_not_reuse_stale_historical_validation_stamp() -> Result<()> {
        let (storage, mut node, txs) = setup_test_txs(1).await?;
        let tx_bytes = txs
            .into_iter()
            .next()
            .expect("fixture should return one tx");
        let cache = StatelessCache::new();

        let mut mempool_app = App::new(storage.latest_snapshot());
        mempool_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        mempool_app
            .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
            .await?;

        let hash: [u8; 32] = sha2::Sha256::digest(&tx_bytes).into();
        let cached = match cache.get(&hash, &tx_bytes) {
            Some(CacheEntry::FullyVerified(artifact)) => artifact.extracted(),
            _ => anyhow::bail!("CheckTx must cache the verified transaction"),
        };
        assert!(cached.has_matching_historical_validation(storage.latest_snapshot().version()));
        node.block().execute().await?;
        assert!(!cached.has_matching_historical_validation(storage.latest_snapshot().version()));

        let mut proposer = App::new(storage.latest_snapshot());
        proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        let proposal = request::PrepareProposal {
            txs: vec![tx_bytes.into()],
            max_tx_bytes: 1024 * 1024,
            local_last_commit: None,
            misbehavior: Vec::new(),
            height: block::Height::from(2u32),
            time: Time::unix_epoch(),
            next_validators_hash: Hash::None,
            proposer_address: account::Id::new([0u8; 20]),
        };

        let (prepared, _) = proposer
            .prepare_proposal(proposal, Some(&cache), false)
            .await;
        assert_eq!(
            prepared.txs.len(),
            2,
            "proposal should still include the user tx and aggregate bundle after re-validation"
        );

        Ok(())
    }

    #[test]
    fn aggregate_bundle_size_estimate_is_monotonic() {
        let chain_id = "shieldd-test";
        let small = vec![
            AggregateBundleFamilyEstimate {
                family_id: ProofFamilyId::Transfer,
                real_count: 8,
                padded_count: 8,
                aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
            },
            AggregateBundleFamilyEstimate {
                family_id: ProofFamilyId::NoteReshape(
                    shieldd_sdk_shielded_pool::NOTE_RESHAPE_FAMILY_SPECS[0].id,
                ),
                real_count: 8,
                padded_count: 8,
                aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
            },
        ];
        let large = vec![
            AggregateBundleFamilyEstimate {
                family_id: ProofFamilyId::Transfer,
                real_count: 256,
                padded_count: 256,
                aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
            },
            AggregateBundleFamilyEstimate {
                family_id: ProofFamilyId::NoteReshape(
                    shieldd_sdk_shielded_pool::NOTE_RESHAPE_FAMILY_SPECS[0].id,
                ),
                real_count: 256,
                padded_count: 256,
                aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
            },
        ];

        let small_size = App::estimate_aggregate_bundle_tx_size_bytes(chain_id, &small);
        let large_size = App::estimate_aggregate_bundle_tx_size_bytes(chain_id, &large);
        assert!(
            large_size >= small_size,
            "larger family counts should not estimate a smaller bundle"
        );
    }

    #[test]
    fn selected_prefix_respects_reduced_target_size() {
        let prefix_payload_bytes = vec![100_000, 250_000, 400_000, 550_000];
        let bundle_bytes = 96_000usize;
        let prefix_len = App::select_prefix_len_with_bundle_budget(
            &prefix_payload_bytes,
            600_000,
            AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
            bundle_bytes,
        );

        assert_eq!(prefix_len, 3);
        assert!(
            prefix_payload_bytes[prefix_len - 1] + bundle_bytes as u64
                <= 600_000 - AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES
        );
    }

    #[test]
    fn fallback_prefix_drops_tail_after_exact_bundle_miss() {
        let prefix_payload_bytes = vec![300_000, 600_000, 900_000];
        let initial_prefix_len = App::select_prefix_len_with_bundle_budget(
            &prefix_payload_bytes,
            1_000_000,
            AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
            80_000,
        );
        assert_eq!(initial_prefix_len, 3);

        let fallback_prefix_len = App::select_prefix_len_with_bundle_budget(
            &prefix_payload_bytes,
            1_000_000,
            AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
            140_000,
        )
        .min(initial_prefix_len.saturating_sub(1));

        assert_eq!(fallback_prefix_len, 2);
    }
}
