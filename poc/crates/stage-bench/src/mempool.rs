use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use cnidarium::Snapshot;
use parking_lot::Mutex;
use penumbra_sdk_app::{
    app::{App, CheckTxProfile, CheckTxSharedContext},
    block_tx_indexing::BlockTxIndexingMode,
    stateless_cache::{StatelessCache, TxArtifact},
};
use penumbra_sdk_num::Amount;
use penumbra_sdk_poc_preconsensus::local_mempool::{
    AdmitOutcome, AdmittedRecord, EvictionPolicy, FeeEvictionPolicy, FeeSource, MempoolCoreConfig,
    MempoolHandle, MempoolSnapshot,
};
use sha2::Digest as _;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticFeeMode {
    Off,
    DeterministicHashV1,
}

impl SyntheticFeeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::DeterministicHashV1 => "deterministic-hash-v1",
        }
    }
}

impl FromStr for SyntheticFeeMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "off" => Ok(Self::Off),
            "deterministic-hash-v1" => Ok(Self::DeterministicHashV1),
            other => anyhow::bail!("unknown synthetic fee mode: {other}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckTxMode {
    /// Extract artifacts only, skip Groth16 batch verify.
    Optimistic,
    /// Full CheckTx: extract + per-tx Groth16 batch verify inside each worker.
    Strict,
    /// Pipelined: N workers extract in parallel, then batch verify across all pending
    /// artifacts in groups of `verify_batch_size` before admission.
    StrictBatched,
}

impl CheckTxMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Optimistic => "optimistic",
            Self::Strict => "strict",
            Self::StrictBatched => "strict-batched",
        }
    }
}

impl FromStr for CheckTxMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "optimistic" => Ok(Self::Optimistic),
            "strict" => Ok(Self::Strict),
            "strict-batched" => Ok(Self::StrictBatched),
            other => anyhow::bail!(
                "unknown checktx mode: {other}; use optimistic, strict, or strict-batched"
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MempoolV1Config {
    pub worker_count: usize,
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub commit_interval_ms: u64,
    pub commit_batch_size: usize,
    pub synthetic_fee_mode: SyntheticFeeMode,
    pub fee_eviction_policy: FeeEvictionPolicy,
    pub checktx_mode: CheckTxMode,
    /// Number of artifacts to batch together for a single Groth16 verify call.
    /// Only used in `StrictBatched` mode. Must be >= 1.
    pub verify_batch_size: usize,
    /// Number of concurrent verify workers in the streaming pipeline.
    /// 0 = auto: available_parallelism()/2, capped at 32.
    pub verify_worker_count: usize,
    /// Number of records each worker accumulates locally before flushing to the mempool actor
    /// as a single batch. Only used in Optimistic and Strict modes. 0 = no batching (1 per call).
    pub admit_batch_size: usize,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct MempoolV1Result {
    pub admitted_tps: f64,
    pub admitted_total: u64,
    pub rejected_total: u64,
    pub checktx_p50_ms: f64,
    pub checktx_p95_ms: f64,
    pub checktx_p99_ms: f64,
    pub stateless_artifact_ms_mean: f64,
    pub stateless_artifact_precheck_ms_mean: f64,
    pub stateless_artifact_action_extract_ms_mean: f64,
    pub stateless_artifact_action_auth_sig_ms_mean: f64,
    pub stateless_artifact_action_extract_public_ms_mean: f64,
    pub stateless_artifact_action_to_batch_item_ms_mean: f64,
    pub stateless_task_join_wall_ms_mean: f64,
    pub stateless_artifact_queue_wait_ms_mean: f64,
    pub check_historical_ms_mean: f64,
    pub checktx_execute_fast_wall_ms_mean: f64,
    pub checktx_fast_prepare_join_wall_ms_mean: f64,
    pub checktx_fast_read_blocking_total_ms_mean: f64,
    pub checktx_fast_apply_wall_ms_mean: f64,
    pub checktx_candidate_read_wall_ms_mean: f64,
    pub checktx_serial_apply_wall_ms_mean: f64,
    pub checktx_serial_sct_append_ms_mean: f64,
    pub checktx_serial_event_emit_ms_mean: f64,
    pub checktx_serial_fee_apply_ms_mean: f64,
    pub execute_check_and_execute_ms_mean: f64,
    pub execute_set_source_ms_mean: f64,
    pub execute_record_clues_ms_mean: f64,
    pub execute_apply_ms_mean: f64,
    pub execute_read_lookup_wait_or_join_ms_mean: f64,
    pub execute_read_nullifier_wait_ms_mean: f64,
    pub execute_spend_nullifier_committed_check_ms_mean: f64,
    pub execute_pay_fee_ms_mean: f64,
    pub admitted_store_peak_txs: usize,
    pub admitted_store_peak_bytes: usize,
    pub nullifier_index_peak_entries: usize,
    pub active_reservations_peak: usize,
    pub frozen_candidates_peak: usize,
    pub evicted_total: u64,
    pub committed_total: u64,
    pub invalidated_total: u64,
    pub replaced_total: u64,
    pub rejected_full_low_fee_total: u64,
    pub rejected_full_no_evictable_total: u64,
    pub evicted_nonstaking_total: u64,
    pub evicted_lowest_staking_total: u64,
    pub commit_prune_ms_mean: f64,
    pub submit_admitted_p50_ms: f64,
    pub submit_admitted_p95_ms: f64,
    pub submit_admitted_p99_ms: f64,
    pub batch_verify_p50_ms: f64,
    pub batch_verify_p95_ms: f64,
    pub batch_verify_p99_ms: f64,
}

#[derive(Default)]
struct WorkerStats {
    durations_ms: Vec<f64>,
    stateless_artifact_ms_sum: f64,
    stateless_artifact_precheck_ms_sum: f64,
    stateless_artifact_action_extract_ms_sum: f64,
    stateless_artifact_action_auth_sig_ms_sum: f64,
    stateless_artifact_action_extract_public_ms_sum: f64,
    stateless_artifact_action_to_batch_item_ms_sum: f64,
    stateless_task_join_wall_ms_sum: f64,
    stateless_artifact_queue_wait_ms_sum: f64,
    check_historical_ms_sum: f64,
    checktx_execute_fast_wall_ms_sum: f64,
    checktx_fast_prepare_join_wall_ms_sum: f64,
    checktx_fast_read_blocking_total_ms_sum: f64,
    checktx_fast_apply_wall_ms_sum: f64,
    checktx_candidate_read_wall_ms_sum: f64,
    checktx_serial_apply_wall_ms_sum: f64,
    checktx_serial_sct_append_ms_sum: f64,
    checktx_serial_event_emit_ms_sum: f64,
    checktx_serial_fee_apply_ms_sum: f64,
    execute_check_and_execute_ms_sum: f64,
    execute_set_source_ms_sum: f64,
    execute_record_clues_ms_sum: f64,
    execute_apply_ms_sum: f64,
    execute_read_lookup_wait_or_join_ms_sum: f64,
    execute_read_nullifier_wait_ms_sum: f64,
    execute_spend_nullifier_committed_check_ms_sum: f64,
    execute_pay_fee_ms_sum: f64,
    submit_admitted_ms: Vec<f64>,
    batch_verify_ms: Vec<f64>,
    accepted_count: u64,
    rejected_count: u64,
}

impl std::ops::AddAssign for WorkerStats {
    fn add_assign(&mut self, rhs: Self) {
        self.stateless_artifact_ms_sum += rhs.stateless_artifact_ms_sum;
        self.stateless_artifact_precheck_ms_sum += rhs.stateless_artifact_precheck_ms_sum;
        self.stateless_artifact_action_extract_ms_sum +=
            rhs.stateless_artifact_action_extract_ms_sum;
        self.stateless_artifact_action_auth_sig_ms_sum +=
            rhs.stateless_artifact_action_auth_sig_ms_sum;
        self.stateless_artifact_action_extract_public_ms_sum +=
            rhs.stateless_artifact_action_extract_public_ms_sum;
        self.stateless_artifact_action_to_batch_item_ms_sum +=
            rhs.stateless_artifact_action_to_batch_item_ms_sum;
        self.stateless_task_join_wall_ms_sum += rhs.stateless_task_join_wall_ms_sum;
        self.stateless_artifact_queue_wait_ms_sum += rhs.stateless_artifact_queue_wait_ms_sum;
        self.check_historical_ms_sum += rhs.check_historical_ms_sum;
        self.checktx_execute_fast_wall_ms_sum += rhs.checktx_execute_fast_wall_ms_sum;
        self.checktx_fast_prepare_join_wall_ms_sum += rhs.checktx_fast_prepare_join_wall_ms_sum;
        self.checktx_fast_read_blocking_total_ms_sum +=
            rhs.checktx_fast_read_blocking_total_ms_sum;
        self.checktx_fast_apply_wall_ms_sum += rhs.checktx_fast_apply_wall_ms_sum;
        self.checktx_candidate_read_wall_ms_sum += rhs.checktx_candidate_read_wall_ms_sum;
        self.checktx_serial_apply_wall_ms_sum += rhs.checktx_serial_apply_wall_ms_sum;
        self.checktx_serial_sct_append_ms_sum += rhs.checktx_serial_sct_append_ms_sum;
        self.checktx_serial_event_emit_ms_sum += rhs.checktx_serial_event_emit_ms_sum;
        self.checktx_serial_fee_apply_ms_sum += rhs.checktx_serial_fee_apply_ms_sum;
        self.execute_check_and_execute_ms_sum += rhs.execute_check_and_execute_ms_sum;
        self.execute_set_source_ms_sum += rhs.execute_set_source_ms_sum;
        self.execute_record_clues_ms_sum += rhs.execute_record_clues_ms_sum;
        self.execute_apply_ms_sum += rhs.execute_apply_ms_sum;
        self.execute_read_lookup_wait_or_join_ms_sum += rhs.execute_read_lookup_wait_or_join_ms_sum;
        self.execute_read_nullifier_wait_ms_sum += rhs.execute_read_nullifier_wait_ms_sum;
        self.execute_spend_nullifier_committed_check_ms_sum +=
            rhs.execute_spend_nullifier_committed_check_ms_sum;
        self.execute_pay_fee_ms_sum += rhs.execute_pay_fee_ms_sum;
        self.durations_ms.extend(rhs.durations_ms);
        self.submit_admitted_ms.extend(rhs.submit_admitted_ms);
        self.batch_verify_ms.extend(rhs.batch_verify_ms);
        self.accepted_count += rhs.accepted_count;
        self.rejected_count += rhs.rejected_count;
    }
}

#[derive(Default)]
struct CommitStats {
    elapsed_ms_sum: f64,
    tick_count: u64,
}

pub async fn run_mempool_lab(
    txs: Vec<Arc<Vec<u8>>>,
    snapshot: Snapshot,
    config: MempoolV1Config,
) -> Result<MempoolV1Result> {
    if config.checktx_mode == CheckTxMode::StrictBatched {
        return run_batched_pipeline(txs, snapshot, config).await;
    }
    anyhow::ensure!(config.worker_count > 0, "worker_count must be > 0");
    anyhow::ensure!(
        config.commit_interval_ms > 0,
        "commit_interval_ms must be > 0"
    );
    anyhow::ensure!(
        config.commit_batch_size > 0,
        "commit_batch_size must be > 0"
    );

    let stateless_cache = Arc::new(StatelessCache::new());
    let shared_context = Arc::new(CheckTxSharedContext::load(&snapshot).await?);
    let snapshot_version = snapshot.version();
    let txs = Arc::new(txs);
    let next_index = Arc::new(AtomicUsize::new(0));
    let next_seq = Arc::new(AtomicU64::new(0));
    let mempool = MempoolHandle::new(MempoolCoreConfig {
        max_store_bytes: config.max_store_bytes,
        max_store_txs: config.max_store_txs,
        ingestion_buffer: config.worker_count.saturating_mul(8).max(64),
        command_buffer: 256,
        eviction_policy: EvictionPolicy::OldestUnreservedFirst,
        fee_eviction_policy: config.fee_eviction_policy,
    });
    let commit_stats = Arc::new(Mutex::new(CommitStats::default()));
    let stop_commits = Arc::new(AtomicBool::new(false));

    let commit_task = {
        let mempool = mempool.clone();
        let commit_stats = commit_stats.clone();
        let stop_commits = stop_commits.clone();
        let commit_interval_ms = config.commit_interval_ms;
        let commit_batch_size = config.commit_batch_size;
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(commit_interval_ms));
            loop {
                interval.tick().await;
                if stop_commits.load(Ordering::Relaxed) {
                    break;
                }

                let tick_started = Instant::now();
                let frozen = mempool
                    .freeze_next_candidate(u64::MAX, commit_batch_size, usize::MAX)
                    .await?;
                let Some(frozen) = frozen else {
                    continue;
                };
                mempool.commit_candidate(frozen.lease.candidate_id).await?;
                let elapsed_ms = tick_started.elapsed().as_secs_f64() * 1000.0;
                let mut stats = commit_stats.lock();
                stats.elapsed_ms_sum += elapsed_ms;
                stats.tick_count += 1;
            }
            Ok::<(), anyhow::Error>(())
        })
    };

    let started = Instant::now();
    let mut workers = tokio::task::JoinSet::new();
    let synthetic_fee_mode = config.synthetic_fee_mode;
    for _ in 0..config.worker_count {
        let txs = txs.clone();
        let next_index = next_index.clone();
        let next_seq = next_seq.clone();
        let snapshot = snapshot.clone();
        let shared_context = shared_context.clone();
        let stateless_cache = stateless_cache.clone();
        let mempool = mempool.clone();
        workers.spawn(async move {
            let mut stats = WorkerStats::default();
            let batch_size = if config.admit_batch_size == 0 {
                1
            } else {
                config.admit_batch_size
            };
            let mut pending: Vec<Arc<AdmittedRecord>> = Vec::with_capacity(batch_size);
            let mut pending_profiles: Vec<_> = Vec::with_capacity(batch_size);

            macro_rules! flush_pending {
                () => {
                    if !pending.is_empty() {
                        let submit_started = Instant::now();
                        let outcomes = mempool
                            .submit_admitted_batch(std::mem::take(&mut pending))
                            .await?;
                        let submit_ms = submit_started.elapsed().as_secs_f64() * 1000.0;
                        for (outcome, profile) in
                            outcomes.into_iter().zip(pending_profiles.drain(..))
                        {
                            match outcome {
                                AdmitOutcome::Admitted { .. } => {
                                    stats.submit_admitted_ms.push(submit_ms);
                                    record_profile(&mut stats, &profile);
                                }
                                _ => {
                                    stats.rejected_count += 1;
                                }
                            }
                        }
                    }
                };
            }

            loop {
                let tx_index = next_index.fetch_add(1, Ordering::Relaxed);
                let Some(tx_bytes) = txs.get(tx_index).cloned() else {
                    flush_pending!();
                    break;
                };

                let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
                let mut app = App::new(snapshot.clone());
                app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
                app.set_checktx_shared_context(shared_context.clone());

                let checktx_result = app
                    .deliver_tx_bytes_v2_extracted_profiled_for_bench(
                        tx_bytes.as_slice(),
                        stateless_cache.as_ref(),
                    )
                    .await;
                match checktx_result {
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
                        if config.checktx_mode == CheckTxMode::Strict {
                            let bv_ms = App::batch_verify_tx_artifact_for_bench(&artifact).await?;
                            stats.batch_verify_ms.push(bv_ms);
                        }
                        let admission_seq = next_seq.fetch_add(1, Ordering::Relaxed);
                        let record = Arc::new(apply_synthetic_fee_mode(
                            AdmittedRecord::from_tx_bytes(
                                admission_seq,
                                tx_bytes,
                                artifact,
                                snapshot_version,
                            ),
                            synthetic_fee_mode,
                        ));
                        pending.push(record);
                        pending_profiles.push(profile);
                        if pending.len() >= batch_size {
                            flush_pending!();
                        }
                    }
                    Err(_error) => {
                        stats.rejected_count += 1;
                    }
                }
            }

            Ok::<WorkerStats, anyhow::Error>(stats)
        });
    }

    let mut total = WorkerStats::default();

    while let Some(joined) = workers.join_next().await {
        let worker = joined.context("waiting for mempool worker task")??;
        total += worker;
    }

    stop_commits.store(true, Ordering::Relaxed);
    commit_task.await.context("waiting for commit task")??;
    let total_wall_s = started.elapsed().as_secs_f64();

    let snapshot = mempool.snapshot().await?;
    let commit_stats = commit_stats.lock();

    Ok(build_result(total, &snapshot, &commit_stats, total_wall_s))
}

fn record_profile(stats: &mut WorkerStats, profile: &CheckTxProfile) {
    stats.accepted_count += 1;
    stats.durations_ms.push(profile.checktx_total_wall_ms);
    stats.stateless_artifact_ms_sum += profile.stateless_artifact_ms;
    stats.stateless_artifact_precheck_ms_sum += profile.stateless_artifact_precheck_ms;
    stats.stateless_artifact_action_extract_ms_sum += profile.stateless_artifact_action_extract_ms;
    stats.stateless_artifact_action_auth_sig_ms_sum +=
        profile.stateless_artifact_action_auth_sig_ms;
    stats.stateless_artifact_action_extract_public_ms_sum +=
        profile.stateless_artifact_action_extract_public_ms;
    stats.stateless_artifact_action_to_batch_item_ms_sum +=
        profile.stateless_artifact_action_to_batch_item_ms;
    stats.stateless_task_join_wall_ms_sum += profile.stateless_task_join_wall_ms;
    stats.stateless_artifact_queue_wait_ms_sum += profile.stateless_artifact_queue_wait_ms;
    stats.check_historical_ms_sum += profile.check_historical_ms;
    stats.checktx_execute_fast_wall_ms_sum += profile.checktx_execute_fast_wall_ms;
    stats.checktx_fast_prepare_join_wall_ms_sum += profile.checktx_fast_prepare_join_wall_ms;
    stats.checktx_fast_read_blocking_total_ms_sum += profile.checktx_fast_read_blocking_total_ms;
    stats.checktx_fast_apply_wall_ms_sum += profile.checktx_fast_apply_wall_ms;
    stats.checktx_candidate_read_wall_ms_sum += profile.checktx_candidate_read_wall_ms;
    stats.checktx_serial_apply_wall_ms_sum += profile.checktx_serial_apply_wall_ms;
    stats.checktx_serial_sct_append_ms_sum += profile.checktx_serial_sct_append_ms;
    stats.checktx_serial_event_emit_ms_sum += profile.checktx_serial_event_emit_ms;
    stats.checktx_serial_fee_apply_ms_sum += profile.checktx_serial_fee_apply_ms;
    stats.execute_check_and_execute_ms_sum += profile.execute_check_and_execute_ms;
    stats.execute_set_source_ms_sum += profile.execute_set_source_ms;
    stats.execute_record_clues_ms_sum += profile.execute_record_clues_ms;
    stats.execute_apply_ms_sum += profile.execute_apply_ms;
    stats.execute_read_lookup_wait_or_join_ms_sum += profile.execute_read_lookup_wait_or_join_ms;
    stats.execute_read_nullifier_wait_ms_sum += profile.execute_read_nullifier_wait_ms;
    stats.execute_spend_nullifier_committed_check_ms_sum +=
        profile.execute_spend_nullifier_committed_check_ms;
    stats.execute_pay_fee_ms_sum += profile.execute_pay_fee_ms;
}

fn build_result(
    total: WorkerStats,
    snapshot: &MempoolSnapshot,
    commit_stats: &CommitStats,
    total_wall_s: f64,
) -> MempoolV1Result {
    let accepted_count = total.accepted_count.max(1) as f64;
    MempoolV1Result {
        admitted_tps: if total_wall_s > 0.0 {
            total.accepted_count as f64 / total_wall_s
        } else {
            0.0
        },
        admitted_total: total.accepted_count,
        rejected_total: total.rejected_count,
        checktx_p50_ms: percentile(&total.durations_ms, 0.50),
        checktx_p95_ms: percentile(&total.durations_ms, 0.95),
        checktx_p99_ms: percentile(&total.durations_ms, 0.99),
        stateless_artifact_ms_mean: total.stateless_artifact_ms_sum / accepted_count,
        stateless_artifact_precheck_ms_mean: total.stateless_artifact_precheck_ms_sum
            / accepted_count,
        stateless_artifact_action_extract_ms_mean: total.stateless_artifact_action_extract_ms_sum
            / accepted_count,
        stateless_artifact_action_auth_sig_ms_mean: total.stateless_artifact_action_auth_sig_ms_sum
            / accepted_count,
        stateless_artifact_action_extract_public_ms_mean:
            total.stateless_artifact_action_extract_public_ms_sum / accepted_count,
        stateless_artifact_action_to_batch_item_ms_mean:
            total.stateless_artifact_action_to_batch_item_ms_sum / accepted_count,
        stateless_task_join_wall_ms_mean: total.stateless_task_join_wall_ms_sum / accepted_count,
        stateless_artifact_queue_wait_ms_mean: total.stateless_artifact_queue_wait_ms_sum
            / accepted_count,
        check_historical_ms_mean: total.check_historical_ms_sum / accepted_count,
        checktx_execute_fast_wall_ms_mean: total.checktx_execute_fast_wall_ms_sum / accepted_count,
        checktx_fast_prepare_join_wall_ms_mean: total.checktx_fast_prepare_join_wall_ms_sum
            / accepted_count,
        checktx_fast_read_blocking_total_ms_mean: total.checktx_fast_read_blocking_total_ms_sum
            / accepted_count,
        checktx_fast_apply_wall_ms_mean: total.checktx_fast_apply_wall_ms_sum / accepted_count,
        checktx_candidate_read_wall_ms_mean: total.checktx_candidate_read_wall_ms_sum
            / accepted_count,
        checktx_serial_apply_wall_ms_mean: total.checktx_serial_apply_wall_ms_sum / accepted_count,
        checktx_serial_sct_append_ms_mean: total.checktx_serial_sct_append_ms_sum / accepted_count,
        checktx_serial_event_emit_ms_mean: total.checktx_serial_event_emit_ms_sum / accepted_count,
        checktx_serial_fee_apply_ms_mean: total.checktx_serial_fee_apply_ms_sum / accepted_count,
        execute_check_and_execute_ms_mean: total.execute_check_and_execute_ms_sum / accepted_count,
        execute_set_source_ms_mean: total.execute_set_source_ms_sum / accepted_count,
        execute_record_clues_ms_mean: total.execute_record_clues_ms_sum / accepted_count,
        execute_apply_ms_mean: total.execute_apply_ms_sum / accepted_count,
        execute_read_lookup_wait_or_join_ms_mean: total.execute_read_lookup_wait_or_join_ms_sum
            / accepted_count,
        execute_read_nullifier_wait_ms_mean: total.execute_read_nullifier_wait_ms_sum
            / accepted_count,
        execute_spend_nullifier_committed_check_ms_mean:
            total.execute_spend_nullifier_committed_check_ms_sum / accepted_count,
        execute_pay_fee_ms_mean: total.execute_pay_fee_ms_sum / accepted_count,
        admitted_store_peak_txs: snapshot.peak_txs,
        admitted_store_peak_bytes: snapshot.peak_bytes,
        nullifier_index_peak_entries: snapshot.peak_nullifier_entries,
        active_reservations_peak: snapshot.peak_reserved_records,
        frozen_candidates_peak: snapshot.peak_frozen_candidates,
        evicted_total: snapshot.evicted_total,
        committed_total: snapshot.committed_total,
        invalidated_total: snapshot.invalidated_total,
        replaced_total: snapshot.replaced_total,
        rejected_full_low_fee_total: snapshot.rejected_full_low_fee_total,
        rejected_full_no_evictable_total: snapshot.rejected_full_no_evictable_total,
        evicted_nonstaking_total: snapshot.evicted_nonstaking_total,
        evicted_lowest_staking_total: snapshot.evicted_lowest_staking_total,
        commit_prune_ms_mean: if commit_stats.tick_count > 0 {
            commit_stats.elapsed_ms_sum / commit_stats.tick_count as f64
        } else {
            0.0
        },
        submit_admitted_p50_ms: percentile(&total.submit_admitted_ms, 0.50),
        submit_admitted_p95_ms: percentile(&total.submit_admitted_ms, 0.95),
        submit_admitted_p99_ms: percentile(&total.submit_admitted_ms, 0.99),
        batch_verify_p50_ms: percentile(&total.batch_verify_ms, 0.50),
        batch_verify_p95_ms: percentile(&total.batch_verify_ms, 0.95),
        batch_verify_p99_ms: percentile(&total.batch_verify_ms, 0.99),
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

pub fn apply_synthetic_fee_mode(record: AdmittedRecord, mode: SyntheticFeeMode) -> AdmittedRecord {
    match mode {
        SyntheticFeeMode::Off => record,
        SyntheticFeeMode::DeterministicHashV1 => {
            let seed = u64::from_le_bytes(record.tx_hash[..8].try_into().expect("hash prefix"));
            let synthetic_amount = Amount::from(1u64 + (seed % 4096));
            let fee_asset_id = record.fee_asset_id;
            record.with_fee_metadata(
                fee_asset_id,
                synthetic_amount,
                FeeSource::SyntheticBenchmark,
            )
        }
    }
}

/// Streaming pipeline for `StrictBatched` mode:
/// Stage 1 (N extract workers) → ch1 → Stage 2 (Batcher) → ch2 (MPMC) →
/// Stage 3 (K async verify workers) → ch3 → Stage 4 (1 admit worker).
async fn run_batched_pipeline(
    txs: Vec<Arc<Vec<u8>>>,
    snapshot: Snapshot,
    config: MempoolV1Config,
) -> Result<MempoolV1Result> {
    use std::pin::pin;
    use std::time::Duration;
    use tokio::time::Instant as TokioInstant;

    anyhow::ensure!(config.worker_count > 0, "worker_count must be > 0");
    anyhow::ensure!(
        config.verify_batch_size > 0,
        "verify_batch_size must be > 0"
    );
    anyhow::ensure!(
        config.commit_interval_ms > 0,
        "commit_interval_ms must be > 0"
    );
    anyhow::ensure!(
        config.commit_batch_size > 0,
        "commit_batch_size must be > 0"
    );

    let k_workers = if config.verify_worker_count == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            / 2
    } else {
        config.verify_worker_count
    }
    .max(1)
    .min(32);

    let stateless_cache = Arc::new(StatelessCache::new());
    let shared_context = Arc::new(CheckTxSharedContext::load(&snapshot).await?);
    let snapshot_version = snapshot.version();
    let txs = Arc::new(txs);
    let tx_count = txs.len();
    let next_index = Arc::new(AtomicUsize::new(0));
    let mempool = MempoolHandle::new(MempoolCoreConfig {
        max_store_bytes: config.max_store_bytes,
        max_store_txs: config.max_store_txs,
        ingestion_buffer: config.worker_count.saturating_mul(8).max(64),
        command_buffer: 256,
        eviction_policy: EvictionPolicy::OldestUnreservedFirst,
        fee_eviction_policy: config.fee_eviction_policy,
    });
    let commit_stats = Arc::new(Mutex::new(CommitStats::default()));
    let stop_commits = Arc::new(AtomicBool::new(false));

    let commit_task = {
        let mempool = mempool.clone();
        let commit_stats = commit_stats.clone();
        let stop_commits = stop_commits.clone();
        let commit_interval_ms = config.commit_interval_ms;
        let commit_batch_size = config.commit_batch_size;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(commit_interval_ms));
            loop {
                interval.tick().await;
                if stop_commits.load(Ordering::Relaxed) {
                    break;
                }
                let tick_started = Instant::now();
                let frozen = mempool
                    .freeze_next_candidate(u64::MAX, commit_batch_size, usize::MAX)
                    .await?;
                let Some(frozen) = frozen else { continue };
                mempool.commit_candidate(frozen.lease.candidate_id).await?;
                let elapsed_ms = tick_started.elapsed().as_secs_f64() * 1000.0;
                let mut stats = commit_stats.lock();
                stats.elapsed_ms_sum += elapsed_ms;
                stats.tick_count += 1;
            }
            Ok::<(), anyhow::Error>(())
        })
    };

    struct Extracted {
        tx_bytes: Arc<Vec<u8>>,
        artifact: Arc<TxArtifact>,
        profile: CheckTxProfile,
    }

    let started = Instant::now();
    let synthetic_fee_mode = config.synthetic_fee_mode;
    let batch_size = config.verify_batch_size;

    // ch1: extract workers → batcher
    let (extract_tx, mut extract_rx) =
        tokio::sync::mpsc::channel::<Extracted>(config.worker_count * 4);
    // ch2: batcher → K verify workers (MPMC)
    let (batch_tx, batch_rx) = async_channel::bounded::<Vec<Extracted>>(k_workers * 2);
    // ch3: verify workers → admit worker
    let (verified_tx, mut verified_rx) =
        tokio::sync::mpsc::channel::<(Vec<Extracted>, f64)>(k_workers * 2);

    // Stage 1: N extract workers
    let mut extract_set = tokio::task::JoinSet::<Result<u64>>::new();
    for _ in 0..config.worker_count {
        let txs = txs.clone();
        let next_index = next_index.clone();
        let snapshot = snapshot.clone();
        let shared_context = shared_context.clone();
        let stateless_cache = stateless_cache.clone();
        let extract_tx = extract_tx.clone();
        extract_set.spawn(async move {
            let mut rejected = 0u64;
            loop {
                let tx_index = next_index.fetch_add(1, Ordering::Relaxed);
                let Some(tx_bytes) = txs.get(tx_index).cloned() else {
                    break;
                };
                let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
                let mut app = App::new(snapshot.clone());
                app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
                app.set_checktx_shared_context(shared_context.clone());
                let Ok((_events, profile)) = app
                    .deliver_tx_bytes_v2_extracted_profiled_for_bench(
                        tx_bytes.as_slice(),
                        stateless_cache.as_ref(),
                    )
                    .await
                else {
                    rejected += 1;
                    continue;
                };
                let Some(artifact) = stateless_cache.get(&tx_hash).and_then(|e| e.artifact())
                else {
                    rejected += 1;
                    continue;
                };
                if extract_tx
                    .send(Extracted {
                        tx_bytes,
                        artifact,
                        profile,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(rejected)
        });
    }
    drop(extract_tx); // close ch1 when all workers are spawned

    // Stage 2: single batcher task
    let batcher = {
        let batch_tx = batch_tx.clone();
        tokio::spawn(async move {
            // Persistent sleep: starts "infinite", reset to 5ms when buf goes 0→1.
            let mut flush_deadline = pin!(tokio::time::sleep(Duration::from_secs(3600)));
            let mut buf: Vec<Extracted> = Vec::with_capacity(batch_size);
            loop {
                tokio::select! {
                    biased;
                    item = extract_rx.recv() => match item {
                        Some(item) => {
                            if buf.is_empty() {
                                flush_deadline.as_mut().reset(
                                    TokioInstant::now() + Duration::from_millis(5),
                                );
                            }
                            buf.push(item);
                            if buf.len() >= batch_size {
                                let _ = batch_tx.send(std::mem::take(&mut buf)).await;
                                flush_deadline.as_mut().reset(
                                    TokioInstant::now() + Duration::from_secs(3600),
                                );
                            }
                        }
                        None => {
                            if !buf.is_empty() {
                                let _ = batch_tx.send(std::mem::take(&mut buf)).await;
                            }
                            break;
                        }
                    },
                    _ = &mut flush_deadline, if !buf.is_empty() => {
                        let _ = batch_tx.send(std::mem::take(&mut buf)).await;
                        flush_deadline.as_mut().reset(
                            TokioInstant::now() + Duration::from_secs(3600),
                        );
                    }
                }
            }
        })
    };
    drop(batch_tx); // K workers hold the remaining sender refs via clone below

    // Stage 3: K async verify workers (plain async — inner fn already uses spawn_blocking)
    let mut verify_set = tokio::task::JoinSet::<Result<()>>::new();
    for _ in 0..k_workers {
        let batch_rx = batch_rx.clone();
        let verified_tx = verified_tx.clone();
        verify_set.spawn(async move {
            while let Ok(batch) = batch_rx.recv().await {
                let artifacts: Vec<Arc<TxArtifact>> =
                    batch.iter().map(|e| e.artifact.clone()).collect();
                let bv_ms = App::batch_verify_artifacts_for_bench(&artifacts).await?;
                if verified_tx.send((batch, bv_ms)).await.is_err() {
                    break;
                }
            }
            Ok(())
        });
    }
    drop(batch_rx);
    drop(verified_tx);

    // Stage 4: admit worker (single task, serial mempool submissions)
    let admit_task = {
        let mempool = mempool.clone();
        tokio::spawn(async move {
            let mut stats = WorkerStats::default();
            let mut rejected_admit = 0u64;
            let mut admission_seq = 0u64;

            while let Some((batch, bv_ms)) = verified_rx.recv().await {
                stats.batch_verify_ms.push(bv_ms);
                let records: Vec<Arc<AdmittedRecord>> = batch
                    .iter()
                    .map(|entry| {
                        let seq = admission_seq;
                        admission_seq += 1;
                        Arc::new(apply_synthetic_fee_mode(
                            AdmittedRecord::from_tx_bytes(
                                seq,
                                entry.tx_bytes.clone(),
                                entry.artifact.clone(),
                                snapshot_version,
                            ),
                            synthetic_fee_mode,
                        ))
                    })
                    .collect();
                let submit_started = Instant::now();
                let outcomes = mempool.submit_admitted_batch(records).await?;
                let submit_ms = submit_started.elapsed().as_secs_f64() * 1000.0;
                for (entry, outcome) in batch.into_iter().zip(outcomes) {
                    match outcome {
                        AdmitOutcome::Admitted { .. } => {
                            stats.submit_admitted_ms.push(submit_ms);
                            record_profile(&mut stats, &entry.profile);
                        }
                        _ => {
                            rejected_admit += 1;
                        }
                    }
                }
            }

            Ok::<_, anyhow::Error>((stats, rejected_admit))
        })
    };

    // Drain extract workers to get reject counts
    let mut rejected_extract = 0u64;
    while let Some(r) = extract_set.join_next().await {
        rejected_extract += r.context("extract worker")??;
    }
    batcher.await.context("batcher task")?;
    while let Some(r) = verify_set.join_next().await {
        r.context("verify worker")??;
    }

    let (mut total, rejected_admit) = admit_task.await.context("admit task")??;

    stop_commits.store(true, Ordering::Relaxed);
    commit_task.await.context("commit task")??;
    let total_wall_s = started.elapsed().as_secs_f64();

    let mempool_snap = mempool.snapshot().await?;
    let commit_stats = commit_stats.lock();
    let admitted_total = total.accepted_count;
    total.rejected_count = rejected_extract
        + rejected_admit
        + (tx_count as u64).saturating_sub(rejected_extract + admitted_total + rejected_admit);

    Ok(build_result(total, &mempool_snap, &commit_stats, total_wall_s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use penumbra_sdk_bench_support::proof_txs::{build_proof_transactions, setup_proof_storage};

    #[tokio::test(flavor = "multi_thread")]
    async fn direct_lab_accepts_valid_transactions() -> Result<()> {
        let (storage, _node, client) = setup_proof_storage(4).await?;
        let txs = build_proof_transactions(client, &storage, 4)
            .await?
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();

        let result = run_mempool_lab(
            txs,
            storage.latest_snapshot(),
            MempoolV1Config {
                worker_count: 2,
                max_store_bytes: 1 << 20,
                max_store_txs: 16,
                commit_interval_ms: 10,
                commit_batch_size: 2,
                synthetic_fee_mode: SyntheticFeeMode::Off,
                fee_eviction_policy: FeeEvictionPolicy::Disabled,
                checktx_mode: CheckTxMode::Strict,
                verify_batch_size: 1,
                verify_worker_count: 0,
                admit_batch_size: 0,
            },
        )
        .await?;

        assert_eq!(result.rejected_total, 0);
        assert_eq!(result.admitted_total, 4);
        assert!(result.checktx_p50_ms >= 0.0);
        assert!(result.stateless_artifact_ms_mean >= 0.0);
        assert!(result.check_historical_ms_mean >= 0.0);
        Ok(())
    }
}
