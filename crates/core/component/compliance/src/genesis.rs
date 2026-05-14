//! Genesis configuration for the compliance component.
//!
//! This module defines the genesis content structure for configuring
//! compliance asset entries at chain initialization.
//!
//! The IMT always contains a structural sentinel leaf and the protocol also
//! seeds the neutral base asset as an explicit unregulated entry. Additional
//! regulated assets may be configured here, while other unregulated assets
//! continue to use IMT non-membership proofs.

use decaf377_rdsa::{SpendAuth, VerificationKey};
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::{penumbra::core::component::compliance::v1 as pb, DomainType};
use serde::{Deserialize, Serialize};

/// Genesis content for the compliance component.
///
/// This allows configuring additional compliance asset entries at genesis.
/// The IMT already contains a structural sentinel and a seeded unregulated base
/// asset; entries listed here are added on top of that baseline.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Content {
    /// Native assets to register explicitly at genesis.
    pub native_assets: Vec<NativeAssetRegistration>,
    /// Compliance registrar keys authorized to register asset policies.
    pub compliance_registrar_vk: Vec<VerificationKey<SpendAuth>>,
}

impl DomainType for Content {
    type Proto = pb::GenesisContent;
}

impl TryFrom<pb::GenesisContent> for Content {
    type Error = anyhow::Error;

    fn try_from(value: pb::GenesisContent) -> Result<Self, Self::Error> {
        Ok(Self {
            native_assets: value
                .native_assets
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            compliance_registrar_vk: value
                .compliance_registrar_vk
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl From<Content> for pb::GenesisContent {
    fn from(value: Content) -> Self {
        Self {
            native_assets: value.native_assets.into_iter().map(Into::into).collect(),
            compliance_registrar_vk: value
                .compliance_registrar_vk
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

/// Registration configuration for a native asset at genesis.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NativeAssetRegistration {
    /// The asset ID to register.
    pub asset_id: asset::Id,
    /// Whether this asset is regulated (requires compliance proofs).
    pub is_regulated: bool,
    /// Issuer detection key (required if is_regulated is true).
    /// Encoded as 32-byte compressed curve point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dk_pub: Option<[u8; 32]>,
    /// Immutable authority key that signs user registration grants for this asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_authority_vk: Option<VerificationKey<SpendAuth>>,
}

impl DomainType for NativeAssetRegistration {
    type Proto = pb::NativeAssetRegistration;
}

impl TryFrom<pb::NativeAssetRegistration> for NativeAssetRegistration {
    type Error = anyhow::Error;

    fn try_from(value: pb::NativeAssetRegistration) -> Result<Self, Self::Error> {
        Ok(Self {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing genesis native asset_id"))?
                .try_into()?,
            is_regulated: value.is_regulated,
            dk_pub: if value.dk_pub.is_empty() {
                None
            } else {
                Some(
                    value
                        .dk_pub
                        .as_slice()
                        .try_into()
                        .map_err(|e| anyhow::anyhow!("genesis dk_pub must be 32 bytes: {e}"))?,
                )
            },
            registration_authority_vk: value
                .registration_authority_vk
                .map(TryInto::try_into)
                .transpose()
                .map_err(|e| anyhow::anyhow!("invalid genesis registration_authority_vk: {e}"))?,
        })
    }
}

impl From<NativeAssetRegistration> for pb::NativeAssetRegistration {
    fn from(value: NativeAssetRegistration) -> Self {
        Self {
            asset_id: Some(value.asset_id.into()),
            is_regulated: value.is_regulated,
            dk_pub: value.dk_pub.map(Vec::from).unwrap_or_default(),
            registration_authority_vk: value.registration_authority_vk.map(Into::into),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_genesis() {
        let content = Content::default();
        assert!(content.native_assets.is_empty());
    }

    #[test]
    fn test_serde_roundtrip() {
        let content = Content::default();
        let json = serde_json::to_string(&content).unwrap();
        let parsed: Content = serde_json::from_str(&json).unwrap();
        assert_eq!(content.native_assets.len(), parsed.native_assets.len());
    }
}
