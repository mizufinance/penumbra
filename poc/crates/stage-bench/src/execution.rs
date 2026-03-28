use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use cnidarium::Storage;
use penumbra_sdk_app::app::App;
use penumbra_sdk_app::block_tx_indexing::BlockTxIndexingMode;
use penumbra_sdk_app::app::CandidateEnvelope;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct ExecutionLabConfig {
    pub warmup_blocks: usize,
    pub block_tx_indexing_mode: BlockTxIndexingMode,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExecutionV1Result {
    pub warmup_blocks: usize,
    pub measured_block_count: usize,
    pub executed_blocks_per_sec: f64,
    pub executed_txs_per_sec: f64,
    pub execution_p50_ms: f64,
    pub execution_p95_ms: f64,
    pub execution_p99_ms: f64,
    pub begin_block_ms_mean: f64,
    pub deliver_txs_wall_ms_mean: f64,
    pub end_block_ms_mean: f64,
    pub commit_ms_mean: f64,
    pub execute_tx_ms_mean: f64,
    pub begin_state_tx_ms_mean: f64,
    pub index_tx_ms_mean: f64,
    pub get_block_height_ms_mean: f64,
    pub clone_tx_ms_mean: f64,
    pub proto_convert_ms_mean: f64,
    pub put_block_transaction_ms_mean: f64,
    pub tx_log_read_ms_mean: f64,
    pub tx_log_encode_ms_mean: f64,
    pub tx_log_put_raw_ms_mean: f64,
    pub check_and_execute_ms_mean: f64,
    pub set_source_ms_mean: f64,
    pub pay_fee_ms_mean: f64,
    pub action_execute_ms_mean: f64,
    pub read_local_precheck_ms_mean: f64,
    pub read_lookup_wait_or_join_ms_mean: f64,
    pub read_historical_check_ms_mean: f64,
    pub read_nullifier_wait_ms_mean: f64,
    pub read_anchor_cache_wait_ms_mean: f64,
    pub spend_action_execute_ms_mean: f64,
    pub spend_nullifier_check_ms_mean: f64,
    pub spend_nullifier_tx_local_scan_ms_mean: f64,
    pub spend_nullifier_block_log_lookup_ms_mean: f64,
    pub spend_nullifier_committed_check_ms_mean: f64,
    pub spend_nullifier_enqueue_ms_mean: f64,
    pub spend_nullifier_stage_ms_mean: f64,
    pub spend_nullifier_merge_ms_mean: f64,
    pub nullifier_lookup_count_mean: f64,
    pub output_action_execute_ms_mean: f64,
    pub output_add_note_payload_ms_mean: f64,
    pub other_action_execute_ms_mean: f64,
    pub record_clues_ms_mean: f64,
    pub apply_ms_mean: f64,
    pub block_tx_count_mean: f64,
}

pub fn prepare_scratch_rocksdb(source: &Path, scratch: &Path) -> Result<()> {
    ensure_distinct_paths(source, scratch)?;
    if scratch.exists() {
        fs::remove_dir_all(scratch)
            .with_context(|| format!("removing existing scratch RocksDB {}", scratch.display()))?;
    }
    copy_dir_recursive(source, scratch)
}

pub async fn preflight_execution_v1_corpus(
    envelopes: &[CandidateEnvelope],
    storage: Storage,
    block_tx_indexing_mode: BlockTxIndexingMode,
) -> Result<()> {
    let mut app = App::new(storage.latest_snapshot());
    app.set_block_tx_indexing_mode(block_tx_indexing_mode);

    for (ordinal, envelope) in envelopes.iter().enumerate() {
        app.execute_validated_candidate_envelope_profiled(envelope, storage.clone())
            .await
            .with_context(|| {
                format!(
                    "preflight execution failed for block ordinal {ordinal}; \
                     execution preflight checks sequential block applicability, not proposal admission"
                )
            })?;
    }

    Ok(())
}

#[derive(Default)]
struct ExecutionStats {
    total_txs: usize,
    begin_block_sum: f64,
    deliver_txs_sum: f64,
    end_block_sum: f64,
    commit_sum: f64,
    execute_tx_sum: f64,
    begin_state_tx_sum: f64,
    index_tx_sum: f64,
    get_block_height_sum: f64,
    clone_tx_sum: f64,
    proto_convert_sum: f64,
    put_block_transaction_sum: f64,
    tx_log_read_sum: f64,
    tx_log_encode_sum: f64,
    tx_log_put_raw_sum: f64,
    check_and_execute_sum: f64,
    set_source_sum: f64,
    pay_fee_sum: f64,
    action_execute_sum: f64,
    read_local_precheck_sum: f64,
    read_lookup_wait_or_join_sum: f64,
    read_historical_check_sum: f64,
    read_nullifier_wait_sum: f64,
    read_anchor_cache_wait_sum: f64,
    spend_action_execute_sum: f64,
    spend_nullifier_check_sum: f64,
    spend_nullifier_tx_local_scan_sum: f64,
    spend_nullifier_block_log_lookup_sum: f64,
    spend_nullifier_committed_check_sum: f64,
    spend_nullifier_enqueue_sum: f64,
    spend_nullifier_stage_sum: f64,
    spend_nullifier_merge_sum: f64,
    nullifier_lookup_count_sum: usize,
    output_action_execute_sum: f64,
    output_add_note_payload_sum: f64,
    other_action_execute_sum: f64,
    record_clues_sum: f64,
    apply_sum: f64,
}

impl ExecutionStats {
    fn accumulate(&mut self, p: &penumbra_sdk_app::app::ExecutionBlockProfile) {
        self.total_txs += p.block_tx_count;
        self.begin_block_sum += p.begin_block_ms;
        self.deliver_txs_sum += p.deliver_txs_wall_ms;
        self.end_block_sum += p.end_block_ms;
        self.commit_sum += p.commit_ms;
        self.execute_tx_sum += p.execute_tx_ms;
        self.begin_state_tx_sum += p.begin_state_tx_ms;
        self.index_tx_sum += p.index_tx_ms;
        self.get_block_height_sum += p.get_block_height_ms;
        self.clone_tx_sum += p.clone_tx_ms;
        self.proto_convert_sum += p.proto_convert_ms;
        self.put_block_transaction_sum += p.put_block_transaction_ms;
        self.tx_log_read_sum += p.tx_log_read_ms;
        self.tx_log_encode_sum += p.tx_log_encode_ms;
        self.tx_log_put_raw_sum += p.tx_log_put_raw_ms;
        self.check_and_execute_sum += p.check_and_execute_ms;
        self.set_source_sum += p.set_source_ms;
        self.pay_fee_sum += p.pay_fee_ms;
        self.action_execute_sum += p.action_execute_ms;
        self.read_local_precheck_sum += p.read_local_precheck_ms;
        self.read_lookup_wait_or_join_sum += p.read_lookup_wait_or_join_ms;
        self.read_historical_check_sum += p.read_historical_check_ms;
        self.read_nullifier_wait_sum += p.read_nullifier_wait_ms;
        self.read_anchor_cache_wait_sum += p.read_anchor_cache_wait_ms;
        self.spend_action_execute_sum += p.spend_action_execute_ms;
        self.spend_nullifier_check_sum += p.spend_nullifier_check_ms;
        self.spend_nullifier_tx_local_scan_sum += p.spend_nullifier_tx_local_scan_ms;
        self.spend_nullifier_block_log_lookup_sum += p.spend_nullifier_block_log_lookup_ms;
        self.spend_nullifier_committed_check_sum += p.spend_nullifier_committed_check_ms;
        self.spend_nullifier_enqueue_sum += p.spend_nullifier_enqueue_ms;
        self.spend_nullifier_stage_sum += p.spend_nullifier_stage_ms;
        self.spend_nullifier_merge_sum += p.spend_nullifier_merge_ms;
        self.nullifier_lookup_count_sum += p.nullifier_lookup_count;
        self.output_action_execute_sum += p.output_action_execute_ms;
        self.output_add_note_payload_sum += p.output_add_note_payload_ms;
        self.other_action_execute_sum += p.other_action_execute_ms;
        self.record_clues_sum += p.record_clues_ms;
        self.apply_sum += p.apply_ms;
    }
}

pub async fn run_execution_lab(
    envelopes: &[CandidateEnvelope],
    storage: Storage,
    config: ExecutionLabConfig,
) -> Result<ExecutionV1Result> {
    let mut app = App::new(storage.latest_snapshot());
    app.set_block_tx_indexing_mode(config.block_tx_indexing_mode);
    let warmup_blocks = config.warmup_blocks.min(envelopes.len());
    let mut durations_ms = Vec::with_capacity(envelopes.len().saturating_sub(warmup_blocks));
    let mut measured_wall_ms = 0.0f64;
    let mut measured_block_count = 0usize;
    let mut stats = ExecutionStats::default();

    for (ordinal, envelope) in envelopes.iter().enumerate() {
        let run_started = Instant::now();
        let profile = app
            .execute_validated_candidate_envelope_profiled(envelope, storage.clone())
            .await
            .with_context(|| format!("executing measured block ordinal {ordinal}"))?;
        let run_wall_ms = run_started.elapsed().as_secs_f64() * 1000.0;

        if ordinal < warmup_blocks {
            continue;
        }

        measured_block_count += 1;
        measured_wall_ms += run_wall_ms;
        durations_ms.push(run_wall_ms);
        stats.accumulate(&profile);
    }

    durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(build_execution_result(stats, &durations_ms, warmup_blocks, measured_block_count, measured_wall_ms))
}

fn build_execution_result(
    s: ExecutionStats,
    durations_ms: &[f64],
    warmup_blocks: usize,
    measured_block_count: usize,
    measured_wall_ms: f64,
) -> ExecutionV1Result {
    let total_secs = measured_wall_ms / 1000.0;
    let block_samples = measured_block_count.max(1);
    let tx_samples = s.total_txs.max(1);
    ExecutionV1Result {
        warmup_blocks,
        measured_block_count,
        executed_blocks_per_sec: if total_secs > 0.0 { measured_block_count as f64 / total_secs } else { 0.0 },
        executed_txs_per_sec: if total_secs > 0.0 { s.total_txs as f64 / total_secs } else { 0.0 },
        execution_p50_ms: percentile(durations_ms, 0.50),
        execution_p95_ms: percentile(durations_ms, 0.95),
        execution_p99_ms: percentile(durations_ms, 0.99),
        begin_block_ms_mean: s.begin_block_sum / block_samples as f64,
        deliver_txs_wall_ms_mean: s.deliver_txs_sum / block_samples as f64,
        end_block_ms_mean: s.end_block_sum / block_samples as f64,
        commit_ms_mean: s.commit_sum / block_samples as f64,
        execute_tx_ms_mean: s.execute_tx_sum / tx_samples as f64,
        begin_state_tx_ms_mean: s.begin_state_tx_sum / tx_samples as f64,
        index_tx_ms_mean: s.index_tx_sum / tx_samples as f64,
        get_block_height_ms_mean: s.get_block_height_sum / tx_samples as f64,
        clone_tx_ms_mean: s.clone_tx_sum / tx_samples as f64,
        proto_convert_ms_mean: s.proto_convert_sum / tx_samples as f64,
        put_block_transaction_ms_mean: s.put_block_transaction_sum / tx_samples as f64,
        tx_log_read_ms_mean: s.tx_log_read_sum / tx_samples as f64,
        tx_log_encode_ms_mean: s.tx_log_encode_sum / tx_samples as f64,
        tx_log_put_raw_ms_mean: s.tx_log_put_raw_sum / tx_samples as f64,
        check_and_execute_ms_mean: s.check_and_execute_sum / tx_samples as f64,
        set_source_ms_mean: s.set_source_sum / tx_samples as f64,
        pay_fee_ms_mean: s.pay_fee_sum / tx_samples as f64,
        action_execute_ms_mean: s.action_execute_sum / tx_samples as f64,
        read_local_precheck_ms_mean: s.read_local_precheck_sum / tx_samples as f64,
        read_lookup_wait_or_join_ms_mean: s.read_lookup_wait_or_join_sum / tx_samples as f64,
        read_historical_check_ms_mean: s.read_historical_check_sum / tx_samples as f64,
        read_nullifier_wait_ms_mean: s.read_nullifier_wait_sum / tx_samples as f64,
        read_anchor_cache_wait_ms_mean: s.read_anchor_cache_wait_sum / tx_samples as f64,
        spend_action_execute_ms_mean: s.spend_action_execute_sum / tx_samples as f64,
        spend_nullifier_check_ms_mean: s.spend_nullifier_check_sum / tx_samples as f64,
        spend_nullifier_tx_local_scan_ms_mean: s.spend_nullifier_tx_local_scan_sum / tx_samples as f64,
        spend_nullifier_block_log_lookup_ms_mean: s.spend_nullifier_block_log_lookup_sum / tx_samples as f64,
        spend_nullifier_committed_check_ms_mean: s.spend_nullifier_committed_check_sum / tx_samples as f64,
        spend_nullifier_enqueue_ms_mean: s.spend_nullifier_enqueue_sum / tx_samples as f64,
        spend_nullifier_stage_ms_mean: s.spend_nullifier_stage_sum / tx_samples as f64,
        spend_nullifier_merge_ms_mean: s.spend_nullifier_merge_sum / tx_samples as f64,
        nullifier_lookup_count_mean: s.nullifier_lookup_count_sum as f64 / tx_samples as f64,
        output_action_execute_ms_mean: s.output_action_execute_sum / tx_samples as f64,
        output_add_note_payload_ms_mean: s.output_add_note_payload_sum / tx_samples as f64,
        other_action_execute_ms_mean: s.other_action_execute_sum / tx_samples as f64,
        record_clues_ms_mean: s.record_clues_sum / tx_samples as f64,
        apply_ms_mean: s.apply_sum / tx_samples as f64,
        block_tx_count_mean: s.total_txs as f64 / block_samples as f64,
    }
}

fn ensure_distinct_paths(source: &Path, scratch: &Path) -> Result<()> {
    let source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    let scratch = scratch
        .canonicalize()
        .unwrap_or_else(|_| scratch.to_path_buf());
    if source == scratch {
        bail!(
            "scratch RocksDB path must differ from source RocksDB path: {}",
            source.display()
        );
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, scratch: &Path) -> Result<()> {
    let metadata = fs::metadata(source)
        .with_context(|| format!("reading source RocksDB directory {}", source.display()))?;
    if !metadata.is_dir() {
        bail!("source RocksDB path is not a directory: {}", source.display());
    }

    fs::create_dir_all(scratch)
        .with_context(|| format!("creating scratch RocksDB directory {}", scratch.display()))?;

    for entry in fs::read_dir(source)
        .with_context(|| format!("reading source RocksDB directory {}", source.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = scratch.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "copying RocksDB file {} -> {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        } else {
            bail!(
                "unsupported RocksDB entry type while copying {}",
                src_path.display()
            );
        }
    }

    Ok(())
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[index.min(values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;
    use std::sync::Arc;

    use anyhow::{anyhow, Result};
    use cnidarium::TempStorage;
    use penumbra_sdk_app::app::MAX_BLOCK_TXS_PAYLOAD_BYTES;
    use penumbra_sdk_app::genesis::{AppState, Content};
    use penumbra_sdk_app::server::consensus::Consensus;
    use penumbra_sdk_app::SUBSTORE_PREFIXES;
    use penumbra_sdk_asset::STAKING_TOKEN_DENOM;
    use penumbra_sdk_keys::test_keys;
    use penumbra_sdk_mock_client::MockClient;
    use penumbra_sdk_mock_consensus::TestNode;
    use penumbra_sdk_shielded_pool::{genesis::Allocation, OutputPlan, SpendPlan};
    use penumbra_sdk_transaction::{
        memo::MemoPlaintext, plan::MemoPlan, TransactionParameters, TransactionPlan,
    };
    use rand_core::OsRng;
    use tempfile::tempdir;

    use super::*;
    use crate::validation::{
        default_validation_builder_config, generate_prebuilt_validation_corpus,
        load_validation_corpus, ValidationCorpus, ValidationGenerationMode,
    };

    async fn setup_test_txs(tx_count: usize) -> Result<(TempStorage, ValidationCorpus)> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;

        let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
            raw_amount: 1_000_000u128.into(),
            raw_denom: STAKING_TOKEN_DENOM.deref().base_denom().denom,
            address: test_keys::ADDRESS_0.to_owned(),
        })
        .take(tx_count)
        .collect();

        let app_state_bytes = serde_json::to_vec(&AppState::Content(Content {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
                allocations,
                ..Default::default()
            },
            ..Default::default()
        }))?;

        let consensus = Consensus::new(storage.as_ref().clone());
        let initial_time = tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?;
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

        let notes: Vec<_> = client.notes.values().cloned().take(tx_count).collect();
        let mut txs = Vec::with_capacity(tx_count);
        for note in notes {
            let mut plan = TransactionPlan {
                actions: vec![
                    SpendPlan::new(
                        &mut OsRng,
                        note.clone(),
                        client
                            .position(note.commit())
                            .ok_or_else(|| anyhow!("note position was unknown to mock client"))?,
                    )
                    .into(),
                    OutputPlan::new(
                        &mut OsRng,
                        note.value(),
                        test_keys::ADDRESS_1.deref().clone(),
                    )
                    .into(),
                ],
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
                .witness_auth_build_with_compliance(&mut plan, storage.latest_snapshot())
                .await?;
            txs.push(Arc::new(tx));
        }

        let corpus_dir = tempdir()?;
        let mut config = default_validation_builder_config();
        config.generation_mode = ValidationGenerationMode::OneShot;
        config.max_block_txs = tx_count;
        config.segment_tx_count = tx_count.max(1);
        config.source_builder_label = "execution_v1_test".to_string();
        generate_prebuilt_validation_corpus(
            txs.clone(),
            storage.latest_snapshot(),
            corpus_dir.path(),
            config,
        )
        .await?;
        let corpus = load_validation_corpus(corpus_dir.path())?;

        Ok((storage, corpus))
    }

    #[tokio::test]
    async fn execution_only_path_replays_without_proposal_validation() -> Result<()> {
        let (storage, corpus) = setup_test_txs(1).await?;
        let mut app = App::new(storage.latest_snapshot());
        let mut envelope = corpus.envelopes.first().expect("test envelope").clone();
        envelope.aggregate_bundle_tx_bytes = None;
        envelope.segment_tx_counts.clear();

        let profile = app
            .execute_validated_candidate_envelope_profiled(&envelope, storage.as_ref().clone())
            .await?;
        assert_eq!(profile.block_tx_count, 1);
        assert!(profile.deliver_txs_wall_ms > 0.0);

        Ok(())
    }

    #[tokio::test]
    async fn preflight_rejects_stale_execution_corpus() -> Result<()> {
        let (storage, corpus) = setup_test_txs(1).await?;
        let duplicated = vec![
            corpus.envelopes.first().expect("first envelope").clone(),
            corpus.envelopes.first().expect("first envelope").clone(),
        ];

        let result = preflight_execution_v1_corpus(
            &duplicated,
            storage.as_ref().clone(),
            BlockTxIndexingMode::DeferredBatch,
        )
        .await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn preflight_ignores_process_proposal_payload_limits() -> Result<()> {
        let (storage, corpus) = setup_test_txs(1).await?;
        let mut envelope = corpus.envelopes.first().expect("test envelope").clone();
        envelope.total_payload_bytes = MAX_BLOCK_TXS_PAYLOAD_BYTES + 1;

        preflight_execution_v1_corpus(
            &[envelope],
            storage.as_ref().clone(),
            BlockTxIndexingMode::DeferredBatch,
        )
        .await?;

        Ok(())
    }

    #[test]
    fn scratch_path_must_differ_from_source() {
        let dir = tempdir().expect("tempdir");
        let result = prepare_scratch_rocksdb(dir.path(), dir.path());
        assert!(result.is_err());
    }
}
