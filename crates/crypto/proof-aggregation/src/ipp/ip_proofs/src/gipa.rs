use ark_ff::{Field, One};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::Rng;
use ark_std::{end_timer, start_timer};
use digest::Digest;
use std::{
    convert::TryInto,
    marker::PhantomData,
    ops::{Add, MulAssign},
};

use crate::{
    challenge::{
        challenge_digest, sample_bounded_challenge, ChallengeContext, ChallengeTraceSink,
        NoopChallengeTraceSink,
    },
    mul_helper, Error, InnerProductArgumentError,
};
use ark_dh_commitments::DoublyHomomorphicCommitment;
use ark_inner_products::InnerProduct;
use ark_std::cfg_iter;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

pub(crate) fn fold_output<T, S>(left: &T, current: &mut T, right: &T, c: &S, c_inv: &S)
where
    T: Clone + Default + Add<Output = T> + MulAssign<S>,
    S: Clone,
{
    {
        let current_value = std::mem::take(current);
        *current = mul_helper(left, c) + current_value + mul_helper(right, c_inv);
    }
}

pub struct GIPA<IP, LMC, RMC, IPC, D> {
    _inner_product: PhantomData<IP>,
    _left_commitment: PhantomData<LMC>,
    _right_commitment: PhantomData<RMC>,
    _inner_product_commitment: PhantomData<IPC>,
    _digest: PhantomData<D>,
}

pub(crate) struct BaseCommitmentCoreInput<KA, KB, KT, MA, MB, OA, OB, OT> {
    pub(crate) ck_a: KA,
    pub(crate) ck_b: KB,
    pub(crate) ck_t: Vec<KT>,
    pub(crate) a: MA,
    pub(crate) b: MB,
    pub(crate) com_a: OA,
    pub(crate) com_b: OB,
    pub(crate) com_t: OT,
}

pub(crate) fn verify_base_commitment_core<KA, KB, KT, MA, MB, MT, OA, OB, OT, E>(
    input: BaseCommitmentCoreInput<KA, KB, KT, MA, MB, OA, OB, OT>,
    inner_product: impl Fn(&[MA], &[MB]) -> Result<MT, E>,
    verify_left: impl Fn(&[KA], &[MA], &OA) -> Result<bool, E>,
    verify_right: impl Fn(&[KB], &[MB], &OB) -> Result<bool, E>,
    verify_target: impl Fn(&[KT], &[MT], &OT) -> Result<bool, E>,
) -> Result<bool, E> {
    let a_base = vec![input.a];
    let b_base = vec![input.b];
    let t = match inner_product(&a_base, &b_base) {
        Result::Ok(value) => value,
        Result::Err(error) => return Result::Err(error),
    };
    let mut t_base = Vec::with_capacity(1);
    t_base.push(t);
    match verify_left(&[input.ck_a], &a_base, &input.com_a) {
        Result::Err(error) => Result::Err(error),
        Result::Ok(false) => Result::Ok(false),
        Result::Ok(true) => match verify_right(&[input.ck_b], &b_base, &input.com_b) {
            Result::Err(error) => Result::Err(error),
            Result::Ok(false) => Result::Ok(false),
            Result::Ok(true) => verify_target(&input.ck_t, &t_base, &input.com_t),
        },
    }
}

// Warmed strict `1k` builder sweeps showed `64` was the only non-regressive
// rescale crossover among `{64, 128, 256, 512}` on the local machine.
const GIPA_RESCALE_PARALLEL_THRESHOLD: usize = 64;

fn rescale_fold_inner<T, S>(scaled_half: &[T], unscaled_half: &[T], scalar: &S) -> Vec<T>
where
    T: Clone + Add<Output = T> + MulAssign<S> + Send + Sync,
    S: Clone + Sync,
{
    if scaled_half.len() >= GIPA_RESCALE_PARALLEL_THRESHOLD {
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

pub(crate) fn compute_final_commitment_keys_core<F, G1, G2>(
    ck_a: &[G1],
    ck_b: &[G2],
    transcript: &[F],
    transcript_inverses: &[F],
    one: F,
    msm_a: impl FnOnce(&[G1], &[F]) -> G1,
    msm_b: impl FnOnce(&[G2], &[F]) -> G2,
) -> (G1, G2)
where
    F: Copy + std::ops::Mul<Output = F>,
    G1: Clone + Add<Output = G1> + MulAssign<F>,
    G2: Clone + Add<Output = G2> + MulAssign<F>,
{
    assert!(ck_a.len().is_power_of_two());
    assert_eq!(ck_a.len(), ck_b.len());
    assert_eq!(transcript.len(), transcript_inverses.len());

    let mut ck_a_agg_challenge_exponents = vec![one];
    let mut ck_b_agg_challenge_exponents = vec![one];
    for i in 0..transcript.len() {
        let c = transcript[i];
        let c_inv = transcript_inverses[i];
        for j in 0..(2_usize).pow(i as u32) {
            ck_a_agg_challenge_exponents.push(ck_a_agg_challenge_exponents[j] * c_inv);
            ck_b_agg_challenge_exponents.push(ck_b_agg_challenge_exponents[j] * c);
        }
    }
    assert_eq!(ck_a_agg_challenge_exponents.len(), ck_a.len());

    (
        msm_a(ck_a, &ck_a_agg_challenge_exponents),
        msm_b(ck_b, &ck_b_agg_challenge_exponents),
    )
}

fn compute_final_commitment_keys<LMC, RMC, IPC>(
    ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
    transcript: &[LMC::Scalar],
) -> Result<(LMC::Key, RMC::Key), Error>
where
    LMC: DoublyHomomorphicCommitment,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
{
    let (ck_a, ck_b, _) = ck;
    let transcript_inverses = transcript
        .iter()
        .map(|challenge| challenge.inverse().unwrap())
        .collect::<Vec<_>>();
    Ok(compute_final_commitment_keys_core(
        ck_a,
        ck_b,
        transcript,
        &transcript_inverses,
        LMC::Scalar::one(),
        LMC::msm_keys,
        RMC::msm_keys,
    ))
}

#[cfg(test)]
fn ordered_msm<K, S>(keys: &[K], scalars: &[S]) -> K
where
    K: Clone + Add<Output = K> + MulAssign<S>,
    S: Copy,
{
    let mut acc = keys[0].clone();
    acc.mul_assign(scalars[0]);
    for index in 1..keys.len() {
        let mut term = keys[index].clone();
        term.mul_assign(scalars[index]);
        acc = acc + term;
    }
    acc
}

#[derive(CanonicalSerialize, CanonicalDeserialize)]
pub struct GIPAProof<IP, LMC, RMC, IPC, D>
where
    D: Digest,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    RMC::Message: MulAssign<LMC::Scalar>,
    IPC::Message: MulAssign<LMC::Scalar>,
    RMC::Key: MulAssign<LMC::Scalar>,
    IPC::Key: MulAssign<LMC::Scalar>,
    RMC::Output: MulAssign<LMC::Scalar>,
    IPC::Output: MulAssign<LMC::Scalar>,
{
    pub(crate) r_commitment_steps: Vec<(
        (LMC::Output, RMC::Output, IPC::Output),
        (LMC::Output, RMC::Output, IPC::Output),
    )>,
    pub(crate) r_base: (LMC::Message, RMC::Message),
    // The fn() is here because PhantomData<T> is Sync iff T is Sync, and these types are not all
    // Sync
    pub(crate) _gipa: PhantomData<fn() -> GIPA<IP, LMC, RMC, IPC, D>>,
}

#[derive(Clone)]
pub struct GIPAAux<IP, LMC, RMC, IPC, D>
where
    D: Digest,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    RMC::Message: MulAssign<LMC::Scalar>,
    IPC::Message: MulAssign<LMC::Scalar>,
    RMC::Key: MulAssign<LMC::Scalar>,
    IPC::Key: MulAssign<LMC::Scalar>,
    RMC::Output: MulAssign<LMC::Scalar>,
    IPC::Output: MulAssign<LMC::Scalar>,
{
    pub(crate) r_transcript: Vec<LMC::Scalar>,
    pub(crate) ck_base: (LMC::Key, RMC::Key),
    pub(crate) _gipa: PhantomData<GIPA<IP, LMC, RMC, IPC, D>>,
}

impl<IP, LMC, RMC, IPC, D> GIPA<IP, LMC, RMC, IPC, D>
where
    D: Digest,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    RMC::Message: MulAssign<LMC::Scalar>,
    IPC::Message: MulAssign<LMC::Scalar>,
    RMC::Key: MulAssign<LMC::Scalar>,
    IPC::Key: MulAssign<LMC::Scalar>,
    RMC::Output: MulAssign<LMC::Scalar>,
    IPC::Output: MulAssign<LMC::Scalar>,
{
    fn rescale_fold<T>(scaled_half: &[T], unscaled_half: &[T], scalar: &LMC::Scalar) -> Vec<T>
    where
        T: Clone + Add<Output = T> + MulAssign<LMC::Scalar> + Send + Sync,
    {
        rescale_fold_inner(scaled_half, unscaled_half, scalar)
    }

    fn commit_round(
        ck_a: &[LMC::Key],
        m_a: &[IP::LeftMessage],
        ck_b: &[RMC::Key],
        m_b: &[IP::RightMessage],
        ck_t: &[IPC::Key],
    ) -> Result<(LMC::Output, RMC::Output, IPC::Output), String> {
        let com_a = LMC::commit(ck_a, m_a).map_err(|err| err.to_string())?;
        let com_b = RMC::commit(ck_b, m_b).map_err(|err| err.to_string())?;
        let inner_product = IP::inner_product(m_a, m_b).map_err(|err| err.to_string())?;
        let inner_products = vec![inner_product];
        let commitment = (
            com_a,
            com_b,
            IPC::commit(ck_t, &inner_products).map_err(|err| err.to_string())?,
        );
        Ok(commitment)
    }

    pub fn setup<R: Rng>(
        rng: &mut R,
        size: usize,
    ) -> Result<(Vec<LMC::Key>, Vec<RMC::Key>, IPC::Key), Error> {
        Ok((
            LMC::setup(rng, size)?,
            RMC::setup(rng, size)?,
            IPC::setup(rng, 1)?.pop().unwrap(),
        ))
    }

    pub fn prove(
        context: &ChallengeContext,
        values: (&[IP::LeftMessage], &[IP::RightMessage], &IP::Output),
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
    ) -> Result<GIPAProof<IP, LMC, RMC, IPC, D>, Error>
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        if IP::inner_product(values.0, values.1)? != values.2.clone() {
            return Err(Box::new(InnerProductArgumentError::InnerProductInvalid));
        }
        if values.0.len().count_ones() != 1 {
            // Power of 2 length
            return Err(Box::new(InnerProductArgumentError::MessageLengthInvalid(
                values.0.len(),
                values.1.len(),
            )));
        }
        if !(LMC::verify(ck.0, values.0, com.0)?
            && RMC::verify(ck.1, values.1, com.1)?
            && IPC::verify(&vec![ck.2.clone()], &vec![values.2.clone()], com.2)?)
        {
            return Err(Box::new(InnerProductArgumentError::InnerProductInvalid));
        }

        let (proof, _) = Self::prove_with_aux(
            context,
            (values.0, values.1),
            (ck.0, ck.1, &vec![ck.2.clone()]),
        )?;
        Ok(proof)
    }

    pub fn verify(
        context: &ChallengeContext,
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<bool, Error> {
        if ck.0.len().count_ones() != 1 || ck.0.len() != ck.1.len() {
            // Power of 2 length
            return Err(Box::new(InnerProductArgumentError::MessageLengthInvalid(
                ck.0.len(),
                ck.1.len(),
            )));
        }
        // Calculate base commitment and transcript
        let mut trace = NoopChallengeTraceSink;
        let (base_com, transcript) = Self::_compute_recursive_challenges(
            context,
            &mut trace,
            b"tipa.generic.gipa.round",
            (com.0.clone(), com.1.clone(), com.2.clone()),
            proof,
        )?;
        // Calculate base commitment keys
        let (ck_a_base, ck_b_base) = Self::_compute_final_commitment_keys(ck, &transcript)?;
        // Verify base commitment
        Self::_verify_base_commitment(
            (&ck_a_base, &ck_b_base, &vec![ck.2.clone()]),
            base_com,
            proof,
        )
    }

    pub fn prove_with_aux(
        context: &ChallengeContext,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &[IPC::Key]),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
        ),
        Error,
    >
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        let (m_a, m_b) = values;
        let (ck_a, ck_b, ck_t) = ck;
        let mut trace = NoopChallengeTraceSink;
        let (proof, aux) = Self::_prove(
            context,
            &mut trace,
            b"tipa.generic.gipa.round",
            (m_a.to_vec(), m_b.to_vec()),
            (ck_a.to_vec(), ck_b.to_vec(), ck_t.to_vec()),
        )?;
        Ok((proof, aux))
    }

    pub fn prove_with_aux_with_stage(
        context: &ChallengeContext,
        stage_label: &'static [u8],
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &[IPC::Key]),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
        ),
        Error,
    >
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        let mut trace = NoopChallengeTraceSink;
        Self::prove_with_aux_with_stage_with_trace(context, &mut trace, stage_label, values, ck)
    }

    pub fn prove_with_aux_with_stage_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        stage_label: &'static [u8],
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &[IPC::Key]),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
        ),
        Error,
    >
    where
        S: ChallengeTraceSink,
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        let (m_a, m_b) = values;
        let (ck_a, ck_b, ck_t) = ck;
        Self::_prove(
            context,
            trace,
            stage_label,
            (m_a.to_vec(), m_b.to_vec()),
            (ck_a.to_vec(), ck_b.to_vec(), ck_t.to_vec()),
        )
    }

    // Returns vector of recursive commitments and transcripts in reverse order
    fn _prove(
        context: &ChallengeContext,
        trace: &mut impl ChallengeTraceSink,
        stage_label: &'static [u8],
        values: (Vec<IP::LeftMessage>, Vec<IP::RightMessage>),
        ck: (Vec<LMC::Key>, Vec<RMC::Key>, Vec<IPC::Key>),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
        ),
        Error,
    >
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        let (mut m_a, mut m_b) = values;
        let (mut ck_a, mut ck_b, ck_t) = ck;
        let mut r_commitment_steps = Vec::new();
        let mut r_transcript: Vec<LMC::Scalar> = Vec::new();

        assert!(m_a.len().is_power_of_two());
        let (m_base, ck_base) = 'recurse: loop {
            let recurse = start_timer!(|| format!("Recurse round size {}", m_a.len()));
            if m_a.len() == 1 {
                // base case
                break 'recurse (
                    (m_a[0].clone(), m_b[0].clone()),
                    (ck_a[0].clone(), ck_b[0].clone()),
                );
            } else {
                // recursive step
                // Recurse with problem of half size
                let split = m_a.len() / 2;

                let m_a_1 = &m_a[split..];
                let m_a_2 = &m_a[..split];
                let ck_a_1 = &ck_a[..split];
                let ck_a_2 = &ck_a[split..];

                let m_b_1 = &m_b[..split];
                let m_b_2 = &m_b[split..];
                let ck_b_1 = &ck_b[split..];
                let ck_b_2 = &ck_b[..split];

                #[cfg(feature = "parallel")]
                let (com_1_result, com_2_result) = rayon::join(
                    || Self::commit_round(ck_a_1, m_a_1, ck_b_1, m_b_1, &ck_t),
                    || Self::commit_round(ck_a_2, m_a_2, ck_b_2, m_b_2, &ck_t),
                );

                #[cfg(not(feature = "parallel"))]
                let (com_1_result, com_2_result) = (
                    Self::commit_round(ck_a_1, m_a_1, ck_b_1, m_b_1, &ck_t),
                    Self::commit_round(ck_a_2, m_a_2, ck_b_2, m_b_2, &ck_t),
                );

                let com_1 = com_1_result.map_err(|err: String| std::io::Error::other(err))?;
                let com_2 = com_2_result.map_err(|err: String| std::io::Error::other(err))?;
                // Per-task self-times: under the parallel seam these overlap in
                // wall-clock, so the two fields are not additive (see field docs).

                // Fiat-Shamir challenge

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
                    let c: LMC::Scalar = u128::from_be_bytes(
                        challenge_digest::<D, _>(context, trace, stage_label, nonce, &hash_input)
                            .as_slice()[0..16]
                            .try_into()
                            .unwrap(),
                    )
                    .into();
                    // Optimization for multiexponentiation to rescale G2 elements with 128-bit challenge
                    // Swap 'c' and 'c_inv' since can't control bit size of c_inv
                    Ok(c.inverse().map(|c_inv| (c_inv, c)))
                })?;

                // Set up values for next step of recursion
                #[cfg(feature = "parallel")]
                let (next_m_a, next_m_b, next_ck_a, next_ck_b) = {
                    let ((next_m_a, next_m_b), (next_ck_a, next_ck_b)) = rayon::join(
                        || {
                            rayon::join(
                                || Self::rescale_fold(m_a_1, m_a_2, &c),
                                || Self::rescale_fold(m_b_2, m_b_1, &c_inv),
                            )
                        },
                        || {
                            rayon::join(
                                || Self::rescale_fold(ck_a_2, ck_a_1, &c_inv),
                                || Self::rescale_fold(ck_b_1, ck_b_2, &c),
                            )
                        },
                    );
                    (next_m_a, next_m_b, next_ck_a, next_ck_b)
                };

                #[cfg(not(feature = "parallel"))]
                let (next_m_a, next_m_b, next_ck_a, next_ck_b) = (
                    Self::rescale_fold(m_a_1, m_a_2, &c),
                    Self::rescale_fold(m_b_2, m_b_1, &c_inv),
                    Self::rescale_fold(ck_a_2, ck_a_1, &c_inv),
                    Self::rescale_fold(ck_b_1, ck_b_2, &c),
                );

                m_a = next_m_a;
                m_b = next_m_b;
                ck_a = next_ck_a;
                ck_b = next_ck_b;

                r_commitment_steps.push((com_1, com_2));
                r_transcript.push(c);
                end_timer!(recurse);
            }
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

    pub fn verify_recursive_challenge_transcript(
        context: &ChallengeContext,
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<((LMC::Output, RMC::Output, IPC::Output), Vec<LMC::Scalar>), Error> {
        Self::verify_recursive_challenge_transcript_with_stage(
            context,
            b"tipa.generic.gipa.round",
            com,
            proof,
        )
    }

    pub fn verify_recursive_challenge_transcript_with_stage(
        context: &ChallengeContext,
        stage_label: &'static [u8],
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<((LMC::Output, RMC::Output, IPC::Output), Vec<LMC::Scalar>), Error> {
        let mut trace = NoopChallengeTraceSink;
        Self::verify_recursive_challenge_transcript_with_stage_with_trace(
            context,
            &mut trace,
            stage_label,
            com,
            proof,
        )
    }

    pub fn verify_recursive_challenge_transcript_with_stage_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        stage_label: &'static [u8],
        com: (&LMC::Output, &RMC::Output, &IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<((LMC::Output, RMC::Output, IPC::Output), Vec<LMC::Scalar>), Error>
    where
        S: ChallengeTraceSink,
    {
        Self::_compute_recursive_challenges(
            context,
            trace,
            stage_label,
            (com.0.clone(), com.1.clone(), com.2.clone()),
            proof,
        )
    }

    fn _compute_recursive_challenges(
        context: &ChallengeContext,
        trace: &mut impl ChallengeTraceSink,
        stage_label: &'static [u8],
        com: (LMC::Output, RMC::Output, IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<((LMC::Output, RMC::Output, IPC::Output), Vec<LMC::Scalar>), Error> {
        let (mut com_a, mut com_b, mut com_t) = com;
        let mut r_transcript: Vec<LMC::Scalar> = Vec::new();
        for (com_1, com_2) in proof.r_commitment_steps.iter().rev() {
            // Fiat-Shamir challenge
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
                let c: LMC::Scalar = u128::from_be_bytes(
                    challenge_digest::<D, _>(context, trace, stage_label, nonce, &hash_input)
                        .as_slice()[0..16]
                        .try_into()
                        .unwrap(),
                )
                .into();
                // Optimization for multiexponentiation to rescale G2 elements with 128-bit challenge
                // Swap 'c' and 'c_inv' since can't control bit size of c_inv
                Ok(c.inverse().map(|c_inv| (c_inv, c)))
            })?;

            fold_output(&com_1.0, &mut com_a, &com_2.0, &c, &c_inv);
            fold_output(&com_1.1, &mut com_b, &com_2.1, &c, &c_inv);
            fold_output(&com_1.2, &mut com_t, &com_2.2, &c, &c_inv);

            r_transcript.push(c);
        }
        r_transcript.reverse();
        Ok(((com_a, com_b, com_t), r_transcript))
    }

    pub(crate) fn _compute_final_commitment_keys(
        ck: (&[LMC::Key], &[RMC::Key], &IPC::Key),
        transcript: &Vec<LMC::Scalar>,
    ) -> Result<(LMC::Key, RMC::Key), Error> {
        compute_final_commitment_keys::<LMC, RMC, IPC>(ck, transcript)
    }

    pub(crate) fn _verify_base_commitment(
        base_ck: (&LMC::Key, &RMC::Key, &Vec<IPC::Key>),
        base_com: (LMC::Output, RMC::Output, IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<bool, Error> {
        let (com_a, com_b, com_t) = base_com;
        let (ck_a_base, ck_b_base, ck_t) = base_ck;
        let input = BaseCommitmentCoreInput {
            ck_a: ck_a_base.clone(),
            ck_b: ck_b_base.clone(),
            ck_t: ck_t.clone(),
            a: proof.r_base.0.clone(),
            b: proof.r_base.1.clone(),
            com_a,
            com_b,
            com_t,
        };
        verify_base_commitment_core(
            input,
            IP::inner_product,
            LMC::verify,
            RMC::verify,
            IPC::verify,
        )
    }
}

impl<IP, LMC, RMC, IPC, D> Clone for GIPAProof<IP, LMC, RMC, IPC, D>
where
    D: Digest,
    IP: InnerProduct<
        LeftMessage = LMC::Message,
        RightMessage = RMC::Message,
        Output = IPC::Message,
    >,
    LMC: DoublyHomomorphicCommitment,
    RMC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    IPC: DoublyHomomorphicCommitment<Scalar = LMC::Scalar>,
    RMC::Message: MulAssign<LMC::Scalar>,
    IPC::Message: MulAssign<LMC::Scalar>,
    RMC::Key: MulAssign<LMC::Scalar>,
    IPC::Key: MulAssign<LMC::Scalar>,
    RMC::Output: MulAssign<LMC::Scalar>,
    IPC::Output: MulAssign<LMC::Scalar>,
{
    fn clone(&self) -> Self {
        GIPAProof {
            r_commitment_steps: self.r_commitment_steps.clone(),
            r_base: self.r_base.clone(),
            _gipa: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Bls12_381;
    use ark_ec::pairing::{Pairing, PairingOutput};
    use ark_ff::UniformRand;
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use blake2::Blake2b;
    use std::cell::RefCell;

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
    fn fold_output_matches_direct_expression() {
        type Scalar = <Bls12_381 as Pairing>::ScalarField;

        let left = Scalar::from(2u64);
        let mut current = Scalar::from(3u64);
        let right = Scalar::from(5u64);
        let c = Scalar::from(7u64);
        let c_inv = Scalar::from(11u64);
        let expected = left * c + current + right * c_inv;

        fold_output(&left, &mut current, &right, &c, &c_inv);

        assert_eq!(current, expected);
    }

    #[test]
    fn rescale_fold_matches_elementwise_expression() {
        type Scalar = <Bls12_381 as Pairing>::ScalarField;

        let scaled = [Scalar::from(2u64), Scalar::from(3u64)];
        let unscaled = [Scalar::from(5u64), Scalar::from(7u64)];
        let scalar = Scalar::from(11u64);

        assert_eq!(
            rescale_fold_inner(&scaled, &unscaled, &scalar),
            vec![
                scaled[0] * scalar + unscaled[0],
                scaled[1] * scalar + unscaled[1],
            ]
        );
    }

    #[test]
    fn pairing_inner_product_test() {
        type IP = PairingInnerProduct<Bls12_381>;
        type IPC =
            IdentityCommitment<PairingOutput<Bls12_381>, <Bls12_381 as Pairing>::ScalarField>;
        type PairingGIPA = GIPA<IP, GC1, GC2, IPC, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
        let (ck_a, ck_b, ck_t) = PairingGIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let m_a = random_generators(&mut rng, TEST_SIZE);
        let m_b = random_generators(&mut rng, TEST_SIZE);
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = GC2::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = PairingGIPA::prove(
            &challenge_context,
            (&m_a, &m_b, &t[0]),
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
        )
        .unwrap();

        assert!(PairingGIPA::verify(
            &challenge_context,
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
            &proof,
        )
        .unwrap());
    }

    #[test]
    fn multiexponentiation_inner_product_test() {
        type IP = MultiexponentiationInnerProduct<<Bls12_381 as Pairing>::G1>;
        type IPC =
            IdentityCommitment<<Bls12_381 as Pairing>::G1, <Bls12_381 as Pairing>::ScalarField>;
        type MultiExpGIPA = GIPA<IP, GC1, SC1, IPC, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
        let (ck_a, ck_b, ck_t) = MultiExpGIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let m_a = random_generators(&mut rng, TEST_SIZE);
        let mut m_b = Vec::new();
        for _ in 0..TEST_SIZE {
            m_b.push(<Bls12_381 as Pairing>::ScalarField::rand(&mut rng));
        }
        let com_a = GC1::commit(&ck_a, &m_a).unwrap();
        let com_b = SC1::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = MultiExpGIPA::prove(
            &challenge_context,
            (&m_a, &m_b, &t[0]),
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
        )
        .unwrap();

        assert!(MultiExpGIPA::verify(
            &challenge_context,
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
            &proof,
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
        type ScalarGIPA = GIPA<IP, SC2, SC2, IPC, Blake2b>;

        let mut rng = StdRng::seed_from_u64(0u64);
        let challenge_context = ChallengeContext::from_statement_digest([0u8; 32]);
        let (ck_a, ck_b, ck_t) = ScalarGIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let mut m_a = Vec::new();
        let mut m_b = Vec::new();
        for _ in 0..TEST_SIZE {
            m_a.push(<Bls12_381 as Pairing>::ScalarField::rand(&mut rng));
            m_b.push(<Bls12_381 as Pairing>::ScalarField::rand(&mut rng));
        }
        let com_a = SC2::commit(&ck_a, &m_a).unwrap();
        let com_b = SC2::commit(&ck_b, &m_b).unwrap();
        let t = vec![IP::inner_product(&m_a, &m_b).unwrap()];
        let com_t = IPC::commit(&vec![ck_t.clone()], &t).unwrap();

        let proof = ScalarGIPA::prove(
            &challenge_context,
            (&m_a, &m_b, &t[0]),
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
        )
        .unwrap();

        let (base_com, transcript) = ScalarGIPA::verify_recursive_challenge_transcript(
            &challenge_context,
            (&com_a, &com_b, &com_t),
            &proof,
        )
        .unwrap();
        let (ck_a_base, ck_b_base) =
            ScalarGIPA::_compute_final_commitment_keys((&ck_a, &ck_b, &ck_t), &transcript).unwrap();
        let delegated = ScalarGIPA::_verify_base_commitment(
            (&ck_a_base, &ck_b_base, &vec![ck_t.clone()]),
            base_com.clone(),
            &proof,
        );
        let core = verify_base_commitment_core(
            BaseCommitmentCoreInput {
                ck_a: ck_a_base,
                ck_b: ck_b_base,
                ck_t: vec![ck_t.clone()],
                a: proof.r_base.0.clone(),
                b: proof.r_base.1.clone(),
                com_a: base_com.0,
                com_b: base_com.1,
                com_t: base_com.2,
            },
            IP::inner_product,
            SC2::verify,
            SC2::verify,
            IPC::verify,
        );
        assert_eq!(delegated.unwrap(), core.unwrap());

        assert!(ScalarGIPA::verify(
            &challenge_context,
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
            &proof,
        )
        .unwrap());
    }

    struct BaseCheckRecorder {
        fail_at: Option<u8>,
        false_at: Option<u8>,
        calls: RefCell<Vec<u8>>,
    }

    impl BaseCheckRecorder {
        fn step<T>(&self, stage: u8, value: T) -> Result<T, u8> {
            self.calls.borrow_mut().push(stage);
            if self.fail_at == Some(stage) {
                Err(stage)
            } else {
                Ok(value)
            }
        }
    }

    fn scripted_base_input() -> BaseCommitmentCoreInput<u8, u8, u8, u8, u8, u8, u8, u8> {
        BaseCommitmentCoreInput {
            ck_a: 1,
            ck_b: 2,
            ck_t: vec![3],
            a: 4,
            b: 5,
            com_a: 6,
            com_b: 7,
            com_t: 8,
        }
    }

    #[test]
    fn base_commitment_core_preserves_failures_and_short_circuit_order() {
        for stage in 0..4 {
            let effect = BaseCheckRecorder {
                fail_at: Some(stage),
                false_at: None,
                calls: RefCell::new(Vec::new()),
            };
            assert_eq!(
                verify_base_commitment_core(
                    scripted_base_input(),
                    |_, _| effect.step(0, 9u8),
                    |_, _, _| effect.step(1, effect.false_at != Some(1)),
                    |_, _, _| effect.step(2, effect.false_at != Some(2)),
                    |_, _, _| effect.step(3, effect.false_at != Some(3)),
                ),
                Err(stage)
            );
            assert_eq!(*effect.calls.borrow(), (0..=stage).collect::<Vec<_>>());
        }

        for stage in 1..4 {
            let effect = BaseCheckRecorder {
                fail_at: None,
                false_at: Some(stage),
                calls: RefCell::new(Vec::new()),
            };
            assert_eq!(
                verify_base_commitment_core(
                    scripted_base_input(),
                    |_, _| effect.step(0, 9u8),
                    |_, _, _| effect.step(1, effect.false_at != Some(1)),
                    |_, _, _| effect.step(2, effect.false_at != Some(2)),
                    |_, _, _| effect.step(3, effect.false_at != Some(3)),
                ),
                Ok(false)
            );
            assert_eq!(*effect.calls.borrow(), (0..=stage).collect::<Vec<_>>());
        }
    }

    #[test]
    fn final_commitment_key_delegator_matches_core_and_transcript_orientation() {
        type F = <Bls12_381 as Pairing>::ScalarField;
        type IPC = IdentityCommitment<F, F>;
        type ScalarGIPA = GIPA<ScalarInnerProduct<F>, SC2, SC2, IPC, Blake2b>;

        let mut rng = StdRng::seed_from_u64(24u64);
        let (ck_a, ck_b, ck_t) = ScalarGIPA::setup(&mut rng, TEST_SIZE).unwrap();
        let transcript = vec![F::from(2u64), F::from(3u64), F::from(5u64)];

        let delegated =
            compute_final_commitment_keys::<SC2, SC2, IPC>((&ck_a, &ck_b, &ck_t), &transcript)
                .unwrap();
        let core = compute_final_commitment_keys_core(
            &ck_a,
            &ck_b,
            &transcript,
            &transcript
                .iter()
                .map(|challenge| challenge.inverse().unwrap())
                .collect::<Vec<_>>(),
            F::from(1u64),
            ordered_msm::<_, F>,
            ordered_msm::<_, F>,
        );

        let two_inv = transcript[0].inverse().unwrap();
        let three_inv = transcript[1].inverse().unwrap();
        let five_inv = transcript[2].inverse().unwrap();
        let inverse_coefficients = vec![
            F::from(1u64),
            two_inv,
            three_inv,
            two_inv * three_inv,
            five_inv,
            two_inv * five_inv,
            three_inv * five_inv,
            two_inv * three_inv * five_inv,
        ];
        let raw_coefficients = vec![
            F::from(1u64),
            transcript[0],
            transcript[1],
            transcript[0] * transcript[1],
            transcript[2],
            transcript[0] * transcript[2],
            transcript[1] * transcript[2],
            transcript[0] * transcript[1] * transcript[2],
        ];

        assert_eq!(delegated, core);
        assert_eq!(core.0, ordered_msm(&ck_a, &inverse_coefficients));
        assert_eq!(core.1, ordered_msm(&ck_b, &raw_coefficients));
    }
}
