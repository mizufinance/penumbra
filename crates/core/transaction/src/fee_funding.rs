#[cfg(any(unix, windows))]
use anyhow::Context;
use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::balance;
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_proto::{core::transaction::v1 as pbt, DomainType};
use shieldd_sdk_shielded_pool::{Transfer, TransferPlan, TransferView};
use shieldd_sdk_txhash::{EffectHash, EffectingData, TransactionContext};

#[cfg(any(unix, windows))]
use crate::WitnessData;
use crate::{ActionView, IsAction, TransactionPerspective};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pbt::FeeFunding", into = "pbt::FeeFunding")]
pub struct FeeFunding {
    pub transfer: Transfer,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pbt::FeeFundingPlan", into = "pbt::FeeFundingPlan")]
pub struct FeeFundingPlan {
    pub transfer: TransferPlan,
}

impl FeeFundingPlan {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.transfer.proof_context
                == shieldd_sdk_shielded_pool::TransferProofContext::FeeFunding,
            "fee funding requires its explicit proof context"
        );
        self.transfer.validate()
    }

    #[cfg(any(unix, windows))]
    pub fn build_unauth(
        &self,
        fvk: &FullViewingKey,
        witness_data: &WitnessData,
        memo_key: &shieldd_sdk_keys::symmetric::PayloadKey,
        recent_position_floor: u64,
    ) -> Result<FeeFunding> {
        self.validate()?;
        let transfer_plan = &self.transfer;
        let auth_paths = self
            .transfer
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

        let transfer = transfer_plan
            .build_unauth_transfer(
                fvk,
                vec![[0; 64].into(); self.transfer.spends.len()],
                auth_paths,
                witness_data.anchor,
                memo_key,
                recent_position_floor,
            )
            .map_err(|e| anyhow!("fee funding proof generation failed: {e}"))?;

        Ok(FeeFunding { transfer })
    }

    pub fn balance(&self) -> shieldd_sdk_asset::Balance {
        self.transfer.balance()
    }

    pub fn value_blinding(&self) -> decaf377::Fr {
        self.transfer.value_blinding
    }

    pub fn effect_hash(
        &self,
        fvk: &FullViewingKey,
        memo_key: &shieldd_sdk_keys::symmetric::PayloadKey,
        recent_position_floor: u64,
    ) -> Result<EffectHash> {
        self.validate()?;
        let transfer = &self.transfer;
        transfer
            .transfer_body(
                fvk,
                memo_key,
                shieldd_sdk_tct::Tree::default().root(),
                recent_position_floor,
            )
            .map(|body| body.effect_hash())
    }
}

impl EffectingData for FeeFunding {
    fn effect_hash(&self) -> EffectHash {
        self.transfer.effect_hash()
    }
}

impl IsAction for FeeFunding {
    fn balance_commitment(&self) -> balance::Commitment {
        self.transfer.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        ActionView::Transfer(match self.transfer.view_from_perspective(txp) {
            ActionView::Transfer(view) => view,
            _ => unreachable!("transfer view expected"),
        })
    }
}

impl FeeFunding {
    pub fn view_from_perspective(&self, txp: &TransactionPerspective) -> TransferView {
        match IsAction::view_from_perspective(self, txp) {
            ActionView::Transfer(view) => view,
            _ => unreachable!("transfer view expected"),
        }
    }

    pub fn context(
        &self,
        anchor: shieldd_sdk_tct::Root,
        effect_hash: EffectHash,
        recent_position_floor: u64,
    ) -> TransactionContext {
        TransactionContext {
            anchor,
            effect_hash,
            recent_position_floor,
        }
    }
}

impl DomainType for FeeFunding {
    type Proto = pbt::FeeFunding;
}

impl From<FeeFunding> for pbt::FeeFunding {
    fn from(value: FeeFunding) -> Self {
        Self {
            transfer: Some(value.transfer.into()),
        }
    }
}

impl TryFrom<pbt::FeeFunding> for FeeFunding {
    type Error = Error;

    fn try_from(proto: pbt::FeeFunding) -> Result<Self, Self::Error> {
        Ok(Self {
            transfer: proto
                .transfer
                .ok_or_else(|| anyhow!("missing fee funding transfer"))?
                .try_into()?,
        })
    }
}

impl DomainType for FeeFundingPlan {
    type Proto = pbt::FeeFundingPlan;
}

impl From<FeeFundingPlan> for pbt::FeeFundingPlan {
    fn from(value: FeeFundingPlan) -> Self {
        let transfer = value.transfer;
        Self {
            transfer: Some(transfer.into()),
        }
    }
}

impl TryFrom<pbt::FeeFundingPlan> for FeeFundingPlan {
    type Error = Error;

    fn try_from(proto: pbt::FeeFundingPlan) -> Result<Self, Self::Error> {
        let transfer: TransferPlan = proto
            .transfer
            .ok_or_else(|| anyhow!("missing fee funding transfer plan"))?
            .try_into()?;
        let plan = Self { transfer };
        plan.validate()?;
        Ok(plan)
    }
}
