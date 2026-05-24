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

use super::advice::AuditAdviceProvider;
use super::screener::{ComplianceScreener, ScreeningResult};
use super::storage::ScannerStore;
use super::sync::{extract_clear_flows, extract_compliance_ciphertexts};
use super::types::{BlockRef, DetectionEvent, TxRef};
use crate::audit::EVIDENCE_STAGE_BUILD;
use crate::{
    issuer_keys::DetectionKey, ComplianceEvidenceObject, OutputRef, TransferOrbisUploadBundle,
};

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
    advice: Arc<dyn AuditAdviceProvider>,
    channel: Channel,
    error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    sync_height_tx: watch::Sender<u64>,
}

impl IssuerComplianceWorker {
    pub async fn new(
        detection_key: DetectionKey,
        target_asset_id: asset::Id,
        storage: Arc<dyn ScannerStore>,
        block_identity: Arc<dyn BlockIdentityProvider>,
        advice: Arc<dyn AuditAdviceProvider>,
        channel: Channel,
    ) -> Result<(Self, WorkerHandle)> {
        let error_slot = Arc::new(Mutex::new(None));
        let last_height = storage
            .last_scanned_block()
            .await?
            .map(|block| block.height)
            .unwrap_or(0);
        let (sync_height_tx, sync_height_rx) = watch::channel(last_height);

        let worker = Self {
            screener: ComplianceScreener::new(detection_key, target_asset_id),
            target_asset_id,
            storage,
            block_identity,
            advice,
            channel,
            error_slot: error_slot.clone(),
            sync_height_tx,
        };

        let handle = WorkerHandle {
            error_slot,
            sync_height: sync_height_rx,
        };

        Ok((worker, handle))
    }

    #[instrument(skip(self))]
    pub async fn run(self) -> Result<()> {
        info!("starting issuer compliance scanner worker");
        self.storage.mark_started().await?;
        if let Err(error) = self.sync(None).await {
            let last_height = self
                .storage
                .last_scanned_block()
                .await
                .ok()
                .flatten()
                .map(|block| block.height)
                .unwrap_or(0);
            let context_msg = format!(
                "compliance sync failed at height {} (check node connection and storage)",
                last_height
            );
            let _ = self.storage.record_error(&context_msg).await;
            if let Ok(mut slot) = self.error_slot.lock() {
                *slot = Some(error.context(context_msg.clone()));
            }
            return Err(anyhow!("{}", context_msg));
        }
        let _ = self.storage.mark_stopped().await;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn catch_up_to_height(self, end_height: u64) -> Result<()> {
        info!(end_height, "starting issuer compliance scanner catch-up");
        self.storage.mark_started().await?;
        if let Err(error) = self.sync(Some(end_height)).await {
            let last_height = self
                .storage
                .last_scanned_block()
                .await
                .ok()
                .flatten()
                .map(|block| block.height)
                .unwrap_or(0);
            let context_msg = format!(
                "compliance catch-up failed at height {} (target {})",
                last_height, end_height
            );
            let _ = self.storage.record_error(&context_msg).await;
            if let Ok(mut slot) = self.error_slot.lock() {
                *slot = Some(error.context(context_msg.clone()));
            }
            return Err(anyhow!("{}", context_msg));
        }
        let _ = self.storage.mark_stopped().await;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync(&self, end_height: Option<u64>) -> Result<()> {
        let start_height = self
            .storage
            .last_scanned_block()
            .await?
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
            self.storage.heartbeat().await?;
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
                self.storage.rollback_to_height(ancestor_height).await?;
                for replay_height in ancestor_height + 1..=height {
                    let replay_block = self.block_identity.block_ref(replay_height).await?;
                    self.process_block(replay_block).await?;
                }
                Ok(())
            }
        }
    }

    async fn reorg_decision(&self, block: &BlockRef) -> Result<ReorgDecision> {
        if let Some(stored_current) = self.storage.block_by_height(block.height).await? {
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

        match self.storage.block_by_height(block.height - 1).await? {
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
            if let Some(stored) = self.storage.block_by_height(height).await? {
                if stored.block_hash == live.block_hash {
                    return Ok(height);
                }
            }
            height -= 1;
        }
    }

    async fn process_block(&self, block: BlockRef) -> Result<()> {
        self.storage.begin_block(&block).await?;
        let transactions = self.fetch_transactions(block.height).await?;

        let mut detection_count = 0u64;
        let mut invalid_count = 0u64;
        let mut flagged_count = 0u64;
        let mut evidence_work = Vec::new();

        for (tx_index, tx) in transactions.iter().enumerate() {
            let tx_ref = TxRef {
                block: block.clone(),
                tx_index: tx_index as u32,
                tx_hash: crate::scanner_transaction_id_from_proto(tx),
            };

            for extracted in extract_compliance_ciphertexts(&tx_ref, tx) {
                let output_ref = extracted.output_ref.clone();
                let upload_bundle_bytes = extracted.upload_bundle_bytes.clone();
                self.storage.save_ciphertext(&extracted).await?;
                match self.screener.screen(extracted) {
                    ScreeningResult::Irrelevant => {
                        self.storage.mark_ciphertext_irrelevant(&output_ref).await?;
                    }
                    ScreeningResult::Detected(event) => {
                        detection_count += 1;
                        if event.is_flagged {
                            flagged_count += 1;
                        }
                        evidence_work.push(PendingEvidenceWork {
                            output_ref: event.output_ref.clone(),
                            event: event.clone(),
                            upload_bundle_bytes,
                        });
                        self.storage.save_detection(&event).await?;
                    }
                    ScreeningResult::InvalidCiphertext(invalid) => {
                        invalid_count += 1;
                        self.storage.save_invalid_ciphertext(&invalid).await?;
                    }
                }
            }

            for clear_flow in extract_clear_flows(&tx_ref, tx) {
                self.storage.save_clear_flow(&clear_flow).await?;
            }
        }

        self.storage.commit_block(&block).await?;
        for work in evidence_work {
            if let Err(error) = self.validate_detected_evidence(work).await {
                warn!(
                    height = block.height,
                    ?error,
                    "failed to validate compliance evidence for detected output"
                );
            }
        }
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

    async fn validate_detected_evidence(&self, work: PendingEvidenceWork) -> Result<()> {
        let output_ref = &work.output_ref;
        let Some(upload_bundle_bytes) = work.upload_bundle_bytes.as_deref() else {
            self.storage
                .record_evidence_failure(
                    output_ref,
                    EVIDENCE_STAGE_BUILD,
                    "detected output is missing Orbis upload bundle bytes",
                )
                .await?;
            return Ok(());
        };
        let upload_bundle = match TransferOrbisUploadBundle::from_bytes(upload_bundle_bytes) {
            Ok(bundle) => bundle,
            Err(error) => {
                self.storage
                    .record_evidence_failure(
                        output_ref,
                        EVIDENCE_STAGE_BUILD,
                        &format!("failed to decode Orbis upload bundle: {error}"),
                    )
                    .await?;
                return Ok(());
            }
        };
        let Some(asset_policy) = self.advice.asset_policy(work.event.asset_id).await? else {
            self.storage
                .record_evidence_failure(
                    output_ref,
                    EVIDENCE_STAGE_BUILD,
                    "asset policy unavailable for detected output",
                )
                .await?;
            return Ok(());
        };
        let evidence = match ComplianceEvidenceObject::from_upload_bundle(
            output_ref.clone(),
            work.event.asset_id,
            work.event.is_flagged,
            work.event.salt,
            work.event.ciphertext,
            &upload_bundle,
        ) {
            Ok(evidence) => evidence,
            Err(error) => {
                self.storage
                    .record_evidence_failure(
                        output_ref,
                        EVIDENCE_STAGE_BUILD,
                        &format!("failed to build compliance evidence: {error}"),
                    )
                    .await?;
                return Ok(());
            }
        };

        self.storage
            .validate_and_save_evidence(&evidence, &upload_bundle, &asset_policy.ring.ring_pk)
            .await?;
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

struct PendingEvidenceWork {
    output_ref: OutputRef,
    event: DetectionEvent,
    upload_bundle_bytes: Option<Vec<u8>>,
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

    use crate::scanner::{NoopAuditAdviceProvider, SqliteScannerStore};
    use crate::{AssetPolicy, ExtractedComplianceCiphertext};

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

    struct FixedAuditAdviceProvider {
        policy: AssetPolicy,
    }

    #[async_trait]
    impl AuditAdviceProvider for FixedAuditAdviceProvider {
        async fn asset_policy(&self, _asset_id: asset::Id) -> Result<Option<AssetPolicy>> {
            Ok(Some(self.policy.clone()))
        }

        async fn ring_info(&self, _ring_id: &str) -> Result<Option<crate::scanner::RingInfo>> {
            Ok(None)
        }

        async fn known_label(&self, _address_key: &[u8]) -> Result<Option<String>> {
            Ok(None)
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
        store.begin_block(&block).await.unwrap();
        store.commit_block(&block).await.unwrap();
        let identity = Arc::new(MemoryBlockIdentity::default());
        let (_worker, handle) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(12345u64)),
            Arc::new(store),
            identity,
            Arc::new(NoopAuditAdviceProvider),
            Channel::from_static("http://localhost:8080").connect_lazy(),
        )
        .await
        .unwrap();

        assert_eq!(handle.current_height(), 7);
    }

    #[tokio::test]
    async fn reorg_decision_accepts_matching_parent() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        let b1 = block(1, 1, 0);
        store.begin_block(&b1).await.unwrap();
        store.commit_block(&b1).await.unwrap();
        let identity = Arc::new(MemoryBlockIdentity::default());
        identity.insert(b1);
        let (worker, _) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(1u64)),
            store,
            identity,
            Arc::new(NoopAuditAdviceProvider),
            Channel::from_static("http://localhost:8080").connect_lazy(),
        )
        .await
        .unwrap();

        assert!(matches!(
            worker.reorg_decision(&block(2, 2, 1)).await.unwrap(),
            ReorgDecision::Process
        ));
    }

    #[tokio::test]
    async fn reorg_decision_walks_back_to_common_ancestor() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        for block in [block(1, 1, 0), block(2, 2, 1), block(3, 3, 2)] {
            store.begin_block(&block).await.unwrap();
            store.commit_block(&block).await.unwrap();
        }

        let identity = Arc::new(MemoryBlockIdentity::default());
        identity.insert(block(1, 1, 0));
        identity.insert(block(2, 20, 1));
        identity.insert(block(3, 30, 20));
        let (worker, _) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(1u64)),
            store,
            identity,
            Arc::new(NoopAuditAdviceProvider),
            Channel::from_static("http://localhost:8080").connect_lazy(),
        )
        .await
        .unwrap();

        assert!(matches!(
            worker.reorg_decision(&block(4, 40, 30)).await.unwrap(),
            ReorgDecision::RollbackTo(1)
        ));
    }

    #[tokio::test]
    async fn worker_validates_detected_evidence_with_advice_provider() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        let block = evidence.output_ref.action.tx.block.clone();
        let event = DetectionEvent {
            output_ref: evidence.output_ref.clone(),
            asset_id: evidence.asset_id,
            is_flagged: evidence.is_flagged,
            salt: evidence.detection_salt,
            sender_slot_id: 0,
            receiver_slot_id: 0,
            ciphertext: evidence.transfer_ciphertext.clone(),
            raw_bytes: evidence.transfer_ciphertext.to_bytes(),
        };

        store.begin_block(&block).await.unwrap();
        store
            .save_ciphertext(&ExtractedComplianceCiphertext {
                output_ref: evidence.output_ref.clone(),
                raw_bytes: evidence.transfer_ciphertext.to_bytes(),
                upload_bundle_bytes: Some(bundle.to_bytes().unwrap()),
            })
            .await
            .unwrap();
        store.save_detection(&event).await.unwrap();
        store.commit_block(&block).await.unwrap();

        let (worker, _) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            evidence.asset_id,
            store.clone(),
            Arc::new(MemoryBlockIdentity::default()),
            Arc::new(FixedAuditAdviceProvider {
                policy: AssetPolicy::simple(DetectionKey::demo().public_key(), 1, ring_pk),
            }),
            Channel::from_static("http://localhost:8080").connect_lazy(),
        )
        .await
        .unwrap();

        worker
            .validate_detected_evidence(PendingEvidenceWork {
                output_ref: evidence.output_ref.clone(),
                event,
                upload_bundle_bytes: Some(bundle.to_bytes().unwrap()),
            })
            .await
            .unwrap();

        let conn = store.lock_conn().unwrap();
        let (status, evidence_count): (String, i64) = conn
            .query_row(
                "SELECT d.audit_status,
                        (SELECT COUNT(*) FROM compliance_evidence_objects)
                 FROM scanner_detections d",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, crate::scanner::types::AUDIT_STATUS_EVIDENCE_VALID);
        assert_eq!(evidence_count, 1);
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
