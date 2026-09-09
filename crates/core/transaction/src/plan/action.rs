#[cfg(all(feature = "prover", any(unix, windows)))]
use crate::{Action, WitnessData};
use anyhow::anyhow;
#[cfg(all(feature = "prover", any(unix, windows)))]
use anyhow::{Context, Result};
use ark_ff::Zero;
use decaf377::Fr;
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::Balance;
use shieldd_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use shieldd_sdk_ibc::IbcRelay;
use shieldd_sdk_keys::{symmetric::PayloadKey, FullViewingKey};
use shieldd_sdk_proto::{core::transaction::v1 as pb_t, DomainType};
use shieldd_sdk_shielded_pool::{
    NoteReshapePlan, ShieldedHostWithdrawalPlan, ShieldedIcs20WithdrawalPlan, ShieldedInputPlan,
    TransferPlan,
};
use shieldd_sdk_txhash::{EffectHash, EffectingData};

/// A declaration of a planned [`crate::Action`], for use in transaction creation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb_t::ActionPlan", into = "pb_t::ActionPlan")]
#[allow(clippy::large_enum_variant)]
pub enum ActionPlan {
    /// Describes a proposed fused transfer.
    Transfer(TransferPlan),
    /// Describes a padded note reshape between one and eight notes.
    NoteReshape(NoteReshapePlan),
    IbcAction(IbcRelay),
    ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalPlan),
    ShieldedHostWithdrawal(ShieldedHostWithdrawalPlan),
    /// Register an asset's regulation status in the compliance registry.
    ComplianceRegisterAsset(MsgRegisterAsset),
    /// Register a user's compliance key for a regulated asset.
    ComplianceRegisterUser(MsgRegisterUser),
}

impl ActionPlan {
    /// Real shielded spends requiring witnesses and authorization signatures.
    pub fn spends(&self) -> &[ShieldedInputPlan] {
        match self {
            ActionPlan::Transfer(plan) => &plan.spends,
            ActionPlan::NoteReshape(plan) => &plan.spends,
            ActionPlan::ShieldedIcs20Withdrawal(plan) => &plan.spends,
            ActionPlan::ShieldedHostWithdrawal(plan) => &plan.spends,
            ActionPlan::IbcAction(_)
            | ActionPlan::ComplianceRegisterAsset(_)
            | ActionPlan::ComplianceRegisterUser(_) => &[],
        }
    }

    /// Builds a planned [`crate::Action`] specified by this [`ActionPlan`].
    #[cfg(all(feature = "prover", any(unix, windows)))]
    pub fn build_unauth(
        action_plan: ActionPlan,
        fvk: &FullViewingKey,
        witness_data: &WitnessData,
        memo_key: Option<PayloadKey>,
        recent_position_floor: u64,
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
                let mut auth_paths = auth_paths;
                if let Some(commitment) = transfer_plan.accumulator_prior_commitment() {
                    auth_paths.push(
                        witness_data
                            .state_commitment_proofs
                            .get(&commitment)
                            .cloned()
                            .context(format!("could not get proof for {commitment:?}"))?,
                    );
                }

                Action::Transfer(
                    transfer_plan
                        .build_unauth_transfer(
                            fvk,
                            vec![[0; 64].into(); transfer_plan.spends.len()],
                            auth_paths,
                            witness_data.anchor,
                            memo_key.as_ref().unwrap_or(&dummy_payload_key),
                            recent_position_floor,
                        )
                        .map_err(|e| anyhow::anyhow!("transfer proof generation failed: {}", e))?,
                )
            }
            NoteReshape(note_reshape_plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let auth_paths = note_reshape_plan
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

                Action::NoteReshape(
                    note_reshape_plan
                        .note_reshape(
                            fvk,
                            vec![[0; 64].into(); note_reshape_plan.spends.len()],
                            auth_paths,
                            witness_data.anchor,
                            memo_key.as_ref().unwrap_or(&dummy_payload_key),
                            recent_position_floor,
                        )
                        .map_err(|e| {
                            anyhow::anyhow!("note reshape proof generation failed: {}", e)
                        })?,
                )
            }
            IbcAction(plan) => Action::IbcRelay(plan.clone()),
            ShieldedIcs20Withdrawal(plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let mut auth_paths = plan
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
                if let Some(commitment) = plan.accumulator_prior_commitment() {
                    auth_paths.push(
                        witness_data
                            .state_commitment_proofs
                            .get(&commitment)
                            .cloned()
                            .context(format!("could not get proof for {commitment:?}"))?,
                    );
                }
                Action::ShieldedIcs20Withdrawal(
                    plan.build_unauth_shielded_ics20_withdrawal(
                        fvk,
                        vec![[0; 64].into(); plan.spends.len()],
                        auth_paths,
                        witness_data.anchor,
                        memo_key.as_ref().unwrap_or(&dummy_payload_key),
                        recent_position_floor,
                    )
                    .map_err(|e| {
                        anyhow::anyhow!("shielded ICS-20 withdrawal proof generation failed: {}", e)
                    })?,
                )
            }
            ShieldedHostWithdrawal(plan) => {
                let dummy_payload_key: PayloadKey = [0u8; 32].into();
                let mut auth_paths = plan
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
                if let Some(commitment) = plan.accumulator_prior_commitment() {
                    auth_paths.push(
                        witness_data
                            .state_commitment_proofs
                            .get(&commitment)
                            .cloned()
                            .context(format!("could not get proof for {commitment:?}"))?,
                    );
                }
                Action::ShieldedHostWithdrawal(
                    plan.build_unauth_shielded_host_withdrawal(
                        fvk,
                        vec![[0; 64].into(); plan.spends.len()],
                        auth_paths,
                        witness_data.anchor,
                        memo_key.as_ref().unwrap_or(&dummy_payload_key),
                        recent_position_floor,
                    )
                    .map_err(|e| {
                        anyhow::anyhow!("shielded host withdrawal proof generation failed: {}", e)
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
            ActionPlan::NoteReshape(_) => 6,
            ActionPlan::IbcAction(_) => 17,
            ActionPlan::ComplianceRegisterAsset(_) => 80,
            ActionPlan::ComplianceRegisterUser(_) => 81,
            ActionPlan::ShieldedIcs20Withdrawal(_) => 200,
            ActionPlan::ShieldedHostWithdrawal(_) => 201,
        }
    }

    pub fn balance(&self) -> Balance {
        use ActionPlan::*;

        match self {
            Transfer(action) => action.balance(),
            NoteReshape(action) => action.balance(),
            ShieldedIcs20Withdrawal(action) => action.balance(),
            ShieldedHostWithdrawal(action) => action.balance(),
            IbcAction(_) | ComplianceRegisterAsset(_) | ComplianceRegisterUser(_) => {
                Balance::default()
            }
        }
    }

    pub fn value_blinding(&self) -> Fr {
        use ActionPlan::*;

        match self {
            Transfer(action) => action.value_blinding,
            NoteReshape(action) => action.value_blinding,
            ShieldedIcs20Withdrawal(action) => action.value_blinding,
            ShieldedHostWithdrawal(action) => action.value_blinding,
            IbcAction(_) | ComplianceRegisterAsset(_) | ComplianceRegisterUser(_) => Fr::zero(),
        }
    }

    /// Compute the effect hash of the action this plan will produce.
    pub fn effect_hash(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        recent_position_floor: u64,
    ) -> anyhow::Result<EffectHash> {
        use ActionPlan::*;

        let effect_hash = match self {
            Transfer(plan) => plan
                .transfer_body(
                    fvk,
                    memo_key,
                    shieldd_sdk_tct::Tree::default().root(),
                    recent_position_floor,
                )
                .map(|body| body.effect_hash())?,
            NoteReshape(plan) => plan
                .note_reshape_body(
                    fvk,
                    memo_key,
                    shieldd_sdk_tct::Tree::default().root(),
                    recent_position_floor,
                )
                .map(|body| body.effect_hash())?,
            IbcAction(plan) => plan.effect_hash(),
            ShieldedIcs20Withdrawal(plan) => plan
                .action_body(
                    fvk,
                    memo_key,
                    shieldd_sdk_tct::Tree::default().root(),
                    recent_position_floor,
                )
                .map(|body| body.effect_hash())?,
            ShieldedHostWithdrawal(plan) => plan
                .action_body(
                    fvk,
                    memo_key,
                    shieldd_sdk_tct::Tree::default().root(),
                    recent_position_floor,
                )
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

impl From<NoteReshapePlan> for ActionPlan {
    fn from(inner: NoteReshapePlan) -> ActionPlan {
        ActionPlan::NoteReshape(inner)
    }
}

impl From<IbcRelay> for ActionPlan {
    fn from(inner: IbcRelay) -> ActionPlan {
        ActionPlan::IbcAction(inner)
    }
}

impl From<ShieldedIcs20WithdrawalPlan> for ActionPlan {
    fn from(inner: ShieldedIcs20WithdrawalPlan) -> ActionPlan {
        ActionPlan::ShieldedIcs20Withdrawal(inner)
    }
}

impl From<ShieldedHostWithdrawalPlan> for ActionPlan {
    fn from(inner: ShieldedHostWithdrawalPlan) -> ActionPlan {
        ActionPlan::ShieldedHostWithdrawal(inner)
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
            ActionPlan::NoteReshape(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::NoteReshape(inner.into())),
            },
            ActionPlan::IbcAction(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::IbcRelayAction(inner.into())),
            },
            ActionPlan::ShieldedIcs20Withdrawal(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ShieldedIcs20Withdrawal(
                    inner.into(),
                )),
            },
            ActionPlan::ShieldedHostWithdrawal(inner) => pb_t::ActionPlan {
                action: Some(pb_t::action_plan::Action::ShieldedHostWithdrawal(
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
                let plan: TransferPlan = inner.try_into()?;
                anyhow::ensure!(
                    plan.proof_context == shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
                    "body transfer requires ordinary proof context"
                );
                Ok(ActionPlan::Transfer(plan))
            }
            pb_t::action_plan::Action::NoteReshape(inner) => {
                Ok(ActionPlan::NoteReshape(inner.try_into()?))
            }
            pb_t::action_plan::Action::IbcRelayAction(inner) => {
                Ok(ActionPlan::IbcAction(inner.try_into()?))
            }
            pb_t::action_plan::Action::ShieldedIcs20Withdrawal(inner) => {
                Ok(ActionPlan::ShieldedIcs20Withdrawal(inner.try_into()?))
            }
            pb_t::action_plan::Action::ShieldedHostWithdrawal(inner) => {
                Ok(ActionPlan::ShieldedHostWithdrawal(inner.try_into()?))
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
