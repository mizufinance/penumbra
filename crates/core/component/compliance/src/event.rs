use anyhow::{anyhow, Context as _};
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::{core::component::compliance::v1 as pb, DomainType, Name as _};
use penumbra_sdk_tct::StateCommitment;

/// Create a user registration event proto for emitting via record_proto.
pub fn user_registered(
    position: u64,
    commitment: StateCommitment,
    address: Address,
    asset_id: asset::Id,
) -> pb::EventUserRegistered {
    pb::EventUserRegistered {
        position,
        commitment: <[u8; 32]>::from(commitment).to_vec(),
        address: Some(address.into()),
        asset_id: Some(asset_id.into()),
    }
}

/// Create an asset registration event proto for emitting via record_proto.
pub fn asset_registered(
    asset_id: asset::Id,
    is_regulated: bool,
    position: u64,
) -> pb::EventAssetRegistered {
    pb::EventAssetRegistered {
        asset_id: Some(asset_id.into()),
        is_regulated,
        position,
    }
}

/// Create a compliance anchor event proto for emitting via record_proto.
pub fn compliance_anchor(
    height: u64,
    user_anchor: StateCommitment,
    asset_anchor: StateCommitment,
) -> pb::EventComplianceAnchor {
    pb::EventComplianceAnchor {
        height,
        user_anchor: <[u8; 32]>::from(user_anchor).to_vec(),
        asset_anchor: <[u8; 32]>::from(asset_anchor).to_vec(),
    }
}

// Domain types for parsing events

#[derive(Debug, Clone)]
pub struct EventUserRegistered {
    pub position: u64,
    pub commitment: StateCommitment,
    pub address: Address,
    pub asset_id: asset::Id,
}

impl DomainType for EventUserRegistered {
    type Proto = pb::EventUserRegistered;
}

impl TryFrom<pb::EventUserRegistered> for EventUserRegistered {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventUserRegistered) -> Result<Self, Self::Error> {
        fn inner(value: pb::EventUserRegistered) -> anyhow::Result<EventUserRegistered> {
            let commitment_bytes: [u8; 32] = value
                .commitment
                .try_into()
                .map_err(|_| anyhow!("commitment must be 32 bytes"))?;
            let commitment = StateCommitment::try_from(commitment_bytes)?;

            Ok(EventUserRegistered {
                position: value.position,
                commitment,
                address: value
                    .address
                    .ok_or(anyhow!("missing `address`"))?
                    .try_into()?,
                asset_id: value
                    .asset_id
                    .ok_or(anyhow!("missing `asset_id`"))?
                    .try_into()?,
            })
        }
        inner(value).context(format!("parsing {}", pb::EventUserRegistered::NAME))
    }
}

impl From<EventUserRegistered> for pb::EventUserRegistered {
    fn from(value: EventUserRegistered) -> Self {
        Self {
            position: value.position,
            commitment: <[u8; 32]>::from(value.commitment).to_vec(),
            address: Some(value.address.into()),
            asset_id: Some(value.asset_id.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventAssetRegistered {
    pub asset_id: asset::Id,
    pub is_regulated: bool,
    pub position: u64,
}

impl DomainType for EventAssetRegistered {
    type Proto = pb::EventAssetRegistered;
}

impl TryFrom<pb::EventAssetRegistered> for EventAssetRegistered {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventAssetRegistered) -> Result<Self, Self::Error> {
        fn inner(value: pb::EventAssetRegistered) -> anyhow::Result<EventAssetRegistered> {
            Ok(EventAssetRegistered {
                asset_id: value
                    .asset_id
                    .ok_or(anyhow!("missing `asset_id`"))?
                    .try_into()?,
                is_regulated: value.is_regulated,
                position: value.position,
            })
        }
        inner(value).context(format!("parsing {}", pb::EventAssetRegistered::NAME))
    }
}

impl From<EventAssetRegistered> for pb::EventAssetRegistered {
    fn from(value: EventAssetRegistered) -> Self {
        Self {
            asset_id: Some(value.asset_id.into()),
            is_regulated: value.is_regulated,
            position: value.position,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventComplianceAnchor {
    pub height: u64,
    pub user_anchor: StateCommitment,
    pub asset_anchor: StateCommitment,
}

impl DomainType for EventComplianceAnchor {
    type Proto = pb::EventComplianceAnchor;
}

impl TryFrom<pb::EventComplianceAnchor> for EventComplianceAnchor {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventComplianceAnchor) -> Result<Self, Self::Error> {
        fn inner(value: pb::EventComplianceAnchor) -> anyhow::Result<EventComplianceAnchor> {
            let user_bytes: [u8; 32] = value
                .user_anchor
                .try_into()
                .map_err(|_| anyhow!("user_anchor must be 32 bytes"))?;
            let asset_bytes: [u8; 32] = value
                .asset_anchor
                .try_into()
                .map_err(|_| anyhow!("asset_anchor must be 32 bytes"))?;

            Ok(EventComplianceAnchor {
                height: value.height,
                user_anchor: StateCommitment::try_from(user_bytes)?,
                asset_anchor: StateCommitment::try_from(asset_bytes)?,
            })
        }
        inner(value).context(format!("parsing {}", pb::EventComplianceAnchor::NAME))
    }
}

impl From<EventComplianceAnchor> for pb::EventComplianceAnchor {
    fn from(value: EventComplianceAnchor) -> Self {
        Self {
            height: value.height,
            user_anchor: <[u8; 32]>::from(value.user_anchor).to_vec(),
            asset_anchor: <[u8; 32]>::from(value.asset_anchor).to_vec(),
        }
    }
}
