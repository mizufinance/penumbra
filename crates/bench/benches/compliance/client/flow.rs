//! Client flow benchmark: end-to-end transaction building.
//!
//! Measures total wall-clock time for a client to build a transaction,
//! including compliance enrichment, authorization, and proof generation
//! (serial vs parallel).
//!
//! v0 (vanilla): just proof generation (no enrichment, simpler circuits).
//! v0.1 (compliance): enrichment + auth + proof generation.
//!
//! Outputs: `benches/compliance/client/results/flow.csv`

use std::path::PathBuf;
use std::time::Instant;

use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_keys::test_keys;
use rand_core::OsRng;

#[path = "../helpers/flow.rs"]
mod flow_helpers;
use flow_helpers::*;

#[allow(unused_imports)]
#[path = "../helpers/vanilla_circuits.rs"]
mod vanilla_circuits;
use vanilla_circuits::*;

const WARMUP: usize = 1;
const SAMPLES: usize = 3;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("can create tokio runtime");
    let mut results = Vec::new();

    let fvk = &test_keys::FULL_VIEWING_KEY;
    let sk = &test_keys::SPEND_KEY;

    // --- Generate vanilla proving keys (one-time cost) ---
    eprintln!("Generating vanilla proving keys...");
    let _ = &*VANILLA_SPEND_KEYS;
    let _ = &*VANILLA_OUTPUT_KEYS;

    // ===== Simple Transfer (1S + 1O) =====
    eprintln!("\n=== Simple Transfer (1S + 1O) ===");

    // --- v0: vanilla build (1 spend proof + 1 output proof, serial) ---
    eprintln!("Benchmarking v0 simple_build_serial...");
    let (vanilla_spend, _) = make_vanilla_spend_circuit();
    let (vanilla_output, _) = make_vanilla_output_circuit();
    let vanilla_spend_pk = &VANILLA_SPEND_KEYS.0;
    let vanilla_output_pk = &VANILLA_OUTPUT_KEYS.0;
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _spend_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            vanilla_spend.clone(),
            vanilla_spend_pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("vanilla spend proof");
        let _output_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            vanilla_output.clone(),
            vanilla_output_pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("vanilla output proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("scenario", "1S1O"), ("stage", "build"), ("mode", "serial")],
        &times,
        None,
    ));

    // --- v0.1: compliance enrichment only ---
    eprintln!("Benchmarking v0.1 simple_enrich...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let (mut spend, mut output, _sct) = build_simple_transfer_unenriched();
        enrich_spend_for_test(&mut OsRng, &mut spend, &test_keys::ADDRESS_0);
        enrich_output_for_test(
            &mut OsRng,
            &mut output,
            &test_keys::ADDRESS_0,
            spend.note.asset_id(),
        );
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "1S1O"), ("stage", "enrich"), ("mode", "")],
        &times,
        None,
    ));

    // --- v0.1: compliance build serial ---
    eprintln!("Benchmarking v0.1 simple_build_serial...");
    let prepared = build_simple_transfer();
    let auth_data = prepared.plan.authorize(OsRng, sk).expect("can authorize");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _tx = prepared
            .plan
            .clone()
            .build(fvk, &prepared.witness_data, &auth_data)
            .expect("can build serial");
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "1S1O"), ("stage", "build"), ("mode", "serial")],
        &times,
        None,
    ));

    // --- v0.1: compliance build concurrent ---
    eprintln!("Benchmarking v0.1 simple_build_concurrent...");
    let prepared = build_simple_transfer();
    let auth_data = prepared.plan.authorize(OsRng, sk).expect("can authorize");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        rt.block_on(async {
            let _tx = prepared
                .plan
                .clone()
                .build_concurrent(fvk, &prepared.witness_data, &auth_data)
                .await
                .expect("can build concurrent");
        });
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[
            ("scenario", "1S1O"),
            ("stage", "build"),
            ("mode", "concurrent"),
        ],
        &times,
        None,
    ));

    // --- v0: vanilla total (just proof gen) ---
    eprintln!("Benchmarking v0 simple_total...");
    let (vanilla_spend, _) = make_vanilla_spend_circuit();
    let (vanilla_output, _) = make_vanilla_output_circuit();
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _spend_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            vanilla_spend.clone(),
            vanilla_spend_pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("vanilla spend proof");
        let _output_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            vanilla_output.clone(),
            vanilla_output_pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("vanilla output proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("scenario", "1S1O"), ("stage", "total"), ("mode", "serial")],
        &times,
        None,
    ));

    // --- v0.1: compliance total serial ---
    eprintln!("Benchmarking v0.1 simple_total_serial...");
    let mut total_times = Vec::with_capacity(SAMPLES);
    for _ in 0..WARMUP {
        let p = build_simple_transfer();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        let _tx = p.plan.build(fvk, &p.witness_data, &ad).unwrap();
    }
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let p = build_simple_transfer();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        let _tx = p.plan.build(fvk, &p.witness_data, &ad).unwrap();
        total_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "1S1O"), ("stage", "total"), ("mode", "serial")],
        &total_times,
        None,
    ));

    // --- v0.1: compliance total concurrent ---
    eprintln!("Benchmarking v0.1 simple_total_concurrent...");
    let mut total_times = Vec::with_capacity(SAMPLES);
    for _ in 0..WARMUP {
        let p = build_simple_transfer();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        rt.block_on(async {
            let _tx = p
                .plan
                .build_concurrent(fvk, &p.witness_data, &ad)
                .await
                .unwrap();
        });
    }
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let p = build_simple_transfer();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        rt.block_on(async {
            let _tx = p
                .plan
                .build_concurrent(fvk, &p.witness_data, &ad)
                .await
                .unwrap();
        });
        total_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    results.push(bench_runner::make_result(
        "v0.1",
        &[
            ("scenario", "1S1O"),
            ("stage", "total"),
            ("mode", "concurrent"),
        ],
        &total_times,
        None,
    ));

    // ===== Multi-Spend (4S + 1O) =====
    eprintln!("\n=== Multi-Spend (4S + 1O) ===");

    // --- v0: vanilla build (4 spend proofs + 1 output proof, serial) ---
    eprintln!("Benchmarking v0 multi4_build_serial...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        for _ in 0..4 {
            let (circuit, _) = make_vanilla_spend_circuit();
            let _proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
                circuit,
                vanilla_spend_pk,
                Fq::rand(&mut OsRng),
                Fq::rand(&mut OsRng),
            )
            .expect("vanilla spend proof");
        }
        let (circuit, _) = make_vanilla_output_circuit();
        let _proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit,
            vanilla_output_pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("vanilla output proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("scenario", "4S1O"), ("stage", "build"), ("mode", "serial")],
        &times,
        None,
    ));

    // --- v0.1: compliance enrichment only ---
    eprintln!("Benchmarking v0.1 multi4_enrich...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let (mut spends, mut output, _sct) = build_multi_spend_4x1_unenriched();
        for spend in &mut spends {
            enrich_spend_for_test(&mut OsRng, spend, &test_keys::ADDRESS_0);
        }
        let asset_id = output.value.asset_id;
        enrich_output_for_test(&mut OsRng, &mut output, &test_keys::ADDRESS_0, asset_id);
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "4S1O"), ("stage", "enrich"), ("mode", "")],
        &times,
        None,
    ));

    // --- v0.1: compliance build serial ---
    eprintln!("Benchmarking v0.1 multi4_build_serial...");
    let prepared = build_multi_spend_4x1();
    let auth_data = prepared.plan.authorize(OsRng, sk).expect("can authorize");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _tx = prepared
            .plan
            .clone()
            .build(fvk, &prepared.witness_data, &auth_data)
            .expect("can build serial");
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "4S1O"), ("stage", "build"), ("mode", "serial")],
        &times,
        None,
    ));

    // --- v0.1: compliance build concurrent ---
    eprintln!("Benchmarking v0.1 multi4_build_concurrent...");
    let prepared = build_multi_spend_4x1();
    let auth_data = prepared.plan.authorize(OsRng, sk).expect("can authorize");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        rt.block_on(async {
            let _tx = prepared
                .plan
                .clone()
                .build_concurrent(fvk, &prepared.witness_data, &auth_data)
                .await
                .expect("can build concurrent");
        });
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[
            ("scenario", "4S1O"),
            ("stage", "build"),
            ("mode", "concurrent"),
        ],
        &times,
        None,
    ));

    // --- v0: vanilla total (same as build, no enrichment) ---
    eprintln!("Benchmarking v0 multi4_total...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        for _ in 0..4 {
            let (circuit, _) = make_vanilla_spend_circuit();
            let _proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
                circuit,
                vanilla_spend_pk,
                Fq::rand(&mut OsRng),
                Fq::rand(&mut OsRng),
            )
            .expect("vanilla spend proof");
        }
        let (circuit, _) = make_vanilla_output_circuit();
        let _proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit,
            vanilla_output_pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("vanilla output proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("scenario", "4S1O"), ("stage", "total"), ("mode", "serial")],
        &times,
        None,
    ));

    // --- v0.1: compliance total serial ---
    eprintln!("Benchmarking v0.1 multi4_total_serial...");
    let mut total_times = Vec::with_capacity(SAMPLES);
    for _ in 0..WARMUP {
        let p = build_multi_spend_4x1();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        let _tx = p.plan.build(fvk, &p.witness_data, &ad).unwrap();
    }
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let p = build_multi_spend_4x1();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        let _tx = p.plan.build(fvk, &p.witness_data, &ad).unwrap();
        total_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    results.push(bench_runner::make_result(
        "v0.1",
        &[("scenario", "4S1O"), ("stage", "total"), ("mode", "serial")],
        &total_times,
        None,
    ));

    // --- v0.1: compliance total concurrent ---
    eprintln!("Benchmarking v0.1 multi4_total_concurrent...");
    let mut total_times = Vec::with_capacity(SAMPLES);
    for _ in 0..WARMUP {
        let p = build_multi_spend_4x1();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        rt.block_on(async {
            let _tx = p
                .plan
                .build_concurrent(fvk, &p.witness_data, &ad)
                .await
                .unwrap();
        });
    }
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let p = build_multi_spend_4x1();
        let ad = p.plan.authorize(OsRng, sk).unwrap();
        rt.block_on(async {
            let _tx = p
                .plan
                .build_concurrent(fvk, &p.witness_data, &ad)
                .await
                .unwrap();
        });
        total_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    results.push(bench_runner::make_result(
        "v0.1",
        &[
            ("scenario", "4S1O"),
            ("stage", "total"),
            ("mode", "concurrent"),
        ],
        &total_times,
        None,
    ));

    // --- Output ---
    bench_runner::print_table(&results);
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/client/results/flow.csv");
    bench_runner::write_csv(&csv_path, &results);
}
