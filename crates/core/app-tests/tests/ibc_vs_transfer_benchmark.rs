use {
    anyhow::{Context as _, Result},
    ark_serialize::CanonicalSerialize,
    common::ibc_tests::{MockRelayer, TestNodeWithIBC, TestStorage, ValidatorKeys},
    decaf377::Fr,
    decaf377_rdsa::VerificationKey,
    penumbra_sdk_app::{
        app::{
            candidate_digest_from_hashes, App, CandidateEnvelope, ExecutionBlockProfile,
            PrepareProposalProfile, ProcessProposalProfile, ProposalArtifactSidecar,
        },
        block_tx_indexing::BlockTxIndexingMode,
        stateless_cache::StatelessCache,
    },
    penumbra_sdk_asset::{asset::Cache, Value, BASE_ASSET_ID},
    penumbra_sdk_compliance::{IbcAssetOrigin, IbcRoute},
    penumbra_sdk_ibc::{
        benchmarking::{self, InboundReceiveBreakdown, StageTiming},
        IbcToken,
    },
    penumbra_sdk_keys::{keys::AddressIndex, test_keys},
    penumbra_sdk_mock_consensus::NodeResumeState,
    penumbra_sdk_num::Amount,
    penumbra_sdk_proto::DomainType as _,
    penumbra_sdk_shielded_pool::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan},
    penumbra_sdk_transaction::{
        memo::MemoPlaintext, plan::MemoPlan, Action, ActionPlan, Transaction,
        TransactionParameters, TransactionPlan,
    },
    rand_core::OsRng,
    serde::{Deserialize, Serialize},
    sha2::Digest as _,
    std::{
        collections::BTreeSet,
        fs,
        ops::Deref as _,
        path::{Path, PathBuf},
        process::Command,
        str::FromStr as _,
        sync::Arc,
        time::{Instant, SystemTime, UNIX_EPOCH},
    },
    tendermint::v0_37::abci::{request, response},
    tendermint::{account, block, Hash, Time},
    tokio::{sync::Semaphore, task::JoinSet},
    tokio_util::bytes::Bytes,
};

mod common;

const SCHEMA_VERSION: u32 = 1;
const SETUP_CACHE_SCHEMA_VERSION: u32 = 1;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "real benchmark; run explicitly with PENUMBRA_IBC_VS_TRANSFER_TX_COUNT=1000"]
async fn ibc_vs_transfer_benchmark() -> Result<()> {
    let _guard = common::set_tracing_subscriber();
    let config = Config::from_env()?;
    fs::create_dir_all(&config.out_dir)?;

    let corpus_start = Instant::now();
    let (corpus, corpus_path, corpus_source) = load_or_build_corpus(&config).await?;
    let corpus_load_or_build_wall_ms = elapsed_ms(corpus_start);
    validate_corpus(&corpus)?;

    let setup_cache_start = Instant::now();
    let setup_cache = load_or_build_setup_cache(&corpus).await?;
    let setup_cache_load_or_build_wall_ms = elapsed_ms(setup_cache_start);

    let mut scenarios = Vec::new();
    if config.includes_scenario("regulated_inner_transfer") {
        scenarios.push(
            run_scenario(
                &config,
                &corpus,
                &setup_cache,
                "regulated_inner_transfer",
                ScenarioKind::InnerTransfer,
            )
            .await?,
        );
    }
    if config.includes_scenario("regulated_outbound_shielded_ics20") {
        scenarios.push(
            run_scenario(
                &config,
                &corpus,
                &setup_cache,
                "regulated_outbound_shielded_ics20",
                ScenarioKind::OutboundShielded,
            )
            .await?,
        );
    }
    if config.includes_scenario("regulated_inbound_ics20_recv_preupdated") {
        scenarios.push(
            run_scenario(
                &config,
                &corpus,
                &setup_cache,
                "regulated_inbound_ics20_recv_preupdated",
                ScenarioKind::InboundRecvPreupdated,
            )
            .await?,
        );
    }
    if config.includes_scenario("regulated_inbound_ics20_full") {
        scenarios.push(
            run_scenario(
                &config,
                &corpus,
                &setup_cache,
                "regulated_inbound_ics20_full",
                ScenarioKind::InboundFull,
            )
            .await?,
        );
    }
    anyhow::ensure!(
        !scenarios.is_empty(),
        "no scenarios selected; check PENUMBRA_IBC_VS_TRANSFER_SCENARIOS"
    );

    let report = BenchmarkReport {
        schema_version: SCHEMA_VERSION,
        generated_at_unix: unix_ts(),
        tx_count: config.tx_count,
        state_setup_tx_count: corpus.state_setup_tx_count(),
        runs: config.runs,
        corpus_path,
        corpus_source,
        corpus_load_or_build_wall_ms,
        corpus_fingerprint: corpus.fingerprint.clone(),
        setup_cache_path: setup_cache.path.clone(),
        setup_cache_source: setup_cache.source.clone(),
        setup_cache_load_or_build_wall_ms,
        setup_cache_fingerprint: setup_cache.metadata.fingerprint.clone(),
        prover_gated_test_status:
            "not run; release app-test execution used current generated proofs, but the full prover-gated shielded-pool suite was not invoked by this benchmark"
                .to_string(),
        scenarios,
    };
    let json_path = config.out_dir.join("results.json");
    fs::write(&json_path, serde_json::to_vec_pretty(&report)?)?;
    fs::write(config.out_dir.join("RESULTS.md"), render_markdown(&report))?;

    println!("wrote {}", json_path.display());
    println!("{}", render_console_summary(&report));
    Ok(())
}

struct Config {
    tx_count: usize,
    runs: usize,
    rebuild_corpus: bool,
    require_reuse: bool,
    out_dir: PathBuf,
    scenarios: Option<BTreeSet<String>>,
}

impl Config {
    fn from_env() -> Result<Self> {
        Ok(Self {
            tx_count: env_usize("PENUMBRA_IBC_VS_TRANSFER_TX_COUNT", 1_000)?,
            runs: env_usize("PENUMBRA_IBC_VS_TRANSFER_RUNS", 3)?,
            rebuild_corpus: env_bool("PENUMBRA_IBC_VS_TRANSFER_REBUILD_CORPUS", false),
            require_reuse: env_bool("PENUMBRA_IBC_VS_TRANSFER_REQUIRE_REUSE", false),
            out_dir: std::env::var("PENUMBRA_IBC_VS_TRANSFER_OUT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| workspace_target_bench_dir()),
            scenarios: std::env::var("PENUMBRA_IBC_VS_TRANSFER_SCENARIOS")
                .ok()
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .collect()
                }),
        })
    }

    fn corpus_path(&self) -> PathBuf {
        self.out_dir
            .join("corpus")
            .join(format!("regulated-{}.json", self.tx_count))
    }

    fn includes_scenario(&self, name: &str) -> bool {
        self.scenarios
            .as_ref()
            .map(|scenarios| scenarios.contains(name))
            .unwrap_or(true)
    }
}

fn workspace_target_bench_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("app-tests crate should live under crates/core/app-tests")
        .to_path_buf()
        .join("target")
        .join("bench")
        .join("ibc-vs-transfer-real")
}

#[derive(Clone, Serialize, Deserialize)]
struct BenchmarkCorpus {
    schema_version: u32,
    tx_count: usize,
    #[serde(default)]
    state_setup_tx_count: Option<usize>,
    created_at_unix: u64,
    fingerprint: String,
    git_commit: Option<String>,
    git_tree_state: String,
    inner_transfer_txs_hex: Vec<String>,
    outbound_shielded_txs_hex: Vec<String>,
    update_client_b_tx_hex: String,
    recv_packet_txs_hex: Vec<String>,
}

impl BenchmarkCorpus {
    fn state_setup_tx_count(&self) -> usize {
        self.state_setup_tx_count.unwrap_or(self.tx_count)
    }

    fn first_n_from_superset(&self, tx_count: usize) -> Result<Self> {
        anyhow::ensure!(
            self.tx_count >= tx_count,
            "cannot slice {} txs from {} tx corpus",
            tx_count,
            self.tx_count
        );
        anyhow::ensure!(
            self.inner_transfer_txs_hex.len() >= tx_count
                && self.outbound_shielded_txs_hex.len() >= tx_count
                && self.recv_packet_txs_hex.len() >= tx_count,
            "superset corpus vectors are shorter than requested {tx_count} txs"
        );
        Ok(Self {
            schema_version: self.schema_version,
            tx_count,
            state_setup_tx_count: Some(self.state_setup_tx_count()),
            created_at_unix: self.created_at_unix,
            fingerprint: corpus_fingerprint(tx_count)?,
            git_commit: self.git_commit.clone(),
            git_tree_state: self.git_tree_state.clone(),
            inner_transfer_txs_hex: self.inner_transfer_txs_hex[..tx_count].to_vec(),
            outbound_shielded_txs_hex: self.outbound_shielded_txs_hex[..tx_count].to_vec(),
            update_client_b_tx_hex: self.update_client_b_tx_hex.clone(),
            recv_packet_txs_hex: self.recv_packet_txs_hex[..tx_count].to_vec(),
        })
    }

    fn inner_transfer_txs(&self) -> Vec<Vec<u8>> {
        decode_txs(&self.inner_transfer_txs_hex)
    }

    fn outbound_shielded_txs(&self) -> Vec<Vec<u8>> {
        decode_txs(&self.outbound_shielded_txs_hex)
    }

    fn update_client_b_tx(&self) -> Vec<u8> {
        hex::decode(&self.update_client_b_tx_hex).expect("corpus update tx hex must decode")
    }

    fn recv_packet_txs(&self) -> Vec<Vec<u8>> {
        decode_txs(&self.recv_packet_txs_hex)
    }
}

#[derive(Clone)]
struct BenchmarkSetupCache {
    path: PathBuf,
    source: String,
    metadata: SetupCacheMetadata,
}

#[derive(Clone, Serialize, Deserialize)]
struct SetupCacheMetadata {
    schema_version: u32,
    fingerprint: String,
    allocation_count: usize,
    corpus_fingerprint: String,
    compliance_policy_hash: String,
    git_commit: Option<String>,
    git_tree_state: String,
    chain_a: CachedChain,
    chain_b: CachedChain,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedChain {
    suffix: String,
    chain_id: String,
    storage_dir_name: String,
    client_id: String,
    connection_id: String,
    channel_id: String,
    port_id: String,
    node: CachedNodeState,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedNodeState {
    last_app_hash_hex: String,
    last_validator_set_hash: Option<String>,
    last_commit: Option<tendermint::block::Commit>,
    consensus_params_hash_hex: String,
    height: u64,
    timestamp: String,
    chain_id: String,
}

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    generated_at_unix: u64,
    tx_count: usize,
    state_setup_tx_count: usize,
    runs: usize,
    corpus_path: PathBuf,
    corpus_source: String,
    corpus_load_or_build_wall_ms: f64,
    corpus_fingerprint: String,
    setup_cache_path: PathBuf,
    setup_cache_source: String,
    setup_cache_load_or_build_wall_ms: f64,
    setup_cache_fingerprint: String,
    prover_gated_test_status: String,
    scenarios: Vec<ScenarioReport>,
}

#[derive(Serialize)]
struct ScenarioReport {
    name: String,
    tx_count: usize,
    runs: Vec<RunReport>,
    summary: ScenarioSummary,
}

#[derive(Serialize)]
struct RunReport {
    run_index: usize,
    setup_wall_ms: f64,
    execute_wall_ms: f64,
    tps: f64,
    ms_per_tx: f64,
    projected_5000_tx_ms: f64,
    send_packet_count: usize,
    write_ack_count: usize,
    detailed: DetailedRunReport,
}

#[derive(Serialize)]
struct ScenarioSummary {
    mean_execute_wall_ms: f64,
    mean_tps: f64,
    mean_ms_per_tx: f64,
    p50_ms_per_tx: f64,
    p95_ms_per_tx: f64,
    p99_ms_per_tx: f64,
    projected_5000_tx_ms_from_mean: f64,
    mean_direct_zk_batch_verify_ms: f64,
    mean_prepare_proposal_ms: f64,
    mean_process_proposal_ms: f64,
    mean_profiled_execute_ms: f64,
    mean_checktx_cold_ms: f64,
    mean_checktx_warm_ms: f64,
    mean_delay_block_ms: f64,
}

#[derive(Default, Serialize)]
struct DetailedRunReport {
    profiled_block_count: usize,
    profiled_tx_count: usize,
    transfer_action_count: usize,
    transfer_input_slots: usize,
    transfer_output_slots: usize,
    transfer_real_inputs: usize,
    transfer_dummy_inputs: usize,
    transfer_real_outputs: usize,
    transfer_dummy_outputs: usize,
    ibc_relay_action_count: usize,
    delay_block_count: usize,
    delay_block_wall_ms: f64,
    direct_artifact_extract_ms: f64,
    direct_artifact_extract_action_ms: f64,
    direct_artifact_extract_public_ms: f64,
    direct_artifact_to_batch_item_ms: f64,
    direct_zk_batch_verify_ms: f64,
    checktx_cold_wall_ms: f64,
    checktx_warm_wall_ms: f64,
    prepare_proposal_wall_ms: f64,
    prepare_zk_batch_verify_ms: f64,
    prepare_stateful_filter_ms: f64,
    prepare_aggregate_build_ms: f64,
    process_proposal_wall_ms: f64,
    process_aggregate_verify_ms: f64,
    process_cold_reconstruction_zk_ms: f64,
    process_stateful_replay_ms: f64,
    execute_block_tx_count: usize,
    execute_begin_block_ms: f64,
    execute_deliver_txs_wall_ms: f64,
    execute_profiled_wall_ms: f64,
    execute_execute_tx_ms: f64,
    execute_begin_state_tx_ms: f64,
    execute_index_tx_ms: f64,
    execute_get_block_height_ms: f64,
    execute_clone_tx_ms: f64,
    execute_proto_convert_ms: f64,
    execute_put_block_transaction_ms: f64,
    execute_tx_log_read_ms: f64,
    execute_tx_log_encode_ms: f64,
    execute_tx_log_put_raw_ms: f64,
    execute_check_and_execute_ms: f64,
    execute_set_source_ms: f64,
    execute_pay_fee_ms: f64,
    execute_historical_state_ms: f64,
    execute_read_local_precheck_ms: f64,
    execute_read_lookup_wait_or_join_ms: f64,
    execute_read_historical_check_ms: f64,
    execute_read_nullifier_wait_ms: f64,
    execute_read_anchor_cache_wait_ms: f64,
    execute_action_ms: f64,
    execute_commit_ms: f64,
    execute_end_block_ms: f64,
    execute_spend_action_ms: f64,
    execute_spend_nullifier_tx_local_scan_ms: f64,
    execute_spend_nullifier_block_log_lookup_ms: f64,
    execute_spend_nullifier_committed_check_ms: f64,
    execute_spend_nullifier_enqueue_ms: f64,
    execute_spend_nullifier_stage_ms: f64,
    execute_spend_nullifier_merge_ms: f64,
    execute_nullifier_lookup_count: usize,
    execute_output_action_ms: f64,
    execute_output_write_ms: f64,
    execute_other_action_ms: f64,
    execute_record_clues_ms: f64,
    execute_apply_ms: f64,
    execute_nullifier_check_ms: f64,
    inbound_receive: InboundReceiveBreakdownReport,
}

#[derive(Clone, Copy, Default, Serialize)]
struct StageTimingReport {
    count: u64,
    total_ms: f64,
    mean_us: f64,
}

#[derive(Clone, Copy, Default, Serialize)]
struct InboundReceiveBreakdownReport {
    channel_read: StageTimingReport,
    connection_read: StageTimingReport,
    timeout_check: StageTimingReport,
    packet_proof_verify: StageTimingReport,
    duplicate_sequence_check: StageTimingReport,
    app_check: StageTimingReport,
    receipt_write: StageTimingReport,
    app_execute_total: StageTimingReport,
    packet_data_decode: StageTimingReport,
    route_resolve: StageTimingReport,
    compliance_check: StageTimingReport,
    mint_unescrow_accounting: StageTimingReport,
    register_denom: StageTimingReport,
    value_balance_read: StageTimingReport,
    mint_note_total: StageTimingReport,
    mint_note_sct_append: StageTimingReport,
    mint_note_build: StageTimingReport,
    mint_note_add_payload_total: StageTimingReport,
    mint_note_pending_payload: StageTimingReport,
    value_balance_write: StageTimingReport,
    event_record: StageTimingReport,
    app_execute_inner: StageTimingReport,
    acknowledgement_read: StageTimingReport,
    acknowledgement_write: StageTimingReport,
    acknowledgement_total: StageTimingReport,
    deferred_sct_reserve: StageTimingReport,
    deferred_sct_materialize: StageTimingReport,
    deferred_sct_pending_payload: StageTimingReport,
}

#[derive(Default)]
struct BlockMeasurement {
    tx_count: usize,
    wall_ms: f64,
    send_packet_count: usize,
    write_ack_count: usize,
    detailed: DetailedRunReport,
}

async fn load_or_build_corpus(config: &Config) -> Result<(BenchmarkCorpus, PathBuf, String)> {
    let path = config.corpus_path();
    let expected_fingerprint = corpus_fingerprint(config.tx_count)?;
    anyhow::ensure!(
        !(config.rebuild_corpus && config.require_reuse),
        "PENUMBRA_IBC_VS_TRANSFER_REBUILD_CORPUS and PENUMBRA_IBC_VS_TRANSFER_REQUIRE_REUSE cannot both be set"
    );
    if !config.rebuild_corpus && path.exists() {
        let corpus: BenchmarkCorpus = serde_json::from_slice(&fs::read(&path)?)?;
        if corpus.schema_version == SCHEMA_VERSION
            && corpus.tx_count == config.tx_count
            && corpus.fingerprint == expected_fingerprint
        {
            return Ok((corpus, path, "loaded".to_string()));
        }
    }

    if !config.rebuild_corpus {
        if let Some((superset, superset_path)) = find_reusable_superset_corpus(config)? {
            let source_tx_count = superset.tx_count;
            let sliced = superset.first_n_from_superset(config.tx_count)?;
            return Ok((
                sliced,
                superset_path.clone(),
                format!(
                    "loaded first {} txs from reusable {} tx corpus",
                    config.tx_count, source_tx_count
                ),
            ));
        }
    }

    anyhow::ensure!(
        !config.require_reuse,
        "PENUMBRA_IBC_VS_TRANSFER_REQUIRE_REUSE=1 but no reusable corpus was found for {} txs; expected an existing corpus such as {}",
        config.tx_count,
        workspace_target_bench_dir()
            .join("corpus")
            .join(format!("regulated-{}.json", config.tx_count))
            .display()
    );

    let start = Instant::now();
    println!(
        "building regulated IBC benchmark corpus: {} txs",
        config.tx_count
    );
    let mut relayer = setup_relayer(config.tx_count).await?;
    println!("building inner transfer tx corpus");
    let inner_transfer_txs = build_inner_transfer_txs(&mut relayer.chain_a_ibc, config.tx_count)
        .await
        .context("building regulated inner transfer corpus")?;
    println!("building shielded outbound IBC tx corpus");
    let outbound_shielded_txs = relayer
        .build_shielded_withdrawal_txs_a_to_b(config.tx_count)
        .await
        .context("building regulated shielded IBC outbound corpus")?;

    println!("executing outbound corpus once to derive real packet proofs");
    let (_end_block, deliver_events) = relayer
        .chain_a_ibc
        .node
        .block()
        .with_data(outbound_shielded_txs.clone())
        .execute()
        .await
        .context("executing outbound corpus once to derive send packets")?;
    relayer
        ._sync_chains()
        .await
        .context("syncing chains after outbound corpus execution")?;
    let packets = MockRelayer::collect_send_packet_events(&deliver_events.0)?;
    anyhow::ensure!(
        packets.len() == config.tx_count,
        "expected {} send_packet events, got {}",
        config.tx_count,
        packets.len()
    );
    println!("building update-client and recv-packet tx corpus");
    let prepared_recv_packets = relayer
        .prepare_recv_packets_a_to_b(&packets)
        .await
        .context("preparing regulated inbound recv packet proofs")?;
    relayer
        ._build_and_send_update_client_a()
        .await
        .context("updating chain A client before building chain B update")?;
    let (update_client_b_tx, proof_height) = relayer.build_update_client_b_tx().await?;
    let recv_packet_txs = relayer
        .build_recv_packet_txs_a_to_b_from_prepared(&prepared_recv_packets, proof_height)
        .await
        .context("building regulated inbound recv corpus")?;
    anyhow::ensure!(
        recv_packet_txs.len() == config.tx_count,
        "expected {} recv txs, got {}",
        config.tx_count,
        recv_packet_txs.len()
    );

    let corpus = BenchmarkCorpus {
        schema_version: SCHEMA_VERSION,
        tx_count: config.tx_count,
        state_setup_tx_count: Some(config.tx_count),
        created_at_unix: unix_ts(),
        fingerprint: expected_fingerprint,
        git_commit: git_commit(),
        git_tree_state: git_tree_state(),
        inner_transfer_txs_hex: encode_txs(&inner_transfer_txs),
        outbound_shielded_txs_hex: encode_txs(&outbound_shielded_txs),
        update_client_b_tx_hex: hex::encode(update_client_b_tx),
        recv_packet_txs_hex: encode_txs(&recv_packet_txs),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_vec_pretty(&corpus)?)?;
    println!(
        "built corpus {} in {:.1}ms",
        path.display(),
        elapsed_ms(start)
    );
    Ok((corpus, path, "rebuilt".to_string()))
}

fn find_reusable_superset_corpus(config: &Config) -> Result<Option<(BenchmarkCorpus, PathBuf)>> {
    let mut candidates = reusable_corpus_paths(config)?;
    candidates.sort();

    let mut reusable = Vec::new();
    for path in candidates {
        let corpus: BenchmarkCorpus = match fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            Some(corpus) => corpus,
            None => continue,
        };
        if corpus.schema_version != SCHEMA_VERSION || corpus.tx_count < config.tx_count {
            continue;
        }
        if corpus.fingerprint != corpus_fingerprint(corpus.tx_count)? {
            continue;
        }
        if corpus.inner_transfer_txs_hex.len() < config.tx_count
            || corpus.outbound_shielded_txs_hex.len() < config.tx_count
            || corpus.recv_packet_txs_hex.len() < config.tx_count
        {
            continue;
        }
        reusable.push((corpus.tx_count, corpus, path));
    }

    reusable.sort_by_key(|(tx_count, _, _)| *tx_count);
    Ok(reusable
        .into_iter()
        .next()
        .map(|(_, corpus, path)| (corpus, path)))
}

fn reusable_corpus_paths(config: &Config) -> Result<Vec<PathBuf>> {
    let mut dirs = vec![
        config.out_dir.join("corpus"),
        workspace_target_bench_dir().join("corpus"),
    ];
    dirs.sort();
    dirs.dedup();

    let mut paths = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry?.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("regulated-") && name.ends_with(".json"))
                .unwrap_or(false)
            {
                continue;
            }
            paths.push(path);
        }
    }
    Ok(paths)
}

async fn setup_relayer(tx_count: usize) -> Result<MockRelayer> {
    setup_relayer_at_root(tx_count, None).await
}

async fn setup_relayer_at_root(
    tx_count: usize,
    storage_root: Option<&Path>,
) -> Result<MockRelayer> {
    let setup_start = Instant::now();
    let start_time_a = tendermint::Time::parse_from_rfc3339("2022-02-11T17:30:50.425417198Z")?;
    let start_time_b = start_time_a;
    let vkeys_a = ValidatorKeys::from_seed([0u8; 32]);
    let vkeys_b = ValidatorKeys::from_seed([1u8; 32]);
    let sk_a = vkeys_a.validator_cons_sk.ed25519_signing_key().unwrap();
    let sk_b = vkeys_b.validator_cons_sk.ed25519_signing_key().unwrap();
    let ska = ed25519_consensus::SigningKey::try_from(sk_a.as_bytes())?;
    let skb = ed25519_consensus::SigningKey::try_from(sk_b.as_bytes())?;
    let registrar_vk = VerificationKey::from(test_keys::SPEND_KEY.spend_auth_key());
    let chain_a_root = storage_root.map(|root| root.join("chain-a"));
    let chain_b_root = storage_root.map(|root| root.join("chain-b"));

    let mut chain_a_ibc = TestNodeWithIBC::new_with_allocations_and_registrar_at_root(
        "a",
        start_time_a,
        (ska.clone(), ska.verification_key()),
        tx_count,
        registrar_vk,
        chain_a_root.as_deref(),
    )
    .await?;
    println!(
        "  setup: chain A initialized at {:.1}ms",
        elapsed_ms(setup_start)
    );
    let mut chain_b_ibc = TestNodeWithIBC::new_with_allocations_and_registrar_at_root(
        "b",
        start_time_b,
        (skb.clone(), skb.verification_key()),
        1,
        registrar_vk,
        chain_b_root.as_deref(),
    )
    .await?;
    println!(
        "  setup: chain B initialized at {:.1}ms",
        elapsed_ms(setup_start)
    );

    for _ in 0..3 {
        chain_a_ibc.node.block().execute().await?;
    }
    for _ in 0..3 {
        chain_b_ibc.node.block().execute().await?;
    }
    println!(
        "  setup: chains fast-forwarded at {:.1}ms",
        elapsed_ms(setup_start)
    );

    let mut relayer = MockRelayer {
        chain_a_ibc,
        chain_b_ibc,
    };
    relayer._create_clients().await?;
    println!(
        "  setup: IBC clients created at {:.1}ms",
        elapsed_ms(setup_start)
    );
    relayer._connection_handshake().await?;
    println!(
        "  setup: IBC connection handshake complete at {:.1}ms",
        elapsed_ms(setup_start)
    );
    relayer._channel_handshake().await?;
    println!(
        "  setup: IBC handshake complete at {:.1}ms",
        elapsed_ms(setup_start)
    );
    register_regulated_assets(&mut relayer).await?;
    println!(
        "  setup: regulated compliance registered at {:.1}ms",
        elapsed_ms(setup_start)
    );
    Ok(relayer)
}

async fn load_or_build_setup_cache(corpus: &BenchmarkCorpus) -> Result<BenchmarkSetupCache> {
    let fingerprint = setup_cache_fingerprint(corpus)?;
    let cache_root = workspace_target_bench_dir().join("setup-cache");
    let cache_path = cache_root.join(&fingerprint);
    let metadata_path = cache_path.join("metadata.json");

    if let Some(metadata) = read_valid_setup_cache_metadata(&metadata_path, corpus, &fingerprint)? {
        return Ok(BenchmarkSetupCache {
            path: cache_path,
            source: "restored".to_string(),
            metadata,
        });
    }

    anyhow::ensure!(
        !std::env::var("PENUMBRA_IBC_VS_TRANSFER_REQUIRE_REUSE")
            .ok()
            .map(|value| env_bool_value(&value))
            .transpose()?
            .unwrap_or(false),
        "PENUMBRA_IBC_VS_TRANSFER_REQUIRE_REUSE=1 but no reusable setup cache was found for {} allocations; expected {}",
        corpus.state_setup_tx_count(),
        metadata_path.display()
    );

    fs::create_dir_all(&cache_root)?;
    let build_path = cache_root.join(format!(".building-{fingerprint}-{}", unix_ts()));
    if build_path.exists() {
        fs::remove_dir_all(&build_path).with_context(|| {
            format!("removing stale setup cache build {}", build_path.display())
        })?;
    }
    fs::create_dir_all(&build_path)?;

    println!(
        "building reusable IBC benchmark setup cache for {} allocations",
        corpus.state_setup_tx_count()
    );
    let metadata = {
        let relayer = setup_relayer_at_root(corpus.state_setup_tx_count(), Some(&build_path))
            .await
            .context("building reusable IBC benchmark setup cache")?;
        setup_cache_metadata(corpus, &fingerprint, &relayer)?
    };
    fs::write(
        build_path.join("metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;

    if cache_path.exists() {
        fs::remove_dir_all(&build_path).with_context(|| {
            format!("discarding redundant setup cache {}", build_path.display())
        })?;
        let metadata = read_valid_setup_cache_metadata(&metadata_path, corpus, &fingerprint)?
            .context("existing setup cache became invalid while building replacement")?;
        return Ok(BenchmarkSetupCache {
            path: cache_path,
            source: "restored-existing-after-build".to_string(),
            metadata,
        });
    }
    fs::rename(&build_path, &cache_path).with_context(|| {
        format!(
            "promoting setup cache {} to {}",
            build_path.display(),
            cache_path.display()
        )
    })?;

    Ok(BenchmarkSetupCache {
        path: cache_path,
        source: "built".to_string(),
        metadata,
    })
}

fn read_valid_setup_cache_metadata(
    metadata_path: &Path,
    corpus: &BenchmarkCorpus,
    expected_fingerprint: &str,
) -> Result<Option<SetupCacheMetadata>> {
    if !metadata_path.exists() {
        return Ok(None);
    }
    let metadata: SetupCacheMetadata = serde_json::from_slice(&fs::read(metadata_path)?)?;
    if metadata.schema_version != SETUP_CACHE_SCHEMA_VERSION
        || metadata.fingerprint != expected_fingerprint
        || metadata.allocation_count != corpus.state_setup_tx_count()
        || metadata.corpus_fingerprint != setup_corpus_fingerprint(corpus)?
        || metadata.compliance_policy_hash != benchmark_compliance_policy_hash()
        || metadata.git_tree_state != git_tree_state()
    {
        return Ok(None);
    }
    let cache_path = metadata_path
        .parent()
        .context("setup cache metadata must have a parent directory")?;
    let chain_a_db = cache_path
        .join(&metadata.chain_a.storage_dir_name)
        .join("storage.db");
    let chain_b_db = cache_path
        .join(&metadata.chain_b.storage_dir_name)
        .join("storage.db");
    if !chain_a_db.exists() || !chain_b_db.exists() {
        return Ok(None);
    }
    Ok(Some(metadata))
}

fn setup_cache_metadata(
    corpus: &BenchmarkCorpus,
    fingerprint: &str,
    relayer: &MockRelayer,
) -> Result<SetupCacheMetadata> {
    Ok(SetupCacheMetadata {
        schema_version: SETUP_CACHE_SCHEMA_VERSION,
        fingerprint: fingerprint.to_string(),
        allocation_count: corpus.state_setup_tx_count(),
        corpus_fingerprint: setup_corpus_fingerprint(corpus)?,
        compliance_policy_hash: benchmark_compliance_policy_hash(),
        git_commit: git_commit(),
        git_tree_state: git_tree_state(),
        chain_a: cached_chain("a", "chain-a", &relayer.chain_a_ibc),
        chain_b: cached_chain("b", "chain-b", &relayer.chain_b_ibc),
    })
}

fn cached_chain(suffix: &str, storage_dir_name: &str, chain: &TestNodeWithIBC) -> CachedChain {
    CachedChain {
        suffix: suffix.to_string(),
        chain_id: chain.chain_id.clone(),
        storage_dir_name: storage_dir_name.to_string(),
        client_id: chain.client_id.to_string(),
        connection_id: chain.connection_id.to_string(),
        channel_id: chain.channel_id.to_string(),
        port_id: chain.port_id.to_string(),
        node: cached_node_state(chain.node.resume_state()),
    }
}

fn cached_node_state(state: NodeResumeState) -> CachedNodeState {
    CachedNodeState {
        last_app_hash_hex: hex::encode(&state.last_app_hash),
        last_validator_set_hash: state.last_validator_set_hash.map(|hash| hash.to_string()),
        last_commit: state.last_commit,
        consensus_params_hash_hex: hex::encode(&state.consensus_params_hash),
        height: u64::from(state.height),
        timestamp: state.timestamp.to_rfc3339(),
        chain_id: state.chain_id.to_string(),
    }
}

async fn restore_relayer_from_setup_cache(cache: &BenchmarkSetupCache) -> Result<MockRelayer> {
    let vkeys_a = ValidatorKeys::from_seed([0u8; 32]);
    let vkeys_b = ValidatorKeys::from_seed([1u8; 32]);
    let sk_a = vkeys_a.validator_cons_sk.ed25519_signing_key().unwrap();
    let sk_b = vkeys_b.validator_cons_sk.ed25519_signing_key().unwrap();
    let ska = ed25519_consensus::SigningKey::try_from(sk_a.as_bytes())?;
    let skb = ed25519_consensus::SigningKey::try_from(sk_b.as_bytes())?;

    let chain_a_storage =
        TestStorage::copied_from(cache.path.join(&cache.metadata.chain_a.storage_dir_name)).await?;
    let chain_b_storage =
        TestStorage::copied_from(cache.path.join(&cache.metadata.chain_b.storage_dir_name)).await?;
    let chain_a_ibc = TestNodeWithIBC::new_from_cached_storage(
        &cache.metadata.chain_a.suffix,
        (ska.clone(), ska.verification_key()),
        chain_a_storage,
        node_resume_state(&cache.metadata.chain_a.node)?,
    )
    .await?;
    let chain_b_ibc = TestNodeWithIBC::new_from_cached_storage(
        &cache.metadata.chain_b.suffix,
        (skb.clone(), skb.verification_key()),
        chain_b_storage,
        node_resume_state(&cache.metadata.chain_b.node)?,
    )
    .await?;

    ensure_restored_chain_matches_metadata(&chain_a_ibc, &cache.metadata.chain_a)?;
    ensure_restored_chain_matches_metadata(&chain_b_ibc, &cache.metadata.chain_b)?;

    Ok(MockRelayer {
        chain_a_ibc,
        chain_b_ibc,
    })
}

fn node_resume_state(cached: &CachedNodeState) -> Result<NodeResumeState> {
    Ok(NodeResumeState {
        last_app_hash: hex::decode(&cached.last_app_hash_hex)?,
        last_validator_set_hash: cached
            .last_validator_set_hash
            .as_deref()
            .map(tendermint::Hash::from_str)
            .transpose()?,
        last_commit: cached.last_commit.clone(),
        consensus_params_hash: hex::decode(&cached.consensus_params_hash_hex)?,
        height: block::Height::try_from(cached.height)?,
        timestamp: Time::parse_from_rfc3339(&cached.timestamp)?,
        chain_id: tendermint::chain::Id::try_from(cached.chain_id.clone())?,
    })
}

fn ensure_restored_chain_matches_metadata(
    chain: &TestNodeWithIBC,
    metadata: &CachedChain,
) -> Result<()> {
    anyhow::ensure!(
        chain.chain_id == metadata.chain_id,
        "restored chain_id mismatch: got {}, expected {}",
        chain.chain_id,
        metadata.chain_id
    );
    anyhow::ensure!(
        chain.client_id.to_string() == metadata.client_id
            && chain.connection_id.to_string() == metadata.connection_id
            && chain.channel_id.to_string() == metadata.channel_id
            && chain.port_id.to_string() == metadata.port_id,
        "restored IBC identifiers mismatch for chain {}",
        metadata.chain_id
    );
    Ok(())
}

fn setup_cache_fingerprint(corpus: &BenchmarkCorpus) -> Result<String> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(SETUP_CACHE_SCHEMA_VERSION.to_le_bytes());
    hasher.update(setup_corpus_fingerprint(corpus)?.as_bytes());
    hasher.update((corpus.state_setup_tx_count() as u64).to_le_bytes());
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(git_tree_state().as_bytes());
    if let Some(commit) = git_commit() {
        hasher.update(commit.as_bytes());
    }
    hasher.update(b"chain-a-seed-0");
    hasher.update(b"chain-b-seed-1");
    hasher.update(b"regulated-compliance-v1");
    hasher.update(benchmark_compliance_policy_hash().as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

fn benchmark_compliance_policy_hash() -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"chain-a:transfer/channel-0/connection-0/transfer/channel-0");
    hasher.update(b"chain-b:transfer/channel-0/connection-0/transfer/channel-0");
    hasher.update(b"chain-b-origin:transfer/channel-0/connection-0/transfer/channel-0:upenumbra");
    hex::encode(hasher.finalize())
}

fn setup_corpus_fingerprint(corpus: &BenchmarkCorpus) -> Result<String> {
    corpus_fingerprint(corpus.state_setup_tx_count())
}

async fn register_regulated_assets(relayer: &mut MockRelayer) -> Result<()> {
    let registrar_sk = test_keys::SPEND_KEY.spend_auth_key();
    let authority_sk = test_keys::SPEND_KEY.spend_auth_key();
    let route_a = IbcRoute::transfer(
        relayer.chain_a_ibc.channel_id.to_string(),
        relayer.chain_a_ibc.connection_id.to_string(),
        relayer.chain_b_ibc.channel_id.to_string(),
    );
    let route_b = IbcRoute::transfer(
        relayer.chain_b_ibc.channel_id.to_string(),
        relayer.chain_b_ibc.connection_id.to_string(),
        relayer.chain_a_ibc.channel_id.to_string(),
    );

    let chain_a_client = relayer.chain_a_ibc.client().await?;
    let chain_b_client = relayer.chain_b_ibc.client().await?;
    let address_a = chain_a_client.fvk.payment_address(AddressIndex::new(0)).0;
    let address_b = chain_b_client.fvk.payment_address(AddressIndex::new(0)).0;
    relayer
        .chain_a_ibc
        .execute_regulated_compliance_setup(
            &[address_a, test_keys::ADDRESS_1.deref().clone()],
            *BASE_ASSET_ID,
            vec![route_a],
            None,
            registrar_sk,
            authority_sk,
        )
        .await?;

    let asset_cache = Cache::with_known_assets();
    let base_denom = asset_cache
        .get(&BASE_ASSET_ID)
        .expect("base asset ID should exist in asset cache")
        .to_string();
    let voucher = IbcToken::new(
        &relayer.chain_b_ibc.channel_id,
        &relayer.chain_b_ibc.port_id,
        &base_denom,
    );
    relayer
        .chain_b_ibc
        .execute_regulated_compliance_setup(
            &[address_b],
            voucher.id(),
            vec![route_b.clone()],
            Some(IbcAssetOrigin {
                route: route_b,
                base_denom,
            }),
            registrar_sk,
            authority_sk,
        )
        .await?;
    Ok(())
}

async fn build_inner_transfer_txs(
    chain: &mut TestNodeWithIBC,
    count: usize,
) -> Result<Vec<Vec<u8>>> {
    let client = Arc::new(chain.client().await?);
    let notes = client
        .notes
        .values()
        .filter(|note| note.asset_id() == *BASE_ASSET_ID)
        .cloned()
        .take(count)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        notes.len() == count,
        "expected {count} base-asset notes for inner transfer, got {}",
        notes.len()
    );

    let snapshot = chain.storage.latest_snapshot();
    let chain_id = chain.chain_id.clone();
    let permits = Arc::new(Semaphore::new(proof_tx_build_concurrency()));
    let mut tasks = JoinSet::new();
    for (ordinal, note) in notes.into_iter().enumerate() {
        let client = client.clone();
        let snapshot = snapshot.clone();
        let chain_id = chain_id.clone();
        let permits = permits.clone();
        tasks.spawn(async move {
            let _permit = permits
                .acquire_owned()
                .await
                .expect("proof tx semaphore should not be closed");
            let spend = ShieldedInputPlan::new(
                &mut OsRng,
                note.clone(),
                client
                    .position(note.commit())
                    .context("note should be in mock client's tree")?,
            );
            let send_amount = Amount::from(1u64);
            let change_amount = note.amount() - send_amount;
            let mut output = ShieldedOutputPlan::new(
                &mut OsRng,
                Value {
                    amount: send_amount,
                    asset_id: note.asset_id(),
                },
                test_keys::ADDRESS_1.deref().clone(),
            );
            let mut change = ShieldedOutputPlan::new(
                &mut OsRng,
                Value {
                    amount: change_amount,
                    asset_id: note.asset_id(),
                },
                note.address(),
            );
            align_metadata(&spend, &mut [&mut output, &mut change]);

            let mut plan = TransactionPlan {
                actions: vec![TransferPlan::new(
                    vec![spend.into()],
                    vec![output.into(), change.into()],
                    Fr::from(1u64),
                )?
                .into()],
                fee_funding: None,
                memo: Some(MemoPlan::new(
                    &mut OsRng,
                    MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
                )),
                detection_data: None,
                transaction_parameters: TransactionParameters {
                    chain_id,
                    ..Default::default()
                },
            }
            .with_populated_detection_data(OsRng, Default::default());
            let tx = client
                .witness_auth_build_with_compliance(&mut plan, snapshot)
                .await?;
            Ok::<(usize, Vec<u8>), anyhow::Error>((ordinal, tx.encode_to_vec()))
        });
    }

    let mut txs = vec![Vec::new(); count];
    while let Some(joined) = tasks.join_next().await {
        let (ordinal, bytes) = joined.context("waiting for inner proof tx build task")??;
        txs[ordinal] = bytes;
    }
    Ok(txs)
}

fn align_metadata(spend: &ShieldedInputPlan, outputs: &mut [&mut ShieldedOutputPlan]) {
    for output in outputs {
        output.asset_anchor = spend.asset_anchor;
        output.compliance_anchor = spend.compliance_anchor;
        output.target_timestamp = spend.target_timestamp;
        output.is_regulated = spend.is_regulated;
        output.tx_blinding_nonce = spend.tx_blinding_nonce;
        output.asset_indexed_leaf = spend.asset_indexed_leaf.clone();
        output.asset_path = spend.asset_path.clone();
        output.asset_position = spend.asset_position;
        output.asset_policy = spend.asset_policy.clone();
    }
}

async fn rebuild_ibc_relay_txs(
    chain: &mut TestNodeWithIBC,
    txs: Vec<Vec<u8>>,
) -> Result<Vec<Vec<u8>>> {
    let client = chain.client().await?;
    let mut rebuilt = Vec::with_capacity(txs.len());
    for (index, bytes) in txs.into_iter().enumerate() {
        let tx = Transaction::decode(bytes.as_slice())
            .with_context(|| format!("decoding stored IBC relay tx {index}"))?;
        let actions = tx.actions().collect::<Vec<_>>();
        anyhow::ensure!(
            actions.len() == 1,
            "stored IBC relay tx {index} has {} actions",
            actions.len()
        );
        let Action::IbcRelay(relay) = actions[0] else {
            anyhow::bail!("stored tx {index} is not an IBC relay action");
        };
        let plan = TransactionPlan {
            actions: vec![ActionPlan::IbcAction(relay.clone())],
            memo: None,
            detection_data: None,
            fee_funding: None,
            transaction_parameters: TransactionParameters {
                chain_id: chain.chain_id.clone(),
                ..Default::default()
            },
        };
        let tx = client
            .witness_auth_build(&plan)
            .await
            .with_context(|| format!("re-signing stored IBC relay tx {index}"))?;
        rebuilt.push(tx.encode_to_vec());
    }
    Ok(rebuilt)
}

fn proof_tx_build_concurrency() -> usize {
    std::env::var("BENCH_PROOF_TX_BUILD_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|parallelism| parallelism.get())
                .unwrap_or(8)
        })
}

#[derive(Clone, Copy)]
enum ScenarioKind {
    InnerTransfer,
    OutboundShielded,
    InboundRecvPreupdated,
    InboundFull,
}

async fn run_scenario(
    config: &Config,
    corpus: &BenchmarkCorpus,
    setup_cache: &BenchmarkSetupCache,
    name: &str,
    kind: ScenarioKind,
) -> Result<ScenarioReport> {
    let mut runs = Vec::with_capacity(config.runs);
    for run_index in 0..config.runs {
        let setup_start = Instant::now();
        let mut relayer = restore_relayer_from_setup_cache(setup_cache)
            .await
            .with_context(|| format!("restoring setup cache for scenario {name}"))?;
        let setup_wall_ms = elapsed_ms(setup_start);

        let measurement = match kind {
            ScenarioKind::InnerTransfer => {
                profile_blocks(
                    &mut relayer.chain_a_ibc,
                    vec![ProfileStep::Timed(corpus.inner_transfer_txs())],
                    config.tx_count,
                    0,
                    0,
                )
                .await
            }
            ScenarioKind::OutboundShielded => {
                profile_blocks(
                    &mut relayer.chain_a_ibc,
                    vec![ProfileStep::Timed(corpus.outbound_shielded_txs())],
                    config.tx_count,
                    config.tx_count,
                    0,
                )
                .await
            }
            ScenarioKind::InboundRecvPreupdated => {
                let update_txs = rebuild_ibc_relay_txs(
                    &mut relayer.chain_b_ibc,
                    vec![corpus.update_client_b_tx()],
                )
                .await?;
                let recv_txs =
                    rebuild_ibc_relay_txs(&mut relayer.chain_b_ibc, corpus.recv_packet_txs())
                        .await?;
                execute_delay_block(&mut relayer.chain_b_ibc)
                    .await
                    .context("advancing destination before client update")?;
                execute_txs(&mut relayer.chain_b_ibc, update_txs).await?;
                execute_delay_block(&mut relayer.chain_b_ibc)
                    .await
                    .context("advancing destination past IBC connection delay")?;
                profile_blocks(
                    &mut relayer.chain_b_ibc,
                    vec![ProfileStep::Timed(recv_txs)],
                    config.tx_count,
                    0,
                    config.tx_count,
                )
                .await
            }
            ScenarioKind::InboundFull => {
                let update_txs = rebuild_ibc_relay_txs(
                    &mut relayer.chain_b_ibc,
                    vec![corpus.update_client_b_tx()],
                )
                .await?;
                let recv_txs =
                    rebuild_ibc_relay_txs(&mut relayer.chain_b_ibc, corpus.recv_packet_txs())
                        .await?;
                profile_blocks(
                    &mut relayer.chain_b_ibc,
                    vec![
                        ProfileStep::DelayOnly,
                        ProfileStep::Timed(update_txs),
                        ProfileStep::DelayOnly,
                        ProfileStep::Timed(recv_txs),
                    ],
                    config.tx_count,
                    0,
                    config.tx_count,
                )
                .await
            }
        }
        .with_context(|| format!("running scenario {name} run {run_index}"))?;
        anyhow::ensure!(
            measurement.tx_count == config.tx_count,
            "scenario {name} run {run_index} executed {} txs, expected {}",
            measurement.tx_count,
            config.tx_count
        );
        let tps = config.tx_count as f64 / (measurement.wall_ms / 1000.0);
        let ms_per_tx = measurement.wall_ms / config.tx_count as f64;
        runs.push(RunReport {
            run_index,
            setup_wall_ms,
            execute_wall_ms: measurement.wall_ms,
            tps,
            ms_per_tx,
            projected_5000_tx_ms: ms_per_tx * 5_000.0,
            send_packet_count: measurement.send_packet_count,
            write_ack_count: measurement.write_ack_count,
            detailed: measurement.detailed,
        });
    }

    Ok(ScenarioReport {
        name: name.to_string(),
        tx_count: config.tx_count,
        summary: summarize(&runs),
        runs,
    })
}

enum ProfileStep {
    Timed(Vec<Vec<u8>>),
    DelayOnly,
}

async fn profile_blocks(
    chain: &mut TestNodeWithIBC,
    blocks: Vec<ProfileStep>,
    denominator_tx_count: usize,
    send_packet_count: usize,
    write_ack_count: usize,
) -> Result<BlockMeasurement> {
    let mut detailed = DetailedRunReport::default();
    detailed.profiled_block_count = blocks.len();
    detailed.profiled_tx_count = blocks
        .iter()
        .map(|step| match step {
            ProfileStep::Timed(txs) => txs.len(),
            ProfileStep::DelayOnly => 0,
        })
        .sum();

    let mut wall_ms = 0.0;
    for step in blocks {
        match step {
            ProfileStep::Timed(txs) => {
                let block = profile_block(chain, txs).await?;
                wall_ms += block.wall_ms;
                detailed.add_block(block.detailed);
            }
            ProfileStep::DelayOnly => {
                let delay_ms = execute_delay_block(chain).await?;
                wall_ms += delay_ms;
                detailed.delay_block_count += 1;
                detailed.delay_block_wall_ms += delay_ms;
            }
        }
    }

    Ok(BlockMeasurement {
        tx_count: denominator_tx_count,
        wall_ms,
        send_packet_count,
        write_ack_count,
        detailed,
    })
}

async fn execute_delay_block(chain: &mut TestNodeWithIBC) -> Result<f64> {
    let start = Instant::now();
    chain
        .node
        .block()
        .execute()
        .await
        .context("executing empty delay block through mock consensus")?;
    Ok(elapsed_ms(start))
}

struct ProfiledBlock {
    wall_ms: f64,
    detailed: DetailedRunReport,
}

async fn profile_block(chain: &mut TestNodeWithIBC, txs: Vec<Vec<u8>>) -> Result<ProfiledBlock> {
    let mut detailed = DetailedRunReport::default();
    detailed.profiled_block_count = 1;
    detailed.profiled_tx_count = txs.len();

    let proof_start = Instant::now();
    let decoded_txs = txs
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            Transaction::decode(bytes.as_slice())
                .map(Arc::new)
                .with_context(|| format!("decoding profiled tx {index}"))
        })
        .collect::<Result<Vec<_>>>()?;
    apply_shape_counts(&mut detailed, &decoded_txs);
    let (artifacts, artifact_profile) =
        App::build_tx_artifacts_extracted_profiled_public("ibc_vs_transfer_extract", &decoded_txs)
            .await?;
    detailed.direct_artifact_extract_ms = elapsed_ms(proof_start);
    detailed.direct_artifact_extract_action_ms = artifact_profile.action_extract_ms;
    detailed.direct_artifact_extract_public_ms = artifact_profile.action_extract_public_ms;
    detailed.direct_artifact_to_batch_item_ms = artifact_profile.action_to_batch_item_ms;

    let zk_start = Instant::now();
    detailed.direct_zk_batch_verify_ms = App::batch_verify_artifacts_for_bench(&artifacts).await?;
    if detailed.direct_zk_batch_verify_ms == 0.0 {
        detailed.direct_zk_batch_verify_ms = elapsed_ms(zk_start);
    }

    detailed.checktx_cold_wall_ms = run_checktx_cold(chain, &txs).await?;
    detailed.checktx_warm_wall_ms = run_checktx_warm(chain, &txs, &artifacts).await?;

    let prepare_request = prepare_request(&txs);
    let mut proposer = App::new(chain.storage.latest_snapshot());
    proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    let prepare_start = Instant::now();
    let (prepared, prepare_profile, sidecar) = proposer
        .prepare_proposal_v2_profiled(prepare_request, None, true)
        .await;
    detailed.prepare_proposal_wall_ms = elapsed_ms(prepare_start);
    apply_prepare_profile(&mut detailed, &prepare_profile);
    ensure_prepare_preserved_user_txs(&txs, &prepared)?;

    let process_request = process_request_from_prepare_response(&prepared);
    let mut validator = App::new(chain.storage.latest_snapshot());
    let process_start = Instant::now();
    let (process_verdict, process_profile) = validator
        .process_proposal_v2_profiled(process_request, None, sidecar.as_ref(), true)
        .await;
    detailed.process_proposal_wall_ms = elapsed_ms(process_start);
    anyhow::ensure!(
        matches!(process_verdict, response::ProcessProposal::Accept),
        "process proposal rejected profiled block: {process_verdict:?}"
    );
    apply_process_profile(&mut detailed, &process_profile);

    let envelope = candidate_envelope_from_txs(&txs)?;
    let mut executor = App::new(chain.storage.latest_snapshot());
    executor.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    benchmarking::reset_inbound_receive_breakdown();
    let execute_start = Instant::now();
    let execution_profile = executor
        .execute_validated_candidate_envelope_profiled(&envelope, chain.storage.as_ref().clone())
        .await?;
    detailed.execute_profiled_wall_ms = elapsed_ms(execute_start);
    apply_execution_profile(&mut detailed, &execution_profile);
    detailed
        .inbound_receive
        .add_snapshot(benchmarking::snapshot_inbound_receive_breakdown());
    benchmarking::reset_inbound_receive_breakdown();

    Ok(ProfiledBlock {
        wall_ms: detailed.execute_profiled_wall_ms,
        detailed,
    })
}

fn apply_shape_counts(report: &mut DetailedRunReport, txs: &[Arc<Transaction>]) {
    for tx in txs {
        for action in tx.actions() {
            match action {
                Action::Transfer(transfer) => {
                    report.transfer_action_count += 1;
                    report.transfer_input_slots += transfer.body.inputs.len();
                    report.transfer_output_slots += transfer.body.outputs.len();
                    let real_inputs = transfer
                        .body
                        .inputs
                        .iter()
                        .filter(|input| !input.is_dummy())
                        .count();
                    let real_outputs = transfer
                        .body
                        .outputs
                        .iter()
                        .filter(|output| !output.is_dummy())
                        .count();
                    report.transfer_real_inputs += real_inputs;
                    report.transfer_dummy_inputs += transfer.body.inputs.len() - real_inputs;
                    report.transfer_real_outputs += real_outputs;
                    report.transfer_dummy_outputs += transfer.body.outputs.len() - real_outputs;
                }
                Action::IbcRelay(_) => {
                    report.ibc_relay_action_count += 1;
                }
                _ => {}
            }
        }
    }
}

async fn run_checktx_cold(chain: &TestNodeWithIBC, txs: &[Vec<u8>]) -> Result<f64> {
    let start = Instant::now();
    let mut app = App::new(chain.storage.latest_snapshot());
    for tx in txs {
        app.deliver_tx_bytes_v1_profiled(tx)
            .await
            .context("cold CheckTx profile failed")?;
    }
    Ok(elapsed_ms(start))
}

async fn run_checktx_warm(
    chain: &TestNodeWithIBC,
    txs: &[Vec<u8>],
    artifacts: &[Arc<penumbra_sdk_app::stateless_cache::TxArtifact>],
) -> Result<f64> {
    let cache = StatelessCache::new();
    for (tx, artifact) in txs.iter().zip(artifacts.iter()) {
        let hash: [u8; 32] = sha2::Sha256::digest(tx).into();
        cache.insert_extracted(hash, artifact.clone());
    }

    let start = Instant::now();
    let mut app = App::new(chain.storage.latest_snapshot());
    for tx in txs {
        app.deliver_tx_bytes_v2_profiled(tx, Some(&cache))
            .await
            .context("warm CheckTx profile failed")?;
    }
    Ok(elapsed_ms(start))
}

impl DetailedRunReport {
    fn add_block(&mut self, block: DetailedRunReport) {
        self.transfer_action_count += block.transfer_action_count;
        self.transfer_input_slots += block.transfer_input_slots;
        self.transfer_output_slots += block.transfer_output_slots;
        self.transfer_real_inputs += block.transfer_real_inputs;
        self.transfer_dummy_inputs += block.transfer_dummy_inputs;
        self.transfer_real_outputs += block.transfer_real_outputs;
        self.transfer_dummy_outputs += block.transfer_dummy_outputs;
        self.ibc_relay_action_count += block.ibc_relay_action_count;
        self.delay_block_count += block.delay_block_count;
        self.delay_block_wall_ms += block.delay_block_wall_ms;
        self.direct_artifact_extract_ms += block.direct_artifact_extract_ms;
        self.direct_artifact_extract_action_ms += block.direct_artifact_extract_action_ms;
        self.direct_artifact_extract_public_ms += block.direct_artifact_extract_public_ms;
        self.direct_artifact_to_batch_item_ms += block.direct_artifact_to_batch_item_ms;
        self.direct_zk_batch_verify_ms += block.direct_zk_batch_verify_ms;
        self.checktx_cold_wall_ms += block.checktx_cold_wall_ms;
        self.checktx_warm_wall_ms += block.checktx_warm_wall_ms;
        self.prepare_proposal_wall_ms += block.prepare_proposal_wall_ms;
        self.prepare_zk_batch_verify_ms += block.prepare_zk_batch_verify_ms;
        self.prepare_stateful_filter_ms += block.prepare_stateful_filter_ms;
        self.prepare_aggregate_build_ms += block.prepare_aggregate_build_ms;
        self.process_proposal_wall_ms += block.process_proposal_wall_ms;
        self.process_aggregate_verify_ms += block.process_aggregate_verify_ms;
        self.process_cold_reconstruction_zk_ms += block.process_cold_reconstruction_zk_ms;
        self.process_stateful_replay_ms += block.process_stateful_replay_ms;
        self.execute_block_tx_count += block.execute_block_tx_count;
        self.execute_begin_block_ms += block.execute_begin_block_ms;
        self.execute_deliver_txs_wall_ms += block.execute_deliver_txs_wall_ms;
        self.execute_profiled_wall_ms += block.execute_profiled_wall_ms;
        self.execute_execute_tx_ms += block.execute_execute_tx_ms;
        self.execute_begin_state_tx_ms += block.execute_begin_state_tx_ms;
        self.execute_index_tx_ms += block.execute_index_tx_ms;
        self.execute_get_block_height_ms += block.execute_get_block_height_ms;
        self.execute_clone_tx_ms += block.execute_clone_tx_ms;
        self.execute_proto_convert_ms += block.execute_proto_convert_ms;
        self.execute_put_block_transaction_ms += block.execute_put_block_transaction_ms;
        self.execute_tx_log_read_ms += block.execute_tx_log_read_ms;
        self.execute_tx_log_encode_ms += block.execute_tx_log_encode_ms;
        self.execute_tx_log_put_raw_ms += block.execute_tx_log_put_raw_ms;
        self.execute_check_and_execute_ms += block.execute_check_and_execute_ms;
        self.execute_set_source_ms += block.execute_set_source_ms;
        self.execute_pay_fee_ms += block.execute_pay_fee_ms;
        self.execute_historical_state_ms += block.execute_historical_state_ms;
        self.execute_read_local_precheck_ms += block.execute_read_local_precheck_ms;
        self.execute_read_lookup_wait_or_join_ms += block.execute_read_lookup_wait_or_join_ms;
        self.execute_read_historical_check_ms += block.execute_read_historical_check_ms;
        self.execute_read_nullifier_wait_ms += block.execute_read_nullifier_wait_ms;
        self.execute_read_anchor_cache_wait_ms += block.execute_read_anchor_cache_wait_ms;
        self.execute_action_ms += block.execute_action_ms;
        self.execute_commit_ms += block.execute_commit_ms;
        self.execute_end_block_ms += block.execute_end_block_ms;
        self.execute_spend_action_ms += block.execute_spend_action_ms;
        self.execute_spend_nullifier_tx_local_scan_ms +=
            block.execute_spend_nullifier_tx_local_scan_ms;
        self.execute_spend_nullifier_block_log_lookup_ms +=
            block.execute_spend_nullifier_block_log_lookup_ms;
        self.execute_spend_nullifier_committed_check_ms +=
            block.execute_spend_nullifier_committed_check_ms;
        self.execute_spend_nullifier_enqueue_ms += block.execute_spend_nullifier_enqueue_ms;
        self.execute_spend_nullifier_stage_ms += block.execute_spend_nullifier_stage_ms;
        self.execute_spend_nullifier_merge_ms += block.execute_spend_nullifier_merge_ms;
        self.execute_nullifier_lookup_count += block.execute_nullifier_lookup_count;
        self.execute_output_action_ms += block.execute_output_action_ms;
        self.execute_output_write_ms += block.execute_output_write_ms;
        self.execute_other_action_ms += block.execute_other_action_ms;
        self.execute_record_clues_ms += block.execute_record_clues_ms;
        self.execute_apply_ms += block.execute_apply_ms;
        self.execute_nullifier_check_ms += block.execute_nullifier_check_ms;
        self.inbound_receive.add(block.inbound_receive);
    }
}

impl InboundReceiveBreakdownReport {
    fn add_snapshot(&mut self, snapshot: InboundReceiveBreakdown) {
        self.channel_read.add_stage(snapshot.channel_read);
        self.connection_read.add_stage(snapshot.connection_read);
        self.timeout_check.add_stage(snapshot.timeout_check);
        self.packet_proof_verify
            .add_stage(snapshot.packet_proof_verify);
        self.duplicate_sequence_check
            .add_stage(snapshot.duplicate_sequence_check);
        self.app_check.add_stage(snapshot.app_check);
        self.receipt_write.add_stage(snapshot.receipt_write);
        self.app_execute_total.add_stage(snapshot.app_execute_total);
        self.packet_data_decode
            .add_stage(snapshot.packet_data_decode);
        self.route_resolve.add_stage(snapshot.route_resolve);
        self.compliance_check.add_stage(snapshot.compliance_check);
        self.mint_unescrow_accounting
            .add_stage(snapshot.mint_unescrow_accounting);
        self.register_denom.add_stage(snapshot.register_denom);
        self.value_balance_read
            .add_stage(snapshot.value_balance_read);
        self.mint_note_total.add_stage(snapshot.mint_note_total);
        self.mint_note_sct_append
            .add_stage(snapshot.mint_note_sct_append);
        self.mint_note_build.add_stage(snapshot.mint_note_build);
        self.mint_note_add_payload_total
            .add_stage(snapshot.mint_note_add_payload_total);
        self.mint_note_pending_payload
            .add_stage(snapshot.mint_note_pending_payload);
        self.value_balance_write
            .add_stage(snapshot.value_balance_write);
        self.event_record.add_stage(snapshot.event_record);
        self.app_execute_inner.add_stage(snapshot.app_execute_inner);
        self.acknowledgement_read
            .add_stage(snapshot.acknowledgement_read);
        self.acknowledgement_write
            .add_stage(snapshot.acknowledgement_write);
        self.acknowledgement_total
            .add_stage(snapshot.acknowledgement_total);
        self.deferred_sct_reserve
            .add_stage(snapshot.deferred_sct_reserve);
        self.deferred_sct_materialize
            .add_stage(snapshot.deferred_sct_materialize);
        self.deferred_sct_pending_payload
            .add_stage(snapshot.deferred_sct_pending_payload);
    }

    fn add(&mut self, other: InboundReceiveBreakdownReport) {
        self.channel_read.add(other.channel_read);
        self.connection_read.add(other.connection_read);
        self.timeout_check.add(other.timeout_check);
        self.packet_proof_verify.add(other.packet_proof_verify);
        self.duplicate_sequence_check
            .add(other.duplicate_sequence_check);
        self.app_check.add(other.app_check);
        self.receipt_write.add(other.receipt_write);
        self.app_execute_total.add(other.app_execute_total);
        self.packet_data_decode.add(other.packet_data_decode);
        self.route_resolve.add(other.route_resolve);
        self.compliance_check.add(other.compliance_check);
        self.mint_unescrow_accounting
            .add(other.mint_unescrow_accounting);
        self.register_denom.add(other.register_denom);
        self.value_balance_read.add(other.value_balance_read);
        self.mint_note_total.add(other.mint_note_total);
        self.mint_note_sct_append.add(other.mint_note_sct_append);
        self.mint_note_build.add(other.mint_note_build);
        self.mint_note_add_payload_total
            .add(other.mint_note_add_payload_total);
        self.mint_note_pending_payload
            .add(other.mint_note_pending_payload);
        self.value_balance_write.add(other.value_balance_write);
        self.event_record.add(other.event_record);
        self.app_execute_inner.add(other.app_execute_inner);
        self.acknowledgement_read.add(other.acknowledgement_read);
        self.acknowledgement_write.add(other.acknowledgement_write);
        self.acknowledgement_total.add(other.acknowledgement_total);
        self.deferred_sct_reserve.add(other.deferred_sct_reserve);
        self.deferred_sct_materialize
            .add(other.deferred_sct_materialize);
        self.deferred_sct_pending_payload
            .add(other.deferred_sct_pending_payload);
    }
}

impl StageTimingReport {
    fn add_stage(&mut self, stage: StageTiming) {
        self.count += stage.count;
        self.total_ms += stage.total_us as f64 / 1000.0;
        self.mean_us = if self.count == 0 {
            0.0
        } else {
            self.total_ms * 1000.0 / self.count as f64
        };
    }

    fn add(&mut self, other: StageTimingReport) {
        self.count += other.count;
        self.total_ms += other.total_ms;
        self.mean_us = if self.count == 0 {
            0.0
        } else {
            self.total_ms * 1000.0 / self.count as f64
        };
    }
}

fn apply_prepare_profile(report: &mut DetailedRunReport, profile: &PrepareProposalProfile) {
    report.prepare_zk_batch_verify_ms += profile.artifact_fill_batch_verify_ms;
    report.prepare_stateful_filter_ms += profile.stateful_filter_execute_ms;
    report.prepare_aggregate_build_ms += profile.total_aggregate_ms;
}

fn apply_process_profile(report: &mut DetailedRunReport, profile: &ProcessProposalProfile) {
    report.process_aggregate_verify_ms += profile.aggregate_verify_ms;
    report.process_cold_reconstruction_zk_ms += profile.cold_reconstruction_batch_verify_ms;
    report.process_stateful_replay_ms += profile.stateful_replay_execute_ms;
}

fn apply_execution_profile(report: &mut DetailedRunReport, profile: &ExecutionBlockProfile) {
    report.execute_block_tx_count += profile.block_tx_count;
    report.execute_begin_block_ms += profile.begin_block_ms;
    report.execute_deliver_txs_wall_ms += profile.deliver_txs_wall_ms;
    report.execute_execute_tx_ms += profile.execute_tx_ms;
    report.execute_begin_state_tx_ms += profile.begin_state_tx_ms;
    report.execute_index_tx_ms += profile.index_tx_ms;
    report.execute_get_block_height_ms += profile.get_block_height_ms;
    report.execute_clone_tx_ms += profile.clone_tx_ms;
    report.execute_proto_convert_ms += profile.proto_convert_ms;
    report.execute_put_block_transaction_ms += profile.put_block_transaction_ms;
    report.execute_tx_log_read_ms += profile.tx_log_read_ms;
    report.execute_tx_log_encode_ms += profile.tx_log_encode_ms;
    report.execute_tx_log_put_raw_ms += profile.tx_log_put_raw_ms;
    report.execute_check_and_execute_ms += profile.check_and_execute_ms;
    report.execute_set_source_ms += profile.set_source_ms;
    report.execute_pay_fee_ms += profile.pay_fee_ms;
    report.execute_historical_state_ms += profile.read_local_precheck_ms
        + profile.read_lookup_wait_or_join_ms
        + profile.read_historical_check_ms
        + profile.read_nullifier_wait_ms
        + profile.read_anchor_cache_wait_ms;
    report.execute_read_local_precheck_ms += profile.read_local_precheck_ms;
    report.execute_read_lookup_wait_or_join_ms += profile.read_lookup_wait_or_join_ms;
    report.execute_read_historical_check_ms += profile.read_historical_check_ms;
    report.execute_read_nullifier_wait_ms += profile.read_nullifier_wait_ms;
    report.execute_read_anchor_cache_wait_ms += profile.read_anchor_cache_wait_ms;
    report.execute_action_ms += profile.action_execute_ms;
    report.execute_commit_ms += profile.commit_ms;
    report.execute_end_block_ms += profile.end_block_ms;
    report.execute_spend_action_ms += profile.spend_action_execute_ms;
    report.execute_spend_nullifier_tx_local_scan_ms += profile.spend_nullifier_tx_local_scan_ms;
    report.execute_spend_nullifier_block_log_lookup_ms +=
        profile.spend_nullifier_block_log_lookup_ms;
    report.execute_spend_nullifier_committed_check_ms += profile.spend_nullifier_committed_check_ms;
    report.execute_spend_nullifier_enqueue_ms += profile.spend_nullifier_enqueue_ms;
    report.execute_spend_nullifier_stage_ms += profile.spend_nullifier_stage_ms;
    report.execute_spend_nullifier_merge_ms += profile.spend_nullifier_merge_ms;
    report.execute_nullifier_lookup_count += profile.nullifier_lookup_count;
    report.execute_output_action_ms += profile.output_action_execute_ms;
    report.execute_output_write_ms += profile.output_add_note_payload_ms;
    report.execute_other_action_ms += profile.other_action_execute_ms;
    report.execute_record_clues_ms += profile.record_clues_ms;
    report.execute_apply_ms += profile.apply_ms;
    report.execute_nullifier_check_ms += profile.spend_nullifier_check_ms;
}

fn prepare_request(txs: &[Vec<u8>]) -> request::PrepareProposal {
    request::PrepareProposal {
        txs: txs.iter().cloned().map(Bytes::from).collect(),
        max_tx_bytes: i64::MAX / 4,
        local_last_commit: None,
        misbehavior: Vec::new(),
        height: block::Height::from(2u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    }
}

fn process_request_from_prepare_response(
    prepared: &response::PrepareProposal,
) -> request::ProcessProposal {
    request::ProcessProposal {
        txs: prepared.txs.clone(),
        proposed_last_commit: None,
        misbehavior: Vec::new(),
        hash: Hash::None,
        height: block::Height::from(2u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    }
}

fn ensure_prepare_preserved_user_txs(
    input_txs: &[Vec<u8>],
    prepared: &response::PrepareProposal,
) -> Result<()> {
    anyhow::ensure!(
        prepared.txs.len() >= input_txs.len(),
        "prepared proposal dropped txs: input={}, prepared={}",
        input_txs.len(),
        prepared.txs.len()
    );
    for (index, expected) in input_txs.iter().enumerate() {
        anyhow::ensure!(
            prepared.txs[index].as_ref() == expected.as_slice(),
            "prepare proposal changed or reordered user tx ordinal {index}"
        );
    }
    Ok(())
}

fn candidate_envelope_from_txs(txs: &[Vec<u8>]) -> Result<CandidateEnvelope> {
    let tx_hashes = txs
        .iter()
        .map(|tx_bytes| sha2::Sha256::digest(tx_bytes).into())
        .collect::<Vec<[u8; 32]>>();
    Ok(CandidateEnvelope {
        txs: txs.to_vec(),
        tx_hashes: tx_hashes.clone(),
        aggregate_bundle_tx_bytes: None,
        sidecar: ProposalArtifactSidecar::build(&[], 0, vec![])?.to_record(),
        segment_tx_counts: Vec::new(),
        block_tx_count: txs.len(),
        total_payload_bytes: txs.iter().map(Vec::len).sum(),
        candidate_digest: candidate_digest_from_hashes(&tx_hashes),
        source_builder_label: "ibc_vs_transfer_profiled_execute".to_string(),
    })
}

async fn execute_txs(chain: &mut TestNodeWithIBC, txs: Vec<Vec<u8>>) -> Result<BlockMeasurement> {
    let tx_count = txs.len();
    let start = Instant::now();
    let (_end_block, deliver_events) = chain.node.block().with_data(txs).execute().await?;
    let wall_ms = elapsed_ms(start);
    let send_packet_count = deliver_events
        .0
        .iter()
        .filter(|event| event.kind == "send_packet")
        .count();
    let write_ack_count = deliver_events
        .0
        .iter()
        .filter(|event| event.kind == "write_acknowledgement")
        .count();
    Ok(BlockMeasurement {
        tx_count,
        wall_ms,
        send_packet_count,
        write_ack_count,
        detailed: DetailedRunReport::default(),
    })
}

fn validate_corpus(corpus: &BenchmarkCorpus) -> Result<()> {
    anyhow::ensure!(
        corpus.state_setup_tx_count() >= corpus.tx_count,
        "corpus state setup count {} is smaller than requested tx count {}",
        corpus.state_setup_tx_count(),
        corpus.tx_count
    );
    anyhow::ensure!(
        corpus.inner_transfer_txs_hex.len() == corpus.tx_count,
        "inner corpus count mismatch"
    );
    anyhow::ensure!(
        corpus.outbound_shielded_txs_hex.len() == corpus.tx_count,
        "outbound corpus count mismatch"
    );
    anyhow::ensure!(
        corpus.recv_packet_txs_hex.len() == corpus.tx_count,
        "recv corpus count mismatch"
    );
    validate_txs("inner_transfer", &corpus.inner_transfer_txs())?;
    validate_txs("outbound_shielded_ics20", &corpus.outbound_shielded_txs())?;
    validate_txs("recv_packet", &corpus.recv_packet_txs())?;
    validate_txs("update_client_b", &[corpus.update_client_b_tx()])?;
    Ok(())
}

fn validate_txs(name: &str, txs: &[Vec<u8>]) -> Result<()> {
    let mut seen_hashes = BTreeSet::new();
    let mut seen_nullifiers = BTreeSet::new();
    for (index, bytes) in txs.iter().enumerate() {
        let hash = hex::encode(sha2::Sha256::digest(bytes));
        anyhow::ensure!(
            seen_hashes.insert(hash),
            "{name} duplicate tx hash at {index}"
        );
        let tx = Transaction::decode(bytes.as_slice())
            .with_context(|| format!("decoding {name} tx {index}"))?;
        anyhow::ensure!(
            tx.encode_to_vec() == *bytes,
            "{name} tx {index} decode round-trip mismatch"
        );
        for nullifier in tx.spent_nullifiers() {
            anyhow::ensure!(
                seen_nullifiers.insert(nullifier),
                "{name} duplicate nullifier at tx {index}"
            );
        }
    }
    Ok(())
}

fn summarize(runs: &[RunReport]) -> ScenarioSummary {
    let mean_execute_wall_ms = mean(runs.iter().map(|run| run.execute_wall_ms));
    let mean_ms_per_tx = mean(runs.iter().map(|run| run.ms_per_tx));
    let mut ms_per_tx = runs.iter().map(|run| run.ms_per_tx).collect::<Vec<_>>();
    ScenarioSummary {
        mean_execute_wall_ms,
        mean_tps: mean(runs.iter().map(|run| run.tps)),
        mean_ms_per_tx,
        p50_ms_per_tx: percentile(&mut ms_per_tx, 0.50),
        p95_ms_per_tx: percentile(&mut ms_per_tx, 0.95),
        p99_ms_per_tx: percentile(&mut ms_per_tx, 0.99),
        projected_5000_tx_ms_from_mean: mean_ms_per_tx * 5_000.0,
        mean_direct_zk_batch_verify_ms: mean(
            runs.iter()
                .map(|run| run.detailed.direct_zk_batch_verify_ms),
        ),
        mean_prepare_proposal_ms: mean(
            runs.iter().map(|run| run.detailed.prepare_proposal_wall_ms),
        ),
        mean_process_proposal_ms: mean(
            runs.iter().map(|run| run.detailed.process_proposal_wall_ms),
        ),
        mean_profiled_execute_ms: mean(
            runs.iter().map(|run| run.detailed.execute_profiled_wall_ms),
        ),
        mean_checktx_cold_ms: mean(runs.iter().map(|run| run.detailed.checktx_cold_wall_ms)),
        mean_checktx_warm_ms: mean(runs.iter().map(|run| run.detailed.checktx_warm_wall_ms)),
        mean_delay_block_ms: mean(runs.iter().map(|run| run.detailed.delay_block_wall_ms)),
    }
}

fn encode_txs(txs: &[Vec<u8>]) -> Vec<String> {
    txs.iter().map(hex::encode).collect()
}

fn decode_txs(txs: &[String]) -> Vec<Vec<u8>> {
    txs.iter()
        .map(|tx| hex::decode(tx).expect("corpus tx hex must decode"))
        .collect()
}

fn corpus_fingerprint(tx_count: usize) -> Result<String> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(SCHEMA_VERSION.to_le_bytes());
    hasher.update((tx_count as u64).to_le_bytes());
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(transfer_verifying_key_digest()?.as_bytes());
    hasher.update(shielded_ics20_verifying_key_digest()?.as_bytes());
    hasher.update(bytes_digest(penumbra_sdk_proof_params::transfer_proving_key_bytes()).as_bytes());
    hasher.update(
        bytes_digest(
            penumbra_sdk_proof_params::shielded_ics20_withdrawal_proving_key_bytes(
                penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical.get(),
            ),
        )
        .as_bytes(),
    );
    hasher.update(penumbra_sdk_proof_params::transfer_circuit_metadata());
    hasher.update(
        penumbra_sdk_proof_params::shielded_ics20_withdrawal_circuit_metadata(
            penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical.get(),
        ),
    );
    if let Some(commit) = git_commit() {
        hasher.update(commit.as_bytes());
    }
    hasher.update(git_tree_state().as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

fn transfer_verifying_key_digest() -> Result<String> {
    let mut bytes = Vec::new();
    penumbra_sdk_proof_params::transfer_proof_verification_key()
        .serialize_compressed(&mut bytes)?;
    Ok(bytes_digest(&bytes))
}

fn shielded_ics20_verifying_key_digest() -> Result<String> {
    let mut bytes = Vec::new();
    penumbra_sdk_proof_params::shielded_ics20_withdrawal_proof_verification_key(
        penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical.get(),
    )
    .serialize_compressed(&mut bytes)?;
    Ok(bytes_digest(&bytes))
}

fn bytes_digest(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

fn render_console_summary(report: &BenchmarkReport) -> String {
    report
        .scenarios
        .iter()
        .map(|scenario| {
            format!(
                "{}: {:.2} TPS, {:.3}ms/tx, projected 5k {:.1}ms",
                scenario.name,
                scenario.summary.mean_tps,
                scenario.summary.mean_ms_per_tx,
                scenario.summary.projected_5000_tx_ms_from_mean
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_markdown(report: &BenchmarkReport) -> String {
    let mut out = format!(
        "# IBC vs Inner Transfer Benchmark Results\n\n\
         Timed sections execute prebuilt transaction bytes against fresh deterministic chain state. \
         Transaction construction and proof generation are excluded.\n\n\
         Corpus: `{}` ({}, {:.1} ms load/build)\n\
         Setup cache: `{}` ({}, {:.1} ms load/build)\n\
         Timed tx count: `{}`; state setup tx count: `{}`\n\n\
         Prover-gated tests: {}\n\n\
         | Scenario | TPS | ms/tx | p95 ms/tx | projected 5k ms |\n\
         |---|---:|---:|---:|---:|\n",
        report.corpus_path.display(),
        report.corpus_source,
        report.corpus_load_or_build_wall_ms,
        report.setup_cache_path.display(),
        report.setup_cache_source,
        report.setup_cache_load_or_build_wall_ms,
        report.tx_count,
        report.state_setup_tx_count,
        report.prover_gated_test_status,
    );
    for scenario in &report.scenarios {
        out.push_str(&format!(
            "| {} | {:.2} | {:.3} | {:.3} | {:.1} |\n",
            scenario.name,
            scenario.summary.mean_tps,
            scenario.summary.mean_ms_per_tx,
            scenario.summary.p95_ms_per_tx,
            scenario.summary.projected_5000_tx_ms_from_mean
        ));
    }
    out.push_str(
        "\n## High-Level Breakdown\n\n\
         | Scenario | direct ZK verify ms | CheckTx cold ms | CheckTx warm ms | PrepareProposal ms | ProcessProposal ms | Execute/Commit ms | IBC delay block ms |\n\
         |---|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for scenario in &report.scenarios {
        out.push_str(&format!(
            "| {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} |\n",
            scenario.name,
            scenario.summary.mean_direct_zk_batch_verify_ms,
            scenario.summary.mean_checktx_cold_ms,
            scenario.summary.mean_checktx_warm_ms,
            scenario.summary.mean_prepare_proposal_ms,
            scenario.summary.mean_process_proposal_ms,
            scenario.summary.mean_profiled_execute_ms,
            scenario.summary.mean_delay_block_ms,
        ));
    }
    out.push_str(
        "\n## Transaction Shape Counts\n\n\
         | Scenario | tx/run | transfer actions/run | transfer input slots/run | real transfer inputs/run | dummy transfer inputs/run | transfer output slots/run | real transfer outputs/run | dummy transfer outputs/run | IBC relay actions/run |\n\
         |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for scenario in &report.scenarios {
        out.push_str(&format!(
            "| {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} |\n",
            scenario.name,
            mean_detail(scenario, |d| d.profiled_tx_count as f64),
            mean_detail(scenario, |d| d.transfer_action_count as f64),
            mean_detail(scenario, |d| d.transfer_input_slots as f64),
            mean_detail(scenario, |d| d.transfer_real_inputs as f64),
            mean_detail(scenario, |d| d.transfer_dummy_inputs as f64),
            mean_detail(scenario, |d| d.transfer_output_slots as f64),
            mean_detail(scenario, |d| d.transfer_real_outputs as f64),
            mean_detail(scenario, |d| d.transfer_dummy_outputs as f64),
            mean_detail(scenario, |d| d.ibc_relay_action_count as f64),
        ));
    }
    out.push_str(
        "\n## Proof And Proposal Breakdown\n\n\
         | Scenario | Stage | total ms/run | ms/tx |\n\
         |---|---|---:|---:|\n",
    );
    for scenario in &report.scenarios {
        append_detail_ms_row(&mut out, scenario, "direct artifact extract", |d| {
            d.direct_artifact_extract_ms
        });
        append_detail_ms_row(&mut out, scenario, "direct artifact action extract", |d| {
            d.direct_artifact_extract_action_ms
        });
        append_detail_ms_row(&mut out, scenario, "direct artifact public extract", |d| {
            d.direct_artifact_extract_public_ms
        });
        append_detail_ms_row(&mut out, scenario, "direct artifact to batch item", |d| {
            d.direct_artifact_to_batch_item_ms
        });
        append_detail_ms_row(&mut out, scenario, "direct ZK batch verify", |d| {
            d.direct_zk_batch_verify_ms
        });
        append_detail_ms_row(&mut out, scenario, "CheckTx cold", |d| {
            d.checktx_cold_wall_ms
        });
        append_detail_ms_row(&mut out, scenario, "CheckTx warm", |d| {
            d.checktx_warm_wall_ms
        });
        append_detail_ms_row(&mut out, scenario, "PrepareProposal wall", |d| {
            d.prepare_proposal_wall_ms
        });
        append_detail_ms_row(&mut out, scenario, "PrepareProposal ZK verify", |d| {
            d.prepare_zk_batch_verify_ms
        });
        append_detail_ms_row(&mut out, scenario, "PrepareProposal stateful filter", |d| {
            d.prepare_stateful_filter_ms
        });
        append_detail_ms_row(&mut out, scenario, "PrepareProposal aggregate build", |d| {
            d.prepare_aggregate_build_ms
        });
        append_detail_ms_row(&mut out, scenario, "ProcessProposal wall", |d| {
            d.process_proposal_wall_ms
        });
        append_detail_ms_row(
            &mut out,
            scenario,
            "ProcessProposal aggregate verify",
            |d| d.process_aggregate_verify_ms,
        );
        append_detail_ms_row(
            &mut out,
            scenario,
            "ProcessProposal cold ZK reconstruction",
            |d| d.process_cold_reconstruction_zk_ms,
        );
        append_detail_ms_row(&mut out, scenario, "ProcessProposal stateful replay", |d| {
            d.process_stateful_replay_ms
        });
    }
    out.push_str(
        "\n## Execute Block Breakdown\n\n\
         | Scenario | Stage | total ms/run | ms/tx |\n\
         |---|---|---:|---:|\n",
    );
    for scenario in &report.scenarios {
        append_detail_ms_row(&mut out, scenario, "execute wall", |d| {
            d.execute_profiled_wall_ms
        });
        append_detail_ms_row(&mut out, scenario, "begin block", |d| {
            d.execute_begin_block_ms
        });
        append_detail_ms_row(&mut out, scenario, "deliver txs wall", |d| {
            d.execute_deliver_txs_wall_ms
        });
        append_detail_ms_row(&mut out, scenario, "execute tx total", |d| {
            d.execute_execute_tx_ms
        });
        append_detail_ms_row(&mut out, scenario, "begin state tx", |d| {
            d.execute_begin_state_tx_ms
        });
        append_detail_ms_row(&mut out, scenario, "index tx", |d| d.execute_index_tx_ms);
        append_detail_ms_row(&mut out, scenario, "check and execute", |d| {
            d.execute_check_and_execute_ms
        });
        append_detail_ms_row(&mut out, scenario, "historical/read total", |d| {
            d.execute_historical_state_ms
        });
        append_detail_ms_row(&mut out, scenario, "read local precheck", |d| {
            d.execute_read_local_precheck_ms
        });
        append_detail_ms_row(&mut out, scenario, "read lookup wait/join", |d| {
            d.execute_read_lookup_wait_or_join_ms
        });
        append_detail_ms_row(&mut out, scenario, "read historical check", |d| {
            d.execute_read_historical_check_ms
        });
        append_detail_ms_row(&mut out, scenario, "read nullifier wait", |d| {
            d.execute_read_nullifier_wait_ms
        });
        append_detail_ms_row(&mut out, scenario, "read anchor cache wait", |d| {
            d.execute_read_anchor_cache_wait_ms
        });
        append_detail_ms_row(&mut out, scenario, "action execute", |d| {
            d.execute_action_ms
        });
        append_detail_ms_row(&mut out, scenario, "spend action", |d| {
            d.execute_spend_action_ms
        });
        append_detail_ms_row(&mut out, scenario, "nullifier check", |d| {
            d.execute_nullifier_check_ms
        });
        append_detail_ms_row(&mut out, scenario, "nullifier committed check", |d| {
            d.execute_spend_nullifier_committed_check_ms
        });
        append_detail_ms_row(&mut out, scenario, "nullifier event/enqueue", |d| {
            d.execute_spend_nullifier_enqueue_ms
        });
        append_detail_ms_row(&mut out, scenario, "output action", |d| {
            d.execute_output_action_ms
        });
        append_detail_ms_row(&mut out, scenario, "output payload staging", |d| {
            d.execute_output_write_ms
        });
        append_detail_ms_row(&mut out, scenario, "other action", |d| {
            d.execute_other_action_ms
        });
        append_detail_ms_row(&mut out, scenario, "record clues", |d| {
            d.execute_record_clues_ms
        });
        append_detail_ms_row(&mut out, scenario, "state tx apply", |d| d.execute_apply_ms);
        append_detail_ms_row(&mut out, scenario, "end block", |d| d.execute_end_block_ms);
        append_detail_ms_row(&mut out, scenario, "commit", |d| d.execute_commit_ms);
    }
    if report
        .scenarios
        .iter()
        .any(|scenario| mean_inbound_stage(scenario, |b| b.app_execute_total).0 > 0.0)
    {
        out.push_str(
            "\n## Inbound Receive Execute Breakdown\n\n\
             | Scenario | Stage | calls/run | total ms/run | us/call |\n\
             |---|---|---:|---:|---:|\n",
        );
        for scenario in &report.scenarios {
            if mean_inbound_stage(scenario, |b| b.app_execute_total).0 == 0.0 {
                continue;
            }
            append_inbound_stage_row(&mut out, scenario, "channel read", |b| b.channel_read);
            append_inbound_stage_row(&mut out, scenario, "connection read", |b| b.connection_read);
            append_inbound_stage_row(&mut out, scenario, "timeout check", |b| b.timeout_check);
            append_inbound_stage_row(&mut out, scenario, "packet proof verify", |b| {
                b.packet_proof_verify
            });
            append_inbound_stage_row(&mut out, scenario, "duplicate/sequence check", |b| {
                b.duplicate_sequence_check
            });
            append_inbound_stage_row(&mut out, scenario, "receipt write", |b| b.receipt_write);
            append_inbound_stage_row(&mut out, scenario, "packet data decode", |b| {
                b.packet_data_decode
            });
            append_inbound_stage_row(&mut out, scenario, "route resolve", |b| b.route_resolve);
            append_inbound_stage_row(&mut out, scenario, "compliance check", |b| {
                b.compliance_check
            });
            append_inbound_stage_row(&mut out, scenario, "mint/unescrow/accounting", |b| {
                b.mint_unescrow_accounting
            });
            append_inbound_stage_row(&mut out, scenario, "register denom", |b| b.register_denom);
            append_inbound_stage_row(&mut out, scenario, "value balance read", |b| {
                b.value_balance_read
            });
            append_inbound_stage_row(&mut out, scenario, "mint note total", |b| b.mint_note_total);
            append_inbound_stage_row(&mut out, scenario, "mint note sct append", |b| {
                b.mint_note_sct_append
            });
            append_inbound_stage_row(&mut out, scenario, "mint note build", |b| b.mint_note_build);
            append_inbound_stage_row(&mut out, scenario, "mint note add payload total", |b| {
                b.mint_note_add_payload_total
            });
            append_inbound_stage_row(&mut out, scenario, "mint note pending payload", |b| {
                b.mint_note_pending_payload
            });
            append_inbound_stage_row(&mut out, scenario, "block sct reserve", |b| {
                b.deferred_sct_reserve
            });
            append_inbound_stage_row(&mut out, scenario, "block sct materialize", |b| {
                b.deferred_sct_materialize
            });
            append_inbound_stage_row(&mut out, scenario, "block sct pending payload", |b| {
                b.deferred_sct_pending_payload
            });
            append_inbound_stage_row(&mut out, scenario, "value balance write", |b| {
                b.value_balance_write
            });
            append_inbound_stage_row(&mut out, scenario, "event record", |b| b.event_record);
            append_inbound_stage_row(&mut out, scenario, "app execute inner", |b| {
                b.app_execute_inner
            });
            append_inbound_stage_row(&mut out, scenario, "ack read", |b| b.acknowledgement_read);
            append_inbound_stage_row(&mut out, scenario, "ack write", |b| b.acknowledgement_write);
            append_inbound_stage_row(&mut out, scenario, "ack total", |b| b.acknowledgement_total);
            append_inbound_stage_row(&mut out, scenario, "recv app execute total", |b| {
                b.app_execute_total
            });
        }
    }
    out
}

fn append_detail_ms_row(
    out: &mut String,
    scenario: &ScenarioReport,
    label: &str,
    accessor: impl Fn(&DetailedRunReport) -> f64,
) {
    let total_ms = mean_detail(scenario, accessor);
    let tx_count = mean_detail(scenario, |d| d.profiled_tx_count as f64);
    let ms_per_tx = if tx_count == 0.0 {
        0.0
    } else {
        total_ms / tx_count
    };
    out.push_str(&format!(
        "| {} | {} | {:.3} | {:.6} |\n",
        scenario.name, label, total_ms, ms_per_tx
    ));
}

fn mean_detail(scenario: &ScenarioReport, accessor: impl Fn(&DetailedRunReport) -> f64) -> f64 {
    mean(scenario.runs.iter().map(|run| accessor(&run.detailed)))
}

fn append_inbound_stage_row(
    out: &mut String,
    scenario: &ScenarioReport,
    label: &str,
    accessor: impl Fn(&InboundReceiveBreakdownReport) -> StageTimingReport,
) {
    let (count, total_ms, mean_us) = mean_inbound_stage(scenario, accessor);
    out.push_str(&format!(
        "| {} | {} | {:.1} | {:.3} | {:.3} |\n",
        scenario.name, label, count, total_ms, mean_us
    ));
}

fn mean_inbound_stage(
    scenario: &ScenarioReport,
    accessor: impl Fn(&InboundReceiveBreakdownReport) -> StageTimingReport,
) -> (f64, f64, f64) {
    let count = mean(
        scenario
            .runs
            .iter()
            .map(|run| accessor(&run.detailed.inbound_receive).count as f64),
    );
    let total_ms = mean(
        scenario
            .runs
            .iter()
            .map(|run| accessor(&run.detailed.inbound_receive).total_ms),
    );
    let mean_us = if count == 0.0 {
        0.0
    } else {
        total_ms * 1000.0 / count
    };
    (count, total_ms, mean_us)
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let mut count = 0usize;
    let mut sum = 0.0;
    for value in values {
        count += 1;
        sum += value;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn percentile(values: &mut [f64], percentile: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    let index = ((values.len().saturating_sub(1)) as f64 * percentile).ceil() as usize;
    values.get(index).copied().unwrap_or_default()
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn env_usize(name: &str, default: usize) -> Result<usize> {
    Ok(std::env::var(name)
        .ok()
        .map(|value| value.parse::<usize>())
        .transpose()
        .with_context(|| format!("parsing {name}"))?
        .unwrap_or(default))
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| env_bool_value(&value).ok())
        .unwrap_or(default)
}

fn env_bool_value(value: &str) -> Result<bool> {
    Ok(matches!(value, "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn git_commit() -> Option<String> {
    git_output(["rev-parse", "HEAD"])
}

fn git_tree_state() -> String {
    match git_output(["status", "--porcelain", "--untracked-files=no"]) {
        Some(output) if output.trim().is_empty() => "clean".to_string(),
        Some(_) => "dirty".to_string(),
        None => "unavailable".to_string(),
    }
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
