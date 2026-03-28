use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use cnidarium::Storage;
use penumbra_sdk_app::app::{App, CheckTxSharedContext};
use penumbra_sdk_app::stateless_cache::StatelessCache;
use penumbra_sdk_app::block_tx_indexing::BlockTxIndexingMode;
use penumbra_sdk_app::SUBSTORE_PREFIXES;
use penumbra_sdk_bench::lookahead_builder::build_candidate_from_frozen_unverified;
use penumbra_sdk_bench::mempool::{apply_synthetic_fee_mode, SyntheticFeeMode};
use penumbra_sdk_bench::single_builder::SingleBuilderMode;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_poc_preconsensus::local_mempool::{
    AdmitOutcome, AdmittedRecord, EvictionPolicy, FeeEvictionPolicy, MempoolCoreConfig,
    MempoolHandle,
};
use sha2::Digest as _;

#[derive(Debug, Parser)]
#[clap(name = "corpus_mempool_once")]
#[clap(about = "Admit a corpus into the mempool, freeze one candidate, build, and verify once")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long)]
    rocksdb_home: Option<PathBuf>,
    #[clap(long, default_value = "strict-mempool")]
    mode: String,
    #[clap(long, default_value_t = 100)]
    max_block_txs: usize,
    #[clap(long, default_value_t = 32)]
    segment_tx_count: usize,
    #[clap(long, default_value_t = 500_000)]
    max_proposal_bytes: usize,
    #[clap(long, default_value_t = 268_435_456)]
    max_store_bytes: usize,
    #[clap(long, default_value_t = 40_000)]
    max_store_txs: usize,
    #[clap(long, default_value_t = 8)]
    rayon_threads_per_batch: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    unsafe {
        std::env::set_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP", "1");
    }

    let cli = Cli::parse();
    let mode: SingleBuilderMode = cli.mode.parse()?;
    penumbra_sdk_proof_aggregation::set_rayon_threads_per_batch_for_bench(
        cli.rayon_threads_per_batch,
    );

    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;
    let rocksdb_home = cli.rocksdb_home.unwrap_or_else(default_rocksdb_home);
    let storage = Storage::load(rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
        .await
        .with_context(|| format!("loading RocksDB from {}", rocksdb_home.display()))?;
    let snapshot = storage.latest_snapshot();
    let snapshot_version = snapshot.version();
    let shared_context = Arc::new(CheckTxSharedContext::load(&snapshot).await?);
    let stateless_cache = Arc::new(StatelessCache::new());
    let mempool = MempoolHandle::new(MempoolCoreConfig {
        max_store_bytes: cli.max_store_bytes,
        max_store_txs: cli.max_store_txs,
        ingestion_buffer: 64,
        command_buffer: 256,
        eviction_policy: EvictionPolicy::OldestUnreservedFirst,
        fee_eviction_policy: FeeEvictionPolicy::Disabled,
    });

    let mut admitted = 0usize;
    let mut rejected = 0usize;
    for (seq, entry) in corpus.entries.iter().enumerate() {
        let tx_bytes = Arc::new(entry.tx_bytes.clone());
        let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
        let mut app = App::new(snapshot.clone());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        app.set_checktx_shared_context(shared_context.clone());

        let deliver = match mode {
            SingleBuilderMode::StrictMempool => {
                app.deliver_tx_bytes_v2_profiled(
                    tx_bytes.as_slice(),
                    Some(stateless_cache.as_ref()),
                )
                .await
            }
            SingleBuilderMode::OptimisticBuilder => {
                app.deliver_tx_bytes_v2_extracted_profiled_for_bench(
                    tx_bytes.as_slice(),
                    stateless_cache.as_ref(),
                )
                .await
            }
        };

        match deliver {
            Ok((_events, _profile)) => {
                let artifact = stateless_cache
                    .get(&tx_hash)
                    .and_then(|entry| entry.artifact())
                    .with_context(|| {
                        format!(
                            "missing cached artifact after successful CheckTx for {}",
                            hex::encode(tx_hash)
                        )
                    })?;
                let record = Arc::new(apply_synthetic_fee_mode(
                    AdmittedRecord::from_tx_bytes(seq as u64, tx_bytes, artifact, snapshot_version),
                    SyntheticFeeMode::Off,
                ));
                match mempool.submit_admitted(record).await? {
                    AdmitOutcome::Admitted { .. } => admitted += 1,
                    _ => rejected += 1,
                }
            }
            Err(_) => rejected += 1,
        }
    }

    println!("admitted_total={admitted}");
    println!("rejected_total={rejected}");
    let frozen = mempool
        .freeze_next_candidate(1, cli.max_block_txs, cli.max_proposal_bytes)
        .await?
        .context("no candidate frozen")?;
    println!(
        "frozen_reserved_tx_count={} frozen_reserved_bytes={}",
        frozen.reserved_tx_count, frozen.reserved_bytes
    );

    let built =
        build_candidate_from_frozen_unverified(frozen.clone(), cli.segment_tx_count).await?;
    println!(
        "built_segment_tx_counts={:?} aggregate_total_ms={:.3} sidecar_build_ms={:.3}",
        built.segment_tx_counts, built.aggregate_total_ms, built.sidecar_build_ms
    );
    let artifacts = built
        .frozen
        .records
        .iter()
        .map(|record| record.artifact.clone())
        .collect::<Vec<_>>();
    App::verify_aggregate_bundle_for_artifacts_public(
        &artifacts,
        &built.bundle,
        Some(&built.segment_tx_counts),
    )
    .await?;
    println!("verify_ok=true");
    Ok(())
}

fn default_rocksdb_home() -> PathBuf {
    let mut home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.push(".penumbra/network_data/node0/pd/rocksdb");
    home
}
