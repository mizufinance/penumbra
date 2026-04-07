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

use crate::{
    public_input_hash::{transfer_statement_hash_from_public, StatementHashError},
    Note, TransferFamilyId,
};

impl TransferFamilyId {
    pub fn proof_verification_key(self) -> &'static PreparedVerifyingKey<Bls12_377> {
        penumbra_sdk_proof_params::transfer_proof_verification_key(self.get())
    }

    pub fn proving_key_bytes(self) -> &'static [u8] {
        penumbra_sdk_proof_params::transfer_proving_key_bytes(self.get())
    }

    pub fn circuit_metadata_bytes(self) -> &'static [u8] {
        penumbra_sdk_proof_params::transfer_circuit_metadata(self.get())
    }
}

#[derive(Clone, Debug)]
pub struct TransferSpendPublic {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub epk: decaf377::Element,
    pub c2_core: Fq,
    pub compliance_ciphertext: Vec<Fq>,
    pub dleq_c: Fq,
    pub dleq_s: Fq,
}

#[derive(Clone, Debug)]
pub struct TransferOutputPublic {
    pub note_commitment: tct::StateCommitment,
    pub epk_1: decaf377::Element,
    pub epk_2: decaf377::Element,
    pub epk_3: decaf377::Element,
    pub c2_core: Fq,
    pub c2_ext: Fq,
    pub c2_sext: Fq,
    pub compliance_ciphertext: Vec<Fq>,
    pub dleq_c_1: Fq,
    pub dleq_s_1: Fq,
    pub dleq_c_2: Fq,
    pub dleq_s_2: Fq,
    pub dleq_c_3: Fq,
    pub dleq_s_3: Fq,
}

#[derive(Clone, Debug)]
pub struct TransferProofPublic {
    pub family_id: TransferFamilyId,
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub asset_anchor: tct::StateCommitment,
    pub compliance_anchor: tct::StateCommitment,
    pub target_timestamp: Fq,
    pub inputs: Vec<TransferSpendPublic>,
    pub outputs: Vec<TransferOutputPublic>,
}

impl TransferProofPublic {
    pub fn validate_shape(&self) -> Result<()> {
        let spec = self.family_id.spec();
        ensure!(
            self.inputs.len() == spec.n_in,
            "{} expects {} inputs, got {}",
            spec.label,
            spec.n_in,
            self.inputs.len()
        );
        ensure!(
            self.outputs.len() == spec.n_out,
            "{} expects {} outputs, got {}",
            spec.label,
            spec.n_out,
            self.outputs.len()
        );
        Ok(())
    }

    pub fn statement_hash(&self) -> Result<Fq, StatementHashError> {
        transfer_statement_hash_from_public(self)
    }
}

#[derive(Clone, Debug)]
pub struct TransferSpendPrivate {
    pub state_commitment_proof: tct::Proof,
    pub spent_note: Note,
    pub spend_auth_randomizer: Fr,
    pub spend_compliance_ephemeral_secret: Fr,
    pub spend_is_flagged: bool,
    pub spend_salt: Fq,
}

#[derive(Clone, Debug)]
pub struct TransferOutputPrivate {
    pub created_note: Note,
    pub recipient_compliance_path: MerklePath,
    pub recipient_compliance_position: u64,
    pub recipient_leaf: ComplianceLeaf,
    pub output_compliance_ephemeral_secret: Fr,
    pub output_r_2: Fr,
    pub output_r_3: Fr,
    pub output_is_flagged: bool,
    pub output_salt: Fq,
}

#[derive(Clone, Debug)]
pub struct TransferProofPrivate {
    pub family_id: TransferFamilyId,
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
    pub tx_blinding_nonce: Fr,
    pub inputs: Vec<TransferSpendPrivate>,
    pub outputs: Vec<TransferOutputPrivate>,
}

impl TransferProofPrivate {
    pub fn validate_shape(&self) -> Result<()> {
        let spec = self.family_id.spec();
        ensure!(
            self.inputs.len() == spec.n_in,
            "{} expects {} private inputs, got {}",
            spec.label,
            spec.n_in,
            self.inputs.len()
        );
        ensure!(
            self.outputs.len() == spec.n_out,
            "{} expects {} private outputs, got {}",
            spec.label,
            spec.n_out,
            self.outputs.len()
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct TransferProof {
    pub inner: Vec<u8>,
}

impl TransferProof {
    fn decoded_proof(&self) -> anyhow::Result<Proof<Bls12_377>> {
        Proof::deserialize_compressed(&self.inner[..]).map_err(|e| anyhow!(e))
    }

    pub fn to_batch_item(
        &self,
        public: &TransferProofPublic,
    ) -> anyhow::Result<penumbra_sdk_proof_params::batch::BatchItem> {
        let proof = self.decoded_proof()?;
        let statement_hash = public.statement_hash()?;

        Ok(penumbra_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    pub fn verify(&self, public: &TransferProofPublic) -> anyhow::Result<()> {
        let item = self.to_batch_item(public)?;
        let vk = public.family_id.proof_verification_key();
        let proof_result = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            vk,
            item.public_inputs.as_slice(),
            &item.proof,
        )
        .map_err(|err| anyhow!(err))?;

        proof_result
            .then_some(())
            .ok_or_else(|| anyhow!("{} proof did not verify", public.family_id.label()))
    }

    pub fn for_family(&self, _family_id: TransferFamilyId) -> anyhow::Result<()> {
        let _: [u8; GROTH16_PROOF_LENGTH_BYTES] = self
            .inner
            .clone()
            .try_into()
            .map_err(|_| anyhow!("malformed transfer proof length"))?;
        Ok(())
    }

    #[cfg(any(unix, windows))]
    pub fn prove(
        public: TransferProofPublic,
        private: TransferProofPrivate,
    ) -> Result<Self, crate::ProofError> {
        let family_id = public.family_id;
        public
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        private
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;

        let prove_result = super::prover_runtime::prove_with_runtime(public, private);

        prove_result.map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} prove: {e}",
                family_id.label()
            ))
        })
    }
}

impl DomainType for TransferProof {
    type Proto = pb::ZkTransferProof;
}

impl From<TransferProof> for pb::ZkTransferProof {
    fn from(proof: TransferProof) -> Self {
        Self { inner: proof.inner }
    }
}

impl TryFrom<pb::ZkTransferProof> for TransferProof {
    type Error = anyhow::Error;

    fn try_from(proto: pb::ZkTransferProof) -> Result<Self, Self::Error> {
        Ok(Self { inner: proto.inner })
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use std::sync::{LazyLock, Mutex};

    use crate::test_proof_helpers::proof_test_helpers::{full_proof_roundtrip, CircuitType};
    use crate::TransferFamilyId;

    static TRANSFER_PROOF_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn transfer_proof_roundtrip_regulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }
        for family_id in TransferFamilyId::ALL {
            full_proof_roundtrip(CircuitType::Transfer(family_id), true);
        }
    }

    #[test]
    fn transfer_proof_roundtrip_unregulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }
        for family_id in TransferFamilyId::ALL {
            full_proof_roundtrip(CircuitType::Transfer(family_id), false);
        }
    }
}
