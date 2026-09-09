use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use shieldd_sdk_proof_aggregation::AggregateBundle;
use shieldd_sdk_proto::{core::transaction::v1 as pb, DomainType, Message as _};
use shieldd_sdk_txhash::{EffectHash, EffectingData};
use std::convert::{TryFrom, TryInto};

use crate::{ActionView, IsAction, TransactionPerspective};

/// An action performed by a Shieldd transaction.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::Action", into = "pb::Action")]
#[allow(clippy::large_enum_variant)]
pub enum Action {
    Transfer(shieldd_sdk_shielded_pool::Transfer),
    NoteReshape(shieldd_sdk_shielded_pool::NoteReshape),
    ShieldedHostWithdrawal(shieldd_sdk_shielded_pool::ShieldedHostWithdrawal),
    ComplianceRegisterAsset(MsgRegisterAsset),
    ComplianceRegisterUser(MsgRegisterUser),
    AggregateBundle(AggregateBundle),
}

impl EffectingData for Action {
    fn effect_hash(&self) -> EffectHash {
        match self {
            Action::Transfer(transfer) => transfer.effect_hash(),
            Action::NoteReshape(note_reshape) => note_reshape.effect_hash(),

            Action::ShieldedHostWithdrawal(withdrawal) => withdrawal.effect_hash(),
            Action::ComplianceRegisterAsset(action) => action.effect_hash(),
            Action::ComplianceRegisterUser(action) => action.effect_hash(),
            Action::AggregateBundle(bundle) => {
                let bytes = pb::AggregateBundle::from(bundle.clone()).encode_to_vec();
                EffectHash(
                    blake2b_simd::Params::new()
                        .personal(b"ShielddAgBH")
                        .hash(&bytes)
                        .as_bytes()[0..32]
                        .try_into()
                        .expect("hash output is 32 bytes"),
                )
            }
        }
    }
}

impl Action {
    /// Create a tracing span to track execution related to this action.
    ///
    /// The `idx` parameter is the index of this action in the transaction.
    pub fn create_span(&self, idx: usize) -> tracing::Span {
        match self {
            Action::Transfer(_) => tracing::info_span!("Transfer", ?idx),
            Action::NoteReshape(_) => tracing::info_span!("NoteReshape", ?idx),

            Action::ShieldedHostWithdrawal(_) => {
                tracing::info_span!("ShieldedHostWithdrawal", ?idx)
            }
            Action::ComplianceRegisterAsset(_) => {
                tracing::info_span!("ComplianceRegisterAsset", ?idx)
            }
            Action::ComplianceRegisterUser(_) => {
                tracing::info_span!("ComplianceRegisterUser", ?idx)
            }
            Action::AggregateBundle(_) => tracing::info_span!("AggregateBundle", ?idx),
        }
    }

    /// Canonical action ordering according to protobuf definitions.
    pub fn variant_index(&self) -> usize {
        match self {
            Action::Transfer(_) => 5,
            Action::NoteReshape(_) => 6,

            Action::ComplianceRegisterAsset(_) => 80,
            Action::ComplianceRegisterUser(_) => 81,
            Action::AggregateBundle(_) => 82,

            Action::ShieldedHostWithdrawal(_) => 201,
        }
    }
}

impl IsAction for Action {
    fn balance_commitment(&self) -> balance::Commitment {
        match self {
            Action::Transfer(transfer) => transfer.balance_commitment(),
            Action::NoteReshape(note_reshape) => note_reshape.balance_commitment(),

            Action::ShieldedHostWithdrawal(withdrawal) => withdrawal.balance_commitment(),

            Action::ComplianceRegisterAsset(_) => balance::Commitment::default(),
            Action::ComplianceRegisterUser(_) => balance::Commitment::default(),
            Action::AggregateBundle(_) => balance::Commitment::default(),
        }
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        match self {
            Action::Transfer(action) => action.view_from_perspective(txp),
            Action::NoteReshape(action) => action.view_from_perspective(txp),

            Action::ShieldedHostWithdrawal(action) => action.view_from_perspective(txp),

            Action::ComplianceRegisterAsset(action) => {
                ActionView::ComplianceRegisterAsset(action.to_owned())
            }
            Action::ComplianceRegisterUser(action) => {
                ActionView::ComplianceRegisterUser(action.to_owned())
            }
            Action::AggregateBundle(action) => ActionView::AggregateBundle(action.to_owned()),
        }
    }
}

impl DomainType for Action {
    type Proto = pb::Action;
}

impl From<Action> for pb::Action {
    fn from(msg: Action) -> Self {
        match msg {
            Action::Transfer(inner) => pb::Action {
                action: Some(pb::action::Action::Transfer(inner.into())),
            },
            Action::NoteReshape(inner) => pb::Action {
                action: Some(pb::action::Action::NoteReshape(inner.into())),
            },

            Action::ShieldedHostWithdrawal(inner) => pb::Action {
                action: Some(pb::action::Action::ShieldedHostWithdrawal(inner.into())),
            },
            Action::ComplianceRegisterAsset(inner) => pb::Action {
                action: Some(pb::action::Action::ComplianceRegisterAsset(inner.into())),
            },
            Action::ComplianceRegisterUser(inner) => pb::Action {
                action: Some(pb::action::Action::ComplianceRegisterUser(inner.into())),
            },
            Action::AggregateBundle(inner) => pb::Action {
                action: Some(pb::action::Action::AggregateBundle(inner.into())),
            },
        }
    }
}

impl TryFrom<pb::Action> for Action {
    type Error = anyhow::Error;

    fn try_from(proto: pb::Action) -> anyhow::Result<Self, Self::Error> {
        if proto.action.is_none() {
            tracing::debug!("proto.action is None - full proto: {:?}", proto);
            anyhow::bail!("missing action content");
        }

        match proto
            .action
            .ok_or_else(|| anyhow!("missing action in Action protobuf"))?
        {
            pb::action::Action::Transfer(inner) => Ok(Action::Transfer(inner.try_into()?)),
            pb::action::Action::NoteReshape(inner) => Ok(Action::NoteReshape(inner.try_into()?)),

            pb::action::Action::ShieldedHostWithdrawal(inner) => {
                Ok(Action::ShieldedHostWithdrawal(inner.try_into()?))
            }
            pb::action::Action::ComplianceRegisterAsset(inner) => {
                Ok(Action::ComplianceRegisterAsset(inner.try_into()?))
            }
            pb::action::Action::ComplianceRegisterUser(inner) => {
                Ok(Action::ComplianceRegisterUser(inner.try_into()?))
            }
            pb::action::Action::AggregateBundle(inner) => {
                Ok(Action::AggregateBundle(inner.try_into()?))
            }
        }
    }
}
