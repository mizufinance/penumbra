use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use super::preconsensus::ProposalArtifactSidecarRecord;

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
