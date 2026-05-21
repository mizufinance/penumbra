pub use crate::test_proof_helpers::proof_test_helpers::{
    CircuitType, REGULATED_ASSET_ID, UNREGULATED_ASSET_ID,
};

use anyhow::Result;
use ibc_types::core::channel::msgs::MsgRecvPacket;
use penumbra_sdk_asset::asset;
use penumbra_sdk_num::Amount;

use crate::{
    ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalProofPrivate,
    ShieldedIcs20WithdrawalProofPublic,
};

pub fn benchmark_transfer_roundtrip_inputs(
    is_regulated: bool,
) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
    crate::test_proof_helpers::proof_test_helpers::build_transfer_roundtrip_inputs(is_regulated)
}

pub fn benchmark_shielded_ics20_withdrawal_roundtrip_inputs(
    family_id: ShieldedIcs20WithdrawalFamilyId,
    is_regulated: bool,
) -> (
    ShieldedIcs20WithdrawalProofPublic,
    ShieldedIcs20WithdrawalProofPrivate,
) {
    crate::test_proof_helpers::proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
        family_id,
        is_regulated,
    )
}

pub fn benchmark_parse_ics20_receive_context(
    msg: &MsgRecvPacket,
) -> Result<(asset::Id, bool, Amount)> {
    crate::component::benchmark_parse_ics20_receive_context(msg)
}
