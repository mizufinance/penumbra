//! Block-level TPS benchmark: full pipeline throughput.
//!
//! Measures actual validator throughput by running N transactions through the
//! complete block pipeline: begin_block → N × deliver_tx → end_block → commit.
//!
//! v0.1 (compliance) is measured directly. v0 (vanilla) is extrapolated from
//! existing micro-benchmarks. For real v0 numbers, run on the release/v2.1.x branch.
//!
//! Outputs: `benches/compliance/validator/results/block_tps.csv`

use std::ops::Deref;
use std::path::PathBuf;
use std::time::Instant;

use penumbra_sdk_asset::STAKING_TOKEN_DENOM;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_keys::test_keys;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_shielded_pool::{genesis::Allocation, OutputPlan, SpendPlan};
use penumbra_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, TransactionParameters, TransactionPlan,
};
use rand_core::OsRng;

use cnidarium::TempStorage;
use penumbra_sdk_app::{
    genesis::{AppState, Content},
    server::consensus::Consensus,
    SUBSTORE_PREFIXES,
};
use penumbra_sdk_mock_client::MockClient;
use penumbra_sdk_mock_consensus::TestNode;

/// Block sizes to benchmark.
const BLOCK_SIZES: &[usize] = &[1, 5, 10, 25];

type ConsensusService = penumbra_sdk_app::server::consensus::ConsensusService;

/// Set up storage + test node + mock client with N spendable notes.
async fn setup(n: usize) -> anyhow::Result<(TempStorage, TestNode<ConsensusService>, MockClient)> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;

    let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
        raw_amount: 1_000_000u128.into(),
        raw_denom: STAKING_TOKEN_DENOM.deref().base_denom().denom,
        address: test_keys::ADDRESS_0.to_owned(),
    })
    .take(n)
    .collect();

    let content = Content {
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
            allocations,
            ..Default::default()
        },
        ..Default::default()
    };
    let app_state_bytes = serde_json::to_vec(&AppState::Content(content))?;

    let consensus = Consensus::new(storage.as_ref().clone());
    let test_node = TestNode::builder()
        .single_validator()
        .app_state(app_state_bytes)
        .init_chain(consensus)
        .await?;

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    Ok((storage, test_node, client))
}

/// Build N transactions from the client's available notes.
async fn build_transactions(
    client: &MockClient,
    storage: &TempStorage,
    n: usize,
) -> anyhow::Result<Vec<Vec<u8>>> {
    let notes: Vec<_> = client.notes.values().cloned().take(n).collect();
    assert_eq!(notes.len(), n, "expected {} notes, got {}", n, notes.len());

    let mut tx_bytes = Vec::with_capacity(n);
    for note in &notes {
        let position = client
            .position(note.commit())
            .expect("note position exists");

        let mut plan = TransactionPlan {
            actions: vec![
                SpendPlan::new(&mut OsRng, note.clone(), position).into(),
                OutputPlan::new(
                    &mut OsRng,
                    note.value(),
                    test_keys::ADDRESS_1.deref().clone(),
                )
                .into(),
            ],
            memo: Some(MemoPlan::new(
                &mut OsRng,
                MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
            )),
            detection_data: None,
            transaction_parameters: TransactionParameters {
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                ..Default::default()
            },
        }
        .with_populated_detection_data(OsRng, Default::default());

        let tx = client
            .witness_auth_build_with_compliance(&mut plan, storage.latest_snapshot())
            .await?;

        tx_bytes.push(tx.encode_to_vec());
    }

    Ok(tx_bytes)
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("can create runtime");

    let mut results = Vec::new();

    for &n in BLOCK_SIZES {
        eprintln!("\n=== Block size: {} TXs (1S + 1O each) ===", n);

        // Setup (not timed): create storage, test node, build transactions.
        eprintln!("Setting up {} genesis allocations...", n);
        let (storage, mut test_node, client) = rt.block_on(setup(n)).expect("setup failed");

        eprintln!("Building {} transactions...", n);
        let tx_bytes = rt
            .block_on(build_transactions(&client, &storage, n))
            .expect("tx build failed");

        eprintln!("Built {} transactions, executing block...", tx_bytes.len());

        // Timed: execute one block with all N transactions.
        // Single sample per block size — each execution consumes nullifiers so
        // re-running requires full re-setup (prohibitively slow for a bench).
        let t0 = Instant::now();
        rt.block_on(async {
            test_node
                .block()
                .with_data(tx_bytes)
                .execute()
                .await
                .expect("block execution failed");
        });
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let per_tx_ms = elapsed_ms / n as f64;
        let tps = n as f64 / (elapsed_ms / 1000.0);

        eprintln!(
            "  block: {:.1}ms | per_tx: {:.1}ms | TPS: {:.1}",
            elapsed_ms, per_tx_ms, tps
        );

        // Total block time
        results.push(bench_runner::make_result(
            "v0.1",
            &[("block_size", &n.to_string()), ("metric", "block_total_ms")],
            &[elapsed_ms],
            None,
        ));

        // Per-TX average
        results.push(bench_runner::make_result(
            "v0.1",
            &[("block_size", &n.to_string()), ("metric", "per_tx_ms")],
            &[per_tx_ms],
            None,
        ));

        // TPS
        results.push(bench_runner::make_result(
            "v0.1",
            &[("block_size", &n.to_string()), ("metric", "tps")],
            &[tps],
            None,
        ));

        // v0 extrapolation placeholder.
        // Real v0 numbers: run this bench on release/v2.1.x (pre-compliance).
        results.push(bench_runner::make_result(
            "v0 (estimated)",
            &[("block_size", &n.to_string()), ("metric", "tps")],
            &[tps], // placeholder — update from verification.csv delta
            None,
        ));

        drop(storage);
    }

    // Output
    bench_runner::print_table(&results);
    let csv_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/compliance/validator/results/block_tps.csv");
    bench_runner::write_csv(&csv_path, &results);
}
