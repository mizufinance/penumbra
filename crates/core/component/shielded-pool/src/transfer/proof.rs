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
    transfer::{transfer_input_count, transfer_output_count, TRANSFER_PROOF_LABEL},
    Note,
};

#[derive(Clone, Debug)]
pub struct TransferSpendPublic {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
}

#[derive(Clone, Debug)]
pub struct TransferOutputPublic {
    pub note_commitment: tct::StateCommitment,
}

#[derive(Clone, Debug)]
pub struct TransferComplianceCiphertextPublic {
    pub epk: decaf377::Element,
    pub c2: Fq,
    pub ciphertext: Vec<Fq>,
}

#[derive(Clone, Debug)]
pub struct TransferComplianceDleqPublic {
    pub c: Fq,
    pub s: Fq,
}

#[derive(Clone, Debug)]
pub struct TransferCompliancePublic {
    pub detection_ciphertext: Vec<Fq>,
    pub sender_core: TransferComplianceCiphertextPublic,
    pub sender_ext: TransferComplianceCiphertextPublic,
    pub output_core: TransferComplianceCiphertextPublic,
    pub output_ext: TransferComplianceCiphertextPublic,
    pub sender_core_dleq: TransferComplianceDleqPublic,
    pub sender_ext_dleq: TransferComplianceDleqPublic,
    pub output_core_dleq: TransferComplianceDleqPublic,
    pub output_ext_dleq: TransferComplianceDleqPublic,
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
    pub is_dummy: bool,
    pub dummy_nullifier_seed: Fq,
    pub dummy_spend_auth_key: Fr,
}

#[derive(Clone, Debug)]
pub struct TransferOutputPrivate {
    pub created_note: Note,
    pub recipient_compliance_path: MerklePath,
    pub recipient_compliance_position: u64,
    pub recipient_leaf: ComplianceLeaf,
    /// Output 0 is always the external receiver leg. Output 1, when present, is sender-owned change.
    pub is_receiver: bool,
}

#[derive(Clone, Debug)]
pub struct TransferCompliancePrivate {
    pub transfer_nonce_root: Fr,
    pub sender_r_core: Fr,
    pub sender_r_ext: Fr,
    pub output_r_core: Fr,
    pub output_r_ext: Fr,
    pub is_flagged: bool,
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
    pub sender_compliance_path: MerklePath,
    pub sender_compliance_position: u64,
    pub sender_leaf: ComplianceLeaf,
    pub compliance: TransferCompliancePrivate,
    pub inputs: Vec<TransferSpendPrivate>,
    pub outputs: Vec<TransferOutputPrivate>,
}

impl TransferProofPrivate {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.inputs.len() == transfer_input_count(),
            "{} expects {} private inputs, got {}",
            TRANSFER_PROOF_LABEL,
            transfer_input_count(),
            self.inputs.len()
        );
        ensure!(
            self.outputs.len() == transfer_output_count(),
            "{} expects {} private outputs, got {}",
            TRANSFER_PROOF_LABEL,
            transfer_output_count(),
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
        self.verify_with_prepared_vk(
            public,
            penumbra_sdk_proof_params::transfer_proof_verification_key(),
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
        public
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        private
            .validate_shape()
            .map_err(|e| crate::ProofError::InvalidPrivateInput(e.to_string()))?;

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
        Ok(Self { inner: proto.inner })
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use std::sync::{LazyLock, Mutex};

    use super::TransferProof;
    use crate::component::transfer_extract_public;
    use crate::test_proof_helpers::proof_test_helpers::{
        build_transfer_action_and_public,
        build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng, full_proof_roundtrip,
        CircuitType,
    };
    use crate::{Note, Rseed, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
    use decaf377::Fr;
    use penumbra_sdk_asset::{Value, BASE_ASSET_ID};
    use penumbra_sdk_compliance::{ComplianceLeaf, MerklePath, QuadTree};
    use penumbra_sdk_keys::test_keys;
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_tct as tct;

    static TRANSFER_PROOF_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn transfer_proof_roundtrip_regulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }
        full_proof_roundtrip(CircuitType::Transfer, true);
    }

    #[test]
    fn transfer_proof_roundtrip_unregulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }
        full_proof_roundtrip(CircuitType::Transfer, false);
    }

    #[test]
    fn transfer_hidden_arity_1x1_roundtrip_sender_to_self() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

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
    fn transfer_hidden_arity_1x1_roundtrip_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

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
    fn transfer_hidden_arity_1x1_roundtrip_base_asset_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

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
    fn transfer_hidden_arity_1x1_roundtrip_test_keys_base_asset_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
        )
        .expect("create base-asset test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, input_note.commit())
            .expect("insert base-asset test note");
        let state_commitment_proof = sct
            .witness(input_note.commit())
            .expect("witness base-asset test note");
        let anchor = sct.root();

        let spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );
        let output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());
        let transfer = TransferPlan::new(vec![spend], vec![output], Fr::rand(&mut rng))
            .expect("build test-key transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
            )
            .expect("derive test-key transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity test-key base-asset sender-to-other transfer")
            .verify(&public)
            .expect("verify hidden-arity test-key base-asset sender-to-other transfer");
    }

    #[test]
    fn transfer_hidden_arity_1x1_roundtrip_registered_base_asset_sender_to_other() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
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
            penumbra_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);

        let mut spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );
        spend.asset_indexed_leaf = asset_indexed_leaf.clone();
        spend.asset_path = asset_path.clone();
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.is_regulated = false;
        spend
            .set_compliance_details(&mut rng)
            .expect("set registered base-asset spend compliance details");

        let mut output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());
        output.asset_indexed_leaf = asset_indexed_leaf;
        output.asset_path = asset_path;
        output.asset_position = asset_position;
        output.asset_anchor = asset_anchor;
        output.is_regulated = false;
        let sender_leaf = spend
            .compliance_leaf
            .clone()
            .expect("registered base-asset sender leaf");
        let recipient_leaf = output
            .compliance_leaf
            .clone()
            .expect("registered base-asset recipient leaf");
        output
            .set_compliance_details(
                &mut rng,
                &recipient_leaf,
                sender_leaf,
                spend.tx_blinding_nonce,
            )
            .expect("set registered base-asset output compliance details");

        let transfer = TransferPlan::new(vec![spend], vec![output], Fr::rand(&mut rng))
            .expect("build registered base-asset transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
            )
            .expect("derive registered base-asset transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity registered base-asset sender-to-other transfer")
            .verify(&public)
            .expect("verify hidden-arity registered base-asset sender-to-other transfer");
    }

    #[test]
    fn transfer_hidden_arity_1x1_roundtrip_registered_base_asset_sender_to_other_high_position() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
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
            penumbra_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);

        let mut spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );
        spend.asset_indexed_leaf = asset_indexed_leaf.clone();
        spend.asset_path = asset_path.clone();
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.is_regulated = false;
        spend
            .set_compliance_details(&mut rng)
            .expect("set registered base-asset spend compliance details");

        let mut output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());
        output.asset_indexed_leaf = asset_indexed_leaf;
        output.asset_path = asset_path;
        output.asset_position = asset_position;
        output.asset_anchor = asset_anchor;
        output.is_regulated = false;
        let sender_leaf = spend
            .compliance_leaf
            .clone()
            .expect("registered base-asset sender leaf");
        let recipient_leaf = output
            .compliance_leaf
            .clone()
            .expect("registered base-asset recipient leaf");
        output
            .set_compliance_details(
                &mut rng,
                &recipient_leaf,
                sender_leaf,
                spend.tx_blinding_nonce,
            )
            .expect("set registered base-asset output compliance details");

        let transfer = TransferPlan::new(vec![spend], vec![output], Fr::rand(&mut rng))
            .expect("build registered base-asset transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
            )
            .expect("derive registered base-asset transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity registered base-asset sender-to-other transfer at high position")
            .verify(&public)
            .expect("verify hidden-arity registered base-asset sender-to-other transfer at high position");
    }

    #[test]
    fn transfer_hidden_arity_1x1_roundtrip_registered_base_asset_sender_to_other_real_user_tree() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
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
            penumbra_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);

        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(
            test_keys::ADDRESS_0
                .diversified_generator()
                .vartime_compress_to_field(),
        );
        let sender_leaf =
            ComplianceLeaf::new(test_keys::ADDRESS_0.clone(), *BASE_ASSET_ID, sender_d);
        let recipient_d = penumbra_sdk_compliance::derive_compliance_scalar(
            test_keys::ADDRESS_1
                .diversified_generator()
                .vartime_compress_to_field(),
        );
        let recipient_leaf =
            ComplianceLeaf::new(test_keys::ADDRESS_1.clone(), *BASE_ASSET_ID, recipient_d);
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

        let mut spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );
        spend.asset_indexed_leaf = asset_indexed_leaf.clone();
        spend.asset_path = asset_path.clone();
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.is_regulated = false;
        spend.compliance_anchor = compliance_anchor;
        spend.compliance_path = sender_compliance_path;
        spend.compliance_position = 0;
        spend
            .set_compliance_details(&mut rng)
            .expect("set registered base-asset spend compliance details");

        let mut output =
            ShieldedOutputPlan::new(&mut rng, input_note.value(), test_keys::ADDRESS_1.clone());
        output.asset_indexed_leaf = asset_indexed_leaf;
        output.asset_path = asset_path;
        output.asset_position = asset_position;
        output.asset_anchor = asset_anchor;
        output.is_regulated = false;
        output.compliance_anchor = compliance_anchor;
        output.compliance_path = recipient_compliance_path;
        output.compliance_position = 1;
        output
            .set_compliance_details(
                &mut rng,
                &recipient_leaf,
                sender_leaf,
                spend.tx_blinding_nonce,
            )
            .expect("set registered base-asset output compliance details");

        let transfer = TransferPlan::new(vec![spend], vec![output], Fr::rand(&mut rng))
            .expect("build registered base-asset transfer plan");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
            )
            .expect("derive registered base-asset transfer public/private inputs");

        TransferProof::prove(public.clone(), private)
            .expect("prove hidden-arity registered base-asset transfer with real user tree")
            .verify(&public)
            .expect("verify hidden-arity registered base-asset transfer with real user tree");
    }

    #[test]
    fn transfer_hidden_arity_1x2_roundtrip_registered_base_asset_with_change_real_user_tree() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

        let mut rng = rand::thread_rng();
        let input_note = Note::from_parts(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(1_000_000u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
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
            penumbra_sdk_compliance::create_default_imt_proof(input_note.asset_id().0);

        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(
            test_keys::ADDRESS_0
                .diversified_generator()
                .vartime_compress_to_field(),
        );
        let sender_leaf =
            ComplianceLeaf::new(test_keys::ADDRESS_0.clone(), *BASE_ASSET_ID, sender_d);
        let recipient_d = penumbra_sdk_compliance::derive_compliance_scalar(
            test_keys::ADDRESS_1
                .diversified_generator()
                .vartime_compress_to_field(),
        );
        let recipient_leaf =
            ComplianceLeaf::new(test_keys::ADDRESS_1.clone(), *BASE_ASSET_ID, recipient_d);
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

        let mut spend = ShieldedInputPlan::new(
            &mut rng,
            input_note.clone(),
            state_commitment_proof.position(),
        );
        spend.asset_indexed_leaf = asset_indexed_leaf.clone();
        spend.asset_path = asset_path.clone();
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.is_regulated = false;
        spend.compliance_anchor = compliance_anchor;
        spend.compliance_path = sender_compliance_path.clone();
        spend.compliance_position = 0;
        spend
            .set_compliance_details(&mut rng)
            .expect("set registered base-asset spend compliance details");

        let mut receiver_output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: Amount::from(1u64),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_1.clone(),
        );
        receiver_output.asset_indexed_leaf = asset_indexed_leaf.clone();
        receiver_output.asset_path = asset_path.clone();
        receiver_output.asset_position = asset_position;
        receiver_output.asset_anchor = asset_anchor;
        receiver_output.is_regulated = false;
        receiver_output.compliance_anchor = compliance_anchor;
        receiver_output.compliance_path = recipient_compliance_path;
        receiver_output.compliance_position = 1;
        receiver_output
            .set_compliance_details(
                &mut rng,
                &recipient_leaf,
                sender_leaf.clone(),
                spend.tx_blinding_nonce,
            )
            .expect("set receiver output compliance details");

        let mut change_output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: Amount::from(999_999u64),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_0.clone(),
        );
        change_output.asset_indexed_leaf = asset_indexed_leaf;
        change_output.asset_path = asset_path;
        change_output.asset_position = asset_position;
        change_output.asset_anchor = asset_anchor;
        change_output.is_regulated = false;
        change_output.compliance_anchor = compliance_anchor;
        change_output.compliance_path = sender_compliance_path;
        change_output.compliance_position = 0;
        change_output
            .set_compliance_details(
                &mut rng,
                &sender_leaf,
                sender_leaf.clone(),
                spend.tx_blinding_nonce,
            )
            .expect("set change output compliance details");

        let transfer = TransferPlan::new(
            vec![spend],
            vec![receiver_output, change_output],
            Fr::rand(&mut rng),
        )
        .expect("build registered base-asset transfer plan with change");
        let (public, private) = transfer
            .transfer_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &[state_commitment_proof],
                anchor,
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

    #[test]
    fn transfer_action_public_matches_proving_public_regulated() {
        let _guard = TRANSFER_PROOF_TEST_MUTEX
            .lock()
            .expect("lock transfer test mutex");
        if super::super::test_runtime::should_skip_transfer_proof_roundtrip_tests() {
            return;
        }

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
            proving_public.compliance.sender_core_dleq.c,
            extracted_public.compliance.sender_core_dleq.c
        );
        assert_eq!(
            proving_public.compliance.sender_core_dleq.s,
            extracted_public.compliance.sender_core_dleq.s
        );
        assert_eq!(
            proving_public.compliance.sender_ext_dleq.c,
            extracted_public.compliance.sender_ext_dleq.c
        );
        assert_eq!(
            proving_public.compliance.sender_ext_dleq.s,
            extracted_public.compliance.sender_ext_dleq.s
        );
        assert_eq!(
            proving_public.compliance.output_core_dleq.c,
            extracted_public.compliance.output_core_dleq.c
        );
        assert_eq!(
            proving_public.compliance.output_core_dleq.s,
            extracted_public.compliance.output_core_dleq.s
        );
        assert_eq!(
            proving_public.compliance.output_ext_dleq.c,
            extracted_public.compliance.output_ext_dleq.c
        );
        assert_eq!(
            proving_public.compliance.output_ext_dleq.s,
            extracted_public.compliance.output_ext_dleq.s
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
        penumbra_sdk_proof_params::batch::batch_verify(
            penumbra_sdk_proof_params::transfer_proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .expect("single-item batch verification should succeed with extracted public");
    }
}
