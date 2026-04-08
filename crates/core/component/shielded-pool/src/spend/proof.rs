use base64::prelude::*;
use std::str::FromStr;

use anyhow::Result;
use ark_r1cs_std::{
    prelude::{EqGadget, FieldVar, ToBitsGadget},
    uint8::UInt8,
};
use ark_serialize::CanonicalDeserialize;
use decaf377::{r1cs::FqVar, Bls12_377, Fq, Fr};

use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_r1cs_std::prelude::AllocVar;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef};
use ark_snark::SNARK;
use decaf377_rdsa::{SpendAuth, VerificationKey};
use penumbra_sdk_proto::{penumbra::core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_tct as tct;
use penumbra_sdk_tct::r1cs::StateCommitmentVar;

use crate::{note, Note, Rseed};
use decaf377::r1cs::ElementVar;
use penumbra_sdk_asset::{balance, Value};
use penumbra_sdk_compliance::r1cs::{verify_compliance_spend, ComplianceWitness};
use penumbra_sdk_keys::keys::{
    AuthorizationKeyVar, Bip44Path, IncomingViewingKeyVar, NullifierKey, NullifierKeyVar,
    SeedPhrase, SpendAuthRandomizerVar, SpendKey,
};
use penumbra_sdk_proof_params::{DummyWitness, VerifyingKeyExt, GROTH16_PROOF_LENGTH_BYTES};
use penumbra_sdk_sct::{Nullifier, NullifierVar};
use tap::Tap;

use crate::public_input_hash::{
    spend_statement_hash_from_public, spend_statement_hash_var, SPEND_STATEMENT_FIELD_COUNT,
};

/// The public input for a [`SpendProof`].
#[derive(Clone, Debug)]
pub struct SpendProofPublic {
    /// the merkle root of the state commitment tree.
    pub anchor: tct::Root,
    /// balance commitment of the note to be spent.
    pub balance_commitment: balance::Commitment,
    /// nullifier of the note to be spent.
    pub nullifier: Nullifier,
    /// the randomized verification spend key.
    pub rk: VerificationKey<SpendAuth>,
    /// Asset registry Merkle root (asset regulation tree anchor)
    pub asset_anchor: tct::StateCommitment,
    /// User compliance registry Merkle root (user tree anchor)
    pub compliance_anchor: tct::StateCommitment,
    /// Ephemeral public key on standard generator G (r_s × G)
    pub epk: decaf377::Element,
    /// Encrypted seed for core tier (ElGamal envelope: C2 = S + ss.compress())
    pub c2_core: Fq,
    /// Compliance ciphertext: detection(2) + core(3) = 5 Fq elements
    pub compliance_ciphertext: Vec<Fq>,
    /// DLEQ target timestamp (Unix UTC seconds, encoded as Fq for circuit)
    pub target_timestamp: Fq,
    /// DLEQ challenge (253-bit truncated, stored as Fq but canonical Fr)
    pub dleq_c: Fq,
    /// DLEQ response (canonical Fr, stored as Fq)
    pub dleq_s: Fq,
    /// Hash of the sender's compliance leaf (for binding with output circuit)
    pub sender_leaf_hash: tct::StateCommitment,
}

/// The private input for a [`SpendProof`].
#[derive(Clone, Debug)]
pub struct SpendProofPrivate {
    /// Inclusion proof for the note commitment.
    pub state_commitment_proof: tct::Proof,
    /// The note being spent.
    pub note: Note,
    /// The blinding factor used for generating the balance commitment.
    pub v_blinding: Fr,
    /// The randomizer used for generating the randomized spend auth key.
    pub spend_auth_randomizer: Fr,
    /// The spend authorization key.
    pub ak: VerificationKey<SpendAuth>,
    /// The nullifier deriving key.
    pub nk: NullifierKey,
    /// Asset registry Merkle path proving asset regulation status
    pub asset_path: penumbra_sdk_compliance::MerklePath,
    /// Position of the asset in the asset registry IMT
    pub asset_position: u64,
    /// The indexed leaf from the asset IMT (for membership/non-membership proofs)
    pub asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf,
    /// Whether this asset is regulated (requires compliance)
    pub is_regulated: bool,
    /// Compliance Merkle path proving user is in compliance registry
    pub compliance_path: penumbra_sdk_compliance::MerklePath,
    /// Position of the compliance leaf in the QuadTree
    pub compliance_position: u64,
    /// User's compliance leaf (address, asset_id)
    pub user_leaf: penumbra_sdk_compliance::ComplianceLeaf,
    /// Ephemeral secret used to encrypt the compliance ciphertext (needed by circuit for verification)
    pub compliance_ephemeral_secret: Fr,
    /// Shared transaction blinding nonce (same for spend and output in one transaction)
    pub tx_blinding_nonce: Fr,
    /// Whether this spend is flagged (amount >= threshold)
    pub is_flagged: bool,
    /// Random salt for DLEQ metadata hash (encrypted in detection tier)
    pub salt: decaf377::Fq,
}

#[cfg(test)]
fn check_satisfaction(public: &SpendProofPublic, private: &SpendProofPrivate) -> Result<()> {
    use penumbra_sdk_keys::keys::FullViewingKey;

    let note_commitment = private.note.commit();
    if note_commitment != private.state_commitment_proof.commitment() {
        anyhow::bail!("note commitment did not match state commitment proof");
    }

    let nullifier = Nullifier::derive(
        &private.nk,
        private.state_commitment_proof.position(),
        &note_commitment,
    );
    if nullifier != public.nullifier {
        anyhow::bail!("nullifier did not match public input");
    }

    let amount_u128: u128 = private.note.value().amount.into();
    if amount_u128 != 0u128 {
        private.state_commitment_proof.verify(public.anchor)?;
    }

    let rk = private.ak.randomize(&private.spend_auth_randomizer);
    if rk != public.rk {
        anyhow::bail!("randomized spend auth key did not match public input");
    }

    let fvk = FullViewingKey::from_components(private.ak, private.nk);
    let ivk = fvk.incoming();
    let transmission_key = ivk.diversified_public(&private.note.diversified_generator());
    if transmission_key != *private.note.transmission_key() {
        anyhow::bail!("transmission key did not match note");
    }

    let balance_commitment = private.note.value().commit(private.v_blinding);
    if balance_commitment != public.balance_commitment {
        anyhow::bail!("balance commitment did not match public input");
    }

    if private.note.diversified_generator() == decaf377::Element::default() {
        anyhow::bail!("diversified generator is identity");
    }
    if private.ak.is_identity() {
        anyhow::bail!("ak is identity");
    }

    Ok(())
}

#[cfg(test)]
fn check_circuit_satisfaction(public: SpendProofPublic, private: SpendProofPrivate) -> Result<()> {
    use ark_relations::r1cs::{self, ConstraintSystem};

    let cs = ConstraintSystem::new_ref();
    let claimed_statement_hash = spend_statement_hash_from_public(&public)
        .map_err(|e| anyhow::anyhow!("failed to compute spend statement hash: {e}"))?;
    let circuit = SpendCircuit {
        public,
        private,
        claimed_statement_hash,
    };
    cs.set_optimization_goal(r1cs::OptimizationGoal::Constraints);
    circuit
        .generate_constraints(cs.clone())
        .expect("can generate constraints from circuit");
    cs.finalize();
    if !cs.is_satisfied()? {
        anyhow::bail!("constraints are not satisfied");
    }
    Ok(())
}

/// Groth16 proof for spending existing notes.
#[derive(Clone, Debug)]
pub struct SpendCircuit {
    public: SpendProofPublic,
    private: SpendProofPrivate,
    claimed_statement_hash: Fq,
}

impl SpendCircuit {
    #[cfg(feature = "benchmark-helpers")]
    pub fn from_parts(
        public: SpendProofPublic,
        private: SpendProofPrivate,
        claimed_statement_hash: Fq,
    ) -> Self {
        Self {
            public,
            private,
            claimed_statement_hash,
        }
    }

    pub fn into_parts(self) -> (SpendProofPublic, SpendProofPrivate, Fq) {
        (self.public, self.private, self.claimed_statement_hash)
    }
}

impl ConstraintSynthesizer<Fq> for SpendCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> ark_relations::r1cs::Result<()> {
        // === Witness Allocation ===
        // Note: in the allocation of the address on `NoteVar` we check the diversified base is not identity.
        let note_var = note::NoteVar::new_witness(cs.clone(), || Ok(self.private.note.clone()))?;
        let claimed_note_commitment = StateCommitmentVar::new_witness(cs.clone(), || {
            Ok(self.private.state_commitment_proof.commitment())
        })?;

        let position_var = tct::r1cs::PositionVar::new_witness(cs.clone(), || {
            Ok(self.private.state_commitment_proof.position())
        })?;
        let position_bits = position_var.to_bits_le()?;
        let merkle_path_var = tct::r1cs::MerkleAuthPathVar::new_witness(cs.clone(), || {
            Ok(self.private.state_commitment_proof)
        })?;

        let v_blinding_arr: [u8; 32] = self.private.v_blinding.to_bytes();
        let v_blinding_vars = UInt8::new_witness_vec(cs.clone(), &v_blinding_arr)?;

        let spend_auth_randomizer_var = SpendAuthRandomizerVar::new_witness(cs.clone(), || {
            Ok(self.private.spend_auth_randomizer)
        })?;
        // Note: in the allocation of `AuthorizationKeyVar` we check it is not identity.
        let ak_element_var: AuthorizationKeyVar =
            AuthorizationKeyVar::new_witness(cs.clone(), || Ok(self.private.ak))?;
        let nk_var = NullifierKeyVar::new_witness(cs.clone(), || Ok(self.private.nk))?;

        // Witness statement fields (single public input is the statement hash).
        let anchor_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(self.public.anchor)))?;
        let claimed_balance_commitment_var =
            ElementVar::new_witness(cs.clone(), || Ok(self.public.balance_commitment.0))?;
        let claimed_nullifier_var = FqVar::new_witness(cs.clone(), || Ok(self.public.nullifier.0))?;
        let rk_var = ElementVar::new_witness(cs.clone(), || {
            decaf377::Encoding(self.public.rk.to_bytes())
                .vartime_decompress()
                .map_err(|_| ark_relations::r1cs::SynthesisError::MalformedVerifyingKey)
        })?;
        let claimed_asset_anchor =
            StateCommitmentVar::new_witness(cs.clone(), || Ok(self.public.asset_anchor))?;
        let claimed_compliance_anchor =
            StateCommitmentVar::new_witness(cs.clone(), || Ok(self.public.compliance_anchor))?;
        let epk_var = ElementVar::new_witness(cs.clone(), || Ok(self.public.epk))?;
        let c2_core_var = FqVar::new_witness(cs.clone(), || Ok(self.public.c2_core))?;

        // Spend ciphertext: detection(2) + core(3) = 5 Fq elements.
        let mut ciphertext_vars = Vec::new();
        for fq in self.public.compliance_ciphertext.iter() {
            ciphertext_vars.push(FqVar::new_witness(cs.clone(), || Ok(*fq))?);
        }

        // DLEQ values are witness data bound by the statement hash.
        let target_timestamp_var =
            FqVar::new_witness(cs.clone(), || Ok(self.public.target_timestamp))?;
        let dleq_c_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_c))?;
        let dleq_s_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_s))?;

        let claimed_statement_hash_var =
            FqVar::new_input(cs.clone(), || Ok(self.claimed_statement_hash))?;

        // === Spend-Specific Integrity Checks ===
        let note_commitment_var = note_var.commit()?;
        note_commitment_var.enforce_equal(&claimed_note_commitment)?;

        // Nullifier integrity.
        let nullifier_var = NullifierVar::derive(&nk_var, &position_var, &claimed_note_commitment)?;
        nullifier_var.inner.enforce_equal(&claimed_nullifier_var)?;

        // Merkle auth path verification against the provided anchor.
        //
        // We short circuit the merkle path verification if the note is a _dummy_ spend (a spend
        // with zero value), since these are never committed to the state commitment tree.
        let is_not_dummy = !note_var.amount().is_eq(&FqVar::zero())?;
        merkle_path_var.verify(
            cs.clone(),
            &is_not_dummy,
            &position_bits,
            anchor_var.clone(),
            claimed_note_commitment.inner(),
        )?;

        // Check integrity of randomized verification key.
        let computed_rk_var = ak_element_var.randomize(&spend_auth_randomizer_var)?;
        computed_rk_var.inner.enforce_equal(&rk_var)?;

        // Check integrity of diversified address.
        let ivk = IncomingViewingKeyVar::derive(&nk_var, &ak_element_var)?;
        let computed_transmission_key =
            ivk.diversified_public(&note_var.diversified_generator())?;
        computed_transmission_key.enforce_equal(&note_var.transmission_key())?;

        // Check integrity of balance commitment.
        let balance_commitment = note_var.value().commit(v_blinding_vars)?;
        balance_commitment
            .inner
            .enforce_equal(&claimed_balance_commitment_var)?;

        // === Spend Compliance Verification (base only — no extension tier) ===

        let compliance_witness = ComplianceWitness {
            is_regulated: self.private.is_regulated,
            asset_indexed_leaf: self.private.asset_indexed_leaf.clone(),
            asset_path: self.private.asset_path.clone(),
            asset_position: self.private.asset_position,
            compliance_path: self.private.compliance_path.clone(),
            compliance_position: self.private.compliance_position,
            user_leaf: self.private.user_leaf.clone(),
            is_flagged: self.private.is_flagged,
            salt: self.private.salt,
        };

        verify_compliance_spend(
            cs.clone(),
            claimed_asset_anchor.inner().clone(),
            claimed_compliance_anchor.inner().clone(),
            epk_var.clone(),
            c2_core_var.clone(),
            ciphertext_vars.clone(),
            target_timestamp_var.clone(),
            dleq_c_var.clone(),
            dleq_s_var.clone(),
            note_var.asset_id(),
            note_var.amount(),
            note_var.diversified_generator(),
            note_var.transmission_key(),
            self.private.compliance_ephemeral_secret,
            compliance_witness,
        )?;

        // === Sender Leaf Binding ===

        let tx_blinding_nonce_var = FqVar::new_witness(cs.clone(), || {
            Ok(Fq::from_le_bytes_mod_order(
                &self.private.tx_blinding_nonce.to_bytes(),
            ))
        })?;

        let sender_leaf_var =
            penumbra_sdk_compliance::r1cs::ComplianceLeafVar::new_witness(cs.clone(), || {
                Ok(self.private.user_leaf.clone())
            })?;
        let sender_leaf_hash = sender_leaf_var.commit(cs.clone())?;

        let computed_blinded_sender =
            penumbra_sdk_compliance::leaf_binding::r1cs::blind_sender_leaf(
                cs.clone(),
                sender_leaf_hash,
                tx_blinding_nonce_var,
            )?;

        let claimed_blinded_sender =
            FqVar::new_witness(cs.clone(), || Ok(self.public.sender_leaf_hash.0))?;
        computed_blinded_sender.enforce_equal(&claimed_blinded_sender)?;

        // Bind all statement fields to a single public input hash.
        let mut statement_fields = Vec::with_capacity(SPEND_STATEMENT_FIELD_COUNT);
        statement_fields.push(anchor_var);
        statement_fields.push(claimed_balance_commitment_var.compress_to_field()?);
        statement_fields.push(claimed_nullifier_var);
        statement_fields.push(rk_var.compress_to_field()?);
        statement_fields.push(claimed_asset_anchor.inner());
        statement_fields.push(claimed_compliance_anchor.inner());
        statement_fields.push(epk_var.compress_to_field()?);
        statement_fields.push(c2_core_var);
        statement_fields.extend(ciphertext_vars);
        statement_fields.push(target_timestamp_var);
        statement_fields.push(dleq_c_var);
        statement_fields.push(dleq_s_var);
        statement_fields.push(claimed_blinded_sender);

        let computed_statement_hash = spend_statement_hash_var(cs.clone(), &statement_fields)?;
        computed_statement_hash.enforce_equal(&claimed_statement_hash_var)?;

        Ok(())
    }
}

impl DummyWitness for SpendCircuit {
    fn with_dummy_witness() -> Self {
        use penumbra_sdk_compliance::crypto::encrypt_spend;
        use penumbra_sdk_compliance::derive_compliance_scalar;

        let seed_phrase = SeedPhrase::from_randomness(&[b'f'; 32]);
        let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk_sender = sk_sender.full_viewing_key();
        let ivk_sender = fvk_sender.incoming();
        let (address, _dtk_d) = ivk_sender.payment_address(0u32.into());

        let note = Note::from_parts(
            address.clone(),
            Value::from_str("1upenumbra").unwrap(),
            Rseed([1u8; 32]),
        )
        .unwrap();

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        let state_commitment_proof = sct.witness(note.commit()).unwrap();

        let dummy_path = penumbra_sdk_compliance::MerklePath {
            layers: (0..16)
                .map(|_| penumbra_sdk_compliance::structs::MerklePathLayer {
                    siblings: vec![vec![0u8; 32], vec![0u8; 32], vec![0u8; 32]],
                })
                .collect(),
        };

        // Use BLACK_HOLE_ACK as ring_pk for unregulated dummy
        let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
        let dk_pub = decaf377::Element::GENERATOR;

        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack = ring_pk * d_fr;

        let mut rng = rand::thread_rng();
        let encryption_result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &address,
            note.asset_id(),
            note.amount(),
            false,
            Fq::from(0u64),
        )
        .expect("can encrypt spend");

        let (epk, c2_core, compliance_ciphertext) = encryption_result
            .ciphertext
            .to_spend_circuit_public_inputs();
        let r_s = encryption_result.r_s;

        let dummy_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            address.clone(),
            penumbra_sdk_asset::asset::Id(Fq::from(0u64)),
            Fq::from(0u64),
        );
        let sender_leaf_hash =
            penumbra_sdk_compliance::blind_sender_leaf(dummy_leaf.commit(), Fr::from(0u64));

        let public = SpendProofPublic {
            anchor: sct.root(),
            balance_commitment: balance::Commitment(decaf377::Element::GENERATOR),
            nullifier: Nullifier(Fq::from(1u64)),
            rk: sk_sender.spend_auth_key().randomize(&Fr::from(1u64)).into(),
            asset_anchor: tct::StateCommitment(Fq::from(0u64)),
            compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
            epk,
            c2_core,
            compliance_ciphertext,
            target_timestamp: Fq::from(0u64),
            dleq_c: Fq::from(0u64),
            dleq_s: Fq::from(0u64),
            sender_leaf_hash,
        };
        let private = SpendProofPrivate {
            state_commitment_proof,
            note,
            v_blinding: Fr::from(1u64),
            spend_auth_randomizer: Fr::from(1u64),
            ak: sk_sender.spend_auth_key().into(),
            nk: *sk_sender.nullifier_key(),
            asset_path: dummy_path.clone(),
            asset_position: 0,
            asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                Fq::from(0u64),
                0,
                penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
            ),
            is_regulated: false,
            compliance_path: dummy_path,
            compliance_position: 0,
            user_leaf: dummy_leaf,
            compliance_ephemeral_secret: r_s,
            tx_blinding_nonce: Fr::from(0u64),
            is_flagged: false,
            salt: decaf377::Fq::from(0u64),
        };
        let claimed_statement_hash = spend_statement_hash_from_public(&public)
            .expect("dummy spend statement hash should compute");
        Self {
            public,
            private,
            claimed_statement_hash,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpendProof([u8; GROTH16_PROOF_LENGTH_BYTES]);

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("error deserializing compressed proof: {0:?}")]
    ProofDeserialize(ark_serialize::SerializationError),
    #[error("Fq types are Bls12-377 field members")]
    Anchor,
    #[error("balance commitment is a Bls12-377 field member")]
    BalanceCommitment,
    #[error("nullifier is a Bls12-377 field member")]
    Nullifier,
    #[error("could not decompress element points: {0:?}")]
    DecompressRk(decaf377::EncodingError),
    #[error("randomized spend key is a Bls12-377 field member")]
    Rk,
    #[error("start position is a Bls12-377 field member")]
    StartPosition,
    #[error("asset anchor is a Bls12-377 field member")]
    AssetAnchor,
    #[error("compliance anchor is a Bls12-377 field member")]
    ComplianceAnchor,
    #[error("epk is a Bls12-377 field member")]
    Epk,
    #[error("c2_core is a Bls12-377 field member")]
    C2Core,
    #[error("sender leaf hash is a Bls12-377 field member")]
    SenderLeafHash,
    #[error("failed computing spend statement hash: {0}")]
    StatementHash(String),
    #[error("error verifying proof: {0:?}")]
    SynthesisError(ark_relations::r1cs::SynthesisError),
    #[error("spend proof did not verify")]
    InvalidProof,
}

impl SpendProof {
    /// Generate a `SpendProof` given the public inputs and witness data.
    #[cfg(any(unix, windows))]
    pub fn prove(
        public: SpendProofPublic,
        private: SpendProofPrivate,
    ) -> Result<Self, crate::ProofError> {
        let client = gnark_spend_client()?;
        client.prove(&public, &private).map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!("gnark spend prove: {e}"))
        })
    }

    /// Construct a `BatchItem` from this proof and its public inputs.
    /// Deserializes the proof and builds the public input vector without verifying.
    /// This is the single source of truth for public input ordering.
    pub fn to_batch_item(
        &self,
        public: SpendProofPublic,
    ) -> Result<penumbra_sdk_proof_params::batch::BatchItem, VerificationError> {
        let proof = Proof::deserialize_compressed_unchecked(&self.0[..])
            .map_err(VerificationError::ProofDeserialize)?;
        let statement_hash = spend_statement_hash_from_public(&public)
            .map_err(|e| VerificationError::StatementHash(e.to_string()))?;

        Ok(penumbra_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    /// Called to verify the proof using the provided public inputs.
    // For debugging proof verification failures,
    // to check that the proof data and verification keys are consistent.
    #[tracing::instrument(level="debug", skip(self, vk), fields(self = ?BASE64_STANDARD.encode(self.clone().encode_to_vec()), vk = ?vk.debug_id()))]
    pub fn verify(
        &self,
        vk: &PreparedVerifyingKey<Bls12_377>,
        public: SpendProofPublic,
    ) -> Result<(), VerificationError> {
        let item = self.to_batch_item(public)?;

        tracing::trace!(public_inputs = ?item.public_inputs);

        let start = std::time::Instant::now();
        Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            vk,
            item.public_inputs.as_slice(),
            &item.proof,
        )
        .map_err(VerificationError::SynthesisError)?
        .tap(|proof_result| tracing::debug!(?proof_result, elapsed = ?start.elapsed()))
        .then_some(())
        .ok_or(VerificationError::InvalidProof)
    }
}

#[cfg(any(unix, windows))]
static GNARK_SPEND_CLIENT: once_cell::sync::OnceCell<crate::gnark::GnarkSpendClient> =
    once_cell::sync::OnceCell::new();

#[cfg(any(unix, windows))]
fn gnark_spend_client() -> Result<&'static crate::gnark::GnarkSpendClient, crate::ProofError> {
    GNARK_SPEND_CLIENT.get_or_try_init(init_gnark_spend_client)
}

#[cfg(any(unix, windows))]
fn init_gnark_spend_client() -> Result<crate::gnark::GnarkSpendClient, crate::ProofError> {
    // Env-var override (dev/CI): explicit artifact directory and library/daemon path.
    if std::env::var_os("PENUMBRA_GNARK_SPEND_LIB").is_some()
        || std::env::var_os("PENUMBRA_GNARK_SPEND_DAEMON").is_some()
        || std::env::var_os("PENUMBRA_GNARK_SPEND_ARTIFACT_DIR").is_some()
    {
        return crate::gnark::GnarkSpendClient::from_env().map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!("gnark spend init: {e}"))
        });
    }

    // Bundled path (test/dev default): build-script-generated library path or installed sidecar.
    let lib_path = crate::gnark::GnarkSpendClient::bundled_lib_path()
        .or_else(crate::gnark::GnarkSpendClient::auto_lib_path)
        .ok_or_else(|| {
            crate::ProofError::ProofGenerationFailed(
                "gnark spend library not found (checked bundled path and executable-adjacent locations)"
                    .into(),
            )
        })?;
    let pk_bytes = penumbra_sdk_proof_params::GNARK_SPEND_PROOF_PROVING_KEY_BYTES;
    if pk_bytes.is_empty() {
        return Err(crate::ProofError::ProofGenerationFailed(
            "gnark spend proving key not bundled \
             (enable bundled-proving-keys feature)"
                .into(),
        ));
    }
    let pvk = penumbra_sdk_proof_params::SPEND_PROOF_VERIFICATION_KEY.clone();
    let metadata = penumbra_sdk_proof_params::GNARK_SPEND_CIRCUIT_METADATA;
    crate::gnark::GnarkSpendClient::from_bundled(&lib_path, pk_bytes, pvk, metadata)
        .map_err(|e| crate::ProofError::ProofGenerationFailed(format!("gnark spend init: {e}")))
}

impl DomainType for SpendProof {
    type Proto = pb::ZkSpendProof;
}

impl From<SpendProof> for pb::ZkSpendProof {
    fn from(proof: SpendProof) -> Self {
        pb::ZkSpendProof {
            inner: proof.0.to_vec(),
        }
    }
}

impl TryFrom<pb::ZkSpendProof> for SpendProof {
    type Error = anyhow::Error;

    fn try_from(proto: pb::ZkSpendProof) -> Result<Self, Self::Error> {
        Ok(SpendProof(proto.inner[..].try_into()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_r1cs_std::prelude::Boolean;
    use decaf377::{Fq, Fr};
    use penumbra_sdk_asset::{asset, Value};
    use penumbra_sdk_keys::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        Address,
    };
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_proof_params::generate_prepared_test_parameters;
    use penumbra_sdk_sct::Nullifier;
    use penumbra_sdk_tct::StateCommitment;
    use proptest::prelude::*;

    use crate::test_proof_helpers::proof_test_helpers::{
        create_imt_membership_proof, create_imt_non_membership_proof, create_user_tree_proof,
    };
    use crate::Note;
    use decaf377_rdsa::{SpendAuth, VerificationKey};
    use penumbra_sdk_tct as tct;
    use rand_core::OsRng;

    fn fr_strategy() -> BoxedStrategy<Fr> {
        any::<[u8; 32]>()
            .prop_map(|bytes| Fr::from_le_bytes_mod_order(&bytes[..]))
            .boxed()
    }

    prop_compose! {
        // Unregulated strategy
        fn arb_valid_spend_statement()(
            v_blinding in fr_strategy(),
            spend_auth_randomizer in fr_strategy(),
            asset_id64 in any::<u64>(),
            address_index in any::<u32>(),
            amount in any::<u64>(),
            seed_phrase_randomness in any::<[u8; 32]>(),
            rseed_randomness in any::<[u8; 32]>(),
            num_commitments in 0..100,
            tester_keys_rand in any::<[u8; 32]>()
        ) -> (SpendProofPublic, SpendProofPrivate, Fr) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();

            // Next, we simulate the case where the SCT is not empty by adding `num_commitments`
            // unrelated items in the SCT.
            for i in 0..num_commitments {
                // To avoid duplicate note commitments, we use the `i` counter as the Rseed randomness
                let rseed = Rseed([i as u8; 32]);
                let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
                sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            }

            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            // Unregulated: BLACK_HOLE_ACK as ring_pk, GENERATOR as dk_pub
            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            // Create valid IMT non-membership proof for unregulated asset
            let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
                create_imt_non_membership_proof(value_to_send.asset_id.0);

            // Create valid user tree proof
            let (compliance_anchor, compliance_path, compliance_position) =
                create_user_tree_proof(&user_leaf);

            // Derive ACK and encrypt
            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor,
                balance_commitment,
                nullifier,
                rk,
                asset_anchor,
                compliance_anchor,
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };

            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path,
                asset_position,
                asset_indexed_leaf,
                is_regulated: false,
                compliance_path,
                compliance_position,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };

            // Tester random key (not used in circuit, just for test structure)
            let my_random_sk_scalar = Fr::from_le_bytes_mod_order(&tester_keys_rand);
            let my_fake_cvk_sk = my_random_sk_scalar;

            (public, private, my_fake_cvk_sk)
        }
    }

    prop_compose! {
        // Regulated strategy
        fn arb_regulated_spend_statement()(
            v_blinding in fr_strategy(),
            spend_auth_randomizer in fr_strategy(),
            asset_id64 in any::<u64>(),
            address_index in any::<u32>(),
            amount in any::<u64>(),
            seed_phrase_randomness in any::<[u8; 32]>(),
            rseed_randomness in any::<[u8; 32]>(),
            num_commitments in 0..100,
            cvk_sk_rand in any::<[u8; 32]>()
        ) -> (SpendProofPublic, SpendProofPrivate, Fr) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();
            for i in 0..num_commitments {
                let rseed = Rseed([i as u8; 32]);
                let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
                sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            }
            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            // Regulated: generate a random ring_sk and derive ring_pk = GENERATOR * ring_sk
            let ring_sk = Fr::from_le_bytes_mod_order(&cvk_sk_rand);
            let ring_pk = decaf377::Element::GENERATOR * ring_sk;
            let dk_pub = decaf377::Element::GENERATOR;

            // Create IMT proof with matching ring_pk (Policy-in-Leaf binds it into the leaf)
            let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
                create_imt_membership_proof(value_to_send.asset_id.0, ring_pk, dk_pub);

            // Derive ACK
            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                d,
            );

            // Create valid user tree proof
            let (compliance_anchor, compliance_path, compliance_position) =
                create_user_tree_proof(&user_leaf);

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            // Compute DLEQ proof using policy hashes from the indexed leaf
            let salt = decaf377::Fq::from(0u64);
            let target_timestamp = Fq::from(0u64);
            let dleq_k = Fr::rand(&mut rng);
            let metadata_hash = penumbra_sdk_compliance::compute_metadata_hash(
                asset_indexed_leaf.ring.policy_id_hash,
                asset_indexed_leaf.ring.resource_hash,
                asset_indexed_leaf.ring.permission_hash,
                Fq::from(1u64), target_timestamp, salt,
            );
            let epk_point = decaf377::Element::GENERATOR * ephemeral_secret;
            let dleq = penumbra_sdk_compliance::compute_dleq_native(
                ephemeral_secret, dleq_k, &ack, &epk_point, metadata_hash,
            );

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor,
                balance_commitment,
                nullifier,
                rk,
                asset_anchor,
                compliance_anchor,
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp,
                dleq_c: dleq.c,
                dleq_s: Fq::from_le_bytes_mod_order(&dleq.s.to_bytes()),
                sender_leaf_hash,
            };
            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path,
                asset_position,
                asset_indexed_leaf,
                is_regulated: true,
                compliance_path,
                compliance_position,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt,
            };
            (public, private, ring_sk)
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(1))]
        #[test]
        fn spend_proof_unregulated_asset((public, private, _) in arb_valid_spend_statement()) {
            assert!(check_satisfaction(&public, &private).is_ok());
            assert!(check_circuit_satisfaction(public, private).is_ok());
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(1))]
        #[test]
        /// Test that regulated spends (using real issuer key) also satisfy the circuit
        fn spend_proof_regulated_asset((public, private, _) in arb_regulated_spend_statement()) {
            assert!(check_satisfaction(&public, &private).is_ok());
            assert!(check_circuit_satisfaction(public, private).is_ok());
        }
    }

    #[cfg(feature = "bundled-proving-keys")]
    #[test]
    fn spend_proof_roundtrip_regulated() {
        use crate::test_proof_helpers::proof_test_helpers::{full_proof_roundtrip, CircuitType};
        full_proof_roundtrip(CircuitType::Spend, true);
    }

    #[cfg(feature = "bundled-proving-keys")]
    #[test]
    fn spend_proof_roundtrip_unregulated() {
        use crate::test_proof_helpers::proof_test_helpers::{full_proof_roundtrip, CircuitType};
        full_proof_roundtrip(CircuitType::Spend, false);
    }

    #[cfg(feature = "bundled-proving-keys")]
    #[test]
    fn spend_proof_plan_path_regulated() {
        use crate::test_proof_helpers::proof_test_helpers::test_spend_plan_path;
        test_spend_plan_path(true);
    }

    #[cfg(feature = "bundled-proving-keys")]
    #[test]
    fn spend_proof_plan_path_unregulated() {
        use crate::test_proof_helpers::proof_test_helpers::test_spend_plan_path;
        test_spend_plan_path(false);
    }

    prop_compose! {
        // This strategy generates a spend statement that uses a Merkle root
        // from prior to the note commitment being added to the SCT. The Merkle
        // path should not verify using this invalid root, and as such the circuit
        // should be unsatisfiable.
        fn arb_invalid_spend_statement_incorrect_anchor()(v_blinding in fr_strategy(), spend_auth_randomizer in fr_strategy(), asset_id64 in any::<u64>(), address_index in any::<u32>(), amount in any::<u64>(), seed_phrase_randomness in any::<[u8; 32]>(), rseed_randomness in any::<[u8; 32]>(), num_commitments in 0..100) -> (SpendProofPublic, SpendProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();

            // Next, we simulate the case where the SCT is not empty by adding `num_commitments`
            // unrelated items in the SCT.
            for i in 0..num_commitments {
                // To avoid duplicate note commitments, we use the `i` counter as the Rseed randomness
                let rseed = Rseed([i as u8; 32]);
                let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
                sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            }
            let incorrect_anchor = sct.root();

            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor: incorrect_anchor,
                balance_commitment,
                nullifier,
                rk,
                asset_anchor: tct::StateCommitment(Fq::from(0u64)),
                compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };

            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };
            (public, private)
        }
    }

    proptest! {
    #[test]
    /// Check that the `SpendCircuit` is not satisfied when using an incorrect
    /// TCT root (`anchor`).
    fn spend_proof_verification_merkle_path_integrity_failure((public, private) in arb_invalid_spend_statement_incorrect_anchor()) {
        assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
    }
    }

    prop_compose! {
        // Recall: The transmission key `pk_d` is derived as:
        //
        // `pk_d ​= [ivk] B_d`
        //
        // where `B_d` is the diversified basepoint and `ivk` is the incoming
        // viewing key.
        //
        // This strategy generates a spend statement that is spending a note
        // that corresponds to a diversified address associated with a different
        // IVK, i.e. the prover cannot demonstrate the transmission key `pk_d`
        // was derived as above and the circuit should be unsatisfiable.
        fn arb_invalid_spend_statement_diversified_address()(v_blinding in fr_strategy(), spend_auth_randomizer in fr_strategy(), asset_id64 in any::<u64>(), address_index in any::<u32>(), amount in any::<u64>(), seed_phrase_randomness in any::<[u8; 32]>(), incorrect_seed_phrase_randomness in any::<[u8; 32]>(), rseed_randomness in any::<[u8; 32]>()) -> (SpendProofPublic, SpendProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (_sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };

            let wrong_seed_phrase = SeedPhrase::from_randomness(&incorrect_seed_phrase_randomness);
            let wrong_sk_sender = SpendKey::from_seed_phrase_bip44(wrong_seed_phrase, &Bip44Path::new(0));
            let wrong_fvk_sender = wrong_sk_sender.full_viewing_key();
            let wrong_ivk_sender = wrong_fvk_sender.incoming();
            let (wrong_sender, _dtk_d) = wrong_ivk_sender.payment_address(address_index.into());

            let note = Note::from_parts(
                wrong_sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();
            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                wrong_sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let b_d_fq = wrong_sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &wrong_sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor,
                balance_commitment,
                nullifier,
                rk,
                asset_anchor: tct::StateCommitment(Fq::from(0u64)),
                compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };

            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };
            (public, private)
        }
    }

    proptest! {
        #[test]
        /// Check that the `SpendCircuit` is not satisfied when the diversified address is wrong.
        fn spend_proof_verification_diversified_address_integrity_failure((public, private) in arb_invalid_spend_statement_diversified_address()) {
            assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
        }
    }

    prop_compose! {
        // This strategy generates a spend statement that derives a nullifier
        // using a different position.
        fn arb_invalid_spend_statement_nullifier()(v_blinding in fr_strategy(), spend_auth_randomizer in fr_strategy(), asset_id64 in any::<u64>(), address_index in any::<u32>(), amount in any::<u64>(), seed_phrase_randomness in any::<[u8; 32]>(), rseed_randomness in any::<[u8; 32]>(), num_commitments in 0..100) -> (SpendProofPublic, SpendProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();

            // Next, we simulate the case where the SCT is not empty by adding `num_commitments`
            // unrelated items in the SCT.
            for i in 0..num_commitments {
                // To avoid duplicate note commitments, we use the `i` counter as the Rseed randomness
                let rseed = Rseed([i as u8; 32]);
                let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
                sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            }
            // Insert one more note commitment and witness it.
            let rseed = Rseed([num_commitments as u8; 32]);
            let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
            sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            let incorrect_position = sct.witness(dummy_note_commitment).expect("can witness note commitment").position();

            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let incorrect_nf = Nullifier::derive(&nk, incorrect_position, &note_commitment);

            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor,
                balance_commitment,
                nullifier: incorrect_nf,
                rk,
                asset_anchor: tct::StateCommitment(Fq::from(0u64)),
                compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };

            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };
            (public, private)
        }
    }

    proptest! {
        #[test]
        /// Check that the `SpendCircuit` is not satisfied, when using an
        /// incorrect nullifier.
        fn spend_proof_verification_nullifier_integrity_failure((public, private) in arb_invalid_spend_statement_nullifier()) {
            assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
        }
    }

    prop_compose! {
        // This statement uses a randomly generated incorrect value blinding factor for deriving the
        // balance commitment.
        fn arb_invalid_spend_statement_v_blinding_factor()(v_blinding in fr_strategy(), incorrect_v_blinding in fr_strategy(), spend_auth_randomizer in fr_strategy(), asset_id64 in any::<u64>(), address_index in any::<u32>(), amount in any::<u64>(), seed_phrase_randomness in any::<[u8; 32]>(), rseed_randomness in any::<[u8; 32]>(), num_commitments in 0..100) -> (SpendProofPublic, SpendProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();

            // Next, we simulate the case where the SCT is not empty by adding `num_commitments`
            // unrelated items in the SCT.
            for i in 0..num_commitments {
                // To avoid duplicate note commitments, we use the `i` counter as the Rseed randomness
                let rseed = Rseed([i as u8; 32]);
                let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
                sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            }

            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor,
                balance_commitment,
                nullifier,
                rk,
                asset_anchor: tct::StateCommitment(Fq::from(0u64)),
                compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };
            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding: incorrect_v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };
            (public, private)
        }
    }

    proptest! {
        #[test]
        /// Check that the `SpendCircuit` is not satisfied when using balance
        /// commitments with different blinding factors.
        fn spend_proof_verification_balance_commitment_integrity_failure((public, private) in arb_invalid_spend_statement_v_blinding_factor()) {
            assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
        }
    }

    prop_compose! {
        // This statement uses a randomly generated incorrect spend auth randomizer for deriving the
        // randomized verification key.
        fn arb_invalid_spend_statement_rk_integrity()(v_blinding in fr_strategy(), spend_auth_randomizer in fr_strategy(), asset_id64 in any::<u64>(), address_index in any::<u32>(), amount in any::<u64>(), seed_phrase_randomness in any::<[u8; 32]>(), rseed_randomness in any::<[u8; 32]>(), num_commitments in 0..100, incorrect_spend_auth_randomizer in fr_strategy()) -> (SpendProofPublic, SpendProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();

            // Next, we simulate the case where the SCT is not empty by adding `num_commitments`
            // unrelated items in the SCT.
            for i in 0..num_commitments {
                // To avoid duplicate note commitments, we use the `i` counter as the Rseed randomness
                let rseed = Rseed([i as u8; 32]);
                let dummy_note_commitment = Note::from_parts(sender.clone(), value_to_send, rseed).expect("can create note").commit();
                sct.insert(tct::Witness::Keep, dummy_note_commitment).expect("should be able to insert note commitments into the SCT");
            }

            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            let incorrect_rsk = sk_sender
                .spend_auth_key()
                .randomize(&incorrect_spend_auth_randomizer);
            let incorrect_rk: VerificationKey<SpendAuth> = incorrect_rsk.into();

            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            let public = SpendProofPublic {
                anchor,
                balance_commitment,
                nullifier,
                rk: incorrect_rk,
                asset_anchor: tct::StateCommitment(Fq::from(0u64)),
                compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };

            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };
            (public, private)
        }
    }

    proptest! {
        #[test]
        /// Check that the `SpendCircuit` is not satisfied when the incorrect randomizable verification key is used.
        fn spend_proof_verification_fails_rk_integrity((public, private) in arb_invalid_spend_statement_rk_integrity()) {
            assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
        }
    }

    prop_compose! {
        fn arb_valid_dummy_spend_statement()(v_blinding in fr_strategy(), spend_auth_randomizer in fr_strategy(), asset_id64 in any::<u64>(), address_index in any::<u32>(), seed_phrase_randomness in any::<[u8; 32]>(), rseed_randomness in any::<[u8; 32]>()) -> (SpendProofPublic, SpendProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (sender, _dtk_d) = ivk_sender.payment_address(address_index.into());
            let value_to_send = Value {
                amount: Amount::from(0u64),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                sender.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
            let nk = *sk_sender.nullifier_key();
            let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();

            let mut sct = tct::Tree::new();
            sct.insert(tct::Witness::Keep, note_commitment).expect("should be able to insert note commitments into the SCT");

            let state_commitment_proof = sct.witness(note_commitment).expect("can witness note commitment");
            let balance_commitment = value_to_send.commit(v_blinding);
            let rk: VerificationKey<SpendAuth> = rsk.into();
            let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

            // use an invalid anchor to verify that the circuit skips inclusion checks for dummy
            // spends
            let invalid_anchor = tct::Tree::new().root();

            let ring_pk = *penumbra_sdk_compliance::BLACK_HOLE_ACK;
            let dk_pub = decaf377::Element::GENERATOR;
            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let b_d_fq = sender.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack = ring_pk * d_fr;

            let mut rng = rand::thread_rng();
            let encryption_result = penumbra_sdk_compliance::crypto::encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &sender,
                note.asset_id(),
                note.amount(),
                false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt compliance details");

            let (epk, c2_core, packed_ciphertext) = encryption_result.ciphertext.to_spend_circuit_public_inputs();
            let ephemeral_secret = encryption_result.r_s;

            let tx_blinding_nonce = Fr::from(0u64);
            let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), tx_blinding_nonce);

            // Create valid IMT non-membership proof for unregulated asset
            let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
                create_imt_non_membership_proof(value_to_send.asset_id.0);

            // Create valid user tree proof
            let (compliance_anchor, compliance_path, compliance_position) =
                create_user_tree_proof(&user_leaf);

            let public = SpendProofPublic {
                anchor: invalid_anchor,
                balance_commitment,
                nullifier,
                rk,
                asset_anchor,
                compliance_anchor,
                epk,
                c2_core,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c: Fq::from(0u64),
                dleq_s: Fq::from(0u64),
                sender_leaf_hash,
            };

            let private = SpendProofPrivate {
                state_commitment_proof,
                note,
                v_blinding,
                spend_auth_randomizer,
                ak,
                nk,
                asset_path,
                asset_position,
                asset_indexed_leaf,
                is_regulated: false,
                compliance_path,
                compliance_position,
                user_leaf,
                compliance_ephemeral_secret: ephemeral_secret,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };
            (public, private)
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(1))]
        #[test]
        /// Check that the `SpendCircuit` is always satisfied for dummy (zero value) spends.
        fn spend_proof_dummy_verification_suceeds((public, private) in arb_valid_dummy_spend_statement()) {
            assert!(check_satisfaction(&public, &private).is_ok());
            assert!(check_circuit_satisfaction(public, private).is_ok());
        }
    }

    struct MerkleProofCircuit {
        /// Witness: Inclusion proof for the note commitment.
        state_commitment_proof: tct::Proof,
        /// Public input: The merkle root of the state commitment tree
        pub anchor: tct::Root,
        pub epoch: Fq,
        pub block: Fq,
        pub commitment_index: Fq,
    }

    impl ConstraintSynthesizer<Fq> for MerkleProofCircuit {
        fn generate_constraints(
            self,
            cs: ConstraintSystemRef<Fq>,
        ) -> ark_relations::r1cs::Result<()> {
            // public inputs
            let anchor_var = FqVar::new_input(cs.clone(), || Ok(Fq::from(self.anchor)))?;
            let epoch_var = FqVar::new_input(cs.clone(), || Ok(self.epoch))?;
            let block_var = FqVar::new_input(cs.clone(), || Ok(self.block))?;
            let commitment_index_var = FqVar::new_input(cs.clone(), || Ok(self.commitment_index))?;

            // witnesses
            let merkle_path_var = tct::r1cs::MerkleAuthPathVar::new_witness(cs.clone(), || {
                Ok(self.state_commitment_proof.clone())
            })?;
            let claimed_note_commitment = StateCommitmentVar::new_witness(cs.clone(), || {
                Ok(self.state_commitment_proof.commitment())
            })?;
            let position_var = tct::r1cs::PositionVar::new_witness(cs.clone(), || {
                Ok(self.state_commitment_proof.position())
            })?;
            let position_bits = position_var.to_bits_le()?;
            merkle_path_var.verify(
                cs,
                &Boolean::TRUE,
                &position_bits,
                anchor_var,
                claimed_note_commitment.inner(),
            )?;

            // Now also verify the commitment index, block, and epoch numbers are all valid. This is not necessary
            // for Merkle proofs in general, but is here to ensure this code is exercised in tests.
            let computed_epoch = position_var.epoch()?;
            let computed_block = position_var.block()?;
            let computed_commitment_index = position_var.commitment()?;
            computed_epoch.enforce_equal(&epoch_var)?;
            computed_block.enforce_equal(&block_var)?;
            computed_commitment_index.enforce_equal(&commitment_index_var)?;
            Ok(())
        }
    }

    impl DummyWitness for MerkleProofCircuit {
        fn with_dummy_witness() -> Self {
            let seed_phrase = SeedPhrase::from_randomness(&[b'f'; 32]);
            let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_sender = sk_sender.full_viewing_key();
            let ivk_sender = fvk_sender.incoming();
            let (address, _dtk_d) = ivk_sender.payment_address(0u32.into());

            let note = Note::from_parts(
                address,
                Value::from_str("1upenumbra").expect("valid value"),
                Rseed([1u8; 32]),
            )
            .expect("can make a note");
            let mut sct = tct::Tree::new();
            let note_commitment = note.commit();
            sct.insert(tct::Witness::Keep, note_commitment)
                .expect("able to insert note commitment into SCT");
            let anchor = sct.root();
            let state_commitment_proof = sct
                .witness(note_commitment)
                .expect("able to witness just-inserted note commitment");
            let position = state_commitment_proof.position();
            let epoch = Fq::from(position.epoch());
            let block = Fq::from(position.block());
            let commitment_index = Fq::from(position.commitment());

            Self {
                state_commitment_proof,
                anchor,
                epoch,
                block,
                commitment_index,
            }
        }
    }

    fn make_random_note_commitment(address: Address) -> StateCommitment {
        let note = Note::from_parts(
            address,
            Value::from_str("1upenumbra").expect("valid value"),
            Rseed([1u8; 32]),
        )
        .expect("can make a note");
        note.commit()
    }

    #[test]
    fn merkle_proof_verification_succeeds() {
        let mut rng = OsRng;
        let (pk, vk) = generate_prepared_test_parameters::<MerkleProofCircuit>(&mut rng);

        let seed_phrase = SeedPhrase::from_randomness(&[b'f'; 32]);
        let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk_sender = sk_sender.full_viewing_key();
        let ivk_sender = fvk_sender.incoming();
        let (address, _dtk_d) = ivk_sender.payment_address(0u32.into());
        // We will incrementally add notes to the state commitment tree, checking the merkle proofs verify
        // at each step.
        let mut sct = tct::Tree::new();

        for _ in 0..5 {
            let note_commitment = make_random_note_commitment(address.clone());
            sct.insert(tct::Witness::Keep, note_commitment).unwrap();
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).unwrap();
            let position = state_commitment_proof.position();
            let epoch = Fq::from(position.epoch());
            let block = Fq::from(position.block());
            let commitment_index = Fq::from(position.commitment());
            let circuit = MerkleProofCircuit {
                state_commitment_proof,
                anchor,
                epoch,
                block,
                commitment_index,
            };
            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                .expect("should be able to form proof");

            let proof_result = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                &vk,
                &[Fq::from(anchor), epoch, block, commitment_index],
                &proof,
            );
            assert!(proof_result.is_ok());
        }

        sct.end_block().expect("can end block");
        for _ in 0..100 {
            let note_commitment = make_random_note_commitment(address.clone());
            sct.insert(tct::Witness::Forget, note_commitment).unwrap();
        }

        for _ in 0..5 {
            let note_commitment = make_random_note_commitment(address.clone());
            sct.insert(tct::Witness::Keep, note_commitment).unwrap();
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).unwrap();
            let position = state_commitment_proof.position();
            let epoch = Fq::from(position.epoch());
            let block = Fq::from(position.block());
            let commitment_index = Fq::from(position.commitment());
            let circuit = MerkleProofCircuit {
                state_commitment_proof,
                anchor,
                epoch,
                block,
                commitment_index,
            };
            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                .expect("should be able to form proof");

            let proof_result = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                &vk,
                &[Fq::from(anchor), epoch, block, commitment_index],
                &proof,
            );
            assert!(proof_result.is_ok());
        }

        sct.end_epoch().expect("can end epoch");
        for _ in 0..100 {
            let note_commitment = make_random_note_commitment(address.clone());
            sct.insert(tct::Witness::Forget, note_commitment).unwrap();
        }

        for _ in 0..5 {
            let note_commitment = make_random_note_commitment(address.clone());
            sct.insert(tct::Witness::Keep, note_commitment).unwrap();
            let anchor = sct.root();
            let state_commitment_proof = sct.witness(note_commitment).unwrap();
            let position = state_commitment_proof.position();
            let epoch = Fq::from(position.epoch());
            let block = Fq::from(position.block());
            let commitment_index = Fq::from(position.commitment());
            let circuit = MerkleProofCircuit {
                state_commitment_proof,
                anchor,
                epoch,
                block,
                commitment_index,
            };
            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                .expect("should be able to form proof");

            let proof_result = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                &vk,
                &[Fq::from(anchor), epoch, block, commitment_index],
                &proof,
            );
            assert!(proof_result.is_ok());
        }
    }

    #[test]
    fn point_compression_roundtrip_for_ecdh() {
        // Verify that compress_to_field() used in ECDH is deterministic and doesn't lose information
        let cvk_sk = Fr::from(999u64);
        let cvk_pk = decaf377::Element::GENERATOR * cvk_sk;

        let ephemeral_sk = Fr::from(12345u64);
        let ephemeral_pk = decaf377::Element::GENERATOR * ephemeral_sk;

        // Prover side: shared_secret = cvk_pk * ephemeral_sk
        let shared_secret_prover = cvk_pk * ephemeral_sk;
        let compressed_prover = shared_secret_prover.vartime_compress_to_field();

        // Issuer side: shared_secret = ephemeral_pk * cvk_sk
        let shared_secret_issuer = ephemeral_pk * cvk_sk;
        let compressed_issuer = shared_secret_issuer.vartime_compress_to_field();

        // Both should produce identical compressed field elements
        assert_eq!(
            compressed_prover, compressed_issuer,
            "ECDH shared secrets must match"
        );

        // Verify compression is deterministic
        let compressed_again = shared_secret_prover.vartime_compress_to_field();
        assert_eq!(
            compressed_prover, compressed_again,
            "Compression must be deterministic"
        );
    }

    /// Compare the canonical gnark path against the legacy Arkworks path for a spend proof.
    ///
    /// Requires `libpenumbra_gnark_spend.{so,dylib}` built in `tools/gnark/`
    /// (`go build -buildmode=c-shared -o libpenumbra_gnark_spend.dylib ./cmd/spendlib`).
    #[cfg(any(unix, windows))]
    #[test]
    #[ignore = "perf: requires libpenumbra_gnark_spend built in tools/gnark/"]
    fn spend_gnark_vs_arkworks_timing() {
        use crate::spend::{SpendCircuit, SpendProofPrivate, SpendProofPublic};
        use crate::test_proof_helpers::proof_test_helpers::{
            generate_test_data, setup_arkworks_groth16_keys, CircuitType, REGULATED_ASSET_ID,
        };
        use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
        use decaf377::{Fq, Fr};
        use penumbra_sdk_asset::Balance;
        use penumbra_sdk_sct::Nullifier;
        use std::time::Instant;

        let mut rng = rand::thread_rng();
        let test_data =
            generate_test_data(&mut rng, REGULATED_ASSET_ID, 100, true, CircuitType::Spend);

        let mut sct = penumbra_sdk_tct::Tree::new();
        let note_commitment = test_data.note.commit();
        sct.insert(penumbra_sdk_tct::Witness::Keep, note_commitment)
            .unwrap();
        let anchor = sct.root();
        let state_commitment_proof = sct.witness(note_commitment).unwrap();

        let balance_commitment = Balance::from(test_data.value).commit(test_data.balance_blinding);
        let nullifier = Nullifier::derive(
            test_data.fvk.nullifier_key(),
            state_commitment_proof.position(),
            &note_commitment,
        );
        let randomizer = Fr::rand(&mut rng);
        let rk = test_data
            .fvk
            .spend_verification_key()
            .randomize(&randomizer);
        let dummy_nonce = Fr::from(0u64);
        let sender_leaf_hash =
            penumbra_sdk_compliance::blind_sender_leaf(test_data.user_leaf.commit(), dummy_nonce);

        let public = SpendProofPublic {
            anchor,
            balance_commitment,
            nullifier,
            rk,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            epk: test_data.epk_1,
            c2_core: test_data.c2_core,
            compliance_ciphertext: test_data.compliance_ciphertext,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c: test_data.dleq_c,
            dleq_s: test_data.dleq_s,
            sender_leaf_hash,
        };
        let private = SpendProofPrivate {
            state_commitment_proof,
            note: test_data.note,
            v_blinding: test_data.balance_blinding,
            spend_auth_randomizer: randomizer,
            ak: *test_data.fvk.spend_verification_key(),
            nk: *test_data.fvk.nullifier_key(),
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated: true,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            tx_blinding_nonce: dummy_nonce,
            is_flagged: false,
            salt: test_data.salt,
        };

        // --- Arkworks prove ---
        let (pk, ark_pvk, blinding_r, blinding_s) = setup_arkworks_groth16_keys::<SpendCircuit>();
        let claimed_hash =
            crate::public_input_hash::spend_statement_hash_from_public(&public).unwrap();
        let circuit = SpendCircuit {
            public: public.clone(),
            private: private.clone(),
            claimed_statement_hash: claimed_hash,
        };
        let t0 = Instant::now();
        let ark_raw_proof =
            Groth16::<decaf377::Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
                circuit, &pk, blinding_r, blinding_s,
            )
            .expect("arkworks prove");
        let ark_prove_ms = t0.elapsed().as_millis();
        // Wrap in SpendProof (compressed) — same as production output.
        use ark_serialize::CanonicalSerialize;
        let mut ark_proof_bytes = Vec::new();
        ark_raw_proof
            .serialize_compressed(&mut ark_proof_bytes)
            .expect("compress arkworks proof");
        let ark_spend_proof = SpendProof(ark_proof_bytes.try_into().expect("proof is 192 bytes"));

        let gnark_pvk = &*penumbra_sdk_proof_params::SPEND_PROOF_VERIFICATION_KEY;

        let t1 = Instant::now();
        ark_spend_proof
            .verify(&ark_pvk, public.clone())
            .expect("arkworks verify");
        let ark_verify_us = t1.elapsed().as_micros();

        // --- gnark prove ---
        let lib_path = crate::gnark::GnarkSpendClient::auto_lib_path()
            .expect("build libpenumbra_gnark_spend in tools/gnark/ first");
        let artifact_dir = auto_gnark_artifact_dir("spend")
            .expect("could not locate crates/crypto/proof-params/src/gen/gnark/spend/");
        let gnark_client = crate::gnark::GnarkSpendClient::load_library(&lib_path, &artifact_dir)
            .expect("gnark spend client");
        let t2 = Instant::now();
        let gnark_proof = gnark_client.prove(&public, &private).expect("gnark prove");
        let gnark_prove_ms = t2.elapsed().as_millis();

        let t3 = Instant::now();
        gnark_proof.verify(gnark_pvk, public).expect("gnark verify");
        let gnark_verify_us = t3.elapsed().as_micros();

        println!("\n=== Spend proof timing ===");
        println!("{:<10} {:>10} {:>12}", "path", "prove ms", "verify µs");
        println!(
            "{:<10} {:>10} {:>12}",
            "arkworks", ark_prove_ms, ark_verify_us
        );
        println!(
            "{:<10} {:>10} {:>12}",
            "gnark", gnark_prove_ms, gnark_verify_us
        );
    }

    /// Locates `crates/crypto/proof-params/src/gen/gnark/<circuit>/` by walking
    /// ancestors of the current executable until the workspace root is found.
    fn auto_gnark_artifact_dir(circuit: &str) -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let mut dir = exe.parent()?;
        loop {
            let candidate = dir
                .join("crates/crypto/proof-params/src/gen/gnark")
                .join(circuit);
            if candidate.is_dir() {
                return Some(candidate);
            }
            dir = dir.parent()?;
        }
    }
}
