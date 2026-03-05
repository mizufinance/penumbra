use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::tps::config::{ScenarioConfig, StabilityConfig, TpsConfig};
use crate::tps::observer::{BlockRecord, CommitRecord, HeightPlan, ObservationOutput};
use crate::tps::sender::SubmissionRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LatencyRecord {
    pub tx_hash_hex: String,
    pub commit_height: u64,
    pub send_elapsed_ms: f64,
    pub commit_observed_elapsed_ms: f64,
    pub latency_ms: f64,
    pub in_steady_window: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SummaryRow {
    pub run_id: String,
    pub label: String,
    pub scenario: String,
    pub offered_tps: u64,
    pub repeat: u32,
    pub sent_tps: f64,
    pub accepted_tps: f64,
    pub committed_tps: f64,
    pub reject_rate_pct: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub steady_commits: u64,
    pub steady_blocks: u64,
    pub warmup_blocks: u64,
    pub block_time_mean_ms: f64,
    pub backlog_start: i64,
    pub backlog_end: i64,
    pub backlog_growth_pct: f64,
    pub stability: String,
    pub run_status: String,
    pub invalid_reason: String,
    pub corpus_required: u64,
    pub corpus_assigned: u64,
    pub corpus_exhausted: bool,
    pub git_rev: String,
    pub host_label: String,
    pub timestamp: u64,
}

pub fn summarize_case(
    cfg: &TpsConfig,
    scenario: &ScenarioConfig,
    offered_tps: u64,
    repeat: u32,
    run_id: &str,
    plan: &HeightPlan,
    submissions: &[SubmissionRecord],
    observation: &ObservationOutput,
) -> (SummaryRow, Vec<LatencyRecord>, Vec<BlockRecord>) {
    let steady_duration_ms =
        (observation.steady_end_elapsed_ms - observation.steady_start_elapsed_ms).max(0.0);
    let steady_duration_s = steady_duration_ms / 1000.0;

    let sent_in_steady = submissions
        .iter()
        .filter(|s| in_steady_by_time(s.sent_elapsed_ms, observation))
        .count();
    let accepted_in_steady = submissions
        .iter()
        .filter(|s| s.async_code == 0 && in_steady_by_time(s.sent_elapsed_ms, observation))
        .count();
    let rejected_in_steady = sent_in_steady.saturating_sub(accepted_in_steady);

    let committed_in_steady = observation
        .commits
        .iter()
        .filter(|c| in_steady_by_height(c.height, plan))
        .count();

    let sent_tps = if steady_duration_s > 0.0 {
        sent_in_steady as f64 / steady_duration_s
    } else {
        0.0
    };
    let accepted_tps = if steady_duration_s > 0.0 {
        accepted_in_steady as f64 / steady_duration_s
    } else {
        0.0
    };
    let committed_tps = if steady_duration_s > 0.0 {
        committed_in_steady as f64 / steady_duration_s
    } else {
        0.0
    };
    let reject_rate_pct = if sent_in_steady == 0 {
        0.0
    } else {
        (rejected_in_steady as f64 / sent_in_steady as f64) * 100.0
    };

    let (latencies, latency_p50, latency_p95, latency_p99) =
        build_latencies(submissions, &observation.commits, plan);

    let block_time_mean_ms = block_time_mean_ms(&observation.blocks, cfg.target_block_time_ms);
    let backlog_start = backlog_at_start(submissions, &observation.commits, observation);
    let backlog_end = backlog_at_end(submissions, &observation.commits, observation);
    let backlog_growth_pct = if backlog_start <= 0 {
        if backlog_end <= 0 {
            0.0
        } else {
            (backlog_end as f64) * 100.0
        }
    } else {
        ((backlog_end - backlog_start) as f64 / backlog_start as f64) * 100.0
    };
    let stability = classify_stability(
        reject_rate_pct,
        latency_p95,
        backlog_growth_pct,
        committed_in_steady as u64,
        &cfg.stability,
    );

    let row = SummaryRow {
        run_id: run_id.to_string(),
        label: cfg.label.clone(),
        scenario: scenario.name.clone(),
        offered_tps,
        repeat,
        sent_tps,
        accepted_tps,
        committed_tps,
        reject_rate_pct,
        p50_ms: latency_p50,
        p95_ms: latency_p95,
        p99_ms: latency_p99,
        steady_commits: committed_in_steady as u64,
        steady_blocks: scenario.steady_blocks,
        warmup_blocks: scenario.warmup_blocks,
        block_time_mean_ms,
        backlog_start,
        backlog_end,
        backlog_growth_pct,
        stability,
        run_status: "ok".to_string(),
        invalid_reason: String::new(),
        corpus_required: 0,
        corpus_assigned: 0,
        corpus_exhausted: false,
        git_rev: git_rev(),
        host_label: host_label(),
        timestamp: unix_ts(),
    };

    (row, latencies, observation.blocks.clone())
}

pub fn classify_stability(
    reject_rate_pct: f64,
    p95_ms: f64,
    backlog_growth_pct: f64,
    steady_commits: u64,
    stability: &StabilityConfig,
) -> String {
    if reject_rate_pct <= stability.max_reject_rate_pct
        && p95_ms <= stability.max_p95_latency_ms
        && backlog_growth_pct <= stability.max_backlog_growth_pct
        && steady_commits >= stability.min_steady_commits
    {
        "pass".to_string()
    } else {
        "fail".to_string()
    }
}

fn build_latencies(
    submissions: &[SubmissionRecord],
    commits: &[CommitRecord],
    plan: &HeightPlan,
) -> (Vec<LatencyRecord>, f64, f64, f64) {
    let mut send_queues: HashMap<String, VecDeque<f64>> = HashMap::new();
    let mut sorted_submissions = submissions.to_vec();
    sorted_submissions.sort_by(|a, b| a.seq.cmp(&b.seq));
    for sub in sorted_submissions {
        if sub.async_code == 0 {
            send_queues
                .entry(sub.tx_hash_hex.clone())
                .or_default()
                .push_back(sub.sent_elapsed_ms);
        }
    }

    let mut sorted_commits = commits.to_vec();
    sorted_commits.sort_by_key(|c| c.height);
    let mut out = Vec::new();
    let mut steady_latencies = Vec::new();

    for commit in sorted_commits {
        if let Some(queue) = send_queues.get_mut(&commit.tx_hash_hex) {
            if let Some(send_elapsed_ms) = queue.pop_front() {
                let latency_ms = (commit.observed_elapsed_ms - send_elapsed_ms).max(0.0);
                let in_steady_window = in_steady_by_height(commit.height, plan);
                if in_steady_window {
                    steady_latencies.push(latency_ms);
                }
                out.push(LatencyRecord {
                    tx_hash_hex: commit.tx_hash_hex,
                    commit_height: commit.height,
                    send_elapsed_ms,
                    commit_observed_elapsed_ms: commit.observed_elapsed_ms,
                    latency_ms,
                    in_steady_window,
                });
            }
        }
    }

    steady_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = percentile(&steady_latencies, 50.0);
    let p95 = percentile(&steady_latencies, 95.0);
    let p99 = percentile(&steady_latencies, 99.0);
    (out, p50, p95, p99)
}

fn percentile(sorted_values: &[f64], pct: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let n = sorted_values.len();
    let idx = (((pct / 100.0) * (n as f64 - 1.0)).round() as usize).min(n - 1);
    sorted_values[idx]
}

fn block_time_mean_ms(blocks: &[BlockRecord], fallback_ms: u64) -> f64 {
    let mut times: Vec<i64> = blocks.iter().filter_map(|b| b.block_time_unix_ms).collect();
    if times.len() < 2 {
        return fallback_ms as f64;
    }
    times.sort_unstable();
    let mut deltas = Vec::with_capacity(times.len() - 1);
    for pair in times.windows(2) {
        let delta = pair[1] - pair[0];
        if delta > 0 {
            deltas.push(delta as f64);
        }
    }
    if deltas.is_empty() {
        fallback_ms as f64
    } else {
        deltas.iter().sum::<f64>() / deltas.len() as f64
    }
}

fn backlog_at_start(
    submissions: &[SubmissionRecord],
    commits: &[CommitRecord],
    observation: &ObservationOutput,
) -> i64 {
    let accepted_before_start = submissions
        .iter()
        .filter(|s| s.async_code == 0 && s.sent_elapsed_ms <= observation.steady_start_elapsed_ms)
        .count() as i64;
    let committed_before_start = commits
        .iter()
        .filter(|c| c.height <= observation.plan.warmup_end_height)
        .count() as i64;
    (accepted_before_start - committed_before_start).max(0)
}

fn backlog_at_end(
    submissions: &[SubmissionRecord],
    commits: &[CommitRecord],
    observation: &ObservationOutput,
) -> i64 {
    let accepted_before_end = submissions
        .iter()
        .filter(|s| s.async_code == 0 && s.sent_elapsed_ms <= observation.steady_end_elapsed_ms)
        .count() as i64;
    let committed_before_end = commits
        .iter()
        .filter(|c| c.height <= observation.plan.end_height)
        .count() as i64;
    (accepted_before_end - committed_before_end).max(0)
}

fn in_steady_by_height(height: u64, plan: &HeightPlan) -> bool {
    height > plan.warmup_end_height && height <= plan.end_height
}

fn in_steady_by_time(elapsed_ms: f64, observation: &ObservationOutput) -> bool {
    elapsed_ms >= observation.steady_start_elapsed_ms
        && elapsed_ms <= observation.steady_end_elapsed_ms
}

fn git_rev() -> String {
    std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string())
}

fn host_label() -> String {
    std::env::var("BENCH_HOST_LABEL")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tps::config::{StabilityConfig, TpsProfile};

    #[test]
    fn stability_classifier_thresholds() {
        let cfg = StabilityConfig {
            max_reject_rate_pct: 1.0,
            max_p95_latency_ms: 100.0,
            max_backlog_growth_pct: 10.0,
            min_steady_commits: 1,
        };
        assert_eq!(classify_stability(1.0, 100.0, 10.0, 1, &cfg), "pass");
        assert_eq!(classify_stability(1.1, 50.0, 5.0, 1, &cfg), "fail");
        assert_eq!(classify_stability(0.0, 50.0, 5.0, 0, &cfg), "fail");
    }

    #[test]
    fn percentile_handles_empty() {
        assert_eq!(percentile(&[], 95.0), 0.0);
    }

    #[test]
    fn block_time_fallback_when_missing_timestamps() {
        let blocks = vec![
            BlockRecord {
                height: 1,
                tx_count: 1,
                observed_elapsed_ms: 1.0,
                block_time_unix_ms: None,
            },
            BlockRecord {
                height: 2,
                tx_count: 1,
                observed_elapsed_ms: 2.0,
                block_time_unix_ms: None,
            },
        ];
        assert_eq!(block_time_mean_ms(&blocks, 6000), 6000.0);
    }

    #[test]
    fn in_steady_height_window_is_correct() {
        let plan = HeightPlan {
            start_height: 10,
            warmup_end_height: 20,
            end_height: 30,
        };
        assert!(!in_steady_by_height(20, &plan));
        assert!(in_steady_by_height(21, &plan));
        assert!(in_steady_by_height(30, &plan));
        assert!(!in_steady_by_height(31, &plan));
    }

    #[test]
    fn type_smoke_for_profile_enum() {
        let _profile = TpsProfile::Regression;
    }
}
