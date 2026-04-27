use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::core::component::validator::v1 as pb;
use penumbra_sdk_proto::DomainType;
use serde::{Deserialize, Serialize};

/// Equal-weight voting power assigned to every validator in the supported validator set.
pub const EQUAL_VALIDATOR_VOTING_POWER: u64 = 1_000_000;

pub fn equal_validator_voting_power() -> Amount {
    EQUAL_VALIDATOR_VOTING_POWER.into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::ValidatorParameters", into = "pb::ValidatorParameters")]
pub struct ValidatorParameters {
    /// The number of validators allowed in the consensus set (Active state).
    pub active_validator_limit: u64,
    /// The number of blocks in the window to check for downtime.
    pub signed_blocks_window_len: u64,
    /// The maximum number of blocks in the window each validator can miss signing without slashing.
    pub missed_blocks_maximum: u64,
}

impl DomainType for ValidatorParameters {
    type Proto = pb::ValidatorParameters;
}

impl TryFrom<pb::ValidatorParameters> for ValidatorParameters {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ValidatorParameters) -> anyhow::Result<Self> {
        Ok(ValidatorParameters {
            active_validator_limit: msg.active_validator_limit,
            missed_blocks_maximum: msg.missed_blocks_maximum,
            signed_blocks_window_len: msg.signed_blocks_window_len,
        })
    }
}

impl From<ValidatorParameters> for pb::ValidatorParameters {
    fn from(params: ValidatorParameters) -> Self {
        pb::ValidatorParameters {
            unbonding_epochs: 1,
            active_validator_limit: params.active_validator_limit,
            base_reward_rate: 0,
            slashing_penalty_misbehavior: 0,
            slashing_penalty_downtime: 0,
            signed_blocks_window_len: params.signed_blocks_window_len,
            missed_blocks_maximum: params.missed_blocks_maximum,
            min_validator_stake: Some(Default::default()),
            unbonding_delay: 100,
        }
    }
}

impl Default for ValidatorParameters {
    fn default() -> Self {
        Self {
            active_validator_limit: 80,
            // Copied from cosmos hub
            signed_blocks_window_len: 10000,
            missed_blocks_maximum: 9500,
        }
    }
}
