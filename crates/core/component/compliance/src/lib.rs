pub mod enrichment;
pub use enrichment::{BatchComplianceData, ComplianceProofProvider};

pub mod event;

pub mod issuer_keys;
pub use event::{EventAssetRegistered, EventComplianceAnchor, EventUserRegistered};
pub use issuer_keys::{
    DetectionKey, DetectionKeyPublic, DetectionTierPlaintext, MasterComplianceKey,
    MasterComplianceKeyPublic, DETECTION_TIER_BYTES, FLAG_BIT_MASK,
};

pub mod structs;
pub use structs::{
    AssetPolicy,
    ComplianceCiphertext,
    ComplianceLeaf,
    CompliancePayload,
    MerklePath,
    MerklePathLayer,
    MsgRegisterAsset,
    MsgRegisterUser,
    // Wire format constants
    AMOUNT_BYTES,
    ASSET_ID_BYTES,
    CIPHERTEXT_PAYLOAD_BYTES,
    DETECTION_TAG_BYTES,
    ENCRYPTED_CORE_BYTES,
    ENCRYPTED_EXTENSION_BYTES,
    EPK_BYTES,
    EPK_G_BYTES,
    GENERATOR_BYTES,
    KEY_BYTES,
    NUM_CIPHERTEXT_FQS,
    TOTAL_PLAINTEXT_BYTES,
    TOTAL_WIRE_BYTES,
};

pub mod tree;
pub use tree::{QuadTree, DEFAULT_DEPTH, ZERO_HASHES};

pub mod indexed_tree;
pub use indexed_tree::{
    compute_imt_root_from_path, IndexedLeaf, IndexedMerkleTree, IMT_LEAF_DOMAIN_SEP,
    IMT_ZERO_HASHES,
};

pub mod state_key;

// Registry requires cnidarium for state access
#[cfg(feature = "component")]
pub mod registry;
#[cfg(feature = "component")]
pub use registry::{ComplianceRegistryRead, ComplianceRegistryWrite};

#[cfg(feature = "component")]
pub mod action_check;
#[cfg(feature = "component")]
pub use action_check::RegulatedAssetCheck;

#[cfg(feature = "component")]
pub mod component;
#[cfg(feature = "component")]
pub use component::{Compliance, RpcServer};

pub mod genesis;
pub use genesis::Content as GenesisContent;

pub mod r1cs;
pub use r1cs::{verify_compliance_integrity, verify_quad_path, ComplianceWitness};

pub mod crypto;
pub use crypto::{
    decrypt_compliance_details_with_dk, decrypt_detection_tier_with_dk,
    decrypt_with_shared_secrets, encrypt_compliance_details, DecryptedComplianceData,
    EncryptionResult, BLACK_HOLE_ACK, COMPLIANCE_STREAM_CIPHER_DOMAIN, ISSUER_DETECTION_DOMAIN,
};

pub mod scanning;
pub use scanning::{
    decrypt_core, decrypt_extension, decrypt_full, decrypt_with_role, CoreData, ExtensionData,
    FullComplianceData, ScannerRole,
};

// Scanner requires tokio and rusqlite for async storage
#[cfg(feature = "component")]
pub mod scanner;
#[cfg(feature = "component")]
pub use scanner::{
    decrypt_compliance, scan_transaction, scan_transaction_for_compliance,
    scan_transaction_for_compliance_with_daily_keys, scan_transactions,
    scan_transactions_for_compliance, ComplianceStorage, DecryptedUserData, DetectedCiphertext,
    DetectedTransfer, IssuerComplianceWorker, PartialAddress, WorkerHandle,
};

pub mod leaf_binding;
pub use leaf_binding::{
    blind_counterparty_leaf, blind_sender_leaf, DOMAIN_SEP_COUNTERPARTY, DOMAIN_SEP_SENDER,
};

/// Create valid IMT non-membership proof for an unregulated asset.
///
/// Returns (asset_anchor, indexed_leaf, merkle_path, position) that satisfy circuit constraints.
/// The asset is proven to be unregulated via non-membership (falls in a gap).
pub fn create_default_imt_proof(
    asset_id: decaf377::Fq,
) -> (
    penumbra_sdk_tct::StateCommitment,
    IndexedLeaf,
    MerklePath,
    u64,
) {
    let tree = IndexedMerkleTree::new();
    let (position, indexed_leaf, auth_path) = tree
        .non_membership_proof(asset_id)
        .expect("can generate non-membership proof for any asset");
    let merkle_path = MerklePath::from_auth_path(auth_path);
    let anchor = penumbra_sdk_tct::StateCommitment(tree.root().0);
    (anchor, indexed_leaf, merkle_path, position)
}

/// Create valid user tree (QuadTree) proof for a compliance leaf.
///
/// Returns (compliance_anchor, merkle_path, position) that satisfy circuit constraints.
pub fn create_default_user_tree_proof(
    user_leaf: &ComplianceLeaf,
) -> (penumbra_sdk_tct::StateCommitment, MerklePath, u64) {
    let mut tree = QuadTree::new();
    let leaf_commitment = user_leaf.commit();
    let position = 0u64;
    tree.update(position, leaf_commitment)
        .expect("can insert leaf");
    let auth_path = tree
        .auth_path(position)
        .expect("can get auth path for inserted leaf");
    let merkle_path = MerklePath::from_auth_path(auth_path);
    let anchor = penumbra_sdk_tct::StateCommitment(tree.root().0);
    (anchor, merkle_path, position)
}

/// Test helpers for compliance tests. Re-exported for use in other crates' tests.
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use decaf377::{Fq, Fr};
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_keys::keys::{AddressComplianceKey, Diversifier, UserComplianceKey};
    use penumbra_sdk_keys::Address;
    use penumbra_sdk_num::Amount;
    use rand_core::{OsRng, RngCore};

    use crate::crypto::encrypt_compliance_details;
    use crate::indexed_tree::{IndexedLeaf, FQ_MAX};
    use crate::structs::{AssetPolicy, ComplianceCiphertext};

    // Re-export for convenience
    pub use crate::crypto::{encrypt_compliance_details as encrypt_single, EncryptionResult};

    /// Create a random UserComplianceKey.
    pub fn make_uck() -> UserComplianceKey {
        UserComplianceKey::new(Fr::rand(&mut OsRng))
    }

    /// Create an address with a specific diversifier byte pattern.
    pub fn make_address(div_byte: u8) -> Address {
        let mut rng = OsRng;
        let diversifier = Diversifier([div_byte; 16]);
        let scalar = Fr::rand(&mut rng);
        let point = decaf377::Element::GENERATOR * scalar;
        let pk_d = decaf377_ka::Public(point.vartime_compress().0);
        let mut ck_bytes = [0u8; 32];
        rng.fill_bytes(&mut ck_bytes);
        let ck = decaf377_fmd::ClueKey(ck_bytes);
        Address::from_components(diversifier, pk_d, ck).unwrap()
    }

    /// Create an ACK and matching address from a UCK with a specific diversifier.
    pub fn make_wallet(uck: &UserComplianceKey, div_byte: u8) -> (AddressComplianceKey, Address) {
        let mut rng = OsRng;
        let diversifier = Diversifier([div_byte; 16]);
        let ack = uck.derive_address_key(&diversifier);
        let scalar = Fr::rand(&mut rng);
        let point = decaf377::Element::GENERATOR * scalar;
        let pk_d = decaf377_ka::Public(point.vartime_compress().0);
        let mut ck_bytes = [0u8; 32];
        rng.fill_bytes(&mut ck_bytes);
        let ck = decaf377_fmd::ClueKey(ck_bytes);
        let addr = Address::from_components(diversifier, pk_d, ck).unwrap();
        (ack, addr)
    }

    /// Create a test IndexedLeaf with the given detection key and threshold.
    pub fn make_test_leaf(dk_pub: decaf377::Element, threshold: u128) -> IndexedLeaf {
        IndexedLeaf {
            value: Fq::from(42u64), // dummy value
            next_index: 0,
            next_value: *FQ_MAX,
            policy: AssetPolicy::new(dk_pub, threshold),
        }
    }

    /// Create a test IndexedLeaf with no threshold (unregulated asset behavior).
    pub fn make_unregulated_leaf(dk_pub: decaf377::Element) -> IndexedLeaf {
        make_test_leaf(dk_pub, u128::MAX)
    }

    /// Encrypt compliance details for a sender/receiver pair, returning both ciphertexts.
    ///
    /// Calls `encrypt_compliance_details` twice - once for sender, once for receiver.
    pub fn encrypt_dual(
        uck: &UserComplianceKey,
        sender_div: u8,
        receiver_div: u8,
        date: u64,
        asset_id: u64,
        amount: u128,
        asset_leaf: &IndexedLeaf,
    ) -> (ComplianceCiphertext, ComplianceCiphertext) {
        let mut rng = OsRng;
        let (ack_s, addr_s) = make_wallet(uck, sender_div);
        let (ack_r, addr_r) = make_wallet(uck, receiver_div);

        // Encrypt for sender (sender's ciphertext, counterparty = receiver)
        let s_result = encrypt_compliance_details(
            &mut rng,
            &ack_s,
            &addr_s,
            date,
            asset::Id(decaf377::Fq::from(asset_id)),
            Amount::from(amount),
            &addr_r,
            asset_leaf,
        )
        .unwrap();

        // Encrypt for receiver (receiver's ciphertext, counterparty = sender)
        let r_result = encrypt_compliance_details(
            &mut rng,
            &ack_r,
            &addr_r,
            date,
            asset::Id(decaf377::Fq::from(asset_id)),
            Amount::from(amount),
            &addr_s,
            asset_leaf,
        )
        .unwrap();

        (s_result.ciphertext, r_result.ciphertext)
    }
}

// Integration tests require cnidarium, tokio, and scanner
#[cfg(all(test, feature = "component"))]
mod tests {
    use super::*;
    use cnidarium::{StateDelta, StateWrite, TempStorage};
    use decaf377::Fq;
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_keys::{keys::AddressComplianceKey, Address};
    use penumbra_sdk_proto::StateWriteProto;
    use penumbra_sdk_tct::StateCommitment;

    #[tokio::test]
    async fn test_compliance_path_generation() {
        // 1. Initialize storage
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Manually initialize trees (since we can't clone StateDelta for init_chain)
        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        // 2. Create and add a single ComplianceLeaf (User 1)
        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rand::thread_rng()),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(100u64)),
        };

        // Calculate the leaf commitment
        let user1_commit = leaf.commit();

        // Add the leaf to the tree
        state.add_compliance_leaf(leaf.clone()).await.unwrap();

        // 3. Verification

        // Retrieve the UserTree
        let tree = state.get_user_tree().await.unwrap();

        // Get the authentication path for User 1 (position 0)
        let path = tree.auth_path(0).unwrap();

        // Assert: path is not empty
        assert!(!path.is_empty(), "Authentication path should not be empty");
        assert_eq!(
            path.len(),
            DEFAULT_DEPTH as usize,
            "Path length should match tree depth"
        );

        // Assert: First layer siblings should be zero hashes
        // Since we only added User 1 at position 0, siblings at positions 1, 2, 3
        // in the first layer should all be zero hashes for level 0
        let first_layer_siblings = path[0];
        let zero_hash_level_0 = ZERO_HASHES[0];

        // All three siblings in the first layer should be zero hashes
        // because positions 1, 2, 3 are empty
        assert_eq!(
            first_layer_siblings[0].0, zero_hash_level_0.0,
            "Sibling 1 (pos 1) should be zero hash"
        );
        assert_eq!(
            first_layer_siblings[1].0, zero_hash_level_0.0,
            "Sibling 2 (pos 2) should be zero hash"
        );
        assert_eq!(
            first_layer_siblings[2].0, zero_hash_level_0.0,
            "Sibling 3 (pos 3) should be zero hash"
        );

        // 4. Manual Check: Verify path computation from leaf to root
        let mut current_hash = user1_commit;
        let mut current_position = 0u64;

        for (_level, siblings) in path.iter().enumerate() {
            // Determine which child (0-3) we are at this level
            let child_index = (current_position % 4) as usize;

            // Reconstruct the 4 children for this parent
            let children = match child_index {
                0 => [current_hash, siblings[0], siblings[1], siblings[2]],
                1 => [siblings[0], current_hash, siblings[1], siblings[2]],
                2 => [siblings[0], siblings[1], current_hash, siblings[2]],
                3 => [siblings[0], siblings[1], siblings[2], current_hash],
                _ => unreachable!(),
            };

            // Hash the 4 children to get the parent
            let parent_hash = poseidon377::hash_4(
                &Fq::from(0u64),
                (children[0].0, children[1].0, children[2].0, children[3].0),
            );
            current_hash = StateCommitment(parent_hash);

            // Move to parent position
            current_position /= 4;
        }

        // After traversing the entire path, we should arrive at the root
        let tree_root = tree.root();
        assert_eq!(
            current_hash.0, tree_root.0,
            "Computed root from path should match tree root"
        );

        // 5. Additional verification: Use the tree's built-in verification
        let verified = QuadTree::verify_auth_path(0, user1_commit, &path, tree_root, DEFAULT_DEPTH);
        assert!(verified, "Built-in path verification should succeed");
    }

    #[tokio::test]
    async fn test_multiple_users_path() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Manually initialize trees
        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        let mut rng = rand::thread_rng();
        let mut commitments = Vec::new();

        // Add 4 users (filling the first group of 4 siblings)
        for i in 0..4u64 {
            let leaf = ComplianceLeaf {
                address: Address::dummy(&mut rng),
                key: AddressComplianceKey::new(
                    decaf377::Element::GENERATOR * decaf377::Fr::from(i + 1),
                ),
                asset_id: asset::Id(Fq::from(i)),
            };

            commitments.push(leaf.commit());
            state.add_compliance_leaf(leaf).await.unwrap();
        }

        let tree = state.get_user_tree().await.unwrap();

        // Now verify the path for User 0
        let path = tree.auth_path(0).unwrap();

        // The first layer siblings should NOT all be zero hashes now
        // because we have users at positions 1, 2, 3
        let first_layer_siblings = path[0];
        let _zero_hash_level_0 = ZERO_HASHES[0];

        // Verify that siblings match the commitments we inserted
        assert_eq!(
            first_layer_siblings[0].0, commitments[1].0,
            "Sibling 0 should be User 1's commitment"
        );
        assert_eq!(
            first_layer_siblings[1].0, commitments[2].0,
            "Sibling 1 should be User 2's commitment"
        );
        assert_eq!(
            first_layer_siblings[2].0, commitments[3].0,
            "Sibling 2 should be User 3's commitment"
        );

        // Verify the path is valid
        let tree_root = tree.root();
        let verified =
            QuadTree::verify_auth_path(0, commitments[0], &path, tree_root, DEFAULT_DEPTH);
        assert!(
            verified,
            "Path verification should succeed with multiple users"
        );
    }

    #[tokio::test]
    async fn test_different_positions() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Manually initialize trees
        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        let mut rng = rand::thread_rng();

        // Add users at positions 0, 5, 10
        let positions = vec![0, 5, 10];
        let mut leaves = Vec::new();

        for &pos in &positions {
            // Add empty users to reach the position
            while state.get_user_count().await.unwrap() < pos {
                let dummy_leaf = ComplianceLeaf {
                    address: Address::dummy(&mut rng),
                    key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
                    asset_id: asset::Id(Fq::from(0u64)),
                };
                state.add_compliance_leaf(dummy_leaf).await.unwrap();
            }

            // Add the actual user
            let leaf = ComplianceLeaf {
                address: Address::dummy(&mut rng),
                key: AddressComplianceKey::new(
                    decaf377::Element::GENERATOR * decaf377::Fr::from(pos + 1),
                ),
                asset_id: asset::Id(Fq::from(pos)),
            };
            state.add_compliance_leaf(leaf.clone()).await.unwrap();
            leaves.push((pos, leaf.commit()));
        }

        let tree = state.get_user_tree().await.unwrap();
        let tree_root = tree.root();

        // Verify paths for each position
        for (pos, commitment) in leaves {
            let path = tree.auth_path(pos).unwrap();
            let verified =
                QuadTree::verify_auth_path(pos, commitment, &path, tree_root, DEFAULT_DEPTH);
            assert!(
                verified,
                "Path verification should succeed for position {}",
                pos
            );
        }
    }

    #[tokio::test]
    async fn test_end_to_end_three_phases() {
        use penumbra_sdk_keys::keys::UserComplianceKey;
        use penumbra_sdk_num::Amount;

        // ========== SETUP ==========
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Initialize user tree
        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        // Initialize asset IMT
        let asset_imt = indexed_tree::IndexedMerkleTree::new();
        let imt_bytes = bincode::serialize(&asset_imt).unwrap();
        state.put_raw(state_key::asset_imt().to_string(), imt_bytes);

        let mut rng = rand::thread_rng();

        // ========== PHASE 1: Asset Registry Integration ==========

        // Create a regulated asset (e.g., USDC)
        let usdc_asset_id = asset::Id(Fq::from(1000u64));

        // Register asset as regulated (only regulated assets go in IMT)
        // Requires detection key (issuer's public key for scanning)
        let issuer_dk_pub = decaf377::Element::GENERATOR;
        state
            .register_regulated_asset(usdc_asset_id, issuer_dk_pub)
            .await
            .unwrap();

        // Verify regulation status via IMT proof
        let proof_data = state.get_asset_proof_data(usdc_asset_id).await.unwrap();
        assert!(
            proof_data.is_regulated,
            "USDC should be registered as regulated"
        );

        // ========== PHASE 2: Wallet ACK Integration ==========

        // Create sender and receiver addresses
        let sender_address = Address::dummy(&mut rng);
        let recipient_address = Address::dummy(&mut rng);

        // Use demo UCK to derive compliance leaves (Phase 2)
        let demo_uck = UserComplianceKey::demo();
        let sender_leaf = ComplianceLeaf::new(&demo_uck, sender_address.clone(), usdc_asset_id);
        let recipient_leaf =
            ComplianceLeaf::new(&demo_uck, recipient_address.clone(), usdc_asset_id);

        // Register sender and recipient in registry
        state
            .add_compliance_leaf(sender_leaf.clone())
            .await
            .unwrap();
        state
            .add_compliance_leaf(recipient_leaf.clone())
            .await
            .unwrap();

        // Verify sender position (should be 0)
        let sender_position = state
            .get_user_leaf_position(&sender_address, usdc_asset_id)
            .await
            .unwrap()
            .expect("Sender should have position");
        assert_eq!(sender_position, 0, "Sender should be at position 0");

        // Verify recipient position (should be 1)
        let recipient_position = state
            .get_user_leaf_position(&recipient_address, usdc_asset_id)
            .await
            .unwrap()
            .expect("Recipient should have position");
        assert_eq!(recipient_position, 1, "Recipient should be at position 1");

        // ========== PHASE 3A: Merkle Path Generation ==========

        // Get merkle paths for both users
        let sender_auth_path = state.get_user_auth_path(sender_position).await.unwrap();
        let recipient_auth_path = state.get_user_auth_path(recipient_position).await.unwrap();

        // Verify auth paths have correct depth
        assert_eq!(
            sender_auth_path.len(),
            DEFAULT_DEPTH as usize,
            "Sender auth path should have correct depth"
        );
        assert_eq!(
            recipient_auth_path.len(),
            DEFAULT_DEPTH as usize,
            "Recipient auth path should have correct depth"
        );

        // ========== PHASE 3B: MerklePath Conversion ==========

        // Convert auth paths to MerklePath format (for ZK circuits)
        let sender_merkle_path = MerklePath::from_auth_path(sender_auth_path.clone());
        let recipient_merkle_path = MerklePath::from_auth_path(recipient_auth_path.clone());

        // Verify conversion worked
        assert_eq!(
            sender_merkle_path.layers.len(),
            DEFAULT_DEPTH as usize,
            "Sender MerklePath should have correct number of layers"
        );
        assert_eq!(
            recipient_merkle_path.layers.len(),
            DEFAULT_DEPTH as usize,
            "Recipient MerklePath should have correct number of layers"
        );

        // Verify each layer has 3 siblings (arity 4 tree)
        for layer in &sender_merkle_path.layers {
            assert_eq!(
                layer.siblings.len(),
                3,
                "Each layer should have 3 siblings in arity-4 tree"
            );
        }

        // ========== PHASE 3C: Transaction Encryption Simulation ==========

        // Simulate transaction parameters
        let amount = Amount::from(100u64);
        let date = 20250101u64; // Example date for key derivation

        // Create asset leaf for encryption
        let asset_leaf = test_helpers::make_test_leaf(issuer_dk_pub, u128::MAX);

        // Encrypt compliance details for sender (counterparty = receiver)
        let sender_result = encrypt_compliance_details(
            &mut rng,
            &sender_leaf.key,
            &sender_address,
            date,
            usdc_asset_id,
            amount,
            &recipient_address,
            &asset_leaf,
        )
        .unwrap();

        // Encrypt compliance details for receiver (counterparty = sender)
        let receiver_result = encrypt_compliance_details(
            &mut rng,
            &recipient_leaf.key,
            &recipient_address,
            date,
            usdc_asset_id,
            amount,
            &sender_address,
            &asset_leaf,
        )
        .unwrap();

        let sender_ciphertext = sender_result.ciphertext;
        let sender_ephemeral = sender_result.ephemeral_secret;
        let receiver_ciphertext = receiver_result.ciphertext;
        let receiver_ephemeral = receiver_result.ephemeral_secret;

        // Verify ciphertexts were generated
        assert_eq!(
            sender_ciphertext.to_bytes().len(),
            TOTAL_WIRE_BYTES,
            "Sender ciphertext should have correct wire format size"
        );
        assert_eq!(
            receiver_ciphertext.to_bytes().len(),
            TOTAL_WIRE_BYTES,
            "Receiver ciphertext should have correct wire format size"
        );

        // ========== VERIFICATION: Full Workflow ==========

        // 1. Verify asset is regulated
        let final_proof = state.get_asset_proof_data(usdc_asset_id).await.unwrap();
        assert!(final_proof.is_regulated, "Asset should remain regulated");

        // 2. Verify both users are registered (via position lookup)
        assert!(state
            .get_user_leaf_position(&sender_address, usdc_asset_id)
            .await
            .unwrap()
            .is_some());
        assert!(state
            .get_user_leaf_position(&recipient_address, usdc_asset_id)
            .await
            .unwrap()
            .is_some());

        // 3. Verify merkle paths are valid
        let tree = state.get_user_tree().await.unwrap();
        let tree_root = tree.root();

        let sender_commit = sender_leaf.commit();
        let recipient_commit = recipient_leaf.commit();

        assert!(
            QuadTree::verify_auth_path(
                sender_position,
                sender_commit,
                &sender_auth_path,
                tree_root,
                DEFAULT_DEPTH
            ),
            "Sender merkle path should verify"
        );

        assert!(
            QuadTree::verify_auth_path(
                recipient_position,
                recipient_commit,
                &recipient_auth_path,
                tree_root,
                DEFAULT_DEPTH
            ),
            "Recipient merkle path should verify"
        );

        // Ephemeral secrets are different (randomized encryption)
        assert_ne!(sender_ephemeral, receiver_ephemeral);
    }

    /// Tests detection and decryption with issuer-only detection.
    /// Issuer's DetectionKey detects; users decrypt with UCK.
    #[tokio::test]
    async fn test_end_to_end_detection_and_decryption() {
        use crate::crypto::encrypt_compliance_details;
        use crate::issuer_keys::DetectionKey;
        use penumbra_sdk_keys::keys::{Diversifier, UserComplianceKey};
        use penumbra_sdk_num::Amount;
        use penumbra_sdk_proto::core::component::shielded_pool::v1::{Output, OutputBody};
        use penumbra_sdk_proto::core::transaction::v1::{
            action::Action, Action as ActionProto, Transaction as ProtoTransaction, TransactionBody,
        };
        use rand_core::{OsRng, RngCore};

        let mut rng = OsRng;

        // Helper to create address from diversifier
        fn make_address(rng: &mut OsRng, div: [u8; 16]) -> Address {
            let scalar = decaf377::Fr::rand(rng);
            let point = decaf377::Element::GENERATOR * scalar;
            let pk_d = decaf377_ka::Public(point.vartime_compress().0);
            let mut ck_bytes = [0u8; 32];
            rng.fill_bytes(&mut ck_bytes);
            let ck = decaf377_fmd::ClueKey(ck_bytes);
            Address::from_components(Diversifier(div), pk_d, ck).unwrap()
        }

        // Create Alice with UCK
        let alice_uck = UserComplianceKey::new(decaf377::Fr::rand(&mut rng));
        let alice_diversifier = Diversifier([1u8; 16]);
        let alice_ack = alice_uck.derive_address_key(&alice_diversifier);
        let alice_address = make_address(&mut rng, [1u8; 16]);

        let bob_uck = UserComplianceKey::new(decaf377::Fr::rand(&mut rng));
        let bob_address = make_address(&mut rng, [2u8; 16]);

        // Setup issuer's DetectionKey
        let issuer_dk = DetectionKey::demo();
        let issuer_dk_pub = issuer_dk.public_key();

        let usdc_asset_id = asset::Id(decaf377::Fq::from(999999u64));
        let amount = Amount::from(1000u128);
        let date = penumbra_sdk_keys::keys::day_index(1700000000);

        // Create asset leaf for encryption
        let asset_leaf = test_helpers::make_test_leaf(issuer_dk_pub, u128::MAX);

        // Encrypt with issuer's DK_pub (detection is issuer-only)
        let encrypt_result = encrypt_compliance_details(
            &mut rng,
            &alice_ack,
            &alice_address,
            date,
            usdc_asset_id,
            amount,
            &bob_address,
            &asset_leaf,
        )
        .unwrap();

        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![ActionProto {
                    action: Some(Action::Output(Output {
                        body: Some(OutputBody {
                            compliance_ciphertext: encrypt_result.ciphertext.to_bytes(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        // Issuer's DetectionKey can detect the transaction
        let mut detected_ciphertexts = Vec::new();
        let matches = scan_transaction(&issuer_dk, Some(usdc_asset_id), &tx, 100, 0, |d| {
            detected_ciphertexts.push(d.ciphertext);
            Ok(())
        })
        .unwrap();
        assert_eq!(matches, 1);
        assert_eq!(detected_ciphertexts.len(), 1);

        // Wrong issuer DK cannot detect (different DK produces garbage asset_id)
        let wrong_dk = DetectionKey::from_seed(&[99u8; 32]);
        let mut wrong_detected = 0;
        scan_transaction(&wrong_dk, Some(usdc_asset_id), &tx, 100, 0, |_| {
            wrong_detected += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(wrong_detected, 0, "wrong DK should not detect the asset");

        // Alice's UCK can decrypt core+extension (user data)
        let decrypted = decrypt_compliance(&alice_uck, date, &detected_ciphertexts[0]).unwrap();
        // Note: DecryptedUserData has nested core/extension, asset_id is issuer-only
        assert_eq!(decrypted.core.amount, amount);
        assert_eq!(
            decrypted.core.self_diversified_generator,
            *alice_address.diversified_generator()
        );
        assert_eq!(
            decrypted.core.self_transmission_key,
            alice_address.transmission_key().0
        );
        assert_eq!(
            decrypted.extension.counterparty_diversified_generator,
            *bob_address.diversified_generator()
        );
        assert_eq!(
            decrypted.extension.counterparty_transmission_key,
            bob_address.transmission_key().0
        );

        // Bob's UCK produces garbage (wrong keys)
        if let Ok(wrong_data) = decrypt_compliance(&bob_uck, date, &detected_ciphertexts[0]) {
            // Wrong keys produce garbage amount
            assert_ne!(wrong_data.core.amount, amount);
        }
    }
}
