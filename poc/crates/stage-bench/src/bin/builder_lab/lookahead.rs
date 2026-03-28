use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use penumbra_sdk_bench::lookahead_builder::{
    build_admitted_transactions, run_builder_lab, BuilderMode, LookaheadLabConfig, LookaheadLabResult,
};
use penumbra_sdk_bench::mempool::SyntheticFeeMode;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_poc_preconsensus::local_mempool::FeeEvictionPolicy;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_transaction::Transaction;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "lookahead_builder_lab")]
#[clap(
    about = "Benchmark whole-candidate lookahead vs monolithic builder under pure proposer slack"
)]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long, default_value = "lookahead")]
    mode: String,
    #[clap(long, default_value_t = 150)]
    offered_tps: usize,
    #[clap(long, default_value_t = 1000)]
    block_interval_ms: u64,
    #[clap(long, default_value_t = 4)]
    num_validators: usize,
    #[clap(long, default_value_t = 0)]
    proposer_index: usize,
    #[clap(long, default_value_t = 256)]
    max_block_txs: usize,
    #[clap(long, default_value_t = 32)]
    segment_tx_count: usize,
    #[clap(long, default_value_t = 2)]
    warmup_local_turns: usize,
    #[clap(long, default_value_t = 8)]
    steady_local_turns: usize,
    #[clap(long, default_value_t = 3_500_000)]
    max_proposal_bytes: usize,
    #[clap(long, default_value_t = 268_435_456)]
    max_store_bytes: usize,
    #[clap(long, default_value_t = 40_000)]
    max_store_txs: usize,
    #[clap(long, default_value = "off")]
    synthetic_fee_spread: String,
    #[clap(long, default_value = "disabled")]
    fee_eviction_policy: String,
    #[clap(long, default_value_t = 100)]
    ready_guard_ms: u64,
    #[clap(long)]
    print_header: bool,
}

#[derive(Serialize)]
struct SummaryRow {
    run_id: String,
    mode: String,
    corpus: String,
    offered_tps: usize,
    block_interval_ms: u64,
    num_validators: usize,
    proposer_index: usize,
    max_block_txs: usize,
    segment_tx_count: usize,
    warmup_local_turns: usize,
    steady_local_turns: usize,
    max_store_bytes: usize,
    max_store_txs: usize,
    synthetic_fee_mode: String,
    fee_eviction_policy: String,
    ready_guard_ms: u64,
    git_rev: String,
    host_label: String,
    timestamp: u64,
    #[serde(flatten)]
    result: LookaheadLabResult,
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    let synthetic_fee_mode: SyntheticFeeMode = cli.synthetic_fee_spread.parse()?;
    let fee_eviction_policy = parse_fee_eviction_policy(&cli.fee_eviction_policy)?;
    let mode = parse_mode(&cli.mode)?;
    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;
    let decoded = corpus
        .entries
        .into_iter()
        .map(|entry| {
            Transaction::decode(entry.tx_bytes.as_slice())
                .map(|tx| (Arc::new(entry.tx_bytes), Arc::new(tx)))
                .with_context(|| format!("decoding tx ordinal {}", entry.ordinal))
        })
        .collect::<Result<Vec<_>>>()?;
    let admitted = build_admitted_transactions(decoded, 512, synthetic_fee_mode).await?;

    let result = run_builder_lab(
        admitted,
        LookaheadLabConfig {
            mode,
            offered_tps: cli.offered_tps,
            block_interval_ms: cli.block_interval_ms,
            num_validators: cli.num_validators,
            proposer_index: cli.proposer_index,
            max_block_txs: cli.max_block_txs,
            segment_tx_count: cli.segment_tx_count,
            warmup_local_turns: cli.warmup_local_turns,
            steady_local_turns: cli.steady_local_turns,
            max_proposal_bytes: cli.max_proposal_bytes,
            ready_guard_ms: cli.ready_guard_ms,
            max_store_bytes: cli.max_store_bytes,
            max_store_txs: cli.max_store_txs,
            synthetic_fee_mode,
            fee_eviction_policy,
        },
    )
    .await?;

    let row = SummaryRow {
        run_id: format!("run-{}-{}", unix_ts(), std::process::id()),
        mode: cli.mode,
        corpus: cli.corpus.display().to_string(),
        offered_tps: cli.offered_tps,
        block_interval_ms: cli.block_interval_ms,
        num_validators: cli.num_validators,
        proposer_index: cli.proposer_index,
        max_block_txs: cli.max_block_txs,
        segment_tx_count: cli.segment_tx_count,
        warmup_local_turns: cli.warmup_local_turns,
        steady_local_turns: cli.steady_local_turns,
        max_store_bytes: cli.max_store_bytes,
        max_store_txs: cli.max_store_txs,
        synthetic_fee_mode: synthetic_fee_mode.as_str().to_string(),
        fee_eviction_policy: cli.fee_eviction_policy.clone(),
        ready_guard_ms: cli.ready_guard_ms,
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

fn parse_mode(raw: &str) -> Result<BuilderMode> {
    match raw {
        "monolithic" => Ok(BuilderMode::Monolithic),
        "lookahead" => Ok(BuilderMode::Lookahead),
        _ => anyhow::bail!("unsupported mode '{raw}', expected monolithic|lookahead"),
    }
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
