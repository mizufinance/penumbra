use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use penumbra_sdk_bench::lookahead_builder::{
    build_admitted_transactions, run_builder_one_shot, BuilderOneShotConfig,
};
use penumbra_sdk_bench::mempool::SyntheticFeeMode;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_poc_preconsensus::local_mempool::FeeEvictionPolicy;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_transaction::Transaction;
use serde::Serialize;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Parser)]
#[clap(name = "builder_one_shot_lab")]
#[clap(about = "Build one exact candidate from the first admissible prefix and record build cost")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long, default_value_t = 1000)]
    max_block_txs: usize,
    #[clap(long, default_value_t = 512)]
    segment_tx_count: usize,
    #[clap(long, default_value_t = 64_000_000)]
    max_proposal_bytes: usize,
    #[clap(long, default_value_t = 268_435_456)]
    max_store_bytes: usize,
    #[clap(long, default_value_t = 40_000)]
    max_store_txs: usize,
    #[clap(long, default_value = "off")]
    synthetic_fee_spread: String,
    #[clap(long, default_value = "disabled")]
    fee_eviction_policy: String,
    #[clap(long, default_value_t = 0)]
    warmup_runs: usize,
    #[clap(long, default_value_t = 1)]
    measured_runs: usize,
    #[clap(long, default_value_t = 0)]
    rayon_threads_per_batch: usize,
    #[clap(long)]
    print_header: bool,
}

#[derive(Serialize)]
struct SummaryRow {
    run_id: String,
    corpus: String,
    max_block_txs: usize,
    segment_tx_count: usize,
    rayon_threads_per_batch: usize,
    max_proposal_bytes: usize,
    max_store_bytes: usize,
    max_store_txs: usize,
    synthetic_fee_mode: String,
    fee_eviction_policy: String,
    warmup_runs: usize,
    measured_runs: usize,
    selected_tx_count: usize,
    selected_payload_bytes: usize,
    segment_count: usize,
    build_wall_ms: f64,
    aggregate_total_ms: f64,
    aggregate_merge_items_ms: f64,
    aggregate_setup_ms: f64,
    aggregate_padding_ms: f64,
    aggregate_collect_proofs_ms: f64,
    aggregate_backend_core_ms: f64,
    aggregate_backend_point_extract_ms: f64,
    aggregate_backend_prepared_srs_ms: f64,
    aggregate_backend_commitment_key_extract_ms: f64,
    aggregate_backend_commitment_ms: f64,
    aggregate_backend_com_a_ms: f64,
    aggregate_backend_com_b_ms: f64,
    aggregate_backend_com_c_ms: f64,
    aggregate_backend_pairing_normalize_batch_ms: f64,
    aggregate_backend_pairing_prepare_ms: f64,
    aggregate_backend_pairing_miller_loop_ms: f64,
    aggregate_backend_pairing_final_exponentiation_ms: f64,
    aggregate_backend_randomizer_ms: f64,
    aggregate_backend_structured_scalar_ms: f64,
    aggregate_backend_weighted_a_ms: f64,
    aggregate_backend_ip_ab_ms: f64,
    aggregate_backend_agg_c_ms: f64,
    aggregate_backend_ck_1_r_ms: f64,
    aggregate_backend_consistency_check_ms: f64,
    aggregate_backend_tipa_ab_ms: f64,
    aggregate_backend_tipa_c_ms: f64,
    aggregate_backend_tipa_ab_gipa_ms: f64,
    aggregate_backend_tipa_ab_gipa_commit_l_ms: f64,
    aggregate_backend_tipa_ab_gipa_commit_r_ms: f64,
    aggregate_backend_tipa_ab_gipa_challenge_ms: f64,
    aggregate_backend_tipa_ab_gipa_rescale_m1_ms: f64,
    aggregate_backend_tipa_ab_gipa_rescale_m2_ms: f64,
    aggregate_backend_tipa_ab_gipa_rescale_ck1_ms: f64,
    aggregate_backend_tipa_ab_gipa_rescale_ck2_ms: f64,
    aggregate_backend_tipa_ab_transcript_inverse_ms: f64,
    aggregate_backend_tipa_ab_kzg_challenge_ms: f64,
    aggregate_backend_tipa_ab_kzg_coefficient_build_ms: f64,
    aggregate_backend_tipa_ab_kzg_eval_quotient_ms: f64,
    aggregate_backend_tipa_ab_kzg_opening_msm_ms: f64,
    aggregate_backend_tipa_ab_kzg_opening_ck_a_ms: f64,
    aggregate_backend_tipa_ab_kzg_opening_ck_b_ms: f64,
    aggregate_backend_tipa_c_gipa_ms: f64,
    aggregate_backend_tipa_c_gipa_commit_l_ms: f64,
    aggregate_backend_tipa_c_gipa_commit_r_ms: f64,
    aggregate_backend_tipa_c_gipa_challenge_ms: f64,
    aggregate_backend_tipa_c_gipa_rescale_m1_ms: f64,
    aggregate_backend_tipa_c_gipa_rescale_m2_ms: f64,
    aggregate_backend_tipa_c_gipa_rescale_ck1_ms: f64,
    aggregate_backend_tipa_c_gipa_rescale_ck2_ms: f64,
    aggregate_backend_tipa_c_transcript_inverse_ms: f64,
    aggregate_backend_tipa_c_kzg_challenge_ms: f64,
    aggregate_backend_tipa_c_kzg_coefficient_build_ms: f64,
    aggregate_backend_tipa_c_kzg_eval_quotient_ms: f64,
    aggregate_backend_tipa_c_kzg_opening_msm_ms: f64,
    aggregate_backend_tipa_c_kzg_opening_ck_a_ms: f64,
    aggregate_proof_serialize_ms: f64,
    aggregate_bundle_tx_build_ms: f64,
    aggregate_spend_ms: f64,
    aggregate_output_ms: f64,
    aggregate_other_ms: f64,
    aggregate_verify_passed: bool,
    aggregate_verify_ms: f64,
    sidecar_build_ms: f64,
    replaced_total: u64,
    rejected_full_low_fee_total: u64,
    rejected_full_no_evictable_total: u64,
    evicted_nonstaking_total: u64,
    evicted_lowest_staking_total: u64,
    git_rev: String,
    host_label: String,
    timestamp: u64,
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    let synthetic_fee_mode: SyntheticFeeMode = cli.synthetic_fee_spread.parse()?;
    let fee_eviction_policy = parse_fee_eviction_policy(&cli.fee_eviction_policy)?;
    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;
    let decoded = corpus
        .entries
        .into_iter()
        .map(|entry| {
            Transaction::decode(entry.tx_bytes.as_slice())
                .map(|tx| (Arc::new(entry.tx_bytes), Arc::new(tx)))
                .with_context(|| format!("decoding tx ordinal {}", entry.ordinal))
        })
        .collect::<Result<Vec<_>>>()?;
    let admitted = build_admitted_transactions(decoded, 512, synthetic_fee_mode).await?;
    let total_runs = cli.warmup_runs + cli.measured_runs;
    anyhow::ensure!(total_runs > 0, "warmup_runs + measured_runs must be > 0");
    anyhow::ensure!(cli.measured_runs > 0, "measured_runs must be > 0");
    let build_cfg = BuilderOneShotConfig {
        max_block_txs: cli.max_block_txs,
        segment_tx_count: cli.segment_tx_count,
        max_proposal_bytes: cli.max_proposal_bytes,
        max_store_bytes: cli.max_store_bytes,
        max_store_txs: cli.max_store_txs,
        fee_eviction_policy,
        rayon_threads_per_batch: cli.rayon_threads_per_batch,
    };
    let mut measured = Vec::with_capacity(cli.measured_runs);
    for run_idx in 0..total_runs {
        let result = run_builder_one_shot(admitted.clone(), build_cfg).await?;
        if run_idx >= cli.warmup_runs {
            measured.push(result);
        }
    }
    let result = average_results(&measured);

    let row = SummaryRow {
        run_id: format!("run-{}-{}", unix_ts(), std::process::id()),
        corpus: cli.corpus.display().to_string(),
        max_block_txs: cli.max_block_txs,
        segment_tx_count: cli.segment_tx_count,
        rayon_threads_per_batch: cli.rayon_threads_per_batch,
        max_proposal_bytes: cli.max_proposal_bytes,
        max_store_bytes: cli.max_store_bytes,
        max_store_txs: cli.max_store_txs,
        synthetic_fee_mode: synthetic_fee_mode.as_str().to_string(),
        fee_eviction_policy: cli.fee_eviction_policy.clone(),
        warmup_runs: cli.warmup_runs,
        measured_runs: cli.measured_runs,
        selected_tx_count: result.selected_tx_count,
        selected_payload_bytes: result.selected_payload_bytes,
        segment_count: result.segment_count,
        build_wall_ms: result.build_wall_ms,
        aggregate_total_ms: result.aggregate_total_ms,
        aggregate_merge_items_ms: result.aggregate_profile.merge_items_ms,
        aggregate_setup_ms: result.aggregate_profile.setup_ms,
        aggregate_padding_ms: result.aggregate_profile.padding_ms,
        aggregate_collect_proofs_ms: result.aggregate_profile.collect_proofs_ms,
        aggregate_backend_core_ms: result.aggregate_profile.backend_core_ms,
        aggregate_backend_point_extract_ms: result.aggregate_profile.backend_point_extract_ms,
        aggregate_backend_prepared_srs_ms: result.aggregate_profile.backend_prepared_srs_ms,
        aggregate_backend_commitment_key_extract_ms: result
            .aggregate_profile
            .backend_commitment_key_extract_ms,
        aggregate_backend_commitment_ms: result.aggregate_profile.backend_commitment_ms,
        aggregate_backend_com_a_ms: result.aggregate_profile.backend_com_a_ms,
        aggregate_backend_com_b_ms: result.aggregate_profile.backend_com_b_ms,
        aggregate_backend_com_c_ms: result.aggregate_profile.backend_com_c_ms,
        aggregate_backend_pairing_normalize_batch_ms: result
            .aggregate_profile
            .backend_pairing_normalize_batch_ms,
        aggregate_backend_pairing_prepare_ms: result.aggregate_profile.backend_pairing_prepare_ms,
        aggregate_backend_pairing_miller_loop_ms: result
            .aggregate_profile
            .backend_pairing_miller_loop_ms,
        aggregate_backend_pairing_final_exponentiation_ms: result
            .aggregate_profile
            .backend_pairing_final_exponentiation_ms,
        aggregate_backend_randomizer_ms: result.aggregate_profile.backend_randomizer_ms,
        aggregate_backend_structured_scalar_ms: result
            .aggregate_profile
            .backend_structured_scalar_ms,
        aggregate_backend_weighted_a_ms: result.aggregate_profile.backend_weighted_a_ms,
        aggregate_backend_ip_ab_ms: result.aggregate_profile.backend_ip_ab_ms,
        aggregate_backend_agg_c_ms: result.aggregate_profile.backend_agg_c_ms,
        aggregate_backend_ck_1_r_ms: result.aggregate_profile.backend_ck_1_r_ms,
        aggregate_backend_consistency_check_ms: result
            .aggregate_profile
            .backend_consistency_check_ms,
        aggregate_backend_tipa_ab_ms: result.aggregate_profile.backend_tipa_ab_ms,
        aggregate_backend_tipa_c_ms: result.aggregate_profile.backend_tipa_c_ms,
        aggregate_backend_tipa_ab_gipa_ms: result.aggregate_profile.backend_tipa_ab_gipa_ms,
        aggregate_backend_tipa_ab_gipa_commit_l_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_commit_l_ms,
        aggregate_backend_tipa_ab_gipa_commit_r_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_commit_r_ms,
        aggregate_backend_tipa_ab_gipa_challenge_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_challenge_ms,
        aggregate_backend_tipa_ab_gipa_rescale_m1_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_rescale_m1_ms,
        aggregate_backend_tipa_ab_gipa_rescale_m2_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_rescale_m2_ms,
        aggregate_backend_tipa_ab_gipa_rescale_ck1_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_rescale_ck1_ms,
        aggregate_backend_tipa_ab_gipa_rescale_ck2_ms: result
            .aggregate_profile
            .backend_tipa_ab_gipa_rescale_ck2_ms,
        aggregate_backend_tipa_ab_transcript_inverse_ms: result
            .aggregate_profile
            .backend_tipa_ab_transcript_inverse_ms,
        aggregate_backend_tipa_ab_kzg_challenge_ms: result
            .aggregate_profile
            .backend_tipa_ab_kzg_challenge_ms,
        aggregate_backend_tipa_ab_kzg_coefficient_build_ms: result
            .aggregate_profile
            .backend_tipa_ab_kzg_coefficient_build_ms,
        aggregate_backend_tipa_ab_kzg_eval_quotient_ms: result
            .aggregate_profile
            .backend_tipa_ab_kzg_eval_quotient_ms,
        aggregate_backend_tipa_ab_kzg_opening_msm_ms: result
            .aggregate_profile
            .backend_tipa_ab_kzg_opening_msm_ms,
        aggregate_backend_tipa_ab_kzg_opening_ck_a_ms: result
            .aggregate_profile
            .backend_tipa_ab_kzg_opening_ck_a_ms,
        aggregate_backend_tipa_ab_kzg_opening_ck_b_ms: result
            .aggregate_profile
            .backend_tipa_ab_kzg_opening_ck_b_ms,
        aggregate_backend_tipa_c_gipa_ms: result.aggregate_profile.backend_tipa_c_gipa_ms,
        aggregate_backend_tipa_c_gipa_commit_l_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_commit_l_ms,
        aggregate_backend_tipa_c_gipa_commit_r_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_commit_r_ms,
        aggregate_backend_tipa_c_gipa_challenge_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_challenge_ms,
        aggregate_backend_tipa_c_gipa_rescale_m1_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_rescale_m1_ms,
        aggregate_backend_tipa_c_gipa_rescale_m2_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_rescale_m2_ms,
        aggregate_backend_tipa_c_gipa_rescale_ck1_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_rescale_ck1_ms,
        aggregate_backend_tipa_c_gipa_rescale_ck2_ms: result
            .aggregate_profile
            .backend_tipa_c_gipa_rescale_ck2_ms,
        aggregate_backend_tipa_c_transcript_inverse_ms: result
            .aggregate_profile
            .backend_tipa_c_transcript_inverse_ms,
        aggregate_backend_tipa_c_kzg_challenge_ms: result
            .aggregate_profile
            .backend_tipa_c_kzg_challenge_ms,
        aggregate_backend_tipa_c_kzg_coefficient_build_ms: result
            .aggregate_profile
            .backend_tipa_c_kzg_coefficient_build_ms,
        aggregate_backend_tipa_c_kzg_eval_quotient_ms: result
            .aggregate_profile
            .backend_tipa_c_kzg_eval_quotient_ms,
        aggregate_backend_tipa_c_kzg_opening_msm_ms: result
            .aggregate_profile
            .backend_tipa_c_kzg_opening_msm_ms,
        aggregate_backend_tipa_c_kzg_opening_ck_a_ms: result
            .aggregate_profile
            .backend_tipa_c_kzg_opening_ck_a_ms,
        aggregate_proof_serialize_ms: result.aggregate_profile.proof_serialize_ms,
        aggregate_bundle_tx_build_ms: result.aggregate_profile.bundle_tx_build_ms,
        aggregate_spend_ms: result.aggregate_profile.spend_ms,
        aggregate_output_ms: result.aggregate_profile.output_ms,
        aggregate_other_ms: result.aggregate_profile.other_ms,
        aggregate_verify_passed: result.aggregate_verify_passed,
        aggregate_verify_ms: result.aggregate_verify_ms,
        sidecar_build_ms: result.sidecar_build_ms,
        replaced_total: result.replaced_total,
        rejected_full_low_fee_total: result.rejected_full_low_fee_total,
        rejected_full_no_evictable_total: result.rejected_full_no_evictable_total,
        evicted_nonstaking_total: result.evicted_nonstaking_total,
        evicted_lowest_staking_total: result.evicted_lowest_staking_total,
        git_rev: std::env::var("BENCH_GIT_REV").unwrap_or_else(|_| "unknown-rev".to_string()),
        host_label: std::env::var("BENCH_HOST_LABEL")
            .or_else(|_| std::env::var("HOSTNAME"))
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown-host".to_string()),
        timestamp: unix_ts(),
    };

    let mut writer = csv::WriterBuilder::new()
        .has_headers(cli.print_header)
        .from_writer(std::io::stdout());
    writer.serialize(&row)?;
    writer.flush()?;
    Ok(())
}

fn average_results(
    results: &[penumbra_sdk_bench::lookahead_builder::BuilderOneShotResult],
) -> penumbra_sdk_bench::lookahead_builder::BuilderOneShotResult {
    assert!(
        !results.is_empty(),
        "at least one measured builder result is required"
    );
    if results.len() == 1 {
        return results[0].clone();
    }
    let sample_count = results.len() as f64;
    let first = &results[0];
    let mut aggregate_profile = first.aggregate_profile.clone();
    let mut build_wall_ms = first.build_wall_ms;
    let mut aggregate_total_ms = first.aggregate_total_ms;
    let mut aggregate_verify_ms = first.aggregate_verify_ms;
    let mut aggregate_verify_passed = first.aggregate_verify_passed;
    let mut sidecar_build_ms = first.sidecar_build_ms;
    let mut replaced_total = first.replaced_total;
    let mut rejected_full_low_fee_total = first.rejected_full_low_fee_total;
    let mut rejected_full_no_evictable_total = first.rejected_full_no_evictable_total;
    let mut evicted_nonstaking_total = first.evicted_nonstaking_total;
    let mut evicted_lowest_staking_total = first.evicted_lowest_staking_total;
    for result in &results[1..] {
        build_wall_ms += result.build_wall_ms;
        aggregate_total_ms += result.aggregate_total_ms;
        aggregate_verify_ms += result.aggregate_verify_ms;
        aggregate_verify_passed &= result.aggregate_verify_passed;
        sidecar_build_ms += result.sidecar_build_ms;
        replaced_total += result.replaced_total;
        rejected_full_low_fee_total += result.rejected_full_low_fee_total;
        rejected_full_no_evictable_total += result.rejected_full_no_evictable_total;
        evicted_nonstaking_total += result.evicted_nonstaking_total;
        evicted_lowest_staking_total += result.evicted_lowest_staking_total;
        aggregate_profile.merge(&result.aggregate_profile);
    }
    build_wall_ms /= sample_count;
    aggregate_total_ms /= sample_count;
    aggregate_verify_ms /= sample_count;
    sidecar_build_ms /= sample_count;
    aggregate_profile.scale(1.0 / sample_count);
    penumbra_sdk_bench::lookahead_builder::BuilderOneShotResult {
        selected_tx_count: first.selected_tx_count,
        selected_payload_bytes: first.selected_payload_bytes,
        segment_count: first.segment_count,
        build_wall_ms,
        aggregate_total_ms,
        aggregate_profile,
        aggregate_verify_passed,
        aggregate_verify_ms,
        sidecar_build_ms,
        replaced_total,
        rejected_full_low_fee_total,
        rejected_full_no_evictable_total,
        evicted_nonstaking_total,
        evicted_lowest_staking_total,
    }
}

fn parse_fee_eviction_policy(value: &str) -> Result<FeeEvictionPolicy> {
    match value {
        "disabled" => Ok(FeeEvictionPolicy::Disabled),
        "launch-staking-priority" => Ok(FeeEvictionPolicy::LaunchStakingPriority),
        other => anyhow::bail!("unknown fee eviction policy: {other}"),
    }
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
