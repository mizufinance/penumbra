//! Vanilla (pre-compliance, v2.0.4) circuit definitions and helpers.
//!
//! Shared between compliance_proofs and compliance_validator benchmarks.
//! Included via `#[path = "vanilla_circuits.rs"] mod vanilla_circuits;`.

#![allow(dead_code)]

use std::str::FromStr;

use ark_ff::ToConstraintField;
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16};
use ark_r1cs_std::prelude::{AllocVar, EqGadget, FieldVar};
use ark_r1cs_std::uint8::UInt8;
use ark_r1cs_std::ToBitsGadget;
use ark_relations::r1cs::{
    ConstraintSynthesizer, ConstraintSystem, ConstraintSystemRef, OptimizationGoal, SynthesisMode,
};
use ark_snark::SNARK;
use decaf377::{r1cs::FqVar, Bls12_377, Fq, Fr};
use decaf377_fmd as fmd;
use decaf377_ka as ka;
use decaf377_rdsa::{SpendAuth, VerificationKey};
use once_cell::sync::Lazy;
use rand_core::OsRng;

use penumbra_sdk_asset::{
    balance, balance::commitment::BalanceCommitmentVar, balance::BalanceVar, Balance, Value,
};
use penumbra_sdk_keys::keys::{
    AuthorizationKeyVar, Bip44Path, Diversifier, IncomingViewingKeyVar, NullifierKey,
    NullifierKeyVar, RandomizedVerificationKey, SeedPhrase, SpendAuthRandomizerVar, SpendKey,
};
use penumbra_sdk_keys::Address;
use penumbra_sdk_proof_params::DummyWitness;
use penumbra_sdk_sct::{Nullifier, NullifierVar};
use penumbra_sdk_shielded_pool::{note, Note, Rseed};
use penumbra_sdk_tct as tct;
use penumbra_sdk_tct::r1cs::StateCommitmentVar;

// ===========================================================================
// Vanilla Spend Circuit (from v2.0.4, pre-compliance)
// ===========================================================================

#[derive(Clone, Debug)]
pub struct VanillaSpendPublic {
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
}

#[derive(Clone, Debug)]
pub struct VanillaSpendPrivate {
    pub state_commitment_proof: tct::Proof,
    pub note: Note,
    pub v_blinding: Fr,
    pub spend_auth_randomizer: Fr,
    pub ak: VerificationKey<SpendAuth>,
    pub nk: NullifierKey,
}

#[derive(Clone, Debug)]
pub struct VanillaSpendCircuit {
    pub public: VanillaSpendPublic,
    pub private: VanillaSpendPrivate,
}

impl ConstraintSynthesizer<Fq> for VanillaSpendCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> ark_relations::r1cs::Result<()> {
        // Witnesses
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
        let ak_element_var: AuthorizationKeyVar =
            AuthorizationKeyVar::new_witness(cs.clone(), || Ok(self.private.ak))?;
        let nk_var = NullifierKeyVar::new_witness(cs.clone(), || Ok(self.private.nk))?;

        // Public inputs
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(Fq::from(self.public.anchor)))?;
        let claimed_balance_commitment_var =
            BalanceCommitmentVar::new_input(cs.clone(), || Ok(self.public.balance_commitment))?;
        let claimed_nullifier_var =
            NullifierVar::new_input(cs.clone(), || Ok(self.public.nullifier))?;
        let rk_var = RandomizedVerificationKey::new_input(cs.clone(), || Ok(self.public.rk))?;

        // Note commitment integrity
        let note_commitment_var = note_var.commit()?;
        note_commitment_var.enforce_equal(&claimed_note_commitment)?;

        // Nullifier integrity
        let nullifier_var = NullifierVar::derive(&nk_var, &position_var, &claimed_note_commitment)?;
        nullifier_var.enforce_equal(&claimed_nullifier_var)?;

        // Merkle auth path
        let is_not_dummy = note_var.amount().is_eq(&FqVar::zero())?.not();
        merkle_path_var.verify(
            cs.clone(),
            &is_not_dummy,
            &position_bits,
            anchor_var,
            claimed_note_commitment.inner(),
        )?;

        // Randomized verification key
        let computed_rk_var = ak_element_var.randomize(&spend_auth_randomizer_var)?;
        computed_rk_var.enforce_equal(&rk_var)?;

        // Diversified address integrity
        let ivk = IncomingViewingKeyVar::derive(&nk_var, &ak_element_var)?;
        let computed_transmission_key =
            ivk.diversified_public(&note_var.diversified_generator())?;
        computed_transmission_key.enforce_equal(&note_var.transmission_key())?;

        // Balance commitment integrity
        let balance_commitment = note_var.value().commit(v_blinding_vars)?;
        balance_commitment.enforce_equal(&claimed_balance_commitment_var)?;

        Ok(())
    }
}

impl DummyWitness for VanillaSpendCircuit {
    fn with_dummy_witness() -> Self {
        let seed_phrase = SeedPhrase::from_randomness(&[b'f'; 32]);
        let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk_sender = sk_sender.full_viewing_key();
        let ivk_sender = fvk_sender.incoming();
        let (address, _dtk_d) = ivk_sender.payment_address(0u32.into());

        let spend_auth_randomizer = Fr::from(1u64);
        let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
        let nk = *sk_sender.nullifier_key();
        let ak = sk_sender.spend_auth_key().into();
        let note = Note::from_parts(
            address,
            Value::from_str("1upenumbra").expect("valid value"),
            Rseed([1u8; 32]),
        )
        .expect("can make a note");
        let v_blinding = Fr::from(1u64);
        let rk: VerificationKey<SpendAuth> = rsk.into();
        let nullifier = Nullifier(Fq::from(1u64));
        let mut sct = tct::Tree::new();
        let note_commitment = note.commit();
        sct.insert(tct::Witness::Keep, note_commitment)
            .expect("able to insert note commitment into SCT");
        let state_commitment_proof = sct
            .witness(note_commitment)
            .expect("able to witness just-inserted note commitment");
        let anchor = sct.root();

        let public = VanillaSpendPublic {
            anchor,
            balance_commitment: balance::Commitment(decaf377::Element::GENERATOR),
            nullifier,
            rk,
        };
        let private = VanillaSpendPrivate {
            state_commitment_proof,
            note,
            v_blinding,
            spend_auth_randomizer,
            ak,
            nk,
        };

        Self { public, private }
    }
}

// ===========================================================================
// Vanilla Output Circuit (from v2.0.4, pre-compliance)
// ===========================================================================

#[derive(Clone, Debug)]
pub struct VanillaOutputPublic {
    pub balance_commitment: balance::Commitment,
    pub note_commitment: note::StateCommitment,
}

#[derive(Clone, Debug)]
pub struct VanillaOutputPrivate {
    pub note: Note,
    pub balance_blinding: Fr,
}

#[derive(Clone, Debug)]
pub struct VanillaOutputCircuit {
    pub public: VanillaOutputPublic,
    pub private: VanillaOutputPrivate,
}

impl ConstraintSynthesizer<Fq> for VanillaOutputCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> ark_relations::r1cs::Result<()> {
        // Witnesses
        let note_var = note::NoteVar::new_witness(cs.clone(), || Ok(self.private.note.clone()))?;
        let balance_blinding_arr: [u8; 32] = self.private.balance_blinding.to_bytes();
        let balance_blinding_vars = UInt8::new_witness_vec(cs.clone(), &balance_blinding_arr)?;

        // Public inputs
        let claimed_note_commitment =
            StateCommitmentVar::new_input(cs.clone(), || Ok(self.public.note_commitment))?;
        let claimed_balance_commitment =
            BalanceCommitmentVar::new_input(cs.clone(), || Ok(self.public.balance_commitment))?;

        // Balance commitment integrity
        let balance_commitment =
            BalanceVar::from_negative_value_var(note_var.value()).commit(balance_blinding_vars)?;
        balance_commitment.enforce_equal(&claimed_balance_commitment)?;

        // Note commitment integrity
        let note_commitment = note_var.commit()?;
        note_commitment.enforce_equal(&claimed_note_commitment)?;

        Ok(())
    }
}

impl DummyWitness for VanillaOutputCircuit {
    fn with_dummy_witness() -> Self {
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
            address,
            Value::from_str("1upenumbra").expect("valid value"),
            Rseed([1u8; 32]),
        )
        .expect("can make a note");
        let balance_blinding = Fr::from(1u64);

        let public = VanillaOutputPublic {
            note_commitment: note.commit(),
            balance_commitment: balance::Commitment(decaf377::Element::GENERATOR),
        };
        let private = VanillaOutputPrivate {
            note,
            balance_blinding,
        };
        Self { public, private }
    }
}

// ===========================================================================
// Lazy key generation for vanilla circuits
// ===========================================================================

pub static VANILLA_SPEND_KEYS: Lazy<(
    ark_groth16::ProvingKey<Bls12_377>,
    ark_groth16::PreparedVerifyingKey<Bls12_377>,
)> = Lazy::new(|| {
    eprintln!("  Generating vanilla spend proving keys...");
    let circuit = VanillaSpendCircuit::with_dummy_witness();
    let (pk, vk) =
        Groth16::<Bls12_377, LibsnarkReduction>::circuit_specific_setup(circuit, &mut OsRng)
            .expect("vanilla spend setup");
    let pvk = ark_groth16::prepare_verifying_key(&vk);
    eprintln!("  Vanilla spend keys ready.");
    (pk, pvk)
});

pub static VANILLA_OUTPUT_KEYS: Lazy<(
    ark_groth16::ProvingKey<Bls12_377>,
    ark_groth16::PreparedVerifyingKey<Bls12_377>,
)> = Lazy::new(|| {
    eprintln!("  Generating vanilla output proving keys...");
    let circuit = VanillaOutputCircuit::with_dummy_witness();
    let (pk, vk) =
        Groth16::<Bls12_377, LibsnarkReduction>::circuit_specific_setup(circuit, &mut OsRng)
            .expect("vanilla output setup");
    let pvk = ark_groth16::prepare_verifying_key(&vk);
    eprintln!("  Vanilla output keys ready.");
    (pk, pvk)
});

// ===========================================================================
// Helpers for building valid circuits
// ===========================================================================

pub fn make_vanilla_spend_circuit() -> (VanillaSpendCircuit, Vec<Fq>) {
    let seed_phrase = SeedPhrase::from_randomness(&[b'f'; 32]);
    let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
    let fvk_sender = sk_sender.full_viewing_key();
    let ivk_sender = fvk_sender.incoming();
    let (address, _dtk_d) = ivk_sender.payment_address(0u32.into());

    let value = Value::from_str("1upenumbra").expect("valid value");
    let note = Note::from_parts(address, value, Rseed([1u8; 32])).expect("can make a note");
    let note_commitment = note.commit();

    let mut sct = tct::Tree::new();
    sct.insert(tct::Witness::Keep, note_commitment).unwrap();
    let anchor = sct.root();
    let state_commitment_proof = sct.witness(note_commitment).unwrap();

    let v_blinding = Fr::from(1u64);
    let balance_commitment = value.commit(v_blinding);
    let spend_auth_randomizer = Fr::from(1u64);
    let rsk = sk_sender.spend_auth_key().randomize(&spend_auth_randomizer);
    let nk = *sk_sender.nullifier_key();
    let ak: VerificationKey<SpendAuth> = sk_sender.spend_auth_key().into();
    let rk: VerificationKey<SpendAuth> = rsk.into();
    let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

    let public = VanillaSpendPublic {
        anchor,
        balance_commitment,
        nullifier,
        rk,
    };
    let private = VanillaSpendPrivate {
        state_commitment_proof,
        note,
        v_blinding,
        spend_auth_randomizer,
        ak,
        nk,
    };

    // Build public inputs for verification
    let mut public_inputs = Vec::new();
    public_inputs.extend(Fq::from(public.anchor).to_field_elements().unwrap());
    public_inputs.extend(public.balance_commitment.0.to_field_elements().unwrap());
    public_inputs.extend(public.nullifier.0.to_field_elements().unwrap());
    let rk_bytes = decaf377_rdsa::VerificationKeyBytes::from(public.rk);
    let rk_bytes_arr: [u8; 32] = rk_bytes.into();
    let rk_element = decaf377::Encoding(rk_bytes_arr)
        .vartime_decompress()
        .expect("valid rk");
    public_inputs.extend(rk_element.to_field_elements().unwrap());

    (VanillaSpendCircuit { public, private }, public_inputs)
}

pub fn make_vanilla_output_circuit() -> (VanillaOutputCircuit, Vec<Fq>) {
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
    let value = Value::from_str("1upenumbra").expect("valid value");
    let note = Note::from_parts(address, value, Rseed([1u8; 32])).expect("can make a note");
    let note_commitment = note.commit();
    let balance_blinding = Fr::from(1u64);
    let balance_commitment = (-Balance::from(value)).commit(balance_blinding);

    let public = VanillaOutputPublic {
        balance_commitment,
        note_commitment,
    };
    let private = VanillaOutputPrivate {
        note,
        balance_blinding,
    };

    // Build public inputs for verification
    let mut public_inputs = Vec::new();
    public_inputs.extend(public.note_commitment.0.to_field_elements().unwrap());
    public_inputs.extend(public.balance_commitment.0.to_field_elements().unwrap());

    (VanillaOutputCircuit { public, private }, public_inputs)
}

/// Count constraints for any circuit over Fq.
pub fn count_constraints<C: ConstraintSynthesizer<Fq>>(circuit: C) -> usize {
    let cs = ConstraintSystem::<Fq>::new_ref();
    cs.set_optimization_goal(OptimizationGoal::Constraints);
    cs.set_mode(SynthesisMode::Setup);
    circuit
        .generate_constraints(cs.clone())
        .expect("can generate constraints");
    cs.finalize();
    cs.num_constraints()
}
