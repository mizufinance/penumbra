use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use penumbra_sdk_bench::tps::{
    aggregate,
    config::TpsConfig,
    corpus, observer, report,
    sender::{self, SenderConfig},
};

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
    /// Validate corpus integrity and observer compatibility.
    Verify {
        #[clap(long)]
        corpus: PathBuf,
        #[clap(long)]
        observer: String,
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

async fn run_config(path: &PathBuf) -> Result<()> {
    let cfg = TpsConfig::load(path)?;
    let cases = cfg.expand_cases();
    anyhow::ensure!(!cases.is_empty(), "config produced zero run cases");

    let mut corpora_by_scenario = std::collections::BTreeMap::new();
    for scenario in &cfg.scenarios {
        if corpora_by_scenario.contains_key(&scenario.name) {
            continue;
        }
        let verify_report = corpus::verify_corpus(&scenario.corpus_dir, &cfg.observer_endpoint)
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
        let (height_tx, height_rx) = tokio::sync::watch::channel(plan.start_height);
        let observer_endpoint = cfg.observer_endpoint.clone();
        let plan_for_observer = plan.clone();
        let observer_task = tokio::spawn(async move {
            observer::observe_until_end(&observer_endpoint, plan_for_observer, t0, height_tx).await
        });

        let sender_output = sender::run_sender(
            &cfg.pd_endpoints,
            &corpus,
            SenderConfig {
                offered_tps: case.offered_tps,
                submit_workers: case.scenario.submit_workers,
                max_inflight: case.scenario.max_inflight,
                end_height: plan.end_height,
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
        row.corpus_required = plan_case.corpus_required as u64;
        row.corpus_assigned = plan_case.corpus_len as u64;
        row.corpus_exhausted = sender_output.corpus_exhausted;

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
        CorpusCommand::Verify {
            corpus: dir,
            observer,
        } => {
            let report = corpus::verify_corpus(&dir, &observer).await?;
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
    case: penumbra_sdk_bench::tps::config::RunCase,
    corpus_start: usize,
    corpus_len: usize,
    corpus_required: usize,
}

fn build_execution_plan(
    cfg: &TpsConfig,
    cases: Vec<penumbra_sdk_bench::tps::config::RunCase>,
    corpora: &std::collections::BTreeMap<String, corpus::Corpus>,
) -> Result<Vec<CaseExecutionPlan>> {
    let mut scenario_case_count = std::collections::BTreeMap::<String, usize>::new();
    for case in &cases {
        *scenario_case_count
            .entry(case.scenario.name.clone())
            .or_default() += 1;
    }

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
    let mut scenario_seen = std::collections::BTreeMap::<String, usize>::new();
    let mut out = Vec::with_capacity(cases.len());

    for (case, min_required) in cases.into_iter().zip(case_min_requirements.into_iter()) {
        let scenario_name = case.scenario.name.clone();
        let available = corpora
            .get(&scenario_name)
            .ok_or_else(|| anyhow::anyhow!("missing corpus for scenario {}", scenario_name))?
            .entries
            .len();
        let cursor = cursors.get(&scenario_name).copied().unwrap_or(0);
        let seen = scenario_seen.get(&scenario_name).copied().unwrap_or(0);
        let total_for_scenario = scenario_case_count
            .get(&scenario_name)
            .copied()
            .unwrap_or(0);
        let is_last_case_for_scenario = seen + 1 == total_for_scenario;

        let assigned = if is_last_case_for_scenario {
            available.saturating_sub(cursor)
        } else {
            min_required
        };

        anyhow::ensure!(
            cursor + assigned <= available,
            "corpus assignment overflow for scenario {} (cursor={}, assigned={}, available={})",
            scenario_name,
            cursor,
            assigned,
            available
        );

        out.push(CaseExecutionPlan {
            case,
            corpus_start: cursor,
            corpus_len: assigned,
            corpus_required: min_required,
        });
        cursors.insert(scenario_name.clone(), cursor + assigned);
        scenario_seen.insert(scenario_name, seen + 1);
    }

    Ok(out)
}

fn required_corpus_txs(
    cfg: &TpsConfig,
    scenario: &penumbra_sdk_bench::tps::config::ScenarioConfig,
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
    use penumbra_sdk_bench::tps::config::{RunCase, ScenarioConfig, StabilityConfig, TpsProfile};

    fn mk_cfg() -> TpsConfig {
        TpsConfig {
            label: "test".to_string(),
            pd_endpoints: vec!["http://127.0.0.1:8080".to_string()],
            observer_endpoint: "http://127.0.0.1:8080".to_string(),
            profile: TpsProfile::Regression,
            target_block_time_ms: 1000,
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
            warmup_blocks: 1,
            steady_blocks: 5,
            submit_workers: 1,
            max_inflight: 10,
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
            },
            entries,
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
    }
}
