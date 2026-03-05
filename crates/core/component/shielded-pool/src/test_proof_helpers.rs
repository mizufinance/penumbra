//! Common test helpers for spend and output proof tests.

pub mod proof_test_helpers {
    /// Test asset ID for regulated assets
    pub const REGULATED_ASSET_ID: u64 = 1;
    /// Test asset ID for unregulated assets
    pub const UNREGULATED_ASSET_ID: u64 = 2;

    use decaf377::{Bls12_377, Fq, Fr};
    use penumbra_sdk_asset::{asset, Value};
    use penumbra_sdk_compliance::{IndexedLeaf, IndexedMerkleTree, MerklePath};
    use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
    use penumbra_sdk_tct as tct;

    use crate::{Note, Rseed};

    /// Create valid IMT proof data for an unregulated asset.
    ///
    /// Returns (asset_anchor, indexed_leaf, merkle_path, position) that satisfy circuit constraints.
    /// The asset is proven to be unregulated via non-membership (falls in a gap).
    pub fn create_imt_non_membership_proof(
        asset_id: Fq,
    ) -> (tct::StateCommitment, IndexedLeaf, MerklePath, u64) {
        penumbra_sdk_compliance::create_default_imt_proof(asset_id)
    }

    /// Create valid user tree (QuadTree) proof data.
    ///
    /// Returns (compliance_anchor, merkle_path, position) that satisfy circuit constraints.
    pub fn create_user_tree_proof(
        user_leaf: &penumbra_sdk_compliance::ComplianceLeaf,
    ) -> (tct::StateCommitment, MerklePath, u64) {
        penumbra_sdk_compliance::default_user_proof(user_leaf)
    }

    /// Create valid IMT proof data for a regulated asset.
    ///
    /// `ring_pk` and `dk_pub` must match the keys used for ACK derivation and encryption,
    /// since Policy-in-Leaf binds these into the leaf commitment verified by the circuit.
    pub fn create_imt_membership_proof(
        asset_id: Fq,
        ring_pk: decaf377::Element,
        dk_pub: decaf377::Element,
    ) -> (tct::StateCommitment, IndexedLeaf, MerklePath, u64) {
        let mut tree = IndexedMerkleTree::new();
        let policy = penumbra_sdk_compliance::AssetPolicy::new(
            dk_pub,
            u128::MAX,
            vec![],
            String::new(),
            ring_pk,
            String::new(),
            String::new(),
            String::new(),
        );
        tree.insert(asset_id, &policy)
            .expect("should be able to insert asset");
        let (position, indexed_leaf, auth_path) = tree
            .membership_proof(asset_id)
            .expect("should be able to generate membership proof");
        let merkle_path = MerklePath::from_auth_path(auth_path);
        let anchor = tct::StateCommitment(tree.root().0);
        (anchor, indexed_leaf, merkle_path, position)
    }

    /// Setup Groth16 proving and verifying keys for a circuit.
    ///
    /// Generic helper that performs circuit-specific setup for any proof circuit.
    pub fn setup_groth16_keys<C>() -> (
        ark_groth16::ProvingKey<Bls12_377>,
        ark_groth16::PreparedVerifyingKey<Bls12_377>,
        Fq,
        Fq,
    )
    where
        C: penumbra_sdk_proof_params::DummyWitness + Clone,
    {
        use ark_groth16::Groth16;
        use ark_snark::SNARK;

        let mut rng = rand::thread_rng();

        let circuit_template = C::with_dummy_witness();
        let (pk, vk) = Groth16::<Bls12_377>::circuit_specific_setup(circuit_template, &mut rng)
            .expect("cannot perform setup");
        let pvk = ark_groth16::prepare_verifying_key(&vk);

        let blinding_r = Fq::rand(&mut rng);
        let blinding_s = Fq::rand(&mut rng);

        (pk, pvk, blinding_r, blinding_s)
    }

    /// Mock compliance inputs for Output circuit tests (7-tuple).
    pub fn mock_compliance_inputs_output() -> (
        decaf377::Element,
        decaf377::Element,
        decaf377::Element,
        Fq,
        Fq,
        Fq,
        Vec<Fq>,
    ) {
        use penumbra_sdk_compliance::structs::{ComplianceCiphertext, OUTPUT_WIRE_BYTES};
        let dummy_ciphertext = vec![0u8; OUTPUT_WIRE_BYTES];
        let ct = ComplianceCiphertext::from_bytes(&dummy_ciphertext)
            .expect("can deserialize dummy ciphertext");
        ct.to_output_circuit_public_inputs()
    }

    /// Circuit type for unified testing
    #[derive(Debug, Clone, Copy)]
    pub enum CircuitType {
        Spend,
        Output,
    }

    /// Test data bundle containing note, keys, and compliance information.
    ///
    /// Used by both groth16 and plan tests to ensure consistency.
    pub struct TestData {
        pub note: Note,
        pub address: penumbra_sdk_keys::Address,
        pub value: penumbra_sdk_asset::Value,
        pub balance_blinding: Fr,
        pub fvk: penumbra_sdk_keys::keys::FullViewingKey,
        pub sk: SpendKey,
        pub user_leaf: penumbra_sdk_compliance::ComplianceLeaf,
        /// Sender address (distinct from receiver for Output, same as receiver for Spend)
        pub sender_address: penumbra_sdk_keys::Address,
        /// Sender's compliance leaf (Output only — contains sender's `d` scalar)
        pub counterparty_leaf: penumbra_sdk_compliance::ComplianceLeaf,
        pub ring_pk: decaf377::Element,
        pub dk_pub: decaf377::Element,
        pub epk_1: decaf377::Element,
        pub epk_2: Option<decaf377::Element>,
        pub epk_3: Option<decaf377::Element>,
        pub c2_core: Fq,
        pub c2_ext: Option<Fq>,
        pub c2_sext: Option<Fq>,
        pub compliance_ciphertext: Vec<Fq>,
        pub compliance_ciphertext_bytes: Vec<u8>,
        pub ephemeral_secret: Fr,
        pub r_2: Option<Fr>,
        pub r_3: Option<Fr>,
        pub asset_anchor: tct::StateCommitment,
        pub asset_indexed_leaf: IndexedLeaf,
        pub asset_path: MerklePath,
        pub asset_position: u64,
        pub compliance_anchor: tct::StateCommitment,
        pub compliance_path: MerklePath,
        pub compliance_position: u64,
        pub salt: Fq,
        pub dleq_c: Fq,
        pub dleq_s: Fq,
        pub dleq_c_2: Fq,
        pub dleq_s_2: Fq,
        pub dleq_c_3: Fq,
        pub dleq_s_3: Fq,
        pub target_timestamp: u64,
    }

    /// Generate unified test data with compliance encryption.
    ///
    /// Creates a note, keys, and compliance data (regulated or unregulated).
    pub fn generate_test_data(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        asset_id: u64,
        amount: u64,
        is_regulated: bool,
        circuit_type: CircuitType,
    ) -> TestData {
        use penumbra_sdk_compliance::crypto::{encrypt_output, encrypt_spend};
        use penumbra_sdk_num::Amount;

        // Receiver identity
        let seed_phrase = SeedPhrase::generate(&mut *rng);
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = sk.full_viewing_key();
        let ivk = fvk.incoming();
        let (address, _dtk_d) = ivk.payment_address(0u32.into());

        // Distinct sender identity (for Output cross-party testing)
        let sender_seed = SeedPhrase::generate(&mut *rng);
        let sender_sk = SpendKey::from_seed_phrase_bip44(sender_seed, &Bip44Path::new(0));
        let sender_ivk = sender_sk.full_viewing_key().incoming();
        let (sender_address, _) = sender_ivk.payment_address(0u32.into());

        let value = Value {
            amount: Amount::from(amount),
            asset_id: asset::Id(Fq::from(asset_id)),
        };

        let note = Note::from_parts(address.clone(), value, Rseed::generate(&mut *rng))
            .expect("can create note");

        let balance_blinding = Fr::rand(&mut *rng);

        // Determine keys before IMT proof (Policy-in-Leaf binds ring_pk into the leaf)
        let (ring_pk, dk_pub) = if is_regulated {
            let ring_sk = Fr::rand(&mut *rng);
            let ring_pk = decaf377::Element::GENERATOR * ring_sk;
            (ring_pk, decaf377::Element::GENERATOR)
        } else {
            (
                *penumbra_sdk_compliance::BLACK_HOLE_ACK,
                decaf377::Element::GENERATOR,
            )
        };

        let asset_id_fq = value.asset_id.0;
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) = if is_regulated {
            create_imt_membership_proof(asset_id_fq, ring_pk, dk_pub)
        } else {
            create_imt_non_membership_proof(asset_id_fq)
        };

        // Receiver ACK
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack_receiver = ring_pk * d_fr;

        // Sender ACK (distinct for Output, same as receiver for Spend)
        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_d_fr = Fr::from_le_bytes_mod_order(&sender_d.to_bytes());
        let ack_sender = ring_pk * sender_d_fr;

        let user_leaf =
            penumbra_sdk_compliance::ComplianceLeaf::new(address.clone(), value.asset_id, d);

        let counterparty_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            sender_address.clone(),
            value.asset_id,
            sender_d,
        );

        let salt = Fq::rand(&mut *rng);
        let target_timestamp: u64 = 1_700_000_000; // fixed Unix timestamp for tests

        let (compliance_ciphertext_bytes, ephemeral_secret, r_2, r_3) = match circuit_type {
            CircuitType::Output => {
                let result = encrypt_output(
                    &mut *rng,
                    &ack_receiver,
                    &ack_sender,
                    &dk_pub,
                    &address,
                    &sender_address,
                    note.asset_id(),
                    note.amount(),
                    false,
                    salt,
                )
                .expect("can encrypt output");
                (
                    result.ciphertext.to_bytes(),
                    result.r_1,
                    Some(result.r_2),
                    Some(result.r_3),
                )
            }
            CircuitType::Spend => {
                let result = encrypt_spend(
                    &mut *rng,
                    &ack_receiver,
                    &dk_pub,
                    &address,
                    note.asset_id(),
                    note.amount(),
                    false,
                    salt,
                )
                .expect("can encrypt spend");
                (result.ciphertext.to_bytes(), result.r_s, None, None)
            }
        };

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct = ComplianceCiphertext::from_bytes(&compliance_ciphertext_bytes)
            .expect("can deserialize ciphertext");

        let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
            match circuit_type {
                CircuitType::Output => {
                    let (e1, e2, e3, c2c, c2e, c2s, ct_fqs) = ct.to_output_circuit_public_inputs();
                    (e1, Some(e2), Some(e3), c2c, Some(c2e), Some(c2s), ct_fqs)
                }
                CircuitType::Spend => {
                    let (e1, c2c, ct_fqs) = ct.to_spend_circuit_public_inputs();
                    (e1, None, None, c2c, None, None, ct_fqs)
                }
            };

        let (compliance_anchor, compliance_path, compliance_position) =
            create_user_tree_proof(&user_leaf);

        // Compute DLEQ proofs using actual policy hashes from the indexed leaf
        let policy_id_hash = asset_indexed_leaf.ring.policy_id_hash;
        let resource_hash = asset_indexed_leaf.ring.resource_hash;
        let permission_hash = asset_indexed_leaf.ring.permission_hash;

        // Tier 1 (core) — used by both Spend and Output, uses receiver ACK
        let dleq_k_1 = Fr::rand(&mut *rng);
        let m_core = penumbra_sdk_compliance::compute_metadata_hash(
            policy_id_hash,
            resource_hash,
            permission_hash,
            Fq::from(1u64),
            Fq::from(target_timestamp),
            salt,
        );
        let epk_for_dleq = decaf377::Element::GENERATOR * ephemeral_secret;
        let dleq_1 = penumbra_sdk_compliance::compute_dleq_native(
            ephemeral_secret,
            dleq_k_1,
            &ack_receiver,
            &epk_for_dleq,
            m_core,
        );
        let dleq_c = dleq_1.c;
        let dleq_s = Fq::from_le_bytes_mod_order(&dleq_1.s.to_bytes());

        // Tiers 2 and 3 (ext, sext) — Output only
        // ext uses ack_receiver, sext uses ack_sender
        let (dleq_c_2, dleq_s_2, dleq_c_3, dleq_s_3) = if let (Some(r2), Some(r3)) = (r_2, r_3) {
            let dleq_k_2 = Fr::rand(&mut *rng);
            let m_ext = penumbra_sdk_compliance::compute_metadata_hash(
                policy_id_hash,
                resource_hash,
                permission_hash,
                Fq::from(2u64),
                Fq::from(target_timestamp),
                salt,
            );
            let epk_2_point = decaf377::Element::GENERATOR * r2;
            let dleq_2 = penumbra_sdk_compliance::compute_dleq_native(
                r2,
                dleq_k_2,
                &ack_receiver,
                &epk_2_point,
                m_ext,
            );

            let dleq_k_3 = Fr::rand(&mut *rng);
            let m_sext = penumbra_sdk_compliance::compute_metadata_hash(
                policy_id_hash,
                resource_hash,
                permission_hash,
                Fq::from(3u64),
                Fq::from(target_timestamp),
                salt,
            );
            let epk_3_point = decaf377::Element::GENERATOR * r3;
            let dleq_3 = penumbra_sdk_compliance::compute_dleq_native(
                r3,
                dleq_k_3,
                &ack_sender,
                &epk_3_point,
                m_sext,
            );

            (
                dleq_2.c,
                Fq::from_le_bytes_mod_order(&dleq_2.s.to_bytes()),
                dleq_3.c,
                Fq::from_le_bytes_mod_order(&dleq_3.s.to_bytes()),
            )
        } else {
            (
                Fq::from(0u64),
                Fq::from(0u64),
                Fq::from(0u64),
                Fq::from(0u64),
            )
        };

        TestData {
            note,
            address,
            value,
            balance_blinding,
            fvk: fvk.clone(),
            sk,
            user_leaf,
            sender_address,
            counterparty_leaf,
            ring_pk,
            dk_pub,
            epk_1,
            epk_2,
            epk_3,
            c2_core,
            c2_ext,
            c2_sext,
            compliance_ciphertext,
            compliance_ciphertext_bytes,
            ephemeral_secret,
            r_2,
            r_3,
            asset_anchor,
            asset_indexed_leaf,
            asset_path,
            asset_position,
            compliance_anchor,
            compliance_path,
            compliance_position,
            salt,
            dleq_c,
            dleq_s,
            dleq_c_2,
            dleq_s_2,
            dleq_c_3,
            dleq_s_3,
            target_timestamp,
        }
    }

    /// Run Output circuit Groth16 roundtrip test.
    fn test_output_groth16(test_data: TestData, is_regulated: bool) {
        use crate::output::{OutputCircuit, OutputProof, OutputProofPrivate, OutputProofPublic};
        use decaf377::Fr;
        use penumbra_sdk_asset::Balance;

        let (pk, pvk, blinding_r, blinding_s) = setup_groth16_keys::<OutputCircuit>();

        let note_commitment = test_data.note.commit();
        let balance_commitment =
            (-Balance::from(test_data.value)).commit(test_data.balance_blinding);

        let tx_blinding_nonce = Fr::from(0u64);
        let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(
            test_data.counterparty_leaf.commit(),
            tx_blinding_nonce,
        );

        let public = OutputProofPublic {
            balance_commitment,
            note_commitment,
            epk_1: test_data.epk_1,
            epk_2: test_data.epk_2.expect("output test requires epk_2"),
            epk_3: test_data.epk_3.expect("output test requires epk_3"),
            c2_core: test_data.c2_core,
            c2_ext: test_data.c2_ext.expect("output test requires c2_ext"),
            c2_sext: test_data.c2_sext.expect("output test requires c2_sext"),
            compliance_ciphertext: test_data.compliance_ciphertext,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c_1: test_data.dleq_c,
            dleq_s_1: test_data.dleq_s,
            dleq_c_2: test_data.dleq_c_2,
            dleq_s_2: test_data.dleq_s_2,
            dleq_c_3: test_data.dleq_c_3,
            dleq_s_3: test_data.dleq_s_3,
            counterparty_leaf_hash,
        };

        let private = OutputProofPrivate {
            note: test_data.note,
            balance_blinding: test_data.balance_blinding,
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            r_2: test_data.r_2.expect("output requires r_2"),
            r_3: test_data.r_3.expect("output requires r_3"),
            counterparty_leaf: test_data.counterparty_leaf,
            tx_blinding_nonce,
            is_flagged: false,
            salt: test_data.salt,
        };

        let proof = OutputProof::prove(blinding_r, blinding_s, &pk, public.clone(), private)
            .expect("can generate proof");
        let item = proof
            .to_batch_item(public.clone())
            .expect("can build output batch item");
        assert_eq!(item.public_inputs.len(), 1);
        proof.verify(&pvk, public).expect("proof should verify");
    }

    /// Run Spend circuit Groth16 roundtrip test.
    fn test_spend_groth16(test_data: TestData, is_regulated: bool) {
        use crate::spend::{SpendCircuit, SpendProof, SpendProofPrivate, SpendProofPublic};
        use penumbra_sdk_asset::Balance;
        use penumbra_sdk_sct::Nullifier;

        let mut rng = rand::thread_rng();

        let (pk, pvk, blinding_r, blinding_s) = setup_groth16_keys::<SpendCircuit>();

        let mut sct = tct::Tree::new();
        let note_commitment = test_data.note.commit();
        sct.insert(tct::Witness::Keep, note_commitment).unwrap();
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
            is_regulated,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            tx_blinding_nonce: dummy_nonce,
            is_flagged: false,
            salt: test_data.salt,
        };

        let proof = SpendProof::prove(blinding_r, blinding_s, &pk, public.clone(), private)
            .expect("can generate proof");
        let item = proof
            .to_batch_item(public.clone())
            .expect("can build spend batch item");
        assert_eq!(item.public_inputs.len(), 1);
        proof.verify(&pvk, public).expect("proof should verify");
    }

    /// Unified Groth16 roundtrip test function.
    ///
    /// Consolidates all Groth16 test logic for both Spend and Output circuits,
    /// and both regulated and unregulated assets.
    pub fn full_groth16_roundtrip(circuit_type: CircuitType, is_regulated: bool) {
        let mut rng = rand::thread_rng();
        let test_data = generate_test_data(&mut rng, 1, 100, is_regulated, circuit_type);
        match circuit_type {
            CircuitType::Output => test_output_groth16(test_data, is_regulated),
            CircuitType::Spend => test_spend_groth16(test_data, is_regulated),
        }
    }

    /// Test the SpendPlan code path: SpendPlan::new() → set fields → set_compliance_details → spend_proof.
    ///
    /// Uses the same test data as the passing `test_spend_groth16` but routes through
    /// the SpendPlan pipeline to isolate whether the enrichment flow introduces bugs.
    pub fn test_spend_plan_path(is_regulated: bool) {
        use crate::spend::{SpendCircuit, SpendProofPublic};
        use crate::SpendPlan;
        use penumbra_sdk_compliance::structs::ComplianceCiphertext;

        let mut rng = rand::thread_rng();
        let test_data = generate_test_data(&mut rng, 1, 100, is_regulated, CircuitType::Spend);

        // Create SCT and get proof (same as test_spend_groth16)
        let mut sct = tct::Tree::new();
        let note_commitment = test_data.note.commit();
        sct.insert(tct::Witness::Keep, note_commitment).unwrap();
        let state_commitment_proof = sct.witness(note_commitment).unwrap();

        // Create SpendPlan through the normal constructor (generates BLACK_HOLE_ACK defaults)
        let mut plan = SpendPlan::new(
            &mut rng,
            test_data.note.clone(),
            state_commitment_proof.position(),
        );

        // Manually set fields as enrich_plan_with_compliance would
        plan.asset_indexed_leaf = test_data.asset_indexed_leaf.clone();
        plan.asset_path = test_data.asset_path.clone();
        plan.asset_position = test_data.asset_position;
        plan.asset_anchor = test_data.asset_anchor;
        plan.compliance_anchor = test_data.compliance_anchor;
        plan.compliance_path = test_data.compliance_path.clone();
        plan.compliance_position = test_data.compliance_position;
        plan.is_regulated = is_regulated;
        plan.target_timestamp = test_data.target_timestamp;

        // Call set_compliance_details — this regenerates ciphertext + DLEQ
        plan.set_compliance_details(&mut rng)
            .expect("set_compliance_details should succeed");

        // Setup fresh Groth16 keys
        let (pk, pvk, _, _) = setup_groth16_keys::<SpendCircuit>();

        // Generate proof via SpendPlan's method
        let proof = plan
            .spend_proof(
                &test_data.fvk,
                state_commitment_proof,
                sct.root(),
                &pk,
                None,
            )
            .expect("spend proof should succeed via plan path");

        // Build public inputs for verification
        let ct = ComplianceCiphertext::from_bytes(&plan.compliance_ciphertext)
            .expect("can parse ciphertext");
        let (epk, c2_core, ct_fqs) = ct.to_spend_circuit_public_inputs();
        let user_leaf = plan.compliance_leaf.clone().unwrap();
        let sender_leaf_hash =
            penumbra_sdk_compliance::blind_sender_leaf(user_leaf.commit(), plan.tx_blinding_nonce);

        let public = SpendProofPublic {
            anchor: sct.root(),
            balance_commitment: plan.balance().commit(plan.value_blinding),
            nullifier: plan.nullifier(&test_data.fvk),
            rk: plan.rk(&test_data.fvk),
            asset_anchor: plan.asset_anchor,
            compliance_anchor: plan.compliance_anchor,
            epk,
            c2_core,
            compliance_ciphertext: ct_fqs,
            target_timestamp: Fq::from(plan.target_timestamp),
            dleq_c: plan.dleq_c,
            dleq_s: plan.dleq_s,
            sender_leaf_hash,
        };

        proof
            .verify(&pvk, public)
            .expect("proof should verify via plan path");
    }

    /// Test the OutputPlan code path with cross-party addresses (distinct sender/receiver ACKs).
    ///
    /// Mirrors `test_spend_plan_path` but for OutputPlan, exercising the dual-ACK DLEQ path.
    pub fn test_output_plan_path(is_regulated: bool) {
        use crate::output::{OutputCircuit, OutputProofPublic};
        use crate::OutputPlan;
        use penumbra_sdk_compliance::structs::ComplianceCiphertext;

        let mut rng = rand::thread_rng();
        let test_data = generate_test_data(&mut rng, 1, 100, is_regulated, CircuitType::Output);

        // Create OutputPlan through the normal constructor
        let mut plan = OutputPlan::new(&mut rng, test_data.value, test_data.address.clone());

        // Set compliance fields as enrich_plan_with_compliance would
        plan.asset_indexed_leaf = test_data.asset_indexed_leaf.clone();
        plan.asset_path = test_data.asset_path.clone();
        plan.asset_position = test_data.asset_position;
        plan.asset_anchor = test_data.asset_anchor;
        plan.compliance_anchor = test_data.compliance_anchor;
        plan.compliance_path = test_data.compliance_path.clone();
        plan.compliance_position = test_data.compliance_position;
        plan.is_regulated = is_regulated;
        plan.target_timestamp = test_data.target_timestamp;

        // Build recipient and sender leaves with proper d scalars
        let recipient_leaf = test_data.user_leaf.clone();
        let sender_leaf = test_data.counterparty_leaf.clone();
        let tx_blinding_nonce = Fr::from(0u64);

        // Call set_compliance_details — regenerates ciphertext + dual-ACK DLEQ
        plan.set_compliance_details(
            &mut rng,
            &recipient_leaf,
            sender_leaf.clone(),
            tx_blinding_nonce,
        )
        .expect("set_compliance_details should succeed");

        // Setup Groth16 keys
        let (pk, pvk, _, _) = setup_groth16_keys::<OutputCircuit>();

        // Generate proof via OutputPlan's method
        let proof = plan
            .output_proof(&pk, None)
            .expect("output proof should succeed via plan path");

        // Build public inputs for verification
        let ct = ComplianceCiphertext::from_bytes(&plan.compliance_ciphertext)
            .expect("can parse ciphertext");
        let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, ct_fqs) =
            ct.to_output_circuit_public_inputs();
        let counterparty_leaf_hash =
            penumbra_sdk_compliance::blind_sender_leaf(sender_leaf.commit(), tx_blinding_nonce);

        let public = OutputProofPublic {
            note_commitment: plan.output_note().commit(),
            balance_commitment: plan.balance().commit(plan.value_blinding),
            epk_1,
            epk_2,
            epk_3,
            c2_core,
            c2_ext,
            c2_sext,
            compliance_ciphertext: ct_fqs,
            target_timestamp: Fq::from(plan.target_timestamp),
            dleq_c_1: plan.dleq_c_1,
            dleq_s_1: plan.dleq_s_1,
            dleq_c_2: plan.dleq_c_2,
            dleq_s_2: plan.dleq_s_2,
            dleq_c_3: plan.dleq_c_3,
            dleq_s_3: plan.dleq_s_3,
            asset_anchor: plan.asset_anchor,
            compliance_anchor: plan.compliance_anchor,
            counterparty_leaf_hash,
        };

        proof
            .verify(&pvk, public)
            .expect("proof should verify via output plan path");
    }
}
