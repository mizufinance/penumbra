use anyhow::{anyhow, ensure, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::{
    ComplianceLeaf, IndexedLeaf, MerklePath, WithdrawalComplianceCiphertext,
};
use shieldd_sdk_keys::keys::NullifierKey;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_tct as tct;

use crate::{
    discovery::{Parameters as RoutingParameters, RoutingTag},
    public_input_hash::shielded_ics20_withdrawal_statement_hash_from_public,
    Note, VolumeAccumulatorPrivate, VolumeAccumulatorPublic,
};

use super::ShieldedIcs20WithdrawalFamilyId;

impl ShieldedIcs20WithdrawalFamilyId {
    pub fn deployed_proof_key(self) -> shieldd_sdk_proof_params::DeployedProofKey {
        match self.get() {
            1 => shieldd_sdk_proof_params::DeployedProofKey::ShieldedIcs20WithdrawalCanonical,
            unknown => {
                panic!("validated shielded ICS-20 withdrawal family has unknown id {unknown}")
            }
        }
    }

    pub fn proof_verification_key(self) -> &'static PreparedVerifyingKey<Bls12_377> {
        self.deployed_proof_key().bundled_pvk()
    }

    pub fn proving_key_bytes(self) -> &'static [u8] {
        shieldd_sdk_proof_params::shielded_ics20_withdrawal_proving_key_bytes(self.get())
    }

    pub fn verifying_key_json_bytes(self) -> &'static [u8] {
        shieldd_sdk_proof_params::shielded_ics20_withdrawal_verifying_key_json_bytes(self.get())
    }

    pub fn circuit_metadata_bytes(self) -> &'static [u8] {
        shieldd_sdk_proof_params::shielded_ics20_withdrawal_circuit_metadata(self.get())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ShieldedIcs20WithdrawalProof {
    pub inner: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalInputPublic {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub history_required: bool,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalChangePublic {
    pub note_commitment: tct::StateCommitment,
    pub recovery_commitment: crate::RecoveryCommitment,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalProofPublic {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub asset_anchor: tct::StateCommitment,
    pub compliance_anchor: tct::StateCommitment,
    pub target_timestamp: Fq,
    pub inputs: Vec<ShieldedIcs20WithdrawalInputPublic>,
    pub change_output: ShieldedIcs20WithdrawalChangePublic,
    pub outbound_asset_id: Fq,
    pub outbound_amount: Fq,
    pub withdrawal_effect_hash_limbs: [Fq; 4],
    pub routing_tag: RoutingTag,
    pub routing_parameter_set_id: Fq,
    pub withdrawal_compliance_ciphertext: WithdrawalComplianceCiphertext,
    pub recent_position_floor: u64,
    pub volume_accumulator: VolumeAccumulatorPublic,
}

pub(crate) fn withdrawal_effect_hash_limbs(bytes: &[u8]) -> [Fq; 4] {
    assert_eq!(
        bytes.len(),
        64,
        "withdrawal effect hash must contain exactly 64 bytes"
    );
    std::array::from_fn(|index| {
        let start = index * 16;
        Fq::from_le_bytes_mod_order(&bytes[start..start + 16])
    })
}

impl ShieldedIcs20WithdrawalProofPublic {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.family_id == ShieldedIcs20WithdrawalFamilyId::Canonical,
            "shielded ICS-20 withdrawal family must be canonical"
        );
        ensure!(
            self.inputs.len() == self.family_id.input_count(),
            "{} expects {} inputs, got {}",
            self.family_id.label(),
            self.family_id.input_count(),
            self.inputs.len()
        );
        Ok(())
    }

    pub fn statement_hash(&self) -> Result<Fq, crate::public_input_hash::StatementHashError> {
        shielded_ics20_withdrawal_statement_hash_from_public(self)
    }
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalRequiredInputPrivate {
    pub state_commitment_proof: tct::Proof,
    pub spent_note: Note,
    pub spend_auth_randomizer: Fr,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalOptionalInputPrivate {
    pub spend: ShieldedIcs20WithdrawalRequiredInputPrivate,
    pub is_dummy: bool,
    pub dummy_nullifier_seed: Fq,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalChangePrivate {
    pub created_note: Note,
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalProofPrivate {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub action_balance_blinding: Fr,
    pub ak: VerificationKey<SpendAuth>,
    pub nk: NullifierKey,
    pub asset_path: MerklePath,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeaf,
    pub is_regulated: bool,
    pub routing_parameters: RoutingParameters,
    pub routing_nonce: Fq,
    pub sender_compliance_path: MerklePath,
    pub sender_compliance_position: u64,
    pub sender_leaf: ComplianceLeaf,
    pub withdrawal_seed: Fq,
    pub withdrawal_randomizer: Fr,
    pub required_input: ShieldedIcs20WithdrawalRequiredInputPrivate,
    pub optional_input: ShieldedIcs20WithdrawalOptionalInputPrivate,
    pub change_output: ShieldedIcs20WithdrawalChangePrivate,
    pub volume_accumulator: VolumeAccumulatorPrivate,
    pub volume_accumulator_seed: Fq,
}

impl ShieldedIcs20WithdrawalProofPrivate {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.family_id == ShieldedIcs20WithdrawalFamilyId::Canonical,
            "shielded ICS-20 withdrawal family must be canonical"
        );
        Ok(())
    }
}

impl ShieldedIcs20WithdrawalProof {
    fn decoded_proof(&self) -> anyhow::Result<Proof<decaf377::Bls12_377>> {
        crate::groth16_proof::decode(&self.inner)
    }

    pub(crate) fn to_batch_item(
        &self,
        public: &ShieldedIcs20WithdrawalProofPublic,
    ) -> anyhow::Result<shieldd_sdk_proof_params::batch::BatchItem> {
        let statement_hash = public.statement_hash()?;
        let proof = self.decoded_proof()?;

        Ok(shieldd_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    pub fn verify(&self, public: &ShieldedIcs20WithdrawalProofPublic) -> anyhow::Result<()> {
        self.verify_with_prepared_vk(public, public.family_id.proof_verification_key())
    }

    pub fn verify_with_prepared_vk(
        &self,
        public: &ShieldedIcs20WithdrawalProofPublic,
        vk: &PreparedVerifyingKey<Bls12_377>,
    ) -> anyhow::Result<()> {
        let item = self.to_batch_item(public)?;
        let proof_result =
            Groth16::<decaf377::Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                vk,
                item.public_inputs.as_slice(),
                &item.proof,
            )
            .map_err(|err| anyhow!(err))?;

        proof_result
            .then_some(())
            .ok_or_else(|| anyhow!("{} proof did not verify", public.family_id.label()))
    }

    pub fn for_family(&self, _family_id: ShieldedIcs20WithdrawalFamilyId) -> Result<()> {
        self.decoded_proof()?;
        Ok(())
    }

    #[cfg(all(feature = "prover", any(unix, windows)))]
    pub fn prove(
        public: ShieldedIcs20WithdrawalProofPublic,
        private: ShieldedIcs20WithdrawalProofPrivate,
    ) -> Result<Self, crate::ProofError> {
        let family_id = public.family_id;
        public
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        private
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPrivateInput(e.to_string()))?;
        if private.family_id != family_id {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded ICS-20 withdrawal family mismatch: public={} private={}",
                family_id.label(),
                private.family_id.label(),
            )));
        }

        super::prover_runtime::prove_with_runtime(public, private).map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} prove: {e}",
                family_id.label()
            ))
        })
    }
}

impl DomainType for ShieldedIcs20WithdrawalProof {
    type Proto = pb::ZkShieldedIcs20WithdrawalProof;
}

impl From<ShieldedIcs20WithdrawalProof> for pb::ZkShieldedIcs20WithdrawalProof {
    fn from(value: ShieldedIcs20WithdrawalProof) -> Self {
        Self { inner: value.inner }
    }
}

impl TryFrom<pb::ZkShieldedIcs20WithdrawalProof> for ShieldedIcs20WithdrawalProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::ZkShieldedIcs20WithdrawalProof) -> Result<Self, Self::Error> {
        let proof = Self { inner: value.inner };
        proof.for_family(ShieldedIcs20WithdrawalFamilyId::Canonical)?;
        Ok(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::{withdrawal_effect_hash_limbs, ShieldedIcs20WithdrawalProof};
    use crate::{test_proof_helpers::proof_test_helpers, ShieldedIcs20WithdrawalFamilyId};
    use decaf377::Fq;
    use rand::SeedableRng;

    #[test]
    fn withdrawal_deployed_key_mapping_matches_generated_registry_for_every_family() {
        for family in ShieldedIcs20WithdrawalFamilyId::ALL {
            assert!(std::ptr::eq(
                family.deployed_proof_key().bundled_pvk(),
                shieldd_sdk_proof_params::shielded_ics20_withdrawal_proof_verification_key(
                    family.get(),
                ),
            ));
        }
    }

    #[test]
    fn shielded_ics20_withdrawal_rejects_wrong_public_shape() {
        let (mut public, _private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                false,
            );
        public.inputs.pop();
        let error = public
            .validate_shape()
            .expect_err("shape validation should reject 1x2 public inputs");
        assert!(
            error.to_string().contains("expects 2 inputs, got 1"),
            "unexpected shape error: {error}"
        );
    }

    #[test]
    fn withdrawal_effect_hash_maps_to_four_little_endian_u128_limbs() {
        let expected = [
            0x0123_4567_89ab_cdef_0011_2233_4455_6677u128,
            0x1020_3040_5060_7080_90a0_b0c0_d0e0_f001u128,
            0xdead_beef_cafe_babe_7654_3210_fedc_ba98u128,
            0xffff_eeee_dddd_cccc_bbbb_aaaa_9999_8888u128,
        ];
        let mut bytes = [0u8; 64];
        for (chunk, value) in bytes.chunks_exact_mut(16).zip(expected) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }

        assert_eq!(
            withdrawal_effect_hash_limbs(&bytes),
            expected.map(|value| Fq::from_le_bytes_mod_order(&value.to_le_bytes()))
        );
    }

    #[cfg(all(feature = "prover", any(unix, windows)))]
    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_shielded_ics20_withdrawal_proof_roundtrip() {
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Withdrawal)
            .expect("proof test prerequisites must be present");

        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        let proof =
            ShieldedIcs20WithdrawalProof::prove(public.clone(), private).unwrap_or_else(|error| {
                panic!("can generate {} proof: {error}", public.family_id.label());
            });
        let item = proof
            .to_batch_item(&public)
            .expect("can build shielded ICS-20 withdrawal batch item");
        assert_eq!(item.public_inputs.len(), 1);
        proof.verify(&public).expect("proof should verify");

        let reject = |label: &str, changed: crate::ShieldedIcs20WithdrawalProofPublic| {
            assert!(
                proof.verify(&changed).is_err(),
                "proof must reject mutated {label}"
            );
        };
        let mut changed = public.clone();
        changed.volume_accumulator.nullifier.0 += Fq::from(1u64);
        reject("volume nullifier", changed);
        let mut changed = public.clone();
        changed.volume_accumulator.commitment.0 += Fq::from(1u64);
        reject("volume commitment", changed);
        let mut changed = public.clone();
        changed.volume_accumulator.day_start += 86_400;
        reject("volume day", changed);
        let mut changed = public.clone();
        changed.target_timestamp += Fq::from(1u64);
        reject("target timestamp", changed);
        let mut changed = public.clone();
        changed.outbound_asset_id += Fq::from(1u64);
        reject("outbound asset", changed);
        let mut changed = public.clone();
        changed.outbound_amount += Fq::from(1u64);
        reject("outbound amount", changed);
        let mut changed = public.clone();
        changed.withdrawal_compliance_ciphertext.epk += decaf377::Element::GENERATOR;
        reject("sender epk", changed);
        let mut changed = public.clone();
        changed.withdrawal_compliance_ciphertext.c2 += Fq::from(1u64);
        reject("sender c2", changed);
        let mut changed = public.clone();
        changed.withdrawal_compliance_ciphertext.key_confirmation += Fq::from(1u64);
        reject("sender key confirmation", changed);
        for index in 0..3 {
            let mut changed = public.clone();
            let word = &mut changed
                .withdrawal_compliance_ciphertext
                .encrypted_sender_address[index * 32..(index + 1) * 32];
            let value =
                Fq::from_bytes_checked((&*word).try_into().unwrap()).unwrap() + Fq::from(1u64);
            word.copy_from_slice(&value.to_bytes());
            reject("sender ciphertext", changed);
        }
    }

    #[cfg(all(feature = "prover", any(unix, windows)))]
    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_shielded_ics20_withdrawal_accumulator_branches_roundtrip() {
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Withdrawal)
            .expect("proof test prerequisites must be present");

        for (seed, mode) in [
            (
                0x4f52_4947_494e_1001,
                proof_test_helpers::WithdrawalAccumulatorTestMode::Origin,
            ),
            (
                0x434f_4e54_1000_0001,
                proof_test_helpers::WithdrawalAccumulatorTestMode::Continuation {
                    prior_volume: 25,
                },
            ),
        ] {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let (public, private) = proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng_and_mode(
                &mut rng,
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
                2,
                mode,
            );
            let proof = ShieldedIcs20WithdrawalProof::prove(public.clone(), private)
                .expect("withdrawal accumulator branch should prove");
            proof
                .verify(&public)
                .expect("withdrawal accumulator branch should verify");
        }
    }
}
