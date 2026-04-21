use std::fs::File;
use std::io::{Read, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use cnidarium::TempStorage;
use penumbra_sdk_app::{
    genesis::{AppState, Content},
    server::consensus::{Consensus, ConsensusService},
    APP_VERSION, SUBSTORE_PREFIXES,
};
use penumbra_sdk_asset::BASE_ASSET_DENOM;
use penumbra_sdk_keys::test_keys;
use penumbra_sdk_mock_client::MockClient;
use penumbra_sdk_mock_consensus::TestNode;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_shielded_pool::{
    genesis::Allocation, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan,
};
use penumbra_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, Transaction, TransactionParameters, TransactionPlan,
};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const POOL_SCHEMA_VERSION: u32 = 2;
const POOL_TX_SHAPE: &str = "synthetic-preconsensus-transfer-v2";
const DEFAULT_SHARD_TX_COUNT: usize = 1_000;
const SYNTHETIC_BENCHMARK_TIME_RFC3339: &str = "2026-01-01T00:00:00Z";

#[derive(Clone)]
pub struct ProofTxPool {
    pub txs: Vec<Arc<Vec<u8>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProofTxPoolMetadata {
    pub schema_version: u32,
    pub created_at: u64,
    pub chain_id: String,
    pub tx_shape: String,
    pub tx_count: usize,
    pub shard_count: usize,
    pub shard_tx_count: usize,
    pub compression: String,
    pub benchmark_time_rfc3339: String,
    pub compatibility_fingerprint: String,
    pub tx_hashes: Vec<String>,
    pub raw_bytes: usize,
    pub compressed_bytes: u64,
}

pub async fn setup_proof_storage(
    n: usize,
) -> anyhow::Result<(TempStorage, TestNode<ConsensusService>, Arc<MockClient>)> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;

    let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
        raw_amount: 1_000_000u128.into(),
        raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
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
    let initial_time = tendermint::Time::parse_from_rfc3339(SYNTHETIC_BENCHMARK_TIME_RFC3339)
        .context("parsing synthetic benchmark initial timestamp")?;
    let mut test_node = TestNode::builder()
        .single_validator()
        .app_state(app_state_bytes)
        .with_initial_timestamp(initial_time)
        .init_chain(consensus)
        .await?;

    test_node.block().execute().await?;

    let client = Arc::new(
        MockClient::new(test_keys::SPEND_KEY.clone())
            .with_sync_to_storage(&storage)
            .await?,
    );

    Ok((storage, test_node, client))
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

pub async fn build_proof_transactions(
    client: Arc<MockClient>,
    storage: &TempStorage,
    n: usize,
) -> anyhow::Result<Vec<Vec<u8>>> {
    let notes: Vec<_> = client.notes.values().cloned().take(n).collect();
    assert_eq!(notes.len(), n, "expected {n} notes, got {}", notes.len());

    let permits = Arc::new(Semaphore::new(proof_tx_build_concurrency()));
    let snapshot = storage.latest_snapshot();
    let mut tasks = JoinSet::new();

    for (ordinal, note) in notes.into_iter().enumerate() {
        let client = client.clone();
        let permits = permits.clone();
        let snapshot = snapshot.clone();
        tasks.spawn(async move {
            let _permit = permits
                .acquire_owned()
                .await
                .expect("proof tx semaphore should not be closed");
            let position = client
                .position(note.commit())
                .expect("note position exists");

            let mut plan = TransactionPlan {
                actions: vec![TransferPlan::from_spend_output(
                    ShieldedInputPlan::new(&mut OsRng, note.clone(), position).into(),
                    ShieldedOutputPlan::new(
                        &mut OsRng,
                        note.value(),
                        test_keys::ADDRESS_1.deref().clone(),
                    )
                    .into(),
                    Default::default(),
                )?
                .into()],
                fee_funding: None,
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
                .witness_auth_build_with_compliance(&mut plan, snapshot)
                .await?;
            Ok::<(usize, Vec<u8>), anyhow::Error>((ordinal, tx.encode_to_vec()))
        });
    }

    let mut tx_bytes = vec![Vec::new(); n];
    while let Some(joined) = tasks.join_next().await {
        let (ordinal, bytes) = joined.context("waiting for proof tx build task")??;
        tx_bytes[ordinal] = bytes;
    }

    Ok(tx_bytes)
}

pub async fn build_proof_tx_pool(
    client: Arc<MockClient>,
    storage: &TempStorage,
    pool_size: usize,
) -> anyhow::Result<ProofTxPool> {
    let txs = build_proof_transactions(client, storage, pool_size)
        .await?
        .into_iter()
        .map(Arc::new)
        .collect();
    Ok(ProofTxPool { txs })
}

pub fn build_proof_tx_workload(tx_count: usize, pool: &ProofTxPool) -> Vec<Vec<u8>> {
    assert!(tx_count > 0, "tx_count must be positive");
    assert!(
        pool.txs.len() >= tx_count,
        "pre-consensus workloads require at least {tx_count} distinct txs in the pool"
    );

    pool.txs
        .iter()
        .take(tx_count)
        .map(|tx| tx.as_ref().clone())
        .collect()
}

pub fn default_pool_dir(tx_count: usize) -> PathBuf {
    PathBuf::from("tmp")
        .join("pre_consensus_pools")
        .join(tx_count.to_string())
}

pub fn save_proof_tx_pool(out_dir: &Path, pool: &ProofTxPool) -> Result<ProofTxPoolMetadata> {
    std::fs::create_dir_all(out_dir.join("txs"))
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let tx_hashes = pool
        .txs
        .iter()
        .map(|tx| hex::encode(sha2::Sha256::digest(tx.as_slice())))
        .collect::<Vec<_>>();
    let raw_bytes = pool.txs.iter().map(|tx| tx.len()).sum::<usize>();

    let shard_tx_count = DEFAULT_SHARD_TX_COUNT;
    let mut compressed_bytes = 0u64;
    let mut shard_count = 0usize;

    for (shard_index, shard) in pool.txs.chunks(shard_tx_count).enumerate() {
        shard_count += 1;
        let shard_path = out_dir
            .join("txs")
            .join(format!("part-{shard_index:03}.bin.zst"));
        let file = File::create(&shard_path)
            .with_context(|| format!("failed to create {}", shard_path.display()))?;
        let mut encoder = zstd::Encoder::new(file, 3).with_context(|| {
            format!("failed to create zstd encoder for {}", shard_path.display())
        })?;
        for tx in shard {
            encoder
                .write_all(&(tx.len() as u32).to_le_bytes())
                .context("writing tx length prefix")?;
            encoder.write_all(tx).context("writing tx bytes")?;
        }
        let file = encoder.finish().context("finishing zstd encoder")?;
        compressed_bytes += file
            .metadata()
            .with_context(|| format!("reading {}", shard_path.display()))?
            .len();
    }

    let metadata = ProofTxPoolMetadata {
        schema_version: POOL_SCHEMA_VERSION,
        created_at: unix_ts(),
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        tx_shape: POOL_TX_SHAPE.to_string(),
        tx_count: pool.txs.len(),
        shard_count,
        shard_tx_count,
        compression: "zstd".to_string(),
        benchmark_time_rfc3339: SYNTHETIC_BENCHMARK_TIME_RFC3339.to_string(),
        compatibility_fingerprint: compatibility_fingerprint(pool.txs.len()),
        tx_hashes,
        raw_bytes,
        compressed_bytes,
    };

    let metadata_path = out_dir.join("metadata.json");
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?)
        .with_context(|| format!("failed to write {}", metadata_path.display()))?;

    Ok(metadata)
}

pub fn load_proof_tx_pool(pool_dir: &Path) -> Result<(ProofTxPool, ProofTxPoolMetadata)> {
    let metadata = read_metadata(pool_dir)?;
    anyhow::ensure!(
        metadata.schema_version == POOL_SCHEMA_VERSION,
        "unsupported proof pool schema_version={} expected={}",
        metadata.schema_version,
        POOL_SCHEMA_VERSION
    );
    anyhow::ensure!(
        metadata.compatibility_fingerprint == compatibility_fingerprint(metadata.tx_count),
        "proof pool compatibility fingerprint mismatch"
    );

    let mut txs = Vec::with_capacity(metadata.tx_count);
    for shard_index in 0..metadata.shard_count {
        let shard_path = pool_dir
            .join("txs")
            .join(format!("part-{shard_index:03}.bin.zst"));
        let file = File::open(&shard_path)
            .with_context(|| format!("failed to open {}", shard_path.display()))?;
        let mut decoder = zstd::Decoder::new(file).with_context(|| {
            format!("failed to create zstd decoder for {}", shard_path.display())
        })?;
        let mut bytes = Vec::new();
        decoder
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read {}", shard_path.display()))?;
        txs.extend(scan_length_delimited_txs(&bytes)?.into_iter().map(Arc::new));
    }

    anyhow::ensure!(
        txs.len() == metadata.tx_count,
        "proof pool tx_count mismatch: metadata={} loaded={}",
        metadata.tx_count,
        txs.len()
    );

    validate_pool(&txs, &metadata.tx_hashes)?;
    Ok((ProofTxPool { txs }, metadata))
}

pub fn verify_proof_tx_pool(pool_dir: &Path) -> Result<ProofTxPoolMetadata> {
    let (_pool, metadata) = load_proof_tx_pool(pool_dir)?;
    Ok(metadata)
}

fn read_metadata(pool_dir: &Path) -> Result<ProofTxPoolMetadata> {
    let metadata_path = pool_dir.join("metadata.json");
    serde_json::from_slice(
        &std::fs::read(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", metadata_path.display()))
}

fn scan_length_delimited_txs(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut txs = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);
    let total_len = bytes.len() as u64;

    while cursor.position() < total_len {
        let mut len_bytes = [0u8; 4];
        cursor
            .read_exact(&mut len_bytes)
            .context("failed to read tx length prefix")?;
        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut tx = vec![0u8; len];
        cursor
            .read_exact(&mut tx)
            .context("failed to read tx bytes")?;
        txs.push(tx);
    }

    Ok(txs)
}

fn validate_pool(txs: &[Arc<Vec<u8>>], expected_hashes: &[String]) -> Result<()> {
    anyhow::ensure!(
        txs.len() == expected_hashes.len(),
        "proof pool hash count mismatch: expected={} loaded={}",
        expected_hashes.len(),
        txs.len()
    );

    let mut seen_hashes = std::collections::BTreeSet::new();
    let mut seen_nullifiers = std::collections::BTreeSet::new();

    for (index, tx_bytes) in txs.iter().enumerate() {
        let actual_hash = hex::encode(sha2::Sha256::digest(tx_bytes.as_slice()));
        anyhow::ensure!(
            actual_hash == expected_hashes[index],
            "proof pool tx hash mismatch at ordinal {index}: expected={}, got={}",
            expected_hashes[index],
            actual_hash
        );
        anyhow::ensure!(
            seen_hashes.insert(actual_hash.clone()),
            "duplicate tx hash in proof pool: {actual_hash}"
        );

        let tx = Transaction::decode(tx_bytes.as_slice())
            .with_context(|| format!("decoding tx ordinal {index}"))?;
        anyhow::ensure!(
            tx.encode_to_vec() == tx_bytes.as_ref().clone(),
            "tx decode round-trip mismatch at ordinal {index}"
        );

        for nullifier in tx.spent_nullifiers() {
            anyhow::ensure!(
                seen_nullifiers.insert(nullifier),
                "duplicate spend nullifier in proof pool at ordinal {index}"
            );
        }
    }

    Ok(())
}

fn compatibility_fingerprint(tx_count: usize) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(POOL_SCHEMA_VERSION.to_le_bytes());
    hasher.update(TestNode::<()>::CHAIN_ID.as_bytes());
    hasher.update(POOL_TX_SHAPE.as_bytes());
    hasher.update(APP_VERSION.to_le_bytes());
    hasher.update(SYNTHETIC_BENCHMARK_TIME_RFC3339.as_bytes());
    hasher.update((tx_count as u64).to_le_bytes());
    hex::encode(hasher.finalize())
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
