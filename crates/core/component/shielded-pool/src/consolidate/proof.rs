use anyhow::{anyhow, ensure, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalDeserialize;
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use penumbra_sdk_asset::balance;
use penumbra_sdk_keys::keys::NullifierKey;
use penumbra_sdk_proof_params::GROTH16_PROOF_LENGTH_BYTES;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct as tct;

use crate::{
    public_input_hash::{consolidate_statement_hash_from_public, StatementHashError},
    Note,
};

use super::ConsolidateFamilyId;

impl ConsolidateFamilyId {
    pub fn proof_verification_key(self) -> &'static PreparedVerifyingKey<Bls12_377> {
        penumbra_sdk_proof_params::consolidate_proof_verification_key(self.get())
    }

    pub fn proving_key_bytes(self) -> &'static [u8] {
        penumbra_sdk_proof_params::consolidate_proving_key_bytes(self.get())
    }

    pub fn circuit_metadata_bytes(self) -> &'static [u8] {
        penumbra_sdk_proof_params::consolidate_circuit_metadata(self.get())
    }
}

#[derive(Clone, Debug)]
pub struct ConsolidateInputPublic {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
}

#[derive(Clone, Debug)]
pub struct ConsolidateOutputPublic {
    pub note_commitment: tct::StateCommitment,
}

#[derive(Clone, Debug)]
pub struct ConsolidateProofPublic {
    pub family_id: ConsolidateFamilyId,
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub inputs: Vec<ConsolidateInputPublic>,
    pub outputs: Vec<ConsolidateOutputPublic>,
}

impl ConsolidateProofPublic {
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
        consolidate_statement_hash_from_public(self)
    }
}

#[derive(Clone, Debug)]
pub struct ConsolidateInputPrivate {
    pub state_commitment_proof: tct::Proof,
    pub spent_note: Note,
    pub spend_auth_randomizer: Fr,
}

#[derive(Clone, Debug)]
pub struct ConsolidateOutputPrivate {
    pub created_note: Note,
}

#[derive(Clone, Debug)]
pub struct ConsolidateProofPrivate {
    pub family_id: ConsolidateFamilyId,
    pub action_balance_blinding: Fr,
    pub ak: VerificationKey<SpendAuth>,
    pub nk: NullifierKey,
    pub inputs: Vec<ConsolidateInputPrivate>,
    pub outputs: Vec<ConsolidateOutputPrivate>,
}

impl ConsolidateProofPrivate {
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

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct ConsolidateProof {
    pub inner: Vec<u8>,
}

impl ConsolidateProof {
    pub fn new(inner: Vec<u8>) -> Self {
        Self { inner }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn decoded_proof(&self) -> anyhow::Result<Proof<Bls12_377>> {
        Proof::deserialize_compressed(&self.inner[..]).map_err(|e| anyhow!(e))
    }

    pub fn to_batch_item(
        &self,
        public: &ConsolidateProofPublic,
    ) -> anyhow::Result<penumbra_sdk_proof_params::batch::BatchItem> {
        let proof = self.decoded_proof()?;
        let statement_hash = public.statement_hash()?;

        Ok(penumbra_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    pub fn for_family(&self, _family_id: ConsolidateFamilyId) -> anyhow::Result<()> {
        let _: [u8; GROTH16_PROOF_LENGTH_BYTES] = self
            .inner
            .clone()
            .try_into()
            .map_err(|_| anyhow!("malformed consolidate proof length"))?;
        Ok(())
    }

    pub fn verify(&self, public: &ConsolidateProofPublic) -> anyhow::Result<()> {
        self.verify_with_prepared_vk(public, public.family_id.proof_verification_key())
    }

    pub fn verify_with_prepared_vk(
        &self,
        public: &ConsolidateProofPublic,
        vk: &PreparedVerifyingKey<Bls12_377>,
    ) -> anyhow::Result<()> {
        let item = self.to_batch_item(public)?;
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

    #[cfg(any(unix, windows))]
    pub fn prove(
        public: ConsolidateProofPublic,
        private: ConsolidateProofPrivate,
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
                "consolidate family mismatch: public={} private={}",
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

impl DomainType for ConsolidateProof {
    type Proto = pb::ZkConsolidateProof;
}

impl From<ConsolidateProof> for pb::ZkConsolidateProof {
    fn from(value: ConsolidateProof) -> Self {
        Self { inner: value.inner }
    }
}

impl TryFrom<pb::ZkConsolidateProof> for ConsolidateProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::ZkConsolidateProof) -> Result<Self, Self::Error> {
        Ok(Self { inner: value.inner })
    }
}
