//! Node/ABCI flow benchmark: representative cross-pass validator work.
//!
//! Emits:
//! - top-level overview: `benches/compliance/flows.csv`
//! - category KPIs: `benches/compliance/node_abci/node_abci.csv`
//! - section overview: `benches/compliance/node_abci/sections.csv`
//! - section KPIs: `benches/compliance/node_abci/sections/<section>.csv`
//! - process_proposal subsections:
//!   `benches/compliance/node_abci/sections/process_proposal_subsections/<subsection>.csv`

use std::ops::Deref;
use std::sync::Arc;
use std::time::Instant;

use cnidarium::TempStorage;
use penumbra_sdk_app::{
    app::App,
    genesis::{AppState, Content},
    stateless_cache::{CacheEntry, StatelessCache},
    AppActionHandler, SUBSTORE_PREFIXES,
};
use penumbra_sdk_asset::STAKING_TOKEN_DENOM;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_keys::test_keys;
use penumbra_sdk_mock_client::MockClient;
use penumbra_sdk_mock_consensus::TestNode;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_shielded_pool::{genesis::Allocation, OutputPlan, SpendPlan};
use penumbra_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, Transaction, TransactionParameters, TransactionPlan,
};
use rand_core::OsRng;
use sha2::Digest;
use tendermint::{
    account, block,
    v0_37::abci::{request, response},
    Hash, Time,
};

const QUICK_TX_COUNT: usize = 10;
const DEEP_TX_COUNT: usize = 25;
const REGRESSION_TX_COUNT: usize = 100;
const DEEP_BLOCK_SIZES: &[usize] = &[1, 5, 10, 25];

type ConsensusService = penumbra_sdk_app::server::consensus::ConsensusService;

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

    let consensus = penumbra_sdk_app::server::consensus::Consensus::new(storage.as_ref().clone());
    let mut test_node = TestNode::builder()
        .single_validator()
        .app_state(app_state_bytes)
        .init_chain(consensus)
        .await?;

    // Prime chain state with one empty block so timestamp-dependent checks
    // in deliver_tx/check_stateless have real block context.
    test_node.block().execute().await?;

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    Ok((storage, test_node, client))
}

async fn build_transactions(
    client: &MockClient,
    storage: &TempStorage,
    n: usize,
) -> anyhow::Result<Vec<Vec<u8>>> {
    let notes: Vec<_> = client.notes.values().cloned().take(n).collect();
    assert_eq!(notes.len(), n, "expected {n} notes, got {}", notes.len());

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

fn checktx_once(rt: &tokio::runtime::Runtime, storage: &TempStorage, tx: &[u8], warm: bool) {
    let mut app = App::new(storage.latest_snapshot());
    let cache = StatelessCache::new();
    if warm {
        let hash: [u8; 32] = sha2::Sha256::digest(tx).into();
        cache.insert(hash, CacheEntry::Valid);
    }

    rt.block_on(async {
        app.deliver_tx_bytes(tx, Some(&cache))
            .await
            .expect("checktx path should accept tx");
    });
}

fn prepare_proposal_once(
    rt: &tokio::runtime::Runtime,
    storage: &TempStorage,
    txs: &[Vec<u8>],
    max_tx_bytes: i64,
) {
    let mut app = App::new(storage.latest_snapshot());
    let req = request::PrepareProposal {
        txs: txs.iter().cloned().map(Into::into).collect(),
        max_tx_bytes,
        local_last_commit: None,
        misbehavior: Vec::new(),
        height: block::Height::from(1u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    };

    let rsp = rt.block_on(async { app.prepare_proposal(req, None).await });
    assert!(!rsp.txs.is_empty(), "prepare_proposal should include txs");
}

fn process_proposal_once(rt: &tokio::runtime::Runtime, storage: &TempStorage, txs: &[Vec<u8>]) {
    let mut app = App::new(storage.latest_snapshot());
    let req = request::ProcessProposal {
        txs: txs.iter().cloned().map(Into::into).collect(),
        proposed_last_commit: None,
        misbehavior: Vec::new(),
        hash: Hash::None,
        height: block::Height::from(1u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    };

    let rsp = rt.block_on(async { app.process_proposal(req).await });
    assert!(
        matches!(rsp, response::ProcessProposal::Accept),
        "process_proposal should accept valid txs"
    );
}

fn deliver_tx_once(rt: &tokio::runtime::Runtime, storage: &TempStorage, tx: &[u8], warm: bool) {
    let mut app = App::new(storage.latest_snapshot());
    let cache = StatelessCache::new();
    if warm {
        let hash: [u8; 32] = sha2::Sha256::digest(tx).into();
        cache.insert(hash, CacheEntry::Valid);
    }

    rt.block_on(async {
        app.deliver_tx_bytes(tx, Some(&cache))
            .await
            .expect("deliver_tx path should accept tx");
    });
}

fn decode_tx_once(tx: &[u8]) {
    let _ = Transaction::decode(tx).expect("tx bytes should decode");
}

fn check_stateless_once(rt: &tokio::runtime::Runtime, tx: &[u8]) {
    let tx = Arc::new(Transaction::decode(tx).expect("tx bytes should decode"));
    rt.block_on(async {
        tx.check_stateless(())
            .await
            .expect("stateless check should pass");
    });
}

fn check_historical_once(rt: &tokio::runtime::Runtime, storage: &TempStorage, tx: &[u8]) {
    let tx = Arc::new(Transaction::decode(tx).expect("tx bytes should decode"));
    let state = Arc::new(storage.latest_snapshot());
    rt.block_on(async {
        tx.check_historical(state)
            .await
            .expect("historical check should pass");
    });
}

fn push_block_metrics(
    rt: &tokio::runtime::Runtime,
    version: &str,
    out: &mut Vec<bench_runner::BenchResult>,
    block_sizes: &[usize],
    include_per_tx: bool,
) {
    for &n in block_sizes {
        let (storage, mut test_node, client) = rt.block_on(setup(n)).expect("setup failed");
        let tx_bytes = rt
            .block_on(build_transactions(&client, &storage, n))
            .expect("tx build failed");

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

        out.push(bench_runner::make_result(
            version,
            &[
                ("flow", "block"),
                ("metric", "tps"),
                ("block_size", &n.to_string()),
            ],
            &[tps],
            None,
        ));
        if include_per_tx {
            out.push(bench_runner::make_result(
                version,
                &[
                    ("flow", "block"),
                    ("metric", "per_tx_ms"),
                    ("block_size", &n.to_string()),
                ],
                &[per_tx_ms],
                None,
            ));
        }

        drop(storage);
    }
}

fn pick_flow_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    flow: &str,
    metric: &str,
    block_size: &str,
) -> bench_runner::BenchResult {
    if let Some(found) = raw.iter().find(|r| {
        r.version == version
            && r.dimensions.iter().any(|(k, v)| k == "flow" && v == flow)
            && r.dimensions
                .iter()
                .any(|(k, v)| k == "metric" && v == metric)
            && r.dimensions
                .iter()
                .any(|(k, v)| k == "block_size" && v == block_size)
    }) {
        return found.clone();
    }
    panic!(
        "missing flow KPI row for version={version}, flow={flow}, metric={metric}, block_size={block_size}"
    )
}

fn flow_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    kpi_block_size: usize,
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    let kpi_block_size = kpi_block_size.to_string();
    rows.push(pick_flow_kpi(
        raw,
        version,
        "abci_roundtrip",
        "latency_ms",
        "",
    ));
    rows.push(pick_flow_kpi(raw, version, "block", "tps", &kpi_block_size));
    if !regression {
        rows.push(pick_flow_kpi(
            raw,
            version,
            "block",
            "per_tx_ms",
            &kpi_block_size,
        ));
    }
    rows
}

fn pick_section_kpi(
    raw: &[bench_runner::BenchResult],
    version: &str,
    section: &str,
    metric: &str,
    variant: &str,
) -> bench_runner::BenchResult {
    if let Some(found) = raw.iter().find(|r| {
        r.version == version
            && r.dimensions
                .iter()
                .any(|(k, v)| k == "section" && v == section)
            && r.dimensions
                .iter()
                .any(|(k, v)| k == "metric" && v == metric)
            && r.dimensions
                .iter()
                .any(|(k, v)| k == "variant" && v == variant)
    }) {
        return found.clone();
    }
    panic!(
        "missing section KPI row for version={version}, section={section}, metric={metric}, variant={variant}"
    )
}

fn section_rows_for_version(
    raw: &[bench_runner::BenchResult],
    version: &str,
    regression: bool,
) -> Vec<bench_runner::BenchResult> {
    let mut rows = Vec::new();
    rows.push(pick_section_kpi(
        raw,
        version,
        "checktx",
        "latency_ms",
        "warm",
    ));
    rows.push(pick_section_kpi(
        raw,
        version,
        "prepare_proposal",
        "latency_ms",
        "default",
    ));
    rows.push(pick_section_kpi(
        raw,
        version,
        "process_proposal",
        "latency_ms",
        "default",
    ));
    rows.push(pick_section_kpi(
        raw,
        version,
        "deliver_tx",
        "latency_ms",
        "warm",
    ));
    if !regression {
        rows.push(pick_section_kpi(
            raw,
            version,
            "checktx",
            "latency_ms",
            "cold",
        ));
        rows.push(pick_section_kpi(
            raw,
            version,
            "deliver_tx",
            "latency_ms",
            "cold",
        ));
    }
    rows
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("can create runtime");

    let version = bench_runner::bench_version();
    let warmup = bench_runner::warmup_count();
    let samples = bench_runner::sample_count();
    let quick = bench_runner::is_quick_profile();
    let regression = bench_runner::is_regression_suite();

    let stage_tx_count = if regression {
        REGRESSION_TX_COUNT
    } else if quick {
        QUICK_TX_COUNT
    } else {
        DEEP_TX_COUNT
    };
    let kpi_block_size = stage_tx_count;
    let block_sizes: Vec<usize> = if regression || quick {
        vec![kpi_block_size]
    } else {
        DEEP_BLOCK_SIZES.to_vec()
    };

    let mut raw_results = Vec::new();

    eprintln!("=== node_abci_flow {version} ===");

    let (storage, _node, client) = rt.block_on(setup(stage_tx_count)).expect("setup failed");
    let txs = rt
        .block_on(build_transactions(&client, &storage, stage_tx_count))
        .expect("tx build failed");
    let tx0 = txs.first().expect("at least one tx");
    let max_tx_bytes = (txs.iter().map(|b| b.len()).sum::<usize>() * 2) as i64;

    let checktx_warm = bench_runner::run_bench(warmup, samples, || {
        checktx_once(&rt, &storage, tx0, true);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("section", "checktx"),
            ("metric", "latency_ms"),
            ("variant", "warm"),
        ],
        &checktx_warm,
        None,
    ));

    if !regression {
        let checktx_cold = bench_runner::run_bench(warmup, samples, || {
            checktx_once(&rt, &storage, tx0, false);
        });
        raw_results.push(bench_runner::make_result(
            &version,
            &[
                ("section", "checktx"),
                ("metric", "latency_ms"),
                ("variant", "cold"),
            ],
            &checktx_cold,
            None,
        ));
    }

    let prepare = bench_runner::run_bench(warmup, samples, || {
        prepare_proposal_once(&rt, &storage, &txs, max_tx_bytes);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("section", "prepare_proposal"),
            ("metric", "latency_ms"),
            ("variant", "default"),
        ],
        &prepare,
        None,
    ));

    let process = bench_runner::run_bench(warmup, samples, || {
        process_proposal_once(&rt, &storage, &txs);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("section", "process_proposal"),
            ("metric", "latency_ms"),
            ("variant", "default"),
        ],
        &process,
        None,
    ));

    if !regression {
        let deliver_cold = bench_runner::run_bench(warmup, samples, || {
            deliver_tx_once(&rt, &storage, tx0, false);
        });
        raw_results.push(bench_runner::make_result(
            &version,
            &[
                ("section", "deliver_tx"),
                ("metric", "latency_ms"),
                ("variant", "cold"),
            ],
            &deliver_cold,
            None,
        ));
    }

    let deliver_warm = bench_runner::run_bench(warmup, samples, || {
        deliver_tx_once(&rt, &storage, tx0, true);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("section", "deliver_tx"),
            ("metric", "latency_ms"),
            ("variant", "warm"),
        ],
        &deliver_warm,
        None,
    ));

    let flow_total = bench_runner::run_bench(warmup, samples, || {
        checktx_once(&rt, &storage, tx0, true);
        prepare_proposal_once(&rt, &storage, &txs, max_tx_bytes);
        process_proposal_once(&rt, &storage, &txs);
        deliver_tx_once(&rt, &storage, tx0, true);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("flow", "abci_roundtrip"),
            ("metric", "latency_ms"),
            ("block_size", ""),
        ],
        &flow_total,
        None,
    ));

    // Subsection rows under section=process_proposal.
    let decode_ms = bench_runner::run_bench(warmup, samples, || {
        decode_tx_once(tx0);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[("subsection", "tx_decode"), ("metric", "decode_tx_ms")],
        &decode_ms,
        None,
    ));

    let stateless_ms = bench_runner::run_bench(warmup, samples, || {
        check_stateless_once(&rt, tx0);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("subsection", "tx_stateless"),
            ("metric", "check_stateless_ms"),
        ],
        &stateless_ms,
        None,
    ));

    let historical_ms = bench_runner::run_bench(warmup, samples, || {
        check_historical_once(&rt, &storage, tx0);
    });
    raw_results.push(bench_runner::make_result(
        &version,
        &[
            ("subsection", "tx_historical"),
            ("metric", "check_historical_ms"),
        ],
        &historical_ms,
        None,
    ));

    // Block metrics are single-sample by design: each execution consumes nullifiers.
    push_block_metrics(&rt, &version, &mut raw_results, &block_sizes, !regression);

    drop(storage);

    let flow_rows = flow_rows_for_version(&raw_results, &version, kpi_block_size, regression);
    let section_rows = section_rows_for_version(&raw_results, &version, regression);
    let subsection_rows: Vec<_> = raw_results
        .iter()
        .filter(|r| r.dimensions.iter().any(|(k, _)| k == "subsection"))
        .cloned()
        .collect();
    let mut flow_with_meta = flow_rows.clone();
    bench_runner::annotate_raw_results(&mut flow_with_meta);
    let mut sections_with_meta = section_rows.clone();
    bench_runner::annotate_raw_results(&mut sections_with_meta);
    let mut subsections_with_meta = subsection_rows.clone();
    bench_runner::annotate_raw_results(&mut subsections_with_meta);
    bench_runner::output_results(&flow_with_meta);

    let flow_path = bench_runner::category_csv_path("node_abci");
    bench_runner::append_csv(&flow_path, &flow_with_meta);

    let sections_overview_path = bench_runner::category_sections_csv_path("node_abci");
    bench_runner::append_csv(&sections_overview_path, &sections_with_meta);

    let flows_overview = bench_runner::to_flow_overview_rows("node_abci", &flow_with_meta);
    let flows_overview_path = bench_runner::flows_overview_csv_path();
    bench_runner::append_csv_scoped(&flows_overview_path, &flows_overview, &["category", "kpi"]);

    for section in [
        "checktx",
        "prepare_proposal",
        "process_proposal",
        "deliver_tx",
    ] {
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
        let section_path = bench_runner::section_csv_path("node_abci", section);
        bench_runner::append_csv(&section_path, &rows);
    }

    for subsection in ["tx_decode", "tx_stateless", "tx_historical"] {
        let mut rows: Vec<_> = subsections_with_meta
            .iter()
            .filter(|r| {
                r.dimensions
                    .iter()
                    .any(|(k, v)| k == "subsection" && v == subsection)
            })
            .cloned()
            .collect();
        for r in &mut rows {
            r.dimensions.retain(|(k, _)| k != "subsection");
        }
        let subsection_path =
            bench_runner::subsection_csv_path("node_abci", "process_proposal", subsection);
        bench_runner::append_csv(&subsection_path, &rows);
    }
}
