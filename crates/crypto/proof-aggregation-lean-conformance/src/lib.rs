#![forbid(unsafe_code)]

//! Dev-only differential oracle for SnarkPack transcript/folding structure.
//!
//! The Lean source under `lean/` emits the expected event shape derived from the
//! spec. This crate compares that shape to traces produced through the public
//! proof-aggregation API.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Command,
    sync::{Mutex, OnceLock},
};

use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_aggregation::trace_schema::{TraceEvent, TraceEventKind};
use penumbra_sdk_proof_aggregation::{
    aggregate_family_with_trace, pad_items_to_power_of_two, srs_id, AggregateStatement, DevSrs,
    ProofFamilyId, AGGREGATE_PROTOCOL_VERSION,
};
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use serde::{Deserialize, Serialize};

// A SnarkPack transcript shape is fully determined by `padded_count = next power
// of two of the real count`: the only count-dependent part of the trace is the
// number of GIPA round challenges, which is `log2(padded_count)`. So the complete
// set of distinct shapes is one per power of two up to the SRS bound, and we can
// enumerate it exhaustively instead of sampling it. `DEFAULT_MAX_PADDED_PROOF_COUNT`
// is 2^15, giving 16 distinct shapes per family.
//
// Two bands: SMOKE runs always (cheap); the full sweep to MAX is release-gated
// because materializing a shape of depth R requires generating ~2^R real proofs.
const MAX_SHAPE_POWER: u32 = 15; // 2^15 == DEFAULT_MAX_PADDED_PROOF_COUNT
const SMOKE_MAX_POWER: u32 = 5; // 2^5 == 32; exercises round depths 0..=5

// Non-power-of-two real counts. Each pads up to the next power of two and must
// produce that power's shape, so these exercise padding canonicality. Kept within
// the smoke band (20 pads to 32) so the always-on test stays fast.
const PADDING_REPRESENTATIVES: [usize; 4] = [3, 5, 12, 20];

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConformanceCase {
    pub family_index: usize,
    pub count: usize,
    pub seed: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventShape {
    pub spec_row_id: String,
    pub primary_level: String,
    pub event_kind: String,
    pub stage_label: String,
    pub nonce: Option<u64>,
    pub round_index: Option<u32>,
    pub abstract_payload: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LeanOracleShapes {
    by_count: BTreeMap<usize, Vec<EventShape>>,
}

#[derive(Debug, thiserror::Error)]
pub enum LeanConformanceError {
    #[error("Lean executable not found; set LEAN_BIN or install elan/lean")]
    LeanNotFound,
    #[error("Lean oracle failed: {0}")]
    LeanFailed(String),
    #[error("invalid Lean oracle line: {0}")]
    InvalidOracleLine(String),
    #[error("missing Lean oracle shape for count {0}")]
    MissingOracleCount(usize),
    #[error("Rust trace generation failed: {0}")]
    RustTrace(String),
    #[error("oracle serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

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

fn case_seed(family_index: usize, count: usize) -> u64 {
    9_001 + (family_index as u64) * 10_000 + count as u64
}

fn cases_for_counts(counts: &[usize]) -> Vec<ConformanceCase> {
    let mut cases = Vec::new();
    for family_index in 0..parity_families().len() {
        for &count in counts {
            cases.push(ConformanceCase {
                family_index,
                count,
                seed: case_seed(family_index, count),
            });
        }
    }
    cases
}

fn counts_through_power(max_power: u32) -> Vec<usize> {
    let mut counts: Vec<usize> = (0..=max_power).map(|p| 1usize << p).collect();
    counts.extend(PADDING_REPRESENTATIVES);
    counts.sort_unstable();
    counts.dedup();
    counts
}

/// Distinct counts in the always-on smoke band (powers of two up to
/// `SMOKE_MAX_POWER` plus the padding representatives).
pub fn smoke_counts() -> Vec<usize> {
    counts_through_power(SMOKE_MAX_POWER)
}

/// Every distinct shape the system can produce: powers of two up to the SRS max
/// plus padding representatives. Used by the release-gated exhaustive sweep.
pub fn all_shape_counts() -> Vec<usize> {
    counts_through_power(MAX_SHAPE_POWER)
}

/// Smoke band: 4 families x the smoke counts. Exhaustive over round depths 0..=7.
pub fn default_conformance_cases() -> Vec<ConformanceCase> {
    cases_for_counts(&smoke_counts())
}

/// Full exhaustive enumeration: 4 families x every distinct shape up to 2^15.
/// Expensive (generates up to 2^15 real proofs per family); release-gated.
pub fn all_shape_conformance_cases() -> Vec<ConformanceCase> {
    cases_for_counts(&all_shape_counts())
}

pub fn lean_oracle_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lean/SnarkpackOracle.lean")
}

pub fn lean_oracle_shapes_for_counts(
    counts: &[usize],
) -> Result<LeanOracleShapes, LeanConformanceError> {
    let lean = lean_bin().ok_or(LeanConformanceError::LeanNotFound)?;
    let output = Command::new(lean)
        .arg("--run")
        .arg(lean_oracle_source())
        .args(counts.iter().map(|count| count.to_string()))
        .output()
        .map_err(|err| LeanConformanceError::LeanFailed(err.to_string()))?;
    if !output.status.success() {
        return Err(LeanConformanceError::LeanFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    parse_lean_oracle_output(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_lean_oracle_output(output: &str) -> Result<LeanOracleShapes, LeanConformanceError> {
    let mut shapes = LeanOracleShapes::default();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split('|').collect::<Vec<_>>();
        if fields.len() != 7 || !fields[0].starts_with("count=") {
            return Err(LeanConformanceError::InvalidOracleLine(line.to_owned()));
        }
        let count = fields[0]["count=".len()..]
            .parse::<usize>()
            .map_err(|_| LeanConformanceError::InvalidOracleLine(line.to_owned()))?;
        let nonce = parse_optional_u64(fields[5], line)?;
        let abstract_payload = (fields[6] != "-").then(|| fields[6].to_owned());
        shapes.by_count.entry(count).or_default().push(EventShape {
            spec_row_id: fields[1].to_owned(),
            primary_level: fields[2].to_owned(),
            event_kind: fields[3].to_owned(),
            stage_label: fields[4].to_owned(),
            nonce,
            round_index: None,
            abstract_payload,
        });
    }
    Ok(shapes)
}

pub fn load_oracle_shapes_from_env() -> Result<LeanOracleShapes, LeanConformanceError> {
    match std::env::var("SNARKPACK_LEAN_ORACLE_SHAPES") {
        Ok(path) => {
            let contents = std::fs::read_to_string(path)
                .map_err(|err| LeanConformanceError::LeanFailed(err.to_string()))?;
            Ok(serde_json::from_str(&contents)?)
        }
        Err(_) => lean_oracle_shapes_for_counts(&smoke_counts()),
    }
}

pub fn write_oracle_shapes_json(
    shapes: &LeanOracleShapes,
    path: impl Into<PathBuf>,
) -> Result<(), LeanConformanceError> {
    let path = path.into();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| LeanConformanceError::LeanFailed(err.to_string()))?;
    }
    let rendered = serde_json::to_string_pretty(shapes)?;
    std::fs::write(path, rendered)
        .map_err(|err| LeanConformanceError::LeanFailed(err.to_string()))?;
    Ok(())
}

pub fn assert_case_matches_oracle(
    case: &ConformanceCase,
    oracle: &LeanOracleShapes,
) -> Result<(), LeanConformanceError> {
    let expected = oracle
        .by_count
        .get(&case.count)
        .ok_or(LeanConformanceError::MissingOracleCount(case.count))?;
    let actual = rust_trace_shape(case)?;
    if actual != *expected {
        pretty_assertions(actual, expected, case);
    }
    Ok(())
}

pub fn assert_case_matches_oracle_cached(
    case: ConformanceCase,
    oracle: &LeanOracleShapes,
) -> Result<(), LeanConformanceError> {
    static CACHE: OnceLock<Mutex<BTreeMap<ConformanceCase, Vec<EventShape>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache.lock().expect("conformance cache lock");
    let actual = match cache.get(&case) {
        Some(shape) => shape.clone(),
        None => {
            let shape = rust_trace_shape(&case)?;
            cache.insert(case.clone(), shape.clone());
            shape
        }
    };
    let expected = oracle
        .by_count
        .get(&case.count)
        .ok_or(LeanConformanceError::MissingOracleCount(case.count))?;
    if actual != *expected {
        pretty_assertions(actual, expected, &case);
    }
    Ok(())
}

pub fn rust_trace_shape(case: &ConformanceCase) -> Result<Vec<EventShape>, LeanConformanceError> {
    let family_id = parity_families()[case.family_index % parity_families().len()];
    let (pvk, items) = sample_items_with_count(case.seed, case.count);
    let srs = DevSrs::default();
    let padded_items = pad_items_to_power_of_two(&items, srs.max_padded_count as usize)
        .map_err(|err| LeanConformanceError::RustTrace(err.to_string()))?;
    let rows = padded_items
        .iter()
        .map(|item| item.public_inputs.clone())
        .collect::<Vec<_>>();
    let statement = AggregateStatement::new(
        AGGREGATE_PROTOCOL_VERSION,
        family_id,
        srs_id(&srs),
        &pvk,
        case.count as u32,
        &rows,
    )
    .map_err(|err| LeanConformanceError::RustTrace(err.to_string()))?;

    let (_proof, trace) = aggregate_family_with_trace(&statement, &pvk, &padded_items, &srs)
        .map_err(|err| LeanConformanceError::RustTrace(err.to_string()))?;
    Ok(trace.iter().map(project_event_shape).collect())
}

fn project_event_shape(event: &TraceEvent) -> EventShape {
    EventShape {
        spec_row_id: event.spec_row_id.to_owned(),
        primary_level: event.primary_level.as_str().to_owned(),
        event_kind: event_kind_name(event.event_kind).to_owned(),
        stage_label: event.stage_label.to_owned(),
        nonce: event.nonce,
        round_index: event.round_index,
        abstract_payload: event.abstract_payload.map(ToOwned::to_owned),
    }
}

fn parity_families() -> [ProofFamilyId; 4] {
    [
        ProofFamilyId::Transfer,
        ProofFamilyId::Consolidate(penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS[0].id),
        ProofFamilyId::Split(penumbra_sdk_shielded_pool::SPLIT_FAMILY_SPECS[0].id),
        ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
    ]
}

fn sample_items_with_count(
    seed: u64,
    count: usize,
) -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let setup_circuit = SquareCircuit {
        x: Some(Fq::from(1u64)),
    };
    let pk = Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
        setup_circuit,
        &mut rng,
    )
    .expect("conformance setup should succeed");
    let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();

    let items = (0..count)
        .map(|_| {
            let x = Fq::rand(&mut rng);
            let circuit = SquareCircuit { x: Some(x) };
            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                .expect("conformance proof should succeed");

            BatchItem {
                proof,
                public_inputs: vec![x * x],
            }
        })
        .collect();

    (pvk, items)
}

fn lean_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("LEAN_BIN") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join("lean"))
            .find(|path| path.exists())
    }) {
        return Some(path);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".elan/bin/lean"))
        .filter(|path| path.exists())
}

fn parse_optional_u64(value: &str, line: &str) -> Result<Option<u64>, LeanConformanceError> {
    if value == "-" {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| LeanConformanceError::InvalidOracleLine(line.to_owned()))
}

fn event_kind_name(kind: TraceEventKind) -> &'static str {
    match kind {
        TraceEventKind::ChallengeContext => "ChallengeContext",
        TraceEventKind::ChallengePreimage => "ChallengePreimage",
        TraceEventKind::ChallengeDigest => "ChallengeDigest",
        TraceEventKind::EquationRole => "EquationRole",
        TraceEventKind::ObjectRole => "ObjectRole",
        TraceEventKind::FilecoinBugClass => "FilecoinBugClass",
    }
}

fn pretty_assertions(
    actual: Vec<EventShape>,
    expected: &[EventShape],
    case: &ConformanceCase,
) -> ! {
    panic!(
        "Lean conformance mismatch for family_index={} count={} seed={}\nactual: {:#?}\nexpected: {:#?}",
        case.family_index, case.count, case.seed, actual, expected
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_oracle() -> &'static LeanOracleShapes {
        static ORACLE: OnceLock<LeanOracleShapes> = OnceLock::new();
        ORACLE.get_or_init(|| load_oracle_shapes_from_env().expect("Lean oracle should run"))
    }

    // Smoke band: exhaustive over round depths 0..=7 and the padding
    // representatives, for all four families. Deterministic, no sampling.
    #[test]
    fn lean_oracle_matches_seeded_trace_shapes() {
        let oracle = test_oracle();
        for case in default_conformance_cases() {
            assert_case_matches_oracle(&case, &oracle).expect("Lean/Rust trace shapes match");
        }
    }

    // Full exhaustive sweep: every distinct transcript shape the system can
    // produce (powers of two up to 2^15) x all families, plus padding
    // representatives. This is certainty over the whole bounded shape domain, not
    // probabilistic coverage. Release-gated because the deepest shapes generate
    // up to 2^15 real proofs per family.
    #[test]
    #[ignore = "exhaustive shape sweep to 2^15; generates up to 32768 real proofs per family"]
    fn lean_oracle_matches_all_shapes_to_max() {
        let oracle =
            lean_oracle_shapes_for_counts(&all_shape_counts()).expect("Lean oracle should run");
        for case in all_shape_conformance_cases() {
            assert_case_matches_oracle_cached(case, &oracle).expect("Lean/Rust trace shapes match");
        }
    }

    #[test]
    fn lean_oracle_output_covers_expected_spec_rows() {
        let oracle = test_oracle();
        let count_8 = oracle.by_count.get(&8).expect("count 8 oracle");
        for row in [
            "fs.context-constructor",
            "fs.stage-labels",
            "fs.challenge-preimage",
            "gipa.input-relation",
            "gipa.round-folding",
            "gipa.challenge-dependency",
            "gipa.verifier-folding",
            "tipa.srs",
            "tipa.ab.gipa",
            "tipa.ab.kzg-challenge",
            "tipa.ab.kzg-equations",
            "ssm.power-sequence",
            "ssm.kzg-challenge",
            "ssm.base-equation",
            "groth16.randomizer",
            "groth16.folded-inputs",
            "groth16.ppe",
        ] {
            assert!(
                count_8.iter().any(|event| event.spec_row_id == row),
                "Lean oracle missing spec row {row}"
            );
        }
    }
}
