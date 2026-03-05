//! Scanning flow benchmark: block-level scanning throughput.
//!
//! Emits:
//! - top-level overview: `benches/compliance/flows.csv`
//! - category KPIs: `benches/compliance/scanner/scanner.csv`
//! - section overview: `benches/compliance/scanner/sections.csv`
//! - section KPIs: `benches/compliance/scanner/sections/<section>.csv`

use decaf377::{Fq, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::{
    decrypt_core, decrypt_detection_tier, decrypt_extension, derive_compliance_scalar,
    issuer_keys::DetectionKey,
    test_helpers::{self, make_address, OutputEncryptionResult, SpendEncryptionResult},
};
use penumbra_sdk_num::Amount;

fn derive_ack(ring_pk: &decaf377::Element, addr: &penumbra_sdk_keys::Address) -> decaf377::Element {
    let b_d_fq = addr.diversified_generator().vartime_compress_to_field();
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    *ring_pk * d_fr
}

/// A simulated transaction's ciphertexts (1 spend + 1 output).
struct TxCiphertexts {
    spend: SpendEncryptionResult,
    output: OutputEncryptionResult,
    /// Whether this TX matches the scanning wallet
    is_match: bool,
    /// Shared secret for spend decryption (if matching)
    spend_ss: decaf377::Element,
    /// Shared secret for output core decryption (if matching)
    output_ss_core: decaf377::Element,
    /// Shared secret for output extension decryption (if matching)
    output_ss_ext: decaf377::Element,
}

/// Generate a block of transactions.
fn generate_block(
    block_size: usize,
    match_rate_pct: usize,
    ring_pk: &decaf377::Element,
    dk_pub: &decaf377::Element,
    wallet_addr: &penumbra_sdk_keys::Address,
    asset_id: asset::Id,
) -> Vec<TxCiphertexts> {
    let match_count = (block_size * match_rate_pct) / 100;
    let ack_wallet = derive_ack(ring_pk, wallet_addr);

    (0..block_size)
        .map(|i| {
            let is_match = i < match_count;

            let sender = make_address((i % 200) as u8);
            let recipient = if is_match {
                wallet_addr.clone()
            } else {
                make_address(((i + 50) % 200) as u8)
            };

            let amt = Amount::from((i as u64 + 1) * 100);

            let spend_result =
                test_helpers::encrypt_test_spend(ring_pk, dk_pub, &sender, asset_id, amt, false);
            let output_result = test_helpers::encrypt_test_output(
                ring_pk, dk_pub, &recipient, &sender, asset_id, amt, false,
            );

            let spend_ss = if is_match {
                ack_wallet * spend_result.r_s
            } else {
                decaf377::Element::default()
            };
            let output_ss_core = if is_match {
                ack_wallet * output_result.r_1
            } else {
                decaf377::Element::default()
            };
            let output_ss_ext = if is_match {
                ack_wallet * output_result.r_2
            } else {
                decaf377::Element::default()
            };

            TxCiphertexts {
                spend: spend_result,
                output: output_result,
                is_match,
                spend_ss,
                output_ss_core,
                output_ss_ext,
            }
        })
        .collect()
}

fn pick_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    block_size: usize,
    match_rate: usize,
    stage: &str,
) -> bench_runner::BenchResult {
    raw.iter()
        .find(|r| {
            r.version == version
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "block_size" && v == &block_size.to_string())
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "match_rate" && v == &format!("{match_rate}%"))
                && r.dimensions.iter().any(|(k, v)| k == "stage" && v == stage)
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "missing KPI row for version={version}, block_size={block_size}, match_rate={match_rate}, stage={stage}"
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
        vec![(10usize, "full")]
    } else {
        vec![
            (10usize, "full"),
            (10usize, "full_per_tx"),
            (100usize, "full"),
            (100usize, "full_per_tx"),
        ]
    };
    for (match_rate, stage) in kpis {
        rows.push(pick_kpi(raw, version, 100, match_rate, stage));
    }
    rows
}

fn pick_section_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    block_size: usize,
    match_rate: usize,
    section: &str,
) -> bench_runner::BenchResult {
    raw.iter()
        .find(|r| {
            r.version == version
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "block_size" && v == &block_size.to_string())
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "match_rate" && v == &format!("{match_rate}%"))
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "section" && v == section)
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "missing section KPI row for version={version}, block_size={block_size}, match_rate={match_rate}, section={section}"
            )
        })
}

fn section_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    for (match_rate, section) in [(10usize, "detect"), (10usize, "decrypt")] {
        rows.push(pick_section_kpi(raw, version, 100, match_rate, section));
        if !regression {
            rows.push(pick_section_kpi(raw, version, 100, 100usize, section));
        }
    }
    rows
}

fn main() {
    let version = bench_runner::bench_version();
    let warmup = bench_runner::warmup_count();
    let samples = bench_runner::sample_count();
    let regression = bench_runner::is_regression_suite();
    let quick = bench_runner::is_quick_profile();

    let block_sizes: Vec<usize> = if regression || quick {
        vec![100]
    } else {
        vec![10, 100]
    };
    let match_rates: Vec<usize> = if regression { vec![10] } else { vec![10, 100] };

    let mut raw_results = Vec::new();

    // Shared setup for measured version.
    let dk = DetectionKey::demo();
    let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
    let dk_pub = dk.public_key();
    let wallet_addr = make_address(42);
    let asset_id = asset::Id(Fq::from(1000u64));

    for &block_size in &block_sizes {
        for &match_pct in &match_rates {
            let label = format!("{version}_block_{block_size}_{match_pct}pct");
            eprintln!("=== {label} ===");

            let block = generate_block(
                block_size,
                match_pct,
                &ring_pk,
                &dk_pub,
                &wallet_addr,
                asset_id,
            );

            // Detection-only scan.
            let detect_times = bench_runner::run_bench(warmup, samples, || {
                let mut matches_count = 0u32;
                for tx in &block {
                    if decrypt_detection_tier(
                        dk.inner(),
                        &tx.spend.ciphertext.epk_1,
                        &tx.spend.ciphertext.detection_tag,
                        &asset_id,
                    )
                    .is_ok()
                    {
                        matches_count += 1;
                    }
                    if decrypt_detection_tier(
                        dk.inner(),
                        &tx.output.ciphertext.epk_1,
                        &tx.output.ciphertext.detection_tag,
                        &asset_id,
                    )
                    .is_ok()
                    {
                        matches_count += 1;
                    }
                }
                std::hint::black_box(matches_count);
            });
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{match_pct}%")),
                    ("stage", "detect"),
                ],
                &detect_times,
                None,
            ));
            let detect_per_tx: Vec<f64> =
                detect_times.iter().map(|t| t / block_size as f64).collect();
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{match_pct}%")),
                    ("stage", "detect_per_tx"),
                ],
                &detect_per_tx,
                None,
            ));

            // Full scan (detection + decrypt matched txs).
            let full_times = bench_runner::run_bench(warmup, samples, || {
                for tx in &block {
                    let spend_detected = decrypt_detection_tier(
                        dk.inner(),
                        &tx.spend.ciphertext.epk_1,
                        &tx.spend.ciphertext.detection_tag,
                        &asset_id,
                    )
                    .is_ok();

                    let output_detected = decrypt_detection_tier(
                        dk.inner(),
                        &tx.output.ciphertext.epk_1,
                        &tx.output.ciphertext.detection_tag,
                        &asset_id,
                    )
                    .is_ok();

                    if tx.is_match && output_detected {
                        let _ = decrypt_core(&tx.output_ss_core, &tx.output.ciphertext);
                        let _ = decrypt_extension(&tx.output_ss_ext, &tx.output.ciphertext);
                    }
                    if tx.is_match && spend_detected {
                        let _ = decrypt_core(&tx.spend_ss, &tx.spend.ciphertext);
                    }
                }
            });
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{match_pct}%")),
                    ("stage", "full"),
                ],
                &full_times,
                None,
            ));
            let full_per_tx: Vec<f64> = full_times.iter().map(|t| t / block_size as f64).collect();
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{match_pct}%")),
                    ("section", "detect"),
                ],
                &detect_times,
                None,
            ));
            let decrypt_extra: Vec<f64> = full_times
                .iter()
                .zip(detect_times.iter())
                .map(|(full, detect)| (full - detect).max(0.0))
                .collect();
            raw_results.push(bench_runner::make_result(
                &version,
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{match_pct}%")),
                    ("section", "decrypt"),
                ],
                &decrypt_extra,
                None,
            ));
            if !regression {
                raw_results.push(bench_runner::make_result(
                    &version,
                    &[
                        ("block_size", &block_size.to_string()),
                        ("match_rate", &format!("{match_pct}%")),
                        ("stage", "full_per_tx"),
                    ],
                    &full_per_tx,
                    None,
                ));
            }
        }
    }

    let flow_rows = flow_rows_for_version(&raw_results, &version, regression);
    let section_rows = section_rows_for_version(&raw_results, &version, regression);
    let mut flow_with_meta = flow_rows.clone();
    bench_runner::annotate_raw_results(&mut flow_with_meta);
    let mut sections_with_meta = section_rows.clone();
    bench_runner::annotate_raw_results(&mut sections_with_meta);
    bench_runner::output_results(&flow_with_meta);

    let flow_path = bench_runner::category_csv_path("scanner");
    bench_runner::append_csv(&flow_path, &flow_with_meta);

    let sections_overview_path = bench_runner::category_sections_csv_path("scanner");
    bench_runner::append_csv(&sections_overview_path, &sections_with_meta);

    let flows_overview = bench_runner::to_flow_overview_rows("scanner", &flow_with_meta);
    let flows_overview_path = bench_runner::flows_overview_csv_path();
    bench_runner::append_csv_scoped(&flows_overview_path, &flows_overview, &["category", "kpi"]);

    for section in ["detect", "decrypt"] {
        let mut rows: Vec<_> = sections_with_meta
            .iter()
            .filter(|r| {
                r.dimensions
                    .iter()
                    .any(|(k, v)| k == "section" && v == section)
            })
            .cloned()
            .collect();
        for r in &mut rows {
            r.dimensions.retain(|(k, _)| k != "section");
        }
        let section_path = bench_runner::section_csv_path("scanner", section);
        bench_runner::append_csv(&section_path, &rows);
    }
}
