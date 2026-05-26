use anyhow::{anyhow, Result};
use penumbra_sdk_proto::{core::transaction::v1 as pb, DomainType};
use penumbra_sdk_shielded_pool::{
    ConsolidateFamilyId, ShieldedIcs20WithdrawalFamilyId, SplitFamilyId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ProofFamilyId {
    Transfer,
    Consolidate(ConsolidateFamilyId),
    Split(SplitFamilyId),
    ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId),
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
            ProofFamilyId::Transfer => Self::Transfer,
            ProofFamilyId::Consolidate(_) => Self::Consolidate,
            ProofFamilyId::Split(_) => Self::Split,
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => Self::ShieldedIcs20Withdrawal,
        }
    }
}

impl ProofFamilyId {
    pub(crate) fn try_from_proto_fields(
        family_id: i32,
        consolidate_family_id: u32,
        split_family_id: u32,
        shielded_ics20_withdrawal_family_id: u32,
    ) -> Result<Self> {
        match pb::ProofFamilyId::try_from(family_id) {
            Ok(pb::ProofFamilyId::Unspecified) => Err(anyhow!("unspecified proof family id")),
            Ok(pb::ProofFamilyId::Transfer) => {
                ensure_only_transfer_related_ids(
                    consolidate_family_id,
                    split_family_id,
                    shielded_ics20_withdrawal_family_id,
                )?;
                Ok(Self::Transfer)
            }
            Ok(pb::ProofFamilyId::Consolidate) => {
                ensure_only_consolidate_family_id(
                    consolidate_family_id,
                    split_family_id,
                    shielded_ics20_withdrawal_family_id,
                )?;
                Ok(Self::Consolidate(consolidate_family_id.try_into()?))
            }
            Ok(pb::ProofFamilyId::Split) => {
                ensure_only_split_family_id(
                    consolidate_family_id,
                    split_family_id,
                    shielded_ics20_withdrawal_family_id,
                )?;
                Ok(Self::Split(split_family_id.try_into()?))
            }
            Ok(pb::ProofFamilyId::ShieldedIcs20Withdrawal) => {
                ensure_only_shielded_ics20_withdrawal_family_id(
                    consolidate_family_id,
                    split_family_id,
                    shielded_ics20_withdrawal_family_id,
                )?;
                Ok(Self::ShieldedIcs20Withdrawal(
                    shielded_ics20_withdrawal_family_id.try_into()?,
                ))
            }
            Err(_) => Err(anyhow!("unknown proof family id {family_id}")),
        }
    }

    pub(crate) fn consolidate_family_id(self) -> u32 {
        match self {
            ProofFamilyId::Consolidate(family_id) => family_id.get(),
            _ => 0,
        }
    }

    pub(crate) fn split_family_id(self) -> u32 {
        match self {
            ProofFamilyId::Split(family_id) => family_id.get(),
            _ => 0,
        }
    }

    pub(crate) fn shielded_ics20_withdrawal_family_id(self) -> u32 {
        match self {
            ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => family_id.get(),
            _ => 0,
        }
    }
}

fn ensure_only_transfer_related_ids(
    consolidate_family_id: u32,
    split_family_id: u32,
    shielded_ics20_withdrawal_family_id: u32,
) -> Result<()> {
    if consolidate_family_id != 0
        || split_family_id != 0
        || shielded_ics20_withdrawal_family_id != 0
    {
        Err(anyhow!(
            "transfer aggregate must not set consolidate/split/shielded_ics20_withdrawal ids: consolidate={consolidate_family_id}, split={split_family_id}, shielded_ics20_withdrawal={shielded_ics20_withdrawal_family_id}"
        ))
    } else {
        Ok(())
    }
}

fn ensure_only_consolidate_family_id(
    consolidate_family_id: u32,
    split_family_id: u32,
    shielded_ics20_withdrawal_family_id: u32,
) -> Result<()> {
    if consolidate_family_id == 0 {
        Err(anyhow!(
            "consolidate aggregate must set consolidate_family_id"
        ))
    } else if split_family_id != 0 || shielded_ics20_withdrawal_family_id != 0 {
        Err(anyhow!(
            "consolidate aggregate must not set split/shielded_ics20_withdrawal ids: split={split_family_id}, shielded_ics20_withdrawal={shielded_ics20_withdrawal_family_id}"
        ))
    } else {
        Ok(())
    }
}

fn ensure_only_split_family_id(
    consolidate_family_id: u32,
    split_family_id: u32,
    shielded_ics20_withdrawal_family_id: u32,
) -> Result<()> {
    if split_family_id == 0 {
        Err(anyhow!("split aggregate must set split_family_id"))
    } else if consolidate_family_id != 0 || shielded_ics20_withdrawal_family_id != 0 {
        Err(anyhow!(
            "split aggregate must not set consolidate/shielded_ics20_withdrawal ids: consolidate={consolidate_family_id}, shielded_ics20_withdrawal={shielded_ics20_withdrawal_family_id}"
        ))
    } else {
        Ok(())
    }
}

fn ensure_only_shielded_ics20_withdrawal_family_id(
    consolidate_family_id: u32,
    split_family_id: u32,
    shielded_ics20_withdrawal_family_id: u32,
) -> Result<()> {
    if shielded_ics20_withdrawal_family_id == 0 {
        Err(anyhow!(
            "shielded_ics20_withdrawal aggregate must set shielded_ics20_withdrawal_family_id"
        ))
    } else if consolidate_family_id != 0 || split_family_id != 0 {
        Err(anyhow!(
            "shielded_ics20_withdrawal aggregate must not set consolidate/split ids: consolidate={consolidate_family_id}, split={split_family_id}"
        ))
    } else {
        Ok(())
    }
}

impl TryFrom<i32> for ProofFamilyId {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self> {
        Self::try_from_proto_fields(value, 0, 0, 0)
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
            consolidate_family_id: value.family_id.consolidate_family_id(),
            split_family_id: value.family_id.split_family_id(),
            shielded_ics20_withdrawal_family_id: value
                .family_id
                .shielded_ics20_withdrawal_family_id(),
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
                value.consolidate_family_id,
                value.split_family_id,
                value.shielded_ics20_withdrawal_family_id,
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
    use penumbra_sdk_shielded_pool::{ConsolidateFamilyId, SplitFamilyId};

    #[test]
    fn aggregate_bundle_proto_round_trip() {
        let bundle = AggregateBundle {
            version: 7,
            srs_id: vec![1, 2, 3, 4],
            families: vec![
                FamilyAggregate {
                    family_id: ProofFamilyId::Consolidate(ConsolidateFamilyId::TwoByOne),
                    real_count: 1,
                    padded_count: 1,
                    aggregate_proof: vec![1, 2, 3],
                },
                FamilyAggregate {
                    family_id: ProofFamilyId::Split(SplitFamilyId::OneByFour),
                    real_count: 2,
                    padded_count: 2,
                    aggregate_proof: vec![4, 5, 6],
                },
            ],
        };

        let proto = bundle.to_proto();
        let decoded = AggregateBundle::try_from(proto).expect("bundle round-trip");
        assert_eq!(decoded, bundle);
    }

    #[test]
    fn aggregate_bundle_decode_rejects_unspecified_family() {
        let proto = penumbra_sdk_proto::core::transaction::v1::AggregateBundle {
            version: 1,
            srs_id: vec![0; 32],
            families: vec![penumbra_sdk_proto::core::transaction::v1::FamilyAggregate {
                family_id: penumbra_sdk_proto::core::transaction::v1::ProofFamilyId::Unspecified
                    as i32,
                consolidate_family_id: 0,
                split_family_id: 0,
                shielded_ics20_withdrawal_family_id: 0,
                real_count: 1,
                padded_count: 1,
                aggregate_proof: vec![1, 2, 3],
            }],
        };

        let err = AggregateBundle::try_from(proto)
            .expect_err("unspecified aggregate family should reject");

        assert!(err.to_string().contains("unspecified proof family id"));
    }
}
