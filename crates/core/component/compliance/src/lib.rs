pub mod enrichment;
pub use enrichment::{BatchComplianceData, ComplianceProofProvider};

pub mod event;

pub mod issuer_keys;
pub use event::{EventAssetRegistered, EventComplianceAnchor, EventUserRegistered};
pub use issuer_keys::{
    DetectionKey, DetectionKeyPublic, MasterComplianceKey, MasterComplianceKeyPublic,
    DETECTION_TIER_BYTES, FLAG_SENTINEL,
};

pub mod structs;
pub use structs::{
    AssetParams,
    AssetPolicy,
    ComplianceCiphertext,
    ComplianceLeaf,
    CompliancePayload,
    DleqProof,
    MerklePath,
    MerklePathLayer,
    MsgRegisterAsset,
    MsgRegisterUser,
    RingData,
    ADDRESS_BYTES,
    // Wire format constants
    AMOUNT_BYTES,
    ASSET_ID_BYTES,
    C2_BYTES,
    DETECTION_TAG_BYTES,
    ENCRYPTED_TIER_BYTES,
    EPK_BYTES,
    FQ_BYTES,
    GENERATOR_BYTES,
    KEY_BYTES,
    TOTAL_PLAINTEXT_BYTES,
    TRANSFER_INPUT_CIPHERTEXT_FQS,
    TRANSFER_INPUT_DLEQ_BYTES,
    TRANSFER_INPUT_WIRE_BYTES,
};

pub mod transfer;
pub use transfer::{
    compute_transfer_dleqs, derive_transfer_salt, encrypt_transfer, TransferComplianceCiphertext,
    TransferComplianceDleqProofs, TransferCompliancePublicInputs, TransferEncryptionResult,
    TRANSFER_CIPHERTEXT_FQS, TRANSFER_CORE_CIPHERTEXT_FQS, TRANSFER_DETECTION_FQS,
    TRANSFER_DLEQ_BYTES, TRANSFER_EXT_CIPHERTEXT_FQS, TRANSFER_WIRE_BYTES,
};

pub mod tree;
pub use tree::{QuadTree, DEFAULT_DEPTH, ZERO_HASHES};

pub mod indexed_tree;
pub use indexed_tree::{
    recompute_root, IndexedLeaf, IndexedMerkleTree, IMT_LEAF_DOMAIN_SEP, IMT_ZERO_HASHES,
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
pub use r1cs::{
    compute_metadata_hash_r1cs, verify_compliance_integrity, verify_dleq_r1cs, verify_quad_path,
    verify_threshold_flag_simple, ComplianceWitness,
};

pub mod crypto;
pub use crypto::{
    compute_dleq_native, compute_metadata_hash, decrypt, decrypt_detection_tier,
    decrypt_tier_bytes, derive_compliance_scalar, encrypt_tier_bytes, fq_to_challenge_scalar,
    verify_dleq_native, DecryptedComplianceData, COMPLIANCE_STREAM_CIPHER_DOMAIN,
    DLEQ_CHALLENGE_DOMAIN, DLEQ_METADATA_DOMAIN, ENCRYPT_PROOF_DOMAIN, ISSUER_DETECTION_DOMAIN,
    UNREGULATED_SINK_DK_PUB, UNREGULATED_SINK_RING_PK,
};

pub mod scanning;
pub use scanning::{decrypt_full_flagged, AddressData, FullComplianceData};

// Scanner requires tokio and rusqlite for async storage
#[cfg(feature = "component")]
pub mod scanner;
#[cfg(feature = "component")]
pub use scanner::{
    detect_scan_transaction, detect_scan_transactions, ComplianceStorage, DetectedCiphertext,
    DetectedTransfer, IssuerComplianceWorker, PartialAddress, WorkerHandle,
};

pub mod ibc;
pub use ibc::IbcComplianceMetadata;

pub mod leaf_binding;
pub use leaf_binding::{
    blind_counterparty_leaf, blind_sender_leaf, DOMAIN_SEP_COUNTERPARTY, DOMAIN_SEP_SENDER,
};

pub mod orbis;
pub use orbis::{compute_adjusted_reader_pk, recover_seed, OrbisReencryptor, SimulatedOrbis};

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
pub fn default_user_proof(
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
    use penumbra_sdk_keys::keys::Diversifier;
    use penumbra_sdk_keys::Address;
    use rand_core::{OsRng, RngCore};

    use crate::indexed_tree::{IndexedLeaf, FQ_MAX};

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

    /// Create a test IndexedLeaf with default (unregulated) policy.
    pub fn make_test_leaf(value: u64) -> IndexedLeaf {
        IndexedLeaf::with_default_policy(Fq::from(value), 0, *FQ_MAX)
    }
}

// Integration tests require cnidarium, tokio, and scanner
#[cfg(all(test, feature = "component"))]
mod tests {
    use super::*;
    use cnidarium::{StateDelta, StateWrite, TempStorage};
    use decaf377::Fq;
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_keys::Address;
    use penumbra_sdk_proto::StateWriteProto;
    use penumbra_sdk_tct::StateCommitment;

    #[tokio::test]
    async fn test_compliance_path_generation() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rand::thread_rng()),
            asset_id: asset::Id(Fq::from(100u64)),
            d: Fq::from(0u64),
        };

        let user1_commit = leaf.commit();
        state.add_compliance_leaf(leaf.clone()).await.unwrap();

        let tree = state.get_user_tree().await.unwrap();
        let path = tree.auth_path(0).unwrap();

        assert!(!path.is_empty());
        assert_eq!(path.len(), DEFAULT_DEPTH as usize);

        // First layer siblings should be zero hashes (only one leaf inserted)
        let first_layer_siblings = path[0];
        let zero_hash_level_0 = ZERO_HASHES[0];
        assert_eq!(first_layer_siblings[0].0, zero_hash_level_0.0);
        assert_eq!(first_layer_siblings[1].0, zero_hash_level_0.0);
        assert_eq!(first_layer_siblings[2].0, zero_hash_level_0.0);

        // Verify path computation from leaf to root
        let mut current_hash = user1_commit;
        let mut current_position = 0u64;

        for (_level, siblings) in path.iter().enumerate() {
            let child_index = (current_position % 4) as usize;
            let children = match child_index {
                0 => [current_hash, siblings[0], siblings[1], siblings[2]],
                1 => [siblings[0], current_hash, siblings[1], siblings[2]],
                2 => [siblings[0], siblings[1], current_hash, siblings[2]],
                3 => [siblings[0], siblings[1], siblings[2], current_hash],
                _ => unreachable!(),
            };
            let parent_hash = poseidon377::hash_4(
                &Fq::from(0u64),
                (children[0].0, children[1].0, children[2].0, children[3].0),
            );
            current_hash = StateCommitment(parent_hash);
            current_position /= 4;
        }

        let tree_root = tree.root();
        assert_eq!(current_hash.0, tree_root.0);

        let verified = QuadTree::verify_auth_path(0, user1_commit, &path, tree_root, DEFAULT_DEPTH);
        assert!(verified);
    }

    #[tokio::test]
    async fn test_multiple_users_path() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        let mut rng = rand::thread_rng();
        let mut commitments = Vec::new();

        for i in 0..4u64 {
            let leaf = ComplianceLeaf {
                address: Address::dummy(&mut rng),
                asset_id: asset::Id(Fq::from(i)),
                d: Fq::from(0u64),
            };
            commitments.push(leaf.commit());
            state.add_compliance_leaf(leaf).await.unwrap();
        }

        let tree = state.get_user_tree().await.unwrap();
        let path = tree.auth_path(0).unwrap();

        let first_layer_siblings = path[0];
        assert_eq!(first_layer_siblings[0].0, commitments[1].0);
        assert_eq!(first_layer_siblings[1].0, commitments[2].0);
        assert_eq!(first_layer_siblings[2].0, commitments[3].0);

        let tree_root = tree.root();
        let verified =
            QuadTree::verify_auth_path(0, commitments[0], &path, tree_root, DEFAULT_DEPTH);
        assert!(verified);
    }

    #[tokio::test]
    async fn test_different_positions() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let user_tree = QuadTree::new();
        let tree_bytes = bincode::serialize(&user_tree).unwrap();
        state.put_raw(state_key::user_tree().to_string(), tree_bytes);
        state.put_proto(state_key::user_count().to_string(), 0u64);

        let mut rng = rand::thread_rng();
        let positions = vec![0, 5, 10];
        let mut leaves = Vec::new();

        for &pos in &positions {
            while state.get_user_count().await.unwrap() < pos {
                let dummy_leaf = ComplianceLeaf {
                    address: Address::dummy(&mut rng),
                    asset_id: asset::Id(Fq::from(0u64)),
                    d: Fq::from(0u64),
                };
                state.add_compliance_leaf(dummy_leaf).await.unwrap();
            }

            let leaf = ComplianceLeaf {
                address: Address::dummy(&mut rng),
                asset_id: asset::Id(Fq::from(pos)),
                d: Fq::from(0u64),
            };
            state.add_compliance_leaf(leaf.clone()).await.unwrap();
            leaves.push((pos, leaf.commit()));
        }

        let tree = state.get_user_tree().await.unwrap();
        let tree_root = tree.root();

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
    async fn test_transfer_compliance_path_generation() {
        use crate::crypto::derive_compliance_scalar;
        use crate::transfer::{encrypt_transfer, TRANSFER_WIRE_BYTES};
        use penumbra_sdk_asset::Value;
        use penumbra_sdk_num::Amount;
        use rand_core::OsRng;

        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let user_tree = QuadTree::new();
        state.put_raw(
            state_key::user_tree().to_string(),
            bincode::serialize(&user_tree).unwrap(),
        );
        state.put_proto(state_key::user_count().to_string(), 0u64);

        let asset_imt = indexed_tree::IndexedMerkleTree::new();
        state.put_raw(
            state_key::asset_imt().to_string(),
            bincode::serialize(&asset_imt).unwrap(),
        );

        let mut rng = rand::thread_rng();
        let asset_id = asset::Id(Fq::from(1000u64));
        let issuer_dk_pub = decaf377::Element::GENERATOR;
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);

        state
            .register_asset_in_imt(
                asset_id,
                AssetPolicy::simple(issuer_dk_pub, 1_000_000, ring_pk),
                true,
            )
            .await
            .unwrap();

        let sender_address = Address::dummy(&mut rng);
        let receiver_address = Address::dummy(&mut rng);
        let sender_leaf = ComplianceLeaf::new(
            sender_address.clone(),
            asset_id,
            derive_compliance_scalar(
                sender_address
                    .diversified_generator()
                    .vartime_compress_to_field(),
            ),
        );
        let receiver_leaf = ComplianceLeaf::new(
            receiver_address.clone(),
            asset_id,
            derive_compliance_scalar(
                receiver_address
                    .diversified_generator()
                    .vartime_compress_to_field(),
            ),
        );

        state
            .add_compliance_leaf(sender_leaf.clone())
            .await
            .unwrap();
        state
            .add_compliance_leaf(receiver_leaf.clone())
            .await
            .unwrap();

        let sender_position = state
            .get_user_leaf_position(&sender_address, asset_id)
            .await
            .unwrap()
            .unwrap();
        let receiver_position = state
            .get_user_leaf_position(&receiver_address, asset_id)
            .await
            .unwrap()
            .unwrap();

        let sender_auth_path = state.get_user_auth_path(sender_position).await.unwrap();
        let receiver_auth_path = state.get_user_auth_path(receiver_position).await.unwrap();
        let sender_ack = ring_pk * decaf377::Fr::from_le_bytes_mod_order(&sender_leaf.d.to_bytes());
        let receiver_ack =
            ring_pk * decaf377::Fr::from_le_bytes_mod_order(&receiver_leaf.d.to_bytes());

        let ciphertext = encrypt_transfer(
            &mut OsRng,
            &sender_ack,
            &receiver_ack,
            &issuer_dk_pub,
            &receiver_address,
            &sender_address,
            Value {
                amount: Amount::from(100u64),
                asset_id,
            },
            false,
            Fq::from(0u64),
        )
        .unwrap()
        .ciphertext;

        assert_eq!(ciphertext.to_bytes().len(), TRANSFER_WIRE_BYTES);
        assert_eq!(sender_auth_path.len(), DEFAULT_DEPTH as usize);
        assert_eq!(receiver_auth_path.len(), DEFAULT_DEPTH as usize);

        let tree = state.get_user_tree().await.unwrap();
        let tree_root = tree.root();
        assert!(QuadTree::verify_auth_path(
            sender_position,
            sender_leaf.commit(),
            &sender_auth_path,
            tree_root,
            DEFAULT_DEPTH
        ));
        assert!(QuadTree::verify_auth_path(
            receiver_position,
            receiver_leaf.commit(),
            &receiver_auth_path,
            tree_root,
            DEFAULT_DEPTH
        ));
    }

    #[tokio::test]
    async fn test_end_to_end_detection_and_decryption() {
        use crate::crypto::derive_compliance_scalar;
        use crate::issuer_keys::DetectionKey;
        use crate::transfer::encrypt_transfer;
        use penumbra_sdk_asset::Value;
        use penumbra_sdk_num::Amount;
        use penumbra_sdk_proto::core::component::shielded_pool::v1::{
            Transfer, TransferBody, TransferOutputBody,
        };
        use penumbra_sdk_proto::core::transaction::v1::{
            action::Action, Action as ActionProto, Transaction as ProtoTransaction, TransactionBody,
        };
        use rand_core::OsRng;

        let issuer_dk = DetectionKey::demo();
        let issuer_dk_pub = issuer_dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = test_helpers::make_address(1);
        let receiver_address = test_helpers::make_address(2);
        let asset_id = asset::Id(decaf377::Fq::from(999999u64));
        let amount = Amount::from(1_000_000u128);

        let sender_ack = ring_pk
            * decaf377::Fr::from_le_bytes_mod_order(
                &derive_compliance_scalar(
                    sender_address
                        .diversified_generator()
                        .vartime_compress_to_field(),
                )
                .to_bytes(),
            );
        let receiver_ack = ring_pk
            * decaf377::Fr::from_le_bytes_mod_order(
                &derive_compliance_scalar(
                    receiver_address
                        .diversified_generator()
                        .vartime_compress_to_field(),
                )
                .to_bytes(),
            );

        let ciphertext = encrypt_transfer(
            &mut OsRng,
            &sender_ack,
            &receiver_ack,
            &issuer_dk_pub,
            &receiver_address,
            &sender_address,
            Value { amount, asset_id },
            true,
            Fq::from(7u64),
        )
        .unwrap()
        .ciphertext;

        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![ActionProto {
                    action: Some(Action::Transfer(Transfer {
                        body: Some(TransferBody {
                            outputs: vec![TransferOutputBody {
                                compliance_ciphertext: ciphertext.to_bytes(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }),
                        ..Default::default()
                    })),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut detected_ciphertexts = Vec::new();
        let matches = detect_scan_transaction(&issuer_dk, asset_id, &tx, 100, 0, |d| {
            detected_ciphertexts.push(d.ciphertext);
            Ok(())
        })
        .unwrap();
        assert_eq!(matches, 1);

        let wrong_dk = DetectionKey::from_seed(&[99u8; 32]);
        let mut wrong_detected = 0;
        detect_scan_transaction(&wrong_dk, asset_id, &tx, 100, 0, |_| {
            wrong_detected += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(wrong_detected, 0);

        let decrypted = decrypt_full_flagged(issuer_dk.inner(), &detected_ciphertexts[0], asset_id)
            .unwrap()
            .expect("flagged transfer should decrypt");
        assert_eq!(decrypted.amount, amount);
        assert_eq!(
            decrypted.sender_address.transmission_key,
            sender_address.transmission_key().0
        );
        assert_eq!(
            decrypted.receiver_address.transmission_key,
            receiver_address.transmission_key().0
        );
    }
}
