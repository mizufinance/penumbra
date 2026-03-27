use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use cnidarium::Storage;
use penumbra_sdk_app::SUBSTORE_PREFIXES;
use penumbra_sdk_bench::mempool::{
    run_mempool_lab, CheckTxMode, MempoolV1Config, MempoolV1Result, SyntheticFeeMode,
};
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_poc_preconsensus::local_mempool::FeeEvictionPolicy;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "mempool_lab")]
#[clap(about = "Benchmark the direct app-level mempool admission stage")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long)]
    rocksdb_home: Option<PathBuf>,
    #[clap(long, default_value_t = 14)]
    worker_count: usize,
    #[clap(long, default_value_t = 1_073_741_824)]
    max_store_bytes: usize,
    #[clap(long, default_value_t = 200_000)]
    max_store_txs: usize,
    #[clap(long, default_value_t = 1000)]
    commit_interval_ms: u64,
    #[clap(long, default_value_t = 2048)]
    commit_batch_size: usize,
    #[clap(long, default_value = "off")]
    synthetic_fee_spread: String,
    #[clap(long, default_value = "disabled")]
    fee_eviction_policy: String,
    #[clap(long, default_value_t = 0)]
    corpus_limit: usize,
    #[clap(long, default_value = "strict")]
    mode: String,
    #[clap(long, default_value_t = 32)]
    verify_batch_size: usize,
    #[clap(long, default_value_t = 0)]
    verify_worker_count: usize,
    #[clap(long, default_value_t = 0)]
    admit_batch_size: usize,
    #[clap(long)]
    print_header: bool,
}

#[derive(Serialize)]
struct SummaryRow {
    run_id: String,
    corpus: String,
    rocksdb_home: String,
    worker_count: usize,
    max_store_bytes: usize,
    max_store_txs: usize,
    commit_interval_ms: u64,
    commit_batch_size: usize,
    checktx_mode: String,
    verify_batch_size: usize,
    verify_worker_count: usize,
    synthetic_fee_mode: String,
    fee_eviction_policy: String,
    corpus_tx_count: usize,
    git_rev: String,
    host_label: String,
    timestamp: u64,
    #[serde(flatten)]
    result: MempoolV1Result,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    if std::env::var_os("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP").is_none() {
        unsafe {
            std::env::set_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP", "1");
        }
    }

    let cli = Cli::parse();
    let synthetic_fee_mode: SyntheticFeeMode = cli.synthetic_fee_spread.parse()?;
    let fee_eviction_policy = parse_fee_eviction_policy(&cli.fee_eviction_policy)?;
    let checktx_mode: CheckTxMode = cli.mode.parse()?;
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

    let result = run_mempool_lab(
        txs.clone(),
        storage.latest_snapshot(),
        MempoolV1Config {
            worker_count: cli.worker_count,
            max_store_bytes: cli.max_store_bytes,
            max_store_txs: cli.max_store_txs,
            commit_interval_ms: cli.commit_interval_ms,
            commit_batch_size: cli.commit_batch_size,
            synthetic_fee_mode,
            fee_eviction_policy,
            checktx_mode,
            verify_batch_size: cli.verify_batch_size,
            verify_worker_count: cli.verify_worker_count,
            admit_batch_size: cli.admit_batch_size,
        },
    )
    .await?;

    let row = SummaryRow {
        run_id: format!("run-{}-{}", unix_ts(), std::process::id()),
        corpus: cli.corpus.display().to_string(),
        rocksdb_home: rocksdb_home.display().to_string(),
        worker_count: cli.worker_count,
        max_store_bytes: cli.max_store_bytes,
        max_store_txs: cli.max_store_txs,
        commit_interval_ms: cli.commit_interval_ms,
        commit_batch_size: cli.commit_batch_size,
        checktx_mode: checktx_mode.as_str().to_string(),
        verify_batch_size: cli.verify_batch_size,
        verify_worker_count: cli.verify_worker_count,
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
