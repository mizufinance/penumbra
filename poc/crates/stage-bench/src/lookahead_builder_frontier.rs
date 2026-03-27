use std::cmp::Ordering;
use std::collections::BTreeMap;

use anyhow::Result;

use crate::lookahead_builder::{
    run_builder_lab, AdmittedTx, BuilderMode, LookaheadLabConfig, LookaheadLabResult,
};
use crate::mempool::SyntheticFeeMode;
use penumbra_sdk_poc_preconsensus::local_mempool::FeeEvictionPolicy;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct FrontierSweepConfig {
    pub mode: BuilderMode,
    pub offered_tps_list: Vec<usize>,
    pub block_interval_ms_list: Vec<u64>,
    pub max_block_txs_list: Vec<usize>,
    pub segment_tx_count_list: Vec<usize>,
    pub ready_guard_ms_list: Vec<u64>,
    pub num_validators: usize,
    pub proposer_index: usize,
    pub warmup_local_turns: usize,
    pub steady_local_turns: usize,
    pub max_proposal_bytes: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct FrontierRawRow {
    pub mode: BuilderMode,
    pub offered_tps: usize,
    pub block_interval_ms: u64,
    pub max_block_txs: usize,
    pub segment_tx_count: usize,
    pub ready_guard_ms: u64,
    pub sustainable: bool,
    #[serde(flatten)]
    pub result: LookaheadLabResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum FrontierSummaryKind {
    GroupBest,
    OverallBest,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct FrontierSummaryRow {
    pub summary_kind: FrontierSummaryKind,
    pub mode: BuilderMode,
    pub offered_tps: usize,
    pub block_interval_ms: u64,
    pub max_block_txs: usize,
    pub segment_tx_count: usize,
    pub ready_guard_ms: u64,
    pub sustainable: bool,
    #[serde(flatten)]
    pub result: LookaheadLabResult,
}

#[derive(Clone, Debug, Default)]
pub struct FrontierRun {
    pub raw_rows: Vec<FrontierRawRow>,
    pub summary_rows: Vec<FrontierSummaryRow>,
}

pub async fn run_frontier(
    admitted: &[Arc<AdmittedTx>],
    config: FrontierSweepConfig,
) -> Result<FrontierRun> {
    let mut raw_rows = Vec::new();

    for &offered_tps in &config.offered_tps_list {
        for &block_interval_ms in &config.block_interval_ms_list {
            for &max_block_txs in &config.max_block_txs_list {
                for &segment_tx_count in &config.segment_tx_count_list {
                    for &ready_guard_ms in &config.ready_guard_ms_list {
                        let result = run_builder_lab(
                            admitted.to_vec(),
                            LookaheadLabConfig {
                                mode: config.mode,
                                offered_tps,
                                block_interval_ms,
                                num_validators: config.num_validators,
                                proposer_index: config.proposer_index,
                                max_block_txs,
                                segment_tx_count,
                                warmup_local_turns: config.warmup_local_turns,
                                steady_local_turns: config.steady_local_turns,
                                max_proposal_bytes: config.max_proposal_bytes,
                                ready_guard_ms,
                                max_store_bytes: usize::MAX,
                                max_store_txs: usize::MAX,
                                synthetic_fee_mode: SyntheticFeeMode::Off,
                                fee_eviction_policy: FeeEvictionPolicy::Disabled,
                            },
                        )
                        .await?;

                        raw_rows.push(FrontierRawRow {
                            mode: config.mode,
                            offered_tps,
                            block_interval_ms,
                            max_block_txs,
                            segment_tx_count,
                            ready_guard_ms,
                            sustainable: is_sustainable(&result),
                            result,
                        });
                    }
                }
            }
        }
    }

    let mut summary_rows = summarize_frontier(&raw_rows);
    if let Some(best) = select_best_row(&raw_rows) {
        summary_rows.push(FrontierSummaryRow {
            summary_kind: FrontierSummaryKind::OverallBest,
            mode: best.mode,
            offered_tps: best.offered_tps,
            block_interval_ms: best.block_interval_ms,
            max_block_txs: best.max_block_txs,
            segment_tx_count: best.segment_tx_count,
            ready_guard_ms: best.ready_guard_ms,
            sustainable: best.sustainable,
            result: best.result.clone(),
        });
    }

    Ok(FrontierRun {
        raw_rows,
        summary_rows,
    })
}

pub fn is_sustainable(result: &LookaheadLabResult) -> bool {
    result.fallback_miss_count == 0
        && result.local_turn_build_budget_overrun_count == 0
        && result.guard_miss_count == 0
        && (result.admission_pool_delta <= 0
            || result.block_limit_saturated_turn_ratio >= 1.0 - f64::EPSILON)
}

pub fn summarize_frontier(raw_rows: &[FrontierRawRow]) -> Vec<FrontierSummaryRow> {
    let mut groups = BTreeMap::new();
    for row in raw_rows {
        groups
            .entry((
                row.mode,
                row.block_interval_ms,
                row.max_block_txs,
                row.segment_tx_count,
                row.ready_guard_ms,
            ))
            .or_insert_with(Vec::new)
            .push(row.clone());
    }

    groups
        .into_values()
        .filter_map(|rows| {
            select_best_row(&rows).map(|best| FrontierSummaryRow {
                summary_kind: FrontierSummaryKind::GroupBest,
                mode: best.mode,
                offered_tps: best.offered_tps,
                block_interval_ms: best.block_interval_ms,
                max_block_txs: best.max_block_txs,
                segment_tx_count: best.segment_tx_count,
                ready_guard_ms: best.ready_guard_ms,
                sustainable: best.sustainable,
                result: best.result.clone(),
            })
        })
        .collect()
}

pub fn select_best_row(rows: &[FrontierRawRow]) -> Option<&FrontierRawRow> {
    rows.iter().max_by(compare_frontier_rows)
}

fn compare_frontier_rows(left: &&FrontierRawRow, right: &&FrontierRawRow) -> Ordering {
    left.sustainable
        .cmp(&right.sustainable)
        .then_with(|| {
            cmp_f64(
                left.result.effective_built_tps,
                right.result.effective_built_tps,
            )
        })
        .then_with(|| {
            cmp_f64(
                left.result.candidate_tx_coverage_ratio,
                right.result.candidate_tx_coverage_ratio,
            )
        })
        .then_with(|| {
            cmp_f64(
                right.result.background_build_candidate_ms_mean,
                left.result.background_build_candidate_ms_mean,
            )
        })
        .then_with(|| left.offered_tps.cmp(&right.offered_tps))
}

fn cmp_f64(left: f64, right: f64) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> LookaheadLabResult {
        LookaheadLabResult {
            candidate_ready_turn_ratio: 1.0,
            candidate_tx_coverage_ratio: 1.0,
            freeze_to_ready_ms_mean: 10.0,
            ready_lead_ms_mean: 100.0,
            background_build_candidate_ms_mean: 25.0,
            fallback_build_candidate_ms_mean: 0.0,
            admission_pool_tx_count_mean: 10.0,
            reserved_tx_count_mean: 0.0,
            frozen_candidate_count_mean: 1.0,
            segment_count_per_candidate_mean: 2.0,
            selected_tx_count_mean: 128.0,
            selected_payload_bytes_mean: 1000.0,
            block_limit_saturated_turn_ratio: 1.0,
            effective_built_tps: 512.0,
            artifact_total_ms_mean: 20.0,
            artifact_precheck_ms_mean: 2.0,
            artifact_action_extract_ms_mean: 6.0,
            artifact_action_auth_sig_ms_mean: 4.0,
            artifact_action_extract_public_ms_mean: 4.0,
            artifact_action_to_batch_item_ms_mean: 4.0,
            aggregate_total_ms_mean: 80.0,
            aggregate_merge_items_ms_mean: 3.0,
            aggregate_setup_ms_mean: 1.0,
            aggregate_padding_ms_mean: 2.0,
            aggregate_collect_proofs_ms_mean: 10.0,
            aggregate_backend_core_ms_mean: 55.0,
            aggregate_backend_point_extract_ms_mean: 5.0,
            aggregate_backend_commitment_ms_mean: 6.0,
            aggregate_backend_randomizer_ms_mean: 1.0,
            aggregate_backend_structured_scalar_ms_mean: 2.0,
            aggregate_backend_weighted_a_ms_mean: 3.0,
            aggregate_backend_ip_ab_ms_mean: 8.0,
            aggregate_backend_agg_c_ms_mean: 9.0,
            aggregate_backend_ck_1_r_ms_mean: 4.0,
            aggregate_backend_consistency_check_ms_mean: 5.0,
            aggregate_backend_tipa_ab_ms_mean: 20.0,
            aggregate_backend_tipa_c_ms_mean: 25.0,
            aggregate_proof_serialize_ms_mean: 4.0,
            aggregate_bundle_tx_build_ms_mean: 0.0,
            aggregate_spend_ms_mean: 45.0,
            aggregate_output_ms_mean: 35.0,
            aggregate_other_ms_mean: 0.0,
            sidecar_build_ms_mean: 1.0,
            fallback_miss_count: 0,
            local_turn_build_budget_overrun_count: 0,
            guard_miss_count: 0,
            guard_satisfied_turn_ratio: 1.0,
            admission_pool_tx_count_start: 100,
            admission_pool_tx_count_end: 100,
            admission_pool_delta: 0,
            local_synthetic_invalidation_count: 0,
            replaced_total: 0,
            rejected_full_low_fee_total: 0,
            rejected_full_no_evictable_total: 0,
            evicted_nonstaking_total: 0,
            evicted_lowest_staking_total: 0,
            ..Default::default()
        }
    }

    fn sample_row(offered_tps: usize) -> FrontierRawRow {
        FrontierRawRow {
            mode: BuilderMode::Lookahead,
            offered_tps,
            block_interval_ms: 1000,
            max_block_txs: 256,
            segment_tx_count: 32,
            ready_guard_ms: 100,
            sustainable: true,
            result: sample_result(),
        }
    }

    #[test]
    fn backlog_growth_is_allowed_when_every_turn_is_saturated() {
        let mut result = sample_result();
        result.admission_pool_delta = 50;
        result.block_limit_saturated_turn_ratio = 1.0;
        assert!(is_sustainable(&result));
    }

    #[test]
    fn backlog_growth_with_unsaturated_turns_is_not_sustainable() {
        let mut result = sample_result();
        result.admission_pool_delta = 50;
        result.block_limit_saturated_turn_ratio = 0.5;
        assert!(!is_sustainable(&result));
    }

    #[test]
    fn ranking_prefers_sustainable_rows() {
        let sustainable = sample_row(300);
        let mut unsustainable = sample_row(600);
        unsustainable.sustainable = false;
        unsustainable.result.effective_built_tps = 1000.0;

        let rows = vec![unsustainable, sustainable];
        let best = select_best_row(&rows).expect("best row");
        assert!(best.sustainable);
        assert_eq!(best.offered_tps, 300);
    }

    #[test]
    fn per_slice_summary_picks_highest_ranked_row() {
        let mut low = sample_row(300);
        low.result.effective_built_tps = 300.0;
        let mut high = sample_row(600);
        high.result.effective_built_tps = 600.0;

        let summary = summarize_frontier(&[low, high]);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].offered_tps, 600);
    }

    #[test]
    fn no_sustainable_row_falls_back_to_best_unsustainable_row() {
        let mut first = sample_row(300);
        first.sustainable = false;
        first.result.effective_built_tps = 300.0;
        let mut second = sample_row(600);
        second.sustainable = false;
        second.result.effective_built_tps = 600.0;

        let summary = summarize_frontier(&[first, second]);
        assert_eq!(summary.len(), 1);
        assert!(!summary[0].sustainable);
        assert_eq!(summary[0].offered_tps, 600);
    }
}
