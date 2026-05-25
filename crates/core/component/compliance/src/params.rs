#[cfg(feature = "component")]
use anyhow::{anyhow, Result};
#[cfg(feature = "component")]
use async_trait::async_trait;
#[cfg(feature = "component")]
use cnidarium::{StateRead, StateWrite};
use penumbra_sdk_proto::penumbra::core::component::compliance::v1 as pb;
use penumbra_sdk_proto::DomainType;
#[cfg(feature = "component")]
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use serde::{Deserialize, Serialize};

#[cfg(feature = "component")]
use crate::state_key;

/// Compliance component chain parameters.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    try_from = "pb::ComplianceParameters",
    into = "pb::ComplianceParameters"
)]
pub struct ComplianceParameters {
    /// Number of recent compliance anchors accepted for proof validation.
    pub anchor_validation_window_blocks: u64,
}

impl DomainType for ComplianceParameters {
    type Proto = pb::ComplianceParameters;
}

impl TryFrom<pb::ComplianceParameters> for ComplianceParameters {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ComplianceParameters) -> anyhow::Result<Self> {
        Ok(Self {
            anchor_validation_window_blocks: msg.anchor_validation_window_blocks,
        })
    }
}

impl From<ComplianceParameters> for pb::ComplianceParameters {
    fn from(params: ComplianceParameters) -> Self {
        Self {
            anchor_validation_window_blocks: params.anchor_validation_window_blocks,
        }
    }
}

impl Default for ComplianceParameters {
    fn default() -> Self {
        Self {
            // 14 days at today's 5s target block time.
            anchor_validation_window_blocks: (14 * 24 * 3600) / 5,
        }
    }
}

/// Read access to compliance component parameters.
#[cfg(feature = "component")]
#[async_trait]
pub trait StateReadExt: StateRead {
    async fn get_compliance_params(&self) -> Result<ComplianceParameters> {
        self.get(state_key::config::compliance_params())
            .await?
            .ok_or_else(|| anyhow!("Missing ComplianceParameters"))
    }
}

#[cfg(feature = "component")]
impl<T: StateRead + ?Sized> StateReadExt for T {}

/// Write access to compliance component parameters.
#[cfg(feature = "component")]
#[async_trait]
pub trait StateWriteExt: StateWrite {
    fn put_compliance_params(&mut self, params: ComplianceParameters) {
        self.put(state_key::config::compliance_params().to_string(), params);
    }
}

#[cfg(feature = "component")]
impl<T: StateWrite + ?Sized> StateWriteExt for T {}
