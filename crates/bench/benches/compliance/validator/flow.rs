//! Validator flow benchmark: batch transaction verification.
//!
//! Measures total wall-clock time for a validator to verify a batch of
//! transactions (100 TXs), replicating the check_stateless pipeline.
//!
//! Emits:
//! - top-level overview: `benches/compliance/flows.csv`
//! - category KPIs: `benches/compliance/validator/validator.csv`
//! - section overview: `benches/compliance/validator/sections.csv`
//! - section KPIs: `benches/compliance/validator/sections/<section>.csv`

use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::structs::ComplianceCiphertext;
use penumbra_sdk_keys::test_keys;
use penumbra_sdk_proof_params::{OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY};
use penumbra_sdk_shielded_pool::{output::OutputProofPublic, Output, Spend, SpendProofPublic};
use penumbra_sdk_transaction::txhash::{AuthorizingData, EffectingData};
use penumbra_sdk_transaction::{Action, Transaction};

#[path = "../helpers/flow.rs"]
mod flow_helpers;
use flow_helpers::*;

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
        let c = decaf377::Fq::from_bytes_checked(&c_bytes).expect("valid dleq_c");
        let s_bytes: [u8; 32] = spend.body.dleq_proof[32..64].try_into().unwrap();
        let s = decaf377::Fq::from_bytes_checked(&s_bytes).expect("valid dleq_s");
        (c, s)
    } else {
        (decaf377::Fq::from(0u64), decaf377::Fq::from(0u64))
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
                target_timestamp: decaf377::Fq::from(spend.body.target_timestamp),
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
            let parse = |offset: usize| -> decaf377::Fq {
                let bytes: [u8; 32] = output.body.dleq_proofs[offset..offset + 32]
                    .try_into()
                    .unwrap();
                decaf377::Fq::from_bytes_checked(&bytes).expect("valid dleq field")
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
                decaf377::Fq::from(0u64),
                decaf377::Fq::from(0u64),
                decaf377::Fq::from(0u64),
                decaf377::Fq::from(0u64),
                decaf377::Fq::from(0u64),
                decaf377::Fq::from(0u64),
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
                target_timestamp: decaf377::Fq::from(output.body.target_timestamp),
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

/// Serial section benchmark: only spend verification path.
fn verify_spend_only_serial(tx: &Transaction) {
    let anchor = tx.anchor;
    let effect_hash = tx.effect_hash();
    for action in tx.actions() {
        if let Action::Spend(spend) = action {
            verify_spend(spend, anchor, effect_hash.as_ref());
        }
    }
}

/// Serial section benchmark: only output verification path.
fn verify_output_only_serial(tx: &Transaction) {
    for action in tx.actions() {
        if let Action::Output(output) = action {
            verify_output(output);
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
// Transaction builder
// ===========================================================================

fn build_transaction(prepared: &PreparedPlan) -> Transaction {
    let rt = tokio::runtime::Runtime::new().expect("can create runtime");
    let fvk = &test_keys::FULL_VIEWING_KEY;
    let sk = &test_keys::SPEND_KEY;
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

fn pick_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    batch_size: &str,
    mode: &str,
) -> bench_runner::BenchResult {
    raw.iter()
        .find(|r| {
            r.version == version
                && r.dimensions
                    .iter()
                    .any(|(k, v)| k == "batch_size" && v == batch_size)
                && r.dimensions.iter().any(|(k, v)| k == "mode" && v == mode)
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!("missing KPI row for version={version}, batch_size={batch_size}, mode={mode}")
        })
}

fn flow_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    // Complete suite: full flow matrix. Regression suite: canonical flow rows.
    let kpis = if regression {
        vec![("100", "parallel"), ("per_tx", "parallel")]
    } else {
        vec![
            ("100", "serial"),
            ("100", "parallel"),
            ("per_tx", "serial"),
            ("per_tx", "parallel"),
            ("1", "serial"),
            ("1", "parallel"),
            ("prove_kpi", "single"),
            ("ratio", "parallel_over_serial"),
        ]
    };
    for (batch_size, mode) in kpis {
        rows.push(pick_kpi(raw, version, batch_size, mode));
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
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    for section in ["binding_sig", "spend_path", "output_path"] {
        rows.push(pick_section_kpi(raw, version, "100", section));
    }
    rows
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("can create runtime");
    let version = bench_runner::bench_version();
    let warmup = bench_runner::warmup_count();
    let samples = bench_runner::sample_count();
    let regression = bench_runner::is_regression_suite();

    let txs: Vec<_> = (0..BATCH_SIZE)
        .map(|_| {
            let prepared = build_simple_transfer();
            build_transaction(&prepared)
        })
        .collect();

    let mut raw_results = Vec::new();

    let parallel_times = bench_runner::run_bench(warmup, samples, || {
        rt.block_on(async {
            for tx in &txs {
                verify_full_tx_parallel(tx).await;
            }
        });
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[("batch_size", "100"), ("mode", "parallel")],
        &parallel_times,
        None,
    ));

    let per_tx_parallel: Vec<f64> = parallel_times
        .iter()
        .map(|t| t / BATCH_SIZE as f64)
        .collect();
    raw_results.push(bench_runner::make_result(
        &version,
        &[("batch_size", "per_tx"), ("mode", "parallel")],
        &per_tx_parallel,
        None,
    ));

    // Section breakdown rows.
    let binding_only = bench_runner::run_bench(warmup, samples, || {
        for tx in &txs {
            verify_binding_sig(tx);
        }
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[("batch_size", "100"), ("section", "binding_sig")],
        &binding_only,
        None,
    ));

    let spend_only = bench_runner::run_bench(warmup, samples, || {
        for tx in &txs {
            verify_spend_only_serial(tx);
        }
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[("batch_size", "100"), ("section", "spend_path")],
        &spend_only,
        None,
    ));

    let output_only = bench_runner::run_bench(warmup, samples, || {
        for tx in &txs {
            verify_output_only_serial(tx);
        }
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[("batch_size", "100"), ("section", "output_path")],
        &output_only,
        None,
    ));

    if !regression {
        let serial_times = bench_runner::run_bench(warmup, samples, || {
            for tx in &txs {
                verify_full_tx_serial(tx);
            }
        });
        raw_results.push(bench_runner::make_result(
            &version,
            &[("batch_size", "100"), ("mode", "serial")],
            &serial_times,
            None,
        ));

        let per_tx_serial: Vec<f64> = serial_times.iter().map(|t| t / BATCH_SIZE as f64).collect();
        raw_results.push(bench_runner::make_result(
            &version,
            &[("batch_size", "per_tx"), ("mode", "serial")],
            &per_tx_serial,
            None,
        ));

        let single_serial = bench_runner::run_bench(warmup, samples, || {
            verify_full_tx_serial(&txs[0]);
        });
        raw_results.push(bench_runner::make_result(
            &version,
            &[("batch_size", "1"), ("mode", "serial")],
            &single_serial,
            None,
        ));

        let single_parallel = bench_runner::run_bench(warmup, samples, || {
            rt.block_on(async {
                verify_full_tx_parallel(&txs[0]).await;
            });
        });
        raw_results.push(bench_runner::make_result(
            &version,
            &[("batch_size", "1"), ("mode", "parallel")],
            &single_parallel,
            None,
        ));

        let ratio: Vec<f64> = parallel_times
            .iter()
            .zip(serial_times.iter())
            .map(|(p, s)| if *s > 0.0 { p / s } else { 0.0 })
            .collect();
        raw_results.push(bench_runner::make_result(
            &version,
            &[("batch_size", "ratio"), ("mode", "parallel_over_serial")],
            &ratio,
            None,
        ));

        // One proving KPI for startup runs.
        let prove_times = bench_runner::run_bench(warmup, samples, || {
            let prepared = build_simple_transfer();
            let _ = build_transaction(&prepared);
        });
        raw_results.push(bench_runner::make_result(
            &version,
            &[("batch_size", "prove_kpi"), ("mode", "single")],
            &prove_times,
            None,
        ));
    }

    let flow_rows = flow_rows_for_version(&raw_results, &version, regression);
    let section_rows = section_rows_for_version(&raw_results, &version);
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

    for section in ["binding_sig", "spend_path", "output_path"] {
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
        let section_path = bench_runner::section_csv_path("validator", section);
        bench_runner::append_csv(&section_path, &rows);
    }
}
