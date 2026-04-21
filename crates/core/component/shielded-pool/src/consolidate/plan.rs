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
use super::{Consolidate, ConsolidateProof};
use super::{
    ConsolidateBody, ConsolidateFamilyId, ConsolidateInputBody, ConsolidateInputPrivate,
    ConsolidateInputPublic, ConsolidateOutputBody, ConsolidateOutputPrivate,
    ConsolidateOutputPublic, ConsolidateProofPrivate, ConsolidateProofPublic,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::ConsolidatePlan", into = "pb::ConsolidatePlan")]
pub struct ConsolidatePlan {
    pub body: ConsolidateBody,
    pub value_blinding: Fr,
    pub balance: Balance,
    pub spends: Vec<ShieldedInputPlan>,
    pub outputs: Vec<ShieldedOutputPlan>,
}

impl ConsolidatePlan {
    pub fn new_auto(
        spends: Vec<ShieldedInputPlan>,
        outputs: Vec<ShieldedOutputPlan>,
        value_blinding: Fr,
    ) -> anyhow::Result<Self> {
        let family_id = ConsolidateFamilyId::smallest_covering_input_count(spends.len())
            .ok_or_else(|| anyhow!("no consolidate family can cover {} inputs", spends.len()))?;
        Self::new(family_id, spends, outputs, value_blinding)
    }

    pub fn new(
        family_id: ConsolidateFamilyId,
        spends: Vec<ShieldedInputPlan>,
        outputs: Vec<ShieldedOutputPlan>,
        value_blinding: Fr,
    ) -> anyhow::Result<Self> {
        ensure!(
            !spends.is_empty(),
            "consolidate requires at least one spend"
        );
        ensure!(
            !outputs.is_empty(),
            "consolidate requires at least one output"
        );
        ensure!(
            spends.len() == family_id.input_count(),
            "consolidate family {:?} expects {} spends, got {}",
            family_id,
            family_id.input_count(),
            spends.len()
        );
        ensure!(
            outputs.len() == family_id.output_count(),
            "consolidate family {:?} expects {} outputs, got {}",
            family_id,
            family_id.output_count(),
            outputs.len()
        );

        let first_spend = spends
            .first()
            .ok_or_else(|| anyhow!("consolidate requires at least one spend"))?;
        let shared_address = first_spend.note.address();
        let shared_asset_id = first_spend.note.asset_id();
        ensure!(
            spends
                .iter()
                .all(|spend| spend.note.address() == shared_address),
            "consolidate spends must all use the same address",
        );
        ensure!(
            outputs
                .iter()
                .all(|output| output.dest_address == shared_address),
            "consolidate outputs must all use the same address as spends",
        );
        ensure!(
            spends
                .iter()
                .all(|spend| spend.note.asset_id() == shared_asset_id)
                && outputs
                    .iter()
                    .all(|output| output.value.asset_id == shared_asset_id),
            "consolidate requires all spends and outputs to use the same asset",
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
        family_id: ConsolidateFamilyId,
        balance_commitment: penumbra_sdk_asset::balance::Commitment,
    ) -> ConsolidateBody {
        let inputs = spends
            .iter()
            .map(|_| ConsolidateInputBody {
                nullifier: penumbra_sdk_sct::Nullifier(Fq::from(0u64)),
                rk: VerificationKey::from(SigningKey::<SpendAuth>::from(Fr::from(0u64))),
                encrypted_backref: crate::EncryptedBackref::dummy(),
            })
            .collect();
        let outputs = outputs
            .iter()
            .map(|output| ConsolidateOutputBody {
                note_payload: output.output_note().payload(),
                wrapped_memo_key: penumbra_sdk_keys::symmetric::WrappedMemoKey([0u8; 48]),
                ovk_wrapped_key: penumbra_sdk_keys::symmetric::OvkWrappedKey([0u8; 48]),
            })
            .collect();

        ConsolidateBody {
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
            "consolidate plan expected {} spends, got {}",
            self.body.family_id.input_count(),
            self.spends.len()
        );
        ensure!(
            self.outputs.len() == self.body.family_id.output_count(),
            "consolidate plan expected {} outputs, got {}",
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
            .ok_or_else(|| anyhow!("consolidate requires at least one spend"))?;
        let shared_address = first_spend.note.address();
        let shared_asset_id = first_spend.note.asset_id();
        ensure!(
            self.spends
                .iter()
                .all(|spend| spend.note.address() == shared_address),
            "consolidate spends must all use the same address",
        );
        ensure!(
            self.outputs
                .iter()
                .all(|output| output.dest_address == shared_address),
            "consolidate outputs must all use the same address as spends",
        );
        ensure!(
            self.spends
                .iter()
                .all(|spend| spend.note.asset_id() == shared_asset_id)
                && self
                    .outputs
                    .iter()
                    .all(|output| output.value.asset_id == shared_asset_id),
            "consolidate requires all spends and outputs to use the same asset",
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
            "consolidate net balance must equal spends plus outputs",
        );
        Ok(())
    }

    pub fn family_id(&self) -> ConsolidateFamilyId {
        self.body.family_id
    }

    pub fn balance(&self) -> Balance {
        self.balance.clone()
    }

    pub fn consolidate_public_private(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proofs: &[tct::Proof],
        anchor: tct::Root,
    ) -> Result<(ConsolidateProofPublic, ConsolidateProofPrivate), crate::ProofError> {
        self.validate_invariants()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if state_commitment_proofs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "consolidate expected {} state commitment proofs, got {}",
                self.spends.len(),
                state_commitment_proofs.len()
            )));
        }

        let input_publics = self
            .spends
            .iter()
            .map(|spend| {
                Ok(ConsolidateInputPublic {
                    nullifier: spend.nullifier(fvk),
                    rk: spend.rk(fvk),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let output_publics = self
            .outputs
            .iter()
            .map(|output| {
                Ok(ConsolidateOutputPublic {
                    note_commitment: output.output_note().commit(),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let input_privates = self
            .spends
            .iter()
            .zip(state_commitment_proofs.iter().cloned())
            .map(|(spend, state_commitment_proof)| {
                Ok(ConsolidateInputPrivate {
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
                Ok(ConsolidateOutputPrivate {
                    created_note: output.output_note(),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        Ok((
            ConsolidateProofPublic {
                family_id: self.body.family_id,
                anchor,
                balance_commitment: self.balance.commit(self.value_blinding),
                inputs: input_publics,
                outputs: output_publics,
            },
            ConsolidateProofPrivate {
                family_id: self.body.family_id,
                action_balance_blinding: self.value_blinding,
                ak: *fvk.spend_verification_key(),
                nk: *fvk.nullifier_key(),
                inputs: input_privates,
                outputs: output_privates,
            },
        ))
    }

    pub fn consolidate_body(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        anchor: tct::Root,
    ) -> anyhow::Result<ConsolidateBody> {
        self.validate_invariants()?;
        let inputs = self
            .spends
            .iter()
            .map(|spend| {
                let spend_body = spend.action_input_body(fvk);
                ConsolidateInputBody {
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
                ConsolidateOutputBody {
                    note_payload,
                    wrapped_memo_key,
                    ovk_wrapped_key,
                }
            })
            .collect();

        Ok(ConsolidateBody {
            family_id: self.body.family_id,
            anchor,
            balance_commitment: self.balance.commit(self.value_blinding),
            inputs,
            outputs,
        })
    }

    #[cfg(any(unix, windows))]
    pub fn consolidate(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
    ) -> Result<Consolidate, crate::ProofError> {
        let body = self
            .consolidate_body(fvk, memo_key, anchor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "consolidate expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let (public, private) =
            self.consolidate_public_private(fvk, &state_commitment_proofs, anchor)?;
        let proof = ConsolidateProof::prove(public, private)?;

        Ok(Consolidate {
            body,
            auth_sigs,
            proof,
        })
    }
}

impl DomainType for ConsolidatePlan {
    type Proto = pb::ConsolidatePlan;
}

impl From<ConsolidatePlan> for pb::ConsolidatePlan {
    fn from(msg: ConsolidatePlan) -> Self {
        Self {
            body: Some(msg.body.into()),
            value_blinding: msg.value_blinding.to_bytes_le().to_vec(),
            balance: Some(msg.balance.into()),
            spends: msg.spends.into_iter().map(Into::into).collect(),
            outputs: msg.outputs.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<pb::ConsolidatePlan> for ConsolidatePlan {
    type Error = Error;

    fn try_from(proto: pb::ConsolidatePlan) -> Result<Self, Self::Error> {
        let plan = Self {
            body: proto
                .body
                .ok_or_else(|| anyhow::anyhow!("missing consolidate body"))?
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
    fn consolidate_family_specs_cover_expected_shapes() {
        assert_eq!(ConsolidateFamilyId::TwoByOne.input_count(), 2);
        assert_eq!(ConsolidateFamilyId::FourByOne.input_count(), 4);
        assert_eq!(ConsolidateFamilyId::EightByOne.input_count(), 8);
        assert!(ConsolidateFamilyId::ALL
            .iter()
            .all(|family| family.output_count() == 1));
        assert_eq!(
            ConsolidateFamilyId::smallest_covering_input_count(2),
            Some(ConsolidateFamilyId::TwoByOne)
        );
        assert_eq!(
            ConsolidateFamilyId::smallest_covering_input_count(3),
            Some(ConsolidateFamilyId::FourByOne)
        );
        assert_eq!(
            ConsolidateFamilyId::smallest_covering_input_count(5),
            Some(ConsolidateFamilyId::EightByOne)
        );
    }
}
