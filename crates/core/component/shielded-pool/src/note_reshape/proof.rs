use anyhow::{anyhow, bail, ensure, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::{ComplianceLeaf, IndexedLeaf, MerklePath};
use shieldd_sdk_keys::keys::NullifierKey;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_tct as tct;

use crate::{
    discovery::{Parameters as RoutingParameters, RoutingTag},
    public_input_hash::{note_reshape_statement_hash_from_public, StatementHashError},
    Note,
};

use super::NoteReshapeFamilyId;

impl NoteReshapeFamilyId {
    pub fn deployed_proof_key(self) -> shieldd_sdk_proof_params::DeployedProofKey {
        match self.get() {
            2 => shieldd_sdk_proof_params::DeployedProofKey::NoteReshapeOneByEight,
            3 => shieldd_sdk_proof_params::DeployedProofKey::NoteReshapeEightByOne,
            unknown => panic!("validated note reshape family has unknown id {unknown}"),
        }
    }

    pub fn proof_verification_key(self) -> &'static PreparedVerifyingKey<Bls12_377> {
        self.deployed_proof_key().bundled_pvk()
    }

    pub fn proving_key_bytes(self) -> &'static [u8] {
        shieldd_sdk_proof_params::note_reshape_proving_key_bytes(self.get())
    }

    pub fn verifying_key_json_bytes(self) -> &'static [u8] {
        shieldd_sdk_proof_params::note_reshape_verifying_key_json_bytes(self.get())
    }

    pub fn circuit_metadata_bytes(self) -> &'static [u8] {
        shieldd_sdk_proof_params::note_reshape_circuit_metadata(self.get())
    }
}

#[derive(Clone, Debug)]
pub struct NoteReshapeInputPublic {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub history_required: bool,
}

#[derive(Clone, Debug)]
pub struct NoteReshapeOutputPublic {
    pub note_commitment: tct::StateCommitment,
}

#[derive(Clone, Debug)]
pub struct NoteReshapeProofPublic {
    pub family_id: NoteReshapeFamilyId,
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub asset_anchor: tct::StateCommitment,
    pub compliance_anchor: tct::StateCommitment,
    pub routing_tag: RoutingTag,
    pub routing_parameter_set_id: Fq,
    pub recent_position_floor: u64,
    pub inputs: Vec<NoteReshapeInputPublic>,
    pub outputs: Vec<NoteReshapeOutputPublic>,
}

impl NoteReshapeProofPublic {
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
        note_reshape_statement_hash_from_public(self)
    }
}

fn validate_dummy_suffix(label: &str, flags: &[bool]) -> Result<()> {
    let mut saw_dummy = false;
    for (index, is_dummy) in flags.iter().copied().enumerate() {
        if is_dummy {
            saw_dummy = true;
        } else if saw_dummy {
            bail!("note reshape {label} dummy flag at slot {index} is not a suffix")
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct NoteReshapeInputPrivate {
    pub state_commitment_proof: tct::Proof,
    pub spent_note: Note,
    pub spend_auth_randomizer: Fr,
    pub(crate) is_dummy: bool,
    pub(crate) dummy_nullifier_seed: Fq,
}

#[derive(Clone, Debug)]
pub struct NoteReshapeOutputPrivate {
    pub created_note: Note,
}

#[derive(Clone, Debug)]
pub struct NoteReshapeProofPrivate {
    pub family_id: NoteReshapeFamilyId,
    pub action_balance_blinding: Fr,
    pub ak: VerificationKey<SpendAuth>,
    pub nk: NullifierKey,
    pub asset_path: MerklePath,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeaf,
    pub sender_compliance_path: MerklePath,
    pub sender_compliance_position: u64,
    pub sender_leaf: ComplianceLeaf,
    pub is_regulated: bool,
    pub routing_parameters: RoutingParameters,
    pub routing_nonce: Fq,
    pub inputs: Vec<NoteReshapeInputPrivate>,
    pub outputs: Vec<NoteReshapeOutputPrivate>,
}

impl NoteReshapeProofPrivate {
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
        if self.family_id.spec().input_padding
            == super::generated::InputPaddingPolicy::SyntheticPrivate
        {
            validate_dummy_suffix(
                "private input",
                &self
                    .inputs
                    .iter()
                    .map(|input| input.is_dummy)
                    .collect::<Vec<_>>(),
            )?;
            ensure!(
                !self.inputs[0].is_dummy,
                "note reshape input slot 0 must be real"
            );
        } else {
            ensure!(
                self.inputs.iter().all(|input| !input.is_dummy),
                "fixed note reshape inputs cannot be dummy"
            );
        }
        self.family_id.validate_real_counts(
            self.inputs.iter().filter(|input| !input.is_dummy).count(),
            self.outputs.len(),
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct NoteReshapeProof {
    pub inner: Vec<u8>,
}

impl NoteReshapeProof {
    pub fn new(inner: Vec<u8>) -> Self {
        Self { inner }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn decoded_proof(&self) -> anyhow::Result<Proof<Bls12_377>> {
        crate::groth16_proof::decode(&self.inner)
    }

    pub(crate) fn to_batch_item(
        &self,
        public: &NoteReshapeProofPublic,
    ) -> anyhow::Result<shieldd_sdk_proof_params::batch::BatchItem> {
        let proof = self.decoded_proof()?;
        let statement_hash = public.statement_hash()?;

        Ok(shieldd_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    pub fn verify(&self, public: &NoteReshapeProofPublic) -> anyhow::Result<()> {
        self.verify_with_prepared_vk(public, public.family_id.proof_verification_key())
    }

    pub fn verify_with_prepared_vk(
        &self,
        public: &NoteReshapeProofPublic,
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
        public: NoteReshapeProofPublic,
        private: NoteReshapeProofPrivate,
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
                "note_reshape family mismatch: public={} private={}",
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

impl DomainType for NoteReshapeProof {
    type Proto = pb::ZkNoteReshapeProof;
}

impl From<NoteReshapeProof> for pb::ZkNoteReshapeProof {
    fn from(value: NoteReshapeProof) -> Self {
        Self { inner: value.inner }
    }
}

impl TryFrom<pb::ZkNoteReshapeProof> for NoteReshapeProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::ZkNoteReshapeProof) -> Result<Self, Self::Error> {
        let proof = Self { inner: value.inner };
        proof.decoded_proof()?;
        Ok(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::NoteReshapeProof;
    use crate::{note_reshape::NoteReshapeFamilyId, test_proof_helpers::proof_test_helpers};

    #[test]
    fn note_reshape_deployed_key_mapping_matches_generated_registry_for_every_family() {
        for family in NoteReshapeFamilyId::ALL {
            assert!(std::ptr::eq(
                family.deployed_proof_key().bundled_pvk(),
                shieldd_sdk_proof_params::note_reshape_proof_verification_key(family.get()),
            ));
        }
    }

    #[test]
    fn note_reshape_proof_public_shape_rejects_wrong_input_and_output_shapes() {
        for family_id in NoteReshapeFamilyId::ALL {
            let (mut public, _) =
                proof_test_helpers::build_note_reshape_roundtrip_inputs(family_id);
            public.inputs.pop();
            assert!(
                public.validate_shape().is_err(),
                "{} must reject an input shape mutation",
                family_id.label()
            );

            let (mut public, _) =
                proof_test_helpers::build_note_reshape_roundtrip_inputs(family_id);
            public.outputs.pop();
            assert!(
                public.validate_shape().is_err(),
                "{} must reject an output shape mutation",
                family_id.label()
            );
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_note_reshape_1x8_roundtrip() {
        assert_roundtrip(NoteReshapeFamilyId::ALL[0]);
    }

    #[cfg(any(unix, windows))]
    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_note_reshape_8x1_roundtrip() {
        assert_roundtrip(NoteReshapeFamilyId::ALL[1]);
    }

    #[cfg(any(unix, windows))]
    fn assert_roundtrip(family_id: NoteReshapeFamilyId) {
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::NoteReshape(
            family_id,
        ))
        .expect("proof test prerequisites must be present");
        let (public, private) = proof_test_helpers::build_note_reshape_roundtrip_inputs(family_id);
        let proof = NoteReshapeProof::prove(public.clone(), private)
            .unwrap_or_else(|error| panic!("prove {} fixture: {error}", family_id.label()));
        proof
            .verify(&public)
            .unwrap_or_else(|error| panic!("verify {} fixture: {error}", family_id.label()));
        for other_family in NoteReshapeFamilyId::ALL {
            if other_family != family_id {
                assert!(
                    proof
                        .verify_with_prepared_vk(&public, other_family.proof_verification_key())
                        .is_err(),
                    "{} proof must not verify with {} VK",
                    family_id.label(),
                    other_family.label()
                );
            }
        }
    }
}
