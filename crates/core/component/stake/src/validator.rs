//! Penumbra validators and related structures.

use penumbra_sdk_proto::{penumbra::core::component::validator::v1 as pb, DomainType};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::{GovernanceKey, IdentityKey};

mod definition;
mod info;
mod state;
mod status;

pub use definition::Definition;
pub use info::Info;
pub use state::State;
pub use status::Status;

/// Describes a Penumbra validator's configuration data.
///
/// This data is unauthenticated; the [`Definition`] action includes
/// a signature over the transaction with the validator's identity key.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(try_from = "pb::Validator", into = "pb::Validator")]
pub struct Validator {
    /// The validator's identity verification key.
    pub identity_key: IdentityKey,

    /// The validator's governance verification key.
    pub governance_key: GovernanceKey,

    /// The validator's consensus key, used by Tendermint for signing blocks and
    /// other consensus operations.
    pub consensus_key: tendermint::PublicKey,

    /// The validator's (human-readable) name.
    /// Length: <= 140 characters.
    pub name: String,

    /// The validator's website URL.
    /// Length: <= 70 characters.
    pub website: String,

    /// The validator's description.
    /// Length: <= 280 characters.
    pub description: String,

    /// Whether the validator is enabled or not.
    ///
    /// Disabled validators immediately leave the active rotation.
    pub enabled: bool,

    /// The sequence number determines which validator data takes priority, and
    /// prevents replay attacks.  The chain only accepts new
    /// [`Definition`]s with increasing sequence numbers, preventing a
    /// third party from replaying previously valid but stale configuration data
    /// as an update.
    pub sequence_number: u32,
}

#[serde_as]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ValidatorToml {
    /// The sequence number determines which validator data takes priority, and
    /// prevents replay attacks.  The chain only accepts new [`Definition`]s with
    /// with increasing sequence numbers, preventing a third-party from replaying
    /// previously valid but stale configuration data as an update.
    pub sequence_number: u32,

    /// Whether the validator is enabled or not.
    ///
    /// Disabled validators immediately leave the active rotation.
    pub enabled: bool,

    /// The validator's (human-readable) name.
    pub name: String,

    /// The validator's website URL.
    pub website: String,

    /// The validator's description.
    pub description: String,

    /// The validator's identity verification key.
    #[serde_as(as = "DisplayFromStr")]
    pub identity_key: IdentityKey,

    /// The validator's governance verification key.
    #[serde_as(as = "DisplayFromStr")]
    pub governance_key: GovernanceKey,

    /// The validator's consensus key, used by Tendermint for signing blocks and
    /// other consensus operations.
    pub consensus_key: tendermint::PublicKey,
}

impl From<Validator> for ValidatorToml {
    fn from(v: Validator) -> Self {
        ValidatorToml {
            identity_key: v.identity_key,
            governance_key: v.governance_key,
            consensus_key: v.consensus_key,
            name: v.name,
            website: v.website,
            description: v.description,
            enabled: v.enabled,
            sequence_number: v.sequence_number,
        }
    }
}

impl TryFrom<ValidatorToml> for Validator {
    type Error = anyhow::Error;

    fn try_from(v: ValidatorToml) -> anyhow::Result<Self> {
        // Validation:
        // - Website has a max length of 70 bytes
        if v.website.len() > 70 {
            anyhow::bail!("validator website field must be less than 70 bytes");
        }

        // - Name has a max length of 140 bytes
        if v.name.len() > 140 {
            anyhow::bail!("validator name must be less than 140 bytes");
        }

        // - Description has a max length of 280 bytes
        if v.description.len() > 280 {
            anyhow::bail!("validator description must be less than 280 bytes");
        }

        Ok(Validator {
            identity_key: v.identity_key,
            governance_key: v.governance_key,
            consensus_key: v.consensus_key,
            name: v.name,
            website: v.website,
            description: v.description,
            enabled: v.enabled,
            sequence_number: v.sequence_number,
        })
    }
}

impl DomainType for Validator {
    type Proto = pb::Validator;
}

impl From<Validator> for pb::Validator {
    fn from(v: Validator) -> Self {
        pb::Validator {
            identity_key: Some(v.identity_key.into()),
            governance_key: Some(v.governance_key.into()),
            consensus_key: v.consensus_key.to_bytes(),
            name: v.name,
            website: v.website,
            description: v.description,
            enabled: v.enabled,
            sequence_number: v.sequence_number,
        }
    }
}

impl TryFrom<pb::Validator> for Validator {
    type Error = anyhow::Error;
    fn try_from(v: pb::Validator) -> Result<Self, Self::Error> {
        // Validation:
        // - Website has a max length of 70 bytes
        if v.website.len() > 70 {
            anyhow::bail!("validator website field must be less than 70 bytes");
        }

        // - Name has a max length of 140 bytes
        if v.name.len() > 140 {
            anyhow::bail!("validator name must be less than 140 bytes");
        }

        // - Description has a max length of 280 bytes
        if v.description.len() > 280 {
            anyhow::bail!("validator description must be less than 280 bytes");
        }

        Ok(Validator {
            identity_key: v
                .identity_key
                .ok_or_else(|| anyhow::anyhow!("missing identity key"))?
                .try_into()?,
            governance_key: v
                .governance_key
                .ok_or_else(|| anyhow::anyhow!("missing governance key"))?
                .try_into()?,
            consensus_key: tendermint::PublicKey::from_raw_ed25519(&v.consensus_key)
                .ok_or_else(|| anyhow::anyhow!("invalid ed25519 consensus pubkey"))?,
            name: v.name,
            website: v.website,
            description: v.description,
            enabled: v.enabled,
            sequence_number: v.sequence_number,
        })
    }
}
