#![no_main]

mod common;

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use penumbra_sdk_app::app::{
    decode_batch_item_for_fuzz, ProposalArtifactSidecar, ProposalArtifactSidecarRecord,
};
use penumbra_sdk_transaction::Transaction;

fuzz_target!(|data: &[u8]| {
    let split = data.len() / 2;
    let _ = decode_batch_item_for_fuzz(data[..split].to_vec(), vec![data[split..].to_vec()]);

    let sidecar = ProposalArtifactSidecar::from_record(ProposalArtifactSidecarRecord {
        chunk_tx_count: 0,
        segment_tx_counts: Vec::new(),
        encoded_bytes: data.len(),
        commitment: [0; 32],
        entries: Vec::new(),
    });
    let tx = Arc::new(Transaction::default());
    let _ = sidecar.decode_artifact([0; 32], tx, data);
});
