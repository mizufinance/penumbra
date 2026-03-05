//! Compliance scanning benchmarks (v0.1 only).
//!
//! Measures decryption tiers, batch scanning throughput, and detection scanning.
//! No vanilla counterpart — scanning is entirely new with compliance.
//!
//! Outputs: `benches/compliance/scanner/decryption.csv`

use std::path::PathBuf;

use decaf377::{Fq, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::{
    decrypt_core, decrypt_core_flagged, decrypt_detection_tier, decrypt_extension,
    decrypt_extension_flagged, decrypt_full, decrypt_full_flagged, derive_compliance_scalar,
    issuer_keys::DetectionKey,
    test_helpers::{self, make_address},
};
use penumbra_sdk_num::Amount;

const WARMUP: usize = 2;
const SAMPLES: usize = 50;

fn derive_ack(ring_pk: &decaf377::Element, addr: &penumbra_sdk_keys::Address) -> decaf377::Element {
    let b_d_fq = addr.diversified_generator().vartime_compress_to_field();
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    *ring_pk * d_fr
}

fn main() {
    let mut results = Vec::new();

    // --- Shared setup ---
    let dk = DetectionKey::demo();
    let dk_pub = dk.public_key();
    let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
    let self_addr = make_address(1);
    let counterparty_addr = make_address(2);
    let asset_id = asset::Id(Fq::from(1000u64));
    let amount = Amount::from(100u64);

    // --- Tier decryption (output ciphertext, shared-secret path) ---
    eprintln!("Benchmarking tier decryption...");
    {
        let result = test_helpers::encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &self_addr,
            &counterparty_addr,
            asset_id,
            amount,
            false,
        );
        let ct = &result.ciphertext;
        let ack_receiver = derive_ack(&ring_pk, &counterparty_addr);
        let ss_core = ack_receiver * result.r_1;
        let ss_ext = ack_receiver * result.r_2;

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_core(&ss_core, ct);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "decrypt_core")],
            &times,
            None,
        ));

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_extension(&ss_ext, ct);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "decrypt_extension")],
            &times,
            None,
        ));

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_full(&ss_core, &ss_ext, ct, asset_id);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "decrypt_full")],
            &times,
            None,
        ));
    }

    // --- Spend-side decryption ---
    eprintln!("Benchmarking spend decryption...");
    {
        let result = test_helpers::encrypt_test_spend(
            &ring_pk, &dk_pub, &self_addr, asset_id, amount, false,
        );
        let ct = &result.ciphertext;
        let ack_self = derive_ack(&ring_pk, &self_addr);
        let ss_core = ack_self * result.r_s;

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_core(&ss_core, ct);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "spend_decrypt_core")],
            &times,
            None,
        ));
    }

    // --- Flagged decryption (issuer DK path) ---
    eprintln!("Benchmarking flagged decryption...");
    {
        let result = test_helpers::encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &self_addr,
            &counterparty_addr,
            asset_id,
            amount,
            true,
        );
        let ct = &result.ciphertext;

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_core_flagged(dk.inner(), ct);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "flagged_decrypt_core")],
            &times,
            None,
        ));

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_extension_flagged(dk.inner(), ct);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "flagged_decrypt_extension")],
            &times,
            None,
        ));

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _data = decrypt_full_flagged(dk.inner(), ct, asset_id);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "flagged_decrypt_full")],
            &times,
            None,
        ));
    }

    // --- Batch core decryption ---
    eprintln!("Benchmarking batch core decryption...");
    for &batch_size in &[10usize, 100, 1000] {
        eprintln!("  batch_size={}", batch_size);
        let mut ciphertexts = Vec::with_capacity(batch_size);
        let mut shared_secrets = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let addr = make_address((i % 200) as u8);
            let counterparty = make_address(((i + 1) % 200) as u8);
            let amt = Amount::from((i as u64 + 1) * 100);
            let result = test_helpers::encrypt_test_output(
                &ring_pk,
                &dk_pub,
                &addr,
                &counterparty,
                asset_id,
                amt,
                false,
            );
            let ack_receiver = derive_ack(&ring_pk, &counterparty);
            let ss = ack_receiver * result.r_1;
            shared_secrets.push(ss);
            ciphertexts.push(result.ciphertext);
        }

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            for (ct, ss) in ciphertexts.iter().zip(shared_secrets.iter()) {
                let _data = decrypt_core(ss, ct);
            }
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("operation", "decrypt"),
                ("batch_size", &batch_size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Detection tier decryption ---
    eprintln!("Benchmarking detection decryption...");
    {
        let result = test_helpers::encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &self_addr,
            &counterparty_addr,
            asset_id,
            amount,
            false,
        );
        let ct = &result.ciphertext;

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result =
                decrypt_detection_tier(dk.inner(), &ct.epk_1, &ct.detection_tag, &asset_id);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "detection_decrypt")],
            &times,
            None,
        ));
    }

    // --- Batch detection scanning ---
    eprintln!("Benchmarking batch detection scanning...");
    let target_asset = asset::Id(Fq::from(1000u64));
    for &batch_size in &[10usize, 100, 1000] {
        eprintln!("  batch_size={}", batch_size);
        let mut ciphertexts = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let addr = make_address((i % 200) as u8);
            let counterparty = make_address(((i + 1) % 200) as u8);
            let amt = Amount::from(100u64);
            // 50% match rate
            let asset = if i % 2 == 0 {
                target_asset
            } else {
                asset::Id(Fq::from(9999u64))
            };
            let result = test_helpers::encrypt_test_output(
                &ring_pk,
                &dk_pub,
                &addr,
                &counterparty,
                asset,
                amt,
                false,
            );
            ciphertexts.push(result.ciphertext);
        }

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let mut _matches = 0u32;
            for ct in &ciphertexts {
                if decrypt_detection_tier(dk.inner(), &ct.epk_1, &ct.detection_tag, &target_asset)
                    .is_ok()
                {
                    _matches += 1;
                }
            }
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("operation", "detection"),
                ("batch_size", &batch_size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Output ---
    bench_runner::output_results(&results);
    let csv_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/compliance/scanner/decryption.csv");
    bench_runner::write_csv(&csv_path, &results);
}
