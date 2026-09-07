use super::*;

use anyhow::{ensure, Context as _};
use shieldd_sdk_asset::{asset, Value};
use shieldd_sdk_compliance::{
    AuditEffect, AuditEffectRecord, AuditLogWrite as _, AuditSource, ComplianceRegistryRead as _,
    ComplianceRegistryWrite as _, UserAssetStatus, UserAssetStatusAction,
};
use shieldd_sdk_ibc::StateWriteExt as _;
use shieldd_sdk_keys::Address;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::execution_client::v1::{
    apply_compliance_action_request, ApplyComplianceActionRequest, ApplyComplianceActionResponse,
    DepositRequest, DepositResponse, HostSource as ProtoHostSource, SeizeNoteRequest,
};
use shieldd_sdk_sct::component::tree::VerificationExt as _;
use shieldd_sdk_shielded_pool::component::{
    AssetRegistry as _, AssetRegistryRead as _, NoteManager as _,
};
use shieldd_sdk_shielded_pool::{CapsuleReleaseRequest, HostWithdrawalDestination, NoteSeizure};
use std::str::FromStr as _;
use std::time::Instant;

const HOST_ACTION_SOURCE_PREFIX: &str = "application/host_action/source";
const HOST_DEPOSIT_DOMAIN: &[u8] = b"shieldd.host_deposit";
const HOST_COMPLIANCE_ACTION_DOMAIN: &[u8] = b"shieldd.host_compliance_action";
const HOST_NOTE_SEIZURE_DOMAIN: &[u8] = b"shieldd.host_note_seizure";
const HOST_PROPOSER_ADDRESS: [u8; 20] = [0u8; 20];

#[derive(Clone, Debug)]
pub struct HostBlock {
    pub height: i64,
    pub time: Time,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostExecutionPhase {
    Idle,
    InitializedGenesis,
    InitializedCheckpointGenesis,
    InBlock,
    EndedBlock,
}

#[derive(Clone, Debug, Default)]
pub struct HostExecutionResponse {
    pub events: Vec<abci::Event>,
}

#[derive(Clone, Debug, Default)]
pub struct HostTxResponse {
    pub code: u32,
    pub data: Vec<u8>,
    pub log: String,
    pub info: String,
    pub gas_wanted: i64,
    pub gas_used: i64,
    pub events: Vec<abci::Event>,
    pub codespace: String,
    pub withdrawals: Vec<HostWithdrawal>,
}

impl HostTxResponse {
    fn accepted(events: Vec<abci::Event>, withdrawals: Vec<HostWithdrawal>) -> Self {
        Self {
            events,
            withdrawals,
            ..Default::default()
        }
    }

    fn rejected(error: anyhow::Error) -> Self {
        Self {
            code: 1,
            log: format!("{error:#}"),
            codespace: "shieldd".to_owned(),
            ..Default::default()
        }
    }
}

/// Host-chain work emitted by an accepted shielded withdrawal.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct HostWithdrawal {
    pub denom: String,
    pub amount: Amount,
    pub destination: HostWithdrawalDestination,
}

#[derive(Clone, Debug)]
pub struct HostCommit {
    pub root_hash: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostCommittedState {
    pub height: u64,
    pub root_hash: Vec<u8>,
}

/// Drives Shieldd execution from a host chain.
///
/// This adapter uses host-only lifecycle hooks and leaves IBC to the host chain.
pub struct HostExecution {
    storage: Storage,
    app: App,
    stateless_cache: Arc<StatelessCache>,
    phase: HostExecutionPhase,
}

#[derive(Debug)]
pub struct HostDepositResult {
    pub response: DepositResponse,
    pub events: Vec<abci::Event>,
}

#[derive(Debug)]
pub struct HostComplianceActionResult {
    pub response: ApplyComplianceActionResponse,
    pub events: Vec<abci::Event>,
}

#[derive(Debug)]
pub struct HostNoteSeizureResult {
    pub source: ProtoHostSource,
    pub replayed: bool,
    pub withdrawal: HostWithdrawal,
    pub current_status: UserAssetStatus,
    pub freeze_generation: u64,
    pub events: Vec<abci::Event>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct HostActionReceipt {
    request_digest: [u8; 32],
    result: HostActionReceiptResult,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
enum HostActionReceiptResult {
    Deposit {
        deposit_id: [u8; 32],
    },
    Compliance {
        previous_status: UserAssetStatus,
        current_status: UserAssetStatus,
        freeze_generation: u64,
    },
    NoteSeizure {
        withdrawal: HostWithdrawal,
        current_status: UserAssetStatus,
        freeze_generation: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct HostSource {
    height: u64,
    tx_hash: [u8; 32],
    msg_index: u32,
    tx_index: u32,
}

impl HostSource {
    fn validate_height(&self, current_height: u64) -> Result<()> {
        ensure!(
            self.height == current_height,
            "host source height {} does not match active block height {current_height}",
            self.height
        );
        Ok(())
    }

    fn source_key(&self, chain_id: &str) -> String {
        format!(
            "{HOST_ACTION_SOURCE_PREFIX}/{}/{:020}/{:010}/{:010}",
            hex::encode(chain_id.as_bytes()),
            self.height,
            self.tx_index,
            self.msg_index,
        )
    }

    fn hash_into(&self, hasher: &mut sha2::Sha256) {
        hasher.update(self.height.to_be_bytes());
        hasher.update(self.tx_index.to_be_bytes());
        hash_bytes(hasher, &self.tx_hash);
        hasher.update(self.msg_index.to_be_bytes());
    }

    fn audit_source(&self, chain_id: String, effect_index: u32) -> AuditSource {
        AuditSource::Host {
            chain_id,
            height: self.height,
            tx_hash: self.tx_hash,
            tx_index: self.tx_index,
            message_index: self.msg_index,
            effect_index,
        }
    }
}

impl TryFrom<ProtoHostSource> for HostSource {
    type Error = anyhow::Error;

    fn try_from(source: ProtoHostSource) -> Result<Self> {
        let tx_hash = source.tx_hash.try_into().map_err(|bytes: Vec<u8>| {
            anyhow::anyhow!("host source tx_hash must be 32 bytes, got {}", bytes.len())
        })?;
        Ok(Self {
            height: source.height,
            tx_hash,
            msg_index: source.msg_index,
            tx_index: source.tx_index,
        })
    }
}

impl From<HostSource> for ProtoHostSource {
    fn from(source: HostSource) -> Self {
        Self {
            height: source.height,
            tx_hash: source.tx_hash.to_vec(),
            msg_index: source.msg_index,
            tx_index: source.tx_index,
        }
    }
}

impl HostExecution {
    pub fn new(storage: Storage) -> Self {
        Self::with_cache(storage, Arc::new(StatelessCache::new()))
    }

    pub fn with_cache(storage: Storage, stateless_cache: Arc<StatelessCache>) -> Self {
        let mut app = App::new(storage.latest_snapshot());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);

        Self {
            storage,
            app,
            stateless_cache,
            phase: HostExecutionPhase::Idle,
        }
    }

    pub fn phase(&self) -> HostExecutionPhase {
        self.phase
    }

    /// Initializes execution state from content genesis or verifies a checkpoint root.
    ///
    /// Content genesis initializes only host-supported execution components.
    pub async fn init_genesis(&mut self, genesis: AppState) -> Result<()> {
        ensure!(
            self.phase == HostExecutionPhase::Idle,
            "init_genesis called while host execution phase is {:?}",
            self.phase
        );

        match &genesis {
            AppState::Content(_) => {
                ensure!(
                    self.storage.latest_version() == u64::MAX,
                    "database already initialized"
                );
                self.app.init_host_chain(&genesis).await;
                self.phase = HostExecutionPhase::InitializedGenesis;
            }
            AppState::Checkpoint(expected_root_hash) => {
                crate::app_version::check_app_version(&self.storage).await?;
                ensure!(
                    self.storage.latest_version() != u64::MAX,
                    "checkpoint genesis requires initialized storage"
                );
                let expected_root_hash: [u8; 32] = expected_root_hash
                    .as_slice()
                    .try_into()
                    .context("checkpoint genesis root hash must be 32 bytes")?;
                let actual_root_hash = self.storage.latest_snapshot().root_hash().await?;
                ensure!(
                    actual_root_hash.0 == expected_root_hash,
                    "checkpoint genesis root hash does not match storage root"
                );
                self.app.init_host_chain(&genesis).await;
                self.phase = HostExecutionPhase::InitializedCheckpointGenesis;
            }
        }

        Ok(())
    }

    pub async fn export_genesis(&self) -> Result<AppState> {
        ensure!(
            self.phase == HostExecutionPhase::Idle,
            "export_genesis called while host execution phase is {:?}",
            self.phase
        );
        ensure!(
            self.storage.latest_version() != u64::MAX,
            "cannot export genesis from uninitialized storage"
        );

        let root_hash = self.storage.latest_snapshot().root_hash().await?;
        Ok(AppState::Checkpoint(root_hash.0.to_vec()))
    }

    /// Returns the host height and application root from the latest durable state.
    pub async fn committed_state(&self) -> Result<HostCommittedState> {
        ensure!(
            self.storage.latest_version() != u64::MAX,
            "cannot get committed state from uninitialized storage"
        );

        let snapshot = self.storage.latest_snapshot();
        let height = snapshot.get_block_height().await?;
        let root_hash = snapshot.root_hash().await?;
        Ok(HostCommittedState {
            height,
            root_hash: root_hash.0.to_vec(),
        })
    }

    async fn begin_block_request(&self, block: HostBlock) -> Result<request::BeginBlock> {
        ensure!(block.height > 0, "begin_block height must be positive");
        let height = u64::try_from(block.height).context("converting host block height")?;
        let height = block::Height::try_from(height).context("converting host block height")?;
        let chain_id = chain::Id::try_from(self.app.state.get_chain_id().await?)
            .context("parsing host chain id")?;
        let root_hash = self.storage.latest_snapshot().root_hash().await?;
        let app_hash =
            AppHash::try_from(root_hash.0.to_vec()).context("converting host app hash")?;

        Ok(request::BeginBlock {
            hash: Hash::None,
            header: block::Header {
                version: block::header::Version { block: 11, app: 1 },
                chain_id,
                height,
                time: block.time,
                last_block_id: None,
                last_commit_hash: None,
                data_hash: None,
                validators_hash: Hash::None,
                next_validators_hash: Hash::None,
                consensus_hash: Hash::None,
                app_hash,
                last_results_hash: None,
                evidence_hash: None,
                proposer_address: account::Id::new(HOST_PROPOSER_ADDRESS),
            },
            last_commit_info: abci::types::CommitInfo {
                round: 0u8.into(),
                votes: Vec::new(),
            },
            byzantine_validators: Vec::new(),
        })
    }

    /// Starts a host block using an ABCI-shaped request with empty validator data.
    pub async fn begin_block(&mut self, block: HostBlock) -> Result<HostExecutionResponse> {
        ensure!(
            matches!(
                self.phase,
                HostExecutionPhase::Idle | HostExecutionPhase::InitializedCheckpointGenesis
            ),
            "begin_block called while host execution phase is {:?}",
            self.phase
        );

        let begin_block = self.begin_block_request(block).await?;
        let events = self.app.begin_host_block(&begin_block).await;
        self.phase = HostExecutionPhase::InBlock;
        Ok(HostExecutionResponse { events })
    }

    pub async fn deposit(&mut self, deposit: DepositRequest) -> Result<HostDepositResult> {
        ensure!(
            self.phase == HostExecutionPhase::InBlock,
            "deposit called while host execution phase is {:?}",
            self.phase
        );

        self.app.deposit(deposit).await
    }

    pub async fn apply_compliance_action(
        &mut self,
        request: ApplyComplianceActionRequest,
    ) -> Result<HostComplianceActionResult> {
        ensure!(
            self.phase == HostExecutionPhase::InBlock,
            "compliance action called while host execution phase is {:?}",
            self.phase
        );
        self.app.apply_compliance_action(request).await
    }

    pub async fn seize_note(&mut self, request: SeizeNoteRequest) -> Result<HostNoteSeizureResult> {
        ensure!(
            self.phase == HostExecutionPhase::InBlock,
            "note seizure called while host execution phase is {:?}",
            self.phase
        );
        self.app.seize_note(request).await
    }

    pub async fn check_tx(&self, tx_bytes: &[u8]) -> Result<HostTxResponse> {
        ensure!(
            self.storage.latest_version() != u64::MAX,
            "check_tx requires initialized storage"
        );

        let mut app = App::new(self.storage.latest_snapshot());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);

        Ok(
            match app
                .deliver_tx_bytes(tx_bytes, Some(self.stateless_cache.as_ref()))
                .await
            {
                Ok(events) => HostTxResponse::accepted(events, Vec::new()),
                Err(error) => HostTxResponse::rejected(error),
            },
        )
    }

    pub async fn deliver_tx(&mut self, tx_bytes: &[u8]) -> Result<HostTxResponse> {
        ensure!(
            self.phase == HostExecutionPhase::InBlock,
            "deliver_tx called while host execution phase is {:?}",
            self.phase
        );

        let tx = match Transaction::decode_canonical(tx_bytes).context("decoding host transaction")
        {
            Ok(tx) => tx,
            Err(error) => return Ok(HostTxResponse::rejected(error)),
        };
        let withdrawals = match self.resolve_host_withdrawals(&tx).await {
            Ok(withdrawals) => withdrawals,
            Err(error) => return Ok(HostTxResponse::rejected(error)),
        };

        Ok(
            match self
                .app
                .deliver_tx_bytes(tx_bytes, Some(self.stateless_cache.as_ref()))
                .await
            {
                Ok(events) => HostTxResponse::accepted(events, withdrawals),
                Err(error) => HostTxResponse::rejected(error),
            },
        )
    }

    async fn resolve_host_withdrawals(&self, tx: &Transaction) -> Result<Vec<HostWithdrawal>> {
        let mut withdrawals = Vec::new();
        for action in tx.shielded_host_withdrawals() {
            let value = action.body.withdrawal.value;
            let metadata = self
                .app
                .state
                .denom_metadata_by_asset(&value.asset_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("host withdrawal asset is not registered"))?;
            withdrawals.push(HostWithdrawal {
                denom: metadata.base_denom().denom,
                amount: value.amount,
                destination: action.body.withdrawal.destination.clone(),
            });
        }
        Ok(withdrawals)
    }

    /// Finishes the current host block without producing validator set updates.
    pub async fn end_block(&mut self, height: i64) -> Result<HostExecutionResponse> {
        ensure!(
            self.phase == HostExecutionPhase::InBlock,
            "end_block called while host execution phase is {:?}",
            self.phase
        );

        let events = self.app.end_host_block(&request::EndBlock { height }).await;
        self.phase = HostExecutionPhase::EndedBlock;
        Ok(HostExecutionResponse { events })
    }

    /// Commits execution state without standalone chain halt or upgrade handling.
    pub async fn commit(&mut self) -> Result<HostCommit> {
        ensure!(
            matches!(
                self.phase,
                HostExecutionPhase::InitializedGenesis | HostExecutionPhase::EndedBlock
            ),
            "commit called while host execution phase is {:?}",
            self.phase
        );

        let root_hash = self.app.commit_host(self.storage.clone()).await;
        self.phase = HostExecutionPhase::Idle;
        Ok(HostCommit {
            root_hash: root_hash.0.to_vec(),
        })
    }

    pub fn rollback(&mut self) {
        let mut app = App::new(self.storage.latest_snapshot());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        self.app = app;
        self.phase = HostExecutionPhase::Idle;
    }

    /// Drops application snapshots before shutting down Cnidarium and RocksDB.
    pub async fn release(self) {
        let Self {
            storage,
            app,
            stateless_cache,
            phase: _,
        } = self;
        drop(app);
        drop(stateless_cache);
        storage.release().await;
    }
}

impl App {
    /// Initializes the Shieldd execution state for a host-owned chain.
    ///
    /// Unlike `App::init_chain`, this skips IBC state.
    async fn init_host_chain(&mut self, app_state: &AppState) {
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should not be referenced elsewhere");
        match app_state {
            AppState::Content(genesis) => {
                crate::app_version::initialize_app_version(&mut state_tx);
                state_tx.put_chain_id(genesis.chain_id.clone());
                Sct::init_chain(&mut state_tx, Some(&genesis.sct_content)).await;
                ShieldedPool::init_chain(&mut state_tx, Some(&genesis.shielded_pool_content)).await;
                state_tx.put_host_withdrawals_enabled(true);
                FeeComponent::init_chain(&mut state_tx, Some(&genesis.fee_content)).await;
                Compliance::init_chain(&mut state_tx, Some(&genesis.compliance_content)).await;
                state_tx.put_ibc_params(genesis.ibc_content.ibc_params.clone());

                state_tx
                    .finish_block()
                    .await
                    .expect("must be able to finish compact block");
            }
            AppState::Checkpoint(_) => {
                ShieldedPool::init_chain(&mut state_tx, None).await;
                FeeComponent::init_chain(&mut state_tx, None).await;
                Compliance::init_chain(&mut state_tx, None).await;
            }
        };

        state_tx.apply();
    }

    /// Runs per-block hooks for execution components only.
    ///
    /// Unlike `App::begin_block`, this skips IBC hooks.
    async fn begin_host_block(&mut self, begin_block: &request::BeginBlock) -> Vec<abci::Event> {
        self.pending_sct_append_log.clear();
        let mut state_tx = StateDelta::new(self.state.clone());

        clear_block_fee_price_cache(&mut state_tx);

        let mut arc_state_tx = Arc::new(state_tx);
        Sct::begin_block(&mut arc_state_tx, begin_block).await;
        ShieldedPool::begin_block(&mut arc_state_tx, begin_block).await;
        FeeComponent::begin_block(&mut arc_state_tx, begin_block).await;

        let state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");

        self.apply(state_tx)
    }

    /// Flushes host transactions and closes execution-component block and epoch state.
    ///
    /// Unlike `App::end_block`, this skips IBC hooks.
    async fn end_host_block(&mut self, end_block: &request::EndBlock) -> Vec<abci::Event> {
        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions in end_block");
        let mut state_tx = StateDelta::new(self.state.clone());
        self.materialize_pending_sct_append_log(&mut state_tx)
            .await
            .expect("must be able to materialize deferred SCT payloads in end_block");

        tracing::debug!("running host app components' `end_block` hooks");
        let mut arc_state_tx = Arc::new(state_tx);
        Sct::end_block(&mut arc_state_tx, end_block).await;
        ShieldedPool::end_block(&mut arc_state_tx, end_block).await;
        FeeComponent::end_block(&mut arc_state_tx, end_block).await;
        Compliance::end_block(&mut arc_state_tx, end_block).await;
        let mut state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");
        tracing::debug!("finished host app components' `end_block` hooks");

        let current_height = state_tx
            .get_block_height()
            .await
            .expect("able to get block height in end_block");
        let current_epoch = state_tx
            .get_current_epoch()
            .await
            .expect("able to get current epoch in end_block");

        let is_end_epoch = current_epoch.is_scheduled_epoch_end(
            current_height,
            state_tx
                .get_epoch_duration_parameter()
                .await
                .expect("able to get epoch duration in end_block"),
        ) || state_tx.is_epoch_ending_early().await;

        if is_end_epoch {
            tracing::info!(?current_height, "ending host epoch");

            let mut arc_state_tx = Arc::new(state_tx);

            Sct::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Sct component");
            ShieldedPool::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on shielded pool component");
            FeeComponent::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Fee component");

            let mut state_tx = Arc::try_unwrap(arc_state_tx)
                .expect("components did not retain copies of shared state");

            state_tx
                .finish_epoch()
                .await
                .expect("must be able to finish compact block");

            shieldd_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
                &mut state_tx,
                current_height + 1,
                Epoch {
                    index: current_epoch.index + 1,
                    start_height: current_height + 1,
                },
            );

            self.apply(state_tx)
        } else {
            shieldd_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
                &mut state_tx,
                current_height + 1,
                current_epoch,
            );

            state_tx
                .finish_block()
                .await
                .expect("must be able to finish compact block");

            self.apply(state_tx)
        }
    }

    /// Persists host execution state and resets snapshots for the next host call.
    ///
    /// Unlike `App::commit`, this does not enforce chain halt or pre-upgrade exits.
    async fn commit_host(&mut self, storage: Storage) -> RootHash {
        self.state
            .ensure_nullifier_block_materialized()
            .expect("cannot commit an open nullifier block");
        let commit_start = Instant::now();
        let flush_start = Instant::now();
        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions before commit");
        let flush_ms = flush_start.elapsed().as_secs_f64() * 1000.0;
        let dummy_state = StateDelta::new(storage.latest_snapshot());
        let state = Arc::try_unwrap(std::mem::replace(&mut self.state, Arc::new(dummy_state)))
            .expect("we have exclusive ownership of the State at commit()");

        let halt_check_ms = 0.0;

        let storage_commit_start = Instant::now();
        let jmt_root = storage
            .commit(state)
            .await
            .expect("must be able to successfully commit to storage");
        let storage_commit_ms = storage_commit_start.elapsed().as_secs_f64() * 1000.0;

        tracing::debug!(?jmt_root, "finished committing host state");

        let snapshot_reset_start = Instant::now();
        let latest_snapshot = storage.latest_snapshot();
        self.committed_snapshot = latest_snapshot.clone();
        self.state = Arc::new(StateDelta::new(latest_snapshot));
        self.pending_sct_append_log.clear();
        let snapshot_reset_ms = snapshot_reset_start.elapsed().as_secs_f64() * 1000.0;
        let total_ms = commit_start.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            commit_total_ms = total_ms,
            commit_flush_deferred_ms = flush_ms,
            commit_halt_check_ms = halt_check_ms,
            commit_storage_commit_ms = storage_commit_ms,
            commit_snapshot_reset_ms = snapshot_reset_ms,
            "host_commit_phase_profile"
        );
        jmt_root
    }

    pub async fn deposit(&mut self, deposit: DepositRequest) -> Result<HostDepositResult> {
        let mut state_tx = StateDelta::new(self.state.clone());
        let chain_id = state_tx.get_chain_id().await?;
        let parsed = ParsedHostDeposit::parse(chain_id, deposit)?;
        let source_key = parsed.source_key();
        let current_height = state_tx.get_block_height().await?;
        parsed.source.validate_height(current_height)?;

        if let Some(receipt) = load_host_action_receipt(&state_tx, &source_key).await? {
            ensure!(
                receipt.request_digest == parsed.deposit_id,
                "host source was already used by a different request"
            );
            let HostActionReceiptResult::Deposit { deposit_id } = receipt.result else {
                anyhow::bail!("host source was already used by a different action kind");
            };
            return Ok(HostDepositResult {
                response: DepositResponse {
                    deposit_id: deposit_id.to_vec(),
                },
                events: Vec::new(),
            });
        }

        let deposit_id = parsed.deposit_id;
        store_host_action_receipt(
            &mut state_tx,
            source_key,
            &HostActionReceipt {
                request_digest: parsed.deposit_id,
                result: HostActionReceiptResult::Deposit { deposit_id },
            },
        )?;

        state_tx.register_denom(&parsed.denom).await;
        if state_tx.is_asset_regulated(parsed.value().asset_id).await? {
            let leaf = state_tx
                .get_user_leaf(&parsed.recipient, parsed.value().asset_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("regulated deposit recipient is not registered"))?;
            ensure!(
                leaf.status == UserAssetStatus::Active,
                "regulated deposit recipient is not active"
            );
        }
        state_tx
            .mint_note(
                parsed.value(),
                &parsed.recipient,
                // TODO(fdymylja): understand host-deposit indexing. This is a
                // bankd tx effect, but compact blocks currently strip tx ids and
                // clients do not know how to fetch the corresponding bankd tx.
                CommitmentSource::Transaction {
                    id: Some(deposit_id),
                },
            )
            .await
            .context("minting host deposit note")?;
        state_tx
            .append_audit_effect(AuditEffectRecord {
                source: parsed.source.audit_source(parsed.chain_id.clone(), 0),
                effect: AuditEffect::PublicDeposit {
                    asset_id: parsed.value().asset_id,
                    amount: parsed.amount.value(),
                    recipient: parsed.recipient.clone(),
                },
            })
            .await?;

        let events = self.apply(state_tx);
        Ok(HostDepositResult {
            response: DepositResponse {
                deposit_id: deposit_id.to_vec(),
            },
            events,
        })
    }

    pub async fn apply_compliance_action(
        &mut self,
        request: ApplyComplianceActionRequest,
    ) -> Result<HostComplianceActionResult> {
        let mut state_tx = StateDelta::new(self.state.clone());
        let chain_id = state_tx.get_chain_id().await?;
        let parsed = ParsedHostComplianceAction::parse(chain_id, request)?;
        let current_height = state_tx.get_block_height().await?;
        parsed.source.validate_height(current_height)?;
        let source_key = parsed.source_key();

        if let Some(receipt) = load_host_action_receipt(&state_tx, &source_key).await? {
            ensure!(
                receipt.request_digest == parsed.request_digest,
                "host source was already used by a different request"
            );
            let HostActionReceiptResult::Compliance {
                previous_status,
                current_status,
                freeze_generation,
            } = receipt.result
            else {
                anyhow::bail!("host source was already used by a different action kind");
            };
            return Ok(HostComplianceActionResult {
                response: compliance_action_response(
                    &parsed.source,
                    previous_status,
                    current_status,
                    freeze_generation,
                    true,
                ),
                events: Vec::new(),
            });
        }

        let event = state_tx
            .apply_user_status_action(
                &parsed.address,
                parsed.asset_id,
                parsed.action,
                parsed.source.height,
            )
            .await?;
        let current_status = event.leaf.status;
        let freeze_generation = event.leaf.freeze_generation;
        state_tx
            .append_audit_effect(AuditEffectRecord {
                source: parsed.source.audit_source(parsed.chain_id.clone(), 0),
                effect: AuditEffect::UserStatusChanged {
                    asset_id: parsed.asset_id,
                    address: parsed.address.clone(),
                    status: event.leaf.status,
                    freeze_generation: event.leaf.freeze_generation,
                    frozen_since_height: event.leaf.frozen_since_height,
                },
            })
            .await?;
        store_host_action_receipt(
            &mut state_tx,
            source_key,
            &HostActionReceipt {
                request_digest: parsed.request_digest,
                result: HostActionReceiptResult::Compliance {
                    previous_status: event.previous_status,
                    current_status,
                    freeze_generation,
                },
            },
        )?;

        let response = compliance_action_response(
            &parsed.source,
            event.previous_status,
            current_status,
            freeze_generation,
            false,
        );
        let events = self.apply(state_tx);
        Ok(HostComplianceActionResult { response, events })
    }

    pub async fn seize_note(&mut self, request: SeizeNoteRequest) -> Result<HostNoteSeizureResult> {
        let mut state_tx = StateDelta::new(self.state.clone());
        let chain_id = state_tx.get_chain_id().await?;
        let parsed = ParsedNoteSeizure::parse(chain_id, request)?;
        let current_height = state_tx.get_block_height().await?;
        parsed.source.validate_height(current_height)?;
        let source_key = parsed.source_key();

        if let Some(receipt) = load_host_action_receipt(&state_tx, &source_key).await? {
            ensure!(
                receipt.request_digest == parsed.request_digest,
                "host source was already used by a different request"
            );
            let HostActionReceiptResult::NoteSeizure {
                withdrawal,
                current_status,
                freeze_generation,
            } = receipt.result
            else {
                anyhow::bail!("host source was already used by a different action kind");
            };
            return Ok(HostNoteSeizureResult {
                source: parsed.source.into(),
                replayed: true,
                withdrawal,
                current_status,
                freeze_generation,
                events: Vec::new(),
            });
        }

        let seizure = &parsed.seizure;
        let authorization = &seizure.authorization;
        ensure!(
            authorization.chain_id == parsed.chain_id,
            "note seizure authorization is for a different chain"
        );
        ensure!(
            current_height <= authorization.expiry_height,
            "note seizure authorization has expired"
        );

        let policy = state_tx
            .get_asset_policy(authorization.asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("note seizure asset is not regulated"))?;
        let seizure_authority = policy
            .seizure_authority_vk
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("regulated asset has no seizure authority"))?;
        authorization.verify_signature(seizure_authority, &seizure.authority_signature)?;

        let leaf = state_tx
            .get_user_leaf(&authorization.address, authorization.asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("note seizure address is not registered for asset"))?;
        ensure!(
            matches!(
                leaf.status,
                UserAssetStatus::Frozen | UserAssetStatus::Seized
            ),
            "note seizure target is not frozen or seized"
        );
        ensure!(
            leaf.freeze_generation == authorization.freeze_generation
                && leaf.frozen_since_height == authorization.frozen_since_height,
            "note seizure authorization does not match the current freeze generation"
        );
        ensure!(
            leaf.rnk_commitment == seizure.rnk_commitment,
            "note seizure RNK commitment differs from the current compliance leaf"
        );

        let current_window = shieldd_sdk_sct::nullifier_tree::generation_state(&state_tx)
            .await?
            .window();
        ensure!(
            seizure.nullifier_window == current_window,
            "note seizure nullifier window is stale"
        );
        state_tx.check_claimed_anchor(seizure.anchor).await?;
        if let Some(history) = seizure.historical_nullifier_proof.as_ref() {
            verify_historical_nullifier_proof(authorization.nullifier, current_window, history)?;
        }

        let authorization_commitment = authorization.commitment()?;
        let release = CapsuleReleaseRequest {
            chain_id: authorization.chain_id.clone(),
            ring_id: policy.ring.ring_id.clone(),
            policy_id: policy.ring.policy_id.clone(),
            permission: policy.ring.permission.clone(),
            resource: policy.ring.resource.clone(),
            ring_pk: policy.ring.ring_pk,
            asset_id: authorization.asset_id,
            address: authorization.address.clone(),
            capk: leaf.capk,
            note_commitment: authorization.note_commitment,
            recovery_commitment: seizure.recovery_capsule.commitment(),
            capsule_epk: seizure.recovery_capsule.epk,
            authority_instruction_commitment: authorization_commitment,
            expiry_height: authorization.expiry_height,
        };
        let recovered_shared = seizure.capsule_release.verify(&release)?;
        let recovery_seed =
            seizure.recovery_capsule.c2 - recovered_shared.vartime_compress_to_field();

        seizure.proof.verify(&seizure.proof_public(recovery_seed))?;
        state_tx
            .check_nullifier_unspent(authorization.nullifier)
            .await?;

        let metadata = state_tx
            .denom_metadata_by_asset(&authorization.asset_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("note seizure asset has no host denomination"))?;
        let withdrawal = HostWithdrawal {
            denom: metadata.base_denom().denom,
            amount: authorization.amount,
            destination: authorization.withdrawal.destination.clone(),
        };
        let lifecycle = state_tx
            .admit_note_seizure(
                &authorization.address,
                authorization.asset_id,
                authorization.freeze_generation,
                authorization.frozen_since_height,
            )
            .await?;
        state_tx
            .nullify(
                authorization.nullifier,
                CommitmentSource::Transaction {
                    id: Some(parsed.request_digest),
                },
            )
            .await?;
        state_tx
            .append_audit_effect(AuditEffectRecord {
                source: parsed.source.audit_source(parsed.chain_id.clone(), 0),
                effect: AuditEffect::NoteSeized {
                    asset_id: authorization.asset_id,
                    address: authorization.address.clone(),
                    nullifier: authorization.nullifier.into(),
                    amount: authorization.amount.value(),
                    freeze_generation: authorization.freeze_generation,
                    authorization_commitment,
                },
            })
            .await?;
        if lifecycle.status_change.is_some() {
            state_tx
                .append_audit_effect(AuditEffectRecord {
                    source: parsed.source.audit_source(parsed.chain_id.clone(), 1),
                    effect: AuditEffect::UserStatusChanged {
                        asset_id: lifecycle.leaf.asset_id,
                        address: lifecycle.leaf.address.clone(),
                        status: lifecycle.leaf.status,
                        freeze_generation: lifecycle.leaf.freeze_generation,
                        frozen_since_height: lifecycle.leaf.frozen_since_height,
                    },
                })
                .await?;
        }
        store_host_action_receipt(
            &mut state_tx,
            source_key,
            &HostActionReceipt {
                request_digest: parsed.request_digest,
                result: HostActionReceiptResult::NoteSeizure {
                    withdrawal: withdrawal.clone(),
                    current_status: lifecycle.leaf.status,
                    freeze_generation: lifecycle.leaf.freeze_generation,
                },
            },
        )?;

        let result = HostNoteSeizureResult {
            source: parsed.source.into(),
            replayed: false,
            withdrawal,
            current_status: lifecycle.leaf.status,
            freeze_generation: lifecycle.leaf.freeze_generation,
            events: self.apply(state_tx),
        };
        Ok(result)
    }
}

struct ParsedHostDeposit {
    chain_id: String,
    source: HostSource,
    denom: asset::Metadata,
    amount: Amount,
    recipient: Address,
    deposit_id: [u8; 32],
}

impl ParsedHostDeposit {
    fn parse(chain_id: String, deposit: DepositRequest) -> Result<Self> {
        let source = deposit
            .source
            .context("host deposit source is required")?
            .try_into()
            .context("invalid host deposit source")?;

        let denom: asset::Metadata = deposit
            .denom
            .as_str()
            .try_into()
            .with_context(|| format!("invalid host deposit denom {}", deposit.denom))?;
        let amount = Amount::try_from(deposit.amount.clone())
            .with_context(|| format!("invalid host deposit amount {}", deposit.amount))?;
        ensure!(
            amount > Amount::zero(),
            "host deposit amount must be nonzero"
        );
        let recipient = Address::from_str(&deposit.recipient)
            .with_context(|| "invalid host deposit recipient address")?;

        let deposit_id = derive_deposit_id(&chain_id, &source, &denom, amount, &recipient);

        Ok(Self {
            chain_id,
            source,
            denom,
            amount,
            recipient,
            deposit_id,
        })
    }

    fn value(&self) -> Value {
        Value {
            amount: self.amount,
            asset_id: self.denom.id(),
        }
    }

    fn source_key(&self) -> String {
        self.source.source_key(&self.chain_id)
    }
}

struct ParsedHostComplianceAction {
    chain_id: String,
    source: HostSource,
    address: Address,
    asset_id: asset::Id,
    action: UserAssetStatusAction,
    request_digest: [u8; 32],
}

impl ParsedHostComplianceAction {
    fn parse(chain_id: String, request: ApplyComplianceActionRequest) -> Result<Self> {
        let source = request
            .source
            .context("host compliance action source is required")?
            .try_into()
            .context("invalid host compliance action source")?;
        let (address, asset_id, action) = match request
            .action
            .context("host compliance action is required")?
        {
            apply_compliance_action_request::Action::Freeze(action) => (
                action
                    .address
                    .context("freeze address is required")?
                    .try_into()
                    .context("invalid freeze address")?,
                action
                    .asset_id
                    .context("freeze asset ID is required")?
                    .try_into()
                    .context("invalid freeze asset ID")?,
                UserAssetStatusAction::Freeze,
            ),
            apply_compliance_action_request::Action::Unfreeze(action) => (
                action
                    .address
                    .context("unfreeze address is required")?
                    .try_into()
                    .context("invalid unfreeze address")?,
                action
                    .asset_id
                    .context("unfreeze asset ID is required")?
                    .try_into()
                    .context("invalid unfreeze asset ID")?,
                UserAssetStatusAction::Unfreeze,
            ),
        };
        let request_digest =
            derive_compliance_action_digest(&chain_id, &source, &address, asset_id, action);
        Ok(Self {
            chain_id,
            source,
            address,
            asset_id,
            action,
            request_digest,
        })
    }

    fn source_key(&self) -> String {
        self.source.source_key(&self.chain_id)
    }
}

struct ParsedNoteSeizure {
    chain_id: String,
    source: HostSource,
    seizure: NoteSeizure,
    request_digest: [u8; 32],
}

impl ParsedNoteSeizure {
    fn parse(chain_id: String, request: SeizeNoteRequest) -> Result<Self> {
        let source = request
            .source
            .context("host note seizure source is required")?
            .try_into()
            .context("invalid host note seizure source")?;
        let seizure_proto = request
            .seizure
            .context("host note seizure evidence is required")?;
        let request_digest =
            derive_note_seizure_digest(&chain_id, &source, &seizure_proto.encode_to_vec());
        let seizure = seizure_proto
            .try_into()
            .context("invalid host note seizure evidence")?;
        Ok(Self {
            chain_id,
            source,
            seizure,
            request_digest,
        })
    }

    fn source_key(&self) -> String {
        self.source.source_key(&self.chain_id)
    }
}

async fn load_host_action_receipt<S: StateRead + ?Sized>(
    state: &S,
    key: &str,
) -> Result<Option<HostActionReceipt>> {
    state
        .get_raw(key)
        .await?
        .map(|bytes| serde_json::from_slice(&bytes).context("decoding host action receipt"))
        .transpose()
}

fn store_host_action_receipt<S: StateWrite + ?Sized>(
    state: &mut S,
    key: String,
    receipt: &HostActionReceipt,
) -> Result<()> {
    state.put_raw(
        key,
        serde_json::to_vec(receipt).context("encoding host action receipt")?,
    );
    Ok(())
}

fn compliance_action_response(
    source: &HostSource,
    previous_status: UserAssetStatus,
    current_status: UserAssetStatus,
    freeze_generation: u64,
    replayed: bool,
) -> ApplyComplianceActionResponse {
    ApplyComplianceActionResponse {
        source: Some(source.clone().into()),
        previous_status: shieldd_sdk_proto::core::component::compliance::v1::UserAssetStatus::from(
            previous_status,
        ) as i32,
        current_status: shieldd_sdk_proto::core::component::compliance::v1::UserAssetStatus::from(
            current_status,
        ) as i32,
        replayed,
        freeze_generation,
    }
}

fn derive_deposit_id(
    chain_id: &str,
    source: &HostSource,
    denom: &asset::Metadata,
    amount: Amount,
    recipient: &Address,
) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hash_bytes(&mut hasher, HOST_DEPOSIT_DOMAIN);
    hash_bytes(&mut hasher, chain_id.as_bytes());
    source.hash_into(&mut hasher);
    hash_bytes(&mut hasher, denom.to_string().as_bytes());
    hash_bytes(&mut hasher, amount.to_string().as_bytes());
    hash_bytes(&mut hasher, recipient.to_string().as_bytes());
    hasher.finalize().into()
}

fn derive_compliance_action_digest(
    chain_id: &str,
    source: &HostSource,
    address: &Address,
    asset_id: asset::Id,
    action: UserAssetStatusAction,
) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hash_bytes(&mut hasher, HOST_COMPLIANCE_ACTION_DOMAIN);
    hash_bytes(&mut hasher, chain_id.as_bytes());
    source.hash_into(&mut hasher);
    hash_bytes(&mut hasher, &address.to_vec());
    hash_bytes(&mut hasher, &asset_id.0.to_bytes());
    hasher.update([match action {
        UserAssetStatusAction::Freeze => 1,
        UserAssetStatusAction::Unfreeze => 2,
    }]);
    hasher.finalize().into()
}

fn derive_note_seizure_digest(chain_id: &str, source: &HostSource, seizure: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hash_bytes(&mut hasher, HOST_NOTE_SEIZURE_DOMAIN);
    hash_bytes(&mut hasher, chain_id.as_bytes());
    source.hash_into(&mut hasher);
    hash_bytes(&mut hasher, seizure);
    hasher.finalize().into()
}

fn hash_bytes(hasher: &mut sha2::Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{AppState, Content};
    use crate::SUBSTORE_PREFIXES;
    use cnidarium::TempStorage;
    use cnidarium_component::ActionHandler as _;
    use decaf377::{Element, Fr};
    use decaf377_rdsa::{SigningKey, SpendAuth};
    use shieldd_sdk_asset::BASE_ASSET_DENOM;
    use shieldd_sdk_compliance::{
        compliance_nullifier_key_commitment, encrypt_withdrawal_with_material, AssetPolicy,
        AuditLogRead as _, ComplianceLeaf, UNREGULATED_SINK_RING_PK,
    };
    use shieldd_sdk_keys::keys::NullifierKey;
    use shieldd_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_proto::execution_client::v1::{FreezeUserAsset, UnfreezeUserAsset};
    use shieldd_sdk_sct::component::tree::{SctManager as _, SctRead as _};
    use shieldd_sdk_shielded_pool::component::StateReadExt as _;
    use shieldd_sdk_shielded_pool::gnark::GnarkNoteSeizureClient;
    use shieldd_sdk_shielded_pool::{
        CapsuleReleaseEvidence, CapsuleReleaseRequest, EvmCall,
        HostExecution as DomainHostExecution, HostTransfer, HostWithdrawal as DomainHostWithdrawal,
        NotePayload, NoteSeizure, NoteSeizureAuthorizationBody, NoteSeizureProofPrivate,
        NoteSeizureProofPublic, RecoveryCapsule, Rseed, ShieldedHostWithdrawal,
        ShieldedHostWithdrawalBody, ShieldedIcs20WithdrawalChangeBody,
        ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalProof,
    };
    use shieldd_sdk_tct as tct;
    use std::ops::Deref as _;

    async fn temp_storage() -> TempStorage {
        TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec())
            .await
            .expect("temp storage")
    }

    fn host_genesis() -> AppState {
        AppState::Content(Content::default().with_chain_id("bankd-local".to_owned()))
    }

    fn host_block(height: i64) -> HostBlock {
        HostBlock {
            height,
            time: Time::from_unix_timestamp(1_700_000_000, 0).expect("valid host block time"),
        }
    }

    fn host_source_at(height: u64, msg_index: u32) -> ProtoHostSource {
        ProtoHostSource {
            height,
            tx_hash: [7u8; 32].to_vec(),
            tx_index: 0,
            msg_index,
        }
    }

    fn host_source(msg_index: u32) -> ProtoHostSource {
        host_source_at(1, msg_index)
    }

    fn deposit_request(msg_index: u32) -> DepositRequest {
        DepositRequest {
            denom: BASE_ASSET_DENOM.to_string(),
            amount: "100".to_owned(),
            recipient: test_keys::ADDRESS_0.to_string(),
            source: Some(host_source(msg_index)),
        }
    }

    fn regulated_test_denom() -> asset::Metadata {
        "host_regulated_test_asset"
            .try_into()
            .expect("custom test denom should parse as a base denom")
    }

    fn regulated_deposit_request(msg_index: u32) -> DepositRequest {
        DepositRequest {
            denom: regulated_test_denom().to_string(),
            ..deposit_request(msg_index)
        }
    }

    fn compliance_request(
        source: ProtoHostSource,
        action: UserAssetStatusAction,
    ) -> ApplyComplianceActionRequest {
        let address = Some(test_keys::ADDRESS_0.deref().clone().into());
        let asset_id = Some(regulated_test_denom().id().into());
        let action = match action {
            UserAssetStatusAction::Freeze => {
                apply_compliance_action_request::Action::Freeze(FreezeUserAsset {
                    address,
                    asset_id,
                })
            }
            UserAssetStatusAction::Unfreeze => {
                apply_compliance_action_request::Action::Unfreeze(UnfreezeUserAsset {
                    address,
                    asset_id,
                })
            }
        };
        ApplyComplianceActionRequest {
            source: Some(source),
            action: Some(action),
        }
    }

    async fn register_regulated_test_user(host: &mut HostExecution) -> Result<()> {
        let mut state_tx = StateDelta::new(host.app.state.clone());
        let asset_id = regulated_test_denom().id();
        state_tx
            .test_only_register_asset(
                asset_id,
                AssetPolicy::for_test(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
                true,
            )
            .await?;
        state_tx
            .test_only_add_compliance_leaf(ComplianceLeaf::registered_for_test(
                test_keys::ADDRESS_0.deref().clone(),
                asset_id,
            ))
            .await?;
        host.app.apply(state_tx);
        Ok(())
    }

    fn host_withdrawal_action() -> ShieldedHostWithdrawal {
        ShieldedHostWithdrawal {
            body: ShieldedHostWithdrawalBody {
                family_id: ShieldedIcs20WithdrawalFamilyId::Canonical,
                anchor: shieldd_sdk_tct::Tree::default().root(),
                balance_commitment: Default::default(),
                inputs: Vec::new(),
                withdrawal: DomainHostWithdrawal {
                    value: Value {
                        amount: 42u64.into(),
                        asset_id: BASE_ASSET_DENOM.id(),
                    },
                    destination: HostWithdrawalDestination::Transfer(HostTransfer {
                        recipient: "bank1recipient".to_owned(),
                    }),
                },
                change_output: ShieldedIcs20WithdrawalChangeBody {
                    note_payload: NotePayload::dummy(),
                    wrapped_memo_key: WrappedMemoKey([0u8; 48]),
                    ovk_wrapped_key: OvkWrappedKey([0u8; 48]),
                },
                target_timestamp: 0,
                compliance_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(0u64)),
                asset_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(0u64)),
                routing_tag: Default::default(),
                routing_parameter_set_id: decaf377::Fq::from(0u64),
                withdrawal_compliance_ciphertext: encrypt_withdrawal_with_material(
                    *UNREGULATED_SINK_RING_PK,
                    test_keys::ADDRESS_0.deref(),
                    decaf377::Fq::from(1u64),
                    decaf377::Fr::from(1u64),
                )
                .expect("valid test withdrawal compliance ciphertext")
                .ciphertext,
                volume_accumulator:
                    shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding(),
            },
            auth_sigs: Vec::new(),
            proof: ShieldedIcs20WithdrawalProof::default(),
        }
    }

    fn host_withdrawal_transaction() -> Transaction {
        let mut tx = Transaction::default();
        tx.transaction_body.actions =
            vec![Action::ShieldedHostWithdrawal(host_withdrawal_action())];
        tx
    }

    #[tokio::test]
    async fn note_seizure_receipt_survives_storage_roundtrip() -> Result<()> {
        let storage = temp_storage().await;
        let mut state = StateDelta::new(storage.latest_snapshot());
        let receipt = HostActionReceipt {
            request_digest: [7u8; 32],
            result: HostActionReceiptResult::NoteSeizure {
                withdrawal: HostWithdrawal {
                    denom: regulated_test_denom().to_string(),
                    amount: Amount::from(42u64),
                    destination: HostWithdrawalDestination::Transfer(HostTransfer {
                        recipient: "bank1seizureauthority".to_owned(),
                    }),
                },
                current_status: UserAssetStatus::Seized,
                freeze_generation: 1,
            },
        };
        store_host_action_receipt(&mut state, "test/receipt".to_owned(), &receipt)?;
        storage.commit(state).await?;
        let loaded = load_host_action_receipt(&storage.latest_snapshot(), "test/receipt")
            .await?
            .context("stored receipt")?;
        assert_eq!(loaded.request_digest, receipt.request_digest);
        let HostActionReceiptResult::NoteSeizure {
            withdrawal,
            current_status,
            freeze_generation,
        } = loaded.result
        else {
            anyhow::bail!("wrong receipt kind");
        };
        assert_eq!(withdrawal.amount, Amount::from(42u64));
        assert_eq!(current_status, UserAssetStatus::Seized);
        assert_eq!(freeze_generation, 1);
        Ok(())
    }

    #[tokio::test]
    async fn deposit_mints_note_and_exact_replay_returns_same_result() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());
        host.init_genesis(host_genesis()).await?;
        host.commit().await?;
        host.begin_block(host_block(1)).await?;

        let mut request = deposit_request(0);
        request.source = Some(host_source_at(1, 0));
        let first = host.deposit(request.clone()).await?;
        assert_eq!(first.response.deposit_id.len(), 32);
        assert!(!first.events.is_empty());

        let replay = host.deposit(request).await?;
        assert_eq!(replay.response.deposit_id, first.response.deposit_id);
        assert!(replay.events.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn compliance_actions_are_typed_atomic_and_replay_safe() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());
        host.init_genesis(host_genesis()).await?;
        host.commit().await?;
        host.begin_block(host_block(1)).await?;
        register_regulated_test_user(&mut host).await?;

        let failed_source = host_source_at(1, 0);
        let failed = host
            .apply_compliance_action(compliance_request(
                failed_source.clone(),
                UserAssetStatusAction::Unfreeze,
            ))
            .await
            .expect_err("an active user cannot be unfrozen");
        assert!(failed.to_string().contains("only a frozen user asset"));

        let first = host
            .apply_compliance_action(compliance_request(
                failed_source.clone(),
                UserAssetStatusAction::Freeze,
            ))
            .await?;
        assert_eq!(first.response.source, Some(failed_source.clone()));
        assert_eq!(first.response.previous_status, 1);
        assert_eq!(first.response.current_status, 2);
        assert!(!first.response.replayed);

        let replay = host
            .apply_compliance_action(compliance_request(
                failed_source.clone(),
                UserAssetStatusAction::Freeze,
            ))
            .await?;
        assert_eq!(replay.response.source, Some(failed_source));
        assert!(replay.response.replayed);
        assert!(replay.events.is_empty());

        let conflict = host
            .apply_compliance_action(compliance_request(
                host_source_at(1, 0),
                UserAssetStatusAction::Unfreeze,
            ))
            .await
            .expect_err("a host source cannot name a different successful action");
        assert!(conflict.to_string().contains("different request"));

        let frozen_deposit = host.deposit(regulated_deposit_request(1)).await;
        assert!(frozen_deposit
            .expect_err("regulated deposits to frozen users must fail")
            .to_string()
            .contains("not active"));

        let unfreeze = host
            .apply_compliance_action(compliance_request(
                host_source_at(1, 2),
                UserAssetStatusAction::Unfreeze,
            ))
            .await?;
        assert_eq!(unfreeze.response.previous_status, 2);
        assert_eq!(unfreeze.response.current_status, 1);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    async fn note_seizure_verifies_capsule_release_and_commits_once() -> Result<()> {
        shieldd_sdk_shielded_pool::gnark::require_proof_test_runtime(
            shieldd_sdk_shielded_pool::gnark::ProofTestFamily::NoteSeizure,
        )?;

        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());
        host.init_genesis(host_genesis()).await?;
        host.commit().await?;
        host.begin_block(host_block(1)).await?;

        let address = test_keys::ADDRESS_0.deref().clone();
        let denom = regulated_test_denom();
        let asset_id = denom.id();
        let amount = Amount::from(42u64);
        let capability_secret = Fr::from_le_bytes_mod_order(
            &shieldd_sdk_compliance::derive_compliance_scalar(&address).to_bytes(),
        );
        let capk = Element::GENERATOR * capability_secret;
        let rnk = decaf377::Fq::from(1u64);
        let authority_sk = SigningKey::<SpendAuth>::from(Fr::from(1u64));
        let policy = AssetPolicy::for_test(Element::GENERATOR, u128::MAX, Element::GENERATOR);
        let leaf = ComplianceLeaf::registered_for_test(address.clone(), asset_id);
        assert_eq!(leaf.capk, capk);

        let rseed = Rseed([17u8; 32]);
        let note_blinding = rseed.derive_note_blinding();
        let (recovery_capsule, opening) =
            RecoveryCapsule::encrypt(amount, note_blinding, capk, rseed)?;
        let note_commitment = shieldd_sdk_shielded_pool::note::commitment_from_address(
            address.clone(),
            Value { amount, asset_id },
            note_blinding,
            recovery_capsule.commitment(),
        )?;

        let mut state_tx = StateDelta::new(host.app.state.clone());
        state_tx.register_denom(&denom).await;
        state_tx
            .test_only_register_asset(asset_id, policy.clone(), true)
            .await?;
        state_tx.test_only_add_compliance_leaf(leaf).await?;
        let leaf = state_tx
            .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Freeze, 1)
            .await?
            .leaf;
        let mut witness_tree = state_tx.get_sct().await;
        witness_tree.insert(tct::Witness::Keep, note_commitment)?;
        let block_root = witness_tree.end_block()?;
        let state_commitment_proof = witness_tree
            .witness(note_commitment)
            .context("witnessing the seized note")?;
        state_tx.write_sct(1, witness_tree, block_root, None).await;
        let nullifier_window = shieldd_sdk_sct::nullifier_tree::generation_state(&state_tx)
            .await?
            .window();
        host.app.apply(state_tx);

        let nullifier = Nullifier::derive(
            &NullifierKey(rnk),
            state_commitment_proof.position(),
            &note_commitment,
        );
        let authorization = NoteSeizureAuthorizationBody {
            chain_id: "bankd-local".to_owned(),
            note_commitment,
            nullifier,
            address: address.clone(),
            asset_id,
            amount,
            freeze_generation: leaf.freeze_generation,
            frozen_since_height: leaf.frozen_since_height,
            withdrawal: DomainHostWithdrawal {
                value: Value { amount, asset_id },
                destination: HostWithdrawalDestination::Transfer(HostTransfer {
                    recipient: "bank1seizureauthority".to_owned(),
                }),
            },
            expiry_height: 20,
        };
        let authority_instruction_commitment = authorization.commitment()?;
        let release_request = CapsuleReleaseRequest {
            chain_id: authorization.chain_id.clone(),
            ring_id: policy.ring.ring_id.clone(),
            policy_id: policy.ring.policy_id.clone(),
            permission: policy.ring.permission.clone(),
            resource: policy.ring.resource.clone(),
            ring_pk: policy.ring.ring_pk,
            asset_id,
            address: address.clone(),
            capk,
            note_commitment,
            recovery_commitment: recovery_capsule.commitment(),
            capsule_epk: recovery_capsule.epk,
            authority_instruction_commitment,
            expiry_height: authorization.expiry_height,
        };
        let capsule_release = CapsuleReleaseEvidence::from_capability_secret_for_test(
            &release_request,
            capability_secret,
        );
        let proof_public = NoteSeizureProofPublic {
            authorization: authorization.clone(),
            anchor: state_commitment_proof.root(),
            history_required: false,
            recent_position_floor: nullifier_window.recent_position_floor,
            recovery_capsule: recovery_capsule.clone(),
            recovery_seed: opening.seed,
            rnk_commitment: compliance_nullifier_key_commitment(rnk),
        };
        let proof_private = NoteSeizureProofPrivate {
            note_blinding,
            state_commitment_proof,
            rnk,
        };
        let proof = GnarkNoteSeizureClient::from_env()?.prove(&proof_public, &proof_private)?;
        let seizure = NoteSeizure {
            authorization: authorization.clone(),
            authority_signature: authority_sk.sign_deterministic(&authorization.signing_bytes()?),
            anchor: proof_public.anchor,
            history_required: false,
            recent_position_floor: nullifier_window.recent_position_floor,
            recovery_capsule,
            rnk_commitment: proof_public.rnk_commitment,
            capsule_release,
            proof,
            nullifier_window,
            historical_nullifier_proof: None,
        };
        let before_audit = host.app.state.get_audit_log_state().await?;
        let mut invalid_seizure = seizure.clone();
        invalid_seizure.capsule_release.recovered_point += Element::GENERATOR;
        let invalid = SeizeNoteRequest {
            source: Some(host_source(0)),
            seizure: Some(invalid_seizure.into()),
        };
        let error = host
            .seize_note(invalid)
            .await
            .expect_err("an invalid capsule release must not consume the note");
        assert!(error.to_string().contains("DLEQ"));
        host.app.state.check_nullifier_unspent(nullifier).await?;
        assert_eq!(host.app.state.get_audit_log_state().await?, before_audit);
        assert_eq!(
            host.app
                .state
                .get_user_leaf(&address, asset_id)
                .await?
                .context("registered seizure target")?
                .status,
            UserAssetStatus::Frozen
        );

        let request = SeizeNoteRequest {
            source: Some(host_source(0)),
            seizure: Some(seizure.into()),
        };

        let first = host.seize_note(request.clone()).await?;
        assert!(!first.replayed);
        assert_eq!(first.withdrawal.amount, amount);
        assert_eq!(first.current_status, UserAssetStatus::Seized);
        assert_eq!(
            host.app.state.verify_audit_log().await?.length,
            before_audit.length + 2
        );

        let mut duplicate = request.clone();
        duplicate.source = Some(host_source(1));
        assert!(host
            .seize_note(duplicate)
            .await
            .expect_err("a new source cannot spend the same note again")
            .to_string()
            .contains("already spent"));

        let replay = host.seize_note(request).await?;
        assert!(replay.replayed);
        assert!(replay.events.is_empty());
        assert_eq!(
            host.app.state.verify_audit_log().await?.length,
            before_audit.length + 2
        );

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_init_genesis_commits_content_genesis() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        assert!(host.commit().await.is_err());
        host.init_genesis(host_genesis()).await?;
        assert_eq!(host.phase(), HostExecutionPhase::InitializedGenesis);
        assert!(host.app.state.host_withdrawals_enabled().await?);
        assert!(host.deposit(deposit_request(0)).await.is_err());

        let response = host.commit().await?;
        assert_eq!(response.root_hash.len(), 32);
        assert_eq!(host.phase(), HostExecutionPhase::Idle);
        assert!(App::is_ready(storage.latest_snapshot()).await);

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_init_genesis_persists_ibc_parameters() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;

        let ibc_params =
            shieldd_sdk_ibc::StateReadExt::get_ibc_params(host.app.state.as_ref()).await?;
        assert_eq!(
            ibc_params,
            shieldd_sdk_ibc::params::IBCParameters::default()
        );

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_exports_checkpoint_genesis() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        let commit = host.commit().await?;

        let exported = host.export_genesis().await?;
        assert!(matches!(
            exported,
            AppState::Checkpoint(root_hash) if root_hash == commit.root_hash
        ));

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_begin_block_request_uses_state_chain_id_without_validators(
    ) -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        host.commit().await?;

        let begin_block = host.begin_block_request(host_block(7)).await?;

        assert_eq!(begin_block.header.chain_id.as_str(), "bankd-local");
        assert_eq!(begin_block.header.height.value(), 7);
        assert_eq!(begin_block.header.validators_hash, Hash::None);
        assert_eq!(begin_block.header.next_validators_hash, Hash::None);
        assert!(begin_block.last_commit_info.votes.is_empty());
        assert!(begin_block.byzantine_validators.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_block_lifecycle_commits_without_validators() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        host.commit().await?;

        host.begin_block(host_block(1)).await?;
        assert_eq!(host.phase(), HostExecutionPhase::InBlock);
        host.end_block(1).await?;
        assert_eq!(host.phase(), HostExecutionPhase::EndedBlock);
        let commit = host.commit().await?;

        assert_eq!(commit.root_hash.len(), 32);
        assert_eq!(host.phase(), HostExecutionPhase::Idle);

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_reports_only_the_latest_committed_state() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        assert!(host.committed_state().await.is_err());

        host.init_genesis(host_genesis()).await?;
        let genesis_commit = host.commit().await?;
        let committed = host.committed_state().await?;
        assert_eq!(committed.height, 0);
        assert_eq!(committed.root_hash, genesis_commit.root_hash);

        host.begin_block(host_block(1)).await?;
        let still_committed = host.committed_state().await?;
        assert_eq!(still_committed, committed);

        host.end_block(1).await?;
        let block_commit = host.commit().await?;
        let committed = host.committed_state().await?;
        assert_eq!(committed.height, 1);
        assert_eq!(committed.root_hash, block_commit.root_hash);

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_check_tx_rejects_invalid_tx_without_entering_block() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        host.commit().await?;

        let response = host.check_tx(b"not a shieldd transaction").await?;

        assert_eq!(response.code, 1);
        assert!(response.log.contains("decoding transaction"));
        assert_eq!(host.phase(), HostExecutionPhase::Idle);

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_check_tx_requires_initialized_storage() {
        let storage = temp_storage().await;
        let host = HostExecution::new(storage.deref().clone());

        let err = host
            .check_tx(&[])
            .await
            .expect_err("uninitialized storage should be rejected");

        assert!(err.to_string().contains("initialized storage"));
    }

    #[tokio::test]
    async fn host_withdrawals_resolve_registered_asset_to_base_denom() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        host.commit().await?;
        host.begin_block(host_block(1)).await?;
        host.deposit(deposit_request(0)).await?;

        let withdrawals = host
            .resolve_host_withdrawals(&host_withdrawal_transaction())
            .await?;

        assert_eq!(withdrawals.len(), 1);
        assert!(matches!(
            &withdrawals[0].destination,
            HostWithdrawalDestination::Transfer(HostTransfer { recipient })
                if recipient == "bank1recipient"
        ));
        assert_eq!(withdrawals[0].denom, BASE_ASSET_DENOM.base_denom().denom);
        assert_eq!(withdrawals[0].amount, 42u64.into());

        Ok(())
    }

    #[tokio::test]
    async fn host_withdrawals_preserve_withdrawal_order() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        host.commit().await?;
        host.begin_block(host_block(1)).await?;
        host.deposit(deposit_request(0)).await?;

        let transfer = host_withdrawal_action();
        let mut execution = host_withdrawal_action();
        execution.body.withdrawal.destination =
            HostWithdrawalDestination::Execution(DomainHostExecution {
                refund_address: test_keys::ADDRESS_0.deref().clone(),
                gas_limit: 200_000,
                calls: vec![EvmCall {
                    contract: [7u8; 20],
                    calldata: vec![0xaa],
                }],
            });
        let mut tx = Transaction::default();
        tx.transaction_body.actions = vec![
            Action::ShieldedHostWithdrawal(transfer),
            Action::ShieldedHostWithdrawal(execution),
        ];

        let withdrawals = host.resolve_host_withdrawals(&tx).await?;

        assert!(matches!(
            withdrawals[0].destination,
            HostWithdrawalDestination::Transfer(_)
        ));
        assert!(matches!(
            withdrawals[1].destination,
            HostWithdrawalDestination::Execution(_)
        ));

        Ok(())
    }

    #[tokio::test]
    async fn standalone_execution_rejects_host_withdrawals() -> Result<()> {
        let storage = temp_storage().await;
        let mut app = App::new(storage.latest_snapshot());
        app.init_chain(&host_genesis()).await;

        let error = host_withdrawal_action()
            .check_historical(app.state.clone())
            .await
            .expect_err("standalone execution must reject host withdrawals");

        assert!(error
            .to_string()
            .contains("shielded host withdrawals are not enabled"));

        Ok(())
    }

    #[tokio::test]
    async fn host_execution_accepts_withdrawals_for_registered_assets() -> Result<()> {
        let storage = temp_storage().await;
        let mut host = HostExecution::new(storage.deref().clone());

        host.init_genesis(host_genesis()).await?;
        host.commit().await?;
        host.begin_block(host_block(1)).await?;
        host.deposit(deposit_request(0)).await?;

        host_withdrawal_action()
            .check_historical(host.app.state.clone())
            .await?;

        Ok(())
    }

    #[test]
    fn deposit_id_changes_with_host_message_index() {
        let recipient = test_keys::ADDRESS_0.clone();
        let first = derive_deposit_id(
            "bankd-local",
            &HostSource::try_from(host_source(0)).unwrap(),
            &BASE_ASSET_DENOM,
            100u64.into(),
            &recipient,
        );
        let second = derive_deposit_id(
            "bankd-local",
            &HostSource::try_from(host_source(1)).unwrap(),
            &BASE_ASSET_DENOM,
            100u64.into(),
            &recipient,
        );

        assert_ne!(first, second);
    }

    #[test]
    fn source_key_uses_host_tx_identity_not_deposit_contents() {
        assert_eq!(
            HostSource::try_from(host_source(3))
                .unwrap()
                .source_key("bankd-local"),
            HostSource::try_from(host_source(3))
                .unwrap()
                .source_key("bankd-local"),
        );
        assert_ne!(
            HostSource::try_from(host_source(3))
                .unwrap()
                .source_key("bankd-local"),
            HostSource::try_from(host_source(4))
                .unwrap()
                .source_key("bankd-local"),
        );
    }

    #[test]
    fn host_source_requires_32_byte_tx_hash() {
        let source = ProtoHostSource {
            height: 42,
            tx_hash: [7u8; 31].to_vec(),
            tx_index: 0,
            msg_index: 0,
        };

        assert!(HostSource::try_from(source).is_err());
    }

    #[test]
    fn accepted_host_tx_response_accepts_empty_withdrawals() {
        let response = HostTxResponse::accepted(Vec::new(), Vec::new());

        assert!(response.withdrawals.is_empty());
    }
}
