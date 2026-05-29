#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;
use penumbra_sdk_proof_aggregation::preflight_aggregate_verify;

fuzz_target!(|data: &[u8]| {
    let _ = preflight_aggregate_verify(common::preflight_input(data));
});
