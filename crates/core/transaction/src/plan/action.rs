#[cfg(any(unix, windows))]
use crate::Action;
#[cfg(any(unix, windows))]
use crate::WitnessData;
use anyhow::anyhow;
#[cfg(any(unix, windows))]
use anyhow::{Context, Result};
use ark_ff::Zero;
use decaf377::Fr;
use penumbra_sdk_asset::Balance;
use penumbra_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_governance::{ProposalSubmit, ValidatorVote};
use penumbra_sdk_ibc::IbcRelay;
#[cfg(any(unix, windows))]
use penumbra_sdk_keys::{symmetric::PayloadKey, FullViewingKey};
use penumbra_sdk_proto::{core::transaction::v1 as pb_t, DomainType};
use penumbra_sdk_shielded_pool::{
    ConsolidatePlan, ShieldedIcs20WithdrawalPlan, SplitPlan, TransferPlan,
};
#[cfg(any(unix, windows))]
use penumbra_sdk_txhash::{EffectHash, EffectingData};
use serde::{Deserialize, Serialize};

/// A declaration of a planned [`Action`], for use in transaction creation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb_t::ActionPlan", into = "pb_t::ActionPlan")]
#[allow(clippy::large_enum_variant)]
pub enum ActionPlan {
    /// Describes a proposed fused transfer.
    Transfer(TransferPlan),
    /// Describes a same-address note consolidation.
    Consolidate(ConsolidatePlan),
    /// Describes a same-address note split.
    Split(SplitPlan),
    ValidatorDefinition(penumbra_sdk_validator::validator::Definition),
    IbcAction(IbcRelay),
    ProposalSubmit(ProposalSubmit),
    ValidatorVote(ValidatorVote),
    ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalPlan),
    /// Register an asset's regulation status in the compliance registry.
    ComplianceRegisterAsset(MsgRegisterAsset),
    /// Register a user's compliance key for a regulated asset.
    ComplianceRegisterUser(MsgRegisterUser),
}

impl ActionPlan {
    /// Builds a planned [`Action`] specified by this [`ActionPlan`].
    #[cfg(any(unix, windows))]
    pub fn build_unauth(
        action_plan: ActionPlan,
        fvk: &FullViewingKey,
        witness_data: &WitnessData,
        memo_key: Option<PayloadKey>,
    ) -> Result<Action> {
        use ActionPlan::*;

        Ok(match action_plan {
            Transfer(transfer_plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let auth_paths = transfer_plan
                    .spends
                    .iter()
                    .map(|spend| {
                        let note_commitment = spend.note.commit();
                        witness_data
                            .state_commitment_proofs
                            .get(&note_commitment)
                            .cloned()
                            .context(format!("could not get proof for {note_commitment:?}"))
                    })
                    .collect::<Result<Vec<_>>>()?;

                Action::Transfer(
                    transfer_plan
                        .transfer(
                            fvk,
                            vec![[0; 64].into(); transfer_plan.spends.len()],
                            auth_paths,
                            witness_data.anchor,
                            memo_key.as_ref().unwrap_or(&dummy_payload_key),
                        )
                        .map_err(|e| anyhow::anyhow!("transfer proof generation failed: {}", e))?,
                )
            }
            Consolidate(consolidate_plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let auth_paths = consolidate_plan
                    .spends
                    .iter()
                    .map(|spend| {
                        let note_commitment = spend.note.commit();
                        witness_data
                            .state_commitment_proofs
                            .get(&note_commitment)
                            .cloned()
                            .context(format!("could not get proof for {note_commitment:?}"))
                    })
                    .collect::<Result<Vec<_>>>()?;

                Action::Consolidate(
                    consolidate_plan
                        .consolidate(
                            fvk,
                            vec![[0; 64].into(); consolidate_plan.spends.len()],
                            auth_paths,
                            witness_data.anchor,
                            memo_key.as_ref().unwrap_or(&dummy_payload_key),
                        )
                        .map_err(|e| {
                            anyhow::anyhow!("consolidate proof generation failed: {}", e)
                        })?,
                )
            }
            Split(split_plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let auth_paths = split_plan
                    .spends
                    .iter()
                    .map(|spend| {
                        let note_commitment = spend.note.commit();
                        witness_data
                            .state_commitment_proofs
                            .get(&note_commitment)
                            .cloned()
                            .context(format!("could not get proof for {note_commitment:?}"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Action::Split(
                    split_plan
                        .split(
                            fvk,
                            vec![[0; 64].into(); split_plan.spends.len()],
                            auth_paths,
                            witness_data.anchor,
                            memo_key.as_ref().unwrap_or(&dummy_payload_key),
                        )
                        .map_err(|e| anyhow::anyhow!("split proof generation failed: {}", e))?,
                )
            }
            ValidatorDefinition(plan) => Action::ValidatorDefinition(plan.clone()),
            IbcAction(plan) => Action::IbcRelay(plan.clone()),
            ProposalSubmit(plan) => Action::ProposalSubmit(plan.clone()),
            ValidatorVote(plan) => Action::ValidatorVote(plan.clone()),
            ShieldedIcs20Withdrawal(plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let auth_paths = plan
                    .spends
                    .iter()
                    .map(|spend| {
                        let note_commitment = spend.note.commit();
                        witness_data
                            .state_commitment_proofs
                            .get(&note_commitment)
                            .cloned()
                            .context(format!("could not get proof for {note_commitment:?}"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Action::ShieldedIcs20Withdrawal(
                    plan.shielded_ics20_withdrawal(
                        fvk,
                        vec![[0; 64].into(); plan.spends.len()],
                        auth_paths,
                        witness_data.anchor,
                        memo_key.as_ref().unwrap_or(&dummy_payload_key),
                    )
                    .map_err(|e| {
                        anyhow::anyhow!("shielded ICS-20 withdrawal proof generation failed: {}", e)
                    })?,
                )
            }
            ComplianceRegisterAsset(msg) => Action::ComplianceRegisterAsset(msg.clone()),
            ComplianceRegisterUser(msg) => Action::ComplianceRegisterUser(msg.clone()),
        })
    }

    /// Canonical action plan ordering according to protobuf definitions.
    pub fn variant_index(&self) -> usize {
        match self {
            ActionPlan::Transfer(_) => 5,
            ActionPlan::Consolidate(_) => 6,
            ActionPlan::Split(_) => 7,
            ActionPlan::ValidatorDefinition(_) => 16,
            ActionPlan::IbcAction(_) => 17,
            ActionPlan::ProposalSubmit(_) => 18,
            ActionPlan::ValidatorVote(_) => 20,
            ActionPlan::ComplianceRegisterAsset(_) => 80,
            ActionPlan::ComplianceRegisterUser(_) => 81,
            ActionPlan::ShieldedIcs20Withdrawal(_) => 200,
        }
    }

    pub fn balance(&self) -> Balance {
        use ActionPlan::*;

        match self {
            Transfer(action) => action.balance(),
            Consolidate(action) => action.balance(),
            Split(action) => action.balance(),
            ProposalSubmit(action) => action.balance(),
            ShieldedIcs20Withdrawal(action) => action.balance(),
            IbcAction(_)
            | ValidatorDefinition(_)
            | ValidatorVote(_)
            | ComplianceRegisterAsset(_)
            | ComplianceRegisterUser(_) => Balance::default(),
        }
    }

    pub fn value_blinding(&self) -> Fr {
        use ActionPlan::*;

        match self {
            Transfer(action) => action.value_blinding,
            Consolidate(action) => action.value_blinding,
            Split(action) => action.value_blinding,
            ShieldedIcs20Withdrawal(action) => action.value_blinding,
            ValidatorDefinition(_)
            | IbcAction(_)
            | ProposalSubmit(_)
            | ValidatorVote(_)
            | ComplianceRegisterAsset(_)
            | ComplianceRegisterUser(_) => Fr::zero(),
        }
    }

    /// Compute the effect hash of the action this plan will produce.
    #[cfg(any(unix, windows))]
    pub fn effect_hash(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
    ) -> anyhow::Result<EffectHash> {
        use ActionPlan::*;

        let effect_hash = match self {
            Transfer(plan) => plan
                .transfer_body(fvk, memo_key, penumbra_sdk_tct::Tree::default().root())
                .map(|body| body.effect_hash())?,
            Consolidate(plan) => plan
                .consolidate_body(fvk, memo_key, penumbra_sdk_tct::Tree::default().root())
                .map(|body| body.effect_hash())?,
            Split(plan) => plan
                .split_body(fvk, memo_key, penumbra_sdk_tct::Tree::default().root())
                .map(|body| body.effect_hash())?,
            ValidatorDefinition(plan) => plan.effect_hash(),
            IbcAction(plan) => plan.effect_hash(),
            ProposalSubmit(plan) => plan.effect_hash(),
            ValidatorVote(plan) => plan.effect_hash(),
            ShieldedIcs20Withdrawal(plan) => plan
                .action_body(fvk, memo_key, penumbra_sdk_tct::Tree::default().root())
                .map(|body| body.effect_hash())?,
            ComplianceRegisterAsset(plan) => plan.effect_hash(),
            ComplianceRegisterUser(plan) => plan.effect_hash(),
        };

        Ok(effect_hash)
    }
}

impl From<TransferPlan> for ActionPlan {
    fn from(inner: TransferPlan) -> ActionPlan {
        ActionPlan::Transfer(inner)
    }
}

impl From<ConsolidatePlan> for ActionPlan {
    fn from(inner: ConsolidatePlan) -> ActionPlan {
        ActionPlan::Consolidate(inner)
    }
}

impl From<SplitPlan> for ActionPlan {
    fn from(inner: SplitPlan) -> ActionPlan {
        ActionPlan::Split(inner)
    }
}

impl From<penumbra_sdk_validator::validator::Definition> for ActionPlan {
    fn from(inner: penumbra_sdk_validator::validator::Definition) -> ActionPlan {
        ActionPlan::ValidatorDefinition(inner)
    }
}

impl From<IbcRelay> for ActionPlan {
    fn from(inner: IbcRelay) -> ActionPlan {
        ActionPlan::IbcAction(inner)
    }
}

impl From<ProposalSubmit> for ActionPlan {
    fn from(inner: ProposalSubmit) -> ActionPlan {
        ActionPlan::ProposalSubmit(inner)
    }
}

impl From<ValidatorVote> for ActionPlan {
    fn from(inner: ValidatorVote) -> ActionPlan {
        ActionPlan::ValidatorVote(inner)
    }
}

impl From<ShieldedIcs20WithdrawalPlan> for ActionPlan {
    fn from(inner: ShieldedIcs20WithdrawalPlan) -> ActionPlan {
        ActionPlan::ShieldedIcs20Withdrawal(inner)
    }
}

impl From<MsgRegisterAsset> for ActionPlan {
    fn from(inner: MsgRegisterAsset) -> ActionPlan {
        ActionPlan::ComplianceRegisterAsset(inner)
    }
}

impl From<MsgRegisterUser> for ActionPlan {
    fn from(inner: MsgRegisterUser) -> ActionPlan {
        ActionPlan::ComplianceRegisterUser(inner)
    }
}

impl DomainType for ActionPlan {
    type Proto = pb_t::ActionPlan;
}

impl From<ActionPlan> for pb_t::ActionPlan {
    fn from(msg: ActionPlan) -> Self {
        match msg {
            ActionPlan::Transfer(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::Transfer(inner.into())),
            },
            ActionPlan::Consolidate(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::Consolidate(inner.into())),
            },
            ActionPlan::Split(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::Split(inner.into())),
            },
            ActionPlan::ValidatorDefinition(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ValidatorDefinition(inner.into())),
            },
            ActionPlan::IbcAction(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::IbcRelayAction(inner.into())),
            },
            ActionPlan::ProposalSubmit(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ProposalSubmit(inner.into())),
            },
            ActionPlan::ValidatorVote(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ValidatorVote(inner.into())),
            },
            ActionPlan::ShieldedIcs20Withdrawal(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ShieldedIcs20Withdrawal(
                    inner.into(),
                )),
            },
            ActionPlan::ComplianceRegisterAsset(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ComplianceRegisterAsset(
                    inner.into(),
                )),
            },
            ActionPlan::ComplianceRegisterUser(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ComplianceRegisterUser(
                    inner.into(),
                )),
            },
        }
    }
}

impl TryFrom<pb_t::ActionPlan> for ActionPlan {
    type Error = anyhow::Error;

    fn try_from(proto: pb_t::ActionPlan) -> anyhow::Result<Self, Self::Error> {
        if proto.action.is_none() {
            anyhow::bail!("missing action content");
        }

        match proto
            .action
            .ok_or_else(|| anyhow!("missing action in ActionPlan proto"))?
        {
            pb_t::action_plan::Action::Transfer(inner) => {
                Ok(ActionPlan::Transfer(inner.try_into()?))
            }
            pb_t::action_plan::Action::Consolidate(inner) => {
                Ok(ActionPlan::Consolidate(inner.try_into()?))
            }
            pb_t::action_plan::Action::Split(inner) => Ok(ActionPlan::Split(inner.try_into()?)),
            pb_t::action_plan::Action::ValidatorDefinition(inner) => {
                Ok(ActionPlan::ValidatorDefinition(inner.try_into()?))
            }
            pb_t::action_plan::Action::IbcRelayAction(inner) => {
                Ok(ActionPlan::IbcAction(inner.try_into()?))
            }
            pb_t::action_plan::Action::ProposalSubmit(inner) => {
                Ok(ActionPlan::ProposalSubmit(inner.try_into()?))
            }
            pb_t::action_plan::Action::ValidatorVote(inner) => {
                Ok(ActionPlan::ValidatorVote(inner.try_into()?))
            }
            pb_t::action_plan::Action::ShieldedIcs20Withdrawal(inner) => {
                Ok(ActionPlan::ShieldedIcs20Withdrawal(inner.try_into()?))
            }
            pb_t::action_plan::Action::ComplianceRegisterAsset(inner) => {
                Ok(ActionPlan::ComplianceRegisterAsset(inner.try_into()?))
            }
            pb_t::action_plan::Action::ComplianceRegisterUser(inner) => {
                Ok(ActionPlan::ComplianceRegisterUser(inner.try_into()?))
            }
        }
    }
}
