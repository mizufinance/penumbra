use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use cnidarium::Storage;
use penumbra_sdk_app::block_tx_indexing::BlockTxIndexingMode;
use penumbra_sdk_app::SUBSTORE_PREFIXES;
use penumbra_sdk_bench::execution::{
    preflight_execution_v1_corpus, prepare_scratch_rocksdb, run_execution_lab,
    ExecutionLabConfig, ExecutionV1Result,
};
use penumbra_sdk_bench::validation::load_validation_corpus;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "execution_lab")]
#[clap(about = "Benchmark app-level execution cost for already-validated candidate envelopes")]
struct Cli {
    #[clap(long)]
    prebuilt_corpus: PathBuf,
    #[clap(long)]
    rocksdb_home: PathBuf,
    #[clap(long)]
    scratch_rocksdb_home: PathBuf,
    #[clap(long, default_value_t = 4)]
    warmup_blocks: usize,
    #[clap(long, default_value = "deferred_batch")]
    block_tx_indexing_mode: String,
    #[clap(long)]
    print_header: bool,
}

#[derive(Serialize)]
struct SummaryRow {
    run_id: String,
    prebuilt_corpus: String,
    rocksdb_home: String,
    scratch_rocksdb_home: String,
    block_tx_indexing_mode: String,
    block_count: usize,
    git_rev: String,
    host_label: String,
    timestamp: u64,
    #[serde(flatten)]
    result: ExecutionV1Result,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    if std::env::var_os("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP").is_none() {
        unsafe {
            std::env::set_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP", "1");
        }
    }

    let cli = Cli::parse();
    let block_tx_indexing_mode = parse_block_tx_indexing_mode(&cli.block_tx_indexing_mode)?;
    let corpus = load_validation_corpus(&cli.prebuilt_corpus)
        .with_context(|| format!("loading prebuilt corpus {}", cli.prebuilt_corpus.display()))?;

    prepare_scratch_rocksdb(&cli.rocksdb_home, &cli.scratch_rocksdb_home)?;
    let preflight_storage =
        Storage::load(cli.scratch_rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
            .await
            .with_context(|| {
                format!(
                    "loading preflight scratch RocksDB from {}",
                    cli.scratch_rocksdb_home.display()
                )
            })?;
    preflight_execution_v1_corpus(
        &corpus.envelopes,
        preflight_storage.clone(),
        block_tx_indexing_mode,
    )
    .await?;
    preflight_storage.release().await;

    prepare_scratch_rocksdb(&cli.rocksdb_home, &cli.scratch_rocksdb_home)?;
    let execution_storage =
        Storage::load(cli.scratch_rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
            .await
            .with_context(|| {
                format!(
                    "loading execution scratch RocksDB from {}",
                    cli.scratch_rocksdb_home.display()
                )
            })?;
    let result = run_execution_lab(
        &corpus.envelopes,
        execution_storage.clone(),
        ExecutionLabConfig {
            warmup_blocks: cli.warmup_blocks,
            block_tx_indexing_mode,
        },
    )
    .await?;
    execution_storage.release().await;

    let row = SummaryRow {
        run_id: format!("run-{}-{}", unix_ts(), std::process::id()),
        prebuilt_corpus: cli.prebuilt_corpus.display().to_string(),
        rocksdb_home: cli.rocksdb_home.display().to_string(),
        scratch_rocksdb_home: cli.scratch_rocksdb_home.display().to_string(),
        block_tx_indexing_mode: block_tx_indexing_mode.as_str().to_string(),
        block_count: corpus.envelopes.len(),
        git_rev: std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string()),
        host_label: std::env::var("BENCH_HOST_LABEL").unwrap_or_else(|_| "unknown-host".to_string()),
        timestamp: unix_ts(),
        result,
    };

    let mut writer = csv::Writer::from_writer(std::io::stdout());
    if cli.print_header {
        writer.serialize(&row)?;
    } else {
        writer.serialize(&row)?;
    }
    writer.flush()?;

    Ok(())
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn parse_block_tx_indexing_mode(value: &str) -> Result<BlockTxIndexingMode> {
    match value {
        "no_index" => Ok(BlockTxIndexingMode::NoIndex),
        "per_tx" => Ok(BlockTxIndexingMode::PerTx),
        "deferred_batch" => Ok(BlockTxIndexingMode::DeferredBatch),
        _ => anyhow::bail!(
            "invalid block tx indexing mode {value:?}, expected one of: no_index, per_tx, deferred_batch"
        ),
    }
}
