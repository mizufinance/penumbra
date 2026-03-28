//! Local full-node correlation benchmark.
//!
//! This bench reuses the existing single-node `pd + CometBFT` TPS workflow and
//! emits benchmark-style CSVs beside the synthetic `pre_consensus` results so
//! local runtime behavior can be compared against the in-process hotspot model.

use anyhow::Result;
use penumbra_sdk_bench_support::bench_runner;
use penumbra_sdk_poc_stage_bench::tps::local_fullnode::{LocalFullnodeRun, SyntheticReference};

const FLOW_METRICS: &[&str] = &[
    "accepted_tps",
    "committed_tps",
    "block_time_mean_ms",
    "local_vs_synthetic_ratio",
];
const SECTION_NAMES: &[&str] = &["runtime", "latency", "correlation"];

fn main() {
    if let Err(error) = run() {
        eprintln!("local_fullnode_flow failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let version = bench_runner::bench_version();
    let run = LocalFullnodeRun::from_env(&version)?;
    let mut flow_rows = flow_rows_for_run(&version, &run);
    let mut section_rows = section_rows_for_run(&version, &run);

    bench_runner::annotate_raw_results(&mut flow_rows);
    bench_runner::annotate_raw_results(&mut section_rows);
    bench_runner::output_results(&flow_rows);

    let category_path = bench_runner::category_csv_path("local_fullnode");
    bench_runner::append_csv(&category_path, &flow_rows);

    let sections_path = bench_runner::category_sections_csv_path("local_fullnode");
    bench_runner::append_csv(&sections_path, &section_rows);

    let flows_overview = bench_runner::to_flow_overview_rows("local_fullnode", &flow_rows);
    let flows_path = bench_runner::flows_overview_csv_path();
    bench_runner::append_csv_scoped(&flows_path, &flows_overview, &["category", "kpi"]);

    for section in SECTION_NAMES {
        let mut rows: Vec<_> = section_rows
            .iter()
            .filter(|row| {
                row.dimensions
                    .iter()
                    .any(|(key, value)| key == "section" && value == *section)
            })
            .cloned()
            .collect();
        for row in &mut rows {
            row.dimensions.retain(|(key, _)| key != "section");
        }
        let path = bench_runner::section_csv_path("local_fullnode", section);
        bench_runner::append_csv(&path, &rows);
    }

    Ok(())
}

fn flow_rows_for_run(version: &str, run: &LocalFullnodeRun) -> Vec<bench_runner::BenchResult> {
    run.rows
        .iter()
        .flat_map(|row| {
            FLOW_METRICS
                .iter()
                .map(move |metric| make_metric_row(version, run, row, metric))
        })
        .collect()
}

fn section_rows_for_run(version: &str, run: &LocalFullnodeRun) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    for row in &run.rows {
        for metric in [
            "sent_tps",
            "accepted_tps",
            "committed_tps",
            "reject_rate_pct",
            "steady_commits",
            "block_time_mean_ms",
            "backlog_growth_pct",
        ] {
            rows.push(make_section_metric_row(
                version, run, row, "runtime", metric,
            ));
        }
        for metric in ["p50_ms", "p95_ms", "p99_ms"] {
            rows.push(make_section_metric_row(
                version, run, row, "latency", metric,
            ));
        }
        for metric in ["synthetic_reference_tps", "local_vs_synthetic_ratio"] {
            rows.push(make_section_metric_row(
                version,
                run,
                row,
                "correlation",
                metric,
            ));
        }
    }
    rows
}

fn make_metric_row(
    version: &str,
    run: &LocalFullnodeRun,
    row: &penumbra_sdk_poc_stage_bench::tps::aggregate::SummaryRow,
    metric: &str,
) -> bench_runner::BenchResult {
    let mean = metric_value(row, &run.synthetic_reference, metric);
    bench_runner::BenchResult {
        version: version.to_string(),
        dimensions: common_dims(run, row, metric),
        mean_ms: mean,
        median_ms: mean,
        samples: 1,
        metrics: None,
    }
}

fn make_section_metric_row(
    version: &str,
    run: &LocalFullnodeRun,
    row: &penumbra_sdk_poc_stage_bench::tps::aggregate::SummaryRow,
    section: &str,
    metric: &str,
) -> bench_runner::BenchResult {
    let mut dimensions = common_dims(run, row, metric);
    dimensions.push(("section".to_string(), section.to_string()));
    dimensions.push(("run_status".to_string(), row.run_status.clone()));
    dimensions.push(("stability".to_string(), row.stability.clone()));
    dimensions.push((
        "synthetic_run_id".to_string(),
        run.synthetic_reference.run_id.clone(),
    ));
    dimensions.push((
        "synthetic_tx_count".to_string(),
        run.synthetic_reference.tx_count.to_string(),
    ));
    dimensions.push((
        "synthetic_mode".to_string(),
        run.synthetic_reference.mode.clone(),
    ));
    dimensions.push((
        "synthetic_concurrency".to_string(),
        run.synthetic_reference.concurrency.to_string(),
    ));
    dimensions.push((
        "synthetic_indexing_mode".to_string(),
        run.synthetic_reference.indexing_mode.clone(),
    ));
    bench_runner::BenchResult {
        version: version.to_string(),
        dimensions,
        mean_ms: metric_value(row, &run.synthetic_reference, metric),
        median_ms: metric_value(row, &run.synthetic_reference, metric),
        samples: 1,
        metrics: None,
    }
}

fn common_dims(
    run: &LocalFullnodeRun,
    row: &penumbra_sdk_poc_stage_bench::tps::aggregate::SummaryRow,
    metric: &str,
) -> Vec<(String, String)> {
    vec![
        ("engine".to_string(), "comet_local".to_string()),
        ("driver".to_string(), "bench_simple".to_string()),
        ("scenario".to_string(), row.scenario.clone()),
        ("offered_tps".to_string(), row.offered_tps.to_string()),
        ("repeat".to_string(), row.repeat.to_string()),
        ("metric".to_string(), metric.to_string()),
        ("runtime_label".to_string(), run.config.run_label.clone()),
    ]
}

fn metric_value(
    row: &penumbra_sdk_poc_stage_bench::tps::aggregate::SummaryRow,
    synthetic_reference: &SyntheticReference,
    metric: &str,
) -> f64 {
    match metric {
        "sent_tps" => row.sent_tps,
        "accepted_tps" => row.accepted_tps,
        "committed_tps" => row.committed_tps,
        "reject_rate_pct" => row.reject_rate_pct,
        "p50_ms" => row.p50_ms,
        "p95_ms" => row.p95_ms,
        "p99_ms" => row.p99_ms,
        "steady_commits" => row.steady_commits as f64,
        "block_time_mean_ms" => row.block_time_mean_ms,
        "backlog_growth_pct" => row.backlog_growth_pct,
        "synthetic_reference_tps" => synthetic_reference.mean_ms,
        "local_vs_synthetic_ratio" => {
            if synthetic_reference.mean_ms <= 0.0 {
                0.0
            } else {
                row.accepted_tps / synthetic_reference.mean_ms
            }
        }
        other => panic!("unsupported local full-node metric: {other}"),
    }
}
