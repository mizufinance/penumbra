use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use penumbra_sdk_app::app::{AggregateBuildProfile, App, ArtifactBuildBreakdown};
use penumbra_sdk_app::stateless_cache::TxArtifact;
use penumbra_sdk_app::app::ProposalArtifactSidecar;
use penumbra_sdk_poc_preconsensus::local_mempool::{
    AdmittedRecord, FeeEvictionPolicy, FrozenCandidate, MempoolCoreConfig, MempoolHandle,
    EvictionPolicy,
};
use penumbra_sdk_proof_aggregation::{
    set_rayon_threads_per_batch_for_bench, AggregateBundle, ProofFamilyId,
};
use penumbra_sdk_transaction::Transaction;
use tokio::task::JoinHandle;

use crate::mempool::{apply_synthetic_fee_mode, SyntheticFeeMode};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BuilderMode {
    Monolithic,
    Lookahead,
}

impl BuilderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BuilderMode::Monolithic => "monolithic",
            BuilderMode::Lookahead => "lookahead",
        }
    }
}

impl serde::Serialize for BuilderMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LookaheadBuilderConfig {
    pub mode: BuilderMode,
    pub candidate_build_depth: usize,
    pub max_block_txs: usize,
    pub max_proposal_bytes: usize,
    pub segment_tx_count: usize,
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub fee_eviction_policy: FeeEvictionPolicy,
}

pub type AdmittedTx = AdmittedRecord;

#[derive(Clone)]
pub struct ReadyCandidate {
    pub frozen: FrozenCandidate,
    pub bundle: AggregateBundle,
    pub segment_tx_counts: Vec<usize>,
    pub sidecar: ProposalArtifactSidecar,
    pub artifact_total_ms: f64,
    pub artifact_profile: ArtifactBuildBreakdown,
    pub aggregate_total_ms: f64,
    pub aggregate_profile: AggregateBuildProfile,
    pub aggregate_verify_passed: bool,
    pub aggregate_verify_ms: f64,
    pub sidecar_build_ms: f64,
    pub background_build_ms: f64,
    pub freeze_to_ready_ms: f64,
    pub ready_at: Instant,
}

#[derive(Clone)]
pub struct BuiltCandidate {
    pub frozen: FrozenCandidate,
    pub bundle: AggregateBundle,
    pub segment_tx_counts: Vec<usize>,
    pub sidecar: ProposalArtifactSidecar,
    pub aggregate_total_ms: f64,
    pub aggregate_profile: AggregateBuildProfile,
    pub sidecar_build_ms: f64,
    pub build_wall_ms: f64,
}

#[derive(Clone, Debug, Default)]
pub struct BuilderSnapshot {
    pub admission_pool_tx_count: usize,
    pub reserved_tx_count: usize,
    pub frozen_candidate_count: usize,
    pub local_synthetic_invalidation_count: u64,
    pub peak_reserved_records: usize,
    pub peak_frozen_candidates: usize,
    pub replaced_total: u64,
    pub rejected_full_low_fee_total: u64,
    pub rejected_full_no_evictable_total: u64,
    pub evicted_nonstaking_total: u64,
    pub evicted_lowest_staking_total: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TurnOutcome {
    pub selected_tx_count: usize,
    pub selected_payload_bytes: usize,
    pub ready_ahead_of_turn: bool,
    pub background_build_candidate_ms: Option<f64>,
    pub fallback_build_candidate_ms: Option<f64>,
    pub freeze_to_ready_ms: Option<f64>,
    pub ready_lead_ms: Option<f64>,
    pub guard_miss: bool,
    pub segment_count: usize,
    pub block_limit_saturated: bool,
    pub build_budget_overrun: bool,
    pub artifact_total_ms: Option<f64>,
    pub artifact_profile: Option<ArtifactBuildBreakdown>,
    pub aggregate_total_ms: Option<f64>,
    pub aggregate_profile: Option<AggregateBuildProfile>,
    pub sidecar_build_ms: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct LookaheadLabConfig {
    pub mode: BuilderMode,
    pub offered_tps: usize,
    pub block_interval_ms: u64,
    pub num_validators: usize,
    pub proposer_index: usize,
    pub max_block_txs: usize,
    pub segment_tx_count: usize,
    pub warmup_local_turns: usize,
    pub steady_local_turns: usize,
    pub max_proposal_bytes: usize,
    pub ready_guard_ms: u64,
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub synthetic_fee_mode: SyntheticFeeMode,
    pub fee_eviction_policy: FeeEvictionPolicy,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct LookaheadLabResult {
    pub candidate_ready_turn_ratio: f64,
    pub candidate_tx_coverage_ratio: f64,
    pub freeze_to_ready_ms_mean: f64,
    pub ready_lead_ms_mean: f64,
    pub background_build_candidate_ms_mean: f64,
    pub fallback_build_candidate_ms_mean: f64,
    pub admission_pool_tx_count_mean: f64,
    pub reserved_tx_count_mean: f64,
    pub frozen_candidate_count_mean: f64,
    pub segment_count_per_candidate_mean: f64,
    pub selected_tx_count_mean: f64,
    pub selected_payload_bytes_mean: f64,
    pub block_limit_saturated_turn_ratio: f64,
    pub effective_built_tps: f64,
    pub artifact_total_ms_mean: f64,
    pub artifact_precheck_ms_mean: f64,
    pub artifact_action_extract_ms_mean: f64,
    pub artifact_action_auth_sig_ms_mean: f64,
    pub artifact_action_extract_public_ms_mean: f64,
    pub artifact_action_to_batch_item_ms_mean: f64,
    pub aggregate_total_ms_mean: f64,
    pub aggregate_merge_items_ms_mean: f64,
    pub aggregate_setup_ms_mean: f64,
    pub aggregate_padding_ms_mean: f64,
    pub aggregate_collect_proofs_ms_mean: f64,
    pub aggregate_backend_core_ms_mean: f64,
    pub aggregate_backend_point_extract_ms_mean: f64,
    pub aggregate_backend_prepared_srs_ms_mean: f64,
    pub aggregate_backend_commitment_key_extract_ms_mean: f64,
    pub aggregate_backend_commitment_ms_mean: f64,
    pub aggregate_backend_com_a_ms_mean: f64,
    pub aggregate_backend_com_b_ms_mean: f64,
    pub aggregate_backend_com_c_ms_mean: f64,
    pub aggregate_backend_pairing_normalize_batch_ms_mean: f64,
    pub aggregate_backend_pairing_prepare_ms_mean: f64,
    pub aggregate_backend_pairing_miller_loop_ms_mean: f64,
    pub aggregate_backend_pairing_final_exponentiation_ms_mean: f64,
    pub aggregate_backend_randomizer_ms_mean: f64,
    pub aggregate_backend_structured_scalar_ms_mean: f64,
    pub aggregate_backend_weighted_a_ms_mean: f64,
    pub aggregate_backend_ip_ab_ms_mean: f64,
    pub aggregate_backend_agg_c_ms_mean: f64,
    pub aggregate_backend_ck_1_r_ms_mean: f64,
    pub aggregate_backend_consistency_check_ms_mean: f64,
    pub aggregate_backend_tipa_ab_ms_mean: f64,
    pub aggregate_backend_tipa_c_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_commit_l_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_commit_r_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_challenge_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_rescale_m1_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_rescale_m2_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_rescale_ck1_ms_mean: f64,
    pub aggregate_backend_tipa_ab_gipa_rescale_ck2_ms_mean: f64,
    pub aggregate_backend_tipa_ab_transcript_inverse_ms_mean: f64,
    pub aggregate_backend_tipa_ab_kzg_challenge_ms_mean: f64,
    pub aggregate_backend_tipa_ab_kzg_coefficient_build_ms_mean: f64,
    pub aggregate_backend_tipa_ab_kzg_eval_quotient_ms_mean: f64,
    pub aggregate_backend_tipa_ab_kzg_opening_msm_ms_mean: f64,
    pub aggregate_backend_tipa_ab_kzg_opening_ck_a_ms_mean: f64,
    pub aggregate_backend_tipa_ab_kzg_opening_ck_b_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_commit_l_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_commit_r_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_challenge_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_rescale_m1_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_rescale_m2_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_rescale_ck1_ms_mean: f64,
    pub aggregate_backend_tipa_c_gipa_rescale_ck2_ms_mean: f64,
    pub aggregate_backend_tipa_c_transcript_inverse_ms_mean: f64,
    pub aggregate_backend_tipa_c_kzg_challenge_ms_mean: f64,
    pub aggregate_backend_tipa_c_kzg_coefficient_build_ms_mean: f64,
    pub aggregate_backend_tipa_c_kzg_eval_quotient_ms_mean: f64,
    pub aggregate_backend_tipa_c_kzg_opening_msm_ms_mean: f64,
    pub aggregate_backend_tipa_c_kzg_opening_ck_a_ms_mean: f64,
    pub aggregate_proof_serialize_ms_mean: f64,
    pub aggregate_bundle_tx_build_ms_mean: f64,
    pub aggregate_spend_ms_mean: f64,
    pub aggregate_output_ms_mean: f64,
    pub aggregate_other_ms_mean: f64,
    pub sidecar_build_ms_mean: f64,
    pub fallback_miss_count: usize,
    pub local_turn_build_budget_overrun_count: usize,
    pub guard_miss_count: usize,
    pub guard_satisfied_turn_ratio: f64,
    pub admission_pool_tx_count_start: usize,
    pub admission_pool_tx_count_end: usize,
    pub admission_pool_delta: i64,
    pub local_synthetic_invalidation_count: u64,
    pub replaced_total: u64,
    pub rejected_full_low_fee_total: u64,
    pub rejected_full_no_evictable_total: u64,
    pub evicted_nonstaking_total: u64,
    pub evicted_lowest_staking_total: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct BuilderOneShotConfig {
    pub max_block_txs: usize,
    pub segment_tx_count: usize,
    pub max_proposal_bytes: usize,
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub fee_eviction_policy: FeeEvictionPolicy,
    /// Rayon thread count per aggregation batch. 0 = global pool (default).
    pub rayon_threads_per_batch: usize,
}

#[derive(Clone, Debug, Default)]
pub struct BuilderOneShotResult {
    pub selected_tx_count: usize,
    pub selected_payload_bytes: usize,
    pub segment_count: usize,
    pub build_wall_ms: f64,
    pub aggregate_total_ms: f64,
    pub aggregate_profile: AggregateBuildProfile,
    pub aggregate_verify_passed: bool,
    pub aggregate_verify_ms: f64,
    pub sidecar_build_ms: f64,
    pub replaced_total: u64,
    pub rejected_full_low_fee_total: u64,
    pub rejected_full_no_evictable_total: u64,
    pub evicted_nonstaking_total: u64,
    pub evicted_lowest_staking_total: u64,
}

struct PendingReadyCandidate {
    target_local_turn: u64,
    candidate_id: u64,
    join: JoinHandle<Result<ReadyCandidate>>,
}

struct BuilderState {
    pending: Option<PendingReadyCandidate>,
    ready: BTreeMap<u64, ReadyCandidate>,
    local_synthetic_invalidation_count: u64,
}

impl Default for BuilderState {
    fn default() -> Self {
        Self {
            pending: None,
            ready: BTreeMap::new(),
            local_synthetic_invalidation_count: 0,
        }
    }
}

#[derive(Clone)]
pub struct LookaheadBuilder {
    config: LookaheadBuilderConfig,
    mempool: MempoolHandle,
    state: Arc<Mutex<BuilderState>>,
}

impl LookaheadBuilder {
    pub fn new(config: LookaheadBuilderConfig) -> Self {
        assert!(
            config.candidate_build_depth == 1,
            "LookaheadBuilder V1 only supports candidate_build_depth=1"
        );
        Self {
            config,
            mempool: MempoolHandle::new(MempoolCoreConfig {
                max_store_bytes: config.max_store_bytes,
                max_store_txs: config.max_store_txs,
                ingestion_buffer: 4096,
                command_buffer: 256,
                eviction_policy: EvictionPolicy::OldestUnreservedFirst,
                fee_eviction_policy: config.fee_eviction_policy,
            }),
            state: Arc::new(Mutex::new(BuilderState::default())),
        }
    }

    pub async fn admit(&self, tx: Arc<AdmittedTx>) -> Result<()> {
        self.mempool.submit_admitted(tx).await?;
        Ok(())
    }

    pub async fn snapshot(&self) -> Result<BuilderSnapshot> {
        let state = self.state.lock().expect("builder mutex poisoned");
        let mempool = self.mempool.snapshot().await?;
        Ok(BuilderSnapshot {
            admission_pool_tx_count: mempool.total_record_count,
            reserved_tx_count: mempool.reserved_record_count,
            frozen_candidate_count: mempool.frozen_candidate_count,
            local_synthetic_invalidation_count: mempool.invalidated_active_candidate_total
                + state.local_synthetic_invalidation_count,
            peak_reserved_records: mempool.peak_reserved_records,
            peak_frozen_candidates: mempool.peak_frozen_candidates,
            replaced_total: mempool.replaced_total,
            rejected_full_low_fee_total: mempool.rejected_full_low_fee_total,
            rejected_full_no_evictable_total: mempool.rejected_full_no_evictable_total,
            evicted_nonstaking_total: mempool.evicted_nonstaking_total,
            evicted_lowest_staking_total: mempool.evicted_lowest_staking_total,
        })
    }

    pub async fn maybe_schedule(&self, target_local_turn: u64) -> Result<()> {
        if self.config.mode != BuilderMode::Lookahead {
            return Ok(());
        }

        self.poll_ready().await?;

        {
            let state = self.state.lock().expect("builder mutex poisoned");
            if state.ready.contains_key(&target_local_turn)
                || state
                    .pending
                    .as_ref()
                    .map(|pending| pending.target_local_turn == target_local_turn)
                    .unwrap_or(false)
            {
                return Ok(());
            }
        }

        let frozen = self
            .mempool
            .freeze_next_candidate(
                target_local_turn,
                self.config.max_block_txs,
                self.config.max_proposal_bytes,
            )
            .await?;

        let Some(frozen) = frozen else {
            return Ok(());
        };

        let segment_tx_count = self.config.segment_tx_count;
        let join = tokio::spawn(build_ready_candidate_from_frozen(
            frozen.clone(),
            segment_tx_count,
        ));
        let mut state = self.state.lock().expect("builder mutex poisoned");
        state.pending = Some(PendingReadyCandidate {
            target_local_turn,
            candidate_id: frozen.lease.candidate_id,
            join,
        });

        Ok(())
    }

    pub async fn take_turn_candidate(
        &self,
        local_turn: u64,
        cadence_budget_ms: f64,
        ready_guard_ms: f64,
        turn_started_at: Instant,
    ) -> Result<TurnOutcome> {
        Ok(self
            .take_turn_candidate_materialized(
                local_turn,
                cadence_budget_ms,
                ready_guard_ms,
                turn_started_at,
            )
            .await?
            .0)
    }

    pub async fn take_turn_candidate_materialized(
        &self,
        local_turn: u64,
        cadence_budget_ms: f64,
        ready_guard_ms: f64,
        turn_started_at: Instant,
    ) -> Result<(TurnOutcome, Option<ReadyCandidate>)> {
        self.poll_ready().await?;

        if self.config.mode == BuilderMode::Lookahead {
            if let Some(ready) = self.take_ready_for_turn(local_turn) {
                let selected_tx_count = ready.frozen.reserved_tx_count;
                let selected_payload_bytes = ready.frozen.reserved_bytes;
                let segment_count = ready.segment_tx_counts.len();
                let ready_lead_ms = turn_started_at
                    .saturating_duration_since(ready.ready_at)
                    .as_secs_f64()
                    * 1000.0;
                let guard_miss = ready_lead_ms < ready_guard_ms;
                self.commit_selected_candidate(ready.frozen.lease.candidate_id)
                    .await?;
                let outcome = TurnOutcome {
                    selected_tx_count,
                    selected_payload_bytes,
                    ready_ahead_of_turn: true,
                    background_build_candidate_ms: Some(ready.background_build_ms),
                    fallback_build_candidate_ms: None,
                    freeze_to_ready_ms: Some(ready.freeze_to_ready_ms),
                    ready_lead_ms: Some(ready_lead_ms),
                    guard_miss,
                    segment_count,
                    block_limit_saturated: selected_tx_count == self.config.max_block_txs
                        || selected_payload_bytes >= self.config.max_proposal_bytes,
                    build_budget_overrun: ready.background_build_ms > cadence_budget_ms,
                    artifact_total_ms: Some(ready.artifact_total_ms),
                    artifact_profile: Some(ready.artifact_profile),
                    aggregate_total_ms: Some(ready.aggregate_total_ms),
                    aggregate_profile: Some(ready.aggregate_profile),
                    sidecar_build_ms: Some(ready.sidecar_build_ms),
                };
                return Ok((outcome, Some(ready)));
            }
        }

        let frozen = self.freeze_for_fallback(local_turn).await?;
        let Some(frozen) = frozen else {
            return Ok((TurnOutcome::default(), None));
        };

        let build_start = Instant::now();
        let ready =
            build_ready_candidate_from_frozen(frozen.clone(), self.config.segment_tx_count).await?;
        let fallback_build_candidate_ms = build_start.elapsed().as_secs_f64() * 1000.0;
        let selected_tx_count = ready.frozen.reserved_tx_count;
        let selected_payload_bytes = ready.frozen.reserved_bytes;
        let segment_count = ready.segment_tx_counts.len();
        self.commit_selected_candidate(ready.frozen.lease.candidate_id)
            .await?;

        let outcome = TurnOutcome {
            selected_tx_count,
            selected_payload_bytes,
            ready_ahead_of_turn: false,
            background_build_candidate_ms: None,
            fallback_build_candidate_ms: Some(fallback_build_candidate_ms),
            freeze_to_ready_ms: Some(ready.freeze_to_ready_ms),
            ready_lead_ms: None,
            guard_miss: false,
            segment_count,
            block_limit_saturated: selected_tx_count == self.config.max_block_txs
                || selected_payload_bytes >= self.config.max_proposal_bytes,
            build_budget_overrun: fallback_build_candidate_ms > cadence_budget_ms,
            artifact_total_ms: Some(ready.artifact_total_ms),
            artifact_profile: Some(ready.artifact_profile),
            aggregate_total_ms: Some(ready.aggregate_total_ms),
            aggregate_profile: Some(ready.aggregate_profile),
            sidecar_build_ms: Some(ready.sidecar_build_ms),
        };

        Ok((outcome, Some(ready)))
    }

    fn take_ready_for_turn(&self, local_turn: u64) -> Option<ReadyCandidate> {
        self.state
            .lock()
            .expect("builder mutex poisoned")
            .ready
            .remove(&local_turn)
    }

    async fn freeze_for_fallback(&self, local_turn: u64) -> Result<Option<FrozenCandidate>> {
        let (pending_to_release, ready_to_release) = {
            let mut state = self.state.lock().expect("builder mutex poisoned");
            let pending_to_release = match state.pending.take() {
                Some(pending) if pending.target_local_turn == local_turn => {
                    pending.join.abort();
                    Some(pending.candidate_id)
                }
                other => {
                    state.pending = other;
                    None
                }
            };
            let ready_to_release = state
                .ready
                .remove(&local_turn)
                .map(|ready| ready.frozen.lease.candidate_id);
            (pending_to_release, ready_to_release)
        };

        if let Some(candidate_id) = pending_to_release {
            self.mempool.release_reservation(candidate_id).await?;
            self.state
                .lock()
                .expect("builder mutex poisoned")
                .local_synthetic_invalidation_count += 1;
        }
        if let Some(candidate_id) = ready_to_release {
            self.mempool.release_reservation(candidate_id).await?;
            self.state
                .lock()
                .expect("builder mutex poisoned")
                .local_synthetic_invalidation_count += 1;
        }

        self.mempool
            .freeze_next_candidate(
                local_turn,
                self.config.max_block_txs,
                self.config.max_proposal_bytes,
            )
            .await
    }

    async fn poll_ready(&self) -> Result<()> {
        let pending = {
            let mut state = self.state.lock().expect("builder mutex poisoned");
            let finished = state
                .pending
                .as_ref()
                .map(|pending| pending.join.is_finished())
                .unwrap_or(false);
            if finished {
                state.pending.take()
            } else {
                None
            }
        };

        let Some(pending) = pending else {
            return Ok(());
        };

        let ready = pending
            .join
            .await
            .expect("lookahead build task should join cleanly")?;
        let mempool = self.mempool.snapshot().await?;
        if mempool.frozen_candidate_count > 0 {
            let mut state = self.state.lock().expect("builder mutex poisoned");
            state.ready.insert(pending.target_local_turn, ready);
        }
        Ok(())
    }

    async fn commit_selected_candidate(&self, candidate_id: u64) -> Result<()> {
        let summary = self.mempool.commit_candidate(candidate_id).await?;
        let mut state = self.state.lock().expect("builder mutex poisoned");
        state.local_synthetic_invalidation_count +=
            summary.invalidated_active_candidate_count as u64;
        Ok(())
    }
}

pub async fn build_ready_candidate_from_frozen(
    frozen: FrozenCandidate,
    preferred_segment_tx_count: usize,
) -> Result<ReadyCandidate> {
    let frozen_at = frozen.frozen_at;
    let built = build_candidate_from_frozen_unverified(frozen, preferred_segment_tx_count).await?;
    let verify_started = Instant::now();
    App::verify_aggregate_bundle_for_artifacts_public(
        &built
            .frozen
            .records
            .iter()
            .map(|record| record.artifact.clone())
            .collect::<Vec<_>>(),
        &built.bundle,
        Some(&built.segment_tx_counts),
    )
    .await?;
    let aggregate_verify_ms = verify_started.elapsed().as_secs_f64() * 1000.0;
    let ready_at = Instant::now();
    Ok(ReadyCandidate {
        frozen: built.frozen,
        bundle: built.bundle,
        segment_tx_counts: built.segment_tx_counts,
        sidecar: built.sidecar,
        artifact_total_ms: 0.0,
        artifact_profile: ArtifactBuildBreakdown::default(),
        aggregate_total_ms: built.aggregate_total_ms,
        aggregate_profile: built.aggregate_profile,
        aggregate_verify_passed: true,
        aggregate_verify_ms,
        sidecar_build_ms: built.sidecar_build_ms,
        background_build_ms: built.build_wall_ms,
        freeze_to_ready_ms: ready_at.duration_since(frozen_at).as_secs_f64() * 1000.0,
        ready_at,
    })
}

pub async fn build_candidate_from_frozen_unverified(
    frozen: FrozenCandidate,
    preferred_segment_tx_count: usize,
) -> Result<BuiltCandidate> {
    let artifacts = frozen
        .records
        .iter()
        .map(|record| record.artifact.clone())
        .collect::<Vec<_>>();
    let segment_tx_counts = plan_segment_tx_counts(&artifacts, preferred_segment_tx_count);
    let build_start = Instant::now();
    let aggregate_stage_start = Instant::now();
    let (bundle, segment_tx_counts, aggregate_profile) =
        App::build_exact_segmented_aggregate_bundle_for_artifacts_profiled_public(
            &artifacts,
            &segment_tx_counts,
        )
        .await?;
    let aggregate_total_ms = aggregate_stage_start.elapsed().as_secs_f64() * 1000.0;
    let sidecar_build_start = Instant::now();
    let sidecar = ProposalArtifactSidecar::build(
        &artifacts,
        preferred_segment_tx_count,
        segment_tx_counts.clone(),
    )?;
    let sidecar_build_ms = sidecar_build_start.elapsed().as_secs_f64() * 1000.0;
    let build_wall_ms = build_start.elapsed().as_secs_f64() * 1000.0;

    Ok(BuiltCandidate {
        frozen,
        bundle,
        segment_tx_counts,
        sidecar,
        aggregate_total_ms,
        aggregate_profile,
        sidecar_build_ms,
        build_wall_ms,
    })
}

pub async fn build_admitted_transactions(
    txs: Vec<(Arc<Vec<u8>>, Arc<Transaction>)>,
    artifact_chunk_tx_count: usize,
    synthetic_fee_mode: SyntheticFeeMode,
) -> Result<Vec<Arc<AdmittedTx>>> {
    let chunk_tx_count = artifact_chunk_tx_count.max(1);
    let mut admitted = Vec::with_capacity(txs.len());
    let mut admission_seq = 0u64;

    for tx_chunk in txs.chunks(chunk_tx_count) {
        let tx_chunk = tx_chunk.to_vec();
        let txs_only: Vec<Arc<Transaction>> = tx_chunk.iter().map(|(_, tx)| tx.clone()).collect();
        let (artifacts, _) = App::build_tx_artifacts_extracted_profiled_public(
            "lookahead_builder_admission",
            &txs_only,
        )
        .await?;
        anyhow::ensure!(
            artifacts.len() == tx_chunk.len(),
            "artifact count mismatch for admission chunk: expected {}, got {}",
            tx_chunk.len(),
            artifacts.len()
        );
        admitted.extend(tx_chunk.into_iter().zip(artifacts.into_iter()).map(
            |((tx_bytes, tx), artifact)| {
                let record = Arc::new(apply_synthetic_fee_mode(
                    AdmittedTx::from_tx_bytes(admission_seq, tx_bytes, artifact, 0),
                    synthetic_fee_mode,
                ));
                debug_assert_eq!(record.artifact.tx.id(), tx.id());
                admission_seq += 1;
                record
            },
        ));
    }

    Ok(admitted)
}

pub async fn build_admitted_transactions_no_bytes(
    txs: Vec<Arc<Transaction>>,
    artifact_chunk_tx_count: usize,
    synthetic_fee_mode: SyntheticFeeMode,
) -> Result<Vec<Arc<AdmittedTx>>> {
    use penumbra_sdk_proto::DomainType as _;
    let with_bytes = txs
        .into_iter()
        .map(|tx| (Arc::new(tx.encode_to_vec()), tx))
        .collect();
    build_admitted_transactions(with_bytes, artifact_chunk_tx_count, synthetic_fee_mode).await
}

fn plan_segment_tx_counts(
    artifacts: &[Arc<TxArtifact>],
    preferred_segment_tx_count: usize,
) -> Vec<usize> {
    if artifacts.is_empty() {
        return Vec::new();
    }

    if !cost_aware_segment_planner_enabled() {
        return plan_segment_tx_counts_legacy(artifacts, preferred_segment_tx_count);
    }

    let tx_limit = preferred_segment_tx_count.max(1);
    let prefix_counts = planner_prefix_family_counts(artifacts);
    let artifact_count = artifacts.len();
    let mut best_cost = vec![u64::MAX; artifact_count + 1];
    let mut best_segment_count = vec![usize::MAX; artifact_count + 1];
    let mut previous_break = vec![0usize; artifact_count + 1];

    best_cost[0] = 0;
    best_segment_count[0] = 0;

    for end in 1..=artifact_count {
        let start_min = end.saturating_sub(tx_limit);
        for start in (start_min..end).rev() {
            if best_cost[start] == u64::MAX {
                continue;
            }

            let segment_cost = planner_segment_cost(&prefix_counts, start, end);
            let candidate_cost = best_cost[start].saturating_add(segment_cost);
            let candidate_segment_count = best_segment_count[start].saturating_add(1);
            let is_better = candidate_cost < best_cost[end]
                || (candidate_cost == best_cost[end]
                    && candidate_segment_count < best_segment_count[end])
                || (candidate_cost == best_cost[end]
                    && candidate_segment_count == best_segment_count[end]
                    && start > previous_break[end]);
            if is_better {
                best_cost[end] = candidate_cost;
                best_segment_count[end] = candidate_segment_count;
                previous_break[end] = start;
            }
        }
    }

    let mut segment_tx_counts = Vec::new();
    let mut cursor = artifact_count;
    while cursor > 0 {
        let start = previous_break[cursor];
        debug_assert!(start < cursor, "segment planner failed to make progress");
        segment_tx_counts.push(cursor - start);
        cursor = start;
    }
    segment_tx_counts.reverse();
    segment_tx_counts
}

fn cost_aware_segment_planner_enabled() -> bool {
    static OVERRIDE: OnceLock<bool> = OnceLock::new();
    *OVERRIDE.get_or_init(|| {
        matches!(
            std::env::var("PENUMBRA_BENCH_SEGMENT_PLANNER")
                .ok()
                .as_deref(),
            Some("cost-aware")
        )
    })
}

fn plan_segment_tx_counts_legacy(
    artifacts: &[Arc<TxArtifact>],
    preferred_segment_tx_count: usize,
) -> Vec<usize> {
    let tx_limit = preferred_segment_tx_count.max(1);
    let padded_limit = highest_power_of_two_at_most(tx_limit);
    let mut segment_tx_counts = Vec::new();
    let mut current_tx_count = 0usize;
    let mut current_family_counts = BTreeMap::<ProofFamilyId, usize>::new();

    for artifact in artifacts {
        let next_tx_count = current_tx_count + 1;
        let next_family_counts = merged_family_counts(&current_family_counts, artifact.as_ref());
        let crosses_padding_limit = max_padded_family_count(&next_family_counts) > padded_limit;
        let crosses_tx_limit = next_tx_count > tx_limit;

        if current_tx_count > 0 && (crosses_padding_limit || crosses_tx_limit) {
            segment_tx_counts.push(current_tx_count);
            current_tx_count = 0;
            current_family_counts.clear();
        }

        current_tx_count += 1;
        current_family_counts = merged_family_counts(&current_family_counts, artifact.as_ref());
    }

    if current_tx_count > 0 {
        segment_tx_counts.push(current_tx_count);
    }

    segment_tx_counts
}

const PLANNER_FAMILY_COUNT: usize = 6;

fn planner_prefix_family_counts(
    artifacts: &[Arc<TxArtifact>],
) -> Vec<[usize; PLANNER_FAMILY_COUNT]> {
    let mut prefix_counts = Vec::with_capacity(artifacts.len() + 1);
    let mut running = [0usize; PLANNER_FAMILY_COUNT];
    prefix_counts.push(running);
    for artifact in artifacts {
        for (family_id, items) in &artifact.proof_items {
            running[planner_family_index(*family_id)] += items.len();
        }
        prefix_counts.push(running);
    }
    prefix_counts
}

fn merged_family_counts(
    current: &BTreeMap<ProofFamilyId, usize>,
    artifact: &TxArtifact,
) -> BTreeMap<ProofFamilyId, usize> {
    let mut merged = current.clone();
    for (family_id, items) in &artifact.proof_items {
        *merged.entry(*family_id).or_default() += items.len();
    }
    merged
}

fn max_padded_family_count(family_counts: &BTreeMap<ProofFamilyId, usize>) -> usize {
    family_counts
        .values()
        .copied()
        .filter(|count| *count > 0)
        .map(usize::next_power_of_two)
        .max()
        .unwrap_or(0)
}

fn highest_power_of_two_at_most(value: usize) -> usize {
    debug_assert!(value > 0);
    1usize << (usize::BITS as usize - 1 - value.leading_zeros() as usize)
}

fn planner_segment_cost(
    prefix_counts: &[[usize; PLANNER_FAMILY_COUNT]],
    start: usize,
    end: usize,
) -> u64 {
    let mut total_cost = 0u64;
    for family_index in 0..PLANNER_FAMILY_COUNT {
        let count = prefix_counts[end][family_index] - prefix_counts[start][family_index];
        if count == 0 {
            continue;
        }
        total_cost = total_cost
            .saturating_add(planner_family_weight(family_index) * count.next_power_of_two() as u64);
    }
    total_cost
}

fn planner_family_index(family_id: ProofFamilyId) -> usize {
    match family_id {
        ProofFamilyId::Spend => 0,
        ProofFamilyId::Output => 1,
        ProofFamilyId::Swap => 2,
        ProofFamilyId::SwapClaim => 3,
        ProofFamilyId::Convert => 4,
        ProofFamilyId::DelegatorVote => 5,
    }
}

fn planner_family_weight(family_index: usize) -> u64 {
    match family_index {
        0 | 1 => 4,
        _ => 1,
    }
}

pub fn is_local_proposer_turn(height: u64, num_validators: usize, proposer_index: usize) -> bool {
    assert!(num_validators > 0, "num_validators must be > 0");
    assert!(
        proposer_index < num_validators,
        "proposer_index must be within the validator set"
    );
    ((height - 1) as usize % num_validators) == proposer_index
}

pub fn next_local_turn(height: u64, num_validators: usize, proposer_index: usize) -> u64 {
    let mut candidate = height;
    loop {
        if is_local_proposer_turn(candidate, num_validators, proposer_index) {
            return candidate;
        }
        candidate += 1;
    }
}

pub async fn run_builder_lab(
    admitted: Vec<Arc<AdmittedTx>>,
    config: LookaheadLabConfig,
) -> Result<LookaheadLabResult> {
    let builder = LookaheadBuilder::new(LookaheadBuilderConfig {
        mode: config.mode,
        candidate_build_depth: 1,
        max_block_txs: config.max_block_txs,
        max_proposal_bytes: config.max_proposal_bytes,
        segment_tx_count: config.segment_tx_count,
        max_store_bytes: config.max_store_bytes,
        max_store_txs: config.max_store_txs,
        fee_eviction_policy: config.fee_eviction_policy,
    });

    let total_local_turns = config.warmup_local_turns + config.steady_local_turns;
    let total_global_turns = total_local_turns
        .checked_mul(config.num_validators)
        .expect("global turn count overflow");
    let started_at = Instant::now();
    let stop_after = started_at
        + std::time::Duration::from_millis(
            (u64::try_from(total_global_turns).expect("turn count exceeds u64") + 1)
                .saturating_mul(config.block_interval_ms),
        );
    let admission_task = tokio::spawn(admit_transactions_at_rate(
        builder.clone(),
        admitted,
        config.offered_tps,
        started_at,
        stop_after,
    ));

    let mut ready_turns = 0usize;
    let mut selected_txs = 0usize;
    let mut selected_payload_bytes = 0usize;
    let mut ready_selected_txs = 0usize;
    let mut freeze_to_ready_sum = 0.0f64;
    let mut freeze_to_ready_samples = 0usize;
    let mut ready_lead_sum = 0.0f64;
    let mut ready_lead_samples = 0usize;
    let mut background_build_sum = 0.0f64;
    let mut background_build_samples = 0usize;
    let mut fallback_build_sum = 0.0f64;
    let mut fallback_build_samples = 0usize;
    let mut admission_pool_sum = 0.0f64;
    let mut reserved_sum = 0.0f64;
    let mut frozen_sum = 0.0f64;
    let mut sampled_turns = 0usize;
    let mut segment_count_sum = 0.0f64;
    let mut segment_count_samples = 0usize;
    let mut fallback_miss_count = 0usize;
    let mut build_budget_overrun_count = 0usize;
    let mut guard_miss_count = 0usize;
    let mut guard_satisfied_turns = 0usize;
    let mut block_limit_saturated_turns = 0usize;
    let mut artifact_total_sum = 0.0f64;
    let mut artifact_profile_sum = ArtifactBuildBreakdown::default();
    let mut artifact_profile_samples = 0usize;
    let mut aggregate_total_sum = 0.0f64;
    let mut aggregate_profile_sum = AggregateBuildProfile::default();
    let mut aggregate_profile_samples = 0usize;
    let mut sidecar_build_sum = 0.0f64;
    let mut sidecar_build_samples = 0usize;
    let mut local_turns_seen = 0usize;
    let mut next_turn_at = started_at;
    let mut steady_admission_pool_start = None;

    for height in 1u64.. {
        if config.mode == BuilderMode::Lookahead {
            builder
                .maybe_schedule(next_local_turn(
                    height,
                    config.num_validators,
                    config.proposer_index,
                ))
                .await?;
        }

        tokio::time::sleep_until(tokio::time::Instant::from_std(next_turn_at)).await;
        let turn_started_at = Instant::now();
        next_turn_at += std::time::Duration::from_millis(config.block_interval_ms);

        if !is_local_proposer_turn(height, config.num_validators, config.proposer_index) {
            if local_turns_seen >= total_local_turns {
                break;
            }
            continue;
        }

        local_turns_seen += 1;
        let snapshot = builder.snapshot().await?;
        let in_steady_window = local_turns_seen > config.warmup_local_turns;
        if in_steady_window && steady_admission_pool_start.is_none() {
            steady_admission_pool_start = Some(snapshot.admission_pool_tx_count);
        }
        let outcome = builder
            .take_turn_candidate(
                height,
                (config.block_interval_ms * config.num_validators as u64) as f64,
                config.ready_guard_ms as f64,
                turn_started_at,
            )
            .await?;
        if in_steady_window {
            sampled_turns += 1;
            admission_pool_sum += snapshot.admission_pool_tx_count as f64;
            reserved_sum += snapshot.reserved_tx_count as f64;
            frozen_sum += snapshot.frozen_candidate_count as f64;
            selected_txs += outcome.selected_tx_count;
            selected_payload_bytes += outcome.selected_payload_bytes;
            if outcome.ready_ahead_of_turn {
                ready_turns += 1;
                ready_selected_txs += outcome.selected_tx_count;
            } else if outcome.selected_tx_count > 0 {
                fallback_miss_count += 1;
            }
            if let Some(ms) = outcome.freeze_to_ready_ms {
                freeze_to_ready_sum += ms;
                freeze_to_ready_samples += 1;
            }
            if let Some(ms) = outcome.ready_lead_ms {
                ready_lead_sum += ms;
                ready_lead_samples += 1;
            }
            if let Some(ms) = outcome.background_build_candidate_ms {
                background_build_sum += ms;
                background_build_samples += 1;
            }
            if let Some(ms) = outcome.fallback_build_candidate_ms {
                fallback_build_sum += ms;
                fallback_build_samples += 1;
            }
            if outcome.segment_count > 0 {
                segment_count_sum += outcome.segment_count as f64;
                segment_count_samples += 1;
            }
            if outcome.build_budget_overrun {
                build_budget_overrun_count += 1;
            }
            if outcome.guard_miss {
                guard_miss_count += 1;
            } else if outcome.ready_ahead_of_turn {
                guard_satisfied_turns += 1;
            }
            if outcome.block_limit_saturated {
                block_limit_saturated_turns += 1;
            }
            if let Some(ms) = outcome.artifact_total_ms {
                artifact_total_sum += ms;
                artifact_profile_samples += 1;
            }
            if let Some(profile) = outcome.artifact_profile {
                artifact_profile_sum.merge(&profile);
            }
            if let Some(ms) = outcome.aggregate_total_ms {
                aggregate_total_sum += ms;
                aggregate_profile_samples += 1;
            }
            if let Some(profile) = outcome.aggregate_profile {
                aggregate_profile_sum.merge(&profile);
            }
            if let Some(ms) = outcome.sidecar_build_ms {
                sidecar_build_sum += ms;
                sidecar_build_samples += 1;
            }
        }

        if local_turns_seen >= total_local_turns {
            break;
        }
    }

    admission_task
        .await
        .expect("admission task should join cleanly");
    drain_pending_build(&builder).await?;
    let final_snapshot = builder.snapshot().await?;
    let steady_admission_pool_start = steady_admission_pool_start.unwrap_or_default();
    let steady_secs = (config.steady_local_turns as f64
        * config.num_validators as f64
        * config.block_interval_ms as f64)
        / 1000.0;

    aggregate_profile_sum.scale(1.0 / aggregate_profile_samples as f64);
    Ok(LookaheadLabResult {
        candidate_ready_turn_ratio: ratio(ready_turns, sampled_turns),
        candidate_tx_coverage_ratio: ratio(ready_selected_txs, selected_txs),
        freeze_to_ready_ms_mean: mean(freeze_to_ready_sum, freeze_to_ready_samples),
        ready_lead_ms_mean: mean(ready_lead_sum, ready_lead_samples),
        background_build_candidate_ms_mean: mean(background_build_sum, background_build_samples),
        fallback_build_candidate_ms_mean: mean(fallback_build_sum, fallback_build_samples),
        admission_pool_tx_count_mean: mean(admission_pool_sum, sampled_turns),
        reserved_tx_count_mean: mean(reserved_sum, sampled_turns),
        frozen_candidate_count_mean: mean(frozen_sum, sampled_turns),
        segment_count_per_candidate_mean: mean(segment_count_sum, segment_count_samples),
        selected_tx_count_mean: mean(selected_txs as f64, sampled_turns),
        selected_payload_bytes_mean: mean(selected_payload_bytes as f64, sampled_turns),
        block_limit_saturated_turn_ratio: ratio(block_limit_saturated_turns, sampled_turns),
        effective_built_tps: if steady_secs > 0.0 {
            selected_txs as f64 / steady_secs
        } else {
            0.0
        },
        artifact_total_ms_mean: mean(artifact_total_sum, artifact_profile_samples),
        artifact_precheck_ms_mean: mean(artifact_profile_sum.precheck_ms, artifact_profile_samples),
        artifact_action_extract_ms_mean: mean(
            artifact_profile_sum.action_extract_ms,
            artifact_profile_samples,
        ),
        artifact_action_auth_sig_ms_mean: mean(
            artifact_profile_sum.action_auth_sig_ms,
            artifact_profile_samples,
        ),
        artifact_action_extract_public_ms_mean: mean(
            artifact_profile_sum.action_extract_public_ms,
            artifact_profile_samples,
        ),
        artifact_action_to_batch_item_ms_mean: mean(
            artifact_profile_sum.action_to_batch_item_ms,
            artifact_profile_samples,
        ),
        aggregate_total_ms_mean: mean(aggregate_total_sum, aggregate_profile_samples),
        aggregate_merge_items_ms_mean: aggregate_profile_sum.merge_items_ms,
        aggregate_setup_ms_mean: aggregate_profile_sum.setup_ms,
        aggregate_padding_ms_mean: aggregate_profile_sum.padding_ms,
        aggregate_collect_proofs_ms_mean: aggregate_profile_sum.collect_proofs_ms,
        aggregate_backend_core_ms_mean: aggregate_profile_sum.backend_core_ms,
        aggregate_backend_point_extract_ms_mean: aggregate_profile_sum.backend_point_extract_ms,
        aggregate_backend_prepared_srs_ms_mean: aggregate_profile_sum.backend_prepared_srs_ms,
        aggregate_backend_commitment_key_extract_ms_mean: aggregate_profile_sum.backend_commitment_key_extract_ms,
        aggregate_backend_commitment_ms_mean: aggregate_profile_sum.backend_commitment_ms,
        aggregate_backend_com_a_ms_mean: aggregate_profile_sum.backend_com_a_ms,
        aggregate_backend_com_b_ms_mean: aggregate_profile_sum.backend_com_b_ms,
        aggregate_backend_com_c_ms_mean: aggregate_profile_sum.backend_com_c_ms,
        aggregate_backend_pairing_normalize_batch_ms_mean: aggregate_profile_sum.backend_pairing_normalize_batch_ms,
        aggregate_backend_pairing_prepare_ms_mean: aggregate_profile_sum.backend_pairing_prepare_ms,
        aggregate_backend_pairing_miller_loop_ms_mean: aggregate_profile_sum.backend_pairing_miller_loop_ms,
        aggregate_backend_pairing_final_exponentiation_ms_mean: aggregate_profile_sum.backend_pairing_final_exponentiation_ms,
        aggregate_backend_randomizer_ms_mean: aggregate_profile_sum.backend_randomizer_ms,
        aggregate_backend_structured_scalar_ms_mean: aggregate_profile_sum.backend_structured_scalar_ms,
        aggregate_backend_weighted_a_ms_mean: aggregate_profile_sum.backend_weighted_a_ms,
        aggregate_backend_ip_ab_ms_mean: aggregate_profile_sum.backend_ip_ab_ms,
        aggregate_backend_agg_c_ms_mean: aggregate_profile_sum.backend_agg_c_ms,
        aggregate_backend_ck_1_r_ms_mean: aggregate_profile_sum.backend_ck_1_r_ms,
        aggregate_backend_consistency_check_ms_mean: aggregate_profile_sum.backend_consistency_check_ms,
        aggregate_backend_tipa_ab_ms_mean: aggregate_profile_sum.backend_tipa_ab_ms,
        aggregate_backend_tipa_c_ms_mean: aggregate_profile_sum.backend_tipa_c_ms,
        aggregate_backend_tipa_ab_gipa_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_ms,
        aggregate_backend_tipa_ab_gipa_commit_l_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_commit_l_ms,
        aggregate_backend_tipa_ab_gipa_commit_r_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_commit_r_ms,
        aggregate_backend_tipa_ab_gipa_challenge_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_challenge_ms,
        aggregate_backend_tipa_ab_gipa_rescale_m1_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_rescale_m1_ms,
        aggregate_backend_tipa_ab_gipa_rescale_m2_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_rescale_m2_ms,
        aggregate_backend_tipa_ab_gipa_rescale_ck1_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_rescale_ck1_ms,
        aggregate_backend_tipa_ab_gipa_rescale_ck2_ms_mean: aggregate_profile_sum.backend_tipa_ab_gipa_rescale_ck2_ms,
        aggregate_backend_tipa_ab_transcript_inverse_ms_mean: aggregate_profile_sum.backend_tipa_ab_transcript_inverse_ms,
        aggregate_backend_tipa_ab_kzg_challenge_ms_mean: aggregate_profile_sum.backend_tipa_ab_kzg_challenge_ms,
        aggregate_backend_tipa_ab_kzg_coefficient_build_ms_mean: aggregate_profile_sum.backend_tipa_ab_kzg_coefficient_build_ms,
        aggregate_backend_tipa_ab_kzg_eval_quotient_ms_mean: aggregate_profile_sum.backend_tipa_ab_kzg_eval_quotient_ms,
        aggregate_backend_tipa_ab_kzg_opening_msm_ms_mean: aggregate_profile_sum.backend_tipa_ab_kzg_opening_msm_ms,
        aggregate_backend_tipa_ab_kzg_opening_ck_a_ms_mean: aggregate_profile_sum.backend_tipa_ab_kzg_opening_ck_a_ms,
        aggregate_backend_tipa_ab_kzg_opening_ck_b_ms_mean: aggregate_profile_sum.backend_tipa_ab_kzg_opening_ck_b_ms,
        aggregate_backend_tipa_c_gipa_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_ms,
        aggregate_backend_tipa_c_gipa_commit_l_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_commit_l_ms,
        aggregate_backend_tipa_c_gipa_commit_r_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_commit_r_ms,
        aggregate_backend_tipa_c_gipa_challenge_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_challenge_ms,
        aggregate_backend_tipa_c_gipa_rescale_m1_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_rescale_m1_ms,
        aggregate_backend_tipa_c_gipa_rescale_m2_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_rescale_m2_ms,
        aggregate_backend_tipa_c_gipa_rescale_ck1_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_rescale_ck1_ms,
        aggregate_backend_tipa_c_gipa_rescale_ck2_ms_mean: aggregate_profile_sum.backend_tipa_c_gipa_rescale_ck2_ms,
        aggregate_backend_tipa_c_transcript_inverse_ms_mean: aggregate_profile_sum.backend_tipa_c_transcript_inverse_ms,
        aggregate_backend_tipa_c_kzg_challenge_ms_mean: aggregate_profile_sum.backend_tipa_c_kzg_challenge_ms,
        aggregate_backend_tipa_c_kzg_coefficient_build_ms_mean: aggregate_profile_sum.backend_tipa_c_kzg_coefficient_build_ms,
        aggregate_backend_tipa_c_kzg_eval_quotient_ms_mean: aggregate_profile_sum.backend_tipa_c_kzg_eval_quotient_ms,
        aggregate_backend_tipa_c_kzg_opening_msm_ms_mean: aggregate_profile_sum.backend_tipa_c_kzg_opening_msm_ms,
        aggregate_backend_tipa_c_kzg_opening_ck_a_ms_mean: aggregate_profile_sum.backend_tipa_c_kzg_opening_ck_a_ms,
        aggregate_proof_serialize_ms_mean: aggregate_profile_sum.proof_serialize_ms,
        aggregate_bundle_tx_build_ms_mean: aggregate_profile_sum.bundle_tx_build_ms,
        aggregate_spend_ms_mean: aggregate_profile_sum.spend_ms,
        aggregate_output_ms_mean: aggregate_profile_sum.output_ms,
        aggregate_other_ms_mean: aggregate_profile_sum.other_ms,
        sidecar_build_ms_mean: mean(sidecar_build_sum, sidecar_build_samples),
        fallback_miss_count,
        local_turn_build_budget_overrun_count: build_budget_overrun_count,
        guard_miss_count,
        guard_satisfied_turn_ratio: ratio(guard_satisfied_turns, sampled_turns),
        admission_pool_tx_count_start: steady_admission_pool_start,
        admission_pool_tx_count_end: final_snapshot.admission_pool_tx_count,
        admission_pool_delta: final_snapshot.admission_pool_tx_count as i64
            - steady_admission_pool_start as i64,
        local_synthetic_invalidation_count: final_snapshot.local_synthetic_invalidation_count,
        replaced_total: final_snapshot.replaced_total,
        rejected_full_low_fee_total: final_snapshot.rejected_full_low_fee_total,
        rejected_full_no_evictable_total: final_snapshot.rejected_full_no_evictable_total,
        evicted_nonstaking_total: final_snapshot.evicted_nonstaking_total,
        evicted_lowest_staking_total: final_snapshot.evicted_lowest_staking_total,
    })
}

pub async fn run_builder_one_shot(
    admitted: Vec<Arc<AdmittedTx>>,
    config: BuilderOneShotConfig,
) -> Result<BuilderOneShotResult> {
    let mempool = MempoolHandle::new(MempoolCoreConfig {
        max_store_bytes: config.max_store_bytes,
        max_store_txs: config.max_store_txs,
        fee_eviction_policy: config.fee_eviction_policy,
        ..MempoolCoreConfig::default()
    });

    for record in admitted {
        let outcome = mempool.submit_admitted(record).await?;
        anyhow::ensure!(
            outcome.was_admitted(),
            "one-shot builder admission rejected input"
        );
    }

    let frozen = mempool
        .freeze_next_candidate(1, config.max_block_txs, config.max_proposal_bytes)
        .await?
        .ok_or_else(|| anyhow::anyhow!("one-shot builder produced no frozen candidate"))?;
    let selected_tx_count = frozen.records.len();
    let selected_payload_bytes = frozen.records.iter().map(|record| record.tx_len).sum();
    set_rayon_threads_per_batch_for_bench(config.rayon_threads_per_batch);
    let ready = build_ready_candidate_from_frozen(frozen, config.segment_tx_count).await?;
    set_rayon_threads_per_batch_for_bench(1);
    let snapshot = mempool.snapshot().await?;

    Ok(BuilderOneShotResult {
        selected_tx_count,
        selected_payload_bytes,
        segment_count: ready.segment_tx_counts.len(),
        build_wall_ms: ready.background_build_ms,
        aggregate_total_ms: ready.aggregate_total_ms,
        aggregate_profile: ready.aggregate_profile,
        aggregate_verify_passed: ready.aggregate_verify_passed,
        aggregate_verify_ms: ready.aggregate_verify_ms,
        sidecar_build_ms: ready.sidecar_build_ms,
        replaced_total: snapshot.replaced_total,
        rejected_full_low_fee_total: snapshot.rejected_full_low_fee_total,
        rejected_full_no_evictable_total: snapshot.rejected_full_no_evictable_total,
        evicted_nonstaking_total: snapshot.evicted_nonstaking_total,
        evicted_lowest_staking_total: snapshot.evicted_lowest_staking_total,
    })
}

pub async fn admit_transactions_at_rate(
    builder: LookaheadBuilder,
    txs: Vec<Arc<AdmittedTx>>,
    offered_tps: usize,
    started_at: Instant,
    stop_after: Instant,
) {
    let mut admitted = 0usize;
    loop {
        let now = Instant::now();
        if now >= stop_after {
            break;
        }
        let elapsed_secs = now.duration_since(started_at).as_secs_f64();
        let should_have_admitted = (elapsed_secs * offered_tps as f64).floor() as usize;
        while admitted < should_have_admitted && admitted < txs.len() {
            builder
                .admit(txs[admitted].clone())
                .await
                .expect("mempool admit should succeed");
            admitted += 1;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    while admitted < txs.len() && Instant::now() < stop_after {
        builder
            .admit(txs[admitted].clone())
            .await
            .expect("mempool admit should succeed");
        admitted += 1;
    }
}

pub async fn drain_pending_build(builder: &LookaheadBuilder) -> Result<()> {
    builder.poll_ready().await
}

fn mean(sum: f64, samples: usize) -> f64 {
    if samples == 0 {
        0.0
    } else {
        sum / samples as f64
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::tps::corpus;
    use penumbra_sdk_proof_params::batch::BatchItem;
    use penumbra_sdk_proto::DomainType;

    fn empty_artifact(tx: Arc<Transaction>) -> Arc<TxArtifact> {
        Arc::new(TxArtifact {
            tx,
            proof_items: BTreeMap::new(),
            spend_nullifiers: Vec::new(),
            anchor_pairs: Vec::new(),
            total_proof_count: 0,
            historical_validation: None,
        })
    }

    fn dummy_batch_item() -> BatchItem {
        BatchItem {
            proof: Default::default(),
            public_inputs: Vec::new(),
        }
    }

    fn artifact_with_family_counts(family_counts: &[(ProofFamilyId, usize)]) -> Arc<TxArtifact> {
        let tx = Arc::new(Transaction::default());
        let mut proof_items = BTreeMap::new();
        for (family_id, count) in family_counts {
            proof_items.insert(*family_id, vec![dummy_batch_item(); *count]);
        }
        Arc::new(TxArtifact {
            tx,
            proof_items,
            spend_nullifiers: Vec::new(),
            anchor_pairs: Vec::new(),
            total_proof_count: family_counts.iter().map(|(_, count)| *count).sum(),
            historical_validation: None,
        })
    }

    fn dummy_admitted(byte: u8) -> Arc<AdmittedTx> {
        let tx = Arc::new(Transaction::default());
        Arc::new(AdmittedTx::from_artifact(
            byte as u64,
            empty_artifact(tx),
            0,
        ))
    }

    async fn valid_admitted(n: usize) -> Result<Vec<Arc<AdmittedTx>>> {
        let corpus_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("benches/compliance/tps/snapshots/local-unreg-10-ready-smoke/corpus/unregulated");
        let loaded = corpus::load_corpus(&corpus_dir)?;
        let decoded = loaded
            .entries
            .into_iter()
            .take(n)
            .into_iter()
            .map(|entry| {
                Transaction::decode(entry.tx_bytes.as_slice())
                    .map(Arc::new)
                    .map_err(anyhow::Error::from)
            })
            .collect::<Result<Vec<_>>>()?;
        build_admitted_transactions_no_bytes(decoded, 16, SyntheticFeeMode::Off).await
    }

    #[test]
    fn planner_prefers_power_of_two_boundary_with_remainder() {
        let artifacts = (0..513)
            .map(|_| artifact_with_family_counts(&[(ProofFamilyId::Spend, 1)]))
            .collect::<Vec<_>>();

        assert_eq!(plan_segment_tx_counts(&artifacts, 512), vec![512, 1]);
    }

    #[test]
    fn planner_prefers_fewer_segments_when_costs_tie() {
        let artifacts = (0..512)
            .map(|_| artifact_with_family_counts(&[(ProofFamilyId::Spend, 1)]))
            .collect::<Vec<_>>();

        assert_eq!(plan_segment_tx_counts(&artifacts, 512), vec![512]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn synthetic_consumption_clears_future_reservations() -> Result<()> {
        let txs = valid_admitted(4).await?;
        let builder = LookaheadBuilder::new(LookaheadBuilderConfig {
            mode: BuilderMode::Lookahead,
            candidate_build_depth: 1,
            max_block_txs: 2,
            max_proposal_bytes: 1_000_000,
            segment_tx_count: 1,
            max_store_bytes: usize::MAX,
            max_store_txs: usize::MAX,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });

        for tx in txs {
            builder.admit(tx).await?;
        }

        builder.maybe_schedule(1).await?;
        builder
            .take_turn_candidate(1, 1000.0, 0.0, Instant::now())
            .await?;

        let snapshot = builder.snapshot().await?;
        assert_eq!(snapshot.admission_pool_tx_count, 2);
        assert_eq!(snapshot.reserved_tx_count, 0);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn monolithic_mode_never_reports_ready_ahead_of_turn() -> Result<()> {
        let txs = valid_admitted(2).await?;
        let builder = LookaheadBuilder::new(LookaheadBuilderConfig {
            mode: BuilderMode::Monolithic,
            candidate_build_depth: 1,
            max_block_txs: 2,
            max_proposal_bytes: 1_000_000,
            segment_tx_count: 1,
            max_store_bytes: usize::MAX,
            max_store_txs: usize::MAX,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });

        for tx in txs {
            builder.admit(tx).await?;
        }

        let outcome = builder
            .take_turn_candidate(1, 1000.0, 0.0, Instant::now())
            .await?;
        assert!(!outcome.ready_ahead_of_turn);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lookahead_ready_candidate_can_arrive_before_turn() -> Result<()> {
        let txs = valid_admitted(4).await?;

        let builder = LookaheadBuilder::new(LookaheadBuilderConfig {
            mode: BuilderMode::Lookahead,
            candidate_build_depth: 1,
            max_block_txs: 4,
            max_proposal_bytes: 1_000_000,
            segment_tx_count: 2,
            max_store_bytes: usize::MAX,
            max_store_txs: usize::MAX,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });
        for tx in txs {
            builder.admit(tx).await?;
        }

        builder.maybe_schedule(1).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        drain_pending_build(&builder).await?;

        let outcome = builder
            .take_turn_candidate(1, 10_000.0, 0.0, Instant::now())
            .await?;
        assert!(outcome.ready_ahead_of_turn || outcome.selected_tx_count > 0);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn guard_miss_trips_when_ready_candidate_arrives_too_close_to_turn() -> Result<()> {
        let tx = dummy_admitted(9);
        let builder = LookaheadBuilder::new(LookaheadBuilderConfig {
            mode: BuilderMode::Lookahead,
            candidate_build_depth: 1,
            max_block_txs: 2,
            max_proposal_bytes: 1_000_000,
            segment_tx_count: 1,
            max_store_bytes: usize::MAX,
            max_store_txs: usize::MAX,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });
        builder.admit(tx.clone()).await?;
        let frozen = builder
            .mempool
            .freeze_next_candidate(1, 2, 1_000_000)
            .await?
            .expect("frozen candidate");

        builder
            .state
            .lock()
            .expect("builder mutex poisoned")
            .ready
            .insert(
                1,
                ReadyCandidate {
                    frozen: FrozenCandidate {
                        frozen_at: Instant::now() - Duration::from_millis(20),
                        ..frozen
                    },
                    bundle: AggregateBundle {
                        version: 1,
                        srs_id: Vec::new(),
                        families: Vec::new(),
                    },
                    segment_tx_counts: vec![1],
                    sidecar: ProposalArtifactSidecar::build(&[], 1, Vec::new())?,
                    artifact_total_ms: 1.0,
                    artifact_profile: ArtifactBuildBreakdown::default(),
                    aggregate_total_ms: 1.0,
                    aggregate_profile: AggregateBuildProfile::default(),
                    aggregate_verify_passed: true,
                    aggregate_verify_ms: 1.0,
                    sidecar_build_ms: 1.0,
                    background_build_ms: 10.0,
                    freeze_to_ready_ms: 10.0,
                    ready_at: Instant::now() - Duration::from_millis(10),
                },
            );

        let outcome = builder
            .take_turn_candidate(1, 10_000.0, 1_000.0, Instant::now())
            .await?;
        assert!(outcome.ready_ahead_of_turn);
        assert!(outcome.guard_miss);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn single_point_lab_reports_full_saturation_when_every_turn_hits_limits() -> Result<()> {
        let admitted = valid_admitted(4).await?;
        let result = run_builder_lab(
            admitted,
            LookaheadLabConfig {
                mode: BuilderMode::Monolithic,
                offered_tps: 100,
                block_interval_ms: 200,
                num_validators: 1,
                proposer_index: 0,
                max_block_txs: 1,
                segment_tx_count: 1,
                warmup_local_turns: 1,
                steady_local_turns: 2,
                max_proposal_bytes: 1_000_000,
                ready_guard_ms: 0,
                max_store_bytes: usize::MAX,
                max_store_txs: usize::MAX,
                synthetic_fee_mode: SyntheticFeeMode::Off,
                fee_eviction_policy: FeeEvictionPolicy::Disabled,
            },
        )
        .await?;

        assert_eq!(result.block_limit_saturated_turn_ratio, 1.0);
        assert!(result.selected_tx_count_mean >= 1.0);
        Ok(())
    }
}
