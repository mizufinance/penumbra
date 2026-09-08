use super::*;
use crate::params::{ComplianceParameters, StateWriteExt as _};
use crate::tree::QuadTree;
use cnidarium::TempStorage;
use decaf377::{Fq, Fr};
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use futures::StreamExt;
use shieldd_sdk_keys::Address;
use shieldd_sdk_sct::component::clock::EpochManager;
use std::collections::BTreeMap;

#[tokio::test]
async fn missing_committed_roots_are_not_reconstructed() {
    let storage = TempStorage::new().await.unwrap();
    let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
    state.initialize_trees().await.unwrap();
    let leaf = ComplianceLeaf::registered_for_test(
        Address::dummy(&mut rand::thread_rng()),
        asset::Id(Fq::from(1u64)),
    );
    state.add_compliance_leaf(leaf).await.unwrap();
    state.ensure_asset_tree_initialized().await.unwrap();
    for key in [state_key::user_tree_root(), state_key::asset_imt_root()] {
        let mut corrupted = cnidarium::StateDelta::new(&state);
        corrupted.delete(key.to_string());
        assert!(corrupted.verify_committed_tree_roots().await.is_err());
    }
}

async fn nv_count(
    state: &cnidarium::StateDelta<cnidarium::Snapshot>,
    prefix: &'static str,
) -> usize {
    let stream = state
        .nonverifiable_range_raw(Some(prefix.as_bytes()), Vec::new()..)
        .unwrap();
    futures::pin_mut!(stream);
    let mut count = 0usize;
    while let Some(entry) = stream.next().await {
        entry.unwrap();
        count += 1;
    }
    count
}

async fn delete_nv_prefix(
    state: &mut cnidarium::StateDelta<cnidarium::Snapshot>,
    prefix: &'static str,
) {
    let mut keys = Vec::new();
    {
        let stream = state
            .nonverifiable_range_raw(Some(prefix.as_bytes()), Vec::new()..)
            .unwrap();
        futures::pin_mut!(stream);
        while let Some(entry) = stream.next().await {
            let (key, _) = entry.unwrap();
            keys.push(key);
        }
    }
    for key in keys {
        state.nonverifiable_delete(key);
    }
}

const TEST_ANCHOR_WINDOW_BLOCKS: u64 = 100;

fn put_test_compliance_params<S: cnidarium::StateWrite>(state: &mut S) {
    state.put_compliance_params(ComplianceParameters {
        anchor_validation_window_blocks: TEST_ANCHOR_WINDOW_BLOCKS,
    });
}

#[test]
fn base_fee_asset_cannot_be_admitted_as_regulated() {
    assert!(ensure_regulated_asset_id(*shieldd_sdk_asset::BASE_ASSET_ID, true).is_err());
    assert!(ensure_regulated_asset_id(*shieldd_sdk_asset::BASE_ASSET_ID, false).is_ok());
    assert!(ensure_regulated_asset_id(asset::Id(Fq::from(9u64)), true).is_ok());
}

#[test]
fn compliance_anchor_facts_require_current_user_root() {
    let user_anchor = StateCommitment(Fq::from(2u64));
    let current_user_anchor = StateCommitment(Fq::from(3u64));
    let asset_anchor = StateCommitment(Fq::from(1u64));
    let error = validate_compliance_anchor_facts(
        &user_anchor,
        &current_user_anchor,
        &asset_anchor,
        &asset_anchor,
    )
    .expect_err("a stale user root cannot be live");
    assert!(
        error.to_string().contains("current user compliance root"),
        "unexpected error: {error:#}"
    );
}

#[tokio::test]
async fn freeze_and_unfreeze_replace_the_leaf_at_its_existing_position() {
    let storage = TempStorage::new().await.unwrap();
    let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
    state.initialize_trees().await.unwrap();
    let address = Address::dummy(&mut rand::thread_rng());
    let asset_id = asset::Id(Fq::from(91u64));
    state
        .test_only_register_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
            true,
        )
        .await
        .unwrap();
    let position = state
        .test_only_add_compliance_leaf(ComplianceLeaf::registered_for_test(
            address.clone(),
            asset_id,
        ))
        .await
        .unwrap();
    let active_root = state.get_user_tree_root().await.unwrap();

    let frozen = state
        .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Freeze, 40)
        .await
        .unwrap();
    assert_eq!(frozen.position, position);
    assert_eq!(frozen.previous_status, UserAssetStatus::Active);
    assert_eq!(frozen.leaf.status, UserAssetStatus::Frozen);
    assert_eq!(frozen.leaf.freeze_generation, 1);
    assert_eq!(frozen.leaf.frozen_since_height, 40);
    assert_ne!(state.get_user_tree_root().await.unwrap(), active_root);
    assert_eq!(
        state
            .get_user_leaf(&address, asset_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        UserAssetStatus::Frozen
    );

    state
        .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Freeze, 41)
        .await
        .expect_err("freeze cannot be applied twice");
    let active = state
        .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Unfreeze, 42)
        .await
        .unwrap();
    assert_eq!(active.position, position);
    assert_eq!(active.previous_status, UserAssetStatus::Frozen);
    assert_eq!(active.leaf.status, UserAssetStatus::Active);
    assert_ne!(state.get_user_tree_root().await.unwrap(), active_root);

    let refrozen = state
        .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Freeze, 50)
        .await
        .unwrap();
    assert_eq!(refrozen.leaf.freeze_generation, 2);
    assert_eq!(refrozen.leaf.frozen_since_height, 50);
    assert_ne!(refrozen.commitment, frozen.commitment);
}

#[tokio::test]
async fn note_seizure_is_terminal_but_allows_more_notes_from_the_same_freeze() {
    let storage = TempStorage::new().await.unwrap();
    let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
    state.initialize_trees().await.unwrap();
    let address = Address::dummy(&mut rand::thread_rng());
    let asset_id = asset::Id(Fq::from(92u64));
    state
        .test_only_register_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
            true,
        )
        .await
        .unwrap();
    state
        .test_only_add_compliance_leaf(ComplianceLeaf::registered_for_test(
            address.clone(),
            asset_id,
        ))
        .await
        .unwrap();

    state
        .admit_note_seizure(&address, asset_id, 1, 40)
        .await
        .expect_err("active leaf cannot admit seizure");
    state
        .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Freeze, 40)
        .await
        .unwrap();
    state
        .admit_note_seizure(&address, asset_id, 2, 40)
        .await
        .expect_err("wrong generation cannot admit seizure");

    let first = state
        .admit_note_seizure(&address, asset_id, 1, 40)
        .await
        .unwrap();
    assert_eq!(first.previous_status, UserAssetStatus::Frozen);
    assert_eq!(first.leaf.status, UserAssetStatus::Seized);
    assert!(first.status_change.is_some());
    let seized_root = state.get_user_tree_root().await.unwrap();

    let next = state
        .admit_note_seizure(&address, asset_id, 1, 40)
        .await
        .unwrap();
    assert_eq!(next.previous_status, UserAssetStatus::Seized);
    assert_eq!(next.leaf.status, UserAssetStatus::Seized);
    assert!(next.status_change.is_none());
    assert_eq!(state.get_user_tree_root().await.unwrap(), seized_root);
    state
        .apply_user_status_action(&address, asset_id, UserAssetStatusAction::Unfreeze, 41)
        .await
        .expect_err("seized leaf cannot be unfrozen");
}

#[tokio::test]
async fn add_compliance_leaf_rejects_identity_capability_before_mutation() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut leaf = ComplianceLeaf::registered_for_test(
        Address::dummy(&mut rand::thread_rng()),
        asset::Id(Fq::from(1u64)),
    );
    leaf.capk = decaf377::Element::IDENTITY;

    let err = state
        .add_compliance_leaf(leaf)
        .await
        .expect_err("durable state must reject an identity capability");

    assert!(
        err.to_string().contains("capk must be nonidentity"),
        "unexpected error: {err:#}"
    );
    assert_eq!(state.get_user_count().await.unwrap(), 0);
    assert_eq!(
        nv_count(&state, state_key::tree_storage::user_node_prefix()).await,
        0
    );
}

#[tokio::test]
async fn add_compliance_leaf_rejects_zero_asset_before_mutation() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let leaf = ComplianceLeaf::registered_for_test(
        Address::dummy(&mut rand::thread_rng()),
        asset::Id(Fq::from(0u64)),
    );

    let err = state
        .add_compliance_leaf(leaf)
        .await
        .expect_err("the IMT sentinel value must not enter the user registry");

    assert!(
        err.to_string().contains("asset ID zero is reserved"),
        "unexpected error: {err:#}"
    );
    assert_eq!(state.get_user_count().await.unwrap(), 0);
    assert_eq!(
        nv_count(&state, state_key::tree_storage::user_node_prefix()).await,
        0
    );
}

#[tokio::test]
async fn test_user_tree_full_returns_domain_error_without_mutation() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let capacity = QuadTree::max_leaves_for_depth(crate::tree::DEFAULT_DEPTH);
    state.put_proto(state_key::user_count().to_string(), capacity);

    let leaf = ComplianceLeaf::registered_for_test(
        Address::dummy(&mut rand::thread_rng()),
        asset::Id(Fq::from(1u64)),
    );
    let err = state
        .add_compliance_leaf(leaf)
        .await
        .expect_err("full user tree should fail before mutation");

    assert!(
        err.to_string().contains("compliance user tree is full"),
        "unexpected error: {err:#}"
    );
    assert_eq!(state.get_user_count().await.unwrap(), capacity);
    assert_eq!(
        nv_count(&state, state_key::tree_storage::user_node_prefix()).await,
        0
    );
}

#[tokio::test]
async fn test_user_tree_uses_nv_nodes_not_full_blob() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let leaf = ComplianceLeaf::registered_for_test(
        Address::dummy(&mut rand::thread_rng()),
        asset::Id(Fq::from(1u64)),
    );
    state.add_compliance_leaf(leaf).await.unwrap();
    let root = state.get_user_tree_root().await.unwrap();

    assert!(
        nv_count(&state, state_key::tree_storage::user_node_prefix()).await
            <= crate::tree::DEFAULT_DEPTH as usize + 1
    );

    let reloaded = state.reconstruct_user_tree().await.unwrap();
    assert_eq!(reloaded.root(), root);
}

#[tokio::test]
async fn test_user_tree_root_check_fails_on_missing_nv_nodes() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let leaf = ComplianceLeaf::registered_for_test(
        Address::dummy(&mut rand::thread_rng()),
        asset::Id(Fq::from(1u64)),
    );
    state.add_compliance_leaf(leaf).await.unwrap();
    state.verify_committed_tree_roots().await.unwrap();

    delete_nv_prefix(&mut state, state_key::tree_storage::user_node_prefix()).await;

    let err = state
        .verify_committed_tree_roots()
        .await
        .expect_err("missing user-tree NV nodes should fail root verification");
    assert!(err.to_string().contains("user tree root mismatch"));
}

#[tokio::test]
async fn test_register_regulated_asset() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let asset_id = asset::Id(Fq::from(123u64));

    // Initially asset is unregulated (not in IMT)
    let proof_before = state.get_asset_proof_data(asset_id).await.unwrap();
    assert!(!proof_before.is_regulated);

    // Register as regulated
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();

    // Now asset is regulated (in IMT)
    let proof_after = state.get_asset_proof_data(asset_id).await.unwrap();
    assert!(proof_after.is_regulated);
    assert_eq!(proof_after.indexed_leaf.value, asset_id.0);
    let tree = state.reconstruct_asset_tree().await.unwrap();
    assert!(tree.contains(asset_id.0));
    assert_eq!(tree.leaf_count(), 2);

    // IMT root should have changed
    let empty_imt = IndexedMerkleTree::new();
    let root = state.get_asset_imt_root().await.unwrap();
    assert_ne!(root.0, empty_imt.root().0);
}

#[tokio::test]
async fn test_asset_imt_uses_nv_nodes_and_leaves_not_full_blob() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let asset_id = asset::Id(Fq::from(777u64));
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();
    let root = state.get_asset_imt_root().await.unwrap();
    let proof_before = state.get_asset_proof_data(asset_id).await.unwrap();

    assert!(
        nv_count(&state, state_key::tree_storage::asset_node_prefix()).await
            <= ((crate::tree::DEFAULT_DEPTH as usize + 1) * 2)
    );
    assert_eq!(
        nv_count(&state, state_key::tree_storage::asset_leaf_prefix()).await,
        2
    );
    assert_eq!(
        nv_count(&state, state_key::tree_storage::asset_value_desc_prefix()).await,
        2
    );

    let reloaded = state.reconstruct_asset_tree().await.unwrap();
    assert_eq!(reloaded.root(), root);
    let proof_after = state.get_asset_proof_data(asset_id).await.unwrap();
    assert_eq!(
        proof_after.indexed_leaf.value,
        proof_before.indexed_leaf.value
    );
    assert_eq!(proof_after.position, proof_before.position);
}

#[tokio::test]
async fn test_asset_imt_root_check_fails_on_missing_nv_leaves() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    state
        .register_regulated_asset(
            asset::Id(Fq::from(777u64)),
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();
    state.verify_committed_tree_roots().await.unwrap();

    delete_nv_prefix(&mut state, state_key::tree_storage::asset_leaf_prefix()).await;

    let err = state
        .verify_committed_tree_roots()
        .await
        .expect_err("missing asset-IMT NV leaves should fail root verification");
    assert!(err.to_string().contains("asset IMT"));
}

#[tokio::test]
async fn full_asset_structure_validation_is_confined_to_readiness() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        u128::MAX,
        decaf377::Element::GENERATOR,
    );
    state
        .register_regulated_asset(asset::Id(Fq::from(777u64)), policy.clone())
        .await
        .unwrap();

    let mut sentinel = state.read_asset_leaf(0).await.unwrap();
    sentinel.next_value = Fq::from(778u64);
    state.nonverifiable_put_raw(
        state_key::tree_storage::asset_leaf(0).into_bytes(),
        bincode::serialize(&sentinel).unwrap(),
    );

    let readiness_error = state
        .verify_committed_tree_roots()
        .await
        .expect_err("a malformed successor chain must fail readiness");
    assert!(
        readiness_error.to_string().contains("successor value"),
        "unexpected error: {readiness_error:#}"
    );

    let next_asset = asset::Id(Fq::from(888u64));
    state
        .register_regulated_asset(next_asset, policy)
        .await
        .expect("an unrelated authenticated mutation should not reconstruct the full tree");
}

#[tokio::test]
async fn asset_mutation_rejects_a_corrupted_touched_path() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        u128::MAX,
        decaf377::Element::GENERATOR,
    );
    state
        .register_regulated_asset(asset::Id(Fq::from(777u64)), policy.clone())
        .await
        .unwrap();

    state.nonverifiable_put_raw(
        state_key::tree_storage::asset_node(0, 0).into_bytes(),
        Fq::from(99_999u64).to_bytes().to_vec(),
    );

    let error = state
        .register_regulated_asset(asset::Id(Fq::from(888u64)), policy)
        .await
        .expect_err("a touched path must authenticate before mutation");
    assert!(
        error.to_string().contains("does not authenticate"),
        "unexpected error: {error:#}"
    );
}

#[tokio::test]
async fn test_direct_read_proofs_match_reconstructed_trees_random_trace() {
    use rand::{Rng, SeedableRng};

    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x5eed);
    let policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        u128::MAX,
        decaf377::Element::GENERATOR,
    );
    let mut user_positions = Vec::new();
    let mut asset_ids = Vec::new();

    for step in 0..32u64 {
        if rng.gen_bool(0.5) {
            let asset_id = asset::Id(Fq::from(10_000u64 + step));
            let leaf = ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset_id);
            let root_before = state.get_user_tree_root().await.unwrap();
            let commitment = leaf.commit();
            let position = state.add_compliance_leaf(leaf).await.unwrap();
            user_positions.push((position, commitment));
            assert_eq!(
                state.get_user_count().await.unwrap(),
                user_positions.len() as u64
            );
            assert_ne!(state.get_user_tree_root().await.unwrap(), root_before);

            let reconstructed = state.reconstruct_user_tree().await.unwrap();
            assert_eq!(
                state.get_user_tree_root().await.unwrap(),
                reconstructed.root()
            );
            let direct_path = state.get_user_auth_path(position).await.unwrap();
            let reconstructed_path = reconstructed.auth_path(position).unwrap();
            assert_eq!(direct_path.len(), 16);
            assert_eq!(direct_path, reconstructed_path);
            assert!(QuadTree::verify_auth_path(
                position,
                commitment,
                &direct_path,
                reconstructed.root(),
                reconstructed.depth()
            ));
        } else {
            let asset_id = asset::Id(Fq::from(20_000u64 + step));
            state
                .register_regulated_asset(asset_id, policy.clone())
                .await
                .unwrap();
            asset_ids.push(asset_id);

            let reconstructed = state.reconstruct_asset_tree().await.unwrap();
            assert_eq!(
                state.get_asset_imt_root().await.unwrap(),
                reconstructed.root()
            );
            let direct_proof = state.get_asset_proof_data(asset_id).await.unwrap();
            let (position, leaf, path) = reconstructed.membership_proof(asset_id.0).unwrap();
            assert_eq!(direct_proof.position, position);
            assert_eq!(direct_proof.indexed_leaf, leaf);
            assert_eq!(direct_proof.auth_path, MerklePath::from_auth_path(path));
        }
    }

    for (position, commitment) in user_positions {
        let direct_path = state.get_user_auth_path(position).await.unwrap();
        let reconstructed = state.reconstruct_user_tree().await.unwrap();
        assert!(QuadTree::verify_auth_path(
            position,
            commitment,
            &direct_path,
            reconstructed.root(),
            reconstructed.depth()
        ));
    }
    for asset_id in asset_ids {
        let direct_proof = state.get_asset_proof_data(asset_id).await.unwrap();
        let reconstructed = state.reconstruct_asset_tree().await.unwrap();
        assert_eq!(
            indexed_tree::recompute_root(
                direct_proof.indexed_leaf.commit(),
                &direct_proof.auth_path,
                direct_proof.position,
            ),
            reconstructed.root()
        );
    }
}

#[tokio::test]
async fn test_asset_proof_direct_read_membership_and_gap_parity() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        u128::MAX,
        decaf377::Element::GENERATOR,
    );
    let registered = [100u64, 300u64];
    for value in registered {
        state
            .register_regulated_asset(asset::Id(Fq::from(value)), policy.clone())
            .await
            .unwrap();
    }

    let cases = [
        asset::Id(Fq::from(50u64)),
        asset::Id(Fq::from(100u64)),
        asset::Id(Fq::from(200u64)),
        asset::Id(Fq::from(300u64)),
        asset::Id(Fq::from(400u64)),
        asset::Id(*FQ_MAX - Fq::from(1u64)),
    ];

    let reconstructed = state.reconstruct_asset_tree().await.unwrap();
    for asset_id in cases {
        let direct = state.get_asset_proof_data(asset_id).await.unwrap();
        if reconstructed.contains(asset_id.0) {
            let (position, leaf, path) = reconstructed.membership_proof(asset_id.0).unwrap();
            assert!(direct.is_regulated);
            assert_eq!(direct.position, position);
            assert_eq!(direct.indexed_leaf, leaf);
            assert_eq!(direct.auth_path, MerklePath::from_auth_path(path));
        } else {
            let (position, leaf, path) = reconstructed.non_membership_proof(asset_id.0).unwrap();
            assert!(!direct.is_regulated);
            assert_eq!(direct.position, position);
            assert_eq!(direct.indexed_leaf, leaf);
            assert_eq!(direct.auth_path, MerklePath::from_auth_path(path));
        }
    }
}

#[tokio::test]
async fn test_cold_user_and_asset_proof_lookup_uses_keyed_storage() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();

    let asset_id = asset::Id(Fq::from(5151u64));
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();
    let leaf = ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset_id);
    let position = state.add_compliance_leaf(leaf).await.unwrap();

    let user_path = state.get_user_auth_path(position).await.unwrap();
    assert_eq!(user_path.len(), crate::tree::DEFAULT_DEPTH as usize);
    let asset_proof = state.get_asset_proof_data(asset_id).await.unwrap();
    assert!(asset_proof.is_regulated);
    assert_eq!(asset_proof.indexed_leaf.value, asset_id.0);
}

#[tokio::test]
async fn test_asset_duplicate_prevention() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let asset_id = asset::Id(Fq::from(789u64));

    // First registration should succeed
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .expect("First registration should succeed");

    // Verify asset is regulated
    let proof = state.get_asset_proof_data(asset_id).await.unwrap();
    assert!(proof.is_regulated);

    // Get IMT leaf count
    let imt = state.reconstruct_asset_tree().await.unwrap();
    let root_before = imt.root();
    let policy_before = state.get_asset_policy(asset_id).await.unwrap();
    let count_before = imt.leaf_count();
    assert_eq!(count_before, 2);

    // Second registration of same asset should be idempotent (succeed but no change)
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .expect("Duplicate registration should be idempotent");

    // Verify IMT leaf count didn't increase
    let imt = state.reconstruct_asset_tree().await.unwrap();
    assert_eq!(imt.root(), root_before);
    assert_eq!(
        state.get_asset_policy(asset_id).await.unwrap(),
        policy_before
    );
    assert_eq!(
        imt.leaf_count(),
        count_before,
        "IMT leaf count should remain unchanged after idempotent registration"
    );
}

#[tokio::test]
async fn test_verify_compliance_leaf() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let mut rng = rand::thread_rng();

    // Create a compliance leaf
    let leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(100u64)));

    // Before adding, verification should fail
    let verified = state.verify_compliance_leaf(&leaf).await.unwrap();
    assert!(!verified, "Leaf should not be verified before being added");

    // Add the leaf to the registry
    state.add_compliance_leaf(leaf.clone()).await.unwrap();

    // After adding, verification should succeed
    let verified = state.verify_compliance_leaf(&leaf).await.unwrap();
    assert!(verified, "Leaf should be verified after being added");

    // Create a different leaf with same asset but different wallet
    let different_leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(100u64)));

    // Different leaf should not verify
    let verified = state.verify_compliance_leaf(&different_leaf).await.unwrap();
    assert!(!verified, "Different leaf should not be verified");
}

#[tokio::test]
async fn test_leaf_json_serialization() {
    let mut rng = rand::thread_rng();

    // Create a compliance leaf
    let original_leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(200u64)));

    // Export to JSON
    let json = original_leaf
        .to_json()
        .expect("JSON serialization should succeed");

    // JSON should not be empty
    assert!(!json.is_empty(), "JSON should not be empty");

    // Import from JSON
    let imported_leaf =
        ComplianceLeaf::from_json(&json).expect("JSON deserialization should succeed");

    // Should be equal to original
    assert_eq!(
        original_leaf, imported_leaf,
        "Imported leaf should match original"
    );

    // Commitments should also match
    assert_eq!(
        original_leaf.commit().0,
        imported_leaf.commit().0,
        "Commitments should match"
    );
}

#[tokio::test]
async fn test_share_and_verify_workflow() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let mut rng = rand::thread_rng();

    // User creates their compliance leaf (private)
    let user_leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(300u64)));

    // User registers on-chain
    state.add_compliance_leaf(user_leaf.clone()).await.unwrap();

    // User exports their leaf to share off-chain with issuer
    let shared_json = user_leaf.to_json().expect("Export should succeed");

    // Issuer receives the JSON and imports it
    let received_leaf = ComplianceLeaf::from_json(&shared_json).expect("Import should succeed");

    // Issuer verifies that this leaf exists on-chain
    let is_valid = state.verify_compliance_leaf(&received_leaf).await.unwrap();
    assert!(
        is_valid,
        "Issuer should be able to verify the shared leaf exists on-chain"
    );

    // Leaf fields are preserved through sharing
    assert_eq!(user_leaf.address, received_leaf.address);
    assert_eq!(user_leaf.asset_id, received_leaf.asset_id);
}

#[tokio::test]
async fn test_verify_with_multiple_leaves() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let mut rng = rand::thread_rng();

    // Add multiple leaves
    let mut leaves = Vec::new();
    for i in 0..5u64 {
        let leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(i + 1)),
        );
        state.add_compliance_leaf(leaf.clone()).await.unwrap();
        leaves.push(leaf);
    }

    // All added leaves should verify
    for leaf in &leaves {
        let verified = state.verify_compliance_leaf(leaf).await.unwrap();
        assert!(verified, "All added leaves should verify");
    }

    // A new leaf not in the tree should not verify
    let new_leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(999u64)));
    let verified = state.verify_compliance_leaf(&new_leaf).await.unwrap();
    assert!(!verified, "Non-registered leaf should not verify");
}

#[tokio::test]
async fn test_comprehensive_integration() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    // Bridged asset (USDC) - regulated
    let usdc_asset_id = asset::Id(Fq::from(12345u64));
    state
        .register_regulated_asset(
            usdc_asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();

    let usdc_proof = state.get_asset_proof_data(usdc_asset_id).await.unwrap();
    assert!(usdc_proof.is_regulated);

    let empty_imt = IndexedMerkleTree::new();
    assert_ne!(
        state.get_asset_imt_root().await.unwrap().0,
        empty_imt.root().0
    );

    // Native asset (shieldd) - unregulated (NOT in IMT)
    let shieldd_asset_id = asset::Id(Fq::from(1u64));
    // Don't register - unregulated assets are proven via non-membership
    let shieldd_proof = state.get_asset_proof_data(shieldd_asset_id).await.unwrap();
    assert!(!shieldd_proof.is_regulated);

    // Distinct users derive distinct capabilities from their full addresses.
    let wallet1 = Address::dummy(&mut rng);
    let leaf1 = ComplianceLeaf::registered_for_test(wallet1.clone(), usdc_asset_id);
    let leaf2 = ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), usdc_asset_id);
    let leaf3 = ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), usdc_asset_id);

    state.add_compliance_leaf(leaf1.clone()).await.unwrap();
    state.add_compliance_leaf(leaf2.clone()).await.unwrap();
    state.add_compliance_leaf(leaf3.clone()).await.unwrap();
    assert_eq!(state.get_user_count().await.unwrap(), 3);
    assert!(state.verify_compliance_leaf(&leaf1).await.unwrap());
    assert!(state.verify_compliance_leaf(&leaf2).await.unwrap());
    assert!(state.verify_compliance_leaf(&leaf3).await.unwrap());

    // Share and verify workflow
    let shared_json = leaf1.to_json().unwrap();
    let received_leaf = ComplianceLeaf::from_json(&shared_json).unwrap();
    assert!(state.verify_compliance_leaf(&received_leaf).await.unwrap());
    assert_eq!(received_leaf.address, leaf1.address);

    // Query unregistered asset - should get non-membership proof
    let unknown_asset = asset::Id(Fq::from(99999u64));
    let unknown_proof = state.get_asset_proof_data(unknown_asset).await.unwrap();
    assert!(!unknown_proof.is_regulated);

    // Authentication paths
    let path = state.get_user_auth_path(0).await.unwrap();
    assert_eq!(path.len(), 16);
    let user_root = state.get_user_tree_root().await.unwrap();
    let tree = state.reconstruct_user_tree().await.unwrap();
    assert!(QuadTree::verify_auth_path(
        0,
        leaf1.commit(),
        &path,
        user_root,
        tree.depth()
    ));

    // Same wallet registered for multiple assets
    let dai_asset_id = asset::Id(Fq::from(67890u64));
    state
        .register_regulated_asset(
            dai_asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();
    let leaf1_dai = ComplianceLeaf::registered_for_test(wallet1, dai_asset_id);
    state.add_compliance_leaf(leaf1_dai.clone()).await.unwrap();
    assert!(state.verify_compliance_leaf(&leaf1).await.unwrap());
    assert!(state.verify_compliance_leaf(&leaf1_dai).await.unwrap());
    assert_eq!(state.get_user_count().await.unwrap(), 4);
}

#[tokio::test]
async fn test_user_leaf_position_lookup() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);
    let mut rng = rand::thread_rng();

    let wallet1 = Address::dummy(&mut rng);
    let wallet2 = Address::dummy(&mut rng);
    let usdc = asset::Id(Fq::from(12345u64));
    let dai = asset::Id(Fq::from(67890u64));

    let leaf1 = ComplianceLeaf::registered_for_test(wallet1.clone(), usdc);
    let leaf2 = ComplianceLeaf::registered_for_test(wallet1.clone(), dai);
    let leaf3 = ComplianceLeaf::registered_for_test(wallet2.clone(), usdc);

    state.add_compliance_leaf(leaf1.clone()).await.unwrap();
    state.add_compliance_leaf(leaf2.clone()).await.unwrap();
    state.add_compliance_leaf(leaf3.clone()).await.unwrap();

    // Position lookups
    assert_eq!(
        state.get_user_leaf_position(&wallet1, usdc).await.unwrap(),
        Some(0)
    );
    assert_eq!(
        state.get_user_leaf_position(&wallet1, dai).await.unwrap(),
        Some(1)
    );
    assert_eq!(
        state.get_user_leaf_position(&wallet2, usdc).await.unwrap(),
        Some(2)
    );
    assert_eq!(
        state
            .get_user_leaf_position(&Address::dummy(&mut rng), usdc)
            .await
            .unwrap(),
        None
    );

    // Auth paths verify correctly
    let tree = state.reconstruct_user_tree().await.unwrap();
    let root = tree.root();
    let path0 = state.get_user_auth_path(0).await.unwrap();
    let path1 = state.get_user_auth_path(1).await.unwrap();
    assert_eq!(path0.len(), 16);
    assert!(QuadTree::verify_auth_path(
        0,
        leaf1.commit(),
        &path0,
        root,
        tree.depth()
    ));
    assert!(QuadTree::verify_auth_path(
        1,
        leaf2.commit(),
        &path1,
        root,
        tree.depth()
    ));
}

/// Tests that get_user_leaf() returns the exact registered leaf (catches ACK mismatch bugs).
#[tokio::test]
async fn test_user_leaf_roundtrip() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    let wallet = Address::dummy(&mut rng);
    let asset_id = asset::Id(Fq::from(12345u64));

    let original_leaf = ComplianceLeaf::registered_for_test(wallet.clone(), asset_id);
    state
        .add_compliance_leaf(original_leaf.clone())
        .await
        .unwrap();

    let fetched_leaf = state
        .get_user_leaf(&wallet, asset_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original_leaf.address, fetched_leaf.address);
    assert_eq!(original_leaf.asset_id, fetched_leaf.asset_id);
    assert_eq!(original_leaf.commit().0, fetched_leaf.commit().0);
    assert!(state
        .get_user_leaf(&Address::dummy(&mut rng), asset_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn user_leaf_record_is_compact_and_authenticated() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    let wallet = Address::dummy(&mut rng);
    let asset_id = asset::Id(Fq::from(54321u64));
    let leaf = ComplianceLeaf::registered_for_test(wallet.clone(), asset_id);
    let position = state.add_compliance_leaf(leaf.clone()).await.unwrap();

    let record = state
        .get_user_leaf_record(&wallet, asset_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        record,
        UserLeafRecord {
            position,
            leaf: leaf.clone()
        }
    );
    assert!(state
        .get_raw(&format!("compliance/user_lookup/{wallet}/{asset_id}"))
        .await
        .unwrap()
        .is_none());
    assert!(state
        .get_raw(&format!("compliance/user_leaf/{wallet}/{asset_id}"))
        .await
        .unwrap()
        .is_none());

    let corrupt_leaf = leaf.clone().with_status_for_test(UserAssetStatus::Frozen);
    let corrupt = UserLeafRecord {
        position,
        leaf: corrupt_leaf,
    };
    state.put_raw(
        state_key::user_leaf_record(&wallet, &asset_id),
        encode_user_leaf_record(&corrupt).unwrap(),
    );
    let error = state
        .get_user_leaf(&wallet, asset_id)
        .await
        .expect_err("record that disagrees with the tree must be rejected");
    assert!(format!("{error:#}").contains("does not match the committed user tree"));
}

// ========== IMT Tests ==========

#[tokio::test]
async fn regulated_asset_identity_keys_fail_before_tree_mutation() {
    let identity_authority = VerificationKey::from(&SigningKey::<SpendAuth>::from(Fr::from(0u64)));
    let cases = [
        (
            "detection key",
            AssetPolicy::for_test(decaf377::Element::IDENTITY, 1, decaf377::Element::GENERATOR),
        ),
        (
            "ring key",
            AssetPolicy::for_test(decaf377::Element::GENERATOR, 1, decaf377::Element::IDENTITY),
        ),
        (
            "registration authority key",
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                1,
                decaf377::Element::GENERATOR,
            )
            .with_registration_authority(identity_authority),
        ),
    ];

    for (index, (key_role, policy)) in cases.into_iter().enumerate() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();
        let root_before = state.get_asset_imt_root().await.unwrap();
        let asset_id = asset::Id(Fq::from(12345u64 + index as u64));

        let error = state
            .register_regulated_asset(asset_id, policy)
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("identity"),
            "{key_role} returned unexpected error: {error:#}"
        );
        assert_eq!(
            state.get_asset_imt_root().await.unwrap(),
            root_before,
            "{key_role} changed the asset tree root"
        );
        assert!(
            !state
                .reconstruct_asset_tree()
                .await
                .unwrap()
                .contains(asset_id.0),
            "{key_role} inserted an asset leaf"
        );
        assert!(
            state.get_asset_policy(asset_id).await.unwrap().is_none(),
            "{key_role} persisted an asset policy"
        );
    }
}

#[tokio::test]
async fn test_imt_get_proof_data_regulated() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let asset_id = asset::Id(Fq::from(12345u64));
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();

    // Get proof data for regulated asset
    let proof_data = state.get_asset_proof_data(asset_id).await.unwrap();

    assert!(proof_data.is_regulated);
    assert_eq!(proof_data.indexed_leaf.value, asset_id.0);
    assert_eq!(proof_data.position, 1); // First after sentinel
    assert_eq!(proof_data.auth_path.layers.len(), 16);

    // Verify the path
    let tree = state.reconstruct_asset_tree().await.unwrap();
    let root = tree.root();
    assert!(IndexedMerkleTree::verify_auth_path(
        proof_data.position,
        &proof_data.indexed_leaf,
        &tree.auth_path(proof_data.position).unwrap(),
        root,
        tree.depth()
    ));
}

#[tokio::test]
async fn test_imt_get_proof_data_unregulated() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    // Register one asset
    let regulated_asset = asset::Id(Fq::from(100u64));
    state
        .register_regulated_asset(
            regulated_asset,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();

    // Get proof for unregistered asset (should be non-membership proof)
    let unregulated_asset = asset::Id(Fq::from(50u64));
    let proof_data = state.get_asset_proof_data(unregulated_asset).await.unwrap();

    assert!(!proof_data.is_regulated);
    // The low leaf should be the sentinel (value=0)
    assert_eq!(proof_data.indexed_leaf.value, Fq::from(0u64));
    assert_eq!(proof_data.position, 0); // Sentinel position

    // Verify the unregulated asset falls in the gap
    assert!(proof_data.indexed_leaf.value < unregulated_asset.0);
    assert!(unregulated_asset.0 < proof_data.indexed_leaf.next_value);
}

#[tokio::test]
async fn test_imt_multiple_regulated_assets() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    // Register multiple assets in non-sorted order
    let assets = [
        asset::Id(Fq::from(500u64)),
        asset::Id(Fq::from(100u64)),
        asset::Id(Fq::from(300u64)),
    ];

    for asset_id in &assets {
        state
            .register_regulated_asset(
                *asset_id,
                AssetPolicy::for_test(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
            )
            .await
            .unwrap();
    }

    let tree = state.reconstruct_asset_tree().await.unwrap();
    assert_eq!(tree.leaf_count(), 4); // sentinel + 3 assets

    // All should have valid membership proofs
    for asset_id in &assets {
        let proof_data = state.get_asset_proof_data(*asset_id).await.unwrap();
        assert!(proof_data.is_regulated);
        assert_eq!(proof_data.indexed_leaf.value, asset_id.0);
    }

    // An unregistered asset in a gap should have non-membership proof
    let unregistered = asset::Id(Fq::from(200u64)); // Between 100 and 300
    let proof_data = state.get_asset_proof_data(unregistered).await.unwrap();
    assert!(!proof_data.is_regulated);
    // Low leaf should be the one with value=100
    assert_eq!(proof_data.indexed_leaf.value, Fq::from(100u64));
}

// ========== Historical Anchor Tests ==========

#[tokio::test]
async fn test_record_and_validate_anchors() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    // Set block height first (required for validation)
    state.put_block_height(1);

    // Add a user and asset
    let leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(100u64)));
    state.add_compliance_leaf(leaf).await.unwrap();
    state
        .register_regulated_asset(
            asset::Id(Fq::from(200u64)),
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();

    // Record anchors at height 1
    state.record_compliance_anchors(1).await.unwrap();

    // Get the anchors
    let user_anchor = state.get_user_tree_root().await.unwrap();
    let asset_anchor = state.get_asset_imt_root().await.unwrap();

    // Validation should succeed
    state
        .validate_compliance_anchors(&user_anchor, &asset_anchor)
        .await
        .unwrap();

    // Append-only user roots remain available as recent history.
    let user_anchor_by_height = state.get_user_anchor_by_height(1).await.unwrap().unwrap();
    assert_eq!(user_anchor.0, user_anchor_by_height.0);

    // Historical validation applies only to the append-only user tree.
    let user_height = state.check_user_anchor(&user_anchor).await.unwrap();
    assert_eq!(user_height, Some(1));
}

#[tokio::test]
async fn test_invalid_anchor_rejected() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);

    // Set block height first (required for validation)
    state.put_block_height(1);

    // Record initial anchors
    state.record_compliance_anchors(1).await.unwrap();

    let valid_user_anchor = state.get_user_tree_root().await.unwrap();
    let valid_asset_anchor = state.get_asset_imt_root().await.unwrap();

    // Create invalid anchors
    let invalid_user_anchor = StateCommitment(Fq::from(12345u64));
    let invalid_asset_anchor = StateCommitment(Fq::from(67890u64));

    // Valid anchors should pass
    assert!(state
        .validate_compliance_anchors(&valid_user_anchor, &valid_asset_anchor)
        .await
        .is_ok());

    // Invalid user anchor should fail
    assert!(state
        .validate_compliance_anchors(&invalid_user_anchor, &valid_asset_anchor)
        .await
        .is_err());

    // Invalid asset anchor should fail
    assert!(state
        .validate_compliance_anchors(&valid_user_anchor, &invalid_asset_anchor)
        .await
        .is_err());
}

#[tokio::test]
async fn historical_anchors_are_retained_but_only_the_current_root_is_accepted() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    // Set initial block height and record anchors at height 1 (empty state)
    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let anchor_at_1 = state.get_user_tree_root().await.unwrap();

    // Add a user and record at height 2
    state.put_block_height(2);
    let leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(100u64)));
    state.add_compliance_leaf(leaf).await.unwrap();
    state.record_compliance_anchors(2).await.unwrap();
    let anchor_at_2 = state.get_user_tree_root().await.unwrap();

    // Both anchors should be different
    assert_ne!(anchor_at_1.0, anchor_at_2.0);

    // Historical records remain available for indexing and audit, but proofs
    // must use the current root so a pre-freeze proof cannot be replayed.
    let asset_anchor = state.get_asset_imt_root().await.unwrap();
    let error = state
        .validate_compliance_anchors(&anchor_at_1, &asset_anchor)
        .await
        .expect_err("a historical user root must not authorize a transaction");
    assert!(
        error.to_string().contains("current user compliance root"),
        "unexpected error: {error:#}"
    );
    assert!(state
        .validate_compliance_anchors(&anchor_at_2, &asset_anchor)
        .await
        .is_ok());

    // Can retrieve both by height
    assert_eq!(
        state.get_user_anchor_by_height(1).await.unwrap().unwrap().0,
        anchor_at_1.0
    );
    assert_eq!(
        state.get_user_anchor_by_height(2).await.unwrap().unwrap().0,
        anchor_at_2.0
    );
}

// ========== Bounded Anchor Window Tests (Phase 7) ==========

#[tokio::test]
async fn stale_user_anchor_is_rejected_immediately_after_status_tree_change() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    // Set initial height and record anchor
    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let old_user_anchor = state.get_user_tree_root().await.unwrap();
    let old_asset_anchor = state.get_asset_imt_root().await.unwrap();

    // Add something to change the tree roots (so old anchors remain distinct)
    let leaf =
        ComplianceLeaf::registered_for_test(Address::dummy(&mut rng), asset::Id(Fq::from(9999u64)));
    state.add_compliance_leaf(leaf).await.unwrap();

    // Advance beyond the retained-history window. The rejection is based on
    // the current root, not the age of the historical record.
    let new_height = 1 + TEST_ANCHOR_WINDOW_BLOCKS + 1;
    state.put_block_height(new_height);
    state.record_compliance_anchors(new_height).await.unwrap();

    // New anchors should be different
    let new_user_anchor = state.get_user_tree_root().await.unwrap();
    assert_ne!(
        old_user_anchor.0, new_user_anchor.0,
        "Anchors should differ after adding leaf"
    );

    // Validation of the old user root must fail immediately.
    let result = state
        .validate_compliance_anchors(&old_user_anchor, &old_asset_anchor)
        .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("current user compliance root"),
        "error should identify the current-root mismatch: {}",
        err_msg
    );
}

#[tokio::test]
async fn stale_asset_anchor_is_rejected_immediately_after_policy_change() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);

    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let user_anchor = state.get_user_tree_root().await.unwrap();
    let stale_asset_anchor = state.get_asset_imt_root().await.unwrap();

    state.put_block_height(2);
    state
        .register_regulated_asset(
            asset::Id(Fq::from(4242u64)),
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();
    let current_asset_anchor = state.get_asset_imt_root().await.unwrap();
    assert_ne!(stale_asset_anchor, current_asset_anchor);

    let error = state
        .validate_compliance_anchors(&user_anchor, &stale_asset_anchor)
        .await
        .expect_err("an asset anchor predating a policy change must be invalid immediately");
    assert!(
        error.to_string().contains("current asset compliance root"),
        "unexpected error: {error:#}"
    );
}

#[tokio::test]
async fn current_roots_remain_valid_when_the_history_window_changes() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);

    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let user_anchor = state.get_user_tree_root().await.unwrap();
    let asset_anchor = state.get_asset_imt_root().await.unwrap();

    for (window, height) in [
        (TEST_ANCHOR_WINDOW_BLOCKS, 1 + TEST_ANCHOR_WINDOW_BLOCKS / 2),
        (10, 12),
    ] {
        state.put_compliance_params(ComplianceParameters {
            anchor_validation_window_blocks: window,
        });
        state.put_block_height(height);
        state.record_compliance_anchors(height).await.unwrap();
        state
            .validate_compliance_anchors(&user_anchor, &asset_anchor)
            .await
            .expect("history-retention policy must not invalidate current roots");
    }
}

#[tokio::test]
async fn test_shortened_anchor_window_pruning_catches_up() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let height_one_user_anchor = state.get_user_tree_root().await.unwrap();

    state.put_block_height(2);
    state
        .add_compliance_leaf(ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(9090u64)),
        ))
        .await
        .unwrap();
    state.record_compliance_anchors(2).await.unwrap();
    let height_two_user_anchor = state.get_user_tree_root().await.unwrap();

    state.put_compliance_params(ComplianceParameters {
        anchor_validation_window_blocks: 10,
    });
    state.put_block_height(50);
    state.record_compliance_anchors(50).await.unwrap();

    assert_eq!(state.get_user_anchor_by_height(1).await.unwrap(), None);
    assert_eq!(state.get_user_anchor_by_height(2).await.unwrap(), None);
    assert_eq!(
        state
            .check_user_anchor(&height_one_user_anchor)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        state
            .check_user_anchor(&height_two_user_anchor)
            .await
            .unwrap(),
        Some(50)
    );
    assert_eq!(
        state
            .get_proto::<u64>(state_key::anchor::pruned_through_height())
            .await
            .unwrap(),
        Some(29)
    );
}

#[tokio::test]
async fn test_genesis_anchor_attack_prevented() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);

    // Genesis: IMT is empty, record anchor at height 0
    state.put_block_height(0);
    state.record_compliance_anchors(0).await.unwrap();
    let genesis_user_anchor = state.get_user_tree_root().await.unwrap();
    let genesis_asset_anchor = state.get_asset_imt_root().await.unwrap();

    // Register USDC as regulated at height 100
    state.put_block_height(100);
    let usdc_id = asset::Id(Fq::from(12345u64));
    state
        .register_regulated_asset(
            usdc_id,
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
        )
        .await
        .unwrap();
    state.record_compliance_anchors(100).await.unwrap();

    // The asset IMT root should have changed
    let new_asset_anchor = state.get_asset_imt_root().await.unwrap();
    assert_ne!(
        genesis_asset_anchor.0, new_asset_anchor.0,
        "IMT root should change after registration"
    );

    // Advance to height past the window (genesis anchor now too old)
    let attack_height = TEST_ANCHOR_WINDOW_BLOCKS + 1;
    state.put_block_height(attack_height);
    state
        .record_compliance_anchors(attack_height)
        .await
        .unwrap();

    // Attempt to use genesis anchor for USDC non-membership proof
    // This is the "genesis anchor attack" - should FAIL
    let result = state
        .validate_compliance_anchors(&genesis_user_anchor, &genesis_asset_anchor)
        .await;

    // The mutable asset-policy tree must match current state regardless of
    // whether a stale root also exceeds the user-history window.
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("current asset compliance root"),
        "Genesis anchor attack should be prevented: {}",
        err_msg
    );

    // But a recent anchor should still work
    let recent_user_anchor = state.get_user_tree_root().await.unwrap();
    let recent_asset_anchor = state.get_asset_imt_root().await.unwrap();
    assert!(state
        .validate_compliance_anchors(&recent_user_anchor, &recent_asset_anchor)
        .await
        .is_ok());
}

#[tokio::test]
async fn test_register_asset_with_custom_daily_volume_limit() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let asset_id = asset::Id(Fq::from(555u64));
    let dk_pub = decaf377::Element::GENERATOR;

    // Register with daily_volume_limit=500
    state
        .register_regulated_asset(
            asset_id,
            AssetPolicy::for_test(dk_pub, 500u128, decaf377::Element::GENERATOR),
        )
        .await
        .unwrap();

    // Read back proof data - asset must be regulated
    let proof = state.get_asset_proof_data(asset_id).await.unwrap();
    assert!(proof.is_regulated);

    // Policy is stored separately via set_asset_policy
    let policy = state
        .get_asset_policy(asset_id)
        .await
        .unwrap()
        .expect("policy should be set after registration");
    assert_eq!(
        policy.params.daily_volume_limit, 500u128,
        "daily_volume_limit should survive round-trip"
    );
    assert_eq!(policy.params.dk_pub, dk_pub);
}

#[tokio::test]
async fn test_get_asset_policy_cached_matches_uncached() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let present_asset = asset::Id(Fq::from(77u64));
    let missing_asset = asset::Id(Fq::from(88u64));
    let policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        u128::MAX,
        decaf377::Element::GENERATOR,
    );

    state
        .register_regulated_asset(present_asset, policy.clone())
        .await
        .unwrap();

    assert_eq!(
        state.get_asset_policy(present_asset).await.unwrap(),
        state.get_asset_policy_cached(present_asset).await.unwrap()
    );
    assert_eq!(
        state.get_asset_policy_cached(present_asset).await.unwrap(),
        Some(policy.clone())
    );

    assert_eq!(
        state.get_asset_policy(missing_asset).await.unwrap(),
        state.get_asset_policy_cached(missing_asset).await.unwrap()
    );
    assert_eq!(
        state.get_asset_policy_cached(missing_asset).await.unwrap(),
        None
    );

    let cached: BTreeMap<asset::Id, Option<AssetPolicy>> = state
        .object_get(state_key::cache::cached_asset_policies())
        .unwrap();
    assert_eq!(cached.get(&present_asset), Some(&Some(policy)));
    assert_eq!(cached.get(&missing_asset), Some(&None));
}

#[tokio::test]
async fn test_anchor_pruning_removes_expired_entries() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    let mut rng = rand::thread_rng();
    put_test_compliance_params(&mut state);

    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let expired_user_anchor = state.get_user_tree_root().await.unwrap();

    state.put_block_height(2);
    state
        .add_compliance_leaf(ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(4242u64)),
        ))
        .await
        .unwrap();
    let prune_height = TEST_ANCHOR_WINDOW_BLOCKS + MAX_ANCHOR_SEARCH_DEPTH_BLOCKS + 2;
    state.put_block_height(prune_height);
    state.record_compliance_anchors(prune_height).await.unwrap();

    assert_eq!(state.get_user_anchor_by_height(1).await.unwrap(), None);
    assert_eq!(
        state.check_user_anchor(&expired_user_anchor).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn test_ibc_origin_lookup_rejects_duplicate_base_denom() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);

    let route = crate::IbcRoute::transfer("channel-0", "connection-0", "channel-7");
    let mut policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        500,
        decaf377::Element::GENERATOR,
    );
    policy.replace_allowed_ibc_routes(vec![route.clone()]);
    policy.params.ibc_origin = Some(crate::IbcAssetOrigin {
        route,
        base_denom: "ubank".to_string(),
    });

    let asset_id = asset::Id(Fq::from(700u64));
    state
        .register_regulated_asset(asset_id, policy.clone())
        .await
        .unwrap();
    assert_eq!(
        state.get_ibc_origin_asset_id("ubank").await.unwrap(),
        Some(asset_id)
    );

    let duplicate = state
        .register_regulated_asset(asset::Id(Fq::from(701u64)), policy)
        .await
        .expect_err("duplicate base denom should fail");
    assert!(
        duplicate.to_string().contains("already registered"),
        "unexpected error: {duplicate:#}"
    );
}

#[tokio::test]
async fn test_replace_asset_ibc_policy_requires_expected_hash() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();

    let old_route = crate::IbcRoute::transfer("channel-0", "connection-0", "channel-7");
    let new_route = crate::IbcRoute::transfer("channel-1", "connection-1", "channel-8");
    let mut policy = AssetPolicy::for_test(
        decaf377::Element::GENERATOR,
        500,
        decaf377::Element::GENERATOR,
    );
    policy.replace_allowed_ibc_routes(vec![old_route.clone()]);
    let expected_hash = indexed_tree::route_policy_to_fq(&policy.params).to_bytes();
    let asset_id = asset::Id(Fq::from(702u64));

    state
        .register_regulated_asset(asset_id, policy)
        .await
        .unwrap();
    state
        .replace_asset_ibc_policy(asset_id, expected_hash, vec![new_route.clone()])
        .await
        .unwrap();

    let updated = state.get_asset_policy(asset_id).await.unwrap().unwrap();
    assert!(updated.permits_ibc_route(&new_route));
    assert!(!updated.permits_ibc_route(&old_route));

    let stale = state
        .replace_asset_ibc_policy(asset_id, expected_hash, vec![])
        .await
        .expect_err("stale route policy hash should fail");
    assert!(
        stale.to_string().contains("did not match"),
        "unexpected error: {stale:#}"
    );
}

#[tokio::test]
async fn test_anchor_pruning_preserves_latest_lookup_for_reused_anchor() {
    let storage = TempStorage::new().await.unwrap();
    let snapshot = storage.latest_snapshot();
    let mut state = cnidarium::StateDelta::new(snapshot);
    state.initialize_trees().await.unwrap();
    put_test_compliance_params(&mut state);

    state.put_block_height(1);
    state.record_compliance_anchors(1).await.unwrap();
    let reused_user_anchor = state.get_user_tree_root().await.unwrap();

    let prune_height = TEST_ANCHOR_WINDOW_BLOCKS + MAX_ANCHOR_SEARCH_DEPTH_BLOCKS + 2;
    state.put_block_height(prune_height);
    state.record_compliance_anchors(prune_height).await.unwrap();

    assert_eq!(state.get_user_anchor_by_height(1).await.unwrap(), None);
    assert_eq!(
        state.check_user_anchor(&reused_user_anchor).await.unwrap(),
        Some(prune_height)
    );
}

#[test]
fn timestamp_freshness_accepts_current_and_exact_boundaries() {
    const CURRENT: i64 = 1_700_000_000;
    assert!(check_timestamp_freshness(CURRENT as u64, CURRENT).is_ok());
    assert!(
        check_timestamp_freshness((CURRENT as u64) - MAX_TIMESTAMP_DRIFT_SECS, CURRENT,).is_ok()
    );
    assert!(
        check_timestamp_freshness((CURRENT as u64) + MAX_TIMESTAMP_DRIFT_SECS, CURRENT,).is_ok()
    );
}

#[test]
fn timestamp_freshness_signed_conversion_matches_u64_policy() {
    let cases = [
        (1u64, 0i64),
        (1, 1),
        (1, 3_601),
        (1, 3_602),
        (7_201, 3_601),
        (7_202, 3_601),
        (i64::MAX as u64, i64::MAX),
    ];
    for (target_timestamp, consensus_timestamp) in cases {
        let block_timestamp = u64::try_from(consensus_timestamp).expect("case is nonnegative");
        let expected = target_timestamp != 0
            && target_timestamp.abs_diff(block_timestamp) <= MAX_TIMESTAMP_DRIFT_SECS;
        assert_eq!(
            check_timestamp_freshness(target_timestamp, consensus_timestamp).is_ok(),
            expected,
            "signed conversion changed freshness for target={target_timestamp}, consensus={consensus_timestamp}"
        );
    }
}

#[test]
fn timestamp_freshness_rejects_negative_consensus_time() {
    let error = check_timestamp_freshness(1, -1)
        .expect_err("pre-epoch consensus time must fail before conversion");
    assert!(
        error.to_string().contains("must be nonnegative"),
        "unexpected rejection reason: {error:#}"
    );
}

#[test]
fn timestamp_freshness_rejects_zero_and_outside_window() {
    assert!(check_timestamp_freshness(0, 0).is_err());
    assert!(check_timestamp_freshness(0, 1).is_err());
    assert!(check_timestamp_freshness(0, MAX_TIMESTAMP_DRIFT_SECS as i64).is_err());
    assert!(check_timestamp_freshness(1000, 1000 + 3601).is_err());
    assert!(check_timestamp_freshness(1000 + 3601, 1000).is_err());
}
