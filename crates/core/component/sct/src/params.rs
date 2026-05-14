use penumbra_sdk_proto::penumbra::core::component::sct::v1 as pb;
use penumbra_sdk_proto::DomainType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::SctParameters", into = "pb::SctParameters")]
/// The configuration parameters for the SCT component.
pub struct SctParameters {
    /// The "default" duration of an epoch in number of blocks.
    /// Note that this is a soft target, and a variety of events
    /// can trigger an epoch transition.
    pub epoch_duration: u64,
    /// Number of recent SCT anchors and historical block metadata retained for witness refresh.
    pub sct_anchor_retention_blocks: u64,
}

impl DomainType for SctParameters {
    type Proto = pb::SctParameters;
}

impl TryFrom<pb::SctParameters> for SctParameters {
    type Error = anyhow::Error;

    fn try_from(msg: pb::SctParameters) -> anyhow::Result<Self> {
        Ok(SctParameters {
            epoch_duration: msg.epoch_duration,
            sct_anchor_retention_blocks: msg.sct_anchor_retention_blocks,
        })
    }
}

impl From<SctParameters> for pb::SctParameters {
    fn from(params: SctParameters) -> Self {
        pb::SctParameters {
            epoch_duration: params.epoch_duration,
            sct_anchor_retention_blocks: params.sct_anchor_retention_blocks,
        }
    }
}

impl Default for SctParameters {
    fn default() -> Self {
        Self {
            // Measured in blocks, assuming a 5s block time
            // this is about a day worth of blocks.
            epoch_duration: 17280,
            // 14 days at today's 5s target block time.
            sct_anchor_retention_blocks: (14 * 24 * 3600) / 5,
        }
    }
}
