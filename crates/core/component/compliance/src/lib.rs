pub mod enrichment;
pub use enrichment::{
    AssetProofData, BatchComplianceData, ComplianceProofProvider, ComplianceQuery, UserProofData,
};

pub mod event;

pub mod audit_log;
pub use audit_log::{
    audit_bytes_commitment, AuditEffect, AuditEffectRecord, AuditLogState, AuditSource,
    IbcOperation, WithdrawalKind, AUDIT_LOG_VERSION, MAX_AUDIT_RECORD_BYTES,
};
#[cfg(feature = "component")]
pub use audit_log::{AuditLogRead, AuditLogWrite};

pub mod issuer_keys;
pub use event::{
    EventAssetRegistered, EventComplianceAnchor, EventUserAssetStatusChanged, EventUserRegistered,
};
pub use issuer_keys::{
    DetectionKey, DetectionKeyPublic, MasterComplianceKey, MasterComplianceKeyPublic,
    DETECTION_TIER_BYTES,
};

pub mod structs;
pub use structs::{
    compliance_nullifier_key_commitment,
    derive_regulated_nullifier_key,
    effective_nullifier_key,
    AssetParams,
    AssetPolicy,
    ComplianceLeaf,
    IbcAssetOrigin,
    IbcRoute,
    MerklePath,
    MerklePathLayer,
    MsgRegisterAsset,
    MsgRegisterUser,
    RingData,
    UserAssetStatus,
    UserAssetStatusAction,
    ADDRESS_BYTES,
    // Wire format constants
    AMOUNT_BYTES,
    ASSET_ID_BYTES,
    C2_BYTES,
    DETECTION_TAG_BYTES,
    EPK_BYTES,
    FQ_BYTES,
    GENERATOR_BYTES,
    KEY_BYTES,
};

pub mod transfer;
pub use transfer::{
    derive_transfer_salt, encrypt_transfer, TransferComplianceCiphertext,
    TransferCompliancePublicInputs, TransferEncryptionResult, TRANSFER_CIPHERTEXT_FQS,
    TRANSFER_CORE_CIPHERTEXT_FQS, TRANSFER_DETECTION_FQS, TRANSFER_EXT_CIPHERTEXT_FQS,
    TRANSFER_WIRE_BYTES,
};

pub mod dleq_evidence;
pub use dleq_evidence::{fq_to_challenge_scalar, verify_dleq, DleqProof, IssuerDhEvidence};

pub mod withdrawal;
pub use withdrawal::{
    address_components, classify_withdrawal_with_issuer, derive_withdrawal_encryption_material,
    encrypt_withdrawal, encrypt_withdrawal_with_material, withdrawal_encryption_key,
    withdrawal_key_confirmation, WithdrawalComplianceCiphertext, WithdrawalEncryptionResult,
    WITHDRAWAL_ADDRESS_BYTES, WITHDRAWAL_ADDRESS_CIPHERTEXT_FQS, WITHDRAWAL_COMPLIANCE_WIRE_BYTES,
    WITHDRAWAL_KEY_CONFIRMATION_DOMAIN,
};

pub mod tree;
pub use tree::{QuadTree, DEFAULT_DEPTH, ZERO_HASHES};

pub mod indexed_tree;
pub use indexed_tree::{
    recompute_root, IndexedLeaf, IndexedMerkleTree, IMT_LEAF_DOMAIN_SEP, IMT_ZERO_HASHES,
};

pub mod state_key;

pub mod params;
pub use params::ComplianceParameters;

// Registry requires cnidarium for state access
#[cfg(feature = "component")]
pub mod registry;
#[cfg(feature = "component")]
pub use registry::{
    AssetGrantAdmission, ComplianceRegistryRead, ComplianceRegistryWrite, GenesisAssetAdmission,
    NoteSeizureLifecycle, UserGrantAdmission, UserLeafRecord,
};

#[cfg(feature = "component")]
pub mod action_check;
#[cfg(feature = "component")]
pub use action_check::RegulatedAssetCheck;

#[cfg(feature = "component")]
pub mod component;
#[cfg(feature = "component")]
pub use component::Compliance;

pub mod genesis;
pub use genesis::Content as GenesisContent;

pub mod crypto;
pub use crypto::{
    compliance_derivation, decrypt_detection_tier, decrypt_tier_bytes, derive_compliance_scalar,
    encrypt_tier_bytes, transfer_key_confirmation, COMPLIANCE_STREAM_CIPHER_DOMAIN,
    ISSUER_DETECTION_DOMAIN, TRANSFER_KEY_CONFIRMATION_DOMAIN, UNREGULATED_SINK_DK_PUB,
    UNREGULATED_SINK_RING_PK,
};

pub mod scanning;
pub use scanning::{
    decrypt_core_amount_if_key_matches, decrypt_flagged_withdrawal_sender, decrypt_full_flagged,
    AddressData, FullComplianceData, WithdrawalComplianceData,
};

pub mod refs;
pub use refs::{ActionRef, BlockRef, ComplianceRecordRef, OutputRef, TxRef};

pub mod evidence;
pub use evidence::{
    ComplianceEvidenceCiphertext, ComplianceEvidenceMetadata, ComplianceEvidenceObject,
    EvidenceObjectType, WithdrawalEvidencePublicData,
};

pub mod audit_validation;
pub use audit_validation::{validate_audit_evidence, AuditValidationInput, AuditValidationStatus};

pub mod audit_status;
pub use audit_status::{AuditStatus, DecryptedVia, FlowType};

pub mod audit_records;
pub use audit_records::{
    filter_subject_candidates, AuditDetectedRef, AuditRoutingSelector, AuditScanExport,
    AuditSubjectCandidate, AuditSubjectRegistration, AuditSubjectRole,
};

#[cfg(feature = "scanner")]
pub mod audit;
#[cfg(feature = "scanner")]
pub use audit::{
    decrypt_flagged_rows, export_detected_refs, export_ledger_rows, export_ledger_rows_json,
    export_scan_json, mark_row_audited, record_address_alias, record_evidence_failure,
    scanner_health_json, validate_and_save_evidence_object,
};

mod tx_id;
pub use tx_id::scanner_transaction_id_from_proto;

// Scanner requires tokio and rusqlite for async storage
#[cfg(feature = "scanner")]
pub mod scanner;
#[cfg(feature = "scanner")]
pub use scanner::{
    extract_clear_flows, extract_compliance_ciphertexts, AuditLedgerRow, AuditRowKey,
    BlockIdentityProvider, CandidateEvidence, ClearFlowEvent, ClearFlowKind, ComplianceScreener,
    DetectionEvent, ExtractedComplianceCiphertext, InvalidCiphertext, IssuerComplianceWorker,
    OutputOutcome, ScannedBlock, ScannedOutput, ScannerSource, ScannerStore, ScreeningResult,
    SqliteScannerStore, WorkerHandle, MAX_INVALID_CIPHERTEXTS_PER_BLOCK,
};

pub mod ibc;
pub use ibc::IbcComplianceMetadata;

pub mod decode_object;
pub use decode_object::{TransferComplianceMetadata, TRANSFER_COMPLIANCE_METADATA_BYTES};

#[cfg(feature = "poc-orbis")]
pub mod poc_orbis_audit;
#[cfg(feature = "poc-orbis")]
pub use poc_orbis_audit::{
    build_poc_orbis_audit_package, decrypt_reencrypted_seed, parse_element,
    validate_decrypted_seed, PocOrbisAccess, PocOrbisAuditBundle, PocOrbisAuditPackage,
    PocOrbisSecretEnvelope, PocOrbisStoredAuditPackage, PocOrbisTier, PocOrbisTierBundle,
};

/// Create valid IMT non-membership proof for an unregulated asset.
///
/// Returns (asset_anchor, indexed_leaf, merkle_path, position) that satisfy circuit constraints.
/// The asset is proven to be unregulated via non-membership (falls in a gap).
pub fn create_default_imt_proof(
    asset_id: decaf377::Fq,
) -> (
    shieldd_sdk_tct::StateCommitment,
    IndexedLeaf,
    MerklePath,
    u64,
) {
    let tree = IndexedMerkleTree::new();
    let (position, indexed_leaf, auth_path) = tree
        .non_membership_proof(asset_id)
        .expect("can generate non-membership proof for any asset");
    let merkle_path = MerklePath::from_auth_path(auth_path);
    let anchor = shieldd_sdk_tct::StateCommitment(tree.root().0);
    (anchor, indexed_leaf, merkle_path, position)
}

/// Create valid user tree (QuadTree) proof for a compliance leaf.
///
/// Returns (compliance_anchor, merkle_path, position) that satisfy circuit constraints.
pub fn default_user_proof(
    user_leaf: &ComplianceLeaf,
) -> (shieldd_sdk_tct::StateCommitment, MerklePath, u64) {
    let mut tree = QuadTree::new();
    let leaf_commitment = user_leaf.commit();
    let position = 0u64;
    tree.update(position, leaf_commitment)
        .expect("can insert leaf");
    let auth_path = tree
        .auth_path(position)
        .expect("can get auth path for inserted leaf");
    let merkle_path = MerklePath::from_auth_path(auth_path);
    let anchor = shieldd_sdk_tct::StateCommitment(tree.root().0);
    (anchor, merkle_path, position)
}

/// Test helpers for compliance tests. Re-exported for use in other crates' tests.
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use decaf377::{Fq, Fr};
    use rand_core::OsRng;
    use shieldd_sdk_keys::keys::Diversifier;
    use shieldd_sdk_keys::Address;

    use crate::indexed_tree::{IndexedLeaf, FQ_MAX};

    /// Create an address with a specific diversifier byte pattern.
    pub fn make_address(div_byte: u8) -> Address {
        let mut rng = OsRng;
        let diversifier = Diversifier([div_byte; 16]);
        let scalar = Fr::rand(&mut rng);
        let point = decaf377::Element::GENERATOR * scalar;
        let pk_d = decaf377_ka::Public(point.vartime_compress().0);
        Address::from_components(diversifier, pk_d).unwrap()
    }

    /// Create a test IndexedLeaf with default (unregulated) policy.
    pub fn make_test_leaf(value: u64) -> IndexedLeaf {
        IndexedLeaf::with_default_policy(Fq::from(value), 0, *FQ_MAX)
    }
}

// Integration tests require cnidarium, tokio, and scanner
#[cfg(all(test, feature = "scanner"))]
mod tests {
    use super::*;
    use crate::registry::ComplianceRegistryComponentWrite as _;
    use cnidarium::{StateDelta, TempStorage};
    use decaf377::Fq;
    use shieldd_sdk_asset::asset;
    use shieldd_sdk_keys::Address;
    use shieldd_sdk_tct::StateCommitment;

    #[tokio::test]
    async fn test_compliance_path_generation() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();

        let leaf = ComplianceLeaf::synthetic_unregulated(
            Address::dummy(&mut rand::thread_rng()),
            asset::Id(Fq::from(100u64)),
        );

        let user1_commit = leaf.commit();
        state
            .test_only_add_compliance_leaf(leaf.clone())
            .await
            .unwrap();

        let tree = state.reconstruct_user_tree().await.unwrap();
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
        state.initialize_trees().await.unwrap();

        let mut rng = rand::thread_rng();
        let mut commitments = Vec::new();

        for i in 0..4u64 {
            let leaf = ComplianceLeaf::synthetic_unregulated(
                Address::dummy(&mut rng),
                asset::Id(Fq::from(i + 1)),
            );
            commitments.push(leaf.commit());
            state.test_only_add_compliance_leaf(leaf).await.unwrap();
        }

        let tree = state.reconstruct_user_tree().await.unwrap();
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
        state.initialize_trees().await.unwrap();

        let mut rng = rand::thread_rng();
        let positions = vec![0, 5, 10];
        let mut leaves = Vec::new();
        for &pos in &positions {
            while state.get_user_count().await.unwrap() < pos {
                let dummy_leaf = ComplianceLeaf::synthetic_unregulated(
                    Address::dummy(&mut rng),
                    asset::Id(Fq::from(1u64)),
                );
                state
                    .test_only_add_compliance_leaf(dummy_leaf)
                    .await
                    .unwrap();
            }

            let leaf = ComplianceLeaf::synthetic_unregulated(
                Address::dummy(&mut rng),
                asset::Id(Fq::from(pos + 1)),
            );
            state
                .test_only_add_compliance_leaf(leaf.clone())
                .await
                .unwrap();
            leaves.push((pos, leaf.commit()));
        }

        let tree = state.reconstruct_user_tree().await.unwrap();
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
        use crate::transfer::{encrypt_transfer, TRANSFER_WIRE_BYTES};
        use rand_core::OsRng;
        use shieldd_sdk_asset::Value;
        use shieldd_sdk_num::Amount;

        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();

        let mut rng = rand::thread_rng();
        let asset_id = asset::Id(Fq::from(1000u64));
        let issuer_dk_pub = decaf377::Element::GENERATOR;
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);

        state
            .test_only_register_asset(
                asset_id,
                AssetPolicy::for_test(issuer_dk_pub, 1_000_000, ring_pk),
                true,
            )
            .await
            .unwrap();

        let sender_address = Address::dummy(&mut rng);
        let receiver_address = Address::dummy(&mut rng);
        let sender_leaf = ComplianceLeaf::registered_from_rnk(
            sender_address.clone(),
            asset_id,
            ring_pk,
            sender_address.diversified_generator() * decaf377::Fr::from(999u64),
            Fq::from(1u64),
        )
        .unwrap();
        let receiver_leaf = ComplianceLeaf::registered_from_rnk(
            receiver_address.clone(),
            asset_id,
            ring_pk,
            receiver_address.diversified_generator() * decaf377::Fr::from(999u64),
            Fq::from(2u64),
        )
        .unwrap();

        state
            .test_only_add_compliance_leaf(sender_leaf.clone())
            .await
            .unwrap();
        state
            .test_only_add_compliance_leaf(receiver_leaf.clone())
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
        let sender_ack = sender_leaf.capk;
        let receiver_ack = receiver_leaf.capk;

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
            Fq::from(1u64),
            Fq::from(2u64),
        )
        .unwrap()
        .ciphertext;
        assert_eq!(ciphertext.to_bytes().len(), TRANSFER_WIRE_BYTES);
        assert_eq!(sender_auth_path.len(), DEFAULT_DEPTH as usize);
        assert_eq!(receiver_auth_path.len(), DEFAULT_DEPTH as usize);

        let tree = state.reconstruct_user_tree().await.unwrap();
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
        use rand_core::OsRng;
        use shieldd_sdk_asset::Value;
        use shieldd_sdk_num::Amount;
        use shieldd_sdk_proto::core::component::shielded_pool::v1::{
            Transfer, TransferBody, TransferOutputBody,
        };
        use shieldd_sdk_proto::core::transaction::v1::{
            action::Action, Action as ActionProto, Transaction as ProtoTransaction, TransactionBody,
        };

        let issuer_dk = DetectionKey::demo();
        let issuer_dk_pub = issuer_dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = test_helpers::make_address(1);
        let receiver_address = test_helpers::make_address(2);
        let asset_id = asset::Id(decaf377::Fq::from(999999u64));
        let amount = Amount::from(1_000_000u128);

        let sender_ack = ring_pk
            * decaf377::Fr::from_le_bytes_mod_order(
                &derive_compliance_scalar(&sender_address).to_bytes(),
            );
        let receiver_ack = ring_pk
            * decaf377::Fr::from_le_bytes_mod_order(
                &derive_compliance_scalar(&receiver_address).to_bytes(),
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
            Fq::from(8u64),
            Fq::from(9u64),
        )
        .unwrap()
        .ciphertext;
        let metadata = TransferComplianceMetadata::from_identifiers(
            "ring",
            "policy",
            "resource",
            "permission",
            1,
            Fq::from(8u64),
            Fq::from(10u64),
            Fq::from(9u64),
            Fq::from(11u64),
        );

        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![ActionProto {
                    action: Some(Action::Transfer(Transfer {
                        body: Some(TransferBody {
                            routing: Some(
                                shieldd_sdk_proto::core::component::shielded_pool::v1::TransferRouting {
                                    tags: vec![
                                        shieldd_sdk_proto::core::component::shielded_pool::v1::RoutingTag { value: 11 },
                                        shieldd_sdk_proto::core::component::shielded_pool::v1::RoutingTag { value: 22 },
                                    ],
                                },
                            ),
                            outputs: vec![TransferOutputBody {
                                compliance_ciphertext: ciphertext.to_bytes(),
                                compliance_metadata: metadata.to_bytes().unwrap(),
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

        let tx_ref = scanner::TxRef {
            block: scanner::BlockRef {
                height: 100,
                block_hash: [1u8; 32],
                parent_hash: [0u8; 32],
                block_time_unix: None,
            },
            tx_index: 0,
            tx_hash: scanner_transaction_id_from_proto(&tx),
        };
        let extracted = scanner::extract_compliance_ciphertexts(&tx_ref, &tx);
        let screener = scanner::ComplianceScreener::new(issuer_dk.clone(), asset_id);
        let mut detected_ciphertexts = Vec::new();
        for extracted in extracted {
            if let scanner::ScreeningResult::Detected(d) = screener.screen(extracted) {
                detected_ciphertexts.push(d.ciphertext);
            }
        }
        assert_eq!(detected_ciphertexts.len(), 1);

        let wrong_dk = DetectionKey::from_seed(&[99u8; 32]);
        let wrong_screener = scanner::ComplianceScreener::new(wrong_dk, asset_id);
        let wrong_detected = scanner::extract_compliance_ciphertexts(&tx_ref, &tx)
            .into_iter()
            .filter(|extracted| {
                matches!(
                    wrong_screener.screen(extracted.clone()),
                    scanner::ScreeningResult::Detected(_)
                )
            })
            .count();
        assert_eq!(wrong_detected, 0);

        let scanner::types::ComplianceCiphertext::Transfer(ciphertext) = &detected_ciphertexts[0]
        else {
            panic!("transfer action must yield transfer compliance ciphertext");
        };
        let decrypted = decrypt_full_flagged(issuer_dk.inner(), ciphertext, &metadata, asset_id)
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
