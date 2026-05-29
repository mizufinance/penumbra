#![no_main]

use libfuzzer_sys::fuzz_target;
use penumbra_sdk_proof_aggregation::deserialize_aggregate_proof_for_fuzz;

fuzz_target!(|data: &[u8]| {
    let _ = deserialize_aggregate_proof_for_fuzz(data);
});
