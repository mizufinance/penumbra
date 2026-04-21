use decaf377::{Fq, Fr};
use decaf377_ka as ka;
use decaf377_rdsa::{SpendAuth, VerificationKey};
use penumbra_sdk_asset::{Balance, Value};
use penumbra_sdk_compliance::MerklePath;
use penumbra_sdk_keys::{
    keys::{IncomingViewingKey, OutgoingViewingKey},
    symmetric::{OvkWrappedKey, WrappedMemoKey},
    Address, FullViewingKey, PayloadKey,
};
use penumbra_sdk_proto::core::component::shielded_pool::v1 as pb;
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct as tct;
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use crate::{Backref, Note, Rseed, TransferInputBody};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::ShieldedInputPlan", into = "pb::ShieldedInputPlan")]
pub struct ShieldedInputPlan {
    pub note: Note,
    pub position: tct::Position,
    pub randomizer: Fr,
    pub value_blinding: Fr,
    pub proof_blinding_r: Fq,
    pub proof_blinding_s: Fq,
    pub compliance_path: MerklePath,
    #[serde(skip)]
    pub compliance_ciphertext: Vec<u8>,
    #[serde(skip)]
    pub compliance_leaf: Option<penumbra_sdk_compliance::ComplianceLeaf>,
    #[serde(skip)]
    pub compliance_ephemeral_secret: Option<Fr>,
    #[serde(skip)]
    pub is_regulated: bool,
    #[serde(skip)]
    pub tx_blinding_nonce: Fr,
    #[serde(skip)]
    pub compliance_anchor: tct::StateCommitment,
    #[serde(skip)]
    pub asset_anchor: tct::StateCommitment,
    #[serde(skip)]
    pub asset_path: MerklePath,
    #[serde(skip)]
    pub asset_position: u64,
    #[serde(skip)]
    pub asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf,
    #[serde(skip)]
    pub compliance_position: u64,
    #[serde(skip)]
    pub dk_pub: decaf377::Element,
    #[serde(skip)]
    pub threshold: u128,
    #[serde(skip)]
    pub ring_pk: decaf377::Element,
    #[serde(skip)]
    pub is_flagged: bool,
    #[serde(skip)]
    pub salt: Fq,
    #[serde(skip)]
    pub dleq_k: Fr,
    #[serde(skip)]
    pub dleq_c: Fq,
    #[serde(skip)]
    pub dleq_s: Fq,
    #[serde(skip)]
    pub target_timestamp: u64,
}

impl ShieldedInputPlan {
    /// Set compliance metadata fields needed by `build_transfer_compliance`.
    /// Per-action ciphertext/DLEQ are not computed here — transfer_body() builds
    /// transfer-level compliance and clears per-action fields.
    pub fn set_compliance_details(
        &mut self,
        _rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    ) -> anyhow::Result<()> {
        let (ring_pk, dk_pub) = if self.is_regulated {
            (
                self.asset_indexed_leaf.ring.ring_pk,
                self.asset_indexed_leaf.params.dk_pub,
            )
        } else {
            (
                *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK,
                *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB,
            )
        };
        let threshold = self.asset_indexed_leaf.params.threshold;
        let amount_u128: u128 = self.note.amount().into();
        let is_flagged = amount_u128 >= threshold;

        let b_d_fq = self
            .note
            .address()
            .diversified_generator()
            .vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let compliance_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            self.note.address().clone(),
            self.note.asset_id(),
            d,
        );

        self.compliance_leaf = Some(compliance_leaf);
        self.ring_pk = ring_pk;
        self.dk_pub = dk_pub;
        self.threshold = threshold;
        self.is_flagged = is_flagged;

        Ok(())
    }

    pub fn new<R: CryptoRng + RngCore>(
        rng: &mut R,
        note: Note,
        position: tct::Position,
    ) -> ShieldedInputPlan {
        let ring_pk = *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK;
        let dk_pub = *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB;

        let note_b_d_fq = note
            .address()
            .diversified_generator()
            .vartime_compress_to_field();
        let note_d = penumbra_sdk_compliance::derive_compliance_scalar(note_b_d_fq);
        let compliance_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            note.address().clone(),
            note.asset_id(),
            note_d,
        );

        let (compliance_anchor, compliance_path, compliance_position) =
            penumbra_sdk_compliance::default_user_proof(&compliance_leaf);
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            penumbra_sdk_compliance::create_default_imt_proof(note.asset_id().0);

        ShieldedInputPlan {
            note,
            position,
            randomizer: Fr::rand(rng),
            value_blinding: Fr::rand(rng),
            proof_blinding_r: Fq::rand(rng),
            proof_blinding_s: Fq::rand(rng),
            compliance_path,
            compliance_ciphertext: Vec::new(),
            compliance_leaf: Some(compliance_leaf),
            compliance_ephemeral_secret: None,
            is_regulated: false,
            tx_blinding_nonce: Fr::rand(rng),
            compliance_anchor,
            asset_anchor,
            asset_path,
            asset_position,
            asset_indexed_leaf,
            compliance_position,
            ring_pk,
            dk_pub,
            threshold: u128::MAX,
            is_flagged: false,
            salt: Fq::from(0u64),
            dleq_k: Fr::from(0u64),
            dleq_c: Fq::from(0u64),
            dleq_s: Fq::from(0u64),
            target_timestamp: 0u64,
        }
    }

    pub fn action_input_body(&self, fvk: &FullViewingKey) -> TransferInputBody {
        let backref = Backref::new(self.note.commit());
        let encrypted_backref = backref.encrypt(&fvk.backref_key(), &self.nullifier(fvk));

        let mut dleq_proof = Vec::with_capacity(64);
        dleq_proof.extend_from_slice(&self.dleq_c.to_bytes());
        dleq_proof.extend_from_slice(&self.dleq_s.to_bytes());

        TransferInputBody {
            nullifier: self.nullifier(fvk),
            rk: self.rk(fvk),
            encrypted_backref,
            compliance_ciphertext: self.compliance_ciphertext.clone(),
            dleq_proof,
        }
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
    pub proof_blinding_r: Fq,
    pub proof_blinding_s: Fq,
    pub compliance_path: MerklePath,
    #[serde(skip)]
    pub compliance_ciphertext: Vec<u8>,
    #[serde(skip)]
    pub compliance_leaf: Option<penumbra_sdk_compliance::ComplianceLeaf>,
    #[serde(skip)]
    pub compliance_ephemeral_secret: Option<Fr>,
    #[serde(skip)]
    pub r_2: Option<Fr>,
    #[serde(skip)]
    pub r_3: Option<Fr>,
    #[serde(skip)]
    pub is_regulated: bool,
    #[serde(skip)]
    pub counterparty_address: Option<Address>,
    #[serde(skip)]
    pub counterparty_leaf: Option<penumbra_sdk_compliance::ComplianceLeaf>,
    #[serde(skip)]
    pub tx_blinding_nonce: Fr,
    #[serde(skip)]
    pub compliance_anchor: tct::StateCommitment,
    #[serde(skip)]
    pub asset_anchor: tct::StateCommitment,
    #[serde(skip)]
    pub asset_path: MerklePath,
    #[serde(skip)]
    pub asset_position: u64,
    #[serde(skip)]
    pub asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf,
    #[serde(skip)]
    pub compliance_position: u64,
    #[serde(skip)]
    pub ring_pk: decaf377::Element,
    #[serde(skip)]
    pub dk_pub: decaf377::Element,
    #[serde(skip)]
    pub threshold: u128,
    #[serde(skip)]
    pub is_flagged: bool,
    #[serde(skip)]
    pub salt: Fq,
    #[serde(skip)]
    pub dleq_k_1: Fr,
    #[serde(skip)]
    pub dleq_k_2: Option<Fr>,
    #[serde(skip)]
    pub dleq_k_3: Option<Fr>,
    #[serde(skip)]
    pub dleq_c_1: Fq,
    #[serde(skip)]
    pub dleq_s_1: Fq,
    #[serde(skip)]
    pub dleq_c_2: Fq,
    #[serde(skip)]
    pub dleq_s_2: Fq,
    #[serde(skip)]
    pub dleq_c_3: Fq,
    #[serde(skip)]
    pub dleq_s_3: Fq,
    #[serde(skip)]
    pub target_timestamp: u64,
}

impl ShieldedOutputPlan {
    /// Set compliance metadata fields needed by `build_transfer_compliance`.
    /// Per-action ciphertext/DLEQ are not computed here — transfer_body() builds
    /// transfer-level compliance and clears per-action fields.
    pub fn set_compliance_details(
        &mut self,
        _rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
        _recipient_leaf: &penumbra_sdk_compliance::ComplianceLeaf,
        sender_leaf: penumbra_sdk_compliance::ComplianceLeaf,
        tx_blinding_nonce: Fr,
    ) -> anyhow::Result<()> {
        let (ring_pk, dk_pub) = if self.is_regulated {
            (
                self.asset_indexed_leaf.ring.ring_pk,
                self.asset_indexed_leaf.params.dk_pub,
            )
        } else {
            (
                *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK,
                *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB,
            )
        };
        let threshold = self.asset_indexed_leaf.params.threshold;
        let note = self.output_note();
        let amount_u128: u128 = note.amount().into();
        let is_flagged = amount_u128 >= threshold;

        let recv_b_d_fq = self
            .dest_address
            .diversified_generator()
            .vartime_compress_to_field();
        let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
        let compliance_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            self.dest_address.clone(),
            note.asset_id(),
            recv_d,
        );

        self.compliance_leaf = Some(compliance_leaf);
        self.counterparty_address = Some(sender_leaf.address.clone());
        self.tx_blinding_nonce = tx_blinding_nonce;
        self.ring_pk = ring_pk;
        self.dk_pub = dk_pub;
        self.threshold = threshold;
        self.is_flagged = is_flagged;
        self.counterparty_leaf = Some(sender_leaf);

        Ok(())
    }

    pub fn new<R: RngCore + CryptoRng>(
        rng: &mut R,
        value: Value,
        dest_address: Address,
    ) -> ShieldedOutputPlan {
        let rseed = Rseed::generate(rng);
        let value_blinding = Fr::rand(rng);
        let ring_pk = *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK;
        let dk_pub = *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB;

        let b_d_fq = dest_address
            .diversified_generator()
            .vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let compliance_leaf =
            penumbra_sdk_compliance::ComplianceLeaf::new(dest_address.clone(), value.asset_id, d);

        let (compliance_anchor, compliance_path, compliance_position) =
            penumbra_sdk_compliance::default_user_proof(&compliance_leaf);
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            penumbra_sdk_compliance::create_default_imt_proof(value.asset_id.0);
        let tx_blinding_nonce = Fr::rand(rng);

        ShieldedOutputPlan {
            value,
            dest_address,
            rseed,
            value_blinding,
            proof_blinding_r: Fq::rand(rng),
            proof_blinding_s: Fq::rand(rng),
            compliance_path,
            compliance_ciphertext: Vec::new(),
            compliance_leaf: Some(compliance_leaf.clone()),
            compliance_ephemeral_secret: None,
            r_2: None,
            r_3: None,
            is_regulated: false,
            counterparty_address: None,
            counterparty_leaf: Some(compliance_leaf),
            tx_blinding_nonce,
            compliance_anchor,
            asset_anchor,
            asset_path,
            asset_position,
            asset_indexed_leaf,
            compliance_position,
            ring_pk,
            dk_pub,
            threshold: u128::MAX,
            is_flagged: false,
            salt: Fq::from(0u64),
            dleq_k_1: Fr::from(0u64),
            dleq_k_2: None,
            dleq_k_3: None,
            dleq_c_1: Fq::from(0u64),
            dleq_s_1: Fq::from(0u64),
            dleq_c_2: Fq::from(0u64),
            dleq_s_2: Fq::from(0u64),
            dleq_c_3: Fq::from(0u64),
            dleq_s_3: Fq::from(0u64),
            target_timestamp: 0u64,
        }
    }

    pub fn output_note(&self) -> Note {
        Note::from_parts(self.dest_address.clone(), self.value, self.rseed)
            .expect("transmission key in address is always valid")
    }

    pub fn action_output_parts(
        &self,
        ovk: &OutgoingViewingKey,
        memo_key: &PayloadKey,
    ) -> (crate::NotePayload, WrappedMemoKey, OvkWrappedKey) {
        let note = self.output_note();
        let esk: ka::Secret = note.ephemeral_secret_key();
        let ovk_wrapped_key = note.encrypt_key(ovk, self.balance().commit(self.value_blinding));
        let wrapped_memo_key = WrappedMemoKey::encrypt(
            memo_key,
            esk,
            note.transmission_key(),
            &note.diversified_generator(),
        );
        (note.payload(), wrapped_memo_key, ovk_wrapped_key)
    }

    pub fn is_viewed_by(&self, ivk: &IncomingViewingKey) -> bool {
        ivk.views_address(&self.dest_address)
    }

    pub fn balance(&self) -> Balance {
        -Balance::from(self.value)
    }
}

impl From<ShieldedInputPlan> for pb::ShieldedInputPlan {
    fn from(msg: ShieldedInputPlan) -> Self {
        use crate::compliance_helpers::{compliance_leaf_to_proto, indexed_leaf_to_proto};

        Self {
            note: Some(msg.note.into()),
            position: u64::from(msg.position),
            randomizer: msg.randomizer.to_bytes().to_vec(),
            value_blinding: msg.value_blinding.to_bytes().to_vec(),
            proof_blinding_r: msg.proof_blinding_r.to_bytes().to_vec(),
            proof_blinding_s: msg.proof_blinding_s.to_bytes().to_vec(),
            target_timestamp: msg.target_timestamp,
            compliance_ciphertext: msg.compliance_ciphertext,
            is_regulated: msg.is_regulated,
            compliance_leaf: msg
                .compliance_leaf
                .map(|leaf| compliance_leaf_to_proto(&leaf)),
            compliance_ephemeral_secret: msg
                .compliance_ephemeral_secret
                .map(|s| s.to_bytes().to_vec())
                .unwrap_or_default(),
            tx_blinding_nonce: msg.tx_blinding_nonce.to_bytes().to_vec(),
            compliance_anchor: Some(msg.compliance_anchor.into()),
            asset_anchor: Some(msg.asset_anchor.into()),
            compliance_path: Some(msg.compliance_path.into()),
            compliance_position: msg.compliance_position,
            asset_path: Some(msg.asset_path.into()),
            asset_position: msg.asset_position,
            asset_indexed_leaf: Some(indexed_leaf_to_proto(&msg.asset_indexed_leaf)),
            is_flagged: msg.is_flagged,
            salt: msg.salt.to_bytes().to_vec(),
            dleq_k: msg.dleq_k.to_bytes().to_vec(),
            dleq_c: msg.dleq_c.to_bytes().to_vec(),
            dleq_s: msg.dleq_s.to_bytes().to_vec(),
            ring_pk: msg.ring_pk.vartime_compress().0.to_vec(),
            dk_pub: msg.dk_pub.vartime_compress().0.to_vec(),
            threshold: msg.threshold.to_le_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::ShieldedInputPlan> for ShieldedInputPlan {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ShieldedInputPlan) -> Result<Self, Self::Error> {
        use crate::compliance_helpers::{
            compliance_leaf_from_proto, parse_ephemeral_secret, parse_indexed_leaf_or_default,
            parse_merkle_path_or_default, parse_state_commitment_or_default,
            parse_tx_blinding_nonce,
        };

        let compliance_leaf = msg
            .compliance_leaf
            .map(|leaf| compliance_leaf_from_proto(leaf, "compliance leaf"))
            .transpose()?;
        let compliance_ephemeral_secret = parse_ephemeral_secret(&msg.compliance_ephemeral_secret)?;
        let tx_blinding_nonce = parse_tx_blinding_nonce(&msg.tx_blinding_nonce)?;
        let compliance_anchor = parse_state_commitment_or_default(msg.compliance_anchor)?;
        let asset_anchor = parse_state_commitment_or_default(msg.asset_anchor)?;
        let compliance_path = parse_merkle_path_or_default(msg.compliance_path)?;
        let asset_path = parse_merkle_path_or_default(msg.asset_path)?;
        let asset_indexed_leaf = parse_indexed_leaf_or_default(msg.asset_indexed_leaf)?;

        Ok(Self {
            note: msg
                .note
                .ok_or_else(|| anyhow::anyhow!("missing note"))?
                .try_into()?,
            position: msg.position.into(),
            randomizer: Fr::from_bytes_checked(msg.randomizer.as_slice().try_into()?)
                .expect("randomizer malformed"),
            value_blinding: Fr::from_bytes_checked(msg.value_blinding.as_slice().try_into()?)
                .expect("value_blinding malformed"),
            proof_blinding_r: Fq::from_bytes_checked(msg.proof_blinding_r.as_slice().try_into()?)
                .expect("proof_blinding_r malformed"),
            proof_blinding_s: Fq::from_bytes_checked(msg.proof_blinding_s.as_slice().try_into()?)
                .expect("proof_blinding_s malformed"),
            compliance_path,
            compliance_ciphertext: msg.compliance_ciphertext,
            compliance_leaf,
            compliance_ephemeral_secret,
            is_regulated: msg.is_regulated,
            tx_blinding_nonce,
            compliance_anchor,
            asset_anchor,
            asset_path,
            asset_position: msg.asset_position,
            asset_indexed_leaf,
            compliance_position: msg.compliance_position,
            ring_pk: if msg.ring_pk.len() == 32 {
                decaf377::Encoding(msg.ring_pk.as_slice().try_into()?)
                    .vartime_decompress()
                    .unwrap_or(*penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK)
            } else {
                *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK
            },
            dk_pub: if msg.dk_pub.len() == 32 {
                decaf377::Encoding(msg.dk_pub.as_slice().try_into()?)
                    .vartime_decompress()
                    .unwrap_or(*penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB)
            } else {
                *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB
            },
            threshold: parse_threshold(&msg.threshold)?,
            is_flagged: msg.is_flagged,
            salt: parse_fq_or_zero("salt", &msg.salt)?,
            dleq_k: parse_fr_or_zero("dleq_k", &msg.dleq_k)?,
            dleq_c: parse_fq_or_zero("dleq_c", &msg.dleq_c)?,
            dleq_s: parse_fq_or_zero("dleq_s", &msg.dleq_s)?,
            target_timestamp: msg.target_timestamp,
        })
    }
}

impl From<ShieldedOutputPlan> for pb::ShieldedOutputPlan {
    fn from(msg: ShieldedOutputPlan) -> Self {
        use crate::compliance_helpers::{compliance_leaf_to_proto, indexed_leaf_to_proto};

        Self {
            value: Some(msg.value.into()),
            dest_address: Some(msg.dest_address.into()),
            rseed: msg.rseed.0.to_vec(),
            value_blinding: msg.value_blinding.to_bytes().to_vec(),
            proof_blinding_r: msg.proof_blinding_r.to_bytes().to_vec(),
            proof_blinding_s: msg.proof_blinding_s.to_bytes().to_vec(),
            target_timestamp: msg.target_timestamp,
            compliance_ciphertext: msg.compliance_ciphertext,
            is_regulated: msg.is_regulated,
            compliance_leaf: msg
                .compliance_leaf
                .map(|leaf| compliance_leaf_to_proto(&leaf)),
            counterparty_leaf: msg
                .counterparty_leaf
                .map(|leaf| compliance_leaf_to_proto(&leaf)),
            compliance_ephemeral_secret: msg
                .compliance_ephemeral_secret
                .map(|s| s.to_bytes().to_vec())
                .unwrap_or_default(),
            counterparty_address: msg.counterparty_address.map(Into::into),
            tx_blinding_nonce: msg.tx_blinding_nonce.to_bytes().to_vec(),
            compliance_anchor: Some(msg.compliance_anchor.into()),
            asset_anchor: Some(msg.asset_anchor.into()),
            compliance_path: Some(msg.compliance_path.into()),
            compliance_position: msg.compliance_position,
            asset_path: Some(msg.asset_path.into()),
            asset_position: msg.asset_position,
            asset_indexed_leaf: Some(indexed_leaf_to_proto(&msg.asset_indexed_leaf)),
            sender_ciphertext: Vec::new(),
            salt: msg.salt.to_bytes().to_vec(),
            dleq_k_1: msg.dleq_k_1.to_bytes().to_vec(),
            dleq_k_2: msg
                .dleq_k_2
                .map(|x| x.to_bytes().to_vec())
                .unwrap_or_default(),
            dleq_k_3: msg
                .dleq_k_3
                .map(|x| x.to_bytes().to_vec())
                .unwrap_or_default(),
            dleq_c_1: msg.dleq_c_1.to_bytes().to_vec(),
            dleq_s_1: msg.dleq_s_1.to_bytes().to_vec(),
            dleq_c_2: msg.dleq_c_2.to_bytes().to_vec(),
            dleq_s_2: msg.dleq_s_2.to_bytes().to_vec(),
            dleq_c_3: msg.dleq_c_3.to_bytes().to_vec(),
            dleq_s_3: msg.dleq_s_3.to_bytes().to_vec(),
            ring_pk: msg.ring_pk.vartime_compress().0.to_vec(),
            dk_pub: msg.dk_pub.vartime_compress().0.to_vec(),
            threshold_bytes: msg.threshold.to_le_bytes().to_vec(),
            is_flagged: msg.is_flagged,
            r_2: msg.r_2.map(|x| x.to_bytes().to_vec()).unwrap_or_default(),
            r_3: msg.r_3.map(|x| x.to_bytes().to_vec()).unwrap_or_default(),
        }
    }
}

impl TryFrom<pb::ShieldedOutputPlan> for ShieldedOutputPlan {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ShieldedOutputPlan) -> Result<Self, Self::Error> {
        use crate::compliance_helpers::{
            compliance_leaf_from_proto, parse_ephemeral_secret, parse_indexed_leaf_or_default,
            parse_merkle_path_or_default, parse_state_commitment_or_default,
            parse_tx_blinding_nonce,
        };

        let compliance_leaf = msg
            .compliance_leaf
            .map(|leaf| compliance_leaf_from_proto(leaf, "compliance leaf"))
            .transpose()?;
        let counterparty_leaf = msg
            .counterparty_leaf
            .map(|leaf| compliance_leaf_from_proto(leaf, "counterparty leaf"))
            .transpose()?;
        let compliance_ephemeral_secret = parse_ephemeral_secret(&msg.compliance_ephemeral_secret)?;
        let tx_blinding_nonce = parse_tx_blinding_nonce(&msg.tx_blinding_nonce)?;
        let compliance_anchor = parse_state_commitment_or_default(msg.compliance_anchor)?;
        let asset_anchor = parse_state_commitment_or_default(msg.asset_anchor)?;
        let compliance_path = parse_merkle_path_or_default(msg.compliance_path)?;
        let asset_path = parse_merkle_path_or_default(msg.asset_path)?;
        let asset_indexed_leaf = parse_indexed_leaf_or_default(msg.asset_indexed_leaf)?;

        Ok(Self {
            value: msg
                .value
                .ok_or_else(|| anyhow::anyhow!("missing value"))?
                .try_into()?,
            dest_address: msg
                .dest_address
                .ok_or_else(|| anyhow::anyhow!("missing dest_address"))?
                .try_into()?,
            rseed: Rseed(
                msg.rseed
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("rseed malformed"))?,
            ),
            value_blinding: Fr::from_bytes_checked(msg.value_blinding.as_slice().try_into()?)
                .expect("value_blinding malformed"),
            proof_blinding_r: Fq::from_bytes_checked(msg.proof_blinding_r.as_slice().try_into()?)
                .expect("proof_blinding_r malformed"),
            proof_blinding_s: Fq::from_bytes_checked(msg.proof_blinding_s.as_slice().try_into()?)
                .expect("proof_blinding_s malformed"),
            compliance_path,
            compliance_ciphertext: msg.compliance_ciphertext,
            compliance_leaf,
            compliance_ephemeral_secret,
            r_2: parse_optional_fr(&msg.r_2)?,
            r_3: parse_optional_fr(&msg.r_3)?,
            is_regulated: msg.is_regulated,
            counterparty_address: msg
                .counterparty_address
                .map(TryInto::try_into)
                .transpose()?,
            counterparty_leaf,
            tx_blinding_nonce,
            compliance_anchor,
            asset_anchor,
            asset_path,
            asset_position: msg.asset_position,
            asset_indexed_leaf,
            compliance_position: msg.compliance_position,
            ring_pk: if msg.ring_pk.len() == 32 {
                decaf377::Encoding(msg.ring_pk.as_slice().try_into()?)
                    .vartime_decompress()
                    .unwrap_or(*penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK)
            } else {
                *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK
            },
            dk_pub: if msg.dk_pub.len() == 32 {
                decaf377::Encoding(msg.dk_pub.as_slice().try_into()?)
                    .vartime_decompress()
                    .unwrap_or(*penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB)
            } else {
                *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB
            },
            threshold: parse_threshold(&msg.threshold_bytes)?,
            is_flagged: msg.is_flagged,
            salt: parse_fq_or_zero("salt", &msg.salt)?,
            dleq_k_1: parse_fr_or_zero("dleq_k_1", &msg.dleq_k_1)?,
            dleq_k_2: parse_optional_fr(&msg.dleq_k_2)?,
            dleq_k_3: parse_optional_fr(&msg.dleq_k_3)?,
            dleq_c_1: parse_fq_or_zero("dleq_c_1", &msg.dleq_c_1)?,
            dleq_s_1: parse_fq_or_zero("dleq_s_1", &msg.dleq_s_1)?,
            dleq_c_2: parse_fq_or_zero("dleq_c_2", &msg.dleq_c_2)?,
            dleq_s_2: parse_fq_or_zero("dleq_s_2", &msg.dleq_s_2)?,
            dleq_c_3: parse_fq_or_zero("dleq_c_3", &msg.dleq_c_3)?,
            dleq_s_3: parse_fq_or_zero("dleq_s_3", &msg.dleq_s_3)?,
            target_timestamp: msg.target_timestamp,
        })
    }
}

fn parse_threshold(bytes: &[u8]) -> anyhow::Result<u128> {
    if bytes.is_empty() {
        return Ok(u128::MAX);
    }
    let slice: [u8; 16] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("threshold malformed"))?;
    Ok(u128::from_le_bytes(slice))
}

fn parse_optional_fr(bytes: &[u8]) -> anyhow::Result<Option<Fr>> {
    if bytes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            Fr::from_bytes_checked(bytes.try_into()?).expect("Fr bytes malformed"),
        ))
    }
}

fn parse_fr_or_zero(label: &str, bytes: &[u8]) -> anyhow::Result<Fr> {
    if bytes.is_empty() {
        return Ok(Fr::from(0u64));
    }
    Fr::from_bytes_checked(bytes.try_into()?).map_err(|_| anyhow::anyhow!("{label} malformed"))
}

fn parse_fq_or_zero(label: &str, bytes: &[u8]) -> anyhow::Result<Fq> {
    if bytes.is_empty() {
        return Ok(Fq::from(0u64));
    }
    Fq::from_bytes_checked(bytes.try_into()?).map_err(|_| anyhow::anyhow!("{label} malformed"))
}
