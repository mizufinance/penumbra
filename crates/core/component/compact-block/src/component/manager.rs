use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use shieldd_sdk_compliance::{ComplianceRegistryRead, ComplianceRegistryWrite};
use shieldd_sdk_fee::component::StateReadExt as _;
use shieldd_sdk_proto::DomainType;
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_sct::component::tree::{SctManager as _, SctRead};
use shieldd_sdk_shielded_pool::component::NoteManager as _;
use tracing::instrument;

use crate::{state_key, CompactBlock, PendingRoutingAction, RoutingActionPayloads, RoutingRecord};

pub trait RoutingManager: StateWrite {
    /// Stage proof-bound routing data until the current compact block is finalized.
    fn stage_routing_actions(&mut self, actions: impl IntoIterator<Item = PendingRoutingAction>) {
        let mut pending = self.pending_routing_actions();
        pending.extend(actions);
        self.object_put(state_key::pending_routing_actions(), pending);
    }

    fn pending_routing_actions(&self) -> Vec<PendingRoutingAction> {
        self.object_get(state_key::pending_routing_actions())
            .unwrap_or_default()
    }
}

impl<T: StateWrite + ?Sized> RoutingManager for T {}

#[async_trait]
pub trait CompactBlockManager: StateWrite {
    /// Finish an SCT block and use the resulting roots to finalize the current `CompactBlock`.
    async fn finish_block(&mut self) -> Result<()> {
        self.finalize_compact_block(false).await
    }

    /// Finish an SCT block and epoch and use the resulting roots to finalize the current `CompactBlock`.
    async fn finish_epoch(&mut self) -> Result<()> {
        self.finalize_compact_block(true).await
    }
}

impl<T: StateWrite + ?Sized> CompactBlockManager for T {}

#[async_trait]
trait Inner: StateWrite {
    #[instrument(skip_all)]
    async fn finalize_compact_block(&mut self, end_epoch: bool) -> Result<()> {
        use shieldd_sdk_shielded_pool::component::StateReadExt as _;
        // Find out what our block height is (this is set even during the genesis block)
        let height = self
            .get_block_height()
            .await
            .expect("height of block is always set");
        tracing::debug!(?height, ?end_epoch, "finishing compact block");

        let app_parameters_updated = height == 0;

        // Check to see if the gas prices have changed, and include them in the compact block
        // if they have (this is signaled by `shieldd_sdk_fee::StateWriteExt::put_gas_prices`):
        let (gas_prices, alt_gas_prices) = if self.gas_prices_changed() || height == 0 {
            (
                Some(
                    self.get_gas_prices()
                        .await
                        .context("could not get gas prices")?,
                ),
                self.get_alt_gas_prices()
                    .await
                    .context("could not get alt gas prices")?,
            )
        } else {
            (None, Vec::new())
        };

        let current_discovery_parameters = self
            .get_current_discovery_parameters()
            .await
            .context("could not get discovery parameters")?;
        let discovery_parameters = (height == 0
            || current_discovery_parameters.as_of_height == height)
            .then_some(current_discovery_parameters);

        // End the block in the SCT and record the block root, epoch root if applicable, and the SCT
        // itself, storing the resultant block and epoch root if applicable in the compact block.
        let (block_root, epoch_root) = self
            .end_sct_block(end_epoch)
            .await
            .context("could not end SCT block")?;

        // Pull out all the pending state payloads.
        let note_payloads = self
            .pending_note_payloads()
            .into_iter()
            // Strip the sources of transaction IDs
            .map(|(pos, note, source)| (pos, (note, source.stripped()).into()));
        let rolled_up_payloads = self
            .pending_rolled_up_payloads()
            .into_iter()
            .map(|(pos, commitment)| (pos, commitment.into()));
        let volume_accumulator_payloads = self
            .pending_volume_accumulator_payloads()
            .into_iter()
            .map(|(pos, payload, source)| {
                (
                    pos,
                    crate::StatePayload::VolumeAccumulator {
                        source: source.stripped(),
                        payload: Box::new(payload),
                    },
                )
            });

        // Sort the payloads by position and put them in the compact block
        let mut state_payloads = note_payloads
            .chain(rolled_up_payloads)
            .chain(volume_accumulator_payloads)
            .collect::<Vec<_>>();
        state_payloads.sort_by_key(|(pos, _)| *pos);
        let state_payloads = state_payloads
            .into_iter()
            .map(|(_, payload)| payload)
            .collect();

        // Add all the pending nullifiers to the compact block
        let nullifiers = self.pending_nullifiers().into_iter().collect();

        //Get the index of the current epoch
        let epoch_index = self
            .get_current_epoch()
            .await
            .expect("epoch is always set")
            .index;

        // Fetch compliance anchors (current tree roots)
        let compliance_user_anchor = self.get_user_tree_root().await.ok();
        let compliance_asset_anchor = self.get_asset_imt_root().await.ok();

        // Drain pending compliance registrations buffered during TX execution
        let compliance_user_registrations = self.pending_user_registrations();
        let compliance_user_status_changes = self.pending_user_status_changes();
        let compliance_asset_registrations = self.pending_asset_registrations();

        let nullifier_window = if height == 0 || end_epoch {
            Some(
                shieldd_sdk_sct::nullifier_tree::generation_state(self)
                    .await
                    .context("could not read nullifier generation state")?
                    .window(),
            )
        } else {
            None
        };

        let pending_routing_actions = self.pending_routing_actions();
        let mut routing_records = Vec::new();
        let mut routing_action_payloads = Vec::with_capacity(pending_routing_actions.len());
        for action in pending_routing_actions {
            for (tag_slot, tag) in action.tags.into_iter().enumerate() {
                routing_records.push(RoutingRecord {
                    tag,
                    height,
                    transaction_id: action.transaction_id,
                    action_index: action.action_index,
                    tag_slot: tag_slot
                        .try_into()
                        .context("routing action has more than 256 tag slots")?,
                });
            }
            routing_action_payloads.push(RoutingActionPayloads {
                transaction_id: action.transaction_id,
                action_index: action.action_index,
                note_payloads: action.note_payloads,
            });
        }
        self.object_delete(state_key::pending_routing_actions());

        let compact_block = CompactBlock {
            height,
            state_payloads,
            nullifiers,
            block_root,
            epoch_root,
            discovery_parameters,
            routing_records,
            routing_action_payloads,
            app_parameters_updated,
            gas_prices,
            alt_gas_prices,
            epoch_index,
            compliance_user_anchor,
            compliance_asset_anchor,
            compliance_user_registrations,
            compliance_user_status_changes,
            compliance_asset_registrations,
            nullifier_window,
        };

        self.nonverifiable_put_raw(
            state_key::compact_block(height).into_bytes(),
            compact_block.encode_to_vec(),
        );

        Ok(())
    }
}

impl<T: StateWrite + ?Sized> Inner for T {}
