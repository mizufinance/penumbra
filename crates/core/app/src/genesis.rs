use serde::{Deserialize, Serialize};
use shieldd_sdk_compliance::genesis::Content as ComplianceContent;
use shieldd_sdk_fee::genesis::Content as FeeContent;
use shieldd_sdk_proto::{shieldd::core::app::v1 as pb, DomainType};
use shieldd_sdk_sct::genesis::Content as SctContent;
use shieldd_sdk_shielded_pool::genesis::Content as ShieldedPoolContent;

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
    /// SCT module genesis state.
    pub sct_content: SctContent,
    /// Shielded pool module genesis state.
    pub shielded_pool_content: ShieldedPoolContent,
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
            sct_content: Some(genesis.sct_content.into()),
            shielded_pool_content: Some(genesis.shielded_pool_content.into()),
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
            fee_content: msg
                .fee_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing fee content"))?
                .try_into()?,
            compliance_content: msg
                .compliance_content
                .map(TryInto::try_into)
                .transpose()?
                .unwrap_or_default(),
            sct_content: msg
                .sct_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing sct content"))?
                .try_into()?,
            shielded_pool_content: msg
                .shielded_pool_content
                .ok_or_else(|| anyhow::anyhow!("proto response missing shielded pool content"))?
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
            sct_content: shieldd_sdk_sct::genesis::Content {
                sct_params: shieldd_sdk_sct::params::SctParameters {
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
            shieldd_sdk_compliance::params::ComplianceParameters::default()
                .anchor_validation_window_blocks
        );
        Ok(())
    }
}
