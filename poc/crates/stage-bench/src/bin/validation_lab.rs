use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{ArgAction, Parser};
use cnidarium::Storage;
use penumbra_sdk_app::SUBSTORE_PREFIXES;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_bench::validation::{
    default_validation_builder_config, generate_prebuilt_validation_corpus, load_validation_corpus,
    run_validation_lab, ValidationGenerationMode, ValidationLabConfig, ValidationV1Result,
};
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_transaction::Transaction;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "validation_lab")]
#[clap(about = "Generate prebuilt validation blocks and benchmark app-level validation cost")]
struct Cli {
    #[clap(long)]
    prebuilt_corpus: PathBuf,
    #[clap(long)]
    tx_corpus: Option<PathBuf>,
    #[clap(long)]
    generate: bool,
    #[clap(long)]
    generate_one_shot: bool,
    #[clap(long, default_value_t = 2048)]
    offered_tps: usize,
    #[clap(long, default_value_t = 1000)]
    block_interval_ms: u64,
    #[clap(long, default_value_t = 4)]
    num_validators: usize,
    #[clap(long, default_value_t = 0)]
    proposer_index: usize,
    #[clap(long, default_value_t = 2048)]
    max_block_txs: usize,
    #[clap(long, default_value_t = 512)]
    segment_tx_count: usize,
    #[clap(long, default_value_t = 0)]
    warmup_local_turns: usize,
    #[clap(long, default_value_t = 1)]
    steady_local_turns: usize,
    #[clap(long, default_value_t = 7_000_000)]
    max_proposal_bytes: usize,
    #[clap(long, default_value_t = 268_435_456)]
    max_store_bytes: usize,
    #[clap(long, default_value_t = 40_000)]
    max_store_txs: usize,
    #[clap(long)]
    source_builder_label: Option<String>,
    #[clap(long)]
    rocksdb_home: Option<PathBuf>,
    #[clap(long, default_value_t = true)]
    with_local_cache: bool,
    #[clap(long, action = ArgAction::Set, default_value_t = false)]
    unchecked_aggregate_deserialization: bool,
    #[clap(long, default_value_t = 0)]
    warmup_blocks: usize,
    #[clap(long)]
    print_header: bool,
}

#[derive(Serialize)]
struct SummaryRow {
    run_id: String,
    prebuilt_corpus: String,
    source_corpus: String,
    rocksdb_home: String,
    with_local_cache: bool,
    unchecked_aggregate_deserialization: bool,
    block_count: usize,
    git_rev: String,
    host_label: String,
    timestamp: u64,
    #[serde(flatten)]
    result: ValidationV1Result,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let rocksdb_home = cli.rocksdb_home.unwrap_or_else(default_rocksdb_home);
    let storage = Storage::load(rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
        .await
        .with_context(|| format!("loading RocksDB from {}", rocksdb_home.display()))?;

    if cli.generate || cli.generate_one_shot {
        let tx_corpus = cli
            .tx_corpus
            .as_ref()
            .context("--tx-corpus is required when --generate or --generate-one-shot is set")?;
        let corpus = corpus::load_corpus(tx_corpus)
            .with_context(|| format!("loading corpus {}", tx_corpus.display()))?;
        let decoded = corpus
            .entries
            .iter()
            .map(|entry| {
                Transaction::decode(entry.tx_bytes.as_slice())
                    .map(Arc::new)
                    .with_context(|| format!("decoding tx ordinal {}", entry.ordinal))
            })
            .collect::<Result<Vec<_>>>()?;
        let generated = generate_prebuilt_validation_corpus(
            decoded,
            storage.latest_snapshot(),
            &cli.prebuilt_corpus,
            {
                let mut cfg = default_validation_builder_config();
                cfg.offered_tps = cli.offered_tps;
                cfg.block_interval_ms = cli.block_interval_ms;
                cfg.num_validators = cli.num_validators;
                cfg.proposer_index = cli.proposer_index;
                cfg.max_block_txs = cli.max_block_txs;
                cfg.segment_tx_count = cli.segment_tx_count;
                cfg.warmup_local_turns = cli.warmup_local_turns;
                cfg.steady_local_turns = cli.steady_local_turns;
                cfg.max_proposal_bytes = cli.max_proposal_bytes;
                cfg.max_store_bytes = cli.max_store_bytes;
                cfg.max_store_txs = cli.max_store_txs;
                cfg.generation_mode = if cli.generate_one_shot {
                    ValidationGenerationMode::OneShot
                } else {
                    ValidationGenerationMode::Cadence
                };
                if let Some(label) = &cli.source_builder_label {
                    cfg.source_builder_label = label.clone();
                }
                cfg
            },
        )
        .await?;
        anyhow::ensure!(
            generated > 0,
            "prebuilt validation corpus generation produced no blocks"
        );
    }

    let validation_corpus = load_validation_corpus(&cli.prebuilt_corpus)
        .with_context(|| format!("loading prebuilt corpus {}", cli.prebuilt_corpus.display()))?;

    let result = run_validation_lab(
        &validation_corpus.envelopes,
        storage.latest_snapshot(),
        ValidationLabConfig {
            with_local_cache: cli.with_local_cache,
            unchecked_aggregate_deserialization: cli.unchecked_aggregate_deserialization,
            warmup_blocks: cli.warmup_blocks,
        },
    )
    .await?;

    let row = SummaryRow {
        run_id: format!("run-{}-{}", unix_ts(), std::process::id()),
        prebuilt_corpus: cli.prebuilt_corpus.display().to_string(),
        source_corpus: cli
            .tx_corpus
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        rocksdb_home: rocksdb_home.display().to_string(),
        with_local_cache: cli.with_local_cache,
        unchecked_aggregate_deserialization: cli.unchecked_aggregate_deserialization,
        block_count: validation_corpus.envelopes.len(),
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
