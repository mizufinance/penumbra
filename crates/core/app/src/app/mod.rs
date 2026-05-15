mod preconsensus;
mod validation_support;

pub use self::preconsensus::{
    CheckTxProfile, PrepareProposalProfile, ProcessProposalProfile, ProposalArtifactSidecar,
    ProposalArtifactSidecarRecord, ProposalArtifactSidecarRecordEntry,
};
pub use self::validation_support::{
    candidate_digest_from_hashes, sidecar_commitment, CandidateEnvelope, EnvelopeValidationResult,
    ValidationNullifierCache, ValidationProfile, ValidationRejectReason, ValidationStageVerdict,
    ValidationVerdict, MAX_VALIDATION_ACTIONS_PER_TX, MAX_VALIDATION_NULLIFIERS_PER_BLOCK,
    MAX_VALIDATION_NULLIFIERS_PER_TX, MAX_VALIDATION_TX_COUNT,
};

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ark_ff::{BigInteger, PrimeField, Zero};
use ark_groth16::PreparedVerifyingKey;
use async_trait::async_trait;
use cnidarium::{ArcStateDeltaExt, Snapshot, StateDelta, StateRead, StateWrite, Storage};
use cnidarium_component::Component;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa as rdsa;
use ibc_types::core::connection::ChainId;
use jmt::RootHash;
use penumbra_sdk_compact_block::{component::CompactBlockManager, StatePayload};
use penumbra_sdk_compliance::registry::ComplianceRegistryRead as _;
use penumbra_sdk_compliance::Compliance;
use penumbra_sdk_fee::component::{
    clear_block_fee_price_cache, FeeComponent, FeePay as _, StateReadExt as _, StateWriteExt as _,
};
use penumbra_sdk_fee::{Fee, Gas, GasPrices};
use penumbra_sdk_governance::component::{Governance, StateReadExt as _, StateWriteExt as _};
use penumbra_sdk_ibc::component::{Ibc, StateWriteExt as _};
use penumbra_sdk_ibc::StateReadExt as _;
use penumbra_sdk_proof_aggregation::{
    aggregate_family_profiled, pad_items_to_power_of_two, prepare_verify_inputs, srs_id,
    verify_family_aggregate_profiled_unchecked, AggregateBuildBackendProfile, AggregateBundle,
    AggregateVerificationProfile, DevSrs, FamilyAggregate, ProofFamilyId,
};
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_proto::core::app::v1::TransactionsByHeightResponse;
use penumbra_sdk_proto::{DomainType, StateWriteProto as _};
use penumbra_sdk_sct::component::clock::EpochRead;
use penumbra_sdk_sct::component::sct::Sct;
use penumbra_sdk_sct::component::source::SourceContext as _;
use penumbra_sdk_sct::component::tree::SctManager as _;
use penumbra_sdk_sct::component::tree::SctRead as _;
use penumbra_sdk_sct::component::{StateReadExt as _, StateWriteExt as _};
use penumbra_sdk_sct::epoch::Epoch;
use penumbra_sdk_sct::{CommitmentSource, Nullifier};
use penumbra_sdk_shielded_pool::component::ClueManager as _;
use penumbra_sdk_shielded_pool::component::{
    transfer_extract_public, transfer_to_batch_item, NoteManager as _, ShieldedPool,
    StateReadExt as _, StateWriteExt as _,
};
use penumbra_sdk_transaction::gas::GasCost as _;
use penumbra_sdk_transaction::{Action, Transaction, TransactionBody, TransactionParameters};
use penumbra_sdk_txhash::AuthorizingData as _;
use penumbra_sdk_validator::component::{
    stake::ConsensusUpdateRead, Staking, StateReadExt as _, StateWriteExt as _,
};
use prost::bytes::Bytes;
use prost::Message as _;
use tendermint::abci::{self, Event};
use tendermint::v0_37::abci::{request, response};
use tendermint::validator::Update;
use tendermint::{account, block, chain, AppHash, Hash, Time};
use tokio::time::sleep;
use tracing::{instrument, Instrument};

use crate::action_handler::transaction::{
    check_and_execute_profiled, check_historical_with_context,
    prepare_candidate_read_blocking_profiled, prepare_candidate_read_profiled,
    supports_parallel_prepare, HistoricalCheckContext, PreparedCandidateRead,
};
use crate::action_handler::AppActionHandler;
use crate::block_tx_indexing::BlockTxIndexingMode;
use crate::event::EventAppParametersChange;
use crate::genesis::AppState;
use crate::params::change::ParameterChangeExt as _;
use crate::params::AppParameters;
use crate::stateless_cache::{CacheEntry, HistoricalValidationStamp, StatelessCache, TxArtifact};
use crate::{metrics, PenumbraHost};
use sha2::Digest as _;
use std::sync::OnceLock;

pub mod state_key;

/// The inter-block state being written to by the application.
type InterBlockState = Arc<StateDelta<Snapshot>>;

/// The maximum size of a CometBFT block payload (1MB)
pub const MAX_BLOCK_TXS_PAYLOAD_BYTES: usize = 1024 * 1024;

/// The maximum size of a single individual transaction (96KB).
pub const MAX_TRANSACTION_SIZE_BYTES: usize = 96 * 1024;

/// The maximum size of the evidence portion of a block (30KB).
pub const MAX_EVIDENCE_SIZE_BYTES: usize = 30 * 1024;

const AGGREGATE_BUNDLE_VERSION: u32 = 1;
const MAX_PADDED_PROOF_COUNT: usize = 32_768;
const AGGREGATE_DEBUG_DIR_ENV: &str = "PENUMBRA_AGGREGATE_DEBUG_DIR";
static AGGREGATE_DEBUG_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct AggregateDebugRow {
    tx_id: String,
    action_index: Option<usize>,
    family_local_index: usize,
    public_inputs: Vec<Fq>,
}

#[derive(Clone, Debug)]
struct AggregateDebugSegmentFamily {
    segment_index: usize,
    family_index: usize,
    family_id: ProofFamilyId,
    rows: Vec<AggregateDebugRow>,
}

fn aggregate_debug_root() -> Option<PathBuf> {
    std::env::var_os(AGGREGATE_DEBUG_DIR_ENV).map(PathBuf::from)
}

fn action_family_id(action: &Action) -> Option<ProofFamilyId> {
    match action {
        Action::Transfer(_) => Some(ProofFamilyId::Transfer),
        Action::Consolidate(consolidate) => {
            Some(ProofFamilyId::Consolidate(consolidate.body.family_id))
        }
        Action::Split(split) => Some(ProofFamilyId::Split(split.body.family_id)),
        Action::ShieldedIcs20Withdrawal(withdrawal) => Some(
            ProofFamilyId::ShieldedIcs20Withdrawal(withdrawal.body.family_id),
        ),
        _ => None,
    }
}

fn proof_verification_key_for_family(
    family_id: ProofFamilyId,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    match family_id {
        ProofFamilyId::Transfer => penumbra_sdk_proof_params::transfer_proof_verification_key(),
        ProofFamilyId::Consolidate(family_id) => family_id.proof_verification_key(),
        ProofFamilyId::Split(family_id) => family_id.proof_verification_key(),
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => family_id.proof_verification_key(),
    }
}

fn proof_family_label(family_id: ProofFamilyId) -> &'static str {
    match family_id {
        ProofFamilyId::Transfer => penumbra_sdk_shielded_pool::TRANSFER_PROOF_LABEL,
        ProofFamilyId::Consolidate(family_id) => family_id.label(),
        ProofFamilyId::Split(family_id) => family_id.label(),
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => family_id.label(),
    }
}

fn proof_family_batch_verify_stage(family_id: ProofFamilyId) -> &'static str {
    match family_id {
        ProofFamilyId::Transfer => "transfer_batch_verify",
        ProofFamilyId::Consolidate(_) => "consolidate_batch_verify",
        ProofFamilyId::Split(_) => "split_batch_verify",
        ProofFamilyId::ShieldedIcs20Withdrawal(_) => "shielded_ics20_withdrawal_batch_verify",
    }
}

fn fq_hex(value: &Fq) -> String {
    hex::encode(value.into_bigint().to_bytes_le())
}

fn maybe_write_aggregate_debug_dump(
    phase: &str,
    segment_index: usize,
    family_index: usize,
    family_id: ProofFamilyId,
    rows: &[AggregateDebugRow],
    padded_public_inputs: &[Vec<Fq>],
    aggregate: Option<&FamilyAggregate>,
) {
    let Some(root) = aggregate_debug_root() else {
        return;
    };
    if let Err(error) = write_aggregate_debug_dump(
        &root,
        phase,
        segment_index,
        family_index,
        family_id,
        rows,
        padded_public_inputs,
        aggregate,
    ) {
        tracing::warn!(
            ?error,
            phase,
            ?family_id,
            "failed to write aggregate debug dump"
        );
    }
}

fn write_aggregate_debug_dump(
    root: &Path,
    phase: &str,
    segment_index: usize,
    family_index: usize,
    family_id: ProofFamilyId,
    rows: &[AggregateDebugRow],
    padded_public_inputs: &[Vec<Fq>],
    aggregate: Option<&FamilyAggregate>,
) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("creating aggregate debug directory {}", root.display()))?;
    let seq = AGGREGATE_DEBUG_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_path = root.join(format!(
        "{seq:06}-{phase}-segment{segment_index:03}-family{family_index:02}-{family_id:?}.txt"
    ));
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&file_path)
        .with_context(|| format!("opening aggregate debug dump {}", file_path.display()))?;

    writeln!(file, "phase={phase}")?;
    writeln!(file, "segment_index={segment_index}")?;
    writeln!(file, "family_index={family_index}")?;
    writeln!(file, "family_id={family_id:?}")?;
    writeln!(file, "real_count={}", rows.len())?;
    writeln!(file, "padded_count={}", padded_public_inputs.len())?;

    if let Some(aggregate) = aggregate {
        writeln!(file, "bundle_real_count={}", aggregate.real_count)?;
        writeln!(file, "bundle_padded_count={}", aggregate.padded_count)?;
        writeln!(
            file,
            "aggregate_proof_sha256={}",
            hex::encode(sha2::Sha256::digest(&aggregate.aggregate_proof))
        )?;
    }

    for (row_index, row) in rows.iter().enumerate() {
        let action_index = row
            .action_index
            .map(|index| index.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let public_inputs = row
            .public_inputs
            .iter()
            .map(fq_hex)
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            file,
            "row[{row_index}].tx_id={} action_index={} family_local_index={} public_inputs=[{}]",
            row.tx_id, action_index, row.family_local_index, public_inputs
        )?;
    }

    for (pad_index, inputs) in padded_public_inputs.iter().enumerate() {
        let rendered = inputs.iter().map(fq_hex).collect::<Vec<_>>().join(",");
        writeln!(file, "padded_public_inputs[{pad_index}]=[{rendered}]")?;
    }

    Ok(())
}
const BATCH_VERIFY_CHUNK_MIN_ITEMS: usize = 512;
const BATCH_VERIFY_MAX_CHUNKS_PER_FAMILY: usize = 8;
const AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES: u64 = 8 * 1024;
const AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER: usize = 24 * 1024;

fn max_transaction_size_bytes() -> usize {
    static OVERRIDE: OnceLock<usize> = OnceLock::new();
    *OVERRIDE.get_or_init(|| {
        std::env::var("PENUMBRA_MAX_TRANSACTION_SIZE_BYTES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(MAX_TRANSACTION_SIZE_BYTES)
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct VerifiedStatefulTxBreakdown {
    check_historical_ms: f64,
    checktx_fast_context_load_ms: f64,
    checktx_fast_read_queue_wait_ms: f64,
    checktx_fast_read_blocking_total_ms: f64,
    checktx_fast_prepare_join_wall_ms: f64,
    checktx_fast_apply_wall_ms: f64,
    get_block_height_ms: f64,
    clone_tx_ms: f64,
    proto_convert_ms: f64,
    put_block_transaction_ms: f64,
    tx_log_read_ms: f64,
    tx_log_encode_ms: f64,
    tx_log_put_raw_ms: f64,
    begin_state_tx_ms: f64,
    index_tx_ms: f64,
    check_and_execute_ms: f64,
    set_source_ms: f64,
    pay_fee_ms: f64,
    action_execute_ms: f64,
    read_local_precheck_ms: f64,
    read_lookup_wait_or_join_ms: f64,
    read_historical_check_ms: f64,
    read_nullifier_wait_ms: f64,
    read_anchor_cache_wait_ms: f64,
    spend_action_execute_ms: f64,
    spend_nullifier_check_ms: f64,
    spend_nullifier_tx_local_scan_ms: f64,
    spend_nullifier_block_log_lookup_ms: f64,
    spend_nullifier_committed_check_ms: f64,
    spend_nullifier_enqueue_ms: f64,
    spend_nullifier_stage_ms: f64,
    spend_nullifier_merge_ms: f64,
    nullifier_lookup_count: usize,
    output_action_execute_ms: f64,
    output_add_note_payload_ms: f64,
    read_anchor_validation_ms: f64,
    read_committed_nullifier_ms: f64,
    read_effects_build_ms: f64,
    candidate_read_wall_ms: f64,
    serial_apply_wall_ms: f64,
    serial_same_block_conflict_ms: f64,
    serial_nullifier_insert_ms: f64,
    proposal_nullifier_lookup_write_ms: f64,
    proposal_pending_nullifier_stage_ms: f64,
    serial_sct_append_ms: f64,
    serial_event_emit_ms: f64,
    serial_fee_apply_ms: f64,
    other_action_execute_ms: f64,
    record_clues_ms: f64,
    apply_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct BlockTxIndexWriteProfile {
    tx_log_read_ms: f64,
    tx_log_encode_ms: f64,
    tx_log_put_raw_ms: f64,
}

#[derive(Default)]
struct PrepareBlockLocalState {
    seen_nullifiers: BTreeSet<Nullifier>,
    staged_nullifiers: Vec<(Nullifier, CommitmentSource)>,
}

#[derive(Clone, Debug)]
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
    VerifiedArtifact(Arc<TxArtifact>),
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
            CandidateData::ExtractedArtifact(artifact)
            | CandidateData::VerifiedArtifact(artifact) => &artifact.tx,
        }
    }

    fn artifact(&self) -> Option<Arc<TxArtifact>> {
        match &self.data {
            CandidateData::ExtractedArtifact(artifact)
            | CandidateData::VerifiedArtifact(artifact) => Some(artifact.clone()),
            CandidateData::Decoded(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AggregateBuildProfile {
    pub merge_items_ms: f64,
    pub setup_ms: f64,
    pub padding_ms: f64,
    pub collect_proofs_ms: f64,
    pub backend_core_ms: f64,
    pub backend_point_extract_ms: f64,
    pub backend_prepared_srs_ms: f64,
    pub backend_commitment_key_extract_ms: f64,
    pub backend_commitment_ms: f64,
    pub backend_com_a_ms: f64,
    pub backend_com_b_ms: f64,
    pub backend_com_c_ms: f64,
    pub backend_pairing_normalize_batch_ms: f64,
    pub backend_pairing_prepare_ms: f64,
    pub backend_pairing_miller_loop_ms: f64,
    pub backend_pairing_final_exponentiation_ms: f64,
    pub backend_randomizer_ms: f64,
    pub backend_structured_scalar_ms: f64,
    pub backend_weighted_a_ms: f64,
    pub backend_ip_ab_ms: f64,
    pub backend_agg_c_ms: f64,
    pub backend_ck_1_r_ms: f64,
    pub backend_consistency_check_ms: f64,
    pub backend_tipa_ab_ms: f64,
    pub backend_tipa_c_ms: f64,
    pub backend_tipa_ab_gipa_ms: f64,
    pub backend_tipa_ab_gipa_commit_l_ms: f64,
    pub backend_tipa_ab_gipa_commit_r_ms: f64,
    pub backend_tipa_ab_gipa_challenge_ms: f64,
    pub backend_tipa_ab_gipa_rescale_m1_ms: f64,
    pub backend_tipa_ab_gipa_rescale_m2_ms: f64,
    pub backend_tipa_ab_gipa_rescale_ck1_ms: f64,
    pub backend_tipa_ab_gipa_rescale_ck2_ms: f64,
    pub backend_tipa_ab_transcript_inverse_ms: f64,
    pub backend_tipa_ab_kzg_challenge_ms: f64,
    pub backend_tipa_ab_kzg_coefficient_build_ms: f64,
    pub backend_tipa_ab_kzg_eval_quotient_ms: f64,
    pub backend_tipa_ab_kzg_opening_msm_ms: f64,
    pub backend_tipa_ab_kzg_opening_ck_a_ms: f64,
    pub backend_tipa_ab_kzg_opening_ck_b_ms: f64,
    pub backend_tipa_c_gipa_ms: f64,
    pub backend_tipa_c_gipa_commit_l_ms: f64,
    pub backend_tipa_c_gipa_commit_r_ms: f64,
    pub backend_tipa_c_gipa_challenge_ms: f64,
    pub backend_tipa_c_gipa_rescale_m1_ms: f64,
    pub backend_tipa_c_gipa_rescale_m2_ms: f64,
    pub backend_tipa_c_gipa_rescale_ck1_ms: f64,
    pub backend_tipa_c_gipa_rescale_ck2_ms: f64,
    pub backend_tipa_c_transcript_inverse_ms: f64,
    pub backend_tipa_c_kzg_challenge_ms: f64,
    pub backend_tipa_c_kzg_coefficient_build_ms: f64,
    pub backend_tipa_c_kzg_eval_quotient_ms: f64,
    pub backend_tipa_c_kzg_opening_msm_ms: f64,
    pub backend_tipa_c_kzg_opening_ck_a_ms: f64,
    pub proof_serialize_ms: f64,
    pub bundle_tx_build_ms: f64,
    pub spend_ms: f64,
    pub output_ms: f64,
    pub other_ms: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionBlockProfile {
    pub block_tx_count: usize,
    pub begin_block_ms: f64,
    pub deliver_txs_wall_ms: f64,
    pub end_block_ms: f64,
    pub commit_ms: f64,
    pub execute_tx_ms: f64,
    pub begin_state_tx_ms: f64,
    pub index_tx_ms: f64,
    pub get_block_height_ms: f64,
    pub clone_tx_ms: f64,
    pub proto_convert_ms: f64,
    pub put_block_transaction_ms: f64,
    pub tx_log_read_ms: f64,
    pub tx_log_encode_ms: f64,
    pub tx_log_put_raw_ms: f64,
    pub check_and_execute_ms: f64,
    pub set_source_ms: f64,
    pub pay_fee_ms: f64,
    pub action_execute_ms: f64,
    pub read_local_precheck_ms: f64,
    pub read_lookup_wait_or_join_ms: f64,
    pub read_historical_check_ms: f64,
    pub read_nullifier_wait_ms: f64,
    pub read_anchor_cache_wait_ms: f64,
    pub spend_action_execute_ms: f64,
    pub spend_nullifier_check_ms: f64,
    pub spend_nullifier_tx_local_scan_ms: f64,
    pub spend_nullifier_block_log_lookup_ms: f64,
    pub spend_nullifier_committed_check_ms: f64,
    pub spend_nullifier_enqueue_ms: f64,
    pub spend_nullifier_stage_ms: f64,
    pub spend_nullifier_merge_ms: f64,
    pub nullifier_lookup_count: usize,
    pub output_action_execute_ms: f64,
    pub output_add_note_payload_ms: f64,
    pub other_action_execute_ms: f64,
    pub record_clues_ms: f64,
    pub apply_ms: f64,
}

impl AggregateBuildProfile {
    fn add_family_time(&mut self, family_id: ProofFamilyId, elapsed_ms: f64) {
        match family_id {
            ProofFamilyId::Transfer
            | ProofFamilyId::Consolidate(_)
            | ProofFamilyId::Split(_)
            | ProofFamilyId::ShieldedIcs20Withdrawal(_) => self.other_ms += elapsed_ms,
        }
    }

    fn apply_backend_build_profile(&mut self, backend: &AggregateBuildBackendProfile) {
        self.collect_proofs_ms = backend.collect_proofs_ms;
        self.backend_core_ms = backend.backend_aggregate_ms;
        self.backend_point_extract_ms = backend.backend_point_extract_ms;
        self.backend_prepared_srs_ms = backend.backend_prepared_srs_ms;
        self.backend_commitment_key_extract_ms = backend.backend_commitment_key_extract_ms;
        self.backend_commitment_ms = backend.backend_commitment_ms;
        self.backend_com_a_ms = backend.backend_com_a_ms;
        self.backend_com_b_ms = backend.backend_com_b_ms;
        self.backend_com_c_ms = backend.backend_com_c_ms;
        self.backend_pairing_normalize_batch_ms = backend.backend_pairing_normalize_batch_ms;
        self.backend_pairing_prepare_ms = backend.backend_pairing_prepare_ms;
        self.backend_pairing_miller_loop_ms = backend.backend_pairing_miller_loop_ms;
        self.backend_pairing_final_exponentiation_ms =
            backend.backend_pairing_final_exponentiation_ms;
        self.backend_randomizer_ms = backend.backend_randomizer_ms;
        self.backend_structured_scalar_ms = backend.backend_structured_scalar_ms;
        self.backend_weighted_a_ms = backend.backend_weighted_a_ms;
        self.backend_ip_ab_ms = backend.backend_ip_ab_ms;
        self.backend_agg_c_ms = backend.backend_agg_c_ms;
        self.backend_ck_1_r_ms = backend.backend_ck_1_r_ms;
        self.backend_consistency_check_ms = backend.backend_consistency_check_ms;
        self.backend_tipa_ab_ms = backend.backend_tipa_ab_ms;
        self.backend_tipa_c_ms = backend.backend_tipa_c_ms;
        self.backend_tipa_ab_gipa_ms = backend.backend_tipa_ab_gipa_ms;
        self.backend_tipa_ab_gipa_commit_l_ms = backend.backend_tipa_ab_gipa_commit_l_ms;
        self.backend_tipa_ab_gipa_commit_r_ms = backend.backend_tipa_ab_gipa_commit_r_ms;
        self.backend_tipa_ab_gipa_challenge_ms = backend.backend_tipa_ab_gipa_challenge_ms;
        self.backend_tipa_ab_gipa_rescale_m1_ms = backend.backend_tipa_ab_gipa_rescale_m1_ms;
        self.backend_tipa_ab_gipa_rescale_m2_ms = backend.backend_tipa_ab_gipa_rescale_m2_ms;
        self.backend_tipa_ab_gipa_rescale_ck1_ms = backend.backend_tipa_ab_gipa_rescale_ck1_ms;
        self.backend_tipa_ab_gipa_rescale_ck2_ms = backend.backend_tipa_ab_gipa_rescale_ck2_ms;
        self.backend_tipa_ab_transcript_inverse_ms = backend.backend_tipa_ab_transcript_inverse_ms;
        self.backend_tipa_ab_kzg_challenge_ms = backend.backend_tipa_ab_kzg_challenge_ms;
        self.backend_tipa_ab_kzg_coefficient_build_ms =
            backend.backend_tipa_ab_kzg_coefficient_build_ms;
        self.backend_tipa_ab_kzg_eval_quotient_ms = backend.backend_tipa_ab_kzg_eval_quotient_ms;
        self.backend_tipa_ab_kzg_opening_msm_ms = backend.backend_tipa_ab_kzg_opening_msm_ms;
        self.backend_tipa_ab_kzg_opening_ck_a_ms = backend.backend_tipa_ab_kzg_opening_ck_a_ms;
        self.backend_tipa_ab_kzg_opening_ck_b_ms = backend.backend_tipa_ab_kzg_opening_ck_b_ms;
        self.backend_tipa_c_gipa_ms = backend.backend_tipa_c_gipa_ms;
        self.backend_tipa_c_gipa_commit_l_ms = backend.backend_tipa_c_gipa_commit_l_ms;
        self.backend_tipa_c_gipa_commit_r_ms = backend.backend_tipa_c_gipa_commit_r_ms;
        self.backend_tipa_c_gipa_challenge_ms = backend.backend_tipa_c_gipa_challenge_ms;
        self.backend_tipa_c_gipa_rescale_m1_ms = backend.backend_tipa_c_gipa_rescale_m1_ms;
        self.backend_tipa_c_gipa_rescale_m2_ms = backend.backend_tipa_c_gipa_rescale_m2_ms;
        self.backend_tipa_c_gipa_rescale_ck1_ms = backend.backend_tipa_c_gipa_rescale_ck1_ms;
        self.backend_tipa_c_gipa_rescale_ck2_ms = backend.backend_tipa_c_gipa_rescale_ck2_ms;
        self.backend_tipa_c_transcript_inverse_ms = backend.backend_tipa_c_transcript_inverse_ms;
        self.backend_tipa_c_kzg_challenge_ms = backend.backend_tipa_c_kzg_challenge_ms;
        self.backend_tipa_c_kzg_coefficient_build_ms =
            backend.backend_tipa_c_kzg_coefficient_build_ms;
        self.backend_tipa_c_kzg_eval_quotient_ms = backend.backend_tipa_c_kzg_eval_quotient_ms;
        self.backend_tipa_c_kzg_opening_msm_ms = backend.backend_tipa_c_kzg_opening_msm_ms;
        self.backend_tipa_c_kzg_opening_ck_a_ms = backend.backend_tipa_c_kzg_opening_ck_a_ms;
        self.proof_serialize_ms = backend.serialize_ms;
    }

    pub fn merge(&mut self, other: &AggregateBuildProfile) {
        self.merge_items_ms += other.merge_items_ms;
        self.setup_ms += other.setup_ms;
        self.padding_ms += other.padding_ms;
        self.collect_proofs_ms += other.collect_proofs_ms;
        self.backend_core_ms += other.backend_core_ms;
        self.backend_point_extract_ms += other.backend_point_extract_ms;
        self.backend_prepared_srs_ms += other.backend_prepared_srs_ms;
        self.backend_commitment_key_extract_ms += other.backend_commitment_key_extract_ms;
        self.backend_commitment_ms += other.backend_commitment_ms;
        self.backend_com_a_ms += other.backend_com_a_ms;
        self.backend_com_b_ms += other.backend_com_b_ms;
        self.backend_com_c_ms += other.backend_com_c_ms;
        self.backend_pairing_normalize_batch_ms += other.backend_pairing_normalize_batch_ms;
        self.backend_pairing_prepare_ms += other.backend_pairing_prepare_ms;
        self.backend_pairing_miller_loop_ms += other.backend_pairing_miller_loop_ms;
        self.backend_pairing_final_exponentiation_ms +=
            other.backend_pairing_final_exponentiation_ms;
        self.backend_randomizer_ms += other.backend_randomizer_ms;
        self.backend_structured_scalar_ms += other.backend_structured_scalar_ms;
        self.backend_weighted_a_ms += other.backend_weighted_a_ms;
        self.backend_ip_ab_ms += other.backend_ip_ab_ms;
        self.backend_agg_c_ms += other.backend_agg_c_ms;
        self.backend_ck_1_r_ms += other.backend_ck_1_r_ms;
        self.backend_consistency_check_ms += other.backend_consistency_check_ms;
        self.backend_tipa_ab_ms += other.backend_tipa_ab_ms;
        self.backend_tipa_c_ms += other.backend_tipa_c_ms;
        self.backend_tipa_ab_gipa_ms += other.backend_tipa_ab_gipa_ms;
        self.backend_tipa_ab_gipa_commit_l_ms += other.backend_tipa_ab_gipa_commit_l_ms;
        self.backend_tipa_ab_gipa_commit_r_ms += other.backend_tipa_ab_gipa_commit_r_ms;
        self.backend_tipa_ab_gipa_challenge_ms += other.backend_tipa_ab_gipa_challenge_ms;
        self.backend_tipa_ab_gipa_rescale_m1_ms += other.backend_tipa_ab_gipa_rescale_m1_ms;
        self.backend_tipa_ab_gipa_rescale_m2_ms += other.backend_tipa_ab_gipa_rescale_m2_ms;
        self.backend_tipa_ab_gipa_rescale_ck1_ms += other.backend_tipa_ab_gipa_rescale_ck1_ms;
        self.backend_tipa_ab_gipa_rescale_ck2_ms += other.backend_tipa_ab_gipa_rescale_ck2_ms;
        self.backend_tipa_ab_transcript_inverse_ms += other.backend_tipa_ab_transcript_inverse_ms;
        self.backend_tipa_ab_kzg_challenge_ms += other.backend_tipa_ab_kzg_challenge_ms;
        self.backend_tipa_ab_kzg_coefficient_build_ms +=
            other.backend_tipa_ab_kzg_coefficient_build_ms;
        self.backend_tipa_ab_kzg_eval_quotient_ms += other.backend_tipa_ab_kzg_eval_quotient_ms;
        self.backend_tipa_ab_kzg_opening_msm_ms += other.backend_tipa_ab_kzg_opening_msm_ms;
        self.backend_tipa_ab_kzg_opening_ck_a_ms += other.backend_tipa_ab_kzg_opening_ck_a_ms;
        self.backend_tipa_ab_kzg_opening_ck_b_ms += other.backend_tipa_ab_kzg_opening_ck_b_ms;
        self.backend_tipa_c_gipa_ms += other.backend_tipa_c_gipa_ms;
        self.backend_tipa_c_gipa_commit_l_ms += other.backend_tipa_c_gipa_commit_l_ms;
        self.backend_tipa_c_gipa_commit_r_ms += other.backend_tipa_c_gipa_commit_r_ms;
        self.backend_tipa_c_gipa_challenge_ms += other.backend_tipa_c_gipa_challenge_ms;
        self.backend_tipa_c_gipa_rescale_m1_ms += other.backend_tipa_c_gipa_rescale_m1_ms;
        self.backend_tipa_c_gipa_rescale_m2_ms += other.backend_tipa_c_gipa_rescale_m2_ms;
        self.backend_tipa_c_gipa_rescale_ck1_ms += other.backend_tipa_c_gipa_rescale_ck1_ms;
        self.backend_tipa_c_gipa_rescale_ck2_ms += other.backend_tipa_c_gipa_rescale_ck2_ms;
        self.backend_tipa_c_transcript_inverse_ms += other.backend_tipa_c_transcript_inverse_ms;
        self.backend_tipa_c_kzg_challenge_ms += other.backend_tipa_c_kzg_challenge_ms;
        self.backend_tipa_c_kzg_coefficient_build_ms +=
            other.backend_tipa_c_kzg_coefficient_build_ms;
        self.backend_tipa_c_kzg_eval_quotient_ms += other.backend_tipa_c_kzg_eval_quotient_ms;
        self.backend_tipa_c_kzg_opening_msm_ms += other.backend_tipa_c_kzg_opening_msm_ms;
        self.backend_tipa_c_kzg_opening_ck_a_ms += other.backend_tipa_c_kzg_opening_ck_a_ms;
        self.proof_serialize_ms += other.proof_serialize_ms;
        self.bundle_tx_build_ms += other.bundle_tx_build_ms;
        self.spend_ms += other.spend_ms;
        self.output_ms += other.output_ms;
        self.other_ms += other.other_ms;
    }

    pub fn scale(&mut self, factor: f64) {
        self.merge_items_ms *= factor;
        self.setup_ms *= factor;
        self.padding_ms *= factor;
        self.collect_proofs_ms *= factor;
        self.backend_core_ms *= factor;
        self.backend_point_extract_ms *= factor;
        self.backend_prepared_srs_ms *= factor;
        self.backend_commitment_key_extract_ms *= factor;
        self.backend_commitment_ms *= factor;
        self.backend_com_a_ms *= factor;
        self.backend_com_b_ms *= factor;
        self.backend_com_c_ms *= factor;
        self.backend_pairing_normalize_batch_ms *= factor;
        self.backend_pairing_prepare_ms *= factor;
        self.backend_pairing_miller_loop_ms *= factor;
        self.backend_pairing_final_exponentiation_ms *= factor;
        self.backend_randomizer_ms *= factor;
        self.backend_structured_scalar_ms *= factor;
        self.backend_weighted_a_ms *= factor;
        self.backend_ip_ab_ms *= factor;
        self.backend_agg_c_ms *= factor;
        self.backend_ck_1_r_ms *= factor;
        self.backend_consistency_check_ms *= factor;
        self.backend_tipa_ab_ms *= factor;
        self.backend_tipa_c_ms *= factor;
        self.backend_tipa_ab_gipa_ms *= factor;
        self.backend_tipa_ab_gipa_commit_l_ms *= factor;
        self.backend_tipa_ab_gipa_commit_r_ms *= factor;
        self.backend_tipa_ab_gipa_challenge_ms *= factor;
        self.backend_tipa_ab_gipa_rescale_m1_ms *= factor;
        self.backend_tipa_ab_gipa_rescale_m2_ms *= factor;
        self.backend_tipa_ab_gipa_rescale_ck1_ms *= factor;
        self.backend_tipa_ab_gipa_rescale_ck2_ms *= factor;
        self.backend_tipa_ab_transcript_inverse_ms *= factor;
        self.backend_tipa_ab_kzg_challenge_ms *= factor;
        self.backend_tipa_ab_kzg_coefficient_build_ms *= factor;
        self.backend_tipa_ab_kzg_eval_quotient_ms *= factor;
        self.backend_tipa_ab_kzg_opening_msm_ms *= factor;
        self.backend_tipa_ab_kzg_opening_ck_a_ms *= factor;
        self.backend_tipa_ab_kzg_opening_ck_b_ms *= factor;
        self.backend_tipa_c_gipa_ms *= factor;
        self.backend_tipa_c_gipa_commit_l_ms *= factor;
        self.backend_tipa_c_gipa_commit_r_ms *= factor;
        self.backend_tipa_c_gipa_challenge_ms *= factor;
        self.backend_tipa_c_gipa_rescale_m1_ms *= factor;
        self.backend_tipa_c_gipa_rescale_m2_ms *= factor;
        self.backend_tipa_c_gipa_rescale_ck1_ms *= factor;
        self.backend_tipa_c_gipa_rescale_ck2_ms *= factor;
        self.backend_tipa_c_transcript_inverse_ms *= factor;
        self.backend_tipa_c_kzg_challenge_ms *= factor;
        self.backend_tipa_c_kzg_coefficient_build_ms *= factor;
        self.backend_tipa_c_kzg_eval_quotient_ms *= factor;
        self.backend_tipa_c_kzg_opening_msm_ms *= factor;
        self.backend_tipa_c_kzg_opening_ck_a_ms *= factor;
        self.proof_serialize_ms *= factor;
        self.bundle_tx_build_ms *= factor;
        self.spend_ms *= factor;
        self.output_ms *= factor;
        self.other_ms *= factor;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AggregateVerifyProfile {
    total_ms: f64,
    expected_segments_ms: f64,
    prepare_inputs_ms: f64,
    backend_deserialize_ms: f64,
    backend_challenge_ms: f64,
    backend_tipa_ab_ms: f64,
    backend_tipa_c_ms: f64,
    backend_public_input_fold_ms: f64,
    backend_ppe_ms: f64,
    backend_core_total_ms: f64,
}

impl AggregateVerifyProfile {
    fn merge_backend_profile(&mut self, backend: &AggregateVerificationProfile) {
        self.backend_deserialize_ms += backend.deserialize_ms;
        self.backend_challenge_ms += backend.challenge_ms;
        self.backend_tipa_ab_ms += backend.tipa_ab_ms;
        self.backend_tipa_c_ms += backend.tipa_c_ms;
        self.backend_public_input_fold_ms += backend.public_input_fold_ms;
        self.backend_ppe_ms += backend.ppe_ms;
        self.backend_core_total_ms += backend.core_total_ms;
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
    tail_tx_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ArtifactBuildBreakdown {
    pub precheck_ms: f64,
    pub action_extract_ms: f64,
    pub action_auth_sig_ms: f64,
    pub action_extract_public_ms: f64,
    pub action_to_batch_item_ms: f64,
    pub batch_verify_ms: f64,
}

impl ArtifactBuildBreakdown {
    pub fn merge(&mut self, other: &ArtifactBuildBreakdown) {
        self.precheck_ms += other.precheck_ms;
        self.action_extract_ms += other.action_extract_ms;
        self.action_auth_sig_ms += other.action_auth_sig_ms;
        self.action_extract_public_ms += other.action_extract_public_ms;
        self.action_to_batch_item_ms += other.action_to_batch_item_ms;
        self.batch_verify_ms += other.batch_verify_ms;
    }
}

#[derive(Clone, Debug)]
pub struct CheckTxSharedContext {
    pub(crate) sct_base_position: penumbra_sdk_tct::Position,
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
            fee.asset_id() == *penumbra_sdk_asset::BASE_ASSET_ID,
            "only base-asset fees are supported, found {}",
            fee.asset_id(),
        );
        Ok(self.base_gas_prices)
    }
}

#[derive(Clone, Debug, Default)]
struct BlockSctAppendLog {
    base_position: Option<penumbra_sdk_tct::Position>,
    next_offset: u64,
    entries: Vec<(penumbra_sdk_tct::Position, StatePayload)>,
}

impl BlockSctAppendLog {
    async fn reserve_positions<S: penumbra_sdk_sct::component::tree::SctRead>(
        &mut self,
        state: &S,
        payloads: Vec<StatePayload>,
    ) -> Result<Vec<(penumbra_sdk_tct::Position, StatePayload)>> {
        if payloads.is_empty() {
            return Ok(Vec::new());
        }

        let base_position = match self.base_position {
            Some(position) => position,
            None => {
                let position = state
                    .get_sct()
                    .await
                    .position()
                    .expect("state commitment tree is not full");
                self.base_position = Some(position);
                position
            }
        };

        let base_position_u64: u64 = base_position.into();
        let start = base_position_u64 + self.next_offset;
        let mut positioned = Vec::with_capacity(payloads.len());
        for (offset, payload) in payloads.into_iter().enumerate() {
            let position = penumbra_sdk_tct::Position::from(start + offset as u64);
            positioned.push((position, payload));
        }
        self.next_offset += positioned.len() as u64;

        Ok(positioned)
    }

    fn append_positioned(&mut self, entries: Vec<(penumbra_sdk_tct::Position, StatePayload)>) {
        self.entries.extend(entries);
    }

    fn take_entries(&mut self) -> Vec<(penumbra_sdk_tct::Position, StatePayload)> {
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

/// The Penumbra application, written as a bundle of [`Component`]s.
///
/// The [`App`] is not a [`Component`], but
/// it constructs the components and exposes a [`commit`](App::commit) that
/// commits the changes to the persistent storage and resets its subcomponents.
pub struct App {
    state: InterBlockState,
    committed_snapshot: Snapshot,
    snapshot_version: u64,
    block_tx_indexing_mode: BlockTxIndexingMode,
    deferred_block_transactions: Vec<penumbra_sdk_proto::core::transaction::v1::Transaction>,
    pending_sct_append_log: BlockSctAppendLog,
    checktx_shared_context: Option<Arc<CheckTxSharedContext>>,
    aggregate_retry_cache: Option<CachedProposalAggregate>,
    proposal_segment_tx_count: Option<usize>,
}

impl App {
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
            penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS
                .into_iter()
                .map(|spec| ProofFamilyId::Consolidate(spec.id)),
        );
        family_ids.extend(
            penumbra_sdk_shielded_pool::SPLIT_FAMILY_SPECS
                .into_iter()
                .map(|spec| ProofFamilyId::Split(spec.id)),
        );
        family_ids.extend(
            penumbra_sdk_shielded_pool::SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS
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

    fn aggregate_debug_segment_families(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_counts: Option<&[usize]>,
    ) -> Vec<AggregateDebugSegmentFamily> {
        let artifact_groups: Vec<&[Arc<TxArtifact>]> = match segment_tx_counts {
            Some(segment_tx_counts) if !segment_tx_counts.is_empty() => {
                let mut groups = Vec::with_capacity(segment_tx_counts.len());
                let mut start = 0usize;
                for &segment_tx_count in segment_tx_counts {
                    let end = start + segment_tx_count;
                    groups.push(&artifacts[start..end]);
                    start = end;
                }
                groups
            }
            _ => vec![artifacts],
        };

        let mut segments = Vec::new();
        for (segment_index, artifact_group) in artifact_groups.into_iter().enumerate() {
            let proof_items = Self::merge_artifact_proof_items(artifact_group);
            let mut family_index = 0usize;
            for family_id in Self::proof_family_ids() {
                let items = proof_items.get(&family_id).cloned().unwrap_or_default();
                if items.is_empty() {
                    continue;
                }
                segments.push(AggregateDebugSegmentFamily {
                    segment_index,
                    family_index,
                    family_id,
                    rows: Self::aggregate_debug_rows_for_family(artifact_group, family_id),
                });
                family_index += 1;
            }
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

        std::env::var("PENUMBRA_PREPARE_PROPOSAL_FILTER_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .map(|value| value.max(1))
            .unwrap_or(default)
    }

    fn accumulate_prepare_candidate_profile(
        profile: &mut PrepareProposalProfile,
        execution_profile: &VerifiedStatefulTxBreakdown,
    ) {
        profile.stateful_filter_check_historical_ms += execution_profile.check_historical_ms;
        profile.stateful_filter_get_block_height_ms += execution_profile.get_block_height_ms;
        profile.stateful_filter_clone_tx_ms += execution_profile.clone_tx_ms;
        profile.stateful_filter_proto_convert_ms += execution_profile.proto_convert_ms;
        profile.stateful_filter_put_block_transaction_ms +=
            execution_profile.put_block_transaction_ms;
        profile.stateful_filter_begin_state_tx_ms += execution_profile.begin_state_tx_ms;
        profile.stateful_filter_index_tx_ms += execution_profile.index_tx_ms;
        profile.stateful_filter_check_and_execute_ms += execution_profile.check_and_execute_ms;
        profile.stateful_filter_set_source_ms += execution_profile.set_source_ms;
        profile.stateful_filter_pay_fee_ms += execution_profile.pay_fee_ms;
        profile.stateful_filter_action_execute_ms += execution_profile.action_execute_ms;
        profile.stateful_filter_read_local_precheck_ms += execution_profile.read_local_precheck_ms;
        profile.stateful_filter_read_lookup_wait_or_join_ms +=
            execution_profile.read_lookup_wait_or_join_ms;
        profile.stateful_filter_read_historical_check_ms +=
            execution_profile.read_historical_check_ms;
        profile.stateful_filter_read_nullifier_wait_ms += execution_profile.read_nullifier_wait_ms;
        profile.stateful_filter_read_anchor_cache_wait_ms +=
            execution_profile.read_anchor_cache_wait_ms;
        profile.stateful_filter_spend_action_execute_ms +=
            execution_profile.spend_action_execute_ms;
        profile.stateful_filter_spend_nullifier_check_ms +=
            execution_profile.spend_nullifier_check_ms;
        profile.stateful_filter_spend_nullifier_tx_local_scan_ms +=
            execution_profile.spend_nullifier_tx_local_scan_ms;
        profile.stateful_filter_spend_nullifier_block_log_lookup_ms +=
            execution_profile.spend_nullifier_block_log_lookup_ms;
        profile.stateful_filter_spend_nullifier_committed_check_ms +=
            execution_profile.spend_nullifier_committed_check_ms;
        profile.stateful_filter_spend_nullifier_enqueue_ms +=
            execution_profile.spend_nullifier_enqueue_ms;
        profile.stateful_filter_spend_nullifier_stage_ms +=
            execution_profile.spend_nullifier_stage_ms;
        profile.stateful_filter_spend_nullifier_merge_ms +=
            execution_profile.spend_nullifier_merge_ms;
        profile.stateful_filter_nullifier_lookup_count += execution_profile.nullifier_lookup_count;
        profile.stateful_filter_output_action_execute_ms +=
            execution_profile.output_action_execute_ms;
        profile.stateful_filter_output_add_note_payload_ms +=
            execution_profile.output_add_note_payload_ms;
        profile.stateful_filter_read_anchor_validation_ms +=
            execution_profile.read_anchor_validation_ms;
        profile.stateful_filter_read_committed_nullifier_ms +=
            execution_profile.read_committed_nullifier_ms;
        profile.stateful_filter_read_effects_build_ms += execution_profile.read_effects_build_ms;
        profile.stateful_filter_candidate_effects_build_ms +=
            execution_profile.read_effects_build_ms;
        profile.stateful_filter_serial_apply_wall_ms += execution_profile.serial_apply_wall_ms;
        profile.stateful_filter_serial_same_block_conflict_ms +=
            execution_profile.serial_same_block_conflict_ms;
        profile.stateful_filter_serial_state_delta_apply_ms += execution_profile.apply_ms;
        profile.stateful_filter_serial_nullifier_insert_ms +=
            execution_profile.serial_nullifier_insert_ms;
        profile.stateful_filter_proposal_nullifier_lookup_write_ms +=
            execution_profile.proposal_nullifier_lookup_write_ms;
        profile.stateful_filter_proposal_pending_nullifier_stage_ms +=
            execution_profile.proposal_pending_nullifier_stage_ms;
        profile.stateful_filter_serial_sct_append_ms += execution_profile.serial_sct_append_ms;
        profile.stateful_filter_serial_event_emit_ms += execution_profile.serial_event_emit_ms;
        profile.stateful_filter_serial_fee_apply_ms += execution_profile.serial_fee_apply_ms;
        profile.stateful_filter_other_action_execute_ms +=
            execution_profile.other_action_execute_ms;
        profile.stateful_filter_record_clues_ms += execution_profile.record_clues_ms;
        profile.stateful_filter_apply_ms += execution_profile.apply_ms;
    }

    fn emit_stateful_filter_breakdown(
        candidate_tx_count: usize,
        included_candidate_count: usize,
        profile: &PrepareProposalProfile,
    ) {
        tracing::info!(
            candidate_tx_count,
            included_candidate_count,
            stateful_filter_execute_ms = profile.stateful_filter_execute_ms,
            stateful_filter_check_historical_ms = profile.stateful_filter_check_historical_ms,
            stateful_filter_check_and_execute_ms = profile.stateful_filter_check_and_execute_ms,
            read_local_precheck_ms = profile.stateful_filter_read_local_precheck_ms,
            read_lookup_wait_or_join_ms = profile.stateful_filter_read_lookup_wait_or_join_ms,
            read_historical_check_ms = profile.stateful_filter_read_historical_check_ms,
            read_nullifier_wait_ms = profile.stateful_filter_read_nullifier_wait_ms,
            read_anchor_cache_wait_ms = profile.stateful_filter_read_anchor_cache_wait_ms,
            read_task_dispatch_ms = profile.stateful_filter_read_task_dispatch_ms,
            stateful_filter_spend_nullifier_committed_check_ms =
                profile.stateful_filter_spend_nullifier_committed_check_ms,
            nullifier_lookup_count = profile.stateful_filter_nullifier_lookup_count,
            stateful_filter_spend_nullifier_check_ms =
                profile.stateful_filter_spend_nullifier_check_ms,
            stateful_filter_output_action_execute_ms =
                profile.stateful_filter_output_action_execute_ms,
            stateful_filter_apply_ms = profile.stateful_filter_apply_ms,
            parallel_read_wall_ms = profile.stateful_filter_parallel_read_wall_ms,
            parallel_read_sum_candidate_ms = profile.stateful_filter_parallel_read_sum_candidate_ms,
            parallel_read_tasks_spawned = profile.stateful_filter_parallel_read_tasks_spawned,
            parallel_read_max_inflight = profile.stateful_filter_parallel_read_max_inflight,
            parallel_read_configured_concurrency =
                profile.stateful_filter_parallel_read_configured_concurrency,
            parallel_read_effective_parallelism =
                if profile.stateful_filter_parallel_read_wall_ms > 0.0 {
                    profile.stateful_filter_parallel_read_sum_candidate_ms
                        / profile.stateful_filter_parallel_read_wall_ms
                } else {
                    0.0
                },
            anchor_cache_hits = profile.stateful_filter_anchor_cache_hits,
            anchor_cache_misses = profile.stateful_filter_anchor_cache_misses,
            anchor_unique_pairs = profile.stateful_filter_anchor_unique_pairs,
            claimed_anchor_cache_hits = profile.stateful_filter_claimed_anchor_cache_hits,
            claimed_anchor_cache_misses = profile.stateful_filter_claimed_anchor_cache_misses,
            claimed_anchor_unique_values = profile.stateful_filter_claimed_anchor_unique_values,
            read_anchor_validation_ms = profile.stateful_filter_read_anchor_validation_ms,
            read_committed_nullifier_ms = profile.stateful_filter_read_committed_nullifier_ms,
            read_effects_build_ms = profile.stateful_filter_read_effects_build_ms,
            candidate_read_wall_ms = profile.stateful_filter_candidate_read_wall_ms,
            candidate_effects_build_ms = profile.stateful_filter_candidate_effects_build_ms,
            serial_apply_wall_ms = profile.stateful_filter_serial_apply_wall_ms,
            serial_same_block_conflict_ms = profile.stateful_filter_serial_same_block_conflict_ms,
            serial_state_delta_apply_ms = profile.stateful_filter_serial_state_delta_apply_ms,
            serial_nullifier_insert_ms = profile.stateful_filter_serial_nullifier_insert_ms,
            proposal_nullifier_lookup_write_ms =
                profile.stateful_filter_proposal_nullifier_lookup_write_ms,
            proposal_pending_nullifier_stage_ms =
                profile.stateful_filter_proposal_pending_nullifier_stage_ms,
            serial_sct_append_ms = profile.stateful_filter_serial_sct_append_ms,
            serial_event_emit_ms = profile.stateful_filter_serial_event_emit_ms,
            serial_fee_apply_ms = profile.stateful_filter_serial_fee_apply_ms,
            candidate_read_wall_ms_per_tx = if included_candidate_count > 0 {
                profile.stateful_filter_candidate_read_wall_ms / included_candidate_count as f64
            } else {
                0.0
            },
            serial_apply_wall_ms_per_tx = if included_candidate_count > 0 {
                profile.stateful_filter_serial_apply_wall_ms / included_candidate_count as f64
            } else {
                0.0
            },
            "stateful_filter_breakdown"
        );
    }

    pub(crate) fn emit_checktx_breakdown(profile: &CheckTxProfile) {
        tracing::info!(
            checktx_total_wall_ms = profile.checktx_total_wall_ms,
            checktx_cache_lookup_ms = profile.checktx_cache_lookup_ms,
            checktx_stateless_phase_wall_ms = profile.checktx_stateless_phase_wall_ms,
            checktx_execute_fast_wall_ms = profile.checktx_execute_fast_wall_ms,
            checktx_fast_prepare_join_wall_ms = profile.checktx_fast_prepare_join_wall_ms,
            checktx_fast_apply_wall_ms = profile.checktx_fast_apply_wall_ms,
            decode_tx_ms = profile.decode_tx_ms,
            stateless_artifact_queue_wait_ms = profile.stateless_artifact_queue_wait_ms,
            stateless_task_join_wall_ms = profile.stateless_task_join_wall_ms,
            stateless_artifact_blocking_total_ms = profile.stateless_artifact_blocking_total_ms,
            stateless_artifact_ms = profile.stateless_artifact_ms,
            stateless_initial_cache_insert_ms = profile.stateless_initial_cache_insert_ms,
            stateless_historical_stamp_ms = profile.stateless_historical_stamp_ms,
            stateless_historical_mark_ms = profile.stateless_historical_mark_ms,
            stateless_final_cache_insert_ms = profile.stateless_final_cache_insert_ms,
            stateless_artifact_precheck_ms = profile.stateless_artifact_precheck_ms,
            stateless_artifact_action_extract_ms = profile.stateless_artifact_action_extract_ms,
            stateless_artifact_action_auth_sig_ms = profile.stateless_artifact_action_auth_sig_ms,
            stateless_artifact_action_extract_public_ms =
                profile.stateless_artifact_action_extract_public_ms,
            stateless_artifact_action_to_batch_item_ms =
                profile.stateless_artifact_action_to_batch_item_ms,
            stateless_artifact_batch_verify_ms = profile.stateless_artifact_batch_verify_ms,
            checktx_fast_context_load_ms = profile.checktx_fast_context_load_ms,
            checktx_fast_read_queue_wait_ms = profile.checktx_fast_read_queue_wait_ms,
            checktx_fast_read_blocking_total_ms = profile.checktx_fast_read_blocking_total_ms,
            check_historical_ms = profile.check_historical_ms,
            execute_ms = profile.execute_ms,
            execute_check_and_execute_ms = profile.execute_check_and_execute_ms,
            execute_read_local_precheck_ms = profile.execute_read_local_precheck_ms,
            execute_read_lookup_wait_or_join_ms = profile.execute_read_lookup_wait_or_join_ms,
            execute_read_historical_check_ms = profile.execute_read_historical_check_ms,
            execute_read_nullifier_wait_ms = profile.execute_read_nullifier_wait_ms,
            execute_read_anchor_cache_wait_ms = profile.execute_read_anchor_cache_wait_ms,
            execute_spend_nullifier_committed_check_ms =
                profile.execute_spend_nullifier_committed_check_ms,
            execute_nullifier_lookup_count = profile.execute_nullifier_lookup_count,
            execute_pay_fee_ms = profile.execute_pay_fee_ms,
            execute_apply_ms = profile.execute_apply_ms,
            checktx_candidate_read_wall_ms = profile.checktx_candidate_read_wall_ms,
            checktx_candidate_effects_build_ms = profile.checktx_candidate_effects_build_ms,
            checktx_serial_apply_wall_ms = profile.checktx_serial_apply_wall_ms,
            checktx_serial_nullifier_insert_ms = profile.checktx_serial_nullifier_insert_ms,
            checktx_serial_sct_append_ms = profile.checktx_serial_sct_append_ms,
            checktx_serial_event_emit_ms = profile.checktx_serial_event_emit_ms,
            checktx_serial_fee_apply_ms = profile.checktx_serial_fee_apply_ms,
            cache_hit_count = profile.cache_hit_count,
            "checktx_breakdown"
        );
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

        let tip = Fee(penumbra_sdk_asset::Value {
            amount: fee.amount() - base_fee.amount(),
            asset_id: fee.asset_id(),
        });

        state.record_proto(penumbra_sdk_proto::core::component::fee::v1::EventPaidFee {
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

    fn handle_proof_verification_result(context: &'static str, result: Result<()>) -> Result<()> {
        match result {
            Ok(()) => Ok(()),
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
        ArtifactBuildBreakdown,
    )> {
        use crate::action_handler::transaction::stateless::{
            check_memo_exists_if_outputs_absent_if_not, check_non_empty_transaction,
            num_clues_equal_to_num_outputs, valid_binding_signature,
        };
        use cnidarium_component::ActionHandler as _;
        use penumbra_sdk_shielded_pool::component::Ics20Transfer;

        let mut proof_items = Self::empty_proof_items();
        let mut artifacts = Vec::with_capacity(txs.len());
        let mut profile = ArtifactBuildBreakdown::default();

        for tx in txs {
            let precheck_start = Instant::now();
            Self::ensure_user_tx_has_no_internal_actions(tx)?;
            valid_binding_signature(tx)?;
            num_clues_equal_to_num_outputs(tx)?;
            check_memo_exists_if_outputs_absent_if_not(tx)?;
            check_non_empty_transaction(tx)?;
            profile.precheck_ms += precheck_start.elapsed().as_secs_f64() * 1000.0;

            let context = tx.context();
            let mut tx_proof_items = Self::empty_proof_items();

            let action_extract_start = Instant::now();
            for action in tx.actions() {
                match action {
                    Action::Transfer(transfer) => {
                        let t1 = Instant::now();
                        let public = transfer_extract_public(transfer, &context)
                            .context("transfer extract public failed")?;
                        profile.action_extract_public_ms += t1.elapsed().as_secs_f64() * 1000.0;

                        let t2 = Instant::now();
                        let item = transfer_to_batch_item(transfer, public)
                            .context("transfer to_batch_item failed")?;
                        profile.action_to_batch_item_ms += t2.elapsed().as_secs_f64() * 1000.0;
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
                        let t1 = Instant::now();
                        let public =
                            penumbra_sdk_shielded_pool::component::shielded_ics20_withdrawal_extract_public(
                                withdrawal,
                                &context,
                            )
                            .context("shielded ICS-20 withdrawal extract public failed")?;
                        profile.action_extract_public_ms += t1.elapsed().as_secs_f64() * 1000.0;

                        let t2 = Instant::now();
                        let item =
                            penumbra_sdk_shielded_pool::component::shielded_ics20_withdrawal_to_batch_item(
                                withdrawal,
                                public,
                            )
                            .context("shielded ICS-20 withdrawal to_batch_item failed")?;
                        profile.action_to_batch_item_ms += t2.elapsed().as_secs_f64() * 1000.0;
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
                    Action::Consolidate(_) => {
                        let consolidate = match action {
                            Action::Consolidate(consolidate) => consolidate,
                            _ => unreachable!(),
                        };
                        let t1 = Instant::now();
                        let public =
                            penumbra_sdk_shielded_pool::component::consolidate_extract_public(
                                consolidate,
                                &context,
                            )
                            .context("consolidate extract public failed")?;
                        profile.action_extract_public_ms += t1.elapsed().as_secs_f64() * 1000.0;

                        let t2 = Instant::now();
                        let item =
                            penumbra_sdk_shielded_pool::component::consolidate_to_batch_item(
                                consolidate,
                                public,
                            )
                            .context("consolidate to_batch_item failed")?;
                        profile.action_to_batch_item_ms += t2.elapsed().as_secs_f64() * 1000.0;
                        let family_id = action_family_id(&Action::Consolidate(consolidate.clone()))
                            .expect("consolidate has a proof family");

                        tx_proof_items
                            .get_mut(&family_id)
                            .expect("consolidate family exists")
                            .push(item.clone());
                        proof_items
                            .get_mut(&family_id)
                            .expect("consolidate family exists")
                            .push(item);
                    }
                    Action::Split(_) => {
                        let split = match action {
                            Action::Split(split) => split,
                            _ => unreachable!(),
                        };
                        let t1 = Instant::now();
                        let public = penumbra_sdk_shielded_pool::component::split_extract_public(
                            split, &context,
                        )
                        .context("split extract public failed")?;
                        profile.action_extract_public_ms += t1.elapsed().as_secs_f64() * 1000.0;

                        let t2 = Instant::now();
                        let item = penumbra_sdk_shielded_pool::component::split_to_batch_item(
                            split, public,
                        )
                        .context("split to_batch_item failed")?;
                        profile.action_to_batch_item_ms += t2.elapsed().as_secs_f64() * 1000.0;
                        let family_id = action_family_id(&Action::Split(split.clone()))
                            .expect("split has a proof family");

                        tx_proof_items
                            .get_mut(&family_id)
                            .expect("split family exists")
                            .push(item.clone());
                        proof_items
                            .get_mut(&family_id)
                            .expect("split family exists")
                            .push(item);
                    }
                    Action::ValidatorDefinition(action) => action.check_stateless(()).await?,
                    Action::ValidatorVote(action) => action.check_stateless(()).await?,
                    Action::ProposalSubmit(action) => action.check_stateless(()).await?,
                    Action::IbcRelay(action) => {
                        action
                            .clone()
                            .with_handler::<Ics20Transfer, PenumbraHost>()
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
                let t1 = Instant::now();
                let public = transfer_extract_public(transfer, &context)
                    .context("fee funding transfer extract public failed")?;
                profile.action_extract_public_ms += t1.elapsed().as_secs_f64() * 1000.0;

                let t2 = Instant::now();
                let item = transfer_to_batch_item(transfer, public)
                    .context("fee funding transfer to_batch_item failed")?;
                profile.action_to_batch_item_ms += t2.elapsed().as_secs_f64() * 1000.0;
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
            profile.action_extract_ms += action_extract_start.elapsed().as_secs_f64() * 1000.0;

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

        Ok((proof_items, artifacts, profile))
    }

    async fn build_tx_artifacts_profiled(
        txs: &[Arc<Transaction>],
    ) -> Result<(Vec<Arc<TxArtifact>>, ArtifactBuildBreakdown)> {
        if txs.is_empty() {
            return Ok((Vec::new(), ArtifactBuildBreakdown::default()));
        }

        let (proof_items, artifacts, mut profile) =
            Self::collect_consensus_proof_items_with_artifacts(txs).await?;
        let batch_verify_start = Instant::now();
        Self::legacy_batch_verify_proof_families(proof_items).await?;
        profile.batch_verify_ms = batch_verify_start.elapsed().as_secs_f64() * 1000.0;
        Ok((artifacts, profile))
    }

    async fn build_tx_artifacts_extracted_profiled(
        txs: &[Arc<Transaction>],
    ) -> Result<(Vec<Arc<TxArtifact>>, ArtifactBuildBreakdown)> {
        if txs.is_empty() {
            return Ok((Vec::new(), ArtifactBuildBreakdown::default()));
        }

        let (_proof_items, artifacts, profile) =
            Self::collect_consensus_proof_items_with_artifacts(txs).await?;
        Ok((artifacts, profile))
    }

    async fn build_tx_artifact_extracted_profiled(
        tx: Arc<Transaction>,
    ) -> Result<(Arc<TxArtifact>, ArtifactBuildBreakdown)> {
        let (mut artifacts, profile) =
            Self::build_tx_artifacts_extracted_profiled(std::slice::from_ref(&tx)).await?;
        artifacts
            .pop()
            .context("single transaction artifact missing")
            .map(|artifact| (artifact, profile))
    }

    async fn build_tx_artifacts_for_stage(
        stage: &'static str,
        txs: &[Arc<Transaction>],
    ) -> Result<(Vec<Arc<TxArtifact>>, ArtifactBuildBreakdown)> {
        let start = Instant::now();
        let result = Self::build_tx_artifacts_profiled(txs).await;
        Self::record_artifact_build(stage, txs.len(), start.elapsed(), result.is_ok());
        result
    }

    pub async fn build_tx_artifacts_extracted_for_stage_public(
        stage: &'static str,
        txs: &[Arc<Transaction>],
    ) -> Result<Vec<Arc<TxArtifact>>> {
        let start = Instant::now();
        let result = Self::build_tx_artifacts_extracted_profiled(txs).await;
        Self::record_artifact_build(stage, txs.len(), start.elapsed(), result.is_ok());
        let (artifacts, _) = result?;
        Ok(artifacts)
    }

    pub async fn build_tx_artifacts_extracted_profiled_public(
        stage: &'static str,
        txs: &[Arc<Transaction>],
    ) -> Result<(Vec<Arc<TxArtifact>>, ArtifactBuildBreakdown)> {
        let start = Instant::now();
        let result = Self::build_tx_artifacts_extracted_profiled(txs).await;
        Self::record_artifact_build(stage, txs.len(), start.elapsed(), result.is_ok());
        result
    }

    async fn build_tx_artifact_extracted_for_stage(
        stage: &'static str,
        tx: Arc<Transaction>,
    ) -> Result<(Arc<TxArtifact>, ArtifactBuildBreakdown)> {
        let start = Instant::now();
        let result = Self::build_tx_artifact_extracted_profiled(tx).await;
        Self::record_artifact_build(stage, 1, start.elapsed(), result.is_ok());
        result
    }

    async fn verify_tx_artifacts_for_stage(
        stage: &'static str,
        artifacts: &[Arc<TxArtifact>],
    ) -> Result<ArtifactBuildBreakdown> {
        let start = Instant::now();
        let proof_items = Self::merge_artifact_proof_items(artifacts);
        let result = Self::legacy_batch_verify_proof_families(proof_items).await;
        Self::record_artifact_build(stage, artifacts.len(), start.elapsed(), result.is_ok());
        result?;
        Ok(ArtifactBuildBreakdown {
            batch_verify_ms: start.elapsed().as_secs_f64() * 1000.0,
            ..Default::default()
        })
    }

    /// Runs Groth16 batch verification on a single pre-extracted artifact.
    /// Used by `mempool_v1_lab` strict mode to measure per-tx proof verify cost.
    pub async fn batch_verify_tx_artifact_for_bench(artifact: &Arc<TxArtifact>) -> Result<f64> {
        let breakdown = Self::verify_tx_artifacts_for_stage(
            "checktx_strict_bench",
            std::slice::from_ref(artifact),
        )
        .await?;
        Ok(breakdown.batch_verify_ms)
    }

    /// Runs Groth16 batch verification across multiple pre-extracted artifacts in one call.
    /// Amortizes the MSM cost across all proofs in the slice.
    pub async fn batch_verify_artifacts_for_bench(artifacts: &[Arc<TxArtifact>]) -> Result<f64> {
        let breakdown =
            Self::verify_tx_artifacts_for_stage("checktx_strict_bench_batched", artifacts).await?;
        Ok(breakdown.batch_verify_ms)
    }

    async fn legacy_batch_verify_proof_families(
        proof_items: BTreeMap<ProofFamilyId, Vec<BatchItem>>,
    ) -> Result<()> {
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
                let result = Self::verify_batch_family_chunks(family_label, items, move |chunk| {
                    batch::batch_verify(proof_verification_key_for_family(family_id), chunk)
                        .map_err(|e| {
                            anyhow::anyhow!("{family_label} batch verification failed: {e}")
                        })
                })
                .await;
                (batch_verify_stage, result)
            });
        }

        while let Some(join_result) = tasks.join_next().await {
            let (batch_stage, result) =
                join_result.context("legacy batch verification task panicked")?;
            Self::handle_proof_verification_result(batch_stage, result)?;
        }

        Ok(())
    }

    fn batch_verify_chunk_size(item_count: usize) -> usize {
        if item_count <= BATCH_VERIFY_CHUNK_MIN_ITEMS {
            return item_count.max(1);
        }

        let available_parallelism = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1);
        let max_chunks = (available_parallelism / 2)
            .max(1)
            .min(BATCH_VERIFY_MAX_CHUNKS_PER_FAMILY);
        let chunk_count = (item_count / BATCH_VERIFY_CHUNK_MIN_ITEMS)
            .max(1)
            .min(max_chunks);

        item_count.div_ceil(chunk_count)
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
            | ProofFamilyId::Consolidate(_)
            | ProofFamilyId::Split(_)
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
            version: AGGREGATE_BUNDLE_VERSION,
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
                detection_data: None,
                memo: None,
            },
            binding_sig: [0; 64].into(),
            anchor: penumbra_sdk_tct::Root(penumbra_sdk_tct::structure::Hash::zero()),
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

    async fn verify_batch_family_chunks<F>(
        family_label: &'static str,
        items: Vec<BatchItem>,
        verify: F,
    ) -> Result<()>
    where
        F: Fn(&Vec<BatchItem>) -> Result<()> + Send + Sync + Copy + 'static,
    {
        if items.is_empty() {
            return Ok(());
        }

        let chunk_size = Self::batch_verify_chunk_size(items.len());
        if chunk_size >= items.len() {
            return tokio::task::spawn_blocking(move || verify(&items))
                .await
                .with_context(|| format!("{family_label} batch verify task panicked"))?;
        }

        let mut tasks = tokio::task::JoinSet::new();
        for chunk in items.chunks(chunk_size) {
            let chunk = chunk.to_vec();
            tasks.spawn_blocking(move || verify(&chunk));
        }

        while let Some(result) = tasks.join_next().await {
            result.with_context(|| format!("{family_label} batch verify task panicked"))??;
        }

        Ok(())
    }

    async fn build_aggregate_bundle_tx(&self, bundle: AggregateBundle) -> Result<Transaction> {
        let anchor = self.state.get_sct().await.root();
        let chain_id = self.state.get_chain_id().await?;
        let mut tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::AggregateBundle(bundle)],
                transaction_parameters: TransactionParameters {
                    expiry_height: 0,
                    chain_id,
                    fee: Fee::default(),
                },
                fee_funding: None,
                detection_data: None,
                memo: None,
            },
            binding_sig: [0; 64].into(),
            anchor,
        };

        let binding_signing_key = rdsa::SigningKey::from(Fr::zero());
        let auth_hash = tx.transaction_body.auth_hash();
        tx.binding_sig = binding_signing_key.sign_deterministic(auth_hash.as_bytes());

        Ok(tx)
    }

    async fn build_family_aggregates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        segment_index: usize,
    ) -> Result<(Vec<FamilyAggregate>, AggregateBuildProfile)> {
        let merge_start = Instant::now();
        let proof_items = Self::merge_artifact_proof_items(artifacts);
        if Self::total_artifact_proof_count(artifacts) == 0 {
            return Ok((Vec::new(), AggregateBuildProfile::default()));
        }

        let mut profile = AggregateBuildProfile::default();
        profile.merge_items_ms = merge_start.elapsed().as_secs_f64() * 1000.0;
        let srs_start = Instant::now();
        let srs = DevSrs::default();
        profile.setup_ms = srs_start.elapsed().as_secs_f64() * 1000.0;
        let mut aggregate_tasks = Vec::new();
        let debug_entries = Self::aggregate_debug_segment_families(artifacts, None);

        for family_id in Self::proof_family_ids() {
            let items = proof_items.get(&family_id).cloned().unwrap_or_default();
            if items.is_empty() {
                continue;
            }

            let real_count = items.len() as u32;
            let padding_start = Instant::now();
            let padded_items = pad_items_to_power_of_two(&items, MAX_PADDED_PROOF_COUNT)?;
            profile.padding_ms += padding_start.elapsed().as_secs_f64() * 1000.0;
            let padded_count = padded_items.len() as u32;
            let srs_for_task = srs;
            let padded_public_inputs = padded_items
                .iter()
                .map(|item| item.public_inputs.clone())
                .collect::<Vec<_>>();
            let debug_entry = debug_entries
                .iter()
                .find(|entry| entry.family_id == family_id)
                .cloned()
                .unwrap_or(AggregateDebugSegmentFamily {
                    segment_index: 0,
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
                move || -> Result<(FamilyAggregate, AggregateBuildProfile, f64)> {
                    let family_start = Instant::now();
                    let (aggregate_proof, backend_profile) = aggregate_family_profiled(
                        family_id,
                        proof_verification_key_for_family(family_id),
                        &padded_items,
                        &srs_for_task,
                    )?;

                    let mut family_profile = AggregateBuildProfile::default();
                    family_profile.apply_backend_build_profile(&backend_profile);

                    Ok((
                        FamilyAggregate {
                            family_id,
                            real_count,
                            padded_count,
                            aggregate_proof,
                        },
                        family_profile,
                        family_start.elapsed().as_secs_f64() * 1000.0,
                    ))
                },
            ));
        }

        let mut families = Vec::new();
        for task in aggregate_tasks {
            let (family, family_profile, family_elapsed_ms) =
                task.await.context("aggregate family task panicked")??;
            profile.add_family_time(family.family_id, family_elapsed_ms);
            profile.merge(&family_profile);
            families.push(family);
        }

        Ok((families, profile))
    }

    async fn build_segmented_family_aggregates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_count: usize,
    ) -> Result<(Vec<FamilyAggregate>, Vec<usize>, AggregateBuildProfile)> {
        if artifacts.is_empty() {
            return Ok((Vec::new(), Vec::new(), AggregateBuildProfile::default()));
        }

        let mut families = Vec::new();
        let mut segment_tx_counts = Vec::new();
        let mut profile = AggregateBuildProfile::default();

        for (segment_index, artifact_segment) in artifacts.chunks(segment_tx_count).enumerate() {
            let (segment_families, segment_profile) =
                Self::build_family_aggregates_for_artifacts(artifact_segment, segment_index)
                    .await?;
            if !artifact_segment.is_empty() {
                segment_tx_counts.push(artifact_segment.len());
            }
            profile.merge(&segment_profile);
            families.extend(segment_families);
        }

        Ok((families, segment_tx_counts, profile))
    }

    async fn build_exact_segmented_family_aggregates_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_counts: &[usize],
    ) -> Result<(Vec<FamilyAggregate>, Vec<usize>, AggregateBuildProfile)> {
        if artifacts.is_empty() {
            anyhow::ensure!(
                segment_tx_counts.is_empty(),
                "empty artifacts must not provide segment counts"
            );
            return Ok((Vec::new(), Vec::new(), AggregateBuildProfile::default()));
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
        let mut profile = AggregateBuildProfile::default();
        let mut start = 0usize;
        let mut segment_tasks = tokio::task::JoinSet::new();
        let mut ordered_segment_results = vec![None; segment_tx_counts.len()];

        for (segment_index, &segment_tx_count) in segment_tx_counts.iter().enumerate() {
            anyhow::ensure!(segment_tx_count > 0, "segment_tx_counts must be positive");
            let end = start + segment_tx_count;
            let artifact_segment = artifacts[start..end].to_vec();
            segment_tasks.spawn(async move {
                let (segment_families, segment_profile) =
                    Self::build_family_aggregates_for_artifacts(&artifact_segment, segment_index)
                        .await?;
                Ok::<_, anyhow::Error>((segment_index, segment_families, segment_profile))
            });
            start = end;
        }

        while let Some(result) = segment_tasks.join_next().await {
            let (segment_index, segment_families, segment_profile) =
                result.context("aggregate segment task panicked")??;
            ordered_segment_results[segment_index] = Some((segment_families, segment_profile));
        }

        for segment_result in ordered_segment_results {
            let (segment_families, segment_profile) =
                segment_result.context("missing aggregate segment result")?;
            profile.merge(&segment_profile);
            families.extend(segment_families);
        }

        Ok((families, segment_tx_counts.to_vec(), profile))
    }

    async fn build_aggregate_bundle_from_families(
        &self,
        families: Vec<FamilyAggregate>,
    ) -> Result<(Option<Bytes>, f64, f64)> {
        if families.is_empty() {
            return Ok((None, 0.0, 0.0));
        }

        let srs_start = Instant::now();
        let srs = DevSrs::default();
        let setup_ms = srs_start.elapsed().as_secs_f64() * 1000.0;
        let tx_build_start = Instant::now();
        let bundle_tx = self
            .build_aggregate_bundle_tx(AggregateBundle {
                version: AGGREGATE_BUNDLE_VERSION,
                srs_id: srs_id(&srs).to_vec(),
                families,
            })
            .await?;
        let tx_build_ms = tx_build_start.elapsed().as_secs_f64() * 1000.0;

        Ok((
            Some(Bytes::from(bundle_tx.encode_to_vec())),
            setup_ms,
            tx_build_ms,
        ))
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
            tx.transaction_body.detection_data.is_none(),
            "aggregate bundle tx must not contain detection data"
        );
        anyhow::ensure!(
            tx.transaction_body.transaction_parameters.fee == Fee::default(),
            "aggregate bundle tx must have zero fee"
        );
        valid_binding_signature(tx)?;
        tx.aggregate_bundle_action()
            .context("aggregate bundle tx missing bundle action")
    }

    fn expected_aggregate_segments(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_counts: Option<&[usize]>,
    ) -> Vec<(ProofFamilyId, Vec<BatchItem>)> {
        let mut expected_segments = Vec::new();

        let artifact_groups: Vec<&[Arc<TxArtifact>]> = match segment_tx_counts {
            Some(segment_tx_counts) if !segment_tx_counts.is_empty() => {
                let mut groups = Vec::with_capacity(segment_tx_counts.len());
                let mut start = 0usize;
                for &segment_tx_count in segment_tx_counts {
                    let end = start + segment_tx_count;
                    groups.push(&artifacts[start..end]);
                    start = end;
                }
                groups
            }
            _ => vec![artifacts],
        };

        for artifact_group in artifact_groups {
            let proof_items = Self::merge_artifact_proof_items(artifact_group);
            for family_id in Self::proof_family_ids() {
                let items = proof_items.get(&family_id).cloned().unwrap_or_default();
                if items.is_empty() {
                    continue;
                }
                expected_segments.push((family_id, items));
            }
        }

        expected_segments
    }

    async fn verify_aggregate_bundle_for_artifacts(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> Result<()> {
        match Self::verify_aggregate_bundle_for_artifacts_raw(artifacts, bundle, segment_tx_counts)
            .await
        {
            Ok(()) => Ok(()),
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
    ) -> Result<()> {
        let (_profile, result) = Self::verify_aggregate_bundle_for_artifacts_raw_profiled(
            artifacts,
            bundle,
            segment_tx_counts,
        )
        .await;
        result
    }

    async fn verify_aggregate_bundle_for_artifacts_raw_profiled(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> (AggregateVerifyProfile, Result<()>) {
        let verify_start = Instant::now();
        let mut profile = AggregateVerifyProfile::default();
        let result: Result<()> = async {
            anyhow::ensure!(
                bundle.version == AGGREGATE_BUNDLE_VERSION,
                "unsupported aggregate bundle version {}",
                bundle.version
            );

            let total_proofs = Self::total_artifact_proof_count(artifacts);
            anyhow::ensure!(
                total_proofs > 0,
                "aggregate bundle requires at least one proof"
            );

            let srs = DevSrs::default();
            anyhow::ensure!(
                bundle.srs_id == srs_id(&srs).to_vec(),
                "aggregate bundle SRS id mismatch"
            );

            if let Some(segment_tx_counts) = segment_tx_counts {
                anyhow::ensure!(
                    segment_tx_counts.iter().sum::<usize>() == artifacts.len(),
                    "aggregate segment coverage mismatch: expected {}, got {}",
                    artifacts.len(),
                    segment_tx_counts.iter().sum::<usize>()
                );
            }

            let expected_segments_start = Instant::now();
            let expected_segments = Self::expected_aggregate_segments(artifacts, segment_tx_counts);
            let debug_segments =
                Self::aggregate_debug_segment_families(artifacts, segment_tx_counts);
            profile.expected_segments_ms = expected_segments_start.elapsed().as_secs_f64() * 1000.0;

            anyhow::ensure!(
                bundle.families.len() == expected_segments.len(),
                "aggregate bundle family count mismatch: expected {}, got {}",
                expected_segments.len(),
                bundle.families.len()
            );

            let mut verify_tasks = Vec::new();
            for (segment_order_index, (family, expected_items)) in
                expected_segments.into_iter().enumerate()
            {
                let aggregate = bundle
                    .families
                    .get(verify_tasks.len())
                    .cloned()
                    .context("missing aggregate family")?;

                anyhow::ensure!(
                    aggregate.family_id == family,
                    "aggregate family ordering mismatch: expected {:?}, got {:?}",
                    family,
                    aggregate.family_id
                );
                anyhow::ensure!(
                    aggregate.real_count == expected_items.len() as u32,
                    "aggregate real_count mismatch for {:?}: expected {}, got {}",
                    family,
                    expected_items.len(),
                    aggregate.real_count
                );

                let prepare_inputs_start = Instant::now();
                let prepared_inputs =
                    prepare_verify_inputs(&expected_items, MAX_PADDED_PROOF_COUNT)?;
                profile.prepare_inputs_ms += prepare_inputs_start.elapsed().as_secs_f64() * 1000.0;
                anyhow::ensure!(
                    aggregate.padded_count == prepared_inputs.padded_count as u32,
                    "aggregate padded_count mismatch for {:?}: expected {}, got {}",
                    family,
                    prepared_inputs.padded_count,
                    aggregate.padded_count
                );

                let debug_entry = debug_segments
                    .get(segment_order_index)
                    .cloned()
                    .context("missing aggregate debug segment")?;
                let padded_public_inputs = prepared_inputs.padded_public_inputs;
                maybe_write_aggregate_debug_dump(
                    "verify",
                    debug_entry.segment_index,
                    debug_entry.family_index,
                    family,
                    &debug_entry.rows,
                    &padded_public_inputs,
                    Some(&aggregate),
                );
                let aggregate_proof = aggregate.aggregate_proof;
                let srs_for_task = srs;
                let debug_segment_index = debug_entry.segment_index;
                let debug_family_index = debug_entry.family_index;

                verify_tasks.push(tokio::task::spawn_blocking(
                    move || -> Result<(usize, usize, ProofFamilyId, AggregateVerificationProfile)> {
                        let backend_profile = verify_family_aggregate_profiled_unchecked(
                            family,
                            proof_verification_key_for_family(family),
                            &aggregate_proof,
                            &padded_public_inputs,
                            &srs_for_task,
                        )?;

                        Ok((
                            debug_segment_index,
                            debug_family_index,
                            family,
                            backend_profile,
                        ))
                    },
                ));
            }

            let mut aggregate_verify_failed = false;
            let mut rejected_families = Vec::new();
            for task in verify_tasks {
                let (segment_index, family_index, family_id, backend_profile) = task
                    .await
                    .context("aggregate verification task panicked")??;
                if !backend_profile.accepted {
                    aggregate_verify_failed = true;
                    rejected_families.push((segment_index, family_index, family_id));
                }
                profile.merge_backend_profile(&backend_profile);
            }

            if aggregate_verify_failed {
                let details = rejected_families
                    .into_iter()
                    .map(|(segment_index, family_index, family_id)| {
                        format!(
                            "segment={} family_index={} family={:?}",
                            segment_index, family_index, family_id
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!("SnarkPack verification rejected aggregate bundle ({details})");
            }

            Ok(())
        }
        .await;

        profile.total_ms = verify_start.elapsed().as_secs_f64() * 1000.0;

        (profile, result)
    }

    pub async fn verify_aggregate_bundle_for_artifacts_public(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> Result<()> {
        Self::verify_aggregate_bundle_for_artifacts(artifacts, bundle, segment_tx_counts).await
    }

    pub async fn verify_aggregate_bundle_for_artifacts_raw_public(
        artifacts: &[Arc<TxArtifact>],
        bundle: &AggregateBundle,
        segment_tx_counts: Option<&[usize]>,
    ) -> Result<()> {
        Self::verify_aggregate_bundle_for_artifacts_raw(artifacts, bundle, segment_tx_counts).await
    }

    pub async fn build_aggregate_bundle_tx_for_snapshot_public(
        snapshot: Snapshot,
        bundle: AggregateBundle,
    ) -> Result<Transaction> {
        Self::new(snapshot).build_aggregate_bundle_tx(bundle).await
    }

    pub async fn validate_candidate_envelope_profiled(
        &self,
        envelope: &CandidateEnvelope,
        nullifier_cache: Option<&ValidationNullifierCache>,
    ) -> Result<(ValidationVerdict, ValidationProfile)> {
        use penumbra_sdk_proto::core::transaction::v1::action::Action as ProtoAction;
        use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;
        use penumbra_sdk_proto::DomainType as _;
        use penumbra_sdk_transaction::Transaction;

        let mut verdict = ValidationVerdict::default();
        let mut profile = ValidationProfile {
            block_tx_count: envelope.block_tx_count,
            total_payload_bytes: envelope.total_payload_bytes,
            ..ValidationProfile::default()
        };

        let reject_shape = |verdict: &mut ValidationVerdict, reason: ValidationRejectReason| {
            verdict.shape = ValidationStageVerdict {
                ok: false,
                reject_reason: Some(reason),
            };
            verdict.reject_reason = Some(reason);
            verdict.final_accept = false;
        };
        let reject_stateful = |verdict: &mut ValidationVerdict, reason: ValidationRejectReason| {
            verdict.shape.ok = true;
            verdict.stateful = ValidationStageVerdict {
                ok: false,
                reject_reason: Some(reason),
            };
            verdict.reject_reason = Some(reason);
            verdict.final_accept = false;
        };
        let reject_aggregate = |verdict: &mut ValidationVerdict, reason: ValidationRejectReason| {
            verdict.shape.ok = true;
            verdict.stateful.ok = true;
            verdict.aggregate = ValidationStageVerdict {
                ok: false,
                reject_reason: Some(reason),
            };
            verdict.reject_reason = Some(reason);
            verdict.final_accept = false;
        };

        let shape_start = Instant::now();
        let computed_payload_bytes = envelope.txs.iter().map(Vec::len).sum::<usize>();
        if envelope.txs.len() > MAX_VALIDATION_TX_COUNT
            || envelope.block_tx_count > MAX_VALIDATION_TX_COUNT
        {
            reject_shape(&mut verdict, ValidationRejectReason::TxCountExceeded);
            profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }
        if computed_payload_bytes != envelope.total_payload_bytes {
            reject_shape(&mut verdict, ValidationRejectReason::PayloadBytesExceeded);
            profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }

        let mut proto_txs = Vec::with_capacity(envelope.txs.len());
        let mut block_spend_nullifier_count = 0usize;
        for tx_bytes in &envelope.txs {
            let proto_tx = match ProtoTransaction::decode(tx_bytes.as_slice()) {
                Ok(tx) => tx,
                Err(_) => {
                    reject_shape(&mut verdict, ValidationRejectReason::TxDecodeFailed);
                    profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((verdict, profile));
                }
            };
            let action_count = proto_tx
                .body
                .as_ref()
                .map(|body| body.actions.len())
                .unwrap_or_default();
            if action_count > MAX_VALIDATION_ACTIONS_PER_TX {
                reject_shape(&mut verdict, ValidationRejectReason::ActionCountExceeded);
                profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;
                return Ok((verdict, profile));
            }

            let tx_spend_nullifier_count: usize = proto_tx
                .body
                .as_ref()
                .map(|body| {
                    body.actions
                        .iter()
                        .map(|action| match &action.action {
                            Some(ProtoAction::Transfer(transfer)) => transfer
                                .body
                                .as_ref()
                                .map(|body| body.inputs.len())
                                .unwrap_or_default(),
                            Some(ProtoAction::Consolidate(consolidate)) => consolidate
                                .body
                                .as_ref()
                                .map(|body| body.inputs.len())
                                .unwrap_or_default(),
                            Some(ProtoAction::Split(split)) => split
                                .body
                                .as_ref()
                                .map(|body| body.inputs.len())
                                .unwrap_or_default(),
                            Some(ProtoAction::ShieldedIcs20Withdrawal(withdrawal)) => withdrawal
                                .body
                                .as_ref()
                                .map(|body| body.inputs.len())
                                .unwrap_or_default(),
                            _ => 0,
                        })
                        .sum()
                })
                .unwrap_or_default();
            if tx_spend_nullifier_count > MAX_VALIDATION_NULLIFIERS_PER_TX {
                reject_shape(
                    &mut verdict,
                    ValidationRejectReason::NullifierCountExceededPerTx,
                );
                profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;
                return Ok((verdict, profile));
            }
            block_spend_nullifier_count += tx_spend_nullifier_count;
            if block_spend_nullifier_count > MAX_VALIDATION_NULLIFIERS_PER_BLOCK {
                reject_shape(
                    &mut verdict,
                    ValidationRejectReason::NullifierCountExceededPerBlock,
                );
                profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;
                return Ok((verdict, profile));
            }
            proto_txs.push(proto_tx);
        }
        profile.shape_check_ms = shape_start.elapsed().as_secs_f64() * 1000.0;

        let sidecar_start = Instant::now();
        if envelope.block_tx_count != envelope.txs.len()
            || envelope.tx_hashes.len() != envelope.txs.len()
        {
            reject_shape(&mut verdict, ValidationRejectReason::TxHashCountMismatch);
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }
        if envelope.segment_tx_counts.iter().sum::<usize>() != envelope.txs.len()
            || envelope.sidecar.segment_tx_counts != envelope.segment_tx_counts
        {
            reject_shape(
                &mut verdict,
                ValidationRejectReason::SegmentCoverageMismatch,
            );
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }

        let computed_hashes = envelope
            .txs
            .iter()
            .map(|tx_bytes| sha2::Sha256::digest(tx_bytes.as_slice()).into())
            .collect::<Vec<[u8; 32]>>();
        if computed_hashes != envelope.tx_hashes {
            reject_shape(&mut verdict, ValidationRejectReason::TxHashMismatch);
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }
        if candidate_digest_from_hashes(&computed_hashes) != envelope.candidate_digest {
            reject_shape(
                &mut verdict,
                ValidationRejectReason::CandidateDigestMismatch,
            );
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }
        if envelope.sidecar.entries.len() != computed_hashes.len() {
            reject_shape(&mut verdict, ValidationRejectReason::SidecarTxCountMismatch);
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }
        if sidecar_commitment(&envelope.sidecar, &computed_hashes) != envelope.sidecar.commitment {
            reject_shape(
                &mut verdict,
                ValidationRejectReason::SidecarCommitmentMismatch,
            );
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }

        let mut sorted_hashes = computed_hashes.clone();
        sorted_hashes.sort_unstable();
        let sidecar_hashes = envelope
            .sidecar
            .entries
            .iter()
            .map(|entry| entry.tx_hash)
            .collect::<Vec<_>>();
        if sidecar_hashes != sorted_hashes {
            reject_shape(
                &mut verdict,
                ValidationRejectReason::SidecarEntrySetMismatch,
            );
            profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }

        let bundle = match envelope.aggregate_bundle() {
            Ok(Some(bundle)) => bundle,
            Ok(None) => {
                reject_shape(&mut verdict, ValidationRejectReason::BundleMissing);
                profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
                return Ok((verdict, profile));
            }
            Err(_) => {
                reject_shape(&mut verdict, ValidationRejectReason::BundleTxShapeInvalid);
                profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
                return Ok((verdict, profile));
            }
        };
        profile.sidecar_check_ms = sidecar_start.elapsed().as_secs_f64() * 1000.0;
        verdict.shape.ok = true;

        let cache_lookup_start = Instant::now();
        let mut tx_nullifiers = Vec::with_capacity(computed_hashes.len());
        for (tx_hash, proto_tx) in computed_hashes.iter().zip(proto_txs.iter()) {
            if let Some(cache) = nullifier_cache {
                if let Some(cached) = cache.get(tx_hash) {
                    profile.cache_hit_count += 1;
                    tx_nullifiers.push((*cached).clone());
                    continue;
                }
            }
            profile.cache_miss_count += 1;
            tx_nullifiers.push(Self::extract_spend_nullifiers_from_proto(proto_tx)?);
        }
        profile.nullifier_cache_lookup_ms = cache_lookup_start.elapsed().as_secs_f64() * 1000.0;

        let extract_start = Instant::now();
        profile.total_spend_nullifiers = tx_nullifiers.iter().map(Vec::len).sum();
        profile.nullifier_extract_ms = extract_start.elapsed().as_secs_f64() * 1000.0;

        let stateful_start = Instant::now();
        let mut seen_nullifiers = std::collections::BTreeSet::new();
        for spend_nullifiers in &tx_nullifiers {
            for nullifier in spend_nullifiers {
                if !seen_nullifiers.insert(*nullifier) {
                    reject_stateful(
                        &mut verdict,
                        ValidationRejectReason::DuplicateSpendNullifier,
                    );
                    profile.stateful_conflict_check_ms =
                        stateful_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((verdict, profile));
                }
                if self.state.spend_info(*nullifier).await?.is_some() {
                    reject_stateful(
                        &mut verdict,
                        ValidationRejectReason::CommittedNullifierConflict,
                    );
                    profile.stateful_conflict_check_ms =
                        stateful_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((verdict, profile));
                }
            }
        }
        profile.stateful_conflict_check_ms = stateful_start.elapsed().as_secs_f64() * 1000.0;
        verdict.stateful.ok = true;

        let aggregate_start = Instant::now();
        let sidecar = ProposalArtifactSidecar::from_record(envelope.sidecar.clone());
        let sidecar_entries = envelope
            .sidecar
            .entries
            .iter()
            .map(|entry| (entry.tx_hash, entry.encoded_entry.as_slice()))
            .collect::<BTreeMap<_, _>>();
        let mut artifacts = Vec::with_capacity(envelope.txs.len());
        for (tx_hash, tx_bytes) in computed_hashes.iter().zip(envelope.txs.iter()) {
            let artifact_cache_lookup_start = Instant::now();
            if let Some(cache) = nullifier_cache {
                if let Some(artifact) = cache.get_artifact(tx_hash) {
                    profile.aggregate_artifact_cache_hit_count += 1;
                    profile.aggregate_artifact_cache_lookup_ms +=
                        artifact_cache_lookup_start.elapsed().as_secs_f64() * 1000.0;
                    artifacts.push(artifact);
                    continue;
                }
            }
            profile.aggregate_artifact_cache_miss_count += 1;
            profile.aggregate_artifact_cache_lookup_ms +=
                artifact_cache_lookup_start.elapsed().as_secs_f64() * 1000.0;

            let tx_decode_start = Instant::now();
            let tx = match Transaction::decode(tx_bytes.as_slice()) {
                Ok(tx) => Arc::new(tx),
                Err(_) => {
                    reject_aggregate(&mut verdict, ValidationRejectReason::TxDecodeFailed);
                    profile.aggregate_tx_decode_ms +=
                        tx_decode_start.elapsed().as_secs_f64() * 1000.0;
                    profile.aggregate_verify_ms = aggregate_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((verdict, profile));
                }
            };
            profile.aggregate_tx_decode_ms += tx_decode_start.elapsed().as_secs_f64() * 1000.0;
            let Some(encoded_entry) = sidecar_entries.get(tx_hash) else {
                reject_aggregate(
                    &mut verdict,
                    ValidationRejectReason::SidecarEntrySetMismatch,
                );
                profile.aggregate_verify_ms = aggregate_start.elapsed().as_secs_f64() * 1000.0;
                return Ok((verdict, profile));
            };
            let sidecar_decode_start = Instant::now();
            let artifact = match sidecar.decode_artifact(*tx_hash, tx, encoded_entry) {
                Ok(artifact) => artifact,
                Err(_) => {
                    reject_aggregate(&mut verdict, ValidationRejectReason::SidecarDecodeFailed);
                    profile.aggregate_sidecar_decode_ms +=
                        sidecar_decode_start.elapsed().as_secs_f64() * 1000.0;
                    profile.aggregate_verify_ms = aggregate_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((verdict, profile));
                }
            };
            profile.aggregate_sidecar_decode_ms +=
                sidecar_decode_start.elapsed().as_secs_f64() * 1000.0;
            artifacts.push(artifact);
        }
        if Self::total_artifact_proof_count(&artifacts) == 0 {
            reject_aggregate(
                &mut verdict,
                ValidationRejectReason::BundlePresentWithZeroProofs,
            );
            profile.aggregate_verify_ms = aggregate_start.elapsed().as_secs_f64() * 1000.0;
            return Ok((verdict, profile));
        }

        let (aggregate_profile, aggregate_result) =
            Self::verify_aggregate_bundle_for_artifacts_raw_profiled(
                &artifacts,
                &bundle,
                Some(&envelope.segment_tx_counts),
            )
            .await;
        profile.aggregate_verify_ms = aggregate_start.elapsed().as_secs_f64() * 1000.0;
        profile.aggregate_expected_segments_ms = aggregate_profile.expected_segments_ms;
        profile.aggregate_prepare_inputs_ms = aggregate_profile.prepare_inputs_ms;
        profile.aggregate_verify_kernel_ms = (aggregate_profile.total_ms
            - aggregate_profile.expected_segments_ms
            - aggregate_profile.prepare_inputs_ms)
            .max(0.0);
        profile.aggregate_backend_deserialize_ms = aggregate_profile.backend_deserialize_ms;
        profile.aggregate_backend_challenge_ms = aggregate_profile.backend_challenge_ms;
        profile.aggregate_backend_tipa_ab_ms = aggregate_profile.backend_tipa_ab_ms;
        profile.aggregate_backend_tipa_c_ms = aggregate_profile.backend_tipa_c_ms;
        profile.aggregate_backend_public_input_fold_ms =
            aggregate_profile.backend_public_input_fold_ms;
        profile.aggregate_backend_ppe_ms = aggregate_profile.backend_ppe_ms;
        profile.aggregate_backend_core_total_ms = aggregate_profile.backend_core_total_ms;
        match aggregate_result {
            Ok(()) => {
                verdict.aggregate.ok = true;
                verdict.final_accept = true;
                Ok((verdict, profile))
            }
            Err(_) => {
                reject_aggregate(&mut verdict, ValidationRejectReason::AggregateVerifyFailed);
                Ok((verdict, profile))
            }
        }
    }

    pub async fn process_candidate_envelope_profiled(
        &mut self,
        envelope: &CandidateEnvelope,
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<(response::ProcessProposal, ProcessProposalProfile)> {
        let context = self.benchmark_block_context().await?;
        let proposal = Self::process_proposal_request_from_envelope(&context, envelope);
        let sidecar = ProposalArtifactSidecar::from_record(envelope.sidecar.clone());

        Ok(self
            .process_proposal_v2_profiled(proposal, stateless_cache, Some(&sidecar), false)
            .await)
    }

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
                Transaction::decode(tx_bytes.as_slice())
                    .map(Arc::new)
                    .with_context(|| format!("decoding execution benchmark tx ordinal {index}"))
            })
            .collect::<Result<Vec<_>>>()?;

        let deliver_txs_start = Instant::now();
        for tx in decoded_txs {
            let execute_tx_start = Instant::now();
            let (_events, execute_profile) =
                self.execute_tx_checked_historical_profiled(tx).await?;
            profile.execute_tx_ms += execute_tx_start.elapsed().as_secs_f64() * 1000.0;
            profile.begin_state_tx_ms += execute_profile.begin_state_tx_ms;
            profile.index_tx_ms += execute_profile.index_tx_ms;
            profile.get_block_height_ms += execute_profile.get_block_height_ms;
            profile.clone_tx_ms += execute_profile.clone_tx_ms;
            profile.proto_convert_ms += execute_profile.proto_convert_ms;
            profile.put_block_transaction_ms += execute_profile.put_block_transaction_ms;
            profile.tx_log_read_ms += execute_profile.tx_log_read_ms;
            profile.tx_log_encode_ms += execute_profile.tx_log_encode_ms;
            profile.tx_log_put_raw_ms += execute_profile.tx_log_put_raw_ms;
            profile.check_and_execute_ms += execute_profile.check_and_execute_ms;
            profile.set_source_ms += execute_profile.set_source_ms;
            profile.pay_fee_ms += execute_profile.pay_fee_ms;
            profile.action_execute_ms += execute_profile.action_execute_ms;
            profile.read_local_precheck_ms += execute_profile.read_local_precheck_ms;
            profile.read_lookup_wait_or_join_ms += execute_profile.read_lookup_wait_or_join_ms;
            profile.read_historical_check_ms += execute_profile.read_historical_check_ms;
            profile.read_nullifier_wait_ms += execute_profile.read_nullifier_wait_ms;
            profile.read_anchor_cache_wait_ms += execute_profile.read_anchor_cache_wait_ms;
            profile.spend_action_execute_ms += execute_profile.spend_action_execute_ms;
            profile.spend_nullifier_check_ms += execute_profile.spend_nullifier_check_ms;
            profile.spend_nullifier_tx_local_scan_ms +=
                execute_profile.spend_nullifier_tx_local_scan_ms;
            profile.spend_nullifier_block_log_lookup_ms +=
                execute_profile.spend_nullifier_block_log_lookup_ms;
            profile.spend_nullifier_committed_check_ms +=
                execute_profile.spend_nullifier_committed_check_ms;
            profile.spend_nullifier_enqueue_ms += execute_profile.spend_nullifier_enqueue_ms;
            profile.spend_nullifier_stage_ms += execute_profile.spend_nullifier_stage_ms;
            profile.spend_nullifier_merge_ms += execute_profile.spend_nullifier_merge_ms;
            profile.nullifier_lookup_count += execute_profile.nullifier_lookup_count;
            profile.output_action_execute_ms += execute_profile.output_action_execute_ms;
            profile.output_add_note_payload_ms += execute_profile.output_add_note_payload_ms;
            profile.other_action_execute_ms += execute_profile.other_action_execute_ms;
            profile.record_clues_ms += execute_profile.record_clues_ms;
            profile.apply_ms += execute_profile.apply_ms;
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

    fn extract_spend_nullifiers_from_proto(
        proto_tx: &penumbra_sdk_proto::core::transaction::v1::Transaction,
    ) -> Result<Vec<Nullifier>> {
        use penumbra_sdk_proto::core::transaction::v1::action::Action as ProtoAction;

        fn push_nullifiers<'a, I>(
            out: &mut Vec<Nullifier>,
            nullifiers: I,
            label: &'static str,
        ) -> Result<()>
        where
            I: IntoIterator<
                Item = &'a penumbra_sdk_proto::core::component::sct::v1::Nullifier,
            >,
        {
            for n in nullifiers {
                out.push(Nullifier::try_from(n.clone()).context(label)?);
            }
            Ok(())
        }

        let mut spend_nullifiers = Vec::new();
        let actions = proto_tx
            .body
            .as_ref()
            .into_iter()
            .flat_map(|body| body.actions.iter());
        for action in actions {
            match &action.action {
                Some(ProtoAction::Transfer(t)) => {
                    push_nullifiers(
                        &mut spend_nullifiers,
                        t.body.iter().flat_map(|b| {
                            b.inputs.iter().filter_map(|i| i.nullifier.as_ref())
                        }),
                        "converting proto transfer nullifier",
                    )?;
                }
                Some(ProtoAction::Consolidate(c)) => {
                    push_nullifiers(
                        &mut spend_nullifiers,
                        c.body.iter().flat_map(|b| {
                            b.inputs.iter().filter_map(|i| i.nullifier.as_ref())
                        }),
                        "converting proto consolidate nullifier",
                    )?;
                }
                Some(ProtoAction::Split(s)) => {
                    push_nullifiers(
                        &mut spend_nullifiers,
                        s.body.iter().flat_map(|b| {
                            b.inputs.iter().filter_map(|i| i.nullifier.as_ref())
                        }),
                        "converting proto split nullifier",
                    )?;
                }
                Some(ProtoAction::ShieldedIcs20Withdrawal(w)) => {
                    push_nullifiers(
                        &mut spend_nullifiers,
                        w.body.iter().flat_map(|b| {
                            b.inputs.iter().filter_map(|i| i.nullifier.as_ref())
                        }),
                        "converting proto shielded ICS-20 withdrawal nullifier",
                    )?;
                }
                _ => {}
            }
        }
        Ok(spend_nullifiers)
    }

    pub async fn build_segmented_aggregate_bundle_for_artifacts_public(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_count: usize,
    ) -> Result<(AggregateBundle, Vec<usize>)> {
        let (families, segment_tx_counts, _) =
            Self::build_segmented_family_aggregates_for_artifacts(artifacts, segment_tx_count)
                .await?;
        Ok((
            AggregateBundle {
                version: AGGREGATE_BUNDLE_VERSION,
                srs_id: srs_id(&DevSrs::default()).to_vec(),
                families,
            },
            segment_tx_counts,
        ))
    }

    pub async fn build_segmented_aggregate_bundle_for_artifacts_profiled_public(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_count: usize,
    ) -> Result<(AggregateBundle, Vec<usize>, AggregateBuildProfile)> {
        let (families, segment_tx_counts, profile) =
            Self::build_segmented_family_aggregates_for_artifacts(artifacts, segment_tx_count)
                .await?;
        Ok((
            AggregateBundle {
                version: AGGREGATE_BUNDLE_VERSION,
                srs_id: srs_id(&DevSrs::default()).to_vec(),
                families,
            },
            segment_tx_counts,
            profile,
        ))
    }

    pub async fn build_exact_segmented_aggregate_bundle_for_artifacts_profiled_public(
        artifacts: &[Arc<TxArtifact>],
        segment_tx_counts: &[usize],
    ) -> Result<(AggregateBundle, Vec<usize>, AggregateBuildProfile)> {
        let (families, segment_tx_counts, profile) =
            Self::build_exact_segmented_family_aggregates_for_artifacts(
                artifacts,
                segment_tx_counts,
            )
            .await?;
        Ok((
            AggregateBundle {
                version: AGGREGATE_BUNDLE_VERSION,
                srs_id: srs_id(&DevSrs::default()).to_vec(),
                families,
            },
            segment_tx_counts,
            profile,
        ))
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

    async fn prepare_proposal_batched_profiled(
        &mut self,
        proposal_height: u64,
        txs: Vec<Bytes>,
        max_proposal_size_bytes: u64,
        stateless_cache: Option<&StatelessCache>,
        allow_oversized_proposal: bool,
    ) -> Result<(
        Vec<Bytes>,
        PrepareProposalProfile,
        Option<ProposalArtifactSidecar>,
    )> {
        let mut candidates = Vec::new();
        let mut proposal_size_bytes = 0u64;
        let mut profile = PrepareProposalProfile::default();

        let candidate_scan_start = Instant::now();
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
                match cache.get(&hash) {
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

            let tx = match Transaction::decode(tx_bytes.as_ref()) {
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
        profile.candidate_scan_ms = candidate_scan_start.elapsed().as_secs_f64() * 1000.0;

        if candidates.is_empty() {
            return Ok((Vec::new(), profile, None));
        }

        // Fast precheck: reject duplicate spends before heavier verification.
        let nullifier_dedup_start = Instant::now();
        let mut seen_nullifiers = HashSet::new();
        let mut deduped = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let mut tx_nullifiers = HashSet::new();
            let mut duplicate = false;

            for nullifier in candidate.tx().spent_nullifiers() {
                if !tx_nullifiers.insert(nullifier) || seen_nullifiers.contains(&nullifier) {
                    duplicate = true;
                    break;
                }
            }

            if duplicate {
                continue;
            }

            seen_nullifiers.extend(tx_nullifiers);
            deduped.push(candidate);
        }
        profile.nullifier_dedup_ms = nullifier_dedup_start.elapsed().as_secs_f64() * 1000.0;

        let deduped_txs: Vec<Arc<Transaction>> = deduped.iter().map(|c| c.tx().clone()).collect();
        let anchor_precheck_start = Instant::now();
        self.precheck_compliance_anchors_dedup(&deduped_txs).await?;
        profile.anchor_precheck_ms = anchor_precheck_start.elapsed().as_secs_f64() * 1000.0;

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
                    Some((candidate.hash, artifact.clone()))
                }
                CandidateData::VerifiedArtifact(_) | CandidateData::Decoded(_) => None,
            })
            .collect::<Vec<_>>();

        if !cache_miss_txs.is_empty() {
            let artifact_fill_start = Instant::now();
            let (miss_artifacts, artifact_profile) =
                Self::build_tx_artifacts_for_stage("prepare_proposal", &cache_miss_txs).await?;
            let mut miss_artifacts = miss_artifacts.into_iter();
            profile.artifact_fill_precheck_ms = artifact_profile.precheck_ms;
            profile.artifact_fill_action_extract_ms = artifact_profile.action_extract_ms;
            profile.artifact_fill_batch_verify_ms = artifact_profile.batch_verify_ms;

            for candidate in &mut deduped {
                if matches!(candidate.data, CandidateData::Decoded(_)) {
                    let artifact = miss_artifacts
                        .next()
                        .expect("artifact count should match decoded candidates");
                    if let Some(cache) = stateless_cache {
                        cache.insert_fully_verified(candidate.hash, artifact.clone());
                    }
                    candidate.data = CandidateData::VerifiedArtifact(artifact);
                }
            }
            profile.artifact_fill_ms = artifact_fill_start.elapsed().as_secs_f64() * 1000.0;
        }

        if !extracted_cache_hits.is_empty() {
            let verify_start = Instant::now();
            let extracted_artifacts = extracted_cache_hits
                .iter()
                .map(|(_, artifact)| artifact.clone())
                .collect::<Vec<_>>();
            let verify_profile = Self::verify_tx_artifacts_for_stage(
                "prepare_proposal_upgrade",
                &extracted_artifacts,
            )
            .await?;
            profile.artifact_fill_ms += verify_start.elapsed().as_secs_f64() * 1000.0;
            profile.artifact_fill_batch_verify_ms += verify_profile.batch_verify_ms;

            if let Some(cache) = stateless_cache {
                for (hash, artifact) in &extracted_cache_hits {
                    cache.insert_fully_verified(*hash, artifact.clone());
                }
            }

            for candidate in &mut deduped {
                if let CandidateData::ExtractedArtifact(artifact) = &candidate.data {
                    candidate.data = CandidateData::VerifiedArtifact(artifact.clone());
                }
            }
        }

        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&self.state)).await?;
        let deduped_candidate_count = deduped.len();
        let stateful_filter_start = Instant::now();
        let included_candidates = if deduped_candidate_count > 1
            && deduped
                .iter()
                .all(|candidate| supports_parallel_prepare(candidate.tx()))
        {
            self.execute_prepare_candidates_parallel_profiled(
                deduped,
                historical_context.clone(),
                &mut profile,
            )
            .await?
        } else {
            let mut included_candidates = Vec::new();
            for candidate in deduped {
                if let Ok((_, execution_profile)) = self
                    .execute_prepare_candidate_profiled(
                        candidate.tx().clone(),
                        candidate.artifact().as_deref(),
                        &historical_context,
                    )
                    .await
                {
                    Self::accumulate_prepare_candidate_profile(&mut profile, &execution_profile);
                    if execution_profile.check_historical_ms == 0.0 {
                        profile.historical_validation_reuse_count += 1;
                    }
                    included_candidates.push(candidate);
                }
            }
            included_candidates
        };
        profile.stateful_filter_execute_ms = stateful_filter_start.elapsed().as_secs_f64() * 1000.0;
        Self::emit_stateful_filter_breakdown(
            deduped_candidate_count,
            included_candidates.len(),
            &profile,
        );

        if self.block_tx_indexing_mode == BlockTxIndexingMode::DeferredBatch {
            let deferred_index_flush_start = Instant::now();
            self.flush_deferred_block_transactions().await?;
            profile.deferred_index_flush_ms =
                deferred_index_flush_start.elapsed().as_secs_f64() * 1000.0;
        }

        if included_candidates.is_empty() {
            return Ok((Vec::new(), profile, None));
        }

        #[derive(Clone)]
        struct ProposalAssemblyResult {
            prefix_len: usize,
            bundle_tx_bytes: Option<Bytes>,
            tail_tx_count: usize,
        }

        let prefix_select_start = Instant::now();
        let included_prefix_payload_bytes = included_candidates
            .iter()
            .scan(0u64, |total, candidate| {
                *total = total.saturating_add(candidate.bytes.len() as u64);
                Some(*total)
            })
            .collect::<Vec<_>>();
        profile.proposal_prefix_select_ms = prefix_select_start.elapsed().as_secs_f64() * 1000.0;

        let max_payload_prefix_len = if allow_oversized_proposal {
            included_candidates.len()
        } else {
            Self::max_prefix_len_for_payload_limit(
                &included_prefix_payload_bytes,
                max_proposal_size_bytes,
            )
        };
        if max_payload_prefix_len == 0 {
            return Ok((Vec::new(), profile, None));
        }

        let chain_id = self.state.get_chain_id().await?;
        let mut current_prefix_len = max_payload_prefix_len;
        let mut best_result: Option<ProposalAssemblyResult> = None;
        let mut fallback_used = false;

        while current_prefix_len > 0 && profile.proposal_assemble_attempts < 2 {
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
                    profile.aggregate_retry_cache_hits += 1;
                    tracing::info!(
                        height = proposal_height,
                        included_tx_count = current_prefix_len,
                        proposal_segment_tx_count = self.proposal_segment_tx_count,
                        "prepare_proposal_aggregate_retry_cache_hit"
                    );
                    best_result = Some(ProposalAssemblyResult {
                        prefix_len: current_prefix_len,
                        bundle_tx_bytes: cached.bundle_tx_bytes.clone(),
                        tail_tx_count: cached.tail_tx_count,
                    });
                    break;
                }
            }
            profile.aggregate_retry_cache_misses += 1;

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
                    return Ok((Vec::new(), profile, None));
                }

                if estimated_prefix_len < current_prefix_len {
                    current_prefix_len = estimated_prefix_len;
                    continue;
                }
            }

            profile.proposal_assemble_attempts += 1;
            let aggregate_start = Instant::now();
            let (families, _segment_tx_counts, aggregate_profile) =
                if let Some(segment_tx_count) = self.proposal_segment_tx_count {
                    let (segment_families, segment_tx_counts, segment_profile) =
                        Self::build_segmented_family_aggregates_for_artifacts(
                            &selected_artifacts,
                            segment_tx_count,
                        )
                        .await?;
                    (segment_families, segment_tx_counts, segment_profile)
                } else {
                    let (families, aggregate_profile) =
                        Self::build_family_aggregates_for_artifacts(&selected_artifacts, 0).await?;
                    let segment_tx_counts = if !selected_artifacts.is_empty() {
                        vec![selected_artifacts.len()]
                    } else {
                        Vec::new()
                    };
                    (families, segment_tx_counts, aggregate_profile)
                };
            let tail_aggregate_ms = aggregate_start.elapsed().as_secs_f64() * 1000.0;
            profile.tail_aggregate_ms += tail_aggregate_ms;
            profile.total_aggregate_ms += tail_aggregate_ms;
            profile.aggregate_merge_items_ms += aggregate_profile.merge_items_ms;
            profile.aggregate_setup_ms += aggregate_profile.setup_ms;
            profile.aggregate_padding_ms += aggregate_profile.padding_ms;
            profile.aggregate_collect_proofs_ms += aggregate_profile.collect_proofs_ms;
            profile.aggregate_backend_core_ms += aggregate_profile.backend_core_ms;
            profile.aggregate_proof_serialize_ms += aggregate_profile.proof_serialize_ms;
            profile.aggregate_bundle_tx_build_ms += aggregate_profile.bundle_tx_build_ms;
            profile.aggregate_spend_ms += aggregate_profile.spend_ms;
            profile.aggregate_output_ms += aggregate_profile.output_ms;
            profile.aggregate_other_ms += aggregate_profile.other_ms;

            let bundle_assemble_start = Instant::now();
            let bundle_result = self.build_aggregate_bundle_from_families(families).await;
            let bundle_assemble_ms = bundle_assemble_start.elapsed().as_secs_f64() * 1000.0;
            profile.bundle_assemble_ms += bundle_assemble_ms;

            match bundle_result {
                Ok((bundle_tx_bytes, bundle_setup_ms, bundle_tx_build_ms)) => {
                    profile.aggregate_setup_ms += bundle_setup_ms;
                    profile.aggregate_bundle_tx_build_ms += bundle_tx_build_ms;
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
                        attempt_index = profile.proposal_assemble_attempts,
                        candidate_prefix_len = current_prefix_len,
                        payload_bytes_before_bundle =
                            included_prefix_payload_bytes[current_prefix_len - 1],
                        estimated_bundle_bytes,
                        actual_bundle_bytes,
                        proposal_size_bytes,
                        max_proposal_size_bytes,
                        oversize = !allow_oversized_proposal
                            && proposal_size_bytes >= max_proposal_size_bytes,
                        aggregate_spend_ms = aggregate_profile.spend_ms,
                        aggregate_output_ms = aggregate_profile.output_ms,
                        aggregate_other_ms = aggregate_profile.other_ms,
                        "prepare_proposal_assembly_attempt"
                    );

                    if !allow_oversized_proposal && proposal_size_bytes >= max_proposal_size_bytes {
                        profile.proposal_oversize_retry_count += 1;
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
                            attempt_index = profile.proposal_assemble_attempts,
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
                        let cached_bundle_tx_bytes = bundle_tx_bytes.clone();
                        best_result = Some(ProposalAssemblyResult {
                            prefix_len: current_prefix_len,
                            bundle_tx_bytes,
                            tail_tx_count: selected_candidates.len(),
                        });
                        self.aggregate_retry_cache = Some(CachedProposalAggregate {
                            height: proposal_height,
                            included_tx_count: current_prefix_len,
                            proposal_txs_digest,
                            proposal_segment_tx_count: self.proposal_segment_tx_count,
                            bundle_tx_bytes: cached_bundle_tx_bytes,
                            tail_tx_count: selected_candidates.len(),
                        });
                        break;
                    }
                }
                Err(err) if err.to_string().contains("padded proof count") => {
                    profile.proposal_oversize_retry_count += 1;
                    tracing::warn!(
                        attempt_index = profile.proposal_assemble_attempts,
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
            profile.tail_tx_count = best_result.tail_tx_count;

            let mut included_txs = included_candidates[..best_result.prefix_len]
                .iter()
                .map(|candidate| candidate.bytes.clone())
                .collect::<Vec<_>>();
            if let Some(bundle_tx_bytes) = best_result.bundle_tx_bytes {
                included_txs.push(bundle_tx_bytes);
            }
            return Ok((included_txs, profile, None));
        }

        Ok((Vec::new(), profile, None))
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

    pub(crate) fn aggregate_retry_cache(&self) -> Option<CachedProposalAggregate> {
        self.aggregate_retry_cache.clone()
    }

    /// Returns whether the application is ready to start.
    #[instrument(skip_all, ret)]
    pub async fn is_ready(state: Snapshot) -> bool {
        // If the chain is halted, we are not ready to start the application.
        // This is a safety mechanism to prevent the chain from starting if it
        // is in a halted state.
        if state.is_chain_halted().await {
            return false;
        }
        if let Err(error) = penumbra_sdk_sct::nullifier_tree::verify_committed_root(&state).await {
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
                state_tx.put_chain_id(genesis.chain_id.clone());
                Sct::init_chain(&mut state_tx, Some(&genesis.sct_content)).await;
                ShieldedPool::init_chain(&mut state_tx, Some(&genesis.shielded_pool_content)).await;
                Staking::init_chain(
                    &mut state_tx,
                    Some(&(
                        genesis.validator_content.clone(),
                        genesis.shielded_pool_content.clone(),
                    )),
                )
                .await;
                Ibc::init_chain(&mut state_tx, Some(&genesis.ibc_content)).await;
                Governance::init_chain(&mut state_tx, Some(&genesis.governance_content)).await;
                FeeComponent::init_chain(&mut state_tx, Some(&genesis.fee_content)).await;
                // Initialize compliance component with empty trees for anchor tracking.
                // Unregulated assets don't need registration (proven via non-membership).
                Compliance::init_chain(&mut state_tx, Some(&genesis.compliance_content)).await;

                state_tx
                    .finish_block()
                    .await
                    .expect("must be able to finish compact block");
            }
            AppState::Checkpoint(_) => {
                ShieldedPool::init_chain(&mut state_tx, None).await;
                Staking::init_chain(&mut state_tx, None).await;
                Ibc::init_chain(&mut state_tx, None).await;
                Governance::init_chain(&mut state_tx, None).await;
                FeeComponent::init_chain(&mut state_tx, None).await;
                Compliance::init_chain(&mut state_tx, None).await;
            }
        };

        // Note that `init_chain` can not emit any events, and we do not want to
        // work around this as it violates the design principle that events are changes
        // to initial data.
        //
        // This means that indexers are responsible for parsing genesis data and bootstrapping
        // their initial state before processing chronological events.
        //
        // See: https://github.com/mizufinance/penumbra/pull/4449#discussion_r1636868800

        state_tx.apply();
    }

    async fn prepare_proposal_impl_profiled(
        &mut self,
        proposal: request::PrepareProposal,
        stateless_cache: Option<&StatelessCache>,
        allow_oversized_proposal: bool,
    ) -> (
        response::PrepareProposal,
        PrepareProposalProfile,
        Option<ProposalArtifactSidecar>,
    ) {
        if self.state.is_chain_halted().await {
            // If we find ourselves preparing a proposal for a halted chain
            // we stop abruptly to prevent any progress.
            // The persistent halt mechanism will prevent restarts until we are ready.
            process::exit(0);
        }

        let num_candidate_txs = proposal.txs.len();
        tracing::debug!(
            "processing PrepareProposal, found {} candidate transactions",
            num_candidate_txs
        );

        // This is a node controlled parameter that is different from the homonymous
        // mempool's `max_tx_bytes`. Comet will send us raw proposals that exceed this
        // limit, presuming that a subset of those transactions will be shed.
        // More context in https://github.com/cometbft/cometbft/blob/v0.37.5/spec/abci/abci%2B%2B_app_requirements.md
        let max_proposal_size_bytes = proposal.max_tx_bytes as u64;
        let (included_txs, profile, sidecar) = match self
            .prepare_proposal_batched_profiled(
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
                (Vec::new(), PrepareProposalProfile::default(), None)
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
        if profile.total_aggregate_ms > 0.0 {
            tracing::info!(
                candidate_tx_count = num_candidate_txs,
                included_tx_count = included_txs.len(),
                proposal_assemble_attempts = profile.proposal_assemble_attempts,
                proposal_oversize_retry_count = profile.proposal_oversize_retry_count,
                aggregate_retry_cache_hits = profile.aggregate_retry_cache_hits,
                aggregate_retry_cache_misses = profile.aggregate_retry_cache_misses,
                proposal_segment_tx_count = self.proposal_segment_tx_count,
                proposal_segment_count = sidecar
                    .as_ref()
                    .map(|sidecar| sidecar.segment_tx_counts.len())
                    .unwrap_or(0),
                sidecar_bytes = sidecar
                    .as_ref()
                    .map(|sidecar| sidecar.encoded_bytes)
                    .unwrap_or(0),
                aggregate_merge_items_ms = profile.aggregate_merge_items_ms,
                aggregate_setup_ms = profile.aggregate_setup_ms,
                aggregate_padding_ms = profile.aggregate_padding_ms,
                aggregate_collect_proofs_ms = profile.aggregate_collect_proofs_ms,
                aggregate_backend_core_ms = profile.aggregate_backend_core_ms,
                aggregate_proof_serialize_ms = profile.aggregate_proof_serialize_ms,
                aggregate_bundle_tx_build_ms = profile.aggregate_bundle_tx_build_ms,
                aggregate_spend_ms = profile.aggregate_spend_ms,
                aggregate_output_ms = profile.aggregate_output_ms,
                aggregate_other_ms = profile.aggregate_other_ms,
                tail_aggregate_ms = profile.tail_aggregate_ms,
                bundle_assemble_ms = profile.bundle_assemble_ms,
                total_aggregate_ms = profile.total_aggregate_ms,
                "prepare_proposal_aggregate_profile"
            );
        }
        tracing::info!(
            candidate_scan_ms = profile.candidate_scan_ms,
            nullifier_dedup_ms = profile.nullifier_dedup_ms,
            anchor_precheck_ms = profile.anchor_precheck_ms,
            artifact_fill_ms = profile.artifact_fill_ms,
            artifact_fill_batch_verify_ms = profile.artifact_fill_batch_verify_ms,
            stateful_filter_execute_ms = profile.stateful_filter_execute_ms,
            candidate_read_wall_ms = profile.stateful_filter_candidate_read_wall_ms,
            candidate_effects_build_ms = profile.stateful_filter_candidate_effects_build_ms,
            serial_apply_wall_ms = profile.stateful_filter_serial_apply_wall_ms,
            serial_same_block_conflict_ms = profile.stateful_filter_serial_same_block_conflict_ms,
            serial_state_delta_apply_ms = profile.stateful_filter_serial_state_delta_apply_ms,
            serial_nullifier_insert_ms = profile.stateful_filter_serial_nullifier_insert_ms,
            proposal_nullifier_lookup_write_ms =
                profile.stateful_filter_proposal_nullifier_lookup_write_ms,
            proposal_pending_nullifier_stage_ms =
                profile.stateful_filter_proposal_pending_nullifier_stage_ms,
            serial_sct_append_ms = profile.stateful_filter_serial_sct_append_ms,
            serial_event_emit_ms = profile.stateful_filter_serial_event_emit_ms,
            serial_fee_apply_ms = profile.stateful_filter_serial_fee_apply_ms,
            deferred_index_flush_ms = profile.deferred_index_flush_ms,
            proposal_prefix_select_ms = profile.proposal_prefix_select_ms,
            sidecar_build_ms = profile.sidecar_build_ms,
            bundle_assemble_ms = profile.bundle_assemble_ms,
            total_aggregate_ms = profile.total_aggregate_ms,
            aggregate_merge_items_ms = profile.aggregate_merge_items_ms,
            aggregate_collect_proofs_ms = profile.aggregate_collect_proofs_ms,
            aggregate_backend_core_ms = profile.aggregate_backend_core_ms,
            aggregate_proof_serialize_ms = profile.aggregate_proof_serialize_ms,
            aggregate_bundle_tx_build_ms = profile.aggregate_bundle_tx_build_ms,
            candidate_read_wall_ms_per_tx = if !included_txs.is_empty() {
                profile.stateful_filter_candidate_read_wall_ms / included_txs.len() as f64
            } else {
                0.0
            },
            serial_apply_wall_ms_per_tx = if !included_txs.is_empty() {
                profile.stateful_filter_serial_apply_wall_ms / included_txs.len() as f64
            } else {
                0.0
            },
            aggregate_total_ms_per_tx = if !included_txs.is_empty() {
                profile.total_aggregate_ms / included_txs.len() as f64
            } else {
                0.0
            },
            proposal_assemble_attempts = profile.proposal_assemble_attempts,
            proposal_oversize_retry_count = profile.proposal_oversize_retry_count,
            tail_tx_count = profile.tail_tx_count,
            proposal_segment_tx_count = self.proposal_segment_tx_count,
            proposal_segment_count = sidecar
                .as_ref()
                .map(|sidecar| sidecar.segment_tx_counts.len())
                .unwrap_or(0),
            sidecar_bytes = sidecar
                .as_ref()
                .map(|sidecar| sidecar.encoded_bytes)
                .unwrap_or(0),
            "prepare_proposal_runtime_profile"
        );

        (
            response::PrepareProposal { txs: included_txs },
            profile,
            sidecar,
        )
    }

    async fn prepare_proposal_impl(
        &mut self,
        proposal: request::PrepareProposal,
        stateless_cache: Option<&StatelessCache>,
    ) -> response::PrepareProposal {
        self.prepare_proposal_impl_profiled(proposal, stateless_cache, false)
            .await
            .0
    }

    /// Synthetic benchmark baseline: no shared artifact cache between mempool,
    /// proposer, and validator stages.
    pub async fn prepare_proposal_v1(
        &mut self,
        proposal: request::PrepareProposal,
    ) -> response::PrepareProposal {
        self.prepare_proposal_impl(proposal, None).await
    }

    pub async fn prepare_proposal_v1_profiled(
        &mut self,
        proposal: request::PrepareProposal,
        allow_oversized_proposal: bool,
    ) -> (
        response::PrepareProposal,
        PrepareProposalProfile,
        Option<ProposalArtifactSidecar>,
    ) {
        self.prepare_proposal_impl_profiled(proposal, None, allow_oversized_proposal)
            .await
    }

    /// Production and synthetic v2 path: use the shared artifact cache.
    pub async fn prepare_proposal_v2(
        &mut self,
        proposal: request::PrepareProposal,
        stateless_cache: Option<&StatelessCache>,
    ) -> response::PrepareProposal {
        self.prepare_proposal_impl(proposal, stateless_cache).await
    }

    pub async fn prepare_proposal_v2_profiled(
        &mut self,
        proposal: request::PrepareProposal,
        stateless_cache: Option<&StatelessCache>,
        allow_oversized_proposal: bool,
    ) -> (
        response::PrepareProposal,
        PrepareProposalProfile,
        Option<ProposalArtifactSidecar>,
    ) {
        self.prepare_proposal_impl_profiled(proposal, stateless_cache, allow_oversized_proposal)
            .await
    }

    #[instrument(skip_all, ret, level = "debug")]
    async fn process_proposal_impl_profiled(
        &mut self,
        proposal: request::ProcessProposal,
        stateless_cache: Option<&StatelessCache>,
        synthetic_sidecar: Option<&ProposalArtifactSidecar>,
        allow_oversized_proposal: bool,
    ) -> (response::ProcessProposal, ProcessProposalProfile) {
        tracing::debug!(
            height = proposal.height.value(),
            proposer = ?proposal.proposer_address,
            proposal_hash = ?proposal.hash,
            "processing proposal"
        );

        let mut profile = ProcessProposalProfile::default();
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
                return (response::ProcessProposal::Reject, profile);
            }};
            ($reason:literal, $($field:tt)*) => {{
                tracing::warn!(
                    height = proposal_height,
                    proposal_hash = %proposal_hash,
                    reason = $reason,
                    $($field)*,
                    "process_proposal_reject_reason"
                );
                return (response::ProcessProposal::Reject, profile);
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
            VerifiedArtifact(Arc<TxArtifact>),
            Decoded(Arc<Transaction>),
        }

        struct UserTx {
            hash: [u8; 32],
            data: UserTxData,
            cache_miss: bool,
            extracted_cache_hit: bool,
        }

        impl UserTx {
            fn tx(&self) -> &Arc<Transaction> {
                match &self.data {
                    UserTxData::ExtractedArtifact(artifact)
                    | UserTxData::VerifiedArtifact(artifact) => &artifact.tx,
                    UserTxData::Decoded(tx) => tx,
                }
            }

            fn artifact(&self) -> Option<Arc<TxArtifact>> {
                match &self.data {
                    UserTxData::ExtractedArtifact(artifact)
                    | UserTxData::VerifiedArtifact(artifact) => Some(artifact.clone()),
                    UserTxData::Decoded(_) => None,
                }
            }
        }

        let proposal_tx_count = proposal.txs.len();
        let mut total_txs_payload_size = 0usize;
        let mut user_txs = Vec::with_capacity(proposal_tx_count);
        let mut bundle_tx: Option<Arc<Transaction>> = None;
        let lookup_start = Instant::now();
        let mut decode_classify_ms = 0.0f64;

        for (index, tx_bytes) in proposal.txs.into_iter().enumerate() {
            let tx_size = tx_bytes.len();
            if !allow_oversized_proposal && tx_size > max_transaction_size_bytes() {
                reject_process_proposal!("tx_size_exceeded", index, tx_size);
            }

            total_txs_payload_size = total_txs_payload_size.saturating_add(tx_size);
            if !allow_oversized_proposal && total_txs_payload_size >= MAX_BLOCK_TXS_PAYLOAD_BYTES {
                reject_process_proposal!(
                    "total_txs_payload_exceeded",
                    index,
                    total_txs_payload_size
                );
            }

            let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_ref()).into();
            if let Some(cache) = stateless_cache {
                match cache.get(&tx_hash) {
                    Some(CacheEntry::Invalid) => {
                        reject_process_proposal!("stateless_cache_invalid", tx_hash = %hex::encode(tx_hash));
                    }
                    Some(CacheEntry::FullyVerified(artifact)) => {
                        Self::record_artifact_reuse("process_proposal");
                        profile.artifact_hit_count += 1;
                        profile.warm_reuse_count += 1;
                        user_txs.push(UserTx {
                            hash: tx_hash,
                            data: UserTxData::VerifiedArtifact(artifact),
                            cache_miss: false,
                            extracted_cache_hit: false,
                        });
                        continue;
                    }
                    Some(CacheEntry::Extracted(artifact)) => {
                        Self::record_artifact_reuse("process_proposal");
                        profile.artifact_hit_count += 1;
                        profile.warm_reuse_count += 1;
                        user_txs.push(UserTx {
                            hash: tx_hash,
                            data: UserTxData::ExtractedArtifact(artifact),
                            cache_miss: false,
                            extracted_cache_hit: true,
                        });
                        continue;
                    }
                    None => {}
                }
            }

            let decode_start = Instant::now();
            let tx = match Transaction::decode(tx_bytes.as_ref()) {
                Ok(tx) => Arc::new(tx),
                Err(_) => reject_process_proposal!("tx_decode_failed", index),
            };

            if tx.is_aggregate_bundle_tx() {
                decode_classify_ms += decode_start.elapsed().as_secs_f64() * 1000.0;
                if index + 1 != proposal_tx_count {
                    reject_process_proposal!("aggregate_bundle_not_last", index, proposal_tx_count);
                }
                if Self::ensure_aggregate_bundle_tx_shape(&tx).is_err() {
                    reject_process_proposal!("aggregate_bundle_bad_shape", index);
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
            decode_classify_ms += decode_start.elapsed().as_secs_f64() * 1000.0;

            profile.artifact_miss_count += 1;
            user_txs.push(UserTx {
                hash: tx_hash,
                data: UserTxData::Decoded(tx),
                cache_miss: true,
                extracted_cache_hit: false,
            });
        }
        profile.artifact_lookup_ms = lookup_start.elapsed().as_secs_f64() * 1000.0;
        profile.tx_decode_classify_ms = decode_classify_ms;

        if !user_txs.is_empty() {
            let sidecar_load_start = Instant::now();
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
            profile.cold_sidecar_load_ms = sidecar_load_start.elapsed().as_secs_f64() * 1000.0;
            profile.cold_sidecar_bytes = synthetic_sidecar.map(|s| s.encoded_bytes).unwrap_or(0);

            if let Some(sidecar) = synthetic_sidecar {
                if !sidecar_hits.is_empty() {
                    let sidecar_decode_start = Instant::now();
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
                    profile.cold_sidecar_decode_ms =
                        sidecar_decode_start.elapsed().as_secs_f64() * 1000.0;
                }
            }

            if !raw_miss_txs.is_empty() {
                let reconstruction_start = Instant::now();
                let (miss_artifacts, artifact_profile) =
                    match Self::build_tx_artifacts_for_stage("process_proposal", &raw_miss_txs)
                        .await
                    {
                        Ok(result) => result,
                        Err(_) => reject_process_proposal!("artifact_reconstruction_failed"),
                    };
                let mut miss_artifacts = miss_artifacts.into_iter();
                profile.cold_reconstruction_ms =
                    reconstruction_start.elapsed().as_secs_f64() * 1000.0;
                profile.cold_reconstruction_precheck_ms = artifact_profile.precheck_ms;
                profile.cold_reconstruction_action_extract_ms = artifact_profile.action_extract_ms;
                profile.cold_reconstruction_batch_verify_ms = artifact_profile.batch_verify_ms;

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
        let nullifier_dedup_start = Instant::now();
        if Self::ensure_unique_spend_nullifiers_from_artifacts(&artifacts).is_err() {
            reject_process_proposal!("duplicate_spend_nullifiers");
        }
        profile.nullifier_dedup_ms = nullifier_dedup_start.elapsed().as_secs_f64() * 1000.0;

        let anchor_recheck_start = Instant::now();
        if self
            .precheck_compliance_anchors_dedup_from_artifacts(&artifacts)
            .await
            .is_err()
        {
            reject_process_proposal!("anchor_recheck_failed");
        }
        profile.anchor_recheck_ms = anchor_recheck_start.elapsed().as_secs_f64() * 1000.0;

        let total_proofs = Self::total_artifact_proof_count(&artifacts);
        let mut aggregate_verify_task: Option<tokio::task::JoinHandle<anyhow::Result<()>>> = None;
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

        if let Some(cache) = stateless_cache {
            for user_tx in &user_txs {
                if user_tx.cache_miss || user_tx.extracted_cache_hit {
                    let artifact = user_tx
                        .artifact()
                        .expect("cache miss proposal tx should have artifact");
                    cache.insert_fully_verified(user_tx.hash, artifact);
                }
            }
        }

        let stateful_replay_start = Instant::now();
        let historical_context = match HistoricalCheckContext::load(Arc::as_ref(&self.state)).await
        {
            Ok(context) => context,
            Err(_) => reject_process_proposal!("historical_context_load_failed"),
        };
        for user_tx in user_txs {
            let execution_profile = match self
                .deliver_tx_with_verified_stateless_profiled(
                    user_tx.tx().clone(),
                    Some(&historical_context),
                )
                .await
            {
                Ok((_, execution_profile)) => execution_profile,
                Err(_) => reject_process_proposal!("stateful_replay_failed"),
            };
            profile.stateful_replay_check_historical_ms += execution_profile.check_historical_ms;
            profile.stateful_replay_get_block_height_ms += execution_profile.get_block_height_ms;
            profile.stateful_replay_clone_tx_ms += execution_profile.clone_tx_ms;
            profile.stateful_replay_proto_convert_ms += execution_profile.proto_convert_ms;
            profile.stateful_replay_put_block_transaction_ms +=
                execution_profile.put_block_transaction_ms;
            profile.stateful_replay_begin_state_tx_ms += execution_profile.begin_state_tx_ms;
            profile.stateful_replay_index_tx_ms += execution_profile.index_tx_ms;
            profile.stateful_replay_check_and_execute_ms += execution_profile.check_and_execute_ms;
            profile.stateful_replay_set_source_ms += execution_profile.set_source_ms;
            profile.stateful_replay_pay_fee_ms += execution_profile.pay_fee_ms;
            profile.stateful_replay_action_execute_ms += execution_profile.action_execute_ms;
            profile.stateful_replay_spend_action_execute_ms +=
                execution_profile.spend_action_execute_ms;
            profile.stateful_replay_spend_nullifier_check_ms +=
                execution_profile.spend_nullifier_check_ms;
            profile.stateful_replay_spend_nullifier_tx_local_scan_ms +=
                execution_profile.spend_nullifier_tx_local_scan_ms;
            profile.stateful_replay_spend_nullifier_block_log_lookup_ms +=
                execution_profile.spend_nullifier_block_log_lookup_ms;
            profile.stateful_replay_spend_nullifier_committed_check_ms +=
                execution_profile.spend_nullifier_committed_check_ms;
            profile.stateful_replay_spend_nullifier_enqueue_ms +=
                execution_profile.spend_nullifier_enqueue_ms;
            profile.stateful_replay_spend_nullifier_stage_ms +=
                execution_profile.spend_nullifier_stage_ms;
            profile.stateful_replay_spend_nullifier_merge_ms +=
                execution_profile.spend_nullifier_merge_ms;
            profile.stateful_replay_output_action_execute_ms +=
                execution_profile.output_action_execute_ms;
            profile.stateful_replay_output_add_note_payload_ms +=
                execution_profile.output_add_note_payload_ms;
            profile.stateful_replay_other_action_execute_ms +=
                execution_profile.other_action_execute_ms;
            profile.stateful_replay_record_clues_ms += execution_profile.record_clues_ms;
            profile.stateful_replay_apply_ms += execution_profile.apply_ms;
        }
        profile.stateful_replay_execute_ms = stateful_replay_start.elapsed().as_secs_f64() * 1000.0;

        if let Some(aggregate_verify_task) = aggregate_verify_task {
            let aggregate_verify_start = Instant::now();
            match aggregate_verify_task.await {
                Ok(Ok(())) => {}
                _ => reject_process_proposal!("aggregate_verify_task_failed"),
            }
            profile.aggregate_verify_ms = aggregate_verify_start.elapsed().as_secs_f64() * 1000.0;
        }

        if self.block_tx_indexing_mode == BlockTxIndexingMode::DeferredBatch {
            let deferred_index_flush_start = Instant::now();
            if self.flush_deferred_block_transactions().await.is_err() {
                reject_process_proposal!("deferred_index_flush_failed");
            }
            profile.deferred_index_flush_ms =
                deferred_index_flush_start.elapsed().as_secs_f64() * 1000.0;
        }

        (response::ProcessProposal::Accept, profile)
    }

    #[instrument(skip_all, ret, level = "debug")]
    async fn process_proposal_impl(
        &mut self,
        proposal: request::ProcessProposal,
        stateless_cache: Option<&StatelessCache>,
    ) -> response::ProcessProposal {
        self.process_proposal_impl_profiled(proposal, stateless_cache, None, false)
            .await
            .0
    }

    /// Synthetic benchmark baseline: no shared artifact cache between mempool,
    /// proposer, and validator stages.
    pub async fn process_proposal_v1(
        &mut self,
        proposal: request::ProcessProposal,
    ) -> response::ProcessProposal {
        self.process_proposal_impl(proposal, None).await
    }

    pub async fn process_proposal_v1_profiled(
        &mut self,
        proposal: request::ProcessProposal,
        allow_oversized_proposal: bool,
    ) -> (response::ProcessProposal, ProcessProposalProfile) {
        self.process_proposal_impl_profiled(proposal, None, None, allow_oversized_proposal)
            .await
    }

    /// Production and synthetic v2 path: use the shared artifact cache.
    pub async fn process_proposal_v2(
        &mut self,
        proposal: request::ProcessProposal,
        stateless_cache: Option<&StatelessCache>,
    ) -> response::ProcessProposal {
        self.process_proposal_impl(proposal, stateless_cache).await
    }

    pub async fn process_proposal_v2_profiled(
        &mut self,
        proposal: request::ProcessProposal,
        stateless_cache: Option<&StatelessCache>,
        synthetic_sidecar: Option<&ProposalArtifactSidecar>,
        allow_oversized_proposal: bool,
    ) -> (response::ProcessProposal, ProcessProposalProfile) {
        self.process_proposal_impl_profiled(
            proposal,
            stateless_cache,
            synthetic_sidecar,
            allow_oversized_proposal,
        )
        .await
    }

    pub async fn process_proposal(
        &mut self,
        proposal: request::ProcessProposal,
        stateless_cache: Option<&StatelessCache>,
    ) -> response::ProcessProposal {
        self.process_proposal_v2(proposal, stateless_cache).await
    }

    pub async fn begin_block(&mut self, begin_block: &request::BeginBlock) -> Vec<abci::Event> {
        self.pending_sct_append_log.clear();
        let mut state_tx = StateDelta::new(self.state.clone());

        // If a app parameter change is scheduled for this block, apply it here,
        // before any other component has executed. This ensures that app
        // parameter changes are consistently applied precisely at the boundary
        // between blocks.
        //
        // Note that because _nothing_ has executed yet, we need to get the
        // current height from the begin_block request, rather than from the
        // state (it will be set by the SCT component, which executes first).
        if let Some(change) = state_tx
            .param_changes_for_height(begin_block.header.height.into())
            .await
            .expect("param changes should always be readable, even if unset")
        {
            let old_params = state_tx
                .get_app_params()
                .await
                .expect("must be able to read app params");
            match change.apply_changes(old_params) {
                Ok(new_params) => {
                    tracing::info!(?change, "applied app parameter change");
                    state_tx.put_app_params(new_params.clone());
                    state_tx.record_proto(
                        EventAppParametersChange {
                            new_parameters: new_params,
                        }
                        .to_proto(),
                    )
                }
                Err(e) => {
                    // N.B. this is an "info" rather than "warn" because it does not report
                    // a problem with _this instance of the application_, but rather is an expected
                    // behavior.
                    tracing::info!(?change, ?e, "failed to apply approved app parameter change");
                }
            }
        }

        clear_block_fee_price_cache(&mut state_tx);

        // Run each of the begin block handlers for each component, in sequence:
        let mut arc_state_tx = Arc::new(state_tx);
        Sct::begin_block(&mut arc_state_tx, begin_block).await;
        ShieldedPool::begin_block(&mut arc_state_tx, begin_block).await;
        Ibc::begin_block::<PenumbraHost, StateDelta<Arc<StateDelta<cnidarium::Snapshot>>>>(
            &mut arc_state_tx,
            begin_block,
        )
        .await;
        Governance::begin_block(&mut arc_state_tx, begin_block).await;
        Staking::begin_block(&mut arc_state_tx, begin_block).await;
        FeeComponent::begin_block(&mut arc_state_tx, begin_block).await;

        let state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");

        self.apply(state_tx)
    }

    /// Wrapper function for [`Self::deliver_tx`] that decodes from bytes.
    ///
    /// When a `StatelessCache` is provided, anchor-independent tx artifacts are
    /// cached by SHA-256 of the raw tx bytes. Cache hits skip decode + stateless
    /// proof work entirely; misses build the artifact once while running
    /// historical checks in parallel.
    async fn deliver_tx_bytes_impl(
        &mut self,
        tx_bytes: &[u8],
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        let (events, _) = self
            .deliver_tx_bytes_impl_profiled(tx_bytes, stateless_cache)
            .await?;
        Ok(events)
    }

    async fn deliver_tx_bytes_impl_profiled(
        &mut self,
        tx_bytes: &[u8],
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<(Vec<abci::Event>, CheckTxProfile)> {
        let total_start = Instant::now();
        let mut profile = CheckTxProfile::default();
        if let Some(cache) = stateless_cache {
            let cache_lookup_start = Instant::now();
            let hash: [u8; 32] = sha2::Sha256::digest(tx_bytes).into();
            let cache_entry = cache.get(&hash);
            profile.checktx_cache_lookup_ms = cache_lookup_start.elapsed().as_secs_f64() * 1000.0;
            match cache_entry {
                Some(CacheEntry::Extracted(artifact) | CacheEntry::FullyVerified(artifact)) => {
                    tracing::debug!("stateless cache hit (valid)");
                    Self::record_artifact_reuse("checktx");
                    profile.cache_hit_count = 1;
                    let skip_historical =
                        artifact.has_matching_historical_validation(self.snapshot_version);
                    let execute_fast_start = Instant::now();
                    let (events, execute_profile) =
                        if supports_parallel_prepare(artifact.tx.as_ref())
                            && self.checktx_shared_context.is_some()
                        {
                            self.execute_checktx_fast_profiled(artifact.tx.clone(), skip_historical)
                                .await?
                        } else {
                            self.deliver_tx_with_verified_stateless_profiled(
                                artifact.tx.clone(),
                                None,
                            )
                            .await?
                        };
                    profile.checktx_execute_fast_wall_ms =
                        execute_fast_start.elapsed().as_secs_f64() * 1000.0;
                    profile.check_historical_ms = execute_profile.check_historical_ms;
                    Self::fill_checktx_execute_profile(&mut profile, &execute_profile);
                    profile.checktx_total_wall_ms = total_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((events, profile));
                }
                Some(CacheEntry::Invalid) => {
                    anyhow::bail!("transaction previously failed stateless checks");
                }
                None => {
                    let miss_start = Instant::now();
                    let (events, miss_profile) = self
                        .deliver_tx_with_stateless_extraction_caching_profiled(
                            tx_bytes, cache, hash,
                        )
                        .await?;
                    let mut miss_profile = miss_profile;
                    miss_profile.checktx_stateless_phase_wall_ms =
                        miss_start.elapsed().as_secs_f64() * 1000.0;
                    miss_profile.checktx_cache_lookup_ms = profile.checktx_cache_lookup_ms;
                    miss_profile.checktx_total_wall_ms =
                        total_start.elapsed().as_secs_f64() * 1000.0;
                    return Ok((events, miss_profile));
                }
            }
        }

        let decode_start = Instant::now();
        let tx = Arc::new(Transaction::decode(tx_bytes).context("decoding transaction")?);
        profile.decode_tx_ms = decode_start.elapsed().as_secs_f64() * 1000.0;
        Self::ensure_user_tx_has_no_internal_actions(&tx)?;
        let execute_fast_start = Instant::now();
        let (events, uncached_profile) = self.deliver_tx_profiled(tx).await?;
        profile = uncached_profile;
        profile.decode_tx_ms = decode_start.elapsed().as_secs_f64() * 1000.0;
        profile.checktx_execute_fast_wall_ms = execute_fast_start.elapsed().as_secs_f64() * 1000.0;
        profile.checktx_total_wall_ms = total_start.elapsed().as_secs_f64() * 1000.0;
        Ok((events, profile))
    }

    /// Synthetic benchmark baseline: no shared artifact cache between mempool,
    /// proposer, and validator stages.
    pub async fn deliver_tx_bytes_v1(&mut self, tx_bytes: &[u8]) -> Result<Vec<abci::Event>> {
        self.deliver_tx_bytes_impl(tx_bytes, None).await
    }

    pub async fn deliver_tx_bytes_v1_profiled(
        &mut self,
        tx_bytes: &[u8],
    ) -> Result<(Vec<abci::Event>, CheckTxProfile)> {
        self.deliver_tx_bytes_impl_profiled(tx_bytes, None).await
    }

    /// Production and synthetic v2 path: use the shared artifact cache.
    pub async fn deliver_tx_bytes_v2(
        &mut self,
        tx_bytes: &[u8],
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        self.deliver_tx_bytes_impl(tx_bytes, stateless_cache).await
    }

    pub async fn deliver_tx_bytes_v2_profiled(
        &mut self,
        tx_bytes: &[u8],
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<(Vec<abci::Event>, CheckTxProfile)> {
        self.deliver_tx_bytes_impl_profiled(tx_bytes, stateless_cache)
            .await
    }

    /// Benchmark-only admission path that reuses extracted stateless artifacts
    /// but intentionally skips per-transaction batch proof verification on
    /// cache misses.
    pub async fn deliver_tx_bytes_v2_extracted_profiled_for_bench(
        &mut self,
        tx_bytes: &[u8],
        stateless_cache: &StatelessCache,
    ) -> Result<(Vec<abci::Event>, CheckTxProfile)> {
        let total_start = Instant::now();
        let mut profile = CheckTxProfile::default();
        let cache_lookup_start = Instant::now();
        let hash: [u8; 32] = sha2::Sha256::digest(tx_bytes).into();
        let cache_entry = stateless_cache.get(&hash);
        profile.checktx_cache_lookup_ms = cache_lookup_start.elapsed().as_secs_f64() * 1000.0;

        match cache_entry {
            Some(CacheEntry::Extracted(artifact) | CacheEntry::FullyVerified(artifact)) => {
                tracing::debug!("stateless cache hit (valid, extracted-for-bench)");
                Self::record_artifact_reuse("checktx_extract_only");
                profile.cache_hit_count = 1;
                let skip_historical =
                    artifact.has_matching_historical_validation(self.snapshot_version);
                let execute_fast_start = Instant::now();
                let (events, execute_profile) = if supports_parallel_prepare(artifact.tx.as_ref())
                    && self.checktx_shared_context.is_some()
                {
                    self.execute_checktx_fast_profiled(artifact.tx.clone(), skip_historical)
                        .await?
                } else {
                    self.deliver_tx_with_verified_stateless_profiled(artifact.tx.clone(), None)
                        .await?
                };
                profile.checktx_execute_fast_wall_ms =
                    execute_fast_start.elapsed().as_secs_f64() * 1000.0;
                profile.check_historical_ms = execute_profile.check_historical_ms;
                Self::fill_checktx_execute_profile(&mut profile, &execute_profile);
                profile.checktx_total_wall_ms = total_start.elapsed().as_secs_f64() * 1000.0;
                Ok((events, profile))
            }
            Some(CacheEntry::Invalid) => {
                anyhow::bail!("transaction previously failed stateless checks");
            }
            None => {
                let miss_start = Instant::now();
                let (events, mut miss_profile) = self
                    .deliver_tx_with_stateless_extraction_caching_profiled(
                        tx_bytes,
                        stateless_cache,
                        hash,
                    )
                    .await?;
                miss_profile.checktx_stateless_phase_wall_ms =
                    miss_start.elapsed().as_secs_f64() * 1000.0;
                miss_profile.checktx_cache_lookup_ms = profile.checktx_cache_lookup_ms;
                miss_profile.checktx_total_wall_ms = total_start.elapsed().as_secs_f64() * 1000.0;
                Ok((events, miss_profile))
            }
        }
    }

    pub async fn deliver_tx_bytes(
        &mut self,
        tx_bytes: &[u8],
        stateless_cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        self.deliver_tx_bytes_v2(tx_bytes, stateless_cache).await
    }

    fn fill_checktx_execute_profile(
        profile: &mut CheckTxProfile,
        execute_profile: &VerifiedStatefulTxBreakdown,
    ) {
        profile.execute_get_block_height_ms = execute_profile.get_block_height_ms;
        profile.checktx_fast_context_load_ms = execute_profile.checktx_fast_context_load_ms;
        profile.checktx_fast_read_queue_wait_ms = execute_profile.checktx_fast_read_queue_wait_ms;
        profile.checktx_fast_read_blocking_total_ms =
            execute_profile.checktx_fast_read_blocking_total_ms;
        profile.checktx_fast_prepare_join_wall_ms =
            execute_profile.checktx_fast_prepare_join_wall_ms;
        profile.checktx_fast_apply_wall_ms = execute_profile.checktx_fast_apply_wall_ms;
        profile.execute_index_tx_ms = execute_profile.index_tx_ms;
        profile.execute_check_and_execute_ms = execute_profile.check_and_execute_ms;
        profile.execute_set_source_ms = execute_profile.set_source_ms;
        profile.execute_pay_fee_ms = execute_profile.pay_fee_ms;
        profile.execute_action_execute_ms = execute_profile.action_execute_ms;
        profile.execute_read_local_precheck_ms = execute_profile.read_local_precheck_ms;
        profile.execute_read_lookup_wait_or_join_ms = execute_profile.read_lookup_wait_or_join_ms;
        profile.execute_read_historical_check_ms = execute_profile.read_historical_check_ms;
        profile.execute_read_nullifier_wait_ms = execute_profile.read_nullifier_wait_ms;
        profile.execute_read_anchor_cache_wait_ms = execute_profile.read_anchor_cache_wait_ms;
        profile.execute_spend_action_execute_ms = execute_profile.spend_action_execute_ms;
        profile.execute_spend_nullifier_check_ms = execute_profile.spend_nullifier_check_ms;
        profile.execute_spend_nullifier_tx_local_scan_ms =
            execute_profile.spend_nullifier_tx_local_scan_ms;
        profile.execute_spend_nullifier_block_log_lookup_ms =
            execute_profile.spend_nullifier_block_log_lookup_ms;
        profile.execute_spend_nullifier_committed_check_ms =
            execute_profile.spend_nullifier_committed_check_ms;
        profile.execute_spend_nullifier_enqueue_ms = execute_profile.spend_nullifier_enqueue_ms;
        profile.execute_spend_nullifier_stage_ms = execute_profile.spend_nullifier_stage_ms;
        profile.execute_spend_nullifier_merge_ms = execute_profile.spend_nullifier_merge_ms;
        profile.execute_nullifier_lookup_count = execute_profile.nullifier_lookup_count;
        profile.execute_output_action_execute_ms = execute_profile.output_action_execute_ms;
        profile.execute_output_add_note_payload_ms = execute_profile.output_add_note_payload_ms;
        profile.execute_other_action_execute_ms = execute_profile.other_action_execute_ms;
        profile.execute_record_clues_ms = execute_profile.record_clues_ms;
        profile.execute_apply_ms = execute_profile.apply_ms;
        profile.checktx_candidate_read_wall_ms = execute_profile.candidate_read_wall_ms;
        profile.checktx_candidate_effects_build_ms = execute_profile.read_effects_build_ms;
        profile.checktx_serial_apply_wall_ms = execute_profile.serial_apply_wall_ms;
        profile.checktx_serial_nullifier_insert_ms = execute_profile.serial_nullifier_insert_ms;
        profile.checktx_serial_sct_append_ms = execute_profile.serial_sct_append_ms;
        profile.checktx_serial_event_emit_ms = execute_profile.serial_event_emit_ms;
        profile.checktx_serial_fee_apply_ms = execute_profile.serial_fee_apply_ms;
        profile.execute_ms = execute_profile.begin_state_tx_ms
            + execute_profile.index_tx_ms
            + execute_profile.check_and_execute_ms
            + execute_profile.apply_ms;
    }

    async fn deliver_tx_with_stateless_extraction_caching_profiled(
        &mut self,
        tx_bytes: &[u8],
        cache: &StatelessCache,
        hash: [u8; 32],
    ) -> Result<(Vec<abci::Event>, CheckTxProfile)> {
        let mut profile = CheckTxProfile::default();
        let decode_start = Instant::now();
        let tx = Arc::new(Transaction::decode(tx_bytes).context("decoding transaction")?);
        profile.decode_tx_ms = decode_start.elapsed().as_secs_f64() * 1000.0;
        Self::ensure_user_tx_has_no_internal_actions(&tx)?;
        let supports_fast_path =
            supports_parallel_prepare(tx.as_ref()) && self.checktx_shared_context.is_some();

        if supports_fast_path {
            let context_load_start = Instant::now();
            let historical_context = self
                .checktx_shared_context
                .as_ref()
                .map(|context| Arc::clone(&context.historical_check_context))
                .ok_or_else(|| {
                    anyhow::anyhow!("missing CheckTxSharedContext for fast CheckTx path")
                })?;
            let context_load_ms = context_load_start.elapsed().as_secs_f64() * 1000.0;
            let snapshot = Arc::new(self.committed_snapshot.clone());
            let tx_for_extract = tx.clone();
            let tx_for_read = tx.clone();
            let historical_context_for_read = historical_context.as_ref().clone();
            let handle = tokio::runtime::Handle::current();
            let span = tracing::Span::current();
            let stateless_spawn_started = Instant::now();
            let stateless = tokio::task::spawn_blocking(move || {
                span.in_scope(|| {
                    let queue_wait_ms = stateless_spawn_started.elapsed().as_secs_f64() * 1000.0;

                    let artifact_start = Instant::now();
                    let artifact_result = handle.block_on(async move {
                        Self::build_tx_artifact_extracted_for_stage(
                            "checktx_extract_only",
                            tx_for_extract,
                        )
                        .await
                    });
                    let artifact_blocking_ms = artifact_start.elapsed().as_secs_f64() * 1000.0;

                    (artifact_result, queue_wait_ms, artifact_blocking_ms)
                })
            });
            let prepare_started = Instant::now();
            let prepared = tokio::spawn(
                async move {
                    let prepared = prepare_candidate_read_profiled(
                        tx_for_read,
                        snapshot,
                        historical_context_for_read,
                        false,
                    )
                    .await;
                    let wall_ms = prepare_started.elapsed().as_secs_f64() * 1000.0;
                    (prepared, wall_ms)
                }
                .instrument(tracing::Span::current()),
            );
            let stateless_join_start = Instant::now();
            let (artifact_result, stateless_artifact_queue_wait_ms, stateless_artifact_ms) =
                stateless.await.context("waiting for extraction task")?;
            profile.stateless_task_join_wall_ms =
                stateless_join_start.elapsed().as_secs_f64() * 1000.0;
            profile.stateless_artifact_queue_wait_ms = stateless_artifact_queue_wait_ms;
            profile.stateless_artifact_blocking_total_ms = stateless_artifact_ms;
            profile.stateless_artifact_ms = stateless_artifact_ms;

            let initial_cache_insert_start = Instant::now();
            match &artifact_result {
                Ok((artifact, _)) => cache.insert_extracted(hash, artifact.clone()),
                Err(_) => cache.insert_invalid(hash),
            }
            profile.stateless_initial_cache_insert_ms =
                initial_cache_insert_start.elapsed().as_secs_f64() * 1000.0;

            let (artifact, artifact_profile) = match artifact_result {
                Ok(artifact) => artifact,
                Err(error) => {
                    prepared.abort();
                    return Err(error).context("extract stateless failed");
                }
            };
            profile.stateless_artifact_precheck_ms = artifact_profile.precheck_ms;
            profile.stateless_artifact_action_extract_ms = artifact_profile.action_extract_ms;
            profile.stateless_artifact_action_auth_sig_ms = artifact_profile.action_auth_sig_ms;
            profile.stateless_artifact_action_extract_public_ms =
                artifact_profile.action_extract_public_ms;
            profile.stateless_artifact_action_to_batch_item_ms =
                artifact_profile.action_to_batch_item_ms;
            profile.stateless_artifact_batch_verify_ms = artifact_profile.batch_verify_ms;

            let prepare_join_start = Instant::now();
            let (prepared_result, fast_read_blocking_ms) = prepared
                .await
                .context("waiting for prepare fast checktx candidate task")?;
            let prepare_join_wall_ms = prepare_join_start.elapsed().as_secs_f64() * 1000.0;
            profile.checktx_fast_prepare_join_wall_ms = prepare_join_wall_ms;
            let mut prepared = prepared_result.context("prepare fast checktx candidate failed")?;
            prepared.checktx_fast_context_load_ms = context_load_ms;
            prepared.checktx_fast_read_queue_wait_ms = 0.0;
            prepared.checktx_fast_read_blocking_total_ms = fast_read_blocking_ms;

            let historical_stamp_start = Instant::now();
            let historical_stamp =
                self.current_historical_validation_stamp(Arc::as_ref(&artifact.tx));
            profile.stateless_historical_stamp_ms =
                historical_stamp_start.elapsed().as_secs_f64() * 1000.0;
            let historical_mark_start = Instant::now();
            let artifact = artifact.with_historical_validation_owned(historical_stamp);
            profile.stateless_historical_mark_ms =
                historical_mark_start.elapsed().as_secs_f64() * 1000.0;
            let final_cache_insert_start = Instant::now();
            cache.insert_extracted(hash, artifact.clone());
            profile.stateless_final_cache_insert_ms =
                final_cache_insert_start.elapsed().as_secs_f64() * 1000.0;

            let execute_fast_start = Instant::now();
            let (events, execute_profile) =
                self.apply_prepared_checktx_profiled(tx, prepared).await?;
            profile.checktx_execute_fast_wall_ms =
                execute_fast_start.elapsed().as_secs_f64() * 1000.0;
            profile.check_historical_ms = execute_profile.check_historical_ms;
            Self::fill_checktx_execute_profile(&mut profile, &execute_profile);
            profile.checktx_fast_prepare_join_wall_ms = prepare_join_wall_ms;
            return Ok((events, profile));
        }

        let tx2 = tx.clone();
        let handle = tokio::runtime::Handle::current();
        let span = tracing::Span::current();
        let stateless_spawn_started = Instant::now();
        let stateless = tokio::task::spawn_blocking(move || {
            span.in_scope(|| {
                let queue_wait_ms = stateless_spawn_started.elapsed().as_secs_f64() * 1000.0;
                let start = Instant::now();
                let result = handle.block_on(async move {
                    Self::build_tx_artifact_extracted_for_stage("checktx_extract_only", tx2).await
                });
                let blocking_total_ms = start.elapsed().as_secs_f64() * 1000.0;
                (result, queue_wait_ms, blocking_total_ms)
            })
        });
        let stateful = if supports_fast_path {
            None
        } else {
            let tx2 = tx.clone();
            let state2 = self.state.clone();
            Some(tokio::spawn(
                async move {
                    let start = Instant::now();
                    let result = tx2.check_historical(state2).await;
                    (result, start.elapsed().as_secs_f64() * 1000.0)
                }
                .instrument(tracing::Span::current()),
            ))
        };

        let stateless_join_start = Instant::now();
        let (artifact_result, stateless_artifact_queue_wait_ms, stateless_artifact_ms) =
            stateless.await.context("waiting for extraction task")?;
        profile.stateless_task_join_wall_ms = stateless_join_start.elapsed().as_secs_f64() * 1000.0;
        profile.stateless_artifact_queue_wait_ms = stateless_artifact_queue_wait_ms;
        profile.stateless_artifact_blocking_total_ms = stateless_artifact_ms;
        profile.stateless_artifact_ms = stateless_artifact_ms;
        let initial_cache_insert_start = Instant::now();
        match &artifact_result {
            Ok((artifact, _)) => cache.insert_extracted(hash, artifact.clone()),
            Err(_) => cache.insert_invalid(hash),
        }
        profile.stateless_initial_cache_insert_ms =
            initial_cache_insert_start.elapsed().as_secs_f64() * 1000.0;
        let (artifact, artifact_profile) = artifact_result.context("extract stateless failed")?;
        profile.stateless_artifact_precheck_ms = artifact_profile.precheck_ms;
        profile.stateless_artifact_action_extract_ms = artifact_profile.action_extract_ms;
        profile.stateless_artifact_action_auth_sig_ms = artifact_profile.action_auth_sig_ms;
        profile.stateless_artifact_action_extract_public_ms =
            artifact_profile.action_extract_public_ms;
        profile.stateless_artifact_action_to_batch_item_ms =
            artifact_profile.action_to_batch_item_ms;
        profile.stateless_artifact_batch_verify_ms = artifact_profile.batch_verify_ms;

        let (events, execute_profile) = if supports_fast_path {
            let historical_stamp_start = Instant::now();
            let historical_stamp =
                self.current_historical_validation_stamp(Arc::as_ref(&artifact.tx));
            profile.stateless_historical_stamp_ms =
                historical_stamp_start.elapsed().as_secs_f64() * 1000.0;
            let execute_fast_start = Instant::now();
            let (events, execute_profile) = self.execute_checktx_fast_profiled(tx, false).await?;
            profile.checktx_execute_fast_wall_ms =
                execute_fast_start.elapsed().as_secs_f64() * 1000.0;
            let historical_mark_start = Instant::now();
            let artifact = artifact.with_historical_validation_owned(historical_stamp);
            profile.stateless_historical_mark_ms =
                historical_mark_start.elapsed().as_secs_f64() * 1000.0;
            let final_cache_insert_start = Instant::now();
            cache.insert_extracted(hash, artifact.clone());
            profile.stateless_final_cache_insert_ms =
                final_cache_insert_start.elapsed().as_secs_f64() * 1000.0;
            profile.check_historical_ms = execute_profile.check_historical_ms;
            (events, execute_profile)
        } else {
            let (stateful_result, check_historical_ms) = stateful
                .expect("stateful task is present on legacy path")
                .await
                .context("waiting for check_stateful task")?;
            profile.check_historical_ms = check_historical_ms;
            stateful_result.context("check_stateful failed")?;

            let historical_stamp_start = Instant::now();
            let historical_stamp =
                self.current_historical_validation_stamp(Arc::as_ref(&artifact.tx));
            profile.stateless_historical_stamp_ms =
                historical_stamp_start.elapsed().as_secs_f64() * 1000.0;
            let historical_mark_start = Instant::now();
            let artifact = artifact.with_historical_validation_owned(historical_stamp);
            profile.stateless_historical_mark_ms =
                historical_mark_start.elapsed().as_secs_f64() * 1000.0;
            let final_cache_insert_start = Instant::now();
            cache.insert_extracted(hash, artifact.clone());
            profile.stateless_final_cache_insert_ms =
                final_cache_insert_start.elapsed().as_secs_f64() * 1000.0;

            self.execute_tx_checked_historical_profiled(artifact.tx.clone())
                .await?
        };
        Self::fill_checktx_execute_profile(&mut profile, &execute_profile);
        Ok((events, profile))
    }

    async fn deliver_tx_profiled(
        &mut self,
        tx: Arc<Transaction>,
    ) -> Result<(Vec<abci::Event>, CheckTxProfile)> {
        let mut profile = CheckTxProfile::default();
        if supports_parallel_prepare(tx.as_ref()) && self.checktx_shared_context.is_some() {
            let tx2 = tx.clone();
            let handle = tokio::runtime::Handle::current();
            let span = tracing::Span::current();
            let stateless_spawn_started = Instant::now();
            let stateless = tokio::task::spawn_blocking(move || {
                span.in_scope(|| {
                    let queue_wait_ms = stateless_spawn_started.elapsed().as_secs_f64() * 1000.0;
                    let start = Instant::now();
                    let result = handle.block_on(async move { tx2.check_stateless(()).await });
                    let blocking_total_ms = start.elapsed().as_secs_f64() * 1000.0;
                    (result, queue_wait_ms, blocking_total_ms)
                })
            });

            let stateless_join_start = Instant::now();
            let (stateless_result, stateless_artifact_queue_wait_ms, stateless_artifact_ms) =
                stateless
                    .await
                    .context("waiting for check_stateless check tasks")?;
            profile.stateless_task_join_wall_ms =
                stateless_join_start.elapsed().as_secs_f64() * 1000.0;
            profile.stateless_artifact_queue_wait_ms = stateless_artifact_queue_wait_ms;
            profile.stateless_artifact_blocking_total_ms = stateless_artifact_ms;
            profile.stateless_artifact_ms = stateless_artifact_ms;
            stateless_result.context("check_stateless failed")?;

            let execute_fast_start = Instant::now();
            let (events, execute_profile) = self.execute_checktx_fast_profiled(tx, false).await?;
            profile.checktx_execute_fast_wall_ms =
                execute_fast_start.elapsed().as_secs_f64() * 1000.0;
            profile.check_historical_ms = execute_profile.check_historical_ms;
            Self::fill_checktx_execute_profile(&mut profile, &execute_profile);
            return Ok((events, profile));
        }

        // Both stateful and stateless checks take the transaction as
        // verification context.  The separate clone of the Arc<Transaction>
        // means it can be passed through the whole tree of checks.
        //
        // We spawn tasks for each set of checks, to do CPU-bound stateless checks
        // and I/O-bound stateful checks at the same time.
        let tx2 = tx.clone();
        let handle = tokio::runtime::Handle::current();
        let span = tracing::Span::current();
        let stateless_spawn_started = Instant::now();
        let stateless = tokio::task::spawn_blocking(move || {
            span.in_scope(|| {
                let queue_wait_ms = stateless_spawn_started.elapsed().as_secs_f64() * 1000.0;
                let start = Instant::now();
                let result = handle.block_on(async move { tx2.check_stateless(()).await });
                let blocking_total_ms = start.elapsed().as_secs_f64() * 1000.0;
                (result, queue_wait_ms, blocking_total_ms)
            })
        });
        let tx2 = tx.clone();
        let state2 = self.state.clone();
        let stateful = tokio::spawn(
            async move {
                let start = Instant::now();
                let result = tx2.check_historical(state2).await;
                (result, start.elapsed().as_secs_f64() * 1000.0)
            }
            .instrument(tracing::Span::current()),
        );

        let stateless_join_start = Instant::now();
        let (stateless_result, stateless_artifact_queue_wait_ms, stateless_artifact_ms) = stateless
            .await
            .context("waiting for check_stateless check tasks")?;
        profile.stateless_task_join_wall_ms = stateless_join_start.elapsed().as_secs_f64() * 1000.0;
        profile.stateless_artifact_queue_wait_ms = stateless_artifact_queue_wait_ms;
        profile.stateless_artifact_blocking_total_ms = stateless_artifact_ms;
        profile.stateless_artifact_ms = stateless_artifact_ms;
        stateless_result.context("check_stateless failed")?;
        let (stateful_result, check_historical_ms) =
            stateful.await.context("waiting for check_stateful tasks")?;
        profile.check_historical_ms = check_historical_ms;
        stateful_result.context("check_stateful failed")?;

        let (events, execute_profile) = self.execute_tx_checked_historical_profiled(tx).await?;
        Self::fill_checktx_execute_profile(&mut profile, &execute_profile);
        Ok((events, profile))
    }

    async fn deliver_tx_with_verified_stateless_profiled(
        &mut self,
        tx: Arc<Transaction>,
        historical_context: Option<&HistoricalCheckContext>,
    ) -> Result<(Vec<abci::Event>, VerifiedStatefulTxBreakdown)> {
        let mut profile = VerifiedStatefulTxBreakdown::default();
        let historical_start = Instant::now();
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
        profile.check_historical_ms = historical_start.elapsed().as_secs_f64() * 1000.0;

        let (events, execute_profile) = self.execute_tx_checked_historical_profiled(tx).await?;
        profile.begin_state_tx_ms = execute_profile.begin_state_tx_ms;
        profile.index_tx_ms = execute_profile.index_tx_ms;
        profile.clone_tx_ms = execute_profile.clone_tx_ms;
        profile.proto_convert_ms = execute_profile.proto_convert_ms;
        profile.get_block_height_ms = execute_profile.get_block_height_ms;
        profile.put_block_transaction_ms = execute_profile.put_block_transaction_ms;
        profile.check_and_execute_ms = execute_profile.check_and_execute_ms;
        profile.set_source_ms = execute_profile.set_source_ms;
        profile.pay_fee_ms = execute_profile.pay_fee_ms;
        profile.action_execute_ms = execute_profile.action_execute_ms;
        profile.read_local_precheck_ms = execute_profile.read_local_precheck_ms;
        profile.read_lookup_wait_or_join_ms = execute_profile.read_lookup_wait_or_join_ms;
        profile.read_historical_check_ms = execute_profile.read_historical_check_ms;
        profile.read_nullifier_wait_ms = execute_profile.read_nullifier_wait_ms;
        profile.read_anchor_cache_wait_ms = execute_profile.read_anchor_cache_wait_ms;
        profile.spend_action_execute_ms = execute_profile.spend_action_execute_ms;
        profile.spend_nullifier_check_ms = execute_profile.spend_nullifier_check_ms;
        profile.spend_nullifier_tx_local_scan_ms = execute_profile.spend_nullifier_tx_local_scan_ms;
        profile.spend_nullifier_block_log_lookup_ms =
            execute_profile.spend_nullifier_block_log_lookup_ms;
        profile.spend_nullifier_committed_check_ms =
            execute_profile.spend_nullifier_committed_check_ms;
        profile.spend_nullifier_enqueue_ms = execute_profile.spend_nullifier_enqueue_ms;
        profile.spend_nullifier_stage_ms = execute_profile.spend_nullifier_stage_ms;
        profile.spend_nullifier_merge_ms = execute_profile.spend_nullifier_merge_ms;
        profile.nullifier_lookup_count = execute_profile.nullifier_lookup_count;
        profile.output_action_execute_ms = execute_profile.output_action_execute_ms;
        profile.output_add_note_payload_ms = execute_profile.output_add_note_payload_ms;
        profile.read_anchor_validation_ms = execute_profile.read_anchor_validation_ms;
        profile.read_committed_nullifier_ms = execute_profile.read_committed_nullifier_ms;
        profile.read_effects_build_ms = execute_profile.read_effects_build_ms;
        profile.other_action_execute_ms = execute_profile.other_action_execute_ms;
        profile.record_clues_ms = execute_profile.record_clues_ms;
        profile.apply_ms = execute_profile.apply_ms;

        Ok((events, profile))
    }

    async fn execute_prepare_candidate_profiled(
        &mut self,
        tx: Arc<Transaction>,
        artifact: Option<&TxArtifact>,
        historical_context: &HistoricalCheckContext,
    ) -> Result<(Vec<abci::Event>, VerifiedStatefulTxBreakdown)> {
        if artifact.is_some_and(|artifact| {
            artifact.has_matching_historical_validation(self.snapshot_version)
        }) {
            return self.execute_tx_checked_historical_profiled(tx).await;
        }

        self.deliver_tx_with_verified_stateless_profiled(tx, Some(historical_context))
            .await
    }

    async fn execute_checktx_fast_profiled(
        &mut self,
        tx: Arc<Transaction>,
        skip_historical: bool,
    ) -> Result<(Vec<abci::Event>, VerifiedStatefulTxBreakdown)> {
        let context_load_start = Instant::now();
        let historical_context = self
            .checktx_shared_context
            .as_ref()
            .map(|context| Arc::clone(&context.historical_check_context))
            .ok_or_else(|| anyhow::anyhow!("missing CheckTxSharedContext for fast CheckTx path"))?;
        let context_load_ms = context_load_start.elapsed().as_secs_f64() * 1000.0;
        let snapshot = self.committed_snapshot.clone();
        let tx_for_read = tx.clone();
        let historical_context_for_read = historical_context.as_ref().clone();
        let handle = tokio::runtime::Handle::current();
        let prepare_join_start = Instant::now();
        let blocking_started = Instant::now();
        let prepared = tokio::task::spawn_blocking(move || {
            let queue_wait_ms = blocking_started.elapsed().as_secs_f64() * 1000.0;
            let read_started = Instant::now();
            let prepared = prepare_candidate_read_blocking_profiled(
                tx_for_read,
                snapshot,
                historical_context_for_read,
                skip_historical,
                handle,
            );
            let blocking_total_ms = read_started.elapsed().as_secs_f64() * 1000.0;
            (prepared, queue_wait_ms, blocking_total_ms)
        })
        .await
        .map_err(|e| anyhow::anyhow!("joining fast CheckTx prepare task: {e}"))?;
        let prepare_join_wall_ms = prepare_join_start.elapsed().as_secs_f64() * 1000.0;
        let (prepared, queue_wait_ms, blocking_total_ms) = prepared;
        let mut prepared = prepared?;

        prepared.checktx_fast_context_load_ms = context_load_ms;
        prepared.checktx_fast_read_queue_wait_ms = queue_wait_ms;
        prepared.checktx_fast_read_blocking_total_ms = blocking_total_ms;
        let apply_start = Instant::now();
        let result = self.apply_prepared_checktx_profiled(tx, prepared).await;
        let apply_wall_ms = apply_start.elapsed().as_secs_f64() * 1000.0;
        let (events, mut profile) = result?;
        profile.checktx_fast_prepare_join_wall_ms = prepare_join_wall_ms;
        profile.checktx_fast_apply_wall_ms = apply_wall_ms;

        Ok((events, profile))
    }

    async fn apply_prepared_checktx_profiled(
        &mut self,
        tx: Arc<Transaction>,
        prepared: PreparedCandidateRead,
    ) -> Result<(Vec<abci::Event>, VerifiedStatefulTxBreakdown)> {
        let serial_apply_start = Instant::now();
        let mut profile = VerifiedStatefulTxBreakdown::default();
        profile.check_historical_ms = prepared.check_historical_ms;
        profile.checktx_fast_context_load_ms = prepared.checktx_fast_context_load_ms;
        profile.checktx_fast_read_queue_wait_ms = prepared.checktx_fast_read_queue_wait_ms;
        profile.checktx_fast_read_blocking_total_ms = prepared.checktx_fast_read_blocking_total_ms;
        profile.action_execute_ms = prepared.execution_profile.action_execute_ms;
        profile.read_local_precheck_ms = prepared.execution_profile.read_local_precheck_ms;
        profile.read_lookup_wait_or_join_ms =
            prepared.execution_profile.read_lookup_wait_or_join_ms;
        profile.read_historical_check_ms = prepared.execution_profile.read_historical_check_ms;
        profile.read_nullifier_wait_ms = prepared.execution_profile.read_nullifier_wait_ms;
        profile.read_anchor_cache_wait_ms = prepared.execution_profile.read_anchor_cache_wait_ms;
        profile.read_anchor_validation_ms = prepared.execution_profile.read_anchor_validation_ms;
        profile.read_committed_nullifier_ms =
            prepared.execution_profile.read_committed_nullifier_ms;
        profile.read_effects_build_ms = prepared.execution_profile.read_effects_build_ms;
        profile.candidate_read_wall_ms = prepared.read_wall_ms;
        profile.nullifier_lookup_count = prepared.execution_profile.nullifier_lookup_count;
        profile.spend_action_execute_ms = prepared.execution_profile.spend_action_execute_ms;
        profile.spend_nullifier_check_ms = prepared.execution_profile.spend_nullifier_check_ms;
        profile.spend_nullifier_committed_check_ms = prepared
            .execution_profile
            .spend_nullifier_committed_check_ms;
        profile.output_action_execute_ms = prepared.execution_profile.output_action_execute_ms;
        profile.output_add_note_payload_ms = prepared.execution_profile.output_add_note_payload_ms;
        profile.record_clues_ms = prepared.execution_profile.record_clues_ms;

        let begin_state_tx_start = Instant::now();
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");
        profile.begin_state_tx_ms = begin_state_tx_start.elapsed().as_secs_f64() * 1000.0;

        let index_start = Instant::now();
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let get_block_height_start = Instant::now();
                let height = state_tx.get_block_height().await?;
                profile.get_block_height_ms =
                    get_block_height_start.elapsed().as_secs_f64() * 1000.0;
                let clone_tx_start = Instant::now();
                let transaction = Arc::as_ref(&tx).clone();
                profile.clone_tx_ms = clone_tx_start.elapsed().as_secs_f64() * 1000.0;
                let proto_convert_start = Instant::now();
                let proto_transaction = transaction.into();
                profile.proto_convert_ms = proto_convert_start.elapsed().as_secs_f64() * 1000.0;
                let put_block_transaction_start = Instant::now();
                let index_write_profile = Self::append_block_transaction_to_state(
                    &mut state_tx,
                    height,
                    proto_transaction,
                )
                .await
                .context("storing transactions")?;
                profile.put_block_transaction_ms =
                    put_block_transaction_start.elapsed().as_secs_f64() * 1000.0;
                profile.tx_log_read_ms = index_write_profile.tx_log_read_ms;
                profile.tx_log_encode_ms = index_write_profile.tx_log_encode_ms;
                profile.tx_log_put_raw_ms = index_write_profile.tx_log_put_raw_ms;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let get_block_height_start = Instant::now();
                let _height = state_tx.get_block_height().await?;
                profile.get_block_height_ms =
                    get_block_height_start.elapsed().as_secs_f64() * 1000.0;
                let clone_tx_start = Instant::now();
                let transaction = Arc::as_ref(&tx).clone();
                profile.clone_tx_ms = clone_tx_start.elapsed().as_secs_f64() * 1000.0;
                let proto_convert_start = Instant::now();
                let proto_transaction = transaction.into();
                profile.proto_convert_ms = proto_convert_start.elapsed().as_secs_f64() * 1000.0;
                let put_block_transaction_start = Instant::now();
                self.deferred_block_transactions.push(proto_transaction);
                profile.put_block_transaction_ms =
                    put_block_transaction_start.elapsed().as_secs_f64() * 1000.0;
            }
        }
        profile.index_tx_ms = index_start.elapsed().as_secs_f64() * 1000.0;

        let check_and_execute_start = Instant::now();
        let tx_id = tx.id();
        let set_source_start = Instant::now();
        state_tx.put_current_source(Some(tx_id.clone()));
        profile.set_source_ms = set_source_start.elapsed().as_secs_f64() * 1000.0;

        let pay_fee_start = Instant::now();
        let gas_used = tx.gas_cost();
        let fee = tx.transaction_body.transaction_parameters.fee;
        if let Some(context) = self.checktx_shared_context.as_ref() {
            Self::apply_checktx_fee_with_context(&mut state_tx, gas_used, fee, context)?;
        } else {
            state_tx.pay_fee(gas_used, fee).await?;
        }
        let pay_fee_ms = pay_fee_start.elapsed().as_secs_f64() * 1000.0;
        profile.pay_fee_ms = pay_fee_ms;
        profile.serial_fee_apply_ms = pay_fee_ms;

        // CheckTx runs against an ephemeral per-transaction app fork. For the
        // supported fast path, committed-state nullifier checks have already
        // run in the read phase, and same-block conflict resolution is a
        // proposer/block concern. However, the fast path still builds an app
        // fork with concrete state for downstream consumers and tests, so the
        // fork should reflect the same semantic spend set as the slow path.
        let nullifier_insert_start = Instant::now();
        state_tx
            .nullify_all(&prepared.effects.spend_nullifiers, tx_id.clone().into())
            .await?;
        profile.serial_nullifier_insert_ms =
            nullifier_insert_start.elapsed().as_secs_f64() * 1000.0;

        for nullifier in &prepared.effects.spend_nullifiers {
            let event_emit_start = Instant::now();
            state_tx.record_proto(
                penumbra_sdk_shielded_pool::event::EventNullifierSpent {
                    nullifier: *nullifier,
                }
                .to_proto(),
            );
            let event_emit_ms = event_emit_start.elapsed().as_secs_f64() * 1000.0;
            profile.serial_event_emit_ms += event_emit_ms;
            profile.spend_nullifier_enqueue_ms += event_emit_ms;
            profile.spend_action_execute_ms += event_emit_ms;
        }

        for payload in &prepared.effects.sct_payloads {
            if let StatePayload::Note { note, .. } = payload {
                let event_emit_start = Instant::now();
                state_tx.record_proto(
                    penumbra_sdk_shielded_pool::event::EventNoteCreated {
                        note_commitment: note.note_commitment,
                    }
                    .to_proto(),
                );
                profile.serial_event_emit_ms += event_emit_start.elapsed().as_secs_f64() * 1000.0;
            }
        }

        // CheckTx runs against an ephemeral fork, so clue bookkeeping is dead work:
        // it only feeds block-level FMD counters and events, and the fork is discarded.
        profile.record_clues_ms = 0.0;

        let sct_append_start = Instant::now();
        if let Some(context) = self.checktx_shared_context.as_ref() {
            let base_position_u64: u64 = context.sct_base_position.into();
            for (offset, payload) in prepared.effects.sct_payloads.iter().enumerate() {
                let commitment_event_start = Instant::now();
                let position = penumbra_sdk_tct::Position::from(base_position_u64 + offset as u64);
                state_tx.record_proto(penumbra_sdk_sct::event::commitment(
                    *payload.commitment(),
                    position,
                    payload.source().clone(),
                ));
                profile.serial_event_emit_ms +=
                    commitment_event_start.elapsed().as_secs_f64() * 1000.0;
            }
        } else {
            let positioned_sct_payloads = self
                .pending_sct_append_log
                .reserve_positions(&state_tx, prepared.effects.sct_payloads.clone())
                .await
                .context("reserving deferred SCT positions")?;
            for (position, payload) in &positioned_sct_payloads {
                let commitment_event_start = Instant::now();
                state_tx.record_proto(penumbra_sdk_sct::event::commitment(
                    *payload.commitment(),
                    *position,
                    payload.source().clone(),
                ));
                profile.serial_event_emit_ms +=
                    commitment_event_start.elapsed().as_secs_f64() * 1000.0;
            }
            self.pending_sct_append_log
                .append_positioned(positioned_sct_payloads);
        }
        profile.serial_sct_append_ms = sct_append_start.elapsed().as_secs_f64() * 1000.0;

        profile.check_and_execute_ms = prepared.execution_profile.action_execute_ms
            + check_and_execute_start.elapsed().as_secs_f64() * 1000.0;

        let apply_start = Instant::now();
        let events = state_tx.apply().1;
        profile.apply_ms = apply_start.elapsed().as_secs_f64() * 1000.0;
        profile.serial_apply_wall_ms = serial_apply_start.elapsed().as_secs_f64() * 1000.0;

        Ok((events, profile))
    }

    async fn apply_prepared_prepare_candidate_profiled(
        &mut self,
        tx: Arc<Transaction>,
        prepared: PreparedCandidateRead,
        block_state: &mut PrepareBlockLocalState,
    ) -> Result<(Vec<abci::Event>, VerifiedStatefulTxBreakdown)> {
        let serial_apply_start = Instant::now();
        let conflict_check_start = Instant::now();
        for nullifier in &prepared.effects.spend_nullifiers {
            anyhow::ensure!(
                !block_state.seen_nullifiers.contains(nullifier),
                "nullifier {} already spent earlier in this proposal",
                nullifier
            );
        }
        let serial_same_block_conflict_ms = conflict_check_start.elapsed().as_secs_f64() * 1000.0;
        // Prepared candidate reads only consult committed state, so they intentionally
        // remain blind to same-block conflicts. Serial apply is the sole resolver for
        // duplicate nullifiers within a single proposal.

        let mut profile = VerifiedStatefulTxBreakdown::default();
        profile.check_historical_ms = prepared.check_historical_ms;
        profile.action_execute_ms = prepared.execution_profile.action_execute_ms;
        profile.read_local_precheck_ms = prepared.execution_profile.read_local_precheck_ms;
        profile.read_lookup_wait_or_join_ms =
            prepared.execution_profile.read_lookup_wait_or_join_ms;
        profile.read_historical_check_ms = prepared.execution_profile.read_historical_check_ms;
        profile.read_nullifier_wait_ms = prepared.execution_profile.read_nullifier_wait_ms;
        profile.read_anchor_cache_wait_ms = prepared.execution_profile.read_anchor_cache_wait_ms;
        profile.read_anchor_validation_ms = prepared.execution_profile.read_anchor_validation_ms;
        profile.read_committed_nullifier_ms =
            prepared.execution_profile.read_committed_nullifier_ms;
        profile.read_effects_build_ms = prepared.execution_profile.read_effects_build_ms;
        profile.serial_same_block_conflict_ms = serial_same_block_conflict_ms;
        profile.nullifier_lookup_count = prepared.execution_profile.nullifier_lookup_count;
        profile.spend_action_execute_ms = prepared.execution_profile.spend_action_execute_ms;
        profile.spend_nullifier_check_ms = prepared.execution_profile.spend_nullifier_check_ms;
        profile.spend_nullifier_committed_check_ms = prepared
            .execution_profile
            .spend_nullifier_committed_check_ms;
        profile.output_action_execute_ms = prepared.execution_profile.output_action_execute_ms;
        profile.output_add_note_payload_ms = prepared.execution_profile.output_add_note_payload_ms;
        profile.record_clues_ms = prepared.execution_profile.record_clues_ms;

        let begin_state_tx_start = Instant::now();
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");
        profile.begin_state_tx_ms = begin_state_tx_start.elapsed().as_secs_f64() * 1000.0;

        let index_start = Instant::now();
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let get_block_height_start = Instant::now();
                let height = state_tx.get_block_height().await?;
                profile.get_block_height_ms =
                    get_block_height_start.elapsed().as_secs_f64() * 1000.0;
                let clone_tx_start = Instant::now();
                let transaction = Arc::as_ref(&tx).clone();
                profile.clone_tx_ms = clone_tx_start.elapsed().as_secs_f64() * 1000.0;
                let proto_convert_start = Instant::now();
                let proto_transaction = transaction.into();
                profile.proto_convert_ms = proto_convert_start.elapsed().as_secs_f64() * 1000.0;
                let put_block_transaction_start = Instant::now();
                let index_write_profile = Self::append_block_transaction_to_state(
                    &mut state_tx,
                    height,
                    proto_transaction,
                )
                .await
                .context("storing transactions")?;
                profile.put_block_transaction_ms =
                    put_block_transaction_start.elapsed().as_secs_f64() * 1000.0;
                profile.tx_log_read_ms = index_write_profile.tx_log_read_ms;
                profile.tx_log_encode_ms = index_write_profile.tx_log_encode_ms;
                profile.tx_log_put_raw_ms = index_write_profile.tx_log_put_raw_ms;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let get_block_height_start = Instant::now();
                let _height = state_tx.get_block_height().await?;
                profile.get_block_height_ms =
                    get_block_height_start.elapsed().as_secs_f64() * 1000.0;
                let clone_tx_start = Instant::now();
                let transaction = Arc::as_ref(&tx).clone();
                profile.clone_tx_ms = clone_tx_start.elapsed().as_secs_f64() * 1000.0;
                let proto_convert_start = Instant::now();
                let proto_transaction = transaction.into();
                profile.proto_convert_ms = proto_convert_start.elapsed().as_secs_f64() * 1000.0;
                let put_block_transaction_start = Instant::now();
                self.deferred_block_transactions.push(proto_transaction);
                profile.put_block_transaction_ms =
                    put_block_transaction_start.elapsed().as_secs_f64() * 1000.0;
            }
        }
        profile.index_tx_ms = index_start.elapsed().as_secs_f64() * 1000.0;

        let check_and_execute_start = Instant::now();
        let tx_id = tx.id();
        let set_source_start = Instant::now();
        state_tx.put_current_source(Some(tx_id.clone()));
        profile.set_source_ms = set_source_start.elapsed().as_secs_f64() * 1000.0;

        let pay_fee_start = Instant::now();
        let gas_used = tx.gas_cost();
        let fee = tx.transaction_body.transaction_parameters.fee;
        state_tx.pay_fee(gas_used, fee).await?;
        let pay_fee_ms = pay_fee_start.elapsed().as_secs_f64() * 1000.0;
        profile.pay_fee_ms = pay_fee_ms;
        profile.serial_fee_apply_ms = pay_fee_ms;

        let source: CommitmentSource = tx_id.clone().into();

        for nullifier in &prepared.effects.spend_nullifiers {
            let event_emit_start = Instant::now();
            state_tx.record_proto(
                penumbra_sdk_shielded_pool::event::EventNullifierSpent {
                    nullifier: *nullifier,
                }
                .to_proto(),
            );
            let event_emit_ms = event_emit_start.elapsed().as_secs_f64() * 1000.0;
            profile.serial_event_emit_ms += event_emit_ms;
            profile.spend_nullifier_enqueue_ms += event_emit_ms;
            profile.spend_action_execute_ms += event_emit_ms;
        }

        for payload in &prepared.effects.sct_payloads {
            if let StatePayload::Note { note, .. } = payload {
                let event_emit_start = Instant::now();
                state_tx.record_proto(
                    penumbra_sdk_shielded_pool::event::EventNoteCreated {
                        note_commitment: note.note_commitment,
                    }
                    .to_proto(),
                );
                profile.serial_event_emit_ms += event_emit_start.elapsed().as_secs_f64() * 1000.0;
            }
        }

        let record_clues_start = Instant::now();
        state_tx.put_current_source(None);
        for clue in tx
            .transaction_body
            .detection_data
            .iter()
            .flat_map(|x| x.fmd_clues.iter())
        {
            state_tx.record_clue(clue.clone(), tx_id.clone()).await?;
        }
        profile.record_clues_ms += record_clues_start.elapsed().as_secs_f64() * 1000.0;

        let sct_append_start = Instant::now();
        let positioned_sct_payloads = self
            .pending_sct_append_log
            .reserve_positions(&state_tx, prepared.effects.sct_payloads.clone())
            .await
            .context("reserving deferred SCT positions")?;
        for (position, payload) in &positioned_sct_payloads {
            let commitment_event_start = Instant::now();
            state_tx.record_proto(penumbra_sdk_sct::event::commitment(
                *payload.commitment(),
                *position,
                payload.source().clone(),
            ));
            profile.serial_event_emit_ms += commitment_event_start.elapsed().as_secs_f64() * 1000.0;
        }
        self.pending_sct_append_log
            .append_positioned(positioned_sct_payloads);
        profile.serial_sct_append_ms = sct_append_start.elapsed().as_secs_f64() * 1000.0;

        profile.check_and_execute_ms = prepared.execution_profile.action_execute_ms
            + check_and_execute_start.elapsed().as_secs_f64() * 1000.0;

        let apply_start = Instant::now();
        let events = state_tx.apply().1;
        profile.apply_ms = apply_start.elapsed().as_secs_f64() * 1000.0;
        profile.serial_apply_wall_ms = serial_apply_start.elapsed().as_secs_f64() * 1000.0;

        block_state.staged_nullifiers.extend(
            prepared
                .effects
                .spend_nullifiers
                .iter()
                .copied()
                .map(|nullifier| (nullifier, source.clone())),
        );
        block_state
            .seen_nullifiers
            .extend(prepared.effects.spend_nullifiers.iter().copied());

        Ok((events, profile))
    }

    async fn execute_prepare_candidates_parallel_profiled(
        &mut self,
        deduped: Vec<Candidate>,
        historical_context: HistoricalCheckContext,
        profile: &mut PrepareProposalProfile,
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
        let mut max_inflight = 0usize;
        let parallel_read_start = Instant::now();
        let mut dispatch_overhead_ms = 0.0f64;
        let mut prepared_results = std::iter::repeat_with(|| None)
            .take(deduped.len())
            .collect::<Vec<_>>();

        while next_to_spawn < deduped.len() || !tasks.is_empty() {
            while next_to_spawn < deduped.len() && tasks.len() < concurrency {
                let dispatch_start = Instant::now();
                let tx = deduped[next_to_spawn].tx().clone();
                let snapshot = snapshot.clone();
                let context = historical_context.clone();
                let handle = tokio::runtime::Handle::current();
                let skip_historical = deduped[next_to_spawn].artifact().is_some_and(|artifact| {
                    artifact.has_matching_historical_validation(self.snapshot_version)
                });
                let index = next_to_spawn;
                tasks.spawn_blocking(move || {
                    let result = prepare_candidate_read_blocking_profiled(
                        tx,
                        Arc::as_ref(&snapshot).clone(),
                        context,
                        skip_historical,
                        handle,
                    );
                    (index, result)
                });
                dispatch_overhead_ms += dispatch_start.elapsed().as_secs_f64() * 1000.0;
                next_to_spawn += 1;
                max_inflight = max_inflight.max(tasks.len());
            }

            if let Some(joined) = tasks.join_next().await {
                let dispatch_start = Instant::now();
                let (index, result) = match joined {
                    Ok(result) => result,
                    Err(error) => {
                        tracing::warn!(?error, "parallel prepare candidate task failed");
                        return Ok(Vec::new());
                    }
                };
                prepared_results[index] = Some(result);
                dispatch_overhead_ms += dispatch_start.elapsed().as_secs_f64() * 1000.0;
            }
        }

        profile.stateful_filter_parallel_read_wall_ms =
            parallel_read_start.elapsed().as_secs_f64() * 1000.0;
        profile.stateful_filter_candidate_read_wall_ms =
            profile.stateful_filter_parallel_read_wall_ms;
        profile.stateful_filter_read_task_dispatch_ms += dispatch_overhead_ms;
        profile.stateful_filter_parallel_read_tasks_spawned = prepared_results.len();
        profile.stateful_filter_parallel_read_max_inflight = max_inflight;
        profile.stateful_filter_parallel_read_configured_concurrency = concurrency;
        let (anchor_hits, anchor_misses, anchor_unique_pairs) =
            historical_context.anchor_cache.stats();
        profile.stateful_filter_anchor_cache_hits = anchor_hits;
        profile.stateful_filter_anchor_cache_misses = anchor_misses;
        profile.stateful_filter_anchor_unique_pairs = anchor_unique_pairs;
        let (claimed_anchor_hits, claimed_anchor_misses, claimed_anchor_unique_values) =
            historical_context.claimed_anchor_cache.stats();
        profile.stateful_filter_claimed_anchor_cache_hits = claimed_anchor_hits;
        profile.stateful_filter_claimed_anchor_cache_misses = claimed_anchor_misses;
        profile.stateful_filter_claimed_anchor_unique_values = claimed_anchor_unique_values;

        let mut block_state = PrepareBlockLocalState::default();
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
            let read_wall_ms = prepared.read_wall_ms;

            match self
                .apply_prepared_prepare_candidate_profiled(
                    candidate.tx().clone(),
                    prepared,
                    &mut block_state,
                )
                .await
            {
                Ok((_, execution_profile)) => {
                    Self::accumulate_prepare_candidate_profile(profile, &execution_profile);
                    profile.stateful_filter_parallel_read_sum_candidate_ms += read_wall_ms;
                    if execution_profile.check_historical_ms == 0.0 {
                        profile.historical_validation_reuse_count += 1;
                    }
                    included_candidates.push(candidate);
                }
                Err(error) => {
                    tracing::debug!(?error, "serial apply rejected prepared candidate");
                }
            }
        }

        self.apply_prepare_proposal_nullifier_batch_profiled(
            &block_state.staged_nullifiers,
            profile,
        )
        .await?;

        Ok(included_candidates)
    }

    async fn apply_prepare_proposal_nullifier_batch_profiled(
        &mut self,
        entries: &[(Nullifier, CommitmentSource)],
        profile: &mut PrepareProposalProfile,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let serial_apply_start = Instant::now();
        let begin_state_tx_start = Instant::now();
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");
        let begin_state_tx_ms = begin_state_tx_start.elapsed().as_secs_f64() * 1000.0;

        let insert_start = Instant::now();
        let batch_profile = state_tx.nullify_proposal_batch(entries).await?;
        let insert_total_ms = insert_start.elapsed().as_secs_f64() * 1000.0;

        let apply_start = Instant::now();
        let _events = state_tx.apply().1;
        let apply_ms = apply_start.elapsed().as_secs_f64() * 1000.0;

        profile.stateful_filter_begin_state_tx_ms += begin_state_tx_ms;
        profile.stateful_filter_serial_apply_wall_ms +=
            serial_apply_start.elapsed().as_secs_f64() * 1000.0;
        profile.stateful_filter_serial_nullifier_insert_ms += insert_total_ms;
        profile.stateful_filter_proposal_nullifier_lookup_write_ms += batch_profile.lookup_write_ms;
        profile.stateful_filter_proposal_pending_nullifier_stage_ms +=
            batch_profile.pending_stage_ms;
        profile.stateful_filter_serial_state_delta_apply_ms += apply_ms;
        profile.stateful_filter_apply_ms += apply_ms;
        Ok(())
    }

    async fn append_block_transaction_to_state<S>(
        state_tx: &mut S,
        height: u64,
        transaction: penumbra_sdk_proto::core::transaction::v1::Transaction,
    ) -> Result<BlockTxIndexWriteProfile>
    where
        S: StateWrite + StateReadExt,
    {
        let mut profile = BlockTxIndexWriteProfile::default();
        let tx_log_read_start = Instant::now();
        let mut transactions_response = state_tx.transactions_by_height(height).await?;
        profile.tx_log_read_ms = tx_log_read_start.elapsed().as_secs_f64() * 1000.0;
        transactions_response.transactions.push(transaction);
        let tx_log_encode_start = Instant::now();
        let encoded = transactions_response.encode_to_vec();
        profile.tx_log_encode_ms = tx_log_encode_start.elapsed().as_secs_f64() * 1000.0;
        let tx_log_put_raw_start = Instant::now();
        state_tx.nonverifiable_put_raw(
            state_key::cometbft_data::transactions_by_height(height).into(),
            encoded,
        );
        profile.tx_log_put_raw_ms = tx_log_put_raw_start.elapsed().as_secs_f64() * 1000.0;
        Ok(profile)
    }

    async fn materialize_pending_sct_append_log<S>(&mut self, state_tx: &mut S) -> Result<()>
    where
        S: StateWrite
            + penumbra_sdk_sct::component::tree::SctManager
            + penumbra_sdk_shielded_pool::component::NoteManager,
    {
        let entries = self.pending_sct_append_log.take_entries();
        if entries.is_empty() {
            return Ok(());
        }

        let mut note_payloads = state_tx.pending_note_payloads();
        let mut rolled_up_payloads = state_tx.pending_rolled_up_payloads();
        let mut last_position = None;

        for (position, payload) in entries {
            debug_assert!(
                last_position
                    .map(|previous| previous <= position)
                    .unwrap_or(true),
                "deferred SCT append log should already be position-sorted"
            );
            last_position = Some(position);

            state_tx
                .add_sct_commitment_at_position(*payload.commitment(), position)
                .await?;

            match payload {
                StatePayload::Note { source, note } => {
                    note_payloads.push_back((position, *note, source));
                }
                StatePayload::RolledUp { commitment, .. } => {
                    rolled_up_payloads.push_back((position, commitment));
                }
            }
        }

        state_tx.object_put(
            penumbra_sdk_shielded_pool::state_key::pending_notes(),
            note_payloads,
        );
        state_tx.object_put(
            penumbra_sdk_shielded_pool::state_key::pending_rolled_up_payloads(),
            rolled_up_payloads,
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

    async fn execute_tx_checked_historical_profiled(
        &mut self,
        tx: Arc<Transaction>,
    ) -> Result<(Vec<abci::Event>, VerifiedStatefulTxBreakdown)> {
        let mut profile = VerifiedStatefulTxBreakdown::default();
        // At this point, the stateful checks should have completed,
        // leaving us with exclusive access to the Arc<State>.
        let begin_state_tx_start = Instant::now();
        let tx_id = tx.id();
        let state_arc_strong_count = Arc::strong_count(&self.state);
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "legacy CheckTx could not begin state transaction after historical checks: tx_id={}, action_count={}, state_arc_strong_count={}",
                    tx_id,
                    tx.actions().count(),
                    state_arc_strong_count,
                )
            })?;
        profile.begin_state_tx_ms = begin_state_tx_start.elapsed().as_secs_f64() * 1000.0;

        // Index the transaction:
        let index_start = Instant::now();
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let get_block_height_start = Instant::now();
                let height = state_tx.get_block_height().await?;
                profile.get_block_height_ms =
                    get_block_height_start.elapsed().as_secs_f64() * 1000.0;
                let clone_tx_start = Instant::now();
                let transaction = Arc::as_ref(&tx).clone();
                profile.clone_tx_ms = clone_tx_start.elapsed().as_secs_f64() * 1000.0;
                let proto_convert_start = Instant::now();
                let proto_transaction = transaction.into();
                profile.proto_convert_ms = proto_convert_start.elapsed().as_secs_f64() * 1000.0;
                let put_block_transaction_start = Instant::now();
                let index_write_profile = Self::append_block_transaction_to_state(
                    &mut state_tx,
                    height,
                    proto_transaction,
                )
                .await
                .context("storing transactions")?;
                profile.put_block_transaction_ms =
                    put_block_transaction_start.elapsed().as_secs_f64() * 1000.0;
                profile.tx_log_read_ms = index_write_profile.tx_log_read_ms;
                profile.tx_log_encode_ms = index_write_profile.tx_log_encode_ms;
                profile.tx_log_put_raw_ms = index_write_profile.tx_log_put_raw_ms;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let get_block_height_start = Instant::now();
                let _height = state_tx.get_block_height().await?;
                profile.get_block_height_ms =
                    get_block_height_start.elapsed().as_secs_f64() * 1000.0;
                let clone_tx_start = Instant::now();
                let transaction = Arc::as_ref(&tx).clone();
                profile.clone_tx_ms = clone_tx_start.elapsed().as_secs_f64() * 1000.0;
                let proto_convert_start = Instant::now();
                let proto_transaction = transaction.into();
                profile.proto_convert_ms = proto_convert_start.elapsed().as_secs_f64() * 1000.0;
                let put_block_transaction_start = Instant::now();
                self.deferred_block_transactions.push(proto_transaction);
                profile.put_block_transaction_ms =
                    put_block_transaction_start.elapsed().as_secs_f64() * 1000.0;
            }
        }
        profile.index_tx_ms = index_start.elapsed().as_secs_f64() * 1000.0;

        let check_and_execute_start = Instant::now();
        let (execution_profile, execution_effects) =
            check_and_execute_profiled(Arc::as_ref(&tx), &mut state_tx, false)
                .await
                .context("executing transaction")?;
        profile.check_and_execute_ms = check_and_execute_start.elapsed().as_secs_f64() * 1000.0;
        profile.set_source_ms = execution_profile.set_source_ms;
        profile.pay_fee_ms = execution_profile.pay_fee_ms;
        profile.action_execute_ms = execution_profile.action_execute_ms;
        profile.read_local_precheck_ms = execution_profile.read_local_precheck_ms;
        profile.read_lookup_wait_or_join_ms = execution_profile.read_lookup_wait_or_join_ms;
        profile.read_historical_check_ms = execution_profile.read_historical_check_ms;
        profile.read_nullifier_wait_ms = execution_profile.read_nullifier_wait_ms;
        profile.read_anchor_cache_wait_ms = execution_profile.read_anchor_cache_wait_ms;
        profile.spend_action_execute_ms = execution_profile.spend_action_execute_ms;
        profile.spend_nullifier_check_ms = execution_profile.spend_nullifier_check_ms;
        profile.spend_nullifier_tx_local_scan_ms =
            execution_profile.spend_nullifier_tx_local_scan_ms;
        profile.spend_nullifier_block_log_lookup_ms =
            execution_profile.spend_nullifier_block_log_lookup_ms;
        profile.spend_nullifier_committed_check_ms =
            execution_profile.spend_nullifier_committed_check_ms;
        profile.spend_nullifier_enqueue_ms = execution_profile.spend_nullifier_enqueue_ms;
        profile.spend_nullifier_stage_ms = execution_profile.spend_nullifier_stage_ms;
        profile.spend_nullifier_merge_ms = execution_profile.spend_nullifier_merge_ms;
        profile.nullifier_lookup_count = execution_profile.nullifier_lookup_count;
        profile.output_action_execute_ms = execution_profile.output_action_execute_ms;
        profile.output_add_note_payload_ms = execution_profile.output_add_note_payload_ms;
        profile.other_action_execute_ms = execution_profile.other_action_execute_ms;
        profile.record_clues_ms = execution_profile.record_clues_ms;

        let positioned_sct_payloads = self
            .pending_sct_append_log
            .reserve_positions(&state_tx, execution_effects.sct_payloads)
            .await
            .context("reserving deferred SCT positions")?;
        for (position, payload) in &positioned_sct_payloads {
            state_tx.record_proto(penumbra_sdk_sct::event::commitment(
                *payload.commitment(),
                *position,
                payload.source().clone(),
            ));
        }
        self.pending_sct_append_log
            .append_positioned(positioned_sct_payloads);

        // At this point, we've completed execution successfully with no errors,
        // so we can apply the transaction to the State. Otherwise, we'd have
        // bubbled up an error and dropped the StateTransaction.
        let apply_start = Instant::now();
        let events = state_tx.apply().1;
        profile.apply_ms = apply_start.elapsed().as_secs_f64() * 1000.0;

        Ok((events, profile))
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
        Governance::end_block(&mut arc_state_tx, end_block).await;
        Staking::end_block(&mut arc_state_tx, end_block).await;
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

        // If a chain upgrade is scheduled for the next block, we trigger an early epoch change
        // so that the upgraded chain starts at a clean epoch boundary.
        let is_chain_upgrade = state_tx
            .is_pre_upgrade_height()
            .await
            .expect("able to detect upgrade heights");

        if is_end_epoch || is_chain_upgrade {
            tracing::info!(%is_end_epoch, %is_chain_upgrade, ?current_height, "ending epoch");

            let mut arc_state_tx = Arc::new(state_tx);

            Sct::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Sct component");
            Ibc::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on IBC component");
            Governance::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Governance component");
            ShieldedPool::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on shielded pool component");
            Staking::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Staking component");
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
            penumbra_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
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
            penumbra_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
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
        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions before commit");
        // We need to extract the State we've built up to commit it.  Fill in a dummy state.
        let dummy_state = StateDelta::new(storage.latest_snapshot());
        let mut state = Arc::try_unwrap(std::mem::replace(&mut self.state, Arc::new(dummy_state)))
            .expect("we have exclusive ownership of the State at commit()");

        // Check if an emergency halt has been signaled.
        let should_halt = state.is_chain_halted().await;

        let is_pre_upgrade_height = state
            .is_pre_upgrade_height()
            .await
            .expect("must be able to read upgrade height");

        // If the next height is an upgrade height, we signal a halt and turn
        // a `halt_bit` on which will prevent the chain from restarting without
        // running a migration.
        if is_pre_upgrade_height {
            tracing::info!("pre-upgrade height reached, signaling halt");
            state.signal_halt();
        }

        // Commit the pending writes, clearing the state.
        let jmt_root = storage
            .commit(state)
            .await
            .expect("must be able to successfully commit to storage");

        // We want to halt the node, but not before we submit an ABCI `Commit`
        // response to `CometBFT`. To do this, we schedule a process exit in `2s`,
        // assuming a `5s` timeout.
        // See #4443 for more context.
        if should_halt || is_pre_upgrade_height {
            tokio::spawn(async move {
                sleep(Duration::from_secs(2)).await;
                tracing::info!("halt signal recorded, exiting process");
                std::process::exit(0);
            });
        }

        tracing::debug!(?jmt_root, "finished committing state");

        // Get the latest version of the state, now that we've committed it.
        let latest_snapshot = storage.latest_snapshot();
        self.snapshot_version = latest_snapshot.version();
        self.committed_snapshot = latest_snapshot.clone();
        self.state = Arc::new(StateDelta::new(latest_snapshot));
        self.pending_sct_append_log.clear();
        jmt_root
    }

    pub fn cometbft_validator_updates(&self) -> Vec<Update> {
        self.state
            .cometbft_validator_updates()
            // If the cometbft validator updates are not set, we return an empty
            // update set, signaling no change to Tendermint.
            .unwrap_or_default()
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
        let ibc_params = self.get_ibc_params().await?;
        let fee_params = self.get_fee_params().await?;
        let governance_params = self.get_governance_params().await?;
        let sct_params = self.get_sct_params().await?;
        let shielded_pool_params = self.get_shielded_pool_params().await?;
        let validator_params = self.get_stake_params().await?;

        Ok(AppParameters {
            chain_id,
            fee_params,
            governance_params,
            ibc_params,
            sct_params,
            shielded_pool_params,
            validator_params,
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
            + penumbra_sdk_validator::StateReadExt
            + penumbra_sdk_governance::component::StateReadExt
            + penumbra_sdk_fee::component::StateReadExt
            + penumbra_sdk_sct::component::clock::EpochRead
            + penumbra_sdk_ibc::component::StateReadExt
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
        transaction: penumbra_sdk_proto::core::transaction::v1::Transaction,
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

    /// Writes the app parameters to the state.
    ///
    /// Each component stores its own parameters separately, so this method
    /// splits up the provided parameters structure and writes it out to each component.
    fn put_app_params(&mut self, params: AppParameters) {
        // To make sure we don't forget to write any parts, destructure the entire params
        let AppParameters {
            chain_id,
            fee_params,
            governance_params,
            ibc_params,
            sct_params,
            shielded_pool_params,
            validator_params,
        } = params;

        // Ignore writes to the chain_id
        // TODO(erwan): we are momentarily not supporting chain_id changes
        // until the IBC host chain changes land.
        // See: https://github.com/mizufinance/penumbra/issues/3617#issuecomment-1917708221
        std::mem::drop(chain_id);

        self.put_fee_params(fee_params);
        self.put_governance_params(governance_params);
        self.put_ibc_params(ibc_params);
        self.put_sct_params(sct_params);
        self.put_shielded_pool_params(shielded_pool_params);
        self.put_stake_params(validator_params);
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}

#[cfg(test)]
mod tests {
    use std::ops::Deref;
    use std::sync::Arc;

    use anyhow::{anyhow, Context, Result};
    use cnidarium::{StateDelta, StateRead, StateWrite, TempStorage};
    use decaf377::{Fq, Fr};
    use futures::StreamExt as _;
    use penumbra_sdk_asset::{asset, Value, BASE_ASSET_DENOM, BASE_ASSET_ID};
    use penumbra_sdk_compact_block::StatePayload;
    use penumbra_sdk_compliance::registry::ComplianceRegistryWrite as _;
    use penumbra_sdk_compliance::{AssetPolicy, ComplianceLeaf};
    use penumbra_sdk_fee::Fee;
    use penumbra_sdk_keys::{test_keys, Address};
    use penumbra_sdk_mock_client::MockClient;
    use penumbra_sdk_mock_consensus::TestNode;
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_proof_aggregation::{AggregateBundle, ProofFamilyId};
    use penumbra_sdk_proto::DomainType;
    use penumbra_sdk_sct::component::clock::{EpochManager as _, EpochRead as _};
    use penumbra_sdk_sct::component::tree::{SctManager as _, SctRead as _};
    use penumbra_sdk_sct::component::StateWriteExt as _;
    use penumbra_sdk_sct::epoch::Epoch;
    use penumbra_sdk_sct::params::SctParameters;
    use penumbra_sdk_sct::{CommitmentSource, NullificationInfo, Nullifier};
    use penumbra_sdk_shielded_pool::component::NoteManager as _;
    use penumbra_sdk_shielded_pool::{
        genesis::Allocation, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan,
    };
    use penumbra_sdk_tct as tct;
    use penumbra_sdk_transaction::{
        memo::{MemoCiphertext, MemoPlaintext, MEMO_CIPHERTEXT_LEN_BYTES},
        plan::MemoPlan,
        Action, DetectionData, Transaction, TransactionParameters, TransactionPlan,
    };
    use rand_core::OsRng;
    use sha2::Digest as _;
    use tendermint::v0_37::abci::{request, response};
    use tendermint::{account, block, Hash, Time};

    use super::PrepareBlockLocalState;
    use crate::action_handler::transaction::{
        prepare_candidate_read_blocking_profiled, prepare_candidate_read_profiled,
        supports_parallel_prepare, HistoricalCheckContext,
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
    fn rolled_up_payload(value: u64) -> StatePayload {
        StatePayload::RolledUp {
            source: CommitmentSource::transaction(),
            commitment: tct::StateCommitment(Fq::from(value)),
        }
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
            shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
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
            let mut output = ShieldedOutputPlan::new(
                &mut OsRng,
                Value {
                    amount: send_amount,
                    asset_id: note.asset_id(),
                },
                test_keys::ADDRESS_1.deref().clone(),
            );
            let mut change = ShieldedOutputPlan::new(
                &mut OsRng,
                Value {
                    amount: change_amount,
                    asset_id: note.asset_id(),
                },
                note.address(),
            );
            for output in [&mut output, &mut change] {
                output.asset_anchor = spend.asset_anchor;
                output.compliance_anchor = spend.compliance_anchor;
                output.target_timestamp = spend.target_timestamp;
                output.is_regulated = spend.is_regulated;
                output.tx_blinding_nonce = spend.tx_blinding_nonce;
                output.asset_indexed_leaf = spend.asset_indexed_leaf.clone();
                output.asset_path = spend.asset_path.clone();
                output.asset_position = spend.asset_position;
                output.asset_policy = spend.asset_policy.clone();
            }
            let mut plan = TransactionPlan {
                actions: vec![TransferPlan::new(
                    vec![spend.into()],
                    vec![output.into(), change.into()],
                    Fr::from(1u64),
                )
                .expect("valid transfer plan")
                .into()],
                memo: Some(MemoPlan::new(
                    &mut OsRng,
                    MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
                )),
                detection_data: None,
                fee_funding: None,
                transaction_parameters: TransactionParameters {
                    chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                    ..Default::default()
                },
            }
            .with_populated_detection_data(OsRng, Default::default());

            let tx = client
                .witness_auth_build_with_compliance(&mut plan, storage.latest_snapshot())
                .await?;
            txs.push(tx.encode_to_vec());
        }

        Ok((storage, test_node, txs))
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
        let (artifacts, _profile) = App::build_tx_artifacts_for_stage("app_test", &decoded).await?;
        let segment_tx_counts = vec![decoded.len()];
        let (bundle, _segment_tx_counts, _aggregate_profile) =
            App::build_exact_segmented_aggregate_bundle_for_artifacts_profiled_public(
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
        let (artifacts, _profile) = App::build_tx_artifacts_for_stage("app_test", &decoded).await?;
        let segment_tx_counts = vec![decoded.len()];
        let (bundle, _, _) =
            App::build_exact_segmented_aggregate_bundle_for_artifacts_profiled_public(
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
    async fn prepare_candidate_read_profiled_supports_unregulated_fixture_txs() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(2).await?;
        let snapshot = Arc::new(storage.latest_snapshot());
        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

        for tx_bytes in txs {
            let tx = Arc::new(Transaction::decode(tx_bytes.as_slice())?);
            assert!(
                supports_parallel_prepare(Arc::as_ref(&tx)),
                "fixture tx should stay on the supported transfer fast path"
            );

            let prepared = prepare_candidate_read_profiled(
                tx.clone(),
                snapshot.clone(),
                historical_context.clone(),
                false,
            )
            .await?;

            assert_eq!(prepared.effects.spend_nullifiers.len(), 1);
            assert_eq!(
                prepared.effects.sct_payloads.len(),
                2,
                "fixture transfer should create receiver and change notes",
            );
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prepare_candidate_read_blocking_profiled_matches_async_fast_path() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(2).await?;
        let snapshot = Arc::new(storage.latest_snapshot());
        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

        for tx_bytes in txs {
            let tx = Arc::new(Transaction::decode(tx_bytes.as_slice())?);
            assert!(supports_parallel_prepare(Arc::as_ref(&tx)));

            let prepared_async = prepare_candidate_read_profiled(
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
                prepare_candidate_read_blocking_profiled(
                    tx_for_blocking,
                    snapshot_for_blocking,
                    context_for_blocking,
                    false,
                    handle,
                )
            })
            .await??;

            assert_eq!(
                prepared_async.effects.spend_nullifiers,
                prepared_blocking.effects.spend_nullifiers
            );
            assert_eq!(
                prepared_async.effects.sct_payloads.len(),
                prepared_blocking.effects.sct_payloads.len()
            );
            assert_eq!(
                prepared_async
                    .effects
                    .sct_payloads
                    .iter()
                    .map(|payload| *payload.commitment())
                    .collect::<Vec<_>>(),
                prepared_blocking
                    .effects
                    .sct_payloads
                    .iter()
                    .map(|payload| *payload.commitment())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                prepared_async.execution_profile.nullifier_lookup_count,
                prepared_blocking.execution_profile.nullifier_lookup_count
            );
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn checktx_fast_path_matches_legacy_for_supported_tx() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx = Arc::new(Transaction::decode(
            txs.first().expect("fixture transaction").as_slice(),
        )?);
        assert!(supports_parallel_prepare(Arc::as_ref(&tx)));
        let shared_context =
            Arc::new(CheckTxSharedContext::load(&storage.latest_snapshot()).await?);

        let mut legacy_app = App::new(storage.latest_snapshot());
        tx.check_historical(legacy_app.state.clone()).await?;
        let (legacy_events, legacy_profile) = legacy_app
            .execute_tx_checked_historical_profiled(tx.clone())
            .await?;

        let mut fast_app = App::new(storage.latest_snapshot());
        fast_app.set_checktx_shared_context(shared_context);
        let (fast_events, fast_profile) = fast_app.execute_checktx_fast_profiled(tx, false).await?;

        let mut legacy_rendered = legacy_events
            .iter()
            .map(|event| format!("{event:?}"))
            .collect::<Vec<_>>();
        legacy_rendered.sort();
        let mut fast_rendered = fast_events
            .iter()
            .map(|event| format!("{event:?}"))
            .collect::<Vec<_>>();
        fast_rendered.sort();
        assert_eq!(legacy_rendered, fast_rendered);
        assert!(
            fast_profile.nullifier_lookup_count >= legacy_profile.nullifier_lookup_count,
            "fast path should not undercount nullifier checks"
        );
        assert!(
            fast_profile.output_add_note_payload_ms >= 0.0,
            "fast path should report a valid note-payload timing metric"
        );

        Ok(())
    }

    #[tokio::test]
    async fn process_candidate_envelope_profiled_accepts_valid_fixture() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(2).await?;
        let envelope = candidate_envelope_from_fixture_txs(&storage, &txs).await?;
        let mut app = App::new(storage.latest_snapshot());

        let (verdict, _profile) = app
            .process_candidate_envelope_profiled(&envelope, None)
            .await?;
        assert!(matches!(verdict, response::ProcessProposal::Accept));

        Ok(())
    }

    #[tokio::test]
    async fn ensure_aggregate_bundle_tx_shape_rejects_memo_detection_fee_and_extra_action(
    ) -> Result<()> {
        let (_storage, _artifacts, bundle, bundle_tx) = aggregate_fixture(1).await?;

        let mut with_memo = bundle_tx.clone();
        with_memo.transaction_body.memo = Some(MemoCiphertext([0; MEMO_CIPHERTEXT_LEN_BYTES]));
        let memo_error =
            App::ensure_aggregate_bundle_tx_shape(&with_memo).expect_err("memo must be rejected");
        assert!(memo_error
            .to_string()
            .contains("aggregate bundle tx must not contain a memo"));

        let mut with_detection = bundle_tx.clone();
        with_detection.transaction_body.detection_data = Some(DetectionData { fmd_clues: vec![] });
        let detection_error = App::ensure_aggregate_bundle_tx_shape(&with_detection)
            .expect_err("detection data must be rejected");
        assert!(detection_error
            .to_string()
            .contains("aggregate bundle tx must not contain detection data"));

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
        bad_srs.srs_id.push(0);
        let srs_error =
            App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &bad_srs, None)
                .await
                .expect_err("bad SRS id must fail verification");
        assert!(srs_error
            .to_string()
            .contains("aggregate bundle SRS id mismatch"));

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
    async fn execute_validated_candidate_envelope_profiled_skips_proposal_validation() -> Result<()>
    {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let envelope = candidate_envelope_from_fixture_txs(&storage, &txs).await?;

        let mut preflight_app = App::new(storage.latest_snapshot());
        let (verdict, _profile) = preflight_app
            .process_candidate_envelope_profiled(&envelope, None)
            .await?;
        assert!(matches!(verdict, response::ProcessProposal::Accept));

        let mut execution_only = envelope.clone();
        execution_only.aggregate_bundle_tx_bytes = None;
        execution_only.segment_tx_counts.clear();
        execution_only.sidecar =
            ProposalArtifactSidecar::from_record(envelope.sidecar.clone()).to_record();

        let mut app = App::new(storage.latest_snapshot());
        let profile = app
            .execute_validated_candidate_envelope_profiled(
                &execution_only,
                storage.as_ref().clone(),
            )
            .await?;
        assert_eq!(profile.block_tx_count, 1);
        assert!(profile.deliver_txs_wall_ms > 0.0);

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
            shared_context.historical_check_context.fmd_meta_params,
            direct_context.fmd_meta_params
        );
        assert_eq!(
            shared_context
                .historical_check_context
                .previous_fmd_parameters,
            direct_context.previous_fmd_parameters
        );
        assert_eq!(
            shared_context
                .historical_check_context
                .current_fmd_parameters,
            direct_context.current_fmd_parameters
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
        let (miss_events, miss_profile) = miss_app
            .deliver_tx_bytes_impl_profiled(&tx_bytes, Some(&cache))
            .await?;
        assert_eq!(miss_profile.cache_hit_count, 0);

        let mut hit_app = App::new(storage.latest_snapshot());
        hit_app.set_checktx_shared_context(shared_context);
        let (hit_events, hit_profile) = hit_app
            .deliver_tx_bytes_impl_profiled(&tx_bytes, Some(&cache))
            .await?;
        assert_eq!(hit_profile.cache_hit_count, 1);
        assert_eq!(miss_events, hit_events);

        Ok(())
    }

    #[tokio::test]
    async fn prepared_reads_are_blind_to_same_block_nullifier_conflicts() -> Result<()> {
        let (storage, _node, txs) = setup_test_txs(1).await?;
        let tx = Arc::new(Transaction::decode(
            txs.first().expect("fixture transaction").as_slice(),
        )?);
        let snapshot = Arc::new(storage.latest_snapshot());
        let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

        let prepared_first = prepare_candidate_read_profiled(
            tx.clone(),
            snapshot.clone(),
            historical_context.clone(),
            false,
        )
        .await?;
        let prepared_second =
            prepare_candidate_read_profiled(tx.clone(), snapshot, historical_context, false)
                .await?;

        anyhow::ensure!(
            prepared_first.execution_profile.nullifier_lookup_count > 0,
            "fixture tx should exercise committed nullifier checks"
        );
        anyhow::ensure!(
            prepared_second.execution_profile.nullifier_lookup_count > 0,
            "fixture tx should exercise committed nullifier checks"
        );

        let mut app = App::new(storage.latest_snapshot());
        let mut block_state = PrepareBlockLocalState::default();

        app.apply_prepared_prepare_candidate_profiled(tx.clone(), prepared_first, &mut block_state)
            .await?;
        let err = app
            .apply_prepared_prepare_candidate_profiled(tx, prepared_second, &mut block_state)
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
        for nullifier in &nullifiers {
            repeated.nullify(*nullifier, source.clone()).await?;
        }

        let mut batched = StateDelta::new(snapshot);
        batched.put_block_height(42);
        batched.nullify_all(&nullifiers, source).await?;

        assert_eq!(repeated.pending_nullifiers(), batched.pending_nullifiers());

        for nullifier in &nullifiers {
            assert_eq!(
                repeated.spend_info(*nullifier).await?,
                batched.spend_info(*nullifier).await?,
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn proposal_batch_nullify_matches_sequential_and_preserves_sources() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let snapshot = storage.latest_snapshot();

        let entries = vec![
            (
                Nullifier(Fq::from(21u64)),
                CommitmentSource::Transaction {
                    id: Some([1u8; 32]),
                },
            ),
            (
                Nullifier(Fq::from(22u64)),
                CommitmentSource::Transaction {
                    id: Some([2u8; 32]),
                },
            ),
            (
                Nullifier(Fq::from(23u64)),
                CommitmentSource::Transaction {
                    id: Some([1u8; 32]),
                },
            ),
        ];

        let mut sequential = StateDelta::new(snapshot.clone());
        sequential.put_block_height(42);
        for (nullifier, source) in &entries {
            sequential.nullify(*nullifier, source.clone()).await?;
        }

        let mut proposal_batch = StateDelta::new(snapshot);
        proposal_batch.put_block_height(42);
        let _profile = proposal_batch.nullify_proposal_batch(&entries).await?;

        assert_eq!(
            sequential.pending_nullifiers(),
            proposal_batch.pending_nullifiers()
        );

        for (nullifier, _) in &entries {
            assert_eq!(
                sequential.spend_info(*nullifier).await?,
                proposal_batch.spend_info(*nullifier).await?,
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn app_readiness_fails_on_corrupted_nullifier_tree_nv() -> Result<()> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        penumbra_sdk_sct::nullifier_tree::insert_batch(
            &mut state,
            [(
                Nullifier(Fq::from(91u64)),
                NullificationInfo {
                    id: [9u8; 32],
                    spend_height: 7,
                },
            )],
        )
        .await?;
        storage.commit(state).await?;
        assert!(App::is_ready(storage.latest_snapshot()).await);

        let mut corrupt = StateDelta::new(storage.latest_snapshot());
        let mut stream = corrupt.nonverifiable_prefix_raw(
            penumbra_sdk_sct::state_key::nullifier_set::tree_node_prefix(),
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
            penumbra_sdk_sct::state_key::tree::incremental_prefix().as_bytes(),
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
        state
            .add_compliance_leaf(ComplianceLeaf::new(
                Address::dummy(&mut rand::thread_rng()),
                asset::Id(Fq::from(123u64)),
                Fq::from(7u64),
            ))
            .await?;
        state
            .register_regulated_asset(
                asset::Id(Fq::from(456u64)),
                AssetPolicy::simple(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
            )
            .await?;
        storage.commit(state).await?;
        assert!(App::is_ready(storage.latest_snapshot()).await);

        let mut corrupt = StateDelta::new(storage.latest_snapshot());
        delete_nv_prefix(
            &mut corrupt,
            penumbra_sdk_compliance::state_key::tree_storage::user_node_prefix().as_bytes(),
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
        app.deliver_tx_bytes_v2(tx_bytes.as_slice(), Some(&cache))
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
    async fn prepare_proposal_upgrades_extracted_cache_entries_to_fully_verified() -> Result<()> {
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

        let extracted = match cache.get(&tx_hash) {
            Some(CacheEntry::Extracted(artifact)) => artifact,
            _ => anyhow::bail!("expected extracted cache entry after CheckTx"),
        };
        assert!(!extracted.proof_items.is_empty());
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
            txs: vec![tx_bytes.into()],
            max_tx_bytes: 1024 * 1024,
            local_last_commit: None,
            misbehavior: Vec::new(),
            height: block::Height::from(1u32),
            time: Time::unix_epoch(),
            next_validators_hash: Hash::None,
            proposer_address: account::Id::new([0u8; 20]),
        };

        let (prepared, profile, _) = proposer
            .prepare_proposal_v2_profiled(proposal, Some(&cache), false)
            .await;
        assert_eq!(
            prepared.txs.len(),
            2,
            "proposal should include user tx plus aggregate bundle"
        );
        assert_eq!(
            profile.historical_validation_reuse_count, 1,
            "prepare_proposal should reuse the CheckTx historical validation on the same snapshot"
        );

        match cache.get(&tx_hash) {
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

        node.block().execute().await?;

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

        let (prepared, profile, _) = proposer
            .prepare_proposal_v2_profiled(proposal, Some(&cache), false)
            .await;
        assert_eq!(
            prepared.txs.len(),
            2,
            "proposal should still include the user tx and aggregate bundle after re-validation"
        );
        assert_eq!(
            profile.historical_validation_reuse_count, 0,
            "prepare_proposal must re-run historical validation after the committed snapshot changes"
        );

        Ok(())
    }

    #[test]
    fn aggregate_bundle_size_estimate_is_monotonic() {
        let chain_id = "penumbra-test";
        let small = vec![
            AggregateBundleFamilyEstimate {
                family_id: ProofFamilyId::Transfer,
                real_count: 8,
                padded_count: 8,
                aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
            },
            AggregateBundleFamilyEstimate {
                family_id: ProofFamilyId::Consolidate(
                    penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS[0].id,
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
                family_id: ProofFamilyId::Consolidate(
                    penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS[0].id,
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
