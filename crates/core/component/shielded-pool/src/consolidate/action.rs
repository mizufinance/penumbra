use std::convert::TryInto;

use anyhow::{Context, Error};
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use penumbra_sdk_asset::balance;
use penumbra_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct as tct;
use penumbra_sdk_txhash::{EffectHash, EffectingData};

use super::{ConsolidateFamilyId, ConsolidateProof};
use crate::{backref::ENCRYPTED_BACKREF_LEN, EncryptedBackref, NotePayload};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(
    try_from = "pb::ConsolidateInputBody",
    into = "pb::ConsolidateInputBody"
)]
pub struct ConsolidateInputBody {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub encrypted_backref: EncryptedBackref,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(
    try_from = "pb::ConsolidateOutputBody",
    into = "pb::ConsolidateOutputBody"
)]
pub struct ConsolidateOutputBody {
    pub note_payload: NotePayload,
    pub wrapped_memo_key: WrappedMemoKey,
    pub ovk_wrapped_key: OvkWrappedKey,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "pb::ConsolidateBody", into = "pb::ConsolidateBody")]
pub struct ConsolidateBody {
    pub family_id: ConsolidateFamilyId,
    pub anchor: penumbra_sdk_tct::Root,
    pub balance_commitment: balance::Commitment,
    pub inputs: Vec<ConsolidateInputBody>,
    pub outputs: Vec<ConsolidateOutputBody>,
}

#[derive(Clone, Debug)]
pub struct Consolidate {
    pub body: ConsolidateBody,
    pub auth_sigs: Vec<Signature<SpendAuth>>,
    pub proof: ConsolidateProof,
}

impl ConsolidateBody {
    pub fn validate_shape(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.inputs.len() == self.family_id.input_count(),
            "consolidate family {:?} expects {} inputs, got {}",
            self.family_id,
            self.family_id.input_count(),
            self.inputs.len()
        );
        anyhow::ensure!(
            self.outputs.len() == self.family_id.output_count(),
            "consolidate family {:?} expects {} outputs, got {}",
            self.family_id,
            self.family_id.output_count(),
            self.outputs.len()
        );
        Ok(())
    }
}

impl EffectingData for ConsolidateBody {
    fn effect_hash(&self) -> EffectHash {
        let mut effecting = self.clone();
        effecting.anchor = tct::Tree::default().root();
        EffectHash::from_proto_effecting_data(&effecting.to_proto())
    }
}

impl EffectingData for Consolidate {
    fn effect_hash(&self) -> EffectHash {
        self.body.effect_hash()
    }
}

impl DomainType for Consolidate {
    type Proto = pb::Consolidate;
}

impl From<Consolidate> for pb::Consolidate {
    fn from(msg: Consolidate) -> Self {
        Self {
            body: Some(msg.body.into()),
            auth_sigs: msg.auth_sigs.into_iter().map(Into::into).collect(),
            proof: Some(msg.proof.into()),
        }
    }
}

impl TryFrom<pb::Consolidate> for Consolidate {
    type Error = Error;

    fn try_from(proto: pb::Consolidate) -> Result<Self, Self::Error> {
        let body: ConsolidateBody = proto
            .body
            .ok_or_else(|| anyhow::anyhow!("missing consolidate body"))?
            .try_into()
            .context("malformed consolidate body")?;
        body.validate_shape()?;

        let auth_sigs = proto
            .auth_sigs
            .into_iter()
            .map(|sig| sig.try_into().context("malformed consolidate auth sig"))
            .collect::<Result<Vec<_>, _>>()?;

        anyhow::ensure!(
            auth_sigs.len() == body.family_id.auth_sig_count(),
            "consolidate expected {} auth sigs, got {}",
            body.family_id.auth_sig_count(),
            auth_sigs.len()
        );

        Ok(Self {
            body,
            auth_sigs,
            proof: proto
                .proof
                .ok_or_else(|| anyhow::anyhow!("missing consolidate proof"))?
                .try_into()
                .context("malformed consolidate proof")?,
        })
    }
}

impl DomainType for ConsolidateInputBody {
    type Proto = pb::ConsolidateInputBody;
}

impl From<ConsolidateInputBody> for pb::ConsolidateInputBody {
    fn from(msg: ConsolidateInputBody) -> Self {
        Self {
            nullifier: Some(msg.nullifier.into()),
            rk: Some(msg.rk.into()),
            encrypted_backref: msg.encrypted_backref.into(),
        }
    }
}

impl TryFrom<pb::ConsolidateInputBody> for ConsolidateInputBody {
    type Error = Error;

    fn try_from(proto: pb::ConsolidateInputBody) -> Result<Self, Self::Error> {
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
        })
    }
}

impl DomainType for ConsolidateOutputBody {
    type Proto = pb::ConsolidateOutputBody;
}

impl From<ConsolidateOutputBody> for pb::ConsolidateOutputBody {
    fn from(msg: ConsolidateOutputBody) -> Self {
        Self {
            note_payload: Some(msg.note_payload.into()),
            wrapped_memo_key: msg.wrapped_memo_key.0.to_vec(),
            ovk_wrapped_key: msg.ovk_wrapped_key.0.to_vec(),
        }
    }
}

impl TryFrom<pb::ConsolidateOutputBody> for ConsolidateOutputBody {
    type Error = Error;

    fn try_from(proto: pb::ConsolidateOutputBody) -> Result<Self, Self::Error> {
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
        })
    }
}

impl DomainType for ConsolidateBody {
    type Proto = pb::ConsolidateBody;
}

impl From<ConsolidateBody> for pb::ConsolidateBody {
    fn from(msg: ConsolidateBody) -> Self {
        Self {
            family_id: msg.family_id.into(),
            anchor: Some(msg.anchor.into()),
            balance_commitment: Some(msg.balance_commitment.into()),
            inputs: msg.inputs.into_iter().map(Into::into).collect(),
            outputs: msg.outputs.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<pb::ConsolidateBody> for ConsolidateBody {
    type Error = Error;

    fn try_from(proto: pb::ConsolidateBody) -> Result<Self, Self::Error> {
        let body = Self {
            family_id: proto.family_id.try_into()?,
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
                .collect::<Result<_, _>>()?,
            outputs: proto
                .outputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        };
        body.validate_shape()?;
        Ok(body)
    }
}
