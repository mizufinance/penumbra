//! Wallet block scanning with storage at the edge and host-supplied chain data.
use crate::{
    compliance_tree::{ComplianceAssetTree, ComplianceSnapshot, ComplianceUserTree},
    sync::{scan_block, FilteredBlock},
    Storage,
};
use anyhow::Context;
use shieldd_sdk_compact_block::CompactBlock;
use shieldd_sdk_compliance::EventUserAssetStatusChanged;
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_sct::CommitmentSource;
use shieldd_sdk_transaction::Transaction;
use std::{collections::BTreeSet, sync::Arc};

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

/// Complete inputs supplied for one committed host block.
pub struct WalletBlock {
    pub block: CompactBlock,
    pub expected_sct_root: shieldd_sdk_tct::Root,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
    pub assets: Vec<shieldd_sdk_asset::asset::Metadata>,
    pub updated_app_parameters: Option<shieldd_sdk_app::params::AppParameters>,
}

pub struct SyncWorker {
    storage: Storage,
    sct: shieldd_sdk_tct::Tree,
    fvk: FullViewingKey,
    compliance_snapshot: Arc<ComplianceSnapshot>,
}

impl SyncWorker {
    pub async fn new(storage: Storage) -> anyhow::Result<Self> {
        Ok(Self {
            sct: storage.state_commitment_tree().await?,
            fvk: storage.full_viewing_key().await?,
            compliance_snapshot: Arc::new(ComplianceSnapshot {
                user_tree: storage.compliance_user_tree().await?,
                asset_tree: storage.compliance_asset_tree().await?,
            }),
            storage,
        })
    }

    async fn prepare_compliance_block(
        &self,
        block: &CompactBlock,
    ) -> anyhow::Result<(
        Option<crate::storage::ComplianceBlockPlan>,
        Arc<ComplianceSnapshot>,
    )> {
        let current = self.compliance_snapshot.clone();
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

    pub async fn scan(&mut self, input: WalletBlock) -> anyhow::Result<()> {
        let WalletBlock {
            block,
            expected_sct_root,
            timestamp,
            transactions,
            assets,
            updated_app_parameters,
        } = input;
        let height = block.height;
        let expected = self
            .storage
            .last_sync_height()
            .await?
            .map(|h| h.checked_add(1).context("wallet height overflow"))
            .transpose()?
            .unwrap_or(0);
        anyhow::ensure!(
            height == expected,
            "out of order wallet block: expected {expected}, got {height}"
        );
        anyhow::ensure!(
            block.app_parameters_updated == updated_app_parameters.is_some(),
            "wallet block parameter update is missing or unsolicited"
        );
        let (compliance_plan, next_compliance_snapshot) =
            self.prepare_compliance_block(&block).await?;
        let mut next_sct = self.sct.clone();
        let epoch = block.epoch_root.map(|root| crate::storage::CompletedEpoch {
            index: block.epoch_index,
            root,
        });
        let mut counterparties = BTreeSet::new();
        let mut filtered_block = scan_block(
            &self.fvk,
            &mut next_sct,
            block,
            &self.storage,
            compliance_plan.as_ref(),
        )
        .await?;
        anyhow::ensure!(
            next_sct.root() == expected_sct_root,
            "wallet SCT root does not match committed host anchor"
        );
        let transactions = relevant_transactions(&mut filtered_block, transactions);
        for transaction in &transactions {
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
                    shieldd_sdk_transaction::Action::NoteReshape(note_reshape) => note_reshape
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
                        .collect(),
                    _ => Vec::new(),
                };

                for (encrypted_note, ovk_wrapped_key, note_commitment, cv, epk) in outputs {
                    if let Ok(decrypted_note) = shieldd_sdk_shielded_pool::Note::decrypt_outgoing(
                        encrypted_note,
                        ovk_wrapped_key,
                        note_commitment,
                        cv,
                        ovk,
                        epk,
                    ) {
                        let dest_address = decrypted_note.address();
                        if !self.fvk.incoming().views_address(&dest_address) {
                            counterparties.insert(dest_address);
                        }
                    }
                }
            }
        }

        self.storage
            .record_block(
                filtered_block,
                transactions,
                &mut next_sct,
                updated_app_parameters,
                compliance_plan,
                crate::storage::WalletBlockMetadata {
                    timestamp,
                    assets,
                    epoch,
                    counterparties,
                },
            )
            .await?;
        self.sct = next_sct;
        self.compliance_snapshot = next_compliance_snapshot;
        Ok(())
    }
}

fn relevant_transactions(
    filtered_block: &mut FilteredBlock,
    all_transactions: Vec<Transaction>,
) -> Vec<Transaction> {
    let spent_nullifiers = filtered_block
        .spent_nullifiers
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
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

    transactions
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
        let worker = SyncWorker::new(storage).await.unwrap();
        let before = worker.compliance_snapshot.clone();
        let block = CompactBlock {
            compliance_user_anchor: Some(before.user_tree.root()),
            compliance_asset_anchor: Some(before.asset_tree.root()),
            ..Default::default()
        };

        let (plan, after) = worker.prepare_compliance_block(&block).await.unwrap();

        assert!(plan.is_none());
        assert!(Arc::ptr_eq(&before, &after));
    }
    #[tokio::test]
    async fn empty_blocks_survive_worker_restart() -> anyhow::Result<()> {
        let storage = Storage::initialize(
            None::<&camino::Utf8Path>,
            (*shieldd_sdk_keys::test_keys::FULL_VIEWING_KEY).clone(),
            Default::default(),
        )
        .await?;
        let mut worker = SyncWorker::new(storage.clone()).await?;
        for height in 0..3 {
            let block = CompactBlock {
                height,
                compliance_user_anchor: Some(worker.compliance_snapshot.user_tree.root()),
                compliance_asset_anchor: Some(worker.compliance_snapshot.asset_tree.root()),
                ..Default::default()
            };
            let mut expected_sct = worker.sct.clone();
            expected_sct.insert_block(block.block_root)?;
            let original_root = worker.sct.root();
            let wrong_root =
                shieldd_sdk_tct::Root::try_from(shieldd_sdk_proto::crypto::tct::v1::MerkleRoot {
                    inner: (decaf377::Fq::from(expected_sct.root()) + decaf377::Fq::from(1u64))
                        .to_bytes()
                        .to_vec(),
                })?;
            assert!(worker
                .scan(WalletBlock {
                    block: block.clone(),
                    expected_sct_root: wrong_root,
                    timestamp: height,
                    transactions: vec![],
                    assets: vec![],
                    updated_app_parameters: None,
                })
                .await
                .is_err());
            assert_eq!(worker.sct.root(), original_root);
            assert_eq!(storage.last_sync_height().await?, height.checked_sub(1));
            assert_eq!(storage.block_timestamp().await.ok(), height.checked_sub(1));
            worker
                .scan(WalletBlock {
                    block,
                    expected_sct_root: expected_sct.root(),
                    timestamp: height,
                    transactions: vec![],
                    assets: vec![],
                    updated_app_parameters: None,
                })
                .await?;
            let resumed = SyncWorker::new(storage.clone()).await?;
            assert_eq!(resumed.sct.root(), worker.sct.root(), "height {height}");
            assert_eq!(
                resumed.sct.position(),
                worker.sct.position(),
                "height {height}"
            );
            assert_eq!(storage.last_sync_height().await?, Some(height));
        }
        Ok(())
    }
}
