use ark_groth16::ProvingKey;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_ka as ka;
use penumbra_sdk_asset::{Balance, Value, STAKING_TOKEN_ASSET_ID};
use penumbra_sdk_compliance::MerklePath;
use penumbra_sdk_keys::{
    keys::{IncomingViewingKey, OutgoingViewingKey},
    symmetric::WrappedMemoKey,
    Address, PayloadKey,
};
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};

use super::{Body, Output, OutputProof, OutputProofPrivate, OutputProofPublic};
use crate::{Note, Rseed};

/// A planned [`Output`](Output).
///
/// Compliance data is stored directly in the plan so it is available for proof generation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::OutputPlan", into = "pb::OutputPlan")]
pub struct OutputPlan {
    pub value: Value,
    pub dest_address: Address,
    pub rseed: Rseed,
    pub value_blinding: Fr,
    pub proof_blinding_r: Fq,
    pub proof_blinding_s: Fq,
    /// Compliance Merkle path for proving user is in the compliance registry
    pub compliance_path: MerklePath,
    /// Precomputed compliance ciphertext (or placeholder if not yet generated)
    #[serde(skip)]
    pub compliance_ciphertext: Vec<u8>,
    /// Compliance leaf for ZK proof
    #[serde(skip)]
    pub compliance_leaf: Option<penumbra_sdk_compliance::ComplianceLeaf>,
    /// Ephemeral secret r_1 used in compliance ciphertext encryption (core tier)
    #[serde(skip)]
    pub compliance_ephemeral_secret: Option<Fr>,
    /// Second ephemeral secret r_2 (ext tier)
    #[serde(skip)]
    pub r_2: Option<Fr>,
    /// Third ephemeral secret r_3 (sext tier)
    #[serde(skip)]
    pub r_3: Option<Fr>,
    /// Whether the asset is regulated (requires compliance)
    #[serde(skip)]
    pub is_regulated: bool,
    /// Counterparty address (the sender of this output)
    #[serde(skip)]
    pub counterparty_address: Option<Address>,
    /// Counterparty compliance leaf (the sender's leaf, for binding)
    #[serde(skip)]
    pub counterparty_leaf: Option<penumbra_sdk_compliance::ComplianceLeaf>,
    /// Shared transaction blinding nonce (same for spend and output in one transaction)
    #[serde(skip)]
    pub tx_blinding_nonce: Fr,
    /// Compliance anchor (user tree root) for proof generation
    #[serde(skip)]
    pub compliance_anchor: penumbra_sdk_tct::StateCommitment,
    /// Asset anchor (asset tree root) for proof generation
    #[serde(skip)]
    pub asset_anchor: penumbra_sdk_tct::StateCommitment,
    /// Asset Merkle path for proving asset is in the asset registry
    #[serde(skip)]
    pub asset_path: MerklePath,
    /// Position of the asset in the asset registry tree
    #[serde(skip)]
    pub asset_position: u64,
    /// The indexed leaf from the asset IMT (for membership/non-membership proofs)
    #[serde(skip)]
    pub asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf,
    /// Position of the user's compliance leaf in the compliance tree
    #[serde(skip)]
    pub compliance_position: u64,
    /// Ring public key (for ACK derivation in the circuit)
    #[serde(skip)]
    pub ring_pk: decaf377::Element,
    /// Issuer's detection key public (for threshold-based flagging)
    #[serde(skip)]
    pub dk_pub: decaf377::Element,
    /// Amount threshold for flagging (u128 to cover full amount range)
    #[serde(skip)]
    pub threshold: u128,
    /// Whether this output is flagged (amount >= threshold)
    #[serde(skip)]
    pub is_flagged: bool,
    /// Random salt for DLEQ metadata hash (encrypted in detection tier).
    #[serde(skip)]
    pub salt: Fq,
    /// DLEQ nonce k_1 (core tier).
    #[serde(skip)]
    pub dleq_k_1: Fr,
    /// DLEQ nonce k_2 (ext tier).
    #[serde(skip)]
    pub dleq_k_2: Option<Fr>,
    /// DLEQ nonce k_3 (sext tier).
    #[serde(skip)]
    pub dleq_k_3: Option<Fr>,
    /// DLEQ challenge/response for core tier (stored as Fq but canonical Fr).
    #[serde(skip)]
    pub dleq_c_1: Fq,
    #[serde(skip)]
    pub dleq_s_1: Fq,
    /// DLEQ challenge/response for ext tier.
    #[serde(skip)]
    pub dleq_c_2: Fq,
    #[serde(skip)]
    pub dleq_s_2: Fq,
    /// DLEQ challenge/response for sext tier.
    #[serde(skip)]
    pub dleq_c_3: Fq,
    #[serde(skip)]
    pub dleq_s_3: Fq,
    /// Target timestamp for DLEQ metadata binding (Unix UTC seconds).
    #[serde(skip)]
    pub target_timestamp: u64,
}

impl OutputPlan {
    /// Set compliance details for this output plan.
    ///
    /// Extracts dk_pub, ring_pk, and threshold from `self.asset_indexed_leaf`.
    /// Must be called after `asset_indexed_leaf` has been set.
    pub fn set_compliance_details(
        &mut self,
        rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
        _recipient_leaf: &penumbra_sdk_compliance::ComplianceLeaf,
        sender_leaf: penumbra_sdk_compliance::ComplianceLeaf,
        tx_blinding_nonce: Fr,
    ) -> anyhow::Result<()> {
        // For unregulated assets, encrypt to BLACK_HOLE_ACK (NUMS point with unknown
        // discrete log) so ciphertext is undecryptable. The low leaf from the IMT
        // non-membership proof stays intact for the circuit proof.
        let (ring_pk, dk_pub) = if self.is_regulated {
            (
                self.asset_indexed_leaf.ring.ring_pk,
                self.asset_indexed_leaf.params.dk_pub,
            )
        } else {
            (
                *penumbra_sdk_compliance::BLACK_HOLE_ACK,
                decaf377::Element::GENERATOR,
            )
        };
        let threshold = self.asset_indexed_leaf.params.threshold;

        let note = self.output_note();
        let amount_u128: u128 = note.amount().into();
        let is_flagged = amount_u128 >= threshold;

        let sender_address = sender_leaf.address.clone();

        let compliance_data = crate::generate_compliance_details_output(
            rng,
            &ring_pk,
            &dk_pub,
            &self.dest_address,
            &sender_address,
            note.asset_id(),
            note.amount(),
            is_flagged,
        )?;

        self.compliance_ciphertext = compliance_data.ciphertext;
        self.compliance_leaf = Some(compliance_data.leaf);
        self.compliance_ephemeral_secret = Some(compliance_data.ephemeral_secret);
        self.r_2 = compliance_data.r_2;
        self.r_3 = compliance_data.r_3;
        self.salt = compliance_data.salt;
        self.dleq_k_1 = compliance_data.dleq_k;
        self.dleq_k_2 = compliance_data.dleq_k_2;
        self.dleq_k_3 = compliance_data.dleq_k_3;
        self.counterparty_address = Some(sender_address.clone());
        self.tx_blinding_nonce = tx_blinding_nonce;
        self.ring_pk = ring_pk;
        self.dk_pub = dk_pub;
        self.threshold = threshold;
        self.is_flagged = is_flagged;
        self.counterparty_leaf = Some(sender_leaf);

        // Compute DLEQ proofs using policy fields from asset_indexed_leaf
        let metadata_hash_core = penumbra_sdk_compliance::compute_metadata_hash(
            self.asset_indexed_leaf.ring.policy_id_hash,
            self.asset_indexed_leaf.ring.resource_hash,
            self.asset_indexed_leaf.ring.permission_hash,
            Fq::from(1u64), // tier=1 (core)
            Fq::from(self.target_timestamp),
            self.salt,
        );
        let metadata_hash_ext = penumbra_sdk_compliance::compute_metadata_hash(
            self.asset_indexed_leaf.ring.policy_id_hash,
            self.asset_indexed_leaf.ring.resource_hash,
            self.asset_indexed_leaf.ring.permission_hash,
            Fq::from(2u64), // tier=2 (ext)
            Fq::from(self.target_timestamp),
            self.salt,
        );
        let metadata_hash_sext = penumbra_sdk_compliance::compute_metadata_hash(
            self.asset_indexed_leaf.ring.policy_id_hash,
            self.asset_indexed_leaf.ring.resource_hash,
            self.asset_indexed_leaf.ring.permission_hash,
            Fq::from(3u64), // tier=3 (sext)
            Fq::from(self.target_timestamp),
            self.salt,
        );

        // Receiver ACK for core/ext tiers
        let recv_b_d_fq = self
            .dest_address
            .diversified_generator()
            .vartime_compress_to_field();
        let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
        let recv_d_fr = Fr::from_le_bytes_mod_order(&recv_d.to_bytes());
        let ack_receiver = ring_pk * recv_d_fr;

        // Sender ACK for sext tier (counterparty disclosure)
        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_d_fr = Fr::from_le_bytes_mod_order(&sender_d.to_bytes());
        let ack_sender = ring_pk * sender_d_fr;

        let r_1 = self.compliance_ephemeral_secret.unwrap();
        let r_2 = self.r_2.unwrap();
        let r_3 = self.r_3.unwrap();

        let epk_1 = decaf377::Element::GENERATOR * r_1;
        let epk_2 = decaf377::Element::GENERATOR * r_2;
        let epk_3 = decaf377::Element::GENERATOR * r_3;

        let dleq_1 = penumbra_sdk_compliance::compute_dleq_native(
            r_1,
            self.dleq_k_1,
            &ack_receiver,
            &epk_1,
            metadata_hash_core,
        );
        let dleq_2 = penumbra_sdk_compliance::compute_dleq_native(
            r_2,
            self.dleq_k_2.unwrap(),
            &ack_receiver,
            &epk_2,
            metadata_hash_ext,
        );
        let dleq_3 = penumbra_sdk_compliance::compute_dleq_native(
            r_3,
            self.dleq_k_3.unwrap(),
            &ack_sender,
            &epk_3,
            metadata_hash_sext,
        );

        self.dleq_c_1 = dleq_1.c;
        self.dleq_s_1 = Fq::from_le_bytes_mod_order(&dleq_1.s.to_bytes());
        self.dleq_c_2 = dleq_2.c;
        self.dleq_s_2 = Fq::from_le_bytes_mod_order(&dleq_2.s.to_bytes());
        self.dleq_c_3 = dleq_3.c;
        self.dleq_s_3 = Fq::from_le_bytes_mod_order(&dleq_3.s.to_bytes());

        Ok(())
    }

    /// Create a new [`OutputPlan`] that sends `value` to `dest_address`.
    pub fn new<R: RngCore + CryptoRng>(
        rng: &mut R,
        value: Value,
        dest_address: Address,
    ) -> OutputPlan {
        let rseed = Rseed::generate(rng);
        let value_blinding = Fr::rand(rng);

        let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
        let dk_pub = decaf377::Element::GENERATOR;

        let b_d_fq = dest_address
            .diversified_generator()
            .vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack = ring_pk * d_fr;

        let encryption_result = penumbra_sdk_compliance::crypto::encrypt_output(
            &mut *rng,
            &ack,
            &ack,
            &dk_pub,
            &dest_address,
            &dest_address,
            value.asset_id,
            value.amount,
            false,
            Fq::from(0u64),
        )
        .expect("can encrypt compliance details");

        let compliance_ciphertext = encryption_result.ciphertext.to_bytes();

        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let compliance_leaf =
            penumbra_sdk_compliance::ComplianceLeaf::new(dest_address.clone(), value.asset_id, d);

        let (compliance_anchor, compliance_path, compliance_position) =
            penumbra_sdk_compliance::default_user_proof(&compliance_leaf);

        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            penumbra_sdk_compliance::create_default_imt_proof(value.asset_id.0);

        let tx_blinding_nonce = Fr::rand(rng);

        Self {
            value,
            dest_address,
            rseed,
            value_blinding,
            proof_blinding_r: Fq::rand(rng),
            proof_blinding_s: Fq::rand(rng),
            compliance_path,
            compliance_ciphertext,
            compliance_leaf: Some(compliance_leaf.clone()),
            compliance_ephemeral_secret: Some(encryption_result.r_1),
            r_2: Some(encryption_result.r_2),
            r_3: Some(encryption_result.r_3),
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

    /// Create a dummy [`OutputPlan`].
    pub fn dummy<R: CryptoRng + RngCore>(rng: &mut R) -> OutputPlan {
        let dummy_address = Address::dummy(rng);
        Self::new(
            rng,
            Value {
                amount: 0u64.into(),
                asset_id: *STAKING_TOKEN_ASSET_ID,
            },
            dummy_address,
        )
    }

    /// Convenience method to construct the [`Output`] described by this plan.
    pub fn output(
        &self,
        ovk: &OutgoingViewingKey,
        memo_key: &PayloadKey,
        pk: &ProvingKey<Bls12_377>,
        compliance_keys: Option<(decaf377::Element, decaf377::Element)>,
    ) -> Result<Output, crate::ProofError> {
        Ok(Output {
            body: self.output_body(ovk, memo_key, compliance_keys),
            proof: self.output_proof(pk, compliance_keys)?,
        })
    }

    pub fn output_note(&self) -> Note {
        Note::from_parts(self.dest_address.clone(), self.value, self.rseed)
            .expect("transmission key in address is always valid")
    }

    /// Construct the [`OutputProof`].
    pub fn output_proof(
        &self,
        pk: &ProvingKey<Bls12_377>,
        _compliance_keys: Option<(decaf377::Element, decaf377::Element)>,
    ) -> Result<OutputProof, crate::ProofError> {
        let note = self.output_note();
        let balance_commitment = self.balance().commit(self.value_blinding);
        let note_commitment = note.commit();

        let asset_anchor = self.asset_anchor;
        let compliance_anchor = self.compliance_anchor;

        let user_leaf = self.compliance_leaf.clone().unwrap_or_else(|| {
            let b_d_fq = note
                .address()
                .diversified_generator()
                .vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            penumbra_sdk_compliance::ComplianceLeaf::new(note.address().clone(), note.asset_id(), d)
        });

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct_obj = ComplianceCiphertext::from_bytes(&self.compliance_ciphertext)
            .expect("can deserialize ciphertext");
        let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
            ct_obj.to_output_circuit_public_inputs();

        let counterparty_leaf_hash = self
            .counterparty_leaf
            .clone()
            .unwrap_or_else(|| {
                let b_d_fq = note
                    .address()
                    .diversified_generator()
                    .vartime_compress_to_field();
                let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
                penumbra_sdk_compliance::ComplianceLeaf::new(
                    note.address().clone(),
                    note.asset_id(),
                    d,
                )
            })
            .commit();

        let blinded_counterparty_leaf = penumbra_sdk_compliance::blind_sender_leaf(
            counterparty_leaf_hash,
            self.tx_blinding_nonce,
        );

        OutputProof::prove(
            self.proof_blinding_r,
            self.proof_blinding_s,
            pk,
            OutputProofPublic {
                balance_commitment,
                note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext,
                target_timestamp: Fq::from(self.target_timestamp),
                dleq_c_1: self.dleq_c_1,
                dleq_s_1: self.dleq_s_1,
                dleq_c_2: self.dleq_c_2,
                dleq_s_2: self.dleq_s_2,
                dleq_c_3: self.dleq_c_3,
                dleq_s_3: self.dleq_s_3,
                asset_anchor,
                compliance_anchor,
                counterparty_leaf_hash: blinded_counterparty_leaf,
            },
            OutputProofPrivate {
                note: note.clone(),
                balance_blinding: self.value_blinding,
                asset_path: self.asset_path.clone(),
                asset_position: self.asset_position,
                asset_indexed_leaf: self.asset_indexed_leaf.clone(),
                is_regulated: self.is_regulated,
                compliance_path: self.compliance_path.clone(),
                compliance_position: self.compliance_position,
                user_leaf,
                compliance_ephemeral_secret: self
                    .compliance_ephemeral_secret
                    .unwrap_or(Fr::from(0u64)),
                r_2: self.r_2.unwrap_or(Fr::from(0u64)),
                r_3: self.r_3.unwrap_or(Fr::from(0u64)),
                counterparty_leaf: self.counterparty_leaf.clone().unwrap_or_else(|| {
                    let b_d_fq = note
                        .address()
                        .diversified_generator()
                        .vartime_compress_to_field();
                    let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
                    penumbra_sdk_compliance::ComplianceLeaf::new(
                        note.address().clone(),
                        note.asset_id(),
                        d,
                    )
                }),
                tx_blinding_nonce: self.tx_blinding_nonce,
                is_flagged: self.is_flagged,
                salt: self.salt,
            },
        )
    }

    pub fn output_body(
        &self,
        ovk: &OutgoingViewingKey,
        memo_key: &PayloadKey,
        _compliance_keys: Option<(decaf377::Element, decaf377::Element)>,
    ) -> Body {
        let note = self.output_note();
        let balance_commitment = self.balance().commit(self.value_blinding);

        let esk: ka::Secret = note.ephemeral_secret_key();
        let ovk_wrapped_key = note.encrypt_key(ovk, balance_commitment);

        let wrapped_memo_key = WrappedMemoKey::encrypt(
            memo_key,
            esk,
            note.transmission_key(),
            &note.diversified_generator(),
        );

        let counterparty_leaf_hash = self
            .counterparty_leaf
            .clone()
            .unwrap_or_else(|| {
                let b_d_fq = note
                    .address()
                    .diversified_generator()
                    .vartime_compress_to_field();
                let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
                penumbra_sdk_compliance::ComplianceLeaf::new(
                    note.address().clone(),
                    note.asset_id(),
                    d,
                )
            })
            .commit();

        let blinded_counterparty_leaf = penumbra_sdk_compliance::blind_sender_leaf(
            counterparty_leaf_hash,
            self.tx_blinding_nonce,
        );

        // Serialize DLEQ proofs: (c_1, s_1, c_2, s_2, c_3, s_3) × 32 bytes = 192 bytes
        let mut dleq_proofs = Vec::with_capacity(192);
        dleq_proofs.extend_from_slice(&self.dleq_c_1.to_bytes());
        dleq_proofs.extend_from_slice(&self.dleq_s_1.to_bytes());
        dleq_proofs.extend_from_slice(&self.dleq_c_2.to_bytes());
        dleq_proofs.extend_from_slice(&self.dleq_s_2.to_bytes());
        dleq_proofs.extend_from_slice(&self.dleq_c_3.to_bytes());
        dleq_proofs.extend_from_slice(&self.dleq_s_3.to_bytes());

        Body {
            note_payload: note.payload(),
            balance_commitment,
            ovk_wrapped_key,
            wrapped_memo_key,
            compliance_ciphertext: self.compliance_ciphertext.clone(),
            target_timestamp: self.target_timestamp,
            counterparty_leaf_hash: blinded_counterparty_leaf,
            compliance_anchor: self.compliance_anchor,
            asset_anchor: self.asset_anchor,
            dleq_proofs,
        }
    }

    pub fn is_viewed_by(&self, ivk: &IncomingViewingKey) -> bool {
        ivk.views_address(&self.dest_address)
    }

    pub fn balance(&self) -> Balance {
        -Balance::from(self.value)
    }
}

impl DomainType for OutputPlan {
    type Proto = pb::OutputPlan;
}

impl From<OutputPlan> for pb::OutputPlan {
    fn from(msg: OutputPlan) -> Self {
        use crate::compliance_helpers::{compliance_leaf_to_proto, indexed_leaf_to_proto};

        Self {
            value: Some(msg.value.into()),
            dest_address: Some(msg.dest_address.into()),
            rseed: msg.rseed.to_bytes().to_vec(),
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
                .map(|k| k.to_bytes().to_vec())
                .unwrap_or_default(),
            dleq_k_3: msg
                .dleq_k_3
                .map(|k| k.to_bytes().to_vec())
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
            r_2: msg.r_2.map(|r| r.to_bytes().to_vec()).unwrap_or_default(),
            r_3: msg.r_3.map(|r| r.to_bytes().to_vec()).unwrap_or_default(),
        }
    }
}

impl TryFrom<pb::OutputPlan> for OutputPlan {
    type Error = anyhow::Error;
    fn try_from(msg: pb::OutputPlan) -> Result<Self, Self::Error> {
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
                .ok_or_else(|| anyhow::anyhow!("missing address"))?
                .try_into()?,
            rseed: Rseed(msg.rseed.as_slice().try_into()?),
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
            r_2: if msg.r_2.len() == 32 {
                Some(
                    Fr::from_bytes_checked(msg.r_2.as_slice().try_into()?)
                        .unwrap_or(Fr::from(0u64)),
                )
            } else {
                None
            },
            r_3: if msg.r_3.len() == 32 {
                Some(
                    Fr::from_bytes_checked(msg.r_3.as_slice().try_into()?)
                        .unwrap_or(Fr::from(0u64)),
                )
            } else {
                None
            },
            is_regulated: msg.is_regulated,
            counterparty_address: msg.counterparty_address.map(|a| a.try_into()).transpose()?,
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
                    .unwrap_or(*penumbra_sdk_compliance::BLACK_HOLE_ACK)
            } else {
                *penumbra_sdk_compliance::BLACK_HOLE_ACK
            },
            dk_pub: if msg.dk_pub.len() == 32 {
                decaf377::Encoding(msg.dk_pub.as_slice().try_into()?)
                    .vartime_decompress()
                    .unwrap_or(decaf377::Element::GENERATOR)
            } else {
                decaf377::Element::GENERATOR
            },
            threshold: if msg.threshold_bytes.len() == 16 {
                u128::from_le_bytes(msg.threshold_bytes.as_slice().try_into()?)
            } else {
                u128::MAX
            },
            is_flagged: msg.is_flagged,
            salt: if msg.salt.len() == 32 {
                Fq::from_bytes_checked(msg.salt.as_slice().try_into()?).unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_k_1: if msg.dleq_k_1.len() == 32 {
                Fr::from_bytes_checked(msg.dleq_k_1.as_slice().try_into()?)
                    .unwrap_or(Fr::from(0u64))
            } else {
                Fr::from(0u64)
            },
            dleq_k_2: if msg.dleq_k_2.len() == 32 {
                Some(
                    Fr::from_bytes_checked(msg.dleq_k_2.as_slice().try_into()?)
                        .unwrap_or(Fr::from(0u64)),
                )
            } else {
                None
            },
            dleq_k_3: if msg.dleq_k_3.len() == 32 {
                Some(
                    Fr::from_bytes_checked(msg.dleq_k_3.as_slice().try_into()?)
                        .unwrap_or(Fr::from(0u64)),
                )
            } else {
                None
            },
            dleq_c_1: if msg.dleq_c_1.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_c_1.as_slice().try_into()?)
                    .unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_s_1: if msg.dleq_s_1.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_s_1.as_slice().try_into()?)
                    .unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_c_2: if msg.dleq_c_2.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_c_2.as_slice().try_into()?)
                    .unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_s_2: if msg.dleq_s_2.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_s_2.as_slice().try_into()?)
                    .unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_c_3: if msg.dleq_c_3.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_c_3.as_slice().try_into()?)
                    .unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_s_3: if msg.dleq_s_3.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_s_3.as_slice().try_into()?)
                    .unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            target_timestamp: msg.target_timestamp,
        })
    }
}

#[cfg(test)]
mod test {
    use super::OutputPlan;
    use crate::output::proof::OutputCircuit;
    use crate::output::OutputProofPublic;

    use crate::test_proof_helpers::proof_test_helpers::*;
    use decaf377::{Fq, Fr};
    use penumbra_sdk_keys::PayloadKey;
    use rand_core::OsRng;

    /// Helper to run the full verification flow for a specific asset ID.
    fn verify_output_proof_with_asset(asset_id_u64: u64) {
        use crate::test_proof_helpers::proof_test_helpers::{
            create_imt_membership_proof, create_imt_non_membership_proof, create_user_tree_proof,
            generate_test_data, CircuitType,
        };

        let mut rng = OsRng;

        // 1. Generate unified test data
        let is_regulated = asset_id_u64 == REGULATED_ASSET_ID;
        let test_data = generate_test_data(
            &mut rng,
            asset_id_u64,
            100,
            is_regulated,
            CircuitType::Output,
        );

        // 2. Setup circuit keys
        let (pk, pvk, _blinding_r, _blinding_s) = setup_groth16_keys::<OutputCircuit>();

        let ovk = test_data.sk.full_viewing_key().outgoing();
        let dummy_memo_key: PayloadKey = [0; 32].into();

        // 3. Create valid IMT proof FIRST (encryption needs the indexed leaf)
        let asset_id_fq = Fq::from(asset_id_u64);
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) = if is_regulated {
            create_imt_membership_proof(asset_id_fq, test_data.ring_pk, test_data.dk_pub)
        } else {
            create_imt_non_membership_proof(asset_id_fq)
        };

        // 4. Create OutputPlan and set IMT proof data BEFORE encryption
        let mut output_plan = OutputPlan::new(&mut rng, test_data.value, test_data.address.clone());
        let blinding_factor = output_plan.value_blinding;

        // Set the IMT proof data BEFORE calling set_compliance_details
        output_plan.asset_anchor = asset_anchor;
        output_plan.asset_path = asset_path;
        output_plan.asset_position = asset_position;
        output_plan.asset_indexed_leaf = asset_indexed_leaf;
        output_plan.is_regulated = is_regulated;

        let tx_blinding_nonce = Fr::rand(&mut rng);

        // Derive proper d scalars for sender and receiver
        let recv_b_d_fq = output_plan
            .dest_address
            .diversified_generator()
            .vartime_compress_to_field();
        let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
        let recipient_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            output_plan.dest_address.clone(),
            output_plan.value.asset_id,
            recv_d,
        );

        let sender_b_d_fq = test_data
            .sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            test_data.sender_address.clone(),
            output_plan.value.asset_id,
            sender_d,
        );
        output_plan
            .set_compliance_details(&mut rng, &recipient_leaf, sender_leaf, tx_blinding_nonce)
            .expect("can set compliance details");

        // Create valid user tree proof
        let user_leaf = output_plan.compliance_leaf.clone().unwrap();
        let (compliance_anchor, compliance_path, compliance_position) =
            create_user_tree_proof(&user_leaf);

        // Set the user tree proof data
        output_plan.compliance_anchor = compliance_anchor;
        output_plan.compliance_path = compliance_path;
        output_plan.compliance_position = compliance_position;

        let _body = output_plan.output_body(ovk, &dummy_memo_key, None);

        let balance_commitment = output_plan.balance().commit(blinding_factor);
        let note_commitment = output_plan.output_note().commit();

        // 4. Generate Proof
        let output_proof = output_plan
            .output_proof(&pk, None)
            .expect("proof generation should succeed");

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct = ComplianceCiphertext::from_bytes(&output_plan.compliance_ciphertext)
            .expect("can deserialize ciphertext");
        let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, packed_ciphertext) =
            ct.to_output_circuit_public_inputs();

        let counterparty_leaf_hash = output_plan.counterparty_leaf.clone().unwrap().commit();

        let blinded_counterparty = penumbra_sdk_compliance::blind_sender_leaf(
            counterparty_leaf_hash,
            output_plan.tx_blinding_nonce,
        );

        output_proof
            .verify(
                &pvk,
                OutputProofPublic {
                    balance_commitment,
                    note_commitment,
                    epk_1,
                    epk_2,
                    epk_3,
                    c2_core,
                    c2_ext,
                    c2_sext,
                    compliance_ciphertext: packed_ciphertext,
                    asset_anchor,
                    compliance_anchor,
                    target_timestamp: Fq::from(output_plan.target_timestamp),
                    dleq_c_1: output_plan.dleq_c_1,
                    dleq_s_1: output_plan.dleq_s_1,
                    dleq_c_2: output_plan.dleq_c_2,
                    dleq_s_2: output_plan.dleq_s_2,
                    dleq_c_3: output_plan.dleq_c_3,
                    dleq_s_3: output_plan.dleq_s_3,
                    counterparty_leaf_hash: blinded_counterparty,
                },
            )
            .unwrap();
    }

    #[test]
    fn test_regulated_asset_output_proof() {
        verify_output_proof_with_asset(REGULATED_ASSET_ID);
    }

    #[test]
    fn test_unregulated_asset_output_proof() {
        verify_output_proof_with_asset(UNREGULATED_ASSET_ID);
    }

    /// Test that output is flagged when amount >= threshold (with distinct sender/receiver)
    #[test]
    fn test_output_flagged_above_threshold() {
        let mut rng = OsRng;
        let threshold = 500u128;

        // Receiver
        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let ivk = sk.full_viewing_key().incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        // Distinct sender
        let sender_seed = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sender_sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            sender_seed,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let sender_ivk = sender_sk.full_viewing_key().incoming();
        let (sender_address, _) = sender_ivk.payment_address(0u32.into());

        // amount=1000 >= threshold=500 => flagged
        let value = penumbra_sdk_asset::Value {
            amount: 1000u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(1u64)),
        };

        let mut output_plan = OutputPlan::new(&mut rng, value, address.clone());
        output_plan.asset_indexed_leaf.params.threshold = threshold;

        let recv_b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
        let recipient_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            address.clone(),
            output_plan.value.asset_id,
            recv_d,
        );

        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            sender_address,
            output_plan.value.asset_id,
            sender_d,
        );

        output_plan
            .set_compliance_details(&mut rng, &recipient_leaf, sender_leaf, Fr::from(0u64))
            .expect("can set compliance details");

        assert!(
            output_plan.is_flagged,
            "amount 1000 >= threshold 500 should be flagged"
        );
    }

    /// Test that output is NOT flagged when amount < threshold (with distinct sender/receiver)
    #[test]
    fn test_output_not_flagged_below_threshold() {
        let mut rng = OsRng;
        let threshold = 500u128;

        // Receiver
        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let ivk = sk.full_viewing_key().incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        // Distinct sender
        let sender_seed = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sender_sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            sender_seed,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let sender_ivk = sender_sk.full_viewing_key().incoming();
        let (sender_address, _) = sender_ivk.payment_address(0u32.into());

        // amount=100 < threshold=500 => NOT flagged
        let value = penumbra_sdk_asset::Value {
            amount: 100u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(1u64)),
        };

        let mut output_plan = OutputPlan::new(&mut rng, value, address.clone());
        output_plan.asset_indexed_leaf.params.threshold = threshold;

        let recv_b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
        let recipient_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            address.clone(),
            output_plan.value.asset_id,
            recv_d,
        );

        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            sender_address,
            output_plan.value.asset_id,
            sender_d,
        );

        output_plan
            .set_compliance_details(&mut rng, &recipient_leaf, sender_leaf, Fr::from(0u64))
            .expect("can set compliance details");

        assert!(
            !output_plan.is_flagged,
            "amount 100 < threshold 500 should NOT be flagged"
        );
    }

    /// Unregulated assets must use BLACK_HOLE_ACK for encryption even when
    /// the IMT low leaf carries a regulated asset's policy keys.
    #[test]
    fn test_unregulated_overrides_low_leaf_keys_to_black_hole() {
        let mut rng = OsRng;

        // Receiver
        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let ivk = sk.full_viewing_key().incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        // Distinct sender
        let sender_seed = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sender_sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            sender_seed,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let sender_ivk = sender_sk.full_viewing_key().incoming();
        let (sender_address, _) = sender_ivk.payment_address(0u32.into());

        let value = penumbra_sdk_asset::Value {
            amount: 100u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(999u64)),
        };

        let mut output_plan = OutputPlan::new(&mut rng, value, address.clone());

        // Simulate a low leaf from a regulated asset's non-membership proof
        let fake_ring_sk = Fr::rand(&mut rng);
        let fake_ring_pk = decaf377::Element::GENERATOR * fake_ring_sk;
        output_plan.asset_indexed_leaf.ring.ring_pk = fake_ring_pk;
        output_plan.asset_indexed_leaf.params.dk_pub = decaf377::Element::GENERATOR;
        output_plan.is_regulated = false;

        let recv_b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
        let recipient_leaf =
            penumbra_sdk_compliance::ComplianceLeaf::new(address.clone(), value.asset_id, recv_d);

        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_leaf =
            penumbra_sdk_compliance::ComplianceLeaf::new(sender_address, value.asset_id, sender_d);

        output_plan
            .set_compliance_details(&mut rng, &recipient_leaf, sender_leaf, Fr::from(0u64))
            .expect("can set compliance details");

        assert_eq!(
            output_plan.ring_pk,
            *penumbra_sdk_compliance::BLACK_HOLE_ACK,
            "Unregulated output must override low leaf ring_pk to BLACK_HOLE_ACK"
        );
        assert_eq!(
            output_plan.dk_pub,
            decaf377::Element::GENERATOR,
            "Unregulated output must use GENERATOR as dk_pub"
        );
    }
}
