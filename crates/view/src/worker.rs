use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use shieldd_sdk_compact_block::CompactBlock;
use shieldd_sdk_compliance::EventUserAssetStatusChanged;
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_proto::core::{
    app::v1::{
        query_service_client::QueryServiceClient as AppQueryServiceClient,
        TransactionsByHeightRequest,
    },
    component::{
        compact_block::v1::{
            query_service_client::QueryServiceClient as CompactBlockQueryServiceClient,
            CompactBlockRangeRequest,
        },
        shielded_pool::v1::{
            query_service_client::QueryServiceClient as ShieldedPoolQueryServiceClient,
            AssetMetadataByIdRequest,
        },
    },
};
use shieldd_sdk_sct::{CommitmentSource, Nullifier};
use shieldd_sdk_transaction::Transaction;
use tap::Tap;
use tokio::sync::{watch, RwLock};
use tonic::transport::Channel;
use tracing::instrument;

use crate::{
    compliance_tree::{ComplianceAssetTree, ComplianceSnapshot, ComplianceUserTree},
    sync::{scan_block, FilteredBlock},
    Storage,
};

// Large local benchmark genesis states can emit compact blocks well above the historic 12MB cap.
const MAX_CB_SIZE_BYTES: usize = 64 * 1024 * 1024;

fn apply_user_status_change(
    tree: &mut ComplianceUserTree,
    event: &EventUserAssetStatusChanged,
) -> anyhow::Result<()> {
    event.validate()?;
    let mut previous_leaf = event.leaf.clone();
    previous_leaf.status = event.previous_status;
    let local_commitment = tree.commitment(event.position).with_context(|| {
        format!(
            "missing user leaf at status-change position {}",
            event.position
        )
    })?;
    anyhow::ensure!(
        local_commitment == previous_leaf.commit(),
        "user status change at position {} does not replace the locally authenticated previous leaf",
        event.position
    );
    tree.update(event.position, event.commitment)
}

fn validate_compliance_anchors(
    block: &CompactBlock,
    user_tree: &ComplianceUserTree,
    asset_tree: &ComplianceAssetTree,
) -> anyhow::Result<()> {
    let user_anchor = block
        .compliance_user_anchor
        .context("compact block is missing its compliance user anchor")?;
    let asset_anchor = block
        .compliance_asset_anchor
        .context("compact block is missing its compliance asset anchor")?;
    anyhow::ensure!(
        user_tree.root() == user_anchor,
        "projected compliance user root does not match compact-block anchor"
    );
    anyhow::ensure!(
        asset_tree.root() == asset_anchor,
        "projected compliance asset root does not match compact-block anchor"
    );
    Ok(())
}

pub struct Worker {
    storage: Storage,
    sct: Arc<RwLock<shieldd_sdk_tct::Tree>>,
    fvk: FullViewingKey, // TODO: notifications (see TODOs on ViewService)
    error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    sync_height_tx: watch::Sender<u64>,
    /// Tonic channel used to create GRPC clients.
    channel: Channel,
    /// Height-consistent compliance trees published with one atomic pointer swap.
    compliance_snapshot: Arc<parking_lot::RwLock<Arc<ComplianceSnapshot>>>,
}

impl Worker {
    /// Creates a new worker, returning:
    ///
    /// - the worker itself;
    /// - a shared, in-memory SCT instance;
    /// - a shared error slot;
    /// - a channel for notifying the client of sync progress;
    /// - a shared immutable compliance snapshot.
    #[instrument(skip_all)]
    pub async fn new(
        storage: Storage,
        channel: Channel,
    ) -> Result<
        (
            Self,
            Arc<RwLock<shieldd_sdk_tct::Tree>>,
            Arc<Mutex<Option<anyhow::Error>>>,
            watch::Receiver<u64>,
            Arc<parking_lot::RwLock<Arc<ComplianceSnapshot>>>,
        ),
        anyhow::Error,
    > {
        tracing::trace!("constructing view server worker");
        let fvk = storage
            .full_viewing_key()
            .await
            .context("failed to retrieve full viewing key from storage")?
            .tap(|_| tracing::debug!("retrieved full viewing key"));

        // Create a shared, in-memory SCT.
        let sct = Arc::new(RwLock::new(storage.state_commitment_tree().await?));
        // Create a shared error slot
        let error_slot = Arc::new(Mutex::new(None));
        // Create a channel for the worker to notify of sync height changes.
        let initial_height = storage.last_sync_height().await?.unwrap_or(0);
        let (sync_height_tx, mut sync_height_rx) = watch::channel(initial_height);
        // Mark the current height as seen, since it's not new.
        sync_height_rx.borrow_and_update();

        // Load compliance trees from storage
        let compliance_snapshot =
            Arc::new(parking_lot::RwLock::new(Arc::new(ComplianceSnapshot {
                user_tree: storage
                    .compliance_user_tree()
                    .await
                    .context("failed to load compliance user tree")?,
                asset_tree: storage
                    .compliance_asset_tree()
                    .await
                    .context("failed to load compliance asset tree")?,
            })));

        Ok((
            Self {
                storage,
                sct: sct.clone(),
                fvk,
                error_slot: error_slot.clone(),
                sync_height_tx,
                channel,
                compliance_snapshot: compliance_snapshot.clone(),
            },
            sct,
            error_slot,
            sync_height_rx,
            compliance_snapshot,
        ))
    }

    /// Process compliance registrations from a CompactBlock.
    ///
    /// This syncs:
    /// 1. All user registrations into the user tree (for auth path computation)
    /// 2. Full leaf data for addresses in sync scope (own + counterparties)
    /// 3. All asset registrations into the asset tree
    /// 4. Compliance anchors for the block
    async fn prepare_compliance_block(
        &self,
        block: &CompactBlock,
    ) -> anyhow::Result<(
        Option<crate::storage::ComplianceBlockPlan>,
        Arc<ComplianceSnapshot>,
    )> {
        let current = self.compliance_snapshot.read().clone();
        let has_events = !block.compliance_user_registrations.is_empty()
            || !block.compliance_user_status_changes.is_empty()
            || !block.compliance_asset_registrations.is_empty();
        if !has_events {
            validate_compliance_anchors(block, &current.user_tree, &current.asset_tree)?;
            return Ok((None, current));
        }

        let height = block.height;
        let mut next_user_tree = current.user_tree.clone();
        let mut next_asset_tree = current.asset_tree.clone();
        let mut leaf_updates = Vec::new();
        let mut asset_policy_updates = Vec::new();

        // Process user registrations
        for event in &block.compliance_user_registrations {
            event.validate()?;
            // Insert commitment into user tree (for path computation)
            let position = next_user_tree.insert(event.commitment)?;
            anyhow::ensure!(
                position == event.position,
                "user registration position mismatch: local {position}, event {}",
                event.position
            );

            // Check if this address is in our sync scope
            let is_in_scope = self
                .storage
                .is_address_in_compliance_scope(&self.fvk, &event.leaf.address)
                .await?;

            // If in scope, store leaf data for offline proof generation
            if is_in_scope {
                leaf_updates.push(crate::storage::ComplianceLeafUpdate {
                    leaf: event.leaf.clone(),
                    position,
                    commitment: event.commitment,
                });
            }
        }

        for event in &block.compliance_user_status_changes {
            apply_user_status_change(&mut next_user_tree, event)?;

            let is_in_scope = self
                .storage
                .is_address_in_compliance_scope(&self.fvk, &event.leaf.address)
                .await?;
            if is_in_scope {
                leaf_updates.push(crate::storage::ComplianceLeafUpdate {
                    leaf: event.leaf.clone(),
                    position: event.position,
                    commitment: event.commitment,
                });
            }
        }

        // Process asset registrations (sync full leaf data including policy)
        for event in &block.compliance_asset_registrations {
            // Debug: log each asset registration event
            tracing::debug!(
                asset_id = ?event.asset_id,
                position = event.position,
                is_regulated = event.is_regulated,
                daily_volume_limit = event.indexed_leaf.params.daily_volume_limit,
                dk_pub_first_byte = event.indexed_leaf.params.dk_pub.vartime_compress().0[0],
                low_leaf_position = event.low_leaf_position,
                "worker: syncing asset registration"
            );

            // Use sync_from_event to preserve policy data (dk_pub, daily_volume_limit)
            // This is critical for correct leaf commitments in proofs
            next_asset_tree.sync_from_event(
                event.indexed_leaf.clone(),
                event.position,
                event.updated_low_leaf.clone(),
                event.low_leaf_position,
            )?;

            // Also store the asset policy in SQLite for direct lookups
            if event.is_regulated {
                asset_policy_updates.push(crate::storage::ComplianceAssetPolicyUpdate {
                    asset_id: event.asset_id,
                    policy: event.asset_policy.clone(),
                });
            }
        }

        validate_compliance_anchors(block, &next_user_tree, &next_asset_tree)?;

        // Debug: log tree state after sync
        let asset_root_after = next_asset_tree.root();
        tracing::debug!(
            asset_leaf_count = next_asset_tree.leaf_count(),
            asset_root = ?asset_root_after.0.to_bytes(),
            "worker: asset tree state after sync"
        );

        let user_root = next_user_tree.root();
        let asset_root = next_asset_tree.root();
        let user_tree = next_user_tree.persistence_plan()?;
        let asset_tree = next_asset_tree.persistence_plan()?;
        let plan = crate::storage::ComplianceBlockPlan {
            height,
            user_tree,
            asset_tree,
            user_root,
            asset_root,
            leaf_updates,
            asset_policy_updates,
        };

        next_asset_tree.clear_dirty_positions();
        next_user_tree.clear_dirty_positions();
        let snapshot = Arc::new(ComplianceSnapshot {
            user_tree: next_user_tree,
            asset_tree: next_asset_tree,
        });

        tracing::debug!(
            height,
            user_registrations = block.compliance_user_registrations.len(),
            user_status_changes = block.compliance_user_status_changes.len(),
            asset_registrations = block.compliance_asset_registrations.len(),
            "processed compliance block"
        );

        Ok((Some(plan), snapshot))
    }

    pub async fn fetch_transactions(
        &self,
        filtered_block: &mut FilteredBlock,
    ) -> anyhow::Result<Vec<Transaction>> {
        let spent_nullifiers = filtered_block
            .spent_nullifiers
            .iter()
            .cloned()
            .collect::<BTreeSet<Nullifier>>();

        let has_tx_sources = filtered_block
            .new_notes
            .values()
            .map(|record| &record.source)
            .any(|source| matches!(source, CommitmentSource::Transaction { .. }));

        // Only make a block request if we detected transactions in the FilteredBlock.
        // TODO: in the future, we could perform chaff downloads.
        if spent_nullifiers.is_empty() && !has_tx_sources {
            return Ok(Vec::new());
        }

        tracing::debug!(
            height = filtered_block.height,
            "fetching full transaction data"
        );

        let all_transactions =
            fetch_transactions(self.channel.clone(), filtered_block.height).await?;

        let mut transactions = Vec::new();

        for tx in all_transactions {
            let tx_id = tx.id().0;

            let mut relevant = false;

            if tx
                .spent_nullifiers()
                .any(|nf| spent_nullifiers.contains(&nf))
            {
                // The transaction is relevant, it spends one of our nullifiers.
                relevant = true;
            }

            // Rehydrate commitment sources.
            for commitment in tx.state_commitments() {
                filtered_block
                    .new_notes
                    .entry(commitment)
                    .and_modify(|record| {
                        relevant = true;
                        record.source = CommitmentSource::Transaction { id: Some(tx_id) };
                    });
            }

            if relevant {
                transactions.push(tx);
            }
        }

        tracing::debug!(
            matched = transactions.len(),
            "filtered relevant transactions"
        );

        Ok(transactions)
    }

    pub async fn sync(&mut self) -> anyhow::Result<()> {
        // Do a single sync run, up to whatever the latest block height is
        tracing::info!("starting client sync");

        let start_height = self
            .storage
            .last_sync_height()
            .await?
            .map(|h| h + 1)
            .unwrap_or(0);

        let mut client = CompactBlockQueryServiceClient::new(self.channel.clone())
            .max_decoding_message_size(MAX_CB_SIZE_BYTES);
        let mut stream = client
            .compact_block_range(tonic::Request::new(CompactBlockRangeRequest {
                start_height,
                end_height: 0,
                // Instruct the server to keep feeding us blocks as they're created.
                keep_alive: true,
            }))
            .await?
            .into_inner();

        // Spawn a task to consume items from the stream (somewhat)
        // independently of the execution of the block scanning.  This has two
        // purposes: first, it allows buffering to smooth performance; second,
        // it makes it slightly more difficult for a remote server to observe
        // the exact timings of the scanning of each CompactBlock.
        let (tx, mut buffered_stream) = tokio::sync::mpsc::channel(1000);
        tokio::spawn(async move {
            while let Some(block) = stream.message().await.transpose() {
                if tx.send(block).await.is_err() {
                    break;
                }
            }
        });

        let mut expected_height = start_height;

        while let Some(block) = buffered_stream.recv().await {
            let block: CompactBlock = block?.try_into()?;

            let height = block.height;
            if height != expected_height {
                tracing::warn!("out of order block detected");
                continue;
            }
            expected_height += 1;

            // Lock the SCT only while processing this block.
            let mut sct_guard = self.sct.write().await;

            if let Some(root) = block.epoch_root {
                // We now know the root for this epoch.
                self.storage
                    .update_epoch(block.epoch_index, Some(root), None)
                    .await?;
                // And also where the next epoch starts, since this block is the last.
                self.storage
                    .update_epoch(block.epoch_index + 1, None, Some(block.height + 1))
                    .await?;
            }

            let (compliance_plan, next_compliance_snapshot) =
                self.prepare_compliance_block(&block).await?;

            if !block.requires_scanning() {
                // Optimization: if the block is empty, seal the in-memory SCT,
                // and skip touching the database:
                sct_guard.end_block()?;
                // We also need to end the epoch, since an epoch boundary might not imply any
                // wallet-visible note changes that require scanning:
                if block.epoch_root.is_some() {
                    sct_guard
                        .end_epoch()
                        .expect("ending the epoch must succeed");
                }
                self.storage.record_empty_block(height).await?;
                anyhow::ensure!(
                    compliance_plan.is_none(),
                    "compliance events must force wallet block persistence"
                );
                *self.compliance_snapshot.write() = next_compliance_snapshot;
                // Notify all watchers of the new height we just recorded.
                self.sync_height_tx.send(height)?;
            } else {
                // Otherwise, scan the block and commit its changes:
                let mut filtered_block = scan_block(
                    &self.fvk,
                    &mut sct_guard,
                    block,
                    &self.storage,
                    compliance_plan.as_ref(),
                )
                .await?;

                // Download any transactions we detected.
                let transactions = self.fetch_transactions(&mut filtered_block).await?;

                // LPNFT asset IDs won't be known to the chain, so we need to pre-populate them in the local
                // registry based on transaction contents.
                for transaction in &transactions {
                    for action in transaction.actions() {
                        match action {
                            _ => (),
                        };
                    }

                    // Extract counterparties from outputs using OVK decryption
                    // This enables offline compliance lookups for future transactions to these addresses
                    let ovk = self.fvk.outgoing();
                    for action in transaction.actions() {
                        let outputs: Vec<_> = match action {
                            shieldd_sdk_transaction::Action::Transfer(transfer) => transfer
                                .body
                                .outputs
                                .iter()
                                .map(|output| {
                                    (
                                        &output.note_payload.encrypted_note,
                                        output.ovk_wrapped_key.clone(),
                                        output.note_payload.note_commitment,
                                        transfer.body.balance_commitment,
                                        &output.note_payload.ephemeral_key,
                                    )
                                })
                                .collect(),
                            shieldd_sdk_transaction::Action::NoteReshape(note_reshape) => {
                                note_reshape
                                    .body
                                    .outputs
                                    .iter()
                                    .map(|output| {
                                        (
                                            &output.note_payload.encrypted_note,
                                            output.ovk_wrapped_key.clone(),
                                            output.note_payload.note_commitment,
                                            note_reshape.body.balance_commitment,
                                            &output.note_payload.ephemeral_key,
                                        )
                                    })
                                    .collect()
                            }
                            _ => Vec::new(),
                        };

                        for (encrypted_note, ovk_wrapped_key, note_commitment, cv, epk) in outputs {
                            if let Ok(decrypted_note) =
                                shieldd_sdk_shielded_pool::Note::decrypt_outgoing(
                                    encrypted_note,
                                    ovk_wrapped_key,
                                    note_commitment,
                                    cv,
                                    ovk,
                                    epk,
                                )
                            {
                                let dest_address = decrypted_note.address();
                                if !self.fvk.incoming().views_address(&dest_address) {
                                    if let Err(e) = self
                                        .storage
                                        .record_counterparty(&dest_address, height)
                                        .await
                                    {
                                        tracing::warn!(
                                            ?dest_address,
                                            ?e,
                                            "failed to record counterparty during sync"
                                        );
                                    } else {
                                        tracing::debug!(
                                            ?dest_address,
                                            height,
                                            "recorded counterparty from historical TX"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Record any new assets we detected.
                for note_record in filtered_block.new_notes.values() {
                    // If the asset is already known, skip it, unless there's useful information
                    // to cross-reference.
                    if self
                        .storage
                        .asset_by_id(&note_record.note.asset_id())
                        .await?
                        .is_some()
                    {
                        continue;
                    } else {
                        // If the asset is unknown, we may be able to query for its denom metadata and store that.

                        let mut client = ShieldedPoolQueryServiceClient::new(self.channel.clone());
                        if let Some(denom_metadata) = client
                            .asset_metadata_by_id(AssetMetadataByIdRequest {
                                asset_id: Some(note_record.note.asset_id().into()),
                            })
                            .await?
                            .into_inner()
                            .denom_metadata
                        {
                            // If we get metadata: great, record it.
                            self.storage
                                .record_asset(denom_metadata.try_into()?)
                                .await?;
                        } else {
                            tracing::warn!(asset_id = ?note_record.note.asset_id(), "received unknown asset ID with no available metadata");
                        }
                    }
                }

                // Commit the block to the database.
                self.storage
                    .record_block(
                        filtered_block.clone(),
                        transactions,
                        &mut sct_guard,
                        self.channel.clone(),
                        compliance_plan,
                    )
                    .await?;
                *self.compliance_snapshot.write() = next_compliance_snapshot;
                // Notify all watchers of the new height we just recorded.
                self.sync_height_tx.send(filtered_block.height)?;
            }
            #[cfg(feature = "sct-divergence-check")]
            sct_divergence_check(self.channel.clone(), height, sct_guard.root()).await?;

            // Release the SCT RwLock
            drop(sct_guard);

            // Check if we should stop waiting for blocks to arrive, because the view
            // services are dropped and we're supposed to shut down.
            if self.sync_height_tx.is_closed() {
                return Ok(());
            }
        }

        Ok(())
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            // Do a single sync run, recording any errors.
            if let Err(e) = self.sync().await {
                tracing::error!(?e, "view worker error");
                self.error_slot
                    .lock()
                    .expect("mutex is not poisoned")
                    .replace(e);
            }
            // Sleep 10s (maybe later use exponential backoff?)
            tokio::time::sleep(Duration::from_secs(10)).await;
            // Clear the error slot before retrying.
            *self.error_slot.lock().expect("mutex is not poisoned") = None;
        }
    }
}

// Fetches all transactions in the block.
async fn fetch_transactions(
    channel: Channel,
    block_height: u64,
) -> anyhow::Result<Vec<Transaction>> {
    let mut client = AppQueryServiceClient::new(channel);
    let request = TransactionsByHeightRequest {
        block_height,
        ..Default::default()
    };
    // HACK: this is not a robust long-term solution but may help
    // avoid "split-brain" block fetch issues, where a client learns
    // of a new block, then immediately tries to fetch it, but that
    // fetch is load-balanced over a different node that hasn't yet
    // learned about that block.
    let response = match client.transactions_by_height(request.clone()).await {
        Ok(rsp) => rsp,
        Err(e) => {
            tracing::warn!(?e, "failed to fetch block, waiting and retrying once");
            tokio::time::sleep(Duration::from_secs(1)).await;
            client.transactions_by_height(request).await?
        }
    };
    let transactions = response
        .into_inner()
        .transactions
        .into_iter()
        .map(TryInto::try_into)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(transactions)
}

#[cfg(feature = "sct-divergence-check")]
async fn sct_divergence_check(
    channel: Channel,
    height: u64,
    actual_root: shieldd_sdk_tct::Root,
) -> anyhow::Result<()> {
    use cnidarium::proto::v1::query_service_client::QueryServiceClient;
    use shieldd_sdk_proto::DomainType;
    use shieldd_sdk_sct::state_key as sct_state_key;

    let mut client = QueryServiceClient::new(channel);
    tracing::info!(?height, "fetching anchor @ height");

    let value = client
        .key_value(cnidarium::proto::v1::KeyValueRequest {
            key: sct_state_key::tree::anchor_by_height(height),
            proof: false,
            ..Default::default()
        })
        .await?
        .into_inner()
        .value
        .context("sct state not found")?;

    let expected_root = shieldd_sdk_tct::Root::decode(value.value.as_slice())?;

    if actual_root == expected_root {
        tracing::info!(?height, ?actual_root, ?expected_root, "sct roots match");
        Ok(())
    } else {
        let e = anyhow::anyhow!(
            "SCT divergence detected at height {}: expected {}, got {}",
            height,
            expected_root,
            actual_root
        );
        // Print the error immediately, so that it's visible in the logs.
        tracing::error!(?e);
        Err(e)
    }
}

#[cfg(test)]
mod compliance_projection_tests {
    use super::*;
    use shieldd_sdk_asset::asset;
    use shieldd_sdk_compliance::{ComplianceLeaf, UserAssetStatus};

    fn status_event(
        leaf: &ComplianceLeaf,
        previous_status: UserAssetStatus,
        position: u64,
    ) -> EventUserAssetStatusChanged {
        EventUserAssetStatusChanged {
            position,
            commitment: leaf.commit(),
            leaf: leaf.clone(),
            previous_status,
        }
    }

    #[test]
    fn status_projection_authenticates_the_previous_leaf_and_event_order() {
        let mut rng = rand::thread_rng();
        let mut active = ComplianceLeaf::synthetic_unregulated(
            shieldd_sdk_keys::Address::dummy(&mut rng),
            asset::Id(decaf377::Fq::from(7u64)),
        );
        let mut tree = ComplianceUserTree::new();
        let position = tree.insert(active.commit()).unwrap();
        active.status = UserAssetStatus::Frozen;
        let freeze = status_event(&active, UserAssetStatus::Active, position);

        apply_user_status_change(&mut tree, &freeze).expect("ordered freeze must project");
        assert_eq!(tree.commitment(position), Some(freeze.commitment));
        apply_user_status_change(&mut tree, &freeze)
            .expect_err("replayed or reordered status event must not project");
    }

    #[test]
    fn empty_compliance_delta_still_requires_exact_block_anchors() {
        let user_tree = ComplianceUserTree::new();
        let asset_tree = ComplianceAssetTree::new();
        let mut block = CompactBlock {
            compliance_user_anchor: Some(user_tree.root()),
            compliance_asset_anchor: Some(asset_tree.root()),
            ..Default::default()
        };
        validate_compliance_anchors(&block, &user_tree, &asset_tree)
            .expect("matching empty-tree anchors must validate");

        block.compliance_user_anchor = None;
        validate_compliance_anchors(&block, &user_tree, &asset_tree)
            .expect_err("an event-free block must not bypass anchor validation");
    }

    #[tokio::test]
    async fn empty_compliance_delta_reuses_the_published_snapshot() {
        let storage = Storage::initialize(
            None::<&camino::Utf8Path>,
            (*shieldd_sdk_keys::test_keys::FULL_VIEWING_KEY).clone(),
            shieldd_sdk_app::params::AppParameters::default(),
        )
        .await
        .unwrap();
        let channel = tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy();
        let (worker, _, _, _, snapshots) = Worker::new(storage, channel).await.unwrap();
        let before = snapshots.read().clone();
        let block = CompactBlock {
            compliance_user_anchor: Some(before.user_tree.root()),
            compliance_asset_anchor: Some(before.asset_tree.root()),
            ..Default::default()
        };

        let (plan, after) = worker.prepare_compliance_block(&block).await.unwrap();

        assert!(plan.is_none());
        assert!(Arc::ptr_eq(&before, &after));
    }
}
