use super::*;

/// Helper to create a test policy.
fn test_policy() -> AssetPolicy {
    AssetPolicy::default_unregulated()
}

#[test]
fn test_imt_new_has_sentinel() {
    let tree = IndexedMerkleTree::new();
    assert_eq!(tree.leaf_count(), 1);
    tree.validate_well_formed().unwrap();

    let sentinel = tree.get_leaf(0).expect("Sentinel should exist");
    assert_eq!(sentinel.value, Fq::from(0u64));
    assert_eq!(sentinel.next_index, 0);
    assert_eq!(sentinel.next_value, *FQ_MAX);
}

fn populated_tree() -> IndexedMerkleTree {
    let mut tree = IndexedMerkleTree::with_depth(4);
    for value in [300u64, 100, 200] {
        tree.insert(Fq::from(value), &test_policy()).unwrap();
    }
    tree.validate_well_formed().unwrap();
    tree
}

#[test]
fn well_formedness_rejects_depth_and_capacity_corruption() {
    let mut depth = populated_tree();
    depth.depth = DEFAULT_DEPTH + 1;
    assert!(depth
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("depth"));

    let mut capacity = IndexedMerkleTree::with_depth(1);
    capacity.leaf_count = 5;
    assert!(capacity
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("capacity"));
}

#[test]
fn well_formedness_rejects_count_and_position_corruption() {
    let mut count = populated_tree();
    count.leaf_count += 1;
    assert!(count
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("leaf map"));

    let mut position = populated_tree();
    let leaf = position.leaves.remove(&1).unwrap();
    position.leaves.insert(position.leaf_count, leaf);
    assert!(position
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("missing position"));
}

#[test]
fn well_formedness_rejects_duplicate_values_and_noncanonical_sentinel() {
    let mut duplicate = populated_tree();
    let duplicated_value = duplicate.leaves[&1].value;
    duplicate.leaves.get_mut(&2).unwrap().value = duplicated_value;
    assert!(duplicate
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("duplicate"));

    let mut sentinel = populated_tree();
    sentinel
        .leaves
        .get_mut(&0)
        .unwrap()
        .params
        .daily_volume_limit = 7;
    assert!(sentinel
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("sentinel policy"));
}

#[test]
fn well_formedness_rejects_incomplete_and_inconsistent_successor_chains() {
    let mut incomplete = populated_tree();
    let sentinel = incomplete.leaves.get_mut(&0).unwrap();
    sentinel.next_index = 0;
    sentinel.next_value = *FQ_MAX;
    assert!(incomplete
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("visits"));

    let mut inconsistent = populated_tree();
    inconsistent.leaves.get_mut(&0).unwrap().next_value = Fq::from(99u64);
    assert!(inconsistent
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("successor value"));

    let mut non_increasing = populated_tree();
    non_increasing.leaves.get_mut(&0).unwrap().next_index = 1;
    non_increasing.leaves.get_mut(&0).unwrap().next_value = Fq::from(300u64);
    non_increasing.leaves.get_mut(&1).unwrap().next_index = 2;
    non_increasing.leaves.get_mut(&1).unwrap().next_value = Fq::from(100u64);
    assert!(non_increasing
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("strictly increasing"));
}

#[test]
fn well_formedness_rejects_derived_index_and_merkle_corruption() {
    let mut index = populated_tree();
    index.value_index.insert(Fq::from(999u64).to_bytes(), 1);
    assert!(index
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("value index"));

    let mut predecessor = populated_tree();
    predecessor
        .predecessor_index
        .insert(FqOrdKey::from(Fq::from(999u64)), 1);
    assert!(predecessor
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("predecessor index"));

    let mut nodes = populated_tree();
    nodes.nodes.insert(
        IndexedMerkleTree::node_key(nodes.depth, 0),
        StateCommitment(Fq::from(9u64)),
    );
    assert!(nodes
        .validate_well_formed()
        .unwrap_err()
        .to_string()
        .contains("stored nodes"));
}

#[test]
fn sparse_reconstruction_and_deserialization_reject_corruption() {
    let tree = populated_tree();
    let mut leaves = tree.leaves.clone();
    leaves.get_mut(&0).unwrap().next_value = Fq::from(99u64);
    assert!(IndexedMerkleTree::try_from_sparse_parts(
        tree.depth,
        tree.nodes.clone(),
        leaves,
        tree.leaf_count,
    )
    .is_err());

    let mut serialized_corruption = tree;
    serialized_corruption.value_index.clear();
    let bytes = bincode::serialize(&serialized_corruption).unwrap();
    assert!(bincode::deserialize::<IndexedMerkleTree>(&bytes).is_err());
}

#[test]
fn insertion_and_policy_update_preserve_all_invariants() {
    let mut tree = IndexedMerkleTree::with_depth(4);
    for value in [400u64, 100, 900, 250, 700] {
        tree.insert(Fq::from(value), &test_policy()).unwrap();
        tree.validate_well_formed().unwrap();
    }
    tree.update_policy(Fq::from(400u64), &test_policy())
        .unwrap();
    tree.validate_well_formed().unwrap();
}

#[test]
fn test_imt_insert_single() {
    let mut tree = IndexedMerkleTree::new();
    let value = Fq::from(100u64);

    let result = tree.insert(value, &test_policy()).unwrap();
    assert_eq!(result.position, 1);
    assert_eq!(tree.leaf_count(), 2);
    assert!(tree.contains(value));

    let leaf = tree.get_leaf(1).unwrap();
    assert_eq!(leaf.value, value);
    assert_eq!(leaf.next_index, 0);
    assert_eq!(leaf.next_value, *FQ_MAX);

    let sentinel = tree.get_leaf(0).unwrap();
    assert_eq!(sentinel.next_index, 1);
    assert_eq!(sentinel.next_value, value);
}

#[test]
fn test_imt_insert_multiple_maintains_order() {
    let mut tree = IndexedMerkleTree::with_depth(4);
    let policy = test_policy();

    let values = [
        Fq::from(500u64),
        Fq::from(100u64),
        Fq::from(300u64),
        Fq::from(200u64),
    ];

    for v in values {
        tree.insert(v, &policy).unwrap();
    }

    let mut collected = Vec::new();
    let mut pos = 0u64;
    loop {
        let leaf = tree.get_leaf(pos).unwrap();
        if leaf.next_value == *FQ_MAX {
            break;
        }
        collected.push(leaf.next_value);
        pos = leaf.next_index;
    }

    let expected = vec![
        Fq::from(100u64),
        Fq::from(200u64),
        Fq::from(300u64),
        Fq::from(500u64),
    ];
    assert_eq!(collected, expected);
}

#[test]
fn test_imt_membership_proof() {
    let mut tree = IndexedMerkleTree::new();
    let value = Fq::from(42u64);
    tree.insert(value, &test_policy()).unwrap();

    let (pos, leaf, path) = tree.membership_proof(value).unwrap();
    assert_eq!(pos, 1);
    assert_eq!(leaf.value, value);
    assert_eq!(path.len(), DEFAULT_DEPTH as usize);

    let root = tree.root();
    assert!(IndexedMerkleTree::verify_auth_path(
        pos,
        &leaf,
        &path,
        root,
        DEFAULT_DEPTH
    ));
}

#[test]
fn test_imt_non_membership_proof() {
    let mut tree = IndexedMerkleTree::new();
    let policy = test_policy();
    tree.insert(Fq::from(100u64), &policy).unwrap();
    tree.insert(Fq::from(300u64), &policy).unwrap();

    let value = Fq::from(200u64);
    let (pos, leaf, path) = tree.non_membership_proof(value).unwrap();

    assert_eq!(leaf.value, Fq::from(100u64));
    assert!(leaf.value < value);
    assert!(value < leaf.next_value);

    let root = tree.root();
    assert!(IndexedMerkleTree::verify_auth_path(
        pos,
        &leaf,
        &path,
        root,
        DEFAULT_DEPTH
    ));
}

#[test]
fn test_imt_non_membership_empty_tree() {
    let tree = IndexedMerkleTree::new();
    let value = Fq::from(12345u64);
    let (pos, leaf, path) = tree.non_membership_proof(value).unwrap();

    assert_eq!(pos, 0);
    assert_eq!(leaf.value, Fq::from(0u64));
    assert!(leaf.value < value);
    assert!(value < leaf.next_value);

    let root = tree.root();
    assert!(IndexedMerkleTree::verify_auth_path(
        pos,
        &leaf,
        &path,
        root,
        DEFAULT_DEPTH
    ));
}

#[test]
fn test_imt_cannot_insert_duplicate() {
    let mut tree = IndexedMerkleTree::new();
    let value = Fq::from(100u64);

    tree.insert(value, &test_policy()).unwrap();
    let result = tree.insert(value, &test_policy());
    assert!(result.is_err());
}

#[test]
fn test_imt_cannot_insert_zero() {
    let mut tree = IndexedMerkleTree::new();
    let result = tree.insert(Fq::from(0u64), &test_policy());
    assert!(result.is_err());
}

#[test]
fn test_imt_serialization_json() {
    let mut tree = IndexedMerkleTree::new();
    let policy = test_policy();
    tree.insert(Fq::from(100u64), &policy).unwrap();
    tree.insert(Fq::from(200u64), &policy).unwrap();

    let serialized = serde_json::to_string(&tree).expect("serialization failed");
    let deserialized: IndexedMerkleTree =
        serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(tree.root().0, deserialized.root().0);
    assert_eq!(tree.leaf_count(), deserialized.leaf_count());
    assert!(deserialized.contains(Fq::from(100u64)));
    assert!(deserialized.contains(Fq::from(200u64)));
}

#[test]
fn test_imt_serialization_bincode() {
    let mut tree = IndexedMerkleTree::new();
    let policy = test_policy();
    tree.insert(Fq::from(100u64), &policy).unwrap();
    tree.insert(Fq::from(200u64), &policy).unwrap();

    let serialized = bincode::serialize(&tree).expect("bincode serialization failed");
    let deserialized: IndexedMerkleTree =
        bincode::deserialize(&serialized).expect("bincode deserialization failed");

    assert_eq!(tree.root().0, deserialized.root().0);
    assert_eq!(tree.leaf_count(), deserialized.leaf_count());
    assert!(deserialized.contains(Fq::from(100u64)));
    assert!(deserialized.contains(Fq::from(200u64)));

    let (pos1, leaf1, path1) = tree.non_membership_proof(Fq::from(999u64)).unwrap();
    let (pos2, leaf2, path2) = deserialized.non_membership_proof(Fq::from(999u64)).unwrap();
    assert_eq!(pos1, pos2);
    assert_eq!(leaf1.value, leaf2.value);
    assert_eq!(leaf1.next_index, leaf2.next_index);
    assert_eq!(leaf1.next_value, leaf2.next_value);
    assert_eq!(path1.len(), path2.len());

    assert!(IndexedMerkleTree::verify_auth_path(
        pos2,
        &leaf2,
        &path2,
        deserialized.root(),
        DEFAULT_DEPTH
    ));
}

#[test]
fn test_imt_root_changes_on_insert() {
    let mut tree = IndexedMerkleTree::new();
    let policy = test_policy();
    let root1 = tree.root();

    tree.insert(Fq::from(100u64), &policy).unwrap();
    let root2 = tree.root();

    tree.insert(Fq::from(200u64), &policy).unwrap();
    let root3 = tree.root();

    assert_ne!(root1.0, root2.0);
    assert_ne!(root2.0, root3.0);
    assert_ne!(root1.0, root3.0);
}

#[test]
fn test_imt_membership_fails_for_missing() {
    let tree = IndexedMerkleTree::new();
    let result = tree.membership_proof(Fq::from(100u64));
    assert!(result.is_err());
}

#[test]
fn test_imt_non_membership_fails_for_existing() {
    let mut tree = IndexedMerkleTree::new();
    tree.insert(Fq::from(100u64), &test_policy()).unwrap();

    let result = tree.non_membership_proof(Fq::from(100u64));
    assert!(result.is_err());
}

#[test]
fn test_imt_with_custom_depth() {
    let mut tree = IndexedMerkleTree::with_depth(4);
    assert_eq!(tree.depth(), 4);

    tree.insert(Fq::from(100u64), &test_policy()).unwrap();
    let (_, leaf, path) = tree.membership_proof(Fq::from(100u64)).unwrap();
    assert_eq!(path.len(), 4);

    let root = tree.root();
    assert!(IndexedMerkleTree::verify_auth_path(
        1, &leaf, &path, root, 4
    ));
}

#[test]
fn test_imt_find_low_leaf_edge_cases() {
    let mut tree = IndexedMerkleTree::new();
    let policy = test_policy();
    tree.insert(Fq::from(100u64), &policy).unwrap();
    tree.insert(Fq::from(200u64), &policy).unwrap();

    let (pos, leaf) = tree.find_low_leaf(Fq::from(50u64)).unwrap();
    assert_eq!(pos, 0);
    assert_eq!(leaf.value, Fq::from(0u64));

    let (_, leaf) = tree.find_low_leaf(Fq::from(150u64)).unwrap();
    assert_eq!(leaf.value, Fq::from(100u64));

    let (_, leaf) = tree.find_low_leaf(Fq::from(100u64)).unwrap();
    assert_eq!(leaf.value, Fq::from(100u64));
}

#[test]
fn test_fq_ord_key_numeric_order() {
    let ordered = [
        Fq::from(0u64),
        Fq::from(1u64),
        Fq::from(2u64),
        Fq::from(10u64),
        Fq::from(u64::MAX),
        *FQ_MAX,
    ];

    for window in ordered.windows(2) {
        assert!(FqOrdKey::from(window[0]) < FqOrdKey::from(window[1]));
    }
}

#[test]
fn test_imt_predecessor_index_matches_linear_scan() {
    let mut tree = IndexedMerkleTree::new();
    let policy = test_policy();
    for value in [400u64, 100, 900, 250, 700] {
        tree.insert(Fq::from(value), &policy).unwrap();
    }

    let linear_low_leaf = |target: Fq| -> Option<(u64, IndexedLeaf)> {
        if let Some(&pos) = tree.value_index.get(&target.to_bytes()) {
            return tree.leaves.get(&pos).cloned().map(|leaf| (pos, leaf));
        }
        let target_key = FqOrdKey::from(target);
        tree.leaves
            .iter()
            .find(|(_, leaf)| {
                FqOrdKey::from(leaf.value) < target_key
                    && target_key < FqOrdKey::from(leaf.next_value)
            })
            .map(|(&pos, leaf)| (pos, leaf.clone()))
    };

    for target in [
        Fq::from(1u64),
        Fq::from(100u64),
        Fq::from(101u64),
        Fq::from(399u64),
        Fq::from(700u64),
        Fq::from(800u64),
        *FQ_MAX - Fq::from(1u64),
    ] {
        let indexed = tree.find_low_leaf(target).unwrap();
        let linear = linear_low_leaf(target).unwrap();
        assert_eq!(indexed.0, linear.0);
        assert_eq!(indexed.1.value, linear.1.value);
        assert_eq!(indexed.1.next_value, linear.1.next_value);
    }

    let mut changed_policy = test_policy();
    changed_policy.params.daily_volume_limit = 42;
    tree.update_policy(Fq::from(400u64), &changed_policy)
        .unwrap();
    let (_, leaf) = tree.find_low_leaf(Fq::from(450u64)).unwrap();
    assert_eq!(leaf.value, Fq::from(400u64));
    assert_eq!(leaf.daily_volume_limit(), 42);
}

#[test]
fn test_fq_max_is_field_modulus_minus_one() {
    let fq_max = *FQ_MAX;
    let fq_max_plus_one = fq_max + Fq::from(1u64);
    assert_eq!(fq_max_plus_one, Fq::from(0u64));
    assert!(fq_less_than(&Fq::from(0u64), &fq_max));
    assert!(fq_less_than(&Fq::from(u64::MAX), &fq_max));
}

#[test]
fn test_non_membership_near_fq_max() {
    let tree = IndexedMerkleTree::new();
    let near_max = *FQ_MAX - Fq::from(1u64);
    let (pos, leaf, path) = tree.non_membership_proof(near_max).unwrap();

    assert_eq!(pos, 0);
    assert_eq!(leaf.value, Fq::from(0u64));
    assert_eq!(leaf.next_value, *FQ_MAX);
    assert!(fq_less_than(&leaf.value, &near_max));
    assert!(fq_less_than(&near_max, &leaf.next_value));

    let root = tree.root();
    assert!(IndexedMerkleTree::verify_auth_path(
        pos,
        &leaf,
        &path,
        root,
        DEFAULT_DEPTH
    ));
}

#[test]
fn test_fq_max_no_non_membership_proof() {
    let tree = IndexedMerkleTree::new();
    let result = tree.non_membership_proof(*FQ_MAX);
    assert!(result.is_err());
}

#[test]
fn test_sentinel_covers_full_range() {
    let tree = IndexedMerkleTree::new();
    let sentinel = tree.get_leaf(0).unwrap();

    assert_eq!(sentinel.value, Fq::from(0u64));
    assert_eq!(sentinel.next_value, *FQ_MAX);

    for v in [1u64, 1000, u64::MAX / 2, u64::MAX] {
        let value = Fq::from(v);
        let result = tree.non_membership_proof(value);
        assert!(result.is_ok(), "Should have proof for value {}", v);
    }
}

#[test]
fn test_indexed_leaf_proto_roundtrip() {
    let leaf = IndexedLeaf::with_default_policy(Fq::from(100u64), 2, Fq::from(200u64));

    let proto: pb::IndexedLeafData = leaf.clone().into();
    let back = IndexedLeaf::try_from(proto).unwrap();

    assert_eq!(back.value, leaf.value);
    assert_eq!(back.next_index, leaf.next_index);
    assert_eq!(back.next_value, leaf.next_value);
    assert_eq!(back.params.dk_pub, leaf.params.dk_pub);
    assert_eq!(
        back.params.daily_volume_limit,
        leaf.params.daily_volume_limit
    );
    assert_eq!(back.params.route_policy_hash, leaf.params.route_policy_hash);
    assert_eq!(back.ring.ring_pk, leaf.ring.ring_pk);
    assert_eq!(back.ring.ring_id_hash, leaf.ring.ring_id_hash);
}

#[test]
fn indexed_leaf_proto_rejects_omitted_policy_facts_and_invalid_points() {
    let leaf = IndexedLeaf::with_default_policy(Fq::from(100u64), 2, Fq::from(200u64));
    let proto: pb::IndexedLeafData = leaf.into();
    let mutations: [(&str, fn(&mut pb::IndexedLeafData)); 10] = [
        ("dk_pub", |p| p.dk_pub.clear()),
        ("daily_volume_limit", |p| p.daily_volume_limit.clear()),
        ("route_policy_hash", |p| p.route_policy_hash.clear()),
        ("ring_pk", |p| p.ring_pk.clear()),
        ("ring_id_hash", |p| p.ring_id_hash.clear()),
        ("policy_id_hash", |p| p.policy_id_hash.clear()),
        ("permission_hash", |p| p.permission_hash.clear()),
        ("resource_hash", |p| p.resource_hash.clear()),
        ("invalid dk_pub", |p| p.dk_pub.fill(0xff)),
        ("invalid ring_pk", |p| p.ring_pk.fill(0xff)),
    ];

    for (label, mutate) in mutations {
        let mut candidate = proto.clone();
        mutate(&mut candidate);
        assert!(
            IndexedLeaf::try_from(candidate).is_err(),
            "indexed leaf {label} alias must fail"
        );
    }
}

#[test]
fn test_indexed_leaf_bincode_roundtrip() {
    let leaf = IndexedLeaf::with_default_policy(Fq::from(100u64), 2, Fq::from(200u64));

    let bytes = bincode::serialize(&leaf).unwrap();
    let back: IndexedLeaf = bincode::deserialize(&bytes).unwrap();

    assert_eq!(back.value, leaf.value);
    assert_eq!(back.next_index, leaf.next_index);
    assert_eq!(back.next_value, leaf.next_value);
    assert_eq!(back.params, leaf.params);
    assert_eq!(back.ring, leaf.ring);
}

#[test]
fn test_insert_result_contains_correct_data() {
    let mut tree = IndexedMerkleTree::with_depth(4);
    let policy = test_policy();

    let result = tree.insert(Fq::from(100u64), &policy).unwrap();

    assert_eq!(result.indexed_leaf.value, Fq::from(100u64));
    assert_eq!(result.updated_low_leaf.value, Fq::from(0u64));
    assert_eq!(result.updated_low_leaf.next_index, 1);
    assert_eq!(result.updated_low_leaf.next_value, Fq::from(100u64));

    let stored_new_leaf = tree.get_leaf(result.position).unwrap();
    assert_eq!(*stored_new_leaf, result.indexed_leaf);

    let stored_low_leaf = tree.get_leaf(result.low_leaf_position).unwrap();
    assert_eq!(*stored_low_leaf, result.updated_low_leaf);
}

#[test]
fn test_string_to_fq_deterministic() {
    let a = string_to_fq("hello");
    let b = string_to_fq("hello");
    assert_eq!(a, b);

    let c = string_to_fq("world");
    assert_ne!(a, c);
}

#[test]
fn test_route_policy_to_fq_order_independent() {
    use crate::structs::IbcRoute;
    let mut a = AssetPolicy::default_unregulated().params;
    a.allowed_ibc_routes = vec![
        IbcRoute::transfer("channel-0", "connection-0", "channel-7"),
        IbcRoute::transfer("channel-1", "connection-1", "channel-8"),
    ];
    let mut b = a.clone();
    b.allowed_ibc_routes.reverse();
    assert_eq!(route_policy_to_fq(&a), route_policy_to_fq(&b));
}

#[test]
fn test_leaf_commit_includes_policy() {
    let leaf1 = IndexedLeaf::with_default_policy(Fq::from(100u64), 0, *FQ_MAX);

    // Same structural values but different policy
    let leaf2 = IndexedLeaf {
        value: Fq::from(100u64),
        next_index: 0,
        next_value: *FQ_MAX,
        params: LeafParams {
            dk_pub: decaf377::Element::default(),
            daily_volume_limit: 1000u128,
            route_policy_hash: string_to_fq(""),
        },
        ring: LeafRing::default(),
    };

    assert_ne!(
        leaf1.commit().0,
        leaf2.commit().0,
        "Different policy should produce different commitment"
    );
}

#[test]
fn test_leaf_from_policy() {
    let policy = AssetPolicy::default_unregulated();
    let leaf = IndexedLeaf::from_policy(Fq::from(42u64), 0, *FQ_MAX, &policy);

    assert_eq!(leaf.value, Fq::from(42u64));
    assert_eq!(leaf.params.dk_pub, *crate::crypto::UNREGULATED_SINK_DK_PUB);
    assert_eq!(leaf.params.daily_volume_limit, u128::MAX);
    assert_eq!(leaf.ring.ring_pk, *crate::crypto::UNREGULATED_SINK_RING_PK);
}
