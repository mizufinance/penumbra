use std::convert::TryInto;

use anyhow::{anyhow, ensure, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalDeserialize;
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use penumbra_sdk_asset::balance;
use penumbra_sdk_compliance::{ComplianceLeaf, IndexedLeaf, MerklePath};
use penumbra_sdk_keys::keys::NullifierKey;
use penumbra_sdk_proof_params::GROTH16_PROOF_LENGTH_BYTES;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct as tct;

use crate::{public_input_hash::shielded_ics20_withdrawal_statement_hash_from_public, Note};

use super::ShieldedIcs20WithdrawalFamilyId;

impl ShieldedIcs20WithdrawalFamilyId {
    pub fn proof_verification_key(self) -> &'static PreparedVerifyingKey<Bls12_377> {
        penumbra_sdk_proof_params::shielded_ics20_withdrawal_proof_verification_key(self.get())
    }

    pub fn proving_key_bytes(self) -> &'static [u8] {
        penumbra_sdk_proof_params::shielded_ics20_withdrawal_proving_key_bytes(self.get())
    }

    pub fn circuit_metadata_bytes(self) -> &'static [u8] {
        penumbra_sdk_proof_params::shielded_ics20_withdrawal_circuit_metadata(self.get())
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
}

#[derive(Clone, Debug)]
pub struct ShieldedIcs20WithdrawalChangePublic {
    pub note_commitment: tct::StateCommitment,
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
    pub withdrawal_effect_hash_lo: Fq,
    pub withdrawal_effect_hash_hi: Fq,
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
pub struct ShieldedIcs20WithdrawalInputPrivate {
    pub state_commitment_proof: tct::Proof,
    pub spent_note: Note,
    pub spend_auth_randomizer: Fr,
    pub is_dummy: bool,
    pub dummy_nullifier_seed: Fq,
    pub dummy_spend_auth_key: Fr,
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
    pub sender_compliance_path: MerklePath,
    pub sender_compliance_position: u64,
    pub sender_leaf: ComplianceLeaf,
    pub inputs: Vec<ShieldedIcs20WithdrawalInputPrivate>,
    pub change_output: ShieldedIcs20WithdrawalChangePrivate,
}

impl ShieldedIcs20WithdrawalProofPrivate {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.family_id == ShieldedIcs20WithdrawalFamilyId::Canonical,
            "shielded ICS-20 withdrawal family must be canonical"
        );
        ensure!(
            self.inputs.len() == self.family_id.input_count(),
            "{} expects {} private inputs, got {}",
            self.family_id.label(),
            self.family_id.input_count(),
            self.inputs.len()
        );
        Ok(())
    }
}

impl ShieldedIcs20WithdrawalProof {
    fn decoded_proof(&self) -> anyhow::Result<Proof<decaf377::Bls12_377>> {
        Proof::deserialize_compressed(&self.inner[..]).map_err(|e| anyhow!(e))
    }

    pub fn to_batch_item(
        &self,
        public: &ShieldedIcs20WithdrawalProofPublic,
    ) -> anyhow::Result<penumbra_sdk_proof_params::batch::BatchItem> {
        let statement_hash = public.statement_hash()?;
        let proof = self.decoded_proof()?;

        Ok(penumbra_sdk_proof_params::batch::BatchItem {
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
        let _: [u8; GROTH16_PROOF_LENGTH_BYTES] = self
            .inner
            .clone()
            .try_into()
            .map_err(|_| anyhow!("malformed shielded ICS-20 withdrawal proof length"))?;
        Ok(())
    }

    #[cfg(any(unix, windows))]
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
        Ok(Self { inner: value.inner })
    }
}

#[cfg(test)]
mod tests {
    use super::ShieldedIcs20WithdrawalProof;
    use crate::{
        shielded_ics20_withdrawal::test_runtime, test_proof_helpers::proof_test_helpers,
        ShieldedIcs20WithdrawalFamilyId,
    };

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

    #[cfg(any(unix, windows))]
    #[test]
    fn shielded_ics20_withdrawal_proof_roundtrip() {
        if test_runtime::should_skip_shielded_ics20_withdrawal_proof_roundtrip_tests() {
            return;
        }

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
    }
}
