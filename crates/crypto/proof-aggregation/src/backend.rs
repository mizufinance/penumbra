use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{ensure, Result};
use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::applications::groth16_aggregation::{
    aggregate_proofs, aggregate_proofs_profiled, verify_aggregate_proof,
    verify_aggregate_proof_profiled, AggregateProof, AggregateProofBuildProfile,
    AggregateProofVerificationProfile,
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::{Bls12_377, Fq};
use digest::Digest;
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_shielded_pool::TransferFamilyId;

use crate::{
    srs::DevSrs,
    transcript::{
        ConvertTranscriptDigest, DelegatorVoteTranscriptDigest, OutputTranscriptDigest,
        SpendTranscriptDigest, SwapClaimTranscriptDigest, SwapTranscriptDigest,
    },
    transfer_family_dispatch::{
        aggregate_transfer_family_generated, aggregate_transfer_family_profiled_generated,
        verify_transfer_family_aggregate_generated,
        verify_transfer_family_aggregate_profiled_unchecked_generated,
    },
    ProofFamilyId,
};

#[derive(Clone, Debug)]
pub struct AggregateVerificationProfile {
    pub deserialize_ms: f64,
    pub challenge_ms: f64,
    pub tipa_ab_ms: f64,
    pub tipa_c_ms: f64,
    pub public_input_fold_ms: f64,
    pub ppe_ms: f64,
    pub core_total_ms: f64,
    pub total_ms: f64,
    pub accepted: bool,
}

impl Default for AggregateVerificationProfile {
    fn default() -> Self {
        Self {
            deserialize_ms: 0.0,
            challenge_ms: 0.0,
            tipa_ab_ms: 0.0,
            tipa_c_ms: 0.0,
            public_input_fold_ms: 0.0,
            ppe_ms: 0.0,
            core_total_ms: 0.0,
            total_ms: 0.0,
            accepted: true,
        }
    }
}

impl AggregateVerificationProfile {
    pub fn merge(&mut self, other: &Self) {
        self.deserialize_ms += other.deserialize_ms;
        self.challenge_ms += other.challenge_ms;
        self.tipa_ab_ms += other.tipa_ab_ms;
        self.tipa_c_ms += other.tipa_c_ms;
        self.public_input_fold_ms += other.public_input_fold_ms;
        self.ppe_ms += other.ppe_ms;
        self.core_total_ms += other.core_total_ms;
        self.total_ms += other.total_ms;
        self.accepted &= other.accepted;
    }
}

#[derive(Clone, Debug, Default)]
pub struct AggregateBuildBackendProfile {
    pub collect_proofs_ms: f64,
    pub backend_aggregate_ms: f64,
    pub backend_point_extract_ms: f64,
    pub backend_prepared_srs_ms: f64,
    pub backend_commitment_key_extract_ms: f64,
    pub backend_commitment_ms: f64,
    pub backend_com_a_ms: f64,
    pub backend_com_b_ms: f64,
    pub backend_com_c_ms: f64,
    pub backend_pairing_normalize_batch_ms: f64,
    pub backend_pairing_prepare_ms: f64,
    pub backend_pairing_miller_loop_ms: f64,
    pub backend_pairing_final_exponentiation_ms: f64,
    pub backend_randomizer_ms: f64,
    pub backend_structured_scalar_ms: f64,
    pub backend_weighted_a_ms: f64,
    pub backend_ip_ab_ms: f64,
    pub backend_agg_c_ms: f64,
    pub backend_ck_1_r_ms: f64,
    pub backend_consistency_check_ms: f64,
    pub backend_tipa_ab_ms: f64,
    pub backend_tipa_c_ms: f64,
    pub backend_tipa_ab_gipa_ms: f64,
    pub backend_tipa_ab_gipa_commit_l_ms: f64,
    pub backend_tipa_ab_gipa_commit_r_ms: f64,
    pub backend_tipa_ab_gipa_challenge_ms: f64,
    pub backend_tipa_ab_gipa_rescale_m1_ms: f64,
    pub backend_tipa_ab_gipa_rescale_m2_ms: f64,
    pub backend_tipa_ab_gipa_rescale_ck1_ms: f64,
    pub backend_tipa_ab_gipa_rescale_ck2_ms: f64,
    pub backend_tipa_ab_transcript_inverse_ms: f64,
    pub backend_tipa_ab_kzg_challenge_ms: f64,
    pub backend_tipa_ab_kzg_coefficient_build_ms: f64,
    pub backend_tipa_ab_kzg_eval_quotient_ms: f64,
    pub backend_tipa_ab_kzg_opening_msm_ms: f64,
    pub backend_tipa_ab_kzg_opening_ck_a_ms: f64,
    pub backend_tipa_ab_kzg_opening_ck_b_ms: f64,
    pub backend_tipa_c_gipa_ms: f64,
    pub backend_tipa_c_gipa_commit_l_ms: f64,
    pub backend_tipa_c_gipa_commit_r_ms: f64,
    pub backend_tipa_c_gipa_challenge_ms: f64,
    pub backend_tipa_c_gipa_rescale_m1_ms: f64,
    pub backend_tipa_c_gipa_rescale_m2_ms: f64,
    pub backend_tipa_c_gipa_rescale_ck1_ms: f64,
    pub backend_tipa_c_gipa_rescale_ck2_ms: f64,
    pub backend_tipa_c_transcript_inverse_ms: f64,
    pub backend_tipa_c_kzg_challenge_ms: f64,
    pub backend_tipa_c_kzg_coefficient_build_ms: f64,
    pub backend_tipa_c_kzg_eval_quotient_ms: f64,
    pub backend_tipa_c_kzg_opening_msm_ms: f64,
    pub backend_tipa_c_kzg_opening_ck_a_ms: f64,
    pub serialize_ms: f64,
    pub total_ms: f64,
}

pub trait AggregationBackend {
    type Srs;

    fn aggregate_family(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &Self::Srs,
    ) -> Result<Vec<u8>>;

    fn verify_family_aggregate(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &Self::Srs,
    ) -> Result<()>;
}

pub struct SnarkpackBackend;

static USE_UNCHECKED_AGGREGATE_DESERIALIZATION: AtomicBool = AtomicBool::new(false);

pub fn set_unchecked_aggregate_deserialization_for_bench(enabled: bool) {
    USE_UNCHECKED_AGGREGATE_DESERIALIZATION.store(enabled, Ordering::Relaxed);
}

/// Per-invocation rayon thread count. 1 = one dedicated thread per GIPA task (default).
/// 0 = use the global pool (all available threads shared across concurrent tasks).
static RAYON_THREADS_PER_BATCH: AtomicUsize = AtomicUsize::new(1);

/// Set the rayon thread count used per `aggregate_with_digest_profiled` call.
/// 1 is the production default (one dedicated thread per GIPA task, no cross-task stealing).
/// 0 falls back to the global pool.
pub fn set_rayon_threads_per_batch_for_bench(n: usize) {
    RAYON_THREADS_PER_BATCH.store(n, Ordering::Relaxed);
}

fn deserialize_aggregate_proof<D: Digest>(
    aggregate_proof_bytes: &[u8],
) -> Result<AggregateProof<Bls12_377, D>> {
    if USE_UNCHECKED_AGGREGATE_DESERIALIZATION.load(Ordering::Relaxed) {
        AggregateProof::<Bls12_377, D>::deserialize_compressed_unchecked(&aggregate_proof_bytes[..])
            .map_err(Into::into)
    } else {
        AggregateProof::<Bls12_377, D>::deserialize_compressed(&aggregate_proof_bytes[..])
            .map_err(Into::into)
    }
}

impl SnarkpackBackend {
    fn verify_transfer_family_aggregate_profiled_unchecked(
        family_id: TransferFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile> {
        verify_transfer_family_aggregate_profiled_unchecked_generated(
            family_id,
            pvk,
            aggregate_proof_bytes,
            padded_public_inputs,
            srs,
        )
    }

    fn aggregate_transfer_family(
        family_id: TransferFamilyId,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>> {
        aggregate_transfer_family_generated(family_id, items, srs)
    }

    fn verify_transfer_family_aggregate(
        family_id: TransferFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<bool> {
        verify_transfer_family_aggregate_generated(
            family_id,
            pvk,
            aggregate_proof_bytes,
            padded_public_inputs,
            srs,
        )
    }

    fn aggregate_transfer_family_profiled(
        family_id: TransferFamilyId,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
        aggregate_transfer_family_profiled_generated(family_id, items, srs)
    }

    pub fn verify_family_aggregate_profiled_unchecked(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile> {
        srs.ensure_supported_count(padded_public_inputs.len())?;
        ensure!(
            !padded_public_inputs.is_empty(),
            "cannot verify an empty aggregate for family {:?}",
            family_id
        );

        match family_id {
            ProofFamilyId::Spend => verify_with_digest_profiled::<SpendTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ProofFamilyId::Output => verify_with_digest_profiled::<OutputTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ProofFamilyId::Transfer(transfer_family_id) => {
                Self::verify_transfer_family_aggregate_profiled_unchecked(
                    transfer_family_id,
                    pvk,
                    aggregate_proof_bytes,
                    padded_public_inputs,
                    srs,
                )
            }
            ProofFamilyId::Swap => verify_with_digest_profiled::<SwapTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ProofFamilyId::SwapClaim => verify_with_digest_profiled::<SwapClaimTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ProofFamilyId::Convert => verify_with_digest_profiled::<ConvertTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ProofFamilyId::DelegatorVote => verify_with_digest_profiled::<
                DelegatorVoteTranscriptDigest,
            >(
                pvk, aggregate_proof_bytes, padded_public_inputs, srs
            ),
        }
    }

    pub fn verify_family_aggregate_profiled(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile> {
        let profile = Self::verify_family_aggregate_profiled_unchecked(
            family_id,
            pvk,
            aggregate_proof_bytes,
            padded_public_inputs,
            srs,
        )?;

        ensure!(
            profile.accepted,
            "SnarkPack verification rejected {:?}",
            family_id
        );
        Ok(profile)
    }
}

impl AggregationBackend for SnarkpackBackend {
    type Srs = DevSrs;

    fn aggregate_family(
        family_id: ProofFamilyId,
        _pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &Self::Srs,
    ) -> Result<Vec<u8>> {
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        match family_id {
            ProofFamilyId::Spend => aggregate_with_digest::<SpendTranscriptDigest>(items, srs),
            ProofFamilyId::Output => aggregate_with_digest::<OutputTranscriptDigest>(items, srs),
            ProofFamilyId::Transfer(transfer_family_id) => {
                Self::aggregate_transfer_family(transfer_family_id, items, srs)
            }
            ProofFamilyId::Swap => aggregate_with_digest::<SwapTranscriptDigest>(items, srs),
            ProofFamilyId::SwapClaim => {
                aggregate_with_digest::<SwapClaimTranscriptDigest>(items, srs)
            }
            ProofFamilyId::Convert => aggregate_with_digest::<ConvertTranscriptDigest>(items, srs),
            ProofFamilyId::DelegatorVote => {
                aggregate_with_digest::<DelegatorVoteTranscriptDigest>(items, srs)
            }
        }
    }

    fn verify_family_aggregate(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &Self::Srs,
    ) -> Result<()> {
        srs.ensure_supported_count(padded_public_inputs.len())?;
        ensure!(
            !padded_public_inputs.is_empty(),
            "cannot verify an empty aggregate for family {:?}",
            family_id
        );

        let accepted = match family_id {
            ProofFamilyId::Spend => verify_with_digest::<SpendTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
            ProofFamilyId::Output => verify_with_digest::<OutputTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
            ProofFamilyId::Transfer(transfer_family_id) => Self::verify_transfer_family_aggregate(
                transfer_family_id,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
            ProofFamilyId::Swap => verify_with_digest::<SwapTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
            ProofFamilyId::SwapClaim => verify_with_digest::<SwapClaimTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
            ProofFamilyId::Convert => verify_with_digest::<ConvertTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
            ProofFamilyId::DelegatorVote => verify_with_digest::<DelegatorVoteTranscriptDigest>(
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            )?,
        };

        ensure!(accepted, "SnarkPack verification rejected {:?}", family_id);
        Ok(())
    }
}

fn collect_proofs(items: &[BatchItem]) -> Vec<ark_groth16::Proof<Bls12_377>> {
    items.iter().map(|item| item.proof.clone()).collect()
}

impl SnarkpackBackend {
    pub fn aggregate_family_profiled(
        family_id: ProofFamilyId,
        _pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        let (bytes, profile) = match family_id {
            ProofFamilyId::Spend => {
                aggregate_with_digest_profiled::<SpendTranscriptDigest>(items, srs)
            }
            ProofFamilyId::Output => {
                aggregate_with_digest_profiled::<OutputTranscriptDigest>(items, srs)
            }
            ProofFamilyId::Transfer(transfer_family_id) => {
                Self::aggregate_transfer_family_profiled(transfer_family_id, items, srs)
            }
            ProofFamilyId::Swap => {
                aggregate_with_digest_profiled::<SwapTranscriptDigest>(items, srs)
            }
            ProofFamilyId::SwapClaim => {
                aggregate_with_digest_profiled::<SwapClaimTranscriptDigest>(items, srs)
            }
            ProofFamilyId::Convert => {
                aggregate_with_digest_profiled::<ConvertTranscriptDigest>(items, srs)
            }
            ProofFamilyId::DelegatorVote => {
                aggregate_with_digest_profiled::<DelegatorVoteTranscriptDigest>(items, srs)
            }
        }?;

        Ok((bytes, profile))
    }
}

pub(crate) fn aggregate_with_digest<D: Digest>(
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    let aggregate = aggregate_proofs::<Bls12_377, D>(&inner_product_srs, &collect_proofs(items))
        .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))?;
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn aggregate_with_digest_profiled<D: Digest>(
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    let rayon_threads = RAYON_THREADS_PER_BATCH.load(Ordering::Relaxed);
    if rayon_threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(rayon_threads)
            .build_scoped(
                |thread| thread.run(),
                |pool| pool.install(|| aggregate_with_digest_profiled_core::<D>(items, srs)),
            )
            .map_err(|e| anyhow::anyhow!("rayon pool build error: {e}"))?
    } else {
        aggregate_with_digest_profiled_core::<D>(items, srs)
    }
}

fn aggregate_with_digest_profiled_core<D: Digest>(
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    let mut profile = AggregateBuildBackendProfile::default();
    let total_start = Instant::now();

    let collect_start = Instant::now();
    let proofs = collect_proofs(items);
    profile.collect_proofs_ms = collect_start.elapsed().as_secs_f64() * 1000.0;

    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    let backend_start = Instant::now();
    let (aggregate, core_profile) =
        aggregate_proofs_profiled::<Bls12_377, D>(&inner_product_srs, &proofs)
            .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))?;
    profile.backend_aggregate_ms = backend_start.elapsed().as_secs_f64() * 1000.0;
    apply_core_build_profile(&mut profile, &core_profile);

    let serialize_start = Instant::now();
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    profile.serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;
    profile.total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

    Ok((bytes, profile))
}

fn apply_core_build_profile(
    profile: &mut AggregateBuildBackendProfile,
    core_profile: &AggregateProofBuildProfile,
) {
    profile.backend_point_extract_ms = core_profile.point_extract_ms;
    profile.backend_prepared_srs_ms = core_profile.prepared_srs_ms;
    profile.backend_commitment_key_extract_ms = core_profile.commitment_key_extract_ms;
    profile.backend_commitment_ms = core_profile.commitment_ms;
    profile.backend_com_a_ms = core_profile.com_a_ms;
    profile.backend_com_b_ms = core_profile.com_b_ms;
    profile.backend_com_c_ms = core_profile.com_c_ms;
    profile.backend_pairing_normalize_batch_ms = core_profile.pairing_normalize_batch_ms;
    profile.backend_pairing_prepare_ms = core_profile.pairing_prepare_ms;
    profile.backend_pairing_miller_loop_ms = core_profile.pairing_miller_loop_ms;
    profile.backend_pairing_final_exponentiation_ms = core_profile.pairing_final_exponentiation_ms;
    profile.backend_randomizer_ms = core_profile.randomizer_ms;
    profile.backend_structured_scalar_ms = core_profile.structured_scalar_ms;
    profile.backend_weighted_a_ms = core_profile.weighted_a_ms;
    profile.backend_ip_ab_ms = core_profile.ip_ab_ms;
    profile.backend_agg_c_ms = core_profile.agg_c_ms;
    profile.backend_ck_1_r_ms = core_profile.ck_1_r_ms;
    profile.backend_consistency_check_ms = core_profile.consistency_check_ms;
    profile.backend_tipa_ab_ms = core_profile.tipa_ab_ms;
    profile.backend_tipa_c_ms = core_profile.tipa_c_ms;
    profile.backend_tipa_ab_gipa_ms = core_profile.tipa_ab_gipa_ms;
    profile.backend_tipa_ab_gipa_commit_l_ms = core_profile.tipa_ab_gipa_commit_l_ms;
    profile.backend_tipa_ab_gipa_commit_r_ms = core_profile.tipa_ab_gipa_commit_r_ms;
    profile.backend_tipa_ab_gipa_challenge_ms = core_profile.tipa_ab_gipa_challenge_ms;
    profile.backend_tipa_ab_gipa_rescale_m1_ms = core_profile.tipa_ab_gipa_rescale_m1_ms;
    profile.backend_tipa_ab_gipa_rescale_m2_ms = core_profile.tipa_ab_gipa_rescale_m2_ms;
    profile.backend_tipa_ab_gipa_rescale_ck1_ms = core_profile.tipa_ab_gipa_rescale_ck1_ms;
    profile.backend_tipa_ab_gipa_rescale_ck2_ms = core_profile.tipa_ab_gipa_rescale_ck2_ms;
    profile.backend_tipa_ab_transcript_inverse_ms = core_profile.tipa_ab_transcript_inverse_ms;
    profile.backend_tipa_ab_kzg_challenge_ms = core_profile.tipa_ab_kzg_challenge_ms;
    profile.backend_tipa_ab_kzg_coefficient_build_ms =
        core_profile.tipa_ab_kzg_coefficient_build_ms;
    profile.backend_tipa_ab_kzg_eval_quotient_ms = core_profile.tipa_ab_kzg_eval_quotient_ms;
    profile.backend_tipa_ab_kzg_opening_msm_ms = core_profile.tipa_ab_kzg_opening_msm_ms;
    profile.backend_tipa_ab_kzg_opening_ck_a_ms = core_profile.tipa_ab_kzg_opening_ck_a_ms;
    profile.backend_tipa_ab_kzg_opening_ck_b_ms = core_profile.tipa_ab_kzg_opening_ck_b_ms;
    profile.backend_tipa_c_gipa_ms = core_profile.tipa_c_gipa_ms;
    profile.backend_tipa_c_gipa_commit_l_ms = core_profile.tipa_c_gipa_commit_l_ms;
    profile.backend_tipa_c_gipa_commit_r_ms = core_profile.tipa_c_gipa_commit_r_ms;
    profile.backend_tipa_c_gipa_challenge_ms = core_profile.tipa_c_gipa_challenge_ms;
    profile.backend_tipa_c_gipa_rescale_m1_ms = core_profile.tipa_c_gipa_rescale_m1_ms;
    profile.backend_tipa_c_gipa_rescale_m2_ms = core_profile.tipa_c_gipa_rescale_m2_ms;
    profile.backend_tipa_c_gipa_rescale_ck1_ms = core_profile.tipa_c_gipa_rescale_ck1_ms;
    profile.backend_tipa_c_gipa_rescale_ck2_ms = core_profile.tipa_c_gipa_rescale_ck2_ms;
    profile.backend_tipa_c_transcript_inverse_ms = core_profile.tipa_c_transcript_inverse_ms;
    profile.backend_tipa_c_kzg_challenge_ms = core_profile.tipa_c_kzg_challenge_ms;
    profile.backend_tipa_c_kzg_coefficient_build_ms = core_profile.tipa_c_kzg_coefficient_build_ms;
    profile.backend_tipa_c_kzg_eval_quotient_ms = core_profile.tipa_c_kzg_eval_quotient_ms;
    profile.backend_tipa_c_kzg_opening_msm_ms = core_profile.tipa_c_kzg_opening_msm_ms;
    profile.backend_tipa_c_kzg_opening_ck_a_ms = core_profile.tipa_c_kzg_opening_ck_a_ms;
}

pub(crate) fn verify_with_digest<D: Digest>(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool> {
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    verify_aggregate_proof::<Bls12_377, D>(
        srs.verifier_srs()?,
        &pvk.vk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| anyhow::anyhow!("SnarkPack verification failed: {e}"))
}

pub(crate) fn verify_with_digest_profiled<D: Digest>(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile> {
    let started = Instant::now();

    let deserialize_started = Instant::now();
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    let deserialize_ms = deserialize_started.elapsed().as_secs_f64() * 1000.0;

    let core_profile = verify_aggregate_proof_profiled::<Bls12_377, D>(
        srs.verifier_srs()?,
        &pvk.vk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| anyhow::anyhow!("SnarkPack verification failed: {e}"))?;

    Ok(profile_with_deserialize(
        core_profile,
        deserialize_ms,
        started.elapsed().as_secs_f64() * 1000.0,
    ))
}

fn profile_with_deserialize(
    core_profile: AggregateProofVerificationProfile,
    deserialize_ms: f64,
    total_ms: f64,
) -> AggregateVerificationProfile {
    AggregateVerificationProfile {
        deserialize_ms,
        challenge_ms: core_profile.challenge_ms,
        tipa_ab_ms: core_profile.tipa_ab_ms,
        tipa_c_ms: core_profile.tipa_c_ms,
        public_input_fold_ms: core_profile.public_input_fold_ms,
        ppe_ms: core_profile.ppe_ms,
        core_total_ms: core_profile.core_total_ms,
        total_ms,
        accepted: core_profile.accepted,
    }
}

#[cfg(test)]
mod tests {
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use ark_snark::SNARK;
    use decaf377::Fq;
    use penumbra_sdk_proof_params::batch;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    use crate::{
        aggregate_family, aggregate_family_profiled, pad_items_to_power_of_two,
        verify_family_aggregate, verify_family_aggregate_profiled,
    };

    use super::*;

    #[derive(Clone)]
    struct SquareCircuit {
        x: Option<Fq>,
    }

    impl ConstraintSynthesizer<Fq> for SquareCircuit {
        fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> Result<(), SynthesisError> {
            let x = FpVar::new_witness(cs.clone(), || {
                self.x.ok_or(SynthesisError::AssignmentMissing)
            })?;
            let x_sq = &x * &x;
            let public = FpVar::new_input(cs, || {
                let x = self.x.ok_or(SynthesisError::AssignmentMissing)?;
                Ok(x * x)
            })?;

            x_sq.enforce_equal(&public)?;
            Ok(())
        }
    }

    fn sample_items() -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
        let mut rng = ChaCha20Rng::seed_from_u64(7);
        let setup_circuit = SquareCircuit {
            x: Some(Fq::from(1u64)),
        };
        let pk =
            Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                setup_circuit,
                &mut rng,
            )
            .expect("setup should succeed");
        let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();

        let items = (0..3)
            .map(|_| {
                let x = Fq::rand(&mut rng);
                let circuit = SquareCircuit { x: Some(x) };
                let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                    .expect("proof generation should succeed");

                BatchItem {
                    proof,
                    public_inputs: vec![x * x],
                }
            })
            .collect();

        (pvk, items)
    }

    #[test]
    fn snarkpack_backend_accepts_valid_aggregate() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let aggregate = aggregate_family(ProofFamilyId::Spend, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let padded_public_inputs = padded_items
            .into_iter()
            .map(|item| item.public_inputs)
            .collect::<Vec<_>>();

        verify_family_aggregate(
            ProofFamilyId::Spend,
            &pvk,
            &aggregate,
            &padded_public_inputs,
            &srs,
        )
        .expect("aggregate verification should succeed");
    }

    #[test]
    fn snarkpack_backend_rejects_malformed_aggregate_bytes() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let mut aggregate = aggregate_family(ProofFamilyId::Spend, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        aggregate.truncate(aggregate.len() / 2);
        let padded_public_inputs = padded_items
            .into_iter()
            .map(|item| item.public_inputs)
            .collect::<Vec<_>>();

        let err = verify_family_aggregate(
            ProofFamilyId::Spend,
            &pvk,
            &aggregate,
            &padded_public_inputs,
            &srs,
        )
        .expect_err("malformed aggregate bytes should be rejected");

        assert!(
            err.to_string().contains("InvalidData")
                || err.to_string().contains("serialization")
                || err.to_string().contains("Not enough data")
                || err.to_string().contains("UnexpectedEof")
                || err.to_string().contains("failed to fill whole buffer"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn snarkpack_backend_rejects_mutated_public_inputs() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let aggregate = aggregate_family(ProofFamilyId::Spend, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let mut padded_public_inputs = padded_items
            .into_iter()
            .map(|item| item.public_inputs)
            .collect::<Vec<_>>();
        padded_public_inputs[0][0] += Fq::from(1u64);

        let err = verify_family_aggregate(
            ProofFamilyId::Spend,
            &pvk,
            &aggregate,
            &padded_public_inputs,
            &srs,
        )
        .expect_err("mutated public inputs should be rejected");

        assert!(
            err.to_string().contains("rejected") || err.to_string().contains("failed"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn snarkpack_backend_rejects_wrong_family_id() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let aggregate = aggregate_family(ProofFamilyId::Spend, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let padded_public_inputs = padded_items
            .into_iter()
            .map(|item| item.public_inputs)
            .collect::<Vec<_>>();

        let err = verify_family_aggregate(
            ProofFamilyId::Output,
            &pvk,
            &aggregate,
            &padded_public_inputs,
            &srs,
        )
        .expect_err("family transcript mismatch should be rejected");

        assert!(
            err.to_string().contains("rejected") || err.to_string().contains("failed"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn snarkpack_profile_accepts_valid_aggregate() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let aggregate = aggregate_family(ProofFamilyId::Spend, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let padded_public_inputs = padded_items
            .into_iter()
            .map(|item| item.public_inputs)
            .collect::<Vec<_>>();

        let profile = verify_family_aggregate_profiled(
            ProofFamilyId::Spend,
            &pvk,
            &aggregate,
            &padded_public_inputs,
            &srs,
        )
        .expect("aggregate verification should succeed");

        assert!(profile.accepted, "profiled verification should accept");
        assert!(profile.total_ms >= profile.deserialize_ms);
        assert!(profile.challenge_ms >= 0.0);
        assert!(profile.core_total_ms >= profile.tipa_ab_ms);
        assert!(profile.core_total_ms >= profile.tipa_c_ms);
        assert!(profile.public_input_fold_ms >= 0.0);
        assert!(profile.ppe_ms >= 0.0);
    }

    #[test]
    fn snarkpack_build_profile_exposes_tipa_subbuckets() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");

        let (_aggregate, profile) =
            aggregate_family_profiled(ProofFamilyId::Spend, &pvk, &padded_items, &srs)
                .expect("profiled aggregation should succeed");

        assert!(profile.backend_tipa_ab_ms >= profile.backend_tipa_ab_gipa_ms);
        assert!(profile.backend_tipa_c_ms >= profile.backend_tipa_c_gipa_ms);
        assert!(profile.backend_tipa_ab_gipa_ms > 0.0);
        assert!(profile.backend_tipa_c_gipa_ms > 0.0);
        assert!(profile.backend_prepared_srs_ms >= 0.0);
        assert!(profile.backend_commitment_key_extract_ms >= 0.0);
        assert!(profile.backend_com_a_ms >= 0.0);
        assert!(profile.backend_com_b_ms >= 0.0);
        assert!(profile.backend_com_c_ms >= 0.0);
        assert!(profile.backend_pairing_normalize_batch_ms >= 0.0);
        assert!(profile.backend_pairing_prepare_ms >= 0.0);
        assert!(profile.backend_pairing_miller_loop_ms >= 0.0);
        assert!(profile.backend_pairing_final_exponentiation_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_coefficient_build_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_eval_quotient_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_opening_msm_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_opening_ck_a_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_opening_ck_b_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_coefficient_build_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_eval_quotient_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_opening_msm_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_opening_ck_a_ms >= 0.0);

        // Subtotals use only direct children of the tipa_ab/tipa_c spans.
        // kzg_coefficient_build_ms, kzg_eval_quotient_ms, kzg_opening_msm_ms are
        // accumulated sums of ck_a and ck_b sub-operations — already contained
        // within kzg_opening_ck_a_ms and kzg_opening_ck_b_ms — so they are not
        // included here to avoid double-counting.
        let tipa_ab_subtotal = profile.backend_tipa_ab_gipa_ms
            + profile.backend_tipa_ab_transcript_inverse_ms
            + profile.backend_tipa_ab_kzg_challenge_ms
            + profile.backend_tipa_ab_kzg_opening_ck_a_ms
            + profile.backend_tipa_ab_kzg_opening_ck_b_ms;
        let tipa_c_subtotal = profile.backend_tipa_c_gipa_ms
            + profile.backend_tipa_c_transcript_inverse_ms
            + profile.backend_tipa_c_kzg_challenge_ms
            + profile.backend_tipa_c_kzg_opening_ck_a_ms;

        assert!(profile.backend_tipa_ab_ms + 5.0 >= tipa_ab_subtotal);
        assert!(profile.backend_tipa_c_ms + 5.0 >= tipa_c_subtotal);
        // kzg sub-ops are bounded by their ck wrapper spans
        assert!(
            profile.backend_tipa_ab_kzg_opening_ck_a_ms
                + profile.backend_tipa_ab_kzg_opening_ck_b_ms
                + 1.0
                >= profile.backend_tipa_ab_kzg_coefficient_build_ms
                    + profile.backend_tipa_ab_kzg_eval_quotient_ms
                    + profile.backend_tipa_ab_kzg_opening_msm_ms
        );
        assert!(
            profile.backend_tipa_ab_gipa_rescale_ck1_ms
                + profile.backend_tipa_ab_gipa_rescale_ck2_ms
                > 0.0
        );
    }

    #[test]
    fn snarkpack_matches_legacy_batch_across_families_and_counts() {
        let (pvk, items) = sample_items();
        let base_item = items.first().expect("at least one sample item").clone();
        let srs = DevSrs::default();

        for family_id in [
            ProofFamilyId::Spend,
            ProofFamilyId::Output,
            ProofFamilyId::Swap,
            ProofFamilyId::SwapClaim,
            ProofFamilyId::Convert,
            ProofFamilyId::DelegatorVote,
        ] {
            for count in [1usize, 2, 4, 8, 64, 256, 1024] {
                let repeated = vec![base_item.clone(); count];
                batch::batch_verify(&pvk, &repeated)
                    .expect("legacy batch verify should accept repeated valid proofs");

                let aggregate = aggregate_family(family_id, &pvk, &repeated, &srs)
                    .expect("aggregation should succeed");
                let padded_public_inputs = repeated
                    .iter()
                    .map(|item| item.public_inputs.clone())
                    .collect::<Vec<_>>();

                verify_family_aggregate(family_id, &pvk, &aggregate, &padded_public_inputs, &srs)
                    .expect("SnarkPack verify should match legacy batch verify");
            }
        }
    }
}
