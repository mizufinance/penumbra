use ark_ec::pairing::{Pairing, PairingOutput};

use ark_ec::CurveGroup;
#[cfg(test)]
use ark_ec::VariableBaseMSM;

use ark_ff::{AdditiveGroup, BigInteger, PrimeField};
use ark_ff::{Field, One, Zero};
#[cfg(test)]
use ark_groth16::VerifyingKey;
use ark_groth16::{PreparedVerifyingKey, Proof};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, SerializationError};

use ark_std::rand::Rng;
use digest::Digest;
use std::{
    fmt,
    marker::PhantomData,
    ops::{Add, Mul, Neg, Sub},
};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::challenge::checked_next_challenge_nonce;
use crate::{
    app_verifier::{
        app_verify_shipping_backend_result_from_parts,
        app_verify_shipping_backend_result_into_parts,
        app_verify_shipping_executed_result_into_parts, AppVerifyCallId, AppVerifyCallResult,
        AppVerifyShippingBackendResult, AppVerifyShippingExecutedResult, AppVerifyShippingResult,
    },
    challenge::{
        challenge_digest, sample_bounded_challenge, ChallengeContext, ChallengeNonceExhausted,
        ChallengeTraceEntry, ChallengeTraceSink, NoopChallengeTraceSink,
    },
    gipa::{fold_output, verify_base_commitment_core, BaseCommitmentCoreInput},
    tipa::{
        prove_commitment_key_kzg_opening_with_affine,
        structured_scalar_message::structured_scalar_power,
        verify_commitment_key_g1_kzg_opening_core, verify_commitment_key_g2_kzg_opening_core,
        PreparedProvingSrs, VerifierSRS, VerifierSRSData, SRS, TIPA,
    },
    Error,
};
use ark_dh_commitments::{
    afgho16::{AFGHOCommitmentG1, AFGHOCommitmentG2},
    identity::{IdentityCommitment, IdentityOutput},
};
use ark_inner_products::cfg_multi_pairing;

use ark_inner_products::cfg_multi_pairing_g1_affine_g2_prepared;

#[cfg(test)]
use ark_inner_products::PairingPreparationCount;
use ark_inner_products::{pair_prepared, prepare_g1, prepare_g2, validate_pairing_shape};
use ark_inner_products::{InnerProduct, MultiexponentiationInnerProduct, PairingInnerProduct};

type PairingInnerProductAB<P, D> = TIPA<
    PairingInnerProduct<P>,
    AFGHOCommitmentG1<P>,
    AFGHOCommitmentG2<P>,
    IdentityCommitment<PairingOutput<P>, <P as Pairing>::ScalarField>,
    P,
    D,
>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BufferedChallengeTraceSink {
    records: Vec<ChallengeTraceEntry>,
}

impl BufferedChallengeTraceSink {
    fn replay_into<S: ChallengeTraceSink>(&self, trace: &mut S) {
        for record in &self.records {
            trace.record(
                record.stage_label,
                record.nonce,
                &record.preimage,
                &record.digest,
            );
        }
    }
}

impl ChallengeTraceSink for BufferedChallengeTraceSink {
    fn record(&mut self, stage_label: &'static [u8], nonce: u64, preimage: &[u8], digest: &[u8]) {
        self.records.push(ChallengeTraceEntry {
            stage_label,
            nonce,
            preimage: preimage.to_vec(),
            digest: digest.to_vec(),
        });
    }
}

fn pairing_inner_product<P: Pairing>(
    left: &[P::G1],
    right: &[P::G2],
) -> Result<PairingOutput<P>, String> {
    PairingInnerProduct::<P>::inner_product(left, right).map_err(|error| error.to_string())
}

fn repeated_suffix<T: PartialEq>(values: &[T], real_count: usize) -> bool {
    real_count > 0
        && real_count < values.len()
        && values[real_count..]
            .iter()
            .all(|value| value == &values[real_count - 1])
}

fn coalesced_key_prefix<T>(keys: &[T], real_count: usize) -> Option<Vec<T>>
where
    T: Clone + for<'a> std::ops::AddAssign<&'a T>,
{
    if real_count == 0 || real_count >= keys.len() {
        return None;
    }
    let mut coalesced = keys[..real_count].to_vec();
    for key in &keys[real_count..] {
        coalesced[real_count - 1] += key;
    }
    Some(coalesced)
}

/// Collapse a repeat-final message suffix into one pairing term.
///
/// `real_count` identifies the authenticated real prefix. Equality is checked
/// only to decide whether the exact optimization applies.
#[cfg(test)]
fn coalesce_repeated_left_suffix<G1, G2>(
    left: &[G1],
    right: &[G2],
    real_count: usize,
) -> Option<(Vec<G1>, Vec<G2>)>
where
    G1: Clone + PartialEq,
    G2: Clone + for<'a> std::ops::AddAssign<&'a G2>,
{
    if left.len() != right.len() || real_count == 0 || real_count >= left.len() {
        return None;
    }

    if !repeated_suffix(left, real_count) {
        return None;
    }
    let coalesced_left = left[..real_count].to_vec();
    let coalesced_right = coalesced_key_prefix(right, real_count)?;
    debug_assert_eq!(coalesced_left.len(), coalesced_right.len());
    Some((coalesced_left, coalesced_right))
}

/// Right-message counterpart of [`coalesce_repeated_left_suffix`].
#[cfg(test)]
fn coalesce_repeated_right_suffix<G1, G2>(
    left: &[G1],
    right: &[G2],
    real_count: usize,
) -> Option<(Vec<G1>, Vec<G2>)>
where
    G1: Clone + for<'a> std::ops::AddAssign<&'a G1>,
    G2: Clone + PartialEq,
{
    if left.len() != right.len() || real_count == 0 || real_count >= right.len() {
        return None;
    }

    if !repeated_suffix(right, real_count) {
        return None;
    }
    let coalesced_left = coalesced_key_prefix(left, real_count)?;
    let coalesced_right = right[..real_count].to_vec();
    debug_assert_eq!(coalesced_left.len(), coalesced_right.len());
    Some((coalesced_left, coalesced_right))
}

fn initial_commitments<P: Pairing>(
    a: &[P::G1],
    b: &[P::G2],
    c: &[P::G1],
    ck_1: &[P::G2],
    ck_2: &[P::G1],
    real_count: usize,
) -> Result<(PairingOutput<P>, PairingOutput<P>, PairingOutput<P>), Error> {
    let coalesced_ck_1 = coalesced_key_prefix(ck_1, real_count);

    let coalesced_ck_2 = coalesced_key_prefix(ck_2, real_count);

    let (com_a_left, com_a_right) = if repeated_suffix(a, real_count) {
        match &coalesced_ck_1 {
            Some(keys) => (&a[..real_count], keys.as_slice()),
            None => (a, ck_1),
        }
    } else {
        (a, ck_1)
    };

    let (com_b_left, com_b_right) = if repeated_suffix(b, real_count) {
        match &coalesced_ck_2 {
            Some(keys) => (keys.as_slice(), &b[..real_count]),
            None => (ck_2, b),
        }
    } else {
        (ck_2, b)
    };

    let (com_c_left, com_c_right) = if repeated_suffix(c, real_count) {
        match &coalesced_ck_1 {
            Some(keys) => (&c[..real_count], keys.as_slice()),
            None => (c, ck_1),
        }
    } else {
        (c, ck_1)
    };

    #[cfg(feature = "parallel")]
    let ((com_a_result, com_b_result), com_c_result) = rayon::join(
        || {
            rayon::join(
                || pairing_inner_product::<P>(com_a_left, com_a_right),
                || pairing_inner_product::<P>(com_b_left, com_b_right),
            )
        },
        || pairing_inner_product::<P>(com_c_left, com_c_right),
    );

    #[cfg(not(feature = "parallel"))]
    let (com_a_result, com_b_result, com_c_result) = (
        pairing_inner_product::<P>(com_a_left, com_a_right),
        pairing_inner_product::<P>(com_b_left, com_b_right),
        pairing_inner_product::<P>(com_c_left, com_c_right),
    );

    // Resolve failures in commitment order regardless of the parallel schedule.
    let error = |error: String| Box::new(std::io::Error::other(error)) as Error;
    Ok((
        com_a_result.map_err(error)?,
        com_b_result.map_err(error)?,
        com_c_result.map_err(error)?,
    ))
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct TippMippCommitmentData<
    GT: CanonicalSerialize + CanonicalDeserialize,
    ABT: CanonicalSerialize + CanonicalDeserialize,
    CT: CanonicalSerialize + CanonicalDeserialize,
> {
    ab: (GT, GT, ABT),
    c: (GT, CT),
}

#[cfg(test)]
type TippMippCommitment<P> = TippMippCommitmentData<
    PairingOutput<P>,
    IdentityOutput<PairingOutput<P>>,
    IdentityOutput<<P as Pairing>::G1>,
>;

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct TippMippGipaProofData<
    GT: CanonicalSerialize + CanonicalDeserialize,
    ABT: CanonicalSerialize + CanonicalDeserialize,
    CT: CanonicalSerialize + CanonicalDeserialize,
    D: Send + Sync,
> {
    r_commitment_steps: Vec<(
        TippMippCommitmentData<GT, ABT, CT>,
        TippMippCommitmentData<GT, ABT, CT>,
    )>,
    _digest: PhantomData<D>,
}

#[cfg(test)]
type TippMippGipaProof<P, D> = TippMippGipaProofData<
    PairingOutput<P>,
    IdentityOutput<PairingOutput<P>>,
    IdentityOutput<<P as Pairing>::G1>,
    D,
>;

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct TippMippProofData<
    G1: CanonicalSerialize + CanonicalDeserialize,
    G2: CanonicalSerialize + CanonicalDeserialize,
    GT: CanonicalSerialize + CanonicalDeserialize,
    ABT: CanonicalSerialize + CanonicalDeserialize,
    CT: CanonicalSerialize + CanonicalDeserialize,
    D: Send + Sync,
> {
    gipa_proof: TippMippGipaProofData<GT, ABT, CT, D>,
    final_ck: (G2, G1),
    final_ck_proofs: (G2, G1),
    final_messages: (G1, G2, G1),
    _digest: PhantomData<D>,
}

#[cfg(test)]
type TippMippProof<P, D> = TippMippProofData<
    <P as Pairing>::G1,
    <P as Pairing>::G2,
    PairingOutput<P>,
    IdentityOutput<PairingOutput<P>>,
    IdentityOutput<<P as Pairing>::G1>,
    D,
>;

#[derive(Clone)]
struct TippMippCoreCommitment<GT, ABT, CT> {
    ab: (GT, GT, ABT),
    c: (GT, CT),
}

#[derive(Clone)]
struct TippMippCoreProof<G1, G2, GT, ABT, CT> {
    gipa_proof: Vec<(
        TippMippCoreCommitment<GT, ABT, CT>,
        TippMippCoreCommitment<GT, ABT, CT>,
    )>,
    final_ck: (G2, G1),
    final_ck_proofs: (G2, G1),
    final_messages: (G1, G2, G1),
}

struct TippMippCoreInput<F, G1, G2, GT, ABT, CT> {
    com_a: GT,
    com_b: GT,
    com_t: ABT,
    com_c: GT,
    com_z: CT,
    ip_ab: GT,
    agg_c: G1,
    proof: TippMippCoreProof<G1, G2, GT, ABT, CT>,
    verifier_g: G1,
    verifier_g_beta: G1,
    verifier_h: G2,
    verifier_h_alpha: G2,
    r: F,
    kzg_g2_r_shift: F,
}

struct AggregateAdapterCoreInput<F, G1, G2, G2Prepared, GT, ABT, CT> {
    randomizer_message: Vec<u8>,
    combined: CombinedChecksCoreInput<F, G1, G2, G2Prepared, GT, ABT, CT>,
}

#[derive(Debug)]
enum AggregateAdapterCoreError<E> {
    RandomizerFailure(E),
    CombinedFailure(CombinedChecksError<E>),
    NonceExhausted,
}

impl<E> From<ChallengeNonceExhausted> for AggregateAdapterCoreError<E> {
    fn from(_: ChallengeNonceExhausted) -> Self {
        Self::NonceExhausted
    }
}

#[allow(dead_code)]
struct AggregateAdapterCoreOutput<F> {
    randomizer: F,
    checks: (bool, bool),
    accepted: bool,
}

fn verify_aggregate_adapter_core<P, D, S, RFX, PE, PPE>(
    input: AggregateAdapterCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        P::G2Prepared,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    randomizer_effect: RFX,
    tipp_mipp_effect: Transcript<P, D, S>,
    tipp_pairing: PE,
    ppe_pairing: PPE,
) -> Result<AggregateAdapterCoreOutput<P::ScalarField>, AggregateAdapterCoreError<String>>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink + Send,
    RFX: FnMut(u64, &[u8]) -> Result<Option<P::ScalarField>, String>,
    PE: Fn(&[P::G1], &[P::G2]) -> Option<PairingOutput<P>> + Sync + Send,
    PPE: Fn(&[P::G1], &[P::G2Prepared]) -> Option<PairingOutput<P>> + Send,
{
    verify_aggregate_adapter_core_from_nonce(
        input,
        randomizer_effect,
        tipp_mipp_effect,
        tipp_pairing,
        ppe_pairing,
        0,
    )
}

#[allow(dead_code)]

fn verify_aggregate_adapter_core_from_nonce<P, D, S, RFX, PE, PPE>(
    input: AggregateAdapterCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        P::G2Prepared,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    mut randomizer_effect: RFX,
    tipp_mipp_effect: Transcript<P, D, S>,
    tipp_pairing: PE,
    ppe_pairing: PPE,
    nonce: u64,
) -> Result<AggregateAdapterCoreOutput<P::ScalarField>, AggregateAdapterCoreError<String>>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink + Send,
    RFX: FnMut(u64, &[u8]) -> Result<Option<P::ScalarField>, String>,
    PE: Fn(&[P::G1], &[P::G2]) -> Option<PairingOutput<P>> + Sync + Send,
    PPE: Fn(&[P::G1], &[P::G2Prepared]) -> Option<PairingOutput<P>> + Send,
{
    let AggregateAdapterCoreInput {
        randomizer_message,
        combined,
    } = input;
    let mut combined = combined;
    let mut nonce = nonce;
    loop {
        let candidate = match randomizer_effect(nonce, &randomizer_message) {
            Ok(candidate) => candidate,
            Err(error) => return Err(AggregateAdapterCoreError::RandomizerFailure(error)),
        };
        if let Some(randomizer) = candidate {
            if randomizer_is_admissible(&randomizer) {
                combined = install_aggregate_randomizer_core(combined, randomizer.clone());
                let output = match verify_combined_checks_core(
                    combined,
                    tipp_mipp_effect,
                    tipp_pairing,
                    ppe_pairing,
                ) {
                    Ok(output) => output,
                    Err(error) => {
                        return Err(AggregateAdapterCoreError::CombinedFailure(error));
                    }
                };
                let CombinedChecksCoreOutput { checks } = output;
                return Ok(AggregateAdapterCoreOutput {
                    randomizer,
                    checks,
                    accepted: checks.0 && checks.1,
                });
            }
        }
        nonce = match checked_next_challenge_nonce(nonce) {
            Some(next) => next,
            None => return Err(AggregateAdapterCoreError::NonceExhausted),
        };
    }
}

/// Install one accepted randomizer into both concrete verifier projections.
///
/// Keeping this mutation in a pure constructor makes it impossible for the
/// combined PPE and TIPP/MIPP checks to observe different randomizers.

fn install_aggregate_randomizer_core<F, G1, G2, G2Prepared, GT, ABT, CT>(
    mut combined: CombinedChecksCoreInput<F, G1, G2, G2Prepared, GT, ABT, CT>,
    randomizer: F,
) -> CombinedChecksCoreInput<F, G1, G2, G2Prepared, GT, ABT, CT>
where
    F: Clone,
{
    combined.r = randomizer.clone();
    combined.tipp_mipp.r = randomizer;
    combined
}

struct CombinedChecksPpeInput<G1, G2Prepared, GT> {
    alpha_beta: GT,
    agg_c: G1,
    gamma_g2_neg_pc: G2Prepared,
    delta_g2_neg_pc: G2Prepared,
    ip_ab: GT,
}

struct CombinedChecksCoreInput<F, G1, G2, G2Prepared, GT, ABT, CT> {
    tipp_mipp: TippMippCoreInput<F, G1, G2, GT, ABT, CT>,
    gamma_abc_g1: Vec<G1>,
    public_inputs: Vec<Vec<F>>,
    r: F,
    ppe: CombinedChecksPpeInput<G1, G2Prepared, GT>,
}

struct CombinedChecksCoreInputParts<F, G1, G2, G2Prepared, GT, ABT, CT> {
    tipp_mipp: TippMippCoreInput<F, G1, G2, GT, ABT, CT>,
    gamma_abc_g1: Vec<G1>,
    public_inputs: Vec<Vec<F>>,
    r: F,
    alpha_beta: GT,
    agg_c: G1,
    gamma_g2_neg_pc: G2Prepared,
    delta_g2_neg_pc: G2Prepared,
    ip_ab: GT,
}

fn combined_checks_core_input_from_parts<F, G1, G2, G2Prepared, GT, ABT, CT>(
    parts: CombinedChecksCoreInputParts<F, G1, G2, G2Prepared, GT, ABT, CT>,
) -> CombinedChecksCoreInput<F, G1, G2, G2Prepared, GT, ABT, CT> {
    let CombinedChecksCoreInputParts {
        tipp_mipp,
        gamma_abc_g1,
        public_inputs,
        r,
        alpha_beta,
        agg_c,
        gamma_g2_neg_pc,
        delta_g2_neg_pc,
        ip_ab,
    } = parts;
    CombinedChecksCoreInput {
        tipp_mipp,
        gamma_abc_g1,
        public_inputs,
        r,
        ppe: CombinedChecksPpeInput {
            alpha_beta,
            agg_c,
            gamma_g2_neg_pc,
            delta_g2_neg_pc,
            ip_ab,
        },
    }
}

#[derive(Debug)]
struct CombinedChecksError<E> {
    kind: usize,
    actual_rounds: usize,
    expected_rounds: usize,
    tipp_mipp_error: Option<E>,
}

struct CombinedChecksCoreOutput {
    checks: (bool, bool),
}

fn verify_combined_ppe_core<F, G1, G2Prepared, GT, E>(
    ppe: &CombinedChecksPpeInput<G1, G2Prepared, GT>,
    gamma_abc_g1: &[G1],
    public_inputs: &[Vec<F>],
    r: &F,
    pairing: &E,
) -> bool
where
    F: Clone
        + PartialEq
        + From<u64>
        + One
        + Zero
        + Add<Output = F>
        + std::ops::Div<Output = F>
        + Mul<Output = F>
        + Sub<Output = F>,
    G1: Clone + Add<Output = G1> + Mul<F, Output = G1> + Neg<Output = G1>,
    G2Prepared: Clone + Send,
    GT: Clone + Mul<F, Output = GT> + Add<Output = GT> + PartialEq,
    E: Fn(&[G1], &[G2Prepared]) -> Option<GT>,
{
    let (r_sum, g_ic) = fold_public_inputs_core(gamma_abc_g1, public_inputs, r);
    verify_ppe_core(
        PreparedPpeVerifierCoreInput {
            alpha_beta: ppe.alpha_beta.clone(),
            r_sum,
            g_ic,
            agg_c: ppe.agg_c.clone(),
            gamma_g2_neg_pc: ppe.gamma_g2_neg_pc.clone(),
            delta_g2_neg_pc: ppe.delta_g2_neg_pc.clone(),
            ip_ab: ppe.ip_ab.clone(),
        },
        pairing,
    )
}

#[allow(dead_code)]

fn verify_combined_checks_core<P, D, S, PE, PPE>(
    input: CombinedChecksCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        P::G2Prepared,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    effect: Transcript<P, D, S>,
    tipp_pairing: PE,
    ppe_pairing: PPE,
) -> Result<CombinedChecksCoreOutput, CombinedChecksError<String>>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink + Send,
    PE: Fn(&[P::G1], &[P::G2]) -> Option<PairingOutput<P>> + Sync + Send,
    PPE: Fn(&[P::G1], &[P::G2Prepared]) -> Option<PairingOutput<P>> + Send,
{
    let num_proofs = input.public_inputs.len();
    if num_proofs == 0 || !num_proofs.is_power_of_two() {
        return Err(CombinedChecksError {
            kind: 0,
            actual_rounds: 0,
            expected_rounds: 0,
            tipp_mipp_error: None,
        });
    }

    let expected_rounds = num_proofs.ilog2() as usize;
    let actual_rounds = input.tipp_mipp.proof.gipa_proof.len();
    if actual_rounds != expected_rounds {
        return Err(CombinedChecksError {
            kind: 1,
            actual_rounds,
            expected_rounds,
            tipp_mipp_error: None,
        });
    }

    let CombinedChecksCoreInput {
        tipp_mipp,
        gamma_abc_g1,
        public_inputs,
        r,
        ppe,
    } = input;

    #[cfg(feature = "parallel")]
    let (tipp_result, ppe_valid) = rayon::join(
        move || {
            let mut effect = effect;
            let result = verify_tipp_mipp_execution_core(&tipp_mipp, &mut effect, &tipp_pairing)
                .map_err(|error| CombinedChecksError {
                    kind: 2,
                    actual_rounds: 0,
                    expected_rounds: 0,
                    tipp_mipp_error: Some(error),
                });
            (result, effect)
        },
        move || {
            let ppe_valid =
                verify_combined_ppe_core(&ppe, &gamma_abc_g1, &public_inputs, &r, &ppe_pairing);
            ppe_valid
        },
    );

    #[cfg(not(feature = "parallel"))]
    let (tipp_result, ppe_valid) = {
        let mut effect = effect;
        let tipp_result = verify_tipp_mipp_execution_core(&tipp_mipp, &mut effect, &tipp_pairing)
            .map_err(|error| CombinedChecksError {
                kind: 2,
                actual_rounds: 0,
                expected_rounds: 0,
                tipp_mipp_error: Some(error),
            });
        let ppe_valid =
            verify_combined_ppe_core(&ppe, &gamma_abc_g1, &public_inputs, &r, &ppe_pairing);
        ((tipp_result, effect), ppe_valid)
    };

    let (tipp_result, _) = tipp_result;
    let tipp_valid = tipp_result?;
    Ok(CombinedChecksCoreOutput {
        checks: (tipp_valid, ppe_valid),
    })
}

/// Challenge values produced by the verifier before the leaf checks.
///
/// Both challenge orders are retained explicitly: chronology is the deployed
/// oracle-query order, while wire order is consumed by the two KZG checks.
struct TippMippChallengeTrace<F> {
    x0: F,
    round_challenges_chrono: Vec<F>,
    round_challenges_wire: Vec<F>,
    final_bridge: F,
    kzg: F,
}

/// Production state after every challenge has been derived successfully.
///
/// This boundary keeps challenge execution observable without a public API.
struct TippMippChallengePrefix<F, GT, ABT, CT> {
    challenges: TippMippChallengeTrace<F>,
    inverse_challenges_reversed: Vec<F>,
    randomizer_inverse: F,
    com_a: GT,
    com_b: GT,
    com_t: ABT,
    com_c: GT,
    com_z: CT,
}

/// Execute the production challenge/fold prefix in its exact failure order.
fn verify_tipp_mipp_challenge_prefix_core<P, D, S>(
    input: &TippMippCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    effect: &mut Transcript<P, D, S>,
) -> Result<
    TippMippChallengePrefix<P::ScalarField, PairingOutput<P>, PairingOutput<P>, P::G1>,
    String,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let mut com_z = input.com_z.clone();

    let x0 = effect.derive_x0(
        &input.r,
        &input.com_a,
        &input.com_b,
        &input.com_c,
        &input.ip_ab,
        &input.agg_c,
    )?;
    let trace_x0 = x0.clone();
    let mut prior_raw_challenge = x0.clone();
    let mut last_raw_challenge = x0;
    let mut raw_transcript_chrono = Vec::new();
    let mut inv_transcript_chrono = Vec::new();
    let mut round_error = None;

    let round_count = input.proof.gipa_proof.len();
    for round_offset in 0..round_count {
        let round_index = round_count - round_offset - 1;
        let (left, right) = &input.proof.gipa_proof[round_index];
        if round_error.is_none() {
            match effect.derive_round(&prior_raw_challenge, left, right) {
                Err(error) => round_error = Some(error),
                Ok(raw_challenge) => match effect.invert_round(&raw_challenge) {
                    Err(error) => round_error = Some(error),
                    Ok(inv_challenge) => {
                        fold_output(
                            &left.c.1,
                            &mut com_z,
                            &right.c.1,
                            &inv_challenge,
                            &raw_challenge,
                        );

                        raw_transcript_chrono.push(raw_challenge.clone());
                        inv_transcript_chrono.push(inv_challenge);
                        prior_raw_challenge = raw_challenge.clone();
                        last_raw_challenge = raw_challenge;
                    }
                },
            }
        }
    }

    if let Some(error) = round_error {
        return Err(error);
    }

    let (com_a, com_b, com_t, com_c) = effect.fold_gt_commitments(
        (&input.com_a, &input.com_b, &input.com_t, &input.com_c),
        &input.proof.gipa_proof,
        &inv_transcript_chrono,
        &raw_transcript_chrono,
    );

    let round_challenges_chrono = raw_transcript_chrono.clone();
    raw_transcript_chrono.reverse();
    inv_transcript_chrono.reverse();

    let final_bridge = effect.derive_final_bridge(
        &last_raw_challenge,
        &input.proof.final_ck,
        &input.proof.final_messages,
    )?;
    let kzg_challenge = effect.derive_kzg(&final_bridge, &input.proof.final_ck)?;
    let r_inverse = effect.invert_randomizer(&input.r)?;

    Ok(TippMippChallengePrefix {
        challenges: TippMippChallengeTrace {
            x0: trace_x0,
            round_challenges_chrono,
            round_challenges_wire: raw_transcript_chrono,
            final_bridge,
            kzg: kzg_challenge,
        },
        inverse_challenges_reversed: inv_transcript_chrono,
        randomizer_inverse: r_inverse,
        com_a,
        com_b,
        com_t,
        com_c,
        com_z,
    })
}

fn verify_tipp_mipp_execution_core<P, D, S, PE>(
    input: &TippMippCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    effect: &mut Transcript<P, D, S>,
    pairing: &PE,
) -> Result<bool, String>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
    PE: Fn(&[P::G1], &[P::G2]) -> Option<PairingOutput<P>> + Sync + Send,
{
    let TippMippChallengePrefix {
        challenges,
        inverse_challenges_reversed,
        randomizer_inverse,
        com_a,
        com_b,
        com_t,
        com_c,
        com_z,
    } = verify_tipp_mipp_challenge_prefix_core(input, effect)?;
    let TippMippChallengeTrace {
        x0: _x0_challenge,
        round_challenges_chrono: _round_challenges_chrono,
        round_challenges_wire,
        final_bridge: _final_bridge_challenge,
        kzg: kzg_challenge,
    } = &challenges;

    let ck_v_final = input.proof.final_ck.0.clone();
    let ck_w_final = input.proof.final_ck.1.clone();
    let ck_v_proof = input.proof.final_ck_proofs.0.clone();
    let ck_w_proof = input.proof.final_ck_proofs.1.clone();

    #[cfg(feature = "parallel")]
    let (ck_v_valid, ck_w_valid) = rayon::join(
        || {
            verify_commitment_key_g2_kzg_opening_core(
                input.verifier_g.clone(),
                input.verifier_g_beta.clone(),
                input.verifier_h.clone(),
                ck_v_final.clone(),
                ck_v_proof.clone(),
                round_challenges_wire,
                &input.kzg_g2_r_shift,
                kzg_challenge,
                pairing,
            )
        },
        || {
            verify_commitment_key_g1_kzg_opening_core(
                input.verifier_g.clone(),
                input.verifier_h_alpha.clone(),
                input.verifier_h.clone(),
                ck_w_final.clone(),
                ck_w_proof.clone(),
                &inverse_challenges_reversed,
                &randomizer_inverse,
                kzg_challenge,
                pairing,
            )
        },
    );

    #[cfg(not(feature = "parallel"))]
    let (ck_v_valid, ck_w_valid) = (
        verify_commitment_key_g2_kzg_opening_core(
            input.verifier_g.clone(),
            input.verifier_g_beta.clone(),
            input.verifier_h.clone(),
            ck_v_final.clone(),
            ck_v_proof.clone(),
            round_challenges_wire,
            &input.kzg_g2_r_shift,
            kzg_challenge,
            pairing,
        ),
        verify_commitment_key_g1_kzg_opening_core(
            input.verifier_g.clone(),
            input.verifier_h_alpha.clone(),
            input.verifier_h.clone(),
            ck_w_final.clone(),
            ck_w_proof.clone(),
            &inverse_challenges_reversed,
            &randomizer_inverse,
            kzg_challenge,
            pairing,
        ),
    );

    let (a_final, b_final, c_final) = &input.proof.final_messages;
    let a_base = vec![a_final.clone()];
    let b_base = vec![b_final.clone()];
    let c_base = vec![c_final.clone()];
    let ck_v_base = vec![ck_v_final.clone()];
    let ck_w_base = vec![ck_w_final.clone()];

    let base_result = verify_base_commitment_core(
        BaseCommitmentCoreInput {
            ck_a: ck_v_base[0].clone(),
            ck_b: ck_w_base[0].clone(),
            ck_t: vec![()],
            a: a_base[0].clone(),
            b: b_base[0].clone(),
            com_a: com_a.clone(),
            com_b: com_b.clone(),
            com_t: com_t.clone(),
        },
        pairing_inner_product::<P>,
        |keys, messages, commitment| Ok(pairing_inner_product::<P>(messages, keys)? == *commitment),
        |keys, messages, commitment| Ok(pairing_inner_product::<P>(keys, messages)? == *commitment),
        |_: &[()], messages, commitment| Ok(messages.len() == 1 && messages[0] == *commitment),
    );
    let base_valid = match base_result {
        Result::Ok(value) => value,
        Result::Err(error) => return Err(error),
    };
    if !base_valid {
        return Ok(false);
    }

    let c_valid = effect.verify_c(&c_base, &ck_v_base, &com_c)?;
    if !c_valid {
        return Ok(false);
    }

    let final_r =
        structured_scalar_final_from_raw_transcript_inner(round_challenges_wire, &input.r);
    let z_valid = effect.verify_z(&c_base, &[final_r], &com_z)?;

    let accepted = ck_v_valid && ck_w_valid && base_valid && c_valid && z_valid;
    Ok(accepted)
}

#[allow(dead_code)]
fn verify_tipp_mipp_core<P, D, S, PE>(
    input: TippMippCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    effect: &mut Transcript<P, D, S>,
    pairing: &PE,
) -> Result<bool, String>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
    PE: Fn(&[P::G1], &[P::G2]) -> Option<PairingOutput<P>> + Sync + Send,
{
    match verify_tipp_mipp_execution_core(&input, effect, pairing) {
        Ok(output) => Ok(output),
        Err(error) => Err(error),
    }
}

/// Inputs whose transition schedule is owned by the aggregate prover rather
/// than by Arkworks pairing, MSM, serialization, or challenge primitives.
#[derive(Clone)]
struct ProverGipaCoreInput<F, G1, G2, GT> {
    a: Vec<G1>,
    b: Vec<G2>,
    c: Vec<G1>,
    public_values: Vec<F>,
    ck_v: Vec<G2>,
    ck_w: Vec<G1>,
    randomizer: F,
    com_a: GT,
    com_b: GT,
    com_c: GT,
    ip_ab: GT,
    agg_c: G1,
}

// Recursive rounds are encoded from the final fold back to the first.
struct ProverGipaCoreOutput<F, G1, G2, GT, ABT, CT> {
    rounds_wire: Vec<(
        TippMippCoreCommitment<GT, ABT, CT>,
        TippMippCoreCommitment<GT, ABT, CT>,
    )>,
    raw_transcript_wire: Vec<F>,
    inv_transcript_wire: Vec<F>,
    last_raw_challenge: F,
    final_ck: (G2, G1),
    final_messages: (G1, G2, G1),
    #[cfg(test)]
    final_public_value: F,
}

/// Ordered source points consumed by the shipping aggregate prover.
struct OrderedSourceProofs<G1, G2> {
    a: Vec<G1>,
    b: Vec<G2>,
    c: Vec<G1>,
}

fn ordered_source_proofs_core<P: Pairing>(
    proofs: &[Proof<P>],
) -> OrderedSourceProofs<P::G1, P::G2> {
    let mut a = Vec::with_capacity(proofs.len());
    let mut b = Vec::with_capacity(proofs.len());
    let mut c = Vec::with_capacity(proofs.len());
    let mut index = 0usize;
    while index < proofs.len() {
        a.push(proofs[index].a.into());
        b.push(proofs[index].b.into());
        c.push(proofs[index].c.into());
        index += 1;
    }
    OrderedSourceProofs { a, b, c }
}

#[derive(Debug)]
struct ProverGipaCoreError<E> {
    kind: usize,
    effect_error: Option<E>,
}

struct ProverGipaRoundOutput<F, G1, G2, GT, ABT, CT> {
    left: TippMippCoreCommitment<GT, ABT, CT>,
    right: TippMippCoreCommitment<GT, ABT, CT>,
    raw_challenge: F,
    inv_challenge: F,
    a: Vec<G1>,
    b: Vec<G2>,
    c: Vec<G1>,
    public_values: Vec<F>,
    ck_v: Vec<G2>,
    ck_w: Vec<G1>,
}

/// Pairing and MSM operations needed for one prover cross-commitment.
///
/// The adapter core owns all five operand pairs and their error order. The
/// concrete implementation below delegates only those algebraic operations
/// to Arkworks.
#[derive(Debug, PartialEq)]
struct ProverRoundPairingOutput<GT> {
    com_a: GT,
    com_b: GT,
    ip_ab: GT,
    com_c: GT,
}

struct SharedProverRoundPairingOutput<GT> {
    products: ProverRoundPairingOutput<GT>,
    #[cfg(test)]
    preparation: PairingPreparationCount,
}

fn shared_prover_round_pairing_products<P: Pairing>(
    a: &[P::G1],
    b: &[P::G2],
    c: &[P::G1],
    ck_v: &[P::G2],
    ck_w: &[P::G1],
) -> Result<SharedProverRoundPairingOutput<PairingOutput<P>>, String> {
    for (left_len, right_len) in [
        (a.len(), ck_v.len()),
        (ck_w.len(), b.len()),
        (a.len(), b.len()),
        (c.len(), ck_v.len()),
    ] {
        validate_pairing_shape(left_len, right_len).map_err(|error| error.to_string())?;
    }

    let a = prepare_g1::<P>(a);
    let b = prepare_g2::<P>(b);
    let c = prepare_g1::<P>(c);
    let ck_v = prepare_g2::<P>(ck_v);
    let ck_w = prepare_g1::<P>(ck_w);
    #[cfg(test)]
    let preparation = a.preparation()
        + b.preparation()
        + c.preparation()
        + ck_v.preparation()
        + ck_w.preparation();

    let com_a = pair_prepared::<P>(&a, &ck_v).map_err(|error| error.to_string())?;
    let com_b = pair_prepared::<P>(&ck_w, &b).map_err(|error| error.to_string())?;
    let ip_ab = pair_prepared::<P>(&a, &b).map_err(|error| error.to_string())?;
    let com_c = pair_prepared::<P>(&c, &ck_v).map_err(|error| error.to_string())?;

    Ok(SharedProverRoundPairingOutput {
        products: ProverRoundPairingOutput {
            com_a,
            com_b,
            ip_ab,
            com_c,
        },
        #[cfg(test)]
        preparation,
    })
}

/// Copies a vector into ordered halves.
fn split_vector_at_core<T: Clone>(values: &[T], split: usize) -> (Vec<T>, Vec<T>) {
    let split = split.min(values.len());
    (values[..split].to_vec(), values[split..].to_vec())
}

#[allow(clippy::too_many_arguments)]
fn prove_tipp_mipp_gipa_round_core<P, D, S>(
    a_left: &[P::G1],
    a_right: &[P::G1],
    b_left: &[P::G2],
    b_right: &[P::G2],
    c_left: &[P::G1],
    c_right: &[P::G1],
    public_left: &[P::ScalarField],
    public_right: &[P::ScalarField],
    v_left: &[P::G2],
    v_right: &[P::G2],
    w_left: &[P::G1],
    w_right: &[P::G1],
    prior_raw_challenge: &P::ScalarField,
    effect: &mut Transcript<P, D, S>,
) -> Result<
    ProverGipaRoundOutput<P::ScalarField, P::G1, P::G2, PairingOutput<P>, PairingOutput<P>, P::G1>,
    ProverGipaCoreError<String>,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    #[cfg(feature = "parallel")]
    let (left_result, right_result) = rayon::join(
        || {
            let result =
                commit_tipp_mipp_round::<P>(a_right, b_left, c_right, public_left, v_left, w_right);
            result
        },
        || {
            let result =
                commit_tipp_mipp_round::<P>(a_left, b_right, c_left, public_right, v_right, w_left);
            result
        },
    );

    #[cfg(not(feature = "parallel"))]
    let (left_result, right_result) = {
        let left =
            commit_tipp_mipp_round::<P>(a_right, b_left, c_right, public_left, v_left, w_right);

        let right =
            commit_tipp_mipp_round::<P>(a_left, b_right, c_left, public_right, v_right, w_left);

        (left, right)
    };

    let left = left_result.map_err(|effect_error| ProverGipaCoreError {
        kind: 2,
        effect_error: Some(effect_error),
    })?;
    let right = right_result.map_err(|effect_error| ProverGipaCoreError {
        kind: 3,
        effect_error: Some(effect_error),
    })?;
    let raw_challenge = effect
        .derive_round(prior_raw_challenge, &left, &right)
        .map_err(|effect_error| ProverGipaCoreError {
            kind: 4,
            effect_error: Some(effect_error),
        })?;
    let inv_challenge = effect
        .invert_round(&raw_challenge)
        .map_err(|effect_error| ProverGipaCoreError {
            kind: 5,
            effect_error: Some(effect_error),
        })?;

    let a = fold_vector_core(a_right, a_left, &inv_challenge);

    let b = fold_vector_core(b_right, b_left, &raw_challenge);

    let c = fold_vector_core(c_right, c_left, &inv_challenge);

    let public_values = fold_vector_core(public_right, public_left, &raw_challenge);

    let ck_v = fold_vector_core(v_right, v_left, &raw_challenge);

    let ck_w = fold_vector_core(w_right, w_left, &inv_challenge);

    Ok(ProverGipaRoundOutput {
        left,
        right,
        raw_challenge,
        inv_challenge,
        a,
        b,
        c,
        public_values,
        ck_v,
        ck_w,
    })
}

/// Production GIPA prover schedule.
///
/// All algebraic state transitions are executed here. The effect boundary is
/// restricted to the five cross commitments, challenge derivation, and exact
/// field inversion.
fn prove_tipp_mipp_gipa_core<P, D, S>(
    input: ProverGipaCoreInput<P::ScalarField, P::G1, P::G2, PairingOutput<P>>,
    effect: &mut Transcript<P, D, S>,
) -> Result<
    ProverGipaCoreOutput<P::ScalarField, P::G1, P::G2, PairingOutput<P>, PairingOutput<P>, P::G1>,
    ProverGipaCoreError<String>,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let input_len = input.a.len();
    if input_len == 0
        || !input_len.is_power_of_two()
        || input.b.len() != input_len
        || input.c.len() != input_len
        || input.public_values.len() != input_len
        || input.ck_v.len() != input_len
        || input.ck_w.len() != input_len
    {
        return Err(ProverGipaCoreError {
            kind: 0,
            effect_error: None,
        });
    }

    let ProverGipaCoreInput {
        mut a,
        mut b,
        mut c,
        mut public_values,
        mut ck_v,
        mut ck_w,
        randomizer,
        com_a,
        com_b,
        com_c,
        ip_ab,
        agg_c,
    } = input;

    let x0 = effect
        .derive_x0(&randomizer, &com_a, &com_b, &com_c, &ip_ab, &agg_c)
        .map_err(|effect_error| ProverGipaCoreError {
            kind: 1,
            effect_error: Some(effect_error),
        })?;
    let mut prior_raw_challenge = x0.clone();
    let mut last_raw_challenge = x0.clone();
    let mut rounds_chrono = Vec::new();
    let mut raw_transcript_chrono = Vec::new();
    let mut inv_transcript_chrono = Vec::new();
    let mut round_error = None;

    while a.len() > 1 && round_error.is_none() {
        let split = a.len() / 2;
        let (a_left, a_right) = split_vector_at_core(&a, split);
        let (b_left, b_right) = split_vector_at_core(&b, split);
        let (c_left, c_right) = split_vector_at_core(&c, split);
        let (public_left, public_right) = split_vector_at_core(&public_values, split);
        let (v_left, v_right) = split_vector_at_core(&ck_v, split);
        let (w_left, w_right) = split_vector_at_core(&ck_w, split);
        match prove_tipp_mipp_gipa_round_core(
            &a_left,
            &a_right,
            &b_left,
            &b_right,
            &c_left,
            &c_right,
            &public_left,
            &public_right,
            &v_left,
            &v_right,
            &w_left,
            &w_right,
            &prior_raw_challenge,
            effect,
        ) {
            Err(error) => round_error = Some(error),
            Ok(output) => {
                a = output.a;
                b = output.b;
                c = output.c;
                public_values = output.public_values;
                ck_v = output.ck_v;
                ck_w = output.ck_w;
                rounds_chrono.push((output.left, output.right));
                raw_transcript_chrono.push(output.raw_challenge.clone());
                inv_transcript_chrono.push(output.inv_challenge);
                prior_raw_challenge = output.raw_challenge.clone();
                last_raw_challenge = output.raw_challenge;
            }
        }
    }

    if let Some(error) = round_error {
        return Err(error);
    }

    let rounds_wire = reverse_chronological_core(rounds_chrono);
    let raw_transcript_wire = reverse_chronological_core(raw_transcript_chrono);
    let inv_transcript_wire = reverse_chronological_core(inv_transcript_chrono);

    Ok(ProverGipaCoreOutput {
        rounds_wire,
        raw_transcript_wire,
        inv_transcript_wire,
        last_raw_challenge,
        final_ck: (ck_v[0].clone(), ck_w[0].clone()),
        final_messages: (a[0].clone(), b[0].clone(), c[0].clone()),
        #[cfg(test)]
        final_public_value: public_values[0].clone(),
    })
}

fn prover_gipa_rounds_to_wire<GT, ABT, CT>(
    rounds: Vec<(
        TippMippCoreCommitment<GT, ABT, CT>,
        TippMippCoreCommitment<GT, ABT, CT>,
    )>,
) -> Vec<(
    TippMippCommitmentData<GT, IdentityOutput<ABT>, IdentityOutput<CT>>,
    TippMippCommitmentData<GT, IdentityOutput<ABT>, IdentityOutput<CT>>,
)>
where
    GT: Clone + CanonicalSerialize + CanonicalDeserialize,
    ABT: Clone + Default + Eq + CanonicalSerialize + CanonicalDeserialize,
    CT: Clone + Default + Eq + CanonicalSerialize + CanonicalDeserialize,
{
    rounds
        .into_iter()
        .map(|(left, right)| {
            (
                TippMippCommitmentData {
                    ab: (left.ab.0, left.ab.1, IdentityOutput(vec![left.ab.2])),
                    c: (left.c.0, IdentityOutput(vec![left.c.1])),
                },
                TippMippCommitmentData {
                    ab: (right.ab.0, right.ab.1, IdentityOutput(vec![right.ab.2])),
                    c: (right.c.0, IdentityOutput(vec![right.c.1])),
                },
            )
        })
        .collect()
}

fn prove_tipp_mipp_buffered<P, D>(
    context: &ChallengeContext,
    prepared_srs: &PreparedProvingSrs<P>,
    a: &[P::G1],
    b_r: &[P::G2],
    c: &[P::G1],
    r_vec: &[P::ScalarField],
    ck_1: &[P::G2],
    ck_2_r_inv: &[P::G1],
    r: &P::ScalarField,
    com: (&PairingOutput<P>, &PairingOutput<P>, &PairingOutput<P>),
    ip_ab: &PairingOutput<P>,
    agg_c: &P::G1,
) -> Result<(AggregateProof<P, D>, BufferedChallengeTraceSink), String>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let mut trace = BufferedChallengeTraceSink::default();

    let execution = prove_tipp_mipp::<P, D, BufferedChallengeTraceSink>(
        context,
        &mut trace,
        prepared_srs,
        a,
        b_r,
        c,
        r_vec,
        ck_1,
        ck_2_r_inv,
        r,
        com,
        ip_ab,
        agg_c,
    )
    .map_err(|err| err.to_string())?;
    Ok((execution, trace))
}

#[allow(clippy::too_many_arguments)]
fn prove_tipp_mipp<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    prepared_srs: &PreparedProvingSrs<P>,
    a: &[P::G1],
    b_r: &[P::G2],
    c: &[P::G1],
    r_vec: &[P::ScalarField],
    ck_1: &[P::G2],
    ck_2_r_inv: &[P::G1],
    r: &P::ScalarField,
    com: (&PairingOutput<P>, &PairingOutput<P>, &PairingOutput<P>),
    ip_ab: &PairingOutput<P>,
    agg_c: &P::G1,
) -> Result<AggregateProof<P, D>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let gipa_execution = prove_tipp_mipp_gipa::<P, D, S>(
        context, trace, a, b_r, c, r_vec, ck_1, ck_2_r_inv, r, com, ip_ab, agg_c,
    )?;

    let raw_transcript = &gipa_execution.raw_transcript_wire;
    let inv_transcript = &gipa_execution.inv_transcript_wire;

    let final_bridge = derive_final_bridge::<P, D, S>(
        context,
        trace,
        &gipa_execution.last_raw_challenge,
        &gipa_execution.final_ck,
        &gipa_execution.final_messages,
    )?;

    let kzg_challenge =
        derive_kzg_challenge::<P, D, S>(context, trace, &final_bridge, &gipa_execution.final_ck)?;

    let r_inverse = r.inverse().ok_or_else(|| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "randomizer must be non-zero before inversion",
        )) as Error
    })?;

    let ck_v_kzg_opening = prove_commitment_key_kzg_opening_with_affine(
        prepared_srs.h_beta_powers_affine(),
        raw_transcript,
        &P::ScalarField::one(),
        &kzg_challenge,
    )?;

    let ck_w_kzg_opening = prove_commitment_key_kzg_opening_with_affine(
        prepared_srs.g_alpha_powers_affine(),
        inv_transcript,
        &r_inverse,
        &kzg_challenge,
    )?;

    let gipa_proof = TippMippGipaProofData {
        r_commitment_steps: prover_gipa_rounds_to_wire(gipa_execution.rounds_wire),
        _digest: PhantomData,
    };
    let tipp_mipp_proof = TippMippProofData {
        gipa_proof: gipa_proof,
        final_ck: gipa_execution.final_ck,
        final_ck_proofs: (ck_v_kzg_opening, ck_w_kzg_opening),
        final_messages: gipa_execution.final_messages,
        _digest: PhantomData,
    };
    Ok(AggregateProofData {
        com_a: *com.0,
        com_b: *com.1,
        com_c: *com.2,
        ip_ab: *ip_ab,
        agg_c: *agg_c,
        tipp_mipp_proof: tipp_mipp_proof,
    })
}

#[allow(clippy::too_many_arguments)]
fn prove_tipp_mipp_gipa<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    a: &[P::G1],
    b_r: &[P::G2],
    c: &[P::G1],
    r_vec: &[P::ScalarField],
    ck_1: &[P::G2],
    ck_2_r_inv: &[P::G1],
    r: &P::ScalarField,
    com: (&PairingOutput<P>, &PairingOutput<P>, &PairingOutput<P>),
    ip_ab: &PairingOutput<P>,
    agg_c: &P::G1,
) -> Result<
    ProverGipaCoreOutput<P::ScalarField, P::G1, P::G2, PairingOutput<P>, PairingOutput<P>, P::G1>,
    Error,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    {
        let mut effect = Transcript::<P, D, S> {
            context,
            trace,
            _pairing: PhantomData,
            _digest: PhantomData,
        };
        let input = ProverGipaCoreInput {
            a: a.to_vec(),
            b: b_r.to_vec(),
            c: c.to_vec(),
            public_values: r_vec.to_vec(),
            ck_v: ck_1.to_vec(),
            ck_w: ck_2_r_inv.to_vec(),
            randomizer: r.clone(),
            com_a: com.0.clone(),
            com_b: com.1.clone(),
            com_c: com.2.clone(),
            ip_ab: ip_ab.clone(),
            agg_c: agg_c.clone(),
        };
        let execution = prove_tipp_mipp_gipa_core(input, &mut effect).map_err(|error| {
            let ProverGipaCoreError { kind, effect_error } = error;
            let message = match (kind, effect_error) {
                (0, _) => {
                    "combined TIPP/MIPP inputs must have equal power-of-two length".to_owned()
                }
                (_, Some(error)) => error,
                (kind, None) => format!("prover GIPA schedule error {kind}"),
            };
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                message,
            )) as Error
        })?;

        return Ok(execution);
    }
}

fn commit_tipp_mipp_round<P: Pairing>(
    a: &[P::G1],
    b: &[P::G2],
    c: &[P::G1],
    r: &[P::ScalarField],
    ck_v: &[P::G2],
    ck_w: &[P::G1],
) -> Result<TippMippCoreCommitment<PairingOutput<P>, PairingOutput<P>, P::G1>, String> {
    let products = shared_prover_round_pairing_products::<P>(a, b, c, ck_v, ck_w)?.products;
    let ip_c = MultiexponentiationInnerProduct::<P::G1>::inner_product(c, r)
        .map_err(|error| error.to_string())?;
    Ok(TippMippCoreCommitment {
        ab: (products.com_a, products.com_b, products.ip_ab),
        c: (products.com_c, ip_c),
    })
}

fn fold_vector_core<T, F>(scaled_half: &[T], unscaled_half: &[T], scalar: &F) -> Vec<T>
where
    T: Clone + std::ops::Mul<F, Output = T> + Add<Output = T>,
    F: Clone,
{
    scaled_half
        .iter()
        .zip(unscaled_half)
        .map(|(scaled, unscaled)| scaled.clone() * scalar.clone() + unscaled.clone())
        .collect()
}

fn reverse_chronological_core<T>(mut values: Vec<T>) -> Vec<T> {
    values.reverse();
    values
}

fn derive_scalar_challenge<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    stage_label: &'static [u8],
    messages: &[u8],
) -> Result<P::ScalarField, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    sample_bounded_challenge::<_, Error, _>(|nonce| {
        Ok(P::ScalarField::from_random_bytes(&challenge_digest::<D, _>(
            context,
            trace,
            stage_label,
            nonce,
            messages,
        ))
        .filter(|challenge| !challenge.is_zero()))
    })
}

fn randomizer_is_admissible<F>(randomizer: &F) -> bool
where
    F: PartialEq + Zero + One,
{
    randomizer != &F::zero() && randomizer != &F::one()
}

fn derive_final_bridge<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    last_raw_challenge: &P::ScalarField,
    final_ck: &(P::G2, P::G1),
    final_messages: &(P::G1, P::G2, P::G1),
) -> Result<P::ScalarField, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    Transcript::<P, D, S> {
        context,
        trace,
        _pairing: PhantomData,
        _digest: PhantomData,
    }
    .derive_final_bridge(last_raw_challenge, final_ck, final_messages)
    .map_err(|error| Box::new(std::io::Error::other(error)) as Error)
}

fn derive_kzg_challenge<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    final_bridge: &P::ScalarField,
    final_ck: &(P::G2, P::G1),
) -> Result<P::ScalarField, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    Transcript::<P, D, S> {
        context,
        trace,
        _pairing: PhantomData,
        _digest: PhantomData,
    }
    .derive_kzg(final_bridge, final_ck)
    .map_err(|error| Box::new(std::io::Error::other(error)) as Error)
}

/// External primitives used by the production TIPP/MIPP adapter.
///
/// The adapter cores below own byte ordering, stage routing, comparisons, and
/// error ordering. Implementations of this trait are restricted to the
/// Arkworks serialization, field, pairing, MSM, and deployed-hash boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TippMippChallengeStage {
    X0,
    Round,
    FinalBridge,
    Kzg,
}

fn tipp_mipp_challenge_stage_label(stage: TippMippChallengeStage) -> &'static [u8] {
    match stage {
        TippMippChallengeStage::X0 => b"tipp-mipp.x0",
        TippMippChallengeStage::Round => b"tipp-mipp.gipa.round",
        TippMippChallengeStage::FinalBridge => b"tipp-mipp.final-bridge",
        TippMippChallengeStage::Kzg => b"tipp-mipp.kzg",
    }
}

#[allow(dead_code)]

fn append_canonical<T: CanonicalSerialize>(value: &T, output: &mut Vec<u8>) -> Result<(), String> {
    value
        .serialize_uncompressed(output)
        .map_err(|error| error.to_string())
}

fn append_singleton<T: CanonicalSerialize>(value: &T, output: &mut Vec<u8>) -> Result<(), String> {
    append_canonical(&1u64, output)?;
    append_canonical(value, output)
}

struct Transcript<'a, P: Pairing, D: Digest + Send + Sync, S: ChallengeTraceSink> {
    context: &'a ChallengeContext,
    trace: &'a mut S,
    _pairing: PhantomData<fn() -> P>,
    _digest: PhantomData<fn() -> D>,
}

type ArkworksTippMippRound<P> = (
    TippMippCoreCommitment<PairingOutput<P>, PairingOutput<P>, <P as Pairing>::G1>,
    TippMippCoreCommitment<PairingOutput<P>, PairingOutput<P>, <P as Pairing>::G1>,
);

#[cfg(test)]
fn fold_arkworks_gt_commitments_independent_msm<P: Pairing>(
    roots: (
        &PairingOutput<P>,
        &PairingOutput<P>,
        &PairingOutput<P>,
        &PairingOutput<P>,
    ),
    rounds_wire: &[ArkworksTippMippRound<P>],
    inverse_challenges_chrono: &[P::ScalarField],
    raw_challenges_chrono: &[P::ScalarField],
) -> (
    PairingOutput<P>,
    PairingOutput<P>,
    PairingOutput<P>,
    PairingOutput<P>,
) {
    assert_eq!(rounds_wire.len(), inverse_challenges_chrono.len());
    assert_eq!(rounds_wire.len(), raw_challenges_chrono.len());

    let term_count = 1 + 2 * rounds_wire.len();
    let mut scalars = Vec::with_capacity(term_count);
    let mut a_bases = Vec::with_capacity(term_count);
    let mut b_bases = Vec::with_capacity(term_count);
    let mut t_bases = Vec::with_capacity(term_count);
    let mut c_bases = Vec::with_capacity(term_count);
    scalars.push(P::ScalarField::one());
    a_bases.push(roots.0.clone());
    b_bases.push(roots.1.clone());
    t_bases.push(roots.2.clone());
    c_bases.push(roots.3.clone());

    for round_offset in 0..rounds_wire.len() {
        let round_index = rounds_wire.len() - round_offset - 1;
        let (left, right) = &rounds_wire[round_index];
        scalars.push(inverse_challenges_chrono[round_offset].clone());
        scalars.push(raw_challenges_chrono[round_offset].clone());
        a_bases.extend([left.ab.0.clone(), right.ab.0.clone()]);
        b_bases.extend([left.ab.1.clone(), right.ab.1.clone()]);
        t_bases.extend([left.ab.2.clone(), right.ab.2.clone()]);
        c_bases.extend([left.c.0.clone(), right.c.0.clone()]);
    }

    (
        PairingOutput::<P>::msm_unchecked(&a_bases, &scalars),
        PairingOutput::<P>::msm_unchecked(&b_bases, &scalars),
        PairingOutput::<P>::msm_unchecked(&t_bases, &scalars),
        PairingOutput::<P>::msm_unchecked(&c_bases, &scalars),
    )
}

fn signed_window_digits<B: BigInteger>(scalar: &B, width: usize, num_bits: usize) -> Vec<i64> {
    let limbs = scalar.as_ref();
    let radix = 1u64 << width;
    let window_mask = radix - 1;
    let digits_count = num_bits.div_ceil(width);
    let mut carry = 0u64;
    let mut digits = Vec::with_capacity(digits_count);

    for index in 0..digits_count {
        let bit_offset = index * width;
        let limb_index = bit_offset / 64;
        let bit_index = bit_offset % 64;
        let bit_buffer = if bit_index < 64 - width || limb_index == limbs.len() - 1 {
            limbs[limb_index] >> bit_index
        } else {
            (limbs[limb_index] >> bit_index) | (limbs[limb_index + 1] << (64 - bit_index))
        };
        let coefficient = carry + (bit_buffer & window_mask);
        carry = (coefficient + radix / 2) >> width;
        let mut digit = coefficient as i64 - (carry << width) as i64;
        if index == digits_count - 1 {
            digit += (carry << width) as i64;
        }
        digits.push(digit);
    }
    digits
}

fn msm_from_signed_digit_schedule<T: AdditiveGroup>(
    bases: &[T],
    scalar_digits: &[i64],
    digits_count: usize,
    width: usize,
) -> T {
    assert_eq!(scalar_digits.len(), bases.len() * digits_count);
    let window_sums = ark_std::cfg_into_iter!(0..digits_count)
        .map(|window| {
            let mut buckets = vec![T::zero(); 1 << width];
            for (digits, base) in scalar_digits.chunks(digits_count).zip(bases) {
                let digit = digits[window];
                if digit > 0 {
                    buckets[(digit - 1) as usize] += base;
                } else if digit < 0 {
                    buckets[(-digit - 1) as usize] -= base;
                }
            }

            let mut running_sum = T::zero();
            let mut result = T::zero();
            for bucket in buckets.into_iter().rev() {
                running_sum += bucket;
                result += running_sum;
            }
            result
        })
        .collect::<Vec<_>>();

    let lowest = window_sums[0];
    lowest
        + window_sums[1..]
            .iter()
            .rev()
            .fold(T::zero(), |mut total, window_sum| {
                total += window_sum;
                for _ in 0..width {
                    total.double_in_place();
                }
                total
            })
}

fn fold_arkworks_gt_commitments_shared_schedule<P: Pairing>(
    roots: (
        &PairingOutput<P>,
        &PairingOutput<P>,
        &PairingOutput<P>,
        &PairingOutput<P>,
    ),
    rounds_wire: &[ArkworksTippMippRound<P>],
    inverse_challenges_chrono: &[P::ScalarField],
    raw_challenges_chrono: &[P::ScalarField],
) -> (
    PairingOutput<P>,
    PairingOutput<P>,
    PairingOutput<P>,
    PairingOutput<P>,
) {
    assert_eq!(rounds_wire.len(), inverse_challenges_chrono.len());
    assert_eq!(rounds_wire.len(), raw_challenges_chrono.len());

    let term_count = 1 + 2 * rounds_wire.len();
    let width: usize = if term_count < 32 {
        3
    } else {
        (ark_std::log2(term_count) * 69 / 100 + 2) as usize
    };
    let num_bits = P::ScalarField::MODULUS_BIT_SIZE as usize;
    let digits_count = num_bits.div_ceil(width);
    let mut scalars = Vec::with_capacity(term_count);
    let mut a_bases = Vec::with_capacity(term_count);
    let mut b_bases = Vec::with_capacity(term_count);
    let mut t_bases = Vec::with_capacity(term_count);
    let mut c_bases = Vec::with_capacity(term_count);
    scalars.push(P::ScalarField::one());
    a_bases.push(roots.0.clone());
    b_bases.push(roots.1.clone());
    t_bases.push(roots.2.clone());
    c_bases.push(roots.3.clone());

    for round_offset in 0..rounds_wire.len() {
        let round_index = rounds_wire.len() - round_offset - 1;
        let (left, right) = &rounds_wire[round_index];
        scalars.push(inverse_challenges_chrono[round_offset].clone());
        scalars.push(raw_challenges_chrono[round_offset].clone());
        a_bases.extend([left.ab.0.clone(), right.ab.0.clone()]);
        b_bases.extend([left.ab.1.clone(), right.ab.1.clone()]);
        t_bases.extend([left.ab.2.clone(), right.ab.2.clone()]);
        c_bases.extend([left.c.0.clone(), right.c.0.clone()]);
    }

    let scalar_digits = scalars
        .iter()
        .flat_map(|scalar| signed_window_digits(&scalar.into_bigint(), width, num_bits))
        .collect::<Vec<_>>();
    (
        msm_from_signed_digit_schedule(&a_bases, &scalar_digits, digits_count, width),
        msm_from_signed_digit_schedule(&b_bases, &scalar_digits, digits_count, width),
        msm_from_signed_digit_schedule(&t_bases, &scalar_digits, digits_count, width),
        msm_from_signed_digit_schedule(&c_bases, &scalar_digits, digits_count, width),
    )
}

impl<'a, P: Pairing, D: Digest + Send + Sync, S: ChallengeTraceSink> Transcript<'a, P, D, S> {
    fn derive_challenge(
        &mut self,
        stage: TippMippChallengeStage,
        messages: &[u8],
    ) -> Result<P::ScalarField, String> {
        let stage_label = tipp_mipp_challenge_stage_label(stage);
        let sample =
            derive_scalar_challenge::<P, D, S>(self.context, self.trace, stage_label, messages)
                .map_err(|error| error.to_string())?;
        Ok(sample)
    }
}

impl<'a, P: Pairing, D: Digest + Send + Sync, S: ChallengeTraceSink> Transcript<'a, P, D, S> {
    fn derive_x0(
        &mut self,
        r: &P::ScalarField,
        com_a: &PairingOutput<P>,
        com_b: &PairingOutput<P>,
        com_c: &PairingOutput<P>,
        ip_ab: &PairingOutput<P>,
        agg_c: &P::G1,
    ) -> Result<P::ScalarField, String> {
        let mut hash_input = Vec::new();
        append_canonical(r, &mut hash_input)?;
        append_canonical(com_a, &mut hash_input)?;
        append_canonical(com_b, &mut hash_input)?;
        append_canonical(com_c, &mut hash_input)?;
        append_canonical(ip_ab, &mut hash_input)?;
        append_canonical(agg_c, &mut hash_input)?;
        self.derive_challenge(TippMippChallengeStage::X0, &hash_input)
    }

    fn derive_round(
        &mut self,
        prior_raw_challenge: &P::ScalarField,
        left: &TippMippCoreCommitment<PairingOutput<P>, PairingOutput<P>, P::G1>,
        right: &TippMippCoreCommitment<PairingOutput<P>, PairingOutput<P>, P::G1>,
    ) -> Result<P::ScalarField, String> {
        let mut hash_input = Vec::new();
        append_canonical(prior_raw_challenge, &mut hash_input)?;
        append_canonical(&left.ab.0, &mut hash_input)?;
        append_canonical(&left.ab.1, &mut hash_input)?;
        append_singleton(&left.ab.2, &mut hash_input)?;
        append_canonical(&left.c.0, &mut hash_input)?;
        append_singleton(&left.c.1, &mut hash_input)?;
        append_canonical(&right.ab.0, &mut hash_input)?;
        append_canonical(&right.ab.1, &mut hash_input)?;
        append_singleton(&right.ab.2, &mut hash_input)?;
        append_canonical(&right.c.0, &mut hash_input)?;
        append_singleton(&right.c.1, &mut hash_input)?;
        self.derive_challenge(TippMippChallengeStage::Round, &hash_input)
    }

    fn invert_round(&self, challenge: &P::ScalarField) -> Result<P::ScalarField, String> {
        match challenge.inverse() {
            Some(inverse) => Ok(inverse),
            None => Err("round challenge must be non-zero".to_owned()),
        }
    }

    fn fold_gt_commitments(
        &self,
        roots: (
            &PairingOutput<P>,
            &PairingOutput<P>,
            &PairingOutput<P>,
            &PairingOutput<P>,
        ),
        rounds_wire: &[ArkworksTippMippRound<P>],
        inverse_challenges_chrono: &[P::ScalarField],
        raw_challenges_chrono: &[P::ScalarField],
    ) -> (
        PairingOutput<P>,
        PairingOutput<P>,
        PairingOutput<P>,
        PairingOutput<P>,
    ) {
        fold_arkworks_gt_commitments_shared_schedule(
            roots,
            rounds_wire,
            inverse_challenges_chrono,
            raw_challenges_chrono,
        )
    }

    fn derive_final_bridge(
        &mut self,
        last_raw_challenge: &P::ScalarField,
        final_ck: &(P::G2, P::G1),
        final_messages: &(P::G1, P::G2, P::G1),
    ) -> Result<P::ScalarField, String> {
        let mut hash_input = Vec::new();
        append_canonical(last_raw_challenge, &mut hash_input)?;
        append_canonical(&final_ck.0, &mut hash_input)?;
        append_canonical(&final_ck.1, &mut hash_input)?;
        append_canonical(&final_messages.0, &mut hash_input)?;
        append_canonical(&final_messages.1, &mut hash_input)?;
        append_canonical(&final_messages.2, &mut hash_input)?;
        self.derive_challenge(TippMippChallengeStage::FinalBridge, &hash_input)
    }

    fn derive_kzg(
        &mut self,
        final_bridge: &P::ScalarField,
        final_ck: &(P::G2, P::G1),
    ) -> Result<P::ScalarField, String> {
        let mut hash_input = Vec::new();
        append_canonical(final_bridge, &mut hash_input)?;
        append_canonical(&final_ck.0, &mut hash_input)?;
        append_canonical(&final_ck.1, &mut hash_input)?;
        self.derive_challenge(TippMippChallengeStage::Kzg, &hash_input)
    }

    fn invert_randomizer(&self, randomizer: &P::ScalarField) -> Result<P::ScalarField, String> {
        match randomizer.inverse() {
            Some(inverse) => Ok(inverse),
            None => Err("randomizer must be non-zero before inversion".to_owned()),
        }
    }

    fn verify_c(
        &self,
        messages: &[P::G1],
        keys: &[P::G2],
        commitment: &PairingOutput<P>,
    ) -> Result<bool, String> {
        Ok(pairing_inner_product::<P>(messages, keys)? == *commitment)
    }

    fn verify_z(
        &self,
        messages: &[P::G1],
        scalars: &[P::ScalarField],
        commitment: &P::G1,
    ) -> Result<bool, String> {
        Ok(
            MultiexponentiationInnerProduct::<P::G1>::inner_product(messages, scalars)
                .map_err(|error| error.to_string())?
                == *commitment,
        )
    }
}

#[cfg(test)]
fn verify_combined_checks<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &AggregateProof<P, D>,
    r: &P::ScalarField,
) -> Result<(bool, bool), Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    // Timings are observational; the core below returns only the
    // two verifier results and the buffered TIPP/MIPP effect state.

    let input = combined_checks_core_input::<P, D>(pvk, public_inputs, proof, r, ip_verifier_srs)?;
    let mut tipp_trace = BufferedChallengeTraceSink::default();
    let effect = Transcript::<P, D, _> {
        context,
        trace: &mut tipp_trace,
        _pairing: PhantomData,
        _digest: PhantomData,
    };
    let output = verify_combined_checks_core(
        input,
        effect,
        cfg_multi_pairing::<P>,
        multi_pairing_prepared::<P>,
    )
    .map_err(combined_checks_core_error)?;
    let CombinedChecksCoreOutput { checks } = output;
    tipp_trace.replay_into(trace);

    Ok(checks)
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
pub struct AggregateProofData<
    G1: CanonicalSerialize + CanonicalDeserialize,
    G2: CanonicalSerialize + CanonicalDeserialize,
    GT: CanonicalSerialize + CanonicalDeserialize,
    ABT: CanonicalSerialize + CanonicalDeserialize,
    CT: CanonicalSerialize + CanonicalDeserialize,
    D: Send + Sync,
> {
    com_a: GT,
    com_b: GT,
    com_c: GT,
    ip_ab: GT,
    agg_c: G1,
    tipp_mipp_proof: TippMippProofData<G1, G2, GT, ABT, CT, D>,
}

/// Aggregate-proof wire tree with a caller-selected target-group encoding.
///
/// This is the aggregate tree shape; selecting another `GT` changes only the target
/// values stored at its leaves. Protocol-specific codecs must still validate
/// the reconstructed target-group values before verification.
#[doc(hidden)]
pub type AggregateProofWithTarget<P, GT, D> = AggregateProofData<
    <P as Pairing>::G1,
    <P as Pairing>::G2,
    GT,
    IdentityOutput<GT>,
    IdentityOutput<<P as Pairing>::G1>,
    D,
>;

pub type AggregateProof<P, D> = AggregateProofWithTarget<P, PairingOutput<P>, D>;

/// Return target leaves in the same order used by
/// `try_map_aggregate_proof_target`.
#[doc(hidden)]
pub fn aggregate_proof_target_values<P, GT, D>(
    proof: &AggregateProofWithTarget<P, GT, D>,
) -> Vec<&GT>
where
    P: Pairing,
    GT: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    D: Send + Sync,
{
    let rounds = &proof.tipp_mipp_proof.gipa_proof.r_commitment_steps;
    let nested_count = rounds
        .iter()
        .map(|(left, right)| left.ab.2 .0.len() + right.ab.2 .0.len())
        .sum::<usize>();
    let mut values = Vec::with_capacity(4 + rounds.len() * 8 + nested_count);
    values.extend([&proof.com_a, &proof.com_b, &proof.com_c, &proof.ip_ab]);
    for (left, right) in rounds {
        for commitment in [left, right] {
            values.extend([&commitment.ab.0, &commitment.ab.1]);
            values.extend(commitment.ab.2 .0.iter());
            values.push(&commitment.c.0);
        }
    }
    values
}

/// Map every target leaf while preserving the aggregate-proof tree exactly.
///
/// The closure is called in `aggregate_proof_target_values` order so codecs
/// can prepare one batch inversion and consume it during the mapping pass.
#[doc(hidden)]
pub fn try_map_aggregate_proof_target<P, FromGT, ToGT, D, E>(
    proof: &AggregateProofWithTarget<P, FromGT, D>,
    mut map: impl FnMut(&FromGT) -> Result<ToGT, E>,
) -> Result<AggregateProofWithTarget<P, ToGT, D>, E>
where
    P: Pairing,
    FromGT: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    ToGT: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    P::G1: Clone,
    P::G2: Clone,
    D: Send + Sync,
{
    let com_a = map(&proof.com_a)?;
    let com_b = map(&proof.com_b)?;
    let com_c = map(&proof.com_c)?;
    let ip_ab = map(&proof.ip_ab)?;
    let mut rounds = Vec::with_capacity(proof.tipp_mipp_proof.gipa_proof.r_commitment_steps.len());
    for (left, right) in &proof.tipp_mipp_proof.gipa_proof.r_commitment_steps {
        let mut map_commitment = |commitment: &TippMippCommitmentData<
            FromGT,
            IdentityOutput<FromGT>,
            IdentityOutput<P::G1>,
        >| {
            let ab_0 = map(&commitment.ab.0)?;
            let ab_1 = map(&commitment.ab.1)?;
            let mut ab_output = Vec::with_capacity(commitment.ab.2 .0.len());
            for value in &commitment.ab.2 .0 {
                ab_output.push(map(value)?);
            }
            let c_0 = map(&commitment.c.0)?;
            Ok(TippMippCommitmentData {
                ab: (ab_0, ab_1, IdentityOutput(ab_output)),
                c: (c_0, commitment.c.1.clone()),
            })
        };
        rounds.push((map_commitment(left)?, map_commitment(right)?));
    }

    Ok(AggregateProofData {
        com_a,
        com_b,
        com_c,
        ip_ab,
        agg_c: proof.agg_c.clone(),
        tipp_mipp_proof: TippMippProofData {
            gipa_proof: TippMippGipaProofData {
                r_commitment_steps: rounds,
                _digest: PhantomData,
            },
            final_ck: proof.tipp_mipp_proof.final_ck.clone(),
            final_ck_proofs: proof.tipp_mipp_proof.final_ck_proofs.clone(),
            final_messages: proof.tipp_mipp_proof.final_messages.clone(),
            _digest: PhantomData,
        },
    })
}

/// Validate every group element after an aggregate proof was canonically
/// decoded with `Validate::No`. The caller supplies the GT predicate so the
/// shipping BLS12-377 backend can use its proved fast membership kernels.
#[doc(hidden)]
pub fn validate_decoded_aggregate_proof<P, D>(
    proof: &AggregateProof<P, D>,
    mut validate_g1: impl FnMut(&P::G1) -> Result<(), SerializationError>,
    mut validate_g2: impl FnMut(&P::G2) -> Result<(), SerializationError>,
    mut validate_gt: impl FnMut(&PairingOutput<P>) -> Result<(), SerializationError>,
) -> Result<(), SerializationError>
where
    P: Pairing,
    D: Send + Sync,
{
    validate_gt(&proof.com_a)?;
    validate_gt(&proof.com_b)?;
    validate_gt(&proof.com_c)?;
    validate_gt(&proof.ip_ab)?;
    validate_g1(&proof.agg_c)?;

    for (left, right) in &proof.tipp_mipp_proof.gipa_proof.r_commitment_steps {
        for commitment in [left, right] {
            validate_gt(&commitment.ab.0)?;
            validate_gt(&commitment.ab.1)?;
            for value in &(commitment.ab.2).0 {
                validate_gt(value)?;
            }
            validate_gt(&commitment.c.0)?;
            for value in &(commitment.c.1).0 {
                validate_g1(value)?;
            }
        }
    }

    validate_g2(&proof.tipp_mipp_proof.final_ck.0)?;
    validate_g1(&proof.tipp_mipp_proof.final_ck.1)?;
    validate_g2(&proof.tipp_mipp_proof.final_ck_proofs.0)?;
    validate_g1(&proof.tipp_mipp_proof.final_ck_proofs.1)?;
    validate_g1(&proof.tipp_mipp_proof.final_messages.0)?;
    validate_g2(&proof.tipp_mipp_proof.final_messages.1)?;
    validate_g1(&proof.tipp_mipp_proof.final_messages.2)?;
    Ok(())
}

/// Internal post-decode representation of a v1 aggregate proof.
///
/// Construction is restricted to `validate_aggregate_proof`, which projects
/// every wire-level identity commitment to its required singleton scalar.
#[doc(hidden)]
pub struct ValidatedAggregateProofData<G1, G2, GT, D> {
    com_a: GT,
    com_b: GT,
    com_c: GT,
    ip_ab: GT,
    agg_c: G1,
    tipp_mipp_proof: TippMippCoreProof<G1, G2, GT, GT, G1>,
    _digest: PhantomData<D>,
}

#[doc(hidden)]
pub type ValidatedAggregateProof<P, D> =
    ValidatedAggregateProofData<<P as Pairing>::G1, <P as Pairing>::G2, PairingOutput<P>, D>;

/// First-order observation derived from the retained shipping execution.
///
/// The challenge trace is in deployed call order: every randomizer attempt,
/// followed by the chronological TIPP/MIPP queries.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShippingVerifierObservation {
    call_id: AppVerifyCallId,
    accepted: bool,
    challenge_context: ChallengeContext,
}

impl ShippingVerifierObservation {
    #[doc(hidden)]
    pub fn call_id(&self) -> AppVerifyCallId {
        self.call_id
    }

    #[doc(hidden)]
    pub fn accepted(&self) -> bool {
        self.accepted
    }

    #[doc(hidden)]
    pub fn challenge_context(&self) -> &ChallengeContext {
        &self.challenge_context
    }
}

/// Opaque production ownership of the exact semantic execution/result pair.
///
/// Application and backend code can transport this value but cannot forge a
/// replacement acceptance bit or inspect retained execution state.
#[doc(hidden)]
pub struct ShippingVerifierExecutionCarrier<P: Pairing> {
    observation: ShippingVerifierObservation,
    _pairing: PhantomData<fn() -> P>,
}

impl<P: Pairing> Clone for ShippingVerifierExecutionCarrier<P> {
    fn clone(&self) -> Self {
        Self {
            observation: self.observation.clone(),
            _pairing: PhantomData,
        }
    }
}

impl<P: Pairing> fmt::Debug for ShippingVerifierExecutionCarrier<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShippingVerifierExecutionCarrier")
            .finish_non_exhaustive()
    }
}

impl<P: Pairing> ShippingVerifierExecutionCarrier<P> {
    #[doc(hidden)]
    pub fn shipping_observation(&self) -> &ShippingVerifierObservation {
        {
            &self.observation
        }
    }
}

/// Remove the application result wrapper while retaining the exact verifier
/// carrier authenticated by it.
#[doc(hidden)]
pub fn shipping_verifier_executed_result_into_parts<P: Pairing>(
    executed: AppVerifyShippingExecutedResult<ShippingVerifierExecutionCarrier<P>>,
) -> (AppVerifyShippingResult, ShippingVerifierExecutionCarrier<P>) {
    let (shipping_result, backend_result) =
        app_verify_shipping_executed_result_into_parts(executed);
    let (carrier, _) = app_verify_shipping_backend_result_into_parts(backend_result);
    (shipping_result, carrier)
}

pub fn setup_inner_product<P, D, R: Rng>(rng: &mut R, size: usize) -> Result<SRS<P>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let (srs, _) = PairingInnerProductAB::<P, D>::setup(rng, size)?;
    Ok(srs)
}

pub fn aggregate_proofs<P, D>(
    context: &ChallengeContext,
    ip_srs: &SRS<P>,
    proofs: &[Proof<P>],
) -> Result<AggregateProof<P, D>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let mut trace = NoopChallengeTraceSink;
    aggregate_proofs_with_trace(context, &mut trace, ip_srs, proofs)
}

pub fn aggregate_proofs_with_trace<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_srs: &SRS<P>,
    proofs: &[Proof<P>],
) -> Result<AggregateProof<P, D>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    aggregate_proofs_with_trace_and_real_count::<P, D, S>(
        context,
        trace,
        ip_srs,
        proofs,
        proofs.len(),
    )
}

/// Aggregate a canonical repeat-final padded vector while retaining its real
/// prefix length for exact pairing-term coalescing.
pub fn aggregate_proofs_with_real_count<P, D>(
    context: &ChallengeContext,
    ip_srs: &SRS<P>,
    proofs: &[Proof<P>],
    real_count: usize,
) -> Result<AggregateProof<P, D>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let mut trace = NoopChallengeTraceSink;
    aggregate_proofs_with_trace_and_real_count::<P, D, _>(
        context, &mut trace, ip_srs, proofs, real_count,
    )
}

#[doc(hidden)]

pub fn aggregate_proofs_with_trace_and_real_count<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_srs: &SRS<P>,
    proofs: &[Proof<P>],
    real_count: usize,
) -> Result<AggregateProof<P, D>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let source_proofs = ordered_source_proofs_core(proofs);

    let a = &source_proofs.a;
    let b = &source_proofs.b;
    let c = &source_proofs.c;

    let prepared_srs = ip_srs.prepare_for_proving();

    let (ck_1, ck_2) = prepared_srs.commitment_keys();

    let (com_a, com_b, com_c) = initial_commitments::<P>(a, b, c, ck_1, ck_2, real_count)?;

    let mut randomizer_message = Vec::new();
    com_a.serialize_uncompressed(&mut randomizer_message)?;
    com_b.serialize_uncompressed(&mut randomizer_message)?;
    com_c.serialize_uncompressed(&mut randomizer_message)?;
    let r = sample_bounded_challenge::<_, Error, _>(|nonce| {
        Ok(P::ScalarField::from_random_bytes(&challenge_digest::<D, _>(
            context,
            trace,
            b"aggregate.randomizer",
            nonce,
            &randomizer_message,
        ))
        .filter(randomizer_is_admissible))
    })?;

    let r_vec = structured_scalar_power(proofs.len(), &r);

    let b_r = b
        .iter()
        .zip(&r_vec)
        .map(|(&b, r)| b * r)
        .collect::<Vec<P::G2>>();

    let ip_ab = PairingInnerProduct::<P>::inner_product(a, &b_r)?;

    let agg_c = MultiexponentiationInnerProduct::<P::G1>::inner_product(c, &r_vec)?;

    let ck_2_r_inv = build_shifted_ck_2::<P>(ck_2, &r);

    #[cfg(debug_assertions)]
    {
        assert_eq!(
            com_b,
            PairingInnerProduct::<P>::inner_product(&ck_2_r_inv, &b_r)?
        );
    }

    let (proof, tipp_mipp_trace) = prove_tipp_mipp_buffered::<P, D>(
        context,
        &prepared_srs,
        a,
        &b_r,
        c,
        &r_vec,
        ck_1,
        &ck_2_r_inv,
        &r,
        (&com_a, &com_b, &com_c),
        &ip_ab,
        &agg_c,
    )?;

    tipp_mipp_trace.replay_into(trace);

    Ok(proof)
}

fn verify_aggregate_proof_adapter_with_trace<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &ValidatedAggregateProof<P, D>,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let randomizer_message = validated_aggregate_randomizer_message(proof)?;
    // Construct the complete adapter input through the shared root used
    // by the shipping refinement.
    let input = shipping_aggregate_adapter_core_input_validated::<P, D>(
        pvk,
        public_inputs,
        proof,
        &P::ScalarField::zero(),
        ip_verifier_srs,
        randomizer_message,
    );
    let mut tipp_trace = BufferedChallengeTraceSink::default();
    let result = verify_aggregate_adapter_core(
        input,
        |nonce, message: &[u8]| {
            Ok(P::ScalarField::from_random_bytes(
                &challenge_digest::<D, _>(context, trace, b"aggregate.randomizer", nonce, message),
            ))
        },
        Transcript::<P, D, _> {
            context,
            trace: &mut tipp_trace,
            _pairing: PhantomData,
            _digest: PhantomData,
        },
        cfg_multi_pairing::<P>,
        multi_pairing_prepared::<P>,
    )
    .map_err(aggregate_adapter_core_error)?;
    let accepted = result.accepted;
    drop(result);
    tipp_trace.replay_into(trace);
    Ok(accepted)
}

#[doc(hidden)]
pub fn verify_validated_aggregate_proof<P, D>(
    context: &ChallengeContext,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &ValidatedAggregateProof<P, D>,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let mut trace = NoopChallengeTraceSink;
    verify_validated_aggregate_proof_with_trace(
        context,
        &mut trace,
        ip_verifier_srs,
        pvk,
        public_inputs,
        proof,
    )
}

#[doc(hidden)]
pub fn verify_validated_aggregate_proof_with_trace<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &ValidatedAggregateProof<P, D>,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    {
        return verify_aggregate_proof_adapter_with_trace(
            context,
            trace,
            ip_verifier_srs,
            pvk,
            public_inputs,
            proof,
        );
    }
}

pub fn verify_aggregate_proof<P, D>(
    context: &ChallengeContext,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>], //TODO: Should use ToConstraintField instead
    proof: &AggregateProof<P, D>,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let mut trace = NoopChallengeTraceSink;
    verify_aggregate_proof_with_trace(
        context,
        &mut trace,
        ip_verifier_srs,
        pvk,
        public_inputs,
        proof,
    )
}

pub fn verify_aggregate_proof_with_trace<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &AggregateProof<P, D>,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let proof = validate_aggregate_proof(proof)?;
    verify_validated_aggregate_proof_with_trace(
        context,
        trace,
        ip_verifier_srs,
        pvk,
        public_inputs,
        &proof,
    )
}

#[doc(hidden)]
/// Shipping-only verifier entry retaining the exact semantic execution
/// consumed by the application. Its first-order observation is derived from
/// that retained value.
#[doc(hidden)]
pub fn verify_validated_aggregate_proof_shipping<P, D>(
    call_id: AppVerifyCallId,
    context: &ChallengeContext,
    ip_verifier_srs: &VerifierSRS<P>,
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &ValidatedAggregateProof<P, D>,
) -> Result<AppVerifyShippingBackendResult<ShippingVerifierExecutionCarrier<P>>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let accepted =
        verify_validated_aggregate_proof(context, ip_verifier_srs, pvk, public_inputs, proof)?;
    let execution = ShippingVerifierExecutionCarrier {
        observation: ShippingVerifierObservation {
            call_id,
            accepted,
            challenge_context: context.clone(),
        },
        _pairing: PhantomData,
    };
    Ok(app_verify_shipping_backend_result_from_parts(
        execution,
        AppVerifyCallResult {
            id: call_id,
            accepted,
        },
    ))
}

fn build_shifted_ck_2<P: Pairing>(ck_2: &[P::G1], r: &P::ScalarField) -> Vec<P::G1> {
    let inverse_powers = inverse_powers::<P>(ck_2.len(), r);

    #[cfg(feature = "parallel")]
    {
        ck_2.par_iter()
            .zip(inverse_powers.par_iter())
            .map(|(ck, power)| *ck * power)
            .collect()
    }

    #[cfg(not(feature = "parallel"))]
    {
        ck_2.iter()
            .zip(inverse_powers.iter())
            .map(|(ck, power)| *ck * power)
            .collect()
    }
}

fn inverse_powers<P: Pairing>(len: usize, r: &P::ScalarField) -> Vec<P::ScalarField> {
    assert!(!r.is_zero(), "inverse_powers requires nonzero r");
    let r_inv = r.inverse().expect("inverse_powers requires nonzero r");
    inverse_powers_with_inverse(len, &r_inv)
}

fn inverse_powers_with_inverse<F>(len: usize, r_inv: &F) -> Vec<F>
where
    F: Clone + One + std::ops::Mul<Output = F>,
{
    let mut powers = Vec::with_capacity(len);
    let mut current = F::one();
    for _ in 0..len {
        powers.push(current.clone());
        current = current * r_inv.clone();
    }
    powers
}

/// Exact message bound by the aggregate randomizer challenge.
///
/// This is a production-used boundary: the three commitments are serialized
/// uncompressed in `com_a || com_b || com_c` order before stage/context/nonce
/// framing is applied by `challenge_digest`.
#[cfg(test)]
fn aggregate_randomizer_message<P, D>(proof: &AggregateProof<P, D>) -> Result<Vec<u8>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    aggregate_randomizer_message_from_commitments(&proof.com_a, &proof.com_b, &proof.com_c)
}

fn validated_aggregate_randomizer_message<P, D>(
    proof: &ValidatedAggregateProof<P, D>,
) -> Result<Vec<u8>, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    aggregate_randomizer_message_from_commitments(&proof.com_a, &proof.com_b, &proof.com_c)
}

fn aggregate_randomizer_message_from_commitments<GT>(
    com_a: &GT,
    com_b: &GT,
    com_c: &GT,
) -> Result<Vec<u8>, Error>
where
    GT: CanonicalSerialize,
{
    let mut message = Vec::new();
    com_a.serialize_uncompressed(&mut message)?;
    com_b.serialize_uncompressed(&mut message)?;
    com_c.serialize_uncompressed(&mut message)?;
    Ok(message)
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AggregateProofShapeError {
    round_index: usize,
    side: usize,
    component: usize,
    actual_len: usize,
}

impl std::fmt::Display for AggregateProofShapeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let side = if self.side == 0 { "left" } else { "right" };
        let component = if self.component == 0 {
            "AB target"
        } else {
            "C target"
        };
        write!(
            formatter,
            "GIPA round {} {side} {component} identity output must contain exactly one element; got {}",
            self.round_index, self.actual_len
        )
    }
}

impl std::error::Error for AggregateProofShapeError {}

fn project_identity_output<T>(
    output: &IdentityOutput<T>,
    round_index: usize,
    side: usize,
    component: usize,
) -> Result<T, AggregateProofShapeError>
where
    T: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
{
    if output.0.len() != 1 {
        return Err(AggregateProofShapeError {
            round_index,
            side,
            component,
            actual_len: output.0.len(),
        });
    }
    Ok(output.0[0].clone())
}

fn shipping_tipp_mipp_core_proof<G1, G2, GT, D>(
    proof: &AggregateProofData<G1, G2, GT, IdentityOutput<GT>, IdentityOutput<G1>, D>,
) -> Result<TippMippCoreProof<G1, G2, GT, GT, G1>, AggregateProofShapeError>
where
    G1: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    G2: CanonicalSerialize + CanonicalDeserialize + Clone,
    GT: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    D: Send + Sync,
{
    let steps = &proof.tipp_mipp_proof.gipa_proof.r_commitment_steps;
    let mut gipa_proof = Vec::with_capacity(steps.len());
    let mut index = 0usize;
    while index < steps.len() {
        let (left, right) = &steps[index];
        gipa_proof.push((
            TippMippCoreCommitment {
                ab: (
                    left.ab.0.clone(),
                    left.ab.1.clone(),
                    project_identity_output(&left.ab.2, index, 0, 0)?,
                ),
                c: (
                    left.c.0.clone(),
                    project_identity_output(&left.c.1, index, 0, 1)?,
                ),
            },
            TippMippCoreCommitment {
                ab: (
                    right.ab.0.clone(),
                    right.ab.1.clone(),
                    project_identity_output(&right.ab.2, index, 1, 0)?,
                ),
                c: (
                    right.c.0.clone(),
                    project_identity_output(&right.c.1, index, 1, 1)?,
                ),
            },
        ));
        index += 1;
    }
    let tipp_mipp = &proof.tipp_mipp_proof;
    Ok(TippMippCoreProof {
        gipa_proof,
        final_ck: tipp_mipp.final_ck.clone(),
        final_ck_proofs: tipp_mipp.final_ck_proofs.clone(),
        final_messages: tipp_mipp.final_messages.clone(),
    })
}

pub(crate) fn validate_aggregate_proof_data<G1, G2, GT, D>(
    proof: &AggregateProofData<G1, G2, GT, IdentityOutput<GT>, IdentityOutput<G1>, D>,
) -> Result<ValidatedAggregateProofData<G1, G2, GT, D>, AggregateProofShapeError>
where
    G1: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    G2: CanonicalSerialize + CanonicalDeserialize + Clone,
    GT: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq,
    D: Send + Sync,
{
    Ok(ValidatedAggregateProofData {
        com_a: proof.com_a.clone(),
        com_b: proof.com_b.clone(),
        com_c: proof.com_c.clone(),
        ip_ab: proof.ip_ab.clone(),
        agg_c: proof.agg_c.clone(),
        tipp_mipp_proof: shipping_tipp_mipp_core_proof(proof)?,
        _digest: PhantomData,
    })
}

#[doc(hidden)]
pub fn validate_aggregate_proof<P, D>(
    proof: &AggregateProof<P, D>,
) -> Result<ValidatedAggregateProof<P, D>, AggregateProofShapeError>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    validate_aggregate_proof_data(proof)
}

fn shipping_tipp_mipp_core_input<F, G1, G2, GT, D>(
    ip_verifier_srs: &VerifierSRSData<G1, G2>,
    proof: &ValidatedAggregateProofData<G1, G2, GT, D>,
    r: &F,
    one: F,
) -> TippMippCoreInput<F, G1, G2, GT, GT, G1>
where
    F: Clone,
    G1: CanonicalSerialize + CanonicalDeserialize + Clone,
    G2: CanonicalSerialize + CanonicalDeserialize + Clone,
    GT: Clone,
    D: Send + Sync,
{
    TippMippCoreInput {
        com_a: proof.com_a.clone(),
        com_b: proof.com_b.clone(),
        com_t: proof.ip_ab.clone(),
        com_c: proof.com_c.clone(),
        com_z: proof.agg_c.clone(),
        ip_ab: proof.ip_ab.clone(),
        agg_c: proof.agg_c.clone(),
        proof: proof.tipp_mipp_proof.clone(),
        verifier_g: ip_verifier_srs.g.clone(),
        verifier_g_beta: ip_verifier_srs.g_beta.clone(),
        verifier_h: ip_verifier_srs.h.clone(),
        verifier_h_alpha: ip_verifier_srs.h_alpha.clone(),
        r: r.clone(),
        kzg_g2_r_shift: one,
    }
}

/// Pure construction of the complete shipping verifier
/// input after strict aggregate-proof validation.

fn shipping_aggregate_adapter_core_input<F, G1, G2, G2Prepared, GT, D>(
    randomizer_message: Vec<u8>,
    ip_verifier_srs: &VerifierSRSData<G1, G2>,
    proof: &ValidatedAggregateProofData<G1, G2, GT, D>,
    gamma_abc_g1: Vec<G1>,
    public_inputs: Vec<Vec<F>>,
    r: &F,
    one: F,
    alpha_beta: GT,
    gamma_g2_neg_pc: G2Prepared,
    delta_g2_neg_pc: G2Prepared,
) -> AggregateAdapterCoreInput<F, G1, G2, G2Prepared, GT, GT, G1>
where
    F: Clone,
    G1: CanonicalSerialize + CanonicalDeserialize + Clone,
    G2: CanonicalSerialize + CanonicalDeserialize + Clone,
    GT: Clone,
    D: Send + Sync,
{
    let combined = combined_checks_core_input_from_parts(CombinedChecksCoreInputParts {
        tipp_mipp: shipping_tipp_mipp_core_input(ip_verifier_srs, proof, r, one),
        gamma_abc_g1,
        public_inputs,
        r: r.clone(),
        alpha_beta,
        agg_c: proof.agg_c.clone(),
        gamma_g2_neg_pc,
        delta_g2_neg_pc,
        ip_ab: proof.ip_ab.clone(),
    });
    AggregateAdapterCoreInput {
        randomizer_message: randomizer_message,
        combined: combined,
    }
}

fn shipping_aggregate_adapter_core_input_validated<P, D>(
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &ValidatedAggregateProof<P, D>,
    r: &P::ScalarField,
    ip_verifier_srs: &VerifierSRS<P>,
    randomizer_message: Vec<u8>,
) -> AggregateAdapterCoreInput<
    P::ScalarField,
    P::G1,
    P::G2,
    P::G2Prepared,
    PairingOutput<P>,
    PairingOutput<P>,
    P::G1,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let gamma_abc_g1 = pvk
        .vk
        .gamma_abc_g1
        .iter()
        .map(|base| P::G1::from(*base))
        .collect();
    shipping_aggregate_adapter_core_input(
        randomizer_message,
        ip_verifier_srs,
        proof,
        gamma_abc_g1,
        public_inputs.to_vec(),
        r,
        P::ScalarField::one(),
        PairingOutput::<P>(pvk.alpha_g1_beta_g2),
        pvk.gamma_g2_neg_pc.clone(),
        pvk.delta_g2_neg_pc.clone(),
    )
}

#[cfg(test)]
fn tipp_mipp_core_input<P, D>(
    ip_verifier_srs: &VerifierSRS<P>,
    proof: &AggregateProof<P, D>,
    r: &P::ScalarField,
) -> Result<
    TippMippCoreInput<P::ScalarField, P::G1, P::G2, PairingOutput<P>, PairingOutput<P>, P::G1>,
    AggregateProofShapeError,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let proof = validate_aggregate_proof(proof)?;
    Ok(shipping_tipp_mipp_core_input(
        ip_verifier_srs,
        &proof,
        r,
        P::ScalarField::one(),
    ))
}

#[cfg(test)]
fn combined_checks_core_input_validated<P, D>(
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &ValidatedAggregateProof<P, D>,
    r: &P::ScalarField,
    ip_verifier_srs: &VerifierSRS<P>,
) -> CombinedChecksCoreInput<
    P::ScalarField,
    P::G1,
    P::G2,
    P::G2Prepared,
    PairingOutput<P>,
    PairingOutput<P>,
    P::G1,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let gamma_abc_g1 = pvk
        .vk
        .gamma_abc_g1
        .iter()
        .map(|base| P::G1::from(*base))
        .collect();
    combined_checks_core_input_from_parts(CombinedChecksCoreInputParts {
        tipp_mipp: shipping_tipp_mipp_core_input(ip_verifier_srs, proof, r, P::ScalarField::one()),
        gamma_abc_g1,
        public_inputs: public_inputs.to_vec(),
        r: r.clone(),
        alpha_beta: PairingOutput::<P>(pvk.alpha_g1_beta_g2),
        agg_c: proof.agg_c.clone(),
        gamma_g2_neg_pc: pvk.gamma_g2_neg_pc.clone(),
        delta_g2_neg_pc: pvk.delta_g2_neg_pc.clone(),
        ip_ab: proof.ip_ab.clone(),
    })
}

#[cfg(test)]
fn combined_checks_core_input<P, D>(
    pvk: &PreparedVerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    proof: &AggregateProof<P, D>,
    r: &P::ScalarField,
    ip_verifier_srs: &VerifierSRS<P>,
) -> Result<
    CombinedChecksCoreInput<
        P::ScalarField,
        P::G1,
        P::G2,
        P::G2Prepared,
        PairingOutput<P>,
        PairingOutput<P>,
        P::G1,
    >,
    AggregateProofShapeError,
>
where
    P: Pairing,
    D: Digest + Send + Sync,
{
    let proof = validate_aggregate_proof(proof)?;
    Ok(combined_checks_core_input_validated(
        pvk,
        public_inputs,
        &proof,
        r,
        ip_verifier_srs,
    ))
}

fn combined_checks_core_error(error: CombinedChecksError<String>) -> Error {
    match error.kind {
        0 => Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "padded proof count must be a nonzero power of two",
        )),
        1 => Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "combined GIPA round count {} does not match log2(proof count) {}",
                error.actual_rounds, error.expected_rounds
            ),
        )),
        _ => Box::new(std::io::Error::other(
            error
                .tipp_mipp_error
                .expect("TIPP/MIPP error kind must carry an error"),
        )),
    }
}

fn aggregate_adapter_core_error(error: AggregateAdapterCoreError<String>) -> Error {
    match error {
        AggregateAdapterCoreError::RandomizerFailure(error) => {
            Box::new(std::io::Error::other(error))
        }
        AggregateAdapterCoreError::CombinedFailure(error) => combined_checks_core_error(error),
        AggregateAdapterCoreError::NonceExhausted => {
            Box::new(crate::challenge::ChallengeNonceExhausted)
        }
    }
}

#[cfg(test)]
fn verify_tipp_mipp<P, D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    ip_verifier_srs: &VerifierSRS<P>,
    proof: &AggregateProof<P, D>,
    r: &P::ScalarField,
) -> Result<bool, Error>
where
    P: Pairing,
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let input = tipp_mipp_core_input(ip_verifier_srs, proof, r)?;
    let mut effect = Transcript::<P, D, S> {
        context,
        trace,
        _pairing: PhantomData,
        _digest: PhantomData,
    };
    verify_tipp_mipp_core(input, &mut effect, &cfg_multi_pairing::<P>)
        .map_err(|error: String| Box::new(std::io::Error::other(error)) as Error)
}

fn structured_scalar_final_from_raw_transcript_inner<F>(raw_transcript_reversed: &[F], r: &F) -> F
where
    F: Clone + One + std::ops::Add<Output = F> + std::ops::Mul<Output = F>,
{
    let mut power = r.clone();
    let mut product = F::one();
    for i in 0..raw_transcript_reversed.len() {
        let challenge = raw_transcript_reversed[i].clone();
        product = product * (F::one() + (challenge * power.clone()));
        power = power.clone() * power;
    }
    product
}

#[cfg(test)]
fn fold_public_inputs<P: Pairing>(
    vk: &VerifyingKey<P>,
    public_inputs: &[Vec<P::ScalarField>],
    r: &P::ScalarField,
) -> (P::ScalarField, P::G1) {
    let gamma_abc_g1 = vk
        .gamma_abc_g1
        .iter()
        .map(|base| P::G1::from(*base))
        .collect::<Vec<_>>();
    fold_public_inputs_baseline_core(&gamma_abc_g1, public_inputs, r)
}

#[cfg(test)]
fn fold_public_inputs_baseline_core<F, G1>(
    gamma_abc_g1: &[G1],
    public_inputs: &[Vec<F>],
    r: &F,
) -> (F, G1)
where
    F: Clone
        + PartialEq
        + From<u64>
        + One
        + Zero
        + std::ops::Add<Output = F>
        + std::ops::Div<Output = F>
        + std::ops::Mul<Output = F>
        + std::ops::Sub<Output = F>,
    G1: Clone + std::ops::Add<Output = G1> + std::ops::Mul<F, Output = G1>,
{
    assert!(!public_inputs.is_empty(), "public inputs must be non-empty");
    let input_arity = public_inputs[0].len();
    assert_eq!(gamma_abc_g1.len(), input_arity + 1);
    for row in public_inputs {
        assert_eq!(row.len(), input_arity);
    }

    let r_sum = if r.clone() == F::one() {
        F::from(public_inputs.len() as u64)
    } else {
        let mut r_power = F::one();
        for _ in 0..public_inputs.len() {
            r_power = r_power * r.clone();
        }
        (r_power - F::one()) / (r.clone() - F::one())
    };

    let mut r_vec = vec![F::one(); public_inputs.len()];
    for index in 1..public_inputs.len() {
        r_vec[index] = r_vec[index - 1].clone() * r.clone();
    }

    let mut folded_public_inputs = vec![F::zero(); input_arity];
    for (row, coefficient) in public_inputs.iter().zip(&r_vec) {
        for input_index in 0..input_arity {
            let term = row[input_index].clone() * coefficient.clone();
            folded_public_inputs[input_index] = folded_public_inputs[input_index].clone() + term;
        }
    }

    let mut g_ic = gamma_abc_g1[0].clone() * r_sum.clone();
    for input_index in 0..input_arity {
        let term =
            gamma_abc_g1[input_index + 1].clone() * folded_public_inputs[input_index].clone();
        g_ic = g_ic + term;
    }

    (r_sum, g_ic)
}

pub(crate) fn fold_public_inputs_core<F, G1>(
    gamma_abc_g1: &[G1],
    public_inputs: &[Vec<F>],
    r: &F,
) -> (F, G1)
where
    F: Clone
        + PartialEq
        + From<u64>
        + One
        + Zero
        + std::ops::Add<Output = F>
        + std::ops::Div<Output = F>
        + std::ops::Mul<Output = F>
        + std::ops::Sub<Output = F>,
    G1: Clone + std::ops::Add<Output = G1> + std::ops::Mul<F, Output = G1>,
{
    assert!(!public_inputs.is_empty(), "public inputs must be non-empty");
    let input_arity = public_inputs[0].len();
    assert_eq!(gamma_abc_g1.len(), input_arity + 1);
    for row_index in 0..public_inputs.len() {
        assert_eq!(public_inputs[row_index].len(), input_arity);
    }

    let mut folded_public_inputs = vec![F::zero(); input_arity];
    let r_is_one = r.clone() == F::one();
    let mut r_power = F::one();
    for row_index in 0..public_inputs.len() {
        let row = &public_inputs[row_index];
        for input_index in 0..input_arity {
            let term = row[input_index].clone() * r_power.clone();
            folded_public_inputs[input_index] = folded_public_inputs[input_index].clone() + term;
        }
        if !r_is_one {
            r_power = r_power * r.clone();
        }
    }

    let r_sum = if r_is_one {
        F::from(public_inputs.len() as u64)
    } else {
        (r_power - F::one()) / (r.clone() - F::one())
    };

    let mut g_ic = gamma_abc_g1[0].clone() * r_sum.clone();
    for input_index in 0..input_arity {
        let term =
            gamma_abc_g1[input_index + 1].clone() * folded_public_inputs[input_index].clone();
        g_ic = g_ic + term;
    }

    (r_sum, g_ic)
}

fn multi_pairing_prepared<P: Pairing>(
    left: &[P::G1],
    right: &[P::G2Prepared],
) -> Option<PairingOutput<P>> {
    cfg_multi_pairing_g1_affine_g2_prepared::<P>(&P::G1::normalize_batch(left), right)
}

#[derive(Clone)]
struct PreparedPpeVerifierCoreInput<F, G1, G2Prepared, GT> {
    alpha_beta: GT,
    r_sum: F,
    g_ic: G1,
    agg_c: G1,
    gamma_g2_neg_pc: G2Prepared,
    delta_g2_neg_pc: G2Prepared,
    ip_ab: GT,
}

fn verify_ppe_core<F, G1, G2Prepared, GT, E>(
    input: PreparedPpeVerifierCoreInput<F, G1, G2Prepared, GT>,
    pairing: &E,
) -> bool
where
    F: Clone,
    G1: Clone + std::ops::Neg<Output = G1>,
    G2Prepared: Clone,
    GT: Clone + std::ops::Mul<F, Output = GT> + std::ops::Add<Output = GT> + PartialEq,
    E: Fn(&[G1], &[G2Prepared]) -> Option<GT>,
{
    let alpha_beta_rsum = input.alpha_beta * input.r_sum;
    let ip_ab = input.ip_ab;
    let left = [-input.g_ic, -input.agg_c];
    let right = [input.gamma_g2_neg_pc, input.delta_g2_neg_pc];
    match pairing(&left, &right) {
        Some(folded) => alpha_beta_rsum + folded == ip_ab,
        None => false,
    }
}

#[cfg(test)]
fn verify_ppe<P: Pairing>(
    pvk: &PreparedVerifyingKey<P>,
    proof: &AggregateProof<P, impl Digest + Send + Sync>,
    r_sum: &P::ScalarField,
    g_ic: P::G1,
) -> bool {
    verify_ppe_core(
        PreparedPpeVerifierCoreInput {
            alpha_beta: PairingOutput::<P>(pvk.alpha_g1_beta_g2),
            r_sum: r_sum.clone(),
            g_ic,
            agg_c: proof.agg_c.clone(),
            gamma_g2_neg_pc: pvk.gamma_g2_neg_pc.clone(),
            delta_g2_neg_pc: pvk.delta_g2_neg_pc.clone(),
            ip_ab: proof.ip_ab.clone(),
        },
        &multi_pairing_prepared::<P>,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use ark_bls12_377::Bls12_377;
    use ark_bls12_381::Bls12_381;
    use ark_ec::{pairing::Pairing, AdditiveGroup, CurveGroup, PrimeGroup};
    use ark_ff::{Field, UniformRand, Zero};
    use ark_groth16::{prepare_verifying_key, VerifyingKey};
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use ark_std::One;
    use blake2::Blake2b;

    fn assert_padding_pairing_coalescing<P: Pairing>() {
        let g1 = P::G1::generator();
        let g2 = P::G2::generator();
        let left = vec![
            g1 * P::ScalarField::from(2u64),
            g1 * P::ScalarField::from(3u64),
            g1 * P::ScalarField::from(5u64),
            g1 * P::ScalarField::from(5u64),
            g1 * P::ScalarField::from(5u64),
        ];
        let right = vec![
            g2 * P::ScalarField::from(7u64),
            g2 * P::ScalarField::from(11u64),
            g2 * P::ScalarField::from(13u64),
            g2 * P::ScalarField::from(17u64),
            g2 * P::ScalarField::from(19u64),
        ];
        let full = PairingInnerProduct::<P>::inner_product(&left, &right)
            .expect("full pairing inner product must construct");
        let (coalesced_left, coalesced_right) = coalesce_repeated_left_suffix(&left, &right, 3)
            .expect("repeated left suffix must coalesce");
        let coalesced = PairingInnerProduct::<P>::inner_product(&coalesced_left, &coalesced_right)
            .expect("coalesced pairing inner product must construct");
        assert_eq!(coalesced_left.len(), 3);
        assert_eq!(coalesced_right.len(), 3);
        assert_eq!(coalesced, full);

        let repeated_right = vec![
            g2 * P::ScalarField::from(23u64),
            g2 * P::ScalarField::from(29u64),
            g2 * P::ScalarField::from(31u64),
            g2 * P::ScalarField::from(31u64),
            g2 * P::ScalarField::from(31u64),
        ];
        let full_right_repeated = PairingInnerProduct::<P>::inner_product(&left, &repeated_right)
            .expect("full pairing inner product must construct");
        let (coalesced_left, coalesced_right) =
            coalesce_repeated_right_suffix(&left, &repeated_right, 3)
                .expect("repeated right suffix must coalesce");
        let coalesced_right_repeated =
            PairingInnerProduct::<P>::inner_product(&coalesced_left, &coalesced_right)
                .expect("coalesced pairing inner product must construct");
        assert_eq!(coalesced_left.len(), 3);
        assert_eq!(coalesced_right.len(), 3);
        assert_eq!(coalesced_right_repeated, full_right_repeated);

        let mut malformed_left = left.clone();
        malformed_left[4] = g1 * P::ScalarField::from(23u64);
        assert!(coalesce_repeated_left_suffix(&malformed_left, &right, 3).is_none());
        assert!(coalesce_repeated_left_suffix(&left, &right, 0).is_none());
        assert!(coalesce_repeated_left_suffix(&left, &right, left.len()).is_none());
    }

    #[test]
    fn padding_pairing_coalescing_is_exact_on_bls12_381() {
        assert_padding_pairing_coalescing::<Bls12_381>();
    }

    #[test]
    fn padding_pairing_coalescing_is_exact_on_bls12_377() {
        assert_padding_pairing_coalescing::<Bls12_377>();
    }

    fn assert_shared_gt_fold_exact<P: Pairing>() {
        let gt = |value: u64| {
            P::pairing(
                P::G1::generator() * P::ScalarField::from(value),
                P::G2::generator(),
            )
        };
        let round = |offset: u64| {
            (
                TippMippCoreCommitment {
                    ab: (gt(offset), gt(offset + 1), gt(offset + 2)),
                    c: (
                        gt(offset + 3),
                        P::G1::generator() * P::ScalarField::from(offset + 4),
                    ),
                },
                TippMippCoreCommitment {
                    ab: (gt(offset + 5), gt(offset + 6), gt(offset + 7)),
                    c: (
                        gt(offset + 8),
                        P::G1::generator() * P::ScalarField::from(offset + 9),
                    ),
                },
            )
        };
        let roots = (gt(2), gt(3), gt(5), gt(7));
        let all_rounds = vec![
            round(11),
            round(29),
            round(47),
            round(61),
            round(79),
            round(97),
        ];
        let all_inverses = [0, 13, 17, 19, 23, 29].map(P::ScalarField::from).to_vec();
        let all_raw = [31, 37, 0, 41, 43, 47].map(P::ScalarField::from).to_vec();
        let root_refs = (&roots.0, &roots.1, &roots.2, &roots.3);

        for round_count in 0..=all_rounds.len() {
            let rounds_wire = &all_rounds[..round_count];
            let inverses = &all_inverses[..round_count];
            let raw = &all_raw[..round_count];
            assert_eq!(
                fold_arkworks_gt_commitments_shared_schedule(root_refs, rounds_wire, inverses, raw,),
                fold_arkworks_gt_commitments_independent_msm(root_refs, rounds_wire, inverses, raw,),
                "round_count={round_count}",
            );
        }
    }

    #[test]
    fn shared_gt_fold_matches_sequential_bls12_381() {
        assert_shared_gt_fold_exact::<Bls12_381>();
    }

    #[test]
    fn shared_gt_fold_matches_sequential_bls12_377() {
        assert_shared_gt_fold_exact::<Bls12_377>();
    }

    #[test]
    fn shared_prover_round_pairing_preparation_preserves_outputs_and_counts_unique_operands() {
        let mut rng = StdRng::seed_from_u64(47);
        let size = 4usize;
        let a = (0..size)
            .map(|_| <Bls12_377 as Pairing>::G1::rand(&mut rng))
            .collect::<Vec<_>>();
        let b = (0..size)
            .map(|_| <Bls12_377 as Pairing>::G2::rand(&mut rng))
            .collect::<Vec<_>>();
        let c = (0..size)
            .map(|_| <Bls12_377 as Pairing>::G1::rand(&mut rng))
            .collect::<Vec<_>>();
        let ck_v = (0..size)
            .map(|_| <Bls12_377 as Pairing>::G2::rand(&mut rng))
            .collect::<Vec<_>>();
        let ck_w = (0..size)
            .map(|_| <Bls12_377 as Pairing>::G1::rand(&mut rng))
            .collect::<Vec<_>>();

        let expected = ProverRoundPairingOutput {
            com_a: PairingInnerProduct::<Bls12_377>::inner_product(&a, &ck_v)
                .expect("com_a must construct"),
            com_b: PairingInnerProduct::<Bls12_377>::inner_product(&ck_w, &b)
                .expect("com_b must construct"),
            ip_ab: PairingInnerProduct::<Bls12_377>::inner_product(&a, &b)
                .expect("ip_ab must construct"),
            com_c: PairingInnerProduct::<Bls12_377>::inner_product(&c, &ck_v)
                .expect("com_c must construct"),
        };
        let actual = shared_prover_round_pairing_products::<Bls12_377>(&a, &b, &c, &ck_v, &ck_w)
            .expect("shared preparation must construct all products");

        assert_eq!(actual.products, expected);
        let scalars = vec![<Bls12_377 as Pairing>::ScalarField::from(7u64); size];
        let commitment =
            commit_tipp_mipp_round::<Bls12_377>(&a, &b, &c, &scalars, &ck_v, &ck_w).unwrap();
        assert_eq!(commitment.ab.0, expected.com_a);
        assert_eq!(commitment.ab.1, expected.com_b);
        assert_eq!(commitment.ab.2, expected.ip_ab);
        assert_eq!(commitment.c.0, expected.com_c);
        let expected_c = c.iter().zip(&scalars).fold(
            <Bls12_377 as Pairing>::G1::zero(),
            |sum, (point, scalar)| sum + *point * scalar,
        );
        assert_eq!(commitment.c.1, expected_c);
        assert_eq!(actual.preparation.g1_normalization_batches, 3);
        assert_eq!(actual.preparation.g2_normalization_batches, 2);
        assert_eq!(actual.preparation.g1_normalized_elements, 3 * size);
        assert_eq!(actual.preparation.g2_normalized_elements, 2 * size);
        assert_eq!(actual.preparation.g1_prepared_elements, 3 * size);
        assert_eq!(actual.preparation.g2_prepared_elements, 2 * size);
    }

    #[test]
    fn shared_prover_round_pairing_preparation_preserves_shape_failure_order() {
        type P = Bls12_377;

        for (a_len, b_len, c_len, ck_v_len, ck_w_len) in [
            (0, 0, 0, 0, 0),
            (2, 1, 1, 1, 1),
            (1, 1, 1, 1, 2),
            (2, 1, 1, 2, 1),
            (1, 1, 2, 1, 1),
        ] {
            let a = vec![<P as Pairing>::G1::default(); a_len];
            let b = vec![<P as Pairing>::G2::default(); b_len];
            let c = vec![<P as Pairing>::G1::default(); c_len];
            let ck_v = vec![<P as Pairing>::G2::default(); ck_v_len];
            let ck_w = vec![<P as Pairing>::G1::default(); ck_w_len];

            let reference = (|| -> Result<(), String> {
                let _ = PairingInnerProduct::<P>::inner_product(&a, &ck_v)
                    .map_err(|error| error.to_string())?;
                let _ = PairingInnerProduct::<P>::inner_product(&ck_w, &b)
                    .map_err(|error| error.to_string())?;
                let _ = PairingInnerProduct::<P>::inner_product(&a, &b)
                    .map_err(|error| error.to_string())?;
                let _ = PairingInnerProduct::<P>::inner_product(&c, &ck_v)
                    .map_err(|error| error.to_string())?;
                Ok(())
            })()
            .expect_err("the reference shape must fail");
            let shared = match shared_prover_round_pairing_products::<P>(&a, &b, &c, &ck_v, &ck_w) {
                Ok(_) => panic!("the shared shape must fail"),
                Err(error) => error,
            };

            assert_eq!(shared, reference);
        }
    }

    #[test]
    fn prover_gipa_folds_match_independent_weighted_sums() {
        type P = Bls12_377;
        type F = <P as Pairing>::ScalarField;
        let g1 = <P as Pairing>::G1::generator();
        let g2 = <P as Pairing>::G2::generator();
        let input = ProverGipaCoreInput {
            a: (1..=4).map(|n| g1 * F::from(n)).collect(),
            b: (10..=13).map(|n| g2 * F::from(n)).collect(),
            c: (5..=8).map(|n| g1 * F::from(n)).collect(),
            public_values: (2..=5).map(F::from).collect(),
            ck_v: (11..=14).map(|n| g2 * F::from(n)).collect(),
            ck_w: (21..=24).map(|n| g1 * F::from(n)).collect(),
            randomizer: F::from(7u64),
            com_a: PairingOutput::<P>::zero(),
            com_b: PairingOutput::<P>::zero(),
            com_c: PairingOutput::<P>::zero(),
            ip_ab: PairingOutput::<P>::zero(),
            agg_c: g1,
        };
        let context = ChallengeContext::from_statement_digest([17; 32]);
        let mut trace = NoopChallengeTraceSink;
        let mut transcript = Transcript::<P, Blake2b, _> {
            context: &context,
            trace: &mut trace,
            _pairing: PhantomData,
            _digest: PhantomData,
        };
        let output = prove_tipp_mipp_gipa_core(input.clone(), &mut transcript).unwrap();
        let raw = &output.raw_transcript_wire;
        let inv = &output.inv_transcript_wire;
        assert_eq!(raw.len(), 2);
        assert_eq!(output.rounds_wire.len(), 2);
        assert_eq!(output.last_raw_challenge, raw[0]);
        for (r, i) in raw.iter().zip(inv) {
            assert_eq!(*r * i, F::one());
        }
        let raw_weights = [F::one(), raw[0], raw[1], raw[0] * raw[1]];
        let inv_weights = [F::one(), inv[0], inv[1], inv[0] * inv[1]];
        let sum_g1 = |points: &[<P as Pairing>::G1], weights: &[F]| {
            points
                .iter()
                .zip(weights)
                .fold(<P as Pairing>::G1::zero(), |sum, (point, weight)| {
                    sum + *point * weight
                })
        };
        let sum_g2 = |points: &[<P as Pairing>::G2], weights: &[F]| {
            points
                .iter()
                .zip(weights)
                .fold(<P as Pairing>::G2::zero(), |sum, (point, weight)| {
                    sum + *point * weight
                })
        };
        assert_eq!(output.final_messages.0, sum_g1(&input.a, &inv_weights));
        assert_eq!(output.final_messages.1, sum_g2(&input.b, &raw_weights));
        assert_eq!(output.final_messages.2, sum_g1(&input.c, &inv_weights));
        assert_eq!(output.final_ck.0, sum_g2(&input.ck_v, &raw_weights));
        assert_eq!(output.final_ck.1, sum_g1(&input.ck_w, &inv_weights));
        assert_eq!(
            output.final_public_value,
            input
                .public_values
                .iter()
                .zip(raw_weights)
                .map(|(value, weight)| *value * weight)
                .sum()
        );
    }

    #[derive(Debug)]
    struct ScriptedAdapterRandomizer<F> {
        candidates: Vec<Result<Option<F>, String>>,
        next: usize,
        nonces: Vec<u64>,
        messages: Vec<Vec<u8>>,
    }

    impl<F: Clone> ScriptedAdapterRandomizer<F> {
        fn new(candidates: Vec<Result<Option<F>, String>>) -> Self {
            Self {
                candidates,
                next: 0,
                nonces: Vec::new(),
                messages: Vec::new(),
            }
        }
    }

    impl<F: Clone> ScriptedAdapterRandomizer<F> {
        fn derive_randomizer(
            &mut self,
            nonce: u64,
            randomizer_message: &[u8],
        ) -> Result<Option<F>, String> {
            self.nonces.push(nonce);
            self.messages.push(randomizer_message.to_vec());
            let result = self.candidates[self.next].clone();
            self.next += 1;
            result
        }
    }

    #[test]
    fn scalar_identity_serialization_matches_arkworks_singleton_vector() {
        type P = Bls12_381;

        let gt = PairingOutput::<P>::generator();
        let mut scalar_abt = Vec::new();
        append_singleton(&gt, &mut scalar_abt).expect("scalar AB target must serialize");
        let mut wrapped_abt = Vec::new();
        IdentityOutput(vec![gt])
            .serialize_uncompressed(&mut wrapped_abt)
            .expect("singleton AB target must serialize");
        assert_eq!(scalar_abt, wrapped_abt);
        assert_eq!(&scalar_abt[..8], &1u64.to_le_bytes());

        let g1 = <P as Pairing>::G1::generator();
        let mut scalar_ct = Vec::new();
        append_singleton(&g1, &mut scalar_ct).expect("scalar C target must serialize");
        let mut wrapped_ct = Vec::new();
        IdentityOutput(vec![g1])
            .serialize_uncompressed(&mut wrapped_ct)
            .expect("singleton C target must serialize");
        assert_eq!(scalar_ct, wrapped_ct);
        assert_eq!(&scalar_ct[..8], &1u64.to_le_bytes());
    }

    fn zero_combined_inputs<P: Pairing>() -> (
        VerifierSRS<P>,
        PreparedVerifyingKey<P>,
        AggregateProof<P, Blake2b>,
    ) {
        let g1 = P::G1::zero();
        let g2 = P::G2::zero();
        let vk = VerifyingKey {
            alpha_g1: g1.into(),
            beta_g2: g2.into(),
            gamma_g2: g2.into(),
            delta_g2: g2.into(),
            gamma_abc_g1: vec![g1.into()],
        };
        (
            VerifierSRSData {
                g: g1,
                h: g2,
                g_beta: g1,
                h_alpha: g2,
            },
            prepare_verifying_key(&vk),
            zero_tipp_mipp_proof(),
        )
    }

    #[test]
    fn tipp_mipp_challenge_prefix_exposes_exact_shipping_values() {
        type P = Bls12_381;

        let (srs, _, mut proof) = zero_combined_inputs::<P>();
        let gt = PairingOutput::<P>::zero();
        let g1 = <P as Pairing>::G1::zero();
        let side = TippMippCommitment {
            ab: (gt, gt, IdentityOutput(vec![gt])),
            c: (gt, IdentityOutput(vec![g1])),
        };
        proof
            .tipp_mipp_proof
            .gipa_proof
            .r_commitment_steps
            .push((side.clone(), side.clone()));
        proof
            .tipp_mipp_proof
            .gipa_proof
            .r_commitment_steps
            .push((side.clone(), side));
        let randomizer = <P as Pairing>::ScalarField::from(7u64);
        let input = tipp_mipp_core_input(&srs, &proof, &randomizer)
            .expect("zero fixture has singleton identity outputs");
        let context = ChallengeContext::from_statement_digest([0u8; 32]);
        let mut trace = crate::challenge::VecChallengeTraceSink::default();
        let prefix = {
            let mut effect = Transcript::<P, Blake2b, _> {
                context: &context,
                trace: &mut trace,
                _pairing: PhantomData,
                _digest: PhantomData,
            };
            let prefix = verify_tipp_mipp_challenge_prefix_core(&input, &mut effect)
                .expect("zero fixture challenge prefix must construct");
            prefix
        };

        let sampled = trace
            .entries()
            .iter()
            .filter_map(|entry| {
                <P as Pairing>::ScalarField::from_random_bytes(&entry.digest)
                    .filter(|candidate| !candidate.is_zero())
            })
            .collect::<Vec<_>>();
        assert_eq!(sampled.len(), 5);
        assert_eq!(prefix.challenges.x0, sampled[0]);
        assert_eq!(
            prefix.challenges.round_challenges_chrono,
            vec![sampled[1], sampled[2]]
        );
        assert_eq!(
            prefix.challenges.round_challenges_wire,
            vec![sampled[2], sampled[1]]
        );
        assert_eq!(prefix.challenges.final_bridge, sampled[3]);
        assert_eq!(prefix.challenges.kzg, sampled[4]);
        assert_eq!(
            prefix.inverse_challenges_reversed,
            vec![
                sampled[2].inverse().expect("sampled challenge is nonzero"),
                sampled[1].inverse().expect("sampled challenge is nonzero"),
            ]
        );
        assert_eq!(
            prefix.randomizer_inverse,
            randomizer.inverse().expect("fixture randomizer is nonzero")
        );

        assert_eq!(
            trace
                .entries()
                .iter()
                .map(|entry| entry.stage_label)
                .collect::<Vec<_>>(),
            vec![
                b"tipp-mipp.x0".as_slice(),
                b"tipp-mipp.gipa.round".as_slice(),
                b"tipp-mipp.gipa.round".as_slice(),
                b"tipp-mipp.final-bridge".as_slice(),
                b"tipp-mipp.kzg".as_slice(),
            ]
        );
    }

    #[test]
    fn validated_aggregate_proof_is_scalar_and_cannot_be_bypassed() {
        type P = Bls12_381;

        let (srs, pvk, mut proof) = zero_combined_inputs::<P>();
        let gt = PairingOutput::<P>::generator();
        let g1 = <P as Pairing>::G1::generator();
        proof.tipp_mipp_proof.gipa_proof.r_commitment_steps.push((
            TippMippCommitment {
                ab: (gt, gt, IdentityOutput(vec![gt])),
                c: (gt, IdentityOutput(vec![g1])),
            },
            TippMippCommitment {
                ab: (gt, gt, IdentityOutput(vec![gt])),
                c: (gt, IdentityOutput(vec![g1])),
            },
        ));

        let validated = validate_aggregate_proof_data(&proof)
            .expect("explicit scalar projection root must accept singleton proof");
        let projected_round = &validated.tipp_mipp_proof.gipa_proof[0];
        assert_eq!(projected_round.0.ab.2, gt);
        assert_eq!(projected_round.0.c.1, g1);
        assert_eq!(projected_round.1.ab.2, gt);
        assert_eq!(projected_round.1.c.1, g1);
        assert_eq!(
            aggregate_randomizer_message(&proof).expect("wire commitments must serialize"),
            validated_aggregate_randomizer_message(&validated)
                .expect("validated commitments must serialize")
        );

        let r = <P as Pairing>::ScalarField::from(7u64);
        let projected =
            combined_checks_core_input_validated(&pvk, &[vec![], vec![]], &validated, &r, &srs);
        assert_eq!(projected.tipp_mipp.proof.gipa_proof[0].0.ab.2, gt);
        assert_eq!(projected.tipp_mipp.proof.gipa_proof[0].0.c.1, g1);

        proof.tipp_mipp_proof.gipa_proof.r_commitment_steps[0]
            .0
            .ab
            .2 = IdentityOutput(Vec::new());
        assert!(
            validate_aggregate_proof(&proof).is_err(),
            "raw vector proof must not bypass validated construction"
        );
        assert_eq!(
            projected.tipp_mipp.proof.gipa_proof[0].0.ab.2, gt,
            "the validated verifier input must retain only its scalar projection"
        );
    }

    #[test]
    fn malformed_identity_outputs_reject_before_transcript_or_folding() {
        type P = Bls12_381;

        let (srs, pvk, base_proof) = zero_combined_inputs::<P>();
        let context = ChallengeContext::from_statement_digest([0u8; 32]);
        let public_inputs = vec![vec![], vec![]];
        let gt = PairingOutput::<P>::zero();
        let g1 = <P as Pairing>::G1::zero();

        let malformed = [
            (
                "zero",
                IdentityOutput(Vec::new()),
                IdentityOutput(vec![g1]),
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1]),
            ),
            (
                "multi",
                IdentityOutput(vec![gt, gt]),
                IdentityOutput(vec![g1]),
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1]),
            ),
            (
                "mixed-left-c",
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1, g1]),
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1]),
            ),
            (
                "mixed-right-ab",
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1]),
                IdentityOutput(Vec::new()),
                IdentityOutput(vec![g1]),
            ),
            (
                "mixed-right-c",
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1]),
                IdentityOutput(vec![gt]),
                IdentityOutput(vec![g1, g1]),
            ),
        ];

        for (case, left_ab, left_c, right_ab, right_c) in malformed {
            let mut proof = base_proof.clone();
            proof.tipp_mipp_proof.gipa_proof.r_commitment_steps.push((
                TippMippCommitment {
                    ab: (gt, gt, left_ab),
                    c: (gt, left_c),
                },
                TippMippCommitment {
                    ab: (gt, gt, right_ab),
                    c: (gt, right_c),
                },
            ));

            assert!(
                validate_aggregate_proof(&proof).is_err(),
                "{} identity output bypassed validated construction",
                case
            );

            let mut trace = crate::challenge::VecChallengeTraceSink::default();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                verify_aggregate_proof_with_trace::<P, Blake2b, _>(
                    &context,
                    &mut trace,
                    &srs,
                    &pvk,
                    &public_inputs,
                    &proof,
                )
            }));

            assert!(outcome.is_ok(), "{} identity output panicked", case);
            let error = outcome
                .expect("verification must not panic")
                .expect_err("malformed identity output must reject");
            assert!(
                error.to_string().contains("identity output"),
                "unexpected {} error: {}",
                case,
                error
            );
            assert!(
                trace.entries().is_empty(),
                "{} identity output reached challenge hashing",
                case
            );
        }
    }

    #[test]
    fn prover_structural_cores_preserve_fold_and_round_field_order() {
        assert_eq!(
            fold_vector_core(&[2u64, 3, 5], &[7u64, 11], &13u64),
            vec![33, 50]
        );
        assert_eq!(reverse_chronological_core(vec![2u64, 3, 5]), vec![5, 3, 2]);

        type P = Bls12_381;
        let gt = PairingOutput::<P>::generator();
        let g1 = <P as Pairing>::G1::generator();
        let commitment = TippMippCommitmentData {
            ab: (gt, gt.double(), IdentityOutput(vec![gt.double().double()])),
            c: (
                gt.double().double().double(),
                IdentityOutput(vec![g1.double()]),
            ),
        };
        assert_eq!(commitment.ab.0, gt);
        assert_eq!(commitment.ab.1, gt.double());
        assert_eq!(commitment.ab.2 .0, vec![gt.double().double()]);
        assert_eq!(commitment.c.0, gt.double().double().double());
        assert_eq!(commitment.c.1 .0, vec![g1.double()]);
    }

    fn assert_aggregate_prover_determinism<P: Pairing>() {
        let mut rng = StdRng::seed_from_u64(0x5eed_7072_6f76_6572);
        let srs =
            setup_inner_product::<P, Blake2b, _>(&mut rng, 2).expect("prover SRS must construct");
        let g1 = P::G1::generator().into_affine();
        let g2 = P::G2::generator().into_affine();
        let proofs = vec![
            Proof {
                a: g1,
                b: g2,
                c: g1,
            },
            Proof {
                a: g1,
                b: g2,
                c: g1,
            },
        ];
        let context = ChallengeContext::from_statement_digest([0x42u8; 32]);

        let mut normal_trace = crate::challenge::VecChallengeTraceSink::default();
        let normal = aggregate_proofs_with_trace::<P, Blake2b, _>(
            &context,
            &mut normal_trace,
            &srs,
            &proofs,
        )
        .expect("normal prover must construct");

        let mut repeated_trace = crate::challenge::VecChallengeTraceSink::default();
        let repeated = aggregate_proofs_with_trace::<P, Blake2b, _>(
            &context,
            &mut repeated_trace,
            &srs,
            &proofs,
        )
        .expect("repeated prover must construct");

        let mut normal_bytes = Vec::new();
        normal
            .serialize_uncompressed(&mut normal_bytes)
            .expect("normal proof must serialize");
        let mut repeated_bytes = Vec::new();
        repeated
            .serialize_uncompressed(&mut repeated_bytes)
            .expect("repeated proof must serialize");

        assert_eq!(normal_bytes, repeated_bytes);
        assert_eq!(normal_trace.entries(), repeated_trace.entries());
        assert_eq!(
            normal_trace
                .entries()
                .iter()
                .map(|entry| (entry.stage_label, entry.nonce))
                .collect::<Vec<_>>(),
            vec![
                (b"aggregate.randomizer".as_slice(), 0),
                (b"tipp-mipp.x0".as_slice(), 0),
                (b"tipp-mipp.gipa.round".as_slice(), 0),
                (b"tipp-mipp.final-bridge".as_slice(), 0),
                (b"tipp-mipp.kzg".as_slice(), 0),
                (b"tipp-mipp.kzg".as_slice(), 1),
            ]
        );
        assert!(normal_trace.entries().iter().all(|entry| entry.nonce == 0));
    }

    #[test]
    fn aggregate_prover_preserves_proof_bytes_and_trace() {
        assert_aggregate_prover_determinism::<Bls12_381>();
    }

    #[test]
    fn combined_checks_schedule_branch_preserves_core_acceptance() {
        let (srs, pvk, proof) = zero_combined_inputs::<Bls12_381>();
        let context = ChallengeContext::from_statement_digest([0u8; 32]);
        let r = <Bls12_381 as Pairing>::ScalarField::from(2u64);
        let public_inputs = vec![vec![]];

        let mut combined_trace = crate::challenge::VecChallengeTraceSink::default();
        let combined = verify_combined_checks::<Bls12_381, Blake2b, _>(
            &context,
            &mut combined_trace,
            &srs,
            &pvk,
            &public_inputs,
            &proof,
            &r,
        )
        .expect("zero combined fixture should reach both checks");

        let mut tipp_trace = crate::challenge::VecChallengeTraceSink::default();
        let tipp =
            verify_tipp_mipp::<Bls12_381, Blake2b, _>(&context, &mut tipp_trace, &srs, &proof, &r)
                .expect("zero TIPP/MIPP fixture should verify");

        assert_eq!(combined.0, tipp);
        assert_eq!(combined_trace.entries(), tipp_trace.entries());
    }

    #[test]
    fn combined_checks_rejects_malformed_counts_before_downstream_calls() {
        let (srs, pvk, proof) = zero_combined_inputs::<Bls12_381>();
        let context = ChallengeContext::from_statement_digest([0u8; 32]);
        let cases = [
            (
                Vec::new(),
                "padded proof count must be a nonzero power of two",
            ),
            (
                vec![vec![], vec![], vec![]],
                "padded proof count must be a nonzero power of two",
            ),
            (
                vec![vec![], vec![]],
                "combined GIPA round count 0 does not match log2(proof count) 1",
            ),
        ];

        for (public_inputs, expected) in cases {
            let mut trace = NoopChallengeTraceSink;
            let error = verify_combined_checks::<Bls12_381, Blake2b, _>(
                &context,
                &mut trace,
                &srs,
                &pvk,
                &public_inputs,
                &proof,
                &<Bls12_381 as Pairing>::ScalarField::from(2u64),
            )
            .expect_err("malformed count must be rejected");
            assert_eq!(error.to_string(), expected);
        }
    }

    fn zero_tipp_mipp_proof<P: Pairing>() -> AggregateProof<P, Blake2b> {
        let g1 = P::G1::zero();
        let g2 = P::G2::zero();
        AggregateProof {
            com_a: PairingOutput::<P>::zero(),
            com_b: PairingOutput::<P>::zero(),
            com_c: PairingOutput::<P>::zero(),
            ip_ab: PairingOutput::<P>::zero(),
            agg_c: g1,
            tipp_mipp_proof: TippMippProof {
                gipa_proof: TippMippGipaProof {
                    r_commitment_steps: Vec::new(),
                    _digest: PhantomData,
                },
                final_ck: (g2, g1),
                final_ck_proofs: (g2, g1),
                final_messages: (g1, g2, g1),
                _digest: PhantomData,
            },
        }
    }

    fn run_scripted_adapter_core(
        candidates: Vec<Result<Option<<Bls12_381 as Pairing>::ScalarField>, String>>,
        nonce: u64,
        malformed_combined_input: bool,
    ) -> Result<
        (
            <Bls12_381 as Pairing>::ScalarField,
            (bool, bool),
            bool,
            Vec<u64>,
            Vec<Vec<u8>>,
        ),
        AggregateAdapterCoreError<String>,
    > {
        let (srs, pvk, proof) = zero_combined_inputs::<Bls12_381>();
        let mut combined = combined_checks_core_input(
            &pvk,
            &[vec![]],
            &proof,
            &<Bls12_381 as Pairing>::ScalarField::zero(),
            &srs,
        )
        .expect("zero fixture has singleton identity outputs");
        if malformed_combined_input {
            combined.public_inputs.clear();
        }
        let randomizer_message = vec![0x41, 0x52, 0x4b];
        let context = ChallengeContext::from_statement_digest([0u8; 32]);
        let mut tipp_trace = NoopChallengeTraceSink;
        let mut randomizer = ScriptedAdapterRandomizer::new(candidates);
        let output = verify_aggregate_adapter_core_from_nonce(
            AggregateAdapterCoreInput {
                randomizer_message: randomizer_message,
                combined: combined,
            },
            |nonce, message: &[u8]| randomizer.derive_randomizer(nonce, message),
            Transcript::<Bls12_381, Blake2b, _> {
                context: &context,
                trace: &mut tipp_trace,
                _pairing: PhantomData,
                _digest: PhantomData,
            },
            cfg_multi_pairing::<Bls12_381>,
            multi_pairing_prepared::<Bls12_381>,
            nonce,
        )?;
        Ok((
            output.randomizer,
            output.checks,
            output.accepted,
            randomizer.nonces,
            randomizer.messages,
        ))
    }

    #[test]
    fn aggregate_adapter_input_constructors_preserve_every_field() {
        let tipp = TippMippCoreInput {
            com_a: 10u64,
            com_b: 11,
            com_t: 12,
            com_c: 13,
            com_z: 14,
            ip_ab: 15,
            agg_c: 16,
            proof: TippMippCoreProof {
                gipa_proof: vec![(
                    TippMippCoreCommitment {
                        ab: (17, 18, 19),
                        c: (20, 21),
                    },
                    TippMippCoreCommitment {
                        ab: (22, 23, 24),
                        c: (25, 26),
                    },
                )],
                final_ck: (27, 28),
                final_ck_proofs: (29, 30),
                final_messages: (31, 32, 33),
            },
            verifier_g: 34,
            verifier_g_beta: 35,
            verifier_h: 36,
            verifier_h_alpha: 37,
            r: 38,
            kzg_g2_r_shift: 39,
        };
        assert_eq!(tipp.com_a, 10);
        assert_eq!(tipp.com_b, 11);
        assert_eq!(tipp.com_t, 12);
        assert_eq!(tipp.com_c, 13);
        assert_eq!(tipp.com_z, 14);
        assert_eq!(tipp.ip_ab, 15);
        assert_eq!(tipp.agg_c, 16);
        assert_eq!(tipp.proof.gipa_proof[0].0.ab, (17, 18, 19));
        assert_eq!(tipp.proof.gipa_proof[0].1.c, (25, 26));
        assert_eq!(tipp.proof.final_ck, (27, 28));
        assert_eq!(tipp.proof.final_ck_proofs, (29, 30));
        assert_eq!(tipp.proof.final_messages, (31, 32, 33));
        assert_eq!(tipp.verifier_g, 34);
        assert_eq!(tipp.verifier_g_beta, 35);
        assert_eq!(tipp.verifier_h, 36);
        assert_eq!(tipp.verifier_h_alpha, 37);
        assert_eq!(tipp.r, 38);
        assert_eq!(tipp.kzg_g2_r_shift, 39);

        let combined = combined_checks_core_input_from_parts(CombinedChecksCoreInputParts {
            tipp_mipp: tipp,
            gamma_abc_g1: vec![40, 41],
            public_inputs: vec![vec![42, 43]],
            r: 44,
            alpha_beta: 45,
            agg_c: 46,
            gamma_g2_neg_pc: 47,
            delta_g2_neg_pc: 48,
            ip_ab: 49,
        });
        assert_eq!(combined.gamma_abc_g1, vec![40, 41]);
        assert_eq!(combined.public_inputs, vec![vec![42, 43]]);
        assert_eq!(combined.r, 44);
        assert_eq!(combined.ppe.alpha_beta, 45);
        assert_eq!(combined.ppe.agg_c, 46);
        assert_eq!(combined.ppe.gamma_g2_neg_pc, 47);
        assert_eq!(combined.ppe.delta_g2_neg_pc, 48);
        assert_eq!(combined.ppe.ip_ab, 49);

        let adapter = AggregateAdapterCoreInput {
            randomizer_message: vec![0x41, 0x52, 0x4b],
            combined: combined,
        };
        assert_eq!(adapter.randomizer_message, vec![0x41, 0x52, 0x4b]);
        assert_eq!(adapter.combined.tipp_mipp.com_a, 10);
        assert_eq!(adapter.combined.ppe.ip_ab, 49);
    }

    #[test]
    fn shipping_nonce_exhaustion_maps_exact_public_error() {
        let verifier_error =
            aggregate_adapter_core_error(AggregateAdapterCoreError::NonceExhausted);
        assert!(verifier_error
            .downcast_ref::<ChallengeNonceExhausted>()
            .is_some());
        assert_eq!(
            verifier_error.to_string(),
            ChallengeNonceExhausted.to_string()
        );
    }

    #[test]
    fn prover_and_verifier_share_randomizer_admissibility() {
        type Scalar = <Bls12_381 as Pairing>::ScalarField;

        assert!(!randomizer_is_admissible(&Scalar::zero()));
        assert!(!randomizer_is_admissible(&Scalar::one()));
        assert!(randomizer_is_admissible(&Scalar::from(2u64)));
    }

    #[test]
    fn aggregate_adapter_core_retries_and_installs_one_randomizer() {
        let scalar = |value| <Bls12_381 as Pairing>::ScalarField::from(value);
        let (randomizer, checks, accepted, nonces, messages) = run_scripted_adapter_core(
            vec![
                Ok(None),
                Ok(Some(scalar(0))),
                Ok(Some(scalar(1))),
                Ok(Some(scalar(7))),
            ],
            0,
            false,
        )
        .expect("the fourth candidate must reach the combined core");

        assert_eq!(randomizer, scalar(7));
        assert_eq!(accepted, checks.0 && checks.1);
        assert_eq!(nonces, vec![0, 1, 2, 3]);
        assert_eq!(messages, vec![vec![0x41, 0x52, 0x4b]; 4]);
    }

    #[test]
    fn aggregate_adapter_core_queries_last_nonce_and_fails_closed() {
        let error = run_scripted_adapter_core(vec![Ok(None)], u64::MAX, false)
            .expect_err("rejection at the last nonce must exhaust");
        assert!(matches!(error, AggregateAdapterCoreError::NonceExhausted));
    }

    #[test]
    fn aggregate_adapter_core_accepts_success_at_last_nonce() {
        let scalar = <Bls12_381 as Pairing>::ScalarField::from(7u64);
        let (randomizer, checks, accepted, nonces, _) =
            run_scripted_adapter_core(vec![Ok(Some(scalar))], u64::MAX, false)
                .expect("success at the last nonce must be accepted");
        assert_eq!(randomizer, scalar);
        assert_eq!(accepted, checks.0 && checks.1);
        assert_eq!(nonces, vec![u64::MAX]);
    }

    #[test]
    fn aggregate_adapter_core_preserves_error_order_and_kind() {
        let randomizer_error =
            run_scripted_adapter_core(vec![Err("randomizer".to_owned())], 0, true)
                .expect_err("randomizer failure must precede malformed combined input");
        assert!(matches!(
            randomizer_error,
            AggregateAdapterCoreError::RandomizerFailure(ref error)
                if error == "randomizer"
        ));

        let combined_error = run_scripted_adapter_core(
            vec![Ok(Some(<Bls12_381 as Pairing>::ScalarField::from(7u64)))],
            0,
            true,
        )
        .expect_err("malformed combined input must retain its error kind");
        assert!(matches!(
            combined_error,
            AggregateAdapterCoreError::CombinedFailure(_)
        ));
    }

    #[test]
    fn aggregate_randomizer_message_is_exact_commitment_order() {
        let mut proof = zero_tipp_mipp_proof::<Bls12_381>();
        let generator = PairingOutput::<Bls12_381>::generator();
        proof.com_a = generator;
        proof.com_b = generator * <Bls12_381 as Pairing>::ScalarField::from(2u64);
        proof.com_c = generator * <Bls12_381 as Pairing>::ScalarField::from(3u64);

        let actual = aggregate_randomizer_message(&proof).expect("commitments must serialize");
        let mut expected = Vec::new();
        proof
            .com_a
            .serialize_uncompressed(&mut expected)
            .expect("com_a must serialize");
        proof
            .com_b
            .serialize_uncompressed(&mut expected)
            .expect("com_b must serialize");
        proof
            .com_c
            .serialize_uncompressed(&mut expected)
            .expect("com_c must serialize");
        assert_eq!(actual, expected);

        let component_len = expected.len() / 3;
        assert_eq!(&actual[..component_len], &expected[..component_len]);
        assert_eq!(
            &actual[component_len..2 * component_len],
            &expected[component_len..2 * component_len]
        );
        assert_eq!(&actual[2 * component_len..], &expected[2 * component_len..]);
    }

    #[test]
    fn combined_input_projection_preserves_shipping_inputs() {
        let (srs, pvk, proof) = zero_combined_inputs::<Bls12_381>();
        let r = <Bls12_381 as Pairing>::ScalarField::from(7u64);
        let public_inputs = vec![vec![]];
        let input = combined_checks_core_input(&pvk, &public_inputs, &proof, &r, &srs)
            .expect("zero fixture has singleton identity outputs");

        assert_eq!(input.public_inputs, public_inputs);
        assert_eq!(input.r, r);
        assert_eq!(input.gamma_abc_g1.len(), pvk.vk.gamma_abc_g1.len());
        assert_eq!(input.ppe.alpha_beta, PairingOutput(pvk.alpha_g1_beta_g2));
        assert_eq!(input.ppe.agg_c, proof.agg_c);
        assert_eq!(input.ppe.gamma_g2_neg_pc, pvk.gamma_g2_neg_pc);
        assert_eq!(input.ppe.delta_g2_neg_pc, pvk.delta_g2_neg_pc);
        assert_eq!(input.ppe.ip_ab, proof.ip_ab);
        assert_eq!(input.tipp_mipp.r, r);
        assert_eq!(input.tipp_mipp.verifier_g, srs.g);
        assert_eq!(input.tipp_mipp.verifier_g_beta, srs.g_beta);
        assert_eq!(input.tipp_mipp.verifier_h, srs.h);
        assert_eq!(input.tipp_mipp.verifier_h_alpha, srs.h_alpha);
    }

    fn assert_tipp_mipp_delegator_core_parity<P: Pairing>(
        srs: &VerifierSRS<P>,
        proof: &AggregateProof<P, Blake2b>,
        r: P::ScalarField,
    ) {
        let context = ChallengeContext::from_statement_digest([0u8; 32]);
        let mut delegated_trace = NoopChallengeTraceSink;
        let delegated =
            verify_tipp_mipp::<P, Blake2b, _>(&context, &mut delegated_trace, srs, proof, &r);

        let input = tipp_mipp_core_input(srs, proof, &r)
            .expect("test fixture has singleton identity outputs");
        let mut core_trace = NoopChallengeTraceSink;
        let mut effect = Transcript::<P, Blake2b, _> {
            context: &context,
            trace: &mut core_trace,
            _pairing: PhantomData,
            _digest: PhantomData,
        };
        let core = verify_tipp_mipp_core(input, &mut effect, &cfg_multi_pairing::<P>);

        match (delegated, core) {
            (Ok(delegated), Ok(core)) => assert_eq!(delegated, core),
            (Err(_), Err(_)) => {}
            (delegated, core) => panic!("delegator/core mismatch: {:?} vs {:?}", delegated, core),
        }
    }

    #[test]
    fn verify_tipp_mipp_delegator_core_parity_success() {
        let srs = VerifierSRS::<Bls12_381> {
            g: <Bls12_381 as Pairing>::G1::zero(),
            h: <Bls12_381 as Pairing>::G2::zero(),
            g_beta: <Bls12_381 as Pairing>::G1::zero(),
            h_alpha: <Bls12_381 as Pairing>::G2::zero(),
        };
        let proof = zero_tipp_mipp_proof::<Bls12_381>();
        assert_tipp_mipp_delegator_core_parity(
            &srs,
            &proof,
            <Bls12_381 as Pairing>::ScalarField::from(2u64),
        );
    }

    #[test]
    fn verify_tipp_mipp_delegator_core_parity_randomizer_failure() {
        let srs = VerifierSRS::<Bls12_381> {
            g: <Bls12_381 as Pairing>::G1::zero(),
            h: <Bls12_381 as Pairing>::G2::zero(),
            g_beta: <Bls12_381 as Pairing>::G1::zero(),
            h_alpha: <Bls12_381 as Pairing>::G2::zero(),
        };
        let proof = zero_tipp_mipp_proof::<Bls12_381>();
        assert_tipp_mipp_delegator_core_parity(
            &srs,
            &proof,
            <Bls12_381 as Pairing>::ScalarField::zero(),
        );
    }

    #[test]
    fn verify_tipp_mipp_delegator_core_parity_base_commitment_failure() {
        let srs = VerifierSRS::<Bls12_381> {
            g: <Bls12_381 as Pairing>::G1::zero(),
            h: <Bls12_381 as Pairing>::G2::zero(),
            g_beta: <Bls12_381 as Pairing>::G1::zero(),
            h_alpha: <Bls12_381 as Pairing>::G2::zero(),
        };
        let mut proof = zero_tipp_mipp_proof::<Bls12_381>();
        proof.com_a = PairingOutput::<Bls12_381>(<Bls12_381 as Pairing>::TargetField::one());
        assert_tipp_mipp_delegator_core_parity(
            &srs,
            &proof,
            <Bls12_381 as Pairing>::ScalarField::from(2u64),
        );
    }

    fn assert_fold_public_inputs_parity<P: Pairing>(rows: usize, r: P::ScalarField) {
        let g1 = P::G1::generator();
        let vk: VerifyingKey<P> = VerifyingKey {
            alpha_g1: g1.into(),
            beta_g2: P::G2::generator().into(),
            gamma_g2: P::G2::generator().into(),
            delta_g2: P::G2::generator().into(),
            gamma_abc_g1: vec![
                g1.into(),
                (g1 * P::ScalarField::from(2u64)).into(),
                (g1 * P::ScalarField::from(3u64)).into(),
            ],
        };
        let public_inputs = (0..rows)
            .map(|row| {
                vec![
                    P::ScalarField::from((row as u64) + 1),
                    P::ScalarField::from((row as u64) + 2),
                ]
            })
            .collect::<Vec<_>>();

        let delegated = fold_public_inputs(&vk, &public_inputs, &r);
        let gamma_abc_g1 = vk
            .gamma_abc_g1
            .iter()
            .map(|base| P::G1::from(*base))
            .collect::<Vec<_>>();
        let core = fold_public_inputs_core(&gamma_abc_g1, &public_inputs, &r);

        assert_eq!(delegated, core);
    }

    #[test]
    fn fold_public_inputs_core_parity_r_one_one_row() {
        assert_fold_public_inputs_parity::<Bls12_381>(
            1,
            <Bls12_381 as Pairing>::ScalarField::one(),
        );
    }

    #[test]
    fn fold_public_inputs_core_parity_r_one_multiple_rows() {
        assert_fold_public_inputs_parity::<Bls12_381>(
            3,
            <Bls12_381 as Pairing>::ScalarField::one(),
        );
    }

    #[test]
    fn fold_public_inputs_core_parity_r_not_one_one_row() {
        assert_fold_public_inputs_parity::<Bls12_381>(
            1,
            <Bls12_381 as Pairing>::ScalarField::from(2u64),
        );
    }

    #[test]
    fn fold_public_inputs_core_parity_r_not_one_multiple_rows() {
        assert_fold_public_inputs_parity::<Bls12_381>(
            3,
            <Bls12_381 as Pairing>::ScalarField::from(2u64),
        );
    }

    #[test]
    fn fold_public_inputs_core_matches_baseline_across_shapes() {
        type P = Bls12_381;
        type F = <P as Pairing>::ScalarField;
        let generator = <P as Pairing>::G1::generator();

        for rows in [1usize, 2, 3, 8, 48] {
            for arity in [0usize, 1, 4] {
                let gamma_abc_g1 = (0..=arity)
                    .map(|index| generator * F::from(index as u64 + 1))
                    .collect::<Vec<_>>();
                let public_inputs = (0..rows)
                    .map(|row| {
                        (0..arity)
                            .map(|input| F::from((row * (arity + 1) + input + 1) as u64))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();

                for r in [F::one(), F::from(2u64), F::from(7u64)] {
                    assert_eq!(
                        fold_public_inputs_core(&gamma_abc_g1, &public_inputs, &r),
                        fold_public_inputs_baseline_core(&gamma_abc_g1, &public_inputs, &r),
                        "rows={rows}, arity={arity}, r={r}",
                    );
                }
            }
        }
    }

    #[test]
    fn fold_public_inputs_core_preserves_shape_rejections() {
        type P = Bls12_381;
        type F = <P as Pairing>::ScalarField;
        let generator = <P as Pairing>::G1::generator();
        let r = F::from(2u64);

        let malformed = [
            (vec![generator], Vec::<Vec<F>>::new()),
            (vec![generator], vec![vec![F::one()]]),
            (vec![generator, generator], vec![vec![F::one()], Vec::new()]),
        ];
        for (gamma_abc_g1, public_inputs) in malformed {
            let baseline = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fold_public_inputs_baseline_core(&gamma_abc_g1, &public_inputs, &r)
            }));
            let streamed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fold_public_inputs_core(&gamma_abc_g1, &public_inputs, &r)
            }));
            assert!(baseline.is_err());
            assert!(streamed.is_err());
        }
    }

    fn assert_prepared_ppe_adapter_parity<P: Pairing>() {
        let g1 = P::G1::generator();
        let g2 = P::G2::generator();
        let vk = VerifyingKey {
            alpha_g1: g1.into(),
            beta_g2: g2.into(),
            gamma_g2: g2.into(),
            delta_g2: g2.into(),
            gamma_abc_g1: vec![g1.into()],
        };
        let pvk = prepare_verifying_key(&vk);
        let tipp_mipp_proof = TippMippProof::<P, Blake2b> {
            gipa_proof: TippMippGipaProof::<P, Blake2b> {
                r_commitment_steps: Vec::new(),
                _digest: PhantomData,
            },
            final_ck: (g2, g1),
            final_ck_proofs: (g2, g1),
            final_messages: (g1, g2, g1),
            _digest: PhantomData,
        };
        let proof = AggregateProof {
            com_a: PairingOutput::<P>::zero(),
            com_b: PairingOutput::<P>::zero(),
            com_c: PairingOutput::<P>::zero(),
            ip_ab: PairingOutput::<P>::zero(),
            agg_c: g1,
            tipp_mipp_proof,
        };
        let r_sum = P::ScalarField::from(3u64);
        let g_ic = g1;
        let delegated = verify_ppe(&pvk, &proof, &r_sum, g_ic);
        let input = PreparedPpeVerifierCoreInput {
            alpha_beta: PairingOutput::<P>(pvk.alpha_g1_beta_g2),
            r_sum: r_sum.clone(),
            g_ic,
            agg_c: proof.agg_c,
            gamma_g2_neg_pc: pvk.gamma_g2_neg_pc.clone(),
            delta_g2_neg_pc: pvk.delta_g2_neg_pc.clone(),
            ip_ab: proof.ip_ab,
        };
        let core = verify_ppe_core(input.clone(), &multi_pairing_prepared::<P>);
        assert_eq!(delegated, core);
        assert!(!delegated);
        assert!(!verify_ppe_core(
            input,
            &|_: &[P::G1], _: &[P::G2Prepared]| None::<PairingOutput<P>>
        ));
    }

    struct RecordingPreparedPairingEffect<P: Pairing> {
        left: std::cell::RefCell<Option<Vec<P::G1>>>,
        right: std::cell::RefCell<Option<Vec<P::G2Prepared>>>,
        output: std::cell::RefCell<Option<PairingOutput<P>>>,
    }

    impl<P: Pairing> Default for RecordingPreparedPairingEffect<P> {
        fn default() -> Self {
            Self {
                left: std::cell::RefCell::new(None),
                right: std::cell::RefCell::new(None),
                output: std::cell::RefCell::new(None),
            }
        }
    }

    impl<P: Pairing> RecordingPreparedPairingEffect<P> {
        fn multi_pairing_prepared(
            &self,
            left: &[P::G1],
            right: &[P::G2Prepared],
        ) -> Option<PairingOutput<P>> {
            self.left.replace(Some(left.to_vec()));
            self.right.replace(Some(right.to_vec()));
            let output =
                cfg_multi_pairing_g1_affine_g2_prepared::<P>(&P::G1::normalize_batch(left), right)?;
            self.output.replace(Some(output.clone()));
            Some(output)
        }
    }

    fn assert_prepared_ppe_matches_three_pair_baseline<P: Pairing>() {
        let g1 = <P as Pairing>::G1::generator();
        let g2 = <P as Pairing>::G2::generator();
        let vk: VerifyingKey<P> = VerifyingKey {
            alpha_g1: g1.into(),
            beta_g2: g2.into(),
            gamma_g2: g2.into(),
            delta_g2: g2.into(),
            gamma_abc_g1: vec![g1.into()],
        };
        let pvk = prepare_verifying_key(&vk);
        let r_sum = <P as Pairing>::ScalarField::from(3u64);
        let g_ic = g1;
        let agg_c = g1 * <P as Pairing>::ScalarField::from(5u64);
        let baseline_gt = cfg_multi_pairing::<P>(
            &[<P as Pairing>::G1::from(vk.alpha_g1) * r_sum, g_ic, agg_c],
            &[
                <P as Pairing>::G2::from(vk.beta_g2),
                <P as Pairing>::G2::from(vk.gamma_g2),
                <P as Pairing>::G2::from(vk.delta_g2),
            ],
        )
        .expect("test pairing should be available");
        let input = PreparedPpeVerifierCoreInput {
            alpha_beta: PairingOutput::<P>(pvk.alpha_g1_beta_g2),
            r_sum,
            g_ic,
            agg_c,
            gamma_g2_neg_pc: pvk.gamma_g2_neg_pc.clone(),
            delta_g2_neg_pc: pvk.delta_g2_neg_pc.clone(),
            ip_ab: baseline_gt.clone(),
        };
        let effect = RecordingPreparedPairingEffect::<P>::default();
        assert!(verify_ppe_core(
            input,
            &|left: &[P::G1], right: &[P::G2Prepared]| effect.multi_pairing_prepared(left, right)
        ));

        assert_eq!(effect.left.borrow().as_ref().unwrap(), &vec![-g_ic, -agg_c]);
        assert_eq!(effect.right.borrow().as_ref().unwrap().len(), 2);
        let expected_prepared_gt = cfg_multi_pairing_g1_affine_g2_prepared::<P>(
            &P::G1::normalize_batch(&[-g_ic, -agg_c]),
            &[pvk.gamma_g2_neg_pc, pvk.delta_g2_neg_pc],
        )
        .expect("test prepared pairing should be available");
        assert_eq!(
            effect.output.borrow().as_ref().unwrap(),
            &expected_prepared_gt
        );
        let alpha_beta_rsum = PairingOutput::<P>(pvk.alpha_g1_beta_g2) * r_sum;
        let optimized_gt = alpha_beta_rsum + effect.output.borrow().as_ref().unwrap().clone();
        assert_eq!(optimized_gt, baseline_gt);
    }

    #[test]
    fn prepared_ppe_matches_three_pair_baseline_operands_and_gt() {
        assert_prepared_ppe_matches_three_pair_baseline::<Bls12_381>();
        assert_prepared_ppe_matches_three_pair_baseline::<Bls12_377>();
    }

    #[test]
    fn prepared_ppe_adapter_parity_bls12_381_including_pairing_failure() {
        assert_prepared_ppe_adapter_parity::<Bls12_381>();
    }

    #[test]
    fn prepared_ppe_adapter_parity_bls12_377_including_pairing_failure() {
        assert_prepared_ppe_adapter_parity::<Bls12_377>();
    }

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
    fn shifted_ck_2_matches_per_element_inversion() {
        let mut rng = StdRng::seed_from_u64(11);
        let r = <Bls12_381 as Pairing>::ScalarField::rand(&mut rng);
        let ck_2 = (0..16)
            .map(|_| <Bls12_381 as Pairing>::G1::rand(&mut rng))
            .collect::<Vec<_>>();

        let optimized = build_shifted_ck_2::<Bls12_381>(&ck_2, &r);
        let expected = ck_2
            .iter()
            .enumerate()
            .map(|(idx, ck)| *ck * r.pow([idx as u64]).inverse().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(optimized, expected);
    }
}
