//! Background worker for continuous compliance scanning (issuer-side).
//!
//! This worker connects to a Penumbra node and continuously scans blocks
//! using the issuer's DetectionKey to identify transactions involving
//! regulated assets.
//!
//! Flow:
//! 1. DetectionKey decrypts detection_tag → gets (asset_id, is_flagged)
//! 2. If flagged, issuer can decrypt core+extension for full visibility
//! 3. Results are logged (persistence to be implemented)

use anyhow::{Context, Result};
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
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tonic::transport::Channel;
use tracing::{debug, info, instrument, warn};

use super::detector::scan_transaction;
use super::storage::ComplianceStorage;
use crate::issuer_keys::DetectionKey;

// Maximum size of a compact block, in bytes (12MB).
const MAX_CB_SIZE_BYTES: usize = 12 * 1024 * 1024;

/// Handle for monitoring and communicating with an IssuerComplianceWorker.
///
/// Provides access to:
/// - Error state (if the worker encountered an error)
/// - Sync progress (current synced height)
pub struct WorkerHandle {
    /// Shared error slot - contains error if worker failed.
    pub error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    /// Watch receiver for sync height updates.
    pub sync_height: watch::Receiver<u64>,
}

impl WorkerHandle {
    /// Check if the worker has encountered an error.
    pub fn has_error(&self) -> bool {
        self.error_slot
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(true)
    }

    /// Get the current synced height.
    pub fn current_height(&self) -> u64 {
        *self.sync_height.borrow()
    }

    /// Take the error from the slot (leaves None).
    pub fn take_error(&self) -> Option<anyhow::Error> {
        self.error_slot.lock().ok().and_then(|mut slot| slot.take())
    }
}

/// Issuer-side compliance worker that uses DetectionKey to scan the blockchain.
///
/// This worker continuously streams blocks and detects transactions involving
/// the issuer's regulated asset. When a flagged transaction is detected,
/// the issuer has full visibility into the transaction details.
pub struct IssuerComplianceWorker {
    /// The issuer's detection key for scanning.
    detection_key: DetectionKey,
    /// The asset this DK corresponds to (DK is per-asset).
    target_asset_id: asset::Id,
    /// Storage for persisting detected transactions.
    storage: Arc<ComplianceStorage>,
    /// gRPC channel to the Penumbra node.
    channel: Channel,
    /// Shared error slot for communicating errors to callers.
    error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    /// Sender for notifying about sync progress.
    sync_height_tx: watch::Sender<u64>,
}

impl IssuerComplianceWorker {
    /// Create a new issuer compliance worker.
    ///
    /// # Arguments
    /// * `detection_key` - The issuer's DetectionKey for scanning
    /// * `target_asset_id` - The asset this DK corresponds to
    /// * `storage` - Storage backend for persisting results
    /// * `channel` - gRPC channel to the Penumbra node
    ///
    /// # Returns
    /// A tuple containing:
    /// - The worker instance
    /// - A handle for monitoring worker health and progress
    pub fn new(
        detection_key: DetectionKey,
        target_asset_id: asset::Id,
        storage: ComplianceStorage,
        channel: Channel,
    ) -> (Self, WorkerHandle) {
        let storage = Arc::new(storage);
        let error_slot = Arc::new(Mutex::new(None));
        let last_height = storage.last_sync_height().unwrap_or_else(|e| {
            warn!(
                ?e,
                "failed to read last sync height from storage, starting from 0"
            );
            0
        });
        let (sync_height_tx, sync_height_rx) = watch::channel(last_height);

        let worker = Self {
            detection_key,
            target_asset_id,
            storage,
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

    /// Run the worker, continuously syncing blocks.
    ///
    /// This method will run indefinitely, streaming new blocks from the node
    /// and scanning them for compliance activity.
    #[instrument(skip(self))]
    pub async fn run(self) -> Result<()> {
        info!("starting issuer compliance scanner worker");

        if let Err(e) = self.sync().await {
            // Store error in error slot for retrieval by caller
            let last_height = self.storage.last_sync_height().unwrap_or(0);
            let context_msg = format!(
                "compliance sync failed at height {} (check node connection and storage)",
                last_height
            );
            if let Ok(mut slot) = self.error_slot.lock() {
                *slot = Some(e.context(context_msg.clone()));
            }
            return Err(anyhow::anyhow!("{}", context_msg));
        }

        Ok(())
    }

    /// Main sync loop - fetches and scans blocks continuously.
    #[instrument(skip(self))]
    async fn sync(&self) -> Result<()> {
        let start_height = self.storage.last_sync_height()? + 1;

        info!(
            start_height,
            "beginning issuer compliance scan from height {}", start_height
        );

        // Connect to CompactBlock service
        let mut compact_block_client = CompactBlockQueryServiceClient::new(self.channel.clone())
            .max_decoding_message_size(MAX_CB_SIZE_BYTES);

        // Stream compact blocks (unbounded = stream forever)
        let mut stream = compact_block_client
            .compact_block_range(CompactBlockRangeRequest {
                start_height,
                end_height: 0,    // 0 = unbounded
                keep_alive: true, // Stream new blocks as they arrive
            })
            .await
            .context("failed to start compact block stream")?
            .into_inner();

        info!("connected to compact block stream");

        // Process blocks from the stream
        while let Some(response) = stream.message().await? {
            let compact_block = response.compact_block.ok_or_else(|| {
                anyhow::anyhow!(
                    "compliance sync: received empty compact block response from node \
                     (possible network or node issue)"
                )
            })?;

            let height = compact_block.height;

            // Skip blocks with no data
            if compact_block.state_payloads.is_empty() && compact_block.nullifiers.is_empty() {
                debug!(height, "skipping empty block");
                self.storage.update_sync_height(height)?;
                let _ = self.sync_height_tx.send(height);
                continue;
            }

            // Fetch full transactions for this block
            let transactions = self.fetch_transactions(height).await?;

            if !transactions.is_empty() {
                debug!(height, tx_count = transactions.len(), "scanning block");

                let mut detection_count = 0;
                let mut flagged_count = 0;

                // Scan each transaction using issuer's DetectionKey
                for (tx_index, tx) in transactions.iter().enumerate() {
                    match scan_transaction(
                        &self.detection_key,
                        self.target_asset_id,
                        tx,
                        height,
                        tx_index,
                        |detected| {
                            detection_count += 1;
                            if detected.is_flagged {
                                flagged_count += 1;
                                info!(
                                    height,
                                    tx_index,
                                    action_index = detected.action_index,
                                    asset_id = %detected.asset_id,
                                    "detected FLAGGED transaction - issuer has full visibility"
                                );
                                // Issuer can decrypt full ciphertext here if needed
                                // detected.ciphertext contains the full ComplianceCiphertext
                            } else {
                                debug!(
                                    height,
                                    tx_index,
                                    action_index = detected.action_index,
                                    asset_id = %detected.asset_id,
                                    "detected compliant transaction"
                                );
                            }
                            Ok(())
                        },
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            warn!(height, tx_index, "failed to scan transaction: {}", e);
                        }
                    }
                }

                if detection_count > 0 {
                    info!(
                        height,
                        detection_count, flagged_count, "scanned block with detections"
                    );
                }
            }

            // Update sync height
            self.storage.update_sync_height(height)?;
            let _ = self.sync_height_tx.send(height);

            if height % 100 == 0 {
                info!(height, "synced to height {}", height);
            }
        }

        Ok(())
    }

    /// Fetch all transactions for a specific block height.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_worker_creation() {
        let dk = DetectionKey::demo();
        let target = asset::Id(decaf377::Fq::from(12345u64));
        let storage = ComplianceStorage::new(":memory:").unwrap();
        let channel = Channel::from_static("http://localhost:8080").connect_lazy();
        let (worker, handle) = IssuerComplianceWorker::new(dk, target, storage, channel);

        // Verify initial state
        assert!(!handle.has_error());
        assert_eq!(handle.current_height(), 0);
        assert_eq!(worker.target_asset_id, target);
    }
}
