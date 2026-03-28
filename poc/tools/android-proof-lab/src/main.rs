mod gnark_spend;

use std::fs::File;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, Proof, ProvingKey};
use ark_relations::r1cs::{
    ConstraintSynthesizer, ConstraintSystem, OptimizationGoal, SynthesisMode,
};
use ark_serialize::CanonicalSerialize;
use clap::{Parser, ValueEnum};
use decaf377::{Bls12_377, Fq, Fr};
use gnark_spend::{
    encode_spend_witness_v1, encode_spend_witness_v1_debug, translate_spend_proof_result,
    GnarkSpendClient,
};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_compliance::blind_sender_leaf;
use penumbra_sdk_proof_params::{
    DummyWitness, GROTH16_PROOF_LENGTH_BYTES, OUTPUT_PROOF_PROVING_KEY, SPEND_PROOF_PROVING_KEY,
};
use penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1 as shielded_pool_pb;
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::public_input_hash::{
    output_statement_hash_from_public, spend_statement_hash_from_public,
};
use penumbra_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::{
    generate_test_data, CircuitType, REGULATED_ASSET_ID, UNREGULATED_ASSET_ID,
};
use penumbra_sdk_shielded_pool::{
    output::{OutputProofPrivate, OutputProofPublic},
    OutputCircuit, OutputProof, SpendCircuit, SpendProof, SpendProofPrivate, SpendProofPublic,
};
use penumbra_sdk_tct as tct;
use rand_core::OsRng;
use serde::Serialize;

#[cfg(test)]
use penumbra_sdk_proof_params::{OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CircuitKind {
    Both,
    Spend,
    Output,
    Parallel,
    #[clap(name = "spend2-output2-parallel")]
    Parallel2x2,
    #[clap(name = "spend2-output2-serial")]
    Serial2x2,
}

impl CircuitKind {
    fn concrete(self) -> &'static [CircuitKind] {
        match self {
            CircuitKind::Both => &[
                CircuitKind::Spend,
                CircuitKind::Output,
                CircuitKind::Parallel,
                CircuitKind::Parallel2x2,
            ],
            CircuitKind::Spend => &[CircuitKind::Spend],
            CircuitKind::Output => &[CircuitKind::Output],
            CircuitKind::Parallel => &[CircuitKind::Parallel],
            CircuitKind::Parallel2x2 => &[CircuitKind::Parallel2x2],
            CircuitKind::Serial2x2 => &[CircuitKind::Serial2x2],
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            CircuitKind::Both => "both",
            CircuitKind::Spend => "spend",
            CircuitKind::Output => "output",
            CircuitKind::Parallel => "spend_output_parallel",
            CircuitKind::Parallel2x2 => "spend2_output2_parallel",
            CircuitKind::Serial2x2 => "spend2_output2_serial",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ComplianceCase {
    Both,
    Regulated,
    Unregulated,
}

impl ComplianceCase {
    fn concrete(self) -> &'static [ComplianceCase] {
        match self {
            ComplianceCase::Both => &[ComplianceCase::Regulated, ComplianceCase::Unregulated],
            ComplianceCase::Regulated => &[ComplianceCase::Regulated],
            ComplianceCase::Unregulated => &[ComplianceCase::Unregulated],
        }
    }

    fn is_regulated(self) -> bool {
        matches!(self, ComplianceCase::Regulated)
    }

    fn asset_id(self) -> u64 {
        match self {
            ComplianceCase::Regulated => REGULATED_ASSET_ID,
            ComplianceCase::Unregulated => UNREGULATED_ASSET_ID,
            ComplianceCase::Both => unreachable!("concrete compliance case required"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ComplianceCase::Both => "both",
            ComplianceCase::Regulated => "regulated",
            ComplianceCase::Unregulated => "unregulated",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Csv,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Backend {
    Arkworks,
    Gnark,
}

impl Backend {
    fn as_str(self) -> &'static str {
        match self {
            Backend::Arkworks => "arkworks",
            Backend::Gnark => "gnark",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ProfileMode {
    None,
    Stage,
    Simpleperf,
    Both,
}

impl ProfileMode {
    fn captures_stage_timings(self) -> bool {
        matches!(self, ProfileMode::Stage | ProfileMode::Both)
    }

    fn as_str(self) -> &'static str {
        match self {
            ProfileMode::None => "none",
            ProfileMode::Stage => "stage",
            ProfileMode::Simpleperf => "simpleperf",
            ProfileMode::Both => "both",
        }
    }
}

#[derive(Debug, Parser)]
#[clap(
    name = "android_proof_lab",
    about = "Benchmark spend/output proof generation in an Android-friendly CLI"
)]
struct Cli {
    #[clap(long, value_enum, default_value_t = Backend::Arkworks)]
    backend: Backend,
    #[clap(long, value_enum, default_value_t = CircuitKind::Both)]
    circuit: CircuitKind,
    #[clap(long, value_enum, default_value_t = ComplianceCase::Regulated)]
    compliance_case: ComplianceCase,
    #[clap(long, default_value_t = 1)]
    cold_iterations: usize,
    #[clap(long, default_value_t = 1)]
    warm_iterations: usize,
    #[clap(long, value_enum, default_value_t = ProfileMode::None)]
    profile_mode: ProfileMode,
    #[clap(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
    #[clap(long)]
    output: Option<PathBuf>,
    #[clap(long)]
    print_header: bool,
    #[clap(long)]
    count_constraints: bool,
    #[clap(long)]
    device_label: Option<String>,
    #[clap(long)]
    gnark_lib: Option<PathBuf>,
    #[clap(long)]
    gnark_artifact_dir: Option<PathBuf>,
    #[clap(long)]
    debug_witness_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
struct BenchmarkTarget {
    circuit: CircuitKind,
    compliance_case: ComplianceCase,
}

#[derive(Clone, Debug)]
struct GnarkConfig {
    lib: PathBuf,
    artifact_dir: PathBuf,
    debug_witness_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct WitnessDebugSidecar {
    compliance_case: String,
    claimed_statement_hash: String,
    statement_fields: Vec<String>,
    payload_sha256: String,
}

#[derive(Clone, Copy, Debug)]
struct CircuitMetrics {
    constraints: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct ProofStageBreakdown {
    instance_build_ms: f64,
    statement_hash_ms: f64,
    circuit_build_ms: f64,
    pk_load_ms: f64,
    create_proof_ms: f64,
    serialize_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct GnarkStageBreakdown {
    witness_pack_ms: f64,
    prove_path_ms: f64,
    proof_translate_ms: f64,
}

impl GnarkStageBreakdown {
    fn add_assign(&mut self, other: Self) {
        self.witness_pack_ms += other.witness_pack_ms;
        self.prove_path_ms += other.prove_path_ms;
        self.proof_translate_ms += other.proof_translate_ms;
    }

    fn divide(self, divisor: usize) -> Self {
        let divisor = divisor as f64;
        Self {
            witness_pack_ms: self.witness_pack_ms / divisor,
            prove_path_ms: self.prove_path_ms / divisor,
            proof_translate_ms: self.proof_translate_ms / divisor,
        }
    }
}

#[derive(Debug, Serialize)]
struct GnarkStageBreakdownRow {
    witness_pack_ms: f64,
    prove_path_ms: f64,
    proof_translate_ms: f64,
}

impl ProofStageBreakdown {
    fn add_assign(&mut self, other: Self) {
        self.instance_build_ms += other.instance_build_ms;
        self.statement_hash_ms += other.statement_hash_ms;
        self.circuit_build_ms += other.circuit_build_ms;
        self.pk_load_ms += other.pk_load_ms;
        self.create_proof_ms += other.create_proof_ms;
        self.serialize_ms += other.serialize_ms;
    }

    fn divide(self, divisor: usize) -> Self {
        let divisor = divisor as f64;
        Self {
            instance_build_ms: self.instance_build_ms / divisor,
            statement_hash_ms: self.statement_hash_ms / divisor,
            circuit_build_ms: self.circuit_build_ms / divisor,
            pk_load_ms: self.pk_load_ms / divisor,
            create_proof_ms: self.create_proof_ms / divisor,
            serialize_ms: self.serialize_ms / divisor,
        }
    }
}

#[derive(Debug, Serialize)]
struct SummaryRow {
    run_id: String,
    backend: String,
    circuit: String,
    compliance_case: String,
    cold_iterations: usize,
    warm_iterations: usize,
    cold_ms: f64,
    warm_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    constraints: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rayon_threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu_mask: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_instance_build_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_statement_hash_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_circuit_build_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_pk_load_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_create_proof_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_serialize_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_instance_build_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_statement_hash_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_circuit_build_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_pk_load_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_create_proof_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_serialize_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correctness_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gnark_lib_load_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gnark_init_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correctness_check_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cold_prover_breakdown: Option<GnarkStageBreakdownRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warm_prover_breakdown: Option<GnarkStageBreakdownRow>,
    git_rev: String,
    host_label: String,
    timestamp: u64,
}

impl From<GnarkStageBreakdown> for GnarkStageBreakdownRow {
    fn from(value: GnarkStageBreakdown) -> Self {
        Self {
            witness_pack_ms: value.witness_pack_ms,
            prove_path_ms: value.prove_path_ms,
            proof_translate_ms: value.proof_translate_ms,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct IterationMeasurement {
    wall_ms: f64,
    stage: Option<ProofStageBreakdown>,
    gnark_stage: Option<GnarkStageBreakdown>,
}

struct MeasurementSummary {
    backend: Backend,
    cold_ms: f64,
    warm_ms: f64,
    metrics: Option<CircuitMetrics>,
    cold_stage: Option<ProofStageBreakdown>,
    warm_stage: Option<ProofStageBreakdown>,
    cold_gnark_stage: Option<GnarkStageBreakdown>,
    warm_gnark_stage: Option<GnarkStageBreakdown>,
    correctness_verified: Option<bool>,
    gnark_lib_load_ms: Option<f64>,
    gnark_init_ms: Option<f64>,
    correctness_check_ms: Option<f64>,
}

struct ProfiledProofResult<P> {
    proof: P,
    proof_bytes: Vec<u8>,
    stage: ProofStageBreakdown,
    wall_ms: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.cold_iterations == 0 {
        bail!("--cold-iterations must be > 0");
    }
    if cli.warm_iterations == 0 {
        bail!("--warm-iterations must be > 0");
    }
    let gnark_config = match cli.backend {
        Backend::Arkworks => None,
        Backend::Gnark => Some(GnarkConfig {
            lib: cli
                .gnark_lib
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--gnark-lib is required with --backend gnark"))?,
            artifact_dir: cli.gnark_artifact_dir.clone().ok_or_else(|| {
                anyhow::anyhow!("--gnark-artifact-dir is required with --backend gnark")
            })?,
            debug_witness_dir: cli.debug_witness_dir.clone(),
        }),
    };

    let targets = build_targets(cli.circuit, cli.compliance_case);
    let run_id = format!("run-{}-{}", unix_ts(), std::process::id());
    let host_label = cli.device_label.unwrap_or_else(host_label);
    let git_rev = git_rev();
    let rayon_threads = rayon_threads();
    let cpu_mask = std::env::var("BENCH_CPU_MASK").ok();
    let build_mode = std::env::var("BENCH_BUILD_MODE").ok();
    let profile_mode = Some(cli.profile_mode.as_str().to_string());

    let mut rows = Vec::with_capacity(targets.len());
    for target in targets {
        let summary = run_target(
            target,
            cli.cold_iterations,
            cli.warm_iterations,
            cli.count_constraints,
            cli.profile_mode,
            cli.backend,
            gnark_config.as_ref(),
        )?;
        rows.push(SummaryRow {
            run_id: run_id.clone(),
            backend: summary.backend.as_str().to_string(),
            circuit: target.circuit.as_str().to_string(),
            compliance_case: target.compliance_case.as_str().to_string(),
            cold_iterations: cli.cold_iterations,
            warm_iterations: cli.warm_iterations,
            cold_ms: summary.cold_ms,
            warm_ms: summary.warm_ms,
            constraints: summary.metrics.map(|m| m.constraints),
            rayon_threads,
            cpu_mask: cpu_mask.clone(),
            build_mode: build_mode.clone(),
            profile_mode: profile_mode.clone(),
            cold_instance_build_ms: summary.cold_stage.map(|s| s.instance_build_ms),
            cold_statement_hash_ms: summary.cold_stage.map(|s| s.statement_hash_ms),
            cold_circuit_build_ms: summary.cold_stage.map(|s| s.circuit_build_ms),
            cold_pk_load_ms: summary.cold_stage.map(|s| s.pk_load_ms),
            cold_create_proof_ms: summary.cold_stage.map(|s| s.create_proof_ms),
            cold_serialize_ms: summary.cold_stage.map(|s| s.serialize_ms),
            warm_instance_build_ms: summary.warm_stage.map(|s| s.instance_build_ms),
            warm_statement_hash_ms: summary.warm_stage.map(|s| s.statement_hash_ms),
            warm_circuit_build_ms: summary.warm_stage.map(|s| s.circuit_build_ms),
            warm_pk_load_ms: summary.warm_stage.map(|s| s.pk_load_ms),
            warm_create_proof_ms: summary.warm_stage.map(|s| s.create_proof_ms),
            warm_serialize_ms: summary.warm_stage.map(|s| s.serialize_ms),
            correctness_verified: summary.correctness_verified,
            gnark_lib_load_ms: summary.gnark_lib_load_ms,
            gnark_init_ms: summary.gnark_init_ms,
            correctness_check_ms: summary.correctness_check_ms,
            cold_prover_breakdown: summary.cold_gnark_stage.map(Into::into),
            warm_prover_breakdown: summary.warm_gnark_stage.map(Into::into),
            git_rev: git_rev.clone(),
            host_label: host_label.clone(),
            timestamp: unix_ts(),
        });
    }

    write_rows(&rows, cli.format, cli.output.as_deref(), cli.print_header)?;
    Ok(())
}

fn build_targets(circuit: CircuitKind, compliance_case: ComplianceCase) -> Vec<BenchmarkTarget> {
    let mut targets = Vec::new();
    for circuit in circuit.concrete() {
        for compliance_case in compliance_case.concrete() {
            targets.push(BenchmarkTarget {
                circuit: *circuit,
                compliance_case: *compliance_case,
            });
        }
    }
    targets
}

fn run_target(
    target: BenchmarkTarget,
    cold_iterations: usize,
    warm_iterations: usize,
    count_constraints: bool,
    profile_mode: ProfileMode,
    backend: Backend,
    gnark_config: Option<&GnarkConfig>,
) -> Result<MeasurementSummary> {
    match target.circuit {
        CircuitKind::Both => bail!("internal error: concrete circuit expected"),
        CircuitKind::Spend => benchmark_spend(
            target.compliance_case,
            cold_iterations,
            warm_iterations,
            count_constraints,
            profile_mode,
            backend,
            gnark_config,
        ),
        CircuitKind::Output => benchmark_output(
            target.compliance_case,
            cold_iterations,
            warm_iterations,
            count_constraints,
            profile_mode,
            backend,
        ),
        CircuitKind::Parallel => {
            if backend == Backend::Gnark {
                bail!("--backend gnark only supports --circuit spend");
            }
            benchmark_parallel(target.compliance_case, cold_iterations, warm_iterations)
        }
        CircuitKind::Parallel2x2 => {
            if backend == Backend::Gnark {
                bail!("--backend gnark only supports --circuit spend");
            }
            benchmark_parallel_2x2(target.compliance_case, cold_iterations, warm_iterations)
        }
        CircuitKind::Serial2x2 => {
            if backend == Backend::Gnark {
                bail!("--backend gnark only supports --circuit spend");
            }
            benchmark_serial_2x2(target.compliance_case, cold_iterations, warm_iterations)
        }
    }
}

fn benchmark_spend(
    compliance_case: ComplianceCase,
    cold_iterations: usize,
    warm_iterations: usize,
    count_constraints: bool,
    profile_mode: ProfileMode,
    backend: Backend,
    gnark_config: Option<&GnarkConfig>,
) -> Result<MeasurementSummary> {
    if backend == Backend::Gnark {
        return benchmark_spend_gnark(
            compliance_case,
            cold_iterations,
            warm_iterations,
            count_constraints,
            profile_mode,
            gnark_config.ok_or_else(|| anyhow::anyhow!("missing gnark config"))?,
        );
    }

    let instance_build_start = Instant::now();
    let (public, private) = spend_instance(compliance_case);
    let instance_build_ms = instance_build_start.elapsed().as_secs_f64() * 1000.0;
    let blinding_r = Fq::rand(&mut OsRng);
    let blinding_s = Fq::rand(&mut OsRng);
    let stage_profile = profile_mode.captures_stage_timings();

    let mut summary = measure(
        cold_iterations,
        warm_iterations,
        move || {
            if stage_profile {
                let profiled =
                    profiled_spend_prove(blinding_r, blinding_s, public.clone(), private.clone())?;
                black_box(&profiled.proof);
                black_box(&profiled.proof_bytes);
                Ok(IterationMeasurement {
                    wall_ms: profiled.wall_ms,
                    stage: Some(profiled.stage),
                    gnark_stage: None,
                })
            } else {
                let start = Instant::now();
                let proof = SpendProof::prove(
                    blinding_r,
                    blinding_s,
                    &SPEND_PROOF_PROVING_KEY,
                    public.clone(),
                    private.clone(),
                )
                .expect("spend proof generation should succeed");
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                black_box(proof);
                Ok(IterationMeasurement {
                    wall_ms,
                    stage: None,
                    gnark_stage: None,
                })
            }
        },
        if count_constraints {
            Some(spend_metrics as fn() -> CircuitMetrics)
        } else {
            None
        },
    )?;

    if let Some(stage) = summary.cold_stage.as_mut() {
        stage.instance_build_ms = instance_build_ms;
    }
    if let Some(stage) = summary.warm_stage.as_mut() {
        stage.instance_build_ms = 0.0;
    }

    Ok(summary)
}

fn benchmark_spend_gnark(
    compliance_case: ComplianceCase,
    cold_iterations: usize,
    warm_iterations: usize,
    count_constraints: bool,
    profile_mode: ProfileMode,
    gnark_config: &GnarkConfig,
) -> Result<MeasurementSummary> {
    let stage_profile = profile_mode.captures_stage_timings();
    let (public, private) = spend_instance(compliance_case);

    let client = GnarkSpendClient::load(&gnark_config.lib, &gnark_config.artifact_dir)?;

    let debug_bundle = encode_spend_witness_v1_debug(&public, &private)?;
    let debug_dump_root = gnark_config
        .debug_witness_dir
        .as_ref()
        .map(|dir| write_gnark_debug_witness(dir, compliance_case, &debug_bundle))
        .transpose()?;
    let witness = debug_bundle.payload;
    let correctness_check_start = Instant::now();
    let proof_call = client.prove_raw(&witness).with_context(|| {
        if let Some(path) = &debug_dump_root {
            format!(
                "replay witness from {} (rust dump {} and sidecar {})",
                path.join("witness.bin").display(),
                path.join("rust.txt").display(),
                path.join("sidecar.json").display()
            )
        } else {
            "prove initial gnark spend witness".to_string()
        }
    })?;
    let (claimed_statement_hash, spend_proof) = translate_spend_proof_result(&proof_call.payload)?;
    let expected_statement_hash = spend_statement_hash_from_public(&public)
        .map_err(|e| anyhow::anyhow!("statement hash: {e}"))?;
    if claimed_statement_hash != expected_statement_hash {
        bail!(
            "gnark claimed statement hash mismatch: got {}, want {}",
            claimed_statement_hash,
            expected_statement_hash
        );
    }
    client.verify(&spend_proof, public.clone())?;
    let correctness_check_ms = correctness_check_start.elapsed().as_secs_f64() * 1000.0;

    let mut summary = measure(
        cold_iterations,
        warm_iterations,
        || {
            let total_start = Instant::now();

            let witness_pack_start = Instant::now();
            let witness = encode_spend_witness_v1(&public, &private)?;
            let witness_pack_ms = witness_pack_start.elapsed().as_secs_f64() * 1000.0;

            let ffi_start = Instant::now();
            let proof_call = client.prove_raw(&witness).with_context(|| {
                if let Some(path) = &debug_dump_root {
                    format!(
                        "replay witness from {} (rust dump {} and sidecar {})",
                        path.join("witness.bin").display(),
                        path.join("rust.txt").display(),
                        path.join("sidecar.json").display()
                    )
                } else {
                    "prove benchmark gnark spend witness".to_string()
                }
            })?;
            let ffi_call_ms = ffi_start.elapsed().as_secs_f64() * 1000.0;

            let translate_start = Instant::now();
            let (claimed_statement_hash, spend_proof) =
                translate_spend_proof_result(&proof_call.payload)?;
            if claimed_statement_hash != expected_statement_hash {
                bail!(
                    "gnark claimed statement hash mismatch during benchmark: got {}, want {}",
                    claimed_statement_hash,
                    expected_statement_hash
                );
            }
            black_box(&spend_proof);
            let proof_translate_ms = translate_start.elapsed().as_secs_f64() * 1000.0;

            Ok(IterationMeasurement {
                wall_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                stage: None,
                gnark_stage: if stage_profile {
                    Some(GnarkStageBreakdown {
                        witness_pack_ms,
                        prove_path_ms: ffi_call_ms + proof_translate_ms,
                        proof_translate_ms,
                    })
                } else {
                    None
                },
            })
        },
        if count_constraints {
            Some(spend_metrics as fn() -> CircuitMetrics)
        } else {
            None
        },
    )?;

    summary.backend = Backend::Gnark;
    summary.correctness_verified = Some(true);
    summary.gnark_lib_load_ms = Some(client.lib_load_ms());
    summary.gnark_init_ms = Some(client.init_ms());
    summary.correctness_check_ms = Some(correctness_check_ms);
    Ok(summary)
}

fn write_gnark_debug_witness(
    root: &Path,
    compliance_case: ComplianceCase,
    debug_bundle: &gnark_spend::SpendWitnessDebugBundle,
) -> Result<PathBuf> {
    let case_dir = root.join(format!("spend-{}", compliance_case.as_str()));
    std::fs::create_dir_all(&case_dir)
        .with_context(|| format!("create debug witness dir {}", case_dir.display()))?;

    let witness_path = case_dir.join("witness.bin");
    std::fs::write(&witness_path, &debug_bundle.payload)
        .with_context(|| format!("write {}", witness_path.display()))?;

    let raw_dump_path = case_dir.join("rust.txt");
    std::fs::write(&raw_dump_path, debug_bundle.raw_dump.as_bytes())
        .with_context(|| format!("write {}", raw_dump_path.display()))?;

    let sidecar_path = case_dir.join("sidecar.json");
    let sidecar = WitnessDebugSidecar {
        compliance_case: compliance_case.as_str().to_string(),
        claimed_statement_hash: debug_bundle.claimed_statement_hash.clone(),
        statement_fields: debug_bundle.statement_fields.clone(),
        payload_sha256: debug_bundle.payload_sha256_hex.clone(),
    };
    std::fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar)?)
        .with_context(|| format!("write {}", sidecar_path.display()))?;

    Ok(case_dir)
}

fn benchmark_parallel(
    compliance_case: ComplianceCase,
    cold_iterations: usize,
    warm_iterations: usize,
) -> Result<MeasurementSummary> {
    let (spend_public, spend_private) = spend_instance(compliance_case);
    let spend_r = Fq::rand(&mut OsRng);
    let spend_s = Fq::rand(&mut OsRng);

    let (output_public, output_private) = output_instance(compliance_case);
    let output_r = Fq::rand(&mut OsRng);
    let output_s = Fq::rand(&mut OsRng);

    measure(
        cold_iterations,
        warm_iterations,
        move || {
            let spend_public = spend_public.clone();
            let spend_private = spend_private.clone();
            let output_public = output_public.clone();
            let output_private = output_private.clone();
            let start = Instant::now();

            thread::scope(|scope| {
                let spend_handle = scope.spawn(move || {
                    let proof = SpendProof::prove(
                        spend_r,
                        spend_s,
                        &SPEND_PROOF_PROVING_KEY,
                        spend_public,
                        spend_private,
                    )
                    .expect("parallel spend proof generation should succeed");
                    black_box(proof);
                });
                let output_handle = scope.spawn(move || {
                    let proof = OutputProof::prove(
                        output_r,
                        output_s,
                        &OUTPUT_PROOF_PROVING_KEY,
                        output_public,
                        output_private,
                    )
                    .expect("parallel output proof generation should succeed");
                    black_box(proof);
                });
                spend_handle
                    .join()
                    .expect("parallel spend worker should not panic");
                output_handle
                    .join()
                    .expect("parallel output worker should not panic");
            });

            Ok(IterationMeasurement {
                wall_ms: start.elapsed().as_secs_f64() * 1000.0,
                stage: None,
                gnark_stage: None,
            })
        },
        None,
    )
}

fn benchmark_parallel_2x2(
    compliance_case: ComplianceCase,
    cold_iterations: usize,
    warm_iterations: usize,
) -> Result<MeasurementSummary> {
    let (spend_public_1, spend_private_1) = spend_instance(compliance_case);
    let spend_r_1 = Fq::rand(&mut OsRng);
    let spend_s_1 = Fq::rand(&mut OsRng);

    let (spend_public_2, spend_private_2) = spend_instance(compliance_case);
    let spend_r_2 = Fq::rand(&mut OsRng);
    let spend_s_2 = Fq::rand(&mut OsRng);

    let (output_public_1, output_private_1) = output_instance(compliance_case);
    let output_r_1 = Fq::rand(&mut OsRng);
    let output_s_1 = Fq::rand(&mut OsRng);

    let (output_public_2, output_private_2) = output_instance(compliance_case);
    let output_r_2 = Fq::rand(&mut OsRng);
    let output_s_2 = Fq::rand(&mut OsRng);

    measure(
        cold_iterations,
        warm_iterations,
        move || {
            let spend_public_1 = spend_public_1.clone();
            let spend_private_1 = spend_private_1.clone();
            let spend_public_2 = spend_public_2.clone();
            let spend_private_2 = spend_private_2.clone();
            let output_public_1 = output_public_1.clone();
            let output_private_1 = output_private_1.clone();
            let output_public_2 = output_public_2.clone();
            let output_private_2 = output_private_2.clone();
            let start = Instant::now();

            thread::scope(|scope| {
                let spend_handle_1 = scope.spawn(move || {
                    let proof = SpendProof::prove(
                        spend_r_1,
                        spend_s_1,
                        &SPEND_PROOF_PROVING_KEY,
                        spend_public_1,
                        spend_private_1,
                    )
                    .expect("parallel spend proof generation should succeed");
                    black_box(proof);
                });
                let spend_handle_2 = scope.spawn(move || {
                    let proof = SpendProof::prove(
                        spend_r_2,
                        spend_s_2,
                        &SPEND_PROOF_PROVING_KEY,
                        spend_public_2,
                        spend_private_2,
                    )
                    .expect("parallel spend proof generation should succeed");
                    black_box(proof);
                });
                let output_handle_1 = scope.spawn(move || {
                    let proof = OutputProof::prove(
                        output_r_1,
                        output_s_1,
                        &OUTPUT_PROOF_PROVING_KEY,
                        output_public_1,
                        output_private_1,
                    )
                    .expect("parallel output proof generation should succeed");
                    black_box(proof);
                });
                let output_handle_2 = scope.spawn(move || {
                    let proof = OutputProof::prove(
                        output_r_2,
                        output_s_2,
                        &OUTPUT_PROOF_PROVING_KEY,
                        output_public_2,
                        output_private_2,
                    )
                    .expect("parallel output proof generation should succeed");
                    black_box(proof);
                });

                spend_handle_1
                    .join()
                    .expect("parallel spend worker should not panic");
                spend_handle_2
                    .join()
                    .expect("parallel spend worker should not panic");
                output_handle_1
                    .join()
                    .expect("parallel output worker should not panic");
                output_handle_2
                    .join()
                    .expect("parallel output worker should not panic");
            });

            Ok(IterationMeasurement {
                wall_ms: start.elapsed().as_secs_f64() * 1000.0,
                stage: None,
                gnark_stage: None,
            })
        },
        None,
    )
}

fn benchmark_serial_2x2(
    compliance_case: ComplianceCase,
    cold_iterations: usize,
    warm_iterations: usize,
) -> Result<MeasurementSummary> {
    let (spend_public_1, spend_private_1) = spend_instance(compliance_case);
    let spend_r_1 = Fq::rand(&mut OsRng);
    let spend_s_1 = Fq::rand(&mut OsRng);

    let (spend_public_2, spend_private_2) = spend_instance(compliance_case);
    let spend_r_2 = Fq::rand(&mut OsRng);
    let spend_s_2 = Fq::rand(&mut OsRng);

    let (output_public_1, output_private_1) = output_instance(compliance_case);
    let output_r_1 = Fq::rand(&mut OsRng);
    let output_s_1 = Fq::rand(&mut OsRng);

    let (output_public_2, output_private_2) = output_instance(compliance_case);
    let output_r_2 = Fq::rand(&mut OsRng);
    let output_s_2 = Fq::rand(&mut OsRng);

    measure(
        cold_iterations,
        warm_iterations,
        move || {
            let start = Instant::now();

            let proof = SpendProof::prove(
                spend_r_1,
                spend_s_1,
                &SPEND_PROOF_PROVING_KEY,
                spend_public_1.clone(),
                spend_private_1.clone(),
            )
            .expect("serial spend proof generation should succeed");
            black_box(proof);

            let proof = SpendProof::prove(
                spend_r_2,
                spend_s_2,
                &SPEND_PROOF_PROVING_KEY,
                spend_public_2.clone(),
                spend_private_2.clone(),
            )
            .expect("serial spend proof generation should succeed");
            black_box(proof);

            let proof = OutputProof::prove(
                output_r_1,
                output_s_1,
                &OUTPUT_PROOF_PROVING_KEY,
                output_public_1.clone(),
                output_private_1.clone(),
            )
            .expect("serial output proof generation should succeed");
            black_box(proof);

            let proof = OutputProof::prove(
                output_r_2,
                output_s_2,
                &OUTPUT_PROOF_PROVING_KEY,
                output_public_2.clone(),
                output_private_2.clone(),
            )
            .expect("serial output proof generation should succeed");
            black_box(proof);

            Ok(IterationMeasurement {
                wall_ms: start.elapsed().as_secs_f64() * 1000.0,
                stage: None,
                gnark_stage: None,
            })
        },
        None,
    )
}

fn benchmark_output(
    compliance_case: ComplianceCase,
    cold_iterations: usize,
    warm_iterations: usize,
    count_constraints: bool,
    profile_mode: ProfileMode,
    backend: Backend,
) -> Result<MeasurementSummary> {
    if backend == Backend::Gnark {
        bail!("--backend gnark only supports --circuit spend");
    }

    let instance_build_start = Instant::now();
    let (public, private) = output_instance(compliance_case);
    let instance_build_ms = instance_build_start.elapsed().as_secs_f64() * 1000.0;
    let blinding_r = Fq::rand(&mut OsRng);
    let blinding_s = Fq::rand(&mut OsRng);
    let stage_profile = profile_mode.captures_stage_timings();

    let mut summary = measure(
        cold_iterations,
        warm_iterations,
        move || {
            if stage_profile {
                let profiled =
                    profiled_output_prove(blinding_r, blinding_s, public.clone(), private.clone())?;
                black_box(&profiled.proof);
                black_box(&profiled.proof_bytes);
                Ok(IterationMeasurement {
                    wall_ms: profiled.wall_ms,
                    stage: Some(profiled.stage),
                    gnark_stage: None,
                })
            } else {
                let start = Instant::now();
                let proof = OutputProof::prove(
                    blinding_r,
                    blinding_s,
                    &OUTPUT_PROOF_PROVING_KEY,
                    public.clone(),
                    private.clone(),
                )
                .expect("output proof generation should succeed");
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                black_box(proof);
                Ok(IterationMeasurement {
                    wall_ms,
                    stage: None,
                    gnark_stage: None,
                })
            }
        },
        if count_constraints {
            Some(output_metrics as fn() -> CircuitMetrics)
        } else {
            None
        },
    )?;

    if let Some(stage) = summary.cold_stage.as_mut() {
        stage.instance_build_ms = instance_build_ms;
    }
    if let Some(stage) = summary.warm_stage.as_mut() {
        stage.instance_build_ms = 0.0;
    }

    Ok(summary)
}

fn profiled_spend_prove(
    blinding_r: Fq,
    blinding_s: Fq,
    public: SpendProofPublic,
    private: SpendProofPrivate,
) -> Result<ProfiledProofResult<SpendProof>> {
    let total_start = Instant::now();
    let mut stage = ProofStageBreakdown::default();

    let statement_hash_start = Instant::now();
    let claimed_statement_hash = spend_statement_hash_from_public(&public)
        .map_err(|e| anyhow::anyhow!("statement hash: {e}"))?;
    stage.statement_hash_ms = statement_hash_start.elapsed().as_secs_f64() * 1000.0;

    let circuit_build_start = Instant::now();
    let circuit = SpendCircuit::from_parts(public, private, claimed_statement_hash);
    stage.circuit_build_ms = circuit_build_start.elapsed().as_secs_f64() * 1000.0;

    let pk_load_start = Instant::now();
    let pk: &ProvingKey<Bls12_377> = &SPEND_PROOF_PROVING_KEY;
    stage.pk_load_ms = pk_load_start.elapsed().as_secs_f64() * 1000.0;

    let create_proof_start = Instant::now();
    let proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
        circuit, pk, blinding_r, blinding_s,
    )
    .map_err(|e| anyhow::anyhow!("proof generation failed: {:?}", e))?;
    stage.create_proof_ms = create_proof_start.elapsed().as_secs_f64() * 1000.0;

    let serialize_start = Instant::now();
    let mut proof_bytes = [0u8; GROTH16_PROOF_LENGTH_BYTES];
    Proof::serialize_compressed(&proof, &mut proof_bytes[..])
        .map_err(|e| anyhow::anyhow!("serialization failed: {:?}", e))?;
    stage.serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;

    let proof = SpendProof::try_from(shielded_pool_pb::ZkSpendProof {
        inner: proof_bytes.to_vec(),
    })?;

    Ok(ProfiledProofResult {
        proof,
        proof_bytes: proof_bytes.to_vec(),
        stage,
        wall_ms: total_start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn profiled_output_prove(
    blinding_r: Fq,
    blinding_s: Fq,
    public: OutputProofPublic,
    private: OutputProofPrivate,
) -> Result<ProfiledProofResult<OutputProof>> {
    let total_start = Instant::now();
    let mut stage = ProofStageBreakdown::default();

    let statement_hash_start = Instant::now();
    let claimed_statement_hash = output_statement_hash_from_public(&public)
        .map_err(|e| anyhow::anyhow!("statement hash: {e}"))?;
    stage.statement_hash_ms = statement_hash_start.elapsed().as_secs_f64() * 1000.0;

    let circuit_build_start = Instant::now();
    let circuit = OutputCircuit::from_parts(public, private, claimed_statement_hash);
    stage.circuit_build_ms = circuit_build_start.elapsed().as_secs_f64() * 1000.0;

    let pk_load_start = Instant::now();
    let pk: &ProvingKey<Bls12_377> = &OUTPUT_PROOF_PROVING_KEY;
    stage.pk_load_ms = pk_load_start.elapsed().as_secs_f64() * 1000.0;

    let create_proof_start = Instant::now();
    let proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
        circuit, pk, blinding_r, blinding_s,
    )
    .map_err(|e| anyhow::anyhow!("proof generation failed: {:?}", e))?;
    stage.create_proof_ms = create_proof_start.elapsed().as_secs_f64() * 1000.0;

    let serialize_start = Instant::now();
    let mut proof_bytes = [0u8; GROTH16_PROOF_LENGTH_BYTES];
    Proof::serialize_compressed(&proof, &mut proof_bytes[..])
        .map_err(|e| anyhow::anyhow!("serialization failed: {:?}", e))?;
    stage.serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;

    let proof = OutputProof::try_from(shielded_pool_pb::ZkOutputProof {
        inner: proof_bytes.to_vec(),
    })?;

    Ok(ProfiledProofResult {
        proof,
        proof_bytes: proof_bytes.to_vec(),
        stage,
        wall_ms: total_start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn spend_instance(compliance_case: ComplianceCase) -> (SpendProofPublic, SpendProofPrivate) {
    let is_regulated = compliance_case.is_regulated();
    let mut rng = OsRng;
    let test_data = generate_test_data(
        &mut rng,
        compliance_case.asset_id(),
        100,
        is_regulated,
        CircuitType::Spend,
    );

    let mut sct = tct::Tree::new();
    let note_commitment = test_data.note.commit();
    sct.insert(tct::Witness::Keep, note_commitment).unwrap();
    let anchor = sct.root();
    let state_commitment_proof = sct.witness(note_commitment).unwrap();

    let balance_commitment = Balance::from(test_data.value).commit(test_data.balance_blinding);
    let nullifier = Nullifier::derive(
        test_data.fvk.nullifier_key(),
        state_commitment_proof.position(),
        &note_commitment,
    );
    let spend_auth_randomizer = Fr::rand(&mut rng);
    let rk = test_data
        .fvk
        .spend_verification_key()
        .randomize(&spend_auth_randomizer);

    let tx_blinding_nonce = Fr::from(0u64);
    let sender_leaf_hash = blind_sender_leaf(test_data.user_leaf.commit(), tx_blinding_nonce);

    (
        SpendProofPublic {
            anchor,
            balance_commitment,
            nullifier,
            rk,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            epk: test_data.epk_1,
            c2_core: test_data.c2_core,
            compliance_ciphertext: test_data.compliance_ciphertext,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c: test_data.dleq_c,
            dleq_s: test_data.dleq_s,
            sender_leaf_hash,
        },
        SpendProofPrivate {
            state_commitment_proof,
            note: test_data.note,
            v_blinding: test_data.balance_blinding,
            spend_auth_randomizer,
            ak: *test_data.fvk.spend_verification_key(),
            nk: *test_data.fvk.nullifier_key(),
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            tx_blinding_nonce,
            is_flagged: false,
            salt: test_data.salt,
        },
    )
}

fn output_instance(compliance_case: ComplianceCase) -> (OutputProofPublic, OutputProofPrivate) {
    let is_regulated = compliance_case.is_regulated();
    let mut rng = OsRng;
    let test_data = generate_test_data(
        &mut rng,
        compliance_case.asset_id(),
        100,
        is_regulated,
        CircuitType::Output,
    );

    let note_commitment = test_data.note.commit();
    let balance_commitment = (-Balance::from(test_data.value)).commit(test_data.balance_blinding);
    let tx_blinding_nonce = Fr::from(0u64);
    let counterparty_leaf_hash =
        blind_sender_leaf(test_data.counterparty_leaf.commit(), tx_blinding_nonce);

    (
        OutputProofPublic {
            balance_commitment,
            note_commitment,
            epk_1: test_data.epk_1,
            epk_2: test_data.epk_2.expect("output proof requires epk_2"),
            epk_3: test_data.epk_3.expect("output proof requires epk_3"),
            c2_core: test_data.c2_core,
            c2_ext: test_data.c2_ext.expect("output proof requires c2_ext"),
            c2_sext: test_data.c2_sext.expect("output proof requires c2_sext"),
            compliance_ciphertext: test_data.compliance_ciphertext,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c_1: test_data.dleq_c,
            dleq_s_1: test_data.dleq_s,
            dleq_c_2: test_data.dleq_c_2,
            dleq_s_2: test_data.dleq_s_2,
            dleq_c_3: test_data.dleq_c_3,
            dleq_s_3: test_data.dleq_s_3,
            counterparty_leaf_hash,
        },
        OutputProofPrivate {
            note: test_data.note,
            balance_blinding: test_data.balance_blinding,
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            r_2: test_data.r_2.expect("output proof requires r_2"),
            r_3: test_data.r_3.expect("output proof requires r_3"),
            counterparty_leaf: test_data.counterparty_leaf,
            tx_blinding_nonce,
            is_flagged: false,
            salt: test_data.salt,
        },
    )
}

fn spend_metrics() -> CircuitMetrics {
    metrics_for(SpendCircuit::with_dummy_witness())
}

fn output_metrics() -> CircuitMetrics {
    metrics_for(OutputCircuit::with_dummy_witness())
}

fn metrics_for(circuit: impl ConstraintSynthesizer<Fq>) -> CircuitMetrics {
    let cs = ConstraintSystem::new_ref();
    cs.set_optimization_goal(OptimizationGoal::Constraints);
    cs.set_mode(SynthesisMode::Setup);
    circuit
        .generate_constraints(cs.clone())
        .expect("dummy witness should synthesize");
    cs.finalize();

    CircuitMetrics {
        constraints: cs.num_constraints(),
    }
}

fn measure(
    cold_iterations: usize,
    warm_iterations: usize,
    mut f: impl FnMut() -> Result<IterationMeasurement>,
    metrics: Option<fn() -> CircuitMetrics>,
) -> Result<MeasurementSummary> {
    let (cold_ms, cold_stage, cold_gnark_stage) = average_measurements(cold_iterations, &mut f)?;
    let (warm_ms, warm_stage, warm_gnark_stage) = average_measurements(warm_iterations, &mut f)?;

    Ok(MeasurementSummary {
        backend: Backend::Arkworks,
        cold_ms,
        warm_ms,
        metrics: metrics.map(|metric_fn| metric_fn()),
        cold_stage,
        warm_stage,
        cold_gnark_stage,
        warm_gnark_stage,
        correctness_verified: None,
        gnark_lib_load_ms: None,
        gnark_init_ms: None,
        correctness_check_ms: None,
    })
}

fn average_measurements(
    iterations: usize,
    f: &mut impl FnMut() -> Result<IterationMeasurement>,
) -> Result<(
    f64,
    Option<ProofStageBreakdown>,
    Option<GnarkStageBreakdown>,
)> {
    let mut total_ms = 0.0;
    let mut total_stage = None::<ProofStageBreakdown>;
    let mut total_gnark_stage = None::<GnarkStageBreakdown>;

    for _ in 0..iterations {
        let measurement = f()?;
        total_ms += measurement.wall_ms;
        if let Some(stage) = measurement.stage {
            let accumulated = total_stage.get_or_insert_with(ProofStageBreakdown::default);
            accumulated.add_assign(stage);
        }
        if let Some(stage) = measurement.gnark_stage {
            let accumulated = total_gnark_stage.get_or_insert_with(GnarkStageBreakdown::default);
            accumulated.add_assign(stage);
        }
    }

    Ok((
        total_ms / iterations as f64,
        total_stage.map(|stage| stage.divide(iterations)),
        total_gnark_stage.map(|stage| stage.divide(iterations)),
    ))
}

fn write_rows(
    rows: &[SummaryRow],
    format: OutputFormat,
    output: Option<&Path>,
    print_header: bool,
) -> Result<()> {
    let writer: Box<dyn Write> = match output {
        Some(path) => Box::new(File::create(path)?),
        None => Box::new(io::stdout()),
    };

    match format {
        OutputFormat::Json => {
            let mut writer = writer;
            serde_json::to_writer_pretty(&mut writer, rows)?;
            writeln!(writer)?;
        }
        OutputFormat::Csv => {
            let mut csv_writer = csv::WriterBuilder::new()
                .has_headers(print_header)
                .from_writer(writer);
            for row in rows {
                csv_writer.serialize(row)?;
            }
            csv_writer.flush()?;
        }
    }

    Ok(())
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn host_label() -> String {
    std::env::var("BENCH_HOST_LABEL")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn git_rev() -> String {
    std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string())
}

fn rayon_threads() -> Option<usize> {
    std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiled_spend_matches_library_prove() {
        let (public, private) = spend_instance(ComplianceCase::Regulated);
        let blinding_r = Fq::from(11u64);
        let blinding_s = Fq::from(22u64);

        let expected = SpendProof::prove(
            blinding_r,
            blinding_s,
            &SPEND_PROOF_PROVING_KEY,
            public.clone(),
            private.clone(),
        )
        .expect("library spend prove should succeed");
        let expected_pb: shielded_pool_pb::ZkSpendProof = expected.into();

        let profiled = profiled_spend_prove(blinding_r, blinding_s, public.clone(), private)
            .expect("profiled spend prove should succeed");

        assert_eq!(profiled.proof_bytes, expected_pb.inner);
        profiled
            .proof
            .verify(&SPEND_PROOF_VERIFICATION_KEY, public)
            .expect("profiled spend proof should verify");
    }

    #[test]
    fn profiled_output_matches_library_prove() {
        let (public, private) = output_instance(ComplianceCase::Regulated);
        let blinding_r = Fq::from(33u64);
        let blinding_s = Fq::from(44u64);

        let expected = OutputProof::prove(
            blinding_r,
            blinding_s,
            &OUTPUT_PROOF_PROVING_KEY,
            public.clone(),
            private.clone(),
        )
        .expect("library output prove should succeed");
        let expected_pb: shielded_pool_pb::ZkOutputProof = expected.into();

        let profiled = profiled_output_prove(blinding_r, blinding_s, public.clone(), private)
            .expect("profiled output prove should succeed");

        assert_eq!(profiled.proof_bytes, expected_pb.inner);
        profiled
            .proof
            .verify(&OUTPUT_PROOF_VERIFICATION_KEY, public)
            .expect("profiled output proof should verify");
    }
}
