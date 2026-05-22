use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use cnidarium::{Snapshot, StateRead, StateWrite};
use cnidarium_component::ActionHandler as _;
use penumbra_sdk_compact_block::StatePayload;
use penumbra_sdk_compliance::registry::{check_timestamp_freshness, ComplianceRegistryRead as _};
use penumbra_sdk_fee::component::FeePay as _;
use penumbra_sdk_sct::component::clock::EpochRead;
use penumbra_sdk_sct::component::source::SourceContext;
use penumbra_sdk_sct::component::tree::VerificationExt as _;
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::component::{ClueManager, Ics20Transfer, StateReadExt as _};
use penumbra_sdk_shielded_pool::fmd;
use penumbra_sdk_tct::StateCommitment;
use penumbra_sdk_transaction::{gas::GasCost as _, Action, Transaction};
use penumbra_sdk_txhash::TransactionId;
use tokio::sync::OnceCell;
use tokio::task::JoinSet;
use tracing::{instrument, Instrument};

use super::AppActionHandler;
use crate::{app::StateReadExt as _, PenumbraHost};

mod stateful;
pub(crate) mod stateless;

use self::stateful::{
    claimed_anchor_is_valid, fmd_parameters_valid_with_context,
    tx_parameters_historical_check_with_context,
};
use stateless::{
    check_memo_exists_if_outputs_absent_if_not, check_non_empty_transaction,
    num_clues_equal_to_num_outputs, valid_binding_signature,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TransactionExecutionProfile {
    pub set_source_ms: f64,
    pub pay_fee_ms: f64,
    pub action_execute_ms: f64,
    pub read_local_precheck_ms: f64,
    pub read_lookup_wait_or_join_ms: f64,
    pub read_historical_check_ms: f64,
    pub read_nullifier_wait_ms: f64,
    pub read_anchor_cache_wait_ms: f64,
    pub read_anchor_validation_ms: f64,
    pub read_committed_nullifier_ms: f64,
    pub read_effects_build_ms: f64,
    pub nullifier_lookup_count: usize,
    pub spend_action_execute_ms: f64,
    pub spend_nullifier_check_ms: f64,
    pub spend_nullifier_tx_local_scan_ms: f64,
    pub spend_nullifier_block_log_lookup_ms: f64,
    pub spend_nullifier_committed_check_ms: f64,
    pub spend_nullifier_enqueue_ms: f64,
    pub spend_nullifier_stage_ms: f64,
    pub spend_nullifier_merge_ms: f64,
    pub output_action_execute_ms: f64,
    pub output_add_note_payload_ms: f64,
    pub other_action_execute_ms: f64,
    pub record_clues_ms: f64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PreparedCandidateEffects {
    pub spend_nullifiers: Vec<Nullifier>,
    pub sct_payloads: Vec<StatePayload>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PreparedCandidateRead {
    pub check_historical_ms: f64,
    pub read_wall_ms: f64,
    pub checktx_fast_context_load_ms: f64,
    pub checktx_fast_read_queue_wait_ms: f64,
    pub checktx_fast_read_blocking_total_ms: f64,
    pub execution_profile: TransactionExecutionProfile,
    pub effects: PreparedCandidateEffects,
}

type AnchorPair = (StateCommitment, StateCommitment);
type ClaimedAnchorKey = penumbra_sdk_tct::Root;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HistoricalCheckProfile {
    pub total_ms: f64,
    pub await_ms: f64,
}

#[derive(Debug, Default)]
pub(crate) struct AnchorValidationCache {
    entries: RwLock<HashMap<AnchorPair, Arc<OnceCell<std::result::Result<(), String>>>>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl AnchorValidationCache {
    fn entry(
        &self,
        pair: AnchorPair,
    ) -> (Arc<OnceCell<std::result::Result<(), String>>>, bool, f64) {
        let read_wait_start = Instant::now();
        let existing = self
            .entries
            .read()
            .expect("anchor cache poisoned")
            .get(&pair)
            .cloned();
        let mut wait_ms = read_wait_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(cell) = existing {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return (cell, true, wait_ms);
        }

        let write_wait_start = Instant::now();
        let mut entries = self.entries.write().expect("anchor cache poisoned");
        wait_ms += write_wait_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(cell) = entries.get(&pair).cloned() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return (cell, true, wait_ms);
        }

        let cell = Arc::new(OnceCell::new());
        entries.insert(pair, cell.clone());
        self.misses.fetch_add(1, Ordering::Relaxed);
        (cell, false, wait_ms)
    }

    pub(crate) fn stats(&self) -> (usize, usize, usize) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.entries.read().expect("anchor cache poisoned").len(),
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct ClaimedAnchorValidationCache {
    entries: RwLock<HashMap<ClaimedAnchorKey, Arc<OnceCell<std::result::Result<(), String>>>>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl ClaimedAnchorValidationCache {
    fn entry(
        &self,
        anchor: ClaimedAnchorKey,
    ) -> (Arc<OnceCell<std::result::Result<(), String>>>, bool) {
        if let Some(cell) = self
            .entries
            .read()
            .expect("claimed anchor cache poisoned")
            .get(&anchor)
            .cloned()
        {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return (cell, true);
        }

        let mut entries = self.entries.write().expect("claimed anchor cache poisoned");
        if let Some(cell) = entries.get(&anchor).cloned() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return (cell, true);
        }

        let cell = Arc::new(OnceCell::new());
        entries.insert(anchor, cell.clone());
        self.misses.fetch_add(1, Ordering::Relaxed);
        (cell, false)
    }

    pub(crate) fn stats(&self) -> (usize, usize, usize) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.entries
                .read()
                .expect("claimed anchor cache poisoned")
                .len(),
        )
    }
}

#[derive(Clone, Debug)]
struct TxExecutionContext {
    block_timestamp: u64,
    source: TransactionId,
}

#[derive(Clone, Debug)]
pub(crate) struct HistoricalCheckContext {
    pub chain_id: String,
    pub block_height: u64,
    pub block_timestamp: u64,
    pub fmd_meta_params: fmd::MetaParameters,
    pub previous_fmd_parameters: fmd::Parameters,
    pub current_fmd_parameters: fmd::Parameters,
    pub anchor_cache: Arc<AnchorValidationCache>,
    pub claimed_anchor_cache: Arc<ClaimedAnchorValidationCache>,
}

impl HistoricalCheckContext {
    pub(crate) async fn load<S: StateRead>(state: &S) -> Result<Self> {
        Self::load_inner(state).await
    }

    pub(crate) async fn load_for_checktx<S: StateRead>(state: &S) -> Result<Self> {
        Self::load_inner(state).await
    }

    async fn load_inner<S: StateRead>(state: &S) -> Result<Self> {
        let shielded_pool_params = state
            .get_shielded_pool_params()
            .await
            .expect("chain params request must succeed");

        Ok(Self {
            chain_id: state.get_chain_id().await?,
            block_height: state.get_block_height().await?,
            block_timestamp: state.get_current_block_timestamp().await?.unix_timestamp() as u64,
            fmd_meta_params: shielded_pool_params.fmd_meta_params,
            previous_fmd_parameters: state
                .get_previous_fmd_parameters()
                .await
                .expect("chain params request must succeed"),
            current_fmd_parameters: state
                .get_current_fmd_parameters()
                .await
                .expect("chain params request must succeed"),
            anchor_cache: Arc::new(AnchorValidationCache::default()),
            claimed_anchor_cache: Arc::new(ClaimedAnchorValidationCache::default()),
        })
    }
}

async fn check_nullifier_read_only<S>(
    state: &S,
    _context: &HistoricalCheckContext,
    nullifier: penumbra_sdk_sct::Nullifier,
) -> Result<f64>
where
    S: StateRead,
{
    let committed_check_start = Instant::now();
    state.check_nullifier_unspent(nullifier).await?;
    Ok(committed_check_start.elapsed().as_secs_f64() * 1000.0)
}

async fn validate_compliance_anchors_read_only<S: StateRead>(
    state: &S,
    user_anchor: &StateCommitment,
    asset_anchor: &StateCommitment,
    block_height: u64,
    anchor_cache: Arc<AnchorValidationCache>,
) -> Result<(f64, f64)> {
    let anchor_pair = (*user_anchor, *asset_anchor);
    let validate_start = Instant::now();
    let (cell, _, cache_wait_ms) = anchor_cache.entry(anchor_pair);

    let result = cell
        .get_or_init(|| async move {
            let user_anchor_height = state
                .check_user_anchor(user_anchor)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "invalid user compliance anchor: not found in history".to_string())?;
            if block_height
                > user_anchor_height + penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS
            {
                return Err(format!(
                    "user compliance anchor too old: height {} is more than {} blocks behind current height {}",
                    user_anchor_height,
                    penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS,
                    block_height
                ));
            }

            let asset_anchor_height = state
                .check_asset_anchor(asset_anchor)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "invalid asset compliance anchor: not found in history".to_string())?;
            if block_height
                > asset_anchor_height + penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS
            {
                return Err(format!(
                    "asset compliance anchor too old: height {} is more than {} blocks behind current height {}",
                    asset_anchor_height,
                    penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS,
                    block_height
                ));
            }

            Ok(())
        })
        .await;

    match result {
        Ok(()) => Ok((
            validate_start.elapsed().as_secs_f64() * 1000.0,
            cache_wait_ms,
        )),
        Err(error) => anyhow::bail!(error.clone()),
    }
}

async fn validate_claimed_anchor_read_only<S: StateRead>(
    state: Arc<S>,
    tx: Arc<Transaction>,
    claimed_anchor_cache: Arc<ClaimedAnchorValidationCache>,
) -> Result<f64> {
    let anchor = tx.anchor;
    let wait_start = Instant::now();
    let (cell, _) = claimed_anchor_cache.entry(anchor);
    let result = cell
        .get_or_init(|| async move {
            claimed_anchor_is_valid(state, Arc::as_ref(&tx))
                .await
                .map_err(|e| e.to_string())
        })
        .await;
    let elapsed_ms = wait_start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(()) => Ok(elapsed_ms),
        Err(error) => anyhow::bail!(error.clone()),
    }
}

pub(crate) fn supports_parallel_prepare(tx: &Transaction) -> bool {
    tx.actions().all(|a| matches!(a, Action::Transfer(_)))
}

fn action_requires_historical_check(action: &Action) -> bool {
    matches!(
        action,
        Action::IbcRelay(_) | Action::ShieldedIcs20Withdrawal(_)
    )
}

fn check_nullifier_read_only_sync(
    handle: &tokio::runtime::Handle,
    snapshot: &Snapshot,
    _context: &HistoricalCheckContext,
    nullifier: penumbra_sdk_sct::Nullifier,
) -> Result<f64> {
    let committed_check_start = Instant::now();
    handle.block_on(snapshot.check_nullifier_unspent(nullifier))?;
    Ok(committed_check_start.elapsed().as_secs_f64() * 1000.0)
}

fn validate_compliance_anchors_read_only_sync(
    handle: &tokio::runtime::Handle,
    snapshot: &Snapshot,
    user_anchor: &StateCommitment,
    asset_anchor: &StateCommitment,
    block_height: u64,
    anchor_cache: Arc<AnchorValidationCache>,
) -> Result<(f64, f64)> {
    let anchor_pair = (*user_anchor, *asset_anchor);
    let validate_start = Instant::now();
    let (cell, _, cache_wait_ms) = anchor_cache.entry(anchor_pair);
    let snapshot = snapshot.clone();
    let user_anchor = *user_anchor;
    let asset_anchor = *asset_anchor;

    let result = handle
        .clone()
        .block_on(cell.get_or_init(|| async move {
            let user_anchor_height = snapshot
                .get_raw(&penumbra_sdk_compliance::state_key::anchor::user_anchor_lookup(
                    &user_anchor,
                ))
                .await
                .map_err(|e| e.to_string())?
                .map(|bytes| {
                    <u64 as penumbra_sdk_proto::Message>::decode(bytes.as_slice())
                        .map_err(|e| anyhow::anyhow!(e).to_string())
                })
                .transpose()?
                .ok_or_else(|| "invalid user compliance anchor: not found in history".to_string())?;
            if block_height
                > user_anchor_height + penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS
            {
                return Err(format!(
                    "user compliance anchor too old: height {} is more than {} blocks behind current height {}",
                    user_anchor_height,
                    penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS,
                    block_height
                ));
            }

            let asset_anchor_height = snapshot
                .get_raw(&penumbra_sdk_compliance::state_key::anchor::asset_anchor_lookup(
                    &asset_anchor,
                ))
                .await
                .map_err(|e| e.to_string())?
                .map(|bytes| {
                    <u64 as penumbra_sdk_proto::Message>::decode(bytes.as_slice())
                        .map_err(|e| anyhow::anyhow!(e).to_string())
                })
                .transpose()?
                .ok_or_else(|| "invalid asset compliance anchor: not found in history".to_string())?;
            if block_height
                > asset_anchor_height + penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS
            {
                return Err(format!(
                    "asset compliance anchor too old: height {} is more than {} blocks behind current height {}",
                    asset_anchor_height,
                    penumbra_sdk_compliance::registry::MAX_ANCHOR_AGE_BLOCKS,
                    block_height
                ));
            }

            Ok(())
        }))
        .clone();

    match result {
        Ok(()) => Ok((
            validate_start.elapsed().as_secs_f64() * 1000.0,
            cache_wait_ms,
        )),
        Err(error) => anyhow::bail!(error),
    }
}

fn validate_claimed_anchor_read_only_sync(
    handle: &tokio::runtime::Handle,
    snapshot: &Snapshot,
    tx: &Transaction,
    claimed_anchor_cache: Arc<ClaimedAnchorValidationCache>,
) -> Result<f64> {
    let anchor = tx.anchor;
    let wait_start = Instant::now();
    let (cell, _) = claimed_anchor_cache.entry(anchor);
    let snapshot = snapshot.clone();

    let result = handle
        .clone()
        .block_on(cell.get_or_init(|| async move {
            if anchor.is_empty() {
                return Ok(());
            }
            if snapshot
                .get_raw(&penumbra_sdk_sct::state_key::tree::anchor_lookup(anchor))
                .await
                .map_err(|e| e.to_string())?
                .map(|bytes| {
                    <u64 as penumbra_sdk_proto::Message>::decode(bytes.as_slice())
                        .map_err(|e| anyhow::anyhow!(e).to_string())
                })
                .transpose()?
                .is_some()
            {
                Ok(())
            } else {
                Err(format!(
                    "provided anchor {} is not a valid SCT root",
                    anchor
                ))
            }
        }))
        .clone();
    let elapsed_ms = wait_start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(()) => Ok(elapsed_ms),
        Err(error) => anyhow::bail!(error),
    }
}

pub(crate) async fn check_historical_with_context_profiled<S: StateRead + 'static>(
    tx: &Transaction,
    state: Arc<S>,
    context: &HistoricalCheckContext,
) -> Result<HistoricalCheckProfile> {
    let total_start = Instant::now();
    let mut await_ms = 0.0;
    let mut action_checks = JoinSet::new();

    tx_parameters_historical_check_with_context(tx, context)?;
    fmd_parameters_valid_with_context(tx, context)?;

    let claimed_anchor_tx = Arc::new(tx.clone());
    let claimed_anchor_wait_start = Instant::now();
    validate_claimed_anchor_read_only(
        state.clone(),
        claimed_anchor_tx,
        context.claimed_anchor_cache.clone(),
    )
    .await?;
    await_ms += claimed_anchor_wait_start.elapsed().as_secs_f64() * 1000.0;

    for (i, action) in tx.actions().cloned().enumerate() {
        if !action_requires_historical_check(&action) {
            continue;
        }

        let state2 = state.clone();
        let span = action.create_span(i);
        action_checks.spawn(async move { action.check_historical(state2).await }.instrument(span));
    }

    while !action_checks.is_empty() {
        let join_wait_start = Instant::now();
        let check = action_checks
            .join_next()
            .await
            .expect("join set must yield while not empty");
        await_ms += join_wait_start.elapsed().as_secs_f64() * 1000.0;
        check??;
    }

    Ok(HistoricalCheckProfile {
        total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
        await_ms,
    })
}

pub(crate) async fn check_historical_with_context<S: StateRead + 'static>(
    tx: &Transaction,
    state: Arc<S>,
    context: &HistoricalCheckContext,
) -> Result<()> {
    check_historical_with_context_profiled(tx, state, context)
        .await
        .map(|_| ())
}

pub(crate) fn check_historical_with_context_sync_profiled(
    tx: &Transaction,
    snapshot: &Snapshot,
    context: &HistoricalCheckContext,
    handle: &tokio::runtime::Handle,
) -> Result<HistoricalCheckProfile> {
    let total_start = Instant::now();

    tx_parameters_historical_check_with_context(tx, context)?;
    fmd_parameters_valid_with_context(tx, context)?;

    let await_ms = validate_claimed_anchor_read_only_sync(
        handle,
        snapshot,
        tx,
        context.claimed_anchor_cache.clone(),
    )?;

    Ok(HistoricalCheckProfile {
        total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
        await_ms,
    })
}

pub(crate) async fn check_and_execute_profiled<S>(
    tx: &Transaction,
    mut state: S,
    record_clues: bool,
) -> Result<TransactionExecutionProfile>
where
    S: StateWrite,
{
    let mut profile = TransactionExecutionProfile::default();
    let tx_id = tx.id();
    let action_spans_enabled = tracing::enabled!(tracing::Level::INFO);

    let set_source_start = Instant::now();
    state.put_current_source(Some(tx_id.clone()));
    profile.set_source_ms = set_source_start.elapsed().as_secs_f64() * 1000.0;

    let pay_fee_start = Instant::now();
    let gas_used = tx.gas_cost();
    let fee = tx.transaction_body.transaction_parameters.fee;
    state.pay_fee(gas_used, fee).await?;
    profile.pay_fee_ms = pay_fee_start.elapsed().as_secs_f64() * 1000.0;

    let action_execute_start = Instant::now();
    for (i, action) in tx.actions().enumerate() {
        let action_start = Instant::now();
        match action {
            Action::IbcRelay(action) => {
                let relay = action.clone().with_handler::<Ics20Transfer, PenumbraHost>();
                let execute = relay.check_and_execute(&mut state);
                if action_spans_enabled {
                    let span = Action::IbcRelay(action.clone()).create_span(i);
                    execute.instrument(span).await?;
                } else {
                    execute.await?;
                }
                profile.other_action_execute_ms += action_start.elapsed().as_secs_f64() * 1000.0;
            }
            _ => {
                if action_spans_enabled {
                    let span = action.create_span(i);
                    action
                        .check_and_execute(&mut state)
                        .instrument(span)
                        .await?;
                } else {
                    action.check_and_execute(&mut state).await?;
                }
                profile.other_action_execute_ms += action_start.elapsed().as_secs_f64() * 1000.0;
            }
        }
    }
    if let Some(fee_funding) = &tx.transaction_body.fee_funding {
        let action_start = Instant::now();
        fee_funding.transfer.check_and_execute(&mut state).await?;
        profile.other_action_execute_ms += action_start.elapsed().as_secs_f64() * 1000.0;
    }
    profile.action_execute_ms = action_execute_start.elapsed().as_secs_f64() * 1000.0;

    if record_clues {
        let record_clues_start = Instant::now();
        state.put_current_source(None);
        for clue in tx
            .transaction_body
            .detection_data
            .iter()
            .flat_map(|x| x.fmd_clues.iter())
        {
            state.record_clue(clue.clone(), tx_id.clone()).await?;
        }
        profile.record_clues_ms = record_clues_start.elapsed().as_secs_f64() * 1000.0;
    }

    Ok(profile)
}

pub(crate) async fn prepare_candidate_read_profiled<S: StateRead + 'static>(
    tx: Arc<Transaction>,
    state: Arc<S>,
    context: HistoricalCheckContext,
    skip_historical: bool,
) -> Result<PreparedCandidateRead> {
    let read_start = Instant::now();
    let mut prepared = PreparedCandidateRead::default();

    let execution_context = TxExecutionContext {
        block_timestamp: context.block_timestamp,
        source: tx.id(),
    };
    let mut anchor_pairs = BTreeSet::new();
    let mut output_payloads = Vec::new();
    let mut spend_nullifiers = Vec::new();
    let mut tx_nullifiers = HashSet::new();
    let action_execute_start = Instant::now();
    let local_precheck_start = Instant::now();

    for (i, action) in tx.actions().enumerate() {
        match action {
            Action::Transfer(transfer) => {
                check_action_timestamp_freshness(
                    transfer.body.target_timestamp,
                    execution_context.block_timestamp,
                )?;
                for input in &transfer.body.inputs {
                    if input.is_dummy() {
                        continue;
                    }
                    anyhow::ensure!(
                        tx_nullifiers.insert(input.nullifier),
                        "transaction contains duplicate spend nullifier {}",
                        input.nullifier
                    );
                    spend_nullifiers.push(input.nullifier);
                }
                anchor_pairs.insert((transfer.body.compliance_anchor, transfer.body.asset_anchor));
                output_payloads.extend(
                    transfer
                        .body
                        .outputs
                        .iter()
                        .filter(|output| !output.is_dummy())
                        .map(|output| output.note_payload.clone()),
                );
            }
            // Split and Consolidate have no nullifiers or compliance anchors to pre-read.
            Action::Split(_) | Action::Consolidate(_) => {}
            _ => anyhow::bail!(
                "parallel prepare only supports transfer actions, found unsupported action {:?} at index {}",
                action,
                i
            ),
        }
    }
    if let Some(fee_funding) = &tx.transaction_body.fee_funding {
        check_action_timestamp_freshness(
            fee_funding.transfer.body.target_timestamp,
            execution_context.block_timestamp,
        )?;
        for input in &fee_funding.transfer.body.inputs {
            if input.is_dummy() {
                continue;
            }
            anyhow::ensure!(
                tx_nullifiers.insert(input.nullifier),
                "transaction contains duplicate spend nullifier {}",
                input.nullifier
            );
            spend_nullifiers.push(input.nullifier);
        }
        anchor_pairs.insert((
            fee_funding.transfer.body.compliance_anchor,
            fee_funding.transfer.body.asset_anchor,
        ));
        output_payloads.extend(
            fee_funding
                .transfer
                .body
                .outputs
                .iter()
                .filter(|output| !output.is_dummy())
                .map(|output| output.note_payload.clone()),
        );
    }
    prepared.execution_profile.read_local_precheck_ms =
        local_precheck_start.elapsed().as_secs_f64() * 1000.0;

    enum ReadTaskResult {
        Anchor { elapsed_ms: f64, cache_wait_ms: f64 },
        Nullifier(f64),
    }

    let historical_future = async {
        if skip_historical {
            Ok(HistoricalCheckProfile::default())
        } else {
            check_historical_with_context_profiled(Arc::as_ref(&tx), state.clone(), &context).await
        }
    };

    let mut read_tasks = JoinSet::new();
    for (user_anchor, asset_anchor) in anchor_pairs {
        let state = state.clone();
        let anchor_cache = context.anchor_cache.clone();
        read_tasks.spawn(async move {
            validate_compliance_anchors_read_only(
                Arc::as_ref(&state),
                &user_anchor,
                &asset_anchor,
                context.block_height,
                anchor_cache,
            )
            .await
            .map(|(elapsed_ms, cache_wait_ms)| ReadTaskResult::Anchor {
                elapsed_ms,
                cache_wait_ms,
            })
        });
    }
    for nullifier in &spend_nullifiers {
        let state = state.clone();
        let nullifier = *nullifier;
        let context = context.clone();
        read_tasks.spawn(async move {
            check_nullifier_read_only(Arc::as_ref(&state), &context, nullifier)
                .await
                .map(ReadTaskResult::Nullifier)
        });
    }

    let read_task_future = async {
        let mut execution_profile = TransactionExecutionProfile::default();
        while let Some(result) = read_tasks.join_next().await {
            match result?? {
                ReadTaskResult::Anchor {
                    elapsed_ms,
                    cache_wait_ms,
                } => {
                    execution_profile.read_anchor_validation_ms += elapsed_ms;
                    execution_profile.read_anchor_cache_wait_ms += cache_wait_ms;
                }
                ReadTaskResult::Nullifier(elapsed_ms) => {
                    execution_profile.read_committed_nullifier_ms += elapsed_ms;
                    execution_profile.read_nullifier_wait_ms += elapsed_ms;
                    execution_profile.spend_nullifier_committed_check_ms += elapsed_ms;
                    execution_profile.spend_nullifier_check_ms += elapsed_ms;
                    execution_profile.nullifier_lookup_count += 1;
                }
            }
        }
        Ok::<TransactionExecutionProfile, anyhow::Error>(execution_profile)
    };

    let read_lookup_wait_start = Instant::now();
    let (historical_profile, read_task_profile) =
        tokio::try_join!(historical_future, read_task_future)?;
    prepared.execution_profile.read_lookup_wait_or_join_ms =
        read_lookup_wait_start.elapsed().as_secs_f64() * 1000.0;

    prepared.check_historical_ms = historical_profile.total_ms;
    prepared.execution_profile.read_historical_check_ms = historical_profile.await_ms;
    prepared.execution_profile.read_anchor_validation_ms +=
        read_task_profile.read_anchor_validation_ms;
    prepared.execution_profile.read_anchor_cache_wait_ms +=
        read_task_profile.read_anchor_cache_wait_ms;
    prepared.execution_profile.read_committed_nullifier_ms +=
        read_task_profile.read_committed_nullifier_ms;
    prepared.execution_profile.read_nullifier_wait_ms += read_task_profile.read_nullifier_wait_ms;
    prepared
        .execution_profile
        .spend_nullifier_committed_check_ms += read_task_profile.spend_nullifier_committed_check_ms;
    prepared.execution_profile.spend_nullifier_check_ms +=
        read_task_profile.spend_nullifier_check_ms;
    prepared.execution_profile.nullifier_lookup_count += read_task_profile.nullifier_lookup_count;

    let effects_build_start = Instant::now();
    prepared.effects.spend_nullifiers = spend_nullifiers;
    prepared.effects.sct_payloads = output_payloads
        .into_iter()
        .map(|note_payload| (note_payload, execution_context.source.clone().into()).into())
        .collect();
    prepared.execution_profile.read_effects_build_ms =
        effects_build_start.elapsed().as_secs_f64() * 1000.0;
    prepared.execution_profile.output_add_note_payload_ms =
        prepared.execution_profile.read_effects_build_ms;
    prepared.execution_profile.output_action_execute_ms =
        prepared.execution_profile.read_effects_build_ms;
    prepared.execution_profile.action_execute_ms =
        action_execute_start.elapsed().as_secs_f64() * 1000.0;
    prepared.read_wall_ms = read_start.elapsed().as_secs_f64() * 1000.0;
    Ok(prepared)
}

pub(crate) fn prepare_candidate_read_blocking_profiled(
    tx: Arc<Transaction>,
    snapshot: Snapshot,
    context: HistoricalCheckContext,
    skip_historical: bool,
    handle: tokio::runtime::Handle,
) -> Result<PreparedCandidateRead> {
    let read_start = Instant::now();
    let mut prepared = PreparedCandidateRead::default();

    let execution_context = TxExecutionContext {
        block_timestamp: context.block_timestamp,
        source: tx.id(),
    };
    let mut anchor_pairs = BTreeSet::new();
    let mut output_payloads = Vec::new();
    let mut spend_nullifiers = Vec::new();
    let mut tx_nullifiers = HashSet::new();
    let action_execute_start = Instant::now();
    let local_precheck_start = Instant::now();

    for (i, action) in tx.actions().enumerate() {
        match action {
            Action::Transfer(transfer) => {
                check_action_timestamp_freshness(
                    transfer.body.target_timestamp,
                    execution_context.block_timestamp,
                )?;
                for input in &transfer.body.inputs {
                    if input.is_dummy() {
                        continue;
                    }
                    anyhow::ensure!(
                        tx_nullifiers.insert(input.nullifier),
                        "transaction contains duplicate spend nullifier {}",
                        input.nullifier
                    );
                    spend_nullifiers.push(input.nullifier);
                }
                anchor_pairs.insert((transfer.body.compliance_anchor, transfer.body.asset_anchor));
                output_payloads.extend(
                    transfer
                        .body
                        .outputs
                        .iter()
                        .filter(|output| !output.is_dummy())
                        .map(|output| output.note_payload.clone()),
                );
            }
            // Split and Consolidate have no nullifiers or compliance anchors to pre-read.
            Action::Split(_) | Action::Consolidate(_) => {}
            _ => anyhow::bail!(
                "parallel prepare only supports transfer actions, found unsupported action {:?} at index {}",
                action,
                i
            ),
        }
    }
    if let Some(fee_funding) = &tx.transaction_body.fee_funding {
        check_action_timestamp_freshness(
            fee_funding.transfer.body.target_timestamp,
            execution_context.block_timestamp,
        )?;
        for input in &fee_funding.transfer.body.inputs {
            if input.is_dummy() {
                continue;
            }
            anyhow::ensure!(
                tx_nullifiers.insert(input.nullifier),
                "transaction contains duplicate spend nullifier {}",
                input.nullifier
            );
            spend_nullifiers.push(input.nullifier);
        }
        anchor_pairs.insert((
            fee_funding.transfer.body.compliance_anchor,
            fee_funding.transfer.body.asset_anchor,
        ));
        output_payloads.extend(
            fee_funding
                .transfer
                .body
                .outputs
                .iter()
                .filter(|output| !output.is_dummy())
                .map(|output| output.note_payload.clone()),
        );
    }
    prepared.execution_profile.read_local_precheck_ms =
        local_precheck_start.elapsed().as_secs_f64() * 1000.0;

    let lookup_wait_start = Instant::now();
    if skip_historical {
        prepared.check_historical_ms = 0.0;
    } else {
        let historical_profile = check_historical_with_context_sync_profiled(
            Arc::as_ref(&tx),
            &snapshot,
            &context,
            &handle,
        )?;
        prepared.check_historical_ms = historical_profile.total_ms;
        prepared.execution_profile.read_historical_check_ms = historical_profile.await_ms;
    }

    for (user_anchor, asset_anchor) in anchor_pairs {
        let (elapsed_ms, cache_wait_ms) = validate_compliance_anchors_read_only_sync(
            &handle,
            &snapshot,
            &user_anchor,
            &asset_anchor,
            context.block_height,
            context.anchor_cache.clone(),
        )?;
        prepared.execution_profile.read_anchor_validation_ms += elapsed_ms;
        prepared.execution_profile.read_anchor_cache_wait_ms += cache_wait_ms;
    }
    for nullifier in &spend_nullifiers {
        let elapsed_ms = check_nullifier_read_only_sync(&handle, &snapshot, &context, *nullifier)?;
        prepared.execution_profile.read_committed_nullifier_ms += elapsed_ms;
        prepared.execution_profile.read_nullifier_wait_ms += elapsed_ms;
        prepared
            .execution_profile
            .spend_nullifier_committed_check_ms += elapsed_ms;
        prepared.execution_profile.spend_nullifier_check_ms += elapsed_ms;
        prepared.execution_profile.nullifier_lookup_count += 1;
    }
    prepared.execution_profile.read_lookup_wait_or_join_ms =
        lookup_wait_start.elapsed().as_secs_f64() * 1000.0;

    let effects_build_start = Instant::now();
    prepared.effects.spend_nullifiers = spend_nullifiers;
    prepared.effects.sct_payloads = output_payloads
        .into_iter()
        .map(|note_payload| (note_payload, execution_context.source.clone().into()).into())
        .collect();
    prepared.execution_profile.read_effects_build_ms =
        effects_build_start.elapsed().as_secs_f64() * 1000.0;
    prepared.execution_profile.output_add_note_payload_ms =
        prepared.execution_profile.read_effects_build_ms;
    prepared.execution_profile.output_action_execute_ms =
        prepared.execution_profile.read_effects_build_ms;
    prepared.execution_profile.action_execute_ms =
        action_execute_start.elapsed().as_secs_f64() * 1000.0;
    prepared.read_wall_ms = read_start.elapsed().as_secs_f64() * 1000.0;
    Ok(prepared)
}

fn check_action_timestamp_freshness(target_timestamp: u64, block_timestamp: u64) -> Result<()> {
    if target_timestamp == 0
        && std::env::var_os("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP").is_some()
    {
        return Ok(());
    }
    check_timestamp_freshness(target_timestamp, block_timestamp)?;
    Ok(())
}

#[async_trait]
impl AppActionHandler for Transaction {
    type CheckStatelessContext = ();

    // We only instrument the top-level `check_stateless`, so we get one span for each transaction.
    #[instrument(skip(self, _context))]
    async fn check_stateless(&self, _context: ()) -> Result<()> {
        // This check should be done first, and complete before all other
        // stateless checks, like proof verification.  In addition to proving
        // that value balances, the binding signature binds the proofs to the
        // transaction, as the binding signature can only be created with
        // knowledge of all of the openings to the commitments the transaction
        // makes proofs against. (This is where the name binding signature comes
        // from).
        //
        // This allows us to cheaply eliminate a large class of invalid
        // transactions upfront -- past this point, we can be sure that the user
        // who submitted the transaction actually formed the proofs, rather than
        // replaying them from another transaction.
        valid_binding_signature(self)?;
        // Other checks probably too cheap to be worth splitting into tasks.
        num_clues_equal_to_num_outputs(self)?;
        check_memo_exists_if_outputs_absent_if_not(self)?;
        // This check ensures that transactions contain at least one action.
        check_non_empty_transaction(self)?;

        let context = self.context();

        // Currently, we need to clone the component actions so that the spawned
        // futures can have 'static lifetimes. In the future, we could try to
        // use the yoke crate, but cloning is almost certainly not a big deal
        // for now.
        let mut action_checks = JoinSet::new();
        for (i, action) in self.actions().cloned().enumerate() {
            let context2 = context.clone();
            let span = action.create_span(i);
            action_checks
                .spawn(async move { action.check_stateless(context2).await }.instrument(span));
        }
        // Now check if any component action failed verification.
        while let Some(check) = action_checks.join_next().await {
            check??;
        }

        if let Some(fee_funding) = &self.transaction_body.fee_funding {
            fee_funding.transfer.check_stateless(context).await?;
        }

        Ok(())
    }

    // We only instrument the top-level `check_stateful`, so we get one span for each transaction.
    #[instrument(skip(self, state))]
    async fn check_historical<S: StateRead + 'static>(&self, state: Arc<S>) -> Result<()> {
        let context = HistoricalCheckContext::load(Arc::as_ref(&state)).await?;
        check_historical_with_context(self, state, &context).await
    }

    // We only instrument the top-level `execute`, so we get one span for each transaction.
    #[instrument(skip(self, state))]
    async fn check_and_execute<S: StateWrite>(&self, state: S) -> Result<()> {
        check_and_execute_profiled(self, state, true).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    // Serializes tests that read/write PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP to
    // prevent env-var races when tests run in parallel.
    static TIMESTAMP_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    use std::ops::Deref;

    use anyhow::Result;
    use decaf377::Fr;
    use penumbra_sdk_asset::{asset, Value, BASE_ASSET_ID};
    use penumbra_sdk_compliance::{ComplianceLeaf, IndexedMerkleTree, MerklePath, QuadTree};
    use penumbra_sdk_fee::Fee;
    use penumbra_sdk_keys::{test_keys, Address};
    use penumbra_sdk_shielded_pool::{Note, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
    use penumbra_sdk_tct as tct;
    use penumbra_sdk_transaction::{
        plan::{CluePlan, DetectionDataPlan, TransactionPlan},
        TransactionParameters, WitnessData,
    };
    use rand_core::OsRng;

    use crate::AppActionHandler;

    use super::{AnchorValidationCache, ClaimedAnchorValidationCache};

    #[tokio::test]
    async fn anchor_validation_cache_counts_shared_pair_once() -> Result<()> {
        let cache = Arc::new(AnchorValidationCache::default());
        let pair = (
            tct::StateCommitment::try_from([0; 32]).expect("valid commitment"),
            tct::StateCommitment::try_from([1; 32]).expect("valid commitment"),
        );

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let cache = cache.clone();
            tasks.spawn(async move {
                let (cell, _hit, _wait_ms) = cache.entry(pair);
                let result = cell
                    .get_or_init(|| async { Ok::<(), String>(()) })
                    .await
                    .clone();
                anyhow::ensure!(result.is_ok(), "cache cell should initialize successfully");
                Ok::<(), anyhow::Error>(())
            });
        }

        while let Some(result) = tasks.join_next().await {
            result??;
        }

        let (hits, misses, unique_pairs) = cache.stats();
        assert_eq!(misses, 1);
        assert_eq!(hits, 7);
        assert_eq!(unique_pairs, 1);

        Ok(())
    }

    #[tokio::test]
    async fn claimed_anchor_validation_cache_counts_shared_anchor_once() -> Result<()> {
        let cache = Arc::new(ClaimedAnchorValidationCache::default());
        let anchor = penumbra_sdk_tct::Tree::new().root();

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let cache = cache.clone();
            tasks.spawn(async move {
                let (cell, _hit) = cache.entry(anchor);
                let result = cell
                    .get_or_init(|| async { Ok::<(), String>(()) })
                    .await
                    .clone();
                anyhow::ensure!(
                    result.is_ok(),
                    "claimed anchor cell should initialize successfully"
                );
                Ok::<(), anyhow::Error>(())
            });
        }

        while let Some(result) = tasks.join_next().await {
            result??;
        }

        let (hits, misses, unique_values) = cache.stats();
        assert_eq!(misses, 1);
        assert_eq!(hits, 7);
        assert_eq!(unique_values, 1);

        Ok(())
    }

    /// Enrich a shielded input plan with valid compliance data for testing.
    /// Uses unregulated compliance for simplicity.
    fn enrich_spend_for_test<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
        spend: &mut ShieldedInputPlan,
        _sender_address: &Address,
    ) {
        let asset_id = spend.note.asset_id();

        // Create IMT non-membership proof (unregulated asset)
        let imt = IndexedMerkleTree::new();
        let (position, indexed_leaf, auth_path) = imt
            .non_membership_proof(asset_id.0)
            .expect("can generate non-membership proof");
        let asset_anchor = tct::StateCommitment(imt.root().0);
        let asset_path = MerklePath::from_auth_path(auth_path);

        // Set IMT data BEFORE set_compliance_details (it reads asset_indexed_leaf for DLEQ)
        spend.asset_anchor = asset_anchor;
        spend.asset_path = asset_path;
        spend.asset_position = position;
        spend.asset_indexed_leaf = indexed_leaf;

        spend
            .set_compliance_details(rng)
            .expect("can set compliance details");

        // Build user tree from the compliance_leaf that set_compliance_details created
        // (has real d derived from address, matching what the circuit will use)
        let user_leaf = spend.compliance_leaf.clone().unwrap();
        let mut user_tree = QuadTree::new();
        user_tree
            .update(0, user_leaf.commit())
            .expect("can update tree");
        let compliance_anchor = tct::StateCommitment(user_tree.root().0);
        let user_auth_path = user_tree.auth_path(0).expect("can get auth path");
        let compliance_path = MerklePath::from_auth_path(user_auth_path);

        spend.compliance_anchor = compliance_anchor;
        spend.compliance_path = compliance_path;
        spend.compliance_position = 0;
    }

    /// Enrich a shielded output plan with valid compliance data for testing.
    /// Uses unregulated compliance for simplicity.
    fn enrich_output_for_test<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
        output: &mut ShieldedOutputPlan,
        sender_address: &Address,
        asset_id: asset::Id,
    ) {
        // Create IMT non-membership proof (unregulated asset)
        let imt = IndexedMerkleTree::new();
        let (position, indexed_leaf, auth_path) = imt
            .non_membership_proof(asset_id.0)
            .expect("can generate non-membership proof");
        let asset_anchor = tct::StateCommitment(imt.root().0);
        let asset_path = MerklePath::from_auth_path(auth_path);

        // Set IMT data BEFORE set_compliance_details (it reads asset_indexed_leaf for DLEQ)
        output.asset_anchor = asset_anchor;
        output.asset_path = asset_path;
        output.asset_position = position;
        output.asset_indexed_leaf = indexed_leaf;

        // Create leaves with real d (matching what the circuit derives)
        let recv_b_d_fq = output
            .dest_address
            .diversified_generator()
            .vartime_compress_to_field();
        let recipient_leaf =
            ComplianceLeaf::new(output.dest_address.clone(), asset_id, recv_b_d_fq);

        let send_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_leaf = ComplianceLeaf::new(sender_address.clone(), asset_id, send_b_d_fq);

        output
            .set_compliance_details(
                rng,
                &recipient_leaf,
                sender_leaf,
                Fr::from(0u64), // tx_blinding_nonce
            )
            .expect("can set compliance details");

        // Build user tree from the compliance_leaf that set_compliance_details created
        // (has real d derived from address, matching what the circuit will use)
        let user_leaf = output.compliance_leaf.clone().unwrap();
        let mut user_tree = QuadTree::new();
        user_tree
            .update(0, user_leaf.commit())
            .expect("can update tree");
        let compliance_anchor = tct::StateCommitment(user_tree.root().0);
        let user_auth_path = user_tree.auth_path(0).expect("can get auth path");
        let compliance_path = MerklePath::from_auth_path(user_auth_path);

        output.compliance_anchor = compliance_anchor;
        output.compliance_path = compliance_path;
        output.compliance_position = 0;
    }

    fn align_transfer_compliance_for_test<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
        spends: &mut [&mut ShieldedInputPlan],
        outputs: &mut [&mut ShieldedOutputPlan],
    ) {
        let tx_blinding_nonce = Fr::rand(rng);
        let leaves = spends
            .iter()
            .map(|spend| {
                spend
                    .compliance_leaf
                    .clone()
                    .expect("test spend has compliance leaf")
            })
            .chain(outputs.iter().map(|output| {
                output
                    .compliance_leaf
                    .clone()
                    .expect("test output has compliance leaf")
            }))
            .collect::<Vec<_>>();

        let mut user_tree = QuadTree::new();
        for (position, leaf) in leaves.iter().enumerate() {
            user_tree
                .update(position as u64, leaf.commit())
                .expect("can update test compliance tree");
        }
        let compliance_anchor = tct::StateCommitment(user_tree.root().0);

        for (position, spend) in spends.iter_mut().enumerate() {
            let auth_path = user_tree
                .auth_path(position as u64)
                .expect("can get spend compliance path");
            spend.compliance_anchor = compliance_anchor;
            spend.compliance_path = MerklePath::from_auth_path(auth_path);
            spend.compliance_position = position as u64;
            spend.tx_blinding_nonce = tx_blinding_nonce;
        }
        for (index, output) in outputs.iter_mut().enumerate() {
            let position = spends.len() + index;
            let auth_path = user_tree
                .auth_path(position as u64)
                .expect("can get output compliance path");
            output.compliance_anchor = compliance_anchor;
            output.compliance_path = MerklePath::from_auth_path(auth_path);
            output.compliance_position = position as u64;
            output.tx_blinding_nonce = tx_blinding_nonce;
        }
    }

    #[tokio::test]
    async fn check_stateless_succeeds_on_valid_spend() -> Result<()> {
        // Generate a note controlled by the test address.
        let value = Value {
            amount: 100u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);

        // Record that note in an SCT, where we can generate an auth path.
        let mut sct = tct::Tree::new();
        // Assume there's a bunch of stuff already in the SCT.
        for _ in 0..5 {
            let random_note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
            sct.insert(tct::Witness::Keep, random_note.commit())
                .unwrap();
        }
        sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        // Do we want to seal the SCT block here?
        let auth_path = sct.witness(note.commit()).unwrap();

        // Create plans and enrich with compliance data
        let mut spend1 = ShieldedInputPlan::new(&mut OsRng, note, auth_path.position());
        let mut output1 =
            ShieldedOutputPlan::new(&mut OsRng, value, test_keys::ADDRESS_1.deref().clone());

        enrich_spend_for_test(&mut OsRng, &mut spend1, &test_keys::ADDRESS_0);
        enrich_output_for_test(
            &mut OsRng,
            &mut output1,
            &test_keys::ADDRESS_0,
            value.asset_id,
        );
        align_transfer_compliance_for_test(&mut OsRng, &mut [&mut spend1], &mut [&mut output1]);

        let transfer =
            TransferPlan::from_spend_output(spend1.into(), output1.into(), Fr::rand(&mut OsRng))
                .expect("valid transfer plan");

        let plan = TransactionPlan {
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                fee: Fee::default(),
                chain_id: "".into(),
            },
            actions: vec![transfer.into()],
            fee_funding: None,
            detection_data: Some(DetectionDataPlan {
                clue_plans: vec![CluePlan::new(
                    &mut OsRng,
                    test_keys::ADDRESS_1.deref().clone(),
                    1.try_into().unwrap(),
                )],
            }),
            memo: None,
        };

        // Build the transaction.
        let fvk = &test_keys::FULL_VIEWING_KEY;
        let sk = &test_keys::SPEND_KEY;
        let auth_data = plan.authorize(OsRng, sk)?;
        let witness_data = WitnessData {
            anchor: sct.root(),
            state_commitment_proofs: plan
                .transfer_plans()
                .flat_map(|transfer| transfer.spends.iter())
                .map(|spend| {
                    (
                        spend.note.commit(),
                        sct.witness(spend.note.commit()).unwrap(),
                    )
                })
                .collect(),
        };
        let tx = plan
            .build_concurrent(fvk, &witness_data, &auth_data)
            .await
            .expect("can build transaction");

        let context = tx.context();

        // On the verifier side, perform stateless verification.
        for action in tx.transaction_body().actions {
            let result = action.check_stateless(context.clone()).await;
            assert!(result.is_ok())
        }

        Ok(())
    }

    #[tokio::test]
    async fn check_stateless_fails_on_auth_path_with_wrong_root() -> Result<()> {
        // Generate a note controlled by the test address.
        let value = Value {
            amount: 100u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);

        // Record that note in an SCT, where we can generate an auth path.
        let mut sct = tct::Tree::new();
        let wrong_root = sct.root();
        sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        let auth_path = sct.witness(note.commit()).unwrap();

        // Create plans and enrich with compliance data
        let mut spend1 = ShieldedInputPlan::new(&mut OsRng, note, auth_path.position());
        let mut output1 =
            ShieldedOutputPlan::new(&mut OsRng, value, test_keys::ADDRESS_1.deref().clone());

        enrich_spend_for_test(&mut OsRng, &mut spend1, &test_keys::ADDRESS_0);
        enrich_output_for_test(
            &mut OsRng,
            &mut output1,
            &test_keys::ADDRESS_0,
            value.asset_id,
        );
        align_transfer_compliance_for_test(&mut OsRng, &mut [&mut spend1], &mut [&mut output1]);

        let transfer =
            TransferPlan::from_spend_output(spend1.into(), output1.into(), Fr::rand(&mut OsRng))
                .expect("valid transfer plan");

        let plan = TransactionPlan {
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                fee: Fee::default(),
                chain_id: "".into(),
            },
            actions: vec![transfer.into()],
            fee_funding: None,
            detection_data: None,
            memo: None,
        };

        // Build the transaction.
        let fvk = &test_keys::FULL_VIEWING_KEY;
        let sk = &test_keys::SPEND_KEY;
        let auth_data = plan.authorize(OsRng, sk)?;
        let witness_data = WitnessData {
            anchor: sct.root(),
            state_commitment_proofs: plan
                .transfer_plans()
                .flat_map(|transfer| transfer.spends.iter())
                .map(|spend| {
                    (
                        spend.note.commit(),
                        sct.witness(spend.note.commit()).unwrap(),
                    )
                })
                .collect(),
        };
        let mut tx = plan
            .build_concurrent(fvk, &witness_data, &auth_data)
            .await
            .expect("can build transaction");

        // Set the anchor to the wrong root.
        tx.anchor = wrong_root;

        // On the verifier side, perform stateless verification.
        let result = tx.check_stateless(()).await;
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn zero_timestamp_requires_benchmark_override() {
        let _guard = TIMESTAMP_ENV_MUTEX.lock().unwrap();
        std::env::remove_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP");
        assert!(super::check_action_timestamp_freshness(0, 1_700_000_000).is_err());
    }

    #[test]
    fn zero_timestamp_is_allowed_when_benchmark_override_is_set() {
        let _guard = TIMESTAMP_ENV_MUTEX.lock().unwrap();
        std::env::set_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP", "1");
        let result = super::check_action_timestamp_freshness(0, 1_700_000_000);
        std::env::remove_var("PENUMBRA_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP");
        assert!(result.is_ok());
    }

    #[test]
    fn nonzero_timestamps_still_enforce_timestamp_freshness() {
        let _guard = TIMESTAMP_ENV_MUTEX.lock().unwrap();
        assert!(super::check_action_timestamp_freshness(1_700_000_000, 1_700_000_100).is_ok());
        assert!(super::check_action_timestamp_freshness(1_700_000_000, 1_700_003_700).is_err());
    }
}
