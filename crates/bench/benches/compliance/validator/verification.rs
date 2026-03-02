//! Validator-side benchmarks: vanilla (v0) vs compliance (v0.1).
//!
//! v0: proof verification only (no ciphertext, no DLEQ).
//! v0.1: full path (ciphertext deserialize + DLEQ parse + proof verify).
//!
//! Outputs: `benches/compliance/validator/results/verification.csv`

use std::path::PathBuf;

use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use rand_core::OsRng;

use penumbra_sdk_asset::Balance;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::structs::{
    ComplianceCiphertext, DleqProof, OUTPUT_WIRE_BYTES, SPEND_WIRE_BYTES,
};
use penumbra_sdk_proof_params::{
    OUTPUT_PROOF_PROVING_KEY, OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_PROVING_KEY,
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

const WARMUP: usize = 1;
const SAMPLES: usize = 10;

// ===========================================================================
// Compliance action builders (reused from previous version)
// ===========================================================================

fn build_spend_action() -> (SpendProof, SpendProofPublic, Vec<u8>) {
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

    let ct_bytes = test_data.compliance_ciphertext_bytes.clone();

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
    let proof = SpendProof::prove(r, s, &SPEND_PROOF_PROVING_KEY, public.clone(), private).unwrap();

    (proof, public, ct_bytes)
}

fn build_output_action() -> (OutputProof, OutputProofPublic, Vec<u8>) {
    let mut rng = OsRng;
    let test_data = generate_test_data(&mut rng, 1, 100, false, CircuitType::Output);

    let note_commitment = test_data.note.commit();
    let balance_commitment = (-Balance::from(test_data.value)).commit(test_data.balance_blinding);

    let tx_blinding_nonce = Fr::from(0u64);
    let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(
        test_data.counterparty_leaf.commit(),
        tx_blinding_nonce,
    );

    let ct_bytes = test_data.compliance_ciphertext_bytes.clone();

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
    let proof =
        OutputProof::prove(r, s, &OUTPUT_PROOF_PROVING_KEY, public.clone(), private).unwrap();

    (proof, public, ct_bytes)
}

// ===========================================================================
// Main
// ===========================================================================

fn main() {
    let mut results = Vec::new();

    // --- Generate vanilla keys ---
    eprintln!("Generating vanilla proving keys (one-time cost)...");
    let _ = &*vanilla_circuits::VANILLA_SPEND_KEYS;
    let _ = &*vanilla_circuits::VANILLA_OUTPUT_KEYS;

    // ===== Spend =====

    // --- v0 spend_verify ---
    eprintln!("Benchmarking v0 spend_verify...");
    {
        let (circuit, public_inputs) = vanilla_circuits::make_vanilla_spend_circuit();
        let pk = &vanilla_circuits::VANILLA_SPEND_KEYS.0;
        let pvk = &vanilla_circuits::VANILLA_SPEND_KEYS.1;
        let proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit,
            pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("can create vanilla spend proof");

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                pvk,
                &public_inputs,
                &proof,
            )
            .expect("valid vanilla spend proof");
        });
        results.push(bench_runner::make_result(
            "v0",
            &[("circuit", "spend"), ("stage", "verify")],
            &times,
            None,
        ));
    }

    // --- v0.1 spend_verify ---
    eprintln!("Benchmarking v0.1 spend_verify...");
    {
        let (proof, public, _ct_bytes) = build_spend_action();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            proof
                .verify(&SPEND_PROOF_VERIFICATION_KEY, public.clone())
                .expect("valid compliance spend proof");
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", "spend"), ("stage", "verify")],
            &times,
            None,
        ));
    }

    // --- v0.1 spend_ct_deserialize ---
    eprintln!("Benchmarking v0.1 spend_ct_deserialize...");
    {
        let (_, _, ct_bytes) = build_spend_action();
        assert_eq!(ct_bytes.len(), SPEND_WIRE_BYTES);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let ct = ComplianceCiphertext::from_bytes(&ct_bytes).expect("valid");
            let _ = ct.to_spend_circuit_public_inputs();
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", "spend"), ("stage", "ct_deserialize")],
            &times,
            None,
        ));
    }

    // --- v0.1 spend_full_verify ---
    eprintln!("Benchmarking v0.1 spend_full_verify...");
    {
        let (proof, public, ct_bytes) = build_spend_action();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _ct = ComplianceCiphertext::from_bytes(&ct_bytes).expect("valid");
            proof
                .verify(&SPEND_PROOF_VERIFICATION_KEY, public.clone())
                .expect("valid proof");
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", "spend"), ("stage", "full_verify")],
            &times,
            None,
        ));
    }

    // ===== Output =====

    // --- v0 output_verify ---
    eprintln!("Benchmarking v0 output_verify...");
    {
        let (circuit, public_inputs) = vanilla_circuits::make_vanilla_output_circuit();
        let pk = &vanilla_circuits::VANILLA_OUTPUT_KEYS.0;
        let pvk = &vanilla_circuits::VANILLA_OUTPUT_KEYS.1;
        let proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit,
            pk,
            Fq::rand(&mut OsRng),
            Fq::rand(&mut OsRng),
        )
        .expect("can create vanilla output proof");

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                pvk,
                &public_inputs,
                &proof,
            )
            .expect("valid vanilla output proof");
        });
        results.push(bench_runner::make_result(
            "v0",
            &[("circuit", "output"), ("stage", "verify")],
            &times,
            None,
        ));
    }

    // --- v0.1 output_verify ---
    eprintln!("Benchmarking v0.1 output_verify...");
    {
        let (proof, public, _ct_bytes) = build_output_action();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            proof
                .verify(&OUTPUT_PROOF_VERIFICATION_KEY, public.clone())
                .expect("valid compliance output proof");
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", "output"), ("stage", "verify")],
            &times,
            None,
        ));
    }

    // --- v0.1 output_ct_deserialize ---
    eprintln!("Benchmarking v0.1 output_ct_deserialize...");
    {
        let (_, _, ct_bytes) = build_output_action();
        assert_eq!(ct_bytes.len(), OUTPUT_WIRE_BYTES);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let ct = ComplianceCiphertext::from_bytes(&ct_bytes).expect("valid");
            let _ = ct.to_output_circuit_public_inputs();
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", "output"), ("stage", "ct_deserialize")],
            &times,
            None,
        ));
    }

    // --- v0.1 output_full_verify ---
    eprintln!("Benchmarking v0.1 output_full_verify...");
    {
        let (proof, public, ct_bytes) = build_output_action();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _ct = ComplianceCiphertext::from_bytes(&ct_bytes).expect("valid");
            proof
                .verify(&OUTPUT_PROOF_VERIFICATION_KEY, public.clone())
                .expect("valid proof");
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", "output"), ("stage", "full_verify")],
            &times,
            None,
        ));
    }

    // --- v0.1 dleq_parse ---
    eprintln!("Benchmarking v0.1 dleq_parse...");
    {
        let mut rng = OsRng;
        let proof = DleqProof {
            c: Fq::rand(&mut rng),
            s: Fr::rand(&mut rng),
        };
        let bytes = proof.to_bytes();

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _p = DleqProof::from_bytes(&bytes);
        });
        results.push(bench_runner::make_result(
            "v0.1",
            &[("circuit", ""), ("stage", "dleq_parse")],
            &times,
            None,
        ));
    }

    // --- Output ---
    bench_runner::print_table(&results);
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/validator/results/verification.csv");
    bench_runner::write_csv(&csv_path, &results);
}
