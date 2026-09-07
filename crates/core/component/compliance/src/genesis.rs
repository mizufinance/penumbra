//! Genesis configuration for the compliance component.
//!
//! This module defines the genesis content structure for configuring
//! compliance asset entries at chain initialization.
//!
//! The IMT always contains a structural sentinel leaf. Regulated assets may be
//! configured here; unregulated assets use IMT non-membership proofs.

use decaf377_rdsa::{SpendAuth, VerificationKey};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::ensure_nonidentity_spend_auth_key;
use shieldd_sdk_proto::{shieldd::core::component::compliance::v1 as pb, DomainType};

use crate::{
    params::ComplianceParameters,
    structs::{AssetPolicy, ComplianceLeaf, OrbisCapabilityCertificate, UserAssetStatus},
};

/// Certified active user installed before regulated genesis allocations.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GenesisUserRegistration {
    pub leaf: ComplianceLeaf,
    pub capability_certificate: OrbisCapabilityCertificate,
}

impl DomainType for GenesisUserRegistration {
    type Proto = pb::GenesisUserRegistration;
}

impl TryFrom<pb::GenesisUserRegistration> for GenesisUserRegistration {
    type Error = anyhow::Error;

    fn try_from(value: pb::GenesisUserRegistration) -> Result<Self, Self::Error> {
        Ok(Self {
            leaf: value
                .leaf
                .ok_or_else(|| anyhow::anyhow!("missing genesis user leaf"))?
                .try_into()?,
            capability_certificate: value
                .capability_certificate
                .ok_or_else(|| anyhow::anyhow!("missing genesis Orbis capability certificate"))?
                .try_into()?,
        })
    }
}

impl From<GenesisUserRegistration> for pb::GenesisUserRegistration {
    fn from(value: GenesisUserRegistration) -> Self {
        Self {
            leaf: Some(value.leaf.into()),
            capability_certificate: Some(value.capability_certificate.into()),
        }
    }
}

/// Genesis content for the compliance component.
///
/// This allows configuring additional compliance asset entries at genesis.
/// The IMT already contains a structural sentinel; entries listed here are
/// regulated additions on top of that baseline.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Content {
    /// Compliance component parameters at genesis.
    #[serde(default)]
    pub compliance_params: ComplianceParameters,
    /// Native assets to register explicitly at genesis.
    pub native_assets: Vec<NativeAssetRegistration>,
    /// Compliance registrar keys authorized to register asset policies.
    pub compliance_registrar_vk: Vec<VerificationKey<SpendAuth>>,
    /// Active users installed before regulated genesis allocations are minted.
    #[serde(default)]
    pub user_registrations: Vec<GenesisUserRegistration>,
}

impl Content {
    pub fn validate_authorization_keys(&self) -> anyhow::Result<()> {
        for registrar_vk in &self.compliance_registrar_vk {
            ensure_nonidentity_spend_auth_key(registrar_vk, "compliance registrar key")?;
        }
        for registration in &self.native_assets {
            registration.validate_authorization_keys()?;
        }
        for registration in &self.user_registrations {
            registration.leaf.validate()?;
            anyhow::ensure!(
                registration.leaf.status == UserAssetStatus::Active,
                "genesis compliance users must start active"
            );
        }
        Ok(())
    }
}

impl DomainType for Content {
    type Proto = pb::GenesisContent;
}

impl TryFrom<pb::GenesisContent> for Content {
    type Error = anyhow::Error;

    fn try_from(value: pb::GenesisContent) -> Result<Self, Self::Error> {
        let content = Self {
            compliance_params: value
                .compliance_params
                .map(TryInto::try_into)
                .transpose()?
                .unwrap_or_default(),
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
            user_registrations: value
                .user_registrations
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        };
        content.validate_authorization_keys()?;
        Ok(content)
    }
}

impl From<Content> for pb::GenesisContent {
    fn from(value: Content) -> Self {
        Self {
            compliance_params: Some(value.compliance_params.into()),
            native_assets: value.native_assets.into_iter().map(Into::into).collect(),
            compliance_registrar_vk: value
                .compliance_registrar_vk
                .into_iter()
                .map(Into::into)
                .collect(),
            user_registrations: value
                .user_registrations
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
    /// Immutable authority key that signs note seizures for this asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seizure_authority_vk: Option<VerificationKey<SpendAuth>>,
    /// Immutable aggregate Orbis ring public key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring_pk: Option<[u8; 32]>,
    /// Orbis ring identifier.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ring_id: String,
    /// ACP policy identifier.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub policy_id: String,
    /// ACP permission required for releases.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub permission: String,
    /// ACP resource required for releases.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resource: String,
}

impl NativeAssetRegistration {
    pub fn validate_authorization_keys(&self) -> anyhow::Result<()> {
        if let Some(registration_authority_vk) = &self.registration_authority_vk {
            ensure_nonidentity_spend_auth_key(
                registration_authority_vk,
                "compliance registration authority key",
            )?;
        }
        if let Some(seizure_authority_vk) = &self.seizure_authority_vk {
            ensure_nonidentity_spend_auth_key(
                seizure_authority_vk,
                "compliance seizure authority key",
            )?;
        }
        Ok(())
    }

    /// Build and validate the on-chain policy represented by this registration.
    pub fn asset_policy(&self) -> anyhow::Result<AssetPolicy> {
        if !self.is_regulated {
            anyhow::ensure!(
                self.dk_pub.is_none(),
                "unregulated genesis asset cannot set dk_pub"
            );
            anyhow::ensure!(
                self.registration_authority_vk.is_none() && self.seizure_authority_vk.is_none(),
                "unregulated genesis asset cannot set compliance authorities"
            );
            anyhow::ensure!(
                self.ring_pk.is_none()
                    && self.ring_id.is_empty()
                    && self.policy_id.is_empty()
                    && self.permission.is_empty()
                    && self.resource.is_empty(),
                "unregulated genesis asset cannot set Orbis configuration"
            );
            return Ok(AssetPolicy::default_unregulated());
        }
        let dk_pub = decaf377::Encoding(
            self.dk_pub
                .ok_or_else(|| anyhow::anyhow!("regulated genesis asset requires dk_pub"))?,
        )
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid regulated genesis dk_pub"))?;
        let registration_authority_vk = self.registration_authority_vk.ok_or_else(|| {
            anyhow::anyhow!("regulated genesis asset requires registration_authority_vk")
        })?;
        let seizure_authority_vk = self.seizure_authority_vk.ok_or_else(|| {
            anyhow::anyhow!("regulated genesis asset requires seizure_authority_vk")
        })?;
        let ring_pk = decaf377::Encoding(
            self.ring_pk
                .ok_or_else(|| anyhow::anyhow!("regulated genesis asset requires ring_pk"))?,
        )
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid regulated genesis ring_pk"))?;
        let policy = AssetPolicy::new(
            dk_pub,
            u128::MAX,
            vec![],
            None,
            self.ring_id.clone(),
            ring_pk,
            self.policy_id.clone(),
            self.permission.clone(),
            self.resource.clone(),
        )
        .with_registration_authority(registration_authority_vk)
        .with_seizure_authority(seizure_authority_vk);
        policy.validate_regulated()?;
        Ok(policy)
    }
}

impl DomainType for NativeAssetRegistration {
    type Proto = pb::NativeAssetRegistration;
}

impl TryFrom<pb::NativeAssetRegistration> for NativeAssetRegistration {
    type Error = anyhow::Error;

    fn try_from(value: pb::NativeAssetRegistration) -> Result<Self, Self::Error> {
        let registration = Self {
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
            seizure_authority_vk: value
                .seizure_authority_vk
                .map(TryInto::try_into)
                .transpose()
                .map_err(|e| anyhow::anyhow!("invalid genesis seizure_authority_vk: {e}"))?,
            ring_pk: if value.ring_pk.is_empty() {
                None
            } else {
                Some(
                    value
                        .ring_pk
                        .as_slice()
                        .try_into()
                        .map_err(|e| anyhow::anyhow!("genesis ring_pk must be 32 bytes: {e}"))?,
                )
            },
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
        };
        registration.validate_authorization_keys()?;
        Ok(registration)
    }
}

impl From<NativeAssetRegistration> for pb::NativeAssetRegistration {
    fn from(value: NativeAssetRegistration) -> Self {
        Self {
            asset_id: Some(value.asset_id.into()),
            is_regulated: value.is_regulated,
            dk_pub: value.dk_pub.map(Vec::from).unwrap_or_default(),
            registration_authority_vk: value.registration_authority_vk.map(Into::into),
            seizure_authority_vk: value.seizure_authority_vk.map(Into::into),
            ring_pk: value.ring_pk.map(Vec::from).unwrap_or_default(),
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
        }
    }
}

#[cfg(test)]
mod tests {
    use decaf377::Fr;
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};

    use super::*;

    #[test]
    fn test_default_genesis() {
        let content = Content::default();
        assert!(content.native_assets.is_empty());
        assert!(content.user_registrations.is_empty());
        assert_eq!(
            content.compliance_params.anchor_validation_window_blocks,
            crate::params::ComplianceParameters::default().anchor_validation_window_blocks
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let content = Content::default();
        let json = serde_json::to_string(&content).unwrap();
        let parsed: Content = serde_json::from_str(&json).unwrap();
        assert_eq!(content.native_assets.len(), parsed.native_assets.len());
    }

    #[test]
    fn genesis_rejects_identity_authorization_keys() {
        let identity = VerificationKey::from(&SigningKey::<SpendAuth>::from(Fr::from(0u64)));
        let registrar_content = Content {
            compliance_registrar_vk: vec![identity],
            ..Default::default()
        };
        let registrar_error = registrar_content
            .validate_authorization_keys()
            .expect_err("genesis registrar keys must be nonidentity");
        assert!(
            registrar_error
                .to_string()
                .contains("compliance registrar key must not be identity"),
            "unexpected rejection reason: {registrar_error:#}"
        );

        let authority_content = Content {
            native_assets: vec![NativeAssetRegistration {
                asset_id: asset::Id(decaf377::Fq::from(1u64)),
                is_regulated: false,
                dk_pub: None,
                registration_authority_vk: Some(identity),
                seizure_authority_vk: None,
                ring_pk: None,
                ring_id: String::new(),
                policy_id: String::new(),
                permission: String::new(),
                resource: String::new(),
            }],
            ..Default::default()
        };
        let authority_error = authority_content
            .validate_authorization_keys()
            .expect_err("genesis registration authority keys must be nonidentity");
        assert!(
            authority_error
                .to_string()
                .contains("compliance registration authority key must not be identity"),
            "unexpected rejection reason: {authority_error:#}"
        );
    }

    #[test]
    fn regulated_genesis_asset_requires_complete_orbis_configuration() {
        let authority = VerificationKey::from(&SigningKey::<SpendAuth>::from(Fr::from(7u64)));
        let mut registration = NativeAssetRegistration {
            asset_id: asset::Id(decaf377::Fq::from(2u64)),
            is_regulated: true,
            dk_pub: Some(decaf377::Element::GENERATOR.vartime_compress().0),
            registration_authority_vk: Some(authority),
            seizure_authority_vk: Some(authority),
            ring_pk: Some(decaf377::Element::GENERATOR.vartime_compress().0),
            ring_id: "ring".to_owned(),
            policy_id: "policy".to_owned(),
            permission: "read".to_owned(),
            resource: "document".to_owned(),
        };
        registration.asset_policy().unwrap();

        registration.policy_id.clear();
        assert!(registration.asset_policy().is_err());
    }
}
