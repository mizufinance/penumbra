use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::tps::aggregate::SummaryRow;

const DEFAULT_OFFERED_TPS: u64 = 20;
const DEFAULT_REPEATS: u32 = 1;
const DEFAULT_WARMUP_BLOCKS: u64 = 2;
const DEFAULT_STEADY_BLOCKS: u64 = 20;
const DEFAULT_TARGET_BLOCK_TIME_MS: u64 = 500;
const DEFAULT_SUBMIT_WORKERS: usize = 1;
const DEFAULT_MAX_INFLIGHT: usize = 2;
const DEFAULT_MIN_STEADY_COMMITS: u64 = 3;
const DEFAULT_SYNTHETIC_TX_COUNT: u64 = 4_000;
const DEFAULT_SYNTHETIC_CONCURRENCY: u64 = 32;
const DEFAULT_SYNTHETIC_MODE: &str = "v2_warm";
const DEFAULT_SYNTHETIC_INDEXING_MODE: &str = "no_index";
const DEFAULT_SCENARIOS: &str = "unregulated";

#[derive(Clone, Debug)]
pub struct LocalFullnodeConfig {
    pub script_path: PathBuf,
    pub run_label: String,
    pub offered_tps: u64,
    pub repeats: u32,
    pub warmup_blocks: u64,
    pub steady_blocks: u64,
    pub target_block_time_ms: u64,
    pub submit_workers: usize,
    pub max_inflight: usize,
    pub min_steady_commits: u64,
    pub auto_refresh: bool,
    pub scenarios: String,
}

#[derive(Clone, Debug)]
pub struct LocalFullnodeRun {
    pub config: LocalFullnodeConfig,
    pub rows: Vec<SummaryRow>,
    pub synthetic_reference: SyntheticReference,
}

#[derive(Clone, Debug)]
pub struct SyntheticReference {
    pub version: String,
    pub tx_count: u64,
    pub mode: String,
    pub concurrency: u64,
    pub indexing_mode: String,
    pub metric: String,
    pub mean_ms: f64,
    pub run_id: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug)]
pub struct SyntheticReferenceSelector {
    pub tx_count: u64,
    pub mode: String,
    pub concurrency: u64,
    pub indexing_mode: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
struct SyntheticBenchRow {
    version: String,
    tx_count: u64,
    mode: String,
    concurrency: u64,
    indexing_mode: String,
    metric: String,
    mean_ms: f64,
    #[allow(dead_code)]
    median_ms: f64,
    #[allow(dead_code)]
    samples: usize,
    #[allow(dead_code)]
    profile: Option<String>,
    run_id: String,
    timestamp: u64,
    #[allow(dead_code)]
    git_rev: Option<String>,
    #[allow(dead_code)]
    host_label: Option<String>,
}

impl LocalFullnodeConfig {
    pub fn from_env() -> Result<Self> {
        let script_path = std::env::var_os("BENCH_LOCAL_FULLNODE_SCRIPT")
            .map(PathBuf::from)
            .unwrap_or_else(default_script_path);
        let run_label_prefix = std::env::var("BENCH_LOCAL_FULLNODE_RUN_LABEL")
            .unwrap_or_else(|_| "local-fullnode".to_string());
        let now = unix_ts();
        let run_label = format!("{run_label_prefix}-{now}-p{}", std::process::id());

        let offered_tps = parse_env_u64("BENCH_LOCAL_FULLNODE_OFFERED_TPS", DEFAULT_OFFERED_TPS)?;
        let repeats = parse_env_u32("BENCH_LOCAL_FULLNODE_REPEATS", DEFAULT_REPEATS)?;
        let warmup_blocks =
            parse_env_u64("BENCH_LOCAL_FULLNODE_WARMUP_BLOCKS", DEFAULT_WARMUP_BLOCKS)?;
        let steady_blocks =
            parse_env_u64("BENCH_LOCAL_FULLNODE_STEADY_BLOCKS", DEFAULT_STEADY_BLOCKS)?;
        let target_block_time_ms = parse_env_u64(
            "BENCH_LOCAL_FULLNODE_TARGET_BLOCK_TIME_MS",
            DEFAULT_TARGET_BLOCK_TIME_MS,
        )?;
        let submit_workers = parse_env_usize(
            "BENCH_LOCAL_FULLNODE_SUBMIT_WORKERS",
            DEFAULT_SUBMIT_WORKERS,
        )?;
        let max_inflight =
            parse_env_usize("BENCH_LOCAL_FULLNODE_MAX_INFLIGHT", DEFAULT_MAX_INFLIGHT)?;
        let min_steady_commits = parse_env_u64(
            "BENCH_LOCAL_FULLNODE_MIN_STEADY_COMMITS",
            DEFAULT_MIN_STEADY_COMMITS,
        )?;
        let auto_refresh = parse_env_bool("BENCH_LOCAL_FULLNODE_AUTO_REFRESH", false)?;
        let scenarios = std::env::var("BENCH_LOCAL_FULLNODE_SCENARIOS")
            .unwrap_or_else(|_| DEFAULT_SCENARIOS.to_string());

        if !script_path.exists() {
            bail!(
                "local full-node driver script does not exist: {}",
                script_path.display()
            );
        }

        Ok(Self {
            script_path,
            run_label,
            offered_tps,
            repeats,
            warmup_blocks,
            steady_blocks,
            target_block_time_ms,
            submit_workers,
            max_inflight,
            min_steady_commits,
            auto_refresh,
            scenarios,
        })
    }
}

impl SyntheticReferenceSelector {
    pub fn from_env(version: &str) -> Result<Self> {
        Ok(Self {
            tx_count: parse_env_u64(
                "BENCH_LOCAL_FULLNODE_SYNTHETIC_TX_COUNT",
                DEFAULT_SYNTHETIC_TX_COUNT,
            )?,
            mode: std::env::var("BENCH_LOCAL_FULLNODE_SYNTHETIC_MODE")
                .unwrap_or_else(|_| DEFAULT_SYNTHETIC_MODE.to_string()),
            concurrency: parse_env_u64(
                "BENCH_LOCAL_FULLNODE_SYNTHETIC_CONCURRENCY",
                DEFAULT_SYNTHETIC_CONCURRENCY,
            )?,
            indexing_mode: std::env::var("BENCH_LOCAL_FULLNODE_SYNTHETIC_INDEXING_MODE")
                .unwrap_or_else(|_| DEFAULT_SYNTHETIC_INDEXING_MODE.to_string()),
            version: version.to_string(),
        })
    }
}

impl LocalFullnodeRun {
    pub fn from_env(version: &str) -> Result<Self> {
        let config = LocalFullnodeConfig::from_env()?;
        let synthetic_selector = SyntheticReferenceSelector::from_env(version)?;
        run_local_fullnode(&config, &synthetic_selector)
    }
}

pub fn run_local_fullnode(
    config: &LocalFullnodeConfig,
    synthetic_selector: &SyntheticReferenceSelector,
) -> Result<LocalFullnodeRun> {
    let tps_csv_path = tps_csv_path();
    let start_ts = unix_ts();
    let mut command = Command::new(&config.script_path);
    command
        .arg("run")
        .arg("--skip-build")
        .arg("--run-label")
        .arg(&config.run_label)
        .arg("--offered-tps")
        .arg(config.offered_tps.to_string())
        .arg("--repeats")
        .arg(config.repeats.to_string())
        .arg("--warmup-blocks")
        .arg(config.warmup_blocks.to_string())
        .arg("--steady-blocks")
        .arg(config.steady_blocks.to_string())
        .arg("--target-block-time-ms")
        .arg(config.target_block_time_ms.to_string())
        .arg("--submit-workers")
        .arg(config.submit_workers.to_string())
        .arg("--max-inflight")
        .arg(config.max_inflight.to_string())
        .arg("--min-steady-commits")
        .arg(config.min_steady_commits.to_string())
        .arg("--scenarios")
        .arg(&config.scenarios)
        .current_dir(repo_root());

    if config.auto_refresh {
        command.arg("--auto-refresh");
    }

    let status = command.status().with_context(|| {
        format!(
            "failed to start local full-node driver {}",
            config.script_path.display()
        )
    })?;
    if !status.success() {
        bail!(
            "local full-node driver failed for run label {} with status {}",
            config.run_label,
            status
        );
    }

    let rows = read_runtime_rows(&tps_csv_path, &config.run_label, start_ts)?;
    let synthetic_reference =
        load_synthetic_reference(&pre_consensus_csv_path(), synthetic_selector)?;

    Ok(LocalFullnodeRun {
        config: config.clone(),
        rows,
        synthetic_reference,
    })
}

pub fn load_synthetic_reference(
    path: &Path,
    selector: &SyntheticReferenceSelector,
) -> Result<SyntheticReference> {
    let mut rdr = csv::Reader::from_path(path)
        .with_context(|| format!("failed to open synthetic reference CSV {}", path.display()))?;
    let mut best: Option<SyntheticReference> = None;

    for row in rdr.deserialize::<SyntheticBenchRow>() {
        let row = row.with_context(|| {
            format!(
                "failed to parse synthetic reference row from {}",
                path.display()
            )
        })?;

        if row.version != selector.version
            || row.tx_count != selector.tx_count
            || row.mode != selector.mode
            || row.concurrency != selector.concurrency
            || row.indexing_mode != selector.indexing_mode
            || row.metric != "preconsensus_tps"
        {
            continue;
        }

        let candidate = SyntheticReference {
            version: row.version,
            tx_count: row.tx_count,
            mode: row.mode,
            concurrency: row.concurrency,
            indexing_mode: row.indexing_mode,
            metric: row.metric,
            mean_ms: row.mean_ms,
            run_id: row.run_id,
            timestamp: row.timestamp,
        };

        let replace = best
            .as_ref()
            .map(|current| candidate.timestamp > current.timestamp)
            .unwrap_or(true);
        if replace {
            best = Some(candidate);
        }
    }

    best.with_context(|| {
        format!(
            "no matching synthetic reference found in {} for version={} tx_count={} mode={} concurrency={} indexing_mode={}",
            path.display(),
            selector.version,
            selector.tx_count,
            selector.mode,
            selector.concurrency,
            selector.indexing_mode
        )
    })
}

fn read_runtime_rows(path: &Path, run_label: &str, start_ts: u64) -> Result<Vec<SummaryRow>> {
    let mut rdr = csv::Reader::from_path(path)
        .with_context(|| format!("failed to open local runtime CSV {}", path.display()))?;
    let mut rows = Vec::new();
    for row in rdr.deserialize::<SummaryRow>() {
        let row =
            row.with_context(|| format!("failed to parse runtime row from {}", path.display()))?;
        if row.label == run_label && row.timestamp >= start_ts {
            rows.push(row);
        }
    }

    rows.sort_by(|a, b| {
        a.scenario
            .cmp(&b.scenario)
            .then_with(|| a.offered_tps.cmp(&b.offered_tps))
            .then_with(|| a.repeat.cmp(&b.repeat))
            .then_with(|| a.timestamp.cmp(&b.timestamp))
    });

    if rows.is_empty() {
        bail!(
            "no local runtime rows found for run label {} in {} after timestamp {}",
            run_label,
            path.display(),
            start_ts
        );
    }

    Ok(rows)
}

fn parse_env_u64(name: &str, default: u64) -> Result<u64> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .with_context(|| format!("{name} must be a positive integer, got {value}")),
        Err(_) => Ok(default),
    }
}

fn parse_env_u32(name: &str, default: u32) -> Result<u32> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u32>()
            .with_context(|| format!("{name} must be a positive integer, got {value}")),
        Err(_) => Ok(default),
    }
}

fn parse_env_usize(name: &str, default: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .with_context(|| format!("{name} must be a positive integer, got {value}")),
        Err(_) => Ok(default),
    }
}

fn parse_env_bool(name: &str, default: bool) -> Result<bool> {
    match std::env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => bail!("{name} must be one of 1|0|true|false|yes|no|on|off"),
        },
        Err(_) => Ok(default),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bench crate has crates/ parent")
        .parent()
        .expect("bench crate has repo root parent")
        .to_path_buf()
}

fn default_script_path() -> PathBuf {
    repo_root().join("scripts/tps/bench-simple.sh")
}

fn tps_csv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/compliance/tps/tps.csv")
}

fn pre_consensus_csv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/pre_consensus/pre_consensus.csv")
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{load_synthetic_reference, SyntheticReferenceSelector};

    #[test]
    fn synthetic_reference_loader_picks_latest_matching_row() {
        let path = test_path("pre_consensus.csv");
        fs::write(
            &path,
            "version,tx_count,mode,concurrency,indexing_mode,metric,mean_ms,median_ms,samples,profile,run_id,timestamp,git_rev,host_label\n\
local,4000,v2_warm,32,no_index,preconsensus_tps,410.00,410.00,1,quick,run-old,10,rev,host\n\
local,4000,v2_warm,32,no_index,preconsensus_tps,435.26,435.26,1,quick,run-new,20,rev,host\n\
local,1000,v2_warm,32,no_index,preconsensus_tps,300.00,300.00,1,quick,run-other,30,rev,host\n",
        )
        .expect("write csv");

        let selector = SyntheticReferenceSelector {
            tx_count: 4_000,
            mode: "v2_warm".to_string(),
            concurrency: 32,
            indexing_mode: "no_index".to_string(),
            version: "local".to_string(),
        };

        let reference = load_synthetic_reference(&path, &selector).expect("load reference");
        assert_eq!(reference.run_id, "run-new");
        assert!((reference.mean_ms - 435.26).abs() < f64::EPSILON);
        let _ = fs::remove_file(path);
    }

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "penumbra-local-fullnode-{name}-{}-{}",
            std::process::id(),
            super::unix_ts()
        ))
    }
}
