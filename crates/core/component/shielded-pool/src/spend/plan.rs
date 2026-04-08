use decaf377::{Fq, Fr};
#[cfg(any(unix, windows))]
use decaf377_rdsa::Signature;
use decaf377_rdsa::SpendAuth;
use penumbra_sdk_asset::{Balance, Value, STAKING_TOKEN_ASSET_ID};
use penumbra_sdk_compliance::MerklePath;
use penumbra_sdk_keys::{keys::AddressIndex, FullViewingKey};
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct as tct;
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};

use super::Body;
#[cfg(any(unix, windows))]
use super::{Spend, SpendProof};
use crate::{Backref, Note, Rseed};
#[cfg(any(unix, windows))]
use crate::{SpendProofPrivate, SpendProofPublic};

/// A planned [`Spend`](Spend).
///
/// # Compliance Data Architecture Decision
///
/// Compliance-related fields (paths, positions, anchors) are stored directly in the plan
/// rather than in a separate WitnessData-like structure. This differs from how SCT Merkle
/// proofs are handled (via WitnessData at build time), but is intentional because:
///
/// 1. **Compliance data is not original Penumbra transaction data** - we have freedom to
///    design it however makes sense for our use case.
///
/// 2. **Plans should be self-contained and portable** - a serialized plan should be buildable
///    without additional chain queries (important for hardware wallets, offline signing).
///
/// 3. **Consistency** - we already have `compliance_path`, `compliance_anchor`, `asset_anchor`
///    in the plan; adding `asset_path`, `asset_position`, `compliance_position` is consistent.
///
/// 4. **Staleness is manageable** - the compliance registry changes infrequently (user/asset
///    registrations are stable), so stale data causing failures is unlikely.
///
/// 5. **Simpler code path** - no need to pass additional data structures at build time.
///
/// If staleness becomes problematic in the future, we can add historical compliance anchor
/// validation on chain (like SCT anchors) without changing client architecture.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::SpendPlan", into = "pb::SpendPlan")]
pub struct SpendPlan {
    pub note: Note,
    pub position: tct::Position,
    pub randomizer: Fr,
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
    /// Ephemeral secret used in compliance ciphertext encryption (needed by circuit)
    #[serde(skip)]
    pub compliance_ephemeral_secret: Option<Fr>,
    /// Whether the asset is regulated (requires compliance)
    #[serde(skip)]
    pub is_regulated: bool,
    /// Shared transaction blinding nonce (same for spend and output in one transaction)
    #[serde(skip)]
    pub tx_blinding_nonce: Fr,
    /// Compliance anchor (user tree root) for proof generation
    #[serde(skip)]
    pub compliance_anchor: tct::StateCommitment,
    /// Asset anchor (asset tree root) for proof generation
    #[serde(skip)]
    pub asset_anchor: tct::StateCommitment,
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
    /// Issuer's detection key.
    #[serde(skip)]
    pub dk_pub: decaf377::Element,
    /// Amount threshold for flagging.
    #[serde(skip)]
    pub threshold: u128,
    /// Ring public key for ACK derivation.
    #[serde(skip)]
    pub ring_pk: decaf377::Element,
    /// Whether this spend is flagged (amount >= threshold).
    #[serde(skip)]
    pub is_flagged: bool,
    /// Random salt for DLEQ metadata hash (encrypted in detection tier).
    #[serde(skip)]
    pub salt: Fq,
    /// DLEQ nonce k (used to compute DLEQ proof natively).
    #[serde(skip)]
    pub dleq_k: Fr,
    /// DLEQ challenge c (253-bit truncated Poseidon output, canonical Fr stored as Fq).
    #[serde(skip)]
    pub dleq_c: Fq,
    /// DLEQ response s (canonical Fr stored as Fq).
    #[serde(skip)]
    pub dleq_s: Fq,
    /// Target timestamp for DLEQ metadata binding (Unix UTC seconds).
    #[serde(skip)]
    pub target_timestamp: u64,
}

impl SpendPlan {
    /// Set compliance details for this spend plan.
    ///
    /// Extracts dk_pub, ring_pk, and threshold from `self.asset_indexed_leaf`.
    /// Must be called after `asset_indexed_leaf` has been set.
    pub fn set_compliance_details(
        &mut self,
        rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
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

        let amount_u128: u128 = self.note.amount().into();
        let is_flagged = amount_u128 >= threshold;

        let compliance_data = crate::generate_compliance_details_spend(
            rng,
            &ring_pk,
            &dk_pub,
            &self.note.address(),
            self.note.asset_id(),
            self.note.amount(),
            is_flagged,
        )?;

        self.compliance_ciphertext = compliance_data.ciphertext;
        self.compliance_leaf = Some(compliance_data.leaf);
        self.compliance_ephemeral_secret = Some(compliance_data.ephemeral_secret);
        self.salt = compliance_data.salt;
        self.dleq_k = compliance_data.dleq_k;
        self.ring_pk = ring_pk;
        self.dk_pub = dk_pub;
        self.threshold = threshold;
        self.is_flagged = is_flagged;

        // Compute DLEQ proof using policy fields from asset_indexed_leaf
        let metadata_hash = penumbra_sdk_compliance::compute_metadata_hash(
            self.asset_indexed_leaf.ring.policy_id_hash,
            self.asset_indexed_leaf.ring.resource_hash,
            self.asset_indexed_leaf.ring.permission_hash,
            Fq::from(1u64), // tier=1 (core) for spend
            Fq::from(self.target_timestamp),
            self.salt,
        );

        let b_d_fq = self
            .note
            .address()
            .diversified_generator()
            .vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack = ring_pk * d_fr;
        let epk = decaf377::Element::GENERATOR * self.compliance_ephemeral_secret.unwrap();

        let dleq = penumbra_sdk_compliance::compute_dleq_native(
            self.compliance_ephemeral_secret.unwrap(),
            self.dleq_k,
            &ack,
            &epk,
            metadata_hash,
        );
        self.dleq_c = dleq.c;
        self.dleq_s = Fq::from_le_bytes_mod_order(&dleq.s.to_bytes());

        Ok(())
    }

    /// Create a new [`SpendPlan`] that spends the given `position`ed `note`.
    pub fn new<R: CryptoRng + RngCore>(
        rng: &mut R,
        note: Note,
        position: tct::Position,
    ) -> SpendPlan {
        let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
        let dk_pub = decaf377::Element::GENERATOR;

        let compliance_data = crate::generate_compliance_details_spend(
            &mut *rng,
            &ring_pk,
            &dk_pub,
            &note.address(),
            note.asset_id(),
            note.amount(),
            false,
        )
        .expect("can encrypt compliance details");

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

        SpendPlan {
            note,
            position,
            randomizer: Fr::rand(rng),
            value_blinding: Fr::rand(rng),
            proof_blinding_r: Fq::rand(rng),
            proof_blinding_s: Fq::rand(rng),
            compliance_path,
            compliance_ciphertext: compliance_data.ciphertext,
            compliance_leaf: Some(compliance_leaf),
            compliance_ephemeral_secret: Some(compliance_data.ephemeral_secret),
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
            salt: compliance_data.salt,
            dleq_k: compliance_data.dleq_k,
            dleq_c: Fq::from(0u64),
            dleq_s: Fq::from(0u64),
            target_timestamp: 0u64,
        }
    }

    /// Create a dummy [`SpendPlan`].
    pub fn dummy<R: CryptoRng + RngCore>(rng: &mut R, fvk: &FullViewingKey) -> SpendPlan {
        // A valid address we can spend; since the note is hidden, we can just pick the default.
        let dummy_address = fvk.payment_address(AddressIndex::default()).0;
        let rseed = Rseed::generate(rng);
        let dummy_note = Note::from_parts(
            dummy_address,
            Value {
                amount: 0u64.into(),
                asset_id: *STAKING_TOKEN_ASSET_ID,
            },
            rseed,
        )
        .expect("dummy note is valid");

        Self::new(rng, dummy_note, 0u64.into())
    }

    /// Convenience method to construct the [`Spend`] described by this [`SpendPlan`].
    #[cfg(any(unix, windows))]
    pub fn spend(
        &self,
        fvk: &FullViewingKey,
        auth_sig: Signature<SpendAuth>,
        auth_path: tct::Proof,
        anchor: tct::Root,
        compliance_keys: Option<(decaf377::Element, decaf377::Element)>,
    ) -> Result<Spend, crate::ProofError> {
        Ok(Spend {
            body: self.spend_body(fvk, compliance_keys),
            auth_sig,
            proof: self.spend_proof(fvk, auth_path, anchor, compliance_keys)?,
        })
    }

    /// Construct the [`spend::Body`] described by this [`SpendPlan`].
    pub fn spend_body(
        &self,
        fvk: &FullViewingKey,
        _compliance_keys: Option<(decaf377::Element, decaf377::Element)>,
    ) -> Body {
        // Construct the backreference for this spend.
        let backref = Backref::new(self.note.commit());
        let encrypted_backref = backref.encrypt(&fvk.backref_key(), &self.nullifier(fvk));

        let user_leaf = self.compliance_leaf.clone().unwrap_or_else(|| {
            let b_d_fq = self
                .note
                .address()
                .diversified_generator()
                .vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            penumbra_sdk_compliance::ComplianceLeaf::new(
                self.note.address().clone(),
                self.note.asset_id(),
                d,
            )
        });
        let sender_leaf_hash = user_leaf.commit();

        let blinded_sender_leaf =
            penumbra_sdk_compliance::blind_sender_leaf(sender_leaf_hash, self.tx_blinding_nonce);

        // Serialize DLEQ proof: c (32 bytes) || s (32 bytes) = 64 bytes
        let mut dleq_proof = Vec::with_capacity(64);
        dleq_proof.extend_from_slice(&self.dleq_c.to_bytes());
        dleq_proof.extend_from_slice(&self.dleq_s.to_bytes());

        Body {
            balance_commitment: self.balance().commit(self.value_blinding),
            nullifier: self.nullifier(fvk),
            rk: self.rk(fvk),
            encrypted_backref,
            compliance_ciphertext: self.compliance_ciphertext.clone(),
            target_timestamp: self.target_timestamp,
            sender_leaf_hash: blinded_sender_leaf,
            compliance_anchor: self.compliance_anchor,
            asset_anchor: self.asset_anchor,
            dleq_proof,
        }
    }

    /// Construct the randomized verification key associated with this [`SpendPlan`].
    pub fn rk(&self, fvk: &FullViewingKey) -> decaf377_rdsa::VerificationKey<SpendAuth> {
        fvk.spend_verification_key().randomize(&self.randomizer)
    }

    /// Construct the [`Nullifier`] associated with this [`SpendPlan`].
    pub fn nullifier(&self, fvk: &FullViewingKey) -> Nullifier {
        let nk = fvk.nullifier_key();
        Nullifier::derive(nk, self.position, &self.note.commit())
    }

    /// Construct the [`SpendProof`] required by the [`spend::Body`] described by this [`SpendPlan`].
    #[cfg(any(unix, windows))]
    pub fn spend_proof(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proof: tct::Proof,
        anchor: tct::Root,
        _compliance_keys: Option<(decaf377::Element, decaf377::Element)>,
    ) -> Result<SpendProof, crate::ProofError> {
        let asset_anchor = self.asset_anchor;
        let compliance_anchor = self.compliance_anchor;

        let user_leaf = self.compliance_leaf.clone().unwrap_or_else(|| {
            let b_d_fq = self
                .note
                .address()
                .diversified_generator()
                .vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            penumbra_sdk_compliance::ComplianceLeaf::new(
                self.note.address().clone(),
                self.note.asset_id(),
                d,
            )
        });

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct = ComplianceCiphertext::from_bytes(&self.compliance_ciphertext).map_err(|e| {
            crate::ProofError::InvalidPublicInput(format!("invalid compliance ciphertext: {}", e))
        })?;
        let (epk, c2_core, compliance_ciphertext) = ct.to_spend_circuit_public_inputs();

        let sender_leaf_hash = user_leaf.commit();
        let blinded_sender_leaf =
            penumbra_sdk_compliance::blind_sender_leaf(sender_leaf_hash, self.tx_blinding_nonce);

        let public = SpendProofPublic {
            anchor,
            balance_commitment: self.balance().commit(self.value_blinding),
            nullifier: self.nullifier(fvk),
            rk: self.rk(fvk),
            asset_anchor,
            compliance_anchor,
            epk,
            c2_core,
            compliance_ciphertext,
            target_timestamp: Fq::from(self.target_timestamp),
            dleq_c: self.dleq_c,
            dleq_s: self.dleq_s,
            sender_leaf_hash: blinded_sender_leaf,
        };

        let private = SpendProofPrivate {
            state_commitment_proof,
            note: self.note.clone(),
            v_blinding: self.value_blinding,
            spend_auth_randomizer: self.randomizer,
            ak: *fvk.spend_verification_key(),
            nk: *fvk.nullifier_key(),
            asset_path: self.asset_path.clone(),
            asset_position: self.asset_position,
            asset_indexed_leaf: self.asset_indexed_leaf.clone(),
            is_regulated: self.is_regulated,
            compliance_path: self.compliance_path.clone(),
            compliance_position: self.compliance_position,
            user_leaf,
            compliance_ephemeral_secret: self.compliance_ephemeral_secret.unwrap_or(Fr::from(0u64)),
            tx_blinding_nonce: self.tx_blinding_nonce,
            is_flagged: self.is_flagged,
            salt: self.salt,
        };

        SpendProof::prove(public, private)
    }

    pub fn balance(&self) -> Balance {
        Value {
            amount: self.note.value().amount,
            asset_id: self.note.value().asset_id,
        }
        .into()
    }
}

impl DomainType for SpendPlan {
    type Proto = pb::SpendPlan;
}

impl From<SpendPlan> for pb::SpendPlan {
    fn from(msg: SpendPlan) -> Self {
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

impl TryFrom<pb::SpendPlan> for SpendPlan {
    type Error = anyhow::Error;
    fn try_from(msg: pb::SpendPlan) -> Result<Self, Self::Error> {
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
            threshold: if msg.threshold.len() == 16 {
                u128::from_le_bytes(msg.threshold.as_slice().try_into()?)
            } else {
                u128::MAX
            },
            is_flagged: msg.is_flagged,
            salt: if msg.salt.len() == 32 {
                Fq::from_bytes_checked(msg.salt.as_slice().try_into()?).unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_k: if msg.dleq_k.len() == 32 {
                Fr::from_bytes_checked(msg.dleq_k.as_slice().try_into()?).unwrap_or(Fr::from(0u64))
            } else {
                Fr::from(0u64)
            },
            dleq_c: if msg.dleq_c.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_c.as_slice().try_into()?).unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            dleq_s: if msg.dleq_s.len() == 32 {
                Fq::from_bytes_checked(msg.dleq_s.as_slice().try_into()?).unwrap_or(Fq::from(0u64))
            } else {
                Fq::from(0u64)
            },
            target_timestamp: msg.target_timestamp,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_proof_helpers::proof_test_helpers::*;
    use rand_core::OsRng;

    #[cfg(feature = "bundled-proving-keys")]
    fn verify_spend_proof_with_asset(asset_id_u64: u64) {
        use crate::test_proof_helpers::proof_test_helpers::{
            create_imt_membership_proof, create_imt_non_membership_proof, create_user_tree_proof,
        };

        let mut rng = OsRng;

        let is_regulated = asset_id_u64 == REGULATED_ASSET_ID;

        // Setup circuit keys and SCT
        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let fvk = sk.full_viewing_key();
        let ivk = fvk.incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        let value = penumbra_sdk_asset::Value {
            amount: 100u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(asset_id_u64)),
        };
        let note =
            crate::Note::from_parts(address.clone(), value, crate::Rseed::generate(&mut rng))
                .expect("can create note");

        let mut sct = penumbra_sdk_tct::Tree::new();
        let note_commitment = note.commit();
        sct.insert(penumbra_sdk_tct::Witness::Keep, note_commitment)
            .unwrap();
        let anchor = sct.root();
        let state_commitment_proof = sct.witness(note_commitment).unwrap();

        // Create IMT proof
        let asset_id_fq = Fq::from(asset_id_u64);
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) = if is_regulated {
            create_imt_membership_proof(
                asset_id_fq,
                *penumbra_sdk_compliance::BLACK_HOLE_ACK,
                decaf377::Element::GENERATOR,
            )
        } else {
            create_imt_non_membership_proof(asset_id_fq)
        };

        // Create SpendPlan
        let mut spend_plan =
            SpendPlan::new(&mut rng, note.clone(), state_commitment_proof.position());

        // Set the IMT proof data
        spend_plan.asset_anchor = asset_anchor;
        spend_plan.asset_path = asset_path;
        spend_plan.asset_position = asset_position;
        spend_plan.asset_indexed_leaf = asset_indexed_leaf;
        spend_plan.is_regulated = is_regulated;

        // set_compliance_details reads policy from asset_indexed_leaf;
        // for unregulated assets it overrides ring_pk to BLACK_HOLE_ACK.
        spend_plan
            .set_compliance_details(&mut rng)
            .expect("can set compliance details");

        // Create valid user tree proof
        let user_leaf = spend_plan.compliance_leaf.clone().unwrap();
        let (compliance_anchor, compliance_path, compliance_position) =
            create_user_tree_proof(&user_leaf);

        spend_plan.compliance_anchor = compliance_anchor;
        spend_plan.compliance_path = compliance_path;
        spend_plan.compliance_position = compliance_position;

        // Generate proof
        let spend_proof = spend_plan
            .spend_proof(&fvk, state_commitment_proof, anchor, None)
            .expect("proof generation should succeed");

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct = ComplianceCiphertext::from_bytes(&spend_plan.compliance_ciphertext)
            .expect("can deserialize ciphertext");
        let (epk, c2_core, packed_ciphertext) = ct.to_spend_circuit_public_inputs();

        let sender_leaf_hash = spend_plan.compliance_leaf.clone().unwrap().commit();
        let blinded_sender = penumbra_sdk_compliance::blind_sender_leaf(
            sender_leaf_hash,
            spend_plan.tx_blinding_nonce,
        );

        spend_proof
            .verify(
                &penumbra_sdk_proof_params::SPEND_PROOF_VERIFICATION_KEY,
                SpendProofPublic {
                    anchor,
                    balance_commitment: spend_plan.balance().commit(spend_plan.value_blinding),
                    nullifier: spend_plan.nullifier(&fvk),
                    rk: spend_plan.rk(&fvk),
                    asset_anchor,
                    compliance_anchor,
                    epk,
                    c2_core,
                    compliance_ciphertext: packed_ciphertext,
                    target_timestamp: Fq::from(spend_plan.target_timestamp),
                    dleq_c: spend_plan.dleq_c,
                    dleq_s: spend_plan.dleq_s,
                    sender_leaf_hash: blinded_sender,
                },
            )
            .unwrap();
    }

    #[cfg(feature = "bundled-proving-keys")]
    #[test]
    fn test_regulated_asset_spend_proof() {
        verify_spend_proof_with_asset(REGULATED_ASSET_ID);
    }

    #[cfg(feature = "bundled-proving-keys")]
    #[test]
    fn test_unregulated_asset_spend_proof() {
        verify_spend_proof_with_asset(UNREGULATED_ASSET_ID);
    }

    /// Test that spend is flagged when amount >= threshold
    #[test]
    fn test_spend_flagged_above_threshold() {
        let mut rng = OsRng;
        let threshold = 500u128;

        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let ivk = sk.full_viewing_key().incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        // amount=1000 >= threshold=500 => flagged
        let value = penumbra_sdk_asset::Value {
            amount: 1000u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(1u64)),
        };
        let note =
            crate::Note::from_parts(address.clone(), value, crate::Rseed::generate(&mut rng))
                .expect("can create note");

        let mut spend_plan = SpendPlan::new(&mut rng, note.clone(), 0u64.into());

        // Inject desired threshold into asset_indexed_leaf before calling set_compliance_details
        spend_plan.asset_indexed_leaf.params.threshold = threshold;

        spend_plan
            .set_compliance_details(&mut rng)
            .expect("can set compliance details");

        assert!(
            spend_plan.is_flagged,
            "amount 1000 >= threshold 500 should be flagged"
        );
    }

    /// Test that spend is NOT flagged when amount < threshold
    #[test]
    fn test_spend_not_flagged_below_threshold() {
        let mut rng = OsRng;
        let threshold = 500u128;

        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let ivk = sk.full_viewing_key().incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        // amount=100 < threshold=500 => NOT flagged
        let value = penumbra_sdk_asset::Value {
            amount: 100u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(1u64)),
        };
        let note =
            crate::Note::from_parts(address.clone(), value, crate::Rseed::generate(&mut rng))
                .expect("can create note");

        let mut spend_plan = SpendPlan::new(&mut rng, note.clone(), 0u64.into());

        // Inject desired threshold into asset_indexed_leaf before calling set_compliance_details
        spend_plan.asset_indexed_leaf.params.threshold = threshold;

        spend_plan
            .set_compliance_details(&mut rng)
            .expect("can set compliance details");

        assert!(
            !spend_plan.is_flagged,
            "amount 100 < threshold 500 should NOT be flagged"
        );
    }

    /// Unregulated assets must use BLACK_HOLE_ACK for encryption even when
    /// the IMT low leaf carries a regulated asset's policy keys.
    #[test]
    fn test_unregulated_overrides_low_leaf_keys_to_black_hole() {
        let mut rng = OsRng;

        let seed_phrase = penumbra_sdk_keys::keys::SeedPhrase::generate(&mut rng);
        let sk = penumbra_sdk_keys::keys::SpendKey::from_seed_phrase_bip44(
            seed_phrase,
            &penumbra_sdk_keys::keys::Bip44Path::new(0),
        );
        let ivk = sk.full_viewing_key().incoming();
        let (address, _) = ivk.payment_address(0u32.into());

        let value = penumbra_sdk_asset::Value {
            amount: 100u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(Fq::from(999u64)),
        };
        let note =
            crate::Note::from_parts(address.clone(), value, crate::Rseed::generate(&mut rng))
                .expect("can create note");

        let mut spend_plan = SpendPlan::new(&mut rng, note, 0u64.into());

        // Simulate a low leaf from a regulated asset's non-membership proof:
        // inject real-looking policy keys that are NOT BLACK_HOLE_ACK.
        let fake_ring_sk = Fr::rand(&mut rng);
        let fake_ring_pk = decaf377::Element::GENERATOR * fake_ring_sk;
        spend_plan.asset_indexed_leaf.ring.ring_pk = fake_ring_pk;
        spend_plan.asset_indexed_leaf.params.dk_pub = decaf377::Element::GENERATOR;
        spend_plan.is_regulated = false;

        spend_plan
            .set_compliance_details(&mut rng)
            .expect("can set compliance details");

        assert_eq!(
            spend_plan.ring_pk,
            *penumbra_sdk_compliance::BLACK_HOLE_ACK,
            "Unregulated spend must override low leaf ring_pk to BLACK_HOLE_ACK"
        );
        assert_eq!(
            spend_plan.dk_pub,
            decaf377::Element::GENERATOR,
            "Unregulated spend must use GENERATOR as dk_pub"
        );
    }
}
