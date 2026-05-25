use anyhow::Context;
use penumbra_sdk_proto::core::component::governance::v1 as pb;
use penumbra_sdk_proto::DomainType;
use serde::{Deserialize, Serialize};

use crate::tally::Ratio;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    try_from = "pb::GovernanceParameters",
    into = "pb::GovernanceParameters"
)]
pub struct GovernanceParameters {
    /// The number of blocks during which a proposal is voted on.
    pub proposal_voting_blocks: u64,
    /// The quorum required for a proposal to be considered valid, as a fraction of the total stake
    /// weight of the network.
    pub proposal_valid_quorum: Ratio,
    /// The threshold for a proposal to pass voting, as a ratio of "yes" votes over "no" votes.
    pub proposal_pass_threshold: Ratio,
    /// The threshold for a proposal to be slashed, as a ratio of "no" votes over all total votes.
    pub proposal_slash_threshold: Ratio,
}

impl DomainType for GovernanceParameters {
    type Proto = pb::GovernanceParameters;
}

impl TryFrom<pb::GovernanceParameters> for GovernanceParameters {
    type Error = anyhow::Error;

    fn try_from(msg: pb::GovernanceParameters) -> anyhow::Result<Self> {
        Ok(GovernanceParameters {
            proposal_voting_blocks: msg.proposal_voting_blocks,
            proposal_valid_quorum: msg
                .proposal_valid_quorum
                .parse()
                .context("couldn't parse proposal_valid_quorum")?,
            proposal_pass_threshold: msg
                .proposal_pass_threshold
                .parse()
                .context("couldn't parse proposal_pass_threshold")?,
            proposal_slash_threshold: msg
                .proposal_slash_threshold
                .parse()
                .context("couldn't parse proposal_slash_threshold")?,
        })
    }
}

impl From<GovernanceParameters> for pb::GovernanceParameters {
    fn from(params: GovernanceParameters) -> Self {
        pb::GovernanceParameters {
            proposal_voting_blocks: params.proposal_voting_blocks,
            proposal_valid_quorum: params.proposal_valid_quorum.to_string(),
            proposal_pass_threshold: params.proposal_pass_threshold.to_string(),
            proposal_slash_threshold: params.proposal_slash_threshold.to_string(),
        }
    }
}

impl Default for GovernanceParameters {
    fn default() -> Self {
        Self {
            // governance
            proposal_voting_blocks: 17_280, // 24 hours, at a 5 second block time
            // governance parameters copied from cosmos hub
            proposal_valid_quorum: Ratio::new(40, 100),
            proposal_pass_threshold: Ratio::new(50, 100),
            // slash threshold means if (no / no + yes + abstain) > slash_threshold, then proposal is slashed
            proposal_slash_threshold: Ratio::new(80, 100),
        }
    }
}
