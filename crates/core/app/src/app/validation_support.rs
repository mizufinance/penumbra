use std::collections::HashMap;
use std::sync::Arc;

use penumbra_sdk_proof_aggregation::AggregateBundle;
use penumbra_sdk_sct::Nullifier;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use super::preconsensus::ProposalArtifactSidecarRecord;
use crate::stateless_cache::TxArtifact;

pub const MAX_VALIDATION_TX_COUNT: usize = 4_096;
pub const MAX_VALIDATION_ACTIONS_PER_TX: usize = 512;
pub const MAX_VALIDATION_NULLIFIERS_PER_TX: usize = 256;
pub const MAX_VALIDATION_NULLIFIERS_PER_BLOCK: usize = 32_768;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateEnvelope {
    pub txs: Vec<Vec<u8>>,
    pub tx_hashes: Vec<[u8; 32]>,
    pub aggregate_bundle_tx_bytes: Option<Vec<u8>>,
    pub sidecar: ProposalArtifactSidecarRecord,
    pub segment_tx_counts: Vec<usize>,
    pub block_tx_count: usize,
    pub total_payload_bytes: usize,
    pub candidate_digest: [u8; 32],
    pub source_builder_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ValidationRejectReason {
    TxCountExceeded,
    PayloadBytesExceeded,
    TxHashCountMismatch,
    CandidateDigestMismatch,
    SegmentCoverageMismatch,
    BundleTxShapeInvalid,
    BundleMissing,
    BundlePresentWithZeroProofs,
    SidecarTxCountMismatch,
    SidecarCommitmentMismatch,
    SidecarEntrySetMismatch,
    SidecarDecodeFailed,
    TxHashMismatch,
    TxDecodeFailed,
    ActionCountExceeded,
    NullifierCountExceededPerTx,
    NullifierCountExceededPerBlock,
    DuplicateSpendNullifier,
    CommittedNullifierConflict,
    AggregateVerifyFailed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationStageVerdict {
    pub ok: bool,
    pub reject_reason: Option<ValidationRejectReason>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ValidationVerdict {
    pub shape: ValidationStageVerdict,
    pub stateful: ValidationStageVerdict,
    pub aggregate: ValidationStageVerdict,
    pub final_accept: bool,
    pub reject_reason: Option<ValidationRejectReason>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ValidationProfile {
    pub shape_check_ms: f64,
    pub sidecar_check_ms: f64,
    pub nullifier_cache_lookup_ms: f64,
    pub nullifier_extract_ms: f64,
    pub stateful_conflict_check_ms: f64,
    pub aggregate_verify_ms: f64,
    pub aggregate_artifact_cache_lookup_ms: f64,
    pub aggregate_tx_decode_ms: f64,
    pub aggregate_sidecar_decode_ms: f64,
    pub aggregate_expected_segments_ms: f64,
    pub aggregate_prepare_inputs_ms: f64,
    pub aggregate_verify_kernel_ms: f64,
    pub aggregate_backend_deserialize_ms: f64,
    pub aggregate_backend_challenge_ms: f64,
    pub aggregate_backend_tipa_ab_ms: f64,
    pub aggregate_backend_tipa_c_ms: f64,
    pub aggregate_backend_public_input_fold_ms: f64,
    pub aggregate_backend_ppe_ms: f64,
    pub aggregate_backend_core_total_ms: f64,
    pub cache_hit_count: usize,
    pub cache_miss_count: usize,
    pub aggregate_artifact_cache_hit_count: usize,
    pub aggregate_artifact_cache_miss_count: usize,
    pub block_tx_count: usize,
    pub total_payload_bytes: usize,
    pub total_spend_nullifiers: usize,
}

#[derive(Clone, Default)]
struct ValidationCacheEntry {
    spend_nullifiers: Arc<Vec<Nullifier>>,
    artifact: Option<Arc<TxArtifact>>,
}

#[derive(Clone, Default)]
pub struct ValidationNullifierCache {
    tx_hash_to_entry: HashMap<[u8; 32], ValidationCacheEntry>,
}

impl ValidationNullifierCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        tx_hash: [u8; 32],
        spend_nullifiers: Vec<Nullifier>,
        artifact: Option<Arc<TxArtifact>>,
    ) {
        self.tx_hash_to_entry.insert(
            tx_hash,
            ValidationCacheEntry {
                spend_nullifiers: Arc::new(spend_nullifiers),
                artifact,
            },
        );
    }

    pub fn get(&self, tx_hash: &[u8; 32]) -> Option<Arc<Vec<Nullifier>>> {
        self.tx_hash_to_entry
            .get(tx_hash)
            .map(|entry| entry.spend_nullifiers.clone())
    }

    pub fn get_artifact(&self, tx_hash: &[u8; 32]) -> Option<Arc<TxArtifact>> {
        self.tx_hash_to_entry
            .get(tx_hash)
            .and_then(|entry| entry.artifact.clone())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvelopeValidationResult {
    pub envelope: CandidateEnvelope,
    pub verdict: ValidationVerdict,
    pub profile: ValidationProfile,
}

impl CandidateEnvelope {
    pub fn aggregate_bundle(&self) -> anyhow::Result<Option<AggregateBundle>> {
        use anyhow::Context as _;
        use penumbra_sdk_proto::DomainType as _;
        use penumbra_sdk_transaction::Transaction;

        let Some(bundle_tx_bytes) = &self.aggregate_bundle_tx_bytes else {
            return Ok(None);
        };
        let tx = Transaction::decode(bundle_tx_bytes.as_slice())
            .context("decoding aggregate bundle tx bytes")?;
        Ok(Some(
            tx.aggregate_bundle_action()
                .cloned()
                .context("aggregate bundle tx missing bundle action")?,
        ))
    }
}

pub fn candidate_digest_from_hashes(tx_hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update((tx_hashes.len() as u64).to_le_bytes());
    for hash in tx_hashes {
        hasher.update(hash);
    }
    hasher.finalize().into()
}

pub fn sidecar_commitment(
    record: &ProposalArtifactSidecarRecord,
    ordered_tx_hashes: &[[u8; 32]],
) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update((record.chunk_tx_count as u64).to_le_bytes());
    for segment_tx_count in &record.segment_tx_counts {
        hasher.update((*segment_tx_count as u64).to_le_bytes());
    }
    let entry_map = record
        .entries
        .iter()
        .map(|entry| (entry.tx_hash, entry.encoded_entry.as_slice()))
        .collect::<HashMap<_, _>>();
    for tx_hash in ordered_tx_hashes {
        if let Some(encoded_entry) = entry_map.get(tx_hash) {
            hasher.update(tx_hash);
            hasher.update((encoded_entry.len() as u64).to_le_bytes());
            hasher.update(*encoded_entry);
        }
    }
    hasher.finalize().into()
}
