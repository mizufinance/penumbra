use anyhow::{anyhow, ensure, Error};
use decaf377::{Fq, Fr};
#[cfg(any(unix, windows))]
use decaf377_rdsa::Signature;
use penumbra_sdk_asset::Balance;
#[cfg(any(unix, windows))]
use penumbra_sdk_compliance::{structs::ComplianceCiphertext, ComplianceLeaf};
use penumbra_sdk_keys::{symmetric::PayloadKey, FullViewingKey};
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_tct as tct;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

#[cfg(any(unix, windows))]
use crate::transfer::{
    Transfer, TransferOutputPrivate, TransferOutputPublic, TransferProof, TransferProofPrivate,
    TransferProofPublic, TransferSpendPrivate, TransferSpendPublic,
};
use crate::transfer::{TransferBody, TransferFamilyId, TransferInputBody, TransferOutputBody};
use crate::{OutputPlan, SpendPlan};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::TransferPlan", into = "pb::TransferPlan")]
pub struct TransferPlan {
    pub body: TransferBody,
    pub value_blinding: Fr,
    pub balance: Balance,
    pub spends: Vec<SpendPlan>,
    pub outputs: Vec<OutputPlan>,
}

impl TransferPlan {
    pub fn new(
        family_id: TransferFamilyId,
        spends: Vec<SpendPlan>,
        outputs: Vec<OutputPlan>,
        value_blinding: Fr,
    ) -> anyhow::Result<Self> {
        ensure!(!spends.is_empty(), "transfer requires at least one spend");
        ensure!(!outputs.is_empty(), "transfer requires at least one output");
        ensure!(
            spends.len() == family_id.input_count(),
            "transfer family {:?} expects {} spends, got {}",
            family_id,
            family_id.input_count(),
            spends.len()
        );
        ensure!(
            outputs.len() == family_id.output_count(),
            "transfer family {:?} expects {} outputs, got {}",
            family_id,
            family_id.output_count(),
            outputs.len()
        );

        let asset_id = spends[0].note.asset_id();
        ensure!(
            spends.iter().all(|spend| spend.note.asset_id() == asset_id)
                && outputs
                    .iter()
                    .all(|output| output.value.asset_id == asset_id),
            "transfer requires all spends and outputs to use the same asset",
        );
        let balance = spends.iter().fold(Balance::default(), |mut acc, spend| {
            acc += spend.balance();
            acc
        }) + outputs.iter().fold(Balance::default(), |mut acc, output| {
            acc += output.balance();
            acc
        });

        Ok(Self {
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
        })
    }

    pub fn from_spend_output(
        spend: SpendPlan,
        output: OutputPlan,
        value_blinding: Fr,
    ) -> anyhow::Result<Self> {
        Self::new(
            TransferFamilyId::OneByOne,
            vec![spend],
            vec![output],
            value_blinding,
        )
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.spends.len(), self.outputs.len())
    }

    pub fn inputs(&self) -> &[SpendPlan] {
        &self.spends
    }

    pub fn outputs(&self) -> &[OutputPlan] {
        &self.outputs
    }

    pub fn spend_randomizers(&self) -> impl Iterator<Item = Fr> + '_ {
        self.spends.iter().map(|spend| spend.randomizer)
    }

    pub fn dest_addresses(&self) -> impl Iterator<Item = penumbra_sdk_keys::Address> + '_ {
        self.outputs
            .iter()
            .map(|output| output.dest_address.clone())
    }

    pub fn num_outputs(&self) -> usize {
        self.outputs.len()
    }

    pub fn balance(&self) -> Balance {
        self.balance.clone()
    }

    fn placeholder_body(
        spends: &[SpendPlan],
        outputs: &[OutputPlan],
        family_id: TransferFamilyId,
        balance_commitment: penumbra_sdk_asset::balance::Commitment,
    ) -> TransferBody {
        let inputs = spends
            .iter()
            .map(|spend| TransferInputBody {
                nullifier: penumbra_sdk_sct::Nullifier(Fq::from(0u64)),
                rk: decaf377_rdsa::VerificationKey::from(decaf377_rdsa::SigningKey::<
                    decaf377_rdsa::SpendAuth,
                >::from(Fr::from(0u64))),
                encrypted_backref: crate::EncryptedBackref::dummy(),
                compliance_ciphertext: spend.compliance_ciphertext.clone(),
                dleq_proof: spend_dleq_proof_bytes(spend),
            })
            .collect();
        let outputs = outputs
            .iter()
            .map(|output| TransferOutputBody {
                note_payload: output.output_note().payload(),
                wrapped_memo_key: penumbra_sdk_keys::symmetric::WrappedMemoKey([0u8; 48]),
                ovk_wrapped_key: penumbra_sdk_keys::symmetric::OvkWrappedKey([0u8; 48]),
                compliance_ciphertext: output.compliance_ciphertext.clone(),
                dleq_proofs: output_dleq_proof_bytes(output),
            })
            .collect();

        TransferBody {
            family_id,
            anchor: tct::Tree::default().root(),
            balance_commitment,
            inputs,
            outputs,
            target_timestamp: spends[0].target_timestamp,
            compliance_anchor: spends[0].compliance_anchor,
            asset_anchor: spends[0].asset_anchor,
        }
    }

    fn validate_invariants(&self) -> anyhow::Result<()> {
        self.body.validate_shape()?;
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
            "transfer net balance must equal spends plus outputs",
        );
        let first_spend = self
            .spends
            .first()
            .ok_or_else(|| anyhow!("transfer requires at least one spend"))?;
        for spend in &self.spends {
            ensure!(
                spend.note.asset_id() == first_spend.note.asset_id(),
                "transfer spends must use the same asset",
            );
            ensure!(
                spend.asset_anchor == first_spend.asset_anchor,
                "transfer spend asset anchors must match",
            );
            ensure!(
                spend.compliance_anchor == first_spend.compliance_anchor,
                "transfer spend compliance anchors must match",
            );
            ensure!(
                spend.target_timestamp == first_spend.target_timestamp,
                "transfer spend timestamps must match",
            );
            ensure!(
                spend.tx_blinding_nonce == first_spend.tx_blinding_nonce,
                "transfer spend tx blinding nonce must match",
            );
            ensure!(
                spend.is_regulated == first_spend.is_regulated,
                "transfer spend regulation flags must match",
            );
        }
        for output in &self.outputs {
            ensure!(
                output.value.asset_id == first_spend.note.asset_id(),
                "transfer outputs must use the same asset as spends",
            );
            ensure!(
                output.asset_anchor == first_spend.asset_anchor,
                "transfer output asset anchors must match spends",
            );
            ensure!(
                output.compliance_anchor == first_spend.compliance_anchor,
                "transfer output compliance anchors must match spends",
            );
            ensure!(
                output.target_timestamp == first_spend.target_timestamp,
                "transfer output timestamps must match spends",
            );
            ensure!(
                output.tx_blinding_nonce == first_spend.tx_blinding_nonce,
                "transfer output tx blinding nonce must match spends",
            );
            ensure!(
                output.is_regulated == first_spend.is_regulated,
                "transfer output regulation flags must match spends",
            );
        }
        Ok(())
    }

    pub fn transfer_body(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        anchor: tct::Root,
    ) -> anyhow::Result<TransferBody> {
        self.validate_invariants()?;

        let inputs = self
            .spends
            .iter()
            .map(|spend| {
                let spend_body = spend.spend_body(fvk, None);
                TransferInputBody {
                    nullifier: spend_body.nullifier,
                    rk: spend_body.rk,
                    encrypted_backref: spend_body.encrypted_backref,
                    compliance_ciphertext: spend_body.compliance_ciphertext,
                    dleq_proof: spend_body.dleq_proof,
                }
            })
            .collect();
        let outputs = self
            .outputs
            .iter()
            .map(|output| {
                let output_body = output.output_body(fvk.outgoing(), memo_key, None);
                TransferOutputBody {
                    note_payload: output_body.note_payload,
                    wrapped_memo_key: output_body.wrapped_memo_key,
                    ovk_wrapped_key: output_body.ovk_wrapped_key,
                    compliance_ciphertext: output_body.compliance_ciphertext,
                    dleq_proofs: output_body.dleq_proofs,
                }
            })
            .collect();

        Ok(TransferBody {
            family_id: self.body.family_id,
            anchor,
            balance_commitment: self.balance.commit(self.value_blinding),
            inputs,
            outputs,
            target_timestamp: self.spends[0].target_timestamp,
            compliance_anchor: self.spends[0].compliance_anchor,
            asset_anchor: self.spends[0].asset_anchor,
        })
    }

    #[cfg(any(unix, windows))]
    pub fn transfer_public_private(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proofs: &[tct::Proof],
        anchor: tct::Root,
    ) -> Result<(TransferProofPublic, TransferProofPrivate), crate::ProofError> {
        self.validate_invariants()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if state_commitment_proofs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "transfer expected {} state commitment proofs, got {}",
                self.spends.len(),
                state_commitment_proofs.len()
            )));
        }
        if self.spends.len() > 1 {
            let sender = self.spends[0].note.address();
            for spend in self.spends.iter().skip(1) {
                if spend.note.address() != sender {
                    return Err(crate::ProofError::InvalidPublicInput(
                        "multi-input transfer requires all spends to use the same sender address"
                            .into(),
                    ));
                }
                if spend.compliance_position != self.spends[0].compliance_position
                    || spend.compliance_path != self.spends[0].compliance_path
                {
                    return Err(crate::ProofError::InvalidPublicInput(
                        "multi-input transfer requires all spends to use the same sender compliance witness"
                            .into(),
                    ));
                }
            }
        }

        let input_publics = self
            .spends
            .iter()
            .map(|spend| {
                let spend_ct = ComplianceCiphertext::from_bytes(&spend.compliance_ciphertext)
                    .map_err(|e| {
                        crate::ProofError::InvalidPublicInput(format!(
                            "invalid transfer spend compliance ciphertext: {e}"
                        ))
                    })?;
                let (epk, c2_core, compliance_ciphertext) =
                    spend_ct.to_spend_circuit_public_inputs();
                Ok(TransferSpendPublic {
                    nullifier: spend.nullifier(fvk),
                    rk: spend.rk(fvk),
                    epk,
                    c2_core,
                    compliance_ciphertext,
                    dleq_c: spend.dleq_c,
                    dleq_s: spend.dleq_s,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let output_publics = self
            .outputs
            .iter()
            .map(|output| {
                let output_ct = ComplianceCiphertext::from_bytes(&output.compliance_ciphertext)
                    .map_err(|e| {
                        crate::ProofError::InvalidPublicInput(format!(
                            "invalid transfer output compliance ciphertext: {e}"
                        ))
                    })?;
                let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
                    output_ct.to_output_circuit_public_inputs();
                Ok(TransferOutputPublic {
                    note_commitment: output.output_note().commit(),
                    epk_1,
                    epk_2,
                    epk_3,
                    c2_core,
                    c2_ext,
                    c2_sext,
                    compliance_ciphertext,
                    dleq_c_1: output.dleq_c_1,
                    dleq_s_1: output.dleq_s_1,
                    dleq_c_2: output.dleq_c_2,
                    dleq_s_2: output.dleq_s_2,
                    dleq_c_3: output.dleq_c_3,
                    dleq_s_3: output.dleq_s_3,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let input_privates = self
            .spends
            .iter()
            .zip(state_commitment_proofs.iter().cloned())
            .map(|(spend, state_commitment_proof)| {
                Ok(TransferSpendPrivate {
                    state_commitment_proof,
                    spent_note: spend.note.clone(),
                    spend_auth_randomizer: spend.randomizer,
                    spend_compliance_ephemeral_secret: spend
                        .compliance_ephemeral_secret
                        .ok_or_else(|| {
                            crate::ProofError::InvalidPublicInput(
                                "spend plan not enriched: compliance_ephemeral_secret is missing"
                                    .into(),
                            )
                        })?,
                    spend_is_flagged: spend.is_flagged,
                    spend_salt: spend.salt,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        let output_privates = self
            .outputs
            .iter()
            .map(|output| {
                let created_note = output.output_note();
                Ok(TransferOutputPrivate {
                    recipient_compliance_path: output.compliance_path.clone(),
                    recipient_compliance_position: output.compliance_position,
                    recipient_leaf: recipient_leaf(output, &created_note),
                    output_compliance_ephemeral_secret: output
                        .compliance_ephemeral_secret
                        .ok_or_else(|| {
                            crate::ProofError::InvalidPublicInput(
                                "output plan not enriched: compliance_ephemeral_secret is missing"
                                    .into(),
                            )
                        })?,
                    output_r_2: output.r_2.ok_or_else(|| {
                        crate::ProofError::InvalidPublicInput(
                            "output plan not enriched: r_2 is missing".into(),
                        )
                    })?,
                    output_r_3: output.r_3.ok_or_else(|| {
                        crate::ProofError::InvalidPublicInput(
                            "output plan not enriched: r_3 is missing".into(),
                        )
                    })?,
                    output_is_flagged: output.is_flagged,
                    output_salt: output.salt,
                    created_note,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;

        Ok((
            TransferProofPublic {
                family_id: self.body.family_id,
                anchor,
                balance_commitment: self.balance.commit(self.value_blinding),
                asset_anchor: self.spends[0].asset_anchor,
                compliance_anchor: self.spends[0].compliance_anchor,
                target_timestamp: Fq::from(self.spends[0].target_timestamp),
                inputs: input_publics,
                outputs: output_publics,
            },
            TransferProofPrivate {
                family_id: self.body.family_id,
                action_balance_blinding: self.value_blinding,
                ak: *fvk.spend_verification_key(),
                nk: *fvk.nullifier_key(),
                asset_path: self.spends[0].asset_path.clone(),
                asset_position: self.spends[0].asset_position,
                asset_indexed_leaf: self.spends[0].asset_indexed_leaf.clone(),
                is_regulated: self.spends[0].is_regulated,
                sender_compliance_path: self.spends[0].compliance_path.clone(),
                sender_compliance_position: self.spends[0].compliance_position,
                sender_leaf: sender_leaf(&self.spends[0]),
                tx_blinding_nonce: self.spends[0].tx_blinding_nonce,
                inputs: input_privates,
                outputs: output_privates,
            },
        ))
    }

    #[cfg(any(unix, windows))]
    pub fn transfer(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<decaf377_rdsa::SpendAuth>>,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
    ) -> Result<Transfer, crate::ProofError> {
        let body = self
            .transfer_body(fvk, memo_key, anchor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "transfer expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let (public, private) =
            self.transfer_public_private(fvk, &state_commitment_proofs, anchor)?;
        let proof = TransferProof::prove(public, private)?;

        Ok(Transfer {
            body,
            auth_sigs,
            proof,
        })
    }
}

impl DomainType for TransferPlan {
    type Proto = pb::TransferPlan;
}

impl From<TransferPlan> for pb::TransferPlan {
    fn from(msg: TransferPlan) -> Self {
        Self {
            body: Some(msg.body.into()),
            value_blinding: msg.value_blinding.to_bytes().to_vec(),
            balance: Some(msg.balance.into()),
            spends: msg.spends.into_iter().map(Into::into).collect(),
            outputs: msg.outputs.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<pb::TransferPlan> for TransferPlan {
    type Error = Error;

    fn try_from(proto: pb::TransferPlan) -> Result<Self, Self::Error> {
        let value_blinding_bytes: [u8; 32] = proto
            .value_blinding
            .try_into()
            .map_err(|_| anyhow!("malformed value blinding"))?;

        Ok(Self {
            body: proto
                .body
                .ok_or_else(|| anyhow!("missing transfer plan body"))?
                .try_into()?,
            value_blinding: Fr::from_bytes_checked(&value_blinding_bytes)
                .map_err(|_| anyhow!("malformed canonical value blinding"))?,
            balance: proto
                .balance
                .ok_or_else(|| anyhow!("missing transfer plan balance"))?
                .try_into()?,
            spends: proto
                .spends
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            outputs: proto
                .outputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

fn spend_dleq_proof_bytes(spend: &SpendPlan) -> Vec<u8> {
    let mut proof = Vec::with_capacity(64);
    proof.extend_from_slice(&spend.dleq_c.to_bytes());
    proof.extend_from_slice(&spend.dleq_s.to_bytes());
    proof
}

fn output_dleq_proof_bytes(output: &OutputPlan) -> Vec<u8> {
    let mut proofs = Vec::with_capacity(192);
    proofs.extend_from_slice(&output.dleq_c_1.to_bytes());
    proofs.extend_from_slice(&output.dleq_s_1.to_bytes());
    proofs.extend_from_slice(&output.dleq_c_2.to_bytes());
    proofs.extend_from_slice(&output.dleq_s_2.to_bytes());
    proofs.extend_from_slice(&output.dleq_c_3.to_bytes());
    proofs.extend_from_slice(&output.dleq_s_3.to_bytes());
    proofs
}

#[cfg(any(unix, windows))]
fn sender_leaf(spend: &SpendPlan) -> ComplianceLeaf {
    spend.compliance_leaf.clone().unwrap_or_else(|| {
        let b_d_fq = spend
            .note
            .address()
            .diversified_generator()
            .vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        penumbra_sdk_compliance::ComplianceLeaf::new(
            spend.note.address().clone(),
            spend.note.asset_id(),
            d,
        )
    })
}

#[cfg(any(unix, windows))]
fn recipient_leaf(output: &OutputPlan, created_note: &crate::Note) -> ComplianceLeaf {
    output.compliance_leaf.clone().unwrap_or_else(|| {
        let b_d_fq = created_note
            .address()
            .diversified_generator()
            .vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        penumbra_sdk_compliance::ComplianceLeaf::new(
            created_note.address().clone(),
            created_note.asset_id(),
            d,
        )
    })
}
