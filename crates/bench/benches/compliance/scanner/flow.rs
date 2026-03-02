//! Scanning flow benchmark: block-level scanning throughput.
//!
//! Simulates processing a block of transactions with varying sizes
//! and match rates. Measures detection-only and full scan pipelines.
//!
//! Outputs: `benches/compliance/scanner/results/flow.csv`

use std::path::PathBuf;

use decaf377::{Fq, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::{
    decrypt_core, decrypt_detection_tier, decrypt_extension, derive_compliance_scalar,
    issuer_keys::DetectionKey,
    test_helpers::{self, make_address, OutputEncryptionResult, SpendEncryptionResult},
};
use penumbra_sdk_num::Amount;

const WARMUP: usize = 1;
const SAMPLES: usize = 10;

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

            // For matching TXs, the wallet is the counterparty (recipient)
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
                // Wallet is counterparty on spend — not directly relevant
                // but we compute it for completeness
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

fn main() {
    let mut results = Vec::new();

    // --- Shared setup ---
    let dk = DetectionKey::demo();
    let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
    let dk_pub = dk.public_key();
    let wallet_addr = make_address(42);
    let asset_id = asset::Id(Fq::from(1000u64));

    for &block_size in &[10usize, 100] {
        for &match_pct in &[10usize, 100] {
            let label = format!("block_{}_{}pct", block_size, match_pct);
            eprintln!("=== {} ===", label);

            let block = generate_block(
                block_size,
                match_pct,
                &ring_pk,
                &dk_pub,
                &wallet_addr,
                asset_id,
            );

            // --- Detection-only scan ---
            // Try detection tier on every ciphertext in the block
            eprintln!("  Benchmarking {}_detect...", label);
            let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
                let mut _matches = 0u32;
                for tx in &block {
                    // Detection on spend ciphertext
                    if decrypt_detection_tier(
                        dk.inner(),
                        &tx.spend.ciphertext.epk_1,
                        &tx.spend.ciphertext.detection_tag,
                        &asset_id,
                    )
                    .is_ok()
                    {
                        _matches += 1;
                    }
                    // Detection on output ciphertext
                    if decrypt_detection_tier(
                        dk.inner(),
                        &tx.output.ciphertext.epk_1,
                        &tx.output.ciphertext.detection_tag,
                        &asset_id,
                    )
                    .is_ok()
                    {
                        _matches += 1;
                    }
                }
            });
            results.push(bench_runner::make_result(
                "v0.1",
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{}%", match_pct)),
                    ("stage", "detect"),
                ],
                &times,
                None,
            ));

            // --- Full scan (detection + core + extension for matches) ---
            eprintln!("  Benchmarking {}_full...", label);
            let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
                for tx in &block {
                    // Detection on both ciphertexts
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

                    // For matches: decrypt core + extension
                    if tx.is_match && output_detected {
                        let _core = decrypt_core(&tx.output_ss_core, &tx.output.ciphertext);
                        let _ext = decrypt_extension(&tx.output_ss_ext, &tx.output.ciphertext);
                    }
                    if tx.is_match && spend_detected {
                        let _core = decrypt_core(&tx.spend_ss, &tx.spend.ciphertext);
                    }
                }
            });
            results.push(bench_runner::make_result(
                "v0.1",
                &[
                    ("block_size", &block_size.to_string()),
                    ("match_rate", &format!("{}%", match_pct)),
                    ("stage", "full"),
                ],
                &times,
                None,
            ));
        }
    }

    // --- Output ---
    bench_runner::print_table(&results);
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/scanner/results/flow.csv");
    bench_runner::write_csv(&csv_path, &results);
}
