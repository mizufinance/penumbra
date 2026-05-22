//! Common test helpers for shielded-pool proof tests.

#[allow(dead_code)]
pub mod proof_test_helpers {
    use std::time::Instant;

    /// Test asset ID for regulated assets
    pub const REGULATED_ASSET_ID: u64 = 1;
    /// Test asset ID for unregulated assets
    pub const UNREGULATED_ASSET_ID: u64 = 2;

    use decaf377::{Fq, Fr};
    use penumbra_sdk_asset::{asset, Balance, Value};
    use penumbra_sdk_compliance::{IndexedLeaf, IndexedMerkleTree, MerklePath};
    use penumbra_sdk_keys::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        PayloadKey,
    };
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_tct as tct;
    use penumbra_sdk_txhash::TransactionContext;

    use crate::{
        Note, Rseed, ShieldedIcs20WithdrawalChangePrivate, ShieldedIcs20WithdrawalChangePublic,
        ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalInputPrivate,
        ShieldedIcs20WithdrawalInputPublic, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic, ShieldedInputPlan,
    };

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
            penumbra_sdk_compliance::DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![],
            None,
            "test-ring-id".to_string(),
            ring_pk,
            "test-policy-id".to_string(),
            "read".to_string(),
            "document".to_string(),
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

    /// Circuit type for unified testing
    #[derive(Debug, Clone, Copy)]
    pub enum CircuitType {
        Transfer,
    }

    /// Shared fixture layer used by all proof-family test builders.
    pub struct BaseTestData {
        pub note: Note,
        pub address: penumbra_sdk_keys::Address,
        pub value: penumbra_sdk_asset::Value,
        pub balance_blinding: Fr,
        pub fvk: penumbra_sdk_keys::keys::FullViewingKey,
        pub sk: SpendKey,
        pub user_leaf: penumbra_sdk_compliance::ComplianceLeaf,
        pub sender_address: penumbra_sdk_keys::Address,
        pub counterparty_leaf: penumbra_sdk_compliance::ComplianceLeaf,
        pub ring_pk: decaf377::Element,
        pub dk_pub: decaf377::Element,
        pub ack_receiver: decaf377::Element,
        pub ack_sender: decaf377::Element,
        pub asset_anchor: tct::StateCommitment,
        pub asset_indexed_leaf: IndexedLeaf,
        pub asset_path: MerklePath,
        pub asset_position: u64,
        pub compliance_anchor: tct::StateCommitment,
        pub compliance_path: MerklePath,
        pub compliance_position: u64,
        pub salt: Fq,
        pub target_timestamp: u64,
        pub asset_policy: penumbra_sdk_compliance::AssetPolicy,
    }

    /// Generate the shared fixture data used by all proof-family tests.
    pub fn generate_base_test_data(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        asset_id: u64,
        amount: u64,
        is_regulated: bool,
    ) -> BaseTestData {
        generate_base_test_data_for_asset(rng, asset::Id(Fq::from(asset_id)), amount, is_regulated)
    }

    pub fn generate_base_test_data_for_asset(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        asset_id: asset::Id,
        amount: u64,
        is_regulated: bool,
    ) -> BaseTestData {
        use penumbra_sdk_num::Amount;

        // Receiver identity
        let seed_phrase = SeedPhrase::generate(&mut *rng);
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = sk.full_viewing_key();
        let ivk = fvk.incoming();
        let (address, _dtk_d) = ivk.payment_address(0u32.into());

        // Distinct sender identity for transfer-side compliance fixtures.
        let sender_seed = SeedPhrase::generate(&mut *rng);
        let sender_sk = SpendKey::from_seed_phrase_bip44(sender_seed, &Bip44Path::new(0));
        let sender_ivk = sender_sk.full_viewing_key().incoming();
        let (sender_address, _) = sender_ivk.payment_address(0u32.into());

        let value = Value {
            amount: Amount::from(amount),
            asset_id,
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
                *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK,
                *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB,
            )
        };

        let asset_id_fq = value.asset_id.0;
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) = if is_regulated {
            create_imt_membership_proof(asset_id_fq, ring_pk, dk_pub)
        } else {
            create_imt_non_membership_proof(asset_id_fq)
        };
        let asset_policy = if is_regulated {
            penumbra_sdk_compliance::AssetPolicy::new(
                dk_pub,
                asset_indexed_leaf.params.threshold,
                penumbra_sdk_compliance::DEFAULT_COMPLIANCE_SLOT_COUNT,
                vec![],
                None,
                "test-ring-id".to_string(),
                ring_pk,
                "test-policy-id".to_string(),
                "read".to_string(),
                "document".to_string(),
            )
        } else {
            penumbra_sdk_compliance::AssetPolicy::default_unregulated()
        };

        // Receiver ACK
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack_receiver = ring_pk * d_fr;

        // Sender ACK used by transfer-side compliance fixtures.
        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_d = penumbra_sdk_compliance::derive_compliance_scalar(sender_b_d_fq);
        let sender_d_fr = Fr::from_le_bytes_mod_order(&sender_d.to_bytes());
        let ack_sender = ring_pk * sender_d_fr;

        let user_leaf =
            penumbra_sdk_compliance::ComplianceLeaf::new(address.clone(), value.asset_id, b_d_fq);

        let counterparty_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            sender_address.clone(),
            value.asset_id,
            sender_b_d_fq,
        );

        let (compliance_anchor, compliance_path, compliance_position) =
            create_user_tree_proof(&user_leaf);

        BaseTestData {
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
            ack_receiver,
            ack_sender,
            asset_anchor,
            asset_indexed_leaf,
            asset_path,
            asset_position,
            compliance_anchor,
            compliance_path,
            compliance_position,
            salt: Fq::rand(&mut *rng),
            target_timestamp: 1_700_000_000,
            asset_policy,
        }
    }

    pub(crate) fn build_transfer_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        is_regulated: bool,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        use crate::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
        use penumbra_sdk_asset::{asset, Value};
        use penumbra_sdk_num::Amount;

        let base = generate_base_test_data(rng, 1, 100, is_regulated);

        let input_amounts = split_transfer_amounts(
            crate::transfer_input_count(),
            100u64
                .checked_mul(crate::transfer_input_count() as u64)
                .expect("transfer test input total fits in u64"),
        );
        let input_total = 100u64
            .checked_mul(crate::transfer_input_count() as u64)
            .expect("transfer test input total fits in u64");
        let output_fee_total = 10u64
            .checked_mul(crate::transfer_output_count() as u64)
            .expect("transfer test fee total fits in u64");
        let output_amounts = split_transfer_amounts(
            crate::transfer_output_count(),
            input_total
                .checked_sub(output_fee_total)
                .unwrap_or_else(|| {
                    panic!(
                        "invalid transfer test family sizes: input_count={} output_count={}",
                        crate::transfer_input_count(),
                        crate::transfer_output_count()
                    )
                }),
        );

        let asset_id = asset::Id(Fq::from(1u64));
        let mut notes = Vec::with_capacity(input_amounts.len());
        for amount in &input_amounts {
            notes.push(
                crate::Note::from_parts(
                    base.address.clone(),
                    Value {
                        amount: Amount::from(*amount),
                        asset_id,
                    },
                    crate::Rseed::generate(rng),
                )
                .expect("create transfer test note"),
            );
        }

        let mut sct = tct::Tree::new();
        for note in &notes {
            sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        }
        let anchor = sct.root();
        let state_commitment_proofs = notes
            .iter()
            .map(|note| {
                sct.witness(note.commit())
                    .expect("state commitment witness")
            })
            .collect::<Vec<_>>();

        let tx_blinding_nonce = Fr::rand(rng);
        let mut spends = Vec::with_capacity(notes.len());
        for (note, proof) in notes.iter().cloned().zip(state_commitment_proofs.iter()) {
            let mut spend = ShieldedInputPlan::new(rng, note, proof.position());
            spend.asset_indexed_leaf = base.asset_indexed_leaf.clone();
            spend.asset_path = base.asset_path.clone();
            spend.asset_position = base.asset_position;
            spend.asset_anchor = base.asset_anchor;
            spend.compliance_anchor = base.compliance_anchor;
            spend.compliance_path = base.compliance_path.clone();
            spend.compliance_position = base.compliance_position;
            spend.is_regulated = is_regulated;
            spend.target_timestamp = base.target_timestamp;
            spend.tx_blinding_nonce = tx_blinding_nonce;
            spend.asset_policy = Some(base.asset_policy.clone());
            spend
                .set_compliance_details(rng)
                .expect("set transfer spend compliance details");
            spends.push(spend);
        }

        let sender_leaf = spends[0]
            .compliance_leaf
            .clone()
            .expect("first transfer spend must have a sender compliance leaf");
        let mut outputs = Vec::with_capacity(output_amounts.len());
        for amount in &output_amounts {
            let mut output = ShieldedOutputPlan::new(
                rng,
                Value {
                    amount: Amount::from(*amount),
                    asset_id,
                },
                base.address.clone(),
            );
            output.asset_indexed_leaf = base.asset_indexed_leaf.clone();
            output.asset_path = base.asset_path.clone();
            output.asset_position = base.asset_position;
            output.asset_anchor = base.asset_anchor;
            output.compliance_anchor = base.compliance_anchor;
            output.compliance_path = base.compliance_path.clone();
            output.compliance_position = base.compliance_position;
            output.is_regulated = is_regulated;
            output.target_timestamp = base.target_timestamp;
            output.tx_blinding_nonce = tx_blinding_nonce;
            output.asset_policy = Some(base.asset_policy.clone());
            output
                .set_compliance_details(rng, &sender_leaf, sender_leaf.clone(), tx_blinding_nonce)
                .expect("set transfer output compliance details");
            outputs.push(output);
        }

        let value_blinding = Fr::rand(rng);
        let transfer = TransferPlan::new(
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            value_blinding,
        )
        .expect("build transfer plan");

        transfer
            .transfer_public_private(&base.fvk, &state_commitment_proofs, anchor)
            .expect("derive transfer public/private inputs")
    }

    pub(crate) fn build_transfer_roundtrip_inputs(
        is_regulated: bool,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        let mut rng = rand::thread_rng();
        build_transfer_roundtrip_inputs_with_rng(&mut rng, is_regulated)
    }

    pub(crate) fn build_transfer_hidden_arity_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        is_regulated: bool,
        send_to_self: bool,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        use penumbra_sdk_asset::asset;

        build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng(
            rng,
            asset::Id(Fq::from(1u64)),
            is_regulated,
            send_to_self,
        )
    }

    pub(crate) fn build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        asset_id: penumbra_sdk_asset::asset::Id,
        is_regulated: bool,
        send_to_self: bool,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        use crate::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
        use penumbra_sdk_asset::Value;
        use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
        use penumbra_sdk_num::Amount;

        let base = generate_base_test_data_for_asset(rng, asset_id, 100, is_regulated);

        let recipient_address = if send_to_self {
            base.address.clone()
        } else {
            let recipient_seed = SeedPhrase::generate(&mut *rng);
            let recipient_sk = SpendKey::from_seed_phrase_bip44(recipient_seed, &Bip44Path::new(1));
            recipient_sk
                .full_viewing_key()
                .incoming()
                .payment_address(0u32.into())
                .0
        };

        let note = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(100u64),
                asset_id,
            },
            crate::Rseed::generate(&mut *rng),
        )
        .expect("create hidden-arity transfer test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, note.commit())
            .expect("insert hidden-arity transfer input note");
        let state_commitment_proof = sct
            .witness(note.commit())
            .expect("witness hidden-arity transfer input note");
        let anchor = sct.root();

        let tx_blinding_nonce = Fr::rand(rng);
        let mut spend =
            ShieldedInputPlan::new(rng, note.clone(), state_commitment_proof.position());
        spend.asset_indexed_leaf = base.asset_indexed_leaf.clone();
        spend.asset_path = base.asset_path.clone();
        spend.asset_position = base.asset_position;
        spend.asset_anchor = base.asset_anchor;
        spend.is_regulated = is_regulated;
        spend.target_timestamp = base.target_timestamp;
        spend.tx_blinding_nonce = tx_blinding_nonce;
        spend.asset_policy = Some(base.asset_policy.clone());
        spend
            .set_compliance_details(rng)
            .expect("set hidden-arity transfer spend compliance details");

        let sender_leaf = spend
            .compliance_leaf
            .clone()
            .expect("hidden-arity transfer spend must have sender leaf");

        let recipient_b_d_fq = recipient_address
            .diversified_generator()
            .vartime_compress_to_field();
        let recipient_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
            recipient_address.clone(),
            asset_id,
            recipient_b_d_fq,
        );
        let (
            shared_compliance_anchor,
            sender_compliance_path,
            sender_compliance_position,
            recipient_compliance_path,
            recipient_compliance_position,
        ) = if send_to_self {
            (
                base.compliance_anchor,
                base.compliance_path.clone(),
                base.compliance_position,
                base.compliance_path.clone(),
                base.compliance_position,
            )
        } else {
            let mut user_tree = penumbra_sdk_compliance::QuadTree::new();
            user_tree
                .update(base.compliance_position, base.user_leaf.commit())
                .expect("insert hidden-arity sender compliance leaf");
            let recipient_position = base
                .compliance_position
                .checked_add(1)
                .expect("recipient compliance position fits");
            user_tree
                .update(recipient_position, recipient_leaf.commit())
                .expect("insert hidden-arity recipient compliance leaf");
            let sender_auth_path = user_tree
                .auth_path(base.compliance_position)
                .expect("sender hidden-arity auth path");
            let recipient_auth_path = user_tree
                .auth_path(recipient_position)
                .expect("recipient hidden-arity auth path");
            (
                tct::StateCommitment(user_tree.root().0),
                MerklePath::from_auth_path(sender_auth_path),
                base.compliance_position,
                MerklePath::from_auth_path(recipient_auth_path),
                recipient_position,
            )
        };
        spend.compliance_anchor = shared_compliance_anchor;
        spend.compliance_path = sender_compliance_path;
        spend.compliance_position = sender_compliance_position;

        let mut output = ShieldedOutputPlan::new(
            rng,
            Value {
                amount: Amount::from(100u64),
                asset_id,
            },
            recipient_address,
        );
        output.asset_indexed_leaf = base.asset_indexed_leaf.clone();
        output.asset_path = base.asset_path.clone();
        output.asset_position = base.asset_position;
        output.asset_anchor = base.asset_anchor;
        output.compliance_anchor = shared_compliance_anchor;
        output.compliance_path = recipient_compliance_path;
        output.compliance_position = recipient_compliance_position;
        output.is_regulated = is_regulated;
        output.target_timestamp = base.target_timestamp;
        output.tx_blinding_nonce = tx_blinding_nonce;
        output.asset_policy = Some(base.asset_policy.clone());
        output
            .set_compliance_details(rng, &recipient_leaf, sender_leaf, tx_blinding_nonce)
            .expect("set hidden-arity transfer output compliance details");

        let transfer = TransferPlan::new(vec![spend], vec![output], Fr::rand(rng))
            .expect("build hidden-arity transfer plan");

        transfer
            .transfer_public_private(&base.fvk, &[state_commitment_proof], anchor)
            .expect("derive hidden-arity transfer public/private inputs")
    }

    pub(crate) fn build_transfer_action_and_public(
        is_regulated: bool,
    ) -> (
        crate::Transfer,
        crate::TransferProofPublic,
        TransactionContext,
    ) {
        use crate::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
        use penumbra_sdk_asset::{asset, Value};

        let mut rng = rand::thread_rng();
        let base = generate_base_test_data(&mut rng, 1, 100, is_regulated);

        let input_amounts = split_transfer_amounts(
            crate::transfer_input_count(),
            100u64
                .checked_mul(crate::transfer_input_count() as u64)
                .expect("transfer test input total fits in u64"),
        );
        let input_total = 100u64
            .checked_mul(crate::transfer_input_count() as u64)
            .expect("transfer test input total fits in u64");
        let output_fee_total = 10u64
            .checked_mul(crate::transfer_output_count() as u64)
            .expect("transfer test fee total fits in u64");
        let output_amounts = split_transfer_amounts(
            crate::transfer_output_count(),
            input_total
                .checked_sub(output_fee_total)
                .expect("transfer test output total fits in u64"),
        );

        let asset_id = asset::Id(Fq::from(1u64));
        let mut notes = Vec::with_capacity(input_amounts.len());
        for amount in &input_amounts {
            notes.push(
                crate::Note::from_parts(
                    base.address.clone(),
                    Value {
                        amount: Amount::from(*amount),
                        asset_id,
                    },
                    crate::Rseed::generate(&mut rng),
                )
                .expect("create transfer test note"),
            );
        }

        let mut sct = tct::Tree::new();
        for note in &notes {
            sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        }
        let anchor = sct.root();
        let state_commitment_proofs = notes
            .iter()
            .map(|note| {
                sct.witness(note.commit())
                    .expect("state commitment witness")
            })
            .collect::<Vec<_>>();

        let tx_blinding_nonce = Fr::rand(&mut rng);
        let mut spends = Vec::with_capacity(notes.len());
        for (note, proof) in notes.iter().cloned().zip(state_commitment_proofs.iter()) {
            let mut spend = ShieldedInputPlan::new(&mut rng, note, proof.position());
            spend.asset_indexed_leaf = base.asset_indexed_leaf.clone();
            spend.asset_path = base.asset_path.clone();
            spend.asset_position = base.asset_position;
            spend.asset_anchor = base.asset_anchor;
            spend.compliance_anchor = base.compliance_anchor;
            spend.compliance_path = base.compliance_path.clone();
            spend.compliance_position = base.compliance_position;
            spend.is_regulated = is_regulated;
            spend.target_timestamp = base.target_timestamp;
            spend.tx_blinding_nonce = tx_blinding_nonce;
            spend.asset_policy = Some(base.asset_policy.clone());
            spend
                .set_compliance_details(&mut rng)
                .expect("set transfer spend compliance details");
            spends.push(spend);
        }

        let sender_leaf = spends[0]
            .compliance_leaf
            .clone()
            .expect("first transfer spend must have a sender compliance leaf");
        let mut outputs = Vec::with_capacity(output_amounts.len());
        for amount in &output_amounts {
            let mut output = ShieldedOutputPlan::new(
                &mut rng,
                Value {
                    amount: Amount::from(*amount),
                    asset_id,
                },
                base.address.clone(),
            );
            output.asset_indexed_leaf = base.asset_indexed_leaf.clone();
            output.asset_path = base.asset_path.clone();
            output.asset_position = base.asset_position;
            output.asset_anchor = base.asset_anchor;
            output.compliance_anchor = base.compliance_anchor;
            output.compliance_path = base.compliance_path.clone();
            output.compliance_position = base.compliance_position;
            output.is_regulated = is_regulated;
            output.target_timestamp = base.target_timestamp;
            output.tx_blinding_nonce = tx_blinding_nonce;
            output.asset_policy = Some(base.asset_policy.clone());
            output
                .set_compliance_details(
                    &mut rng,
                    &sender_leaf,
                    sender_leaf.clone(),
                    tx_blinding_nonce,
                )
                .expect("set transfer output compliance details");
            outputs.push(output);
        }

        let value_blinding = Fr::rand(&mut rng);
        let transfer_plan = TransferPlan::new(
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            value_blinding,
        )
        .expect("build transfer plan");

        let (public, _) = transfer_plan
            .transfer_public_private(&base.fvk, &state_commitment_proofs, anchor)
            .expect("derive transfer public/private inputs");
        let transfer = transfer_plan
            .transfer(
                &base.fvk,
                vec![crate::note_reshape::dummy_spend_auth_sig(); crate::transfer_input_count()],
                state_commitment_proofs,
                anchor,
                &PayloadKey::random_key(&mut rng),
            )
            .expect("build transfer action");

        (
            transfer,
            public,
            TransactionContext {
                anchor,
                effect_hash: Default::default(),
            },
        )
    }

    pub(crate) fn build_consolidate_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: crate::ConsolidateFamilyId,
    ) -> (
        crate::ConsolidateProofPublic,
        crate::ConsolidateProofPrivate,
    ) {
        use crate::{ConsolidatePlan, ShieldedInputPlan, ShieldedOutputPlan};
        use penumbra_sdk_asset::{asset, Value};
        use penumbra_sdk_num::Amount;

        let base = generate_base_test_data(rng, 1, 100, false);
        let input_total = 100u64
            .checked_mul(family_id.input_count() as u64)
            .expect("consolidate input total fits in u64");
        let input_amounts = split_transfer_amounts(family_id.input_count(), input_total);
        let asset_id = asset::Id(Fq::from(1u64));

        let notes = input_amounts
            .iter()
            .map(|amount| {
                crate::Note::from_parts(
                    base.address.clone(),
                    Value {
                        amount: Amount::from(*amount),
                        asset_id,
                    },
                    crate::Rseed::generate(rng),
                )
                .expect("create consolidate test note")
            })
            .collect::<Vec<_>>();

        let mut sct = tct::Tree::new();
        for note in &notes {
            sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        }
        let anchor = sct.root();
        let state_commitment_proofs = notes
            .iter()
            .map(|note| {
                sct.witness(note.commit())
                    .expect("state commitment witness")
            })
            .collect::<Vec<_>>();

        let spends = notes
            .iter()
            .cloned()
            .zip(state_commitment_proofs.iter())
            .map(|(note, proof)| ShieldedInputPlan::new(rng, note, proof.position()))
            .collect::<Vec<_>>();

        let outputs = vec![ShieldedOutputPlan::new(
            rng,
            Value {
                amount: Amount::from(input_total),
                asset_id,
            },
            base.address.clone(),
        )];

        let plan = ConsolidatePlan::new(
            family_id,
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            Fr::rand(rng),
        )
        .expect("build consolidate plan");
        plan.consolidate_public_private(&base.fvk, &state_commitment_proofs, anchor)
            .expect("derive consolidate public/private inputs")
    }

    pub(crate) fn build_consolidate_roundtrip_inputs(
        family_id: crate::ConsolidateFamilyId,
    ) -> (
        crate::ConsolidateProofPublic,
        crate::ConsolidateProofPrivate,
    ) {
        let mut rng = rand::thread_rng();
        build_consolidate_roundtrip_inputs_with_rng(&mut rng, family_id)
    }

    pub(crate) fn build_split_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: crate::SplitFamilyId,
    ) -> (crate::SplitProofPublic, crate::SplitProofPrivate) {
        use crate::{ShieldedInputPlan, ShieldedOutputPlan, SplitPlan};
        use penumbra_sdk_asset::{asset, Value};
        use penumbra_sdk_num::Amount;

        let base = generate_base_test_data(rng, 1, 100, false);
        let input_total = 100u64
            .checked_mul(family_id.output_count() as u64)
            .expect("split input total fits in u64");
        let output_amounts = split_transfer_amounts(family_id.output_count(), input_total);
        let asset_id = asset::Id(Fq::from(1u64));

        let note = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(input_total),
                asset_id,
            },
            crate::Rseed::generate(rng),
        )
        .expect("create split test note");

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, note.commit()).unwrap();
        let anchor = sct.root();
        let state_commitment_proof = sct
            .witness(note.commit())
            .expect("state commitment witness");

        let spends = vec![ShieldedInputPlan::new(
            rng,
            note,
            state_commitment_proof.position(),
        )];
        let outputs = output_amounts
            .iter()
            .map(|amount| {
                ShieldedOutputPlan::new(
                    rng,
                    Value {
                        amount: Amount::from(*amount),
                        asset_id,
                    },
                    base.address.clone(),
                )
            })
            .collect::<Vec<_>>();

        let plan = SplitPlan::new(
            family_id,
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            Fr::rand(rng),
        )
        .expect("build split plan");
        plan.split_public_private(&base.fvk, &[state_commitment_proof], anchor)
            .expect("derive split public/private inputs")
    }

    pub(crate) fn build_split_roundtrip_inputs(
        family_id: crate::SplitFamilyId,
    ) -> (crate::SplitProofPublic, crate::SplitProofPrivate) {
        let mut rng = rand::thread_rng();
        build_split_roundtrip_inputs_with_rng(&mut rng, family_id)
    }

    pub(crate) fn build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: ShieldedIcs20WithdrawalFamilyId,
        is_regulated: bool,
    ) -> (
        crate::ShieldedIcs20WithdrawalProofPublic,
        crate::ShieldedIcs20WithdrawalProofPrivate,
    ) {
        let base = generate_base_test_data(
            rng,
            if is_regulated {
                REGULATED_ASSET_ID
            } else {
                UNREGULATED_ASSET_ID
            },
            120,
            is_regulated,
        );

        let note_a = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(70u64),
                asset_id: base.value.asset_id,
            },
            crate::Rseed::generate(rng),
        )
        .expect("create shielded ICS-20 withdrawal note a");
        let note_b = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(50u64),
                asset_id: base.value.asset_id,
            },
            crate::Rseed::generate(rng),
        )
        .expect("create shielded ICS-20 withdrawal note b");

        let mut spend_a = ShieldedInputPlan::new(rng, note_a.clone(), 0u64.into());
        let mut spend_b = ShieldedInputPlan::new(rng, note_b.clone(), 1u64.into());
        for spend in [&mut spend_a, &mut spend_b] {
            spend.is_regulated = is_regulated;
            spend.tx_blinding_nonce = Fr::from(11u64);
            spend.target_timestamp = base.target_timestamp;
            spend.compliance_anchor = base.compliance_anchor;
            spend.compliance_path = base.compliance_path.clone();
            spend.compliance_position = base.compliance_position;
            spend.asset_anchor = base.asset_anchor;
            spend.asset_path = base.asset_path.clone();
            spend.asset_position = base.asset_position;
            spend.asset_indexed_leaf = base.asset_indexed_leaf.clone();
            spend.compliance_leaf = Some(base.user_leaf.clone());
            spend.dk_pub = base.dk_pub;
            spend.ring_pk = base.ring_pk;
            spend.threshold = base.asset_indexed_leaf.params.threshold;
        }

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, note_a.commit()).unwrap();
        sct.insert(tct::Witness::Keep, note_b.commit()).unwrap();
        let anchor = sct.root();
        let state_commitment_proofs = vec![
            sct.witness(note_a.commit()).expect("witness note a"),
            sct.witness(note_b.commit()).expect("witness note b"),
        ];
        let change_note = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(20u64),
                asset_id: base.value.asset_id,
            },
            crate::Rseed::generate(rng),
        )
        .expect("create shielded ICS-20 withdrawal change note");

        let input_publics = vec![
            ShieldedIcs20WithdrawalInputPublic {
                nullifier: spend_a.nullifier(&base.fvk),
                rk: spend_a.rk(&base.fvk),
            },
            ShieldedIcs20WithdrawalInputPublic {
                nullifier: spend_b.nullifier(&base.fvk),
                rk: spend_b.rk(&base.fvk),
            },
        ];
        let input_privates = vec![
            ShieldedIcs20WithdrawalInputPrivate {
                state_commitment_proof: state_commitment_proofs[0].clone(),
                spent_note: note_a,
                spend_auth_randomizer: spend_a.randomizer,
                is_dummy: false,
                dummy_nullifier_seed: Fq::from(0u64),
                dummy_spend_auth_key: Fr::from(0u64),
            },
            ShieldedIcs20WithdrawalInputPrivate {
                state_commitment_proof: state_commitment_proofs[1].clone(),
                spent_note: note_b,
                spend_auth_randomizer: spend_b.randomizer,
                is_dummy: false,
                dummy_nullifier_seed: Fq::from(0u64),
                dummy_spend_auth_key: Fr::from(0u64),
            },
        ];

        (
            ShieldedIcs20WithdrawalProofPublic {
                family_id,
                anchor,
                balance_commitment: Balance::default().commit(Fr::from(13u64)),
                asset_anchor: base.asset_anchor,
                compliance_anchor: base.compliance_anchor,
                target_timestamp: Fq::from(base.target_timestamp),
                inputs: input_publics,
                change_output: ShieldedIcs20WithdrawalChangePublic {
                    note_commitment: change_note.commit(),
                },
                outbound_asset_id: base.value.asset_id.0,
                outbound_amount: Fq::from(100u64),
                withdrawal_effect_hash_lo: Fq::from(21u64),
                withdrawal_effect_hash_hi: Fq::from(22u64),
            },
            ShieldedIcs20WithdrawalProofPrivate {
                family_id,
                action_balance_blinding: Fr::from(13u64),
                ak: *base.fvk.spend_verification_key(),
                nk: *base.fvk.nullifier_key(),
                asset_path: base.asset_path,
                asset_position: base.asset_position,
                asset_indexed_leaf: base.asset_indexed_leaf,
                is_regulated,
                sender_compliance_path: base.compliance_path,
                sender_compliance_position: base.compliance_position,
                sender_leaf: base.user_leaf,
                inputs: input_privates,
                change_output: ShieldedIcs20WithdrawalChangePrivate {
                    created_note: change_note,
                },
            },
        )
    }

    pub(crate) fn build_shielded_ics20_withdrawal_roundtrip_inputs(
        family_id: ShieldedIcs20WithdrawalFamilyId,
        is_regulated: bool,
    ) -> (
        crate::ShieldedIcs20WithdrawalProofPublic,
        crate::ShieldedIcs20WithdrawalProofPrivate,
    ) {
        let mut rng = rand::thread_rng();
        build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng(&mut rng, family_id, is_regulated)
    }

    fn split_transfer_amounts(parts: usize, total: u64) -> Vec<u64> {
        assert!(
            parts > 0,
            "transfer test amount split requires at least one part"
        );
        assert!(
            total >= parts as u64,
            "transfer test amount split requires total >= number of parts"
        );
        let base = total / parts as u64;
        let remainder = total % parts as u64;
        (0..parts)
            .map(|index| base + u64::from(index < remainder as usize))
            .collect()
    }

    fn test_transfer_proof_roundtrip(is_regulated: bool) {
        let mode = if is_regulated {
            "regulated"
        } else {
            "unregulated"
        };
        let started = Instant::now();
        eprintln!("[transfer roundtrip] start mode={mode}");

        let phase_started = Instant::now();
        let expected_pvk = penumbra_sdk_proof_params::transfer_proof_verification_key();
        let (public, private) = build_transfer_roundtrip_inputs(is_regulated);
        eprintln!(
            "[transfer roundtrip] mode={mode} built inputs in {:.2}s",
            phase_started.elapsed().as_secs_f64()
        );

        let phase_started = Instant::now();
        let proof = crate::TransferProof::prove(public.clone(), private).unwrap_or_else(|error| {
            panic!("can generate transfer proof: {error}");
        });
        eprintln!(
            "[transfer roundtrip] mode={mode} proved in {:.2}s",
            phase_started.elapsed().as_secs_f64()
        );

        let phase_started = Instant::now();
        let item = proof.to_batch_item(&public).unwrap_or_else(|error| {
            panic!("can build transfer batch item: {error}");
        });
        assert_eq!(item.public_inputs.len(), 1);
        eprintln!(
            "[transfer roundtrip] mode={mode} built batch item in {:.2}s",
            phase_started.elapsed().as_secs_f64()
        );

        let phase_started = Instant::now();
        proof.verify(&public).expect("proof should verify");
        penumbra_sdk_proof_params::batch::batch_verify(expected_pvk, std::slice::from_ref(&item))
            .expect("single-item batch verification should succeed");
        eprintln!(
            "[transfer roundtrip] mode={mode} verified in {:.2}s",
            phase_started.elapsed().as_secs_f64()
        );
        eprintln!(
            "[transfer roundtrip] done mode={mode} total {:.2}s",
            started.elapsed().as_secs_f64()
        );
    }

    /// Unified proof roundtrip test function for the surviving shielded families.
    pub fn full_proof_roundtrip(circuit_type: CircuitType, is_regulated: bool) {
        match circuit_type {
            CircuitType::Transfer => test_transfer_proof_roundtrip(is_regulated),
        }
    }
}
