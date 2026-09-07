use decaf377::Fr;
use decaf377_rdsa::{SpendAuth, VerificationKey};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::{Balance, Value};
use shieldd_sdk_keys::{keys::IncomingViewingKey, Address, FullViewingKey};
use shieldd_sdk_proto::core::component::shielded_pool::v1 as pb;
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_tct as tct;
use std::convert::{TryFrom, TryInto};

use crate::{Backref, Note, Rseed, TransferInputBody};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::ShieldedInputPlan", into = "pb::ShieldedInputPlan")]
pub struct ShieldedInputPlan {
    pub note: Note,
    pub position: tct::Position,
    pub randomizer: Fr,
    pub value_blinding: Fr,
}

impl ShieldedInputPlan {
    pub fn new<R: CryptoRng + RngCore>(
        rng: &mut R,
        note: Note,
        position: tct::Position,
    ) -> ShieldedInputPlan {
        ShieldedInputPlan {
            note,
            position,
            randomizer: Fr::rand(rng),
            value_blinding: Fr::rand(rng),
        }
    }

    pub fn action_input_body(
        &self,
        fvk: &FullViewingKey,
        recent_position_floor: u64,
    ) -> anyhow::Result<TransferInputBody> {
        let backref = Backref::new(self.note.commit());
        let encrypted_backref = backref.encrypt(&fvk.backref_key(), &self.nullifier(fvk));

        Ok(TransferInputBody {
            nullifier: self.nullifier(fvk),
            rk: self.rk(fvk),
            encrypted_backref,
            compliance_ciphertext: Vec::new(),
            history_required: shieldd_sdk_sct::nullifier_generation::is_old(
                u64::from(self.position),
                recent_position_floor,
            )?,
        })
    }

    pub fn rk(&self, fvk: &FullViewingKey) -> VerificationKey<SpendAuth> {
        fvk.spend_verification_key().randomize(&self.randomizer)
    }

    pub fn nullifier(&self, fvk: &FullViewingKey) -> Nullifier {
        let nk = fvk.nullifier_key();
        Nullifier::derive(nk, self.position, &self.note.commit())
    }

    pub fn balance(&self) -> Balance {
        Value {
            amount: self.note.value().amount,
            asset_id: self.note.value().asset_id,
        }
        .into()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::ShieldedOutputPlan", into = "pb::ShieldedOutputPlan")]
pub struct ShieldedOutputPlan {
    pub value: Value,
    pub dest_address: Address,
    pub rseed: Rseed,
    pub value_blinding: Fr,
}

impl ShieldedOutputPlan {
    pub fn new<R: RngCore + CryptoRng>(
        rng: &mut R,
        value: Value,
        dest_address: Address,
    ) -> ShieldedOutputPlan {
        let rseed = Rseed::generate(rng);
        let value_blinding = Fr::rand(rng);
        ShieldedOutputPlan {
            value,
            dest_address,
            rseed,
            value_blinding,
        }
    }

    pub fn output_note(&self) -> Note {
        Note::from_parts(self.dest_address.clone(), self.value, self.rseed)
            .expect("transmission key in address is always valid")
    }

    pub fn is_viewed_by(&self, ivk: &IncomingViewingKey) -> bool {
        ivk.views_address(&self.dest_address)
    }

    pub fn balance(&self) -> Balance {
        -Balance::from(self.value)
    }
}

impl From<ShieldedInputPlan> for pb::ShieldedInputPlan {
    fn from(plan: ShieldedInputPlan) -> Self {
        Self {
            note: Some(plan.note.into()),
            position: plan.position.into(),
            randomizer: plan.randomizer.to_bytes().to_vec(),
            value_blinding: plan.value_blinding.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::ShieldedInputPlan> for ShieldedInputPlan {
    type Error = anyhow::Error;
    fn try_from(plan: pb::ShieldedInputPlan) -> anyhow::Result<Self> {
        Ok(Self {
            note: plan
                .note
                .ok_or_else(|| anyhow::anyhow!("missing note"))?
                .try_into()?,
            position: plan.position.into(),
            randomizer: parse_fr(&plan.randomizer)?,
            value_blinding: parse_fr(&plan.value_blinding)?,
        })
    }
}

impl From<ShieldedOutputPlan> for pb::ShieldedOutputPlan {
    fn from(plan: ShieldedOutputPlan) -> Self {
        Self {
            value: Some(plan.value.into()),
            dest_address: Some(plan.dest_address.into()),
            rseed: plan.rseed.0.to_vec(),
            value_blinding: plan.value_blinding.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::ShieldedOutputPlan> for ShieldedOutputPlan {
    type Error = anyhow::Error;
    fn try_from(plan: pb::ShieldedOutputPlan) -> anyhow::Result<Self> {
        Ok(Self {
            value: plan
                .value
                .ok_or_else(|| anyhow::anyhow!("missing value"))?
                .try_into()?,
            dest_address: plan
                .dest_address
                .ok_or_else(|| anyhow::anyhow!("missing destination"))?
                .try_into()?,
            rseed: Rseed(plan.rseed.as_slice().try_into()?),
            value_blinding: parse_fr(&plan.value_blinding)?,
        })
    }
}

fn parse_fr(bytes: &[u8]) -> anyhow::Result<Fr> {
    Fr::from_bytes_checked(bytes.try_into()?).map_err(|_| anyhow::anyhow!("noncanonical scalar"))
}
