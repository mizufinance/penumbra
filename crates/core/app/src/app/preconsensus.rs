use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use crate::stateless_cache::TxArtifact;
use anyhow::{Context, Result};
use ark_groth16::Proof;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::Bls12_377;
use penumbra_sdk_proof_aggregation::ProofFamilyId;
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct::StateCommitment;
use penumbra_sdk_transaction::Transaction;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CheckTxProfile {
    pub checktx_total_wall_ms: f64,
    pub checktx_cache_lookup_ms: f64,
    pub checktx_stateless_phase_wall_ms: f64,
    pub checktx_execute_fast_wall_ms: f64,
    pub checktx_fast_prepare_join_wall_ms: f64,
    pub checktx_fast_apply_wall_ms: f64,
    pub decode_tx_ms: f64,
    pub stateless_artifact_queue_wait_ms: f64,
    pub stateless_task_join_wall_ms: f64,
    pub stateless_artifact_blocking_total_ms: f64,
    pub stateless_artifact_ms: f64,
    pub stateless_initial_cache_insert_ms: f64,
    pub stateless_historical_stamp_ms: f64,
    pub stateless_historical_mark_ms: f64,
    pub stateless_final_cache_insert_ms: f64,
    pub stateless_artifact_precheck_ms: f64,
    pub stateless_artifact_action_extract_ms: f64,
    pub stateless_artifact_action_auth_sig_ms: f64,
    pub stateless_artifact_action_extract_public_ms: f64,
    pub stateless_artifact_action_to_batch_item_ms: f64,
    pub stateless_artifact_batch_verify_ms: f64,
    pub check_historical_ms: f64,
    pub checktx_fast_context_load_ms: f64,
    pub checktx_fast_read_queue_wait_ms: f64,
    pub checktx_fast_read_blocking_total_ms: f64,
    pub execute_ms: f64,
    pub execute_get_block_height_ms: f64,
    pub execute_index_tx_ms: f64,
    pub execute_check_and_execute_ms: f64,
    pub execute_set_source_ms: f64,
    pub execute_pay_fee_ms: f64,
    pub execute_action_execute_ms: f64,
    pub execute_read_local_precheck_ms: f64,
    pub execute_read_lookup_wait_or_join_ms: f64,
    pub execute_read_historical_check_ms: f64,
    pub execute_read_nullifier_wait_ms: f64,
    pub execute_read_anchor_cache_wait_ms: f64,
    pub execute_spend_action_execute_ms: f64,
    pub execute_spend_nullifier_check_ms: f64,
    pub execute_spend_nullifier_tx_local_scan_ms: f64,
    pub execute_spend_nullifier_block_log_lookup_ms: f64,
    pub execute_spend_nullifier_committed_check_ms: f64,
    pub execute_spend_nullifier_enqueue_ms: f64,
    pub execute_spend_nullifier_stage_ms: f64,
    pub execute_spend_nullifier_merge_ms: f64,
    pub execute_nullifier_lookup_count: usize,
    pub execute_output_action_execute_ms: f64,
    pub execute_output_add_note_payload_ms: f64,
    pub execute_other_action_execute_ms: f64,
    pub execute_record_clues_ms: f64,
    pub execute_apply_ms: f64,
    pub checktx_candidate_read_wall_ms: f64,
    pub checktx_candidate_effects_build_ms: f64,
    pub checktx_serial_apply_wall_ms: f64,
    pub checktx_serial_nullifier_insert_ms: f64,
    pub checktx_serial_sct_append_ms: f64,
    pub checktx_serial_event_emit_ms: f64,
    pub checktx_serial_fee_apply_ms: f64,
    pub cache_hit_count: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrepareProposalProfile {
    pub candidate_scan_ms: f64,
    pub nullifier_dedup_ms: f64,
    pub anchor_precheck_ms: f64,
    pub artifact_fill_ms: f64,
    pub artifact_fill_precheck_ms: f64,
    pub artifact_fill_action_extract_ms: f64,
    pub artifact_fill_batch_verify_ms: f64,
    pub stateful_filter_execute_ms: f64,
    pub stateful_filter_check_historical_ms: f64,
    pub stateful_filter_get_block_height_ms: f64,
    pub stateful_filter_clone_tx_ms: f64,
    pub stateful_filter_proto_convert_ms: f64,
    pub stateful_filter_put_block_transaction_ms: f64,
    pub stateful_filter_begin_state_tx_ms: f64,
    pub stateful_filter_index_tx_ms: f64,
    pub stateful_filter_check_and_execute_ms: f64,
    pub stateful_filter_set_source_ms: f64,
    pub stateful_filter_pay_fee_ms: f64,
    pub stateful_filter_action_execute_ms: f64,
    pub stateful_filter_read_local_precheck_ms: f64,
    pub stateful_filter_read_lookup_wait_or_join_ms: f64,
    pub stateful_filter_read_historical_check_ms: f64,
    pub stateful_filter_read_nullifier_wait_ms: f64,
    pub stateful_filter_read_anchor_cache_wait_ms: f64,
    pub stateful_filter_read_task_dispatch_ms: f64,
    pub stateful_filter_spend_action_execute_ms: f64,
    pub stateful_filter_spend_nullifier_check_ms: f64,
    pub stateful_filter_spend_nullifier_tx_local_scan_ms: f64,
    pub stateful_filter_spend_nullifier_block_log_lookup_ms: f64,
    pub stateful_filter_spend_nullifier_committed_check_ms: f64,
    pub stateful_filter_spend_nullifier_enqueue_ms: f64,
    pub stateful_filter_spend_nullifier_stage_ms: f64,
    pub stateful_filter_spend_nullifier_merge_ms: f64,
    pub stateful_filter_nullifier_lookup_count: usize,
    pub stateful_filter_output_action_execute_ms: f64,
    pub stateful_filter_output_add_note_payload_ms: f64,
    pub stateful_filter_other_action_execute_ms: f64,
    pub stateful_filter_record_clues_ms: f64,
    pub stateful_filter_apply_ms: f64,
    pub stateful_filter_parallel_read_wall_ms: f64,
    pub stateful_filter_candidate_read_wall_ms: f64,
    pub stateful_filter_candidate_effects_build_ms: f64,
    pub stateful_filter_serial_apply_wall_ms: f64,
    pub stateful_filter_serial_same_block_conflict_ms: f64,
    pub stateful_filter_serial_state_delta_apply_ms: f64,
    pub stateful_filter_serial_nullifier_insert_ms: f64,
    pub stateful_filter_proposal_nullifier_lookup_write_ms: f64,
    pub stateful_filter_proposal_pending_nullifier_stage_ms: f64,
    pub stateful_filter_serial_sct_append_ms: f64,
    pub stateful_filter_serial_event_emit_ms: f64,
    pub stateful_filter_serial_fee_apply_ms: f64,
    pub stateful_filter_parallel_read_sum_candidate_ms: f64,
    pub stateful_filter_parallel_read_tasks_spawned: usize,
    pub stateful_filter_parallel_read_max_inflight: usize,
    pub stateful_filter_parallel_read_configured_concurrency: usize,
    pub stateful_filter_anchor_cache_hits: usize,
    pub stateful_filter_anchor_cache_misses: usize,
    pub stateful_filter_anchor_unique_pairs: usize,
    pub stateful_filter_claimed_anchor_cache_hits: usize,
    pub stateful_filter_claimed_anchor_cache_misses: usize,
    pub stateful_filter_claimed_anchor_unique_values: usize,
    pub stateful_filter_read_anchor_validation_ms: f64,
    pub stateful_filter_read_committed_nullifier_ms: f64,
    pub stateful_filter_read_effects_build_ms: f64,
    pub historical_validation_reuse_count: usize,
    pub sct_batch_flush_ms: f64,
    pub deferred_index_flush_ms: f64,
    pub select_ready_chunks_ms: f64,
    pub tail_aggregate_ms: f64,
    pub sidecar_build_ms: f64,
    pub bundle_assemble_ms: f64,
    pub total_aggregate_ms: f64,
    pub aggregate_merge_items_ms: f64,
    pub aggregate_setup_ms: f64,
    pub aggregate_padding_ms: f64,
    pub aggregate_collect_proofs_ms: f64,
    pub aggregate_backend_core_ms: f64,
    pub aggregate_proof_serialize_ms: f64,
    pub aggregate_bundle_tx_build_ms: f64,
    pub aggregate_spend_ms: f64,
    pub aggregate_output_ms: f64,
    pub aggregate_other_ms: f64,
    pub ready_chunk_count: usize,
    pub tail_tx_count: usize,
    pub sidecar_bytes: usize,
    pub proposal_prefix_select_ms: f64,
    pub proposal_assemble_attempts: usize,
    pub proposal_oversize_retry_count: usize,
    pub speculative_tail_reuse_count: usize,
    pub speculative_tail_recompute_count: usize,
    pub aggregate_retry_cache_hits: usize,
    pub aggregate_retry_cache_misses: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProcessProposalProfile {
    pub artifact_lookup_ms: f64,
    pub tx_decode_classify_ms: f64,
    pub cold_reconstruction_ms: f64,
    pub cold_reconstruction_precheck_ms: f64,
    pub cold_reconstruction_action_extract_ms: f64,
    pub cold_reconstruction_batch_verify_ms: f64,
    pub nullifier_dedup_ms: f64,
    pub anchor_recheck_ms: f64,
    pub aggregate_verify_ms: f64,
    pub cold_sidecar_load_ms: f64,
    pub cold_sidecar_decode_ms: f64,
    pub stateful_replay_execute_ms: f64,
    pub stateful_replay_check_historical_ms: f64,
    pub stateful_replay_get_block_height_ms: f64,
    pub stateful_replay_clone_tx_ms: f64,
    pub stateful_replay_proto_convert_ms: f64,
    pub stateful_replay_put_block_transaction_ms: f64,
    pub stateful_replay_begin_state_tx_ms: f64,
    pub stateful_replay_index_tx_ms: f64,
    pub stateful_replay_check_and_execute_ms: f64,
    pub stateful_replay_set_source_ms: f64,
    pub stateful_replay_pay_fee_ms: f64,
    pub stateful_replay_action_execute_ms: f64,
    pub stateful_replay_spend_action_execute_ms: f64,
    pub stateful_replay_spend_nullifier_check_ms: f64,
    pub stateful_replay_spend_nullifier_tx_local_scan_ms: f64,
    pub stateful_replay_spend_nullifier_block_log_lookup_ms: f64,
    pub stateful_replay_spend_nullifier_committed_check_ms: f64,
    pub stateful_replay_spend_nullifier_enqueue_ms: f64,
    pub stateful_replay_spend_nullifier_stage_ms: f64,
    pub stateful_replay_spend_nullifier_merge_ms: f64,
    pub stateful_replay_output_action_execute_ms: f64,
    pub stateful_replay_output_add_note_payload_ms: f64,
    pub stateful_replay_other_action_execute_ms: f64,
    pub stateful_replay_record_clues_ms: f64,
    pub stateful_replay_apply_ms: f64,
    pub sct_batch_flush_ms: f64,
    pub deferred_index_flush_ms: f64,
    pub artifact_hit_count: usize,
    pub artifact_miss_count: usize,
    pub warm_reuse_count: usize,
    pub cold_sidecar_bytes: usize,
}

#[derive(Clone)]
pub struct ProposalArtifactSidecar {
    pub chunk_tx_count: usize,
    pub segment_tx_counts: Vec<usize>,
    pub encoded_bytes: usize,
    pub commitment: [u8; 32],
    entries: BTreeMap<[u8; 32], Arc<Vec<u8>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalArtifactSidecarRecordEntry {
    pub tx_hash: [u8; 32],
    pub encoded_entry: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalArtifactSidecarRecord {
    pub chunk_tx_count: usize,
    pub segment_tx_counts: Vec<usize>,
    pub encoded_bytes: usize,
    pub commitment: [u8; 32],
    pub entries: Vec<ProposalArtifactSidecarRecordEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncodedBatchItem {
    proof: Vec<u8>,
    public_inputs: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncodedArtifactSidecarEntry {
    tx_hash: [u8; 32],
    proof_items: BTreeMap<ProofFamilyId, Vec<EncodedBatchItem>>,
    spend_nullifiers: Vec<[u8; 32]>,
    anchor_pairs: Vec<([u8; 32], [u8; 32])>,
    total_proof_count: usize,
}

impl ProposalArtifactSidecar {
    pub fn build(
        artifacts: &[Arc<TxArtifact>],
        chunk_tx_count: usize,
        segment_tx_counts: Vec<usize>,
    ) -> Result<Self> {
        anyhow::ensure!(
            segment_tx_counts.iter().sum::<usize>() == artifacts.len(),
            "sidecar segment counts must cover every artifact exactly once"
        );

        let mut entries = BTreeMap::new();
        let mut encoded_bytes = 0usize;
        let mut hasher = sha2::Sha256::new();
        hasher.update((chunk_tx_count as u64).to_le_bytes());
        for segment_tx_count in &segment_tx_counts {
            hasher.update((*segment_tx_count as u64).to_le_bytes());
        }

        for artifact in artifacts {
            let tx_hash: [u8; 32] =
                sha2::Sha256::digest(artifact.tx.encode_to_vec().as_slice()).into();
            let encoded_entry = encode_sidecar_entry(tx_hash, artifact)?;
            encoded_bytes += encoded_entry.len();
            hasher.update(tx_hash);
            hasher.update((encoded_entry.len() as u64).to_le_bytes());
            hasher.update(encoded_entry.as_slice());
            entries.insert(tx_hash, Arc::new(encoded_entry));
        }

        Ok(Self {
            chunk_tx_count,
            segment_tx_counts,
            encoded_bytes,
            commitment: hasher.finalize().into(),
            entries,
        })
    }

    pub fn entry_bytes(&self, tx_hash: &[u8; 32]) -> Option<Arc<Vec<u8>>> {
        self.entries.get(tx_hash).cloned()
    }

    pub fn to_record(&self) -> ProposalArtifactSidecarRecord {
        ProposalArtifactSidecarRecord {
            chunk_tx_count: self.chunk_tx_count,
            segment_tx_counts: self.segment_tx_counts.clone(),
            encoded_bytes: self.encoded_bytes,
            commitment: self.commitment,
            entries: self
                .entries
                .iter()
                .map(
                    |(tx_hash, encoded_entry)| ProposalArtifactSidecarRecordEntry {
                        tx_hash: *tx_hash,
                        encoded_entry: (**encoded_entry).clone(),
                    },
                )
                .collect(),
        }
    }

    pub fn from_record(record: ProposalArtifactSidecarRecord) -> Self {
        Self {
            chunk_tx_count: record.chunk_tx_count,
            segment_tx_counts: record.segment_tx_counts,
            encoded_bytes: record.encoded_bytes,
            commitment: record.commitment,
            entries: record
                .entries
                .into_iter()
                .map(|entry| (entry.tx_hash, Arc::new(entry.encoded_entry)))
                .collect(),
        }
    }

    pub fn decode_artifact(
        &self,
        tx_hash: [u8; 32],
        tx: Arc<Transaction>,
        encoded_entry: &[u8],
    ) -> Result<Arc<TxArtifact>> {
        let decoded: EncodedArtifactSidecarEntry =
            bincode::deserialize(encoded_entry).context("decoding sidecar entry")?;
        anyhow::ensure!(
            decoded.tx_hash == tx_hash,
            "sidecar entry tx hash mismatch: expected {}, got {}",
            hex::encode(tx_hash),
            hex::encode(decoded.tx_hash)
        );

        let proof_items = decoded
            .proof_items
            .into_iter()
            .map(|(family_id, items)| {
                let decoded_items = items
                    .into_iter()
                    .map(decode_batch_item)
                    .collect::<Result<Vec<_>>>()?;
                Ok((family_id, decoded_items))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

        let spend_nullifiers = decoded
            .spend_nullifiers
            .into_iter()
            .map(|bytes| Nullifier::try_from(bytes.as_slice()))
            .collect::<Result<Vec<_>, _>>()
            .context("decoding sidecar nullifiers")?;
        let anchor_pairs = decoded
            .anchor_pairs
            .into_iter()
            .map(|(compliance, asset)| {
                Ok((
                    StateCommitment::try_from(compliance)
                        .context("decoding sidecar compliance anchor")?,
                    StateCommitment::try_from(asset).context("decoding sidecar asset anchor")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Arc::new(TxArtifact {
            tx,
            proof_items,
            spend_nullifiers,
            anchor_pairs,
            total_proof_count: decoded.total_proof_count,
            historical_validation: None,
        }))
    }
}

fn encode_sidecar_entry(tx_hash: [u8; 32], artifact: &TxArtifact) -> Result<Vec<u8>> {
    let proof_items = artifact
        .proof_items
        .iter()
        .map(|(family_id, items)| {
            let encoded_items = items
                .iter()
                .map(encode_batch_item)
                .collect::<Result<Vec<_>>>()?;
            Ok((*family_id, encoded_items))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let spend_nullifiers = artifact
        .spend_nullifiers
        .iter()
        .copied()
        .map(Into::into)
        .collect::<Vec<[u8; 32]>>();
    let anchor_pairs = artifact
        .anchor_pairs
        .iter()
        .copied()
        .map(|(compliance, asset)| (compliance.into(), asset.into()))
        .collect::<Vec<_>>();

    bincode::serialize(&EncodedArtifactSidecarEntry {
        tx_hash,
        proof_items,
        spend_nullifiers,
        anchor_pairs,
        total_proof_count: artifact.total_proof_count,
    })
    .context("encoding sidecar entry")
}

fn encode_batch_item(item: &BatchItem) -> Result<EncodedBatchItem> {
    let mut proof = Vec::new();
    item.proof
        .serialize_compressed(&mut proof)
        .context("serializing sidecar proof")?;
    let public_inputs = item
        .public_inputs
        .iter()
        .map(|input| {
            let mut encoded = Vec::new();
            input
                .serialize_compressed(&mut encoded)
                .context("serializing sidecar public input")?;
            Ok(encoded)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(EncodedBatchItem {
        proof,
        public_inputs,
    })
}

fn decode_batch_item(encoded: EncodedBatchItem) -> Result<BatchItem> {
    let proof = Proof::<Bls12_377>::deserialize_compressed(&mut Cursor::new(encoded.proof))
        .context("decoding sidecar proof")?;
    let public_inputs = encoded
        .public_inputs
        .into_iter()
        .map(|bytes| {
            <decaf377::Fq as CanonicalDeserialize>::deserialize_compressed(&mut Cursor::new(bytes))
                .context("decoding sidecar public input")
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(BatchItem {
        proof,
        public_inputs,
    })
}

#[cfg(any(test, feature = "fuzzing"))]
pub fn decode_batch_item_for_fuzz(proof: Vec<u8>, public_inputs: Vec<Vec<u8>>) -> Result<()> {
    decode_batch_item(EncodedBatchItem {
        proof,
        public_inputs,
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn decode_batch_item_do_not_panic(
            proof in prop::collection::vec(any::<u8>(), 0usize..=4096),
            public_inputs in prop::collection::vec(
                prop::collection::vec(any::<u8>(), 0usize..=128),
                0usize..=4,
            ),
        ) {
            let _ = decode_batch_item_for_fuzz(proof, public_inputs);
        }

        #[test]
        fn decode_artifact_do_not_panic(
            tx_hash in any::<[u8; 32]>(),
            entry_bytes in prop::collection::vec(any::<u8>(), 0usize..=4096),
        ) {
            let sidecar = ProposalArtifactSidecar {
                chunk_tx_count: 0,
                segment_tx_counts: Vec::new(),
                encoded_bytes: 0,
                commitment: [0; 32],
                entries: BTreeMap::new(),
            };
            let tx = Arc::new(Transaction::default());
            let _ = sidecar.decode_artifact(tx_hash, tx.clone(), &entry_bytes);

            let structured = EncodedArtifactSidecarEntry {
                tx_hash,
                proof_items: BTreeMap::new(),
                spend_nullifiers: Vec::new(),
                anchor_pairs: Vec::new(),
                total_proof_count: 0,
            };
            if let Ok(encoded) = bincode::serialize(&structured) {
                let _ = sidecar.decode_artifact(tx_hash, tx, &encoded);
            }
        }
    }
}
