use anyhow::{anyhow, ensure, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::{ComplianceLeaf, IndexedLeaf, MerklePath, TransferComplianceMetadata};
use shieldd_sdk_keys::keys::NullifierKey;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_tct as tct;

use crate::{
    discovery::{Parameters as RoutingParameters, TransferRouting},
    public_input_hash::{transfer_statement_hash_from_public, StatementHashError},
    transfer::{transfer_input_count, transfer_output_count, TRANSFER_PROOF_LABEL},
    Note, TransferProofContext, VolumeAccumulatorPrivate, VolumeAccumulatorPublic,
};

#[derive(Clone, Debug)]
pub struct TransferSpendPublic {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
    pub history_required: bool,
}

#[derive(Clone, Debug)]
pub struct TransferOutputPublic {
    pub note_commitment: tct::StateCommitment,
    pub recovery_commitment: crate::RecoveryCommitment,
}

#[derive(Clone, Debug)]
pub struct TransferComplianceCiphertextPublic {
    pub epk: decaf377::Element,
    pub c2: Fq,
    pub ciphertext: Vec<Fq>,
}

#[derive(Clone, Debug)]
pub struct TransferCompliancePublic {
    pub detection_ciphertext: Vec<Fq>,
    pub metadata: TransferComplianceMetadata,
    pub sender_core_key_confirmation: Fq,
    pub output_core_key_confirmation: Fq,
    pub sender_core: TransferComplianceCiphertextPublic,
    pub sender_ext: TransferComplianceCiphertextPublic,
    pub output_core: TransferComplianceCiphertextPublic,
    pub output_ext: TransferComplianceCiphertextPublic,
}

#[derive(Clone, Debug)]
pub struct TransferProofPublic {
    pub anchor: tct::Root,
    pub balance_commitment: balance::Commitment,
    pub asset_anchor: tct::StateCommitment,
    pub compliance_anchor: tct::StateCommitment,
    pub target_timestamp: Fq,
    pub inputs: Vec<TransferSpendPublic>,
    pub outputs: Vec<TransferOutputPublic>,
    pub compliance: TransferCompliancePublic,
    pub routing: TransferRouting,
    pub routing_parameter_set_id: Fq,
    pub recent_position_floor: u64,
    pub volume_accumulator: VolumeAccumulatorPublic,
    pub proof_context: TransferProofContext,
}

impl TransferProofPublic {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.inputs.len() == transfer_input_count(),
            "{} expects {} inputs, got {}",
            TRANSFER_PROOF_LABEL,
            transfer_input_count(),
            self.inputs.len()
        );
        ensure!(
            self.outputs.len() == transfer_output_count(),
            "{} expects {} outputs, got {}",
            TRANSFER_PROOF_LABEL,
            transfer_output_count(),
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
}

#[derive(Clone, Debug)]
pub struct TransferOptionalSpendPrivate {
    pub spend: TransferSpendPrivate,
    pub is_dummy: bool,
    pub dummy_nullifier_seed: Fq,
}

#[derive(Clone, Debug)]
pub struct TransferReceiverOutputPrivate {
    pub created_note: Note,
    pub recipient_compliance_path: MerklePath,
    pub recipient_compliance_position: u64,
    pub recipient_leaf: ComplianceLeaf,
}

#[derive(Clone, Debug)]
pub struct TransferChangeOutputPrivate {
    pub created_note: Note,
}

#[derive(Clone, Debug)]
pub struct TransferTierRandomizers {
    pub core: Fr,
    pub ext: Fr,
}

#[derive(Clone, Debug)]
pub struct TransferCompliancePrivate {
    pub transfer_nonce_root: Fr,
    pub sender: TransferTierRandomizers,
    pub output: TransferTierRandomizers,
}

#[derive(Clone, Debug)]
pub struct TransferProofPrivate {
    pub action_balance_blinding: Fr,
    pub ak: VerificationKey<SpendAuth>,
    pub nk: NullifierKey,
    pub asset_path: MerklePath,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeaf,
    pub is_regulated: bool,
    pub routing_parameters: RoutingParameters,
    pub sender_compliance_path: MerklePath,
    pub sender_compliance_position: u64,
    pub sender_leaf: ComplianceLeaf,
    pub compliance: TransferCompliancePrivate,
    pub required_input: TransferSpendPrivate,
    pub optional_input: TransferOptionalSpendPrivate,
    pub receiver_output: TransferReceiverOutputPrivate,
    pub change_output: TransferChangeOutputPrivate,
    pub volume_accumulator: VolumeAccumulatorPrivate,
}

#[derive(Clone, Debug, Default)]
pub struct TransferProof {
    pub inner: Vec<u8>,
}

impl TransferProof {
    fn decoded_proof(&self) -> anyhow::Result<Proof<Bls12_377>> {
        crate::groth16_proof::decode(&self.inner)
    }

    pub(crate) fn to_batch_item(
        &self,
        public: &TransferProofPublic,
    ) -> anyhow::Result<shieldd_sdk_proof_params::batch::BatchItem> {
        let proof = self.decoded_proof()?;
        let statement_hash = public.statement_hash()?;

        Ok(shieldd_sdk_proof_params::batch::BatchItem {
            proof,
            public_inputs: vec![statement_hash],
        })
    }

    pub fn verify(&self, public: &TransferProofPublic) -> anyhow::Result<()> {
        self.verify_with_prepared_vk(
            public,
            shieldd_sdk_proof_params::transfer_proof_verification_key(),
        )
    }

    pub fn verify_with_prepared_vk(
        &self,
        public: &TransferProofPublic,
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
            .ok_or_else(|| anyhow!("{TRANSFER_PROOF_LABEL} proof did not verify"))
    }

    pub fn validate_encoding(&self) -> anyhow::Result<()> {
        self.decoded_proof()?;
        Ok(())
    }

    #[cfg(any(unix, windows))]
    pub fn prove(
        public: TransferProofPublic,
        private: TransferProofPrivate,
    ) -> Result<Self, crate::ProofError> {
        public
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        let prove_result = super::prover_runtime::prove_with_runtime(public, private);

        prove_result.map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {TRANSFER_PROOF_LABEL} prove: {e}"
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
        let proof = Self { inner: proto.inner };
        proof.validate_encoding()?;
        Ok(proof)
    }
}

#[cfg(feature = "component")]
#[cfg(all(test, any(unix, windows)))]
mod tests {
    use std::sync::{LazyLock, Mutex};

    use super::TransferProof;
    #[cfg(feature = "component")]
    use crate::component::transfer_extract_public;
    #[cfg(feature = "component")]
    use crate::test_proof_helpers::proof_test_helpers::build_transfer_action_and_public;
    use crate::test_proof_helpers::proof_test_helpers::{
        build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng, full_proof_roundtrip,
        CircuitType,
    };
    use crate::{
        Note, RecoveryCommitment, Rseed, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan,
    };
    use decaf377::Fr;
    use shieldd_sdk_asset::{Value, BASE_ASSET_ID};
    use shieldd_sdk_compliance::{ComplianceLeaf, MerklePath, QuadTree};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_num::Amount;
    use shieldd_sdk_tct as tct;

    static TRANSFER_PROOF_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn compliance_leaf_for(address: &shieldd_sdk_keys::Address) -> ComplianceLeaf {
        ComplianceLeaf::synthetic_unregulated(address.clone(), *BASE_ASSET_ID)
    }

    fn sender_recipient_compliance_witnesses() -> (
        ComplianceLeaf,
        ComplianceLeaf,
        tct::StateCommitment,
        MerklePath,
        MerklePath,
    ) {
        let sender_leaf = compliance_leaf_for(&test_keys::ADDRESS_0);
        let recipient_leaf = compliance_leaf_for(&test_keys::ADDRESS_1);
        let mut user_tree = QuadTree::new();
        user_tree
            .update(0, sender_leaf.commit())
            .expect("insert sender user leaf");
        user_tree
            .update(1, recipient_leaf.commit())
            .expect("insert recipient user leaf");
        let compliance_anchor = tct::StateCommitment(user_tree.root().0);
        let sender_compliance_path =
            MerklePath::from_auth_path(user_tree.auth_path(0).expect("sender auth path"));
        let recipient_compliance_path =
            MerklePath::from_auth_path(user_tree.auth_path(1).expect("recipient auth path"));

        (
            sender_leaf,
            recipient_leaf,
            compliance_anchor,
            sender_compliance_path,
            recipient_compliance_path,
        )
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_proof_roundtrip_regulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");
        full_proof_roundtrip(CircuitType::Transfer, true);
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_proof_roundtrip_unregulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");
        full_proof_roundtrip(CircuitType::Transfer, false);
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_accumulator_origin() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_accumulating_hidden_arity_roundtrip_inputs_with_rng(
                &mut rand::thread_rng(),
                100,
            );
        TransferProof::prove(public.clone(), private)
            .expect("prove transfer with a daily accumulator origin")
            .verify(&public)
            .expect("verify transfer with a daily accumulator origin");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_accumulator_continuation() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_continuing_accumulator_roundtrip_inputs_with_rng(
                &mut rand::thread_rng(),
            );
        TransferProof::prove(public.clone(), private)
            .expect("prove transfer continuing a daily accumulator")
            .verify(&public)
            .expect("verify transfer continuing a daily accumulator");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_sender_to_self() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_with_rng(
                &mut rand::thread_rng(),
                false,
                true,
            );
        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity sender-to-self transfer")
            .verify(&public)
            .expect("verify hidden-arity sender-to-self transfer");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_repro_unregulated_nonbase_test_usd() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");
        // test_usd real asset id (base denom wtest_usd).
        let test_usd = shieldd_sdk_asset::asset::REGISTRY
            .parse_unit("test_usd")
            .id();
        eprintln!("test_usd id = {}", test_usd.0);
        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng(
                &mut rand::thread_rng(),
                test_usd,
                false,
                false,
            );
        TransferProof::prove(public.clone(), private)
            .expect("prove unregulated non-base test_usd transfer")
            .verify(&public)
            .expect("verify unregulated non-base test_usd transfer");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_repro_unregulated_nonbase_test_usd_populated_tree() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");
        let test_usd = shieldd_sdk_asset::asset::REGISTRY
            .parse_unit("test_usd")
            .id();
        // Predecessor (low) leaf is a regulated asset just below test_usd, mirroring
        // the live registry gap; daily_volume_limit = 5e20 like regulated_usd.
        let low_asset_id = test_usd.0 - decaf377::Fq::from(1u64);
        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_for_asset_populated(
                &mut rand::thread_rng(),
                test_usd,
                low_asset_id,
                500_000_000_000_000_000_000u128,
                false,
            );
        TransferProof::prove(public.clone(), private)
            .expect("prove unregulated non-base test_usd transfer (populated tree)")
            .verify(&public)
            .expect("verify unregulated non-base test_usd transfer (populated tree)");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_with_rng(
                &mut rand::thread_rng(),
                false,
                false,
            );
        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity sender-to-other transfer")
            .verify(&public)
            .expect("verify hidden-arity sender-to-other transfer");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_base_asset_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let (public, private) = build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng(
            &mut rand::thread_rng(),
            *BASE_ASSET_ID,
            false,
            false,
        );
        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity base-asset sender-to-other transfer")
            .verify(&public)
            .expect("verify hidden-arity base-asset sender-to-other transfer");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_test_keys_base_asset_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("create base-asset test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, input_note.commit())
            .expect("insert base-asset test note");
        let state_commitment_proof = sct
            .witness(input_note.commit())
            .expect("witness base-asset test note");
        let anchor = sct.root();

        let (
            sender_leaf,
            recipient_leaf,
            compliance_anchor,
            sender_compliance_path,
            recipient_compliance_path,
        ) = sender_recipient_compliance_witnesses();
        let spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );
        let output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());
        let mut compliance =
            crate::test_plan_helpers::transfer_context(&spend, &output.dest_address);
        compliance.witness.user_root = compliance_anchor;
        compliance.witness.sender = crate::UserWitness {
            leaf: sender_leaf,
            path: sender_compliance_path,
            position: 0,
        };
        compliance.recipient = crate::UserWitness {
            leaf: recipient_leaf,
            path: recipient_compliance_path,
            position: 1,
        };
        let transfer = TransferPlan::new(
            vec![spend],
            vec![output],
            Fr::rand(&mut rng),
            compliance.clone(),
            crate::VolumeAccumulatorPlan::padding(compliance.timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build test-key transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
                0,
            )
            .expect("derive test-key transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity test-key base-asset sender-to-other transfer")
            .verify(&public)
            .expect("verify hidden-arity test-key base-asset sender-to-other transfer");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_registered_base_asset_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("create registered base-asset test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, input_note.commit())
            .expect("insert registered base-asset note");
        let state_commitment_proof = sct
            .witness(input_note.commit())
            .expect("witness registered base-asset note");
        let anchor = sct.root();

        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            shieldd_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);
        let (
            sender_leaf,
            recipient_leaf,
            compliance_anchor,
            sender_compliance_path,
            recipient_compliance_path,
        ) = sender_recipient_compliance_witnesses();

        let spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );

        let output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());

        let mut compliance =
            crate::test_plan_helpers::transfer_context(&spend, &output.dest_address);
        compliance.witness.user_root = compliance_anchor;
        compliance.witness.sender = crate::UserWitness {
            leaf: sender_leaf,
            path: sender_compliance_path,
            position: 0,
        };
        compliance.recipient = crate::UserWitness {
            leaf: recipient_leaf,
            path: recipient_compliance_path,
            position: 1,
        };
        compliance.witness.asset.root = asset_anchor;
        compliance.witness.asset.leaf = asset_indexed_leaf;
        compliance.witness.asset.path = asset_path;
        compliance.witness.asset.position = asset_position;
        let transfer = TransferPlan::new(
            vec![spend],
            vec![output],
            Fr::rand(&mut rng),
            compliance.clone(),
            crate::VolumeAccumulatorPlan::padding(compliance.timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build registered base-asset transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
                0,
            )
            .expect("derive registered base-asset transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity registered base-asset sender-to-other transfer")
            .verify(&public)
            .expect("verify hidden-arity registered base-asset sender-to-other transfer");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_registered_base_asset_sender_to_other_high_position(
    ) {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("create registered base-asset test note");

        let mut sct = tct::Tree::new();
        for _ in 0..512 {
            let filler_note = Note::from_parts(
                test_keys::ADDRESS_1.clone(),
                Value {
                    amount: Amount::from(1u64),
                    asset_id: *BASE_ASSET_ID,
                },
                Rseed::generate(&mut rng),
                RecoveryCommitment::unavailable(),
            )
            .expect("create filler note");
            sct.insert(tct::Witness::Forget, filler_note.commit())
                .expect("insert filler note");
        }
        sct.insert(tct::Witness::Keep, input_note.commit())
            .expect("insert registered base-asset note");
        let state_commitment_proof = sct
            .witness(input_note.commit())
            .expect("witness registered base-asset note");
        let anchor = sct.root();

        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            shieldd_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);
        let (
            sender_leaf,
            recipient_leaf,
            compliance_anchor,
            sender_compliance_path,
            recipient_compliance_path,
        ) = sender_recipient_compliance_witnesses();

        let spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );

        let output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());

        let mut compliance =
            crate::test_plan_helpers::transfer_context(&spend, &output.dest_address);
        compliance.witness.user_root = compliance_anchor;
        compliance.witness.sender = crate::UserWitness {
            leaf: sender_leaf,
            path: sender_compliance_path,
            position: 0,
        };
        compliance.recipient = crate::UserWitness {
            leaf: recipient_leaf,
            path: recipient_compliance_path,
            position: 1,
        };
        compliance.witness.asset.root = asset_anchor;
        compliance.witness.asset.leaf = asset_indexed_leaf;
        compliance.witness.asset.path = asset_path;
        compliance.witness.asset.position = asset_position;
        let transfer = TransferPlan::new(
            vec![spend],
            vec![output],
            Fr::rand(&mut rng),
            compliance.clone(),
            crate::VolumeAccumulatorPlan::padding(compliance.timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build registered base-asset transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
                0,
            )
            .expect("derive registered base-asset transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity registered base-asset sender-to-other transfer at high position")
            .verify(&public)
            .expect("verify hidden-arity registered base-asset sender-to-other transfer at high position");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x1_roundtrip_registered_base_asset_sender_to_other_real_user_tree(
    ) {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("create registered base-asset test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, input_note.commit())
            .expect("insert registered base-asset note");
        let state_commitment_proof = sct
            .witness(input_note.commit())
            .expect("witness registered base-asset note");
        let anchor = sct.root();

        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            shieldd_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);
        let (
            sender_leaf,
            recipient_leaf,
            compliance_anchor,
            sender_compliance_path,
            recipient_compliance_path,
        ) = sender_recipient_compliance_witnesses();

        let spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );

        let output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());

        let mut compliance =
            crate::test_plan_helpers::transfer_context(&spend, &output.dest_address);
        compliance.witness.user_root = compliance_anchor;
        compliance.witness.sender = crate::UserWitness {
            leaf: sender_leaf,
            path: sender_compliance_path,
            position: 0,
        };
        compliance.recipient = crate::UserWitness {
            leaf: recipient_leaf,
            path: recipient_compliance_path,
            position: 1,
        };
        compliance.witness.asset.root = asset_anchor;
        compliance.witness.asset.leaf = asset_indexed_leaf;
        compliance.witness.asset.path = asset_path;
        compliance.witness.asset.position = asset_position;
        let transfer = TransferPlan::new(
            vec![spend],
            vec![output],
            Fr::rand(&mut rng),
            compliance.clone(),
            crate::VolumeAccumulatorPlan::padding(compliance.timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build registered base-asset transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
                0,
            )
            .expect("derive registered base-asset transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity registered base-asset transfer with real user tree")
            .verify(&public)
            .expect("verify hidden-arity registered base-asset transfer with real user tree");
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_hidden_arity_1x2_roundtrip_registered_base_asset_with_change_real_user_tree(
    ) {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("create registered base-asset test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, input_note.commit())
            .expect("insert registered base-asset note");
        let state_commitment_proof = sct
            .witness(input_note.commit())
            .expect("witness registered base-asset note");
        let anchor = sct.root();

        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            shieldd_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);
        let (
            sender_leaf,
            recipient_leaf,
            compliance_anchor,
            sender_compliance_path,
            recipient_compliance_path,
        ) = sender_recipient_compliance_witnesses();

        let spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );

        let receiver_output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: Amount::from(1u64),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_1.clone(),
        );

        let change_output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: Amount::from(999_999u64),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_0.clone(),
        );

        let mut compliance =
            crate::test_plan_helpers::transfer_context(&spend, &receiver_output.dest_address);
        compliance.witness.user_root = compliance_anchor;
        compliance.witness.sender = crate::UserWitness {
            leaf: sender_leaf,
            path: sender_compliance_path,
            position: 0,
        };
        compliance.recipient = crate::UserWitness {
            leaf: recipient_leaf,
            path: recipient_compliance_path,
            position: 1,
        };
        compliance.witness.asset.root = asset_anchor;
        compliance.witness.asset.leaf = asset_indexed_leaf;
        compliance.witness.asset.path = asset_path;
        compliance.witness.asset.position = asset_position;
        let transfer = TransferPlan::new(
            vec![spend],
            vec![receiver_output, change_output],
            Fr::rand(&mut rng),
            compliance.clone(),
            crate::VolumeAccumulatorPlan::padding(compliance.timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build registered base-asset transfer plan with change");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
                0,
            )
            .expect("derive registered base-asset transfer-with-change public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect(
                "prove hidden-arity registered base-asset transfer with change and real user tree",
            )
            .verify(&public)
            .expect(
                "verify hidden-arity registered base-asset transfer with change and real user tree",
            );
    }

    #[cfg(feature = "component")]
    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_transfer_action_public_matches_proving_public_regulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Transfer)
            .expect("proof test prerequisites must be present");

        let (transfer, proving_public, context) = build_transfer_action_and_public(true);
        let extracted_public =
            transfer_extract_public(&transfer, &context).expect("extract transfer public");

        assert_eq!(proving_public.anchor, extracted_public.anchor);
        assert_eq!(
            proving_public.balance_commitment,
            extracted_public.balance_commitment
        );
        assert_eq!(proving_public.asset_anchor, extracted_public.asset_anchor);
        assert_eq!(
            proving_public.compliance_anchor,
            extracted_public.compliance_anchor
        );
        assert_eq!(
            proving_public.target_timestamp,
            extracted_public.target_timestamp
        );
        assert_eq!(proving_public.inputs.len(), extracted_public.inputs.len());
        for (expected, actual) in proving_public
            .inputs
            .iter()
            .zip(extracted_public.inputs.iter())
        {
            assert_eq!(expected.nullifier, actual.nullifier);
            assert_eq!(expected.rk, actual.rk);
        }
        assert_eq!(proving_public.outputs.len(), extracted_public.outputs.len());
        for (expected, actual) in proving_public
            .outputs
            .iter()
            .zip(extracted_public.outputs.iter())
        {
            assert_eq!(expected.note_commitment, actual.note_commitment);
        }
        assert_eq!(
            proving_public.compliance.detection_ciphertext,
            extracted_public.compliance.detection_ciphertext
        );
        assert_eq!(
            proving_public.compliance.sender_core.epk,
            extracted_public.compliance.sender_core.epk
        );
        assert_eq!(
            proving_public.compliance.sender_core.c2,
            extracted_public.compliance.sender_core.c2
        );
        assert_eq!(
            proving_public.compliance.sender_core.ciphertext,
            extracted_public.compliance.sender_core.ciphertext
        );
        assert_eq!(
            proving_public.compliance.sender_ext.epk,
            extracted_public.compliance.sender_ext.epk
        );
        assert_eq!(
            proving_public.compliance.sender_ext.c2,
            extracted_public.compliance.sender_ext.c2
        );
        assert_eq!(
            proving_public.compliance.sender_ext.ciphertext,
            extracted_public.compliance.sender_ext.ciphertext
        );
        assert_eq!(
            proving_public.compliance.output_core.epk,
            extracted_public.compliance.output_core.epk
        );
        assert_eq!(
            proving_public.compliance.output_core.c2,
            extracted_public.compliance.output_core.c2
        );
        assert_eq!(
            proving_public.compliance.output_core.ciphertext,
            extracted_public.compliance.output_core.ciphertext
        );
        assert_eq!(
            proving_public.compliance.output_ext.epk,
            extracted_public.compliance.output_ext.epk
        );
        assert_eq!(
            proving_public.compliance.output_ext.c2,
            extracted_public.compliance.output_ext.c2
        );
        assert_eq!(
            proving_public.compliance.output_ext.ciphertext,
            extracted_public.compliance.output_ext.ciphertext
        );
        assert_eq!(
            proving_public.compliance.metadata,
            extracted_public.compliance.metadata
        );

        assert_eq!(
            proving_public
                .statement_hash()
                .expect("proving statement hash"),
            extracted_public
                .statement_hash()
                .expect("extracted statement hash"),
            "extracted transfer public must match proving public",
        );

        let item = transfer
            .proof
            .to_batch_item(&extracted_public)
            .expect("build batch item from extracted transfer public");
        shieldd_sdk_proof_params::batch::batch_verify(
            shieldd_sdk_proof_params::transfer_proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .expect("single-item batch verification should succeed with extracted public");
    }
}
