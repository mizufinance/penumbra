use crate::{
    validator::{State, Validator},
    IdentityKey,
};
use anyhow::{anyhow, Context as _};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{core::component::validator::v1 as pb, DomainType, Name as _};
use tendermint::abci::types::Misbehavior;

#[derive(Clone, Debug)]
pub struct EventValidatorStateChange {
    pub identity_key: IdentityKey,
    pub state: State,
}

impl TryFrom<pb::EventValidatorStateChange> for EventValidatorStateChange {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventValidatorStateChange) -> Result<Self, Self::Error> {
        fn inner(
            value: pb::EventValidatorStateChange,
        ) -> anyhow::Result<EventValidatorStateChange> {
            Ok(EventValidatorStateChange {
                identity_key: value
                    .identity_key
                    .ok_or(anyhow!("missing `identity_key`"))?
                    .try_into()?,
                state: value.state.ok_or(anyhow!("missing `state`"))?.try_into()?,
            })
        }
        inner(value).context(format!("parsing {}", pb::EventValidatorStateChange::NAME))
    }
}

impl From<EventValidatorStateChange> for pb::EventValidatorStateChange {
    fn from(value: EventValidatorStateChange) -> Self {
        Self {
            identity_key: Some(value.identity_key.into()),
            state: Some(value.state.into()),
        }
    }
}

impl DomainType for EventValidatorStateChange {
    type Proto = pb::EventValidatorStateChange;
}

#[derive(Clone, Debug)]
pub struct EventValidatorVotingPowerChange {
    pub identity_key: IdentityKey,
    pub voting_power: Amount,
}

impl TryFrom<pb::EventValidatorVotingPowerChange> for EventValidatorVotingPowerChange {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventValidatorVotingPowerChange) -> Result<Self, Self::Error> {
        fn inner(
            value: pb::EventValidatorVotingPowerChange,
        ) -> anyhow::Result<EventValidatorVotingPowerChange> {
            Ok(EventValidatorVotingPowerChange {
                identity_key: value
                    .identity_key
                    .ok_or(anyhow!("missing `identity_key`"))?
                    .try_into()?,
                voting_power: value
                    .voting_power
                    .ok_or(anyhow!("missing `voting_power`"))?
                    .try_into()?,
            })
        }
        inner(value).context(format!(
            "parsing {}",
            pb::EventValidatorVotingPowerChange::NAME
        ))
    }
}

impl From<EventValidatorVotingPowerChange> for pb::EventValidatorVotingPowerChange {
    fn from(value: EventValidatorVotingPowerChange) -> Self {
        Self {
            identity_key: Some(value.identity_key.into()),
            voting_power: Some(value.voting_power.into()),
        }
    }
}

impl DomainType for EventValidatorVotingPowerChange {
    type Proto = pb::EventValidatorVotingPowerChange;
}

#[derive(Clone, Debug)]
pub struct EventValidatorDefinitionUpload {
    pub validator: Validator,
}

impl TryFrom<pb::EventValidatorDefinitionUpload> for EventValidatorDefinitionUpload {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventValidatorDefinitionUpload) -> Result<Self, Self::Error> {
        fn inner(
            value: pb::EventValidatorDefinitionUpload,
        ) -> anyhow::Result<EventValidatorDefinitionUpload> {
            Ok(EventValidatorDefinitionUpload {
                validator: value
                    .validator
                    .ok_or(anyhow!("missing `validator`"))?
                    .try_into()?,
            })
        }
        inner(value).context(format!(
            "parsing {}",
            pb::EventValidatorDefinitionUpload::NAME
        ))
    }
}

impl From<EventValidatorDefinitionUpload> for pb::EventValidatorDefinitionUpload {
    fn from(value: EventValidatorDefinitionUpload) -> Self {
        Self {
            validator: Some(value.validator.into()),
        }
    }
}

impl DomainType for EventValidatorDefinitionUpload {
    type Proto = pb::EventValidatorDefinitionUpload;
}

#[derive(Clone, Debug)]
pub struct EventValidatorMissedBlock {
    pub identity_key: IdentityKey,
}

impl TryFrom<pb::EventValidatorMissedBlock> for EventValidatorMissedBlock {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventValidatorMissedBlock) -> Result<Self, Self::Error> {
        fn inner(
            value: pb::EventValidatorMissedBlock,
        ) -> anyhow::Result<EventValidatorMissedBlock> {
            Ok(EventValidatorMissedBlock {
                identity_key: value
                    .identity_key
                    .ok_or(anyhow!("missing `identity_key`"))?
                    .try_into()?,
            })
        }
        inner(value).context(format!("parsing {}", pb::EventValidatorMissedBlock::NAME))
    }
}

impl From<EventValidatorMissedBlock> for pb::EventValidatorMissedBlock {
    fn from(value: EventValidatorMissedBlock) -> Self {
        Self {
            identity_key: Some(value.identity_key.into()),
        }
    }
}

impl DomainType for EventValidatorMissedBlock {
    type Proto = pb::EventValidatorMissedBlock;
}

#[derive(Clone, Debug)]
pub struct EventTombstoneValidator {
    pub evidence_height: u64,
    pub current_height: u64,
    pub identity_key: IdentityKey,
    pub address: Vec<u8>,
    pub voting_power: u64,
}

impl EventTombstoneValidator {
    pub fn from_evidence(
        current_height: u64,
        identity_key: IdentityKey,
        evidence: &Misbehavior,
    ) -> Self {
        Self {
            evidence_height: evidence.height.value(),
            current_height,
            identity_key,
            address: evidence.validator.address.to_vec(),
            voting_power: evidence.validator.power.value(),
        }
    }
}

impl TryFrom<pb::EventTombstoneValidator> for EventTombstoneValidator {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventTombstoneValidator) -> Result<Self, Self::Error> {
        fn inner(value: pb::EventTombstoneValidator) -> anyhow::Result<EventTombstoneValidator> {
            Ok(EventTombstoneValidator {
                evidence_height: value.evidence_height,
                current_height: value.current_height,
                identity_key: value
                    .identity_key
                    .ok_or(anyhow!("missing `identity_key`"))?
                    .try_into()?,
                address: value.address,
                voting_power: value.voting_power,
            })
        }
        inner(value).context(format!("parsing {}", pb::EventTombstoneValidator::NAME))
    }
}

impl From<EventTombstoneValidator> for pb::EventTombstoneValidator {
    fn from(value: EventTombstoneValidator) -> Self {
        Self {
            evidence_height: value.evidence_height,
            current_height: value.current_height,
            identity_key: Some(value.identity_key.into()),
            address: value.address,
            voting_power: value.voting_power,
        }
    }
}

impl DomainType for EventTombstoneValidator {
    type Proto = pb::EventTombstoneValidator;
}
