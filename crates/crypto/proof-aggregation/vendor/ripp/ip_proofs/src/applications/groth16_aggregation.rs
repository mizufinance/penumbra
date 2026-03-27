use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ff::{Field, One, Zero};
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

use ark_std::rand::Rng;
use digest::Digest;
use std::time::Instant;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::{
    tipa::{
        prove_pairing_inner_product_with_prepared_srs_shift,
        prove_pairing_inner_product_with_prepared_srs_shift_profiled,
        structured_scalar_message::{
            structured_scalar_power, TIPAWithSSM, TIPAWithSSMProof, TipaWithSsmBuildProfile,
        },
        TIPAProof, TipaBuildProfile, VerifierSRS, SRS, TIPA,
    },
    Error,
};
use ark_dh_commitments::{
    afgho16::{AFGHOCommitmentG1, AFGHOCommitmentG2},
    identity::{HomomorphicPlaceholderValue, IdentityCommitment, IdentityOutput},
};
use ark_inner_products::{
    cfg_multi_pairing, pairing_profile_snapshot, reset_pairing_profile_accumulator, InnerProduct,
    MultiexponentiationInnerProduct, PairingComputationProfile, PairingInnerProduct,
};

type PairingInnerProductAB<P, D> = TIPA<
    PairingInnerProduct<P>,
    AFGHOCommitmentG1<P>,
    AFGHOCommitmentG2<P>,
    IdentityCommitment<PairingOutput<P>, <P as Pairing>::ScalarField>,
    P,
    D,
>;

type PairingInnerProductABProof<P, D> = TIPAProof<
    PairingInnerProduct<P>,
    AFGHOCommitmentG1<P>,
    AFGHOCommitmentG2<P>,
    IdentityCommitment<PairingOutput<P>, <P as Pairing>::ScalarField>,
    P,
    D,
>;

type MultiExpInnerProductC<P, D> = TIPAWithSSM<
    MultiexponentiationInnerProduct<<P as Pairing>::G1>,
    AFGHOCommitmentG1<P>,
    IdentityCommitment<<P as Pairing>::G1, <P as Pairing>::ScalarField>,
    P,
    D,
>;

type MultiExpInnerProductCProof<P, D> = TIPAWithSSMProof<
    MultiexponentiationInnerProduct<<P as Pairing>::G1>,
    AFGHOCommitmentG1<P>,
    IdentityCommitment<<P as Pairing>::G1, <P as Pairing>::ScalarField>,
    P,
    D,
>;

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct AggregateProof<P: Pairing, D: Digest> {
    com_a: PairingOutput<P>,
    com_b: PairingOutput<P>,
    com_c: PairingOutput<P>,
    ip_ab: PairingOutput<P>,
    agg_c: P::G1,
    tipa_proof_ab: PairingInnerProductABProof<P, D>,
    tipa_proof_c: MultiExpInnerProductCProof<P, D>,
}

#[derive(Clone, Debug, Default)]
pub struct AggregateProofVerificationProfile {
    pub challenge_ms: f64,
    pub tipa_ab_ms: f64,
    pub tipa_c_ms: f64,
    pub public_input_fold_ms: f64,
    pub ppe_ms: f64,
    pub core_total_ms: f64,
    pub accepted: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AggregateProofBuildProfile {
    pub point_extract_ms: f64,
    pub prepared_srs_ms: f64,
    pub commitment_key_extract_ms: f64,
    pub commitment_ms: f64,
    pub com_a_ms: f64,
    pub com_b_ms: f64,
    pub com_c_ms: f64,
    pub pairing_normalize_batch_ms: f64,
    pub pairing_prepare_ms: f64,
    pub pairing_miller_loop_ms: f64,
    pub pairing_final_exponentiation_ms: f64,
    pub randomizer_ms: f64,
    pub structured_scalar_ms: f64,
    pub weighted_a_ms: f64,
    pub ip_ab_ms: f64,
    pub agg_c_ms: f64,
    pub ck_1_r_ms: f64,
    pub consistency_check_ms: f64,
    pub tipa_ab_ms: f64,
    pub tipa_c_ms: f64,
    pub tipa_ab_gipa_ms: f64,
    pub tipa_ab_gipa_commit_l_ms: f64,
    pub tipa_ab_gipa_commit_r_ms: f64,
    pub tipa_ab_gipa_challenge_ms: f64,
    pub tipa_ab_gipa_rescale_m1_ms: f64,
    pub tipa_ab_gipa_rescale_m2_ms: f64,
    pub tipa_ab_gipa_rescale_ck1_ms: f64,
    pub tipa_ab_gipa_rescale_ck2_ms: f64,
    pub tipa_ab_transcript_inverse_ms: f64,
    pub tipa_ab_kzg_challenge_ms: f64,
    pub tipa_ab_kzg_coefficient_build_ms: f64,
    pub tipa_ab_kzg_eval_quotient_ms: f64,
    pub tipa_ab_kzg_opening_msm_ms: f64,
    pub tipa_ab_kzg_opening_ck_a_ms: f64,
    pub tipa_ab_kzg_opening_ck_b_ms: f64,
    pub tipa_c_gipa_ms: f64,
    pub tipa_c_gipa_commit_l_ms: f64,
    pub tipa_c_gipa_commit_r_ms: f64,
    pub tipa_c_gipa_challenge_ms: f64,
    pub tipa_c_gipa_rescale_m1_ms: f64,
    pub tipa_c_gipa_rescale_m2_ms: f64,
    pub tipa_c_gipa_rescale_ck1_ms: f64,
    pub tipa_c_gipa_rescale_ck2_ms: f64,
    pub tipa_c_transcript_inverse_ms: f64,
    pub tipa_c_kzg_challenge_ms: f64,
    pub tipa_c_kzg_coefficient_build_ms: f64,
    pub tipa_c_kzg_eval_quotient_ms: f64,
    pub tipa_c_kzg_opening_msm_ms: f64,
    pub tipa_c_kzg_opening_ck_a_ms: f64,
    pub total_ms: f64,
}

pub fn setup_inner_product<P, D, R: Rng>(rng: &mut R, size: usize) -> Result<SRS<P>, Error>
where
    P: Pairing,
    D: Digest,
{
    let (srs, _) = PairingInnerProductAB::<P, D>::setup(rng, size)?;
    Ok(srs)
}

pub fn aggregate_proofs<P, D>(
    ip_srs: &SRS<P>,
    proofs: &[Proof<P>],
) -> Result<AggregateProof<P, D>, Error>
where
    P: Pairing,
    D: Digest,
{
    let a = proofs
        .iter()
        .map(|proof| proof.a.into())
        .collect::<Vec<P::G1>>();
    let b = proofs
        .iter()
        .map(|proof| proof.b.into())
        .collect::<Vec<P::G2>>();
    let c = proofs
        .iter()
        .map(|proof| proof.c.into())
        .collect::<Vec<P::G1>>();

    let prepared_srs = ip_srs.prepare_for_proving();
    let (ck_1, ck_2) = prepared_srs.commitment_keys();

    let com_a = PairingInnerProduct::<P>::inner_product(&a, ck_1)?;
    let com_b = PairingInnerProduct::<P>::inner_product(ck_2, &b)?;
    let com_c = PairingInnerProduct::<P>::inner_product(&c, ck_1)?;

    // Random linear combination of proofs
    let mut counter_nonce: usize = 0;
    let r = loop {
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&counter_nonce.to_be_bytes()[..]);
        com_a.serialize_uncompressed(&mut hash_input)?;
        com_b.serialize_uncompressed(&mut hash_input)?;
        com_c.serialize_uncompressed(&mut hash_input)?;
        if let Some(r) = <P::ScalarField>::from_random_bytes(&D::digest(&hash_input)) {
            break r;
        };
        counter_nonce += 1;
    };

    let r_vec = structured_scalar_power(proofs.len(), &r);
    let a_r = a
        .iter()
        .zip(&r_vec)
        .map(|(&a, r)| a * r)
        .collect::<Vec<P::G1>>();
    let ip_ab = PairingInnerProduct::<P>::inner_product(&a_r, &b)?;
    let agg_c = MultiexponentiationInnerProduct::<P::G1>::inner_product(&c, &r_vec)?;

    let ck_1_r = build_shifted_ck_1::<P>(&ck_1, &r);

    #[cfg(debug_assertions)]
    assert_eq!(
        com_a,
        PairingInnerProduct::<P>::inner_product(&a_r, &ck_1_r)?
    );

    let tipa_proof_ab = prove_pairing_inner_product_with_prepared_srs_shift::<P, D>(
        &prepared_srs,
        (&a_r, &b),
        (&ck_1_r, ck_2, &HomomorphicPlaceholderValue),
        &r,
    )?;

    let tipa_proof_c =
        MultiExpInnerProductC::<P, D>::prove_with_prepared_structured_scalar_message(
            &prepared_srs,
            (&c, &r_vec),
            (ck_1, &HomomorphicPlaceholderValue),
        )?;

    Ok(AggregateProof {
        com_a,
        com_b,
        com_c,
        ip_ab,
        agg_c,
        tipa_proof_ab,
        tipa_proof_c,
    })
}

pub fn aggregate_proofs_profiled<P, D>(
    ip_srs: &SRS<P>,
    proofs: &[Proof<P>],
) -> Result<(AggregateProof<P, D>, AggregateProofBuildProfile), Error>
where
    P: Pairing,
    D: Digest,
{
    let started = Instant::now();
    let mut profile = AggregateProofBuildProfile::default();

    let point_extract_started = Instant::now();
    let a = proofs
        .iter()
        .map(|proof| proof.a.into())
        .collect::<Vec<P::G1>>();
    let b = proofs
        .iter()
        .map(|proof| proof.b.into())
        .collect::<Vec<P::G2>>();
    let c = proofs
        .iter()
        .map(|proof| proof.c.into())
        .collect::<Vec<P::G1>>();
    profile.point_extract_ms = point_extract_started.elapsed().as_secs_f64() * 1000.0;

    let prepared_srs_started = Instant::now();
    let prepared_srs = ip_srs.prepare_for_proving();
    profile.prepared_srs_ms = prepared_srs_started.elapsed().as_secs_f64() * 1000.0;

    let commitment_key_extract_started = Instant::now();
    let (ck_1, ck_2) = prepared_srs.commitment_keys();
    profile.commitment_key_extract_ms =
        commitment_key_extract_started.elapsed().as_secs_f64() * 1000.0;

    reset_pairing_profile_accumulator();
    let commitment_started = Instant::now();
    let com_a_started = Instant::now();
    let com_a = PairingInnerProduct::<P>::inner_product(&a, ck_1)?;
    profile.com_a_ms = com_a_started.elapsed().as_secs_f64() * 1000.0;
    let com_b_started = Instant::now();
    let com_b = PairingInnerProduct::<P>::inner_product(ck_2, &b)?;
    profile.com_b_ms = com_b_started.elapsed().as_secs_f64() * 1000.0;
    let com_c_started = Instant::now();
    let com_c = PairingInnerProduct::<P>::inner_product(&c, ck_1)?;
    profile.com_c_ms = com_c_started.elapsed().as_secs_f64() * 1000.0;
    profile.commitment_ms = commitment_started.elapsed().as_secs_f64() * 1000.0;

    let randomizer_started = Instant::now();
    let mut counter_nonce: usize = 0;
    let r = loop {
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&counter_nonce.to_be_bytes()[..]);
        com_a.serialize_uncompressed(&mut hash_input)?;
        com_b.serialize_uncompressed(&mut hash_input)?;
        com_c.serialize_uncompressed(&mut hash_input)?;
        if let Some(r) = <P::ScalarField>::from_random_bytes(&D::digest(&hash_input)) {
            break r;
        };
        counter_nonce += 1;
    };
    profile.randomizer_ms = randomizer_started.elapsed().as_secs_f64() * 1000.0;

    let structured_scalar_started = Instant::now();
    let r_vec = structured_scalar_power(proofs.len(), &r);
    profile.structured_scalar_ms = structured_scalar_started.elapsed().as_secs_f64() * 1000.0;

    let weighted_a_started = Instant::now();
    let a_r = a
        .iter()
        .zip(&r_vec)
        .map(|(&a, r)| a * r)
        .collect::<Vec<P::G1>>();
    profile.weighted_a_ms = weighted_a_started.elapsed().as_secs_f64() * 1000.0;

    let ip_ab_started = Instant::now();
    let ip_ab = PairingInnerProduct::<P>::inner_product(&a_r, &b)?;
    profile.ip_ab_ms = ip_ab_started.elapsed().as_secs_f64() * 1000.0;

    let agg_c_started = Instant::now();
    let agg_c = MultiexponentiationInnerProduct::<P::G1>::inner_product(&c, &r_vec)?;
    profile.agg_c_ms = agg_c_started.elapsed().as_secs_f64() * 1000.0;

    let ck_1_r_started = Instant::now();
    let ck_1_r = build_shifted_ck_1::<P>(&ck_1, &r);
    profile.ck_1_r_ms = ck_1_r_started.elapsed().as_secs_f64() * 1000.0;

    #[cfg(debug_assertions)]
    {
        let consistency_started = Instant::now();
        assert_eq!(
            com_a,
            PairingInnerProduct::<P>::inner_product(&a_r, &ck_1_r)?
        );
        profile.consistency_check_ms = consistency_started.elapsed().as_secs_f64() * 1000.0;
    }

    let tipa_ab_started = Instant::now();
    let (tipa_proof_ab, tipa_ab_profile) =
        prove_pairing_inner_product_with_prepared_srs_shift_profiled::<P, D>(
            &prepared_srs,
            (&a_r, &b),
            (&ck_1_r, ck_2, &HomomorphicPlaceholderValue),
            &r,
        )?;
    profile.tipa_ab_ms = tipa_ab_started.elapsed().as_secs_f64() * 1000.0;
    apply_tipa_ab_profile(&mut profile, &tipa_ab_profile);

    let tipa_c_started = Instant::now();
    let (tipa_proof_c, tipa_c_profile) =
        MultiExpInnerProductC::<P, D>::prove_with_prepared_structured_scalar_message_profiled(
            &prepared_srs,
            (&c, &r_vec),
            (ck_1, &HomomorphicPlaceholderValue),
        )?;
    profile.tipa_c_ms = tipa_c_started.elapsed().as_secs_f64() * 1000.0;
    apply_tipa_c_profile(&mut profile, &tipa_c_profile);
    apply_pairing_profile(&mut profile, &pairing_profile_snapshot());
    profile.total_ms = started.elapsed().as_secs_f64() * 1000.0;

    Ok((
        AggregateProof {
            com_a,
            com_b,
            com_c,
            ip_ab,
            agg_c,
            tipa_proof_ab,
            tipa_proof_c,
        },
        profile,
    ))
}

pub fn verify_aggregate_proof<P, D>(
    ip_verifier_srs: &VerifierSRS<P>,
    vk: &VerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>], //TODO: Should use ToConstraintField instead
    proof: &AggregateProof<P, D>,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest,
{
    let r = derive_randomizer::<P, D>(proof)?;
    let tipa_proof_ab_valid = verify_tipa_ab::<P, D>(ip_verifier_srs, proof, &r)?;
    let tipa_proof_c_valid = verify_tipa_c::<P, D>(ip_verifier_srs, proof, &r)?;
    let (r_sum, g_ic) = fold_public_inputs::<P>(vk, public_inputs, &r);
    let ppe_valid = verify_ppe::<P>(vk, proof, &r_sum, g_ic);

    Ok(tipa_proof_ab_valid && tipa_proof_c_valid && ppe_valid)
}

pub fn verify_aggregate_proof_profiled<P, D>(
    ip_verifier_srs: &VerifierSRS<P>,
    vk: &VerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &AggregateProof<P, D>,
) -> Result<AggregateProofVerificationProfile, Error>
where
    P: Pairing,
    D: Digest,
{
    let started = Instant::now();

    let challenge_started = Instant::now();
    let r = derive_randomizer::<P, D>(proof)?;
    let challenge_ms = challenge_started.elapsed().as_secs_f64() * 1000.0;

    let tipa_ab_started = Instant::now();
    let tipa_proof_ab_valid = verify_tipa_ab::<P, D>(ip_verifier_srs, proof, &r)?;
    let tipa_ab_ms = tipa_ab_started.elapsed().as_secs_f64() * 1000.0;

    let tipa_c_started = Instant::now();
    let tipa_proof_c_valid = verify_tipa_c::<P, D>(ip_verifier_srs, proof, &r)?;
    let tipa_c_ms = tipa_c_started.elapsed().as_secs_f64() * 1000.0;

    let public_input_fold_started = Instant::now();
    let (r_sum, g_ic) = fold_public_inputs::<P>(vk, public_inputs, &r);
    let public_input_fold_ms = public_input_fold_started.elapsed().as_secs_f64() * 1000.0;

    let ppe_started = Instant::now();
    let ppe_valid = verify_ppe::<P>(vk, proof, &r_sum, g_ic);
    let ppe_ms = ppe_started.elapsed().as_secs_f64() * 1000.0;

    Ok(AggregateProofVerificationProfile {
        challenge_ms,
        tipa_ab_ms,
        tipa_c_ms,
        public_input_fold_ms,
        ppe_ms,
        core_total_ms: started.elapsed().as_secs_f64() * 1000.0,
        accepted: tipa_proof_ab_valid && tipa_proof_c_valid && ppe_valid,
    })
}

fn build_shifted_ck_1<P: Pairing>(ck_1: &[P::G2], r: &P::ScalarField) -> Vec<P::G2> {
    let inverse_powers = inverse_powers::<P>(ck_1.len(), r);

    #[cfg(feature = "parallel")]
    {
        ck_1.par_iter()
            .zip(inverse_powers.par_iter())
            .map(|(ck, power)| *ck * power)
            .collect()
    }

    #[cfg(not(feature = "parallel"))]
    {
        ck_1.iter()
            .zip(inverse_powers.iter())
            .map(|(ck, power)| *ck * power)
            .collect()
    }
}

fn inverse_powers<P: Pairing>(len: usize, r: &P::ScalarField) -> Vec<P::ScalarField> {
    let mut powers = Vec::with_capacity(len);
    let r_inv = r.inverse().unwrap();
    let mut current = P::ScalarField::one();
    for _ in 0..len {
        powers.push(current);
        current *= r_inv;
    }
    powers
}

fn apply_tipa_ab_profile(
    profile: &mut AggregateProofBuildProfile,
    tipa_profile: &TipaBuildProfile,
) {
    profile.tipa_ab_gipa_ms = tipa_profile.gipa_ms;
    profile.tipa_ab_gipa_commit_l_ms = tipa_profile.gipa.commit_l_ms;
    profile.tipa_ab_gipa_commit_r_ms = tipa_profile.gipa.commit_r_ms;
    profile.tipa_ab_gipa_challenge_ms = tipa_profile.gipa.challenge_ms;
    profile.tipa_ab_gipa_rescale_m1_ms = tipa_profile.gipa.rescale_m1_ms;
    profile.tipa_ab_gipa_rescale_m2_ms = tipa_profile.gipa.rescale_m2_ms;
    profile.tipa_ab_gipa_rescale_ck1_ms = tipa_profile.gipa.rescale_ck1_ms;
    profile.tipa_ab_gipa_rescale_ck2_ms = tipa_profile.gipa.rescale_ck2_ms;
    profile.tipa_ab_transcript_inverse_ms = tipa_profile.transcript_inverse_ms;
    profile.tipa_ab_kzg_challenge_ms = tipa_profile.kzg_challenge_ms;
    profile.tipa_ab_kzg_coefficient_build_ms = tipa_profile.kzg_coefficient_build_ms;
    profile.tipa_ab_kzg_eval_quotient_ms = tipa_profile.kzg_eval_quotient_ms;
    profile.tipa_ab_kzg_opening_msm_ms = tipa_profile.kzg_opening_msm_ms;
    profile.tipa_ab_kzg_opening_ck_a_ms = tipa_profile.kzg_opening_ck_a_ms;
    profile.tipa_ab_kzg_opening_ck_b_ms = tipa_profile.kzg_opening_ck_b_ms;
}

fn apply_tipa_c_profile(
    profile: &mut AggregateProofBuildProfile,
    tipa_profile: &TipaWithSsmBuildProfile,
) {
    profile.tipa_c_gipa_ms = tipa_profile.gipa_ms;
    profile.tipa_c_gipa_commit_l_ms = tipa_profile.gipa.commit_l_ms;
    profile.tipa_c_gipa_commit_r_ms = tipa_profile.gipa.commit_r_ms;
    profile.tipa_c_gipa_challenge_ms = tipa_profile.gipa.challenge_ms;
    profile.tipa_c_gipa_rescale_m1_ms = tipa_profile.gipa.rescale_m1_ms;
    profile.tipa_c_gipa_rescale_m2_ms = tipa_profile.gipa.rescale_m2_ms;
    profile.tipa_c_gipa_rescale_ck1_ms = tipa_profile.gipa.rescale_ck1_ms;
    profile.tipa_c_gipa_rescale_ck2_ms = tipa_profile.gipa.rescale_ck2_ms;
    profile.tipa_c_transcript_inverse_ms = tipa_profile.transcript_inverse_ms;
    profile.tipa_c_kzg_challenge_ms = tipa_profile.kzg_challenge_ms;
    profile.tipa_c_kzg_coefficient_build_ms = tipa_profile.kzg_coefficient_build_ms;
    profile.tipa_c_kzg_eval_quotient_ms = tipa_profile.kzg_eval_quotient_ms;
    profile.tipa_c_kzg_opening_msm_ms = tipa_profile.kzg_opening_msm_ms;
    profile.tipa_c_kzg_opening_ck_a_ms = tipa_profile.kzg_opening_ck_a_ms;
}

fn apply_pairing_profile(
    profile: &mut AggregateProofBuildProfile,
    pairing_profile: &PairingComputationProfile,
) {
    profile.pairing_normalize_batch_ms = pairing_profile.normalize_batch_ms;
    profile.pairing_prepare_ms = pairing_profile.prepare_ms;
    profile.pairing_miller_loop_ms = pairing_profile.miller_loop_ms;
    profile.pairing_final_exponentiation_ms = pairing_profile.final_exponentiation_ms;
}

fn derive_randomizer<P, D>(proof: &AggregateProof<P, D>) -> Result<P::ScalarField, Error>
where
    P: Pairing,
    D: Digest,
{
    let mut counter_nonce: usize = 0;
    loop {
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&counter_nonce.to_be_bytes()[..]);
        proof.com_a.serialize_uncompressed(&mut hash_input)?;
        proof.com_b.serialize_uncompressed(&mut hash_input)?;
        proof.com_c.serialize_uncompressed(&mut hash_input)?;
        if let Some(r) = <P::ScalarField>::from_random_bytes(&D::digest(&hash_input)) {
            break Ok(r);
        };
        counter_nonce += 1;
    }
}

fn verify_tipa_ab<P, D>(
    ip_verifier_srs: &VerifierSRS<P>,
    proof: &AggregateProof<P, D>,
    r: &P::ScalarField,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest,
{
    PairingInnerProductAB::<P, D>::verify_with_srs_shift(
        ip_verifier_srs,
        &HomomorphicPlaceholderValue,
        (
            &proof.com_a,
            &proof.com_b,
            &IdentityOutput(vec![proof.ip_ab.clone()]),
        ),
        &proof.tipa_proof_ab,
        r,
    )
}

fn verify_tipa_c<P, D>(
    ip_verifier_srs: &VerifierSRS<P>,
    proof: &AggregateProof<P, D>,
    r: &P::ScalarField,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest,
{
    MultiExpInnerProductC::<P, D>::verify_with_structured_scalar_message(
        ip_verifier_srs,
        &HomomorphicPlaceholderValue,
        (&proof.com_c, &IdentityOutput(vec![proof.agg_c.clone()])),
        r,
        &proof.tipa_proof_c,
    )
}

fn fold_public_inputs<P: Pairing>(
    vk: &VerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    r: &P::ScalarField,
) -> (P::ScalarField, P::G1) {
    let r_sum = (r.pow(&[public_inputs.len() as u64]) - &<P::ScalarField>::one())
        / &(r.clone() - &<P::ScalarField>::one());
    assert_eq!(vk.gamma_abc_g1.len(), public_inputs[0].len() + 1);
    let r_vec = structured_scalar_power(public_inputs.len(), r);
    let mut folded_public_inputs = vec![P::ScalarField::zero(); public_inputs[0].len()];
    for (inputs, challenge_power) in public_inputs.iter().zip(&r_vec) {
        for (acc, input) in folded_public_inputs.iter_mut().zip(inputs) {
            *acc += *input * challenge_power;
        }
    }

    let mut g_ic = P::G1::from(vk.gamma_abc_g1[0]) * r_sum;
    for (base, folded_input) in vk
        .gamma_abc_g1
        .iter()
        .skip(1)
        .zip(folded_public_inputs.iter())
    {
        g_ic += P::G1::from(*base) * folded_input;
    }

    (r_sum, g_ic)
}

fn verify_ppe<P: Pairing>(
    vk: &VerifyingKey<P>,
    proof: &AggregateProof<P, impl Digest>,
    r_sum: &P::ScalarField,
    g_ic: P::G1,
) -> bool {
    cfg_multi_pairing::<P>(
        &[P::G1::from(vk.alpha_g1) * r_sum, g_ic, proof.agg_c.clone()],
        &[
            P::G2::from(vk.beta_g2),
            P::G2::from(vk.gamma_g2),
            P::G2::from(vk.delta_g2),
        ],
    )
    .map(|pairing_output| pairing_output == proof.ip_ab)
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{build_shifted_ck_1, inverse_powers};
    use ark_bls12_381::Bls12_381;
    use ark_ec::pairing::Pairing;
    use ark_ff::{Field, UniformRand};
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use ark_std::One;

    #[test]
    fn inverse_powers_match_structured_inverses() {
        let mut rng = StdRng::seed_from_u64(7);
        let r = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);
        let powers = inverse_powers::<Bls12_381>(8, &r);

        let mut expected = <Bls12_381 as Pairing>::ScalarField::one();
        for power in powers {
            assert_eq!(power, expected);
            expected *= r.inverse().unwrap();
        }
    }

    #[test]
    fn shifted_ck_1_matches_per_element_inversion() {
        let mut rng = StdRng::seed_from_u64(11);
        let r = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);
        let ck_1 = (0..16)
            .map(|_| <Bls12_381 as Pairing>::G2::rand(&mut rng))
            .collect::<Vec<_>>();

        let optimized = build_shifted_ck_1::<Bls12_381>(&ck_1, &r);
        let expected = ck_1
            .iter()
            .enumerate()
            .map(|(idx, ck)| *ck * r.pow([idx as u64]).inverse().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(optimized, expected);
    }
}
