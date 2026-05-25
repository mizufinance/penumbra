use penumbra_sdk_compliance::genesis::Content as ComplianceContent;
use penumbra_sdk_fee::genesis::Content as FeeContent;
use penumbra_sdk_governance::genesis::Content as GovernanceContent;
use penumbra_sdk_ibc::genesis::Content as IBCContent;
use penumbra_sdk_proto::{penumbra::core::app::v1 as pb, DomainType};
use penumbra_sdk_sct::genesis::Content as SctContent;
use penumbra_sdk_shielded_pool::genesis::Content as ShieldedPoolContent;
use penumbra_sdk_validator::genesis::Content as ValidatorContent;
use serde::{Deserialize, Serialize};

/// The application state at genesis.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(try_from = "pb::GenesisAppState", into = "pb::GenesisAppState")]
#[allow(clippy::large_enum_variant)]
pub enum AppState {
    /// The application state at genesis.
    Content(Content),
    /// The checkpointed application state at genesis, contains a free-form hash.
    Checkpoint(Vec<u8>),
}

impl AppState {
    pub fn content(&self) -> Option<&Content> {
        match self {
            AppState::Content(content) => Some(content),
            _ => None,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(try_from = "pb::GenesisContent", into = "pb::GenesisContent")]
pub struct Content {
    /// The chain ID.
    pub chain_id: String,
    /// Fee module genesis state.
    pub fee_content: FeeContent,
    /// Compliance module genesis state.
    pub compliance_content: ComplianceContent,
    /// Governance module genesis state.
    pub governance_content: GovernanceContent,
    /// IBC module genesis state.
    pub ibc_content: IBCContent,
    // Sct module genesis state.
    pub sct_content: SctContent,
    /// Shielded pool module genesis state.
    pub shielded_pool_content: ShieldedPoolContent,
    /// Validator component genesis state.
    pub validator_content: ValidatorContent,
}

impl DomainType for Content {
    type Proto = pb::GenesisContent;
}

impl Default for AppState {
    fn default() -> Self {
        Self::Content(Default::default())
    }
}

impl From<AppState> for pb::GenesisAppState {
    fn from(a: AppState) -> Self {
        let genesis_state = match a {
            AppState::Content(c) => {
                pb::genesis_app_state::GenesisAppState::GenesisContent(c.into())
            }
            AppState::Checkpoint(h) => pb::genesis_app_state::GenesisAppState::GenesisCheckpoint(h),
        };

        pb::GenesisAppState {
            genesis_app_state: Some(genesis_state),
        }
    }
}

impl From<Content> for pb::GenesisContent {
    fn from(genesis: Content) -> Self {
        pb::GenesisContent {
            chain_id: genesis.chain_id,
            fee_content: Some(genesis.fee_content.into()),
            compliance_content: Some(genesis.compliance_content.into()),
            governance_content: Some(genesis.governance_content.into()),
            ibc_content: Some(genesis.ibc_content.into()),
            sct_content: Some(genesis.sct_content.into()),
            shielded_pool_content: Some(genesis.shielded_pool_content.into()),
            validator_content: Some(genesis.validator_content.into()),
        }
    }
}

impl TryFrom<pb::GenesisAppState> for AppState {
    type Error = anyhow::Error;

    fn try_from(msg: pb::GenesisAppState) -> Result<Self, Self::Error> {
        let state = msg
            .genesis_app_state
            .ok_or_else(|| anyhow::anyhow!("missing genesis_app_state field in proto"))?;
        match state {
            pb::genesis_app_state::GenesisAppState::GenesisContent(c) => {
                Ok(AppState::Content(c.try_into()?))
            }
            pb::genesis_app_state::GenesisAppState::GenesisCheckpoint(h) => {
                Ok(AppState::Checkpoint(h))
            }
        }
    }
}

impl TryFrom<pb::GenesisContent> for Content {
    type Error = anyhow::Error;

    fn try_from(msg: pb::GenesisContent) -> Result<Self, Self::Error> {
        Ok(Content {
            chain_id: msg.chain_id,
            governance_content: msg
                .governance_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing governance content"))?
                .try_into()?,
            fee_content: msg
                .fee_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing fee content"))?
                .try_into()?,
            compliance_content: msg
                .compliance_content
                .map(TryInto::try_into)
                .transpose()?
                .unwrap_or_default(),
            ibc_content: msg
                .ibc_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing ibc content"))?
                .try_into()?,
            sct_content: msg
                .sct_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing sct content"))?
                .try_into()?,
            shielded_pool_content: msg
                .shielded_pool_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing shielded pool content"))?
                .try_into()?,
            validator_content: msg
                .validator_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing validator content"))?
                .try_into()?,
        })
    }
}

impl DomainType for AppState {
    type Proto = pb::GenesisAppState;
}

impl Content {
    pub fn with_chain_id(self, chain_id: String) -> Self {
        Self { chain_id, ..self }
    }

    pub fn with_epoch_duration(self, epoch_duration: u64) -> Self {
        Self {
            sct_content: penumbra_sdk_sct::genesis::Content {
                sct_params: penumbra_sdk_sct::params::SctParameters {
                    epoch_duration,
                    ..Default::default()
                },
            },
            ..self
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    /// Check that the default implementation of contains zero validators,
    /// requiring validators to be passed in out of band. N.B. there's also a
    /// `validators` field in the [`tendermint::Genesis`] struct, which we don't use,
    /// preferring the AppState definition instead.
    #[test]
    fn check_validator_defaults() -> anyhow::Result<()> {
        let a = Content {
            ..Default::default()
        };
        assert!(a.validator_content.validators.is_empty());
        Ok(())
    }

    #[test]
    fn missing_compliance_content_uses_default() -> anyhow::Result<()> {
        let mut proto: pb::GenesisContent = Content::default().into();
        proto.compliance_content = None;

        let content = Content::try_from(proto)?;

        assert!(content.compliance_content.native_assets.is_empty());
        assert!(content
            .compliance_content
            .compliance_registrar_vk
            .is_empty());
        assert_eq!(
            content
                .compliance_content
                .compliance_params
                .anchor_validation_window_blocks,
            penumbra_sdk_compliance::params::ComplianceParameters::default()
                .anchor_validation_window_blocks
        );
        Ok(())
    }
}
