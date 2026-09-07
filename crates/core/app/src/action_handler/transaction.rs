use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, RwLock};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use cnidarium::{Snapshot, StateRead, StateWrite};
use shieldd_sdk_compact_block::{
    component::RoutingManager as _, PendingRoutingAction, StatePayload,
};
use shieldd_sdk_compliance::registry::{check_timestamp_freshness, ComplianceRegistryRead as _};
use shieldd_sdk_fee::component::FeePay as _;
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_sct::component::source::SourceContext;
use shieldd_sdk_sct::component::tree::VerificationExt as _;
use shieldd_sdk_sct::nullifier_generation::{empty_history_head, PROTOCOL_VERSION};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_shielded_pool::component::{
    note_reshape_execute_verified, shielded_host_withdrawal_execute_verified,
    shielded_ics20_withdrawal_execute_verified, transfer_execute_validated,
    transfer_execute_verified, transfer_validate_verified, Ics20Transfer, StateReadExt as _,
};
use shieldd_sdk_shielded_pool::discovery;
use shieldd_sdk_tct::StateCommitment;
use shieldd_sdk_transaction::{gas::GasCost as _, Action, Transaction};
use shieldd_sdk_txhash::{AuthorizingData, TransactionId};
use tokio::sync::OnceCell;
use tokio::task::JoinSet;
use tracing::{instrument, Instrument};

use super::AppActionHandler;
use crate::{
    app::{StateReadExt as _, MAX_TRANSACTION_ACTION_COUNT, MAX_TRANSACTION_NULLIFIER_COUNT},
    stateless_cache::{ProofSlot, VerifiedTxArtifact},
    ShielddHost,
};

mod stateful;
pub(crate) mod stateless;

use self::stateful::{
    claimed_anchor_is_valid, discovery_parameters_valid_with_context,
    tx_parameters_historical_check_with_context,
};
use crate::stateless_cache::VerifiedHistoricalInput;
use stateless::{
    check_memo_exists_if_outputs_absent_if_not, check_non_empty_transaction,
    valid_binding_signature,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct PreparedCandidateRead {
    pub spend_nullifiers: Vec<Nullifier>,
    pub sct_payloads: Vec<StatePayload>,
    pub routing_actions: Vec<PendingRoutingAction>,
}

type AnchorValidationKey = (StateCommitment, StateCommitment, u64);
type ClaimedAnchorKey = shieldd_sdk_tct::Root;
type ValidationCell = Arc<OnceCell<std::result::Result<(), String>>>;

#[derive(Debug)]
pub(crate) struct ValidationCache<K> {
    entries: RwLock<HashMap<K, ValidationCell>>,
}

impl<K> Default for ValidationCache<K> {
    fn default() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
}

impl<K: Eq + std::hash::Hash> ValidationCache<K> {
    fn entry(&self, key: K) -> ValidationCell {
        if let Some(cell) = self
            .entries
            .read()
            .expect("validation cache poisoned")
            .get(&key)
        {
            return cell.clone();
        }
        self.entries
            .write()
            .expect("validation cache poisoned")
            .entry(key)
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone()
    }
}

type AnchorValidationCache = ValidationCache<AnchorValidationKey>;
type ClaimedAnchorValidationCache = ValidationCache<ClaimedAnchorKey>;

#[derive(Clone, Debug)]
struct TxExecutionContext {
    block_timestamp: u64,
    source: TransactionId,
}

fn transaction_routing_actions(tx: &Transaction) -> Result<Vec<PendingRoutingAction>> {
    let transaction_id = tx.id();
    let mut routing_actions = Vec::new();
    for (action_index, action) in tx.actions().enumerate() {
        let action_index = u32::try_from(action_index).context("action index exceeds u32")?;
        let action = match action {
            Action::Transfer(transfer) => Some(PendingRoutingAction {
                transaction_id,
                action_index,
                tags: transfer.body.routing.tags.to_vec(),
                note_payloads: transfer
                    .body
                    .outputs
                    .iter()
                    .map(|output| output.note_payload.clone())
                    .collect(),
            }),
            Action::NoteReshape(note_reshape) => Some(PendingRoutingAction {
                transaction_id,
                action_index,
                tags: vec![note_reshape.body.routing_tag],
                note_payloads: note_reshape
                    .body
                    .outputs
                    .iter()
                    .map(|output| output.note_payload.clone())
                    .collect(),
            }),
            Action::ShieldedIcs20Withdrawal(withdrawal) => Some(PendingRoutingAction {
                transaction_id,
                action_index,
                tags: vec![withdrawal.body.routing_tag],
                note_payloads: vec![withdrawal.body.change_output.note_payload.clone()],
            }),
            Action::ShieldedHostWithdrawal(withdrawal) => Some(PendingRoutingAction {
                transaction_id,
                action_index,
                tags: vec![withdrawal.body.routing_tag],
                note_payloads: vec![withdrawal.body.change_output.note_payload.clone()],
            }),
            _ => None,
        };
        routing_actions.extend(action);
    }
    if let Some(fee_funding) = &tx.transaction_body.fee_funding {
        let action_index =
            u32::try_from(tx.transaction_body.actions.len()).context("action index exceeds u32")?;
        routing_actions.push(PendingRoutingAction {
            transaction_id,
            action_index,
            tags: fee_funding.transfer.body.routing.tags.to_vec(),
            note_payloads: fee_funding
                .transfer
                .body
                .outputs
                .iter()
                .map(|output| output.note_payload.clone())
                .collect(),
        });
    }
    Ok(routing_actions)
}

#[derive(Clone, Debug)]
pub(crate) struct HistoricalCheckContext {
    pub chain_id: String,
    pub block_height: u64,
    pub block_timestamp: u64,
    pub discovery_grace_period_blocks: u64,
    pub previous_discovery_parameters: discovery::Parameters,
    pub current_discovery_parameters: discovery::Parameters,
    pub anchor_cache: Arc<AnchorValidationCache>,
    pub claimed_anchor_cache: Arc<ClaimedAnchorValidationCache>,
    pub nullifier_window: shieldd_sdk_sct::nullifier_generation::NullifierWindow,
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
        let nullifier_window = shieldd_sdk_sct::nullifier_tree::generation_state(state)
            .await?
            .window();

        Ok(Self {
            chain_id: state.get_chain_id().await?,
            block_height: state.get_block_height().await?,
            block_timestamp: state.get_current_block_timestamp().await?.unix_timestamp() as u64,
            discovery_grace_period_blocks: shielded_pool_params.discovery_grace_period_blocks,
            previous_discovery_parameters: state
                .get_previous_discovery_parameters()
                .await
                .expect("chain params request must succeed"),
            current_discovery_parameters: state
                .get_current_discovery_parameters()
                .await
                .expect("chain params request must succeed"),
            anchor_cache: Arc::new(AnchorValidationCache::default()),
            claimed_anchor_cache: Arc::new(ClaimedAnchorValidationCache::default()),
            nullifier_window,
        })
    }
}

pub(crate) fn transaction_action_count_allowed(
    body_action_count: usize,
    has_fee_funding: bool,
) -> bool {
    body_action_count.saturating_add(usize::from(has_fee_funding)) <= MAX_TRANSACTION_ACTION_COUNT
}

pub(crate) fn transaction_nullifier_count_allowed(nullifier_count: usize) -> bool {
    nullifier_count <= MAX_TRANSACTION_NULLIFIER_COUNT
}

pub(crate) fn transaction_nullifier_count(tx: &Transaction) -> usize {
    tx.spent_nullifier_count()
}

pub(crate) fn ensure_transaction_resource_bounds(tx: &Transaction) -> Result<()> {
    let body_action_count = tx.transaction_body.actions.len();
    let has_fee_funding = tx.transaction_body.fee_funding.is_some();
    anyhow::ensure!(
        transaction_action_count_allowed(body_action_count, has_fee_funding),
        "transaction action count {} exceeds maximum {}",
        body_action_count.saturating_add(usize::from(has_fee_funding)),
        MAX_TRANSACTION_ACTION_COUNT
    );
    let nullifier_count = transaction_nullifier_count(tx);
    anyhow::ensure!(
        transaction_nullifier_count_allowed(nullifier_count),
        "transaction spend nullifier count {} exceeds maximum {}",
        nullifier_count,
        MAX_TRANSACTION_NULLIFIER_COUNT
    );
    Ok(())
}

pub(crate) fn validate_transaction_envelope(tx: &Transaction) -> Result<()> {
    ensure_transaction_resource_bounds(tx)?;
    tx.transaction_body.validate_nullifier_history()?;
    valid_binding_signature(tx)?;
    check_memo_exists_if_outputs_absent_if_not(tx)?;
    check_non_empty_transaction(tx)
}

pub(crate) fn verify_historical_proofs(tx: &Transaction) -> Result<Vec<VerifiedHistoricalInput>> {
    tx.transaction_body.validate_nullifier_history()?;
    let old_nullifiers = tx.transaction_body.historical_nullifiers();
    if old_nullifiers.is_empty() {
        return Ok(Vec::new());
    }
    let window = tx
        .transaction_body
        .nullifier_window
        .context("old inputs require a nullifier window")?;
    let auth_hash = tx.auth_hash();
    old_nullifiers
        .into_iter()
        .zip(&tx.transaction_body.historical_nullifier_proofs)
        .map(|(nullifier, bundle)| {
            let nullifier_bytes: [u8; 32] = nullifier.into();
            let mut expected_head = empty_history_head();
            for chunk in &bundle.completed_chunks {
                shieldd_sdk_proof_params::historical::verify_chunk(
                    shieldd_sdk_proof_params::historical::chunk_verification_key(),
                    shieldd_sdk_proof_params::historical::ChunkClaim {
                        protocol_version: PROTOCOL_VERSION,
                        nullifier: nullifier_bytes,
                        chunk_index: chunk.chunk_index,
                        start_history_head: expected_head,
                        end_history_head: chunk.end_history_head,
                    },
                    &chunk.groth16_proof,
                )?;
                expected_head = chunk.end_history_head;
            }
            for generation in &bundle.tail {
                shieldd_sdk_proof_params::historical::verify_generation(
                    shieldd_sdk_proof_params::historical::generation_verification_key(),
                    shieldd_sdk_proof_params::historical::GenerationClaim {
                        protocol_version: PROTOCOL_VERSION,
                        nullifier: nullifier_bytes,
                        generation_index: generation.generation_index,
                        generation_root: generation.generation_root,
                        generation_start_position: generation.generation_start_position,
                        generation_end_position: generation.generation_end_position,
                        start_history_head: expected_head,
                        end_history_head: shieldd_sdk_sct::nullifier_generation::append_history(
                            expected_head,
                            generation.generation_index,
                            generation.generation_root,
                            generation.generation_start_position,
                            generation.generation_end_position,
                        )?,
                    },
                    &generation.groth16_proof,
                )?;
                expected_head = shieldd_sdk_sct::nullifier_generation::append_history(
                    expected_head,
                    generation.generation_index,
                    generation.generation_root,
                    generation.generation_start_position,
                    generation.generation_end_position,
                )?;
            }
            anyhow::ensure!(
                expected_head == window.archived_history_head,
                "verified historical proof has the wrong terminal history head"
            );
            Ok(VerifiedHistoricalInput::new(nullifier, window, auth_hash))
        })
        .collect()
}

async fn check_nullifier_read_only<S>(
    state: &S,
    _context: &HistoricalCheckContext,
    nullifier: shieldd_sdk_sct::Nullifier,
) -> Result<()>
where
    S: StateRead,
{
    state.check_nullifier_unspent(nullifier).await?;
    Ok(())
}

async fn validate_compliance_anchors_read_only<S: StateRead>(
    state: &S,
    user_anchor: &StateCommitment,
    asset_anchor: &StateCommitment,
    block_height: u64,
    anchor_cache: Arc<AnchorValidationCache>,
) -> Result<()> {
    let anchor_key = (*user_anchor, *asset_anchor, block_height);

    let cell = anchor_cache.entry(anchor_key);

    let result = cell
        .get_or_init(|| async move {
            let current_user_anchor = state
                .get_user_tree_root()
                .await
                .map_err(|e| e.to_string())?;
            if *user_anchor != current_user_anchor {
                return Err(
                    "user compliance anchor does not match the current user compliance root"
                        .to_string(),
                );
            }

            let current_asset_anchor = state
                .get_asset_imt_root()
                .await
                .map_err(|e| e.to_string())?;
            if *asset_anchor != current_asset_anchor {
                return Err(
                    "asset compliance anchor does not match the current asset compliance root"
                        .to_string(),
                );
            }

            Ok(())
        })
        .await;

    match result {
        Ok(()) => Ok(()),
        Err(error) => anyhow::bail!(error.clone()),
    }
}

async fn validate_claimed_anchor_read_only<S: StateRead>(
    state: Arc<S>,
    tx: Arc<Transaction>,
    claimed_anchor_cache: Arc<ClaimedAnchorValidationCache>,
) -> Result<()> {
    let anchor = tx.anchor;

    let cell = claimed_anchor_cache.entry(anchor);
    let result = cell
        .get_or_init(|| async move {
            claimed_anchor_is_valid(state, Arc::as_ref(&tx))
                .await
                .map_err(|e| e.to_string())
        })
        .await;

    match result {
        Ok(()) => Ok(()),
        Err(error) => anyhow::bail!(error.clone()),
    }
}

pub(crate) fn supports_parallel_prepare(tx: &Transaction) -> bool {
    tx.actions().all(|a| matches!(a, Action::Transfer(_)))
}

fn action_requires_historical_check(action: &Action) -> bool {
    matches!(
        action,
        Action::IbcRelay(_)
            | Action::ShieldedIcs20Withdrawal(_)
            | Action::ShieldedHostWithdrawal(_)
    )
}

fn check_nullifier_read_only_sync(
    handle: &tokio::runtime::Handle,
    snapshot: &Snapshot,
    _context: &HistoricalCheckContext,
    nullifier: shieldd_sdk_sct::Nullifier,
) -> Result<()> {
    handle.block_on(snapshot.check_nullifier_unspent(nullifier))?;
    Ok(())
}

fn validate_compliance_anchors_read_only_sync(
    handle: &tokio::runtime::Handle,
    snapshot: &Snapshot,
    user_anchor: &StateCommitment,
    asset_anchor: &StateCommitment,
    block_height: u64,
    anchor_cache: Arc<AnchorValidationCache>,
) -> Result<()> {
    let anchor_key = (*user_anchor, *asset_anchor, block_height);

    let cell = anchor_cache.entry(anchor_key);
    let snapshot = snapshot.clone();
    let user_anchor = *user_anchor;
    let asset_anchor = *asset_anchor;

    let result = handle
        .clone()
        .block_on(cell.get_or_init(|| async move {
            let current_user_anchor = snapshot
                .get_user_tree_root()
                .await
                .map_err(|e| e.to_string())?;
            if user_anchor != current_user_anchor {
                return Err(
                    "user compliance anchor does not match the current user compliance root"
                        .to_string(),
                );
            }

            let current_asset_anchor = snapshot
                .get_asset_imt_root()
                .await
                .map_err(|e| e.to_string())?;
            if asset_anchor != current_asset_anchor {
                return Err(
                    "asset compliance anchor does not match the current asset compliance root"
                        .to_string(),
                );
            }

            Ok(())
        }))
        .clone();

    match result {
        Ok(()) => Ok(()),
        Err(error) => anyhow::bail!(error),
    }
}

fn validate_claimed_anchor_read_only_sync(
    handle: &tokio::runtime::Handle,
    snapshot: &Snapshot,
    tx: &Transaction,
    claimed_anchor_cache: Arc<ClaimedAnchorValidationCache>,
) -> Result<()> {
    let anchor = tx.anchor;

    let cell = claimed_anchor_cache.entry(anchor);
    let snapshot = snapshot.clone();

    let result = handle
        .clone()
        .block_on(cell.get_or_init(|| async move {
            if anchor.is_empty() {
                return Ok(());
            }
            if snapshot
                .get_raw(&shieldd_sdk_sct::state_key::tree::anchor_lookup(anchor))
                .await
                .map_err(|e| e.to_string())?
                .map(|bytes| {
                    <u64 as shieldd_sdk_proto::Message>::decode(bytes.as_slice())
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

    match result {
        Ok(()) => Ok(()),
        Err(error) => anyhow::bail!(error),
    }
}

pub(crate) async fn check_historical_with_context<S: StateRead + 'static>(
    tx: &Transaction,
    state: Arc<S>,
    context: &HistoricalCheckContext,
) -> Result<()> {
    let mut action_checks = JoinSet::new();

    ensure_transaction_resource_bounds(tx)?;
    tx_parameters_historical_check_with_context(tx, context)?;
    stateful::nullifier_window_valid_with_context(tx, context)?;
    discovery_parameters_valid_with_context(tx, context)?;

    let claimed_anchor_tx = Arc::new(tx.clone());

    validate_claimed_anchor_read_only(
        state.clone(),
        claimed_anchor_tx,
        context.claimed_anchor_cache.clone(),
    )
    .await?;

    for (i, action) in tx.actions().cloned().enumerate() {
        if !action_requires_historical_check(&action) {
            continue;
        }

        let state2 = state.clone();
        let span = action.create_span(i);
        action_checks.spawn(async move { action.check_historical(state2).await }.instrument(span));
    }

    while !action_checks.is_empty() {
        let check = action_checks
            .join_next()
            .await
            .expect("join set must yield while not empty");

        check??;
    }

    Ok(())
}

pub(crate) fn check_historical_with_context_sync(
    tx: &Transaction,
    snapshot: &Snapshot,
    context: &HistoricalCheckContext,
    handle: &tokio::runtime::Handle,
) -> Result<()> {
    ensure_transaction_resource_bounds(tx)?;
    tx_parameters_historical_check_with_context(tx, context)?;
    stateful::nullifier_window_valid_with_context(tx, context)?;
    discovery_parameters_valid_with_context(tx, context)?;

    validate_claimed_anchor_read_only_sync(
        handle,
        snapshot,
        tx,
        context.claimed_anchor_cache.clone(),
    )?;

    Ok(())
}

pub(crate) async fn check_and_execute<S>(artifact: &VerifiedTxArtifact, mut state: S) -> Result<()>
where
    S: StateWrite,
{
    let tx = artifact.tx().as_ref();
    artifact.ensure_historical_coverage()?;

    ensure_transaction_resource_bounds(tx)?;
    let tx_context = tx.context();
    let tx_id = tx.id();
    let action_spans_enabled = tracing::enabled!(tracing::Level::INFO);

    state.put_current_source(Some(tx_id.clone()));

    let gas_used = tx.gas_cost();
    let fee = tx.transaction_body.transaction_parameters.fee;
    state.pay_fee(gas_used, fee).await?;

    // Fee funding is hashed before body actions and validates against the
    // pre-transaction roots used to build its proof. Its effects remain in
    // their original post-body position to preserve commitment ordering.
    let validated_fee_funding = if let Some(fee_funding) = &tx.transaction_body.fee_funding {
        let validated = transfer_validate_verified(
            &fee_funding.transfer,
            &tx_context,
            artifact.proof_for_slot(ProofSlot::FeeFunding)?,
            &mut state,
        )
        .await?;

        Some(validated)
    } else {
        None
    };
    for (i, action) in tx.actions().enumerate() {
        match action {
            Action::Transfer(action) => {
                transfer_execute_verified(
                    action,
                    &tx_context,
                    artifact.proof_for_slot(ProofSlot::BodyAction(i))?,
                    &mut state,
                )
                .await?;
            }
            Action::NoteReshape(action) => {
                note_reshape_execute_verified(
                    action,
                    &tx_context,
                    artifact.proof_for_slot(ProofSlot::BodyAction(i))?,
                    &mut state,
                )
                .await?;
            }
            Action::ShieldedIcs20Withdrawal(action) => {
                shielded_ics20_withdrawal_execute_verified(
                    action,
                    &tx_context,
                    artifact.proof_for_slot(ProofSlot::BodyAction(i))?,
                    &mut state,
                )
                .await?;
            }
            Action::ShieldedHostWithdrawal(action) => {
                shielded_host_withdrawal_execute_verified(
                    action,
                    &tx_context,
                    artifact.proof_for_slot(ProofSlot::BodyAction(i))?,
                    &mut state,
                )
                .await?;
            }
            Action::IbcRelay(action) => {
                let relay = action.clone().with_handler::<Ics20Transfer, ShielddHost>();
                let execute = relay.check_and_execute(&mut state);
                if action_spans_enabled {
                    let span = Action::IbcRelay(action.clone()).create_span(i);
                    execute.instrument(span).await?;
                } else {
                    execute.await?;
                }
            }
            action @ (Action::ComplianceRegisterAsset(_) | Action::ComplianceRegisterUser(_)) => {
                if action_spans_enabled {
                    let span = action.create_span(i);
                    action
                        .check_and_execute(&mut state)
                        .instrument(span)
                        .await?;
                } else {
                    action.check_and_execute(&mut state).await?;
                }
            }
            Action::AggregateBundle(_) => anyhow::bail!(
                "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
            ),
        }
    }
    if let Some(fee_funding) = &tx.transaction_body.fee_funding {
        transfer_execute_validated(
            &fee_funding.transfer,
            &tx_context,
            validated_fee_funding.expect("fee funding validation must exist"),
            &mut state,
        )
        .await?;
    }
    state.stage_routing_actions(transaction_routing_actions(tx)?);

    Ok(())
}

pub(crate) async fn prepare_candidate_read<S: StateRead + 'static>(
    tx: Arc<Transaction>,
    state: Arc<S>,
    context: HistoricalCheckContext,
    skip_historical: bool,
) -> Result<PreparedCandidateRead> {
    let mut prepared = PreparedCandidateRead::default();
    ensure_transaction_resource_bounds(tx.as_ref())?;

    let execution_context = TxExecutionContext {
        block_timestamp: context.block_timestamp,
        source: tx.id(),
    };
    let mut anchor_pairs = BTreeSet::new();
    let mut output_payloads = Vec::new();
    let mut spend_nullifiers = Vec::new();
    let mut tx_nullifiers = HashSet::new();

    for (i, action) in tx.actions().enumerate() {
        match action {
            Action::Transfer(transfer) => {
                check_action_timestamp_freshness(
                    transfer.body.target_timestamp,
                    execution_context.block_timestamp,
                )?;
                for input in &transfer.body.inputs {
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
                        .map(|output| output.note_payload.clone()),
                );
            }
            Action::NoteReshape(note_reshape) => {
                anchor_pairs.insert((
                    note_reshape.body.compliance_anchor,
                    note_reshape.body.asset_anchor,
                ));
                for input in &note_reshape.body.inputs {
                    anyhow::ensure!(
                        tx_nullifiers.insert(input.nullifier),
                        "transaction contains duplicate spend nullifier {}",
                        input.nullifier
                    );
                    spend_nullifiers.push(input.nullifier);
                }
                output_payloads.extend(
                    note_reshape
                        .body
                        .outputs
                        .iter()
                        .map(|output| output.note_payload.clone()),
                );
            }
            _ => anyhow::bail!(
                "parallel prepare only supports transfer and note reshape actions, found unsupported action {:?} at index {}",
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
                .map(|output| output.note_payload.clone()),
        );
    }
    let read_nullifiers = spend_nullifiers.clone();

    let historical_future = async {
        if !skip_historical {
            check_historical_with_context(tx.as_ref(), state.clone(), &context).await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let mut read_tasks = JoinSet::new();
    for (user_anchor, asset_anchor) in anchor_pairs {
        let state = state.clone();
        let anchor_cache = context.anchor_cache.clone();
        let block_height = context.block_height;
        read_tasks.spawn(async move {
            validate_compliance_anchors_read_only(
                state.as_ref(),
                &user_anchor,
                &asset_anchor,
                block_height,
                anchor_cache,
            )
            .await
        });
    }
    for nullifier in read_nullifiers {
        let state = state.clone();
        let context = context.clone();
        read_tasks.spawn(async move {
            check_nullifier_read_only(state.as_ref(), &context, nullifier).await
        });
    }
    let read_task_future = async {
        while let Some(result) = read_tasks.join_next().await {
            result??;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(historical_future, read_task_future)?;

    prepared.spend_nullifiers = spend_nullifiers;
    prepared.sct_payloads = output_payloads
        .into_iter()
        .map(|note_payload| (note_payload, execution_context.source.clone().into()).into())
        .collect();
    prepared.routing_actions = transaction_routing_actions(tx.as_ref())?;

    Ok(prepared)
}

pub(crate) fn prepare_candidate_read_blocking(
    tx: Arc<Transaction>,
    snapshot: Snapshot,
    context: HistoricalCheckContext,
    skip_historical: bool,
    handle: tokio::runtime::Handle,
) -> Result<PreparedCandidateRead> {
    let mut prepared = PreparedCandidateRead::default();
    ensure_transaction_resource_bounds(tx.as_ref())?;

    let execution_context = TxExecutionContext {
        block_timestamp: context.block_timestamp,
        source: tx.id(),
    };
    let mut anchor_pairs = BTreeSet::new();
    let mut output_payloads = Vec::new();
    let mut spend_nullifiers = Vec::new();
    let mut tx_nullifiers = HashSet::new();

    for (i, action) in tx.actions().enumerate() {
        match action {
            Action::Transfer(transfer) => {
                check_action_timestamp_freshness(
                    transfer.body.target_timestamp,
                    execution_context.block_timestamp,
                )?;
                for input in &transfer.body.inputs {
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
                        .map(|output| output.note_payload.clone()),
                );
            }
            Action::NoteReshape(note_reshape) => {
                anchor_pairs.insert((
                    note_reshape.body.compliance_anchor,
                    note_reshape.body.asset_anchor,
                ));
                for input in &note_reshape.body.inputs {
                    anyhow::ensure!(
                        tx_nullifiers.insert(input.nullifier),
                        "transaction contains duplicate spend nullifier {}",
                        input.nullifier
                    );
                    spend_nullifiers.push(input.nullifier);
                }
                output_payloads.extend(
                    note_reshape
                        .body
                        .outputs
                        .iter()
                        .map(|output| output.note_payload.clone()),
                );
            }
            _ => anyhow::bail!(
                "parallel prepare only supports transfer and note reshape actions, found unsupported action {:?} at index {}",
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
                .map(|output| output.note_payload.clone()),
        );
    }
    let read_nullifiers = spend_nullifiers.clone();

    if skip_historical {
    } else {
        check_historical_with_context_sync(Arc::as_ref(&tx), &snapshot, &context, &handle)?;
    }

    for (user_anchor, asset_anchor) in anchor_pairs {
        validate_compliance_anchors_read_only_sync(
            &handle,
            &snapshot,
            &user_anchor,
            &asset_anchor,
            context.block_height,
            context.anchor_cache.clone(),
        )?;
    }
    for nullifier in &read_nullifiers {
        check_nullifier_read_only_sync(&handle, &snapshot, &context, *nullifier)?;
    }

    prepared.spend_nullifiers = spend_nullifiers;
    prepared.sct_payloads = output_payloads
        .into_iter()
        .map(|note_payload| (note_payload, execution_context.source.clone().into()).into())
        .collect();
    prepared.routing_actions = transaction_routing_actions(tx.as_ref())?;

    Ok(prepared)
}

fn check_action_timestamp_freshness(target_timestamp: u64, block_timestamp: u64) -> Result<()> {
    #[cfg(any(test, feature = "benchmark-helpers"))]
    if target_timestamp == 0 && crate::app::benchmark_zero_timestamp_allowed() {
        return Ok(());
    }
    check_timestamp_freshness(
        target_timestamp,
        i64::try_from(block_timestamp).context("block timestamp exceeds i64 range")?,
    )?;
    Ok(())
}

#[async_trait]
impl AppActionHandler for Transaction {
    // We only instrument the top-level `check_stateful`, so we get one span for each transaction.
    #[instrument(skip(self, state))]
    async fn check_historical<S: StateRead + 'static>(&self, state: Arc<S>) -> Result<()> {
        let context = HistoricalCheckContext::load(Arc::as_ref(&state)).await?;
        check_historical_with_context(self, state, &context).await
    }

    // We only instrument the top-level `execute`, so we get one span for each transaction.
    #[instrument(skip(self, _state))]
    async fn check_and_execute<S: StateWrite>(&self, _state: S) -> Result<()> {
        anyhow::bail!("transaction execution requires the canonical verified App pipeline")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    // Serializes tests that read/write SHIELDD_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP to
    // prevent env-var races when tests run in parallel.
    static TIMESTAMP_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    use anyhow::Result;
    use shieldd_sdk_tct as tct;

    use super::{
        transaction_action_count_allowed, transaction_nullifier_count_allowed,
        AnchorValidationCache, ClaimedAnchorValidationCache,
    };

    #[test]
    fn transaction_action_count_policy_is_fixed_at_boundary() {
        assert!(transaction_action_count_allowed(512, false));
        assert!(!transaction_action_count_allowed(513, false));
        assert!(transaction_action_count_allowed(511, true));
        assert!(!transaction_action_count_allowed(512, true));
        assert!(!transaction_action_count_allowed(usize::MAX, true));
    }

    #[test]
    fn transaction_nullifier_count_policy_is_fixed_at_boundary() {
        assert!(transaction_nullifier_count_allowed(256));
        assert!(!transaction_nullifier_count_allowed(257));
        assert!(!transaction_nullifier_count_allowed(usize::MAX));
    }

    #[tokio::test]
    async fn anchor_validation_cache_counts_shared_pair_once() -> Result<()> {
        let cache = Arc::new(AnchorValidationCache::default());
        let key = (
            tct::StateCommitment::try_from([0; 32]).expect("valid commitment"),
            tct::StateCommitment::try_from([1; 32]).expect("valid commitment"),
            100,
        );

        let initializations = Arc::new(AtomicUsize::new(0));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let initializations = initializations.clone();
            tasks.spawn(async move {
                let cell = cache.entry(key);
                let result = cell
                    .get_or_init(|| async {
                        initializations.fetch_add(1, Ordering::Relaxed);
                        Ok::<(), String>(())
                    })
                    .await
                    .clone();
                anyhow::ensure!(result.is_ok(), "cache cell should initialize successfully");
                Ok::<(), anyhow::Error>(())
            });
        }

        while let Some(result) = tasks.join_next().await {
            result??;
        }

        assert_eq!(initializations.load(Ordering::Relaxed), 1);
        assert_eq!(cache.entries.read().unwrap().len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn claimed_anchor_validation_cache_counts_shared_anchor_once() -> Result<()> {
        let cache = Arc::new(ClaimedAnchorValidationCache::default());
        let anchor = shieldd_sdk_tct::Tree::new().root();

        let initializations = Arc::new(AtomicUsize::new(0));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let initializations = initializations.clone();
            tasks.spawn(async move {
                let cell = cache.entry(anchor);
                let result = cell
                    .get_or_init(|| async {
                        initializations.fetch_add(1, Ordering::Relaxed);
                        Ok::<(), String>(())
                    })
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

        assert_eq!(initializations.load(Ordering::Relaxed), 1);
        assert_eq!(cache.entries.read().unwrap().len(), 1);

        Ok(())
    }

    #[test]
    fn zero_timestamp_requires_benchmark_override() {
        let _guard = TIMESTAMP_ENV_MUTEX.lock().unwrap();
        std::env::remove_var("SHIELDD_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP");
        assert!(super::check_action_timestamp_freshness(0, 1_700_000_000).is_err());
    }

    #[test]
    fn zero_timestamp_is_allowed_when_benchmark_override_is_set() {
        let _guard = TIMESTAMP_ENV_MUTEX.lock().unwrap();
        std::env::set_var("SHIELDD_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP", "1");
        let result = super::check_action_timestamp_freshness(0, 1_700_000_000);
        std::env::remove_var("SHIELDD_BENCH_ALLOW_ZERO_TARGET_TIMESTAMP");
        assert!(result.is_ok());
    }

    #[test]
    fn nonzero_timestamps_still_enforce_timestamp_freshness() {
        let _guard = TIMESTAMP_ENV_MUTEX.lock().unwrap();
        assert!(super::check_action_timestamp_freshness(1_700_000_000, 1_700_000_100).is_ok());
        assert!(super::check_action_timestamp_freshness(1_700_000_000, 1_700_003_700).is_err());
    }
}
