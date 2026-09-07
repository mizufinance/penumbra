use std::convert::TryInto;

use anyhow::{Context, Error};
use decaf377_rdsa::{Signature, SpendAuth};
use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::WithdrawalComplianceCiphertext;
use shieldd_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_tct as tct;
use shieldd_sdk_txhash::{EffectHash, EffectingData};

use crate::{
    discovery::RoutingTag, shielded_ics20_withdrawal::ShieldedIcs20WithdrawalProof,
    Ics20Withdrawal, TransferInputBody, TransferProofContext, VolumeAccumulatorPayload,
};

use super::generated::ShieldedIcs20WithdrawalFamilyId;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(
    try_from = "pb::ShieldedIcs20WithdrawalChangeBody",
    into = "pb::ShieldedIcs20WithdrawalChangeBody"
)]
pub struct ShieldedIcs20WithdrawalChangeBody {
    pub note_payload: crate::NotePayload,
    pub wrapped_memo_key: WrappedMemoKey,
    pub ovk_wrapped_key: OvkWrappedKey,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(
    try_from = "pb::ShieldedIcs20WithdrawalBody",
    into = "pb::ShieldedIcs20WithdrawalBody"
)]
pub struct ShieldedIcs20WithdrawalBody {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub inputs: Vec<TransferInputBody>,
    pub withdrawal: Ics20Withdrawal,
    pub change_output: ShieldedIcs20WithdrawalChangeBody,
    pub target_timestamp: u64,
    pub compliance_anchor: tct::StateCommitment,
    pub asset_anchor: tct::StateCommitment,
    pub routing_tag: RoutingTag,
    pub routing_parameter_set_id: decaf377::Fq,
    pub withdrawal_compliance_ciphertext: WithdrawalComplianceCiphertext,
    pub volume_accumulator: VolumeAccumulatorPayload,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20Withdrawal {
    pub body: ShieldedIcs20WithdrawalBody,
    pub auth_sigs: Vec<Signature<SpendAuth>>,
    pub proof: ShieldedIcs20WithdrawalProof,
}

impl ShieldedIcs20WithdrawalBody {
    pub fn validate_shape(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.family_id == ShieldedIcs20WithdrawalFamilyId::Canonical,
            "shielded ICS-20 withdrawal family must be canonical"
        );
        anyhow::ensure!(
            self.inputs.len() == self.family_id.input_count(),
            "shielded ICS-20 withdrawal family {:?} expects {} inputs, got {}",
            self.family_id,
            self.family_id.input_count(),
            self.inputs.len()
        );
        WithdrawalComplianceCiphertext::from_bytes(
            &self.withdrawal_compliance_ciphertext.to_bytes(),
        )?;
        self.volume_accumulator
            .validate(TransferProofContext::Ordinary)?;
        Ok(())
    }
}

impl EffectingData for ShieldedIcs20WithdrawalBody {
    fn effect_hash(&self) -> EffectHash {
        let mut effecting = self.clone();
        effecting.anchor = tct::Tree::default().root();
        EffectHash::from_proto_effecting_data(&effecting.to_proto())
    }
}

impl EffectingData for ShieldedIcs20Withdrawal {
    fn effect_hash(&self) -> EffectHash {
        self.body.effect_hash()
    }
}

impl DomainType for ShieldedIcs20Withdrawal {
    type Proto = pb::ShieldedIcs20Withdrawal;
}

impl From<ShieldedIcs20Withdrawal> for pb::ShieldedIcs20Withdrawal {
    fn from(value: ShieldedIcs20Withdrawal) -> Self {
        Self {
            body: Some(value.body.into()),
            auth_sigs: value.auth_sigs.into_iter().map(Into::into).collect(),
            proof: Some(value.proof.into()),
        }
    }
}

impl TryFrom<pb::ShieldedIcs20Withdrawal> for ShieldedIcs20Withdrawal {
    type Error = Error;

    fn try_from(value: pb::ShieldedIcs20Withdrawal) -> Result<Self, Self::Error> {
        let body: ShieldedIcs20WithdrawalBody = value
            .body
            .ok_or_else(|| anyhow::anyhow!("missing shielded ICS-20 withdrawal body"))?
            .try_into()
            .context("malformed shielded ICS-20 withdrawal body")?;
        body.validate_shape()?;

        let auth_sigs = value
            .auth_sigs
            .into_iter()
            .map(|sig| {
                sig.try_into()
                    .context("malformed shielded ICS-20 withdrawal auth sig")
            })
            .collect::<Result<Vec<_>, _>>()?;

        anyhow::ensure!(
            auth_sigs.len() == body.family_id.auth_sig_count(),
            "shielded ICS-20 withdrawal expected {} auth sigs, got {}",
            body.family_id.auth_sig_count(),
            auth_sigs.len()
        );

        Ok(Self {
            body,
            auth_sigs,
            proof: value
                .proof
                .ok_or_else(|| anyhow::anyhow!("missing shielded ICS-20 withdrawal proof"))?
                .try_into()
                .context("malformed shielded ICS-20 withdrawal proof")?,
        })
    }
}

impl DomainType for ShieldedIcs20WithdrawalChangeBody {
    type Proto = pb::ShieldedIcs20WithdrawalChangeBody;
}

impl From<ShieldedIcs20WithdrawalChangeBody> for pb::ShieldedIcs20WithdrawalChangeBody {
    fn from(value: ShieldedIcs20WithdrawalChangeBody) -> Self {
        Self {
            note_payload: Some(value.note_payload.into()),
            wrapped_memo_key: value.wrapped_memo_key.0.to_vec(),
            ovk_wrapped_key: value.ovk_wrapped_key.0.to_vec(),
        }
    }
}

impl TryFrom<pb::ShieldedIcs20WithdrawalChangeBody> for ShieldedIcs20WithdrawalChangeBody {
    type Error = Error;

    fn try_from(value: pb::ShieldedIcs20WithdrawalChangeBody) -> Result<Self, Self::Error> {
        Ok(Self {
            note_payload: value
                .note_payload
                .ok_or_else(|| anyhow::anyhow!("missing change note payload"))?
                .try_into()
                .context("malformed change note payload")?,
            wrapped_memo_key: value.wrapped_memo_key[..]
                .try_into()
                .context("malformed change wrapped memo key")?,
            ovk_wrapped_key: value.ovk_wrapped_key[..]
                .try_into()
                .context("malformed change ovk wrapped key")?,
        })
    }
}

impl DomainType for ShieldedIcs20WithdrawalBody {
    type Proto = pb::ShieldedIcs20WithdrawalBody;
}

impl From<ShieldedIcs20WithdrawalBody> for pb::ShieldedIcs20WithdrawalBody {
    fn from(value: ShieldedIcs20WithdrawalBody) -> Self {
        Self {
            family_id: value.family_id.get(),
            anchor: Some(value.anchor.into()),
            balance_commitment: Some(value.balance_commitment.into()),
            inputs: value.inputs.into_iter().map(Into::into).collect(),
            withdrawal: Some(value.withdrawal.into()),
            change_output: Some(value.change_output.into()),
            target_timestamp: value.target_timestamp,
            compliance_anchor: Some(value.compliance_anchor.into()),
            asset_anchor: Some(value.asset_anchor.into()),
            routing_tag: Some(value.routing_tag.into()),
            routing_parameter_set_id: value.routing_parameter_set_id.to_bytes().to_vec(),
            withdrawal_compliance_ciphertext: value
                .withdrawal_compliance_ciphertext
                .to_bytes()
                .to_vec(),
            volume_accumulator: Some(value.volume_accumulator.into()),
        }
    }
}

impl TryFrom<pb::ShieldedIcs20WithdrawalBody> for ShieldedIcs20WithdrawalBody {
    type Error = Error;

    fn try_from(value: pb::ShieldedIcs20WithdrawalBody) -> Result<Self, Self::Error> {
        let body = Self {
            family_id: value.family_id.try_into()?,
            anchor: value
                .anchor
                .ok_or_else(|| anyhow::anyhow!("missing shielded ICS-20 withdrawal anchor"))?
                .try_into()
                .context("malformed shielded ICS-20 withdrawal anchor")?,
            balance_commitment: value
                .balance_commitment
                .ok_or_else(|| {
                    anyhow::anyhow!("missing shielded ICS-20 withdrawal balance commitment")
                })?
                .try_into()
                .context("malformed shielded ICS-20 withdrawal balance commitment")?,
            inputs: value
                .inputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            withdrawal: value
                .withdrawal
                .ok_or_else(|| anyhow::anyhow!("missing embedded ICS-20 withdrawal payload"))?
                .try_into()
                .context("malformed embedded ICS-20 withdrawal payload")?,
            change_output: value
                .change_output
                .ok_or_else(|| anyhow::anyhow!("missing shielded ICS-20 withdrawal change output"))?
                .try_into()
                .context("malformed shielded ICS-20 withdrawal change output")?,
            target_timestamp: value.target_timestamp,
            compliance_anchor: value
                .compliance_anchor
                .ok_or_else(|| {
                    anyhow::anyhow!("missing shielded ICS-20 withdrawal compliance anchor")
                })?
                .try_into()
                .context("malformed shielded ICS-20 withdrawal compliance anchor")?,
            asset_anchor: value
                .asset_anchor
                .ok_or_else(|| anyhow::anyhow!("missing shielded ICS-20 withdrawal asset anchor"))?
                .try_into()
                .context("malformed shielded ICS-20 withdrawal asset anchor")?,
            routing_tag: value
                .routing_tag
                .ok_or_else(|| anyhow::anyhow!("missing shielded ICS-20 withdrawal routing tag"))?
                .try_into()?,
            routing_parameter_set_id: decaf377::Fq::from_bytes_checked(
                &value
                    .routing_parameter_set_id
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("routing parameter set id must be 32 bytes"))?,
            )
            .map_err(|_| anyhow::anyhow!("routing parameter set id must be canonical"))?,
            withdrawal_compliance_ciphertext: WithdrawalComplianceCiphertext::from_bytes(
                &value.withdrawal_compliance_ciphertext,
            )
            .context("malformed withdrawal compliance ciphertext")?,
            volume_accumulator: value
                .volume_accumulator
                .ok_or_else(|| anyhow::anyhow!("missing volume accumulator payload"))?
                .try_into()
                .context("malformed volume accumulator payload")?,
        };
        body.validate_shape()?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{pb, ShieldedIcs20WithdrawalBody};

    // TXN-M3: unknown withdrawal family_id must be rejected on decode, before
    // reaching the panicking registry lookups in consensus verification.
    #[test]
    fn unknown_family_id_is_rejected_at_wire_boundary() {
        let proto = pb::ShieldedIcs20WithdrawalBody {
            family_id: u32::MAX,
            ..Default::default()
        };
        let err = ShieldedIcs20WithdrawalBody::try_from(proto)
            .expect_err("unknown withdrawal family id must be rejected on decode");
        assert!(
            err.to_string().contains("family"),
            "expected a family-id error, got: {err}"
        );
    }
}
