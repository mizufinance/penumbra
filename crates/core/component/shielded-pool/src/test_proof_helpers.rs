//! Common test helpers for shielded-pool proof tests.

#[allow(dead_code)]
pub mod proof_test_helpers {
    use std::time::Instant;

    /// Test asset ID for regulated assets
    pub const REGULATED_ASSET_ID: u64 = 1;
    /// Test asset ID for unregulated assets
    pub const UNREGULATED_ASSET_ID: u64 = 2;

    use decaf377::{Fq, Fr};
    use shieldd_sdk_asset::{asset, Balance, Value};
    use shieldd_sdk_compliance::{
        encrypt_withdrawal_with_material, withdrawal_encryption_key, IndexedLeaf,
        IndexedMerkleTree, MerklePath,
    };
    use shieldd_sdk_keys::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        Address, FullViewingKey, PayloadKey,
    };
    use shieldd_sdk_num::Amount;
    use shieldd_sdk_tct as tct;
    use shieldd_sdk_txhash::TransactionContext;

    use crate::{
        note_reshape_padding::{dummy_state_commitment_proof, HiddenArityPadder},
        Note, Rseed, ShieldedIcs20WithdrawalChangePrivate, ShieldedIcs20WithdrawalChangePublic,
        ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalInputPublic,
        ShieldedIcs20WithdrawalOptionalInputPrivate, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic, ShieldedIcs20WithdrawalRequiredInputPrivate,
        ShieldedInputPlan,
    };

    #[derive(Clone, Copy)]
    pub(crate) enum WithdrawalAccumulatorTestMode {
        Padding,
        Origin,
        Continuation { prior_volume: u128 },
    }

    /// Create valid IMT proof data for an unregulated asset.
    ///
    /// Returns (asset_anchor, indexed_leaf, merkle_path, position) that satisfy circuit constraints.
    /// The asset is proven to be unregulated via non-membership (falls in a gap).
    pub fn create_imt_non_membership_proof(
        asset_id: Fq,
    ) -> (tct::StateCommitment, IndexedLeaf, MerklePath, u64) {
        shieldd_sdk_compliance::create_default_imt_proof(asset_id)
    }

    /// Create valid user tree (QuadTree) proof data.
    ///
    /// Returns (compliance_anchor, merkle_path, position) that satisfy circuit constraints.
    pub fn create_user_tree_proof(
        user_leaf: &shieldd_sdk_compliance::ComplianceLeaf,
    ) -> (tct::StateCommitment, MerklePath, u64) {
        shieldd_sdk_compliance::default_user_proof(user_leaf)
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
        create_imt_membership_proof_with_daily_volume_limit(asset_id, ring_pk, dk_pub, u128::MAX)
    }

    pub fn create_imt_membership_proof_with_daily_volume_limit(
        asset_id: Fq,
        ring_pk: decaf377::Element,
        dk_pub: decaf377::Element,
        daily_volume_limit: u128,
    ) -> (tct::StateCommitment, IndexedLeaf, MerklePath, u64) {
        let mut tree = IndexedMerkleTree::new();
        let policy = shieldd_sdk_compliance::AssetPolicy::new(
            dk_pub,
            daily_volume_limit,
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

    /// Non-membership IMT proof against a POPULATED tree, where the predecessor
    /// (low) leaf is a real regulated asset — mirroring the live registry where an
    /// unregulated asset's gap predecessor is a registered asset's leaf, not the
    /// empty-tree sentinel. Used to reproduce the orbis live-transfer constraint.
    pub fn create_imt_non_membership_proof_populated(
        asset_id: Fq,
        low_asset_id: Fq,
        low_ring_pk: decaf377::Element,
        low_dk_pub: decaf377::Element,
        low_daily_volume_limit: u128,
    ) -> (tct::StateCommitment, IndexedLeaf, MerklePath, u64) {
        let mut tree = IndexedMerkleTree::new();
        let policy = shieldd_sdk_compliance::AssetPolicy::new(
            low_dk_pub,
            low_daily_volume_limit,
            vec![],
            None,
            "low-ring-id".to_string(),
            low_ring_pk,
            "low-policy-id".to_string(),
            "read".to_string(),
            "document".to_string(),
        );
        tree.insert(low_asset_id, &policy)
            .expect("insert populated low asset");
        let (low_pos, low_leaf, auth_path) = tree
            .non_membership_proof(asset_id)
            .expect("populated non-membership proof");
        let merkle_path = MerklePath::from_auth_path(auth_path);
        let anchor = tct::StateCommitment(tree.root().0);
        (anchor, low_leaf, merkle_path, low_pos)
    }

    /// Circuit type for unified testing
    #[derive(Debug, Clone, Copy)]
    pub enum CircuitType {
        Transfer,
    }

    /// Shared fixture layer used by all proof-family test builders.
    pub struct BaseTestData {
        pub note: Note,
        pub address: shieldd_sdk_keys::Address,
        pub value: shieldd_sdk_asset::Value,
        pub balance_blinding: Fr,
        pub fvk: shieldd_sdk_keys::keys::FullViewingKey,
        pub sk: SpendKey,
        pub user_leaf: shieldd_sdk_compliance::ComplianceLeaf,
        pub sender_address: shieldd_sdk_keys::Address,
        pub counterparty_leaf: shieldd_sdk_compliance::ComplianceLeaf,
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
        pub asset_policy: shieldd_sdk_compliance::AssetPolicy,
    }

    impl BaseTestData {
        pub fn action_witness(&self) -> crate::ActionWitness {
            crate::ActionWitness {
                asset: crate::AssetWitness {
                    asset_id: self.value.asset_id,
                    root: self.asset_anchor,
                    leaf: self.asset_indexed_leaf.clone(),
                    position: self.asset_position,
                    path: self.asset_path.clone(),
                    is_regulated: self.asset_indexed_leaf.value == self.value.asset_id.0,
                },
                policy: (self.asset_indexed_leaf.value == self.value.asset_id.0)
                    .then(|| self.asset_policy.clone()),
                user_root: self.compliance_anchor,
                sender: crate::UserWitness {
                    leaf: self.user_leaf.clone(),
                    position: self.compliance_position,
                    path: self.compliance_path.clone(),
                },
            }
        }

        pub fn transfer_context(&self, nonce: Fr) -> crate::TransferContext {
            let witness = self.action_witness();
            crate::TransferContext {
                recipient: witness.sender.clone(),
                witness,
                timestamp: self.target_timestamp,
                nonce,
            }
        }
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
        use shieldd_sdk_num::Amount;

        // Receiver identity
        let seed_phrase = SeedPhrase::generate(&mut *rng);
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0))
            .expect("test spend key should satisfy key refinements");
        let fvk = sk.full_viewing_key();
        let ivk = fvk.incoming();
        let address = ivk.payment_address(0u32.into());

        // Distinct sender identity for transfer-side compliance fixtures.
        let sender_seed = SeedPhrase::generate(&mut *rng);
        let sender_sk = SpendKey::from_seed_phrase_bip44(sender_seed, &Bip44Path::new(0))
            .expect("test spend key should satisfy key refinements");
        let sender_fvk = sender_sk.full_viewing_key();
        let sender_ivk = sender_fvk.incoming();
        let sender_address = sender_ivk.payment_address(0u32.into());

        let value = Value {
            amount: Amount::from(amount),
            asset_id,
        };

        let note = Note::from_parts(
            address.clone(),
            value,
            Rseed::generate(&mut *rng),
            crate::RecoveryCommitment::unavailable(),
        )
        .expect("can create note");

        let balance_blinding = Fr::rand(&mut *rng);

        // Determine keys before IMT proof (Policy-in-Leaf binds ring_pk into the leaf)
        let (ring_pk, dk_pub) = if is_regulated {
            let ring_sk = Fr::from(1u64);
            let ring_pk = decaf377::Element::GENERATOR * ring_sk;
            (ring_pk, decaf377::Element::GENERATOR)
        } else {
            (
                *shieldd_sdk_compliance::UNREGULATED_SINK_RING_PK,
                *shieldd_sdk_compliance::UNREGULATED_SINK_DK_PUB,
            )
        };

        let asset_id_fq = value.asset_id.0;
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) = if is_regulated {
            create_imt_membership_proof(asset_id_fq, ring_pk, dk_pub)
        } else {
            create_imt_non_membership_proof(asset_id_fq)
        };
        let asset_policy = if is_regulated {
            shieldd_sdk_compliance::AssetPolicy::new(
                dk_pub,
                asset_indexed_leaf.params.daily_volume_limit,
                vec![],
                None,
                "test-ring-id".to_string(),
                ring_pk,
                "test-policy-id".to_string(),
                "read".to_string(),
                "document".to_string(),
            )
        } else {
            shieldd_sdk_compliance::AssetPolicy::default_unregulated()
        };

        // Receiver ACK
        let d = shieldd_sdk_compliance::derive_compliance_scalar(&address);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack_receiver = ring_pk * d_fr;

        // Sender ACK used by transfer-side compliance fixtures.
        let sender_d = shieldd_sdk_compliance::derive_compliance_scalar(&sender_address);
        let sender_d_fr = Fr::from_le_bytes_mod_order(&sender_d.to_bytes());
        let ack_sender = ring_pk * sender_d_fr;

        let make_leaf = |address: Address, wallet_fvk: &FullViewingKey| {
            if is_regulated {
                let rnk_dh_pk = address.diversified_generator().clone();
                let rnk = shieldd_sdk_compliance::derive_regulated_nullifier_key(
                    wallet_fvk.incoming(),
                    &address,
                    value.asset_id,
                    ring_pk,
                    rnk_dh_pk,
                )
                .expect("test regulated nullifier derivation is valid");
                shieldd_sdk_compliance::ComplianceLeaf::registered_from_rnk(
                    address,
                    value.asset_id,
                    ring_pk,
                    rnk_dh_pk,
                    rnk,
                )
                .expect("test compliance registration is valid")
            } else {
                shieldd_sdk_compliance::ComplianceLeaf::synthetic_unregulated(
                    address,
                    value.asset_id,
                )
            }
        };
        let user_leaf = make_leaf(address.clone(), &fvk);
        let counterparty_leaf = make_leaf(sender_address.clone(), &sender_fvk);

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
        use shieldd_sdk_asset::Value;
        use shieldd_sdk_num::Amount;

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
                    crate::RecoveryCommitment::unavailable(),
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
            let spend = ShieldedInputPlan::new(rng, note, proof.position());
            spends.push(spend);
        }

        let mut outputs = Vec::with_capacity(output_amounts.len());
        for amount in &output_amounts {
            let output = ShieldedOutputPlan::new(
                rng,
                Value {
                    amount: Amount::from(*amount),
                    asset_id,
                },
                base.address.clone(),
            );
            outputs.push(output);
        }

        let value_blinding = Fr::rand(rng);
        let transfer = TransferPlan::new(
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            value_blinding,
            base.transfer_context(tx_blinding_nonce),
            crate::VolumeAccumulatorPlan::padding(base.target_timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build transfer plan");

        transfer
            .transfer_public_private(&base.fvk, &state_commitment_proofs, anchor, 0)
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
        use shieldd_sdk_asset::asset;

        build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng(
            rng,
            asset::Id(Fq::from(1u64)),
            is_regulated,
            send_to_self,
        )
    }

    pub(crate) fn build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        asset_id: shieldd_sdk_asset::asset::Id,
        is_regulated: bool,
        send_to_self: bool,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        let base = generate_base_test_data_for_asset(rng, asset_id, 100, is_regulated);
        build_transfer_hidden_arity_from_base(rng, base, asset_id, send_to_self, false, None)
    }

    pub(crate) fn build_transfer_flagged_hidden_arity_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        let asset_id = shieldd_sdk_asset::asset::Id(Fq::from(1u64));
        let mut base = generate_base_test_data_for_asset(rng, asset_id, 100, true);
        let daily_volume_limit = 1u128;
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            create_imt_membership_proof_with_daily_volume_limit(
                asset_id.0,
                base.ring_pk,
                base.dk_pub,
                daily_volume_limit,
            );
        base.asset_anchor = asset_anchor;
        base.asset_indexed_leaf = asset_indexed_leaf;
        base.asset_path = asset_path;
        base.asset_position = asset_position;
        base.asset_policy = shieldd_sdk_compliance::AssetPolicy::new(
            base.dk_pub,
            daily_volume_limit,
            vec![],
            None,
            "test-ring-id".to_string(),
            base.ring_pk,
            "test-policy-id".to_string(),
            "read".to_string(),
            "document".to_string(),
        );
        build_transfer_hidden_arity_from_base(rng, base, asset_id, false, false, None)
    }

    /// Reproduces the orbis live unregulated non-base transfer: identical to
    /// [`build_transfer_hidden_arity_roundtrip_inputs_for_asset_with_rng`] for an
    /// unregulated asset, except the IMT non-membership predecessor is a real
    /// regulated asset (populated tree) rather than the empty-tree sentinel.
    pub(crate) fn build_transfer_hidden_arity_roundtrip_inputs_for_asset_populated(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        asset_id: shieldd_sdk_asset::asset::Id,
        low_asset_id: Fq,
        low_daily_volume_limit: u128,
        send_to_self: bool,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        let mut base = generate_base_test_data_for_asset(rng, asset_id, 100, false);
        // Real regulated predecessor leaf keys (distinct from the unregulated sink).
        let low_ring_sk = Fr::rand(&mut *rng);
        let low_ring_pk = decaf377::Element::GENERATOR * low_ring_sk;
        let low_dk_pub = decaf377::Element::GENERATOR;
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            create_imt_non_membership_proof_populated(
                asset_id.0,
                low_asset_id,
                low_ring_pk,
                low_dk_pub,
                low_daily_volume_limit,
            );
        base.asset_anchor = asset_anchor;
        base.asset_indexed_leaf = asset_indexed_leaf;
        base.asset_path = asset_path;
        base.asset_position = asset_position;
        build_transfer_hidden_arity_from_base(rng, base, asset_id, send_to_self, false, None)
    }

    pub(crate) fn build_transfer_accumulating_hidden_arity_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        daily_volume_limit: u128,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        let asset_id = shieldd_sdk_asset::asset::Id(Fq::from(1u64));
        let mut base = generate_base_test_data_for_asset(rng, asset_id, 100, true);
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            create_imt_membership_proof_with_daily_volume_limit(
                asset_id.0,
                base.ring_pk,
                base.dk_pub,
                daily_volume_limit,
            );
        base.asset_anchor = asset_anchor;
        base.asset_indexed_leaf = asset_indexed_leaf;
        base.asset_path = asset_path;
        base.asset_position = asset_position;
        base.asset_policy = shieldd_sdk_compliance::AssetPolicy::new(
            base.dk_pub,
            daily_volume_limit,
            vec![],
            None,
            "test-ring-id".to_string(),
            base.ring_pk,
            "test-policy-id".to_string(),
            "read".to_string(),
            "document".to_string(),
        );
        build_transfer_hidden_arity_from_base(rng, base, asset_id, false, true, None)
    }

    pub(crate) fn build_transfer_continuing_accumulator_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        let asset_id = shieldd_sdk_asset::asset::Id(Fq::from(1u64));
        let daily_volume_limit = 125u128;
        let mut base = generate_base_test_data_for_asset(rng, asset_id, 100, true);
        let (asset_anchor, asset_indexed_leaf, asset_path, asset_position) =
            create_imt_membership_proof_with_daily_volume_limit(
                asset_id.0,
                base.ring_pk,
                base.dk_pub,
                daily_volume_limit,
            );
        base.asset_anchor = asset_anchor;
        base.asset_indexed_leaf = asset_indexed_leaf;
        base.asset_path = asset_path;
        base.asset_position = asset_position;
        base.asset_policy = shieldd_sdk_compliance::AssetPolicy::new(
            base.dk_pub,
            daily_volume_limit,
            vec![],
            None,
            "test-ring-id".to_string(),
            base.ring_pk,
            "test-policy-id".to_string(),
            "read".to_string(),
            "document".to_string(),
        );
        build_transfer_hidden_arity_from_base(rng, base, asset_id, false, true, Some(25))
    }

    fn build_transfer_hidden_arity_from_base(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        base: BaseTestData,
        asset_id: shieldd_sdk_asset::asset::Id,
        send_to_self: bool,
        accumulate_volume: bool,
        continuation_prior_volume: Option<u128>,
    ) -> (crate::TransferProofPublic, crate::TransferProofPrivate) {
        use crate::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
        use shieldd_sdk_asset::Value;
        use shieldd_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
        use shieldd_sdk_num::Amount;

        let recipient_address = if send_to_self {
            base.address.clone()
        } else {
            let recipient_seed = SeedPhrase::generate(&mut *rng);
            let recipient_sk = SpendKey::from_seed_phrase_bip44(recipient_seed, &Bip44Path::new(1))
                .expect("test spend key should satisfy key refinements");
            recipient_sk
                .full_viewing_key()
                .incoming()
                .payment_address(0u32.into())
        };

        let note = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(100u64),
                asset_id,
            },
            crate::Rseed::generate(&mut *rng),
            crate::RecoveryCommitment::unavailable(),
        )
        .expect("create hidden-arity transfer test note");

        let day_start = crate::select_accumulator_day(base.target_timestamp);
        let accumulator_subject = crate::VolumeAccumulatorState::subject(&base.address, asset_id);
        let accumulator_prior_state =
            continuation_prior_volume.map(|undisclosed_volume| crate::VolumeAccumulatorState {
                subject: accumulator_subject,
                day_start,
                undisclosed_volume,
                blinding: Fq::rand(&mut *rng),
            });

        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, note.commit())
            .expect("insert hidden-arity transfer input note");
        if let Some(prior) = &accumulator_prior_state {
            sct.insert(tct::Witness::Keep, prior.commitment())
                .expect("insert prior volume accumulator state");
        }
        let state_commitment_proof = sct
            .witness(note.commit())
            .expect("witness hidden-arity transfer input note");
        let accumulator_prior_proof = accumulator_prior_state.as_ref().map(|prior| {
            sct.witness(prior.commitment())
                .expect("witness prior volume accumulator state")
        });
        let anchor = sct.root();

        let tx_blinding_nonce = Fr::rand(rng);
        let spend = ShieldedInputPlan::new(rng, note.clone(), state_commitment_proof.position());

        let recipient_leaf = if send_to_self {
            base.user_leaf.clone()
        } else if base.action_witness().asset.is_regulated {
            shieldd_sdk_compliance::ComplianceLeaf::registered_from_rnk(
                recipient_address.clone(),
                asset_id,
                base.ring_pk,
                recipient_address.diversified_generator().clone(),
                Fq::from(2u64),
            )
            .expect("test recipient compliance registration is valid")
        } else {
            shieldd_sdk_compliance::ComplianceLeaf::synthetic_unregulated(
                recipient_address.clone(),
                asset_id,
            )
        };
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
            let mut user_tree = shieldd_sdk_compliance::QuadTree::new();
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

        let output = ShieldedOutputPlan::new(
            rng,
            Value {
                amount: Amount::from(100u64),
                asset_id,
            },
            recipient_address,
        );

        let mut context = base.transfer_context(tx_blinding_nonce);
        context.witness.user_root = shared_compliance_anchor;
        context.witness.sender.path = sender_compliance_path;
        context.witness.sender.position = sender_compliance_position;
        context.recipient = crate::UserWitness {
            leaf: recipient_leaf,
            path: recipient_compliance_path,
            position: recipient_compliance_position,
        };
        let volume_accumulator = if accumulate_volume {
            let prior_volume = accumulator_prior_state
                .as_ref()
                .map(|state| state.undisclosed_volume)
                .unwrap_or(0);
            let successor_blinding = Fq::rand(rng);
            let accumulator = match accumulator_prior_state {
                Some(prior) => crate::VolumeAccumulatorPlan::continuation(
                    prior.clone(),
                    prior.commitment(),
                    accumulator_prior_proof
                        .as_ref()
                        .map(|proof| u64::from(proof.position()))
                        .expect("continuation has a prior proof"),
                    prior_volume + 100,
                    successor_blinding,
                )
                .expect("valid continuation accumulator plan"),
                None => crate::VolumeAccumulatorPlan::origin(crate::VolumeAccumulatorState {
                    subject: accumulator_subject,
                    day_start,
                    undisclosed_volume: prior_volume + 100,
                    blinding: successor_blinding,
                }),
            };
            accumulator
        } else {
            crate::VolumeAccumulatorPlan::padding(day_start)
        };

        let transfer = TransferPlan::new(
            vec![spend],
            vec![output],
            Fr::rand(rng),
            context,
            volume_accumulator,
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build hidden-arity transfer plan");

        let mut state_commitment_proofs = vec![state_commitment_proof];
        if let Some(proof) = accumulator_prior_proof {
            state_commitment_proofs.push(proof);
        }

        transfer
            .transfer_public_private(&base.fvk, &state_commitment_proofs, anchor, 0)
            .expect("derive hidden-arity transfer public/private inputs")
    }

    pub(crate) fn build_transfer_action_and_public(
        is_regulated: bool,
    ) -> (
        crate::Transfer,
        crate::TransferProofPublic,
        TransactionContext,
    ) {
        build_transfer_action_and_public_inner(is_regulated, true)
    }

    pub fn build_transfer_action_and_public_without_proof(
        is_regulated: bool,
    ) -> (
        crate::Transfer,
        crate::TransferProofPublic,
        TransactionContext,
    ) {
        build_transfer_action_and_public_inner(is_regulated, false)
    }

    fn build_transfer_action_and_public_inner(
        is_regulated: bool,
        generate_proof: bool,
    ) -> (
        crate::Transfer,
        crate::TransferProofPublic,
        TransactionContext,
    ) {
        use crate::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan};
        use shieldd_sdk_asset::{asset, Value};

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
                    crate::RecoveryCommitment::unavailable(),
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
            let spend = ShieldedInputPlan::new(&mut rng, note, proof.position());
            spends.push(spend);
        }

        let mut outputs = Vec::with_capacity(output_amounts.len());
        for amount in &output_amounts {
            let output = ShieldedOutputPlan::new(
                &mut rng,
                Value {
                    amount: Amount::from(*amount),
                    asset_id,
                },
                base.address.clone(),
            );
            outputs.push(output);
        }

        let value_blinding = Fr::rand(&mut rng);
        let transfer_plan = TransferPlan::new(
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            value_blinding,
            base.transfer_context(tx_blinding_nonce),
            crate::VolumeAccumulatorPlan::padding(base.target_timestamp),
            crate::TransferProofContext::Ordinary,
            crate::discovery::Parameters::default(),
        )
        .expect("build transfer plan");

        let (public, _) = transfer_plan
            .transfer_public_private(&base.fvk, &state_commitment_proofs, anchor, 0)
            .expect("derive transfer public/private inputs");
        let effect_hash = shieldd_sdk_txhash::EffectHash::default();
        let auth_sigs = transfer_plan
            .spend_randomizers()
            .map(|randomizer| {
                let rsk = base.sk.spend_auth_key().randomize(&randomizer);
                rsk.sign(&mut rng, effect_hash.as_ref())
            })
            .collect();
        let memo_key = PayloadKey::random_key(&mut rng);
        let transfer = if generate_proof {
            transfer_plan
                .build_unauth_transfer(
                    &base.fvk,
                    auth_sigs,
                    state_commitment_proofs,
                    anchor,
                    &memo_key,
                    0,
                )
                .expect("build transfer action")
        } else {
            transfer_plan
                .build_unauth_transfer_with_proof(
                    &base.fvk,
                    auth_sigs,
                    anchor,
                    &memo_key,
                    crate::TransferProof::default(),
                    0,
                )
                .expect("build transfer action without proof")
        };

        (
            transfer,
            public,
            TransactionContext {
                anchor,
                effect_hash,
                recent_position_floor: 0,
            },
        )
    }

    pub(crate) fn build_note_reshape_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: crate::NoteReshapeFamilyId,
    ) -> (
        crate::NoteReshapeProofPublic,
        crate::NoteReshapeProofPrivate,
    ) {
        use crate::{NoteReshapePlan, ShieldedInputPlan, ShieldedOutputPlan};
        use shieldd_sdk_asset::Value;
        use shieldd_sdk_num::Amount;

        let base = generate_base_test_data(rng, REGULATED_ASSET_ID, 100, true);
        let real_inputs = family_id.min_real_inputs();
        let real_outputs = family_id.min_real_outputs();
        let input_total = 100u64
            .checked_mul(if family_id.is_many_to_one() {
                real_inputs
            } else {
                real_outputs
            } as u64)
            .expect("note reshape input total fits in u64");
        let input_amounts = if family_id.is_many_to_one() {
            split_transfer_amounts(real_inputs, input_total)
        } else {
            vec![input_total]
        };
        let output_amounts = if family_id.is_many_to_one() {
            vec![input_total]
        } else {
            split_transfer_amounts(real_outputs, input_total)
        };
        let asset_id = base.value.asset_id;

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
                    crate::RecoveryCommitment::unavailable(),
                )
                .expect("create note reshape test note")
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

        let tx_blinding_nonce = Fr::rand(rng);
        let spends = notes
            .iter()
            .cloned()
            .zip(state_commitment_proofs.iter())
            .map(|(note, proof)| {
                let spend = ShieldedInputPlan::new(rng, note, proof.position());
                spend
            })
            .collect::<Vec<_>>();

        let outputs = output_amounts
            .iter()
            .map(|amount| {
                let output = ShieldedOutputPlan::new(
                    rng,
                    Value {
                        amount: Amount::from(*amount),
                        asset_id,
                    },
                    base.address.clone(),
                );
                output
            })
            .collect::<Vec<_>>();

        let plan = NoteReshapePlan::new(
            family_id,
            spends.into_iter().map(Into::into).collect(),
            outputs.into_iter().map(Into::into).collect(),
            Fr::rand(rng),
            crate::NoteReshapeContext {
                witness: base.action_witness(),
                nonce: tx_blinding_nonce,
            },
            crate::discovery::Parameters::default(),
        )
        .expect("build note reshape plan");
        plan.note_reshape_public_private(&base.fvk, &state_commitment_proofs, anchor, 0)
            .expect("derive note reshape public/private inputs")
    }

    pub(crate) fn build_note_reshape_roundtrip_inputs(
        family_id: crate::NoteReshapeFamilyId,
    ) -> (
        crate::NoteReshapeProofPublic,
        crate::NoteReshapeProofPrivate,
    ) {
        let mut rng = rand::thread_rng();
        build_note_reshape_roundtrip_inputs_with_rng(&mut rng, family_id)
    }

    pub(crate) fn build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng_and_mode(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: ShieldedIcs20WithdrawalFamilyId,
        is_regulated: bool,
        real_spends: usize,
        accumulator_mode: WithdrawalAccumulatorTestMode,
    ) -> (
        crate::ShieldedIcs20WithdrawalProofPublic,
        crate::ShieldedIcs20WithdrawalProofPrivate,
    ) {
        assert!(
            (1..=2).contains(&real_spends),
            "withdrawal test helper supports one or two real spends"
        );
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
            crate::RecoveryCommitment::unavailable(),
        )
        .expect("create shielded ICS-20 withdrawal note a");
        let note_b = crate::Note::from_parts(
            base.address.clone(),
            Value {
                amount: Amount::from(50u64),
                asset_id: base.value.asset_id,
            },
            crate::Rseed::generate(rng),
            crate::RecoveryCommitment::unavailable(),
        )
        .expect("create shielded ICS-20 withdrawal note b");

        let spend_a = ShieldedInputPlan::new(rng, note_a.clone(), 0u64.into());
        let spend_b = ShieldedInputPlan::new(rng, note_b.clone(), 1u64.into());

        let day_start = crate::select_accumulator_day(base.target_timestamp);
        let accumulator_subject =
            crate::VolumeAccumulatorState::subject(&base.address, base.value.asset_id);
        let accumulator_prior_state = match accumulator_mode {
            WithdrawalAccumulatorTestMode::Continuation { prior_volume } => {
                Some(crate::VolumeAccumulatorState {
                    subject: accumulator_subject,
                    day_start,
                    undisclosed_volume: prior_volume,
                    blinding: Fq::rand(&mut *rng),
                })
            }
            _ => None,
        };
        let mut sct = tct::Tree::new();
        sct.insert(tct::Witness::Keep, note_a.commit())
            .expect("insert first withdrawal note");
        if real_spends == 2 {
            sct.insert(tct::Witness::Keep, note_b.commit())
                .expect("insert second withdrawal note");
        }
        if let Some(prior) = &accumulator_prior_state {
            sct.insert(tct::Witness::Keep, prior.commitment())
                .expect("insert prior withdrawal accumulator commitment");
        }
        let anchor = sct.root();
        let required_proof = sct.witness(note_a.commit()).expect("witness note a");
        let optional_proof =
            (real_spends == 2).then(|| sct.witness(note_b.commit()).expect("witness note b"));
        let accumulator_prior_proof = accumulator_prior_state.as_ref().map(|prior| {
            sct.witness(prior.commitment())
                .expect("witness prior withdrawal accumulator")
        });
        let (change_note, _) = crate::Note::from_parts_with_recovery(
            base.address.clone(),
            Value {
                amount: Amount::from(20u64),
                asset_id: base.value.asset_id,
            },
            crate::Rseed::generate(rng),
            base.user_leaf.capk,
        )
        .expect("create shielded ICS-20 withdrawal change note");

        let padder = HiddenArityPadder {
            value_blinding: Fr::from(13u64),
            first_spend_randomizer: spend_a.randomizer,
            sender_address: base.address.clone(),
            asset_id: base.value.asset_id,
            capk: base.user_leaf.capk,
            nullifier_domain_sep_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.nullifier",
            nullifier_seed_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.nullifier_seed",
            spend_auth_key_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.spend_auth_key",
            spend_auth_randomizer_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.spend_auth_randomizer",
            input_note_label: b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.input_note",
            output_note_label: b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.output_note",
        };
        let mut input_publics = vec![ShieldedIcs20WithdrawalInputPublic {
            nullifier: spend_a.nullifier(
                &base
                    .action_witness()
                    .nullifier_key(&base.fvk)
                    .expect("fixture nullifier key"),
            ),
            rk: spend_a.rk(&base.fvk),
            history_required: false,
        }];
        input_publics.push(if real_spends == 2 {
            ShieldedIcs20WithdrawalInputPublic {
                nullifier: spend_b.nullifier(
                    &base
                        .action_witness()
                        .nullifier_key(&base.fvk)
                        .expect("fixture nullifier key"),
                ),
                rk: spend_b.rk(&base.fvk),
                history_required: false,
            }
        } else {
            ShieldedIcs20WithdrawalInputPublic {
                nullifier: padder.synthetic_dummy_nullifier(1),
                rk: padder.synthetic_dummy_verification_key(1),
                history_required: false,
            }
        });
        let required_input = ShieldedIcs20WithdrawalRequiredInputPrivate {
            state_commitment_proof: required_proof,
            spent_note: note_a,
            spend_auth_randomizer: spend_a.randomizer,
        };
        let optional_input = if let Some(state_commitment_proof) = optional_proof {
            ShieldedIcs20WithdrawalOptionalInputPrivate {
                spend: ShieldedIcs20WithdrawalRequiredInputPrivate {
                    state_commitment_proof,
                    spent_note: note_b,
                    spend_auth_randomizer: spend_b.randomizer,
                },
                is_dummy: false,
                dummy_nullifier_seed: Fq::from(0u64),
            }
        } else {
            let dummy_note = padder.synthetic_dummy_input_note(1);
            ShieldedIcs20WithdrawalOptionalInputPrivate {
                spend: ShieldedIcs20WithdrawalRequiredInputPrivate {
                    state_commitment_proof: dummy_state_commitment_proof(dummy_note.commit()),
                    spent_note: dummy_note,
                    spend_auth_randomizer: padder.synthetic_dummy_spend_auth_randomizer(1),
                },
                is_dummy: true,
                dummy_nullifier_seed: padder.synthetic_dummy_nullifier_seed(1),
            }
        };

        let routing_parameters = crate::discovery::Parameters::default();
        let routing_nonce = Fq::from(29u64);
        let routing_tag = crate::discovery::single_tag(
            &base.address,
            is_regulated,
            &routing_parameters,
            routing_nonce,
        );
        let outbound_amount = if real_spends == 2 { 100u128 } else { 50u128 };
        let (withdrawal_key, _) = withdrawal_encryption_key(
            is_regulated,
            is_regulated && matches!(accumulator_mode, WithdrawalAccumulatorTestMode::Padding),
            &base.user_leaf,
            &base.asset_indexed_leaf,
        )
        .expect("select withdrawal compliance key");
        let withdrawal = encrypt_withdrawal_with_material(
            withdrawal_key,
            &base.address,
            Fq::from(31u64),
            Fr::from(37u64),
        )
        .expect("encrypt withdrawal sender address");
        let volume_plan = match accumulator_mode {
            WithdrawalAccumulatorTestMode::Padding => {
                crate::VolumeAccumulatorPlan::padding(base.target_timestamp)
            }
            WithdrawalAccumulatorTestMode::Origin => {
                crate::VolumeAccumulatorPlan::origin(crate::VolumeAccumulatorState {
                    subject: accumulator_subject,
                    day_start,
                    undisclosed_volume: outbound_amount,
                    blinding: Fq::rand(&mut *rng),
                })
            }
            WithdrawalAccumulatorTestMode::Continuation { prior_volume } => {
                let prior = accumulator_prior_state
                    .clone()
                    .expect("continuation has prior state");
                crate::VolumeAccumulatorPlan::continuation(
                    prior.clone(),
                    prior.commitment(),
                    accumulator_prior_proof
                        .as_ref()
                        .map(|proof| u64::from(proof.position()))
                        .expect("continuation has prior proof"),
                    prior_volume + outbound_amount,
                    Fq::rand(&mut *rng),
                )
                .expect("valid withdrawal continuation")
            }
        };
        let volume_payload = volume_plan.selected_payload(
            base.fvk.nullifier_key(),
            base.fvk.outgoing(),
            Fq::from_le_bytes_mod_order(&Fr::from(11u64).to_bytes()),
            crate::TransferProofContext::Ordinary,
        );

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
                    recovery_commitment: change_note.recovery_commitment(),
                },
                outbound_asset_id: base.value.asset_id.0,
                outbound_amount: Fq::from(outbound_amount),
                withdrawal_effect_hash_limbs: [
                    Fq::from(21u64),
                    Fq::from(22u64),
                    Fq::from(23u64),
                    Fq::from(24u64),
                ],
                routing_tag,
                routing_parameter_set_id: routing_parameters.id(),
                withdrawal_compliance_ciphertext: withdrawal.ciphertext,
                recent_position_floor: 0,
                volume_accumulator: crate::VolumeAccumulatorPublic {
                    nullifier: volume_payload.nullifier,
                    commitment: volume_payload.commitment,
                    day_start: volume_payload.day_start,
                },
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
                routing_parameters,
                routing_nonce,
                sender_compliance_path: base.compliance_path,
                sender_compliance_position: base.compliance_position,
                sender_leaf: base.user_leaf,
                withdrawal_seed: withdrawal.seed,
                withdrawal_randomizer: withdrawal.r,
                required_input,
                optional_input,
                change_output: ShieldedIcs20WithdrawalChangePrivate {
                    created_note: change_note,
                },
                volume_accumulator_seed: Fq::from_le_bytes_mod_order(&Fr::from(11u64).to_bytes()),
                volume_accumulator: crate::VolumeAccumulatorPrivate {
                    prior_proof: accumulator_prior_proof.unwrap_or_else(|| {
                        dummy_state_commitment_proof(volume_plan.prior_commitment())
                    }),
                    plan: volume_plan,
                },
            },
        )
    }

    pub(crate) fn build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng_and_real_spends(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: ShieldedIcs20WithdrawalFamilyId,
        is_regulated: bool,
        real_spends: usize,
    ) -> (
        crate::ShieldedIcs20WithdrawalProofPublic,
        crate::ShieldedIcs20WithdrawalProofPrivate,
    ) {
        build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng_and_mode(
            rng,
            family_id,
            is_regulated,
            real_spends,
            WithdrawalAccumulatorTestMode::Padding,
        )
    }

    pub(crate) fn build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng(
        rng: &mut (impl rand::RngCore + rand_core::CryptoRng),
        family_id: ShieldedIcs20WithdrawalFamilyId,
        is_regulated: bool,
    ) -> (
        crate::ShieldedIcs20WithdrawalProofPublic,
        crate::ShieldedIcs20WithdrawalProofPrivate,
    ) {
        build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng_and_real_spends(
            rng,
            family_id,
            is_regulated,
            2,
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
        let expected_pvk = shieldd_sdk_proof_params::transfer_proof_verification_key();
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
        shieldd_sdk_proof_params::batch::batch_verify(expected_pvk, std::slice::from_ref(&item))
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
