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
    challenge::{challenge_digest, ChallengeContext, ChallengeTraceSink, NoopChallengeTraceSink},
    mul_helper, Error, InnerProductArgumentError,
};
use ark_dh_commitments::DoublyHomomorphicCommitment;
use ark_inner_products::InnerProduct;
use ark_std::cfg_iter;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Pre-optimization sequential multiexponentiation `Σ scalarsᵢ · keysᵢ`,
/// retained as the `bench-baseline` A/B comparison for the `msm_keys` path in
/// `_compute_final_commitment_keys`. Byte-identical to `msm_keys`; selected only
/// under the bench-only `bench-baseline` feature, never in production.
#[cfg(feature = "bench-baseline")]
fn fold_keys_baseline<K, S>(keys: &[K], scalars: &[S]) -> K
where
    K: Clone + Add<Output = K> + MulAssign<S>,
    S: Copy,
{
    assert!(
        !keys.is_empty(),
        "fold_keys_baseline requires non-empty keys"
    );
    assert_eq!(keys.len(), scalars.len());
    let mut acc = keys[0].clone();
    acc *= scalars[0];
    for (k, s) in keys[1..].iter().zip(&scalars[1..]) {
        let mut term = k.clone();
        term *= *s;
        acc = acc + term;
    }
    acc
}

#[cfg(feature = "bench-baseline")]
fn fold_output_baseline<T, S>(left: &T, current: &T, right: &T, c: &S, c_inv: &S) -> T
where
    T: Clone + Add<Output = T> + MulAssign<S>,
    S: Clone,
{
    mul_helper(left, c) + current.clone() + mul_helper(right, c_inv)
}

fn fold_output<T, S>(left: &T, current: &mut T, right: &T, c: &S, c_inv: &S)
where
    T: Clone + Default + Add<Output = T> + MulAssign<S>,
    S: Clone,
{
    #[cfg(feature = "bench-baseline")]
    {
        *current = fold_output_baseline(left, current, right, c, c_inv);
    }

    #[cfg(not(feature = "bench-baseline"))]
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

// Warmed strict `1k` builder sweeps showed `64` was the only non-regressive
// rescale crossover among `{64, 128, 256, 512}` on the local machine.
const GIPA_RESCALE_PARALLEL_THRESHOLD: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct GipaBuildProfile {
    pub total_ms: f64,
    /// Per-task self-time for the left/right round commitments. Under the
    /// parallel seam these two run concurrently via `rayon::join`, so their
    /// wall-clock windows overlap — they are not additive and their sum can
    /// exceed `total_ms`. Read each as its own work, not as a serial subtotal.
    pub commit_l_ms: f64,
    pub commit_r_ms: f64,
    pub commit_ab_ms: f64,
    pub commit_com_a_ms: f64,
    pub commit_com_b_ms: f64,
    pub challenge_ms: f64,
    pub rescale_m1_ms: f64,
    pub rescale_m2_ms: f64,
    pub rescale_ck1_ms: f64,
    pub rescale_ck2_ms: f64,
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

//TODO: Can extend GIPA to support "identity commitments" in addition to "compact commitments", i.e. for SIPP

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
    #[inline]
    fn use_parallel_rescale(len: usize) -> bool {
        #[cfg(feature = "parallel")]
        {
            len >= GIPA_RESCALE_PARALLEL_THRESHOLD
        }
        #[cfg(not(feature = "parallel"))]
        {
            let _ = len;
            false
        }
    }

    fn rescale_fold<T>(scaled_half: &[T], unscaled_half: &[T], scalar: &LMC::Scalar) -> Vec<T>
    where
        T: Clone + Add<Output = T> + MulAssign<LMC::Scalar> + Send + Sync,
    {
        if Self::use_parallel_rescale(scaled_half.len()) {
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

    fn rescale_fold_profiled<T>(
        scaled_half: &[T],
        unscaled_half: &[T],
        scalar: &LMC::Scalar,
    ) -> (Vec<T>, f64)
    where
        T: Clone + Add<Output = T> + MulAssign<LMC::Scalar> + Send + Sync,
    {
        let started = std::time::Instant::now();
        let folded = Self::rescale_fold(scaled_half, unscaled_half, scalar);
        (folded, started.elapsed().as_secs_f64() * 1000.0)
    }

    fn commit_round_profiled(
        ck_a: &[LMC::Key],
        m_a: &[IP::LeftMessage],
        ck_b: &[RMC::Key],
        m_b: &[IP::RightMessage],
        ck_t: &[IPC::Key],
    ) -> Result<((LMC::Output, RMC::Output, IPC::Output), f64), String> {
        let started = std::time::Instant::now();
        let com_a = LMC::commit(ck_a, m_a).map_err(|err| err.to_string())?;
        let com_b = RMC::commit(ck_b, m_b).map_err(|err| err.to_string())?;
        let inner_product = IP::inner_product(m_a, m_b).map_err(|err| err.to_string())?;
        let inner_products = vec![inner_product];
        let commitment = (
            com_a,
            com_b,
            IPC::commit(ck_t, &inner_products).map_err(|err| err.to_string())?,
        );
        Ok((commitment, started.elapsed().as_secs_f64() * 1000.0))
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
        let (proof, aux, _) = Self::_prove_profiled(
            context,
            &mut trace,
            b"tipa.generic.gipa.round",
            (m_a.to_vec(), m_b.to_vec()),
            (ck_a.to_vec(), ck_b.to_vec(), ck_t.to_vec()),
        )?;
        Ok((proof, aux))
    }

    pub fn prove_with_aux_profiled(
        context: &ChallengeContext,
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &[IPC::Key]),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
            GipaBuildProfile,
        ),
        Error,
    >
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        Self::prove_with_aux_profiled_with_stage(context, b"tipa.generic.gipa.round", values, ck)
    }

    pub fn prove_with_aux_profiled_with_stage(
        context: &ChallengeContext,
        stage_label: &'static [u8],
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &[IPC::Key]),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
            GipaBuildProfile,
        ),
        Error,
    >
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        let mut trace = NoopChallengeTraceSink;
        Self::prove_with_aux_profiled_with_stage_with_trace(
            context,
            &mut trace,
            stage_label,
            values,
            ck,
        )
    }

    pub fn prove_with_aux_profiled_with_stage_with_trace<S>(
        context: &ChallengeContext,
        trace: &mut S,
        stage_label: &'static [u8],
        values: (&[IP::LeftMessage], &[IP::RightMessage]),
        ck: (&[LMC::Key], &[RMC::Key], &[IPC::Key]),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
            GipaBuildProfile,
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
        Self::_prove_profiled(
            context,
            trace,
            stage_label,
            (m_a.to_vec(), m_b.to_vec()),
            (ck_a.to_vec(), ck_b.to_vec(), ck_t.to_vec()),
        )
    }

    // Returns vector of recursive commitments and transcripts in reverse order
    fn _prove_profiled(
        context: &ChallengeContext,
        trace: &mut impl ChallengeTraceSink,
        stage_label: &'static [u8],
        values: (Vec<IP::LeftMessage>, Vec<IP::RightMessage>),
        ck: (Vec<LMC::Key>, Vec<RMC::Key>, Vec<IPC::Key>),
    ) -> Result<
        (
            GIPAProof<IP, LMC, RMC, IPC, D>,
            GIPAAux<IP, LMC, RMC, IPC, D>,
            GipaBuildProfile,
        ),
        Error,
    >
    where
        LMC::Output: Send,
        RMC::Output: Send,
        IPC::Output: Send,
    {
        let total_started = std::time::Instant::now();
        let (mut m_a, mut m_b) = values;
        let (mut ck_a, mut ck_b, ck_t) = ck;
        let mut r_commitment_steps = Vec::new();
        let mut r_transcript: Vec<LMC::Scalar> = Vec::new();
        let mut profile = GipaBuildProfile::default();
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

                #[cfg(all(feature = "parallel", not(feature = "bench-baseline")))]
                let (com_1_result, com_2_result) = rayon::join(
                    || Self::commit_round_profiled(ck_a_1, m_a_1, ck_b_1, m_b_1, &ck_t),
                    || Self::commit_round_profiled(ck_a_2, m_a_2, ck_b_2, m_b_2, &ck_t),
                );

                #[cfg(any(not(feature = "parallel"), feature = "bench-baseline"))]
                let (com_1_result, com_2_result) = (
                    Self::commit_round_profiled(ck_a_1, m_a_1, ck_b_1, m_b_1, &ck_t),
                    Self::commit_round_profiled(ck_a_2, m_a_2, ck_b_2, m_b_2, &ck_t),
                );

                let (com_1, commit_l_ms) =
                    com_1_result.map_err(|err: String| std::io::Error::other(err))?;
                let (com_2, commit_r_ms) =
                    com_2_result.map_err(|err: String| std::io::Error::other(err))?;
                // Per-task self-times: under the parallel seam these overlap in
                // wall-clock, so the two fields are not additive (see field docs).
                profile.commit_l_ms += commit_l_ms;
                profile.commit_r_ms += commit_r_ms;

                // Fiat-Shamir challenge
                let challenge_started = std::time::Instant::now();
                let mut counter_nonce: u64 = 0;
                let default_transcript = Default::default();
                let transcript = r_transcript.last().unwrap_or(&default_transcript);
                let (c, c_inv) = 'challenge: loop {
                    let mut hash_input = Vec::new();
                    transcript.serialize_uncompressed(&mut hash_input)?;
                    com_1.0.serialize_uncompressed(&mut hash_input)?;
                    com_1.1.serialize_uncompressed(&mut hash_input)?;
                    com_1.2.serialize_uncompressed(&mut hash_input)?;
                    com_2.0.serialize_uncompressed(&mut hash_input)?;
                    com_2.1.serialize_uncompressed(&mut hash_input)?;
                    com_2.2.serialize_uncompressed(&mut hash_input)?;
                    let c: LMC::Scalar = u128::from_be_bytes(
                        challenge_digest::<D, _>(
                            context,
                            trace,
                            stage_label,
                            counter_nonce,
                            &hash_input,
                        )
                        .as_slice()[0..16]
                            .try_into()
                            .unwrap(),
                    )
                    .into();
                    if let Some(c_inv) = c.inverse() {
                        // Optimization for multiexponentiation to rescale G2 elements with 128-bit challenge
                        // Swap 'c' and 'c_inv' since can't control bit size of c_inv
                        break 'challenge (c_inv, c);
                    }
                    counter_nonce += 1;
                };
                profile.challenge_ms += challenge_started.elapsed().as_secs_f64() * 1000.0;

                // Set up values for next step of recursion
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
                                || Self::rescale_fold_profiled(m_a_1, m_a_2, &c),
                                || Self::rescale_fold_profiled(m_b_2, m_b_1, &c_inv),
                            )
                        },
                        || {
                            rayon::join(
                                || Self::rescale_fold_profiled(ck_a_2, ck_a_1, &c_inv),
                                || Self::rescale_fold_profiled(ck_b_1, ck_b_2, &c),
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
                    Self::rescale_fold_profiled(m_a_1, m_a_2, &c),
                    Self::rescale_fold_profiled(m_b_2, m_b_1, &c_inv),
                    Self::rescale_fold_profiled(ck_a_2, ck_a_1, &c_inv),
                    Self::rescale_fold_profiled(ck_b_1, ck_b_2, &c),
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
                end_timer!(recurse);
            }
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

    // Helper function used to calculate recursive challenges from proof execution (transcript in reverse)
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
            let mut counter_nonce: u64 = 0;
            let default_transcript = Default::default();
            let transcript = r_transcript.last().unwrap_or(&default_transcript);
            let (c, c_inv) = 'challenge: loop {
                let mut hash_input = Vec::new();
                transcript.serialize_uncompressed(&mut hash_input)?;
                com_1.0.serialize_uncompressed(&mut hash_input)?;
                com_1.1.serialize_uncompressed(&mut hash_input)?;
                com_1.2.serialize_uncompressed(&mut hash_input)?;
                com_2.0.serialize_uncompressed(&mut hash_input)?;
                com_2.1.serialize_uncompressed(&mut hash_input)?;
                com_2.2.serialize_uncompressed(&mut hash_input)?;
                let c: LMC::Scalar = u128::from_be_bytes(
                    challenge_digest::<D, _>(
                        context,
                        trace,
                        stage_label,
                        counter_nonce,
                        &hash_input,
                    )
                    .as_slice()[0..16]
                        .try_into()
                        .unwrap(),
                )
                .into();
                if let Some(c_inv) = c.inverse() {
                    // Optimization for multiexponentiation to rescale G2 elements with 128-bit challenge
                    // Swap 'c' and 'c_inv' since can't control bit size of c_inv
                    break 'challenge (c_inv, c);
                }
                counter_nonce += 1;
            };

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
        // Calculate base commitment keys
        let (ck_a, ck_b, _) = ck;
        assert!(ck_a.len().is_power_of_two());

        let mut ck_a_agg_challenge_exponents = vec![LMC::Scalar::one()];
        let mut ck_b_agg_challenge_exponents = vec![LMC::Scalar::one()];
        for (i, c) in transcript.iter().enumerate() {
            let c_inv = c.inverse().unwrap();
            for j in 0..(2_usize).pow(i as u32) {
                ck_a_agg_challenge_exponents.push(ck_a_agg_challenge_exponents[j] * &c_inv);
                ck_b_agg_challenge_exponents.push(ck_b_agg_challenge_exponents[j] * c);
            }
        }
        assert_eq!(ck_a_agg_challenge_exponents.len(), ck_a.len());
        // Recombine the final commitment keys by multiexponentiation. The
        // commitment trait's `msm_keys` is byte-identical to the prior
        // sequential fold; group-backed keys (AFGHO) use a real MSM.
        //
        // The `bench-baseline` feature swaps in the pre-optimization sequential
        // fold so the A/B harness can measure the MSM delta on the real verify
        // path in the same release build. See the optimization playbook.
        #[cfg(not(feature = "bench-baseline"))]
        let (ck_a_base, ck_b_base) = (
            LMC::msm_keys(ck_a, &ck_a_agg_challenge_exponents),
            RMC::msm_keys(ck_b, &ck_b_agg_challenge_exponents),
        );
        #[cfg(feature = "bench-baseline")]
        let (ck_a_base, ck_b_base) = (
            fold_keys_baseline::<LMC::Key, LMC::Scalar>(ck_a, &ck_a_agg_challenge_exponents),
            fold_keys_baseline::<RMC::Key, RMC::Scalar>(ck_b, &ck_b_agg_challenge_exponents),
        );
        Ok((ck_a_base, ck_b_base))
    }

    pub(crate) fn _verify_base_commitment(
        base_ck: (&LMC::Key, &RMC::Key, &Vec<IPC::Key>),
        base_com: (LMC::Output, RMC::Output, IPC::Output),
        proof: &GIPAProof<IP, LMC, RMC, IPC, D>,
    ) -> Result<bool, Error> {
        let (com_a, com_b, com_t) = base_com;
        let (ck_a_base, ck_b_base, ck_t) = base_ck;
        let a_base = vec![proof.r_base.0.clone()];
        let b_base = vec![proof.r_base.1.clone()];
        let t_base = vec![IP::inner_product(&a_base, &b_base)?];

        Ok(LMC::verify(&vec![ck_a_base.clone()], &a_base, &com_a)?
            && RMC::verify(&vec![ck_b_base.clone()], &b_base, &com_b)?
            && IPC::verify(&ck_t, &t_base, &com_t)?)
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

        assert!(ScalarGIPA::verify(
            &challenge_context,
            (&ck_a, &ck_b, &ck_t),
            (&com_a, &com_b, &com_t),
            &proof,
        )
        .unwrap());
    }
}
