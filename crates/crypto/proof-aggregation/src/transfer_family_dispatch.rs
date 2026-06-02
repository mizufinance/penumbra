use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::challenge::ChallengeContext;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_params::batch::BatchItem;

use crate::{
    backend::{AggregateBuildBackendProfile, AggregateVerificationProfile, AggregateVerifyError},
    srs::DevSrs,
    transcript::TransferTranscriptDigest,
};

pub(crate) fn verify_transfer_aggregate_profiled_status(
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
    crate::backend::verify_with_digest_profiled::<TransferTranscriptDigest>(
        challenge_context,
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}

pub(crate) fn aggregate_transfer(
    challenge_context: &ChallengeContext,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    crate::backend::aggregate_with_digest::<TransferTranscriptDigest>(challenge_context, items, srs)
}

pub(crate) fn verify_transfer_aggregate(
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool, AggregateVerifyError> {
    crate::backend::verify_with_digest::<TransferTranscriptDigest>(
        challenge_context,
        pvk,
        aggregate_proof_bytes,
        padded_public_inputs,
        srs,
    )
}

pub(crate) fn aggregate_transfer_profiled(
    items: &[BatchItem],
    srs: &DevSrs,
    challenge_context: &ChallengeContext,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    crate::backend::aggregate_with_digest_profiled::<TransferTranscriptDigest>(
        items,
        srs,
        challenge_context,
    )
}
