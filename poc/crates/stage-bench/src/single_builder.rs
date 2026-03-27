use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use cnidarium::Snapshot;
use parking_lot::Mutex;
use penumbra_sdk_app::{
    app::{App, CheckTxSharedContext},
    block_tx_indexing::BlockTxIndexingMode,
    stateless_cache::StatelessCache,
};
use penumbra_sdk_poc_preconsensus::local_mempool::{
    AdmitOutcome, AdmittedRecord, EvictionPolicy, FeeEvictionPolicy, MempoolCoreConfig,
    MempoolHandle, MempoolSnapshot,
};
use penumbra_sdk_proof_aggregation::set_rayon_threads_per_batch_for_bench;
use sha2::Digest as _;
use tokio::sync::Notify;

use crate::lookahead_builder::build_candidate_from_frozen_unverified;
use crate::mempool::{apply_synthetic_fee_mode, SyntheticFeeMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SingleBuilderMode {
    StrictMempool,
    OptimisticBuilder,
}

impl SingleBuilderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StrictMempool => "strict-mempool",
            Self::OptimisticBuilder => "optimistic-builder",
        }
    }
}

impl FromStr for SingleBuilderMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "strict-mempool" => Ok(Self::StrictMempool),
            "optimistic-builder" => Ok(Self::OptimisticBuilder),
            other => anyhow::bail!("unknown single-builder mode: {other}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SingleBuilderConfig {
    pub mode: SingleBuilderMode,
    pub offered_tps: usize,
    pub block_interval_ms: u64,
    pub warmup_blocks: usize,
    pub measured_blocks: usize,
    pub max_block_txs: usize,
    pub segment_tx_count: usize,
    pub max_proposal_bytes: usize,
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub synthetic_fee_mode: SyntheticFeeMode,
    pub fee_eviction_policy: FeeEvictionPolicy,
    /// Rayon thread count per aggregation batch. 0 = global pool.
    pub rayon_threads_per_batch: usize,
    /// Debug control: wait for admission to finish before the builder starts ticking.
    pub builder_after_admission: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SingleBuilderResult {
    pub admitted_tps: f64,
    pub admitted_total: u64,
    pub rejected_total: u64,
    pub checktx_p50_ms: f64,
    pub checktx_p95_ms: f64,
    pub checktx_p99_ms: f64,
    pub candidate_count: usize,
    pub empty_turn_count: usize,
    pub selected_tx_count_mean: f64,
    pub selected_payload_bytes_mean: f64,
    pub candidate_ready_ms_mean: f64,
    pub candidate_ready_ms_p50: f64,
    pub candidate_ready_ms_p95: f64,
    pub aggregate_total_ms_mean: f64,
    pub sidecar_build_ms_mean: f64,
    pub aggregate_verify_ms_mean: f64,
    pub commit_candidate_ms_mean: f64,
    pub commit_candidate_ms_p50: f64,
    pub commit_candidate_ms_p95: f64,
    pub block_cycle_ms_mean: f64,
    pub admission_to_candidate_ready_ms_p50: f64,
    pub admission_to_candidate_ready_ms_p95: f64,
    pub final_total_record_count: usize,
    pub final_current_bytes: usize,
    pub peak_txs: usize,
    pub peak_bytes: usize,
    pub peak_nullifier_entries: usize,
    pub peak_reserved_records: usize,
    pub peak_frozen_candidates: usize,
    pub evicted_total: u64,
    pub committed_total: u64,
    pub invalidated_total: u64,
    pub replaced_total: u64,
    pub rejected_full_low_fee_total: u64,
    pub rejected_full_no_evictable_total: u64,
    pub evicted_nonstaking_total: u64,
    pub evicted_lowest_staking_total: u64,
}

#[derive(Default)]
struct AdmissionStats {
    durations_ms: Vec<f64>,
    admitted_total: u64,
    rejected_total: u64,
}

#[derive(Default)]
struct BuilderStats {
    candidate_ready_ms: Vec<f64>,
    commit_candidate_ms: Vec<f64>,
    admission_to_ready_ms: Vec<f64>,
    aggregate_total_ms_sum: f64,
    sidecar_build_ms_sum: f64,
    aggregate_verify_ms_sum: f64,
    selected_tx_count_sum: u64,
    selected_payload_bytes_sum: u64,
    candidate_count: usize,
    empty_turn_count: usize,
}

pub async fn run_single_builder_lab(
    txs: Vec<Arc<Vec<u8>>>,
    snapshot: Snapshot,
    config: SingleBuilderConfig,
) -> Result<SingleBuilderResult> {
    anyhow::ensure!(config.offered_tps > 0, "offered_tps must be > 0");
    anyhow::ensure!(
        config.block_interval_ms > 0,
        "block_interval_ms must be > 0"
    );
    anyhow::ensure!(config.measured_blocks > 0, "measured_blocks must be > 0");

    struct RayonThreadsGuard;
    impl Drop for RayonThreadsGuard {
        fn drop(&mut self) {
            set_rayon_threads_per_batch_for_bench(1);
        }
    }
    set_rayon_threads_per_batch_for_bench(config.rayon_threads_per_batch);
    let _rayon_threads_guard = RayonThreadsGuard;

    let stateless_cache = Arc::new(StatelessCache::new());
    let shared_context = Arc::new(CheckTxSharedContext::load(&snapshot).await?);
    let snapshot_version = snapshot.version();
    let mempool = MempoolHandle::new(MempoolCoreConfig {
        max_store_bytes: config.max_store_bytes,
        max_store_txs: config.max_store_txs,
        ingestion_buffer: 64,
        command_buffer: 256,
        eviction_policy: EvictionPolicy::OldestUnreservedFirst,
        fee_eviction_policy: config.fee_eviction_policy,
    });
    let admitted_at = Arc::new(Mutex::new(HashMap::<[u8; 32], Instant>::new()));
    let txs = Arc::new(txs);
    let next_index = Arc::new(AtomicUsize::new(0));
    let next_seq = Arc::new(AtomicU64::new(0));
    let admission_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let admission_notify = Arc::new(Notify::new());
    let started = tokio::time::Instant::now();
    let total_blocks = config.warmup_blocks + config.measured_blocks;
    let stop_at = started + Duration::from_millis(config.block_interval_ms * total_blocks as u64);

    let admission_task = {
        let txs = txs.clone();
        let next_index = next_index.clone();
        let next_seq = next_seq.clone();
        let snapshot = snapshot.clone();
        let shared_context = shared_context.clone();
        let stateless_cache = stateless_cache.clone();
        let mempool = mempool.clone();
        let admitted_at = admitted_at.clone();
        let admission_done = admission_done.clone();
        let admission_notify = admission_notify.clone();
        tokio::spawn(async move {
            let mut stats = AdmissionStats::default();
            let mut next_release = started;
            let release_interval = Duration::from_secs_f64(1.0 / config.offered_tps as f64);

            loop {
                let idx = next_index.load(Ordering::Relaxed);
                if idx >= txs.len() || tokio::time::Instant::now() >= stop_at {
                    break;
                }

                let now = tokio::time::Instant::now();
                if now < next_release {
                    tokio::time::sleep_until(next_release).await;
                }
                next_release += release_interval;

                let tx_index = next_index.fetch_add(1, Ordering::Relaxed);
                let Some(tx_bytes) = txs.get(tx_index).cloned() else {
                    break;
                };

                let mut app = App::new(snapshot.clone());
                app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
                app.set_checktx_shared_context(shared_context.clone());

                let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
                let deliver_result = match config.mode {
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

                match deliver_result {
                    Ok((_events, profile)) => {
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
                            AdmittedRecord::from_tx_bytes(
                                next_seq.fetch_add(1, Ordering::Relaxed),
                                tx_bytes,
                                artifact,
                                snapshot_version,
                            ),
                            config.synthetic_fee_mode,
                        ));
                        let outcome = mempool.submit_admitted(record).await?;
                        match outcome {
                            AdmitOutcome::Admitted { .. } => {
                                stats.admitted_total += 1;
                                stats.durations_ms.push(profile.checktx_total_wall_ms);
                                admitted_at.lock().insert(tx_hash, Instant::now());
                            }
                            _ => {
                                stats.rejected_total += 1;
                            }
                        }
                    }
                    Err(_) => {
                        stats.rejected_total += 1;
                    }
                }
            }

            admission_done.store(true, Ordering::Release);
            admission_notify.notify_waiters();
            Ok::<AdmissionStats, anyhow::Error>(stats)
        })
    };

    let builder_task = {
        let mempool = mempool.clone();
        let admitted_at = admitted_at.clone();
        let admission_done = admission_done.clone();
        let admission_notify = admission_notify.clone();
        tokio::spawn(async move {
            let mut stats = BuilderStats::default();
            let builder_started = if config.builder_after_admission {
                while !admission_done.load(Ordering::Acquire) {
                    admission_notify.notified().await;
                }
                tokio::time::Instant::now()
            } else {
                started
            };
            for turn in 1..=total_blocks {
                let tick_at =
                    builder_started + Duration::from_millis(config.block_interval_ms * turn as u64);
                tokio::time::sleep_until(tick_at).await;
                let measured_turn = turn > config.warmup_blocks;

                let Some(frozen) = mempool
                    .freeze_next_candidate(
                        turn as u64,
                        config.max_block_txs,
                        config.max_proposal_bytes,
                    )
                    .await?
                else {
                    if measured_turn {
                        stats.empty_turn_count += 1;
                    }
                    continue;
                };

                let build_started = Instant::now();
                let built =
                    build_candidate_from_frozen_unverified(frozen.clone(), config.segment_tx_count)
                        .await?;
                let build_only_ms = build_started.elapsed().as_secs_f64() * 1000.0;

                let verify_started = Instant::now();
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
                let aggregate_verify_ms = verify_started.elapsed().as_secs_f64() * 1000.0;

                let candidate_ready_ms = match config.mode {
                    SingleBuilderMode::StrictMempool => build_only_ms,
                    SingleBuilderMode::OptimisticBuilder => build_only_ms + aggregate_verify_ms,
                };
                let ready_at = Instant::now();

                if measured_turn {
                    stats.candidate_count += 1;
                    stats.selected_tx_count_sum += built.frozen.reserved_tx_count as u64;
                    stats.selected_payload_bytes_sum += built.frozen.reserved_bytes as u64;
                    stats.candidate_ready_ms.push(candidate_ready_ms);
                    stats.aggregate_total_ms_sum += built.aggregate_total_ms;
                    stats.sidecar_build_ms_sum += built.sidecar_build_ms;
                    stats.aggregate_verify_ms_sum += aggregate_verify_ms;
                    for record in &built.frozen.records {
                        if let Some(admitted) = admitted_at.lock().get(&record.tx_hash).copied() {
                            stats
                                .admission_to_ready_ms
                                .push(ready_at.duration_since(admitted).as_secs_f64() * 1000.0);
                        }
                    }
                }

                let commit_started = Instant::now();
                let _summary = mempool
                    .commit_candidate(built.frozen.lease.candidate_id)
                    .await?;
                let commit_ms = commit_started.elapsed().as_secs_f64() * 1000.0;
                if measured_turn {
                    stats.commit_candidate_ms.push(commit_ms);
                }
            }

            Ok::<BuilderStats, anyhow::Error>(stats)
        })
    };

    let admission = admission_task
        .await
        .context("waiting for single-builder admission task")??;
    let builder = builder_task
        .await
        .context("waiting for single-builder builder task")??;
    let total_wall_s = started.elapsed().as_secs_f64();
    let final_snapshot = mempool.snapshot().await?;

    Ok(finalize_result(
        admission,
        builder,
        final_snapshot,
        total_wall_s,
    ))
}

fn finalize_result(
    admission: AdmissionStats,
    builder: BuilderStats,
    final_snapshot: MempoolSnapshot,
    total_wall_s: f64,
) -> SingleBuilderResult {
    let candidate_count = builder.candidate_count.max(1) as f64;
    SingleBuilderResult {
        admitted_tps: if total_wall_s > 0.0 {
            admission.admitted_total as f64 / total_wall_s
        } else {
            0.0
        },
        admitted_total: admission.admitted_total,
        rejected_total: admission.rejected_total,
        checktx_p50_ms: percentile(&admission.durations_ms, 0.50),
        checktx_p95_ms: percentile(&admission.durations_ms, 0.95),
        checktx_p99_ms: percentile(&admission.durations_ms, 0.99),
        candidate_count: builder.candidate_count,
        empty_turn_count: builder.empty_turn_count,
        selected_tx_count_mean: builder.selected_tx_count_sum as f64 / candidate_count,
        selected_payload_bytes_mean: builder.selected_payload_bytes_sum as f64 / candidate_count,
        candidate_ready_ms_mean: mean(&builder.candidate_ready_ms),
        candidate_ready_ms_p50: percentile(&builder.candidate_ready_ms, 0.50),
        candidate_ready_ms_p95: percentile(&builder.candidate_ready_ms, 0.95),
        aggregate_total_ms_mean: builder.aggregate_total_ms_sum / candidate_count,
        sidecar_build_ms_mean: builder.sidecar_build_ms_sum / candidate_count,
        aggregate_verify_ms_mean: builder.aggregate_verify_ms_sum / candidate_count,
        commit_candidate_ms_mean: mean(&builder.commit_candidate_ms),
        commit_candidate_ms_p50: percentile(&builder.commit_candidate_ms, 0.50),
        commit_candidate_ms_p95: percentile(&builder.commit_candidate_ms, 0.95),
        block_cycle_ms_mean: mean(&builder.candidate_ready_ms) + mean(&builder.commit_candidate_ms),
        admission_to_candidate_ready_ms_p50: percentile(&builder.admission_to_ready_ms, 0.50),
        admission_to_candidate_ready_ms_p95: percentile(&builder.admission_to_ready_ms, 0.95),
        final_total_record_count: final_snapshot.total_record_count,
        final_current_bytes: final_snapshot.current_bytes,
        peak_txs: final_snapshot.peak_txs,
        peak_bytes: final_snapshot.peak_bytes,
        peak_nullifier_entries: final_snapshot.peak_nullifier_entries,
        peak_reserved_records: final_snapshot.peak_reserved_records,
        peak_frozen_candidates: final_snapshot.peak_frozen_candidates,
        evicted_total: final_snapshot.evicted_total,
        committed_total: final_snapshot.committed_total,
        invalidated_total: final_snapshot.invalidated_total,
        replaced_total: final_snapshot.replaced_total,
        rejected_full_low_fee_total: final_snapshot.rejected_full_low_fee_total,
        rejected_full_no_evictable_total: final_snapshot.rejected_full_no_evictable_total,
        evicted_nonstaking_total: final_snapshot.evicted_nonstaking_total,
        evicted_lowest_staking_total: final_snapshot.evicted_lowest_staking_total,
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn percentile(values: &[f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((sorted.len() - 1) as f64 * quantile).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use penumbra_sdk_bench_support::proof_txs::{build_proof_transactions, setup_proof_storage};

    async fn build_small_workload(n: usize) -> Result<(Vec<Arc<Vec<u8>>>, Snapshot)> {
        let (storage, _node, client) = setup_proof_storage(n).await?;
        let txs = build_proof_transactions(client, &storage, n)
            .await?
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        Ok((txs, storage.latest_snapshot()))
    }

    fn base_config(mode: SingleBuilderMode) -> SingleBuilderConfig {
        SingleBuilderConfig {
            mode,
            offered_tps: 50,
            block_interval_ms: 100,
            warmup_blocks: 1,
            measured_blocks: 2,
            max_block_txs: 8,
            segment_tx_count: 4,
            max_proposal_bytes: 8_000_000,
            max_store_bytes: 1 << 20,
            max_store_txs: 32,
            synthetic_fee_mode: SyntheticFeeMode::Off,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
            rayon_threads_per_batch: 1,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "expensive real-proof smoke"]
    async fn strict_mode_accepts_valid_corpus() -> Result<()> {
        let (txs, snapshot) = build_small_workload(2).await?;
        let result =
            run_single_builder_lab(txs, snapshot, base_config(SingleBuilderMode::StrictMempool))
                .await?;
        assert!(result.admitted_total > 0);
        assert!(result.candidate_count > 0);
        assert!(result.selected_tx_count_mean > 0.0);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "expensive real-proof smoke"]
    async fn optimistic_mode_accepts_valid_corpus() -> Result<()> {
        let (txs, snapshot) = build_small_workload(2).await?;
        let result = run_single_builder_lab(
            txs,
            snapshot,
            base_config(SingleBuilderMode::OptimisticBuilder),
        )
        .await?;
        assert!(result.admitted_total > 0);
        assert!(result.candidate_count > 0);
        assert!(result.aggregate_verify_ms_mean >= 0.0);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "expensive real-proof smoke"]
    async fn mode_comparison_sanity_holds() -> Result<()> {
        let (txs, snapshot) = build_small_workload(2).await?;
        let strict = run_single_builder_lab(
            txs.clone(),
            snapshot.clone(),
            base_config(SingleBuilderMode::StrictMempool),
        )
        .await?;
        let optimistic = run_single_builder_lab(
            txs,
            snapshot,
            base_config(SingleBuilderMode::OptimisticBuilder),
        )
        .await?;

        assert!(strict.checktx_p50_ms >= optimistic.checktx_p50_ms);
        assert!(optimistic.candidate_ready_ms_mean >= strict.candidate_ready_ms_mean);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_case_counts_empty_turns() -> Result<()> {
        let (storage, _node, _client) = setup_proof_storage(1).await?;
        let result = run_single_builder_lab(
            Vec::new(),
            storage.latest_snapshot(),
            base_config(SingleBuilderMode::StrictMempool),
        )
        .await?;
        assert_eq!(result.candidate_count, 0);
        assert_eq!(result.empty_turn_count, 2);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "expensive real-proof smoke"]
    async fn saturation_respects_max_block_txs() -> Result<()> {
        let (txs, snapshot) = build_small_workload(3).await?;
        let mut cfg = base_config(SingleBuilderMode::StrictMempool);
        cfg.max_block_txs = 1;
        cfg.offered_tps = 200;
        let result = run_single_builder_lab(txs, snapshot, cfg).await?;
        assert!(result.candidate_count > 0);
        assert!(result.selected_tx_count_mean <= 1.0);
        Ok(())
    }
}
