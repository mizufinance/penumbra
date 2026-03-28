use std::fs::File;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use ark_ec::{
    pairing::Pairing, scalar_mul::variable_base::VariableBaseMSM, AffineRepr, CurveGroup,
    PrimeGroup,
};
use ark_ff::{BigInteger, PrimeField, Zero};
use clap::{Parser, ValueEnum};
use decaf377::Bls12_377;
use icicle_bls12_377::curve::{
    BaseField as IcicleBaseField, G1Affine as IcicleG1Affine, G1Projective as IcicleG1Projective,
    ScalarField as IcicleScalarField,
};
use icicle_core::{
    affine::Affine,
    bignum::BigNum,
    msm::{msm as icicle_msm, MSMConfig},
    projective::Projective,
};
use icicle_runtime::{memory::HostSlice, Device};
use penumbra_sdk_proof_params::{OUTPUT_PROOF_PROVING_KEY, SPEND_PROOF_PROVING_KEY};
use serde::Serialize;

type ArkG1 = <Bls12_377 as Pairing>::G1;
type ArkG1Affine = <Bls12_377 as Pairing>::G1Affine;
type ArkScalar = <ArkG1 as PrimeGroup>::ScalarField;
type ArkBase = <ArkG1Affine as AffineRepr>::BaseField;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CircuitKind {
    Both,
    Spend,
    Output,
}

impl CircuitKind {
    fn concrete(self) -> &'static [CircuitKind] {
        match self {
            Self::Both => &[Self::Spend, Self::Output],
            Self::Spend => &[Self::Spend],
            Self::Output => &[Self::Output],
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Spend => "spend",
            Self::Output => "output",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum QueryFamily {
    All,
    #[clap(name = "a-query")]
    AQuery,
    #[clap(name = "b-g1-query")]
    BG1Query,
    #[clap(name = "h-query")]
    HQuery,
    #[clap(name = "l-query")]
    LQuery,
}

impl QueryFamily {
    fn concrete(self) -> &'static [QueryFamily] {
        match self {
            Self::All => &[Self::AQuery, Self::BG1Query, Self::HQuery, Self::LQuery],
            Self::AQuery => &[Self::AQuery],
            Self::BG1Query => &[Self::BG1Query],
            Self::HQuery => &[Self::HQuery],
            Self::LQuery => &[Self::LQuery],
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::AQuery => "a_query",
            Self::BG1Query => "b_g1_query",
            Self::HQuery => "h_query",
            Self::LQuery => "l_query",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Csv,
    Json,
}

#[derive(Debug, Parser)]
#[clap(
    name = "icicle_cpu_spike",
    about = "Benchmark ICICLE CPU MSM against Arkworks on real Penumbra proving-key queries"
)]
struct Cli {
    #[clap(long, value_enum, default_value_t = CircuitKind::Both)]
    circuit: CircuitKind,
    #[clap(long, value_enum, default_value_t = QueryFamily::All)]
    query_family: QueryFamily,
    #[clap(long, default_value_t = 5)]
    iterations: usize,
    #[clap(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,
    #[clap(long)]
    output: Option<PathBuf>,
    #[clap(long)]
    print_header: bool,
    #[clap(long)]
    device_label: Option<String>,
}

#[derive(Debug, Serialize)]
struct SpikeRow {
    run_id: String,
    circuit: String,
    query_family: String,
    iterations: usize,
    query_len: usize,
    arkworks_mean_ms: f64,
    icicle_kernel_mean_ms: f64,
    icicle_cached_bases_mean_ms: f64,
    icicle_full_conversion_mean_ms: f64,
    arkworks_vs_icicle_kernel_speedup: f64,
    arkworks_vs_icicle_cached_bases_speedup: f64,
    arkworks_vs_icicle_full_conversion_speedup: f64,
    correctness_match: bool,
    icicle_registered_devices: String,
    git_rev: String,
    host_label: String,
    timestamp: u64,
}

#[derive(Clone, Copy)]
struct QueryTarget {
    circuit: CircuitKind,
    family: QueryFamily,
}

struct QueryBench<'a> {
    bases: &'a [ArkG1Affine],
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.iterations == 0 {
        bail!("--iterations must be > 0");
    }

    init_icicle_cpu()?;
    let icicle_registered_devices = icicle_runtime::get_registered_devices()
        .unwrap_or_default()
        .join(",");

    let run_id = format!("run-{}-{}", unix_ts(), std::process::id());
    let host_label = cli.device_label.unwrap_or_else(host_label);
    let git_rev = git_rev();

    let mut rows = Vec::new();
    for target in build_targets(cli.circuit, cli.query_family) {
        let query = load_query(target)?;
        rows.push(benchmark_query(
            &run_id,
            &host_label,
            &git_rev,
            &icicle_registered_devices,
            target,
            query,
            cli.iterations,
        )?);
    }

    write_rows(&rows, cli.format, cli.output.as_deref(), cli.print_header)
}

fn init_icicle_cpu() -> Result<()> {
    let cpu = Device::new("CPU", 0);
    let backend_load_err = icicle_runtime::load_backend_from_env_or_default().err();
    if !icicle_runtime::is_device_available(&cpu) {
        if let Some(err) = backend_load_err {
            return Err(anyhow!(
                "ICICLE CPU device is not available after backend load failed: {err}"
            ));
        }
        bail!("ICICLE CPU device is not available");
    }
    icicle_runtime::set_device(&cpu).context("failed to set ICICLE device to CPU")?;
    Ok(())
}

fn build_targets(circuit: CircuitKind, family: QueryFamily) -> Vec<QueryTarget> {
    let mut targets = Vec::new();
    for &circuit in circuit.concrete() {
        for &family in family.concrete() {
            targets.push(QueryTarget { circuit, family });
        }
    }
    targets
}

fn load_query(target: QueryTarget) -> Result<QueryBench<'static>> {
    let pk = match target.circuit {
        CircuitKind::Spend => &*SPEND_PROOF_PROVING_KEY,
        CircuitKind::Output => &*OUTPUT_PROOF_PROVING_KEY,
        CircuitKind::Both => unreachable!("concrete circuit target required"),
    };

    let bases = match target.family {
        QueryFamily::AQuery => pk.a_query.as_slice(),
        QueryFamily::BG1Query => pk.b_g1_query.as_slice(),
        QueryFamily::HQuery => pk.h_query.as_slice(),
        QueryFamily::LQuery => pk.l_query.as_slice(),
        QueryFamily::All => unreachable!("concrete query family required"),
    };

    Ok(QueryBench { bases })
}

fn benchmark_query(
    run_id: &str,
    host_label: &str,
    git_rev: &str,
    icicle_registered_devices: &str,
    target: QueryTarget,
    query: QueryBench<'_>,
    iterations: usize,
) -> Result<SpikeRow> {
    let scalars = deterministic_scalars(query.bases.len());
    let icicle_scalars = scalars
        .iter()
        .copied()
        .map(scalar_to_icicle)
        .collect::<Vec<_>>();
    let icicle_bases = query
        .bases
        .iter()
        .copied()
        .map(ark_affine_to_icicle)
        .collect::<Result<Vec<_>>>()?;

    let expected = <ArkG1 as VariableBaseMSM>::msm_unchecked(query.bases, &scalars);
    let icicle_result = run_icicle_kernel(&icicle_scalars, &icicle_bases)?;
    let actual = icicle_projective_to_ark(icicle_result)?;
    let correctness_match = expected.into_affine() == actual.into_affine();

    let _ = <ArkG1 as VariableBaseMSM>::msm_unchecked(query.bases, &scalars);
    let _ = run_icicle_kernel(&icicle_scalars, &icicle_bases)?;
    {
        let converted_scalars = scalars
            .iter()
            .copied()
            .map(scalar_to_icicle)
            .collect::<Vec<_>>();
        let projective = run_icicle_kernel(&converted_scalars, &icicle_bases)?;
        let _ = icicle_projective_to_ark(projective)?;
    }
    {
        let converted_scalars = scalars
            .iter()
            .copied()
            .map(scalar_to_icicle)
            .collect::<Vec<_>>();
        let converted_bases = query
            .bases
            .iter()
            .copied()
            .map(ark_affine_to_icicle)
            .collect::<Result<Vec<_>>>()?;
        let projective = run_icicle_kernel(&converted_scalars, &converted_bases)?;
        let _ = icicle_projective_to_ark(projective)?;
    }

    let mut arkworks_total = 0.0;
    let mut icicle_kernel_total = 0.0;
    let mut icicle_cached_bases_total = 0.0;
    let mut icicle_full_conversion_total = 0.0;

    for _ in 0..iterations {
        let start = Instant::now();
        let _ = black_box(<ArkG1 as VariableBaseMSM>::msm_unchecked(
            query.bases,
            black_box(&scalars),
        ));
        arkworks_total += elapsed_ms(start);

        let start = Instant::now();
        black_box(run_icicle_kernel(
            black_box(&icicle_scalars),
            black_box(&icicle_bases),
        )?);
        icicle_kernel_total += elapsed_ms(start);

        let start = Instant::now();
        let converted_scalars = scalars
            .iter()
            .copied()
            .map(scalar_to_icicle)
            .collect::<Vec<_>>();
        let projective =
            run_icicle_kernel(black_box(&converted_scalars), black_box(&icicle_bases))?;
        let _ = black_box(icicle_projective_to_ark(projective)?);
        icicle_cached_bases_total += elapsed_ms(start);

        let start = Instant::now();
        let converted_scalars = scalars
            .iter()
            .copied()
            .map(scalar_to_icicle)
            .collect::<Vec<_>>();
        let converted_bases = query
            .bases
            .iter()
            .copied()
            .map(ark_affine_to_icicle)
            .collect::<Result<Vec<_>>>()?;
        let projective =
            run_icicle_kernel(black_box(&converted_scalars), black_box(&converted_bases))?;
        let _ = black_box(icicle_projective_to_ark(projective)?);
        icicle_full_conversion_total += elapsed_ms(start);
    }

    let arkworks_mean_ms = arkworks_total / iterations as f64;
    let icicle_kernel_mean_ms = icicle_kernel_total / iterations as f64;
    let icicle_cached_bases_mean_ms = icicle_cached_bases_total / iterations as f64;
    let icicle_full_conversion_mean_ms = icicle_full_conversion_total / iterations as f64;

    Ok(SpikeRow {
        run_id: run_id.to_string(),
        circuit: target.circuit.as_str().to_string(),
        query_family: target.family.as_str().to_string(),
        iterations,
        query_len: query.bases.len(),
        arkworks_mean_ms,
        icicle_kernel_mean_ms,
        icicle_cached_bases_mean_ms,
        icicle_full_conversion_mean_ms,
        arkworks_vs_icicle_kernel_speedup: ratio(arkworks_mean_ms, icicle_kernel_mean_ms),
        arkworks_vs_icicle_cached_bases_speedup: ratio(
            arkworks_mean_ms,
            icicle_cached_bases_mean_ms,
        ),
        arkworks_vs_icicle_full_conversion_speedup: ratio(
            arkworks_mean_ms,
            icicle_full_conversion_mean_ms,
        ),
        correctness_match,
        icicle_registered_devices: icicle_registered_devices.to_string(),
        git_rev: git_rev.to_string(),
        host_label: host_label.to_string(),
        timestamp: unix_ts(),
    })
}

fn run_icicle_kernel(
    scalars: &[IcicleScalarField],
    bases: &[IcicleG1Affine],
) -> Result<IcicleG1Projective> {
    if scalars.is_empty() || bases.is_empty() {
        bail!("cannot benchmark an empty MSM");
    }

    let config = MSMConfig::default();
    let mut result = vec![IcicleG1Projective::zero()];
    icicle_msm(
        HostSlice::from_slice(scalars),
        HostSlice::from_slice(bases),
        &config,
        HostSlice::from_mut_slice(&mut result),
    )
    .map_err(|err| anyhow!("ICICLE MSM failed: {err}"))?;

    result
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("ICICLE returned no MSM result"))
}

fn deterministic_scalars(len: usize) -> Vec<ArkScalar> {
    (0..len).map(|i| ArkScalar::from((i as u64) + 1)).collect()
}

fn scalar_to_icicle(value: ArkScalar) -> IcicleScalarField {
    IcicleScalarField::from_bytes_le(&value.into_bigint().to_bytes_le())
}

fn base_to_icicle(value: ArkBase) -> IcicleBaseField {
    IcicleBaseField::from_bytes_le(&value.into_bigint().to_bytes_le())
}

fn icicle_base_to_ark(value: IcicleBaseField) -> ArkBase {
    ArkBase::from_le_bytes_mod_order(&pad_to(value.to_bytes_le(), 48))
}

fn ark_affine_to_icicle(point: ArkG1Affine) -> Result<IcicleG1Affine> {
    match point.xy() {
        Some((x, y)) => Ok(IcicleG1Affine::from_xy(
            base_to_icicle(x),
            base_to_icicle(y),
        )),
        None => Ok(IcicleG1Affine::zero()),
    }
}

fn icicle_projective_to_ark(point: IcicleG1Projective) -> Result<ArkG1> {
    let affine = point.to_affine();
    if affine == IcicleG1Affine::zero() {
        return Ok(ArkG1::zero());
    }

    let x = icicle_base_to_ark(affine.x());
    let y = icicle_base_to_ark(affine.y());
    Ok(ArkG1::from(ArkG1Affine::new_unchecked(x, y)))
}

fn pad_to(mut bytes: Vec<u8>, size: usize) -> Vec<u8> {
    bytes.resize(size, 0);
    bytes
}

fn ratio(base: f64, candidate: f64) -> f64 {
    if candidate == 0.0 {
        f64::INFINITY
    } else {
        base / candidate
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn git_rev() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown-rev".to_string())
}

fn host_label() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock drift before unix epoch")
        .as_secs()
}

fn write_rows(
    rows: &[SpikeRow],
    format: OutputFormat,
    output: Option<&Path>,
    print_header: bool,
) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(rows, output),
        OutputFormat::Csv => write_csv(rows, output, print_header),
    }
}

fn write_json(rows: &[SpikeRow], output: Option<&Path>) -> Result<()> {
    match output {
        Some(path) => {
            let file = File::create(path)
                .with_context(|| format!("failed to create output file {}", path.display()))?;
            serde_json::to_writer_pretty(file, rows)?;
        }
        None => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            serde_json::to_writer_pretty(&mut lock, rows)?;
            writeln!(lock)?;
        }
    }

    Ok(())
}

fn write_csv(rows: &[SpikeRow], output: Option<&Path>, print_header: bool) -> Result<()> {
    match output {
        Some(path) => {
            let file = File::create(path)
                .with_context(|| format!("failed to create output file {}", path.display()))?;
            let mut writer = csv::WriterBuilder::new()
                .has_headers(print_header)
                .from_writer(file);
            for row in rows {
                writer.serialize(row)?;
            }
            writer.flush()?;
        }
        None => {
            let stdout = io::stdout();
            let lock = stdout.lock();
            let mut writer = csv::WriterBuilder::new()
                .has_headers(print_header)
                .from_writer(lock);
            for row in rows {
                writer.serialize(row)?;
            }
            writer.flush()?;
        }
    }

    Ok(())
}
