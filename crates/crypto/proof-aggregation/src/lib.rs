//! Consensus proof-family aggregation transport and backend facade.
//!
//! The aggregation backend is implemented locally using vendored arkworks
//! SnarkPack code adapted from `ripp`.

mod backend;
mod bundle;
mod padding;
mod srs;
mod transcript;
mod transfer_family_dispatch;

use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_params::batch::BatchItem;

pub use backend::AggregateBuildBackendProfile;
use backend::SnarkpackBackend;
pub use backend::{
    set_rayon_threads_per_batch_for_bench, set_unchecked_aggregate_deserialization_for_bench,
    AggregateVerificationProfile, AggregationBackend,
};
pub use bundle::{AggregateBundle, FamilyAggregate, ProofFamilyId};
pub use padding::{pad_items_to_power_of_two, prepare_verify_inputs, PreparedVerifyInputs};
pub use srs::{
    srs_id, srs_report, DevSrs, DevSrsReport, DEFAULT_MAX_PADDED_PROOF_COUNT, DEV_SRS_BACKEND_ID,
    DEV_SRS_CURVE_ID, DEV_SRS_VERSION,
};

pub fn aggregate_family(
    family_id: ProofFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    SnarkpackBackend::aggregate_family(family_id, pvk, items, srs)
}

pub fn aggregate_family_profiled(
    family_id: ProofFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    SnarkpackBackend::aggregate_family_profiled(family_id, pvk, items, srs)
}

pub fn verify_family_aggregate(
    family_id: ProofFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<()> {
    SnarkpackBackend::verify_family_aggregate(
        family_id,
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}

pub fn verify_family_aggregate_profiled(
    family_id: ProofFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile> {
    SnarkpackBackend::verify_family_aggregate_profiled(
        family_id,
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}

pub fn verify_family_aggregate_profiled_unchecked(
    family_id: ProofFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile> {
    SnarkpackBackend::verify_family_aggregate_profiled_unchecked(
        family_id,
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}
