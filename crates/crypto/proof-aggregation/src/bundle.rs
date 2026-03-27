use anyhow::{anyhow, Result};
use penumbra_sdk_proto::{core::transaction::v1 as pb, DomainType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ProofFamilyId {
    Spend,
    Output,
    Swap,
    SwapClaim,
    Convert,
    DelegatorVote,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FamilyAggregate {
    pub family_id: ProofFamilyId,
    pub real_count: u32,
    pub padded_count: u32,
    pub aggregate_proof: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AggregateBundle {
    pub version: u32,
    pub srs_id: Vec<u8>,
    pub families: Vec<FamilyAggregate>,
}

impl From<ProofFamilyId> for pb::ProofFamilyId {
    fn from(value: ProofFamilyId) -> Self {
        match value {
            ProofFamilyId::Spend => Self::Spend,
            ProofFamilyId::Output => Self::Output,
            ProofFamilyId::Swap => Self::Swap,
            ProofFamilyId::SwapClaim => Self::SwapClaim,
            ProofFamilyId::Convert => Self::Convert,
            ProofFamilyId::DelegatorVote => Self::DelegatorVote,
        }
    }
}

impl TryFrom<i32> for ProofFamilyId {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self> {
        match pb::ProofFamilyId::try_from(value) {
            Ok(pb::ProofFamilyId::Unspecified) => Err(anyhow!("unspecified proof family id")),
            Ok(pb::ProofFamilyId::Spend) => Ok(Self::Spend),
            Ok(pb::ProofFamilyId::Output) => Ok(Self::Output),
            Ok(pb::ProofFamilyId::Swap) => Ok(Self::Swap),
            Ok(pb::ProofFamilyId::SwapClaim) => Ok(Self::SwapClaim),
            Ok(pb::ProofFamilyId::Convert) => Ok(Self::Convert),
            Ok(pb::ProofFamilyId::DelegatorVote) => Ok(Self::DelegatorVote),
            Err(_) => Err(anyhow!("unknown proof family id {value}")),
        }
    }
}

impl DomainType for AggregateBundle {
    type Proto = pb::AggregateBundle;
}

impl From<AggregateBundle> for pb::AggregateBundle {
    fn from(value: AggregateBundle) -> Self {
        Self {
            version: value.version,
            srs_id: value.srs_id,
            families: value.families.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<pb::AggregateBundle> for AggregateBundle {
    type Error = anyhow::Error;

    fn try_from(value: pb::AggregateBundle) -> Result<Self> {
        Ok(Self {
            version: value.version,
            srs_id: value.srs_id,
            families: value
                .families
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl DomainType for FamilyAggregate {
    type Proto = pb::FamilyAggregate;
}

impl From<FamilyAggregate> for pb::FamilyAggregate {
    fn from(value: FamilyAggregate) -> Self {
        Self {
            family_id: pb::ProofFamilyId::from(value.family_id) as i32,
            real_count: value.real_count,
            padded_count: value.padded_count,
            aggregate_proof: value.aggregate_proof,
        }
    }
}

impl TryFrom<pb::FamilyAggregate> for FamilyAggregate {
    type Error = anyhow::Error;

    fn try_from(value: pb::FamilyAggregate) -> Result<Self> {
        Ok(Self {
            family_id: value.family_id.try_into()?,
            real_count: value.real_count,
            padded_count: value.padded_count,
            aggregate_proof: value.aggregate_proof,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AggregateBundle, FamilyAggregate, ProofFamilyId};
    use penumbra_sdk_proto::DomainType;

    #[test]
    fn aggregate_bundle_proto_round_trip() {
        let bundle = AggregateBundle {
            version: 7,
            srs_id: vec![1, 2, 3, 4],
            families: vec![FamilyAggregate {
                family_id: ProofFamilyId::SwapClaim,
                real_count: 3,
                padded_count: 4,
                aggregate_proof: vec![9, 8, 7],
            }],
        };

        let proto = bundle.to_proto();
        let decoded = AggregateBundle::try_from(proto).expect("bundle round-trip");
        assert_eq!(decoded, bundle);
    }
}
