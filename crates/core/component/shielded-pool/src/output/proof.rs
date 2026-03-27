use base64::prelude::*;
use std::str::FromStr;

use anyhow::Result;
use ark_groth16::r1cs_to_qap::LibsnarkReduction;
use ark_r1cs_std::uint8::UInt8;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_fmd as fmd;
use decaf377_ka as ka;

use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, ProvingKey};
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef};
use ark_snark::SNARK;
use penumbra_sdk_keys::{keys::Diversifier, Address};
use penumbra_sdk_proto::{penumbra::core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_tct::r1cs::StateCommitmentVar;
use penumbra_sdk_tct::StateCommitment;

use crate::{note, Note, Rseed};
use decaf377::r1cs::{ElementVar, FqVar};
use penumbra_sdk_asset::{balance, balance::BalanceVar, Value};
use penumbra_sdk_compliance::r1cs::{verify_compliance_integrity, ComplianceWitness};
use penumbra_sdk_proof_params::{DummyWitness, VerifyingKeyExt, GROTH16_PROOF_LENGTH_BYTES};

use crate::public_input_hash::{
    output_statement_hash_from_public, output_statement_hash_var, OUTPUT_STATEMENT_FIELD_COUNT,
};

/// The public input for an [`OutputProof`].
#[derive(Clone, Debug)]
pub struct OutputProofPublic {
    /// A hiding commitment to the balance.
    pub balance_commitment: balance::Commitment,
    /// A hiding commitment to the note.
    pub note_commitment: note::StateCommitment,
    /// EPK 1: r_1 × G (core tier)
    pub epk_1: decaf377::Element,
    /// EPK 2: r_2 × G (ext tier)
    pub epk_2: decaf377::Element,
    /// EPK 3: r_3 × G (sext tier)
    pub epk_3: decaf377::Element,
    /// ElGamal C2 for core tier
    pub c2_core: Fq,
    /// ElGamal C2 for ext tier
    pub c2_ext: Fq,
    /// ElGamal C2 for sext tier
    pub c2_sext: Fq,
    /// Packed ciphertext: 11 Fqs [detection:2, core:3, ext:3, sext:3]
    pub compliance_ciphertext: Vec<Fq>,
    /// DLEQ target timestamp (Unix UTC seconds, encoded as Fq for circuit)
    pub target_timestamp: Fq,
    /// DLEQ proofs for 3 tiers: core (c_1, s_1), ext (c_2, s_2), sext (c_3, s_3)
    pub dleq_c_1: Fq,
    pub dleq_s_1: Fq,
    pub dleq_c_2: Fq,
    pub dleq_s_2: Fq,
    pub dleq_c_3: Fq,
    pub dleq_s_3: Fq,
    /// Asset registry Merkle root
    pub asset_anchor: StateCommitment,
    /// User compliance registry Merkle root
    pub compliance_anchor: StateCommitment,
    /// Blinded hash of the counterparty's (sender's) compliance leaf.
    /// Checked against spend's `sender_leaf_hash` by `validate_spend_output_binding()`.
    pub counterparty_leaf_hash: StateCommitment,
}

/// The private input for an [`OutputProof`].
#[derive(Clone, Debug)]
pub struct OutputProofPrivate {
    /// The note being created.
    pub note: Note,
    /// A blinding factor to hide the balance of the transaction.
    pub balance_blinding: Fr,
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
    /// Ephemeral secret r_1 used to encrypt the compliance ciphertext (core tier)
    pub compliance_ephemeral_secret: Fr,
    /// Additional ephemeral secrets for output (r_2, r_3)
    pub r_2: Fr,
    pub r_3: Fr,
    /// The counterparty's (sender's) compliance leaf
    pub counterparty_leaf: penumbra_sdk_compliance::ComplianceLeaf,
    /// Shared transaction blinding nonce (same for spend and output in one transaction)
    pub tx_blinding_nonce: Fr,
    /// Whether this output is flagged (amount >= threshold).
    pub is_flagged: bool,
    /// Random salt for DLEQ metadata hash (encrypted in detection tier)
    pub salt: decaf377::Fq,
}

#[cfg(test)]
fn check_satisfaction(public: &OutputProofPublic, private: &OutputProofPrivate) -> Result<()> {
    use penumbra_sdk_asset::Balance;

    if private.note.diversified_generator() == decaf377::Element::default() {
        anyhow::bail!("diversified generator is identity");
    }

    let balance_commitment =
        (-Balance::from(private.note.value())).commit(private.balance_blinding);
    if balance_commitment != public.balance_commitment {
        anyhow::bail!("balance commitment did not match public input");
    }

    let note_commitment = private.note.commit();
    if note_commitment != public.note_commitment {
        anyhow::bail!("note commitment did not match public input");
    }

    Ok(())
}

#[cfg(test)]
fn check_circuit_satisfaction(
    public: OutputProofPublic,
    private: OutputProofPrivate,
) -> Result<()> {
    use ark_relations::r1cs::{self, ConstraintSystem};

    let cs = ConstraintSystem::new_ref();
    let claimed_statement_hash = output_statement_hash_from_public(&public)
        .map_err(|e| anyhow::anyhow!("failed to compute output statement hash: {e}"))?;
    let circuit = OutputCircuit {
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

/// Groth16 circuit for creating new notes with compliance binding.
#[derive(Clone, Debug)]
pub struct OutputCircuit {
    public: OutputProofPublic,
    private: OutputProofPrivate,
    claimed_statement_hash: Fq,
}

impl OutputCircuit {
    #[cfg(feature = "benchmark-helpers")]
    pub fn from_parts(
        public: OutputProofPublic,
        private: OutputProofPrivate,
        claimed_statement_hash: Fq,
    ) -> Self {
        Self {
            public,
            private,
            claimed_statement_hash,
        }
    }

    fn new(
        public: OutputProofPublic,
        private: OutputProofPrivate,
        claimed_statement_hash: Fq,
    ) -> Self {
        Self {
            public,
            private,
            claimed_statement_hash,
        }
    }

    pub fn into_parts(self) -> (OutputProofPublic, OutputProofPrivate, Fq) {
        (self.public, self.private, self.claimed_statement_hash)
    }
}

impl ConstraintSynthesizer<Fq> for OutputCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> ark_relations::r1cs::Result<()> {
        // Witnesses
        // Note: In the allocation of the address on `NoteVar`, we check the diversified base is not identity.
        let note_var = note::NoteVar::new_witness(cs.clone(), || Ok(self.private.note.clone()))?;
        let balance_blinding_arr: [u8; 32] = self.private.balance_blinding.to_bytes();
        let balance_blinding_vars = UInt8::new_witness_vec(cs.clone(), &balance_blinding_arr)?;

        // Witness statement fields (single public input is the statement hash).
        let claimed_note_commitment =
            StateCommitmentVar::new_witness(cs.clone(), || Ok(self.public.note_commitment))?;
        let claimed_balance_commitment =
            ElementVar::new_witness(cs.clone(), || Ok(self.public.balance_commitment.0))?;
        let claimed_asset_anchor =
            StateCommitmentVar::new_witness(cs.clone(), || Ok(self.public.asset_anchor))?;
        let claimed_compliance_anchor =
            StateCommitmentVar::new_witness(cs.clone(), || Ok(self.public.compliance_anchor))?;

        // Check integrity of balance commitment (negative for outputs).
        let balance_commitment =
            BalanceVar::from_negative_value_var(note_var.value()).commit(balance_blinding_vars)?;
        balance_commitment
            .inner
            .enforce_equal(&claimed_balance_commitment)?;

        // Check note commitment integrity.
        let note_commitment = note_var.commit()?;
        note_commitment.enforce_equal(&claimed_note_commitment)?;

        let epk_1_var = ElementVar::new_witness(cs.clone(), || Ok(self.public.epk_1))?;
        let epk_2_var = ElementVar::new_witness(cs.clone(), || Ok(self.public.epk_2))?;
        let epk_3_var = ElementVar::new_witness(cs.clone(), || Ok(self.public.epk_3))?;

        let c2_core_var = FqVar::new_witness(cs.clone(), || Ok(self.public.c2_core))?;
        let c2_ext_var = FqVar::new_witness(cs.clone(), || Ok(self.public.c2_ext))?;
        let c2_sext_var = FqVar::new_witness(cs.clone(), || Ok(self.public.c2_sext))?;

        let mut ciphertext_vars = Vec::new();
        for fq in self.public.compliance_ciphertext.iter() {
            ciphertext_vars.push(FqVar::new_witness(cs.clone(), || Ok(*fq))?);
        }

        let target_timestamp_var =
            FqVar::new_witness(cs.clone(), || Ok(self.public.target_timestamp))?;
        let dleq_c_1_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_c_1))?;
        let dleq_s_1_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_s_1))?;
        let dleq_c_2_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_c_2))?;
        let dleq_s_2_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_s_2))?;
        let dleq_c_3_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_c_3))?;
        let dleq_s_3_var = FqVar::new_witness(cs.clone(), || Ok(self.public.dleq_s_3))?;

        let claimed_statement_hash_var =
            FqVar::new_input(cs.clone(), || Ok(self.claimed_statement_hash))?;

        let counterparty_leaf_var =
            penumbra_sdk_compliance::r1cs::ComplianceLeafVar::new_witness(cs.clone(), || {
                Ok(self.private.counterparty_leaf.clone())
            })?;

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

        let _esk_bits_vars = verify_compliance_integrity(
            cs.clone(),
            claimed_asset_anchor.inner().clone(),
            claimed_compliance_anchor.inner().clone(),
            epk_1_var.clone(),
            epk_2_var.clone(),
            epk_3_var.clone(),
            c2_core_var.clone(),
            c2_ext_var.clone(),
            c2_sext_var.clone(),
            ciphertext_vars.clone(),
            target_timestamp_var.clone(),
            dleq_c_1_var.clone(),
            dleq_s_1_var.clone(),
            dleq_c_2_var.clone(),
            dleq_s_2_var.clone(),
            dleq_c_3_var.clone(),
            dleq_s_3_var.clone(),
            note_var.asset_id(),
            note_var.amount(),
            note_var.diversified_generator(),
            note_var.transmission_key(),
            counterparty_leaf_var.address.diversified_generator.clone(),
            counterparty_leaf_var.address.transmission_key.clone(),
            self.private.compliance_ephemeral_secret,
            self.private.r_2,
            self.private.r_3,
            counterparty_leaf_var.d.clone(),
            compliance_witness,
        )?;

        let tx_blinding_nonce_var = FqVar::new_witness(cs.clone(), || {
            Ok(Fq::from_le_bytes_mod_order(
                &self.private.tx_blinding_nonce.to_bytes(),
            ))
        })?;

        // Counterparty leaf binding: proves the counterparty in this output matches
        // the sender from the spend proof (via validate_spend_output_binding).
        let counterparty_leaf_hash = counterparty_leaf_var.commit(cs.clone())?;
        let computed_blinded_counterparty =
            penumbra_sdk_compliance::leaf_binding::r1cs::blind_sender_leaf(
                cs.clone(),
                counterparty_leaf_hash,
                tx_blinding_nonce_var,
            )?;
        let claimed_blinded_counterparty =
            FqVar::new_witness(cs.clone(), || Ok(self.public.counterparty_leaf_hash.0))?;
        computed_blinded_counterparty.enforce_equal(&claimed_blinded_counterparty)?;

        // Bind all statement fields to a single public input hash.
        let mut statement_fields = Vec::with_capacity(OUTPUT_STATEMENT_FIELD_COUNT);
        statement_fields.push(claimed_note_commitment.inner());
        statement_fields.push(claimed_balance_commitment.compress_to_field()?);
        statement_fields.push(claimed_asset_anchor.inner());
        statement_fields.push(claimed_compliance_anchor.inner());
        statement_fields.push(epk_1_var.compress_to_field()?);
        statement_fields.push(epk_2_var.compress_to_field()?);
        statement_fields.push(epk_3_var.compress_to_field()?);
        statement_fields.push(c2_core_var);
        statement_fields.push(c2_ext_var);
        statement_fields.push(c2_sext_var);
        statement_fields.extend(ciphertext_vars);
        statement_fields.push(target_timestamp_var);
        statement_fields.push(dleq_c_1_var);
        statement_fields.push(dleq_s_1_var);
        statement_fields.push(dleq_c_2_var);
        statement_fields.push(dleq_s_2_var);
        statement_fields.push(dleq_c_3_var);
        statement_fields.push(dleq_s_3_var);
        statement_fields.push(claimed_blinded_counterparty);

        let computed_statement_hash = output_statement_hash_var(cs.clone(), &statement_fields)?;
        computed_statement_hash.enforce_equal(&claimed_statement_hash_var)?;

        Ok(())
    }
}

impl DummyWitness for OutputCircuit {
    fn with_dummy_witness() -> Self {
        use penumbra_sdk_compliance::crypto::encrypt_output;

        let diversifier_bytes = [1u8; 16];
        let pk_d_bytes = decaf377::Element::GENERATOR.vartime_compress().0;
        let clue_key_bytes = [1; 32];
        let diversifier = Diversifier(diversifier_bytes);
        let address = Address::from_components(
            diversifier,
            ka::Public(pk_d_bytes),
            fmd::ClueKey(clue_key_bytes),
        )
        .expect("generated 1 address");
        let note = Note::from_parts(
            address.clone(),
            Value::from_str("1upenumbra").expect("valid value"),
            Rseed([1u8; 32]),
        )
        .expect("can make a note");
        let balance_blinding = Fr::from(1u64);

        let dk_pub = decaf377::Element::GENERATOR;

        // Unregulated dummy: circuit uses BLACK_HOLE_ACK directly
        let ack = *penumbra_sdk_compliance::BLACK_HOLE_ACK;

        let mut rng = rand::thread_rng();
        let encryption_result = encrypt_output(
            &mut rng,
            &ack,
            &ack,
            &dk_pub,
            &address,
            &address, // recipient = sender for dummy
            note.asset_id(),
            note.amount(),
            false,
            Fq::from(0u64),
        )
        .expect("can encrypt");

        let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
            encryption_result
                .ciphertext
                .to_output_circuit_public_inputs();

        let asset_anchor = penumbra_sdk_tct::StateCommitment(Fq::from(0u64));
        let compliance_anchor = penumbra_sdk_tct::StateCommitment(Fq::from(0u64));

        let dummy_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            address.clone(),
            penumbra_sdk_asset::asset::Id(Fq::from(0u64)),
            Fq::from(0u64),
        );
        let counterparty_leaf_hash =
            penumbra_sdk_compliance::blind_sender_leaf(dummy_leaf.commit(), Fr::from(0u64));

        let public = OutputProofPublic {
            note_commitment: note.commit(),
            balance_commitment: balance::Commitment(decaf377::Element::GENERATOR),
            epk_1,
            epk_2,
            epk_3,
            c2_core,
            c2_ext,
            c2_sext,
            compliance_ciphertext,
            target_timestamp: Fq::from(0u64),
            dleq_c_1: Fq::from(0u64),
            dleq_s_1: Fq::from(0u64),
            dleq_c_2: Fq::from(0u64),
            dleq_s_2: Fq::from(0u64),
            dleq_c_3: Fq::from(0u64),
            dleq_s_3: Fq::from(0u64),
            asset_anchor,
            compliance_anchor,
            counterparty_leaf_hash,
        };
        let private = OutputProofPrivate {
            note,
            balance_blinding,
            asset_path: penumbra_sdk_compliance::MerklePath::default(),
            asset_position: 0,
            asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                Fq::from(0u64),
                0,
                penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
            ),
            is_regulated: false,
            compliance_path: penumbra_sdk_compliance::MerklePath::default(),
            compliance_position: 0,
            user_leaf: penumbra_sdk_compliance::ComplianceLeaf::new(
                address,
                penumbra_sdk_asset::asset::Id(Fq::from(0u64)),
                Fq::from(0u64),
            ),
            compliance_ephemeral_secret: encryption_result.r_1,
            r_2: encryption_result.r_2,
            r_3: encryption_result.r_3,
            counterparty_leaf: dummy_leaf,
            tx_blinding_nonce: Fr::from(0u64),
            is_flagged: false,
            salt: decaf377::Fq::from(0u64),
        };
        let claimed_statement_hash = output_statement_hash_from_public(&public)
            .expect("dummy output statement hash should compute");
        OutputCircuit {
            public,
            private,
            claimed_statement_hash,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputProof([u8; GROTH16_PROOF_LENGTH_BYTES]);

impl OutputProof {
    #![allow(clippy::too_many_arguments)]
    pub fn prove(
        blinding_r: Fq,
        blinding_s: Fq,
        pk: &ProvingKey<Bls12_377>,
        public: OutputProofPublic,
        private: OutputProofPrivate,
    ) -> Result<Self, crate::ProofError> {
        let claimed_statement_hash = output_statement_hash_from_public(&public)
            .map_err(|e| crate::ProofError::InvalidPublicInput(format!("statement hash: {e}")))?;
        let circuit = OutputCircuit::new(public, private, claimed_statement_hash);

        #[cfg(debug_assertions)]
        {
            // In debug builds, preflight constraint satisfaction to aid diagnosis.
            use ark_relations::r1cs::ConstraintSystem;
            let cs = ConstraintSystem::<Fq>::new_ref();
            circuit.clone().generate_constraints(cs.clone())?;

            if !cs.is_satisfied().unwrap_or(false) {
                let unsatisfied = cs
                    .which_is_unsatisfied()
                    .map(|opt| {
                        opt.map(|s| format!(" Failing constraint: {}", s))
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                tracing::error!(
                    "Output circuit: {} constraints, {} instance vars, {} witness vars.{}",
                    cs.num_constraints(),
                    cs.num_instance_variables(),
                    cs.num_witness_variables(),
                    unsatisfied,
                );
                return Err(crate::ProofError::UnsatisfiedConstraints(format!(
                    "Output circuit constraints not satisfied.{}",
                    unsatisfied,
                )));
            }
        }

        let proof = Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(
            circuit, pk, blinding_r, blinding_s,
        )
        .map_err(|e| crate::ProofError::ProofGenerationFailed(format!("{:?}", e)))?;

        let mut proof_bytes = [0u8; GROTH16_PROOF_LENGTH_BYTES];
        Proof::serialize_compressed(&proof, &mut proof_bytes[..]).map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!("serialization failed: {:?}", e))
        })?;

        Ok(Self(proof_bytes))
    }

    /// Construct a `BatchItem` from this proof and its public inputs.
    /// Deserializes the proof and builds the public input vector without verifying.
    /// This is the single source of truth for public input ordering.
    pub fn to_batch_item(
        &self,
        public: OutputProofPublic,
    ) -> anyhow::Result<penumbra_sdk_proof_params::batch::BatchItem> {
        let proof =
            Proof::deserialize_compressed_unchecked(&self.0[..]).map_err(|e| anyhow::anyhow!(e))?;
        let statement_hash = output_statement_hash_from_public(&public)
            .map_err(|e| anyhow::anyhow!("failed computing output statement hash: {e}"))?;

        Ok(penumbra_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    #[tracing::instrument(level="debug", skip(self, vk), fields(self = ?BASE64_STANDARD.encode(self.clone().encode_to_vec()), vk = ?vk.debug_id()))]
    pub fn verify(
        &self,
        vk: &PreparedVerifyingKey<Bls12_377>,
        public: OutputProofPublic,
    ) -> anyhow::Result<()> {
        let item = self.to_batch_item(public)?;

        let proof_result = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            vk,
            item.public_inputs.as_slice(),
            &item.proof,
        )
        .map_err(|err| anyhow::anyhow!(err))?;

        proof_result
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("output proof did not verify"))
    }
}

impl DomainType for OutputProof {
    type Proto = pb::ZkOutputProof;
}

impl From<OutputProof> for pb::ZkOutputProof {
    fn from(proof: OutputProof) -> Self {
        pb::ZkOutputProof {
            inner: proof.0.to_vec(),
        }
    }
}

impl TryFrom<pb::ZkOutputProof> for OutputProof {
    type Error = anyhow::Error;

    fn try_from(proto: pb::ZkOutputProof) -> Result<Self, Self::Error> {
        Ok(OutputProof(proto.inner[..].try_into()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::{Fq, Fr};
    use penumbra_sdk_asset::{asset, Balance, Value};
    use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_tct as tct;
    use proptest::prelude::*;

    use crate::test_proof_helpers::proof_test_helpers::{
        create_imt_membership_proof, create_imt_non_membership_proof, create_user_tree_proof,
        mock_compliance_inputs_output,
    };
    use crate::{note, Note};

    fn fq_strategy() -> BoxedStrategy<Fq> {
        any::<[u8; 32]>()
            .prop_map(|bytes| Fq::from_le_bytes_mod_order(&bytes[..]))
            .boxed()
    }

    fn fr_strategy() -> BoxedStrategy<Fr> {
        any::<[u8; 32]>()
            .prop_map(|bytes| Fr::from_le_bytes_mod_order(&bytes[..]))
            .boxed()
    }

    prop_compose! {
        fn arb_valid_output_statement()(
            seed_phrase_randomness in any::<[u8; 32]>(),
            sender_seed_randomness in any::<[u8; 32]>(),
            rseed_randomness in any::<[u8; 32]>(),
            amount in any::<u64>(),
            balance_blinding in fr_strategy(),
            asset_id64 in any::<u64>(),
            address_index in any::<u32>(),
            cvk_sk_rand in any::<[u8; 32]>()
        ) -> (OutputProofPublic, OutputProofPrivate, Fr) {
            use penumbra_sdk_compliance::crypto::encrypt_output;
            use penumbra_sdk_compliance::derive_compliance_scalar;

            // Receiver
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_recipient = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_recipient = sk_recipient.full_viewing_key();
            let ivk_recipient = fvk_recipient.incoming();
            let (dest, _dtk_d) = ivk_recipient.payment_address(address_index.into());

            // Distinct sender
            let sender_seed = SeedPhrase::from_randomness(&sender_seed_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(sender_seed, &Bip44Path::new(0));
            let ivk_sender = sk_sender.full_viewing_key().incoming();
            let (sender_addr, _) = ivk_sender.payment_address(0u32.into());

            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                dest.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let balance_commitment = (-Balance::from(value_to_send)).commit(balance_blinding);

            let ring_pk_scalar = Fr::from_le_bytes_mod_order(&cvk_sk_rand);
            let ring_pk = decaf377::Element::GENERATOR * ring_pk_scalar;
            let dk_pub = decaf377::Element::GENERATOR;

            // Create IMT proof with matching ring_pk (Policy-in-Leaf binds it into the leaf)
            let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
                create_imt_membership_proof(value_to_send.asset_id.0, ring_pk, dk_pub);

            // Receiver ACK
            let b_d_fq = dest.diversified_generator().vartime_compress_to_field();
            let d = derive_compliance_scalar(b_d_fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            let ack_receiver = ring_pk * d_fr;

            // Sender ACK (distinct)
            let sender_b_d_fq = sender_addr.diversified_generator().vartime_compress_to_field();
            let sender_d = derive_compliance_scalar(sender_b_d_fq);
            let sender_d_fr = Fr::from_le_bytes_mod_order(&sender_d.to_bytes());
            let ack_sender = ring_pk * sender_d_fr;

            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                dest.clone(),
                value_to_send.asset_id,
                d,
            );

            // Create valid user tree proof for regulated asset
            let (compliance_anchor, compliance_path, compliance_position) =
                create_user_tree_proof(&user_leaf);

            // Generate compliance ciphertext with distinct ACKs
            let mut rng = rand::thread_rng();
            let encrypt_result = encrypt_output(
                &mut rng,
                &ack_receiver, &ack_sender, &dk_pub,
                &dest, &sender_addr,
                note.asset_id(), note.amount(), false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt");
            let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, packed_ciphertext) =
                encrypt_result.ciphertext.to_output_circuit_public_inputs();

            // Compute per-tier DLEQ proofs (core/ext: receiver ACK, sext: sender ACK)
            let salt = decaf377::Fq::from(0u64);
            let target_timestamp = Fq::from(0u64);

            let dleq_k_1 = Fr::rand(&mut rng);
            let m_core = penumbra_sdk_compliance::compute_metadata_hash(
                asset_indexed_leaf.ring.policy_id_hash,
                asset_indexed_leaf.ring.resource_hash,
                asset_indexed_leaf.ring.permission_hash,
                Fq::from(1u64), target_timestamp, salt,
            );
            let epk_1_point = decaf377::Element::GENERATOR * encrypt_result.r_1;
            let dleq_1 = penumbra_sdk_compliance::compute_dleq_native(
                encrypt_result.r_1, dleq_k_1, &ack_receiver, &epk_1_point, m_core,
            );

            let dleq_k_2 = Fr::rand(&mut rng);
            let m_ext = penumbra_sdk_compliance::compute_metadata_hash(
                asset_indexed_leaf.ring.policy_id_hash,
                asset_indexed_leaf.ring.resource_hash,
                asset_indexed_leaf.ring.permission_hash,
                Fq::from(2u64), target_timestamp, salt,
            );
            let epk_2_point = decaf377::Element::GENERATOR * encrypt_result.r_2;
            let dleq_2 = penumbra_sdk_compliance::compute_dleq_native(
                encrypt_result.r_2, dleq_k_2, &ack_receiver, &epk_2_point, m_ext,
            );

            let dleq_k_3 = Fr::rand(&mut rng);
            let m_sext = penumbra_sdk_compliance::compute_metadata_hash(
                asset_indexed_leaf.ring.policy_id_hash,
                asset_indexed_leaf.ring.resource_hash,
                asset_indexed_leaf.ring.permission_hash,
                Fq::from(3u64), target_timestamp, salt,
            );
            let epk_3_point = decaf377::Element::GENERATOR * encrypt_result.r_3;
            let dleq_3 = penumbra_sdk_compliance::compute_dleq_native(
                encrypt_result.r_3, dleq_k_3, &ack_sender, &epk_3_point, m_sext,
            );

            let counterparty_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender_addr,
                value_to_send.asset_id,
                sender_d,
            );
            let tx_blinding_nonce = Fr::from(0u64);
            let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(counterparty_leaf.commit(), tx_blinding_nonce);

            let public = OutputProofPublic {
                balance_commitment,
                note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp,
                dleq_c_1: dleq_1.c,
                dleq_s_1: Fq::from_le_bytes_mod_order(&dleq_1.s.to_bytes()),
                dleq_c_2: dleq_2.c,
                dleq_s_2: Fq::from_le_bytes_mod_order(&dleq_2.s.to_bytes()),
                dleq_c_3: dleq_3.c,
                dleq_s_3: Fq::from_le_bytes_mod_order(&dleq_3.s.to_bytes()),
                asset_anchor,
                compliance_anchor,
                counterparty_leaf_hash,
            };
            let private = OutputProofPrivate {
                note,
                balance_blinding,
                asset_path,
                asset_position,
                asset_indexed_leaf,
                is_regulated: true,
                compliance_path,
                compliance_position,
                user_leaf,
                compliance_ephemeral_secret: encrypt_result.r_1,
                r_2: encrypt_result.r_2,
                r_3: encrypt_result.r_3,
                counterparty_leaf,
                tx_blinding_nonce,
                is_flagged: false,
                salt,
            };

            (public, private, ring_pk_scalar)
        }
    }

    prop_compose! {
        fn arb_unregulated_output_statement()(
            seed_phrase_randomness in any::<[u8; 32]>(),
            sender_seed_randomness in any::<[u8; 32]>(),
            rseed_randomness in any::<[u8; 32]>(),
            amount in any::<u64>(),
            balance_blinding in fr_strategy(),
            asset_id64 in any::<u64>(),
            address_index in any::<u32>(),
            tester_keys_rand in any::<[u8; 32]>()
        ) -> (OutputProofPublic, OutputProofPrivate, Fr) {
            use penumbra_sdk_compliance::crypto::encrypt_output;

            // Receiver
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_recipient = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_recipient = sk_recipient.full_viewing_key();
            let ivk_recipient = fvk_recipient.incoming();
            let (dest, _dtk_d) = ivk_recipient.payment_address(address_index.into());

            // Distinct sender (ACK still BLACK_HOLE for unregulated, but leaf has proper d)
            let sender_seed = SeedPhrase::from_randomness(&sender_seed_randomness);
            let sk_sender = SpendKey::from_seed_phrase_bip44(sender_seed, &Bip44Path::new(0));
            let ivk_sender = sk_sender.full_viewing_key().incoming();
            let (sender_addr, _) = ivk_sender.payment_address(0u32.into());

            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                dest.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();
            let balance_commitment = (-Balance::from(value_to_send)).commit(balance_blinding);

            // Unregulated: circuit uses BLACK_HOLE_ACK directly
            let dk_pub = decaf377::Element::GENERATOR;
            let ack = *penumbra_sdk_compliance::BLACK_HOLE_ACK;

            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                dest.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );
            let counterparty_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                sender_addr,
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            // Create valid IMT non-membership proof for unregulated asset
            let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
                create_imt_non_membership_proof(value_to_send.asset_id.0);
            let compliance_anchor = tct::StateCommitment(Fq::from(0u64));

            // Generate valid compliance ciphertext using real encryption with BLACK_HOLE_ACK
            let mut rng = rand::thread_rng();
            let encrypt_result = encrypt_output(
                &mut rng,
                &ack, &ack, &dk_pub,
                &dest, &dest,
                note.asset_id(), note.amount(), false,
                decaf377::Fq::from(0u64),
            ).expect("can encrypt");
            let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, packed_ciphertext) =
                encrypt_result.ciphertext.to_output_circuit_public_inputs();

            let tx_blinding_nonce = Fr::from(0u64);
            let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(counterparty_leaf.commit(), tx_blinding_nonce);

            let public = OutputProofPublic {
                balance_commitment,
                note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c_1: Fq::from(0u64),
                dleq_s_1: Fq::from(0u64),
                dleq_c_2: Fq::from(0u64),
                dleq_s_2: Fq::from(0u64),
                dleq_c_3: Fq::from(0u64),
                dleq_s_3: Fq::from(0u64),
                asset_anchor,
                compliance_anchor,
                counterparty_leaf_hash,
            };
            let private = OutputProofPrivate {
                note,
                balance_blinding,
                asset_path,
                asset_position,
                asset_indexed_leaf,
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf,
                compliance_ephemeral_secret: encrypt_result.r_1,
                r_2: encrypt_result.r_2,
                r_3: encrypt_result.r_3,
                counterparty_leaf,
                tx_blinding_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };

            let my_random_sk_scalar = Fr::from_le_bytes_mod_order(&tester_keys_rand);

            (public, private, my_random_sk_scalar)
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(1))]
        #[test]
        fn output_proof_happy_path((public, private, _cvk_sk) in arb_valid_output_statement()) {
            assert!(check_satisfaction(&public, &private).is_ok());
            assert!(check_circuit_satisfaction(public, private).is_ok());
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(1))]
        #[test]
        fn output_proof_unregulated_asset((public, private, _cvk_sk) in arb_unregulated_output_statement()) {
            assert!(check_satisfaction(&public, &private).is_ok());
            assert!(check_circuit_satisfaction(public, private).is_ok());
        }
    }

    #[test]
    fn output_proof_full_groth16_roundtrip_regulated() {
        use crate::test_proof_helpers::proof_test_helpers::{full_groth16_roundtrip, CircuitType};
        full_groth16_roundtrip(CircuitType::Output, true);
    }

    #[test]
    fn output_proof_full_groth16_roundtrip_unregulated() {
        use crate::test_proof_helpers::proof_test_helpers::{full_groth16_roundtrip, CircuitType};
        full_groth16_roundtrip(CircuitType::Output, false);
    }

    #[test]
    fn output_proof_plan_path_regulated() {
        use crate::test_proof_helpers::proof_test_helpers::test_output_plan_path;
        test_output_plan_path(true);
    }

    #[test]
    fn output_proof_plan_path_unregulated() {
        use crate::test_proof_helpers::proof_test_helpers::test_output_plan_path;
        test_output_plan_path(false);
    }

    prop_compose! {
        fn arb_invalid_output_note_commitment_integrity()(
            seed_phrase_randomness in any::<[u8; 32]>(),
            rseed_randomness in any::<[u8; 32]>(),
            amount in any::<u64>(),
            balance_blinding in fr_strategy(),
            asset_id64 in any::<u64>(),
            address_index in any::<u32>(),
            incorrect_note_blinding in fq_strategy(),
            cvk_sk_rand in any::<[u8; 32]>()
        ) -> (OutputProofPublic, OutputProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_recipient = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_recipient = sk_recipient.full_viewing_key();
            let ivk_recipient = fvk_recipient.incoming();
            let (dest, _dtk_d) = ivk_recipient.payment_address(address_index.into());

            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                dest.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let balance_commitment = (-Balance::from(value_to_send)).commit(balance_blinding);

            let incorrect_note_commitment = note::commitment(
                incorrect_note_blinding,
                value_to_send,
                note.diversified_generator(),
                note.transmission_key_s(),
                note.clue_key(),
            );

            let _ring_pk_scalar = Fr::from_le_bytes_mod_order(&cvk_sk_rand);

            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                dest.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let asset_anchor = tct::StateCommitment(Fq::from(0u64));
            let compliance_anchor = tct::StateCommitment(Fq::from(0u64));
            let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, packed_ciphertext) = mock_compliance_inputs_output();

            let dummy_leaf = user_leaf.clone();
            let dummy_nonce = Fr::from(0u64);
            let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(dummy_leaf.commit(), dummy_nonce);

            let bad_public = OutputProofPublic {
                balance_commitment,
                note_commitment: incorrect_note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c_1: Fq::from(0u64),
                dleq_s_1: Fq::from(0u64),
                dleq_c_2: Fq::from(0u64),
                dleq_s_2: Fq::from(0u64),
                dleq_c_3: Fq::from(0u64),
                dleq_s_3: Fq::from(0u64),
                asset_anchor,
                compliance_anchor,
                counterparty_leaf_hash,
            };
            let private = OutputProofPrivate {
                note,
                balance_blinding,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf: user_leaf.clone(),
                compliance_ephemeral_secret: Fr::from(0u64),
                r_2: Fr::from(0u64),
                r_3: Fr::from(0u64),
                counterparty_leaf: dummy_leaf.clone(),
                tx_blinding_nonce: dummy_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };

            (bad_public, private)
        }
    }

    proptest! {
        #[test]
        fn output_proof_verification_fails_note_commitment_integrity((public, private) in arb_invalid_output_note_commitment_integrity()) {
            assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
        }
    }

    prop_compose! {
        fn arb_invalid_output_balance_commitment_integrity()(
            seed_phrase_randomness in any::<[u8; 32]>(),
            rseed_randomness in any::<[u8; 32]>(),
            amount in any::<u64>(),
            balance_blinding in fr_strategy(),
            asset_id64 in any::<u64>(),
            address_index in any::<u32>(),
            incorrect_v_blinding in fr_strategy(),
            cvk_sk_rand in any::<[u8; 32]>()
        ) -> (OutputProofPublic, OutputProofPrivate) {
            let seed_phrase = SeedPhrase::from_randomness(&seed_phrase_randomness);
            let sk_recipient = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
            let fvk_recipient = sk_recipient.full_viewing_key();
            let ivk_recipient = fvk_recipient.incoming();
            let (dest, _dtk_d) = ivk_recipient.payment_address(address_index.into());

            let value_to_send = Value {
                amount: Amount::from(amount),
                asset_id: asset::Id(Fq::from(asset_id64)),
            };
            let note = Note::from_parts(
                dest.clone(),
                value_to_send,
                Rseed(rseed_randomness),
            ).expect("should be able to create note");
            let note_commitment = note.commit();

            let incorrect_balance_commitment = (-Balance::from(value_to_send)).commit(incorrect_v_blinding);

            let _ring_pk_scalar = Fr::from_le_bytes_mod_order(&cvk_sk_rand);

            let user_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
                dest.clone(),
                value_to_send.asset_id,
                Fq::from(0u64),
            );

            let asset_anchor = tct::StateCommitment(Fq::from(0u64));
            let compliance_anchor = tct::StateCommitment(Fq::from(0u64));
            let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, packed_ciphertext) = mock_compliance_inputs_output();

            let dummy_leaf = user_leaf.clone();
            let dummy_nonce = Fr::from(0u64);
            let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(dummy_leaf.commit(), dummy_nonce);

            let bad_public = OutputProofPublic {
                balance_commitment: incorrect_balance_commitment,
                note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext: packed_ciphertext,
                target_timestamp: Fq::from(0u64),
                dleq_c_1: Fq::from(0u64),
                dleq_s_1: Fq::from(0u64),
                dleq_c_2: Fq::from(0u64),
                dleq_s_2: Fq::from(0u64),
                dleq_c_3: Fq::from(0u64),
                dleq_s_3: Fq::from(0u64),
                asset_anchor,
                compliance_anchor,
                counterparty_leaf_hash,
            };

            let private = OutputProofPrivate {
                note,
                balance_blinding,
                asset_path: penumbra_sdk_compliance::MerklePath::default(),
                asset_position: 0,
                asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                    Fq::from(0u64), 0, penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                ),
                is_regulated: false,
                compliance_path: penumbra_sdk_compliance::MerklePath::default(),
                compliance_position: 0,
                user_leaf: user_leaf.clone(),
                compliance_ephemeral_secret: Fr::from(0u64),
                r_2: Fr::from(0u64),
                r_3: Fr::from(0u64),
                counterparty_leaf: dummy_leaf.clone(),
                tx_blinding_nonce: dummy_nonce,
                is_flagged: false,
                salt: decaf377::Fq::from(0u64),
            };

            (bad_public, private)
        }
    }

    proptest! {
        #[test]
        fn output_proof_verification_fails_balance_commitment_integrity((public, private) in arb_invalid_output_balance_commitment_integrity()) {
            assert!(check_satisfaction(&public, &private).is_err());
            assert!(check_circuit_satisfaction(public, private).is_err());
        }
    }
}
