use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use penumbra_sdk_poc_stage_bench::tps::{
    aggregate,
    config::{EndpointKind, TpsConfig},
    corpus, observer,
    regulated_local::{self, BuildLocalArgs},
    report::{self, ResponseCodeCount, ResponseLogCount, RunDiagnostics, SubmissionRollup},
    sender::{self, SenderConfig},
};
use penumbra_sdk_fee::FeeTier;
use penumbra_sdk_keys::Address;

const MEMPOOL_CHECKTX_TOTAL: &str = "penumbra_pd_mempool_checktx_total";
const MEMPOOL_CHECKTX_DURATION: &str = "penumbra_pd_mempool_checktx_duration_seconds";
const MEMPOOL_CHECKTX_PENDING: &str = "penumbra_pd_mempool_checktx_pending";
const MEMPOOL_CHECKTX_IN_FLIGHT: &str = "penumbra_pd_mempool_checktx_in_flight";
const COMET_MEMPOOL_SIZE: &str = "cometbft_mempool_size";
const COMET_MEMPOOL_SIZE_BYTES: &str = "cometbft_mempool_size_bytes";
const COMET_MEMPOOL_RECHECK_TIMES: &str = "cometbft_mempool_recheck_times";

#[derive(Debug, Parser)]
#[clap(name = "compliance_tps")]
#[clap(about = "Consensus TPS harness for external Penumbra clusters")]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the TPS matrix from a YAML configuration file.
    Run {
        #[clap(long)]
        config: PathBuf,
    },
    /// Corpus-related operations.
    Corpus {
        #[clap(subcommand)]
        command: CorpusCommand,
    },
    /// Reporting operations.
    Report {
        #[clap(subcommand)]
        command: ReportCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CorpusCommand {
    /// Write a genesis allocations CSV from a local pcli wallet's address indexes.
    WriteGenesisAllocations {
        #[clap(long)]
        wallet_home: PathBuf,
        #[clap(long)]
        count: usize,
        #[clap(long)]
        denom: String,
        #[clap(long)]
        amount: u128,
        #[clap(long)]
        out: PathBuf,
    },
    /// Count distinct spendable source indexes for an asset in a local wallet DB.
    CountLocalSources {
        #[clap(long)]
        wallet_home: PathBuf,
        #[clap(long)]
        asset: String,
        #[clap(long)]
        no_sync: bool,
    },
    /// Build a corpus locally from a synced pcli wallet and view DB.
    BuildLocal {
        #[clap(long)]
        scenario: String,
        #[clap(long)]
        wallet_home: PathBuf,
        #[clap(long)]
        asset: String,
        #[clap(long)]
        count: usize,
        #[clap(long, default_value_t = 0)]
        source_start: usize,
        #[clap(long)]
        to_address: Address,
        #[clap(long)]
        out: PathBuf,
        #[clap(long)]
        observer: Option<String>,
        #[clap(long, default_value = "low")]
        fee_tier: FeeTier,
        #[clap(long)]
        asset_kind: Option<String>,
        #[clap(long, default_value = "local")]
        source_label: String,
        #[clap(long)]
        chain_id: Option<String>,
        #[clap(long, default_value = "unknown")]
        genesis_hash: String,
        #[clap(long, default_value = "")]
        notes: String,
        #[clap(long, default_value_t = default_build_concurrency())]
        concurrency: usize,
        #[clap(long)]
        no_sync: bool,
    },
    /// Merge multiple corpus directories into one corpus.
    Merge {
        #[clap(long, required = true)]
        inputs: Vec<PathBuf>,
        #[clap(long)]
        out: PathBuf,
        #[clap(long, default_value = "")]
        source_label: String,
        #[clap(long, default_value = "")]
        notes: String,
    },
    /// Validate corpus integrity and observer compatibility.
    Verify {
        #[clap(long)]
        corpus: PathBuf,
        #[clap(long)]
        observer: String,
        #[clap(long, default_value = "tendermint-proxy")]
        endpoint_kind: String,
    },
    /// Convert offline Transaction JSON files into corpus artifacts.
    Pack {
        #[clap(long)]
        json_dir: PathBuf,
        #[clap(long)]
        out: PathBuf,
        #[clap(long)]
        scenario: String,
        #[clap(long)]
        asset_kind: String,
        #[clap(long, default_value = "local")]
        source_label: String,
        #[clap(long, default_value = "unknown")]
        chain_id: String,
        #[clap(long, default_value = "unknown")]
        genesis_hash: String,
        #[clap(long, default_value = "")]
        notes: String,
    },
    /// Append offline Transaction JSON files into an existing corpus.
    Append {
        #[clap(long)]
        json_dir: PathBuf,
        #[clap(long)]
        corpus: PathBuf,
        #[clap(long)]
        asset_kind: String,
        #[clap(long, default_value = "local")]
        source_label: String,
        #[clap(long, default_value = "")]
        notes: String,
    },
}

#[derive(Debug, Subcommand)]
enum ReportCommand {
    /// Rebuild top-level `tps.csv` and `profiles.csv` from runs/*/summary.json.
    Summarize {
        #[clap(long)]
        input: PathBuf,
        #[clap(long)]
        output: PathBuf,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { config } => run_config(&config).await,
        Command::Corpus { command } => run_corpus(command).await,
        Command::Report { command } => run_report(command).await,
    }
}

fn parse_endpoint_kind(raw: &str) -> Result<EndpointKind> {
    match raw {
        "tendermint-proxy" => Ok(EndpointKind::TendermintProxy),
        "node-service" => Ok(EndpointKind::NodeService),
        _ => Err(anyhow::anyhow!(
            "unsupported endpoint kind '{raw}', expected tendermint-proxy|node-service"
        )),
    }
}

async fn run_config(path: &PathBuf) -> Result<()> {
    let cfg = TpsConfig::load(path)?;
    let cases = cfg.expand_cases();
    anyhow::ensure!(!cases.is_empty(), "config produced zero run cases");

    let mut corpora_by_scenario = std::collections::BTreeMap::new();
    for scenario in &cfg.scenarios {
        if corpora_by_scenario.contains_key(&scenario.name) {
            continue;
        }
        let verify_report = corpus::verify_corpus(
            &scenario.corpus_dir,
            &cfg.observer_endpoint,
            &cfg.endpoint_kind,
        )
        .await
        .with_context(|| {
            format!(
                "failed corpus verify for scenario {} at {}",
                scenario.name,
                scenario.corpus_dir.display()
            )
        })?;
        eprintln!(
            "verified corpus scenario={} tx_count={} chain_id={}",
            scenario.name,
            verify_report.tx_count,
            verify_report
                .chain_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        );

        let corpus = corpus::load_corpus(&scenario.corpus_dir).with_context(|| {
            format!(
                "failed loading corpus for scenario {} from {}",
                scenario.name,
                scenario.corpus_dir.display()
            )
        })?;
        corpora_by_scenario.insert(scenario.name.clone(), corpus);
    }

    let execution_plan = build_execution_plan(&cfg, cases, &corpora_by_scenario)?;
    for (scenario_name, assigned) in execution_plan.iter().fold(
        std::collections::BTreeMap::<String, usize>::new(),
        |mut acc, plan| {
            *acc.entry(plan.case.scenario.name.clone()).or_default() += plan.corpus_len;
            acc
        },
    ) {
        if let Some(corpus) = corpora_by_scenario.get(&scenario_name) {
            eprintln!(
                "scenario={} corpus_assigned={} corpus_available={}",
                scenario_name,
                assigned,
                corpus.entries.len()
            );
        }
    }

    let mut invalid_runs = Vec::new();
    for plan_case in execution_plan {
        let case = plan_case.case;
        let run_id = make_run_id(&case.scenario.name, case.offered_tps, case.repeat);
        eprintln!(
            "=== compliance_tps run_id={} scenario={} offered_tps={} repeat={} ===",
            run_id, case.scenario.name, case.offered_tps, case.repeat
        );

        let scenario_corpus = corpora_by_scenario
            .get(&case.scenario.name)
            .ok_or_else(|| {
                anyhow::anyhow!("missing loaded corpus for scenario {}", case.scenario.name)
            })?;
        let assigned_entries = scenario_corpus.entries
            [plan_case.corpus_start..(plan_case.corpus_start + plan_case.corpus_len)]
            .to_vec();
        let corpus = corpus::Corpus {
            manifest: scenario_corpus.manifest.clone(),
            entries: assigned_entries,
        };

        let plan = observer::plan_heights(
            &cfg.observer_endpoint,
            &cfg.endpoint_kind,
            case.scenario.warmup_blocks,
            case.scenario.steady_blocks,
        )
        .await
        .with_context(|| {
            format!(
                "failed planning heights for scenario={} offered_tps={} repeat={}",
                case.scenario.name, case.offered_tps, case.repeat
            )
        })?;

        let t0 = Instant::now();
        let metrics_before = if let Some(metrics_endpoint) = &cfg.metrics_endpoint {
            Some(
                fetch_pd_metrics_snapshot(metrics_endpoint)
                    .await
                    .with_context(|| format!("failed fetching pd metrics before run {}", run_id))?,
            )
        } else {
            None
        };
        let (height_tx, height_rx) = tokio::sync::watch::channel(plan.start_height);
        let observer_endpoint = cfg.observer_endpoint.clone();
        let endpoint_kind = cfg.endpoint_kind.clone();
        let plan_for_observer = plan.clone();
        let observer_task = tokio::spawn(async move {
            observer::observe_until_end(
                &observer_endpoint,
                &endpoint_kind,
                plan_for_observer,
                t0,
                height_tx,
            )
            .await
        });

        let sender_output = sender::run_sender(
            &cfg.pd_endpoints,
            &corpus,
            SenderConfig {
                offered_tps: case.offered_tps,
                submit_workers: case.scenario.submit_workers,
                max_inflight: case.scenario.max_inflight,
                end_height: plan.end_height,
                submit_mode: case.scenario.submit_mode.clone(),
                endpoint_kind: cfg.endpoint_kind.clone(),
                pacer_tick_ms: case.scenario.pacer_tick_ms,
                disable_pacer: case.scenario.disable_pacer,
                burst_profile: case.scenario.burst_profile.clone(),
            },
            t0,
            height_rx,
        )
        .await
        .with_context(|| {
            format!(
                "sender failed for run_id={} scenario={} offered_tps={} repeat={}",
                run_id, case.scenario.name, case.offered_tps, case.repeat
            )
        })?;

        if sender_output.corpus_exhausted {
            eprintln!(
                "WARNING: sender exhausted corpus before end height for run_id={} scenario={} offered_tps={} repeat={}",
                run_id, case.scenario.name, case.offered_tps, case.repeat
            );
        }

        let observation = observer_task
            .await
            .context("observer task join failure")?
            .with_context(|| {
                format!(
                    "observer failed for run_id={} scenario={} offered_tps={} repeat={}",
                    run_id, case.scenario.name, case.offered_tps, case.repeat
                )
            })?;
        let metrics_after = if let Some(metrics_endpoint) = &cfg.metrics_endpoint {
            Some(
                fetch_pd_metrics_snapshot(metrics_endpoint)
                    .await
                    .with_context(|| format!("failed fetching pd metrics after run {}", run_id))?,
            )
        } else {
            None
        };
        let comet_metrics_after = if let Some(metrics_endpoint) = &cfg.comet_metrics_endpoint {
            Some(
                fetch_comet_metrics_snapshot(metrics_endpoint)
                    .await
                    .with_context(|| {
                        format!("failed fetching comet metrics after run {}", run_id)
                    })?,
            )
        } else {
            None
        };

        let (mut row, latencies, blocks) = aggregate::summarize_case(
            &cfg,
            &case.scenario,
            case.offered_tps,
            case.repeat,
            &run_id,
            &plan,
            &sender_output.submissions,
            &observation,
        );
        row.corpus_digest = scenario_corpus
            .manifest
            .corpus_digest
            .clone()
            .unwrap_or_default();
        row.corpus_required = plan_case.corpus_required as u64;
        row.corpus_assigned = plan_case.corpus_len as u64;
        row.corpus_exhausted = sender_output.corpus_exhausted;
        let submission_rollup = build_submission_rollup(&sender_output.submissions);
        if let (Some(before), Some(after)) = (metrics_before.as_ref(), metrics_after.as_ref()) {
            row.pd_checktx_new_ok_total = after
                .mempool
                .new_ok_total
                .saturating_sub(before.mempool.new_ok_total);
            row.pd_checktx_new_rejected_total = after
                .mempool
                .new_rejected_total
                .saturating_sub(before.mempool.new_rejected_total);
            row.checktx_accepted_total = row.pd_checktx_new_ok_total;
            row.checktx_rejected_total = row.pd_checktx_new_rejected_total;
            let duration_sum_delta = (after.mempool.new_duration_sum_seconds
                - before.mempool.new_duration_sum_seconds)
                .max(0.0);
            let duration_count_delta = after
                .mempool
                .new_duration_count
                .saturating_sub(before.mempool.new_duration_count);
            if duration_count_delta > 0 {
                row.pd_checktx_new_duration_mean_ms =
                    (duration_sum_delta / duration_count_delta as f64) * 1000.0;
            }
            row.pd_checktx_pending_final = after.mempool.pending;
            row.pd_checktx_in_flight_final = after.mempool.in_flight;
            let total_checktx = row
                .pd_checktx_new_ok_total
                .saturating_add(row.pd_checktx_new_rejected_total);
            if total_checktx > 0 {
                row.checktx_reject_rate_pct =
                    (row.pd_checktx_new_rejected_total as f64 / total_checktx as f64) * 100.0;
            }
            if row.submit_mode == "async" && row.pd_checktx_new_ok_total > 0 {
                let total_window_ms = (observation.steady_end_elapsed_ms
                    - observation.steady_start_elapsed_ms)
                    .max(0.0);
                if total_window_ms > 0.0 {
                    row.checktx_accepted_tps =
                        row.pd_checktx_new_ok_total as f64 / (total_window_ms / 1000.0);
                }
            }
        }
        if let Some(comet) = comet_metrics_after.as_ref() {
            row.comet_mempool_size_final = comet.mempool.size;
            row.comet_mempool_size_bytes_final = comet.mempool.size_bytes;
            row.comet_mempool_recheck_total_final = comet.mempool.recheck_total;
        }

        let mut invalid_reasons = Vec::new();
        if sender_output.corpus_exhausted {
            invalid_reasons.push("corpus_exhausted".to_string());
        }
        if row.steady_commits < cfg.stability.min_steady_commits {
            invalid_reasons.push(format!(
                "steady_commits_below_min({}<{})",
                row.steady_commits, cfg.stability.min_steady_commits
            ));
        }
        if invalid_reasons.is_empty() {
            row.run_status = "ok".to_string();
            row.invalid_reason.clear();
        } else {
            row.run_status = "invalid".to_string();
            row.invalid_reason = invalid_reasons.join(";");
            row.stability = "fail".to_string();
            invalid_runs.push(format!("{}: {}", row.run_id, row.invalid_reason));
        }

        report::write_case_artifacts(
            &run_id,
            &cfg,
            &row,
            &blocks,
            &sender_output.submissions,
            &latencies,
            Some(&RunDiagnostics {
                submission_rollup: Some(submission_rollup),
                pd_metrics_before: metrics_before.as_ref().map(|snapshot| snapshot.raw.clone()),
                pd_metrics_after: metrics_after.as_ref().map(|snapshot| snapshot.raw.clone()),
                comet_metrics_after: comet_metrics_after
                    .as_ref()
                    .map(|snapshot| snapshot.raw.clone()),
            }),
        )
        .with_context(|| format!("failed writing run artifacts for {}", run_id))?;
    }

    let runs_dir = report::runs_root();
    let tps_csv = report::tps_root().join("tps.csv");
    report::summarize_runs(&runs_dir, &tps_csv)?;
    eprintln!("Updated {}", tps_csv.display());
    eprintln!(
        "Updated {}",
        tps_csv.with_file_name("profiles.csv").display()
    );

    if !invalid_runs.is_empty() {
        anyhow::bail!(
            "{} invalid run(s): {}",
            invalid_runs.len(),
            invalid_runs.join(" | ")
        );
    }

    Ok(())
}

async fn run_corpus(command: CorpusCommand) -> Result<()> {
    match command {
        CorpusCommand::WriteGenesisAllocations {
            wallet_home,
            count,
            denom,
            amount,
            out,
        } => {
            regulated_local::write_genesis_allocations(
                regulated_local::WriteGenesisAllocationsArgs {
                    wallet_home,
                    count,
                    denom,
                    amount,
                    out,
                },
            )?;
        }
        CorpusCommand::CountLocalSources {
            wallet_home,
            asset,
            no_sync,
        } => {
            regulated_local::count_local_sources(regulated_local::CountLocalSourcesArgs {
                wallet_home,
                asset,
                sync: !no_sync,
            })
            .await?;
        }
        CorpusCommand::BuildLocal {
            scenario,
            wallet_home,
            asset,
            count,
            source_start,
            to_address,
            out,
            observer,
            fee_tier,
            asset_kind,
            source_label,
            chain_id,
            genesis_hash,
            notes,
            concurrency,
            no_sync,
        } => {
            regulated_local::build_local(BuildLocalArgs {
                scenario,
                wallet_home,
                asset,
                asset_kind,
                count,
                source_start,
                to_address,
                out,
                observer,
                fee_tier,
                source_label,
                chain_id,
                genesis_hash,
                notes,
                concurrency,
                sync: !no_sync,
            })
            .await?;
        }
        CorpusCommand::Merge {
            inputs,
            out,
            source_label,
            notes,
        } => {
            corpus::merge_corpora(&inputs, &out, &source_label, &notes)?;
        }
        CorpusCommand::Verify {
            corpus: dir,
            observer,
            endpoint_kind,
        } => {
            let endpoint_kind = parse_endpoint_kind(&endpoint_kind)?;
            let report = corpus::verify_corpus(&dir, &observer, &endpoint_kind).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        CorpusCommand::Pack {
            json_dir,
            out,
            scenario,
            asset_kind,
            source_label,
            chain_id,
            genesis_hash,
            notes,
        } => {
            let txs = corpus::load_transactions_from_json_dir(&json_dir)?;
            corpus::build_corpus_from_transactions(
                &out,
                &scenario,
                &source_label,
                &chain_id,
                &genesis_hash,
                &notes,
                &asset_kind,
                &txs,
            )?;
            println!(
                "Corpus written to {} ({} transactions)",
                out.display(),
                txs.len()
            );
        }
        CorpusCommand::Append {
            json_dir,
            corpus: dir,
            asset_kind,
            source_label,
            notes,
        } => {
            let txs = corpus::load_transactions_from_json_dir(&json_dir)?;
            let added = corpus::append_corpus_from_transactions(
                &dir,
                &asset_kind,
                &source_label,
                &notes,
                &txs,
            )?;
            println!(
                "Corpus appended at {} (+{} transactions)",
                dir.display(),
                added
            );
        }
    }
    Ok(())
}

fn default_build_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

#[derive(Clone, Debug, Default)]
struct PdMempoolMetrics {
    new_total: u64,
    new_ok_total: u64,
    new_rejected_total: u64,
    new_duration_sum_seconds: f64,
    new_duration_count: u64,
    pending: f64,
    in_flight: f64,
}

#[derive(Clone, Debug, Default)]
struct CometMempoolMetrics {
    size: f64,
    size_bytes: f64,
    recheck_total: f64,
}

#[derive(Clone, Debug, Default)]
struct PdMetricsSnapshot {
    raw: String,
    mempool: PdMempoolMetrics,
}

#[derive(Clone, Debug, Default)]
struct CometMetricsSnapshot {
    raw: String,
    mempool: CometMempoolMetrics,
}

async fn fetch_metrics_body(metrics_endpoint: &str) -> Result<String> {
    reqwest::get(metrics_endpoint)
        .await
        .with_context(|| format!("GET {}", metrics_endpoint))?
        .error_for_status()
        .with_context(|| format!("non-success response from {}", metrics_endpoint))?
        .text()
        .await
        .with_context(|| format!("reading metrics body from {}", metrics_endpoint))
}

async fn fetch_pd_metrics_snapshot(metrics_endpoint: &str) -> Result<PdMetricsSnapshot> {
    let raw = fetch_metrics_body(metrics_endpoint).await?;
    Ok(PdMetricsSnapshot {
        mempool: PdMempoolMetrics {
            new_total: sum_prometheus_metric_values(&raw, MEMPOOL_CHECKTX_TOTAL, &[("kind", "new")])
                as u64,
            new_ok_total: sum_prometheus_metric_values(
                &raw,
                MEMPOOL_CHECKTX_TOTAL,
                &[("kind", "new"), ("code", "0")],
            ) as u64,
            new_duration_sum_seconds: sum_prometheus_metric_values(
                &raw,
                &format!("{MEMPOOL_CHECKTX_DURATION}_sum_seconds"),
                &[("kind", "new")],
            ),
            new_duration_count: sum_prometheus_metric_values(
                &raw,
                &format!("{MEMPOOL_CHECKTX_DURATION}_count_seconds"),
                &[("kind", "new")],
            ) as u64,
            pending: parse_prometheus_gauge(&raw, MEMPOOL_CHECKTX_PENDING).unwrap_or(0.0),
            in_flight: parse_prometheus_gauge(&raw, MEMPOOL_CHECKTX_IN_FLIGHT).unwrap_or(0.0),
            new_rejected_total: 0,
        },
        raw,
    }
    .with_rejected_total())
}

async fn fetch_comet_metrics_snapshot(metrics_endpoint: &str) -> Result<CometMetricsSnapshot> {
    let raw = fetch_metrics_body(metrics_endpoint).await?;
    Ok(CometMetricsSnapshot {
        mempool: CometMempoolMetrics {
            size: sum_prometheus_metric_values(&raw, COMET_MEMPOOL_SIZE, &[]),
            size_bytes: sum_prometheus_metric_values(&raw, COMET_MEMPOOL_SIZE_BYTES, &[]),
            recheck_total: sum_prometheus_metric_values(&raw, COMET_MEMPOOL_RECHECK_TIMES, &[]),
        },
        raw,
    })
}

impl PdMetricsSnapshot {
    fn with_rejected_total(mut self) -> Self {
        self.mempool.new_rejected_total = self
            .mempool
            .new_total
            .saturating_sub(self.mempool.new_ok_total);
        self
    }
}

fn parse_prometheus_gauge(body: &str, metric_name: &str) -> Option<f64> {
    iter_prometheus_samples(body)
        .find(|sample| sample.name == metric_name && sample.labels.is_empty())
        .map(|sample| sample.value)
}

fn sum_prometheus_metric_values(body: &str, metric_name: &str, labels: &[(&str, &str)]) -> f64 {
    iter_prometheus_samples(body)
        .filter(|sample| sample.name == metric_name && labels_match(&sample.labels, labels))
        .map(|sample| sample.value)
        .sum()
}

fn labels_match(labels: &BTreeMap<String, String>, required: &[(&str, &str)]) -> bool {
    required
        .iter()
        .all(|(key, value)| labels.get(*key).is_some_and(|actual| actual == value))
}

#[derive(Clone, Debug)]
struct PrometheusSample {
    name: String,
    labels: BTreeMap<String, String>,
    value: f64,
}

fn iter_prometheus_samples(body: &str) -> impl Iterator<Item = PrometheusSample> + '_ {
    body.lines()
        .filter_map(|line| parse_prometheus_sample(line.trim()))
}

fn parse_prometheus_sample(line: &str) -> Option<PrometheusSample> {
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut parts = line.split_whitespace();
    let sample = parts.next()?;
    let value = parts.next()?.parse::<f64>().ok()?;
    let (name, labels) = parse_prometheus_metric_key(sample)?;
    Some(PrometheusSample {
        name,
        labels,
        value,
    })
}

fn parse_prometheus_metric_key(sample: &str) -> Option<(String, BTreeMap<String, String>)> {
    if let Some((name, rest)) = sample.split_once('{') {
        let labels = rest.strip_suffix('}')?;
        Some((name.to_string(), parse_prometheus_labels(labels)))
    } else {
        Some((sample.to_string(), BTreeMap::new()))
    }
}

fn parse_prometheus_labels(labels: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for label in labels.split(',') {
        if label.is_empty() {
            continue;
        }
        if let Some((key, value)) = label.split_once('=') {
            out.insert(key.to_string(), value.trim_matches('"').to_string());
        }
    }
    out
}

fn build_submission_rollup(submissions: &[sender::SubmissionRecord]) -> SubmissionRollup {
    let mut code_counts = BTreeMap::<u64, usize>::new();
    let mut log_counts = BTreeMap::<String, usize>::new();
    let mut accepted_submissions = 0usize;

    for submission in submissions {
        *code_counts.entry(submission.response_code).or_default() += 1;
        if submission.response_code == 0 {
            accepted_submissions += 1;
        }
        let normalized_log = submission.response_log.trim();
        if !normalized_log.is_empty() {
            *log_counts.entry(normalized_log.to_string()).or_default() += 1;
        }
    }

    let mut response_code_counts = code_counts
        .into_iter()
        .map(|(code, count)| ResponseCodeCount { code, count })
        .collect::<Vec<_>>();
    response_code_counts.sort_by(|a, b| a.code.cmp(&b.code));

    let mut top_response_logs = log_counts
        .into_iter()
        .map(|(log, count)| ResponseLogCount { log, count })
        .collect::<Vec<_>>();
    top_response_logs.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.log.cmp(&b.log)));
    top_response_logs.truncate(10);

    SubmissionRollup {
        total_submissions: submissions.len(),
        accepted_submissions,
        response_code_counts,
        top_response_logs,
    }
}

async fn run_report(command: ReportCommand) -> Result<()> {
    match command {
        ReportCommand::Summarize { input, output } => {
            report::summarize_runs(&input, &output)?;
            println!("Updated {}", output.display());
            println!(
                "Updated {}",
                output.with_file_name("profiles.csv").display()
            );
        }
    }
    Ok(())
}

fn make_run_id(scenario: &str, offered_tps: u64, repeat: u32) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "run-{}-{}-{}-r{}-p{}",
        ts,
        scenario,
        offered_tps,
        repeat,
        std::process::id()
    )
}

#[derive(Clone, Debug)]
struct CaseExecutionPlan {
    case: penumbra_sdk_poc_stage_bench::tps::config::RunCase,
    corpus_start: usize,
    corpus_len: usize,
    corpus_required: usize,
}

fn build_execution_plan(
    cfg: &TpsConfig,
    cases: Vec<penumbra_sdk_poc_stage_bench::tps::config::RunCase>,
    corpora: &std::collections::BTreeMap<String, corpus::Corpus>,
) -> Result<Vec<CaseExecutionPlan>> {
    let mut case_min_requirements = Vec::with_capacity(cases.len());
    let mut scenario_min_total = std::collections::BTreeMap::<String, usize>::new();
    for case in &cases {
        let required = required_corpus_txs(cfg, &case.scenario, case.offered_tps);
        case_min_requirements.push(required);
        *scenario_min_total
            .entry(case.scenario.name.clone())
            .or_default() += required;
    }

    for (scenario_name, required_total) in scenario_min_total {
        let available = corpora
            .get(&scenario_name)
            .ok_or_else(|| anyhow::anyhow!("missing corpus for scenario {}", scenario_name))?
            .entries
            .len();
        anyhow::ensure!(
            required_total <= available,
            "insufficient corpus for scenario {}: required_min_total={} available={}",
            scenario_name,
            required_total,
            available
        );
    }

    let mut cursors = std::collections::BTreeMap::<String, usize>::new();
    let mut remaining_cases = cases.iter().fold(
        std::collections::BTreeMap::<String, usize>::new(),
        |mut counts, case| {
            *counts.entry(case.scenario.name.clone()).or_default() += 1;
            counts
        },
    );
    let mut out = Vec::with_capacity(cases.len());

    for (case, min_required) in cases.into_iter().zip(case_min_requirements.into_iter()) {
        let scenario_name = case.scenario.name.clone();
        let available = corpora
            .get(&scenario_name)
            .ok_or_else(|| anyhow::anyhow!("missing corpus for scenario {}", scenario_name))?
            .entries
            .len();
        let cursor = cursors.get(&scenario_name).copied().unwrap_or(0);
        let cases_left = remaining_cases.get(&scenario_name).copied().unwrap_or(0);
        let is_last_case_for_scenario = cases_left == 1;

        let assigned = if is_last_case_for_scenario {
            available.saturating_sub(cursor)
        } else {
            min_required
        };
        let corpus_start = cursor;

        anyhow::ensure!(
            corpus_start + assigned <= available,
            "corpus assignment overflow for scenario {} (cursor={}, assigned={}, available={})",
            scenario_name,
            corpus_start,
            assigned,
            available
        );

        out.push(CaseExecutionPlan {
            case,
            corpus_start,
            corpus_len: assigned,
            corpus_required: min_required,
        });
        cursors.insert(scenario_name.clone(), cursor + assigned);
        let remaining = remaining_cases
            .get_mut(&scenario_name)
            .expect("case count should exist");
        *remaining = remaining.saturating_sub(1);
    }

    Ok(out)
}

fn required_corpus_txs(
    cfg: &TpsConfig,
    scenario: &penumbra_sdk_poc_stage_bench::tps::config::ScenarioConfig,
    offered_tps: u64,
) -> usize {
    // Required tx budget for one case:
    // offered_tps * ((warmup + steady) * target_block_time_s) * 2.0
    let window_blocks = scenario
        .warmup_blocks
        .saturating_add(scenario.steady_blocks) as u128;
    let target_block_time_ms = cfg.target_block_time_ms as u128;
    let offered_tps = offered_tps as u128;
    let numerator = offered_tps
        .saturating_mul(window_blocks)
        .saturating_mul(target_block_time_ms)
        .saturating_mul(20); // safety factor 2.0
    let denom = 10u128.saturating_mul(1000); // safety denominator + ms -> s
    let required = (numerator.saturating_add(denom - 1)) / denom;
    required.max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use penumbra_sdk_poc_stage_bench::tps::{
        config::{RunCase, ScenarioConfig, StabilityConfig, TpsProfile},
        sender::SubmitMode,
    };

    fn mk_cfg() -> TpsConfig {
        TpsConfig {
            label: "test".to_string(),
            pd_endpoints: vec!["http://127.0.0.1:8080".to_string()],
            observer_endpoint: "http://127.0.0.1:8080".to_string(),
            endpoint_kind: penumbra_sdk_poc_stage_bench::tps::config::EndpointKind::TendermintProxy,
            metrics_endpoint: None,
            comet_metrics_endpoint: None,
            profile: TpsProfile::Regression,
            target_block_time_ms: 1000,
            mempool_checktx_concurrency: None,
            scenarios: vec![],
            stability: StabilityConfig {
                max_reject_rate_pct: 1.0,
                max_p95_latency_ms: 1000.0,
                max_backlog_growth_pct: 10.0,
                min_steady_commits: 1,
            },
        }
    }

    fn mk_scenario(name: &str) -> ScenarioConfig {
        ScenarioConfig {
            name: name.to_string(),
            corpus_dir: std::path::PathBuf::from("."),
            offered_tps: vec![10],
            repeats: 1,
            repeat_start: 1,
            warmup_blocks: 1,
            steady_blocks: 5,
            submit_workers: 1,
            max_inflight: 10,
            submit_mode: SubmitMode::Async,
            pacer_tick_ms: 50,
            disable_pacer: false,
            burst_profile: None,
        }
    }

    fn mk_corpus(available: usize) -> corpus::Corpus {
        let entries = (0..available)
            .map(|i| corpus::CorpusEntry {
                ordinal: i,
                tx_hash_hex: format!("{i:064x}"),
                offset: 0,
                length: 1,
                asset_kind: "test".to_string(),
                tx_bytes: vec![0],
            })
            .collect();
        corpus::Corpus {
            manifest: corpus::Manifest {
                chain_id: "unknown".to_string(),
                genesis_hash: "unknown".to_string(),
                scenario: "test".to_string(),
                tx_count: available,
                created_at: 0,
                source_label: "test".to_string(),
                notes: String::new(),
                ..corpus::Manifest::default()
            },
            entries,
        }
    }

    fn assert_no_overlap(plan: &[CaseExecutionPlan]) {
        let mut assigned = std::collections::HashSet::new();
        for case in plan {
            for ordinal in case.corpus_start..(case.corpus_start + case.corpus_len) {
                assert!(
                    assigned.insert(ordinal),
                    "ordinal {ordinal} assigned to more than one case"
                );
            }
        }
    }

    #[test]
    fn required_corpus_uses_two_x_safety_factor() {
        let cfg = mk_cfg();
        let scenario = mk_scenario("unregulated");
        // 10 tps * 6 blocks * 1s * 2.0 = 120 tx
        assert_eq!(required_corpus_txs(&cfg, &scenario, 10), 120);
    }

    #[test]
    fn plan_assigns_remaining_to_last_case() {
        let cfg = mk_cfg();
        let scenario = mk_scenario("unregulated");
        let cases = vec![
            RunCase {
                scenario: scenario.clone(),
                offered_tps: 10,
                repeat: 1,
            },
            RunCase {
                scenario,
                offered_tps: 10,
                repeat: 2,
            },
        ];

        let mut corpora = std::collections::BTreeMap::new();
        corpora.insert("unregulated".to_string(), mk_corpus(300));

        let plan = build_execution_plan(&cfg, cases, &corpora).expect("plan builds");
        assert_eq!(plan.len(), 2);
        // First case gets required min (120), second gets remainder (180).
        assert_eq!(plan[0].corpus_len, 120);
        assert_eq!(plan[1].corpus_len, 180);
        assert_eq!(plan[0].corpus_start, 0);
        assert_eq!(plan[1].corpus_start, 120);
        assert_no_overlap(&plan);
    }

    #[test]
    fn plan_rejects_insufficient_total_corpus() {
        let cfg = mk_cfg();
        let scenario = mk_scenario("unregulated");
        let cases = vec![
            RunCase {
                scenario: scenario.clone(),
                offered_tps: 10,
                repeat: 1,
            },
            RunCase {
                scenario,
                offered_tps: 10,
                repeat: 2,
            },
        ];

        let mut corpora = std::collections::BTreeMap::new();
        corpora.insert("unregulated".to_string(), mk_corpus(200));

        let err = build_execution_plan(&cfg, cases, &corpora).expect_err("plan should fail");
        assert!(err.to_string().contains("insufficient corpus"));
    }

    #[test]
    fn single_case_gets_full_remaining_window() {
        let cfg = mk_cfg();
        let scenario = mk_scenario("unregulated");
        let cases = vec![RunCase {
            scenario,
            offered_tps: 10,
            repeat: 1,
        }];

        let mut corpora = std::collections::BTreeMap::new();
        corpora.insert("unregulated".to_string(), mk_corpus(300));

        let plan = build_execution_plan(&cfg, cases, &corpora).expect("plan builds");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].corpus_start, 0);
        assert_eq!(plan[0].corpus_len, 300);
    }
}
