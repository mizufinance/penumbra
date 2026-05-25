use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_params::batch::BatchItem;

use crate::{
    backend::{AggregateBuildBackendProfile, AggregateVerificationProfile},
    srs::DevSrs,
    transcript::TransferTranscriptDigest,
};

pub(crate) fn verify_transfer_aggregate_profiled_unchecked(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile> {
    crate::backend::verify_with_digest_profiled::<TransferTranscriptDigest>(
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}

pub(crate) fn aggregate_transfer(items: &[BatchItem], srs: &DevSrs) -> Result<Vec<u8>> {
    crate::backend::aggregate_with_digest::<TransferTranscriptDigest>(items, srs)
}

pub(crate) fn verify_transfer_aggregate(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool> {
    crate::backend::verify_with_digest::<TransferTranscriptDigest>(
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}

pub(crate) fn aggregate_transfer_profiled(
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    crate::backend::aggregate_with_digest_profiled::<TransferTranscriptDigest>(items, srs)
}
