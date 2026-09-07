use ark_ec::{pairing::Pairing, CurveGroup, PrimeGroup};
use ark_ff::{Field, One, UniformRand, Zero};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
#[cfg(test)]
use ark_std::cfg_iter;
use ark_std::rand::Rng;
#[cfg(test)]
use core::convert::TryInto;
use digest::Digest;
use itertools::Itertools;
use std::{marker::PhantomData, ops::MulAssign};

#[cfg(all(test, feature = "parallel"))]
use rayon::prelude::*;

use crate::{
    challenge::{
        challenge_digest, sample_bounded_challenge, ChallengeContext, ChallengeTraceSink,
        NoopChallengeTraceSink,
    },
    gipa::{GIPAProof, GIPA},
    Error,
};
#[cfg(test)]
use crate::{gipa::GIPAAux, mul_helper};
#[cfg(test)]
use ark_dh_commitments::identity::{HomomorphicPlaceholderValue, IdentityCommitment};
use ark_dh_commitments::{
    afgho16::{AFGHOCommitmentG1, AFGHOCommitmentG2},
    pedersen::PedersenCommitment,
    DoublyHomomorphicCommitment,
};
#[cfg(test)]
use ark_inner_products::cfg_multi_pairing_g1_affine_g2_prepared;
#[cfg(test)]
use ark_inner_products::PairingInnerProduct;
use ark_inner_products::{cfg_multi_pairing, InnerProduct};

pub mod structured_scalar_message;

pub trait TIPACompatibleSetup {}

impl<G: CurveGroup> TIPACompatibleSetup for PedersenCommitment<G> {}
impl<P: Pairing> TIPACompatibleSetup for AFGHOCommitmentG1<P> {}
impl<P: Pairing> TIPACompatibleSetup for AFGHOCommitmentG2<P> {}

pub struct TIPA<IP, LMC, RMC, IPC, P, D> {
    _inner_product: PhantomData<IP>,
    _left_commitment: PhantomData<LMC>,
    _right_commitment: PhantomData<RMC>,
    _inner_product_commitment: PhantomData<IPC>,
    _pair: PhantomData<P>,
    _digest: PhantomData<D>,
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
pub struct VerifierSRSData<
    G1: CanonicalSerialize + CanonicalDeserialize,
    G2: CanonicalSerialize + CanonicalDeserialize,
> {
    pub g: G1,
    pub h: G2,
    pub g_beta: G1,
    pub h_alpha: G2,
}

pub type VerifierSRS<P> = VerifierSRSData<<P as Pairing>::G1, <P as Pairing>::G2>;

#[derive(Clone)]
pub struct PreparedProvingSrs<P: Pairing> {
    g_alpha_powers_affine: Vec<P::G1Affine>,
    h_beta_powers_affine: Vec<P::G2Affine>,
    ck_1: Vec<P::G2>,
    ck_2: Vec<P::G1>,
}

/// Projects the even-indexed commitment key without iterator-specific logic.
fn even_power_projection_core<T: Clone>(powers: &[T]) -> Vec<T> {
    let mut projected = Vec::with_capacity(powers.len());
    let mut index = 0usize;
    while index < powers.len() {
        projected.push(powers[index].clone());
        index += 2;
    }
    projected
}

#[cfg(test)]
type PairingTipaProof<P, D> = TIPAProof<
    PairingInnerProduct<P>,
    AFGHOCommitmentG1<P>,
    AFGHOCommitmentG2<P>,
    IdentityCommitment<ark_ec::pairing::PairingOutput<P>, <P as Pairing>::ScalarField>,
    P,
    D,
>;

#[cfg(test)]
const PAIRING_GIPA_RESCALE_THRESHOLD: usize = 64;

impl<P: Pairing> SRS<P> {
    pub fn get_commitment_keys(&self) -> (Vec<P::G2>, Vec<P::G1>) {
        let ck_1 = even_power_projection_core(&self.h_beta_powers);
        let ck_2 = even_power_projection_core(&self.g_alpha_powers);
        (ck_1, ck_2)
    }

    pub fn prepare_for_proving(&self) -> PreparedProvingSrs<P> {
        PreparedProvingSrs::new(self)
    }

    pub fn get_verifier_key(&self) -> VerifierSRS<P> {
        VerifierSRS::<P> {
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
        let ck_1 = even_power_projection_core(&srs.h_beta_powers);
        let ck_2 = even_power_projection_core(&srs.g_alpha_powers);
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn pairing_affine<P: Pairing>(
    left: &[P::G1Affine],
    right: &[P::G2Affine],
) -> Result<ark_ec::pairing::PairingOutput<P>, Error> {
    let right_prepared = prepare_g2_affine_vec::<P>(right);
    pairing_affine_with_prepared_g2::<P>(left, &right_prepared)
}

#[cfg(test)]
fn pairing_affine_with_prepared_g2<P: Pairing>(
    left: &[P::G1Affine],
    right: &[P::G2Prepared],
) -> Result<ark_ec::pairing::PairingOutput<P>, Error> {
    cfg_multi_pairing_g1_affine_g2_prepared::<P>(left, right)
        .ok_or_else(|| Box::new(std::io::Error::other("pairing length mismatch")) as Error)
}

#[cfg(test)]
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
    let (proof, aux) =
        prove_pairing_inner_product_gipa_with_aux::<P, D>(context, trace, values, ck)?;

    let (ck_a_final, ck_b_final) = aux.ck_base;
    let transcript = aux.r_transcript;

    let transcript_inverse = transcript.iter().map(|x| x.inverse().unwrap()).collect();

    let r_inverse = r_shift.inverse().ok_or_else(|| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "r_shift must be non-zero before inversion",
        )) as Error
    })?;

    let c = sample_bounded_challenge::<_, Error, _>(|nonce| {
        let mut hash_input = Vec::new();
        if let Some(first) = transcript.first() {
            first.serialize_uncompressed(&mut hash_input)?;
        }
        ck_a_final.serialize_uncompressed(&mut hash_input)?;
        ck_b_final.serialize_uncompressed(&mut hash_input)?;
        Ok(P::ScalarField::from_random_bytes(
            &challenge_digest::<D, _>(context, trace, b"tipa.ab.kzg", nonce, &hash_input),
        ))
    })?;

    let ck_a_kzg_opening = prove_commitment_key_kzg_opening_with_affine(
        prepared_srs.h_beta_powers_affine(),
        &transcript_inverse,
        &r_inverse,
        &c,
    )?;

    let ck_b_kzg_opening = prove_commitment_key_kzg_opening_with_affine(
        prepared_srs.g_alpha_powers_affine(),
        &transcript,
        &P::ScalarField::one(),
        &c,
    )?;

    Ok(TIPAProof {
        gipa_proof: proof,
        final_ck: (ck_a_final, ck_b_final),
        final_ck_proof: (ck_a_kzg_opening, ck_b_kzg_opening),
        _pair: PhantomData,
    })
}

#[cfg(test)]
fn prove_pairing_inner_product_gipa_with_aux<P, D>(
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
    ),
    Error,
>
where
    P: Pairing,
    D: Digest,
{
    let (mut m_a, mut m_b) = (values.0.to_vec(), values.1.to_vec());
    let (mut ck_a, mut ck_b) = (ck.0.to_vec(), ck.1.to_vec());
    let ck_t = ck.2;
    let mut r_commitment_steps = Vec::new();
    let mut r_transcript: Vec<P::ScalarField> = Vec::new();

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
            let ip = pairing_affine_with_prepared_g2::<P>(&m_a_1_affine, &m_b_1_prepared)
                .map_err(|err| err.to_string())?;

            let com_a =
                pairing_affine::<P>(&m_a_1_affine, ck_a_1_affine).map_err(|err| err.to_string())?;

            let com_b = pairing_affine_with_prepared_g2::<P>(&ck_b_1_affine, &m_b_1_prepared)
                .map_err(|err| err.to_string())?;

            let com = (
                com_a,
                com_b,
                IdentityCommitment::<ark_ec::pairing::PairingOutput<P>, P::ScalarField>::commit(
                    &vec![ck_t.clone()],
                    &vec![ip],
                )
                .map_err(|err| err.to_string())?,
            );
            Ok::<_, String>(com)
        };
        let commit_r = || {
            let ip = pairing_affine_with_prepared_g2::<P>(&m_a_2_affine, &m_b_2_prepared)
                .map_err(|err| err.to_string())?;

            let com_a =
                pairing_affine::<P>(&m_a_2_affine, ck_a_2_affine).map_err(|err| err.to_string())?;

            let com_b = pairing_affine_with_prepared_g2::<P>(&ck_b_2_affine, &m_b_2_prepared)
                .map_err(|err| err.to_string())?;

            let com = (
                com_a,
                com_b,
                IdentityCommitment::<ark_ec::pairing::PairingOutput<P>, P::ScalarField>::commit(
                    &vec![ck_t.clone()],
                    &vec![ip],
                )
                .map_err(|err| err.to_string())?,
            );
            Ok::<_, String>(com)
        };

        #[cfg(feature = "parallel")]
        let (commit_l_result, commit_r_result) = rayon::join(commit_l, commit_r);

        #[cfg(not(feature = "parallel"))]
        let (commit_l_result, commit_r_result) = (commit_l(), commit_r());

        let com_1 = commit_l_result.map_err(|err: String| std::io::Error::other(err))?;
        let com_2 = commit_r_result.map_err(|err: String| std::io::Error::other(err))?;
        // Per-task self-times: under the parallel seam `commit_l`/`commit_r` run
        // concurrently, so the `_l`/`_r` fields overlap in wall-clock and are not
        // additive (their sum can exceed the round's elapsed time).

        let default_transcript = Default::default();
        let transcript = r_transcript.last().unwrap_or(&default_transcript);
        let (c, c_inv) = sample_bounded_challenge::<_, Error, _>(|nonce| {
            let mut hash_input = Vec::new();
            transcript.serialize_uncompressed(&mut hash_input)?;
            com_1.0.serialize_uncompressed(&mut hash_input)?;
            com_1.1.serialize_uncompressed(&mut hash_input)?;
            com_1.2.serialize_uncompressed(&mut hash_input)?;
            com_2.0.serialize_uncompressed(&mut hash_input)?;
            com_2.1.serialize_uncompressed(&mut hash_input)?;
            com_2.2.serialize_uncompressed(&mut hash_input)?;
            let c: P::ScalarField = u128::from_be_bytes(
                challenge_digest::<D, _>(context, trace, b"tipa.ab.gipa.round", nonce, &hash_input)
                    .as_slice()[0..16]
                    .try_into()
                    .unwrap(),
            )
            .into();
            Ok(c.inverse().map(|c_inv| (c_inv, c)))
        })?;

        #[cfg(feature = "parallel")]
        let (next_m_a, next_m_b, next_ck_a, next_ck_b) = {
            let ((next_m_a, next_m_b), (next_ck_a, next_ck_b)) = rayon::join(
                || {
                    rayon::join(
                        || rescale_fold_curve(m_a_1, m_a_2, &c),
                        || rescale_fold_curve(m_b_2, m_b_1, &c_inv),
                    )
                },
                || {
                    rayon::join(
                        || rescale_fold_curve(ck_a_2, ck_a_1, &c_inv),
                        || rescale_fold_curve(ck_b_1, ck_b_2, &c),
                    )
                },
            );
            (next_m_a, next_m_b, next_ck_a, next_ck_b)
        };

        #[cfg(not(feature = "parallel"))]
        let (next_m_a, next_m_b, next_ck_a, next_ck_b) = (
            rescale_fold_curve(m_a_1, m_a_2, &c),
            rescale_fold_curve(m_b_2, m_b_1, &c_inv),
            rescale_fold_curve(ck_a_2, ck_a_1, &c_inv),
            rescale_fold_curve(ck_b_1, ck_b_2, &c),
        );

        m_a = next_m_a;
        m_b = next_m_b;
        ck_a = next_ck_a;
        ck_b = next_ck_b;

        r_commitment_steps.push((com_1, com_2));
        r_transcript.push(c);
    };

    r_transcript.reverse();
    r_commitment_steps.reverse();

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
        let prepared_srs = srs.prepare_for_proving();
        Self::prove_with_prepared_srs_shift(context, &prepared_srs, values, ck, r_shift)
    }

    pub fn prove_with_prepared_srs_shift(
        context: &ChallengeContext,
        prepared_srs: &PreparedProvingSrs<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<TIPAProof<IP, LMC, RMC, IPC, P, D>, Error> {
        let mut trace = NoopChallengeTraceSink;
        Self::prove_with_prepared_srs_shift_with_trace(
            context,
            &mut trace,
            prepared_srs,
            values,
            ck,
            r_shift,
        )
    }

    pub fn prove_with_prepared_srs_shift_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        prepared_srs: &PreparedProvingSrs<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<TIPAProof<IP, LMC, RMC, IPC, P, D>, Error>
    where
        S: ChallengeTraceSink,
    {
        Self::prove_with_prepared_srs_shift_with_labels_with_trace(
            context,
            trace,
            b"tipa.generic.gipa.round",
            b"tipa.generic.kzg",
            prepared_srs,
            values,
            ck,
            r_shift,
        )
    }

    fn prove_with_prepared_srs_shift_with_labels_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        gipa_stage_label: &'static [u8],
        kzg_stage_label: &'static [u8],
        prepared_srs: &PreparedProvingSrs<P>,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        r_shift: &P::ScalarField,
    ) -> Result<TIPAProof<IP, LMC, RMC, IPC, P, D>, Error>
    where
        S: ChallengeTraceSink,
    {
        // Run GIPA

        let (proof, aux) = <GIPA<IP, LMC, RMC, IPC, D>>::prove_with_aux_with_stage_with_trace(
            context,
            trace,
            gipa_stage_label,
            values,
            (ck.0, ck.1, &vec![ck.2.clone()]),
        )?;

        // Prove final commitment keys are wellformed
        let (ck_a_final, ck_b_final) = aux.ck_base;
        let transcript = aux.r_transcript;

        let transcript_inverse = transcript.iter().map(|x| x.inverse().unwrap()).collect();

        let r_inverse = r_shift.inverse().ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "r_shift must be non-zero before inversion",
            )) as Error
        })?;

        // KZG challenge point

        let c = sample_bounded_challenge::<_, Error, _>(|nonce| {
            let mut hash_input = Vec::new();
            if let Some(first) = transcript.first() {
                first.serialize_uncompressed(&mut hash_input)?;
            }
            ck_a_final.serialize_uncompressed(&mut hash_input)?;
            ck_b_final.serialize_uncompressed(&mut hash_input)?;
            Ok(LMC::Scalar::from_random_bytes(&challenge_digest::<D, _>(
                context,
                trace,
                kzg_stage_label,
                nonce,
                &hash_input,
            )))
        })?;

        // Complete KZG proofs

        let ck_a_kzg_opening = prove_commitment_key_kzg_opening_with_affine(
            prepared_srs.h_beta_powers_affine(),
            &transcript_inverse,
            &r_inverse,
            &c,
        )?;

        let ck_b_kzg_opening = prove_commitment_key_kzg_opening_with_affine(
            prepared_srs.g_alpha_powers_affine(),
            &transcript,
            &<P::ScalarField>::one(),
            &c,
        )?;

        Ok(TIPAProof {
            gipa_proof: proof,
            final_ck: (ck_a_final, ck_b_final),
            final_ck_proof: (ck_a_kzg_opening, ck_b_kzg_opening),
            _pair: PhantomData,
        })
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
        let c = sample_bounded_challenge::<_, Error, _>(|nonce| {
            let mut hash_input = Vec::new();
            if let Some(first) = transcript.first() {
                first.serialize_uncompressed(&mut hash_input)?;
            }
            ck_a_final.serialize_uncompressed(&mut hash_input)?;
            ck_b_final.serialize_uncompressed(&mut hash_input)?;
            Ok(LMC::Scalar::from_random_bytes(&challenge_digest::<D, _>(
                context,
                trace,
                kzg_stage_label,
                nonce,
                &hash_input,
            )))
        })?;

        let r_shift_inverse = r_shift.inverse().ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "r_shift must be non-zero before inversion",
            )) as Error
        })?;
        #[cfg(feature = "parallel")]
        let (ck_a_result, ck_b_result) = rayon::join(
            || {
                verify_commitment_key_g2_kzg_opening::<P>(
                    v_srs,
                    ck_a_final,
                    ck_a_proof,
                    &transcript_inverse,
                    &r_shift_inverse,
                    &c,
                )
                .map_err(|err| err.to_string())
            },
            || {
                verify_commitment_key_g1_kzg_opening::<P>(
                    v_srs,
                    ck_b_final,
                    ck_b_proof,
                    &transcript,
                    &<P::ScalarField>::one(),
                    &c,
                )
                .map_err(|err| err.to_string())
            },
        );

        #[cfg(not(feature = "parallel"))]
        let (ck_a_result, ck_b_result) = (
            verify_commitment_key_g2_kzg_opening::<P>(
                v_srs,
                ck_a_final,
                ck_a_proof,
                &transcript_inverse,
                &r_shift_inverse,
                &c,
            )
            .map_err(|err| err.to_string()),
            verify_commitment_key_g1_kzg_opening::<P>(
                v_srs,
                ck_b_final,
                ck_b_proof,
                &transcript,
                &<P::ScalarField>::one(),
                &c,
            )
            .map_err(|err| err.to_string()),
        );

        let ck_a_valid = ck_a_result.map_err(|err: String| std::io::Error::other(err))?;
        let ck_b_valid = ck_b_result.map_err(|err: String| std::io::Error::other(err))?;

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
    let opening =
        prove_commitment_key_kzg_opening_with_affine(&affines, transcript, r_shift, kzg_challenge)?;
    Ok(opening)
}

/// Coefficients of `(P(X) - P(z)) / (X - z)`, in ascending degree order.
///
/// The constant subtraction changes only the zero-degree remainder, so the
/// quotient is determined by the original coefficients of `P`. Returning an
/// empty vector for constants makes the helper total; shipping KZG inputs are
/// nonempty power-of-two coefficient vectors.
fn synthetic_division_coefficients<F>(coefficients: &[F], point: &F) -> Vec<F>
where
    F: Clone + std::ops::Add<Output = F> + std::ops::Mul<Output = F>,
{
    if coefficients.len() <= 1 {
        return Vec::new();
    }

    let mut descending = Vec::with_capacity(coefficients.len() - 1);
    let mut index = coefficients.len() - 1;
    let mut accumulator = coefficients[index].clone();
    descending.push(accumulator.clone());
    while index > 1 {
        index -= 1;
        accumulator = coefficients[index].clone() + accumulator * point.clone();
        descending.push(accumulator.clone());
    }
    descending.reverse();
    descending
}

pub fn prove_commitment_key_kzg_opening_with_affine<G: CurveGroup>(
    srs_powers: &[G::Affine],
    transcript: &Vec<G::ScalarField>,
    r_shift: &G::ScalarField,
    challenge: &G::ScalarField,
) -> Result<G, Error> {
    let coefficients = polynomial_coefficients_from_transcript(transcript, r_shift);
    assert_eq!(srs_powers.len(), coefficients.len());
    let quotient = synthetic_division_coefficients(&coefficients, challenge);
    #[cfg(debug_assertions)]
    {
        let evaluation =
            polynomial_evaluation_product_form_from_transcript(transcript, challenge, r_shift);
        debug_assert!(
            coefficients.len() <= 1
                || coefficients[0] - evaluation + *challenge * quotient[0]
                    == G::ScalarField::zero(),
            "product-form evaluation must match the coefficient polynomial"
        );
    }
    G::msm(&srs_powers[..quotient.len()], &quotient)
        .map_err(|_| Box::new(std::io::Error::other("KZG opening MSM length mismatch")) as Error)
}

#[derive(Clone)]
struct KzgG2VerifierCoreInput<F, G1, G2, GT> {
    g: G1,
    g_beta: G1,
    h: G2,
    ck_final: G2,
    ck_opening: G2,
    eval: F,
    z: F,
    _pairing_output: PhantomData<GT>,
}

#[derive(Clone)]
struct KzgG1VerifierCoreInput<F, G1, G2, GT> {
    g: G1,
    h_alpha: G2,
    h: G2,
    ck_final: G1,
    ck_opening: G1,
    eval: F,
    z: F,
    _pairing_output: PhantomData<GT>,
}

fn verify_commitment_key_g2_kzg_equation_core<F, G1, G2, GT, E>(
    input: KzgG2VerifierCoreInput<F, G1, G2, GT>,
    pairing: &E,
) -> bool
where
    F: Clone,
    G1: Clone
        + std::ops::Mul<F, Output = G1>
        + std::ops::Sub<Output = G1>
        + std::ops::Neg<Output = G1>,
    G2: Clone + std::ops::Mul<F, Output = G2> + std::ops::Sub<Output = G2>,
    GT: Zero,
    E: Fn(&[G1], &[G2]) -> Option<GT>,
{
    let right_0 = input.ck_final - input.h * input.eval.clone();
    let left_1 = -(input.g_beta - input.g.clone() * input.z.clone());
    match pairing(&[input.g, left_1], &[right_0, input.ck_opening]) {
        Some(output) => output.is_zero(),
        None => false,
    }
}

fn verify_commitment_key_g1_kzg_equation_core<F, G1, G2, GT, E>(
    input: KzgG1VerifierCoreInput<F, G1, G2, GT>,
    pairing: &E,
) -> bool
where
    F: Clone,
    G1: Clone
        + std::ops::Mul<F, Output = G1>
        + std::ops::Sub<Output = G1>
        + std::ops::Neg<Output = G1>,
    G2: Clone + std::ops::Mul<F, Output = G2> + std::ops::Sub<Output = G2>,
    GT: Zero,
    E: Fn(&[G1], &[G2]) -> Option<GT>,
{
    let left_0 = input.ck_final - input.g * input.eval.clone();
    let right_1 = input.h_alpha - input.h.clone() * input.z.clone();
    match pairing(&[left_0, -input.ck_opening], &[input.h, right_1]) {
        Some(output) => output.is_zero(),
        None => false,
    }
}

pub(crate) fn verify_commitment_key_g2_kzg_opening_core<F, G1, G2, GT, E>(
    g: G1,
    g_beta: G1,
    h: G2,
    ck_final: G2,
    ck_opening: G2,
    transcript: &Vec<F>,
    r_shift: &F,
    z: &F,
    pairing: &E,
) -> bool
where
    F: Clone + One + std::ops::Add<Output = F> + std::ops::Mul<Output = F>,
    G1: Clone
        + std::ops::Mul<F, Output = G1>
        + std::ops::Sub<Output = G1>
        + std::ops::Neg<Output = G1>,
    G2: Clone + std::ops::Mul<F, Output = G2> + std::ops::Sub<Output = G2>,
    GT: Zero,
    E: Fn(&[G1], &[G2]) -> Option<GT>,
{
    let eval = polynomial_evaluation_product_form_from_transcript(transcript, z, r_shift);
    verify_commitment_key_g2_kzg_equation_core(
        KzgG2VerifierCoreInput {
            g,
            g_beta,
            h,
            ck_final,
            ck_opening,
            eval,
            z: z.clone(),
            _pairing_output: PhantomData,
        },
        pairing,
    )
}

pub(crate) fn verify_commitment_key_g1_kzg_opening_core<F, G1, G2, GT, E>(
    g: G1,
    h_alpha: G2,
    h: G2,
    ck_final: G1,
    ck_opening: G1,
    transcript: &Vec<F>,
    r_shift: &F,
    z: &F,
    pairing: &E,
) -> bool
where
    F: Clone + One + std::ops::Add<Output = F> + std::ops::Mul<Output = F>,
    G1: Clone
        + std::ops::Mul<F, Output = G1>
        + std::ops::Sub<Output = G1>
        + std::ops::Neg<Output = G1>,
    G2: Clone + std::ops::Mul<F, Output = G2> + std::ops::Sub<Output = G2>,
    GT: Zero,
    E: Fn(&[G1], &[G2]) -> Option<GT>,
{
    let eval = polynomial_evaluation_product_form_from_transcript(transcript, z, r_shift);
    verify_commitment_key_g1_kzg_equation_core(
        KzgG1VerifierCoreInput {
            g,
            h_alpha,
            h,
            ck_final,
            ck_opening,
            eval,
            z: z.clone(),
            _pairing_output: PhantomData,
        },
        pairing,
    )
}

pub fn verify_commitment_key_g2_kzg_opening<P: Pairing>(
    v_srs: &VerifierSRS<P>,
    ck_final: &P::G2,
    ck_opening: &P::G2,
    transcript: &Vec<P::ScalarField>,
    r_shift: &P::ScalarField,
    kzg_challenge: &P::ScalarField,
) -> Result<bool, Error> {
    Ok(verify_commitment_key_g2_kzg_opening_core(
        v_srs.g.clone(),
        v_srs.g_beta.clone(),
        v_srs.h.clone(),
        ck_final.clone(),
        ck_opening.clone(),
        transcript,
        r_shift,
        kzg_challenge,
        &cfg_multi_pairing::<P>,
    ))
}

pub fn verify_commitment_key_g1_kzg_opening<P: Pairing>(
    v_srs: &VerifierSRS<P>,
    ck_final: &P::G1,
    ck_opening: &P::G1,
    transcript: &Vec<P::ScalarField>,
    r_shift: &P::ScalarField,
    kzg_challenge: &P::ScalarField,
) -> Result<bool, Error> {
    Ok(verify_commitment_key_g1_kzg_opening_core(
        v_srs.g.clone(),
        v_srs.h_alpha.clone(),
        v_srs.h.clone(),
        ck_final.clone(),
        ck_opening.clone(),
        transcript,
        r_shift,
        kzg_challenge,
        &cfg_multi_pairing::<P>,
    ))
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

fn polynomial_evaluation_product_form_from_transcript<F>(
    transcript: &Vec<F>,
    z: &F,
    r_shift: &F,
) -> F
where
    F: Clone + One + std::ops::Add<Output = F> + std::ops::Mul<Output = F>,
{
    let mut power_2_zr = (z.clone() * z.clone()) * r_shift.clone();
    let mut product_form = Vec::new();
    for x in transcript.iter() {
        product_form.push(F::one() + (x.clone() * power_2_zr.clone()));
        power_2_zr = power_2_zr.clone() * power_2_zr;
    }

    let mut product = F::one();
    for factor in product_form {
        product = product * factor;
    }
    product
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
    use ark_bls12_377::Bls12_377;
    use ark_bls12_381::Bls12_381;
    use ark_ec::{pairing::PairingOutput, PrimeGroup, VariableBaseMSM};
    use ark_ff::Zero;
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
    fn polynomial_coefficients_match_direct_expansion() {
        type Scalar = <Bls12_377 as Pairing>::ScalarField;

        let transcript = vec![Scalar::from(3u64), Scalar::from(5u64)];
        let r_shift = Scalar::from(7u64);

        assert_eq!(
            polynomial_coefficients_from_transcript(&transcript, &r_shift),
            vec![
                Scalar::from(1u64),
                Scalar::from(0u64),
                Scalar::from(21u64),
                Scalar::from(0u64),
                Scalar::from(245u64),
                Scalar::from(0u64),
                Scalar::from(5145u64),
            ]
        );
    }

    #[test]
    fn polynomial_evaluation_product_form_matches_coefficients() {
        type Scalar = <Bls12_377 as Pairing>::ScalarField;

        let transcript = vec![Scalar::from(3u64), Scalar::from(5u64), Scalar::from(13u64)];
        let r_shift = Scalar::from(7u64);
        let z = Scalar::from(11u64);
        let coefficients = polynomial_coefficients_from_transcript(&transcript, &r_shift);
        let evaluation = coefficients
            .iter()
            .rev()
            .fold(Scalar::zero(), |accumulator, coefficient| {
                accumulator * z + coefficient
            });

        assert_eq!(
            polynomial_evaluation_product_form_from_transcript(&transcript, &z, &r_shift),
            evaluation
        );
    }

    #[test]
    fn synthetic_division_constructs_exact_monic_linear_quotient() {
        type Scalar = <Bls12_377 as Pairing>::ScalarField;

        fn evaluate(coefficients: &[Scalar], point: Scalar) -> Scalar {
            coefficients
                .iter()
                .rev()
                .fold(Scalar::zero(), |accumulator, coefficient| {
                    accumulator * point + coefficient
                })
        }

        assert!(synthetic_division_coefficients::<Scalar>(&[], &Scalar::from(7u64)).is_empty());
        assert!(
            synthetic_division_coefficients(&[Scalar::from(9u64)], &Scalar::from(7u64)).is_empty()
        );

        let coefficients = vec![
            Scalar::from(2u64),
            Scalar::from(3u64),
            Scalar::from(5u64),
            Scalar::from(7u64),
        ];
        let opening_point = Scalar::from(11u64);
        let quotient = synthetic_division_coefficients(&coefficients, &opening_point);
        assert_eq!(
            quotient,
            vec![
                Scalar::from(905u64),
                Scalar::from(82u64),
                Scalar::from(7u64),
            ]
        );

        let evaluation = evaluate(&coefficients, opening_point);
        for point in [Scalar::zero(), Scalar::one(), Scalar::from(19u64)] {
            assert_eq!(
                evaluate(&coefficients, point) - evaluation,
                (point - opening_point) * evaluate(&quotient, point)
            );
        }
    }

    fn assert_kzg_adapter_parity<P: Pairing>() {
        let v_srs: VerifierSRS<P> = VerifierSRSData {
            g: P::G1::generator(),
            h: P::G2::generator(),
            g_beta: P::G1::generator(),
            h_alpha: P::G2::generator(),
        };
        let transcript = vec![P::ScalarField::from(3u64), P::ScalarField::from(5u64)];
        let r_shift = P::ScalarField::from(7u64);
        let challenge = P::ScalarField::from(11u64);
        let eval =
            polynomial_evaluation_product_form_from_transcript(&transcript, &challenge, &r_shift);

        let g2_input = KzgG2VerifierCoreInput {
            g: v_srs.g.clone(),
            g_beta: v_srs.g_beta.clone(),
            h: v_srs.h.clone(),
            ck_final: v_srs.h.clone() * eval.clone(),
            ck_opening: P::G2::zero(),
            eval: eval.clone(),
            z: challenge.clone(),
            _pairing_output: PhantomData,
        };
        let delegated_g2 = verify_commitment_key_g2_kzg_opening::<P>(
            &v_srs,
            &g2_input.ck_final,
            &g2_input.ck_opening,
            &transcript,
            &r_shift,
            &P::ScalarField::from(11u64),
        )
        .unwrap();
        let core_g2 = verify_commitment_key_g2_kzg_opening_core(
            g2_input.g.clone(),
            g2_input.g_beta.clone(),
            g2_input.h.clone(),
            g2_input.ck_final.clone(),
            g2_input.ck_opening.clone(),
            &transcript,
            &r_shift,
            &challenge,
            &cfg_multi_pairing::<P>,
        );
        let equation_g2 =
            verify_commitment_key_g2_kzg_equation_core(g2_input.clone(), &cfg_multi_pairing::<P>);
        assert_eq!(delegated_g2, core_g2);
        assert_eq!(delegated_g2, equation_g2);
        assert!(delegated_g2);
        assert!(!verify_commitment_key_g2_kzg_opening_core::<
            _,
            _,
            _,
            PairingOutput<P>,
            _,
        >(
            g2_input.g,
            g2_input.g_beta,
            g2_input.h,
            g2_input.ck_final,
            g2_input.ck_opening,
            &transcript,
            &r_shift,
            &challenge,
            &|_: &[P::G1], _: &[P::G2]| None::<ark_ec::pairing::PairingOutput<P>>,
        ));

        let invalid_g2_final = v_srs.h.clone() * eval.clone() + P::G2::generator();
        let invalid_g2_input = KzgG2VerifierCoreInput {
            g: v_srs.g.clone(),
            g_beta: v_srs.g_beta.clone(),
            h: v_srs.h.clone(),
            ck_final: invalid_g2_final.clone(),
            ck_opening: P::G2::zero(),
            eval: eval.clone(),
            z: challenge.clone(),
            _pairing_output: PhantomData,
        };
        let delegated_invalid_g2 = verify_commitment_key_g2_kzg_opening::<P>(
            &v_srs,
            &invalid_g2_final,
            &P::G2::zero(),
            &transcript,
            &r_shift,
            &challenge,
        )
        .unwrap();
        let core_invalid_g2 = verify_commitment_key_g2_kzg_opening_core(
            invalid_g2_input.g.clone(),
            invalid_g2_input.g_beta.clone(),
            invalid_g2_input.h.clone(),
            invalid_g2_input.ck_final.clone(),
            invalid_g2_input.ck_opening.clone(),
            &transcript,
            &r_shift,
            &challenge,
            &cfg_multi_pairing::<P>,
        );
        let equation_invalid_g2 =
            verify_commitment_key_g2_kzg_equation_core(invalid_g2_input, &cfg_multi_pairing::<P>);
        assert_eq!(delegated_invalid_g2, core_invalid_g2);
        assert_eq!(delegated_invalid_g2, equation_invalid_g2);
        assert!(!delegated_invalid_g2);

        let g1_input = KzgG1VerifierCoreInput {
            g: v_srs.g.clone(),
            h_alpha: v_srs.h_alpha.clone(),
            h: v_srs.h.clone(),
            ck_final: v_srs.g.clone() * eval.clone(),
            ck_opening: P::G1::zero(),
            eval: eval.clone(),
            z: challenge.clone(),
            _pairing_output: PhantomData,
        };
        let delegated_g1 = verify_commitment_key_g1_kzg_opening::<P>(
            &v_srs,
            &g1_input.ck_final,
            &g1_input.ck_opening,
            &transcript,
            &r_shift,
            &challenge,
        )
        .unwrap();
        let core_g1 = verify_commitment_key_g1_kzg_opening_core(
            g1_input.g.clone(),
            g1_input.h_alpha.clone(),
            g1_input.h.clone(),
            g1_input.ck_final.clone(),
            g1_input.ck_opening.clone(),
            &transcript,
            &r_shift,
            &challenge,
            &cfg_multi_pairing::<P>,
        );
        let equation_g1 =
            verify_commitment_key_g1_kzg_equation_core(g1_input.clone(), &cfg_multi_pairing::<P>);
        assert_eq!(delegated_g1, core_g1);
        assert_eq!(delegated_g1, equation_g1);
        assert!(delegated_g1);
        assert!(!verify_commitment_key_g1_kzg_opening_core::<
            _,
            _,
            _,
            PairingOutput<P>,
            _,
        >(
            g1_input.g,
            g1_input.h_alpha,
            g1_input.h,
            g1_input.ck_final,
            g1_input.ck_opening,
            &transcript,
            &r_shift,
            &challenge,
            &|_: &[P::G1], _: &[P::G2]| None::<ark_ec::pairing::PairingOutput<P>>,
        ));

        let invalid_g1_final = v_srs.g.clone() * eval + P::G1::generator();
        let invalid_g1_input = KzgG1VerifierCoreInput {
            g: v_srs.g.clone(),
            h_alpha: v_srs.h_alpha.clone(),
            h: v_srs.h.clone(),
            ck_final: invalid_g1_final.clone(),
            ck_opening: P::G1::zero(),
            eval: polynomial_evaluation_product_form_from_transcript(
                &transcript,
                &challenge,
                &r_shift,
            ),
            z: challenge.clone(),
            _pairing_output: PhantomData,
        };
        let delegated_invalid_g1 = verify_commitment_key_g1_kzg_opening::<P>(
            &v_srs,
            &invalid_g1_final,
            &P::G1::zero(),
            &transcript,
            &r_shift,
            &challenge,
        )
        .unwrap();
        let core_invalid_g1 = verify_commitment_key_g1_kzg_opening_core(
            invalid_g1_input.g.clone(),
            invalid_g1_input.h_alpha.clone(),
            invalid_g1_input.h.clone(),
            invalid_g1_input.ck_final.clone(),
            invalid_g1_input.ck_opening.clone(),
            &transcript,
            &r_shift,
            &challenge,
            &cfg_multi_pairing::<P>,
        );
        let equation_invalid_g1 =
            verify_commitment_key_g1_kzg_equation_core(invalid_g1_input, &cfg_multi_pairing::<P>);
        assert_eq!(delegated_invalid_g1, core_invalid_g1);
        assert_eq!(delegated_invalid_g1, equation_invalid_g1);
        assert!(!delegated_invalid_g1);
    }

    #[test]
    fn kzg_adapter_parity_bls12_381_including_pairing_failure() {
        assert_kzg_adapter_parity::<Bls12_381>();
    }

    #[test]
    fn kzg_adapter_parity_bls12_377_including_pairing_failure() {
        assert_kzg_adapter_parity::<Bls12_377>();
    }

    #[test]
    fn pairing_inner_product_test() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingTIPA = TIPA<IP, GC1, GC2, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
        let (srs, ck_t) = PairingTIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let (ck_a, ck_b) = srs.get_commitment_keys();
        let v_srs = srs.get_verifier_key();
        let m_a = random_generators(&mut rng, TEST_SIZE);
        let m_b = random_generators(&mut rng, TEST_SIZE);
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = GC2::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = PairingTIPA::prove(
            &challenge_context,
            &srs,
            (&m_a, &m_b),
            (&ck_a, &ck_b, &ck_t),
        )
        .unwrap();

        assert!(PairingTIPA::verify(
            &challenge_context,
            &v_srs,
            &ck_t,
            (&com_a, &com_b, &com_t),
            &proof
        )
        .unwrap());
    }

    #[test]
    fn multiexponentiation_inner_product_test() {
        type IP = MultiexponentiationInnerProduct<<Bls12_381 as Pairing>::G1>;
        type IPC =
            IdentityCommitment<<Bls12_381 as Pairing>::G1, <Bls12_381 as Pairing>::ScalarField>;
        type MultiExpTIPA = TIPA<IP, GC1, SC1, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
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

        let proof = MultiExpTIPA::prove(
            &challenge_context,
            &srs,
            (&m_a, &m_b),
            (&ck_a, &ck_b, &ck_t),
        )
        .unwrap();

        assert!(MultiExpTIPA::verify(
            &challenge_context,
            &v_srs,
            &ck_t,
            (&com_a, &com_b, &com_t),
            &proof
        )
        .unwrap());
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
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
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

        let proof = ScalarTIPA::prove(
            &challenge_context,
            &srs,
            (&m_a, &m_b),
            (&ck_a, &ck_b, &ck_t),
        )
        .unwrap();

        assert!(ScalarTIPA::verify(
            &challenge_context,
            &v_srs,
            &ck_t,
            (&com_a, &com_b, &com_t),
            &proof
        )
        .unwrap());
    }

    #[test]
    fn pairing_inner_product_with_srs_shift_test() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingTIPA = TIPA<IP, GC1, GC2, IPC, Bls12_381, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
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
            &challenge_context,
            &srs,
            (&m_a_r, &m_b),
            (&ck_a_r, &ck_b, &ck_t),
            &r_scalar,
        )
        .unwrap();

        assert!(PairingTIPA::verify_with_srs_shift(
            &challenge_context,
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
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
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

        let mut generic_trace = NoopChallengeTraceSink;
        let generic = PairingTIPA::prove_with_prepared_srs_shift_with_labels_with_trace(
            &challenge_context,
            &mut generic_trace,
            b"tipa.ab.gipa.round",
            b"tipa.ab.kzg",
            &prepared,
            (&m_a_r, &m_b),
            (&ck_a_r, &ck_b, &ck_t),
            &r_scalar,
        )
        .unwrap();
        let mut trace = NoopChallengeTraceSink;
        let specialized =
            prove_pairing_inner_product_with_prepared_srs_shift::<Bls12_381, Blake2b>(
                &challenge_context,
                &mut trace,
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

        assert!(PairingTIPA::verify_with_srs_shift_and_labels(
            &challenge_context,
            b"tipa.ab.gipa.round",
            b"tipa.ab.kzg",
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
        let affine_opening = prove_commitment_key_kzg_opening_with_affine::<
            <Bls12_381 as Pairing>::G1,
        >(&affines, &transcript, &r_shift, &challenge)
        .unwrap();

        assert_eq!(projective_opening, affine_opening);
    }

    #[test]
    fn affine_kzg_matches_exact_quotient_msm() {
        type G = <Bls12_381 as Pairing>::G1;
        type F = <Bls12_381 as Pairing>::ScalarField;

        let mut rng = StdRng::seed_from_u64(29u64);
        let generator = G::generator();
        let s = F::rand(&mut rng);
        let powers = structured_generators_scalar_power(31, &generator, &s);
        let affines = G::normalize_batch(&powers);
        let transcript = (0..4).map(|_| F::rand(&mut rng)).collect::<Vec<_>>();
        let r_shift = F::rand(&mut rng);
        let challenge = F::rand(&mut rng);

        let coefficients = polynomial_coefficients_from_transcript(&transcript, &r_shift);
        let mut quotient = synthetic_division_coefficients(&coefficients, &challenge);
        quotient.resize(affines.len(), F::zero());
        let expected = G::msm(&affines, &quotient).expect("lengths match");

        let opening = prove_commitment_key_kzg_opening_with_affine::<G>(
            &affines,
            &transcript,
            &r_shift,
            &challenge,
        )
        .expect("opening must construct");
        assert_eq!(opening, expected);
    }
}
