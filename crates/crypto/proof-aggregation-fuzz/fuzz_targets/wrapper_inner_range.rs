#![no_main]

use libfuzzer_sys::fuzz_target;
use penumbra_sdk_proof_aggregation::decode_wrapped_aggregate_proof_inner_range;

fuzz_target!(|data: &[u8]| {
    let mut digest = [0u8; 32];
    let digest_len = data.len().min(32);
    digest[..digest_len].copy_from_slice(&data[..digest_len]);
    let max = data
        .first()
        .map(|byte| (*byte as usize).saturating_mul(257));
    if let Ok(range) = decode_wrapped_aggregate_proof_inner_range(data, digest, max) {
        assert!(range.start <= range.end);
        assert!(range.end <= data.len());
    }
});
