use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures::{stream::BoxStream, TryStreamExt};
use shieldd_sdk_asset::asset;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info, instrument, warn};

use super::screener::{ComplianceScreener, ScreeningResult};
use super::storage::ScannerStore;
use super::sync::extract_compliance_ciphertexts;
use super::types::{
    BlockRef, CandidateEvidence, OutputOutcome, ScannedBlock, ScannedOutput, TxRef,
};
use crate::issuer_keys::DetectionKey;

const BLOCK_IDENTITY_MAX_ATTEMPTS: usize = 5;
const BLOCK_IDENTITY_INITIAL_BACKOFF: Duration = Duration::from_millis(200);

#[async_trait]
pub trait BlockIdentityProvider: Send + Sync {
    async fn block_ref(&self, height: u64) -> Result<BlockRef>;
}

/// Host-provided canonical blocks and their transaction data.
#[async_trait]
pub trait ScannerSource: BlockIdentityProvider {
    async fn heights(
        &self,
        start: u64,
        end: Option<u64>,
    ) -> Result<BoxStream<'static, Result<u64>>>;
    async fn transactions(
        &self,
        block: &BlockRef,
    ) -> Result<Vec<shieldd_sdk_proto::core::transaction::v1::Transaction>>;
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
    source: Arc<dyn ScannerSource>,
    error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    sync_height_tx: watch::Sender<u64>,
}

impl IssuerComplianceWorker {
    pub async fn new(
        detection_key: DetectionKey,
        target_asset_id: asset::Id,
        storage: Arc<dyn ScannerStore>,
        source: Arc<dyn ScannerSource>,
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
            source,
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

        let mut stream = self.source.heights(start_height, end_height).await?;
        let mut expected_height = Some(start_height);
        while let Some(height) = stream.try_next().await? {
            anyhow::ensure!(
                Some(height) == expected_height && end_height.is_none_or(|end| height <= end),
                "scanner source returned noncontiguous or out-of-range height"
            );
            self.process_height(height).await?;
            self.storage.heartbeat().await?;
            expected_height = height.checked_add(1);
        }
        if let Some(end) = end_height {
            anyhow::ensure!(
                self.storage
                    .last_scanned_block()
                    .await?
                    .is_some_and(|block| block.height == end),
                "scanner source ended before requested height"
            );
        }

        Ok(())
    }

    async fn process_height(&self, height: u64) -> Result<()> {
        let block = self.block_ref(height).await?;
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
                    let replay_block = self.block_ref(replay_height).await?;
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
            let live = self.block_ref(height).await?;
            if let Some(stored) = self.storage.block_by_height(height).await? {
                if stored.block_hash == live.block_hash {
                    return Ok(height);
                }
            }
            height -= 1;
        }
    }

    async fn process_block(&self, block: BlockRef) -> Result<()> {
        let mut scanned = ScannedBlock::new(block.clone());
        let transactions = self.source.transactions(&block).await?;

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
                let outcome = match self.screener.screen(extracted.clone()) {
                    ScreeningResult::Irrelevant => OutputOutcome::Irrelevant,
                    ScreeningResult::Detected(event) => {
                        detection_count += 1;
                        flagged_count += u64::from(event.is_flagged);
                        let evidence = CandidateEvidence::from_detection(
                            &event,
                            extracted.metadata_bytes.as_deref(),
                        );
                        OutputOutcome::Detected { event, evidence }
                    }
                    ScreeningResult::InvalidCiphertext(invalid) => {
                        invalid_count += 1;
                        OutputOutcome::Invalid {
                            reason: invalid.reason,
                        }
                    }
                };
                scanned.outputs.push(ScannedOutput {
                    ciphertext: extracted,
                    outcome,
                });
            }
        }

        self.storage.commit_scanned_block(&scanned).await?;
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

    async fn block_ref(&self, height: u64) -> Result<BlockRef> {
        let mut backoff = BLOCK_IDENTITY_INITIAL_BACKOFF;
        for attempt in 1..=BLOCK_IDENTITY_MAX_ATTEMPTS {
            match self.source.block_ref(height).await {
                Ok(block) => {
                    anyhow::ensure!(block.height == height, "host block identity height mismatch");
                    return Ok(block);
                }
                Err(error) if attempt == BLOCK_IDENTITY_MAX_ATTEMPTS => return Err(error)
                    .with_context(|| format!("failed to fetch block identity for height {height} after {attempt} attempts")),
                Err(error) => {
                    warn!(height, attempt, ?error, "failed to fetch block identity, retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = backoff.saturating_mul(2);
                }
            }
        }
        unreachable!("bounded retry returns on its final attempt")
    }
}

enum ReorgDecision {
    AlreadyProcessed,
    Process,
    RollbackTo(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use std::collections::HashMap;

    use crate::scanner::SqliteScannerStore;
    use crate::ExtractedComplianceCiphertext;

    #[derive(Default)]
    struct MemoryBlockIdentity {
        blocks: Mutex<HashMap<u64, BlockRef>>,
        failures: Mutex<HashMap<u64, usize>>,
        heights: Mutex<Vec<u64>>,
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

    #[async_trait]
    impl ScannerSource for MemoryBlockIdentity {
        async fn heights(
            &self,
            _start: u64,
            _end: Option<u64>,
        ) -> Result<BoxStream<'static, Result<u64>>> {
            Ok(Box::pin(futures::stream::iter(
                self.heights.lock().unwrap().clone().into_iter().map(Ok),
            )))
        }
        async fn transactions(
            &self,
            _block: &BlockRef,
        ) -> Result<Vec<shieldd_sdk_proto::core::transaction::v1::Transaction>> {
            Ok(vec![])
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
    async fn catch_up_rejects_skipped_empty_and_truncated_streams() {
        for heights in [vec![3], vec![], vec![1]] {
            let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
            let source = Arc::new(MemoryBlockIdentity::default());
            *source.heights.lock().unwrap() = heights.clone();
            for height in 1..=3 {
                source.insert(block(height, height as u8, height as u8 - 1));
            }
            let (worker, _) = IssuerComplianceWorker::new(
                DetectionKey::demo(),
                asset::Id(decaf377::Fq::from(12345u64)),
                store.clone(),
                source,
            )
            .await
            .unwrap();
            assert!(
                worker.catch_up_to_height(3).await.is_err(),
                "accepted {heights:?}"
            );
            assert_eq!(
                store.last_scanned_block().await.unwrap().map(|b| b.height),
                if heights == vec![1] { Some(1) } else { None }
            );
        }
    }

    #[tokio::test]
    async fn worker_creation_uses_stored_height() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let block = block(7, 7, 6);
        store
            .commit_scanned_block(&ScannedBlock::new(block.clone()))
            .await
            .unwrap();
        let identity = Arc::new(MemoryBlockIdentity::default());
        let (_worker, handle) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(12345u64)),
            Arc::new(store),
            identity,
        )
        .await
        .unwrap();

        assert_eq!(handle.current_height(), 7);
    }

    #[tokio::test]
    async fn reorg_decision_accepts_matching_parent() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        let b1 = block(1, 1, 0);
        store
            .commit_scanned_block(&ScannedBlock::new(b1.clone()))
            .await
            .unwrap();
        let identity = Arc::new(MemoryBlockIdentity::default());
        identity.insert(b1);
        let (worker, _) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(1u64)),
            store,
            identity,
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
            store
                .commit_scanned_block(&ScannedBlock::new(block.clone()))
                .await
                .unwrap();
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
        )
        .await
        .unwrap();

        assert!(matches!(
            worker.reorg_decision(&block(4, 40, 30)).await.unwrap(),
            ReorgDecision::RollbackTo(1)
        ));
    }

    #[tokio::test]
    async fn worker_validates_detected_metadata_only_evidence() {
        let store = Arc::new(SqliteScannerStore::new(":memory:").unwrap());
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        let block = evidence.output_ref().action.tx.block.clone();
        let crate::ComplianceEvidenceCiphertext::Transfer(ciphertext) = &evidence.ciphertext else {
            panic!("transfer fixture expected")
        };
        let event = crate::scanner::DetectionEvent {
            record_ref: evidence.record_ref.clone(),
            asset_id: evidence.asset_id,
            is_flagged: evidence.is_flagged,
            salt: evidence.detection_salt,
            routing_tags: [11, 22],
            ciphertext: super::super::types::ComplianceCiphertext::Transfer(ciphertext.clone()),
            raw_bytes: ciphertext.to_bytes(),
            public_withdrawal: None,
        };

        let metadata_bytes = metadata.to_bytes().unwrap();
        let candidate = CandidateEvidence::from_detection(&event, Some(&metadata_bytes));
        store
            .commit_scanned_block(&ScannedBlock {
                block,
                outputs: vec![ScannedOutput {
                    ciphertext: ExtractedComplianceCiphertext {
                        record_ref: evidence.record_ref.clone(),
                        kind: super::super::types::ComplianceCiphertextKind::Transfer,
                        public_withdrawal: None,
                        routing_tags: [11, 22],
                        raw_bytes: evidence.ciphertext_bytes(),
                        metadata_bytes: Some(metadata_bytes),
                    },
                    outcome: OutputOutcome::Detected {
                        event,
                        evidence: candidate,
                    },
                }],
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

    #[tokio::test]
    async fn host_identity_retries_transient_failure_and_rejects_wrong_height() {
        let source = Arc::new(MemoryBlockIdentity::default());
        source.insert(block(2, 2, 1));
        source.failures.lock().unwrap().insert(2, 1);
        let (worker, _) = IssuerComplianceWorker::new(
            DetectionKey::demo(),
            asset::Id(decaf377::Fq::from(1u64)),
            Arc::new(SqliteScannerStore::new(":memory:").unwrap()),
            source.clone(),
        )
        .await
        .unwrap();
        assert_eq!(worker.block_ref(2).await.unwrap().height, 2);
        source.blocks.lock().unwrap().insert(2, block(3, 3, 2));
        assert!(worker
            .block_ref(2)
            .await
            .unwrap_err()
            .to_string()
            .contains("height mismatch"));
    }
}
