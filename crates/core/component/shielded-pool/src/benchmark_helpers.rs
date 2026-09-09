pub use crate::test_proof_helpers::proof_test_helpers::{
    CircuitType, REGULATED_ASSET_ID, UNREGULATED_ASSET_ID,
};

use crate::{
    ShieldedWithdrawalFamilyId, ShieldedWithdrawalProofPrivate, ShieldedWithdrawalProofPublic,
};

pub fn benchmark_transfer_roundtrip_inputs(
    is_regulated: bool,
) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
    crate::test_proof_helpers::proof_test_helpers::build_transfer_roundtrip_inputs(is_regulated)
}

pub fn benchmark_shielded_withdrawal_roundtrip_inputs(
    family_id: ShieldedWithdrawalFamilyId,
    is_regulated: bool,
) -> (
    ShieldedWithdrawalProofPublic,
    ShieldedWithdrawalProofPrivate,
) {
    crate::test_proof_helpers::proof_test_helpers::build_shielded_withdrawal_roundtrip_inputs(
        family_id,
        is_regulated,
    )
}
