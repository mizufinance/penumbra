#[cfg(any(test, feature = "benchmark-helpers"))]
mod aggregate_diagnostics;
mod batch_input;
mod candidate;
mod delivery;
mod host;
mod lifecycle;
pub use batch_input::{BatchCandidate, BatchPreparation, BatchVerdict, PreparedBatch};
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
use shieldd_sdk_ibc::{StateReadExt as _, StateWriteExt as _};
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
use tendermint::Time;
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

pub mod state_key;

/// The inter-block state being written to by the application.
type InterBlockState = Arc<StateDelta<Snapshot>>;

/// The default maximum batch payload size (1 MB)
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
    height: u64,
    time: Time,
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
        let time = self
            .state
            .get_current_block_timestamp()
            .await?
            .checked_add(Duration::from_secs(1))
            .context("execution benchmark timestamp overflow")?;
        Ok(BenchBlockContext {
            height: next_height,
            time,
        })
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    fn process_proposal_request_from_envelope(
        context: &BenchBlockContext,
        envelope: &CandidateEnvelope,
    ) -> BatchCandidate {
        let mut txs = envelope
            .txs
            .iter()
            .cloned()
            .map(Bytes::from)
            .collect::<Vec<_>>();
        if let Some(bundle) = &envelope.aggregate_bundle_tx_bytes {
            txs.push(Bytes::from(bundle.clone()));
        }
        BatchCandidate {
            height: context.height,
            txs,
        }
    }

    pub(crate) fn ensure_user_tx_has_no_internal_actions(tx: &Transaction) -> Result<()> {
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

        state.accumulate_fees(base_fee.amount(), tip.amount());
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
    ) -> Result<BatchVerdict> {
        let context = self.benchmark_block_context().await?;
        let proposal = Self::process_proposal_request_from_envelope(&context, envelope);
        let sidecar = ProposalArtifactSidecar::from_record(envelope.sidecar.clone());

        Ok(self
            .validate_batch(proposal, stateless_cache, Some(&sidecar), false)
            .await)
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub async fn execute_validated_candidate_envelope_profiled(
        &mut self,
        envelope: &CandidateEnvelope,
        storage: Storage,
    ) -> Result<ExecutionBlockProfile> {
        let context = self.benchmark_block_context().await?;
        let begin_block = cnidarium_component::BlockContext {
            height: context.height,
            time: context.time,
        };
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

        let end_block_start = Instant::now();
        let _events = self.end_block(context.height).await;
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
        prepared: &PreparedBatch,
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

    /// Override the proposer aggregate segment size. Production default is 200.
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

    pub async fn prepare_batch(
        &mut self,
        mut proposal: BatchPreparation,
        stateless_cache: Option<&StatelessCache>,
        allow_oversized_proposal: bool,
    ) -> (PreparedBatch, Option<ProposalArtifactSidecar>) {
        let num_candidate_txs = proposal.txs.len();
        truncate_prepare_candidates(&mut proposal.txs);
        tracing::debug!(
            "processing PrepareProposal, found {} candidate transactions",
            num_candidate_txs
        );

        let max_proposal_size_bytes = prepare_proposal_payload_limit(proposal.max_tx_bytes);
        let (included_txs, sidecar) = match self
            .prepare_proposal_batched(
                proposal.height,
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

        tracing::debug!(
            "finished processing PrepareProposal, including {}/{} candidate transactions",
            included_txs.len(),
            num_candidate_txs
        );

        (PreparedBatch { txs: included_txs }, sidecar)
    }

    #[instrument(skip_all, ret, level = "debug")]
    pub async fn validate_batch(
        &mut self,
        proposal: BatchCandidate,
        stateless_cache: Option<&StatelessCache>,
        synthetic_sidecar: Option<&ProposalArtifactSidecar>,
        allow_oversized_proposal: bool,
    ) -> BatchVerdict {
        tracing::debug!(height = proposal.height, "processing proposal");

        let proposal_height = proposal.height;
        macro_rules! reject_process_proposal {
            ($reason:literal) => {{
                tracing::warn!(
                    height = proposal_height,
                    reason = $reason,
                    "process_proposal_reject_reason"
                );
                return BatchVerdict::Reject;
            }};
            ($reason:literal, $($field:tt)*) => {{
                tracing::warn!(
                    height = proposal_height,
                    reason = $reason,
                    $($field)*,
                    "process_proposal_reject_reason"
                );
                return BatchVerdict::Reject;
            }};
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

        BatchVerdict::Accept
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
            state_key::block_data::transactions_by_height(height).into(),
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
            state_key::block_data::transactions_by_height(height).into(),
            transactions_response.encode_to_vec(),
        );
        state_tx.apply();
        Ok(())
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

    /// Gets the chain revision number from the chain ID.
    async fn get_revision_number(&self) -> Result<u64> {
        let chain_id = self.get_chain_id().await?;
        Ok(ibc_types::core::connection::ChainId::from_string(&chain_id).version())
    }

    /// Returns the set of app parameters
    async fn get_app_params(&self) -> Result<AppParameters> {
        let chain_id = self.get_chain_id().await?;
        let compliance_params = self.get_compliance_params().await?;
        let fee_params = self.get_fee_params().await?;
        let ibc_params = self.get_ibc_params().await?;
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
                state_key::block_data::transactions_by_height(block_height).as_bytes(),
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

    /// Appends a transaction to the durable block log consumed by host queries.
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
            state_key::block_data::transactions_by_height(height).into(),
            transactions_response.encode_to_vec(),
        );
        Ok(())
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}

#[cfg(test)]
mod tests;
