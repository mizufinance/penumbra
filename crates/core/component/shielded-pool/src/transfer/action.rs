use std::convert::TryInto;

use anyhow::{Context, Error};
use decaf377::Fq;
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use shieldd_sdk_asset::balance;
use shieldd_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_txhash::{EffectHash, EffectingData};

use super::generated::{transfer_auth_sig_count, transfer_input_count, transfer_output_count};
use crate::{
    backref::ENCRYPTED_BACKREF_LEN, discovery::TransferRouting, transfer::TransferProof,
    EncryptedBackref, NotePayload, TransferProofContext, VolumeAccumulatorPayload,
};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::TransferInputBody", into = "pb::TransferInputBody")]
pub struct TransferInputBody {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub encrypted_backref: EncryptedBackref,
    pub compliance_ciphertext: Vec<u8>,
    pub history_required: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::TransferOutputBody", into = "pb::TransferOutputBody")]
pub struct TransferOutputBody {
    pub note_payload: NotePayload,
    pub wrapped_memo_key: WrappedMemoKey,
    pub ovk_wrapped_key: OvkWrappedKey,
    pub compliance_ciphertext: Vec<u8>,
    pub compliance_metadata: Vec<u8>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::TransferBody", into = "pb::TransferBody")]
pub struct TransferBody {
    pub anchor: shieldd_sdk_tct::Root,
    pub balance_commitment: balance::Commitment,
    pub inputs: Vec<TransferInputBody>,
    pub outputs: Vec<TransferOutputBody>,
    pub target_timestamp: u64,
    pub compliance_anchor: shieldd_sdk_tct::StateCommitment,
    pub asset_anchor: shieldd_sdk_tct::StateCommitment,
    pub routing: TransferRouting,
    pub routing_parameter_set_id: Fq,
    pub volume_accumulator: VolumeAccumulatorPayload,
    pub proof_context: TransferProofContext,
}

#[derive(Clone, Debug)]
pub struct Transfer {
    pub body: TransferBody,
    pub auth_sigs: Vec<Signature<SpendAuth>>,
    pub proof: TransferProof,
}

impl TransferBody {
    pub fn validate_shape(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.inputs.len() == transfer_input_count(),
            "transfer expects {} inputs, got {}",
            transfer_input_count(),
            self.inputs.len()
        );
        anyhow::ensure!(
            self.outputs.len() == transfer_output_count(),
            "transfer expects {} outputs, got {}",
            transfer_output_count(),
            self.outputs.len()
        );
        self.volume_accumulator.validate(self.proof_context)?;
        Ok(())
    }
}

impl EffectingData for TransferBody {
    fn effect_hash(&self) -> EffectHash {
        let mut effecting = self.clone();
        // Transfer effect hashes intentionally do not commit to the witness anchor.
        // The anchor is proof context rather than user-visible action identity, and
        // keeping it out of the effect hash makes `TransferPlan` and the finalized
        // transaction action hash the same effecting data even when the real anchor
        // is filled in later during build/proving.
        effecting.anchor = shieldd_sdk_tct::Tree::default().root();
        // Compliance records are proof-bound effects. Committing to their exact
        // deterministic bytes prevents an untrusted builder from replacing the
        // encryption randomness or metadata after obtaining spend authorization.
        EffectHash::from_proto_effecting_data(&effecting.to_proto())
    }
}

impl EffectingData for Transfer {
    fn effect_hash(&self) -> EffectHash {
        self.body.effect_hash()
    }
}

impl DomainType for Transfer {
    type Proto = pb::Transfer;
}

impl From<Transfer> for pb::Transfer {
    fn from(msg: Transfer) -> Self {
        pb::Transfer {
            body: Some(msg.body.into()),
            auth_sigs: msg.auth_sigs.into_iter().map(Into::into).collect(),
            proof: Some(msg.proof.into()),
        }
    }
}

impl TryFrom<pb::Transfer> for Transfer {
    type Error = Error;

    fn try_from(proto: pb::Transfer) -> Result<Self, Self::Error> {
        let body: TransferBody = proto
            .body
            .ok_or_else(|| anyhow::anyhow!("missing transfer body"))?
            .try_into()
            .context("malformed transfer body")?;
        body.validate_shape()?;

        let auth_sigs = proto
            .auth_sigs
            .into_iter()
            .map(|sig| sig.try_into().context("malformed transfer auth sig"))
            .collect::<Result<Vec<_>, _>>()?;

        anyhow::ensure!(
            auth_sigs.len() == transfer_auth_sig_count(),
            "transfer expected {} auth sigs, got {}",
            transfer_auth_sig_count(),
            auth_sigs.len()
        );

        Ok(Self {
            body,
            auth_sigs,
            proof: proto
                .proof
                .ok_or_else(|| anyhow::anyhow!("missing transfer proof"))?
                .try_into()
                .context("malformed transfer proof")?,
        })
    }
}

impl DomainType for TransferInputBody {
    type Proto = pb::TransferInputBody;
}

impl From<TransferInputBody> for pb::TransferInputBody {
    fn from(msg: TransferInputBody) -> Self {
        Self {
            nullifier: Some(msg.nullifier.into()),
            rk: Some(msg.rk.into()),
            encrypted_backref: msg.encrypted_backref.into(),
            compliance_ciphertext: msg.compliance_ciphertext,
            history_required: msg.history_required,
        }
    }
}

impl TryFrom<pb::TransferInputBody> for TransferInputBody {
    type Error = Error;

    fn try_from(proto: pb::TransferInputBody) -> Result<Self, Self::Error> {
        let bytes: [u8; ENCRYPTED_BACKREF_LEN] = proto
            .encrypted_backref
            .try_into()
            .map_err(|_| anyhow::anyhow!("encrypted backref must be exactly 48 bytes"))?;
        let encrypted_backref = EncryptedBackref::try_from(bytes)
            .map_err(|_| anyhow::anyhow!("invalid encrypted backref"))?;

        Ok(Self {
            nullifier: proto
                .nullifier
                .ok_or_else(|| anyhow::anyhow!("missing nullifier"))?
                .try_into()
                .context("malformed nullifier")?,
            rk: proto
                .rk
                .ok_or_else(|| anyhow::anyhow!("missing rk"))?
                .try_into()
                .context("malformed rk")?,
            encrypted_backref,
            compliance_ciphertext: proto.compliance_ciphertext,
            history_required: proto.history_required,
        })
    }
}

impl DomainType for TransferOutputBody {
    type Proto = pb::TransferOutputBody;
}

impl From<TransferOutputBody> for pb::TransferOutputBody {
    fn from(msg: TransferOutputBody) -> Self {
        Self {
            note_payload: Some(msg.note_payload.into()),
            wrapped_memo_key: msg.wrapped_memo_key.0.to_vec(),
            ovk_wrapped_key: msg.ovk_wrapped_key.0.to_vec(),
            compliance_ciphertext: msg.compliance_ciphertext,
            compliance_metadata: msg.compliance_metadata,
        }
    }
}

impl TryFrom<pb::TransferOutputBody> for TransferOutputBody {
    type Error = Error;

    fn try_from(proto: pb::TransferOutputBody) -> Result<Self, Self::Error> {
        Ok(Self {
            note_payload: proto
                .note_payload
                .ok_or_else(|| anyhow::anyhow!("missing note payload"))?
                .try_into()
                .context("malformed note payload")?,
            wrapped_memo_key: proto.wrapped_memo_key[..]
                .try_into()
                .context("malformed wrapped memo key")?,
            ovk_wrapped_key: proto.ovk_wrapped_key[..]
                .try_into()
                .context("malformed ovk wrapped key")?,
            compliance_ciphertext: proto.compliance_ciphertext,
            compliance_metadata: proto.compliance_metadata,
        })
    }
}

impl DomainType for TransferBody {
    type Proto = pb::TransferBody;
}

impl From<TransferBody> for pb::TransferBody {
    fn from(msg: TransferBody) -> Self {
        Self {
            anchor: Some(msg.anchor.into()),
            balance_commitment: Some(msg.balance_commitment.into()),
            inputs: msg.inputs.into_iter().map(Into::into).collect(),
            outputs: msg.outputs.into_iter().map(Into::into).collect(),
            target_timestamp: msg.target_timestamp,
            compliance_anchor: Some(msg.compliance_anchor.into()),
            asset_anchor: Some(msg.asset_anchor.into()),
            routing: Some(msg.routing.into()),
            routing_parameter_set_id: msg.routing_parameter_set_id.to_bytes().to_vec(),
            volume_accumulator: Some(msg.volume_accumulator.into()),
            proof_context: msg.proof_context.into(),
        }
    }
}

impl TryFrom<pb::TransferBody> for TransferBody {
    type Error = Error;

    fn try_from(proto: pb::TransferBody) -> Result<Self, Self::Error> {
        let body = Self {
            anchor: proto
                .anchor
                .ok_or_else(|| anyhow::anyhow!("missing anchor"))?
                .try_into()
                .context("malformed anchor")?,
            balance_commitment: proto
                .balance_commitment
                .ok_or_else(|| anyhow::anyhow!("missing balance commitment"))?
                .try_into()
                .context("malformed balance commitment")?,
            inputs: proto
                .inputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            outputs: proto
                .outputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            target_timestamp: proto.target_timestamp,
            compliance_anchor: proto
                .compliance_anchor
                .ok_or_else(|| anyhow::anyhow!("missing compliance anchor"))?
                .try_into()
                .context("malformed compliance anchor")?,
            asset_anchor: proto
                .asset_anchor
                .ok_or_else(|| anyhow::anyhow!("missing asset anchor"))?
                .try_into()
                .context("malformed asset anchor")?,
            routing: proto
                .routing
                .ok_or_else(|| anyhow::anyhow!("missing transfer routing"))?
                .try_into()
                .context("malformed transfer routing")?,
            routing_parameter_set_id: Fq::from_bytes_checked(
                &proto
                    .routing_parameter_set_id
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("routing parameter set id must be 32 bytes"))?,
            )
            .map_err(|_| anyhow::anyhow!("routing parameter set id must be canonical"))?,
            volume_accumulator: proto
                .volume_accumulator
                .ok_or_else(|| anyhow::anyhow!("missing volume accumulator payload"))?
                .try_into()
                .context("malformed volume accumulator payload")?,
            proof_context: proto.proof_context.try_into()?,
        };
        body.validate_shape()?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::pb;
    use shieldd_sdk_compliance::TRANSFER_COMPLIANCE_METADATA_BYTES;

    #[test]
    fn transfer_output_public_metadata_is_one_bounded_protobuf_field() {
        let metadata = vec![0xa5; TRANSFER_COMPLIANCE_METADATA_BYTES];
        let output = pb::TransferOutputBody {
            note_payload: None,
            wrapped_memo_key: Vec::new(),
            ovk_wrapped_key: Vec::new(),
            compliance_ciphertext: Vec::new(),
            compliance_metadata: metadata.clone(),
        };

        let encoded = output.encode_to_vec();
        // Field 5, length-delimited, followed by the canonical 264-byte record.
        assert_eq!(&encoded[..3], &[0x2a, 0x88, 0x02]);
        assert_eq!(&encoded[3..], metadata.as_slice());
    }
}
