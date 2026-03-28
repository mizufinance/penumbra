//! Validator flow benchmark: proof-family extraction and validator-side verification.
//!
//! Measures the validator-side proof path for a batch of spend/output transactions
//! under legacy batch verification and SnarkPack aggregation verification.
//!
//! Emits:
//! - top-level overview: `benches/compliance/flows.csv`
//! - category KPIs: `benches/compliance/validator/validator.csv`
//! - section overview: `benches/compliance/validator/sections.csv`
//! - section KPIs: `benches/compliance/validator/sections/<section>.csv`

use std::time::Instant;

use penumbra_sdk_bench_support::bench_runner;
use penumbra_sdk_bench_support::extraction::SpendOutputExtractionProfile;
use penumbra_sdk_proof_aggregation::{
    aggregate_family, pad_items_to_power_of_two, prepare_verify_inputs,
    verify_family_aggregate_profiled, AggregateVerificationProfile, DevSrs, PreparedVerifyInputs,
    ProofFamilyId,
};
use penumbra_sdk_proof_params::{
    batch::{self, BatchItem},
    OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY,
};
use penumbra_sdk_transaction::Transaction;

#[path = "../helpers/extraction.rs"]
mod extraction_helpers;
#[path = "../helpers/flow.rs"]
mod flow_helpers;
use flow_helpers::*;

const DEFAULT_BATCH_SIZES: &[usize] = &[100];
const DEEP_BATCH_SIZES: &[usize] = &[100, 1000];

const SECTION_NAMES: &[&str] = &[
    "binding_sig",
    "spend_auth_sig",
    "spend_extract",
    "spend_extract.to_batch_item",
    "output_extract",
    "output_extract.ciphertext_parse",
    "output_extract.to_batch_item",
    "extract",
    "legacy_batch_verify",
    "snarkpack_verify",
    "snarkpack_verify.deserialize",
    "snarkpack_verify.tipa_ab",
    "snarkpack_verify.tipa_c",
];

#[derive(Clone)]
struct FamilyFixture {
    aggregate_proof: Vec<u8>,
}

#[derive(Clone)]
struct BatchSampleProfile {
    total_ms: f64,
    extraction: SpendOutputExtractionProfile,
    legacy_batch_verify_ms: f64,
}

#[derive(Clone)]
struct SnarkpackSampleProfile {
    total_ms: f64,
    snarkpack_verify_ms: f64,
    verification: AggregateVerificationProfile,
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn verify_legacy_batch(spend_items: &[BatchItem], output_items: &[BatchItem]) {
    batch::batch_verify(&SPEND_PROOF_VERIFICATION_KEY, spend_items)
        .expect("spend legacy batch verify should succeed");
    batch::batch_verify(&OUTPUT_PROOF_VERIFICATION_KEY, output_items)
        .expect("output legacy batch verify should succeed");
}

fn build_family_fixture(
    family_id: ProofFamilyId,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Option<FamilyFixture> {
    if items.is_empty() {
        return None;
    }

    let padded_items =
        pad_items_to_power_of_two(items, srs.max_padded_count as usize).expect("padding");
    let aggregate_proof = match family_id {
        ProofFamilyId::Spend => {
            aggregate_family(family_id, &SPEND_PROOF_VERIFICATION_KEY, &padded_items, srs)
                .expect("spend aggregation should succeed")
        }
        ProofFamilyId::Output => aggregate_family(
            family_id,
            &OUTPUT_PROOF_VERIFICATION_KEY,
            &padded_items,
            srs,
        )
        .expect("output aggregation should succeed"),
        _ => unreachable!("validator benchmark only covers spend/output"),
    };

    Some(FamilyFixture { aggregate_proof })
}

fn build_snarkpack_fixture(
    spend_items: &[BatchItem],
    output_items: &[BatchItem],
) -> (Option<FamilyFixture>, Option<FamilyFixture>) {
    let srs = DevSrs::default();
    (
        build_family_fixture(ProofFamilyId::Spend, spend_items, &srs),
        build_family_fixture(ProofFamilyId::Output, output_items, &srs),
    )
}

fn prepare_snarkpack_inputs(
    spend_items: &[BatchItem],
    output_items: &[BatchItem],
    srs: &DevSrs,
) -> (Option<PreparedVerifyInputs>, Option<PreparedVerifyInputs>) {
    let spend_inputs = if spend_items.is_empty() {
        None
    } else {
        Some(
            prepare_verify_inputs(spend_items, srs.max_padded_count as usize)
                .expect("spend verify inputs"),
        )
    };
    let output_inputs = if output_items.is_empty() {
        None
    } else {
        Some(
            prepare_verify_inputs(output_items, srs.max_padded_count as usize)
                .expect("output verify inputs"),
        )
    };

    (spend_inputs, output_inputs)
}

fn profile_snarkpack_core(
    spend_inputs: Option<&PreparedVerifyInputs>,
    output_inputs: Option<&PreparedVerifyInputs>,
    spend_fixture: &Option<FamilyFixture>,
    output_fixture: &Option<FamilyFixture>,
    srs: &DevSrs,
) -> AggregateVerificationProfile {
    let mut profile = AggregateVerificationProfile::default();

    if let (Some(fixture), Some(inputs)) = (spend_fixture.as_ref(), spend_inputs) {
        let spend_profile = verify_family_aggregate_profiled(
            ProofFamilyId::Spend,
            &SPEND_PROOF_VERIFICATION_KEY,
            &fixture.aggregate_proof,
            &inputs.padded_public_inputs,
            srs,
        )
        .expect("spend SnarkPack verify should succeed");
        profile.merge(&spend_profile);
    }

    if let (Some(fixture), Some(inputs)) = (output_fixture.as_ref(), output_inputs) {
        let output_profile = verify_family_aggregate_profiled(
            ProofFamilyId::Output,
            &OUTPUT_PROOF_VERIFICATION_KEY,
            &fixture.aggregate_proof,
            &inputs.padded_public_inputs,
            srs,
        )
        .expect("output SnarkPack verify should succeed");
        profile.merge(&output_profile);
    }

    profile
}

fn profile_batch_sample(batch_txs: &[Transaction]) -> BatchSampleProfile {
    let total_start = Instant::now();
    let profiled = extraction_helpers::extract_proof_items_profiled(batch_txs)
        .expect("profiled extraction succeeds");
    let verify_start = Instant::now();
    verify_legacy_batch(&profiled.spend_items, &profiled.output_items);

    BatchSampleProfile {
        total_ms: elapsed_ms(total_start),
        extraction: profiled.profile,
        legacy_batch_verify_ms: elapsed_ms(verify_start),
    }
}

fn profile_snarkpack_sample(
    batch_txs: &[Transaction],
    spend_fixture: &Option<FamilyFixture>,
    output_fixture: &Option<FamilyFixture>,
    srs: &DevSrs,
) -> SnarkpackSampleProfile {
    let total_start = Instant::now();
    let profiled = extraction_helpers::extract_proof_items_profiled(batch_txs)
        .expect("profiled extraction succeeds");
    let verify_start = Instant::now();
    let (spend_inputs, output_inputs) =
        prepare_snarkpack_inputs(&profiled.spend_items, &profiled.output_items, srs);
    let verification = profile_snarkpack_core(
        spend_inputs.as_ref(),
        output_inputs.as_ref(),
        spend_fixture,
        output_fixture,
        srs,
    );

    SnarkpackSampleProfile {
        total_ms: elapsed_ms(total_start),
        snarkpack_verify_ms: elapsed_ms(verify_start),
        verification,
    }
}

fn build_transaction(prepared: &PreparedPlan) -> Transaction {
    let rt = tokio::runtime::Runtime::new().expect("can create runtime");
    let fvk = &penumbra_sdk_keys::test_keys::FULL_VIEWING_KEY;
    let sk = &penumbra_sdk_keys::test_keys::SPEND_KEY;
    let auth_data = prepared
        .plan
        .authorize(rand_core::OsRng, sk)
        .expect("can authorize");
    rt.block_on(async {
        prepared
            .plan
            .clone()
            .build_concurrent(fvk, &prepared.witness_data, &auth_data)
            .await
            .expect("can build transaction")
    })
}

fn benchmark_batch_sizes(regression: bool, quick: bool) -> Vec<usize> {
    let defaults = if regression || quick {
        DEFAULT_BATCH_SIZES
    } else {
        DEEP_BATCH_SIZES
    };
    bench_runner::usize_list_env("BENCH_VALIDATOR_BATCH_SIZES", defaults)
}

fn pick_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    batch_size: &str,
    mode: &str,
    metric: &str,
) -> bench_runner::BenchResult {
    raw.iter()
        .find(|r| {
            r.version == version
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "batch_size" && v == batch_size)
                && r.dimensions.iter().any(|(k, v)| k == "mode" && v == mode)
                && r.dimensions.iter().any(|(k, v)| k == "metric" && v == metric)
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "missing KPI row for version={version}, batch_size={batch_size}, mode={mode}, metric={metric}"
            )
        })
}

fn flow_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    batch_sizes: &[usize],
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();

    for batch_size in batch_sizes {
        let batch_size = batch_size.to_string();
        rows.push(pick_kpi(raw, version, &batch_size, "batch", "latency_ms"));
        rows.push(pick_kpi(
            raw,
            version,
            &batch_size,
            "snarkpack",
            "latency_ms",
        ));

        if !regression {
            rows.push(pick_kpi(raw, version, &batch_size, "batch", "per_tx_ms"));
            rows.push(pick_kpi(
                raw,
                version,
                &batch_size,
                "snarkpack",
                "per_tx_ms",
            ));
            rows.push(pick_kpi(
                raw,
                version,
                &batch_size,
                "snarkpack_over_batch",
                "ratio",
            ));
        }
    }

    rows
}

fn pick_section_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    batch_size: &str,
    section: &str,
) -> bench_runner::BenchResult {
    raw.iter()
        .find(|r| {
            r.version == version
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "batch_size" && v == batch_size)
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "section" && v == section)
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "missing section row for version={version}, batch_size={batch_size}, section={section}"
            )
        })
}

fn section_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    batch_sizes: &[usize],
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    for batch_size in batch_sizes {
        let batch_size = batch_size.to_string();
        for section in SECTION_NAMES {
            rows.push(pick_section_kpi(raw, version, &batch_size, section));
        }
    }
    rows
}

fn push_section_result(
    out: &mut Vec<bench_runner::BenchResult>,
    version: &str,
    batch_size: usize,
    section: &str,
    times: &[f64],
) {
    let batch_size = batch_size.to_string();
    out.push(bench_runner::make_result(
        version,
        &[("batch_size", &batch_size), ("section", section)],
        times,
        None,
    ));
}

fn profile_times(
    profiles: &[SpendOutputExtractionProfile],
    selector: impl Fn(&SpendOutputExtractionProfile) -> f64,
) -> Vec<f64> {
    profiles.iter().map(selector).collect()
}

fn main() {
    let version = bench_runner::bench_version();
    let warmup = bench_runner::warmup_count();
    let samples = bench_runner::sample_count();
    let quick = bench_runner::is_quick_profile();
    let regression = bench_runner::is_regression_suite();
    let batch_sizes = benchmark_batch_sizes(regression, quick);
    let max_batch_size = *batch_sizes.iter().max().expect("at least one batch size");

    let txs: Vec<_> = (0..max_batch_size)
        .map(|_| {
            let prepared = build_simple_transfer();
            build_transaction(&prepared)
        })
        .collect();

    let srs = DevSrs::default();
    let mut raw_results = Vec::new();

    for &batch_size in &batch_sizes {
        let batch_size_label = batch_size.to_string();
        let batch_txs = &txs[..batch_size];
        let base_extracted =
            extraction_helpers::extract_proof_items(batch_txs).expect("extraction succeeds");
        let base_spend_items = base_extracted.spend_items;
        let base_output_items = base_extracted.output_items;
        let (spend_fixture, output_fixture) =
            build_snarkpack_fixture(&base_spend_items, &base_output_items);

        let batch_profiles =
            bench_runner::run_collect(warmup, samples, || profile_batch_sample(batch_txs));
        let batch_times = batch_profiles
            .iter()
            .map(|profile| profile.total_ms)
            .collect::<Vec<_>>();
        raw_results.push(bench_runner::make_result(
            &version,
            &[
                ("batch_size", &batch_size_label),
                ("mode", "batch"),
                ("metric", "latency_ms"),
            ],
            &batch_times,
            None,
        ));

        let snarkpack_profiles = bench_runner::run_collect(warmup, samples, || {
            profile_snarkpack_sample(batch_txs, &spend_fixture, &output_fixture, &srs)
        });
        let snarkpack_times = snarkpack_profiles
            .iter()
            .map(|profile| profile.total_ms)
            .collect::<Vec<_>>();
        raw_results.push(bench_runner::make_result(
            &version,
            &[
                ("batch_size", &batch_size_label),
                ("mode", "snarkpack"),
                ("metric", "latency_ms"),
            ],
            &snarkpack_times,
            None,
        ));

        if !regression {
            let per_tx_batch: Vec<f64> =
                batch_times.iter().map(|t| t / batch_size as f64).collect();
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("batch_size", &batch_size_label),
                    ("mode", "batch"),
                    ("metric", "per_tx_ms"),
                ],
                &per_tx_batch,
                None,
            ));

            let per_tx_snarkpack: Vec<f64> = snarkpack_times
                .iter()
                .map(|t| t / batch_size as f64)
                .collect();
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("batch_size", &batch_size_label),
                    ("mode", "snarkpack"),
                    ("metric", "per_tx_ms"),
                ],
                &per_tx_snarkpack,
                None,
            ));

            let ratio: Vec<f64> = snarkpack_times
                .iter()
                .zip(batch_times.iter())
                .map(|(snarkpack, batch)| if *batch > 0.0 { snarkpack / batch } else { 0.0 })
                .collect();
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("batch_size", &batch_size_label),
                    ("mode", "snarkpack_over_batch"),
                    ("metric", "ratio"),
                ],
                &ratio,
                None,
            ));
        }

        let extraction_profiles = batch_profiles
            .iter()
            .map(|profile| profile.extraction.clone())
            .collect::<Vec<_>>();
        for (section, times) in [
            (
                "binding_sig",
                profile_times(&extraction_profiles, |profile| profile.binding_sig_ms),
            ),
            (
                "spend_auth_sig",
                profile_times(&extraction_profiles, |profile| profile.spend_auth_sig_ms),
            ),
            (
                "spend_extract",
                profile_times(&extraction_profiles, |profile| profile.spend_extract_ms),
            ),
            (
                "spend_extract.to_batch_item",
                profile_times(&extraction_profiles, |profile| {
                    profile.spend_to_batch_item_ms
                }),
            ),
            (
                "output_extract",
                profile_times(&extraction_profiles, |profile| profile.output_extract_ms),
            ),
            (
                "output_extract.ciphertext_parse",
                profile_times(&extraction_profiles, |profile| {
                    profile.output_ciphertext_parse_ms
                }),
            ),
            (
                "output_extract.to_batch_item",
                profile_times(&extraction_profiles, |profile| {
                    profile.output_to_batch_item_ms
                }),
            ),
            (
                "extract",
                profile_times(
                    &extraction_profiles,
                    penumbra_sdk_bench_support::extraction::SpendOutputExtractionProfile::total_extract_ms,
                ),
            ),
        ] {
            push_section_result(&mut raw_results, &version, batch_size, section, &times);
        }

        let legacy_batch_verify_times = batch_profiles
            .iter()
            .map(|profile| profile.legacy_batch_verify_ms)
            .collect::<Vec<_>>();
        push_section_result(
            &mut raw_results,
            &version,
            batch_size,
            "legacy_batch_verify",
            &legacy_batch_verify_times,
        );

        let snarkpack_verify_times = snarkpack_profiles
            .iter()
            .map(|profile| profile.snarkpack_verify_ms)
            .collect::<Vec<_>>();
        push_section_result(
            &mut raw_results,
            &version,
            batch_size,
            "snarkpack_verify",
            &snarkpack_verify_times,
        );

        let verification_profiles = snarkpack_profiles
            .iter()
            .map(|profile| profile.verification.clone())
            .collect::<Vec<_>>();
        let deserialize_times = verification_profiles
            .iter()
            .map(|profile| profile.deserialize_ms)
            .collect::<Vec<_>>();
        push_section_result(
            &mut raw_results,
            &version,
            batch_size,
            "snarkpack_verify.deserialize",
            &deserialize_times,
        );

        let tipa_ab_times = verification_profiles
            .iter()
            .map(|profile| profile.tipa_ab_ms)
            .collect::<Vec<_>>();
        push_section_result(
            &mut raw_results,
            &version,
            batch_size,
            "snarkpack_verify.tipa_ab",
            &tipa_ab_times,
        );

        let tipa_c_times = verification_profiles
            .iter()
            .map(|profile| profile.tipa_c_ms)
            .collect::<Vec<_>>();
        push_section_result(
            &mut raw_results,
            &version,
            batch_size,
            "snarkpack_verify.tipa_c",
            &tipa_c_times,
        );
    }

    let flow_rows = flow_rows_for_version(&raw_results, &version, &batch_sizes, regression);
    let section_rows = section_rows_for_version(&raw_results, &version, &batch_sizes);
    let mut flow_with_meta = flow_rows.clone();
    bench_runner::annotate_raw_results(&mut flow_with_meta);
    let mut sections_with_meta = section_rows.clone();
    bench_runner::annotate_raw_results(&mut sections_with_meta);
    bench_runner::output_results(&flow_with_meta);

    let flow_path = bench_runner::category_csv_path("validator");
    bench_runner::append_csv(&flow_path, &flow_with_meta);

    let sections_overview_path = bench_runner::category_sections_csv_path("validator");
    bench_runner::append_csv(&sections_overview_path, &sections_with_meta);

    let flows_overview = bench_runner::to_flow_overview_rows("validator", &flow_with_meta);
    let flows_overview_path = bench_runner::flows_overview_csv_path();
    bench_runner::append_csv_scoped(&flows_overview_path, &flows_overview, &["category", "kpi"]);

    for section in SECTION_NAMES {
        let mut rows: Vec<_> = sections_with_meta
            .iter()
            .filter(|r| {
                r.dimensions
                    .iter()
                    .any(|(k, v)| k == "section" && v == *section)
            })
            .cloned()
            .collect();
        for r in &mut rows {
            r.dimensions.retain(|(k, _)| k != "section");
        }
        let section_path = bench_runner::section_csv_path("validator", section);
        bench_runner::append_csv(&section_path, &rows);
    }
}
