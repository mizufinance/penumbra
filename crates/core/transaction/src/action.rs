use anyhow::anyhow;
use penumbra_sdk_asset::balance;
use penumbra_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_proof_aggregation::AggregateBundle;
use penumbra_sdk_proto::{core::transaction::v1 as pb, DomainType, Message as _};
use penumbra_sdk_txhash::{EffectHash, EffectingData};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use crate::{ActionView, IsAction, TransactionPerspective};

/// An action performed by a Penumbra transaction.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::Action", into = "pb::Action")]
#[allow(clippy::large_enum_variant)]
pub enum Action {
    Transfer(penumbra_sdk_shielded_pool::Transfer),
    Consolidate(penumbra_sdk_shielded_pool::Consolidate),
    Split(penumbra_sdk_shielded_pool::Split),
    ValidatorDefinition(penumbra_sdk_validator::validator::Definition),
    IbcRelay(penumbra_sdk_ibc::IbcRelay),
    ProposalSubmit(penumbra_sdk_governance::ProposalSubmit),
    ValidatorVote(penumbra_sdk_governance::ValidatorVote),
    ShieldedIcs20Withdrawal(penumbra_sdk_shielded_pool::ShieldedIcs20Withdrawal),
    ComplianceRegisterAsset(MsgRegisterAsset),
    ComplianceRegisterUser(MsgRegisterUser),
    AggregateBundle(AggregateBundle),
}

impl EffectingData for Action {
    fn effect_hash(&self) -> EffectHash {
        match self {
            Action::Transfer(transfer) => transfer.effect_hash(),
            Action::Consolidate(consolidate) => consolidate.effect_hash(),
            Action::Split(split) => split.effect_hash(),
            Action::ProposalSubmit(submit) => submit.effect_hash(),
            Action::ValidatorVote(vote) => vote.effect_hash(),
            Action::ValidatorDefinition(defn) => defn.effect_hash(),
            Action::IbcRelay(payload) => payload.effect_hash(),
            Action::ShieldedIcs20Withdrawal(withdrawal) => withdrawal.effect_hash(),
            Action::ComplianceRegisterAsset(action) => action.effect_hash(),
            Action::ComplianceRegisterUser(action) => action.effect_hash(),
            Action::AggregateBundle(bundle) => {
                let bytes = pb::AggregateBundle::from(bundle.clone()).encode_to_vec();
                EffectHash(
                    blake2b_simd::Params::new()
                        .personal(b"PenumbraAgBH")
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
            Action::Consolidate(_) => tracing::info_span!("Consolidate", ?idx),
            Action::Split(_) => tracing::info_span!("Split", ?idx),
            Action::ValidatorDefinition(_) => tracing::info_span!("ValidatorDefinition", ?idx),
            Action::IbcRelay(msg) => {
                let action_span = tracing::info_span!("IbcAction", ?idx);
                msg.create_span(&action_span)
            }
            Action::ProposalSubmit(_) => tracing::info_span!("ProposalSubmit", ?idx),
            Action::ValidatorVote(_) => tracing::info_span!("ValidatorVote", ?idx),
            Action::ShieldedIcs20Withdrawal(_) => {
                tracing::info_span!("ShieldedIcs20Withdrawal", ?idx)
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
            Action::Consolidate(_) => 6,
            Action::Split(_) => 7,
            Action::ValidatorDefinition(_) => 16,
            Action::IbcRelay(_) => 17,
            Action::ProposalSubmit(_) => 18,
            Action::ValidatorVote(_) => 20,
            Action::ComplianceRegisterAsset(_) => 80,
            Action::ComplianceRegisterUser(_) => 81,
            Action::AggregateBundle(_) => 82,
            Action::ShieldedIcs20Withdrawal(_) => 200,
        }
    }
}

impl IsAction for Action {
    fn balance_commitment(&self) -> balance::Commitment {
        match self {
            Action::Transfer(transfer) => transfer.balance_commitment(),
            Action::Consolidate(consolidate) => consolidate.balance_commitment(),
            Action::Split(split) => split.balance_commitment(),
            Action::ProposalSubmit(submit) => submit.balance_commitment(),
            Action::ValidatorVote(vote) => vote.balance_commitment(),
            Action::ShieldedIcs20Withdrawal(withdrawal) => withdrawal.balance_commitment(),
            Action::IbcRelay(action) => action.balance_commitment(),
            Action::ValidatorDefinition(_) => balance::Commitment::default(),
            Action::ComplianceRegisterAsset(_) => balance::Commitment::default(),
            Action::ComplianceRegisterUser(_) => balance::Commitment::default(),
            Action::AggregateBundle(_) => balance::Commitment::default(),
        }
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        match self {
            Action::Transfer(action) => action.view_from_perspective(txp),
            Action::Consolidate(action) => action.view_from_perspective(txp),
            Action::Split(action) => action.view_from_perspective(txp),
            Action::ProposalSubmit(action) => action.view_from_perspective(txp),
            Action::ValidatorVote(action) => action.view_from_perspective(txp),
            Action::ShieldedIcs20Withdrawal(action) => action.view_from_perspective(txp),
            Action::ValidatorDefinition(action) => {
                ActionView::ValidatorDefinition(action.to_owned())
            }
            Action::IbcRelay(action) => ActionView::IbcRelay(action.to_owned()),
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
            Action::Consolidate(inner) => pb::Action {
                action: Some(pb::action::Action::Consolidate(inner.into())),
            },
            Action::Split(inner) => pb::Action {
                action: Some(pb::action::Action::Split(inner.into())),
            },
            Action::ValidatorDefinition(inner) => pb::Action {
                action: Some(pb::action::Action::ValidatorDefinition(inner.into())),
            },
            Action::IbcRelay(inner) => pb::Action {
                action: Some(pb::action::Action::IbcRelayAction(inner.into())),
            },
            Action::ProposalSubmit(inner) => pb::Action {
                action: Some(pb::action::Action::ProposalSubmit(inner.into())),
            },
            Action::ValidatorVote(inner) => pb::Action {
                action: Some(pb::action::Action::ValidatorVote(inner.into())),
            },
            Action::ShieldedIcs20Withdrawal(inner) => pb::Action {
                action: Some(pb::action::Action::ShieldedIcs20Withdrawal(inner.into())),
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
            pb::action::Action::Consolidate(inner) => Ok(Action::Consolidate(inner.try_into()?)),
            pb::action::Action::Split(inner) => Ok(Action::Split(inner.try_into()?)),
            pb::action::Action::ValidatorDefinition(inner) => {
                Ok(Action::ValidatorDefinition(inner.try_into()?))
            }
            pb::action::Action::IbcRelayAction(inner) => Ok(Action::IbcRelay(inner.try_into()?)),
            pb::action::Action::ProposalSubmit(inner) => {
                Ok(Action::ProposalSubmit(inner.try_into()?))
            }
            pb::action::Action::ValidatorVote(inner) => {
                Ok(Action::ValidatorVote(inner.try_into()?))
            }
            pb::action::Action::ShieldedIcs20Withdrawal(inner) => {
                Ok(Action::ShieldedIcs20Withdrawal(inner.try_into()?))
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
