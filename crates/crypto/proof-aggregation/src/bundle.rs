use anyhow::{anyhow, Result};
use penumbra_sdk_proto::{core::transaction::v1 as pb, DomainType};
use penumbra_sdk_shielded_pool::TransferFamilyId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ProofFamilyId {
    Spend,
    Output,
    Transfer(TransferFamilyId),
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
            ProofFamilyId::Transfer(_) => Self::Transfer,
            ProofFamilyId::Swap => Self::Swap,
            ProofFamilyId::SwapClaim => Self::SwapClaim,
            ProofFamilyId::Convert => Self::Convert,
            ProofFamilyId::DelegatorVote => Self::DelegatorVote,
        }
    }
}

impl ProofFamilyId {
    fn try_from_proto_fields(family_id: i32, transfer_family_id: u32) -> Result<Self> {
        match pb::ProofFamilyId::try_from(family_id) {
            Ok(pb::ProofFamilyId::Unspecified) => Err(anyhow!("unspecified proof family id")),
            Ok(pb::ProofFamilyId::Spend) => {
                ensure_no_transfer_family_id("spend", transfer_family_id)?;
                Ok(Self::Spend)
            }
            Ok(pb::ProofFamilyId::Output) => {
                ensure_no_transfer_family_id("output", transfer_family_id)?;
                Ok(Self::Output)
            }
            Ok(pb::ProofFamilyId::Transfer) => Ok(Self::Transfer(transfer_family_id.try_into()?)),
            Ok(pb::ProofFamilyId::Swap) => {
                ensure_no_transfer_family_id("swap", transfer_family_id)?;
                Ok(Self::Swap)
            }
            Ok(pb::ProofFamilyId::SwapClaim) => {
                ensure_no_transfer_family_id("swap_claim", transfer_family_id)?;
                Ok(Self::SwapClaim)
            }
            Ok(pb::ProofFamilyId::Convert) => {
                ensure_no_transfer_family_id("convert", transfer_family_id)?;
                Ok(Self::Convert)
            }
            Ok(pb::ProofFamilyId::DelegatorVote) => {
                ensure_no_transfer_family_id("delegator_vote", transfer_family_id)?;
                Ok(Self::DelegatorVote)
            }
            Err(_) => Err(anyhow!("unknown proof family id {family_id}")),
        }
    }

    fn transfer_family_id(self) -> u32 {
        match self {
            ProofFamilyId::Transfer(family_id) => family_id.get(),
            _ => 0,
        }
    }
}

fn ensure_no_transfer_family_id(family: &str, transfer_family_id: u32) -> Result<()> {
    if transfer_family_id == 0 {
        Ok(())
    } else {
        Err(anyhow!(
            "{family} aggregate must not set transfer_family_id={transfer_family_id}"
        ))
    }
}

impl TryFrom<i32> for ProofFamilyId {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self> {
        Self::try_from_proto_fields(value, 0)
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
            transfer_family_id: value.family_id.transfer_family_id(),
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
            family_id: ProofFamilyId::try_from_proto_fields(
                value.family_id,
                value.transfer_family_id,
            )?,
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
