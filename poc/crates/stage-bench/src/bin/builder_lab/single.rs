use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use cnidarium::Storage;
use penumbra_sdk_app::SUBSTORE_PREFIXES;
use penumbra_sdk_bench::mempool::SyntheticFeeMode;
use penumbra_sdk_bench::single_builder::{
    run_single_builder_lab, SingleBuilderConfig, SingleBuilderMode, SingleBuilderResult,
};
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_poc_preconsensus::local_mempool::FeeEvictionPolicy;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "single_builder_lab")]
#[clap(about = "Benchmark one mempool + one builder under concurrent admission and block cadence")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long)]
    rocksdb_home: Option<PathBuf>,
    #[clap(long)]
    mode: String,
    #[clap(long, default_value_t = 50)]
    offered_tps: usize,
    #[clap(long, default_value_t = 1000)]
    block_interval_ms: u64,
    #[clap(long, default_value_t = 2)]
    warmup_blocks: usize,
    #[clap(long, default_value_t = 8)]
    measured_blocks: usize,
    #[clap(long, default_value_t = 256)]
    max_block_txs: usize,
    #[clap(long, default_value_t = 32)]
    segment_tx_count: usize,
    #[clap(long, default_value_t = 3_500_000)]
    max_proposal_bytes: usize,
    #[clap(long, default_value_t = 268_435_456)]
    max_store_bytes: usize,
    #[clap(long, default_value_t = 40_000)]
    max_store_txs: usize,
    #[clap(long, default_value_t = 1)]
    rayon_threads_per_batch: usize,
    #[clap(long, default_value = "off")]
    synthetic_fee_spread: String,
    #[clap(long, default_value = "disabled")]
    fee_eviction_policy: String,
    #[clap(long, default_value_t = 0)]
    corpus_limit: usize,
    #[clap(long)]
    debug_run_dir: Option<PathBuf>,
    #[clap(long)]
    builder_after_admission: bool,
    #[clap(long)]
    print_header: bool,
}

#[derive(Serialize)]
struct SummaryRow {
    run_id: String,
    mode: String,
    corpus: String,
    rocksdb_home: String,
    offered_tps: usize,
    block_interval_ms: u64,
    max_block_txs: usize,
    segment_tx_count: usize,
    max_proposal_bytes: usize,
    max_store_bytes: usize,
    max_store_txs: usize,
    rayon_threads_per_batch: usize,
    synthetic_fee_mode: String,
    fee_eviction_policy: String,
    corpus_tx_count: usize,
    git_rev: String,
    host_label: String,
    timestamp: u64,
    #[serde(flatten)]
    result: SingleBuilderResult,
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    if std::env::var_os("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP").is_none() {
        unsafe {
            std::env::set_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP", "1");
        }
    }

    let cli = Cli::parse_from(args);
    if let Some(debug_run_dir) = &cli.debug_run_dir {
        std::fs::create_dir_all(debug_run_dir)
            .with_context(|| format!("creating debug run directory {}", debug_run_dir.display()))?;
        unsafe {
            std::env::set_var("PENUMBRA_AGGREGATE_DEBUG_DIR", debug_run_dir);
        }
    }
    let mode: SingleBuilderMode = cli.mode.parse()?;
    let synthetic_fee_mode: SyntheticFeeMode = cli.synthetic_fee_spread.parse()?;
    let fee_eviction_policy = parse_fee_eviction_policy(&cli.fee_eviction_policy)?;
    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;

    let txs = corpus
        .entries
        .iter()
        .take(if cli.corpus_limit == 0 {
            corpus.entries.len()
        } else {
            cli.corpus_limit.min(corpus.entries.len())
        })
        .map(|entry| Arc::new(entry.tx_bytes.clone()))
        .collect::<Vec<_>>();

    let rocksdb_home = cli.rocksdb_home.unwrap_or_else(default_rocksdb_home);
    let storage = Storage::load(rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
        .await
        .with_context(|| format!("loading RocksDB from {}", rocksdb_home.display()))?;

    let result = run_single_builder_lab(
        txs.clone(),
        storage.latest_snapshot(),
        SingleBuilderConfig {
            mode,
            offered_tps: cli.offered_tps,
            block_interval_ms: cli.block_interval_ms,
            warmup_blocks: cli.warmup_blocks,
            measured_blocks: cli.measured_blocks,
            max_block_txs: cli.max_block_txs,
            segment_tx_count: cli.segment_tx_count,
            max_proposal_bytes: cli.max_proposal_bytes,
            max_store_bytes: cli.max_store_bytes,
            max_store_txs: cli.max_store_txs,
            synthetic_fee_mode,
            fee_eviction_policy,
            rayon_threads_per_batch: cli.rayon_threads_per_batch,
            builder_after_admission: cli.builder_after_admission,
        },
    )
    .await?;

    let row = SummaryRow {
        run_id: format!("run-{}-{}", unix_ts(), std::process::id()),
        mode: mode.as_str().to_string(),
        corpus: cli.corpus.display().to_string(),
        rocksdb_home: rocksdb_home.display().to_string(),
        offered_tps: cli.offered_tps,
        block_interval_ms: cli.block_interval_ms,
        max_block_txs: cli.max_block_txs,
        segment_tx_count: cli.segment_tx_count,
        max_proposal_bytes: cli.max_proposal_bytes,
        max_store_bytes: cli.max_store_bytes,
        max_store_txs: cli.max_store_txs,
        rayon_threads_per_batch: cli.rayon_threads_per_batch,
        synthetic_fee_mode: synthetic_fee_mode.as_str().to_string(),
        fee_eviction_policy: cli.fee_eviction_policy.clone(),
        corpus_tx_count: txs.len(),
        git_rev: std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string()),
        host_label: std::env::var("BENCH_HOST_LABEL")
            .or_else(|_| std::env::var("HOSTNAME"))
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown-host".to_string()),
        timestamp: unix_ts(),
        result,
    };

    let mut writer = csv::WriterBuilder::new()
        .has_headers(cli.print_header)
        .from_writer(std::io::stdout());
    writer.serialize(&row)?;
    writer.flush()?;
    Ok(())
}

fn default_rocksdb_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".penumbra/network_data/node0/pd/rocksdb")
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn parse_fee_eviction_policy(value: &str) -> Result<FeeEvictionPolicy> {
    match value {
        "disabled" => Ok(FeeEvictionPolicy::Disabled),
        "launch-staking-priority" => Ok(FeeEvictionPolicy::LaunchStakingPriority),
        other => anyhow::bail!("unknown fee eviction policy: {other}"),
    }
}
