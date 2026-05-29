use ark_ec::{pairing::Pairing, CurveGroup, PrimeGroup};
use ark_ff::{Field, One, UniformRand, Zero};
use ark_poly::polynomial::{univariate::DensePolynomial, DenseUVPolynomial};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::cfg_iter;
use ark_std::rand::Rng;
use ark_std::{end_timer, start_timer};
use digest::Digest;
use itertools::Itertools;
use std::{convert::TryInto, marker::PhantomData, ops::MulAssign};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::{
    challenge::{challenge_digest, ChallengeContext, ChallengeTraceSink, NoopChallengeTraceSink},
    gipa::{GIPAAux, GIPAProof, GipaBuildProfile, GIPA},
    mul_helper, Error,
};
use ark_dh_commitments::{
    afgho16::{AFGHOCommitmentG1, AFGHOCommitmentG2},
    identity::{HomomorphicPlaceholderValue, IdentityCommitment},
    pedersen::PedersenCommitment,
    DoublyHomomorphicCommitment,
};
use ark_inner_products::{
    cfg_multi_pairing, cfg_multi_pairing_g1_affine_g2_prepared, InnerProduct, PairingInnerProduct,
};

pub mod structured_scalar_message;

//TODO: Could generalize: Don't need TIPA over G1 and G2, would work with G1 and G1 or over different pairing engines
pub trait TIPACompatibleSetup {}

impl<G: CurveGroup> TIPACompatibleSetup for PedersenCommitment<G> {}
impl<P: Pairing> TIPACompatibleSetup for AFGHOCommitmentG1<P> {}
impl<P: Pairing> TIPACompatibleSetup for AFGHOCommitmentG2<P> {}

//TODO: May need to add "reverse" MultiexponentiationInnerProduct to allow for MIP with G2 messages (because TIP hard-coded G1 left and G2 right)
pub struct TIPA<IP, LMC, RMC, IPC, P, D> {
    _inner_product: PhantomData<IP>,
    _left_commitment: PhantomData<LMC>,
    _right_commitment: PhantomData<RMC>,
    _inner_product_commitment: PhantomData<IPC>,
    _pair: PhantomData<P>,
    _digest: PhantomData<D>,
}

#[derive(Clone, Debug, Default)]
pub struct TipaBuildProfile {
    pub total_ms: f64,
    pub gipa_ms: f64,
    pub transcript_inverse_ms: f64,
    pub kzg_challenge_ms: f64,
    pub kzg_coefficient_build_ms: f64,
    pub kzg_eval_quotient_ms: f64,
    pub kzg_opening_msm_ms: f64,
    pub kzg_opening_ck_a_ms: f64,
    pub kzg_opening_ck_b_ms: f64,
    pub gipa: GipaBuildProfile,
}

#[derive(Clone, Debug, Default)]
pub struct KzgOpeningBuildProfile {
    pub total_ms: f64,
    pub coefficient_build_ms: f64,
    pub eval_quotient_ms: f64,
    pub opening_msm_ms: f64,
}

#[derive(CanonicalSerialize, CanonicalDeserialize)]
pub struct TIPAProof<IP, LMC, RMC, IPC, P, D>
where
    D: Digest,
    P: Pairing,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment + TIPACompatibleSetup,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar> + TIPACompatibleSetup,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    RMC::Message: MulAssign<LMC::Scalar>,
    IPC::Message: MulAssign<LMC::Scalar>,
    RMC::Key: MulAssign<LMC::Scalar>,
    IPC::Key: MulAssign<LMC::Scalar>,
    RMC::Output: MulAssign<LMC::Scalar>,
    IPC::Output: MulAssign<LMC::Scalar>,
{
    gipa_proof: GIPAProof<IP, LMC, RMC, IPC, D>,
    final_ck: (LMC::Key, RMC::Key),
    final_ck_proof: (P::G2, P::G1),
    _pair: PhantomData<P>,
}

impl<IP, LMC, RMC, IPC, P, D> Clone for TIPAProof<IP, LMC, RMC, IPC, P, D>
where
    D: Digest,
    P: Pairing,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment + TIPACompatibleSetup,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar> + TIPACompatibleSetup,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    RMC::Message: MulAssign<LMC::Scalar>,
    IPC::Message: MulAssign<LMC::Scalar>,
    RMC::Key: MulAssign<LMC::Scalar>,
    IPC::Key: MulAssign<LMC::Scalar>,
    RMC::Output: MulAssign<LMC::Scalar>,
    IPC::Output: MulAssign<LMC::Scalar>,
{
    fn clone(&self) -> Self {
        Self {
            gipa_proof: self.gipa_proof.clone(),
            final_ck: self.final_ck.clone(),
            final_ck_proof: self.final_ck_proof.clone(),
            _pair: PhantomData,
        }
    }
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct SRS<P: Pairing> {
    pub g_alpha_powers: Vec<P::G1>,
    pub h_beta_powers: Vec<P::G2>,
    pub g_beta: P::G1,
    pub h_alpha: P::G2,
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct VerifierSRS<P: Pairing> {
    pub g: P::G1,
    pub h: P::G2,
    pub g_beta: P::G1,
    pub h_alpha: P::G2,
}

#[derive(Clone)]
pub struct PreparedProvingSrs<P: Pairing> {
    g_alpha_powers_affine: Vec<P::G1Affine>,
    h_beta_powers_affine: Vec<P::G2Affine>,
    ck_1: Vec<P::G2>,
    ck_2: Vec<P::G1>,
}

type PairingTipaProof<P, D> = TIPAProof<
    PairingInnerProduct<P>,
    AFGHOCommitmentG1<P>,
    AFGHOCommitmentG2<P>,
    IdentityCommitment<ark_ec::pairing::PairingOutput<P>, <P as Pairing>::ScalarField>,
    P,
    D,
>;

const PAIRING_GIPA_RESCALE_THRESHOLD: usize = 64;

//TODO: Change SRS to return reference iterator - requires changes to TIPA and GIPA signatures
impl<P: Pairing> SRS<P> {
    pub fn get_commitment_keys(&self) -> (Vec<P::G2>, Vec<P::G1>) {
        let ck_1 = self.h_beta_powers.iter().step_by(2).cloned().collect();
        let ck_2 = self.g_alpha_powers.iter().step_by(2).cloned().collect();
        (ck_1, ck_2)
    }

    pub fn prepare_for_proving(&self) -> PreparedProvingSrs<P> {
        PreparedProvingSrs::new(self)
    }

    pub fn get_verifier_key(&self) -> VerifierSRS<P> {
        VerifierSRS {
            g: self.g_alpha_powers[0].clone(),
            h: self.h_beta_powers[0].clone(),
            g_beta: self.g_beta.clone(),
            h_alpha: self.h_alpha.clone(),
        }
    }
}

impl<P: Pairing> PreparedProvingSrs<P> {
    pub fn new(srs: &SRS<P>) -> Self {
        let g_alpha_powers_affine = <P as Pairing>::G1::normalize_batch(&srs.g_alpha_powers);
        let h_beta_powers_affine = <P as Pairing>::G2::normalize_batch(&srs.h_beta_powers);
        let ck_1 = srs.h_beta_powers.iter().step_by(2).cloned().collect();
        let ck_2 = srs.g_alpha_powers.iter().step_by(2).cloned().collect();
        Self {
            g_alpha_powers_affine,
            h_beta_powers_affine,
            ck_1,
            ck_2,
        }
    }

    pub fn commitment_keys(&self) -> (&[P::G2], &[P::G1]) {
        (&self.ck_1, &self.ck_2)
    }

    pub fn g_alpha_powers_affine(&self) -> &[P::G1Affine] {
        &self.g_alpha_powers_affine
    }

    pub fn h_beta_powers_affine(&self) -> &[P::G2Affine] {
        &self.h_beta_powers_affine
    }
}

#[inline]
fn use_pairing_rescale_parallel(len: usize) -> bool {
    #[cfg(feature = "parallel")]
    {
        len >= PAIRING_GIPA_RESCALE_THRESHOLD
    }
    #[cfg(not(feature = "parallel"))]
    {
        let _ = len;
        false
    }
}

fn rescale_fold_curve<G>(scaled_half: &[G], unscaled_half: &[G], scalar: &G::ScalarField) -> Vec<G>
where
    G: CurveGroup + Send + Sync,
{
    if use_pairing_rescale_parallel(scaled_half.len()) {
        cfg_iter!(scaled_half)
            .map(|point| mul_helper(point, scalar))
            .zip(unscaled_half)
            .map(|(scaled, base)| scaled + base.clone())
            .collect()
    } else {
        scaled_half
            .iter()
            .map(|point| mul_helper(point, scalar))
            .zip(unscaled_half.iter())
            .map(|(scaled, base)| scaled + base.clone())
            .collect()
    }
}

fn rescale_fold_curve_profiled<G>(
    scaled_half: &[G],
    unscaled_half: &[G],
    scalar: &G::ScalarField,
) -> (Vec<G>, f64)
where
    G: CurveGroup + Send + Sync,
{
    let started = std::time::Instant::now();
    let folded = rescale_fold_curve(scaled_half, unscaled_half, scalar);
    (folded, started.elapsed().as_secs_f64() * 1000.0)
}

fn prepare_g2_affine_vec<P: Pairing>(points: &[P::G2Affine]) -> Vec<P::G2Prepared> {
    let mut prepared = Vec::with_capacity(points.len());

    #[cfg(feature = "parallel")]
    {
        points
            .par_iter()
            .cloned()
            .map(P::G2Prepared::from)
            .collect_into_vec(&mut prepared);
    }

    #[cfg(not(feature = "parallel"))]
    {
        prepared.extend(points.iter().cloned().map(P::G2Prepared::from));
    }

    prepared
}

fn pairing_affine<P: Pairing>(
    left: &[P::G1Affine],
    right: &[P::G2Affine],
) -> Result<ark_ec::pairing::PairingOutput<P>, Error> {
    let right_prepared = prepare_g2_affine_vec::<P>(right);
    pairing_affine_with_prepared_g2::<P>(left, &right_prepared)
}

fn pairing_affine_with_prepared_g2<P: Pairing>(
    left: &[P::G1Affine],
    right: &[P::G2Prepared],
) -> Result<ark_ec::pairing::PairingOutput<P>, Error> {
    cfg_multi_pairing_g1_affine_g2_prepared::<P>(left, right)
        .ok_or_else(|| Box::new(std::io::Error::other("pairing length mismatch")) as Error)
}

pub(crate) fn prove_pairing_inner_product_with_prepared_srs_shift<P, D>(
    context: &ChallengeContext,
    trace: &mut impl ChallengeTraceSink,
    prepared_srs: &PreparedProvingSrs<P>,
    values: (&[P::G1], &[P::G2]),
    ck: (&[P::G2], &[P::G1], &HomomorphicPlaceholderValue),
    r_shift: &P::ScalarField,
) -> Result<PairingTipaProof<P, D>, Error>
where
    P: Pairing,
    D: Digest,
{
    let (proof, profile) = prove_pairing_inner_product_with_prepared_srs_shift_profiled::<P, D>(
        context,
        trace,
        prepared_srs,
        values,
        ck,
        r_shift,
    )?;
    debug_assert!(profile.total_ms >= 0.0);
    Ok(proof)
}

pub(crate) fn prove_pairing_inner_product_with_prepared_srs_shift_profiled<P, D>(
    context: &ChallengeContext,
    trace: &mut impl ChallengeTraceSink,
    prepared_srs: &PreparedProvingSrs<P>,
    values: (&[P::G1], &[P::G2]),
    ck: (&[P::G2], &[P::G1], &HomomorphicPlaceholderValue),
    r_shift: &P::ScalarField,
) -> Result<(PairingTipaProof<P, D>, TipaBuildProfile), Error>
where
    P: Pairing,
    D: Digest,
{
    let total_started = std::time::Instant::now();
    let mut profile = TipaBuildProfile::default();

    let gipa_started = std::time::Instant::now();
    let (proof, aux, gipa_profile) =
        prove_pairing_inner_product_gipa_with_aux_profiled::<P, D>(context, trace, values, ck)?;
    profile.gipa_ms = gipa_started.elapsed().as_secs_f64() * 1000.0;
    profile.gipa = gipa_profile;

    let (ck_a_final, ck_b_final) = aux.ck_base;
    let transcript = aux.r_transcript;
    let transcript_inverse_started = std::time::Instant::now();
    let transcript_inverse = transcript.iter().map(|x| x.inverse().unwrap()).collect();
    profile.transcript_inverse_ms = transcript_inverse_started.elapsed().as_secs_f64() * 1000.0;
    let r_inverse = r_shift.inverse().unwrap();

    let kzg_challenge_started = std::time::Instant::now();
    let mut counter_nonce: u64 = 0;
    let c = loop {
        let mut hash_input = Vec::new();
        if let Some(first) = transcript.first() {
            first.serialize_uncompressed(&mut hash_input)?;
        }
        ck_a_final.serialize_uncompressed(&mut hash_input)?;
        ck_b_final.serialize_uncompressed(&mut hash_input)?;
        if let Some(c) = P::ScalarField::from_random_bytes(&challenge_digest::<D, _>(
            context,
            trace,
            b"tipa.ab.kzg",
            counter_nonce,
            &hash_input,
        )) {
            break c;
        };
        counter_nonce += 1;
    };
    profile.kzg_challenge_ms = kzg_challenge_started.elapsed().as_secs_f64() * 1000.0;

    let kzg_opening_ck_a_started = std::time::Instant::now();
    let (ck_a_kzg_opening, ck_a_kzg_profile) =
        prove_commitment_key_kzg_opening_with_affine_profiled(
            prepared_srs.h_beta_powers_affine(),
            &transcript_inverse,
            &r_inverse,
            &c,
        )?;
    profile.kzg_opening_ck_a_ms = kzg_opening_ck_a_started.elapsed().as_secs_f64() * 1000.0;
    profile.kzg_coefficient_build_ms += ck_a_kzg_profile.coefficient_build_ms;
    profile.kzg_eval_quotient_ms += ck_a_kzg_profile.eval_quotient_ms;
    profile.kzg_opening_msm_ms += ck_a_kzg_profile.opening_msm_ms;

    let kzg_opening_ck_b_started = std::time::Instant::now();
    let (ck_b_kzg_opening, ck_b_kzg_profile) =
        prove_commitment_key_kzg_opening_with_affine_profiled(
            prepared_srs.g_alpha_powers_affine(),
            &transcript,
            &P::ScalarField::one(),
            &c,
        )?;
    profile.kzg_opening_ck_b_ms = kzg_opening_ck_b_started.elapsed().as_secs_f64() * 1000.0;
    profile.kzg_coefficient_build_ms += ck_b_kzg_profile.coefficient_build_ms;
    profile.kzg_eval_quotient_ms += ck_b_kzg_profile.eval_quotient_ms;
    profile.kzg_opening_msm_ms += ck_b_kzg_profile.opening_msm_ms;
    profile.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;

    Ok((
        TIPAProof {
            gipa_proof: proof,
            final_ck: (ck_a_final, ck_b_final),
            final_ck_proof: (ck_a_kzg_opening, ck_b_kzg_opening),
            _pair: PhantomData,
        },
        profile,
    ))
}

fn prove_pairing_inner_product_gipa_with_aux_profiled<P, D>(
    context: &ChallengeContext,
    trace: &mut impl ChallengeTraceSink,
    values: (&[P::G1], &[P::G2]),
    ck: (&[P::G2], &[P::G1], &HomomorphicPlaceholderValue),
) -> Result<
    (
        GIPAProof<
            PairingInnerProduct<P>,
            AFGHOCommitmentG1<P>,
            AFGHOCommitmentG2<P>,
            IdentityCommitment<ark_ec::pairing::PairingOutput<P>, P::ScalarField>,
            D,
        >,
        GIPAAux<
            PairingInnerProduct<P>,
            AFGHOCommitmentG1<P>,
            AFGHOCommitmentG2<P>,
            IdentityCommitment<ark_ec::pairing::PairingOutput<P>, P::ScalarField>,
            D,
        >,
        GipaBuildProfile,
    ),
    Error,
>
where
    P: Pairing,
    D: Digest,
{
    let total_started = std::time::Instant::now();
    let (mut m_a, mut m_b) = (values.0.to_vec(), values.1.to_vec());
    let (mut ck_a, mut ck_b) = (ck.0.to_vec(), ck.1.to_vec());
    let ck_t = ck.2;
    let mut r_commitment_steps = Vec::new();
    let mut r_transcript: Vec<P::ScalarField> = Vec::new();
    let mut profile = GipaBuildProfile::default();
    assert!(m_a.len().is_power_of_two());

    let (m_base, ck_base) = 'recurse: loop {
        if m_a.len() == 1 {
            break 'recurse ((m_a[0], m_b[0]), (ck_a[0], ck_b[0]));
        }

        let split = m_a.len() / 2;
        let m_a_1 = &m_a[split..];
        let m_a_2 = &m_a[..split];
        let ck_a_1 = &ck_a[..split];
        let ck_a_2 = &ck_a[split..];
        let m_b_1 = &m_b[..split];
        let m_b_2 = &m_b[split..];
        let ck_b_1 = &ck_b[split..];
        let ck_b_2 = &ck_b[..split];

        let m_a_1_affine = P::G1::normalize_batch(m_a_1);
        let m_a_2_affine = P::G1::normalize_batch(m_a_2);
        let m_b_1_affine = P::G2::normalize_batch(m_b_1);
        let m_b_2_affine = P::G2::normalize_batch(m_b_2);
        let ck_b_1_affine = P::G1::normalize_batch(ck_b_1);
        let ck_b_2_affine = P::G1::normalize_batch(ck_b_2);
        let ck_a_affine = P::G2::normalize_batch(&ck_a);
        let ck_a_1_affine = &ck_a_affine[..split];
        let ck_a_2_affine = &ck_a_affine[split..];
        let m_b_1_prepared = prepare_g2_affine_vec::<P>(&m_b_1_affine);
        let m_b_2_prepared = prepare_g2_affine_vec::<P>(&m_b_2_affine);

        let commit_l = || {
            let commit_started = std::time::Instant::now();
            let ip_started = std::time::Instant::now();
            let ip = pairing_affine_with_prepared_g2::<P>(&m_a_1_affine, &m_b_1_prepared)
                .map_err(|err| err.to_string())?;
            let ip_ms = ip_started.elapsed().as_secs_f64() * 1000.0;
            let com_a_started = std::time::Instant::now();
            let com_a =
                pairing_affine::<P>(&m_a_1_affine, ck_a_1_affine).map_err(|err| err.to_string())?;
            let com_a_ms = com_a_started.elapsed().as_secs_f64() * 1000.0;
            let com_b_started = std::time::Instant::now();
            let com_b = pairing_affine_with_prepared_g2::<P>(&ck_b_1_affine, &m_b_1_prepared)
                .map_err(|err| err.to_string())?;
            let com_b_ms = com_b_started.elapsed().as_secs_f64() * 1000.0;
            let com = (
                com_a,
                com_b,
                IdentityCommitment::<ark_ec::pairing::PairingOutput<P>, P::ScalarField>::commit(
                    &vec![ck_t.clone()],
                    &vec![ip],
                )
                .map_err(|err| err.to_string())?,
            );
            Ok::<_, String>((
                com,
                commit_started.elapsed().as_secs_f64() * 1000.0,
                ip_ms,
                com_a_ms,
                com_b_ms,
            ))
        };
        let commit_r = || {
            let commit_started = std::time::Instant::now();
            let ip_started = std::time::Instant::now();
            let ip = pairing_affine_with_prepared_g2::<P>(&m_a_2_affine, &m_b_2_prepared)
                .map_err(|err| err.to_string())?;
            let ip_ms = ip_started.elapsed().as_secs_f64() * 1000.0;
            let com_a_started = std::time::Instant::now();
            let com_a =
                pairing_affine::<P>(&m_a_2_affine, ck_a_2_affine).map_err(|err| err.to_string())?;
            let com_a_ms = com_a_started.elapsed().as_secs_f64() * 1000.0;
            let com_b_started = std::time::Instant::now();
            let com_b = pairing_affine_with_prepared_g2::<P>(&ck_b_2_affine, &m_b_2_prepared)
                .map_err(|err| err.to_string())?;
            let com_b_ms = com_b_started.elapsed().as_secs_f64() * 1000.0;
            let com = (
                com_a,
                com_b,
                IdentityCommitment::<ark_ec::pairing::PairingOutput<P>, P::ScalarField>::commit(
                    &vec![ck_t.clone()],
                    &vec![ip],
                )
                .map_err(|err| err.to_string())?,
            );
            Ok::<_, String>((
                com,
                commit_started.elapsed().as_secs_f64() * 1000.0,
                ip_ms,
                com_a_ms,
                com_b_ms,
            ))
        };

        #[cfg(all(feature = "parallel", not(feature = "bench-baseline")))]
        let (commit_l_result, commit_r_result) = rayon::join(commit_l, commit_r);

        #[cfg(any(not(feature = "parallel"), feature = "bench-baseline"))]
        let (commit_l_result, commit_r_result) = (commit_l(), commit_r());

        let (com_1, commit_l_ms, ip_l_ms, com_a_l_ms, com_b_l_ms) =
            commit_l_result.map_err(|err: String| std::io::Error::other(err))?;
        let (com_2, commit_r_ms, ip_r_ms, com_a_r_ms, com_b_r_ms) =
            commit_r_result.map_err(|err: String| std::io::Error::other(err))?;
        profile.commit_l_ms += commit_l_ms;
        profile.commit_r_ms += commit_r_ms;
        profile.commit_ab_ms += ip_l_ms + ip_r_ms;
        profile.commit_com_a_ms += com_a_l_ms + com_a_r_ms;
        profile.commit_com_b_ms += com_b_l_ms + com_b_r_ms;

        let challenge_started = std::time::Instant::now();
        let mut counter_nonce: u64 = 0;
        let default_transcript = Default::default();
        let transcript = r_transcript.last().unwrap_or(&default_transcript);
        let (c, c_inv) = loop {
            let mut hash_input = Vec::new();
            transcript.serialize_uncompressed(&mut hash_input)?;
            com_1.0.serialize_uncompressed(&mut hash_input)?;
            com_1.1.serialize_uncompressed(&mut hash_input)?;
            com_1.2.serialize_uncompressed(&mut hash_input)?;
            com_2.0.serialize_uncompressed(&mut hash_input)?;
            com_2.1.serialize_uncompressed(&mut hash_input)?;
            com_2.2.serialize_uncompressed(&mut hash_input)?;
            let c: P::ScalarField = u128::from_be_bytes(
                challenge_digest::<D, _>(
                    context,
                    trace,
                    b"tipa.ab.gipa.round",
                    counter_nonce,
                    &hash_input,
                )
                .as_slice()[0..16]
                    .try_into()
                    .unwrap(),
            )
            .into();
            if let Some(c_inv) = c.inverse() {
                break (c_inv, c);
            }
            counter_nonce += 1;
        };
        profile.challenge_ms += challenge_started.elapsed().as_secs_f64() * 1000.0;

        #[cfg(all(feature = "parallel", not(feature = "bench-baseline")))]
        let (
            (next_m_a, rescale_m1_ms),
            (next_m_b, rescale_m2_ms),
            (next_ck_a, rescale_ck1_ms),
            (next_ck_b, rescale_ck2_ms),
        ) = {
            let ((next_m_a, next_m_b), (next_ck_a, next_ck_b)) = rayon::join(
                || {
                    rayon::join(
                        || rescale_fold_curve_profiled(m_a_1, m_a_2, &c),
                        || rescale_fold_curve_profiled(m_b_2, m_b_1, &c_inv),
                    )
                },
                || {
                    rayon::join(
                        || rescale_fold_curve_profiled(ck_a_2, ck_a_1, &c_inv),
                        || rescale_fold_curve_profiled(ck_b_1, ck_b_2, &c),
                    )
                },
            );
            (next_m_a, next_m_b, next_ck_a, next_ck_b)
        };

        #[cfg(any(not(feature = "parallel"), feature = "bench-baseline"))]
        let (
            (next_m_a, rescale_m1_ms),
            (next_m_b, rescale_m2_ms),
            (next_ck_a, rescale_ck1_ms),
            (next_ck_b, rescale_ck2_ms),
        ) = (
            rescale_fold_curve_profiled(m_a_1, m_a_2, &c),
            rescale_fold_curve_profiled(m_b_2, m_b_1, &c_inv),
            rescale_fold_curve_profiled(ck_a_2, ck_a_1, &c_inv),
            rescale_fold_curve_profiled(ck_b_1, ck_b_2, &c),
        );

        m_a = next_m_a;
        m_b = next_m_b;
        ck_a = next_ck_a;
        ck_b = next_ck_b;
        profile.rescale_m1_ms += rescale_m1_ms;
        profile.rescale_m2_ms += rescale_m2_ms;
        profile.rescale_ck1_ms += rescale_ck1_ms;
        profile.rescale_ck2_ms += rescale_ck2_ms;

        r_commitment_steps.push((com_1, com_2));
        r_transcript.push(c);
    };

    r_transcript.reverse();
    r_commitment_steps.reverse();
    profile.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;

    Ok((
        GIPAProof {
            r_commitment_steps,
            r_base: m_base,
            _gipa: PhantomData,
        },
        GIPAAux {
            r_transcript,
            ck_base,
            _gipa: PhantomData,
        },
        profile,
    ))
}

impl<IP, LMC, RMC, IPC, P, D> TIPA<IP, LMC, RMC, IPC, P, D>
where
    D: Digest,
    P: Pairing,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment<Scalar = P::ScalarField, Key = P::G2> + TIPACompatibleSetup,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar, Key = P::G1> + TIPACompatibleSetup,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    LMC::Message: MulAssign<P::ScalarField>,
    RMC::Message: MulAssign<P::ScalarField>,
    IPC::Message: MulAssign<P::ScalarField>,
    IPC::Key: MulAssign<P::ScalarField>,
    LMC::Output: MulAssign<P::ScalarField>,
    RMC::Output: MulAssign<P::ScalarField>,
    IPC::Output: MulAssign<P::ScalarField>,
    LMC::Output: Send,
    RMC::Output: Send,
    IPC::Output: Send,
{
    pub fn setup<R: Rng>(rng: &mut R, size: usize) -> Result<(SRS<P>, IPC::Key), Error> {
        let alpha = <P::ScalarField>::rand(rng);
        let beta = <P::ScalarField>::rand(rng);
        let g = <P::G1>::generator();
        let h = <P::G2>::generator();
        Ok((
            SRS {
                g_alpha_powers: structured_generators_scalar_power(2 * size - 1, &g, &alpha),
                h_beta_powers: structured_generators_scalar_power(2 * size - 1, &h, &beta),
                g_beta: g * beta,
                h_alpha: h * alpha,
            },
            IPC::setup(rng, 1)?.pop().unwrap(),
        ))
    }

    pub fn prove(
        context: &ChallengeContext,
        srs: &SRS<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
    ) -> Result<TIPAProof<IP, LMC, RMC, IPC, P, D>, Error> {
        Self::prove_with_srs_shift(context, srs, values, ck, &<P::ScalarField>::one())
    }

    // Shifts KZG proof for left message by scalar r (used for efficient composition with aggregation protocols)
    // LMC commitment key should already be shifted before being passed as input
    pub fn prove_with_srs_shift(
        context: &ChallengeContext,
        srs: &SRS<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<TIPAProof<IP, LMC, RMC, IPC, P, D>, Error> {
        let (proof, profile) =
            Self::prove_with_srs_shift_profiled(context, srs, values, ck, r_shift)?;
        debug_assert!(profile.total_ms >= 0.0);
        Ok(proof)
    }

    pub fn prove_with_srs_shift_profiled(
        context: &ChallengeContext,
        srs: &SRS<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<(TIPAProof<IP, LMC, RMC, IPC, P, D>, TipaBuildProfile), Error> {
        let prepared_srs = srs.prepare_for_proving();
        Self::prove_with_prepared_srs_shift_profiled(context, &prepared_srs, values, ck, r_shift)
    }

    pub fn prove_with_prepared_srs_shift(
        context: &ChallengeContext,
        prepared_srs: &PreparedProvingSrs<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<TIPAProof<IP, LMC, RMC, IPC, P, D>, Error> {
        let (proof, profile) = Self::prove_with_prepared_srs_shift_profiled(
            context,
            prepared_srs,
            values,
            ck,
            r_shift,
        )?;
        debug_assert!(profile.total_ms >= 0.0);
        Ok(proof)
    }

    pub fn prove_with_prepared_srs_shift_profiled(
        context: &ChallengeContext,
        prepared_srs: &PreparedProvingSrs<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<(TIPAProof<IP, LMC, RMC, IPC, P, D>, TipaBuildProfile), Error> {
        let mut trace = NoopChallengeTraceSink;
        Self::prove_with_prepared_srs_shift_profiled_with_trace(
            context,
            &mut trace,
            prepared_srs,
            values,
            ck,
            r_shift,
        )
    }

    pub fn prove_with_prepared_srs_shift_profiled_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        prepared_srs: &PreparedProvingSrs<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<(TIPAProof<IP, LMC, RMC, IPC, P, D>, TipaBuildProfile), Error>
    where
        S: ChallengeTraceSink,
    {
        let total_started = std::time::Instant::now();
        let mut profile = TipaBuildProfile::default();

        // Run GIPA
        let gipa_started = std::time::Instant::now();
        let (proof, aux, gipa_profile) =
            <GIPA<IP, LMC, RMC, IPC, D>>::prove_with_aux_profiled_with_stage_with_trace(
                context,
                trace,
                b"tipa.generic.gipa.round",
                values,
                (ck.0, ck.1, &vec![ck.2.clone()]),
            )?;
        profile.gipa_ms = gipa_started.elapsed().as_secs_f64() * 1000.0;
        profile.gipa = gipa_profile;

        // Prove final commitment keys are wellformed
        let (ck_a_final, ck_b_final) = aux.ck_base;
        let transcript = aux.r_transcript;
        let transcript_inverse_started = std::time::Instant::now();
        let transcript_inverse = transcript.iter().map(|x| x.inverse().unwrap()).collect();
        profile.transcript_inverse_ms = transcript_inverse_started.elapsed().as_secs_f64() * 1000.0;
        let r_inverse = r_shift.inverse().unwrap();

        // KZG challenge point
        let kzg_challenge_started = std::time::Instant::now();
        let mut counter_nonce: u64 = 0;
        let c = loop {
            let mut hash_input = Vec::new();
            if let Some(first) = transcript.first() {
                first.serialize_uncompressed(&mut hash_input)?;
            }
            ck_a_final.serialize_uncompressed(&mut hash_input)?;
            ck_b_final.serialize_uncompressed(&mut hash_input)?;
            if let Some(c) = LMC::Scalar::from_random_bytes(&challenge_digest::<D, _>(
                context,
                trace,
                b"tipa.generic.kzg",
                counter_nonce,
                &hash_input,
            )) {
                break c;
            };
            counter_nonce += 1;
        };
        profile.kzg_challenge_ms = kzg_challenge_started.elapsed().as_secs_f64() * 1000.0;

        // Complete KZG proofs
        let kzg_opening_ck_a_started = std::time::Instant::now();
        let (ck_a_kzg_opening, ck_a_kzg_profile) =
            prove_commitment_key_kzg_opening_with_affine_profiled(
                prepared_srs.h_beta_powers_affine(),
                &transcript_inverse,
                &r_inverse,
                &c,
            )?;
        profile.kzg_opening_ck_a_ms = kzg_opening_ck_a_started.elapsed().as_secs_f64() * 1000.0;
        profile.kzg_coefficient_build_ms += ck_a_kzg_profile.coefficient_build_ms;
        profile.kzg_eval_quotient_ms += ck_a_kzg_profile.eval_quotient_ms;
        profile.kzg_opening_msm_ms += ck_a_kzg_profile.opening_msm_ms;
        let kzg_opening_ck_b_started = std::time::Instant::now();
        let (ck_b_kzg_opening, ck_b_kzg_profile) =
            prove_commitment_key_kzg_opening_with_affine_profiled(
                prepared_srs.g_alpha_powers_affine(),
                &transcript,
                &<P::ScalarField>::one(),
                &c,
            )?;
        profile.kzg_opening_ck_b_ms = kzg_opening_ck_b_started.elapsed().as_secs_f64() * 1000.0;
        profile.kzg_coefficient_build_ms += ck_b_kzg_profile.coefficient_build_ms;
        profile.kzg_eval_quotient_ms += ck_b_kzg_profile.eval_quotient_ms;
        profile.kzg_opening_msm_ms += ck_b_kzg_profile.opening_msm_ms;
        profile.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;

        Ok((
            TIPAProof {
                gipa_proof: proof,
                final_ck: (ck_a_final, ck_b_final),
                final_ck_proof: (ck_a_kzg_opening, ck_b_kzg_opening),
                _pair: PhantomData,
            },
            profile,
        ))
    }

    pub fn verify(
        context: &ChallengeContext,
        v_srs: &VerifierSRS<P>,
        ck_t: &IPC::Key,
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &TIPAProof<IP, LMC, RMC, IPC, P, D>,
    ) -> Result<bool, Error> {
        Self::verify_with_srs_shift(context, v_srs, ck_t, com, proof, &<P::ScalarField>::one())
    }

    pub fn verify_with_srs_shift(
        context: &ChallengeContext,
        v_srs: &VerifierSRS<P>,
        ck_t: &IPC::Key,
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &TIPAProof<IP, LMC, RMC, IPC, P, D>,
        r_shift: &P::ScalarField,
    ) -> Result<bool, Error> {
        Self::verify_with_srs_shift_and_labels(
            context,
            b"tipa.generic.gipa.round",
            b"tipa.generic.kzg",
            v_srs,
            ck_t,
            com,
            proof,
            r_shift,
        )
    }

    pub fn verify_with_srs_shift_and_labels(
        context: &ChallengeContext,
        gipa_stage_label: &'static [u8],
        kzg_stage_label: &'static [u8],
        v_srs: &VerifierSRS<P>,
        ck_t: &IPC::Key,
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &TIPAProof<IP, LMC, RMC, IPC, P, D>,
        r_shift: &P::ScalarField,
    ) -> Result<bool, Error> {
        let mut trace = NoopChallengeTraceSink;
        Self::verify_with_srs_shift_and_labels_with_trace(
            context,
            &mut trace,
            gipa_stage_label,
            kzg_stage_label,
            v_srs,
            ck_t,
            com,
            proof,
            r_shift,
        )
    }

    pub fn verify_with_srs_shift_and_labels_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        gipa_stage_label: &'static [u8],
        kzg_stage_label: &'static [u8],
        v_srs: &VerifierSRS<P>,
        ck_t: &IPC::Key,
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &TIPAProof<IP, LMC, RMC, IPC, P, D>,
        r_shift: &P::ScalarField,
    ) -> Result<bool, Error>
    where
        S: ChallengeTraceSink,
    {
        let (base_com, transcript) =
            GIPA::verify_recursive_challenge_transcript_with_stage_with_trace(
                context,
                trace,
                gipa_stage_label,
                com,
                &proof.gipa_proof,
            )?;
        let transcript_inverse = transcript.iter().map(|x| x.inverse().unwrap()).collect();

        // Verify commitment keys wellformed
        let (ck_a_final, ck_b_final) = &proof.final_ck;
        let (ck_a_proof, ck_b_proof) = &proof.final_ck_proof;

        // KZG challenge point
        let mut counter_nonce: u64 = 0;
        let c = loop {
            let mut hash_input = Vec::new();
            if let Some(first) = transcript.first() {
                first.serialize_uncompressed(&mut hash_input)?;
            }
            ck_a_final.serialize_uncompressed(&mut hash_input)?;
            ck_b_final.serialize_uncompressed(&mut hash_input)?;
            if let Some(c) = LMC::Scalar::from_random_bytes(&challenge_digest::<D, _>(
                context,
                trace,
                kzg_stage_label,
                counter_nonce,
                &hash_input,
            )) {
                break c;
            };
            counter_nonce += 1;
        };

        let ck_a_valid = verify_commitment_key_g2_kzg_opening(
            v_srs,
            &ck_a_final,
            &ck_a_proof,
            &transcript_inverse,
            &r_shift.inverse().unwrap(),
            &c,
        )?;
        let ck_b_valid = verify_commitment_key_g1_kzg_opening(
            v_srs,
            &ck_b_final,
            &ck_b_proof,
            &transcript,
            &<P::ScalarField>::one(),
            &c,
        )?;

        // Verify base inner product commitment
        let (com_a, com_b, com_t) = base_com;
        let a_base = vec![proof.gipa_proof.r_base.0.clone()];
        let b_base = vec![proof.gipa_proof.r_base.1.clone()];
        let t_base = vec![IP::inner_product(&a_base, &b_base)?];
        let base_valid = LMC::verify(&vec![ck_a_final.clone()], &a_base, &com_a)?
            && RMC::verify(&vec![ck_b_final.clone()], &b_base, &com_b)?
            && IPC::verify(&vec![ck_t.clone()], &t_base, &com_t)?;

        Ok(ck_a_valid && ck_b_valid && base_valid)
    }
}

pub fn prove_commitment_key_kzg_opening<G: CurveGroup>(
    srs_powers: &Vec<G>,
    transcript: &Vec<G::ScalarField>,
    r_shift: &G::ScalarField,
    kzg_challenge: &G::ScalarField,
) -> Result<G, Error> {
    let affines = G::normalize_batch(srs_powers);
    let (opening, profile) = prove_commitment_key_kzg_opening_with_affine_profiled(
        &affines,
        transcript,
        r_shift,
        kzg_challenge,
    )?;
    debug_assert!(profile.total_ms >= 0.0);
    Ok(opening)
}

pub fn prove_commitment_key_kzg_opening_with_affine<G: CurveGroup>(
    srs_powers: &[G::Affine],
    transcript: &Vec<G::ScalarField>,
    r_shift: &G::ScalarField,
    kzg_challenge: &G::ScalarField,
) -> Result<G, Error> {
    let (opening, profile) = prove_commitment_key_kzg_opening_with_affine_profiled(
        srs_powers,
        transcript,
        r_shift,
        kzg_challenge,
    )?;
    debug_assert!(profile.total_ms >= 0.0);
    Ok(opening)
}

pub fn prove_commitment_key_kzg_opening_with_affine_profiled<G: CurveGroup>(
    srs_powers: &[G::Affine],
    transcript: &Vec<G::ScalarField>,
    r_shift: &G::ScalarField,
    kzg_challenge: &G::ScalarField,
) -> Result<(G, KzgOpeningBuildProfile), Error> {
    let total_started = std::time::Instant::now();
    let mut profile = KzgOpeningBuildProfile::default();

    let coefficient_build_started = std::time::Instant::now();
    let ck_coefficients = polynomial_coefficients_from_transcript(transcript, r_shift);
    let ck_polynomial = DensePolynomial::from_coefficients_slice(&ck_coefficients);
    profile.coefficient_build_ms = coefficient_build_started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(srs_powers.len(), ck_polynomial.coeffs.len());

    let eval_quotient_started = std::time::Instant::now();
    let eval = start_timer!(|| "polynomial eval");
    let ck_polynomial_c_eval =
        polynomial_evaluation_product_form_from_transcript(transcript, kzg_challenge, r_shift);
    end_timer!(eval);

    let quotient = start_timer!(|| "polynomial quotient");
    let quotient_polynomial = &(&ck_polynomial
        - &DensePolynomial::from_coefficients_vec(vec![ck_polynomial_c_eval]))
        / &(DensePolynomial::from_coefficients_vec(vec![-*kzg_challenge, <G::ScalarField>::one()]));
    end_timer!(quotient);

    let mut quotient_polynomial_coeffs = quotient_polynomial.coeffs;
    quotient_polynomial_coeffs.resize(srs_powers.len(), <G::ScalarField>::zero());
    profile.eval_quotient_ms = eval_quotient_started.elapsed().as_secs_f64() * 1000.0;

    let opening_msm_started = std::time::Instant::now();
    let multiexp = start_timer!(|| "opening multiexp");
    let opening = G::msm(srs_powers, &quotient_polynomial_coeffs)
        .map_err(|_| Box::new(std::io::Error::other("KZG opening MSM length mismatch")) as Error)?;
    end_timer!(multiexp);
    profile.opening_msm_ms = opening_msm_started.elapsed().as_secs_f64() * 1000.0;
    profile.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;

    Ok((opening, profile))
}

//TODO: Figure out how to avoid needing two separate methods for verification of opposite groups
pub fn verify_commitment_key_g2_kzg_opening<P: Pairing>(
    v_srs: &VerifierSRS<P>,
    ck_final: &P::G2,
    ck_opening: &P::G2,
    transcript: &Vec<P::ScalarField>,
    r_shift: &P::ScalarField,
    kzg_challenge: &P::ScalarField,
) -> Result<bool, Error> {
    let ck_polynomial_c_eval =
        polynomial_evaluation_product_form_from_transcript(transcript, kzg_challenge, r_shift);
    let left = vec![
        v_srs.g.clone(),
        -(v_srs.g_beta.clone() - v_srs.g.clone() * kzg_challenge),
    ];
    let right = vec![
        *ck_final - v_srs.h.clone() * ck_polynomial_c_eval,
        ck_opening.clone(),
    ];
    Ok(cfg_multi_pairing::<P>(&left, &right)
        .map(|pairing_output| pairing_output == ark_ec::pairing::PairingOutput::<P>::zero())
        .unwrap_or(false))
}

pub fn verify_commitment_key_g1_kzg_opening<P: Pairing>(
    v_srs: &VerifierSRS<P>,
    ck_final: &P::G1,
    ck_opening: &P::G1,
    transcript: &Vec<P::ScalarField>,
    r_shift: &P::ScalarField,
    kzg_challenge: &P::ScalarField,
) -> Result<bool, Error> {
    let ck_polynomial_c_eval =
        polynomial_evaluation_product_form_from_transcript(transcript, kzg_challenge, r_shift);
    let left = vec![
        *ck_final - v_srs.g.clone() * ck_polynomial_c_eval,
        -ck_opening.clone(),
    ];
    let right = vec![
        v_srs.h.clone(),
        v_srs.h_alpha.clone() - v_srs.h.clone() * kzg_challenge,
    ];
    Ok(cfg_multi_pairing::<P>(&left, &right)
        .map(|pairing_output| pairing_output == ark_ec::pairing::PairingOutput::<P>::zero())
        .unwrap_or(false))
}

pub fn structured_generators_scalar_power<G: CurveGroup>(
    num: usize,
    g: &G,
    s: &G::ScalarField,
) -> Vec<G> {
    assert!(num > 0);
    let mut powers_of_scalar = vec![];
    let mut pow_s = G::ScalarField::one();
    for _ in 0..num {
        powers_of_scalar.push(pow_s);
        pow_s *= s;
    }

    g.clone()
        .batch_mul(&powers_of_scalar)
        .into_iter()
        .map(Into::into)
        .collect()
}

fn polynomial_evaluation_product_form_from_transcript<F: Field>(
    transcript: &Vec<F>,
    z: &F,
    r_shift: &F,
) -> F {
    let mut power_2_zr = (z.clone() * z) * r_shift;
    let mut product_form = Vec::new();
    for x in transcript.iter() {
        product_form.push(F::one() + (x.clone() * &power_2_zr));
        power_2_zr *= power_2_zr;
    }
    product_form.iter().product()
}

fn polynomial_coefficients_from_transcript<F: Field>(transcript: &Vec<F>, r_shift: &F) -> Vec<F> {
    let mut coefficients = vec![F::one()];
    let mut power_2_r = r_shift.clone();
    for (i, x) in transcript.iter().enumerate() {
        for j in 0..(2_usize).pow(i as u32) {
            coefficients.push(coefficients[j] * &(x.clone() * &power_2_r));
        }
        power_2_r *= power_2_r;
    }
    // Interleave with 0 coefficients
    coefficients
        .iter()
        .interleave(vec![F::zero()].iter().cycle().take(coefficients.len() - 1))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Bls12_381;
    use ark_ec::pairing::PairingOutput;
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use blake2::Blake2b;

    use crate::tipa::structured_scalar_message::structured_scalar_power;
    use ark_dh_commitments::{
        afgho16::{AFGHOCommitmentG1, AFGHOCommitmentG2},
        identity::IdentityCommitment,
        pedersen::PedersenCommitment,
        random_generators,
    };
    use ark_inner_products::{
        InnerProduct, MultiexponentiationInnerProduct, PairingInnerProduct, ScalarInnerProduct,
    };

    type GC1 = AFGHOCommitmentG1<Bls12_381>;
    type GC2 = AFGHOCommitmentG2<Bls12_381>;
    type SC1 = PedersenCommitment<<Bls12_381 as Pairing>::G1>;
    type SC2 = PedersenCommitment<<Bls12_381 as Pairing>::G2>;

    const TEST_SIZE: usize = 8;

    #[test]
    fn pairing_inner_product_test() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingTIPA = TIPA<IP, GC1, GC2, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let (srs, ck_t) = PairingTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let (ck_a, ck_b) = srs.get_commitment_keys();
        let v_srs = srs.get_verifier_key();
        let m_a = random_generators(&mut rng, TEST_SIZE);
        let m_b = random_generators(&mut rng, TEST_SIZE);
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = GC2::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = PairingTIPA::prove(&srs, (&m_a, &m_b), (&ck_a, &ck_b, &ck_t)).unwrap();

        assert!(PairingTIPA::verify(&v_srs, &ck_t, (&com_a, &com_b, &com_t), &proof).unwrap());
    }

    #[test]
    fn multiexponentiation_inner_product_test() {
        type IP = MultiexponentiationInnerProduct<<Bls12_381 as Pairing>::G1>;
        type IPC =
            IdentityCommitment<<Bls12_381 as Pairing>::G1, <Bls12_381 as Pairing>::ScalarField>;
        type MultiExpTIPA = TIPA<IP, GC1, SC1, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let (srs, ck_t) = MultiExpTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let (ck_a, ck_b) = srs.get_commitment_keys();
        let v_srs = srs.get_verifier_key();
        let m_a = random_generators(&mut rng, TEST_SIZE);
        let mut m_b = Vec::new();
        for _ in 0..TEST_SIZE {
            m_b.push(<Bls12_381 as Pairing>::ScalarField::rand(&mut rng));
        }
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = SC1::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = MultiExpTIPA::prove(&srs, (&m_a, &m_b), (&ck_a, &ck_b, &ck_t)).unwrap();

        assert!(MultiExpTIPA::verify(&v_srs, &ck_t, (&com_a, &com_b, &com_t), &proof).unwrap());
    }

    #[test]
    fn scalar_inner_product_test() {
        type IP = ScalarInnerProduct<<Bls12_381 as Pairing>::ScalarField>;
        type IPC = IdentityCommitment<
            <Bls12_381 as Pairing>::ScalarField,
            <Bls12_381 as Pairing>::ScalarField,
        >;
        type ScalarTIPA = TIPA<IP, SC2, SC1, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let (srs, ck_t) = ScalarTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let (ck_a, ck_b) = srs.get_commitment_keys();
        let v_srs = srs.get_verifier_key();
        let mut m_a = Vec::new();
        let mut m_b = Vec::new();
        for _ in 0..TEST_SIZE {
            m_a.push(<Bls12_381 as Pairing>::ScalarField::rand(&mut rng));
            m_b.push(<Bls12_381 as Pairing>::ScalarField::rand(&mut rng));
        }
        let com_a = SC2::commit(&ck_a, &m_a).unwrap();
        let com_b = SC1::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = ScalarTIPA::prove(&srs, (&m_a, &m_b), (&ck_a, &ck_b, &ck_t)).unwrap();

        assert!(ScalarTIPA::verify(&v_srs, &ck_t, (&com_a, &com_b, &com_t), &proof).unwrap());
    }

    #[test]
    fn pairing_inner_product_with_srs_shift_test() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingTIPA = TIPA<IP, GC1, GC2, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let (srs, ck_t) = PairingTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let (ck_a, ck_b) = srs.get_commitment_keys();
        let v_srs = srs.get_verifier_key();

        let m_a = random_generators(&mut rng, TEST_SIZE);
        let m_b = random_generators(&mut rng, TEST_SIZE);
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = GC2::commit(&ck_b, &m_b).unwrap();

        let r_scalar = <<Bls12_381 as Pairing>::ScalarField>::rand(&mut rng);
        let r_vec = structured_scalar_power(TEST_SIZE, &r_scalar);
        let m_a_r = m_a
            .iter()
            .zip(&r_vec)
            .map(|(&a, r)| a * r)
            .collect::<Vec<<Bls12_381 as Pairing>::G1>>();
        let ck_a_r = ck_a
            .iter()
            .zip(&r_vec)
            .map(|(&ck, r)| ck * r.inverse().unwrap())
            .collect::<Vec<<Bls12_381 as Pairing>::G2>>();

        let t = vec![IP::inner_product(&m_a_r, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        assert_eq!(com_a, IP::inner_product(&m_a_r, &ck_a_r).unwrap());

        let proof = PairingTIPA::prove_with_srs_shift(
            &srs,
            (&m_a_r, &m_b),
            (&ck_a_r, &ck_b, &ck_t),
            &r_scalar,
        )
        .unwrap();

        assert!(PairingTIPA::verify_with_srs_shift(
            &v_srs,
            &ck_t,
            (&com_a, &com_b, &com_t),
            &proof,
            &r_scalar
        )
        .unwrap());
    }

    #[test]
    fn specialized_pairing_prepared_g2_path_matches_generic_proof_bytes() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingTIPA = TIPA<IP, GC1, GC2, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(31u64);
        let (srs, ck_t) = PairingTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let prepared = srs.prepare_for_proving();
        let (ck_a, ck_b) = srs.get_commitment_keys();
        let v_srs = srs.get_verifier_key();
        let m_a = random_generators(&mut rng, TEST_SIZE);
        let m_b = random_generators(&mut rng, TEST_SIZE);
        let r_scalar = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);
        let r_vec = structured_scalar_power(TEST_SIZE, &r_scalar);
        let m_a_r = m_a
            .iter()
            .zip(&r_vec)
            .map(|(&a, r)| a * r)
            .collect::<Vec<<Bls12_381 as Pairing>::G1>>();
        let ck_a_r = ck_a
            .iter()
            .zip(&r_vec)
            .map(|(&ck, r)| ck * r.inverse().unwrap())
            .collect::<Vec<<Bls12_381 as Pairing>::G2>>();
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = GC2::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a_r, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let generic = PairingTIPA::prove_with_prepared_srs_shift(
            &prepared,
            (&m_a_r, &m_b),
            (&ck_a_r, &ck_b, &ck_t),
            &r_scalar,
        )
        .unwrap();
        let (specialized, profile) =
            prove_pairing_inner_product_with_prepared_srs_shift_profiled::<Bls12_381, Blake2b>(
                &prepared,
                (&m_a_r, &m_b),
                (&ck_a_r, &ck_b, &ck_t),
                &r_scalar,
            )
            .unwrap();

        let mut generic_bytes = Vec::new();
        generic.serialize_uncompressed(&mut generic_bytes).unwrap();
        let mut specialized_bytes = Vec::new();
        specialized
            .serialize_uncompressed(&mut specialized_bytes)
            .unwrap();

        assert_eq!(generic_bytes, specialized_bytes);
        assert!(profile.gipa.commit_ab_ms >= 0.0);
        assert!(profile.gipa.commit_com_a_ms >= 0.0);
        assert!(profile.gipa.commit_com_b_ms >= 0.0);
        assert!(PairingTIPA::verify_with_srs_shift(
            &v_srs,
            &ck_t,
            (&com_a, &com_b, &com_t),
            &specialized,
            &r_scalar
        )
        .unwrap());
    }

    #[test]
    fn prepared_proving_srs_matches_commitment_keys() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingTIPA = TIPA<IP, GC1, GC2, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(17u64);
        let (srs, _) = PairingTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let prepared = srs.prepare_for_proving();
        let (expected_ck_1, expected_ck_2) = srs.get_commitment_keys();
        let (prepared_ck_1, prepared_ck_2) = prepared.commitment_keys();

        assert_eq!(prepared_ck_1, expected_ck_1.as_slice());
        assert_eq!(prepared_ck_2, expected_ck_2.as_slice());
        assert_eq!(
            prepared.g_alpha_powers_affine().len(),
            srs.g_alpha_powers.len()
        );
        assert_eq!(
            prepared.h_beta_powers_affine().len(),
            srs.h_beta_powers.len()
        );
    }

    #[test]
    fn affine_kzg_opening_matches_projective_path() {
        let mut rng = StdRng::seed_from_u64(23u64);
        let g = <Bls12_381 as Pairing>::G1::generator();
        let s = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);
        let powers = structured_generators_scalar_power(31, &g, &s);
        let affines = <Bls12_381 as Pairing>::G1::normalize_batch(&powers);
        let transcript = (0..4)
            .map(|_| <Bls12_381 as Pairing>::ScalarField::rand(&mut rng))
            .collect::<Vec<_>>();
        let r_shift = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);
        let challenge = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);

        let projective_opening =
            prove_commitment_key_kzg_opening(&powers, &transcript, &r_shift, &challenge).unwrap();
        let (affine_opening, profile) = prove_commitment_key_kzg_opening_with_affine_profiled::<
            <Bls12_381 as Pairing>::G1,
        >(&affines, &transcript, &r_shift, &challenge)
        .unwrap();

        assert_eq!(projective_opening, affine_opening);
        assert!(profile.coefficient_build_ms >= 0.0);
        assert!(profile.eval_quotient_ms >= 0.0);
        assert!(profile.opening_msm_ms >= 0.0);
    }
}
