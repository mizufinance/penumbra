//! Client-side compliance enrichment benchmarks: vanilla (v0) vs compliance (v0.1).
//!
//! v0: no compliance enrichment existed — 0ms baseline.
//! v0.1: encryption + DLEQ proofs added per action during TX building.
//!
//! Outputs: `benches/compliance/client/results/enrichment.csv`

use std::path::PathBuf;

use decaf377::{Fq, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::crypto::{
    compute_metadata_hash, compute_output_dleqs, compute_spend_dleq, encrypt_output, encrypt_spend,
};
use penumbra_sdk_compliance::derive_compliance_scalar;
use penumbra_sdk_compliance::issuer_keys::DetectionKey;
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use rand_core::OsRng;

const WARMUP: usize = 2;
const SAMPLES: usize = 30;

fn derive_ack(ring_pk: &decaf377::Element, b_d_fq: Fq) -> decaf377::Element {
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    *ring_pk * d_fr
}

fn main() {
    let mut results = Vec::new();

    // --- Shared setup ---
    let mut rng = OsRng;
    let dk = DetectionKey::demo();
    let dk_pub = dk.public_key();
    let sk_ring = Fr::rand(&mut rng);
    let ring_pk = decaf377::Element::GENERATOR * sk_ring;

    let sender_address = Address::dummy(&mut rng);
    let recipient_address = Address::dummy(&mut rng);
    let asset_id = asset::Id(Fq::from(1000u64));
    let amount = Amount::from(50_000u128);

    let sender_b_d_fq = sender_address
        .diversified_generator()
        .vartime_compress_to_field();
    let recipient_b_d_fq = recipient_address
        .diversified_generator()
        .vartime_compress_to_field();
    let ack_sender = derive_ack(&ring_pk, sender_b_d_fq);
    let ack_recipient = derive_ack(&ring_pk, recipient_b_d_fq);

    // --- Spend client crypto ---
    // v0: no compliance crypto existed
    results.push(bench_runner::make_zero_result(
        "v0",
        &[("scenario", "spend_crypto")],
        SAMPLES,
    ));

    // v0.1: encrypt + DLEQ for spend
    eprintln!("Benchmarking spend_client_crypto...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let salt = Fq::rand(&mut OsRng);
        let result = encrypt_spend(
            &mut OsRng,
            &ack_sender,
            &dk_pub,
            &sender_address,
            asset_id,
            amount,
            false,
            salt,
        )
        .expect("can encrypt");

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(0u64),
            salt,
        );
        let k = Fr::rand(&mut OsRng);
        let _dleq = compute_spend_dleq(result.r_s, k, &ack_sender, metadata_hash);
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "spend_crypto")],
        &times,
        None,
    ));

    // --- Output client crypto ---
    // v0: no compliance crypto existed
    results.push(bench_runner::make_zero_result(
        "v0",
        &[("scenario", "output_crypto")],
        SAMPLES,
    ));

    // v0.1: encrypt + 3 DLEQs for output
    eprintln!("Benchmarking output_client_crypto...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let salt = Fq::rand(&mut OsRng);
        let result = encrypt_output(
            &mut OsRng,
            &ack_recipient,
            &ack_sender,
            &dk_pub,
            &recipient_address,
            &sender_address,
            asset_id,
            amount,
            false,
            salt,
        )
        .expect("can encrypt");

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(0u64),
            salt,
        );
        let k_1 = Fr::rand(&mut OsRng);
        let k_2 = Fr::rand(&mut OsRng);
        let k_3 = Fr::rand(&mut OsRng);
        let _dleqs = compute_output_dleqs(
            result.r_1,
            result.r_2,
            result.r_3,
            k_1,
            k_2,
            k_3,
            &ack_recipient,
            &ack_sender,
            metadata_hash,
        );
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "output_crypto")],
        &times,
        None,
    ));

    // --- Simple transfer (1 spend + 1 output) ---
    // v0: no enrichment in vanilla
    results.push(bench_runner::make_zero_result(
        "v0",
        &[("scenario", "1S1O")],
        SAMPLES,
    ));

    // v0.1: spend + output enrichment
    eprintln!("Benchmarking simple_transfer...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let salt_spend = Fq::rand(&mut OsRng);
        let salt_output = Fq::rand(&mut OsRng);
        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(0u64),
            salt_spend,
        );

        // Spend
        let spend_result = encrypt_spend(
            &mut OsRng,
            &ack_sender,
            &dk_pub,
            &sender_address,
            asset_id,
            amount,
            false,
            salt_spend,
        )
        .expect("can encrypt spend");
        let k = Fr::rand(&mut OsRng);
        let _spend_dleq = compute_spend_dleq(spend_result.r_s, k, &ack_sender, metadata_hash);

        // Output
        let output_result = encrypt_output(
            &mut OsRng,
            &ack_recipient,
            &ack_sender,
            &dk_pub,
            &recipient_address,
            &sender_address,
            asset_id,
            amount,
            false,
            salt_output,
        )
        .expect("can encrypt output");
        let k_1 = Fr::rand(&mut OsRng);
        let k_2 = Fr::rand(&mut OsRng);
        let k_3 = Fr::rand(&mut OsRng);
        let _output_dleqs = compute_output_dleqs(
            output_result.r_1,
            output_result.r_2,
            output_result.r_3,
            k_1,
            k_2,
            k_3,
            &ack_recipient,
            &ack_sender,
            metadata_hash,
        );
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "1S1O")],
        &times,
        None,
    ));

    // --- Multi-spend (4 spends + 1 output) ---
    // v0: no enrichment in vanilla
    results.push(bench_runner::make_zero_result(
        "v0",
        &[("scenario", "4S1O")],
        SAMPLES,
    ));

    // v0.1: 4 spend + 1 output enrichment
    eprintln!("Benchmarking multi_spend_4x1...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(0u64),
            Fq::rand(&mut OsRng),
        );

        // 4 spend encryptions + DLEQs
        for _ in 0..4 {
            let salt = Fq::rand(&mut OsRng);
            let result = encrypt_spend(
                &mut OsRng,
                &ack_sender,
                &dk_pub,
                &sender_address,
                asset_id,
                Amount::from(10_000u128),
                false,
                salt,
            )
            .expect("can encrypt");
            let k = Fr::rand(&mut OsRng);
            let _dleq = compute_spend_dleq(result.r_s, k, &ack_sender, metadata_hash);
        }

        // 1 output encryption + 3 DLEQs
        let output_result = encrypt_output(
            &mut OsRng,
            &ack_recipient,
            &ack_sender,
            &dk_pub,
            &recipient_address,
            &sender_address,
            asset_id,
            Amount::from(40_000u128),
            false,
            Fq::rand(&mut OsRng),
        )
        .expect("can encrypt");
        let k_1 = Fr::rand(&mut OsRng);
        let k_2 = Fr::rand(&mut OsRng);
        let k_3 = Fr::rand(&mut OsRng);
        let _dleqs = compute_output_dleqs(
            output_result.r_1,
            output_result.r_2,
            output_result.r_3,
            k_1,
            k_2,
            k_3,
            &ack_recipient,
            &ack_sender,
            metadata_hash,
        );
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "4S1O")],
        &times,
        None,
    ));

    // --- Output ---
    bench_runner::print_table(&results);
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/client/results/enrichment.csv");
    bench_runner::write_csv(&csv_path, &results);
}
