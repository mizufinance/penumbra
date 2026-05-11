use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::core::{
    app::v1::{
        query_service_client::QueryServiceClient as AppQueryServiceClient,
        TransactionsByHeightRequest,
    },
    component::compact_block::v1::{
        query_service_client::QueryServiceClient as CompactBlockQueryServiceClient,
        CompactBlockRangeRequest,
    },
};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetBlockByHeightRequest,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tonic::transport::Channel;
use tracing::{debug, info, instrument, warn};

use super::screener::{ComplianceScreener, ScreeningResult};
use super::storage::{ScannerStore, SqliteScannerStore};
use super::sync::{extract_clear_flows, extract_compliance_ciphertexts};
use super::types::{BlockRef, TxRef};
use crate::issuer_keys::DetectionKey;

const MAX_CB_SIZE_BYTES: usize = 64 * 1024 * 1024;
const BLOCK_IDENTITY_MAX_ATTEMPTS: usize = 5;
const BLOCK_IDENTITY_INITIAL_BACKOFF: Duration = Duration::from_millis(200);

#[async_trait]
pub trait BlockIdentityProvider: Send + Sync {
    async fn block_ref(&self, height: u64) -> Result<BlockRef>;
}

pub struct TendermintProxyBlockIdentityProvider {
    channel: Channel,
    max_attempts: usize,
    initial_backoff: Duration,
}

impl TendermintProxyBlockIdentityProvider {
    pub fn new(channel: Channel) -> Self {
        Self {
            channel,
            max_attempts: BLOCK_IDENTITY_MAX_ATTEMPTS,
            initial_backoff: BLOCK_IDENTITY_INITIAL_BACKOFF,
        }
    }

    async fn fetch_once(&self, height: u64) -> std::result::Result<BlockRef, BlockIdentityError> {
        let mut client = TendermintProxyServiceClient::new(self.channel.clone());
        let response = client
            .get_block_by_height(GetBlockByHeightRequest {
                height: height as i64,
            })
            .await
            .map_err(|e| BlockIdentityError::Unavailable(anyhow!(e)))?
            .into_inner();

        parse_block_ref(height, response).map_err(BlockIdentityError::Malformed)
    }
}

#[async_trait]
impl BlockIdentityProvider for TendermintProxyBlockIdentityProvider {
    async fn block_ref(&self, height: u64) -> Result<BlockRef> {
        let mut attempt = 1usize;
        let mut backoff = self.initial_backoff;
        loop {
            match self.fetch_once(height).await {
                Ok(block) => return Ok(block),
                Err(BlockIdentityError::Malformed(error)) => return Err(error),
                Err(BlockIdentityError::Unavailable(error)) if attempt < self.max_attempts => {
                    warn!(
                        height,
                        attempt,
                        ?error,
                        "failed to fetch block identity, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    attempt += 1;
                    backoff = backoff.saturating_mul(2);
                }
                Err(BlockIdentityError::Unavailable(error)) => {
                    let message =
                        format!("failed to fetch block identity for height {height} after {attempt} attempts");
                    return Err(error).context(message);
                }
            }
        }
    }
}

enum BlockIdentityError {
    Unavailable(anyhow::Error),
    Malformed(anyhow::Error),
}

pub struct WorkerHandle {
    pub error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    pub sync_height: watch::Receiver<u64>,
}

impl WorkerHandle {
    pub fn has_error(&self) -> bool {
        self.error_slot
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(true)
    }

    pub fn current_height(&self) -> u64 {
        *self.sync_height.borrow()
    }

    pub fn take_error(&self) -> Option<anyhow::Error> {
        self.error_slot.lock().ok().and_then(|mut slot| slot.take())
    }
}

pub struct IssuerComplianceWorker {
    screener: ComplianceScreener,
    target_asset_id: asset::Id,
    storage: Arc<dyn ScannerStore>,
    block_identity: Arc<dyn BlockIdentityProvider>,
    channel: Channel,
    error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    sync_height_tx: watch::Sender<u64>,
}

impl IssuerComplianceWorker {
    pub fn new(
        detection_key: DetectionKey,
        target_asset_id: asset::Id,
        storage: SqliteScannerStore,
        channel: Channel,
    ) -> (Self, WorkerHandle) {
        Self::new_with_dependencies(
            detection_key,
            target_asset_id,
            Arc::new(storage),
            Arc::new(TendermintProxyBlockIdentityProvider::new(channel.clone())),
            channel,
        )
    }

    pub fn new_with_dependencies(
        detection_key: DetectionKey,
        target_asset_id: asset::Id,
        storage: Arc<dyn ScannerStore>,
        block_identity: Arc<dyn BlockIdentityProvider>,
        channel: Channel,
    ) -> (Self, WorkerHandle) {
        let error_slot = Arc::new(Mutex::new(None));
        let last_height = storage
            .last_scanned_block()
            .ok()
            .flatten()
            .map(|block| block.height)
            .unwrap_or(0);
        let (sync_height_tx, sync_height_rx) = watch::channel(last_height);

        let worker = Self {
            screener: ComplianceScreener::new(detection_key, target_asset_id),
            target_asset_id,
            storage,
            block_identity,
            channel,
            error_slot: error_slot.clone(),
            sync_height_tx,
        };

        let handle = WorkerHandle {
            error_slot,
            sync_height: sync_height_rx,
        };

        (worker, handle)
    }

    #[instrument(skip(self))]
    pub async fn run(self) -> Result<()> {
        info!("starting issuer compliance scanner worker");
        if let Err(error) = self.sync(None).await {
            let last_height = self
                .storage
                .last_scanned_block()
                .ok()
                .flatten()
                .map(|block| block.height)
                .unwrap_or(0);
            let context_msg = format!(
                "compliance sync failed at height {} (check node connection and storage)",
                last_height
            );
            if let Ok(mut slot) = self.error_slot.lock() {
                *slot = Some(error.context(context_msg.clone()));
            }
            return Err(anyhow!("{}", context_msg));
        }
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn catch_up_to_height(self, end_height: u64) -> Result<()> {
        info!(end_height, "starting issuer compliance scanner catch-up");
        if let Err(error) = self.sync(Some(end_height)).await {
            let last_height = self
                .storage
                .last_scanned_block()
                .ok()
                .flatten()
                .map(|block| block.height)
                .unwrap_or(0);
            let context_msg = format!(
                "compliance catch-up failed at height {} (target {})",
                last_height, end_height
            );
            if let Ok(mut slot) = self.error_slot.lock() {
                *slot = Some(error.context(context_msg.clone()));
            }
            return Err(anyhow!("{}", context_msg));
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync(&self, end_height: Option<u64>) -> Result<()> {
        let start_height = self
            .storage
            .last_scanned_block()?
            .map(|block| block.height + 1)
            .unwrap_or(1);

        if let Some(end_height) = end_height {
            if start_height > end_height {
                info!(
                    start_height,
                    end_height, "issuer compliance scanner already caught up"
                );
                return Ok(());
            }
        }

        info!(start_height, end_height, "beginning issuer compliance scan");

        let mut compact_block_client = CompactBlockQueryServiceClient::new(self.channel.clone())
            .max_decoding_message_size(MAX_CB_SIZE_BYTES);

        let mut stream = compact_block_client
            .compact_block_range(CompactBlockRangeRequest {
                start_height,
                end_height: end_height.unwrap_or(0),
                keep_alive: end_height.is_none(),
            })
            .await
            .context("failed to start compact block stream")?
            .into_inner();

        info!("connected to compact block stream");

        while let Some(response) = stream.message().await? {
            let compact_block = response.compact_block.ok_or_else(|| {
                anyhow!(
                    "compliance sync: received empty compact block response from node \
                     (possible network or node issue)"
                )
            })?;
            self.process_height(compact_block.height).await?;
        }

        Ok(())
    }

    async fn process_height(&self, height: u64) -> Result<()> {
        let block = self.block_identity.block_ref(height).await?;
        match self.reorg_decision(&block).await? {
            ReorgDecision::AlreadyProcessed => {
                debug!(height, "scanner block already processed");
                Ok(())
            }
            ReorgDecision::Process => self.process_block(block).await,
            ReorgDecision::RollbackTo(ancestor_height) => {
                warn!(
                    height,
                    ancestor_height,
                    "detected scanner reorg, rolling back and replaying live chain"
                );
                self.storage.rollback_to_height(ancestor_height)?;
                for replay_height in ancestor_height + 1..=height {
                    let replay_block = self.block_identity.block_ref(replay_height).await?;
                    self.process_block(replay_block).await?;
                }
                Ok(())
            }
        }
    }

    async fn reorg_decision(&self, block: &BlockRef) -> Result<ReorgDecision> {
        if let Some(stored_current) = self.storage.block_by_height(block.height)? {
            if stored_current.block_hash == block.block_hash {
                return Ok(ReorgDecision::AlreadyProcessed);
            }
            return Ok(ReorgDecision::RollbackTo(
                self.find_common_ancestor(block.height.saturating_sub(1))
                    .await?,
            ));
        }

        if block.height <= 1 {
            return Ok(ReorgDecision::Process);
        }

        match self.storage.block_by_height(block.height - 1)? {
            Some(parent) if parent.block_hash == block.parent_hash => Ok(ReorgDecision::Process),
            Some(_) => Ok(ReorgDecision::RollbackTo(
                self.find_common_ancestor(block.height - 1).await?,
            )),
            None => Ok(ReorgDecision::Process),
        }
    }

    async fn find_common_ancestor(&self, mut height: u64) -> Result<u64> {
        loop {
            if height == 0 {
                return Ok(0);
            }
            let live = self.block_identity.block_ref(height).await?;
            if let Some(stored) = self.storage.block_by_height(height)? {
                if stored.block_hash == live.block_hash {
                    return Ok(height);
                }
            }
            height -= 1;
        }
    }

    async fn process_block(&self, block: BlockRef) -> Result<()> {
        self.storage.begin_block(&block)?;
        let transactions = self.fetch_transactions(block.height).await?;

        let mut detection_count = 0u64;
        let mut invalid_count = 0u64;
        let mut flagged_count = 0u64;

        for (tx_index, tx) in transactions.iter().enumerate() {
            let tx_ref = TxRef {
                block: block.clone(),
                tx_index: tx_index as u32,
                tx_hash: crate::scanner_transaction_id_from_proto(tx),
            };

            for extracted in extract_compliance_ciphertexts(&tx_ref, tx) {
                let output_ref = extracted.output_ref.clone();
                self.storage.save_ciphertext(&extracted)?;
                match self.screener.screen(extracted) {
                    ScreeningResult::Irrelevant => {
                        self.storage.mark_ciphertext_irrelevant(&output_ref)?;
                    }
                    ScreeningResult::Detected(event) => {
                        detection_count += 1;
                        if event.is_flagged {
                            flagged_count += 1;
                        }
                        self.storage.save_detection(&event)?;
                    }
                    ScreeningResult::InvalidCiphertext(invalid) => {
                        invalid_count += 1;
                        self.storage.save_invalid_ciphertext(&invalid)?;
                    }
                }
            }

            for clear_flow in extract_clear_flows(&tx_ref, tx) {
                self.storage.save_clear_flow(&clear_flow)?;
            }
        }

        self.storage.commit_block(&block)?;
        let _ = self.sync_height_tx.send(block.height);

        if detection_count > 0 || invalid_count > 0 {
            info!(
                height = block.height,
                detection_count,
                flagged_count,
                invalid_count,
                asset_id = %self.target_asset_id,
                "scanned compliance block"
            );
        } else if block.height % 100 == 0 {
            info!(height = block.height, "synced compliance scanner");
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn fetch_transactions(
        &self,
        height: u64,
    ) -> Result<Vec<penumbra_sdk_proto::core::transaction::v1::Transaction>> {
        let mut client = AppQueryServiceClient::new(self.channel.clone());

        let response = client
            .transactions_by_height(TransactionsByHeightRequest {
                block_height: height,
            })
            .await
            .context("failed to fetch transactions")?
            .into_inner();

        Ok(response.transactions)
    }
}

enum ReorgDecision {
    AlreadyProcessed,
    Process,
    RollbackTo(u64),
}

fn parse_block_ref(
    requested_height: u64,
    response: penumbra_sdk_proto::util::tendermint_proxy::v1::GetBlockByHeightResponse,
) -> Result<BlockRef> {
    let block_id = response
        .block_id
        .ok_or_else(|| anyhow!("block identity response missing block_id"))?;
    let block = response
        .block
        .ok_or_else(|| anyhow!("block identity response missing block"))?;
    let header = block
        .header
        .ok_or_else(|| anyhow!("block identity response missing block header"))?;

    let header_height = u64::try_from(header.height)
        .map_err(|_| anyhow!("block header height is negative: {}", header.height))?;
    anyhow::ensure!(
        header_height == requested_height,
        "block identity height mismatch: requested {}, got {}",
        requested_height,
        header_height
    );

    let block_hash = parse_hash(&block_id.hash, "block hash")?;
    let parent_hash = match header.last_block_id {
        Some(parent) => {
            if requested_height == 1 && parent.hash.is_empty() {
                [0u8; 32]
            } else {
                parse_hash(&parent.hash, "parent hash")?
            }
        }
        None if requested_height == 1 => [0u8; 32],
        None => anyhow::bail!("block identity response missing parent block id"),
    };

    Ok(BlockRef {
        height: requested_height,
        block_hash,
        parent_hash,
        block_time_unix: header.time.map(|time| time.seconds),
    })
}

fn parse_hash(bytes: &[u8], label: &str) -> Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| anyhow!("{label} must be 32 bytes, got {}", bytes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use penumbra_sdk_proto::util::tendermint_proxy::v1::GetBlockByHeightResponse;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryBlockIdentity {
        blocks: Mutex<HashMap<u64, BlockRef>>,
        failures: Mutex<HashMap<u64, usize>>,
    }

    impl MemoryBlockIdentity {
        fn insert(&self, block: BlockRef) {
            self.blocks.lock().unwrap().insert(block.height, block);
        }
    }

    #[async_trait]
    impl BlockIdentityProvider for MemoryBlockIdentity {
        async fn block_ref(&self, height: u64) -> Result<BlockRef> {
            let mut failures = self.failures.lock().unwrap();
            if let Some(remaining) = failures.get_mut(&height) {
                if *remaining > 0 {
                    *remaining -= 1;
                    bail!("transient failure");
                }
            }
            drop(failures);
            self.blocks
                .lock()
                .unwrap()
                .get(&height)
                .cloned()
                .ok_or_else(|| anyhow!("missing block {height}"))
        }
    }

    fn block(height: u64, hash_byte: u8, parent_byte: u8) -> BlockRef {
        BlockRef {
            height,
            block_hash: [hash_byte; 32],
            parent_hash: [parent_byte; 32],
            block_time_unix: Some(height as i64),
        }
    }

    #[tokio::test]
    async fn worker_creation_uses_stored_height() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let block = block(7, 7, 6);
        store.begin_block(&block).unwrap();
        store.commit_block(&block).unwrap();
        let identity = Arc::new(MemoryBlockIdentity::default());
        let (_worker, handle) = IssuerComplianceWorker::new_with_dependencies(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(12345u64)),
            Arc::new(store),
            identity,
            Channel::from_static("http://localhost:8080").connect_lazy(),
        );

        assert_eq!(handle.current_height(), 7);
    }

    #[tokio::test]
    async fn reorg_decision_accepts_matching_parent() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        let b1 = block(1, 1, 0);
        store.begin_block(&b1).unwrap();
        store.commit_block(&b1).unwrap();
        let identity = Arc::new(MemoryBlockIdentity::default());
        identity.insert(b1);
        let (worker, _) = IssuerComplianceWorker::new_with_dependencies(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(1u64)),
            store,
            identity,
            Channel::from_static("http://localhost:8080").connect_lazy(),
        );

        assert!(matches!(
            worker.reorg_decision(&block(2, 2, 1)).await.unwrap(),
            ReorgDecision::Process
        ));
    }

    #[tokio::test]
    async fn reorg_decision_walks_back_to_common_ancestor() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        for block in [block(1, 1, 0), block(2, 2, 1), block(3, 3, 2)] {
            store.begin_block(&block).unwrap();
            store.commit_block(&block).unwrap();
        }

        let identity = Arc::new(MemoryBlockIdentity::default());
        identity.insert(block(1, 1, 0));
        identity.insert(block(2, 20, 1));
        identity.insert(block(3, 30, 20));
        let (worker, _) = IssuerComplianceWorker::new_with_dependencies(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(1u64)),
            store,
            identity,
            Channel::from_static("http://localhost:8080").connect_lazy(),
        );

        assert!(matches!(
            worker.reorg_decision(&block(4, 40, 30)).await.unwrap(),
            ReorgDecision::RollbackTo(1)
        ));
    }

    #[test]
    fn parse_block_ref_rejects_malformed_hash() {
        let response = GetBlockByHeightResponse {
            block_id: Some(penumbra_sdk_proto::tendermint::types::BlockId {
                hash: vec![1, 2, 3],
                part_set_header: None,
            }),
            block: Some(penumbra_sdk_proto::tendermint::types::Block {
                header: Some(penumbra_sdk_proto::tendermint::types::Header {
                    height: 2,
                    last_block_id: Some(penumbra_sdk_proto::tendermint::types::BlockId {
                        hash: vec![0u8; 32],
                        part_set_header: None,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        };

        assert!(parse_block_ref(2, response).is_err());
    }
}
