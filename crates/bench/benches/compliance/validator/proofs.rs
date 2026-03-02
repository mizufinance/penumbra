//! Proof benchmarks: vanilla (v0) vs compliance (v0.1).
//!
//! Uses VanillaSpendCircuit and VanillaOutputCircuit from vanilla_circuits.rs
//! for v0 comparison. Generates proving keys on the fly for vanilla.
//! Uses bundled proving keys for compliance.
//!
//! Outputs: `benches/compliance/validator/results/proofs.csv`

use std::path::PathBuf;

use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use rand_core::OsRng;

use penumbra_sdk_asset::Balance;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_proof_params::{
    DummyWitness, OUTPUT_PROOF_PROVING_KEY, OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_PROVING_KEY,
    SPEND_PROOF_VERIFICATION_KEY,
};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::{
    generate_test_data, CircuitType,
};
use penumbra_sdk_shielded_pool::{
    output::{OutputProofPrivate, OutputProofPublic},
    OutputProof, SpendProof, SpendProofPrivate, SpendProofPublic,
};
use penumbra_sdk_tct as tct;

#[allow(unused_imports)]
#[path = "../helpers/vanilla_circuits.rs"]
mod vanilla_circuits;
use vanilla_circuits::*;

const WARMUP: usize = 1;
const SAMPLES: usize = 5;

// ===========================================================================
// Compliance helpers
// ===========================================================================

fn make_compliance_spend_inputs() -> (SpendProofPublic, SpendProofPrivate, Fq, Fq) {
    let mut rng = OsRng;
    let test_data = generate_test_data(&mut rng, 1, 100, false, CircuitType::Spend);

    let mut sct = tct::Tree::new();
    let note_commitment = test_data.note.commit();
    sct.insert(tct::Witness::Keep, note_commitment).unwrap();
    let anchor = sct.root();
    let state_commitment_proof = sct.witness(note_commitment).unwrap();

    let balance_commitment = Balance::from(test_data.value).commit(test_data.balance_blinding);
    let nullifier = Nullifier::derive(
        test_data.fvk.nullifier_key(),
        state_commitment_proof.position(),
        &note_commitment,
    );
    let randomizer = Fr::rand(&mut rng);
    let rk = test_data
        .fvk
        .spend_verification_key()
        .randomize(&randomizer);

    let dummy_nonce = Fr::from(0u64);
    let sender_leaf_hash =
        penumbra_sdk_compliance::blind_sender_leaf(test_data.user_leaf.commit(), dummy_nonce);

    let public = SpendProofPublic {
        anchor,
        balance_commitment,
        nullifier,
        rk,
        asset_anchor: test_data.asset_anchor,
        compliance_anchor: test_data.compliance_anchor,
        epk: test_data.epk_1,
        c2_core: test_data.c2_core,
        compliance_ciphertext: test_data.compliance_ciphertext,
        target_timestamp: Fq::from(test_data.target_timestamp),
        dleq_c: test_data.dleq_c,
        dleq_s: test_data.dleq_s,
        sender_leaf_hash,
    };

    let private = SpendProofPrivate {
        state_commitment_proof,
        note: test_data.note,
        v_blinding: test_data.balance_blinding,
        spend_auth_randomizer: randomizer,
        ak: *test_data.fvk.spend_verification_key(),
        nk: *test_data.fvk.nullifier_key(),
        asset_path: test_data.asset_path,
        asset_position: test_data.asset_position,
        asset_indexed_leaf: test_data.asset_indexed_leaf,
        is_regulated: false,
        compliance_path: test_data.compliance_path,
        compliance_position: test_data.compliance_position,
        user_leaf: test_data.user_leaf,
        compliance_ephemeral_secret: test_data.ephemeral_secret,
        tx_blinding_nonce: dummy_nonce,
        is_flagged: false,
        salt: test_data.salt,
    };

    let r = Fq::rand(&mut rng);
    let s = Fq::rand(&mut rng);
    (public, private, r, s)
}

fn make_compliance_output_inputs() -> (OutputProofPublic, OutputProofPrivate, Fq, Fq) {
    let mut rng = OsRng;
    let test_data = generate_test_data(&mut rng, 1, 100, false, CircuitType::Output);

    let note_commitment = test_data.note.commit();
    let balance_commitment = (-Balance::from(test_data.value)).commit(test_data.balance_blinding);

    let tx_blinding_nonce = Fr::from(0u64);
    let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(
        test_data.counterparty_leaf.commit(),
        tx_blinding_nonce,
    );

    let public = OutputProofPublic {
        balance_commitment,
        note_commitment,
        epk_1: test_data.epk_1,
        epk_2: test_data.epk_2.expect("output needs epk_2"),
        epk_3: test_data.epk_3.expect("output needs epk_3"),
        c2_core: test_data.c2_core,
        c2_ext: test_data.c2_ext.expect("output needs c2_ext"),
        c2_sext: test_data.c2_sext.expect("output needs c2_sext"),
        compliance_ciphertext: test_data.compliance_ciphertext,
        asset_anchor: test_data.asset_anchor,
        compliance_anchor: test_data.compliance_anchor,
        target_timestamp: Fq::from(test_data.target_timestamp),
        dleq_c_1: test_data.dleq_c,
        dleq_s_1: test_data.dleq_s,
        dleq_c_2: test_data.dleq_c_2,
        dleq_s_2: test_data.dleq_s_2,
        dleq_c_3: test_data.dleq_c_3,
        dleq_s_3: test_data.dleq_s_3,
        counterparty_leaf_hash,
    };

    let private = OutputProofPrivate {
        note: test_data.note,
        balance_blinding: test_data.balance_blinding,
        asset_path: test_data.asset_path,
        asset_position: test_data.asset_position,
        asset_indexed_leaf: test_data.asset_indexed_leaf,
        is_regulated: false,
        compliance_path: test_data.compliance_path,
        compliance_position: test_data.compliance_position,
        user_leaf: test_data.user_leaf,
        compliance_ephemeral_secret: test_data.ephemeral_secret,
        r_2: test_data.r_2.expect("output requires r_2"),
        r_3: test_data.r_3.expect("output requires r_3"),
        counterparty_leaf: test_data.counterparty_leaf,
        tx_blinding_nonce,
        is_flagged: false,
        salt: test_data.salt,
    };

    let r = Fq::rand(&mut rng);
    let s = Fq::rand(&mut rng);
    (public, private, r, s)
}

// ===========================================================================
// Main
// ===========================================================================

fn main() {
    use penumbra_sdk_shielded_pool::{OutputCircuit, SpendCircuit};

    let mut results = Vec::new();

    // --- Constraint counts ---
    eprintln!("Counting constraints...");
    let vanilla_spend_constraints = count_constraints(VanillaSpendCircuit::with_dummy_witness());
    let compliance_spend_constraints = count_constraints(SpendCircuit::with_dummy_witness());
    let vanilla_output_constraints = count_constraints(VanillaOutputCircuit::with_dummy_witness());
    let compliance_output_constraints = count_constraints(OutputCircuit::with_dummy_witness());

    eprintln!(
        "  Spend:  vanilla={}, compliance={} (+{:.1}%)",
        vanilla_spend_constraints,
        compliance_spend_constraints,
        (compliance_spend_constraints as f64 / vanilla_spend_constraints as f64 - 1.0) * 100.0,
    );
    eprintln!(
        "  Output: vanilla={}, compliance={} (+{:.1}%)",
        vanilla_output_constraints,
        compliance_output_constraints,
        (compliance_output_constraints as f64 / vanilla_output_constraints as f64 - 1.0) * 100.0,
    );

    // --- Vanilla key generation (triggers Lazy) ---
    eprintln!("Generating vanilla proving keys (one-time cost)...");
    let _ = &*VANILLA_SPEND_KEYS;
    let _ = &*VANILLA_OUTPUT_KEYS;

    // --- Spend Prove ---
    eprintln!("Benchmarking spend_prove...");

    // v0: vanilla spend prove
    let (vanilla_circuit, _vanilla_public_inputs) = make_vanilla_spend_circuit();
    let vanilla_pk = &VANILLA_SPEND_KEYS.0;
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let circuit = vanilla_circuit.clone();
        let r = Fq::rand(&mut OsRng);
        let s = Fq::rand(&mut OsRng);
        let _proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit, vanilla_pk, r, s,
        )
        .expect("can create vanilla spend proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("circuit", "spend"), ("operation", "prove")],
        &times,
        Some(vanilla_spend_constraints),
    ));

    // v0.1: compliance spend prove
    let (c_pub, c_priv, c_r, c_s) = make_compliance_spend_inputs();
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _proof = SpendProof::prove(
            c_r,
            c_s,
            &SPEND_PROOF_PROVING_KEY,
            c_pub.clone(),
            c_priv.clone(),
        )
        .expect("can create compliance spend proof");
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("circuit", "spend"), ("operation", "prove")],
        &times,
        Some(compliance_spend_constraints),
    ));

    // --- Spend Verify ---
    eprintln!("Benchmarking spend_verify...");

    // v0: vanilla spend verify
    let (vanilla_circuit, vanilla_public_inputs) = make_vanilla_spend_circuit();
    let vanilla_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
        vanilla_circuit,
        vanilla_pk,
        Fq::rand(&mut OsRng),
        Fq::rand(&mut OsRng),
    )
    .expect("can create vanilla proof for verify bench");
    let vanilla_pvk = &VANILLA_SPEND_KEYS.1;
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            vanilla_pvk,
            &vanilla_public_inputs,
            &vanilla_proof,
        )
        .expect("valid vanilla spend proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("circuit", "spend"), ("operation", "verify")],
        &times,
        Some(vanilla_spend_constraints),
    ));

    // v0.1: compliance spend verify
    let (c_pub, c_priv, c_r, c_s) = make_compliance_spend_inputs();
    let c_proof = SpendProof::prove(c_r, c_s, &SPEND_PROOF_PROVING_KEY, c_pub.clone(), c_priv)
        .expect("can create compliance proof for verify bench");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        c_proof
            .verify(&SPEND_PROOF_VERIFICATION_KEY, c_pub.clone())
            .expect("valid compliance spend proof");
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("circuit", "spend"), ("operation", "verify")],
        &times,
        Some(compliance_spend_constraints),
    ));

    // --- Output Prove ---
    eprintln!("Benchmarking output_prove...");

    // v0: vanilla output prove
    let (vanilla_out_circuit, _vanilla_out_inputs) = make_vanilla_output_circuit();
    let vanilla_out_pk = &VANILLA_OUTPUT_KEYS.0;
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let circuit = vanilla_out_circuit.clone();
        let r = Fq::rand(&mut OsRng);
        let s = Fq::rand(&mut OsRng);
        let _proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit,
            vanilla_out_pk,
            r,
            s,
        )
        .expect("can create vanilla output proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("circuit", "output"), ("operation", "prove")],
        &times,
        Some(vanilla_output_constraints),
    ));

    // v0.1: compliance output prove
    let (o_pub, o_priv, o_r, o_s) = make_compliance_output_inputs();
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _proof = OutputProof::prove(
            o_r,
            o_s,
            &OUTPUT_PROOF_PROVING_KEY,
            o_pub.clone(),
            o_priv.clone(),
        )
        .expect("can create compliance output proof");
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("circuit", "output"), ("operation", "prove")],
        &times,
        Some(compliance_output_constraints),
    ));

    // --- Output Verify ---
    eprintln!("Benchmarking output_verify...");

    // v0: vanilla output verify
    let (vanilla_out_circuit, vanilla_out_inputs) = make_vanilla_output_circuit();
    let vanilla_out_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
        vanilla_out_circuit,
        vanilla_out_pk,
        Fq::rand(&mut OsRng),
        Fq::rand(&mut OsRng),
    )
    .expect("can create vanilla output proof for verify bench");
    let vanilla_out_pvk = &VANILLA_OUTPUT_KEYS.1;
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            vanilla_out_pvk,
            &vanilla_out_inputs,
            &vanilla_out_proof,
        )
        .expect("valid vanilla output proof");
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("circuit", "output"), ("operation", "verify")],
        &times,
        Some(vanilla_output_constraints),
    ));

    // v0.1: compliance output verify
    let (o_pub, o_priv, o_r, o_s) = make_compliance_output_inputs();
    let o_proof = OutputProof::prove(o_r, o_s, &OUTPUT_PROOF_PROVING_KEY, o_pub.clone(), o_priv)
        .expect("can create compliance output proof for verify bench");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        o_proof
            .verify(&OUTPUT_PROOF_VERIFICATION_KEY, o_pub.clone())
            .expect("valid compliance output proof");
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("circuit", "output"), ("operation", "verify")],
        &times,
        Some(compliance_output_constraints),
    ));

    // --- Output ---
    bench_runner::print_table(&results);

    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/validator/results/proofs.csv");
    bench_runner::write_csv(&csv_path, &results);
}
