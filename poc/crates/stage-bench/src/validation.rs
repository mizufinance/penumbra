use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use cnidarium::Snapshot;
use penumbra_sdk_app::app::App;
use penumbra_sdk_app::app::ProposalArtifactSidecar;
use penumbra_sdk_app::app::{
    candidate_digest_from_hashes, CandidateEnvelope, ValidationNullifierCache,
    ValidationRejectReason,
};
use penumbra_sdk_poc_preconsensus::local_mempool::{
    FeeEvictionPolicy, MempoolCoreConfig, MempoolHandle,
};
use penumbra_sdk_proof_aggregation::set_unchecked_aggregate_deserialization_for_bench;
use penumbra_sdk_proto::core::transaction::v1::action::Action as ProtoAction;
use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;
use penumbra_sdk_proto::{DomainType, Message as _};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_transaction::Transaction;
use serde::{Deserialize, Serialize};

use crate::lookahead_builder::{
    admit_transactions_at_rate, build_admitted_transactions_no_bytes,
    build_ready_candidate_from_frozen, drain_pending_build, is_local_proposer_turn,
    next_local_turn, BuilderMode, LookaheadBuilder, LookaheadBuilderConfig, ReadyCandidate,
};
use crate::mempool::SyntheticFeeMode;

const CORPUS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct ValidationCorpusBuildConfig {
    pub generation_mode: ValidationGenerationMode,
    pub offered_tps: usize,
    pub block_interval_ms: u64,
    pub num_validators: usize,
    pub proposer_index: usize,
    pub max_block_txs: usize,
    pub segment_tx_count: usize,
    pub warmup_local_turns: usize,
    pub steady_local_turns: usize,
    pub max_proposal_bytes: usize,
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub synthetic_fee_mode: SyntheticFeeMode,
    pub fee_eviction_policy: FeeEvictionPolicy,
    pub source_builder_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationGenerationMode {
    Cadence,
    OneShot,
}

#[derive(Clone, Debug)]
pub struct ValidationLabConfig {
    pub with_local_cache: bool,
    pub unchecked_aggregate_deserialization: bool,
    pub warmup_blocks: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ValidationV1Result {
    pub warmup_blocks: usize,
    pub measured_block_count: usize,
    pub validated_blocks_per_sec: f64,
    pub validated_txs_per_sec: f64,
    pub validation_p50_ms: f64,
    pub validation_p95_ms: f64,
    pub validation_p99_ms: f64,
    pub shape_check_ms_mean: f64,
    pub sidecar_check_ms_mean: f64,
    pub nullifier_cache_lookup_ms_mean: f64,
    pub nullifier_extract_ms_mean: f64,
    pub nullifier_cache_hit_ratio: f64,
    pub stateful_conflict_check_ms_mean: f64,
    pub aggregate_verify_ms_mean: f64,
    pub aggregate_artifact_cache_lookup_ms_mean: f64,
    pub aggregate_tx_decode_ms_mean: f64,
    pub aggregate_sidecar_decode_ms_mean: f64,
    pub aggregate_expected_segments_ms_mean: f64,
    pub aggregate_prepare_inputs_ms_mean: f64,
    pub aggregate_verify_kernel_ms_mean: f64,
    pub aggregate_backend_deserialize_ms_mean: f64,
    pub aggregate_backend_challenge_ms_mean: f64,
    pub aggregate_backend_tipa_ab_ms_mean: f64,
    pub aggregate_backend_tipa_c_ms_mean: f64,
    pub aggregate_backend_public_input_fold_ms_mean: f64,
    pub aggregate_backend_ppe_ms_mean: f64,
    pub aggregate_backend_core_total_ms_mean: f64,
    pub aggregate_artifact_cache_hit_ratio: f64,
    pub accept_total: usize,
    pub reject_total: usize,
    pub aggregate_verify_fail_total: usize,
    pub committed_nullifier_conflict_total: usize,
    pub duplicate_spend_nullifier_total: usize,
    pub sidecar_reject_total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationCorpusManifest {
    pub schema_version: u32,
    pub created_at: u64,
    pub source_builder_label: String,
    pub block_count: usize,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationCorpusIndexEntry {
    pub ordinal: usize,
    pub file_name: String,
    pub block_tx_count: usize,
    pub total_payload_bytes: usize,
    pub candidate_digest_hex: String,
}

#[derive(Clone, Debug)]
pub struct ValidationCorpus {
    pub manifest: ValidationCorpusManifest,
    pub envelopes: Vec<CandidateEnvelope>,
}

pub async fn generate_prebuilt_validation_corpus(
    txs: Vec<Arc<Transaction>>,
    snapshot: Snapshot,
    out_dir: &Path,
    config: ValidationCorpusBuildConfig,
) -> Result<usize> {
    match config.generation_mode {
        ValidationGenerationMode::Cadence => {
            generate_prebuilt_validation_corpus_cadence(txs, snapshot, out_dir, config).await
        }
        ValidationGenerationMode::OneShot => {
            generate_prebuilt_validation_corpus_one_shot(txs, snapshot, out_dir, config).await
        }
    }
}

async fn generate_prebuilt_validation_corpus_cadence(
    txs: Vec<Arc<Transaction>>,
    snapshot: Snapshot,
    out_dir: &Path,
    config: ValidationCorpusBuildConfig,
) -> Result<usize> {
    let admitted =
        build_admitted_transactions_no_bytes(txs, 512, config.synthetic_fee_mode).await?;
    let builder = LookaheadBuilder::new(LookaheadBuilderConfig {
        mode: BuilderMode::Lookahead,
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

    let mut next_turn_at = started_at;
    let mut local_turns_seen = 0usize;
    let mut envelopes = Vec::new();

    for height in 1u64.. {
        builder
            .maybe_schedule(next_local_turn(
                height,
                config.num_validators,
                config.proposer_index,
            ))
            .await?;

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
        let (_outcome, candidate) = builder
            .take_turn_candidate_materialized(
                height,
                (config.block_interval_ms * config.num_validators as u64) as f64,
                0.0,
                turn_started_at,
            )
            .await?;
        if local_turns_seen > config.warmup_local_turns {
            if let Some(candidate) = candidate {
                envelopes.push(
                    candidate_envelope_from_ready(
                        &candidate,
                        snapshot.clone(),
                        &config.source_builder_label,
                    )
                    .await?,
                );
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
    write_validation_corpus(
        out_dir,
        &ValidationCorpusManifest {
            schema_version: CORPUS_SCHEMA_VERSION,
            created_at: unix_ts(),
            source_builder_label: config.source_builder_label,
            block_count: envelopes.len(),
            notes: "validation_v1_strict".to_string(),
        },
        &envelopes,
    )?;
    Ok(envelopes.len())
}

async fn generate_prebuilt_validation_corpus_one_shot(
    txs: Vec<Arc<Transaction>>,
    snapshot: Snapshot,
    out_dir: &Path,
    config: ValidationCorpusBuildConfig,
) -> Result<usize> {
    let admitted =
        build_admitted_transactions_no_bytes(txs, 512, config.synthetic_fee_mode).await?;
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
            "one-shot mempool admission rejected input"
        );
    }

    let frozen = mempool
        .freeze_next_candidate(1, config.max_block_txs, config.max_proposal_bytes)
        .await?
        .context("one-shot validation corpus generation produced no frozen candidate")?;
    let ready = build_ready_candidate_from_frozen(frozen, config.segment_tx_count).await?;
    let envelope =
        candidate_envelope_from_ready(&ready, snapshot, &config.source_builder_label).await?;

    write_validation_corpus(
        out_dir,
        &ValidationCorpusManifest {
            schema_version: CORPUS_SCHEMA_VERSION,
            created_at: unix_ts(),
            source_builder_label: config.source_builder_label,
            block_count: 1,
            notes: format!("validation_v1_one_shot_target_{}", config.max_block_txs),
        },
        &[envelope],
    )?;
    Ok(1)
}

pub fn write_validation_corpus(
    out_dir: &Path,
    manifest: &ValidationCorpusManifest,
    envelopes: &[CandidateEnvelope],
) -> Result<()> {
    fs::create_dir_all(out_dir.join("blocks"))
        .with_context(|| format!("creating {}", out_dir.display()))?;

    let index = envelopes
        .iter()
        .enumerate()
        .map(|(ordinal, envelope)| ValidationCorpusIndexEntry {
            ordinal,
            file_name: format!("block-{ordinal:05}.json"),
            block_tx_count: envelope.block_tx_count,
            total_payload_bytes: envelope.total_payload_bytes,
            candidate_digest_hex: hex::encode(envelope.candidate_digest),
        })
        .collect::<Vec<_>>();

    fs::write(
        out_dir.join("manifest.json"),
        serde_json::to_vec_pretty(manifest)?,
    )
    .with_context(|| format!("writing {}", out_dir.join("manifest.json").display()))?;

    let mut writer = csv::Writer::from_path(out_dir.join("index.csv"))
        .with_context(|| format!("writing {}", out_dir.join("index.csv").display()))?;
    for row in &index {
        writer.serialize(row)?;
    }
    writer.flush()?;

    for row in &index {
        let bytes = serde_json::to_vec_pretty(&envelopes[row.ordinal])?;
        fs::write(out_dir.join("blocks").join(&row.file_name), bytes).with_context(|| {
            format!(
                "writing {}",
                out_dir.join("blocks").join(&row.file_name).display()
            )
        })?;
    }

    Ok(())
}

pub fn load_validation_corpus(corpus_dir: &Path) -> Result<ValidationCorpus> {
    let manifest: ValidationCorpusManifest = serde_json::from_slice(
        &fs::read(corpus_dir.join("manifest.json"))
            .with_context(|| format!("reading {}", corpus_dir.join("manifest.json").display()))?,
    )
    .with_context(|| format!("decoding {}", corpus_dir.join("manifest.json").display()))?;

    let mut reader = csv::Reader::from_path(corpus_dir.join("index.csv"))
        .with_context(|| format!("reading {}", corpus_dir.join("index.csv").display()))?;
    let mut envelopes = Vec::new();
    for row in reader.deserialize::<ValidationCorpusIndexEntry>() {
        let row = row?;
        let bytes =
            fs::read(corpus_dir.join("blocks").join(&row.file_name)).with_context(|| {
                format!(
                    "reading {}",
                    corpus_dir.join("blocks").join(&row.file_name).display()
                )
            })?;
        let envelope: CandidateEnvelope = serde_json::from_slice(&bytes)
            .with_context(|| format!("decoding block record {}", row.file_name))?;
        envelopes.push(envelope);
    }

    Ok(ValidationCorpus {
        manifest,
        envelopes,
    })
}

#[derive(Default)]
struct ValidationStats {
    total_txs: usize,
    shape_check_sum: f64,
    sidecar_check_sum: f64,
    nullifier_cache_lookup_sum: f64,
    nullifier_extract_sum: f64,
    stateful_conflict_check_sum: f64,
    aggregate_verify_sum: f64,
    aggregate_artifact_cache_lookup_sum: f64,
    aggregate_tx_decode_sum: f64,
    aggregate_sidecar_decode_sum: f64,
    aggregate_expected_segments_sum: f64,
    aggregate_prepare_inputs_sum: f64,
    aggregate_verify_kernel_sum: f64,
    aggregate_backend_deserialize_sum: f64,
    aggregate_backend_challenge_sum: f64,
    aggregate_backend_tipa_ab_sum: f64,
    aggregate_backend_tipa_c_sum: f64,
    aggregate_backend_public_input_fold_sum: f64,
    aggregate_backend_ppe_sum: f64,
    aggregate_backend_core_total_sum: f64,
    cache_hits: usize,
    cache_misses: usize,
    aggregate_artifact_cache_hits: usize,
    aggregate_artifact_cache_misses: usize,
    accept_total: usize,
    reject_total: usize,
    aggregate_verify_fail_total: usize,
    committed_nullifier_conflict_total: usize,
    duplicate_spend_nullifier_total: usize,
    sidecar_reject_total: usize,
}

impl ValidationStats {
    fn accumulate(&mut self, profile: &penumbra_sdk_app::app::ValidationProfile, block_tx_count: usize) {
        self.total_txs += block_tx_count;
        self.shape_check_sum += profile.shape_check_ms;
        self.sidecar_check_sum += profile.sidecar_check_ms;
        self.nullifier_cache_lookup_sum += profile.nullifier_cache_lookup_ms;
        self.nullifier_extract_sum += profile.nullifier_extract_ms;
        self.stateful_conflict_check_sum += profile.stateful_conflict_check_ms;
        self.aggregate_verify_sum += profile.aggregate_verify_ms;
        self.aggregate_artifact_cache_lookup_sum += profile.aggregate_artifact_cache_lookup_ms;
        self.aggregate_tx_decode_sum += profile.aggregate_tx_decode_ms;
        self.aggregate_sidecar_decode_sum += profile.aggregate_sidecar_decode_ms;
        self.aggregate_expected_segments_sum += profile.aggregate_expected_segments_ms;
        self.aggregate_prepare_inputs_sum += profile.aggregate_prepare_inputs_ms;
        self.aggregate_verify_kernel_sum += profile.aggregate_verify_kernel_ms;
        self.aggregate_backend_deserialize_sum += profile.aggregate_backend_deserialize_ms;
        self.aggregate_backend_challenge_sum += profile.aggregate_backend_challenge_ms;
        self.aggregate_backend_tipa_ab_sum += profile.aggregate_backend_tipa_ab_ms;
        self.aggregate_backend_tipa_c_sum += profile.aggregate_backend_tipa_c_ms;
        self.aggregate_backend_public_input_fold_sum += profile.aggregate_backend_public_input_fold_ms;
        self.aggregate_backend_ppe_sum += profile.aggregate_backend_ppe_ms;
        self.aggregate_backend_core_total_sum += profile.aggregate_backend_core_total_ms;
        self.cache_hits += profile.cache_hit_count;
        self.cache_misses += profile.cache_miss_count;
        self.aggregate_artifact_cache_hits += profile.aggregate_artifact_cache_hit_count;
        self.aggregate_artifact_cache_misses += profile.aggregate_artifact_cache_miss_count;
    }
}

pub async fn run_validation_lab(
    envelopes: &[CandidateEnvelope],
    snapshot: Snapshot,
    config: ValidationLabConfig,
) -> Result<ValidationV1Result> {
    set_unchecked_aggregate_deserialization_for_bench(config.unchecked_aggregate_deserialization);
    let app = App::new(snapshot);
    let cache = if config.with_local_cache {
        Some(build_local_nullifier_cache(envelopes)?)
    } else {
        None
    };
    let cache_ref = cache.as_ref();

    let mut durations_ms = Vec::with_capacity(envelopes.len());
    let warmup_blocks = config.warmup_blocks.min(envelopes.len());
    let mut measured_wall_ms = 0.0f64;
    let mut measured_block_count = 0usize;
    let mut stats = ValidationStats::default();

    for (ordinal, envelope) in envelopes.iter().enumerate() {
        let run_started = Instant::now();
        let (verdict, profile) = app
            .validate_candidate_envelope_profiled(envelope, cache_ref)
            .await?;
        let run_wall_ms = run_started.elapsed().as_secs_f64() * 1000.0;

        if ordinal < warmup_blocks {
            continue;
        }

        measured_block_count += 1;
        measured_wall_ms += run_wall_ms;
        durations_ms.push(run_wall_ms);
        stats.accumulate(&profile, envelope.block_tx_count);

        if verdict.final_accept {
            stats.accept_total += 1;
        } else {
            stats.reject_total += 1;
        }
        match verdict.reject_reason {
            Some(ValidationRejectReason::AggregateVerifyFailed) => {
                stats.aggregate_verify_fail_total += 1;
            }
            Some(ValidationRejectReason::CommittedNullifierConflict) => {
                stats.committed_nullifier_conflict_total += 1;
            }
            Some(ValidationRejectReason::DuplicateSpendNullifier) => {
                stats.duplicate_spend_nullifier_total += 1;
            }
            Some(
                ValidationRejectReason::SidecarTxCountMismatch
                | ValidationRejectReason::SidecarCommitmentMismatch
                | ValidationRejectReason::SidecarDecodeFailed
                | ValidationRejectReason::SidecarEntrySetMismatch
                | ValidationRejectReason::BundleMissing
                | ValidationRejectReason::BundleTxShapeInvalid,
            ) => {
                stats.sidecar_reject_total += 1;
            }
            _ => {}
        }
    }

    durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let result = build_validation_result(stats, &durations_ms, warmup_blocks, measured_block_count, measured_wall_ms);
    set_unchecked_aggregate_deserialization_for_bench(false);
    Ok(result)
}

fn build_validation_result(
    s: ValidationStats,
    durations_ms: &[f64],
    warmup_blocks: usize,
    measured_block_count: usize,
    measured_wall_ms: f64,
) -> ValidationV1Result {
    let total_secs = measured_wall_ms / 1000.0;
    let sample_count = durations_ms.len().max(1);
    ValidationV1Result {
        warmup_blocks,
        measured_block_count,
        validated_blocks_per_sec: if total_secs > 0.0 { measured_block_count as f64 / total_secs } else { 0.0 },
        validated_txs_per_sec: if total_secs > 0.0 { s.total_txs as f64 / total_secs } else { 0.0 },
        validation_p50_ms: percentile(durations_ms, 0.50),
        validation_p95_ms: percentile(durations_ms, 0.95),
        validation_p99_ms: percentile(durations_ms, 0.99),
        shape_check_ms_mean: mean(s.shape_check_sum, sample_count),
        sidecar_check_ms_mean: mean(s.sidecar_check_sum, sample_count),
        nullifier_cache_lookup_ms_mean: mean(s.nullifier_cache_lookup_sum, sample_count),
        nullifier_extract_ms_mean: mean(s.nullifier_extract_sum, sample_count),
        nullifier_cache_hit_ratio: ratio(s.cache_hits, s.cache_hits + s.cache_misses),
        stateful_conflict_check_ms_mean: mean(s.stateful_conflict_check_sum, sample_count),
        aggregate_verify_ms_mean: mean(s.aggregate_verify_sum, sample_count),
        aggregate_artifact_cache_lookup_ms_mean: mean(s.aggregate_artifact_cache_lookup_sum, sample_count),
        aggregate_tx_decode_ms_mean: mean(s.aggregate_tx_decode_sum, sample_count),
        aggregate_sidecar_decode_ms_mean: mean(s.aggregate_sidecar_decode_sum, sample_count),
        aggregate_expected_segments_ms_mean: mean(s.aggregate_expected_segments_sum, sample_count),
        aggregate_prepare_inputs_ms_mean: mean(s.aggregate_prepare_inputs_sum, sample_count),
        aggregate_verify_kernel_ms_mean: mean(s.aggregate_verify_kernel_sum, sample_count),
        aggregate_backend_deserialize_ms_mean: mean(s.aggregate_backend_deserialize_sum, sample_count),
        aggregate_backend_challenge_ms_mean: mean(s.aggregate_backend_challenge_sum, sample_count),
        aggregate_backend_tipa_ab_ms_mean: mean(s.aggregate_backend_tipa_ab_sum, sample_count),
        aggregate_backend_tipa_c_ms_mean: mean(s.aggregate_backend_tipa_c_sum, sample_count),
        aggregate_backend_public_input_fold_ms_mean: mean(s.aggregate_backend_public_input_fold_sum, sample_count),
        aggregate_backend_ppe_ms_mean: mean(s.aggregate_backend_ppe_sum, sample_count),
        aggregate_backend_core_total_ms_mean: mean(s.aggregate_backend_core_total_sum, sample_count),
        aggregate_artifact_cache_hit_ratio: ratio(s.aggregate_artifact_cache_hits, s.aggregate_artifact_cache_hits + s.aggregate_artifact_cache_misses),
        accept_total: s.accept_total,
        reject_total: s.reject_total,
        aggregate_verify_fail_total: s.aggregate_verify_fail_total,
        committed_nullifier_conflict_total: s.committed_nullifier_conflict_total,
        duplicate_spend_nullifier_total: s.duplicate_spend_nullifier_total,
        sidecar_reject_total: s.sidecar_reject_total,
    }
}

pub fn default_validation_builder_config() -> ValidationCorpusBuildConfig {
    ValidationCorpusBuildConfig {
        generation_mode: ValidationGenerationMode::Cadence,
        offered_tps: 2048,
        block_interval_ms: 1000,
        num_validators: 4,
        proposer_index: 0,
        max_block_txs: 2048,
        segment_tx_count: 200,
        warmup_local_turns: 1,
        steady_local_turns: 8,
        max_proposal_bytes: 7_000_000,
        max_store_bytes: 256 * 1024 * 1024,
        max_store_txs: 40_000,
        synthetic_fee_mode: SyntheticFeeMode::DeterministicHashV1,
        fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        source_builder_label: "validation_v1_strict".to_string(),
    }
}

async fn candidate_envelope_from_ready(
    ready: &ReadyCandidate,
    snapshot: Snapshot,
    source_builder_label: &str,
) -> Result<CandidateEnvelope> {
    let txs = ready
        .frozen
        .records
        .iter()
        .map(|record| record.tx_bytes.as_ref().clone())
        .collect::<Vec<_>>();
    let tx_hashes = ready
        .frozen
        .records
        .iter()
        .map(|record| record.tx_hash)
        .collect::<Vec<_>>();
    let total_payload_bytes = txs.iter().map(Vec::len).sum::<usize>();
    let aggregate_bundle_tx =
        App::build_aggregate_bundle_tx_for_snapshot_public(snapshot, ready.bundle.clone()).await?;
    Ok(CandidateEnvelope {
        txs,
        tx_hashes: tx_hashes.clone(),
        aggregate_bundle_tx_bytes: Some(aggregate_bundle_tx.encode_to_vec()),
        sidecar: ready.sidecar.to_record(),
        segment_tx_counts: ready.segment_tx_counts.clone(),
        block_tx_count: tx_hashes.len(),
        total_payload_bytes,
        candidate_digest: candidate_digest_from_hashes(&tx_hashes),
        source_builder_label: source_builder_label.to_string(),
    })
}

fn build_local_nullifier_cache(
    envelopes: &[CandidateEnvelope],
) -> Result<ValidationNullifierCache> {
    let mut cache = ValidationNullifierCache::new();
    for envelope in envelopes {
        let sidecar = ProposalArtifactSidecar::from_record(envelope.sidecar.clone());
        let sidecar_entries = envelope
            .sidecar
            .entries
            .iter()
            .map(|entry| (entry.tx_hash, entry.encoded_entry.as_slice()))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (tx_hash, tx_bytes) in envelope.tx_hashes.iter().zip(envelope.txs.iter()) {
            if cache.get(tx_hash).is_some() {
                continue;
            }
            let tx = Transaction::decode(tx_bytes.as_slice()).with_context(|| {
                format!(
                    "decoding tx for validation cache build: {}",
                    hex::encode(tx_hash)
                )
            })?;
            let proto_tx = ProtoTransaction::decode(tx_bytes.as_slice())
                .context("decoding tx for validation cache build")?;
            let nullifiers = proto_spend_nullifiers(&proto_tx)?;
            let encoded_entry = sidecar_entries.get(tx_hash).copied().with_context(|| {
                format!(
                    "missing sidecar entry in cache build: {}",
                    hex::encode(tx_hash)
                )
            })?;
            let artifact = sidecar
                .decode_artifact(*tx_hash, Arc::new(tx), encoded_entry)
                .with_context(|| {
                    format!(
                        "decoding sidecar artifact for cache build: {}",
                        hex::encode(tx_hash)
                    )
                })?;
            cache.insert(*tx_hash, nullifiers, Some(artifact));
        }
    }
    Ok(cache)
}

fn proto_spend_nullifiers(proto_tx: &ProtoTransaction) -> Result<Vec<Nullifier>> {
    let mut spend_nullifiers = Vec::new();
    for action in proto_tx
        .body
        .as_ref()
        .into_iter()
        .flat_map(|body| body.actions.iter())
    {
        let Some(ProtoAction::Spend(spend)) = &action.action else {
            continue;
        };
        let Some(body) = &spend.body else {
            continue;
        };
        let Some(nullifier) = &body.nullifier else {
            continue;
        };
        spend_nullifiers.push(
            Nullifier::try_from(nullifier.clone()).context("converting cached proto nullifier")?,
        );
    }
    Ok(spend_nullifiers)
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

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[index.min(values.len() - 1)]
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
