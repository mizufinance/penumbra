//! Client flow benchmark: end-to-end transaction building.
//!
//! Emits:
//! - top-level overview: `benches/compliance/flows.csv`
//! - category KPIs: `benches/compliance/client/client.csv`
//! - section overview: `benches/compliance/client/sections.csv`
//! - section KPIs: `benches/compliance/client/sections/<section>.csv`

use std::time::Instant;

use penumbra_sdk_bench_support::bench_runner;
use penumbra_sdk_keys::test_keys;
use rand_core::OsRng;

#[path = "../helpers/flow.rs"]
mod flow_helpers;
use flow_helpers::*;

fn timed_total(warmup: usize, samples: usize, mut f: impl FnMut()) -> Vec<f64> {
    for _ in 0..warmup {
        f();
    }
    let mut out = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        f();
        out.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    out
}

fn pick_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    scenario: &str,
    stage: &str,
    mode: &str,
) -> bench_runner::BenchResult {
    raw.iter()
        .find(|r| {
            r.version == version
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "scenario" && v == scenario)
                && r.dimensions.iter().any(|(k, v)| k == "stage" && v == stage)
                && r.dimensions.iter().any(|(k, v)| k == "mode" && v == mode)
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "missing KPI row for version={version}, scenario={scenario}, stage={stage}, mode={mode}"
            )
        })
}

fn flow_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    let kpis = if regression {
        vec![("1S1O", "total", "concurrent"), ("1S1O", "prove", "serial")]
    } else {
        vec![
            ("1S1O", "total", "serial"),
            ("1S1O", "total", "concurrent"),
            ("1S1O", "prove", "serial"),
            ("4S1O", "total", "serial"),
        ]
    };
    for (scenario, stage, mode) in kpis {
        rows.push(pick_kpi(raw, version, scenario, stage, mode));
    }
    rows
}

fn section_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    let kpis = if regression {
        vec![
            ("1S1O", "enrich", ""),
            ("1S1O", "authorize", ""),
            ("1S1O", "tx_build", "concurrent"),
        ]
    } else {
        vec![
            ("1S1O", "enrich", ""),
            ("1S1O", "authorize", ""),
            ("1S1O", "tx_build", "serial"),
            ("1S1O", "tx_build", "concurrent"),
        ]
    };
    for (scenario, stage, mode) in kpis {
        rows.push(pick_kpi(raw, version, scenario, stage, mode));
    }
    rows
}

fn push_version_rows(
    out: &mut Vec<bench_runner::BenchResult>,
    version: &str,
    warmup: usize,
    samples: usize,
    include_concurrent: bool,
    include_multi: bool,
    rt: &tokio::runtime::Runtime,
) {
    let fvk = &test_keys::FULL_VIEWING_KEY;
    let sk = &test_keys::SPEND_KEY;

    // 1S1O enrich
    let enrich_times = bench_runner::run_bench(warmup, samples, || {
        let (mut spend, mut output, _sct) = build_simple_transfer_unenriched();
        enrich_spend_for_test(&mut OsRng, &mut spend, &test_keys::ADDRESS_0);
        enrich_output_for_test(
            &mut OsRng,
            &mut output,
            &test_keys::ADDRESS_0,
            spend.note.asset_id(),
        );
    });
    out.push(bench_runner::make_result(
        version,
        &[("scenario", "1S1O"), ("stage", "enrich"), ("mode", "")],
        &enrich_times,
        None,
    ));

    // 1S1O authorize
    let prepared_auth = build_simple_transfer();
    let authorize_times = bench_runner::run_bench(warmup, samples, || {
        let _ = prepared_auth
            .plan
            .clone()
            .authorize(OsRng, sk)
            .expect("authorize");
    });
    out.push(bench_runner::make_result(
        version,
        &[("scenario", "1S1O"), ("stage", "authorize"), ("mode", "")],
        &authorize_times,
        None,
    ));

    // 1S1O build serial / prove serial
    let prepared = build_simple_transfer();
    let auth_data = prepared.plan.authorize(OsRng, sk).expect("authorize");
    let build_serial = bench_runner::run_bench(warmup, samples, || {
        let _ = prepared
            .plan
            .clone()
            .build(fvk, &prepared.witness_data, &auth_data)
            .expect("build serial");
    });
    out.push(bench_runner::make_result(
        version,
        &[
            ("scenario", "1S1O"),
            ("stage", "tx_build"),
            ("mode", "serial"),
        ],
        &build_serial,
        None,
    ));
    out.push(bench_runner::make_result(
        version,
        &[("scenario", "1S1O"), ("stage", "prove"), ("mode", "serial")],
        &build_serial,
        None,
    ));

    if include_concurrent {
        // 1S1O build concurrent
        let prepared = build_simple_transfer();
        let auth_data = prepared.plan.authorize(OsRng, sk).expect("authorize");
        let times = bench_runner::run_bench(warmup, samples, || {
            rt.block_on(async {
                let _ = prepared
                    .plan
                    .clone()
                    .build_concurrent(fvk, &prepared.witness_data, &auth_data)
                    .await
                    .expect("build concurrent");
            });
        });
        out.push(bench_runner::make_result(
            version,
            &[
                ("scenario", "1S1O"),
                ("stage", "tx_build"),
                ("mode", "concurrent"),
            ],
            &times,
            None,
        ));
    }

    // 1S1O total serial / concurrent
    let total_serial = timed_total(warmup, samples, || {
        let p = build_simple_transfer();
        let ad = p.plan.authorize(OsRng, sk).expect("authorize");
        let _ = p
            .plan
            .build(fvk, &p.witness_data, &ad)
            .expect("build serial");
    });
    out.push(bench_runner::make_result(
        version,
        &[("scenario", "1S1O"), ("stage", "total"), ("mode", "serial")],
        &total_serial,
        None,
    ));

    if include_concurrent {
        let total_concurrent = timed_total(warmup, samples, || {
            let p = build_simple_transfer();
            let ad = p.plan.authorize(OsRng, sk).expect("authorize");
            rt.block_on(async {
                let _ = p
                    .plan
                    .build_concurrent(fvk, &p.witness_data, &ad)
                    .await
                    .expect("build concurrent");
            });
        });
        out.push(bench_runner::make_result(
            version,
            &[
                ("scenario", "1S1O"),
                ("stage", "total"),
                ("mode", "concurrent"),
            ],
            &total_concurrent,
            None,
        ));
    }

    if include_multi {
        let total_multi = timed_total(warmup, samples, || {
            let p = build_multi_spend_4x1();
            let ad = p.plan.authorize(OsRng, sk).expect("authorize");
            let _ = p
                .plan
                .build(fvk, &p.witness_data, &ad)
                .expect("build serial");
        });
        out.push(bench_runner::make_result(
            version,
            &[("scenario", "4S1O"), ("stage", "total"), ("mode", "serial")],
            &total_multi,
            None,
        ));
    }
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("can create tokio runtime");
    let version = bench_runner::bench_version();
    let warmup = bench_runner::warmup_count();
    let samples = bench_runner::sample_count();
    let regression = bench_runner::is_regression_suite();
    let quick = bench_runner::is_quick_profile();
    let include_multi = !quick && !regression;
    let include_concurrent = true;

    let mut raw_results = Vec::new();
    push_version_rows(
        &mut raw_results,
        &version,
        warmup,
        samples,
        include_concurrent,
        include_multi,
        &rt,
    );

    let flow_rows = flow_rows_for_version(&raw_results, &version, regression);
    let section_rows = section_rows_for_version(&raw_results, &version, regression);
    let mut flow_with_meta = flow_rows.clone();
    bench_runner::annotate_raw_results(&mut flow_with_meta);
    let mut sections_with_meta = section_rows.clone();
    bench_runner::annotate_raw_results(&mut sections_with_meta);
    bench_runner::output_results(&flow_with_meta);

    let flow_path = bench_runner::category_csv_path("client");
    bench_runner::append_csv(&flow_path, &flow_with_meta);

    let sections_overview_path = bench_runner::category_sections_csv_path("client");
    bench_runner::append_csv(&sections_overview_path, &sections_with_meta);

    let flows_overview = bench_runner::to_flow_overview_rows("client", &flow_with_meta);
    let flows_overview_path = bench_runner::flows_overview_csv_path();
    bench_runner::append_csv_scoped(&flows_overview_path, &flows_overview, &["category", "kpi"]);

    for section in ["enrich", "authorize", "tx_build"] {
        let mut rows: Vec<_> = sections_with_meta
            .iter()
            .filter(|r| {
                r.dimensions
                    .iter()
                    .any(|(k, v)| k == "stage" && v == section)
            })
            .cloned()
            .collect();
        for r in &mut rows {
            r.dimensions
                .retain(|(k, v)| k != "stage" && !(k == "mode" && v.is_empty()));
        }
        let section_path = bench_runner::section_csv_path("client", section);
        bench_runner::append_csv(&section_path, &rows);
    }
}
