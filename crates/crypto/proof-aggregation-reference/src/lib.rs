#![forbid(unsafe_code)]

//! Dev-only independent SnarkPack reference path.
//!
//! This crate stays outside `shieldd-sdk-proof-aggregation` so Cargo prevents
//! it from importing private production aggregation modules. It re-implements
//! the slow aggregation equations and decodes production bytes into
//! reference-owned proof structs.

use std::ops::{Add, MulAssign};

use ark_ec::{
    pairing::{Pairing, PairingOutput},
    AffineRepr, CurveGroup, PrimeGroup, VariableBaseMSM,
};
use ark_ff::{Field, One, Zero};
use ark_groth16::{PreparedVerifyingKey, Proof, VerifyingKey};
use ark_poly::polynomial::{univariate::DensePolynomial, DenseUVPolynomial};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use blake2::Blake2b;
use decaf377::{Bls12_377, Fq};
use digest::Digest;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use shieldd_sdk_proof_aggregation::{
    encode_wrapped_aggregate_proof, preflight_aggregate_verify, srs_id, AggregatePreflightInput,
    AggregateStatement, DevSrs, ProofFamilyId, DEFAULT_MAX_PADDED_PROOF_COUNT,
};
use shieldd_sdk_proof_params::batch::BatchItem;

type P = Bls12_377;
type G1 = <P as Pairing>::G1;
type G2 = <P as Pairing>::G2;
type Fr = <P as Pairing>::ScalarField;

const DEV_SRS_SEED: [u8; 32] = [0x50; 32];
const CHALLENGE_DOMAIN: &[u8] = b"shieldd.snarkpack.challenge\0";

pub type ReferenceResult<T> = Result<T, ReferencePathError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceAggregate {
    pub wrapped_proof: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceVerifyReport {
    pub accepted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReferencePathError {
    #[error("invalid reference input: {0}")]
    InvalidInput(String),
    #[error("malformed reference proof bytes: {0}")]
    MalformedProof(String),
    #[error("reference verification rejected: {0}")]
    Rejected(String),
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize, Default, Eq, PartialEq)]
struct IdentityOutput<T>(Vec<T>)
where
    T: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq;

impl<T> Add for IdentityOutput<T>
where
    T: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq + Add<T, Output = T>,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        IdentityOutput(
            self.0
                .into_iter()
                .zip(rhs.0)
                .map(|(left, right)| left + right)
                .collect(),
        )
    }
}

impl<T> MulAssign<Fr> for IdentityOutput<T>
where
    T: CanonicalSerialize + CanonicalDeserialize + Clone + Default + Eq + MulAssign<Fr>,
{
    fn mul_assign(&mut self, rhs: Fr) {
        for value in &mut self.0 {
            value.mul_assign(rhs);
        }
    }
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceAggregateProof {
    com_a: PairingOutput<P>,
    com_b: PairingOutput<P>,
    com_c: PairingOutput<P>,
    ip_ab: PairingOutput<P>,
    agg_c: G1,
    tipp_mipp_proof: ReferenceTippMippProof,
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceTippMippProof {
    gipa_proof: ReferenceTippMippGipaProof,
    final_ck: (G2, G1),
    final_ck_proofs: (G2, G1),
    final_messages: (G1, G2, G1),
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceTippMippGipaProof {
    r_commitment_steps: Vec<(TippMippCommitment, TippMippCommitment)>,
}

type AbCommitmentTriple = (
    PairingOutput<P>,
    PairingOutput<P>,
    IdentityOutput<PairingOutput<P>>,
);
type CCommitmentPair = (PairingOutput<P>, IdentityOutput<G1>);

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct TippMippCommitment {
    ab: AbCommitmentTriple,
    c: CCommitmentPair,
}

#[derive(Clone)]
struct ReferenceSrs {
    g_alpha_powers: Vec<G1>,
    h_beta_powers: Vec<G2>,
    g_beta: G1,
    h_alpha: G2,
}

#[derive(Clone)]
struct ReferenceVerifierSrs {
    g: G1,
    h: G2,
    g_beta: G1,
    h_alpha: G2,
}

#[derive(Clone)]
struct TippMippAux {
    raw_transcript: Vec<Fr>,
    inv_transcript: Vec<Fr>,
    last_raw_challenge: Fr,
    final_ck: (G2, G1),
    final_messages: (G1, G2, G1),
}

pub fn reference_aggregate_family(
    statement: &AggregateStatement,
    _pvk: &PreparedVerifyingKey<P>,
    items: &[BatchItem],
    srs: &DevSrs,
) -> ReferenceResult<ReferenceAggregate> {
    if items.is_empty() {
        return Err(ReferencePathError::InvalidInput(
            "cannot aggregate an empty proof family".to_owned(),
        ));
    }
    ensure_reference_srs_matches_public_id(srs)?;

    let reference_srs = reference_srs_for_count(srs, items.len())?;
    let aggregate = reference_aggregate_proofs(
        statement.family_id(),
        statement.challenge_context().as_bytes(),
        &reference_srs,
        &items
            .iter()
            .map(|item| item.proof.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut inner_proof_bytes = Vec::new();
    aggregate
        .serialize_compressed(&mut inner_proof_bytes)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    let wrapped_proof =
        encode_wrapped_aggregate_proof(statement.statement_digest(), &inner_proof_bytes)
            .map_err(|err| ReferencePathError::InvalidInput(err.to_string()))?;

    Ok(ReferenceAggregate { wrapped_proof })
}

pub fn reference_verify_family_aggregate(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<P>,
    wrapped_proof: &[u8],
    srs: &DevSrs,
) -> ReferenceResult<ReferenceVerifyReport> {
    reference_verify_family_aggregate_inner(statement, pvk, wrapped_proof, srs)
}

fn reference_verify_family_aggregate_inner(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<P>,
    wrapped_proof: &[u8],
    srs: &DevSrs,
) -> ReferenceResult<ReferenceVerifyReport> {
    ensure_reference_srs_matches_public_id(srs)?;
    let call = preflight_aggregate_verify(AggregatePreflightInput {
        statement,
        pvk,
        aggregate_proof_bytes: wrapped_proof,
        srs,
    })
    .map_err(|err| ReferencePathError::Rejected(err.to_string()))?;

    let aggregate = ReferenceAggregateProof::deserialize_compressed(call.inner_proof_bytes())
        .map_err(|err| {
            ReferencePathError::MalformedProof(format!("aggregate proof decode failed: {err}"))
        })?;
    let verifier_srs = reference_verifier_srs(call.srs())?;
    let accepted = reference_verify_aggregate_proof(
        call.family_id(),
        call.challenge_context().as_bytes(),
        &verifier_srs,
        &call.pvk().vk,
        call.padded_public_inputs(),
        &aggregate,
    )?;

    Ok(ReferenceVerifyReport { accepted })
}

fn reference_aggregate_proofs(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    srs: &ReferenceSrs,
    proofs: &[Proof<P>],
) -> ReferenceResult<ReferenceAggregateProof> {
    let a = proofs
        .iter()
        .map(|proof| proof.a.into_group())
        .collect::<Vec<_>>();
    let b = proofs
        .iter()
        .map(|proof| proof.b.into_group())
        .collect::<Vec<_>>();
    let c = proofs
        .iter()
        .map(|proof| proof.c.into_group())
        .collect::<Vec<_>>();

    let (ck_1, ck_2) = srs.commitment_keys();
    let com_a = pairing_inner_product(&a, &ck_1)?;
    let com_b = pairing_inner_product(&ck_2, &b)?;
    let com_c = pairing_inner_product(&c, &ck_1)?;

    let r = derive_randomizer(family_id, context, &com_a, &com_b, &com_c)?;
    let r_vec = structured_scalar_power(proofs.len(), &r);
    let b_r = b
        .iter()
        .zip(&r_vec)
        .map(|(point, scalar)| *point * scalar)
        .collect::<Vec<_>>();
    let ip_ab = pairing_inner_product(&a, &b_r)?;
    let agg_c = multiexp_inner_product(&c, &r_vec)?;
    let ck_2_r_inv = build_shifted_ck_2(&ck_2, &r);

    let tipp_mipp_proof = prove_tipp_mipp(
        family_id,
        context,
        srs,
        (&a, &b_r, &c, &r_vec),
        (&ck_1, &ck_2_r_inv),
        &r,
        (&com_a, &com_b, &com_c),
        &ip_ab,
        &agg_c,
    )?;

    Ok(ReferenceAggregateProof {
        com_a,
        com_b,
        com_c,
        ip_ab,
        agg_c,
        tipp_mipp_proof,
    })
}

fn reference_verify_aggregate_proof(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    verifier_srs: &ReferenceVerifierSrs,
    vk: &VerifyingKey<P>,
    public_inputs: &[Vec<Fq>],
    proof: &ReferenceAggregateProof,
) -> ReferenceResult<bool> {
    let r = derive_randomizer(family_id, context, &proof.com_a, &proof.com_b, &proof.com_c)?;
    let tipp_mipp_valid = verify_tipp_mipp(family_id, context, verifier_srs, proof, &r)?;
    let (r_sum, g_ic) = fold_public_inputs(vk, public_inputs, &r);
    let ppe_valid = verify_ppe(vk, proof, &r_sum, g_ic);
    Ok(tipp_mipp_valid && ppe_valid)
}

fn prove_tipp_mipp(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    srs: &ReferenceSrs,
    values: (&[G1], &[G2], &[G1], &[Fr]),
    ck: (&[G2], &[G1]),
    r: &Fr,
    com: (&PairingOutput<P>, &PairingOutput<P>, &PairingOutput<P>),
    ip_ab: &PairingOutput<P>,
    agg_c: &G1,
) -> ReferenceResult<ReferenceTippMippProof> {
    let (gipa_proof, aux) =
        prove_tipp_mipp_gipa(family_id, context, values, ck, r, com, ip_ab, agg_c)?;
    let r_inverse = r
        .inverse()
        .ok_or_else(|| ReferencePathError::Rejected("zero randomizer".to_owned()))?;
    let final_bridge = derive_final_bridge(
        family_id,
        context,
        &aux.last_raw_challenge,
        &aux.final_ck,
        &aux.final_messages,
    )?;
    let z = derive_tipp_mipp_kzg_challenge(family_id, context, &final_bridge, &aux.final_ck)?;
    let ck_v_opening =
        prove_commitment_key_kzg_opening(&srs.h_beta_powers, &aux.raw_transcript, &Fr::one(), &z)?;
    let ck_w_opening =
        prove_commitment_key_kzg_opening(&srs.g_alpha_powers, &aux.inv_transcript, &r_inverse, &z)?;

    Ok(ReferenceTippMippProof {
        gipa_proof,
        final_ck: aux.final_ck,
        final_ck_proofs: (ck_v_opening, ck_w_opening),
        final_messages: aux.final_messages,
    })
}

fn prove_tipp_mipp_gipa(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    values: (&[G1], &[G2], &[G1], &[Fr]),
    ck: (&[G2], &[G1]),
    r: &Fr,
    com: (&PairingOutput<P>, &PairingOutput<P>, &PairingOutput<P>),
    ip_ab: &PairingOutput<P>,
    agg_c: &G1,
) -> ReferenceResult<(ReferenceTippMippGipaProof, TippMippAux)> {
    let mut m_a = values.0.to_vec();
    let mut m_b = values.1.to_vec();
    let mut m_c = values.2.to_vec();
    let mut m_r = values.3.to_vec();
    let mut ck_v = ck.0.to_vec();
    let mut ck_w = ck.1.to_vec();
    let mut r_commitment_steps = Vec::new();
    let mut raw_transcript = Vec::new();
    let mut inv_transcript = Vec::new();
    let mut prior_raw = derive_x0(family_id, context, r, com, ip_ab, agg_c)?;
    let mut last_raw = prior_raw;
    loop {
        if m_a.len() == 1 {
            break;
        }
        let split = m_a.len() / 2;
        let com_1 = commit_tipp_mipp_round(
            &m_a[split..],
            &m_b[..split],
            &m_c[split..],
            &m_r[..split],
            &ck_v[..split],
            &ck_w[split..],
        )?;
        let com_2 = commit_tipp_mipp_round(
            &m_a[..split],
            &m_b[split..],
            &m_c[..split],
            &m_r[split..],
            &ck_v[split..],
            &ck_w[..split],
        )?;
        let raw = derive_tipp_mipp_round_challenge(family_id, context, &prior_raw, &com_1, &com_2)?;
        let inv = raw
            .inverse()
            .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))?;

        m_a = rescale_fold(&m_a[split..], &m_a[..split], &inv);
        m_b = rescale_fold(&m_b[split..], &m_b[..split], &raw);
        m_c = rescale_fold(&m_c[split..], &m_c[..split], &inv);
        m_r = rescale_fold(&m_r[split..], &m_r[..split], &raw);
        ck_v = rescale_fold(&ck_v[split..], &ck_v[..split], &raw);
        ck_w = rescale_fold(&ck_w[split..], &ck_w[..split], &inv);
        r_commitment_steps.push((com_1, com_2));
        raw_transcript.push(raw);
        inv_transcript.push(inv);
        prior_raw = raw;
        last_raw = raw;
    }
    r_commitment_steps.reverse();
    raw_transcript.reverse();
    inv_transcript.reverse();
    Ok((
        ReferenceTippMippGipaProof { r_commitment_steps },
        TippMippAux {
            raw_transcript,
            inv_transcript,
            last_raw_challenge: last_raw,
            final_ck: (ck_v[0], ck_w[0]),
            final_messages: (m_a[0], m_b[0], m_c[0]),
        },
    ))
}

fn verify_tipp_mipp(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    verifier_srs: &ReferenceVerifierSrs,
    proof: &ReferenceAggregateProof,
    r: &Fr,
) -> ReferenceResult<bool> {
    let tipp_mipp = &proof.tipp_mipp_proof;
    let mut com_a = proof.com_a;
    let mut com_b = proof.com_b;
    let mut com_t = IdentityOutput(vec![proof.ip_ab]);
    let mut com_c = proof.com_c;
    let mut com_z = IdentityOutput(vec![proof.agg_c]);
    let mut prior_raw = derive_x0(
        family_id,
        context,
        r,
        (&proof.com_a, &proof.com_b, &proof.com_c),
        &proof.ip_ab,
        &proof.agg_c,
    )?;
    let mut last_raw = prior_raw;
    let mut raw_transcript = Vec::new();
    let mut inv_transcript = Vec::new();

    for (com_1, com_2) in tipp_mipp.gipa_proof.r_commitment_steps.iter().rev() {
        let raw = derive_tipp_mipp_round_challenge(family_id, context, &prior_raw, com_1, com_2)?;
        let inv = raw
            .inverse()
            .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))?;
        com_a = scale(com_1.ab.0, &inv) + com_a + scale(com_2.ab.0, &raw);
        com_b = scale(com_1.ab.1, &inv) + com_b + scale(com_2.ab.1, &raw);
        com_t = scale(com_1.ab.2.clone(), &inv) + com_t + scale(com_2.ab.2.clone(), &raw);
        com_c = scale(com_1.c.0, &inv) + com_c + scale(com_2.c.0, &raw);
        com_z = scale(com_1.c.1.clone(), &inv) + com_z + scale(com_2.c.1.clone(), &raw);
        raw_transcript.push(raw);
        inv_transcript.push(inv);
        prior_raw = raw;
        last_raw = raw;
    }
    raw_transcript.reverse();
    inv_transcript.reverse();

    let final_bridge = derive_final_bridge(
        family_id,
        context,
        &last_raw,
        &tipp_mipp.final_ck,
        &tipp_mipp.final_messages,
    )?;
    let z = derive_tipp_mipp_kzg_challenge(family_id, context, &final_bridge, &tipp_mipp.final_ck)?;
    let r_inverse = r
        .inverse()
        .ok_or_else(|| ReferencePathError::Rejected("zero randomizer".to_owned()))?;
    let (ck_v_final, ck_w_final) = &tipp_mipp.final_ck;
    let (ck_v_proof, ck_w_proof) = &tipp_mipp.final_ck_proofs;
    let ck_v_valid = verify_commitment_key_g2_kzg_opening(
        verifier_srs,
        ck_v_final,
        ck_v_proof,
        &raw_transcript,
        &Fr::one(),
        &z,
    )?;
    let ck_w_valid = verify_commitment_key_g1_kzg_opening(
        verifier_srs,
        ck_w_final,
        ck_w_proof,
        &inv_transcript,
        &r_inverse,
        &z,
    )?;

    let (a_final, b_final, c_final) = tipp_mipp.final_messages;
    let ab_base = pairing_inner_product(&[a_final], &[b_final])?;
    let final_r = structured_scalar_final_from_raw_transcript(&raw_transcript, r);
    let z_base = multiexp_inner_product(&[c_final], &[final_r])?;
    let base_valid = pairing_inner_product(&[a_final], &[*ck_v_final])? == com_a
        && pairing_inner_product(&[*ck_w_final], &[b_final])? == com_b
        && IdentityOutput(vec![ab_base]) == com_t
        && pairing_inner_product(&[c_final], &[*ck_v_final])? == com_c
        && IdentityOutput(vec![z_base]) == com_z;

    Ok(ck_v_valid && ck_w_valid && base_valid)
}

fn commit_tipp_mipp_round(
    a: &[G1],
    b: &[G2],
    c: &[G1],
    r: &[Fr],
    ck_v: &[G2],
    ck_w: &[G1],
) -> ReferenceResult<TippMippCommitment> {
    Ok(TippMippCommitment {
        ab: (
            pairing_inner_product(a, ck_v)?,
            pairing_inner_product(ck_w, b)?,
            IdentityOutput(vec![pairing_inner_product(a, b)?]),
        ),
        c: (
            pairing_inner_product(c, ck_v)?,
            IdentityOutput(vec![multiexp_inner_product(c, r)?]),
        ),
    })
}

fn structured_scalar_final_from_raw_transcript(raw_transcript_reversed: &[Fr], r: &Fr) -> Fr {
    let mut power = *r;
    let mut product = Fr::one();
    for challenge in raw_transcript_reversed {
        product *= Fr::one() + (*challenge * power);
        power *= power;
    }
    product
}

fn derive_randomizer(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    com_a: &PairingOutput<P>,
    com_b: &PairingOutput<P>,
    com_c: &PairingOutput<P>,
) -> ReferenceResult<Fr> {
    let mut nonce = 0u64;
    loop {
        let mut messages = Vec::new();
        com_a
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        com_b
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        com_c
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        let digest = challenge_digest(
            family_id,
            context,
            b"aggregate.randomizer",
            nonce,
            &messages,
        );
        if let Some(r) = Fr::from_random_bytes(&digest) {
            return Ok(r);
        }
        nonce += 1;
    }
}

fn derive_scalar_challenge(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    stage_label: &'static [u8],
    messages: &[u8],
) -> ReferenceResult<Fr> {
    let mut nonce = 0u64;
    loop {
        let digest = challenge_digest(family_id, context, stage_label, nonce, messages);
        if let Some(c) = Fr::from_random_bytes(&digest) {
            if !c.is_zero() {
                return Ok(c);
            }
        }
        nonce += 1;
    }
}

fn derive_x0(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    r: &Fr,
    com: (&PairingOutput<P>, &PairingOutput<P>, &PairingOutput<P>),
    ip_ab: &PairingOutput<P>,
    agg_c: &G1,
) -> ReferenceResult<Fr> {
    let mut messages = Vec::new();
    r.serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    com.0
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    com.1
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    com.2
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    ip_ab
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    agg_c
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    derive_scalar_challenge(family_id, context, b"tipp-mipp.x0", &messages)
}

fn derive_tipp_mipp_round_challenge(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    prior_raw: &Fr,
    com_1: &TippMippCommitment,
    com_2: &TippMippCommitment,
) -> ReferenceResult<Fr> {
    let mut messages = Vec::new();
    prior_raw
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    serialize_tipp_mipp_commitment(com_1, &mut messages)?;
    serialize_tipp_mipp_commitment(com_2, &mut messages)?;
    derive_scalar_challenge(family_id, context, b"tipp-mipp.gipa.round", &messages)
}

fn derive_final_bridge(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    last_raw: &Fr,
    final_ck: &(G2, G1),
    final_messages: &(G1, G2, G1),
) -> ReferenceResult<Fr> {
    let mut messages = Vec::new();
    last_raw
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_ck
        .0
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_ck
        .1
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_messages
        .0
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_messages
        .1
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_messages
        .2
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    derive_scalar_challenge(family_id, context, b"tipp-mipp.final-bridge", &messages)
}

fn derive_tipp_mipp_kzg_challenge(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    final_bridge: &Fr,
    final_ck: &(G2, G1),
) -> ReferenceResult<Fr> {
    let mut messages = Vec::new();
    final_bridge
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_ck
        .0
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    final_ck
        .1
        .serialize_uncompressed(&mut messages)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    derive_scalar_challenge(family_id, context, b"tipp-mipp.kzg", &messages)
}

fn challenge_digest(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    stage_label: &'static [u8],
    nonce: u64,
    messages: &[u8],
) -> Vec<u8> {
    let preimage = challenge_preimage(context, stage_label, nonce, messages);
    let mut digest = Blake2b::new();
    digest.update(transcript_family_domain(family_id).as_slice());
    digest.update(&preimage);
    digest.finalize().to_vec()
}

fn challenge_preimage(
    context: &[u8; 32],
    stage_label: &'static [u8],
    nonce: u64,
    messages: &[u8],
) -> Vec<u8> {
    let stage_label_len = u32::try_from(stage_label.len()).expect("challenge stage label fits u32");
    let mut preimage = Vec::with_capacity(
        CHALLENGE_DOMAIN.len() + 4 + stage_label.len() + 32 + 8 + messages.len(),
    );
    preimage.extend_from_slice(CHALLENGE_DOMAIN);
    preimage.extend_from_slice(&stage_label_len.to_le_bytes());
    preimage.extend_from_slice(stage_label);
    preimage.extend_from_slice(context);
    preimage.extend_from_slice(&nonce.to_le_bytes());
    preimage.extend_from_slice(messages);
    preimage
}

fn transcript_family_domain(family_id: ProofFamilyId) -> Vec<u8> {
    match family_id {
        ProofFamilyId::Transfer => b"shieldd.snarkpack.transfer".to_vec(),
        ProofFamilyId::NoteReshape(family_id) => {
            format!("shieldd.snarkpack.{}", family_id.label()).into_bytes()
        }
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => {
            format!("shieldd.snarkpack.{}", family_id.label()).into_bytes()
        }
    }
}

fn reference_srs_for_count(srs: &DevSrs, item_count: usize) -> ReferenceResult<ReferenceSrs> {
    if srs.max_padded_count != DEFAULT_MAX_PADDED_PROOF_COUNT {
        return Err(ReferencePathError::InvalidInput(format!(
            "only default dev SRS size {} is supported, got {}",
            DEFAULT_MAX_PADDED_PROOF_COUNT, srs.max_padded_count
        )));
    }
    if item_count == 0 || item_count > srs.max_padded_count as usize {
        return Err(ReferencePathError::InvalidInput(format!(
            "unsupported proof count {item_count}"
        )));
    }
    let generated = generate_default_reference_srs();
    let required_power_count = (2 * item_count) - 1;
    Ok(ReferenceSrs {
        g_alpha_powers: generated.g_alpha_powers[..required_power_count].to_vec(),
        h_beta_powers: generated.h_beta_powers[..required_power_count].to_vec(),
        g_beta: generated.g_beta,
        h_alpha: generated.h_alpha,
    })
}

fn reference_verifier_srs(srs: &DevSrs) -> ReferenceResult<ReferenceVerifierSrs> {
    let srs = reference_srs_for_count(srs, 1)?;
    Ok(srs.verifier_srs())
}

fn ensure_reference_srs_matches_public_id(srs: &DevSrs) -> ReferenceResult<()> {
    let generated = generate_default_reference_srs();
    let mut bytes = Vec::new();
    generated
        .serialize_compressed(&mut bytes)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;

    // Compare against the public production SRS id without exposing production
    // SRS internals to this crate.
    let production_id = srs_id(srs);
    let reference_id = reference_srs_id(srs, &bytes);
    if production_id != reference_id {
        return Err(ReferencePathError::InvalidInput(
            "independently derived reference SRS id does not match production id".to_owned(),
        ));
    }
    Ok(())
}

fn reference_srs_id(srs: &DevSrs, serialized_srs: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::default();
    sha2::Digest::update(
        &mut hasher,
        format!(
            "shieldd.proof_aggregation.srs:backend={}:curve={}:max_padded_count={}",
            shieldd_sdk_proof_aggregation::DEV_SRS_BACKEND_ID,
            shieldd_sdk_proof_aggregation::DEV_SRS_CURVE_ID,
            srs.max_padded_count
        )
        .as_bytes(),
    );
    sha2::Digest::update(&mut hasher, serialized_srs);
    sha2::Digest::finalize(hasher).into()
}

impl ReferenceSrs {
    fn commitment_keys(&self) -> (Vec<G2>, Vec<G1>) {
        (
            self.h_beta_powers.iter().step_by(2).copied().collect(),
            self.g_alpha_powers.iter().step_by(2).copied().collect(),
        )
    }

    fn verifier_srs(&self) -> ReferenceVerifierSrs {
        ReferenceVerifierSrs {
            g: self.g_alpha_powers[0],
            h: self.h_beta_powers[0],
            g_beta: self.g_beta,
            h_alpha: self.h_alpha,
        }
    }
}

impl CanonicalSerialize for ReferenceSrs {
    fn serialize_with_mode<W: ark_serialize::Write>(
        &self,
        mut writer: W,
        compress: ark_serialize::Compress,
    ) -> Result<(), ark_serialize::SerializationError> {
        self.g_alpha_powers
            .serialize_with_mode(&mut writer, compress)?;
        self.h_beta_powers
            .serialize_with_mode(&mut writer, compress)?;
        self.g_beta.serialize_with_mode(&mut writer, compress)?;
        self.h_alpha.serialize_with_mode(&mut writer, compress)
    }

    fn serialized_size(&self, compress: ark_serialize::Compress) -> usize {
        self.g_alpha_powers.serialized_size(compress)
            + self.h_beta_powers.serialized_size(compress)
            + self.g_beta.serialized_size(compress)
            + self.h_alpha.serialized_size(compress)
    }
}

fn generate_default_reference_srs() -> ReferenceSrs {
    let mut rng = ChaCha20Rng::from_seed(DEV_SRS_SEED);
    let alpha = <Fr as ark_ff::UniformRand>::rand(&mut rng);
    let beta = <Fr as ark_ff::UniformRand>::rand(&mut rng);
    let g = G1::generator();
    let h = G2::generator();
    ReferenceSrs {
        g_alpha_powers: structured_generators_scalar_power(
            (2 * DEFAULT_MAX_PADDED_PROOF_COUNT as usize) - 1,
            &g,
            &alpha,
        ),
        h_beta_powers: structured_generators_scalar_power(
            (2 * DEFAULT_MAX_PADDED_PROOF_COUNT as usize) - 1,
            &h,
            &beta,
        ),
        g_beta: g * beta,
        h_alpha: h * alpha,
    }
}

fn pairing_inner_product(left: &[G1], right: &[G2]) -> ReferenceResult<PairingOutput<P>> {
    if left.len() != right.len() {
        return Err(ReferencePathError::InvalidInput(format!(
            "pairing input length mismatch: left={}, right={}",
            left.len(),
            right.len()
        )));
    }
    Ok(P::multi_pairing(
        G1::normalize_batch(left),
        G2::normalize_batch(right),
    ))
}

fn multiexp_inner_product(left: &[G1], right: &[Fr]) -> ReferenceResult<G1> {
    if left.len() != right.len() {
        return Err(ReferencePathError::InvalidInput(format!(
            "MSM input length mismatch: left={}, right={}",
            left.len(),
            right.len()
        )));
    }
    G1::msm(&G1::normalize_batch(left), right)
        .map_err(|_| ReferencePathError::InvalidInput("MSM length mismatch".to_owned()))
}

fn structured_scalar_power(num: usize, s: &Fr) -> Vec<Fr> {
    let mut powers = vec![Fr::one()];
    for i in 1..num {
        powers.push(powers[i - 1] * s);
    }
    powers
}

fn structured_generators_scalar_power<G: CurveGroup>(
    num: usize,
    g: &G,
    s: &G::ScalarField,
) -> Vec<G> {
    let mut powers = Vec::with_capacity(num);
    let mut current = G::ScalarField::one();
    for _ in 0..num {
        powers.push(current);
        current *= s;
    }
    g.batch_mul(&powers)
        .into_iter()
        .map(|point| point.into_group())
        .collect()
}

fn build_shifted_ck_2(ck_2: &[G1], r: &Fr) -> Vec<G1> {
    let inverse_powers = inverse_powers(ck_2.len(), r);
    ck_2.iter()
        .zip(inverse_powers.iter())
        .map(|(ck, power)| *ck * power)
        .collect()
}

fn inverse_powers(len: usize, r: &Fr) -> Vec<Fr> {
    let mut powers = Vec::with_capacity(len);
    let r_inv = r.inverse().expect("randomizer must be nonzero");
    let mut current = Fr::one();
    for _ in 0..len {
        powers.push(current);
        current *= r_inv;
    }
    powers
}

fn rescale_fold<T>(scaled_half: &[T], unscaled_half: &[T], scalar: &Fr) -> Vec<T>
where
    T: Clone + Add<Output = T> + MulAssign<Fr>,
{
    scaled_half
        .iter()
        .cloned()
        .zip(unscaled_half.iter().cloned())
        .map(|(mut scaled, base)| {
            scaled.mul_assign(*scalar);
            scaled + base
        })
        .collect()
}

fn scale<T>(mut value: T, scalar: &Fr) -> T
where
    T: MulAssign<Fr>,
{
    value.mul_assign(*scalar);
    value
}

fn prove_commitment_key_kzg_opening<G: CurveGroup>(
    srs_powers: &[G],
    transcript: &[G::ScalarField],
    r_shift: &G::ScalarField,
    kzg_challenge: &G::ScalarField,
) -> ReferenceResult<G> {
    let ck_coefficients = polynomial_coefficients_from_transcript(transcript, r_shift);
    let ck_polynomial = DensePolynomial::from_coefficients_slice(&ck_coefficients);
    if srs_powers.len() != ck_polynomial.coeffs.len() {
        return Err(ReferencePathError::InvalidInput(
            "KZG SRS/polynomial length mismatch".to_owned(),
        ));
    }
    let ck_polynomial_c_eval =
        polynomial_evaluation_product_form_from_transcript(transcript, kzg_challenge, r_shift);
    let quotient_polynomial = &(&ck_polynomial
        - &DensePolynomial::from_coefficients_vec(vec![ck_polynomial_c_eval]))
        / &(DensePolynomial::from_coefficients_vec(vec![-*kzg_challenge, G::ScalarField::one()]));
    let mut quotient_coeffs = quotient_polynomial.coeffs;
    quotient_coeffs.resize(srs_powers.len(), G::ScalarField::zero());
    G::msm(&G::normalize_batch(srs_powers), &quotient_coeffs)
        .map_err(|_| ReferencePathError::InvalidInput("KZG opening MSM mismatch".to_owned()))
}

fn verify_commitment_key_g2_kzg_opening(
    v_srs: &ReferenceVerifierSrs,
    ck_final: &G2,
    ck_opening: &G2,
    transcript: &[Fr],
    r_shift: &Fr,
    kzg_challenge: &Fr,
) -> ReferenceResult<bool> {
    let ck_polynomial_c_eval =
        polynomial_evaluation_product_form_from_transcript(transcript, kzg_challenge, r_shift);
    let left = vec![v_srs.g, -(v_srs.g_beta - v_srs.g * kzg_challenge)];
    let right = vec![*ck_final - v_srs.h * ck_polynomial_c_eval, *ck_opening];
    Ok(pairing_inner_product(&left, &right)? == PairingOutput::<P>::zero())
}

fn verify_commitment_key_g1_kzg_opening(
    v_srs: &ReferenceVerifierSrs,
    ck_final: &G1,
    ck_opening: &G1,
    transcript: &[Fr],
    r_shift: &Fr,
    kzg_challenge: &Fr,
) -> ReferenceResult<bool> {
    let ck_polynomial_c_eval =
        polynomial_evaluation_product_form_from_transcript(transcript, kzg_challenge, r_shift);
    let left = vec![*ck_final - v_srs.g * ck_polynomial_c_eval, -*ck_opening];
    let right = vec![v_srs.h, v_srs.h_alpha - v_srs.h * kzg_challenge];
    Ok(pairing_inner_product(&left, &right)? == PairingOutput::<P>::zero())
}

fn polynomial_evaluation_product_form_from_transcript<F: Field>(
    transcript: &[F],
    z: &F,
    r_shift: &F,
) -> F {
    let mut power_2_zr = (*z * z) * r_shift;
    let mut product = F::one();
    for x in transcript {
        product *= F::one() + (*x * power_2_zr);
        power_2_zr *= power_2_zr;
    }
    product
}

fn polynomial_coefficients_from_transcript<F: Field>(transcript: &[F], r_shift: &F) -> Vec<F> {
    let mut coefficients = vec![F::one()];
    let mut power_2_r = *r_shift;
    for (i, x) in transcript.iter().enumerate() {
        for j in 0..(2_usize).pow(i as u32) {
            coefficients.push(coefficients[j] * (*x * power_2_r));
        }
        power_2_r *= power_2_r;
    }

    let mut interleaved =
        Vec::with_capacity(coefficients.len().saturating_mul(2).saturating_sub(1));
    for (index, coefficient) in coefficients.into_iter().enumerate() {
        if index > 0 {
            interleaved.push(F::zero());
        }
        interleaved.push(coefficient);
    }
    interleaved
}

fn fold_public_inputs(vk: &VerifyingKey<P>, public_inputs: &[Vec<Fq>], r: &Fr) -> (Fr, G1) {
    let r_sum = (r.pow(&[public_inputs.len() as u64]) - Fr::one()) / (*r - Fr::one());
    let r_vec = structured_scalar_power(public_inputs.len(), r);
    let mut folded_public_inputs = vec![Fq::zero(); public_inputs[0].len()];
    for (inputs, challenge_power) in public_inputs.iter().zip(&r_vec) {
        for (acc, input) in folded_public_inputs.iter_mut().zip(inputs) {
            *acc += *input * challenge_power;
        }
    }

    let mut g_ic = vk.gamma_abc_g1[0].into_group() * r_sum;
    for (base, folded_input) in vk
        .gamma_abc_g1
        .iter()
        .skip(1)
        .zip(folded_public_inputs.iter())
    {
        g_ic += base.into_group() * folded_input;
    }
    (r_sum, g_ic)
}

fn verify_ppe(vk: &VerifyingKey<P>, proof: &ReferenceAggregateProof, r_sum: &Fr, g_ic: G1) -> bool {
    pairing_inner_product(
        &[vk.alpha_g1.into_group() * r_sum, g_ic, proof.agg_c],
        &[
            vk.beta_g2.into_group(),
            vk.gamma_g2.into_group(),
            vk.delta_g2.into_group(),
        ],
    )
    .map(|pairing_output| pairing_output == proof.ip_ab)
    .unwrap_or(false)
}

fn serialize_ab_commitment(com: &AbCommitmentTriple, out: &mut Vec<u8>) -> ReferenceResult<()> {
    com.0
        .serialize_uncompressed(&mut *out)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    com.1
        .serialize_uncompressed(&mut *out)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    com.2
        .serialize_uncompressed(&mut *out)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    Ok(())
}

fn serialize_tipp_mipp_commitment(
    com: &TippMippCommitment,
    out: &mut Vec<u8>,
) -> ReferenceResult<()> {
    serialize_ab_commitment(&com.ab, out)?;
    com.c
        .0
        .serialize_uncompressed(&mut *out)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    com.c
        .1
        .serialize_uncompressed(&mut *out)
        .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use ark_snark::SNARK;
    use shieldd_sdk_proof_aggregation::{
        aggregate_family, decode_wrapped_aggregate_proof, encode_wrapped_aggregate_proof,
        verify_family_aggregate, AGGREGATE_PROTOCOL_VERSION,
    };
    use shieldd_sdk_shielded_pool::{NoteReshapeFamilyId, ShieldedIcs20WithdrawalFamilyId};

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

    fn fixture() -> (
        PreparedVerifyingKey<P>,
        Vec<BatchItem>,
        AggregateStatement,
        DevSrs,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(11);
        let pk = Groth16::<P, LibsnarkReduction>::generate_random_parameters_with_reduction(
            SquareCircuit {
                x: Some(Fq::from(1u64)),
            },
            &mut rng,
        )
        .expect("setup should succeed");
        let pvk = PreparedVerifyingKey::from(pk.vk.clone());
        let items = [2u64, 3u64]
            .into_iter()
            .map(|x| {
                let x = Fq::from(x);
                let proof = Groth16::<P, LibsnarkReduction>::prove(
                    &pk,
                    SquareCircuit { x: Some(x) },
                    &mut rng,
                )
                .expect("proof should build");
                BatchItem {
                    proof,
                    public_inputs: vec![x * x],
                }
            })
            .collect::<Vec<_>>();
        let srs = DevSrs::default();
        let padded = shieldd_sdk_proof_aggregation::pad_items_to_power_of_two(
            &items,
            srs.max_padded_count as usize,
        )
        .expect("padding should succeed");
        let rows = padded
            .iter()
            .map(|item| item.public_inputs.clone())
            .collect::<Vec<_>>();
        let statement = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            srs_id(&srs),
            &pvk,
            items.len() as u32,
            &rows,
        )
        .expect("statement should build");
        (pvk, padded, statement, srs)
    }

    fn sample_items_with_count(
        seed: u64,
        count: usize,
    ) -> (PreparedVerifyingKey<P>, Vec<BatchItem>) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let pk = Groth16::<P, LibsnarkReduction>::generate_random_parameters_with_reduction(
            SquareCircuit {
                x: Some(Fq::from(1u64)),
            },
            &mut rng,
        )
        .expect("setup should succeed");
        let pvk = PreparedVerifyingKey::from(pk.vk.clone());
        let items = (0..count)
            .map(|_| {
                let x = Fq::rand(&mut rng);
                let proof = Groth16::<P, LibsnarkReduction>::prove(
                    &pk,
                    SquareCircuit { x: Some(x) },
                    &mut rng,
                )
                .expect("proof should build");
                BatchItem {
                    proof,
                    public_inputs: vec![x * x],
                }
            })
            .collect();
        (pvk, items)
    }

    fn parity_families() -> Vec<ProofFamilyId> {
        let mut families = vec![ProofFamilyId::Transfer];
        families.extend(
            NoteReshapeFamilyId::ALL
                .into_iter()
                .map(ProofFamilyId::NoteReshape),
        );
        families.push(ProofFamilyId::ShieldedIcs20Withdrawal(
            ShieldedIcs20WithdrawalFamilyId::Canonical,
        ));
        families
    }

    fn statement_for_items(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<P>,
        real_count: usize,
        padded_items: &[BatchItem],
        srs: &DevSrs,
    ) -> AggregateStatement {
        let rows = padded_items
            .iter()
            .map(|item| item.public_inputs.clone())
            .collect::<Vec<_>>();
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            family_id,
            srs_id(srs),
            pvk,
            real_count as u32,
            &rows,
        )
        .expect("statement should build")
    }

    #[test]
    fn reference_verifier_accepts_production_prover() {
        let (pvk, items, statement, srs) = fixture();
        let production = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
        let report = reference_verify_family_aggregate(&statement, &pvk, &production, &srs)
            .expect("reference verifier should run");
        assert!(report.accepted);
        verify_family_aggregate(&statement, &pvk, &production, &srs)
            .expect("production verifier should accept its aggregate");
    }

    #[test]
    fn reference_prover_cross_verifies_with_production() {
        let (pvk, items, statement, srs) = fixture();
        let reference = reference_aggregate_family(&statement, &pvk, &items, &srs)
            .expect("reference aggregate should succeed");
        verify_family_aggregate(&statement, &pvk, &reference.wrapped_proof, &srs)
            .expect("production verifier should accept reference aggregate");
    }

    #[test]
    fn reference_verifier_rejects_invalid_public_boundary_bytes() {
        let (pvk, items, statement, srs) = fixture();
        let production = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
        let inner = decode_wrapped_aggregate_proof(&production, statement.statement_digest(), None)
            .expect("wrapper decode");
        let mut mutated_inner = inner.to_vec();
        mutated_inner[0] ^= 0x01;
        let wrapped = encode_wrapped_aggregate_proof(statement.statement_digest(), &mutated_inner)
            .expect("wrapper encode");
        let err = reference_verify_family_aggregate(&statement, &pvk, &wrapped, &srs)
            .expect_err("malformed inner proof bytes should reject");
        assert!(matches!(
            err,
            ReferencePathError::MalformedProof(_) | ReferencePathError::Rejected(_)
        ));
    }

    #[test]
    fn reference_verifier_rejects_invalid_wrapper_digest() {
        let (pvk, items, statement, srs) = fixture();
        let mut production = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
        production[40] ^= 0x01;
        let err = reference_verify_family_aggregate(&statement, &pvk, &production, &srs)
            .expect_err("wrapper digest mutation should reject");
        assert!(matches!(err, ReferencePathError::Rejected(_)));
    }

    #[test]
    fn reference_srs_matches_public_production_id() {
        let srs = DevSrs::default();
        let generated = generate_default_reference_srs();
        let mut reference_bytes = Vec::new();
        generated
            .serialize_compressed(&mut reference_bytes)
            .expect("reference serialize");
        let production_bytes = srs
            .serialized_inner_product_srs_compressed()
            .expect("production serialize");
        assert_eq!(reference_bytes, production_bytes);
        ensure_reference_srs_matches_public_id(&srs).expect("matching srs id");
    }

    #[test]
    #[ignore = "slow release/scheduled two-way interop band"]
    fn slow_two_way_interop_band() {
        for (family_index, family_id) in parity_families().into_iter().enumerate() {
            for count in [1usize, 2, 4, 8] {
                let (pvk, items) = sample_items_with_count(
                    9_000 + (family_index as u64) * 100 + count as u64,
                    count,
                );
                let srs = DevSrs::default();
                let padded = shieldd_sdk_proof_aggregation::pad_items_to_power_of_two(
                    &items,
                    srs.max_padded_count as usize,
                )
                .expect("padding should succeed");
                let statement = statement_for_items(family_id, &pvk, count, &padded, &srs);
                let reference = reference_aggregate_family(&statement, &pvk, &padded, &srs)
                    .expect("reference aggregate should succeed");
                reference_verify_family_aggregate(&statement, &pvk, &reference.wrapped_proof, &srs)
                    .expect("reference verifier should accept reference aggregate");
                verify_family_aggregate(&statement, &pvk, &reference.wrapped_proof, &srs)
                    .expect("production verifier should accept reference aggregate");
            }
        }
    }
}
