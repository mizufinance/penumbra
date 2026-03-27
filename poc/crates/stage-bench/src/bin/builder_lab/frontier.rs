use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use penumbra_sdk_bench::lookahead_builder::{build_admitted_transactions, BuilderMode};
use penumbra_sdk_bench::lookahead_builder_frontier::{
    run_frontier, FrontierRawRow, FrontierRun, FrontierSummaryRow, FrontierSweepConfig,
};
use penumbra_sdk_bench::mempool::SyntheticFeeMode;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_transaction::Transaction;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "lookahead_builder_frontier")]
#[clap(about = "Run the builder frontier sweep and emit raw + summary CSVs")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long, default_value = "lookahead")]
    mode: String,
    #[clap(long)]
    raw_output: PathBuf,
    #[clap(long)]
    summary_output: PathBuf,
    #[clap(long, default_value = "150,300,600")]
    offered_tps_list: String,
    #[clap(long, default_value = "500,1000,2000")]
    block_interval_ms_list: String,
    #[clap(long, default_value = "128,256,512")]
    max_block_txs_list: String,
    #[clap(long, default_value = "32")]
    segment_tx_count_list: String,
    #[clap(long, default_value = "100")]
    ready_guard_ms_list: String,
    #[clap(long, default_value_t = 4)]
    num_validators: usize,
    #[clap(long, default_value_t = 0)]
    proposer_index: usize,
    #[clap(long, default_value_t = 2)]
    warmup_local_turns: usize,
    #[clap(long, default_value_t = 8)]
    steady_local_turns: usize,
    #[clap(long, default_value_t = 3_500_000)]
    max_proposal_bytes: usize,
}

#[derive(Serialize)]
struct RawRow<'a> {
    run_id: &'a str,
    corpus: &'a str,
    num_validators: usize,
    proposer_index: usize,
    warmup_local_turns: usize,
    steady_local_turns: usize,
    git_rev: &'a str,
    host_label: &'a str,
    timestamp: u64,
    #[serde(flatten)]
    row: &'a FrontierRawRow,
}

#[derive(Serialize)]
struct SummaryRow<'a> {
    run_id: &'a str,
    corpus: &'a str,
    git_rev: &'a str,
    host_label: &'a str,
    timestamp: u64,
    #[serde(flatten)]
    row: &'a FrontierSummaryRow,
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
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
    let admitted = build_admitted_transactions(decoded, 512, SyntheticFeeMode::Off).await?;

    let sweep = FrontierSweepConfig {
        mode,
        offered_tps_list: parse_usize_list(&cli.offered_tps_list)?,
        block_interval_ms_list: parse_u64_list(&cli.block_interval_ms_list)?,
        max_block_txs_list: parse_usize_list(&cli.max_block_txs_list)?,
        segment_tx_count_list: parse_usize_list(&cli.segment_tx_count_list)?,
        ready_guard_ms_list: parse_u64_list(&cli.ready_guard_ms_list)?,
        num_validators: cli.num_validators,
        proposer_index: cli.proposer_index,
        warmup_local_turns: cli.warmup_local_turns,
        steady_local_turns: cli.steady_local_turns,
        max_proposal_bytes: cli.max_proposal_bytes,
    };

    let frontier = run_frontier(&admitted, sweep).await?;
    fs::create_dir_all(
        cli.raw_output
            .parent()
            .expect("raw_output should have a parent directory"),
    )?;
    fs::create_dir_all(
        cli.summary_output
            .parent()
            .expect("summary_output should have a parent directory"),
    )?;

    let run_id = format!("run-{}-{}", unix_ts(), std::process::id());
    let timestamp = unix_ts();
    let git_rev = std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string());
    let host_label = std::env::var("BENCH_HOST_LABEL")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string());
    write_raw_csv(&frontier, &cli, &run_id, timestamp, &git_rev, &host_label)?;
    write_summary_csv(&frontier, &cli, &run_id, timestamp, &git_rev, &host_label)?;
    Ok(())
}

fn write_raw_csv(
    frontier: &FrontierRun,
    cli: &Cli,
    run_id: &str,
    timestamp: u64,
    git_rev: &str,
    host_label: &str,
) -> Result<()> {
    let mut writer = csv::Writer::from_path(&cli.raw_output)?;
    let corpus = cli.corpus.display().to_string();
    for row in &frontier.raw_rows {
        writer.serialize(RawRow {
            run_id,
            corpus: &corpus,
            num_validators: cli.num_validators,
            proposer_index: cli.proposer_index,
            warmup_local_turns: cli.warmup_local_turns,
            steady_local_turns: cli.steady_local_turns,
            git_rev,
            host_label,
            timestamp,
            row,
        })?;
    }
    writer.flush()?;
    Ok(())
}

fn write_summary_csv(
    frontier: &FrontierRun,
    cli: &Cli,
    run_id: &str,
    timestamp: u64,
    git_rev: &str,
    host_label: &str,
) -> Result<()> {
    let mut writer = csv::Writer::from_path(&cli.summary_output)?;
    let corpus = cli.corpus.display().to_string();
    for row in &frontier.summary_rows {
        writer.serialize(SummaryRow {
            run_id,
            corpus: &corpus,
            git_rev,
            host_label,
            timestamp,
            row,
        })?;
    }
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

fn mode_label(mode: BuilderMode) -> &'static str {
    match mode {
        BuilderMode::Monolithic => "monolithic",
        BuilderMode::Lookahead => "lookahead",
    }
}

fn parse_usize_list(raw: &str) -> Result<Vec<usize>> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("parsing integer list entry '{value}'"))
        })
        .collect()
}

fn parse_u64_list(raw: &str) -> Result<Vec<u64>> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("parsing integer list entry '{value}'"))
        })
        .collect()
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
