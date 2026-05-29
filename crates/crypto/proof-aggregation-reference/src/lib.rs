#![forbid(unsafe_code)]

//! Dev-only independent SnarkPack reference path.
//!
//! This crate stays outside `penumbra-sdk-proof-aggregation` so Cargo prevents
//! it from importing private production aggregation modules. It re-implements
//! the slow aggregation equations and decodes production bytes into
//! reference-owned proof structs.

use std::{
    fmt,
    ops::{Add, MulAssign},
};

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
use penumbra_sdk_proof_aggregation::{
    encode_wrapped_aggregate_proof, preflight_aggregate_verify, srs_id, AggregatePreflightInput,
    AggregateStatement, DevSrs, ProofFamilyId, DEFAULT_MAX_PADDED_PROOF_COUNT,
};
pub use penumbra_sdk_proof_aggregation_trace_schema::{
    FilecoinBugClass, TraceComparisonLevel, TraceEvent, TraceEventError, TraceEventKind,
    TracePolicy, TRACE_POLICIES,
};
use penumbra_sdk_proof_params::batch::BatchItem;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

type P = Bls12_377;
type G1 = <P as Pairing>::G1;
type G2 = <P as Pairing>::G2;
type Fr = <P as Pairing>::ScalarField;

const DEV_SRS_SEED: [u8; 32] = [0x50; 32];
const CHALLENGE_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge.v1\0";
const CHALLENGE_CONTEXT_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge_context.v1\0";

pub type ReferenceResult<T> = Result<T, ReferencePathError>;
pub type ReferenceTraceEntry = TraceEvent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceAggregate {
    pub wrapped_proof: Vec<u8>,
    pub prover_trace: Vec<ReferenceTraceEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceVerifyReport {
    pub accepted: bool,
    pub verifier_trace: Vec<ReferenceTraceEntry>,
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
    tipa_proof_ab: ReferenceTipaAbProof,
    tipa_proof_c: ReferenceTipaCProof,
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceTipaAbProof {
    gipa_proof: ReferenceGipaAbProof,
    final_ck: (G2, G1),
    final_ck_proof: (G2, G1),
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceTipaCProof {
    gipa_proof: ReferenceGipaCProof,
    final_ck: G2,
    final_ck_proof: G2,
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceGipaAbProof {
    r_commitment_steps: Vec<(AbCommitmentTriple, AbCommitmentTriple)>,
    r_base: (G1, G2),
}

#[derive(Clone, CanonicalSerialize, CanonicalDeserialize)]
struct ReferenceGipaCProof {
    r_commitment_steps: Vec<(CCommitmentTriple, CCommitmentTriple)>,
    r_base: (G1, Fr),
}

type AbCommitmentTriple = (
    PairingOutput<P>,
    PairingOutput<P>,
    IdentityOutput<PairingOutput<P>>,
);
type CCommitmentTriple = (PairingOutput<P>, Fr, IdentityOutput<G1>);

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
struct GipaAbAux {
    r_transcript: Vec<Fr>,
    ck_base: (G2, G1),
}

#[derive(Clone)]
struct GipaCAux {
    r_transcript: Vec<Fr>,
    ck_base: G2,
}

#[derive(Clone, Debug, Default)]
struct ReferenceTraceCollector {
    events: Vec<TraceEvent>,
    mutant: Option<VerifierMutant>,
}

impl ReferenceTraceCollector {
    fn with_mutant(mutant: VerifierMutant) -> Self {
        Self {
            events: Vec::new(),
            mutant: Some(mutant),
        }
    }

    fn into_events(self) -> Vec<TraceEvent> {
        self.events
    }

    fn mutates_context_constructor(&self) -> bool {
        self.mutant == Some(VerifierMutant::ContextConstructor)
    }

    fn mutates_stage_labels(&self) -> bool {
        self.mutant == Some(VerifierMutant::StageLabels)
    }

    fn mutates_challenge_preimage(&self) -> bool {
        self.mutant == Some(VerifierMutant::ChallengePreimage)
    }

    fn mutates_spec_row(&self, spec_row_id: &str) -> bool {
        matches!(
            (self.mutant, spec_row_id),
            (
                Some(VerifierMutant::GipaChallengeDependency),
                "gipa.challenge-dependency"
            ) | (
                Some(VerifierMutant::TipaAbKzgChallenge),
                "tipa.ab.kzg-challenge"
            ) | (Some(VerifierMutant::SsmKzgChallenge), "ssm.kzg-challenge")
                | (
                    Some(VerifierMutant::Groth16Randomizer),
                    "groth16.randomizer"
                )
        )
    }

    fn record_context(&mut self, statement_digest: [u8; 32]) {
        self.events.push(TraceEvent {
            spec_row_id: "fs.context-constructor",
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengeContext,
            stage_label: "statement",
            nonce: None,
            round_index: None,
            byte_payload: challenge_context_constructor_preimage(statement_digest),
            abstract_payload: None,
            filecoin_bug_class: None,
        });
    }

    fn record_challenge(
        &mut self,
        spec_row_id: &'static str,
        stage_label: &'static [u8],
        nonce: u64,
        preimage: Vec<u8>,
        digest: &[u8],
    ) {
        let stage_label = stage_label_str(stage_label);
        self.events.push(TraceEvent {
            spec_row_id: "fs.stage-labels",
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengePreimage,
            stage_label,
            nonce: Some(nonce),
            round_index: None,
            byte_payload: stage_label.as_bytes().to_vec(),
            abstract_payload: None,
            filecoin_bug_class: None,
        });
        self.events.push(TraceEvent {
            spec_row_id: "fs.challenge-preimage",
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengePreimage,
            stage_label,
            nonce: Some(nonce),
            round_index: None,
            byte_payload: preimage.clone(),
            abstract_payload: None,
            filecoin_bug_class: None,
        });
        self.events.push(TraceEvent {
            spec_row_id,
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengeDigest,
            stage_label,
            nonce: Some(nonce),
            round_index: None,
            byte_payload: digest.to_vec(),
            abstract_payload: None,
            filecoin_bug_class: None,
        });
    }

    fn record_abstract(&mut self, spec_row_id: &'static str, stage_label: &'static str) {
        self.events.push(TraceEvent {
            spec_row_id,
            primary_level: TraceComparisonLevel::AbstractTrace,
            event_kind: TraceEventKind::EquationRole,
            stage_label,
            nonce: None,
            round_index: None,
            byte_payload: Vec::new(),
            abstract_payload: Some(stage_label),
            filecoin_bug_class: None,
        });
    }
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

    let mut trace = ReferenceTraceCollector::default();
    trace.record_context(statement.statement_digest());
    emit_static_abstract_trace(&mut trace);

    let reference_srs = reference_srs_for_count(srs, items.len())?;
    let aggregate = reference_aggregate_proofs(
        statement.family_id(),
        statement.challenge_context().as_bytes(),
        &mut trace,
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

    Ok(ReferenceAggregate {
        wrapped_proof,
        prover_trace: trace.into_events(),
    })
}

pub fn reference_verify_family_aggregate(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<P>,
    wrapped_proof: &[u8],
    srs: &DevSrs,
) -> ReferenceResult<ReferenceVerifyReport> {
    reference_verify_family_aggregate_inner(statement, pvk, wrapped_proof, srs, None)
}

pub fn reference_verify_family_aggregate_with_verifier_mutant(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<P>,
    wrapped_proof: &[u8],
    srs: &DevSrs,
    mutant: VerifierMutant,
) -> ReferenceResult<ReferenceVerifyReport> {
    reference_verify_family_aggregate_inner(statement, pvk, wrapped_proof, srs, Some(mutant))
}

fn reference_verify_family_aggregate_inner(
    statement: &AggregateStatement,
    pvk: &PreparedVerifyingKey<P>,
    wrapped_proof: &[u8],
    srs: &DevSrs,
    mutant: Option<VerifierMutant>,
) -> ReferenceResult<ReferenceVerifyReport> {
    ensure_reference_srs_matches_public_id(srs)?;
    let call = preflight_aggregate_verify(AggregatePreflightInput {
        statement,
        pvk,
        aggregate_proof_bytes: wrapped_proof,
        srs,
    })
    .map_err(|err| ReferencePathError::Rejected(err.to_string()))?;

    let mut trace = mutant
        .map(ReferenceTraceCollector::with_mutant)
        .unwrap_or_default();
    trace.record_context(statement.statement_digest());
    emit_static_abstract_trace(&mut trace);

    let aggregate = ReferenceAggregateProof::deserialize_compressed(call.inner_proof_bytes())
        .map_err(|err| {
            ReferencePathError::MalformedProof(format!("aggregate proof decode failed: {err}"))
        })?;
    let verifier_srs = reference_verifier_srs(call.srs())?;
    let accepted = reference_verify_aggregate_proof(
        call.family_id(),
        call.challenge_context().as_bytes(),
        &mut trace,
        &verifier_srs,
        &call.pvk().vk,
        call.padded_public_inputs(),
        &aggregate,
    )?;

    Ok(ReferenceVerifyReport {
        accepted,
        verifier_trace: trace.into_events(),
    })
}

fn emit_static_abstract_trace(trace: &mut ReferenceTraceCollector) {
    trace.record_abstract("gipa.input-relation", "gipa.input-relation");
    trace.record_abstract("gipa.round-folding", "gipa.round-folding");
    trace.record_abstract("gipa.verifier-folding", "gipa.verifier-folding");
    trace.record_abstract("tipa.srs", "tipa.srs");
    trace.record_abstract("tipa.ab.gipa", "tipa.ab.gipa");
    trace.record_abstract("tipa.ab.kzg-equations", "tipa.ab.kzg-equations");
    trace.record_abstract("ssm.power-sequence", "ssm.power-sequence");
    trace.record_abstract("ssm.base-equation", "ssm.base-equation");
    trace.record_abstract("groth16.folded-inputs", "groth16.folded-inputs");
    trace.record_abstract("groth16.ppe", "groth16.ppe");
}

fn reference_aggregate_proofs(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
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

    let r = derive_randomizer(family_id, context, trace, &com_a, &com_b, &com_c)?;
    let r_vec = structured_scalar_power(proofs.len(), &r);
    let a_r = a
        .iter()
        .zip(&r_vec)
        .map(|(point, scalar)| *point * scalar)
        .collect::<Vec<_>>();
    let ip_ab = pairing_inner_product(&a_r, &b)?;
    let agg_c = multiexp_inner_product(&c, &r_vec)?;
    let ck_1_r = build_shifted_ck_1(&ck_1, &r);

    let tipa_proof_ab = prove_tipa_ab(
        family_id,
        context,
        trace,
        srs,
        (&a_r, &b),
        (&ck_1_r, &ck_2),
        &r,
    )?;
    let tipa_proof_c = prove_tipa_c(family_id, context, trace, srs, (&c, &r_vec), &ck_1)?;

    Ok(ReferenceAggregateProof {
        com_a,
        com_b,
        com_c,
        ip_ab,
        agg_c,
        tipa_proof_ab,
        tipa_proof_c,
    })
}

fn reference_verify_aggregate_proof(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    verifier_srs: &ReferenceVerifierSrs,
    vk: &VerifyingKey<P>,
    public_inputs: &[Vec<Fq>],
    proof: &ReferenceAggregateProof,
) -> ReferenceResult<bool> {
    let r = derive_randomizer(
        family_id,
        context,
        trace,
        &proof.com_a,
        &proof.com_b,
        &proof.com_c,
    )?;
    let tipa_ab_valid = verify_tipa_ab(family_id, context, trace, verifier_srs, proof, &r)?;
    let tipa_c_valid = verify_tipa_c(family_id, context, trace, verifier_srs, proof, &r)?;
    let (r_sum, g_ic) = fold_public_inputs(vk, public_inputs, &r);
    let ppe_valid = verify_ppe(vk, proof, &r_sum, g_ic);
    Ok(tipa_ab_valid && tipa_c_valid && ppe_valid)
}

fn prove_tipa_ab(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    srs: &ReferenceSrs,
    values: (&[G1], &[G2]),
    ck: (&[G2], &[G1]),
    r_shift: &Fr,
) -> ReferenceResult<ReferenceTipaAbProof> {
    let (gipa_proof, aux) =
        prove_gipa_ab(family_id, context, trace, b"tipa.ab.gipa.round", values, ck)?;
    let (ck_a_final, ck_b_final) = aux.ck_base;
    let transcript = aux.r_transcript;
    let transcript_inverse = transcript
        .iter()
        .map(|x| {
            x.inverse()
                .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))
        })
        .collect::<ReferenceResult<Vec<_>>>()?;
    let r_inverse = r_shift
        .inverse()
        .ok_or_else(|| ReferencePathError::Rejected("zero randomizer".to_owned()))?;
    let c = derive_kzg_challenge_ab(
        family_id,
        context,
        trace,
        b"tipa.ab.kzg",
        &transcript,
        &ck_a_final,
        &ck_b_final,
    )?;
    let ck_a_kzg_opening =
        prove_commitment_key_kzg_opening(&srs.h_beta_powers, &transcript_inverse, &r_inverse, &c)?;
    let ck_b_kzg_opening =
        prove_commitment_key_kzg_opening(&srs.g_alpha_powers, &transcript, &Fr::one(), &c)?;

    Ok(ReferenceTipaAbProof {
        gipa_proof,
        final_ck: (ck_a_final, ck_b_final),
        final_ck_proof: (ck_a_kzg_opening, ck_b_kzg_opening),
    })
}

fn prove_tipa_c(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    srs: &ReferenceSrs,
    values: (&[G1], &[Fr]),
    ck_a: &[G2],
) -> ReferenceResult<ReferenceTipaCProof> {
    let (gipa_proof, aux) = prove_gipa_c(
        family_id,
        context,
        trace,
        b"tipa.c.gipa.round",
        values,
        ck_a,
    )?;
    let ck_a_final = aux.ck_base;
    let transcript = aux.r_transcript;
    let transcript_inverse = transcript
        .iter()
        .map(|x| {
            x.inverse()
                .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))
        })
        .collect::<ReferenceResult<Vec<_>>>()?;
    let c = derive_kzg_challenge_c(
        family_id,
        context,
        trace,
        b"tipa.c.kzg",
        &transcript,
        &ck_a_final,
    )?;
    let ck_a_kzg_opening =
        prove_commitment_key_kzg_opening(&srs.h_beta_powers, &transcript_inverse, &Fr::one(), &c)?;

    Ok(ReferenceTipaCProof {
        gipa_proof,
        final_ck: ck_a_final,
        final_ck_proof: ck_a_kzg_opening,
    })
}

fn verify_tipa_ab(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    verifier_srs: &ReferenceVerifierSrs,
    proof: &ReferenceAggregateProof,
    r: &Fr,
) -> ReferenceResult<bool> {
    let (base_com, transcript) = compute_recursive_challenges_ab(
        family_id,
        context,
        trace,
        b"tipa.ab.gipa.round",
        (
            proof.com_a.clone(),
            proof.com_b.clone(),
            IdentityOutput(vec![proof.ip_ab.clone()]),
        ),
        &proof.tipa_proof_ab.gipa_proof,
    )?;
    let transcript_inverse = transcript
        .iter()
        .map(|x| {
            x.inverse()
                .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))
        })
        .collect::<ReferenceResult<Vec<_>>>()?;
    let (ck_a_final, ck_b_final) = &proof.tipa_proof_ab.final_ck;
    let (ck_a_proof, ck_b_proof) = &proof.tipa_proof_ab.final_ck_proof;
    let c = derive_kzg_challenge_ab(
        family_id,
        context,
        trace,
        b"tipa.ab.kzg",
        &transcript,
        ck_a_final,
        ck_b_final,
    )?;
    let r_inverse = r
        .inverse()
        .ok_or_else(|| ReferencePathError::Rejected("zero randomizer".to_owned()))?;
    let ck_a_valid = verify_commitment_key_g2_kzg_opening(
        verifier_srs,
        ck_a_final,
        ck_a_proof,
        &transcript_inverse,
        &r_inverse,
        &c,
    )?;
    let ck_b_valid = verify_commitment_key_g1_kzg_opening(
        verifier_srs,
        ck_b_final,
        ck_b_proof,
        &transcript,
        &Fr::one(),
        &c,
    )?;
    let (com_a, com_b, com_t) = base_com;
    let a_base = [proof.tipa_proof_ab.gipa_proof.r_base.0];
    let b_base = [proof.tipa_proof_ab.gipa_proof.r_base.1];
    let t_base = IdentityOutput(vec![pairing_inner_product(&a_base, &b_base)?]);
    let base_valid = pairing_inner_product(&a_base, &[*ck_a_final])? == com_a
        && pairing_inner_product(&[*ck_b_final], &b_base)? == com_b
        && t_base == com_t;

    Ok(ck_a_valid && ck_b_valid && base_valid)
}

fn verify_tipa_c(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    verifier_srs: &ReferenceVerifierSrs,
    proof: &ReferenceAggregateProof,
    r: &Fr,
) -> ReferenceResult<bool> {
    let (base_com, transcript) = compute_recursive_challenges_c(
        family_id,
        context,
        trace,
        b"tipa.c.gipa.round",
        (
            proof.com_c.clone(),
            Fr::zero(),
            IdentityOutput(vec![proof.agg_c]),
        ),
        &proof.tipa_proof_c.gipa_proof,
    )?;
    let transcript_inverse = transcript
        .iter()
        .map(|x| {
            x.inverse()
                .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))
        })
        .collect::<ReferenceResult<Vec<_>>>()?;
    let ck_a_final = &proof.tipa_proof_c.final_ck;
    let ck_a_proof = &proof.tipa_proof_c.final_ck_proof;
    let c = derive_kzg_challenge_c(
        family_id,
        context,
        trace,
        b"tipa.c.kzg",
        &transcript,
        ck_a_final,
    )?;
    let ck_a_valid = verify_commitment_key_g2_kzg_opening(
        verifier_srs,
        ck_a_final,
        ck_a_proof,
        &transcript_inverse,
        &Fr::one(),
        &c,
    )?;
    let mut power_2_b = *r;
    let mut b_base = Fr::one();
    for x in &transcript {
        let x_inverse = x
            .inverse()
            .ok_or_else(|| ReferencePathError::Rejected("zero transcript challenge".to_owned()))?;
        b_base *= Fr::one() + (x_inverse * power_2_b);
        power_2_b *= power_2_b;
    }

    let (com_a, _, com_t) = base_com;
    let a_base = [proof.tipa_proof_c.gipa_proof.r_base.0];
    let t_base = IdentityOutput(vec![multiexp_inner_product(&a_base, &[b_base])?]);
    let base_valid = pairing_inner_product(&a_base, &[*ck_a_final])? == com_a && t_base == com_t;

    Ok(ck_a_valid && base_valid)
}

fn prove_gipa_ab(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    values: (&[G1], &[G2]),
    ck: (&[G2], &[G1]),
) -> ReferenceResult<(ReferenceGipaAbProof, GipaAbAux)> {
    let mut m_a = values.0.to_vec();
    let mut m_b = values.1.to_vec();
    let mut ck_a = ck.0.to_vec();
    let mut ck_b = ck.1.to_vec();
    let mut r_commitment_steps = Vec::new();
    let mut r_transcript = Vec::new();
    let (m_base, ck_base) = loop {
        if m_a.len() == 1 {
            break ((m_a[0], m_b[0]), (ck_a[0], ck_b[0]));
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

        let com_1 = (
            pairing_inner_product(m_a_1, ck_a_1)?,
            pairing_inner_product(ck_b_1, m_b_1)?,
            IdentityOutput(vec![pairing_inner_product(m_a_1, m_b_1)?]),
        );
        let com_2 = (
            pairing_inner_product(m_a_2, ck_a_2)?,
            pairing_inner_product(ck_b_2, m_b_2)?,
            IdentityOutput(vec![pairing_inner_product(m_a_2, m_b_2)?]),
        );
        let (c, c_inv) = derive_gipa_challenge(
            family_id,
            context,
            trace,
            stage_label,
            &r_transcript,
            &com_1,
            &com_2,
        )?;

        m_a = rescale_fold(m_a_1, m_a_2, &c);
        m_b = rescale_fold(m_b_2, m_b_1, &c_inv);
        ck_a = rescale_fold(ck_a_2, ck_a_1, &c_inv);
        ck_b = rescale_fold(ck_b_1, ck_b_2, &c);
        r_commitment_steps.push((com_1, com_2));
        r_transcript.push(c);
    };
    r_transcript.reverse();
    r_commitment_steps.reverse();

    Ok((
        ReferenceGipaAbProof {
            r_commitment_steps,
            r_base: m_base,
        },
        GipaAbAux {
            r_transcript,
            ck_base,
        },
    ))
}

fn prove_gipa_c(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    values: (&[G1], &[Fr]),
    ck_a: &[G2],
) -> ReferenceResult<(ReferenceGipaCProof, GipaCAux)> {
    let mut m_a = values.0.to_vec();
    let mut m_b = values.1.to_vec();
    let mut ck_a = ck_a.to_vec();
    let mut r_commitment_steps = Vec::new();
    let mut r_transcript = Vec::new();
    let (m_base, ck_base) = loop {
        if m_a.len() == 1 {
            break ((m_a[0], m_b[0]), ck_a[0]);
        }
        let split = m_a.len() / 2;
        let m_a_1 = &m_a[split..];
        let m_a_2 = &m_a[..split];
        let ck_a_1 = &ck_a[..split];
        let ck_a_2 = &ck_a[split..];
        let m_b_1 = &m_b[..split];
        let m_b_2 = &m_b[split..];

        let com_1 = (
            pairing_inner_product(m_a_1, ck_a_1)?,
            Fr::zero(),
            IdentityOutput(vec![multiexp_inner_product(m_a_1, m_b_1)?]),
        );
        let com_2 = (
            pairing_inner_product(m_a_2, ck_a_2)?,
            Fr::zero(),
            IdentityOutput(vec![multiexp_inner_product(m_a_2, m_b_2)?]),
        );
        let (c, c_inv) = derive_gipa_challenge_c(
            family_id,
            context,
            trace,
            stage_label,
            &r_transcript,
            &com_1,
            &com_2,
        )?;

        m_a = rescale_fold(m_a_1, m_a_2, &c);
        m_b = rescale_fold(m_b_2, m_b_1, &c_inv);
        ck_a = rescale_fold(ck_a_2, ck_a_1, &c_inv);
        r_commitment_steps.push((com_1, com_2));
        r_transcript.push(c);
    };
    r_transcript.reverse();
    r_commitment_steps.reverse();

    Ok((
        ReferenceGipaCProof {
            r_commitment_steps,
            r_base: m_base,
        },
        GipaCAux {
            r_transcript,
            ck_base,
        },
    ))
}

fn compute_recursive_challenges_ab(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    com: AbCommitmentTriple,
    proof: &ReferenceGipaAbProof,
) -> ReferenceResult<(AbCommitmentTriple, Vec<Fr>)> {
    let (mut com_a, mut com_b, mut com_t) = com;
    let mut r_transcript = Vec::new();
    for (com_1, com_2) in proof.r_commitment_steps.iter().rev() {
        let (c, c_inv) = derive_gipa_challenge(
            family_id,
            context,
            trace,
            stage_label,
            &r_transcript,
            com_1,
            com_2,
        )?;
        com_a = scale(com_1.0.clone(), &c) + com_a + scale(com_2.0.clone(), &c_inv);
        com_b = scale(com_1.1.clone(), &c) + com_b + scale(com_2.1.clone(), &c_inv);
        com_t = scale(com_1.2.clone(), &c) + com_t + scale(com_2.2.clone(), &c_inv);
        r_transcript.push(c);
    }
    r_transcript.reverse();
    Ok(((com_a, com_b, com_t), r_transcript))
}

fn compute_recursive_challenges_c(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    com: CCommitmentTriple,
    proof: &ReferenceGipaCProof,
) -> ReferenceResult<(CCommitmentTriple, Vec<Fr>)> {
    let (mut com_a, mut com_b, mut com_t) = com;
    let mut r_transcript = Vec::new();
    for (com_1, com_2) in proof.r_commitment_steps.iter().rev() {
        let (c, c_inv) = derive_gipa_challenge_c(
            family_id,
            context,
            trace,
            stage_label,
            &r_transcript,
            com_1,
            com_2,
        )?;
        com_a = scale(com_1.0.clone(), &c) + com_a + scale(com_2.0.clone(), &c_inv);
        com_b = (com_1.1 * c) + com_b + (com_2.1 * c_inv);
        com_t = scale(com_1.2.clone(), &c) + com_t + scale(com_2.2.clone(), &c_inv);
        r_transcript.push(c);
    }
    r_transcript.reverse();
    Ok(((com_a, com_b, com_t), r_transcript))
}

fn derive_randomizer(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
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
        let (preimage, digest) = challenge_digest_for_trace(
            trace,
            family_id,
            context,
            b"aggregate.randomizer",
            nonce,
            &messages,
            "groth16.randomizer",
        );
        trace.record_challenge(
            "groth16.randomizer",
            b"aggregate.randomizer",
            nonce,
            preimage,
            &digest,
        );
        if let Some(r) = Fr::from_random_bytes(&digest) {
            return Ok(r);
        }
        nonce += 1;
    }
}

fn derive_gipa_challenge(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    transcript: &[Fr],
    com_1: &AbCommitmentTriple,
    com_2: &AbCommitmentTriple,
) -> ReferenceResult<(Fr, Fr)> {
    let previous = transcript.last().copied().unwrap_or_default();
    let mut nonce = 0u64;
    loop {
        let mut messages = Vec::new();
        previous
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        serialize_ab_commitment(com_1, &mut messages)?;
        serialize_ab_commitment(com_2, &mut messages)?;
        let (preimage, digest) = challenge_digest_for_trace(
            trace,
            family_id,
            context,
            stage_label,
            nonce,
            &messages,
            "gipa.challenge-dependency",
        );
        trace.record_challenge(
            "gipa.challenge-dependency",
            stage_label,
            nonce,
            preimage,
            &digest,
        );
        let c: Fr = u128::from_be_bytes(digest[0..16].try_into().map_err(|_| {
            ReferencePathError::MalformedProof("short challenge digest".to_owned())
        })?)
        .into();
        if let Some(c_inv) = c.inverse() {
            return Ok((c_inv, c));
        }
        nonce += 1;
    }
}

fn derive_gipa_challenge_c(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    transcript: &[Fr],
    com_1: &CCommitmentTriple,
    com_2: &CCommitmentTriple,
) -> ReferenceResult<(Fr, Fr)> {
    let previous = transcript.last().copied().unwrap_or_default();
    let mut nonce = 0u64;
    loop {
        let mut messages = Vec::new();
        previous
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        serialize_c_commitment(com_1, &mut messages)?;
        serialize_c_commitment(com_2, &mut messages)?;
        let (preimage, digest) = challenge_digest_for_trace(
            trace,
            family_id,
            context,
            stage_label,
            nonce,
            &messages,
            "gipa.challenge-dependency",
        );
        trace.record_challenge(
            "gipa.challenge-dependency",
            stage_label,
            nonce,
            preimage,
            &digest,
        );
        let c: Fr = u128::from_be_bytes(digest[0..16].try_into().map_err(|_| {
            ReferencePathError::MalformedProof("short challenge digest".to_owned())
        })?)
        .into();
        if let Some(c_inv) = c.inverse() {
            return Ok((c_inv, c));
        }
        nonce += 1;
    }
}

fn derive_kzg_challenge_ab(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    transcript: &[Fr],
    ck_a_final: &G2,
    ck_b_final: &G1,
) -> ReferenceResult<Fr> {
    let mut nonce = 0u64;
    loop {
        let mut messages = Vec::new();
        if let Some(first) = transcript.first() {
            first
                .serialize_uncompressed(&mut messages)
                .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        }
        ck_a_final
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        ck_b_final
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        let (preimage, digest) = challenge_digest_for_trace(
            trace,
            family_id,
            context,
            stage_label,
            nonce,
            &messages,
            "tipa.ab.kzg-challenge",
        );
        trace.record_challenge(
            "tipa.ab.kzg-challenge",
            stage_label,
            nonce,
            preimage,
            &digest,
        );
        if let Some(c) = Fr::from_random_bytes(&digest) {
            return Ok(c);
        }
        nonce += 1;
    }
}

fn derive_kzg_challenge_c(
    family_id: ProofFamilyId,
    context: &[u8; 32],
    trace: &mut ReferenceTraceCollector,
    stage_label: &'static [u8],
    transcript: &[Fr],
    ck_a_final: &G2,
) -> ReferenceResult<Fr> {
    let mut nonce = 0u64;
    loop {
        let mut messages = Vec::new();
        if let Some(first) = transcript.first() {
            first
                .serialize_uncompressed(&mut messages)
                .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        }
        ck_a_final
            .serialize_uncompressed(&mut messages)
            .map_err(|err| ReferencePathError::MalformedProof(err.to_string()))?;
        let (preimage, digest) = challenge_digest_for_trace(
            trace,
            family_id,
            context,
            stage_label,
            nonce,
            &messages,
            "ssm.kzg-challenge",
        );
        trace.record_challenge("ssm.kzg-challenge", stage_label, nonce, preimage, &digest);
        if let Some(c) = Fr::from_random_bytes(&digest) {
            return Ok(c);
        }
        nonce += 1;
    }
}

fn challenge_digest_for_trace(
    trace: &ReferenceTraceCollector,
    family_id: ProofFamilyId,
    context: &[u8; 32],
    stage_label: &'static [u8],
    nonce: u64,
    messages: &[u8],
    spec_row_id: &'static str,
) -> (Vec<u8>, Vec<u8>) {
    let mut context = *context;
    if trace.mutates_context_constructor() {
        context = [0u8; 32];
    }
    let stage_label = if trace.mutates_stage_labels() {
        b"mutant.stage-labels".as_slice()
    } else {
        stage_label
    };
    let empty_messages = [];
    let messages = if trace.mutates_challenge_preimage() || trace.mutates_spec_row(spec_row_id) {
        empty_messages.as_slice()
    } else {
        messages
    };

    let preimage = challenge_preimage(&context, stage_label, nonce, messages);
    let mut digest = Blake2b::new();
    digest.update(transcript_family_domain(family_id).as_slice());
    digest.update(&preimage);
    (preimage, digest.finalize().to_vec())
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

fn challenge_context_constructor_preimage(statement_digest: [u8; 32]) -> Vec<u8> {
    let mut preimage = Vec::with_capacity(CHALLENGE_CONTEXT_DOMAIN.len() + 32);
    preimage.extend_from_slice(CHALLENGE_CONTEXT_DOMAIN);
    preimage.extend_from_slice(&statement_digest);
    preimage
}

fn transcript_family_domain(family_id: ProofFamilyId) -> Vec<u8> {
    match family_id {
        ProofFamilyId::Transfer => b"penumbra.snarkpack.transfer.v1".to_vec(),
        ProofFamilyId::Consolidate(family_id) => {
            format!("penumbra.snarkpack.{}.v1", family_id.label()).into_bytes()
        }
        ProofFamilyId::Split(family_id) => {
            format!("penumbra.snarkpack.{}.v1", family_id.label()).into_bytes()
        }
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => {
            format!("penumbra.snarkpack.{}.v1", family_id.label()).into_bytes()
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
            "penumbra.proof_aggregation.srs.v{}:backend={}:curve={}:max_padded_count={}",
            penumbra_sdk_proof_aggregation::DEV_SRS_VERSION,
            penumbra_sdk_proof_aggregation::DEV_SRS_BACKEND_ID,
            penumbra_sdk_proof_aggregation::DEV_SRS_CURVE_ID,
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

fn build_shifted_ck_1(ck_1: &[G2], r: &Fr) -> Vec<G2> {
    let inverse_powers = inverse_powers(ck_1.len(), r);
    ck_1.iter()
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

fn serialize_c_commitment(com: &CCommitmentTriple, out: &mut Vec<u8>) -> ReferenceResult<()> {
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

fn stage_label_str(stage_label: &'static [u8]) -> &'static str {
    std::str::from_utf8(stage_label).expect("SnarkPack stage labels are static ASCII")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifierMutant {
    ContextConstructor,
    ChallengePreimage,
    StageLabels,
    GipaChallengeDependency,
    TipaAbKzgChallenge,
    SsmKzgChallenge,
    Groth16Randomizer,
}

impl fmt::Display for VerifierMutant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spec_row_id())
    }
}

impl VerifierMutant {
    pub const fn spec_row_id(self) -> &'static str {
        match self {
            Self::ContextConstructor => "fs.context-constructor",
            Self::ChallengePreimage => "fs.challenge-preimage",
            Self::StageLabels => "fs.stage-labels",
            Self::GipaChallengeDependency => "gipa.challenge-dependency",
            Self::TipaAbKzgChallenge => "tipa.ab.kzg-challenge",
            Self::SsmKzgChallenge => "ssm.kzg-challenge",
            Self::Groth16Randomizer => "groth16.randomizer",
        }
    }
}

pub const VERIFIER_MUTANTS: &[VerifierMutant] = &[
    VerifierMutant::ContextConstructor,
    VerifierMutant::ChallengePreimage,
    VerifierMutant::StageLabels,
    VerifierMutant::GipaChallengeDependency,
    VerifierMutant::TipaAbKzgChallenge,
    VerifierMutant::SsmKzgChallenge,
    VerifierMutant::Groth16Randomizer,
];

pub fn filecoin_shape_bug_class_events() -> Vec<TraceEvent> {
    [
        FilecoinBugClass::FirstRoundHashOmission,
        FilecoinBugClass::FinalRandomnessOmission,
        FilecoinBugClass::PublicMessageReordering,
        FilecoinBugClass::HiddenDefaultContext,
        FilecoinBugClass::ProverVerifierChallengeMismatch,
    ]
    .into_iter()
    .map(|bug_class| TraceEvent {
        spec_row_id: "fs.filecoin-bug-class",
        primary_level: TraceComparisonLevel::FilecoinShape,
        event_kind: TraceEventKind::FilecoinBugClass,
        stage_label: "bellperson.v0.21.0",
        nonce: None,
        round_index: None,
        byte_payload: Vec::new(),
        abstract_payload: Some("filecoin-shape"),
        filecoin_bug_class: Some(bug_class),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use ark_snark::SNARK;
    use penumbra_sdk_proof_aggregation::{
        aggregate_family, aggregate_family_with_trace, decode_wrapped_aggregate_proof,
        encode_wrapped_aggregate_proof, pad_items_to_power_of_two, verify_family_aggregate,
        verify_family_aggregate_with_trace, AGGREGATE_PROTOCOL_VERSION, MAX_AGGREGATE_PROOF_BYTES,
    };
    use penumbra_sdk_proof_params::batch;
    use penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId;
    use proptest::prelude::*;

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
        let padded = pad_items_to_power_of_two(&items, srs.max_padded_count as usize)
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

    fn parity_families() -> [ProofFamilyId; 4] {
        [
            ProofFamilyId::Transfer,
            ProofFamilyId::Consolidate(penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS[0].id),
            ProofFamilyId::Split(penumbra_sdk_shielded_pool::SPLIT_FAMILY_SPECS[0].id),
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
        ]
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

    fn wrong_pvk() -> PreparedVerifyingKey<P> {
        let mut rng = ChaCha20Rng::seed_from_u64(99);
        let pk = Groth16::<P, LibsnarkReduction>::generate_random_parameters_with_reduction(
            SquareCircuit {
                x: Some(Fq::from(1u64)),
            },
            &mut rng,
        )
        .expect("setup should succeed");
        PreparedVerifyingKey::from(pk.vk)
    }

    fn mutated_public_input_statement(
        original: &AggregateStatement,
        pvk: &PreparedVerifyingKey<P>,
        srs: &DevSrs,
    ) -> AggregateStatement {
        let mut rows = original.padded_public_inputs().to_vec();
        rows[0][0] += Fq::from(1u64);
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            original.family_id(),
            srs_id(srs),
            pvk,
            original.real_count(),
            &rows,
        )
        .expect("mutated statement should build")
    }

    fn wrong_family_statement(
        original: &AggregateStatement,
        pvk: &PreparedVerifyingKey<P>,
        srs: &DevSrs,
    ) -> AggregateStatement {
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::ShieldedIcs20Withdrawal(
                penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical,
            ),
            srs_id(srs),
            pvk,
            original.real_count(),
            original.padded_public_inputs(),
        )
        .expect("wrong-family statement should build")
    }

    fn wrong_srs_statement(
        original: &AggregateStatement,
        pvk: &PreparedVerifyingKey<P>,
        srs: &DevSrs,
    ) -> AggregateStatement {
        let mut wrong_srs_id = srs_id(srs);
        wrong_srs_id[0] ^= 0x01;
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            original.family_id(),
            wrong_srs_id,
            pvk,
            original.real_count(),
            original.padded_public_inputs(),
        )
        .expect("wrong-SRS statement should build")
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ExpectedInputRejection {
        InvalidInput,
        MalformedProof,
        Rejected,
    }

    struct InputMutationCase {
        statement: AggregateStatement,
        pvk: PreparedVerifyingKey<P>,
        proof: Vec<u8>,
        srs: DevSrs,
    }

    struct InputFixture {
        pvk: PreparedVerifyingKey<P>,
        items: Vec<BatchItem>,
        statement: AggregateStatement,
        srs: DevSrs,
        proof: Vec<u8>,
    }

    impl InputFixture {
        fn new() -> Self {
            let (pvk, items, statement, srs) = fixture();
            let proof = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
            Self {
                pvk,
                items,
                statement,
                srs,
                proof,
            }
        }

        fn valid_case(&self) -> InputMutationCase {
            InputMutationCase {
                statement: self.statement.clone(),
                pvk: self.pvk.clone(),
                proof: self.proof.clone(),
                srs: self.srs,
            }
        }
    }

    struct InputMutant {
        name: &'static str,
        spec_row_id: &'static str,
        expected: ExpectedInputRejection,
        apply: fn(&InputFixture) -> Result<InputMutationCase, ReferencePathError>,
    }

    fn mutate_public_input(
        fixture: &InputFixture,
    ) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.statement =
            mutated_public_input_statement(&fixture.statement, &fixture.pvk, &fixture.srs);
        Ok(case)
    }

    fn mutate_real_count(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            fixture.statement.family_id(),
            srs_id(&fixture.srs),
            &fixture.pvk,
            1,
            fixture.statement.padded_public_inputs(),
        )
        .map(|statement| InputMutationCase {
            statement,
            ..fixture.valid_case()
        })
        .map_err(|err| ReferencePathError::InvalidInput(err.to_string()))
    }

    fn mutate_family_id(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.statement = wrong_family_statement(&fixture.statement, &fixture.pvk, &fixture.srs);
        Ok(case)
    }

    fn mutate_srs_id(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.statement = wrong_srs_statement(&fixture.statement, &fixture.pvk, &fixture.srs);
        Ok(case)
    }

    fn mutate_vk_digest(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.pvk = wrong_pvk();
        Ok(case)
    }

    fn mutate_padding(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        let mut rows = fixture
            .items
            .iter()
            .map(|item| item.public_inputs.clone())
            .collect::<Vec<_>>();
        rows[1][0] += Fq::from(1u64);
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            fixture.statement.family_id(),
            srs_id(&fixture.srs),
            &fixture.pvk,
            1,
            &rows,
        )
        .map(|statement| InputMutationCase {
            statement,
            ..fixture.valid_case()
        })
        .map_err(|err| ReferencePathError::InvalidInput(err.to_string()))
    }

    fn mutate_wrapper_digest(
        fixture: &InputFixture,
    ) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.proof[40] ^= 0x01;
        Ok(case)
    }

    fn mutate_inner_proof_byte(
        fixture: &InputFixture,
    ) -> Result<InputMutationCase, ReferencePathError> {
        let inner = decode_wrapped_aggregate_proof(
            &fixture.proof,
            fixture.statement.statement_digest(),
            None,
        )
        .map_err(|err| ReferencePathError::Rejected(err.to_string()))?;
        let mut mutated_inner = Vec::new();
        mutated_inner.extend_from_slice(inner);
        mutated_inner[0] ^= 0x01;
        let mut case = fixture.valid_case();
        case.proof =
            encode_wrapped_aggregate_proof(fixture.statement.statement_digest(), &mutated_inner)
                .map_err(|err| ReferencePathError::InvalidInput(err.to_string()))?;
        Ok(case)
    }

    fn mutate_truncation(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.proof.truncate(case.proof.len() / 2);
        Ok(case)
    }

    fn mutate_oversize(fixture: &InputFixture) -> Result<InputMutationCase, ReferencePathError> {
        let mut case = fixture.valid_case();
        case.proof = vec![0u8; MAX_AGGREGATE_PROOF_BYTES + 1];
        Ok(case)
    }

    const INPUT_MUTANTS: &[InputMutant] = &[
        InputMutant {
            name: "public-input",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_public_input,
        },
        InputMutant {
            name: "real-count",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::InvalidInput,
            apply: mutate_real_count,
        },
        InputMutant {
            name: "family-id",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_family_id,
        },
        InputMutant {
            name: "srs-id",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_srs_id,
        },
        InputMutant {
            name: "vk-digest",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_vk_digest,
        },
        InputMutant {
            name: "padding-noncanonical",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::InvalidInput,
            apply: mutate_padding,
        },
        InputMutant {
            name: "wrapper-digest",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_wrapper_digest,
        },
        InputMutant {
            name: "inner-proof-byte",
            spec_row_id: "groth16.randomizer",
            expected: ExpectedInputRejection::MalformedProof,
            apply: mutate_inner_proof_byte,
        },
        InputMutant {
            name: "truncation",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_truncation,
        },
        InputMutant {
            name: "oversize",
            spec_row_id: "fs.context-constructor",
            expected: ExpectedInputRejection::Rejected,
            apply: mutate_oversize,
        },
    ];

    fn assert_reference_rejects(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<P>,
        proof: &[u8],
        srs: &DevSrs,
        label: &str,
    ) {
        match reference_verify_family_aggregate(statement, pvk, proof, srs) {
            Ok(report) => assert!(!report.accepted, "{label} should not be accepted"),
            Err(_) => {}
        }
    }

    fn validate_trace(trace: &[TraceEvent]) {
        assert!(!trace.is_empty(), "trace should not be empty");
        for event in trace {
            event.validate().expect("trace event should satisfy policy");
        }
    }

    fn penumbra_byte_trace_rows() -> BTreeSet<&'static str> {
        TRACE_POLICIES
            .iter()
            .filter(|policy| policy.primary_level == TraceComparisonLevel::PenumbraByte)
            .map(|policy| policy.spec_row_id)
            .collect()
    }

    fn assert_expected_rejection(
        mutant: &InputMutant,
        result: Result<InputMutationCase, ReferencePathError>,
    ) {
        match (mutant.expected, result) {
            (ExpectedInputRejection::InvalidInput, Err(ReferencePathError::InvalidInput(_))) => {}
            (ExpectedInputRejection::MalformedProof, Ok(case)) => {
                let err = reference_verify_family_aggregate(
                    &case.statement,
                    &case.pvk,
                    &case.proof,
                    &case.srs,
                )
                .expect_err("input mutant should reject as malformed proof");
                assert!(
                    matches!(err, ReferencePathError::MalformedProof(_)),
                    "input mutant {} rejected with wrong error: {err}",
                    mutant.name
                );
            }
            (ExpectedInputRejection::Rejected, Ok(case)) => {
                assert_reference_rejects(
                    &case.statement,
                    &case.pvk,
                    &case.proof,
                    &case.srs,
                    mutant.name,
                );
            }
            (expected, _) => panic!("input mutant {} expected {expected:?}", mutant.name),
        }
    }

    fn reference_parity_case(
        count: usize,
        seed: u64,
        family_index: usize,
        mutation: u8,
    ) -> Result<(), TestCaseError> {
        let (pvk, items) = sample_items_with_count(seed, count);
        let srs = DevSrs::default();
        let padded_items = pad_items_to_power_of_two(&items, srs.max_padded_count as usize)
            .expect("padding should succeed");
        let family_id = parity_families()[family_index];
        let statement = statement_for_items(family_id, &pvk, count, &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs).expect("aggregate");

        let batch_accepts = batch::batch_verify(&pvk, &padded_items).is_ok();
        let production_accepts =
            verify_family_aggregate(&statement, &pvk, &aggregate, &srs).is_ok();
        let reference_accepts =
            reference_verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
                .map(|report| report.accepted)
                .unwrap_or(false);
        prop_assert_eq!(production_accepts, batch_accepts);
        prop_assert_eq!(reference_accepts, batch_accepts);

        let mut mutated_items = padded_items.clone();
        let mut mutated_statement = statement.clone();
        let mut mutated_aggregate = aggregate.clone();
        match mutation % 2 {
            0 => {
                for item in &mut mutated_items {
                    item.proof.c = Default::default();
                }
                mutated_aggregate = aggregate_family(&statement, &pvk, &mutated_items, &srs)
                    .expect("mutated proof aggregation should serialize");
            }
            1 => {
                for item in &mut mutated_items {
                    item.public_inputs[0] += Fq::from(1u64);
                }
                mutated_statement =
                    statement_for_items(family_id, &pvk, count, &mutated_items, &srs);
            }
            _ => {}
        }

        let batch_accepts = batch::batch_verify(&pvk, &mutated_items).is_ok();
        let production_accepts =
            verify_family_aggregate(&mutated_statement, &pvk, &mutated_aggregate, &srs).is_ok();
        let reference_accepts =
            reference_verify_family_aggregate(&mutated_statement, &pvk, &mutated_aggregate, &srs)
                .map(|report| report.accepted)
                .unwrap_or(false);
        prop_assert!(!batch_accepts, "mutated batch oracle must reject");
        prop_assert_eq!(production_accepts, batch_accepts);
        prop_assert_eq!(reference_accepts, batch_accepts);
        Ok(())
    }

    #[test]
    fn reference_verifier_accepts_production_prover() {
        let (pvk, items, statement, srs) = fixture();
        let production = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
        let report = reference_verify_family_aggregate(&statement, &pvk, &production, &srs)
            .expect("reference verifier should run");
        assert!(report.accepted);
        assert!(report
            .verifier_trace
            .iter()
            .any(|event| event.spec_row_id == "groth16.randomizer"));
    }

    #[test]
    fn reference_prover_cross_verifies_with_production() {
        let (pvk, items, statement, srs) = fixture();
        let reference = reference_aggregate_family(&statement, &pvk, &items, &srs)
            .expect("reference aggregate");
        verify_family_aggregate(&statement, &pvk, &reference.wrapped_proof, &srs)
            .expect("production verifier should accept reference aggregate");
        let report =
            reference_verify_family_aggregate(&statement, &pvk, &reference.wrapped_proof, &srs)
                .expect("reference verifier should run");
        assert!(report.accepted);
        assert!(!reference.prover_trace.is_empty());
    }

    #[test]
    fn production_and_reference_traces_match_declared_levels() {
        let (pvk, items, statement, srs) = fixture();
        let (production_proof, production_prover_trace) =
            aggregate_family_with_trace(&statement, &pvk, &items, &srs)
                .expect("production aggregate with trace");
        let production_report =
            verify_family_aggregate_with_trace(&statement, &pvk, &production_proof, &srs)
                .expect("production verify with trace");
        assert!(production_report.accepted);
        validate_trace(&production_prover_trace);
        validate_trace(&production_report.trace);
        assert_eq!(production_prover_trace, production_report.trace);

        let reference_report =
            reference_verify_family_aggregate(&statement, &pvk, &production_proof, &srs)
                .expect("reference verifier should run");
        assert!(reference_report.accepted);
        validate_trace(&reference_report.verifier_trace);
        assert_eq!(production_prover_trace, reference_report.verifier_trace);

        let reference = reference_aggregate_family(&statement, &pvk, &items, &srs)
            .expect("reference aggregate");
        let reference_report =
            reference_verify_family_aggregate(&statement, &pvk, &reference.wrapped_proof, &srs)
                .expect("reference verifier should run");
        assert!(reference_report.accepted);
        validate_trace(&reference.prover_trace);
        validate_trace(&reference_report.verifier_trace);
        assert_eq!(reference.prover_trace, reference_report.verifier_trace);

        let production_report =
            verify_family_aggregate_with_trace(&statement, &pvk, &reference.wrapped_proof, &srs)
                .expect("production verifier should run");
        assert!(production_report.accepted);
        validate_trace(&production_report.trace);
        assert_eq!(reference.prover_trace, production_report.trace);
    }

    #[test]
    fn reference_verifier_rejects_mutated_wrapper_digest() {
        let (pvk, items, statement, srs) = fixture();
        let mut production = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
        production[40] ^= 0x01;
        let err = reference_verify_family_aggregate(&statement, &pvk, &production, &srs)
            .expect_err("digest mutation should reject");
        assert!(matches!(err, ReferencePathError::Rejected(_)));
    }

    #[test]
    fn reference_verifier_rejects_malformed_proof_bytes() {
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
        assert!(matches!(err, ReferencePathError::MalformedProof(_)));
    }

    #[test]
    fn reference_verifier_rejects_required_input_mutations() {
        let fixture = InputFixture::new();
        for mutant in INPUT_MUTANTS {
            assert_expected_rejection(mutant, (mutant.apply)(&fixture));
        }
    }

    #[test]
    fn input_mutant_matrix_is_declared_per_byte_binding_row() {
        let names = INPUT_MUTANTS
            .iter()
            .map(|mutant| mutant.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "public-input",
                "real-count",
                "family-id",
                "srs-id",
                "vk-digest",
                "padding-noncanonical",
                "wrapper-digest",
                "inner-proof-byte",
                "truncation",
                "oversize",
            ]
        );
    }

    #[test]
    fn verifier_mutants_reject_valid_proofs() {
        let (pvk, items, statement, srs) = fixture();
        let production = aggregate_family(&statement, &pvk, &items, &srs).expect("aggregate");
        for mutant in VERIFIER_MUTANTS {
            assert!(
                penumbra_byte_trace_rows().contains(mutant.spec_row_id()),
                "verifier mutant {mutant} must name a PenumbraByte trace row"
            );
            let report = reference_verify_family_aggregate_with_verifier_mutant(
                &statement,
                &pvk,
                &production,
                &srs,
                *mutant,
            )
            .unwrap_or_else(|err| panic!("mutant {mutant} should run and reject, got error {err}"));
            assert!(
                !report.accepted,
                "verifier mutant {mutant} should reject a valid proof"
            );
        }
    }

    #[test]
    fn verifier_mutant_matrix_is_declared_per_byte_binding_row() {
        let names = VERIFIER_MUTANTS
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "fs.context-constructor",
                "fs.challenge-preimage",
                "fs.stage-labels",
                "gipa.challenge-dependency",
                "tipa.ab.kzg-challenge",
                "ssm.kzg-challenge",
                "groth16.randomizer",
            ]
        );
    }

    #[test]
    fn mutation_matrices_cover_penumbra_byte_trace_rows() {
        let expected = penumbra_byte_trace_rows();
        let covered = INPUT_MUTANTS
            .iter()
            .map(|mutant| mutant.spec_row_id)
            .chain(VERIFIER_MUTANTS.iter().map(|mutant| mutant.spec_row_id()))
            .collect::<BTreeSet<_>>();
        assert!(
            expected.is_subset(&covered),
            "input/verifier mutation matrices must cover every PenumbraByte row: expected={expected:?}, covered={covered:?}"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1))]

        #[test]
        fn reference_property_matches_production_and_batch_oracles(
            count in 1usize..=8,
            seed in any::<u64>(),
            family_index in 0usize..4,
            mutation in 0u8..2,
        ) {
            reference_parity_case(count, seed, family_index, mutation)?;
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4))]

        #[test]
        #[ignore]
        fn reference_property_matches_production_and_batch_oracles_slow(
            count in 1usize..=8,
            seed in any::<u64>(),
            family_index in 0usize..4,
            mutation in 0u8..2,
        ) {
            reference_parity_case(count, seed, family_index, mutation)?;
        }
    }

    #[test]
    fn filecoin_shape_events_cover_all_bug_classes() {
        let events = filecoin_shape_bug_class_events();
        assert_eq!(events.len(), 5);
        for event in events {
            event.validate().expect("filecoin shape marker");
        }
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
        let diff = reference_bytes
            .iter()
            .zip(&production_bytes)
            .position(|(left, right)| left != right);
        assert_eq!(
            diff,
            None,
            "first SRS byte diff at {:?}: ref={:?} prod={:?}",
            diff,
            diff.and_then(|idx| reference_bytes.get(idx)),
            diff.and_then(|idx| production_bytes.get(idx))
        );
        ensure_reference_srs_matches_public_id(&DevSrs::default()).expect("matching srs id");
    }
}
