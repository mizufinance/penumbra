use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::challenge::ChallengeContext;
use decaf377::{Bls12_377, Fq};
use shieldd_sdk_proof_params::batch::BatchItem;

use crate::{backend::AggregateVerifyError, srs::DevSrs, transcript::TransferTranscriptDigest};

pub(crate) fn aggregate_transfer_real_count(
    challenge_context: &ChallengeContext,
    items: &[BatchItem],
    real_count: usize,
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    crate::backend::aggregate_with_digest_real_count::<TransferTranscriptDigest>(
        challenge_context,
        items,
        real_count,
        srs,
    )
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
