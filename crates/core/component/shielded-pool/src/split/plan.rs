use anyhow::{anyhow, ensure, Context, Error};
use decaf377::{Fq, Fr};
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_keys::symmetric::PayloadKey;
use penumbra_sdk_keys::FullViewingKey;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_tct as tct;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use crate::{ShieldedInputPlan, ShieldedOutputPlan};
#[cfg(any(unix, windows))]
use decaf377_rdsa::Signature;

#[cfg(any(unix, windows))]
use super::{Split, SplitProof};
use super::{
    SplitBody, SplitFamilyId, SplitInputBody, SplitInputPrivate, SplitInputPublic, SplitOutputBody,
    SplitOutputPrivate, SplitOutputPublic, SplitProofPrivate, SplitProofPublic,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::SplitPlan", into = "pb::SplitPlan")]
pub struct SplitPlan {
    pub body: SplitBody,
    pub value_blinding: Fr,
    pub balance: Balance,
    pub spends: Vec<ShieldedInputPlan>,
    pub outputs: Vec<ShieldedOutputPlan>,
}

impl SplitPlan {
    pub fn new_auto(
        spends: Vec<ShieldedInputPlan>,
        outputs: Vec<ShieldedOutputPlan>,
        value_blinding: Fr,
    ) -> anyhow::Result<Self> {
        let family_id = SplitFamilyId::smallest_covering_output_count(outputs.len())
            .ok_or_else(|| anyhow!("no split family can cover {} outputs", outputs.len()))?;
        Self::new(family_id, spends, outputs, value_blinding)
    }

    pub fn new(
        family_id: SplitFamilyId,
        spends: Vec<ShieldedInputPlan>,
        outputs: Vec<ShieldedOutputPlan>,
        value_blinding: Fr,
    ) -> anyhow::Result<Self> {
        ensure!(!spends.is_empty(), "split requires at least one spend");
        ensure!(!outputs.is_empty(), "split requires at least one output");
        ensure!(
            spends.len() == family_id.input_count(),
            "split family {:?} expects {} spends, got {}",
            family_id,
            family_id.input_count(),
            spends.len()
        );
        ensure!(
            outputs.len() == family_id.output_count(),
            "split family {:?} expects {} outputs, got {}",
            family_id,
            family_id.output_count(),
            outputs.len()
        );

        let first_spend = spends
            .first()
            .ok_or_else(|| anyhow!("split requires at least one spend"))?;
        let shared_address = first_spend.note.address();
        let shared_asset_id = first_spend.note.asset_id();
        ensure!(
            spends
                .iter()
                .all(|spend| spend.note.address() == shared_address),
            "split spends must all use the same address",
        );
        ensure!(
            outputs
                .iter()
                .all(|output| output.dest_address == shared_address),
            "split outputs must all use the same address as spends",
        );
        ensure!(
            spends
                .iter()
                .all(|spend| spend.note.asset_id() == shared_asset_id)
                && outputs
                    .iter()
                    .all(|output| output.value.asset_id == shared_asset_id),
            "split requires all spends and outputs to use the same asset",
        );

        let balance = spends.iter().fold(Balance::default(), |mut acc, spend| {
            acc += spend.balance();
            acc
        }) + outputs.iter().fold(Balance::default(), |mut acc, output| {
            acc += output.balance();
            acc
        });

        let plan = Self {
            body: Self::placeholder_body(
                &spends,
                &outputs,
                family_id,
                balance.commit(value_blinding),
            ),
            value_blinding,
            balance,
            spends,
            outputs,
        };
        plan.validate_invariants()?;
        Ok(plan)
    }

    fn placeholder_body(
        spends: &[ShieldedInputPlan],
        outputs: &[ShieldedOutputPlan],
        family_id: SplitFamilyId,
        balance_commitment: penumbra_sdk_asset::balance::Commitment,
    ) -> SplitBody {
        let inputs = spends
            .iter()
            .map(|_| SplitInputBody {
                nullifier: penumbra_sdk_sct::Nullifier(Fq::from(0u64)),
                rk: VerificationKey::from(SigningKey::<SpendAuth>::from(Fr::from(0u64))),
                encrypted_backref: crate::EncryptedBackref::dummy(),
            })
            .collect();
        let outputs = outputs
            .iter()
            .map(|output| SplitOutputBody {
                note_payload: output.output_note().payload(),
                wrapped_memo_key: penumbra_sdk_keys::symmetric::WrappedMemoKey([0u8; 48]),
                ovk_wrapped_key: penumbra_sdk_keys::symmetric::OvkWrappedKey([0u8; 48]),
            })
            .collect();

        SplitBody {
            family_id,
            anchor: tct::Tree::default().root(),
            balance_commitment,
            inputs,
            outputs,
        }
    }

    pub fn validate_shape(&self) -> anyhow::Result<()> {
        self.body.validate_shape()?;
        ensure!(
            self.spends.len() == self.body.family_id.input_count(),
            "split plan expected {} spends, got {}",
            self.body.family_id.input_count(),
            self.spends.len()
        );
        ensure!(
            self.outputs.len() == self.body.family_id.output_count(),
            "split plan expected {} outputs, got {}",
            self.body.family_id.output_count(),
            self.outputs.len()
        );
        Ok(())
    }

    pub fn validate_invariants(&self) -> anyhow::Result<()> {
        self.validate_shape()?;
        let first_spend = self
            .spends
            .first()
            .ok_or_else(|| anyhow!("split requires at least one spend"))?;
        let shared_address = first_spend.note.address();
        let shared_asset_id = first_spend.note.asset_id();
        ensure!(
            self.spends
                .iter()
                .all(|spend| spend.note.address() == shared_address),
            "split spends must all use the same address",
        );
        ensure!(
            self.outputs
                .iter()
                .all(|output| output.dest_address == shared_address),
            "split outputs must all use the same address as spends",
        );
        ensure!(
            self.spends
                .iter()
                .all(|spend| spend.note.asset_id() == shared_asset_id)
                && self
                    .outputs
                    .iter()
                    .all(|output| output.value.asset_id == shared_asset_id),
            "split requires all spends and outputs to use the same asset",
        );
        ensure!(
            self.balance
                == self
                    .spends
                    .iter()
                    .fold(Balance::default(), |mut acc, spend| {
                        acc += spend.balance();
                        acc
                    })
                    + self
                        .outputs
                        .iter()
                        .fold(Balance::default(), |mut acc, output| {
                            acc += output.balance();
                            acc
                        }),
            "split net balance must equal spends plus outputs",
        );
        Ok(())
    }

    pub fn family_id(&self) -> SplitFamilyId {
        self.body.family_id
    }

    pub fn balance(&self) -> Balance {
        self.balance.clone()
    }

    pub fn split_public_private(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proofs: &[tct::Proof],
        anchor: tct::Root,
    ) -> Result<(SplitProofPublic, SplitProofPrivate), crate::ProofError> {
        self.validate_invariants()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if state_commitment_proofs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "split expected {} state commitment proofs, got {}",
                self.spends.len(),
                state_commitment_proofs.len()
            )));
        }

        let input_publics = self
            .spends
            .iter()
            .map(|spend| {
                Ok(SplitInputPublic {
                    nullifier: spend.nullifier(fvk),
                    rk: spend.rk(fvk),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let output_publics = self
            .outputs
            .iter()
            .map(|output| {
                Ok(SplitOutputPublic {
                    note_commitment: output.output_note().commit(),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let input_privates = self
            .spends
            .iter()
            .zip(state_commitment_proofs.iter().cloned())
            .map(|(spend, state_commitment_proof)| {
                Ok(SplitInputPrivate {
                    state_commitment_proof,
                    spent_note: spend.note.clone(),
                    spend_auth_randomizer: spend.randomizer,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let output_privates = self
            .outputs
            .iter()
            .map(|output| {
                Ok(SplitOutputPrivate {
                    created_note: output.output_note(),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        Ok((
            SplitProofPublic {
                family_id: self.body.family_id,
                anchor,
                balance_commitment: self.balance.commit(self.value_blinding),
                inputs: input_publics,
                outputs: output_publics,
            },
            SplitProofPrivate {
                family_id: self.body.family_id,
                action_balance_blinding: self.value_blinding,
                ak: *fvk.spend_verification_key(),
                nk: *fvk.nullifier_key(),
                inputs: input_privates,
                outputs: output_privates,
            },
        ))
    }

    pub fn split_body(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        anchor: tct::Root,
    ) -> anyhow::Result<SplitBody> {
        self.validate_invariants()?;
        let inputs = self
            .spends
            .iter()
            .map(|spend| {
                let spend_body = spend.action_input_body(fvk);
                SplitInputBody {
                    nullifier: spend_body.nullifier,
                    rk: spend_body.rk,
                    encrypted_backref: spend_body.encrypted_backref,
                }
            })
            .collect();
        let outputs = self
            .outputs
            .iter()
            .map(|output| {
                let (note_payload, wrapped_memo_key, ovk_wrapped_key) =
                    output.action_output_parts(fvk.outgoing(), memo_key);
                SplitOutputBody {
                    note_payload,
                    wrapped_memo_key,
                    ovk_wrapped_key,
                }
            })
            .collect();

        Ok(SplitBody {
            family_id: self.body.family_id,
            anchor,
            balance_commitment: self.balance.commit(self.value_blinding),
            inputs,
            outputs,
        })
    }

    #[cfg(any(unix, windows))]
    pub fn split(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
    ) -> Result<Split, crate::ProofError> {
        let body = self
            .split_body(fvk, memo_key, anchor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "split expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let (public, private) = self.split_public_private(fvk, &state_commitment_proofs, anchor)?;
        let proof = SplitProof::prove(public, private)?;

        Ok(Split {
            body,
            auth_sigs,
            proof,
        })
    }
}

impl DomainType for SplitPlan {
    type Proto = pb::SplitPlan;
}

impl From<SplitPlan> for pb::SplitPlan {
    fn from(msg: SplitPlan) -> Self {
        Self {
            body: Some(msg.body.into()),
            value_blinding: msg.value_blinding.to_bytes_le().to_vec(),
            balance: Some(msg.balance.into()),
            spends: msg.spends.into_iter().map(Into::into).collect(),
            outputs: msg.outputs.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<pb::SplitPlan> for SplitPlan {
    type Error = Error;

    fn try_from(proto: pb::SplitPlan) -> Result<Self, Self::Error> {
        let plan = Self {
            body: proto
                .body
                .ok_or_else(|| anyhow::anyhow!("missing split body"))?
                .try_into()?,
            value_blinding: Fr::from_bytes_checked(
                proto
                    .value_blinding
                    .as_slice()
                    .try_into()
                    .context("malformed value blinding")?,
            )
            .map_err(|_| anyhow::anyhow!("invalid value blinding"))?,
            balance: proto
                .balance
                .ok_or_else(|| anyhow::anyhow!("missing balance"))?
                .try_into()?,
            spends: proto
                .spends
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            outputs: proto
                .outputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        };
        plan.validate_invariants()?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_family_specs_cover_expected_shapes() {
        assert_eq!(SplitFamilyId::OneByFour.output_count(), 4);
        assert_eq!(SplitFamilyId::OneByEight.output_count(), 8);
        assert!(SplitFamilyId::ALL
            .iter()
            .all(|family| family.input_count() == 1));
        assert_eq!(
            SplitFamilyId::smallest_covering_output_count(4),
            Some(SplitFamilyId::OneByFour)
        );
        assert_eq!(
            SplitFamilyId::smallest_covering_output_count(5),
            Some(SplitFamilyId::OneByEight)
        );
    }
}
