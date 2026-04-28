#[cfg(any(unix, windows))]
use anyhow::Context;
use anyhow::{anyhow, Error, Result};
use penumbra_sdk_asset::balance;
use penumbra_sdk_keys::FullViewingKey;
use penumbra_sdk_proto::{core::transaction::v1 as pbt, DomainType};
use penumbra_sdk_shielded_pool::{Transfer, TransferPlan, TransferView};
use penumbra_sdk_txhash::{EffectHash, EffectingData, TransactionContext};
use serde::{Deserialize, Serialize};

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
    #[cfg(any(unix, windows))]
    pub fn build_unauth(
        &self,
        fvk: &FullViewingKey,
        witness_data: &WitnessData,
        memo_key: &penumbra_sdk_keys::symmetric::PayloadKey,
    ) -> Result<FeeFunding> {
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

        let transfer = self
            .transfer
            .transfer(
                fvk,
                vec![[0; 64].into(); self.transfer.spends.len()],
                auth_paths,
                witness_data.anchor,
                memo_key,
            )
            .map_err(|e| anyhow!("fee funding proof generation failed: {e}"))?;

        Ok(FeeFunding { transfer })
    }

    pub fn balance(&self) -> penumbra_sdk_asset::Balance {
        self.transfer.balance()
    }

    pub fn value_blinding(&self) -> decaf377::Fr {
        self.transfer.value_blinding
    }

    pub fn effect_hash(
        &self,
        fvk: &FullViewingKey,
        memo_key: &penumbra_sdk_keys::symmetric::PayloadKey,
    ) -> Result<EffectHash> {
        self.transfer
            .transfer_body(fvk, memo_key, penumbra_sdk_tct::Tree::default().root())
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
        anchor: penumbra_sdk_tct::Root,
        effect_hash: EffectHash,
    ) -> TransactionContext {
        TransactionContext {
            anchor,
            effect_hash,
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
        Self {
            transfer: Some(value.transfer.into()),
        }
    }
}

impl TryFrom<pbt::FeeFundingPlan> for FeeFundingPlan {
    type Error = Error;

    fn try_from(proto: pbt::FeeFundingPlan) -> Result<Self, Self::Error> {
        Ok(Self {
            transfer: proto
                .transfer
                .ok_or_else(|| anyhow!("missing fee funding transfer plan"))?
                .try_into()?,
        })
    }
}
