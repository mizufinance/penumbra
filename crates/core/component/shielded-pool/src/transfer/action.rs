use std::convert::TryInto;

use anyhow::{Context, Error};
use decaf377::Fq;
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use penumbra_sdk_asset::balance;
use penumbra_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_txhash::{EffectHash, EffectingData};

use super::generated::{transfer_auth_sig_count, transfer_input_count, transfer_output_count};
use crate::{
    backref::ENCRYPTED_BACKREF_LEN, transfer::TransferProof, EncryptedBackref, NotePayload,
};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::TransferInputBody", into = "pb::TransferInputBody")]
pub struct TransferInputBody {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub encrypted_backref: EncryptedBackref,
    pub compliance_ciphertext: Vec<u8>,
}

impl TransferInputBody {
    pub fn is_dummy(&self) -> bool {
        self.encrypted_backref.is_empty() || self.nullifier.0 == Fq::from(0u64)
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::TransferOutputBody", into = "pb::TransferOutputBody")]
pub struct TransferOutputBody {
    pub note_payload: NotePayload,
    pub wrapped_memo_key: WrappedMemoKey,
    pub ovk_wrapped_key: OvkWrappedKey,
    pub compliance_ciphertext: Vec<u8>,
    pub orbis_upload_bundle: Vec<u8>,
}

impl TransferOutputBody {
    pub fn is_dummy(&self) -> bool {
        self.wrapped_memo_key.0 == [0u8; 48] && self.ovk_wrapped_key.0 == [0u8; 48]
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::TransferBody", into = "pb::TransferBody")]
pub struct TransferBody {
    pub anchor: penumbra_sdk_tct::Root,
    pub balance_commitment: balance::Commitment,
    pub inputs: Vec<TransferInputBody>,
    pub outputs: Vec<TransferOutputBody>,
    pub target_timestamp: u64,
    pub compliance_anchor: penumbra_sdk_tct::StateCommitment,
    pub asset_anchor: penumbra_sdk_tct::StateCommitment,
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
        effecting.anchor = penumbra_sdk_tct::Tree::default().root();
        // Transfer compliance bytes are constructed during body assembly and are not
        // part of the user-selected economic effect of the transfer action. Clearing
        // them keeps the effect hash stable across repeated body construction while
        // the full transaction auth hash still commits to the serialized body.
        for input in &mut effecting.inputs {
            input.compliance_ciphertext.clear();
        }
        for output in &mut effecting.outputs {
            output.compliance_ciphertext.clear();
            output.orbis_upload_bundle.clear();
        }
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
        }
    }
}

impl TryFrom<pb::TransferInputBody> for TransferInputBody {
    type Error = Error;

    fn try_from(proto: pb::TransferInputBody) -> Result<Self, Self::Error> {
        let encrypted_backref = if proto.encrypted_backref.len() == ENCRYPTED_BACKREF_LEN {
            let bytes: [u8; ENCRYPTED_BACKREF_LEN] = proto
                .encrypted_backref
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid encrypted backref"))?;
            EncryptedBackref::try_from(bytes)
                .map_err(|_| anyhow::anyhow!("invalid encrypted backref"))?
        } else if proto.encrypted_backref.is_empty() {
            EncryptedBackref::dummy()
        } else {
            anyhow::bail!("invalid encrypted backref length")
        };

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
            orbis_upload_bundle: msg.orbis_upload_bundle,
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
            orbis_upload_bundle: proto.orbis_upload_bundle,
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
        };
        body.validate_shape()?;
        Ok(body)
    }
}
