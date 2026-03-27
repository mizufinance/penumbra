use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::tps::aggregate::{LatencyRecord, SummaryRow};
use crate::tps::config::TpsConfig;
use crate::tps::observer::BlockRecord;
use crate::tps::sender::SubmissionRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSummary {
    pub row: SummaryRow,
    pub markdown_summary: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SubmissionRollup {
    pub total_submissions: usize,
    pub accepted_submissions: usize,
    pub response_code_counts: Vec<ResponseCodeCount>,
    pub top_response_logs: Vec<ResponseLogCount>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResponseCodeCount {
    pub code: u64,
    pub count: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResponseLogCount {
    pub log: String,
    pub count: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunDiagnostics {
    pub submission_rollup: Option<SubmissionRollup>,
    pub pd_metrics_before: Option<String>,
    pub pd_metrics_after: Option<String>,
    pub comet_metrics_after: Option<String>,
}

pub fn tps_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/compliance/tps")
}

pub fn runs_root() -> PathBuf {
    tps_root().join("runs")
}

pub fn write_case_artifacts(
    run_id: &str,
    cfg: &TpsConfig,
    row: &SummaryRow,
    blocks: &[BlockRecord],
    submissions: &[SubmissionRecord],
    latencies: &[LatencyRecord],
    diagnostics: Option<&RunDiagnostics>,
) -> Result<PathBuf> {
    let run_dir = runs_root().join(run_id);
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create {}", run_dir.display()))?;

    write_blocks_csv(&run_dir.join("blocks.csv"), blocks)?;
    write_submissions_csv(&run_dir.join("submissions.csv"), submissions)?;
    write_latency_csv(&run_dir.join("latency.csv"), latencies)?;
    let config_yaml = serde_yaml::to_string(cfg)
        .with_context(|| format!("failed to serialize config for {}", run_id))?;
    std::fs::write(run_dir.join("config.snapshot.yaml"), config_yaml)
        .with_context(|| format!("failed to write config snapshot for {}", run_id))?;

    let summary = RunSummary {
        row: row.clone(),
        markdown_summary: markdown_for_row(row),
    };
    let summary_json = serde_json::to_vec_pretty(&summary)
        .with_context(|| format!("failed to serialize summary for {}", run_id))?;
    std::fs::write(run_dir.join("summary.json"), summary_json)
        .with_context(|| format!("failed to write summary for {}", run_id))?;

    if let Some(diagnostics) = diagnostics {
        if let Some(rollup) = &diagnostics.submission_rollup {
            write_json(&run_dir.join("submission_rollup.json"), rollup)
                .with_context(|| format!("failed to write submission rollup for {}", run_id))?;
        }
        if let Some(body) = &diagnostics.pd_metrics_before {
            std::fs::write(run_dir.join("pd-metrics-before.txt"), body)
                .with_context(|| format!("failed to write pd metrics before for {}", run_id))?;
        }
        if let Some(body) = &diagnostics.pd_metrics_after {
            std::fs::write(run_dir.join("pd-metrics-after.txt"), body)
                .with_context(|| format!("failed to write pd metrics after for {}", run_id))?;
        }
        if let Some(body) = &diagnostics.comet_metrics_after {
            std::fs::write(run_dir.join("comet-metrics-after.txt"), body)
                .with_context(|| format!("failed to write comet metrics after for {}", run_id))?;
        }
    }

    Ok(run_dir)
}

pub fn summarize_runs(input_runs_dir: &Path, output_tps_csv: &Path) -> Result<()> {
    let mut summaries = read_all_summaries(input_runs_dir)?;
    summaries.sort_by(summary_order);

    if let Some(parent) = output_tps_csv.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_tps_csv(output_tps_csv, &summaries)?;

    let profiles_csv = output_tps_csv.with_file_name("profiles.csv");
    write_profiles_csv(&profiles_csv, &summaries)?;
    Ok(())
}

fn write_blocks_csv(path: &Path, rows: &[BlockRecord]) -> Result<()> {
    let mut rows = rows.to_vec();
    rows.sort_by_key(|r| r.height);
    let mut wtr = csv::Writer::from_path(path)?;
    for row in rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_submissions_csv(path: &Path, rows: &[SubmissionRecord]) -> Result<()> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| a.seq.cmp(&b.seq));
    let mut wtr = csv::Writer::from_path(path)?;
    for row in rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_latency_csv(path: &Path, rows: &[LatencyRecord]) -> Result<()> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| {
        a.commit_height
            .cmp(&b.commit_height)
            .then_with(|| a.tx_hash_hex.cmp(&b.tx_hash_hex))
    });
    let mut wtr = csv::Writer::from_path(path)?;
    for row in rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_tps_csv(path: &Path, summaries: &[RunSummary]) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    for summary in summaries {
        wtr.serialize(&summary.row)?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct ProfileRow {
    label: String,
    scenario: String,
    submit_mode: String,
    offered_tps: u64,
    repeats: usize,
    sent_tps_mean: f64,
    accepted_tps_mean: f64,
    broadcast_accepted_tps_mean: f64,
    checktx_accepted_tps_mean: f64,
    committed_tps_mean: f64,
    reject_rate_pct_mean: f64,
    checktx_reject_rate_pct_mean: f64,
    p95_ms_mean: f64,
    pass_rate_pct: f64,
}

fn write_profiles_csv(path: &Path, summaries: &[RunSummary]) -> Result<()> {
    let mut buckets: BTreeMap<(String, String, String, u64), Vec<&RunSummary>> = BTreeMap::new();
    for summary in summaries {
        let key = (
            summary.row.label.clone(),
            summary.row.scenario.clone(),
            summary.row.submit_mode.clone(),
            summary.row.offered_tps,
        );
        buckets.entry(key).or_default().push(summary);
    }

    let mut rows = Vec::with_capacity(buckets.len());
    for ((label, scenario, submit_mode, offered_tps), bucket) in buckets {
        let repeats = bucket.len();
        let sent_tps_mean = avg(bucket.iter().map(|s| s.row.sent_tps));
        let accepted_tps_mean = avg(bucket.iter().map(|s| s.row.accepted_tps));
        let broadcast_accepted_tps_mean = avg(bucket.iter().map(|s| s.row.broadcast_accepted_tps));
        let checktx_accepted_tps_mean = avg(bucket.iter().map(|s| s.row.checktx_accepted_tps));
        let committed_tps_mean = avg(bucket.iter().map(|s| s.row.committed_tps));
        let reject_rate_pct_mean = avg(bucket.iter().map(|s| s.row.reject_rate_pct));
        let checktx_reject_rate_pct_mean =
            avg(bucket.iter().map(|s| s.row.checktx_reject_rate_pct));
        let p95_ms_mean = avg(bucket.iter().map(|s| s.row.p95_ms));
        let pass_count = bucket.iter().filter(|s| s.row.stability == "pass").count();
        let pass_rate_pct = if repeats == 0 {
            0.0
        } else {
            (pass_count as f64 / repeats as f64) * 100.0
        };

        rows.push(ProfileRow {
            label,
            scenario,
            submit_mode,
            offered_tps,
            repeats,
            sent_tps_mean,
            accepted_tps_mean,
            broadcast_accepted_tps_mean,
            checktx_accepted_tps_mean,
            committed_tps_mean,
            reject_rate_pct_mean,
            checktx_reject_rate_pct_mean,
            p95_ms_mean,
            pass_rate_pct,
        });
    }

    let mut wtr = csv::Writer::from_path(path)?;
    for row in rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

fn read_all_summaries(runs_dir: &Path) -> Result<Vec<RunSummary>> {
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(runs_dir)
        .with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        let summary_path = path.join("summary.json");
        if !summary_path.exists() {
            continue;
        }
        let summary: RunSummary = serde_json::from_slice(
            &std::fs::read(&summary_path)
                .with_context(|| format!("failed to read {}", summary_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", summary_path.display()))?;
        out.push(summary);
    }
    Ok(out)
}

fn summary_order(a: &RunSummary, b: &RunSummary) -> std::cmp::Ordering {
    a.row
        .scenario
        .cmp(&b.row.scenario)
        .then_with(|| a.row.offered_tps.cmp(&b.row.offered_tps))
        .then_with(|| a.row.repeat.cmp(&b.row.repeat))
        .then_with(|| a.row.run_id.cmp(&b.row.run_id))
}

fn avg(it: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<f64> = it.collect();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn markdown_for_row(row: &SummaryRow) -> String {
    format!(
        "# TPS Run Summary\n\n- run_id: `{}`\n- label: `{}`\n- scenario: `{}`\n- corpus_digest: `{}`\n- offered_tps: `{}`\n- repeat: `{}`\n- committed_tps: `{:.2}`\n- steady_commits: `{}`\n- p95_ms: `{:.2}`\n- reject_rate_pct: `{:.2}`\n- stability: `{}`\n- run_status: `{}`\n- invalid_reason: `{}`\n",
        row.run_id,
        row.label,
        row.scenario,
        row.corpus_digest,
        row.offered_tps,
        row.repeat,
        row.committed_tps,
        row.steady_commits,
        row.p95_ms,
        row.reject_rate_pct,
        row.stability,
        row.run_status,
        if row.invalid_reason.is_empty() {
            "none".to_string()
        } else {
            row.invalid_reason.clone()
        }
    )
}
