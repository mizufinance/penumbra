//! Consensus proof-family aggregation transport and backend facade.
//!
//! The aggregation backend is implemented locally using vendored arkworks
//! SnarkPack code adapted from `ripp`.

mod aggregate_proof_wrapper;
mod backend;
mod bundle;
mod padding;
mod srs;
mod statement;
mod transcript;
mod transfer_family_dispatch;

use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use decaf377::Bls12_377;
use penumbra_sdk_proof_params::batch::BatchItem;

pub use aggregate_proof_wrapper::{
    decode_wrapped_aggregate_proof, encode_wrapped_aggregate_proof, AggregateProofBytesError,
    AGGREGATE_PROOF_WRAPPER_DOMAIN, MAX_AGGREGATE_PROOF_BYTES,
};
pub use backend::AggregateBuildBackendProfile;
use backend::SnarkpackBackend;
pub use backend::{
    set_rayon_threads_per_batch_for_bench, AggregateVerificationProfile, AggregateVerifyError,
    AggregationBackend,
};
pub use bundle::{AggregateBundle, FamilyAggregate, ProofFamilyId};
pub use padding::PADDING_RULE_DOMAIN;
pub use padding::{pad_items_to_power_of_two, prepare_verify_inputs, PreparedVerifyInputs};
pub use srs::{
    srs_id, srs_report, DevSrs, DevSrsReport, DEFAULT_MAX_PADDED_PROOF_COUNT, DEV_SRS_BACKEND_ID,
    DEV_SRS_CURVE_ID, DEV_SRS_VERSION,
};
pub use statement::{
    aggregate_verification_key_digest, challenge_context, encode_statement, statement_digest,
    validate_counts, validate_row_arity, AggregateStatement, AggregateStatementError,
    StatementEncodingInput, AGGREGATE_PROTOCOL_VERSION,
};

pub fn aggregate_family(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    SnarkpackBackend::aggregate_family(statement, pvk, items, srs)
}

pub fn aggregate_family_profiled(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    SnarkpackBackend::aggregate_family_profiled(statement, pvk, items, srs)
}

pub fn verify_family_aggregate(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    srs: &DevSrs,
) -> std::result::Result<(), AggregateVerifyError> {
    SnarkpackBackend::verify_family_aggregate(statement, pvk, aggregate_proof_bytes, srs)
}

pub fn verify_family_aggregate_profiled(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    srs: &DevSrs,
) -> std::result::Result<AggregateVerificationProfile, AggregateVerifyError> {
    SnarkpackBackend::verify_family_aggregate_profiled(statement, pvk, aggregate_proof_bytes, srs)
}

pub fn verify_family_aggregate_profiled_status(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    srs: &DevSrs,
) -> std::result::Result<AggregateVerificationProfile, AggregateVerifyError> {
    SnarkpackBackend::verify_family_aggregate_profiled_status(
        statement,
        pvk,
        aggregate_proof_bytes,
        srs,
    )
}
