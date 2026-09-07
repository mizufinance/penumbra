use std::convert::TryInto;

use anyhow::{Context, Error};
use decaf377_rdsa::{Signature, SpendAuth};
use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::WithdrawalComplianceCiphertext;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_tct as tct;
use shieldd_sdk_txhash::{EffectHash, EffectingData};

use crate::{
    discovery::RoutingTag, HostWithdrawal, ShieldedIcs20WithdrawalChangeBody,
    ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalProof, TransferInputBody,
    TransferProofContext, VolumeAccumulatorPayload,
};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(
    try_from = "pb::ShieldedHostWithdrawalBody",
    into = "pb::ShieldedHostWithdrawalBody"
)]
pub struct ShieldedHostWithdrawalBody {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub inputs: Vec<TransferInputBody>,
    pub withdrawal: HostWithdrawal,
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
pub struct ShieldedHostWithdrawal {
    pub body: ShieldedHostWithdrawalBody,
    pub auth_sigs: Vec<Signature<SpendAuth>>,
    pub proof: ShieldedIcs20WithdrawalProof,
}

impl ShieldedHostWithdrawalBody {
    pub fn validate_shape(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.family_id == ShieldedIcs20WithdrawalFamilyId::Canonical,
            "shielded host withdrawal family must be canonical"
        );
        anyhow::ensure!(
            self.inputs.len() == self.family_id.input_count(),
            "shielded host withdrawal family {:?} expects {} inputs, got {}",
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

impl EffectingData for ShieldedHostWithdrawalBody {
    fn effect_hash(&self) -> EffectHash {
        let mut effecting = self.clone();
        effecting.anchor = tct::Tree::default().root();
        EffectHash::from_proto_effecting_data(&effecting.to_proto())
    }
}

impl EffectingData for ShieldedHostWithdrawal {
    fn effect_hash(&self) -> EffectHash {
        self.body.effect_hash()
    }
}

impl DomainType for ShieldedHostWithdrawal {
    type Proto = pb::ShieldedHostWithdrawal;
}

impl From<ShieldedHostWithdrawal> for pb::ShieldedHostWithdrawal {
    fn from(value: ShieldedHostWithdrawal) -> Self {
        Self {
            body: Some(value.body.into()),
            auth_sigs: value.auth_sigs.into_iter().map(Into::into).collect(),
            proof: Some(value.proof.into()),
        }
    }
}

impl TryFrom<pb::ShieldedHostWithdrawal> for ShieldedHostWithdrawal {
    type Error = Error;

    fn try_from(value: pb::ShieldedHostWithdrawal) -> Result<Self, Self::Error> {
        let body: ShieldedHostWithdrawalBody = value
            .body
            .ok_or_else(|| anyhow::anyhow!("missing shielded host withdrawal body"))?
            .try_into()
            .context("malformed shielded host withdrawal body")?;
        body.validate_shape()?;

        let auth_sigs = value
            .auth_sigs
            .into_iter()
            .map(|sig| {
                sig.try_into()
                    .context("malformed shielded host withdrawal auth sig")
            })
            .collect::<Result<Vec<_>, _>>()?;

        anyhow::ensure!(
            auth_sigs.len() == body.family_id.auth_sig_count(),
            "shielded host withdrawal expected {} auth sigs, got {}",
            body.family_id.auth_sig_count(),
            auth_sigs.len()
        );

        Ok(Self {
            body,
            auth_sigs,
            proof: value
                .proof
                .ok_or_else(|| anyhow::anyhow!("missing shielded host withdrawal proof"))?
                .try_into()
                .context("malformed shielded host withdrawal proof")?,
        })
    }
}

impl DomainType for ShieldedHostWithdrawalBody {
    type Proto = pb::ShieldedHostWithdrawalBody;
}

impl From<ShieldedHostWithdrawalBody> for pb::ShieldedHostWithdrawalBody {
    fn from(value: ShieldedHostWithdrawalBody) -> Self {
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

impl TryFrom<pb::ShieldedHostWithdrawalBody> for ShieldedHostWithdrawalBody {
    type Error = Error;

    fn try_from(value: pb::ShieldedHostWithdrawalBody) -> Result<Self, Self::Error> {
        let body = Self {
            family_id: value.family_id.try_into()?,
            anchor: value
                .anchor
                .ok_or_else(|| anyhow::anyhow!("missing shielded host withdrawal anchor"))?
                .try_into()
                .context("malformed shielded host withdrawal anchor")?,
            balance_commitment: value
                .balance_commitment
                .ok_or_else(|| {
                    anyhow::anyhow!("missing shielded host withdrawal balance commitment")
                })?
                .try_into()
                .context("malformed shielded host withdrawal balance commitment")?,
            inputs: value
                .inputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            withdrawal: value
                .withdrawal
                .ok_or_else(|| anyhow::anyhow!("missing embedded host withdrawal payload"))?
                .try_into()
                .context("malformed embedded host withdrawal payload")?,
            change_output: value
                .change_output
                .ok_or_else(|| anyhow::anyhow!("missing shielded host withdrawal change output"))?
                .try_into()
                .context("malformed shielded host withdrawal change output")?,
            target_timestamp: value.target_timestamp,
            compliance_anchor: value
                .compliance_anchor
                .ok_or_else(|| {
                    anyhow::anyhow!("missing shielded host withdrawal compliance anchor")
                })?
                .try_into()
                .context("malformed shielded host withdrawal compliance anchor")?,
            asset_anchor: value
                .asset_anchor
                .ok_or_else(|| anyhow::anyhow!("missing shielded host withdrawal asset anchor"))?
                .try_into()
                .context("malformed shielded host withdrawal asset anchor")?,
            routing_tag: value
                .routing_tag
                .ok_or_else(|| anyhow::anyhow!("missing shielded host withdrawal routing tag"))?
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
    use super::{pb, ShieldedHostWithdrawalBody};

    #[test]
    fn unknown_family_id_is_rejected_at_wire_boundary() {
        let proto = pb::ShieldedHostWithdrawalBody {
            family_id: u32::MAX,
            ..Default::default()
        };
        let error = ShieldedHostWithdrawalBody::try_from(proto)
            .expect_err("unknown withdrawal family id must be rejected on decode");
        assert!(
            error.to_string().contains("family"),
            "expected a family-id error, got: {error}"
        );
    }
}
