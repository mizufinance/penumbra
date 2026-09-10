use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use shieldd_sdk_proto::{core::transaction::v1 as pb, DomainType};
use shieldd_sdk_shielded_pool::{NoteReshapeFamilyId, ShieldedWithdrawalFamilyId};

const PROOF_FAMILY_TRANSFER: u32 = pb::ProofFamilyId::Transfer as u32;
const PROOF_FAMILY_NOTE_RESHAPE: u32 = pb::ProofFamilyId::NoteReshape as u32;
const PROOF_FAMILY_SHIELDED_WITHDRAWAL: u32 = pb::ProofFamilyId::ShieldedWithdrawal as u32;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ProofFamilyId {
    Transfer,
    NoteReshape(NoteReshapeFamilyId),
    ShieldedWithdrawal(ShieldedWithdrawalFamilyId),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FamilyRouteKind {
    Transfer,
    NoteReshape,
    ShieldedWithdrawal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FamilyRoute {
    pub kind: FamilyRouteKind,
    pub subfamily_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FamilyProtoFields {
    pub family_id: u32,
    pub note_reshape_family_id: u32,
    pub shielded_withdrawal_family_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FamilyRouteError {
    UnknownProofFamily,
    MissingSubfamily,
    UnexpectedSubfamily,
    UnknownSubfamily,
}

/// Canonical protobuf fields for a registered proof family.
///
/// Statement construction and bundle encoding both use this function, so the
/// family bytes bound by Fiat--Shamir cannot drift from the inverse router.
pub(crate) fn family_proto_fields(family_id: ProofFamilyId) -> FamilyProtoFields {
    match family_id {
        ProofFamilyId::Transfer => FamilyProtoFields {
            family_id: PROOF_FAMILY_TRANSFER,
            note_reshape_family_id: 0,
            shielded_withdrawal_family_id: 0,
        },
        ProofFamilyId::NoteReshape(subfamily) => FamilyProtoFields {
            family_id: PROOF_FAMILY_NOTE_RESHAPE,
            note_reshape_family_id: subfamily.get(),
            shielded_withdrawal_family_id: 0,
        },
        ProofFamilyId::ShieldedWithdrawal(subfamily) => FamilyProtoFields {
            family_id: PROOF_FAMILY_SHIELDED_WITHDRAWAL,
            note_reshape_family_id: 0,
            shielded_withdrawal_family_id: subfamily.get(),
        },
    }
}

pub fn family_route_from_proto_fields(
    family_id: u32,
    note_reshape_family_id: u32,
    shielded_withdrawal_family_id: u32,
) -> Result<FamilyRoute, FamilyRouteError> {
    match family_id {
        PROOF_FAMILY_TRANSFER => {
            if note_reshape_family_id != 0 || shielded_withdrawal_family_id != 0 {
                Err(FamilyRouteError::UnexpectedSubfamily)
            } else {
                Ok(FamilyRoute {
                    kind: FamilyRouteKind::Transfer,
                    subfamily_id: 0,
                })
            }
        }
        PROOF_FAMILY_NOTE_RESHAPE => {
            if note_reshape_family_id == 0 {
                Err(FamilyRouteError::MissingSubfamily)
            } else if shielded_withdrawal_family_id != 0 {
                Err(FamilyRouteError::UnexpectedSubfamily)
            } else if NoteReshapeFamilyId::try_from(note_reshape_family_id).is_ok() {
                Ok(FamilyRoute {
                    kind: FamilyRouteKind::NoteReshape,
                    subfamily_id: note_reshape_family_id,
                })
            } else {
                Err(FamilyRouteError::UnknownSubfamily)
            }
        }
        PROOF_FAMILY_SHIELDED_WITHDRAWAL => {
            if shielded_withdrawal_family_id == 0 {
                Err(FamilyRouteError::MissingSubfamily)
            } else if note_reshape_family_id != 0 {
                Err(FamilyRouteError::UnexpectedSubfamily)
            } else if ShieldedWithdrawalFamilyId::try_from(shielded_withdrawal_family_id).is_ok() {
                Ok(FamilyRoute {
                    kind: FamilyRouteKind::ShieldedWithdrawal,
                    subfamily_id: shielded_withdrawal_family_id,
                })
            } else {
                Err(FamilyRouteError::UnknownSubfamily)
            }
        }
        _ => Err(FamilyRouteError::UnknownProofFamily),
    }
}

impl From<ProofFamilyId> for pb::ProofFamilyId {
    fn from(value: ProofFamilyId) -> Self {
        match value {
            ProofFamilyId::Transfer => Self::Transfer,
            ProofFamilyId::NoteReshape(_) => Self::NoteReshape,
            ProofFamilyId::ShieldedWithdrawal(_) => Self::ShieldedWithdrawal,
        }
    }
}

impl ProofFamilyId {
    pub(crate) fn try_from_proto_fields(
        family_id: i32,
        note_reshape_family_id: u32,
        shielded_withdrawal_family_id: u32,
    ) -> Result<Self> {
        let family_id_u32 =
            u32::try_from(family_id).map_err(|_| anyhow!("unknown proof family id {family_id}"))?;
        if family_id_u32 == pb::ProofFamilyId::Unspecified as u32 {
            return Err(anyhow!("unspecified proof family id"));
        }
        let route = family_route_from_proto_fields(
            family_id_u32,
            note_reshape_family_id,
            shielded_withdrawal_family_id,
        )
        .map_err(|err| family_route_error_message(err, family_id))?;
        match route.kind {
            FamilyRouteKind::Transfer => Ok(Self::Transfer),
            FamilyRouteKind::NoteReshape => Ok(Self::NoteReshape(route.subfamily_id.try_into()?)),
            FamilyRouteKind::ShieldedWithdrawal => {
                Ok(Self::ShieldedWithdrawal(route.subfamily_id.try_into()?))
            }
        }
    }
}

fn family_route_error_message(err: FamilyRouteError, family_id: i32) -> anyhow::Error {
    match err {
        FamilyRouteError::UnknownProofFamily => anyhow!("unknown proof family id {family_id}"),
        FamilyRouteError::MissingSubfamily => {
            anyhow!("aggregate family {family_id} is missing its required subfamily id")
        }
        FamilyRouteError::UnexpectedSubfamily => {
            anyhow!("aggregate family {family_id} has subfamily ids for another family")
        }
        FamilyRouteError::UnknownSubfamily => {
            anyhow!("aggregate family {family_id} has an unknown subfamily id")
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
        let fields = family_proto_fields(value.family_id);
        Self {
            family_id: fields.family_id as i32,
            note_reshape_family_id: fields.note_reshape_family_id,
            shielded_withdrawal_family_id: fields.shielded_withdrawal_family_id,
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
                value.note_reshape_family_id,
                value.shielded_withdrawal_family_id,
            )?,
            real_count: value.real_count,
            padded_count: value.padded_count,
            aggregate_proof: value.aggregate_proof,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        family_proto_fields, family_route_from_proto_fields, AggregateBundle, FamilyAggregate,
        FamilyRouteKind, ProofFamilyId,
    };
    use shieldd_sdk_proto::DomainType;
    use shieldd_sdk_shielded_pool::{NoteReshapeFamilyId, ShieldedWithdrawalFamilyId};

    #[test]
    fn canonical_family_fields_round_trip_through_router() {
        let mut families = vec![
            ProofFamilyId::Transfer,
            ProofFamilyId::ShieldedWithdrawal(ShieldedWithdrawalFamilyId::Canonical),
        ];
        families.extend(
            NoteReshapeFamilyId::ALL
                .into_iter()
                .map(ProofFamilyId::NoteReshape),
        );

        for family in families {
            let fields = family_proto_fields(family);
            let route = family_route_from_proto_fields(
                fields.family_id,
                fields.note_reshape_family_id,
                fields.shielded_withdrawal_family_id,
            )
            .expect("canonical family fields must route");
            match family {
                ProofFamilyId::Transfer => {
                    assert_eq!(route.kind, FamilyRouteKind::Transfer);
                    assert_eq!(route.subfamily_id, 0);
                }
                ProofFamilyId::NoteReshape(subfamily) => {
                    assert_eq!(route.kind, FamilyRouteKind::NoteReshape);
                    assert_eq!(route.subfamily_id, subfamily.get());
                }
                ProofFamilyId::ShieldedWithdrawal(subfamily) => {
                    assert_eq!(route.kind, FamilyRouteKind::ShieldedWithdrawal);
                    assert_eq!(route.subfamily_id, subfamily.get());
                }
            }
        }
    }

    #[test]
    fn family_router_rejects_unknown_missing_and_cross_family_fields() {
        let transfer = shieldd_sdk_proto::core::transaction::v1::ProofFamilyId::Transfer as u32;
        let note_reshape =
            shieldd_sdk_proto::core::transaction::v1::ProofFamilyId::NoteReshape as u32;
        let withdrawal =
            shieldd_sdk_proto::core::transaction::v1::ProofFamilyId::ShieldedWithdrawal as u32;

        assert_eq!(
            family_route_from_proto_fields(u32::MAX, 0, 0),
            Err(super::FamilyRouteError::UnknownProofFamily)
        );
        assert_eq!(
            family_route_from_proto_fields(note_reshape, 0, 0),
            Err(super::FamilyRouteError::MissingSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(note_reshape, 0, 1),
            Err(super::FamilyRouteError::MissingSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(withdrawal, 0, 0),
            Err(super::FamilyRouteError::MissingSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(withdrawal, 1, 0),
            Err(super::FamilyRouteError::MissingSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(transfer, 1, 0),
            Err(super::FamilyRouteError::UnexpectedSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(note_reshape, 1, 1),
            Err(super::FamilyRouteError::UnexpectedSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(withdrawal, 1, 1),
            Err(super::FamilyRouteError::UnexpectedSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(note_reshape, u32::MAX, 0),
            Err(super::FamilyRouteError::UnknownSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(withdrawal, 0, u32::MAX),
            Err(super::FamilyRouteError::UnknownSubfamily)
        );
        assert_eq!(
            family_route_from_proto_fields(u32::MAX, 1, 1),
            Err(super::FamilyRouteError::UnknownProofFamily)
        );
    }

    #[test]
    fn aggregate_bundle_proto_round_trip() {
        let bundle = AggregateBundle {
            version: 7,
            srs_id: vec![1, 2, 3, 4],
            families: vec![
                FamilyAggregate {
                    family_id: ProofFamilyId::NoteReshape(NoteReshapeFamilyId::EightByOne),
                    real_count: 1,
                    padded_count: 1,
                    aggregate_proof: vec![1, 2, 3],
                },
                FamilyAggregate {
                    family_id: ProofFamilyId::NoteReshape(NoteReshapeFamilyId::OneByEight),
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
        let proto = shieldd_sdk_proto::core::transaction::v1::AggregateBundle {
            version: 1,
            srs_id: vec![0; 32],
            families: vec![shieldd_sdk_proto::core::transaction::v1::FamilyAggregate {
                family_id: shieldd_sdk_proto::core::transaction::v1::ProofFamilyId::Unspecified
                    as i32,
                note_reshape_family_id: 0,
                shielded_withdrawal_family_id: 0,
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
