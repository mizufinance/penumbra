//! Application facts consumed by aggregate-verifier validation.

use crate::{bundle::family_proto_fields, ProofFamilyId};

#[doc(hidden)]
pub use ark_ip_proofs::app_verifier::{
    app_verify_accepted_join_projection_core, app_verify_family_count_core,
    app_verify_join_acceptance_core, app_verify_plan_identity_core, app_verify_plan_ids_core,
    app_verify_plan_padding_core, app_verify_preflight_core,
    app_verify_prepare_public_input_rows_core, app_verify_reduce_core,
    app_verify_shipping_into_parts_core, app_verify_shipping_projection_core,
    AppVerifyAcceptedJoinProjection, AppVerifyAcceptedJoinProjectionError, AppVerifyCallId,
    AppVerifyCallResult, AppVerifyExpectedCall, AppVerifyFamilyCode, AppVerifyPlanError,
    AppVerifyPlannerIndexedExecutedRecord, AppVerifyPreflightError, AppVerifyReductionError,
    AppVerifySegmentRange, AppVerifyShippingCall, AppVerifyShippingInput, AppVerifyShippingResult,
    APP_VERIFY_PROTOCOL_VERSION,
};

#[doc(hidden)]
pub fn app_verify_family_code(family_id: ProofFamilyId) -> AppVerifyFamilyCode {
    let fields = family_proto_fields(family_id);
    AppVerifyFamilyCode {
        proof_family_id: fields.family_id,
        note_reshape_family_id: fields.note_reshape_family_id,
        shielded_withdrawal_family_id: fields.shielded_withdrawal_family_id,
    }
}
