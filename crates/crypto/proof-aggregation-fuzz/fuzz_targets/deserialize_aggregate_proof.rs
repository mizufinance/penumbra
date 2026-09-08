#![no_main]

use libfuzzer_sys::fuzz_target;
use shieldd_sdk_proof_aggregation::deserialize_aggregate_proof_for_fuzz;
use shieldd_sdk_proof_aggregation_reference::reference_decode_aggregate;

fuzz_target!(|data: &[u8]| {
    let actual = deserialize_aggregate_proof_for_fuzz(data).is_ok();
    let reference = reference_decode_aggregate(data);
    assert_eq!(
        actual,
        reference.is_ok(),
        "production/reference decoder disagreement"
    );
    if let Ok(canonical) = reference {
        assert_eq!(canonical, data);
    }
});
