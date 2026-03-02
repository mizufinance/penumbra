//! Validator flow benchmark: batch transaction verification.
//!
//! Measures total wall-clock time for a validator to verify a batch of
//! transactions (100 TXs), replicating the check_stateless pipeline.
//!
//! v0 (vanilla): proof verification only (no ciphertext, no DLEQ).
//! v0.1 (compliance): full pipeline (binding sig + ciphertext deser + DLEQ + proof verify).
//!
//! Outputs: `benches/compliance/validator/results/flow.csv`

use std::path::PathBuf;

use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::structs::ComplianceCiphertext;
use penumbra_sdk_keys::test_keys;
use penumbra_sdk_proof_params::{OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY};
use penumbra_sdk_shielded_pool::{output::OutputProofPublic, Output, Spend, SpendProofPublic};
use penumbra_sdk_transaction::txhash::{AuthorizingData, EffectingData};
use penumbra_sdk_transaction::{Action, Transaction};
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
const BATCH_SIZE: usize = 100;

// ===========================================================================
// Compliance verification helpers
// ===========================================================================

fn verify_binding_sig(tx: &Transaction) {
    let auth_hash = tx.auth_hash();
    tx.binding_verification_key()
        .verify(auth_hash.as_bytes(), tx.binding_sig())
        .expect("binding sig valid");
}

fn verify_spend(spend: &Spend, anchor: penumbra_sdk_tct::Root, effect_hash: &[u8]) {
    spend
        .body
        .rk
        .verify(effect_hash, &spend.auth_sig)
        .expect("spend auth sig valid");

    let ct = ComplianceCiphertext::from_bytes(&spend.body.compliance_ciphertext)
        .expect("valid ciphertext");
    let (epk, c2_core, compliance_ciphertext) = ct.to_spend_circuit_public_inputs();

    let (dleq_c, dleq_s) = if spend.body.dleq_proof.len() == 64 {
        let c_bytes: [u8; 32] = spend.body.dleq_proof[..32].try_into().unwrap();
        let c = Fq::from_bytes_checked(&c_bytes).expect("valid dleq_c");
        let s_bytes: [u8; 32] = spend.body.dleq_proof[32..64].try_into().unwrap();
        let s = Fq::from_bytes_checked(&s_bytes).expect("valid dleq_s");
        (c, s)
    } else {
        (Fq::from(0u64), Fq::from(0u64))
    };

    spend
        .proof
        .verify(
            &SPEND_PROOF_VERIFICATION_KEY,
            SpendProofPublic {
                anchor,
                balance_commitment: spend.body.balance_commitment,
                nullifier: spend.body.nullifier,
                rk: spend.body.rk,
                asset_anchor: spend.body.asset_anchor,
                compliance_anchor: spend.body.compliance_anchor,
                epk,
                c2_core,
                compliance_ciphertext,
                target_timestamp: Fq::from(spend.body.target_timestamp),
                dleq_c,
                dleq_s,
                sender_leaf_hash: spend.body.sender_leaf_hash,
            },
        )
        .expect("valid spend proof");
}

fn verify_output(output: &Output) {
    let ct = ComplianceCiphertext::from_bytes(&output.body.compliance_ciphertext)
        .expect("valid ciphertext");
    let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
        ct.to_output_circuit_public_inputs();

    let (dleq_c_1, dleq_s_1, dleq_c_2, dleq_s_2, dleq_c_3, dleq_s_3) =
        if output.body.dleq_proofs.len() == 192 {
            let parse = |offset: usize| -> Fq {
                let bytes: [u8; 32] = output.body.dleq_proofs[offset..offset + 32]
                    .try_into()
                    .unwrap();
                Fq::from_bytes_checked(&bytes).expect("valid dleq field")
            };
            (
                parse(0),
                parse(32),
                parse(64),
                parse(96),
                parse(128),
                parse(160),
            )
        } else {
            (
                Fq::from(0u64),
                Fq::from(0u64),
                Fq::from(0u64),
                Fq::from(0u64),
                Fq::from(0u64),
                Fq::from(0u64),
            )
        };

    output
        .proof
        .verify(
            &OUTPUT_PROOF_VERIFICATION_KEY,
            OutputProofPublic {
                balance_commitment: output.body.balance_commitment,
                note_commitment: output.body.note_payload.note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext,
                target_timestamp: Fq::from(output.body.target_timestamp),
                dleq_c_1,
                dleq_s_1,
                dleq_c_2,
                dleq_s_2,
                dleq_c_3,
                dleq_s_3,
                asset_anchor: output.body.asset_anchor,
                compliance_anchor: output.body.compliance_anchor,
                counterparty_leaf_hash: output.body.counterparty_leaf_hash,
            },
        )
        .expect("valid output proof");
}

/// Serial: binding sig + actions one by one.
fn verify_full_tx_serial(tx: &Transaction) {
    verify_binding_sig(tx);
    let anchor = tx.anchor;
    let effect_hash = tx.effect_hash();
    for action in tx.actions() {
        match action {
            Action::Spend(spend) => verify_spend(spend, anchor, effect_hash.as_ref()),
            Action::Output(output) => verify_output(output),
            _ => {}
        }
    }
}

/// Production flow: binding sig first, then actions in parallel via JoinSet.
/// Matches check_stateless in transaction.rs.
async fn verify_full_tx_parallel(tx: &Transaction) {
    verify_binding_sig(tx);
    let anchor = tx.anchor;
    let effect_hash = tx.effect_hash();
    let effect_hash_bytes: Vec<u8> = effect_hash.as_ref().to_vec();

    let mut action_checks = tokio::task::JoinSet::new();
    for action in tx.actions().cloned() {
        let eh = effect_hash_bytes.clone();
        action_checks.spawn_blocking(move || match &action {
            Action::Spend(spend) => verify_spend(spend, anchor, &eh),
            Action::Output(output) => verify_output(output),
            _ => {}
        });
    }
    while let Some(result) = action_checks.join_next().await {
        result.expect("action check succeeded");
    }
}

// ===========================================================================
// Vanilla verification helpers
// ===========================================================================

struct VanillaProofSet {
    spend_proof: Proof<Bls12_377>,
    spend_public_inputs: Vec<Fq>,
    output_proof: Proof<Bls12_377>,
    output_public_inputs: Vec<Fq>,
}

fn build_vanilla_proof_set() -> VanillaProofSet {
    let vanilla_spend_pk = &VANILLA_SPEND_KEYS.0;
    let vanilla_output_pk = &VANILLA_OUTPUT_KEYS.0;

    let (spend_circuit, spend_inputs) = make_vanilla_spend_circuit();
    let spend_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
        spend_circuit,
        vanilla_spend_pk,
        Fq::rand(&mut OsRng),
        Fq::rand(&mut OsRng),
    )
    .expect("vanilla spend proof");

    let (output_circuit, output_inputs) = make_vanilla_output_circuit();
    let output_proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
        output_circuit,
        vanilla_output_pk,
        Fq::rand(&mut OsRng),
        Fq::rand(&mut OsRng),
    )
    .expect("vanilla output proof");

    VanillaProofSet {
        spend_proof,
        spend_public_inputs: spend_inputs,
        output_proof,
        output_public_inputs: output_inputs,
    }
}

fn verify_vanilla_proof_set(
    proofs: &VanillaProofSet,
    spend_pvk: &PreparedVerifyingKey<Bls12_377>,
    output_pvk: &PreparedVerifyingKey<Bls12_377>,
) {
    Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
        spend_pvk,
        &proofs.spend_public_inputs,
        &proofs.spend_proof,
    )
    .expect("valid vanilla spend proof");
    Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
        output_pvk,
        &proofs.output_public_inputs,
        &proofs.output_proof,
    )
    .expect("valid vanilla output proof");
}

// ===========================================================================
// Transaction builder
// ===========================================================================

fn build_transaction(prepared: &PreparedPlan) -> Transaction {
    let rt = tokio::runtime::Runtime::new().expect("can create runtime");
    let fvk = &test_keys::FULL_VIEWING_KEY;
    let sk = &test_keys::SPEND_KEY;
    let auth_data = prepared.plan.authorize(OsRng, sk).expect("can authorize");
    rt.block_on(async {
        prepared
            .plan
            .clone()
            .build_concurrent(fvk, &prepared.witness_data, &auth_data)
            .await
            .expect("can build transaction")
    })
}

// ===========================================================================
// Main
// ===========================================================================

/// Vanilla parallel: spawn spend + output verification concurrently.
async fn verify_vanilla_proof_set_parallel(
    proofs: &VanillaProofSet,
    spend_pvk: &'static PreparedVerifyingKey<Bls12_377>,
    output_pvk: &'static PreparedVerifyingKey<Bls12_377>,
) {
    let sp = proofs.spend_proof.clone();
    let si = proofs.spend_public_inputs.clone();
    let op = proofs.output_proof.clone();
    let oi = proofs.output_public_inputs.clone();

    let spend_handle = tokio::task::spawn_blocking(move || {
        Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(spend_pvk, &si, &sp)
            .expect("valid vanilla spend proof");
    });
    let output_handle = tokio::task::spawn_blocking(move || {
        Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(output_pvk, &oi, &op)
            .expect("valid vanilla output proof");
    });

    spend_handle.await.expect("spend task ok");
    output_handle.await.expect("output task ok");
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("can create runtime");
    let mut results = Vec::new();

    // --- Generate vanilla proving keys ---
    eprintln!("Generating vanilla proving keys...");
    let _ = &*VANILLA_SPEND_KEYS;
    let _ = &*VANILLA_OUTPUT_KEYS;
    let vanilla_spend_pvk = &VANILLA_SPEND_KEYS.1;
    let vanilla_output_pvk = &VANILLA_OUTPUT_KEYS.1;

    // ===== Build proof/TX pools =====
    eprintln!("Building {} vanilla proof sets...", BATCH_SIZE);
    let vanilla_proofs: Vec<_> = (0..BATCH_SIZE).map(|_| build_vanilla_proof_set()).collect();

    eprintln!("Building {} compliance transactions...", BATCH_SIZE);
    let compliance_txs: Vec<_> = (0..BATCH_SIZE)
        .map(|_| {
            let prepared = build_simple_transfer();
            build_transaction(&prepared)
        })
        .collect();

    // ===== Serial batch (total CPU cost) =====
    eprintln!("\n=== Serial Batch {} TXs (1S + 1O each) ===", BATCH_SIZE);

    eprintln!("Benchmarking v0 batch_{}_serial...", BATCH_SIZE);
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        for proofs in &vanilla_proofs {
            verify_vanilla_proof_set(proofs, vanilla_spend_pvk, vanilla_output_pvk);
        }
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("batch_size", "100"), ("mode", "serial")],
        &times,
        None,
    ));

    eprintln!("Benchmarking v0.1 batch_{}_serial...", BATCH_SIZE);
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        for tx in &compliance_txs {
            verify_full_tx_serial(tx);
        }
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("batch_size", "100"), ("mode", "serial")],
        &times,
        None,
    ));

    // ===== Production flow: TXs sequential, actions parallel within each TX =====
    // Matches ProcessProposal: for tx in block { deliver_tx(tx).await }
    // Inside deliver_tx: check_stateless uses JoinSet for actions
    eprintln!(
        "\n=== Production Flow: {} TXs sequential, actions parallel ===",
        BATCH_SIZE
    );

    eprintln!("Benchmarking v0 batch_{}_parallel...", BATCH_SIZE);
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        rt.block_on(async {
            for proofs in &vanilla_proofs {
                verify_vanilla_proof_set_parallel(proofs, vanilla_spend_pvk, vanilla_output_pvk)
                    .await;
            }
        });
    });
    results.push(bench_runner::make_result(
        "v0",
        &[("batch_size", "100"), ("mode", "parallel")],
        &times,
        None,
    ));

    eprintln!("Benchmarking v0.1 batch_{}_parallel...", BATCH_SIZE);
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        rt.block_on(async {
            for tx in &compliance_txs {
                verify_full_tx_parallel(tx).await;
            }
        });
    });
    results.push(bench_runner::make_result(
        "v0.1",
        &[("batch_size", "100"), ("mode", "parallel")],
        &times,
        None,
    ));

    // --- Per-TX averages from both modes ---
    eprintln!("\n=== Per-TX Averages ===");

    let v0_serial = bench_runner::run_bench(WARMUP, SAMPLES, || {
        for proofs in &vanilla_proofs {
            verify_vanilla_proof_set(proofs, vanilla_spend_pvk, vanilla_output_pvk);
        }
    });
    let v0_per_tx: Vec<f64> = v0_serial.iter().map(|t| t / BATCH_SIZE as f64).collect();
    results.push(bench_runner::make_result(
        "v0",
        &[("batch_size", "per_tx"), ("mode", "serial")],
        &v0_per_tx,
        None,
    ));

    let v01_serial = bench_runner::run_bench(WARMUP, SAMPLES, || {
        for tx in &compliance_txs {
            verify_full_tx_serial(tx);
        }
    });
    let v01_per_tx: Vec<f64> = v01_serial.iter().map(|t| t / BATCH_SIZE as f64).collect();
    results.push(bench_runner::make_result(
        "v0.1",
        &[("batch_size", "per_tx"), ("mode", "serial")],
        &v01_per_tx,
        None,
    ));

    let v0_parallel = bench_runner::run_bench(WARMUP, SAMPLES, || {
        rt.block_on(async {
            for proofs in &vanilla_proofs {
                verify_vanilla_proof_set_parallel(proofs, vanilla_spend_pvk, vanilla_output_pvk)
                    .await;
            }
        });
    });
    let v0_per_tx_p: Vec<f64> = v0_parallel.iter().map(|t| t / BATCH_SIZE as f64).collect();
    results.push(bench_runner::make_result(
        "v0",
        &[("batch_size", "per_tx"), ("mode", "parallel")],
        &v0_per_tx_p,
        None,
    ));

    let v01_parallel = bench_runner::run_bench(WARMUP, SAMPLES, || {
        rt.block_on(async {
            for tx in &compliance_txs {
                verify_full_tx_parallel(tx).await;
            }
        });
    });
    let v01_per_tx_p: Vec<f64> = v01_parallel.iter().map(|t| t / BATCH_SIZE as f64).collect();
    results.push(bench_runner::make_result(
        "v0.1",
        &[("batch_size", "per_tx"), ("mode", "parallel")],
        &v01_per_tx_p,
        None,
    ));

    // --- Output ---
    bench_runner::print_table(&results);
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/validator/results/flow.csv");
    bench_runner::write_csv(&csv_path, &results);
}
