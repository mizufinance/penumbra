//! Consensus proof-family aggregation transport and backend facade.
//!
//! The aggregation backend is Shieldd-owned SnarkPack implementation code
//! forked from `arkworks-rs/ripp`.

mod aggregate_proof_wrapper;
mod app_verifier;
mod backend;
mod bundle;
mod canonical_encoding;
mod padding;
mod preflight;
mod srs;
mod statement;
mod strict_deserialize;
mod torus;
mod transcript;
mod transfer_family_dispatch;

use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use decaf377::Bls12_377;
use shieldd_sdk_proof_params::batch::BatchItem;

pub use aggregate_proof_wrapper::{
    decode_wrapped_aggregate_proof, decode_wrapped_aggregate_proof_inner_range,
    decode_wrapped_torus_aggregate_proof, encode_wrapped_aggregate_proof,
    encode_wrapped_torus_aggregate_proof, AggregateProofBytesError,
    AGGREGATE_PROOF_TORUS_WRAPPER_DOMAIN, AGGREGATE_PROOF_WRAPPER_DOMAIN,
    MAX_AGGREGATE_PROOF_BYTES,
};
#[doc(hidden)]
pub use app_verifier::{
    app_verify_accepted_join_projection_core, app_verify_family_code, app_verify_family_count_core,
    app_verify_join_acceptance_core, app_verify_plan_identity_core, app_verify_plan_ids_core,
    app_verify_plan_padding_core, app_verify_preflight_core, app_verify_reduce_core,
    app_verify_shipping_projection_core, AppVerifyAcceptedJoinProjection,
    AppVerifyAcceptedJoinProjectionError, AppVerifyCallId, AppVerifyCallResult,
    AppVerifyExpectedCall, AppVerifyFamilyCode, AppVerifyPlanError,
    AppVerifyPlannerIndexedExecutedRecord, AppVerifyPreflightError, AppVerifyReductionError,
    AppVerifySegmentRange, AppVerifyShippingCall, AppVerifyShippingInput, AppVerifyShippingResult,
};
#[doc(hidden)]
pub use ark_ip_proofs::applications::groth16_aggregation::ShippingVerifierObservation;
use backend::SnarkpackBackend;
pub use backend::{AggregateVerifyError, ShippingAggregateVerification};
pub use bundle::{AggregateBundle, FamilyAggregate, ProofFamilyId};
pub use padding::PADDING_RULE_DOMAIN;
pub use padding::{pad_items_to_power_of_two, prepare_verify_inputs, PreparedVerifyInputs};
pub use preflight::{
    preflight_aggregate_verify, AggregatePreflightInput, VerifiedAggregateBackendCall,
    VerifiedChallengeContext, VerifiedInnerProofBytes,
};
pub use srs::{
    load_active_production_srs, load_production_srs_for_id, srs_id, srs_report, DevSrs,
    DevSrsReport, DEFAULT_DEV_SRS_ID, DEFAULT_MAX_PADDED_PROOF_COUNT, DEV_SRS_BACKEND_ID,
    DEV_SRS_CURVE_ID, PRODUCTION_SRS_ARTIFACT_DIR_ENV,
};
pub use statement::{
    aggregate_verification_key_digest, challenge_context, encode_statement, statement_digest,
    validate_counts, validate_repeat_final_padding, validate_row_arity, AggregateStatement,
    AggregateStatementError, StatementEncodingInput, StatementFieldBytes, StatementPaddedRows,
    StatementPublicInputRow, AGGREGATE_PROTOCOL_VERSION,
};
pub fn aggregate_family(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    SnarkpackBackend::aggregate_family(statement, pvk, items, srs)
}

/// Build the SnarkPack torus-v2 wire format while retaining the v1 proof
/// relation and Fiat-Shamir transcript.
pub fn aggregate_family_torus(
    statement: &AggregateStatement,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    SnarkpackBackend::aggregate_family_torus(statement, items, srs)
}

pub fn verify_family_aggregate(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    srs: &DevSrs,
) -> std::result::Result<(), AggregateVerifyError> {
    SnarkpackBackend::verify_family_aggregate(statement, pvk, aggregate_proof_bytes, srs)
}

/// Verify a torus aggregate after reconstructing and validating every standard
/// target-group value.
pub fn verify_family_aggregate_torus(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    srs: &DevSrs,
) -> std::result::Result<(), AggregateVerifyError> {
    SnarkpackBackend::verify_family_aggregate_torus(statement, pvk, aggregate_proof_bytes, srs)
}

#[doc(hidden)]
pub fn verify_shipping_family_aggregate(
    application_call: AppVerifyShippingCall,
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    srs: &DevSrs,
) -> std::result::Result<ShippingAggregateVerification, AggregateVerifyError> {
    SnarkpackBackend::verify_shipping_family_aggregate(
        application_call,
        statement,
        pvk,
        aggregate_proof_bytes,
        srs,
    )
}

#[cfg(any(test, feature = "fuzzing"))]
pub fn deserialize_aggregate_proof_for_fuzz(
    aggregate_proof_bytes: &[u8],
) -> std::result::Result<(), AggregateVerifyError> {
    backend::deserialize_aggregate_proof::<transcript::TransferTranscriptDigest>(
        aggregate_proof_bytes,
    )
    .map(|_| ())
}

#[cfg(test)]
mod decoder_corpus_tests {
    #[test]
    fn curated_inner_proofs_reach_successful_decoding() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../proof-aggregation-fuzz/corpus/deserialize_aggregate_proof");
        for index in [0, 1, 2, 3, 7, 11, 15] {
            let name = format!("valid-baseline-{index:02}");
            let bytes = std::fs::read(directory.join(&name)).expect("committed corpus seed");
            super::deserialize_aggregate_proof_for_fuzz(&bytes).expect(&name);
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(super::deserialize_aggregate_proof_for_fuzz(&trailing).is_err());
            assert!(
                super::deserialize_aggregate_proof_for_fuzz(&bytes[..bytes.len() - 1]).is_err()
            );
        }
    }
}
