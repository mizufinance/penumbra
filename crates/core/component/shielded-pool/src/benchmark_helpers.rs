pub use crate::test_proof_helpers::proof_test_helpers::{
    generate_test_data, CircuitType, REGULATED_ASSET_ID, UNREGULATED_ASSET_ID,
};

pub fn benchmark_transfer_roundtrip_inputs(
    family_id: crate::TransferFamilyId,
    is_regulated: bool,
) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
    crate::test_proof_helpers::proof_test_helpers::build_transfer_roundtrip_inputs(
        family_id,
        is_regulated,
    )
}
