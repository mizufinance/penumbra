use std::convert::TryFrom;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use shieldd_sdk_compliance::event::{
    EventAssetRegistered, EventUserAssetStatusChanged, EventUserRegistered,
};
use shieldd_sdk_fee::GasPrices;
use shieldd_sdk_proto::{
    core::component::compact_block::v1::CompactBlockRangeResponse,
    shieldd::core::component::compact_block::v1 as pb, DomainType,
};
use shieldd_sdk_sct::{nullifier_generation::NullifierWindow, Nullifier};
use shieldd_sdk_shielded_pool::discovery;
use shieldd_sdk_tct::{
    builder::{block, epoch},
    StateCommitment,
};

use super::{RoutingActionPayloads, RoutingRecord, StatePayload};

/// A compressed delta update with the minimal data from a block required to
/// synchronize private client state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pb::CompactBlock", into = "pb::CompactBlock")]
pub struct CompactBlock {
    pub height: u64,
    /// State payloads describing new state fragments.
    pub state_payloads: Vec<StatePayload>,
    /// Nullifiers identifying spent notes.
    pub nullifiers: Vec<Nullifier>,
    /// The block root of this block.
    pub block_root: block::Root,
    /// The epoch root of this epoch, if this block ends an epoch (`None` otherwise).
    pub epoch_root: Option<epoch::Root>,
    /// Latest discovery parameters. `None` if unchanged.
    pub discovery_parameters: Option<discovery::Parameters>,
    /// Fixed-shape action-level routing records.
    pub routing_records: Vec<RoutingRecord>,
    /// Encrypted note payloads grouped by their producing action.
    pub routing_action_payloads: Vec<RoutingActionPayloads>,
    /// Set if the app parameters have been updated. Notifies the client that it should re-sync from the fullnode RPC.
    pub app_parameters_updated: bool,
    /// Updated gas prices for the native token, if they have changed.
    pub gas_prices: Option<GasPrices>,
    // The epoch index
    pub epoch_index: u64,
    /// Compliance user tree anchor (root) for this block.
    pub compliance_user_anchor: Option<StateCommitment>,
    /// Compliance asset tree anchor (root) for this block.
    pub compliance_asset_anchor: Option<StateCommitment>,
    /// User registrations in this block (for compliance tree sync).
    pub compliance_user_registrations: Vec<EventUserRegistered>,
    /// In-place user status changes in this block.
    pub compliance_user_status_changes: Vec<EventUserAssetStatusChanged>,
    /// Asset registrations in this block (for compliance tree sync).
    pub compliance_asset_registrations: Vec<EventAssetRegistered>,
    /// Exact nullifier window, present at genesis and app-epoch boundaries.
    pub nullifier_window: Option<NullifierWindow>,
    // **IMPORTANT NOTE FOR FUTURE HUMANS**: if you want to add new fields to the `CompactBlock`,
    // you must update `CompactBlock::requires_scanning` to check for the emptiness of those fields,
    // because the client will skip processing any compact block that is marked as not requiring
    // scanning.
}

impl Default for CompactBlock {
    fn default() -> Self {
        Self {
            height: 0,
            state_payloads: Vec::new(),
            nullifiers: Vec::new(),
            block_root: block::Finalized::default().root(),
            epoch_root: None,
            discovery_parameters: None,
            routing_records: Vec::new(),
            routing_action_payloads: Vec::new(),
            app_parameters_updated: false,
            gas_prices: None,
            epoch_index: 0,
            compliance_user_anchor: None,
            compliance_asset_anchor: None,
            compliance_user_registrations: Vec::new(),
            compliance_user_status_changes: Vec::new(),
            compliance_asset_registrations: Vec::new(),
            nullifier_window: None,
        }
    }
}

impl CompactBlock {
    /// Returns true if the compact block contains any data that requires scanning.
    pub fn requires_scanning(&self) -> bool {
        !self.state_payloads.is_empty() // need to scan notes
            || !self.nullifiers.is_empty() // need to collect nullifiers
            || self.discovery_parameters.is_some() // need to save latest discovery parameters
            || !self.routing_records.is_empty() // need to scan routing records
            || self.app_parameters_updated // need to save latest app parameters
            || self.gas_prices.is_some() // need to save latest gas prices
            || !self.compliance_user_registrations.is_empty() // need to sync user tree
            || !self.compliance_user_status_changes.is_empty() // need to update user tree
            || !self.compliance_asset_registrations.is_empty() // need to sync asset tree
            || self.nullifier_window.is_some() // need to persist the planning window
    }
}

impl DomainType for CompactBlock {
    type Proto = pb::CompactBlock;
}

impl From<CompactBlock> for pb::CompactBlock {
    fn from(cb: CompactBlock) -> Self {
        pb::CompactBlock {
            height: cb.height,
            state_payloads: cb.state_payloads.into_iter().map(Into::into).collect(),
            nullifiers: cb.nullifiers.into_iter().map(Into::into).collect(),
            // We don't serialize block roots if they are the empty block, because we don't need to
            block_root: if cb.block_root.is_empty_finalized() {
                None
            } else {
                Some(cb.block_root.into())
            },
            epoch_root: cb.epoch_root.map(Into::into),
            discovery_parameters: cb.discovery_parameters.map(Into::into),
            routing_records: cb.routing_records.into_iter().map(Into::into).collect(),
            app_parameters_updated: cb.app_parameters_updated,
            gas_prices: cb.gas_prices.map(Into::into),
            epoch_index: cb.epoch_index,
            compliance_user_anchor: cb
                .compliance_user_anchor
                .map(|a| <[u8; 32]>::from(a).to_vec())
                .unwrap_or_default(),
            compliance_asset_anchor: cb
                .compliance_asset_anchor
                .map(|a| <[u8; 32]>::from(a).to_vec())
                .unwrap_or_default(),
            compliance_user_registrations: cb
                .compliance_user_registrations
                .into_iter()
                .map(Into::into)
                .collect(),
            compliance_user_status_changes: cb
                .compliance_user_status_changes
                .into_iter()
                .map(Into::into)
                .collect(),
            compliance_asset_registrations: cb
                .compliance_asset_registrations
                .into_iter()
                .map(Into::into)
                .collect(),
            routing_action_payloads: cb
                .routing_action_payloads
                .into_iter()
                .map(Into::into)
                .collect(),
            nullifier_window: cb.nullifier_window.map(Into::into),
        }
    }
}

impl TryFrom<pb::CompactBlock> for CompactBlock {
    type Error = anyhow::Error;

    fn try_from(value: pb::CompactBlock) -> Result<Self, Self::Error> {
        let compliance_user_anchor = if value.compliance_user_anchor.is_empty() {
            None
        } else {
            let bytes: [u8; 32] = value
                .compliance_user_anchor
                .try_into()
                .map_err(|_| anyhow::anyhow!("compliance_user_anchor must be 32 bytes"))?;
            Some(StateCommitment::try_from(bytes)?)
        };
        let compliance_asset_anchor = if value.compliance_asset_anchor.is_empty() {
            None
        } else {
            let bytes: [u8; 32] = value
                .compliance_asset_anchor
                .try_into()
                .map_err(|_| anyhow::anyhow!("compliance_asset_anchor must be 32 bytes"))?;
            Some(StateCommitment::try_from(bytes)?)
        };

        Ok(CompactBlock {
            height: value.height,
            state_payloads: value
                .state_payloads
                .into_iter()
                .map(StatePayload::try_from)
                .collect::<Result<Vec<StatePayload>>>()?,
            nullifiers: value
                .nullifiers
                .into_iter()
                .map(Nullifier::try_from)
                .collect::<Result<Vec<Nullifier>>>()?,
            block_root: value
                .block_root
                .map(TryInto::try_into)
                .transpose()?
                // If the block root wasn't present, that means it's the default finalized block root
                .unwrap_or_else(|| block::Finalized::default().root()),
            epoch_root: value.epoch_root.map(TryInto::try_into).transpose()?,
            discovery_parameters: value
                .discovery_parameters
                .map(TryInto::try_into)
                .transpose()?,
            routing_records: value
                .routing_records
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>>>()?,
            app_parameters_updated: value.app_parameters_updated,
            gas_prices: value.gas_prices.map(TryInto::try_into).transpose()?,
            epoch_index: value.epoch_index,
            compliance_user_anchor,
            compliance_asset_anchor,
            compliance_user_registrations: value
                .compliance_user_registrations
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<EventUserRegistered>>>()?,
            compliance_user_status_changes: value
                .compliance_user_status_changes
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<EventUserAssetStatusChanged>>>()?,
            compliance_asset_registrations: value
                .compliance_asset_registrations
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<EventAssetRegistered>>>()?,
            routing_action_payloads: value
                .routing_action_payloads
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>>>()?,
            nullifier_window: value.nullifier_window.map(TryInto::try_into).transpose()?,
        })
    }
}

impl From<CompactBlock> for CompactBlockRangeResponse {
    fn from(cb: CompactBlock) -> Self {
        Self {
            compact_block: Some(cb.into()),
        }
    }
}

impl TryFrom<CompactBlockRangeResponse> for CompactBlock {
    type Error = anyhow::Error;

    fn try_from(response: CompactBlockRangeResponse) -> Result<Self, Self::Error> {
        response
            .compact_block
            .ok_or_else(|| anyhow::anyhow!("empty CompactBlockRangeResponse message"))?
            .try_into()
    }
}
