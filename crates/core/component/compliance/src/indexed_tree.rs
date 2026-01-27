use anyhow::{anyhow, bail, Result};
use ark_ff::PrimeField;
use blake2b_simd::Params;
use decaf377::Fq;
use once_cell::sync::Lazy;
use penumbra_sdk_proto::{core::component::compliance::v1 as pb, DomainType};
use penumbra_sdk_tct::StateCommitment;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::tree::DEFAULT_DEPTH;

/// Compare two Fq values numerically using their BigInteger representation.
/// This is faster than using PartialOrd on Fq directly in debug builds.
#[inline]
fn fq_less_than(a: &Fq, b: &Fq) -> bool {
    let a_bigint = a.into_bigint();
    let b_bigint = b.into_bigint();
    a_bigint < b_bigint
}

/// Domain separator for IMT leaf commitments.
/// Uses blake2b personalization with max 16 bytes.
pub static IMT_LEAF_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    let hash = Params::default()
        .personal(b"pen.imt.leaf____") // Exactly 16 bytes
        .hash(b"");
    Fq::from_le_bytes_mod_order(hash.as_bytes())
});

/// The maximum value representable in the field (modulus - 1).
/// Used as the sentinel's next_value to cover the entire range.
/// In a prime field, -1 = p - 1 where p is the modulus.
pub static FQ_MAX: Lazy<Fq> = Lazy::new(|| Fq::from(0u64) - Fq::from(1u64));

/// Precomputed zero hashes for each level of the IMT.
/// These use the IMT leaf domain separator for level 0.
pub static IMT_ZERO_HASHES: Lazy<Vec<StateCommitment>> = Lazy::new(|| {
    let mut zeros = Vec::with_capacity((DEFAULT_DEPTH + 1) as usize);

    // Level 0: empty leaf hash using IMT domain separator
    let empty_leaf_hash = poseidon377::hash_4(
        &*IMT_LEAF_DOMAIN_SEP,
        (
            Fq::from(0u64),
            Fq::from(0u64),
            Fq::from(0u64),
            Fq::from(0u64),
        ),
    );
    zeros.push(StateCommitment(empty_leaf_hash));

    // Compute zero hashes for each level up to DEFAULT_DEPTH
    for i in 1..=(DEFAULT_DEPTH as usize) {
        let prev = zeros[i - 1].0;
        let hash = poseidon377::hash_4(&Fq::from(0u64), (prev, prev, prev, prev));
        zeros.push(StateCommitment(hash));
    }

    zeros
});

/// A leaf in the Indexed Merkle Tree forming a sorted linked list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedLeaf {
    /// The value stored in this leaf (e.g., asset_id).
    pub value: Fq,
    /// Position in the tree of the next-higher value leaf.
    pub next_index: u64,
    /// The value at next_index (for efficient gap verification).
    pub next_value: Fq,
}

/// Result of an IMT insertion with full data for client sync.
#[derive(Debug, Clone)]
pub struct InsertResult {
    pub position: u64,
    pub indexed_leaf: IndexedLeaf,
    pub low_leaf_position: u64,
    pub updated_low_leaf: IndexedLeaf,
}

impl IndexedLeaf {
    /// Compute the commitment for this leaf.
    pub fn commit(&self) -> StateCommitment {
        let hash = poseidon377::hash_4(
            &*IMT_LEAF_DOMAIN_SEP,
            (
                self.value,
                Fq::from(self.next_index),
                self.next_value,
                Fq::from(0u64),
            ),
        );
        StateCommitment(hash)
    }
}

/// Serialization helper for IndexedLeaf.
#[derive(Serialize, Deserialize)]
struct IndexedLeafSerde {
    value: [u8; 32],
    next_index: u64,
    next_value: [u8; 32],
}

impl Serialize for IndexedLeaf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let helper = IndexedLeafSerde {
            value: self.value.to_bytes(),
            next_index: self.next_index,
            next_value: self.next_value.to_bytes(),
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IndexedLeaf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = IndexedLeafSerde::deserialize(deserializer)?;
        let value = Fq::from_bytes_checked(&helper.value)
            .map_err(|_| serde::de::Error::custom("invalid value Fq bytes"))?;
        let next_value = Fq::from_bytes_checked(&helper.next_value)
            .map_err(|_| serde::de::Error::custom("invalid next_value Fq bytes"))?;
        Ok(IndexedLeaf {
            value,
            next_index: helper.next_index,
            next_value,
        })
    }
}

// Proto conversion implementations for IndexedLeaf
impl DomainType for IndexedLeaf {
    type Proto = pb::IndexedLeafData;
}

impl From<IndexedLeaf> for pb::IndexedLeafData {
    fn from(leaf: IndexedLeaf) -> Self {
        pb::IndexedLeafData {
            value: leaf.value.to_bytes().to_vec(),
            next_index: leaf.next_index,
            next_value: leaf.next_value.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::IndexedLeafData> for IndexedLeaf {
    type Error = anyhow::Error;

    fn try_from(proto: pb::IndexedLeafData) -> Result<Self> {
        let value_bytes: [u8; 32] = proto.value.try_into().map_err(|v: Vec<u8>| {
            anyhow!("IndexedLeaf proto: value must be 32 bytes, got {}", v.len())
        })?;
        let next_value_bytes: [u8; 32] = proto.next_value.try_into().map_err(|v: Vec<u8>| {
            anyhow!(
                "IndexedLeaf proto: next_value must be 32 bytes, got {}",
                v.len()
            )
        })?;

        Ok(IndexedLeaf {
            value: Fq::from_bytes_checked(&value_bytes)
                .map_err(|e| anyhow!("IndexedLeaf proto: invalid value field element: {}", e))?,
            next_index: proto.next_index,
            next_value: Fq::from_bytes_checked(&next_value_bytes).map_err(|e| {
                anyhow!("IndexedLeaf proto: invalid next_value field element: {}", e)
            })?,
        })
    }
}

/// An Indexed Merkle Tree (IMT) for the asset registry.
///
/// Only regulated assets are stored. Unregulated status is proven via
/// non-membership proofs (the asset falls in a "gap" between two adjacent leaves).
#[derive(Clone, Debug)]
pub struct IndexedMerkleTree {
    depth: u8,
    /// Internal node hashes. Key format: level << 48 | position
    nodes: BTreeMap<u64, StateCommitment>,
    /// Leaf data at each position.
    leaves: BTreeMap<u64, IndexedLeaf>,
    /// Reverse index: value -> position for O(1) lookup.
    value_index: BTreeMap<[u8; 32], u64>,
    /// Current number of leaves (including sentinel).
    leaf_count: u64,
}

/// Serialization helper for IndexedMerkleTree.
#[derive(Serialize, Deserialize)]
struct IndexedMerkleTreeSerde {
    depth: u8,
    nodes: Vec<(u64, [u8; 32])>,
    leaves: Vec<(u64, IndexedLeafSerde)>,
    value_index: Vec<([u8; 32], u64)>,
    leaf_count: u64,
}

impl Serialize for IndexedMerkleTree {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let nodes: Vec<(u64, [u8; 32])> = self
            .nodes
            .iter()
            .map(|(k, v)| (*k, v.0.to_bytes()))
            .collect();

        let leaves: Vec<(u64, IndexedLeafSerde)> = self
            .leaves
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    IndexedLeafSerde {
                        value: v.value.to_bytes(),
                        next_index: v.next_index,
                        next_value: v.next_value.to_bytes(),
                    },
                )
            })
            .collect();

        let value_index: Vec<([u8; 32], u64)> =
            self.value_index.iter().map(|(k, v)| (*k, *v)).collect();

        let helper = IndexedMerkleTreeSerde {
            depth: self.depth,
            nodes,
            leaves,
            value_index,
            leaf_count: self.leaf_count,
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IndexedMerkleTree {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = IndexedMerkleTreeSerde::deserialize(deserializer)?;

        let nodes: BTreeMap<u64, StateCommitment> = helper
            .nodes
            .into_iter()
            .map(|(k, bytes)| {
                let fq = Fq::from_bytes_checked(&bytes)
                    .map_err(|_| serde::de::Error::custom("invalid node Fq bytes"))?;
                Ok((k, StateCommitment(fq)))
            })
            .collect::<Result<_, D::Error>>()?;

        let leaves: BTreeMap<u64, IndexedLeaf> = helper
            .leaves
            .into_iter()
            .map(|(k, v)| {
                let value = Fq::from_bytes_checked(&v.value)
                    .map_err(|_| serde::de::Error::custom("invalid leaf value Fq bytes"))?;
                let next_value = Fq::from_bytes_checked(&v.next_value)
                    .map_err(|_| serde::de::Error::custom("invalid leaf next_value Fq bytes"))?;
                Ok((
                    k,
                    IndexedLeaf {
                        value,
                        next_index: v.next_index,
                        next_value,
                    },
                ))
            })
            .collect::<Result<_, D::Error>>()?;

        let value_index: BTreeMap<[u8; 32], u64> = helper.value_index.into_iter().collect();

        Ok(IndexedMerkleTree {
            depth: helper.depth,
            nodes,
            leaves,
            value_index,
            leaf_count: helper.leaf_count,
        })
    }
}

impl IndexedMerkleTree {
    /// Create a new IMT with the low sentinel at position 0.
    ///
    /// The sentinel has `value = 0, next_index = 0, next_value = MAX`.
    /// This covers the entire range, so any asset_id falls in the "gap" initially.
    pub fn new() -> Self {
        let mut tree = Self {
            depth: DEFAULT_DEPTH,
            nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            value_index: BTreeMap::new(),
            leaf_count: 0,
        };

        // Create the low sentinel
        let sentinel = IndexedLeaf {
            value: Fq::from(0u64),
            next_index: 0, // Points to itself (end of list)
            next_value: *FQ_MAX,
        };

        // Insert sentinel at position 0
        tree.leaves.insert(0, sentinel.clone());
        tree.value_index.insert(Fq::from(0u64).to_bytes(), 0);
        tree.leaf_count = 1;

        // Update the tree hashes
        let commitment = tree.leaves.get(&0).unwrap().commit();
        tree.update_path(0, commitment);

        tree
    }

    /// Create a new IMT with a custom depth.
    ///
    /// # Panics
    /// Panics if depth > DEFAULT_DEPTH (16), as this would exceed precomputed zero hashes
    /// and cause overflow in shift operations (depth * 2 must be < 64).
    pub fn with_depth(depth: u8) -> Self {
        assert!(
            depth <= DEFAULT_DEPTH,
            "depth {} exceeds maximum of {}",
            depth,
            DEFAULT_DEPTH
        );
        let mut tree = Self {
            depth,
            nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            value_index: BTreeMap::new(),
            leaf_count: 0,
        };

        let sentinel = IndexedLeaf {
            value: Fq::from(0u64),
            next_index: 0,
            next_value: *FQ_MAX,
        };

        tree.leaves.insert(0, sentinel.clone());
        tree.value_index.insert(Fq::from(0u64).to_bytes(), 0);
        tree.leaf_count = 1;

        let commitment = tree.leaves.get(&0).unwrap().commit();
        tree.update_path(0, commitment);

        tree
    }

    #[inline]
    fn node_key(level: u8, position: u64) -> u64 {
        ((level as u64) << 48) | position
    }

    /// Compute max leaves safely, avoiding overflow in shift operations.
    #[inline]
    fn max_leaves_for_depth(depth: u8) -> u64 {
        debug_assert!(depth <= 31, "depth must be <= 31 to avoid shift overflow");
        1u64 << ((depth as u32) * 2)
    }

    fn get_node(&self, level: u8, position: u64) -> StateCommitment {
        let key = Self::node_key(level, position);
        self.nodes
            .get(&key)
            .copied()
            .unwrap_or_else(|| IMT_ZERO_HASHES[level as usize])
    }

    fn set_node(&mut self, level: u8, position: u64, hash: StateCommitment) {
        let key = Self::node_key(level, position);
        if hash.0 != IMT_ZERO_HASHES[level as usize].0 {
            self.nodes.insert(key, hash);
        } else {
            self.nodes.remove(&key);
        }
    }

    fn hash_children(
        child0: StateCommitment,
        child1: StateCommitment,
        child2: StateCommitment,
        child3: StateCommitment,
    ) -> StateCommitment {
        let hash = poseidon377::hash_4(&Fq::from(0u64), (child0.0, child1.0, child2.0, child3.0));
        StateCommitment(hash)
    }

    /// Update the path from a leaf position to the root.
    fn update_path(&mut self, position: u64, leaf_hash: StateCommitment) {
        self.set_node(0, position, leaf_hash);

        let mut current_position = position;
        for level in 0..self.depth {
            let parent_position = current_position / 4;
            let base_position = parent_position * 4;

            let child0 = self.get_node(level, base_position);
            let child1 = self.get_node(level, base_position + 1);
            let child2 = self.get_node(level, base_position + 2);
            let child3 = self.get_node(level, base_position + 3);

            let parent_hash = Self::hash_children(child0, child1, child2, child3);
            self.set_node(level + 1, parent_position, parent_hash);

            current_position = parent_position;
        }
    }

    /// Find the "low leaf" for a given value.
    ///
    /// The low leaf is the leaf where `low.value < target <= low.next_value`.
    /// For non-membership proofs, we need `low.value < target < low.next_value`.
    ///
    /// Uses direct iteration over leaves (small number expected in practice).
    pub fn find_low_leaf(&self, target: Fq) -> Option<(u64, IndexedLeaf)> {
        // For exact match, use the index
        if let Some(&pos) = self.value_index.get(&target.to_bytes()) {
            let leaf = self.leaves.get(&pos)?;
            return Some((pos, leaf.clone()));
        }

        // For gap search, iterate through leaves to find one where target is in range
        // This is O(n) but n is small (number of regulated assets)
        for (&pos, leaf) in &self.leaves {
            // Check if target falls in this leaf's gap: leaf.value < target < leaf.next_value
            // Use fq_less_than for faster comparison in debug builds
            if fq_less_than(&leaf.value, &target) && fq_less_than(&target, &leaf.next_value) {
                return Some((pos, leaf.clone()));
            }
        }

        None
    }

    /// Check if a value exists in the tree.
    pub fn contains(&self, value: Fq) -> bool {
        self.value_index.contains_key(&value.to_bytes())
    }

    /// Get the position of a value in the tree if it exists.
    pub fn get_position(&self, value: Fq) -> Option<u64> {
        self.value_index.get(&value.to_bytes()).copied()
    }

    /// Get the leaf at a given position.
    pub fn get_leaf(&self, position: u64) -> Option<&IndexedLeaf> {
        self.leaves.get(&position)
    }

    /// Insert a new value into the tree.
    ///
    /// Returns the position of the new leaf.
    pub fn insert(&mut self, value: Fq) -> Result<u64> {
        let result = self.insert_with_data(value)?;
        Ok(result.position)
    }

    /// Insert a new value and return full data for client sync.
    pub fn insert_with_data(&mut self, value: Fq) -> Result<InsertResult> {
        // Check if already exists
        if self.contains(value) {
            bail!(
                "IMT insert failed: value {:?} already exists at position {:?}",
                value.to_bytes(),
                self.get_position(value)
            );
        }

        // Don't allow inserting zero (reserved for sentinel)
        if value == Fq::from(0u64) {
            bail!("IMT insert failed: zero value is reserved for sentinel leaf");
        }

        // Find the low leaf
        let (low_pos, low_leaf) = self.find_low_leaf(value).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT insert failed: could not find low leaf for value {:?} (tree has {} leaves)",
                value.to_bytes(),
                self.leaf_count
            )
        })?;

        // Ensure this is a gap (non-membership)
        if low_leaf.value == value {
            bail!(
                "IMT insert failed: value {:?} already exists (exact match at position {})",
                value.to_bytes(),
                low_pos
            );
        }

        // Check tree capacity
        let max_leaves = Self::max_leaves_for_depth(self.depth);
        if self.leaf_count >= max_leaves {
            bail!(
                "IMT insert failed: tree is full ({}/{} leaves, depth {})",
                self.leaf_count,
                max_leaves,
                self.depth
            );
        }

        let new_pos = self.leaf_count;

        // Create new leaf
        let new_leaf = IndexedLeaf {
            value,
            next_index: low_leaf.next_index,
            next_value: low_leaf.next_value,
        };

        // Update low leaf to point to new leaf
        let updated_low_leaf = IndexedLeaf {
            value: low_leaf.value,
            next_index: new_pos,
            next_value: value,
        };

        // Store the new leaf
        self.leaves.insert(new_pos, new_leaf.clone());
        self.value_index.insert(value.to_bytes(), new_pos);
        self.leaf_count += 1;

        // Update the low leaf
        self.leaves.insert(low_pos, updated_low_leaf.clone());

        // Update tree hashes for both modified leaves
        let new_leaf_commitment = new_leaf.commit();
        self.update_path(new_pos, new_leaf_commitment);

        let updated_low_commitment = updated_low_leaf.commit();
        self.update_path(low_pos, updated_low_commitment);

        Ok(InsertResult {
            position: new_pos,
            indexed_leaf: new_leaf,
            low_leaf_position: low_pos,
            updated_low_leaf,
        })
    }

    /// Get the authentication path for a position.
    pub fn auth_path(&self, position: u64) -> Result<Vec<[StateCommitment; 3]>> {
        let max_leaves = Self::max_leaves_for_depth(self.depth);
        if position >= max_leaves {
            bail!(
                "Position {} exceeds maximum leaves {} for depth {}",
                position,
                max_leaves,
                self.depth
            );
        }

        let mut path = Vec::with_capacity(self.depth as usize);
        let mut current_position = position;

        for level in 0..self.depth {
            let child_index = (current_position % 4) as usize;
            let base_position = (current_position / 4) * 4;

            let children = [
                self.get_node(level, base_position),
                self.get_node(level, base_position + 1),
                self.get_node(level, base_position + 2),
                self.get_node(level, base_position + 3),
            ];

            let siblings = match child_index {
                0 => [children[1], children[2], children[3]],
                1 => [children[0], children[2], children[3]],
                2 => [children[0], children[1], children[3]],
                3 => [children[0], children[1], children[2]],
                _ => unreachable!(),
            };

            path.push(siblings);
            current_position /= 4;
        }

        Ok(path)
    }

    /// Get a membership proof for a value that exists in the tree.
    pub fn membership_proof(
        &self,
        value: Fq,
    ) -> Result<(u64, IndexedLeaf, Vec<[StateCommitment; 3]>)> {
        let position = self.value_index.get(&value.to_bytes()).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT membership proof failed: value {:?} not found in tree (tree has {} leaves)",
                value.to_bytes(),
                self.leaf_count
            )
        })?;

        let leaf = self
            .leaves
            .get(position)
            .ok_or_else(|| {
                anyhow::anyhow!(
                "IMT internal error: leaf not found at position {} (index exists but leaf missing)",
                position
            )
            })?
            .clone();

        let path = self.auth_path(*position)?;

        Ok((*position, leaf, path))
    }

    /// Get a non-membership proof for a value that does NOT exist in the tree.
    ///
    /// Returns the low leaf that proves the value falls in a gap.
    pub fn non_membership_proof(
        &self,
        value: Fq,
    ) -> Result<(u64, IndexedLeaf, Vec<[StateCommitment; 3]>)> {
        if self.contains(value) {
            bail!(
                "IMT non-membership proof failed: value {:?} exists in tree at position {:?}",
                value.to_bytes(),
                self.get_position(value)
            );
        }

        let (low_pos, low_leaf) = self.find_low_leaf(value).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT non-membership proof failed: could not find gap for value {:?} (tree has {} leaves)",
                value.to_bytes(),
                self.leaf_count
            )
        })?;

        // Verify it's actually a gap
        if low_leaf.value >= value || value >= low_leaf.next_value {
            bail!(
                "IMT non-membership proof failed: value {:?} not in gap [{:?}, {:?})",
                value.to_bytes(),
                low_leaf.value.to_bytes(),
                low_leaf.next_value.to_bytes()
            );
        }

        let path = self.auth_path(low_pos)?;

        Ok((low_pos, low_leaf, path))
    }

    /// Get the root hash of the tree.
    pub fn root(&self) -> StateCommitment {
        self.get_node(self.depth, 0)
    }

    /// Get the depth of the tree.
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Get the number of leaves in the tree (including sentinel).
    pub fn leaf_count(&self) -> u64 {
        self.leaf_count
    }

    /// Verify an authentication path.
    pub fn verify_auth_path(
        position: u64,
        leaf: &IndexedLeaf,
        auth_path: &[[StateCommitment; 3]],
        expected_root: StateCommitment,
        depth: u8,
    ) -> bool {
        let mut current_hash = leaf.commit();
        let mut current_position = position;

        for siblings in auth_path.iter().take(depth as usize) {
            let child_index = (current_position % 4) as usize;

            let children = match child_index {
                0 => [current_hash, siblings[0], siblings[1], siblings[2]],
                1 => [siblings[0], current_hash, siblings[1], siblings[2]],
                2 => [siblings[0], siblings[1], current_hash, siblings[2]],
                3 => [siblings[0], siblings[1], siblings[2], current_hash],
                _ => unreachable!(),
            };

            current_hash = Self::hash_children(children[0], children[1], children[2], children[3]);
            current_position /= 4;
        }

        current_hash.0 == expected_root.0
    }
}

impl Default for IndexedMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imt_new_has_sentinel() {
        let tree = IndexedMerkleTree::new();
        assert_eq!(tree.leaf_count(), 1);

        let sentinel = tree.get_leaf(0).expect("Sentinel should exist");
        assert_eq!(sentinel.value, Fq::from(0u64));
        assert_eq!(sentinel.next_index, 0);
        assert_eq!(sentinel.next_value, *FQ_MAX);
    }

    #[test]
    fn test_imt_insert_single() {
        let mut tree = IndexedMerkleTree::new();
        let value = Fq::from(100u64);

        let pos = tree.insert(value).unwrap();
        assert_eq!(pos, 1);
        assert_eq!(tree.leaf_count(), 2);
        assert!(tree.contains(value));

        // Check the new leaf
        let leaf = tree.get_leaf(1).unwrap();
        assert_eq!(leaf.value, value);
        assert_eq!(leaf.next_index, 0); // Points to sentinel (end of list)
        assert_eq!(leaf.next_value, *FQ_MAX);

        // Check sentinel was updated
        let sentinel = tree.get_leaf(0).unwrap();
        assert_eq!(sentinel.next_index, 1);
        assert_eq!(sentinel.next_value, value);
    }

    #[test]
    fn test_imt_insert_multiple_maintains_order() {
        // Use smaller depth for faster test execution
        let mut tree = IndexedMerkleTree::with_depth(4);

        // Insert in non-sorted order
        let values = [
            Fq::from(500u64),
            Fq::from(100u64),
            Fq::from(300u64),
            Fq::from(200u64),
        ];

        for v in values {
            tree.insert(v).unwrap();
        }

        // Verify sorted linked list by following from sentinel
        // We collect next_value from each leaf until we hit a leaf with next_value == FQ_MAX
        let mut collected = Vec::new();
        let mut pos = 0u64;
        loop {
            let leaf = tree.get_leaf(pos).unwrap();
            // Stop when we reach a leaf whose next_value is FQ_MAX (end of list)
            if leaf.next_value == *FQ_MAX {
                break;
            }
            collected.push(leaf.next_value);
            pos = leaf.next_index;
        }

        // Should be sorted
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
        tree.insert(value).unwrap();

        let (pos, leaf, path) = tree.membership_proof(value).unwrap();
        assert_eq!(pos, 1);
        assert_eq!(leaf.value, value);
        assert_eq!(path.len(), DEFAULT_DEPTH as usize);

        // Verify the path
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

        // Insert some values
        tree.insert(Fq::from(100u64)).unwrap();
        tree.insert(Fq::from(300u64)).unwrap();

        // Get non-membership proof for value in gap (200)
        let value = Fq::from(200u64);
        let (pos, leaf, path) = tree.non_membership_proof(value).unwrap();

        // The low leaf should be the one with value=100
        assert_eq!(leaf.value, Fq::from(100u64));
        assert!(leaf.value < value);
        assert!(value < leaf.next_value);

        // Verify the path
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

        // Any value should have a valid non-membership proof
        let value = Fq::from(12345u64);
        let (pos, leaf, path) = tree.non_membership_proof(value).unwrap();

        // Should use sentinel as low leaf
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

        tree.insert(value).unwrap();
        let result = tree.insert(value);
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_cannot_insert_zero() {
        let mut tree = IndexedMerkleTree::new();
        let result = tree.insert(Fq::from(0u64));
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_serialization_json() {
        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(100u64)).unwrap();
        tree.insert(Fq::from(200u64)).unwrap();

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
        tree.insert(Fq::from(100u64)).unwrap();
        tree.insert(Fq::from(200u64)).unwrap();

        let serialized = bincode::serialize(&tree).expect("bincode serialization failed");
        let deserialized: IndexedMerkleTree =
            bincode::deserialize(&serialized).expect("bincode deserialization failed");

        assert_eq!(
            tree.root().0,
            deserialized.root().0,
            "Root must match after bincode roundtrip"
        );
        assert_eq!(tree.leaf_count(), deserialized.leaf_count());
        assert!(deserialized.contains(Fq::from(100u64)));
        assert!(deserialized.contains(Fq::from(200u64)));

        // Verify proofs work on deserialized tree
        let (pos1, leaf1, path1) = tree.non_membership_proof(Fq::from(999u64)).unwrap();
        let (pos2, leaf2, path2) = deserialized.non_membership_proof(Fq::from(999u64)).unwrap();
        assert_eq!(pos1, pos2);
        assert_eq!(leaf1.value, leaf2.value);
        assert_eq!(leaf1.next_index, leaf2.next_index);
        assert_eq!(leaf1.next_value, leaf2.next_value);
        assert_eq!(path1.len(), path2.len());

        // Verify the proof actually verifies against the root
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
        let root1 = tree.root();

        tree.insert(Fq::from(100u64)).unwrap();
        let root2 = tree.root();

        tree.insert(Fq::from(200u64)).unwrap();
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
        tree.insert(Fq::from(100u64)).unwrap();

        let result = tree.non_membership_proof(Fq::from(100u64));
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_with_custom_depth() {
        let mut tree = IndexedMerkleTree::with_depth(4);
        assert_eq!(tree.depth(), 4);

        tree.insert(Fq::from(100u64)).unwrap();
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
        tree.insert(Fq::from(100u64)).unwrap();
        tree.insert(Fq::from(200u64)).unwrap();

        // Value less than first real value (but > 0)
        let (pos, leaf) = tree.find_low_leaf(Fq::from(50u64)).unwrap();
        assert_eq!(pos, 0); // Sentinel
        assert_eq!(leaf.value, Fq::from(0u64));

        // Value between 100 and 200
        let (_, leaf) = tree.find_low_leaf(Fq::from(150u64)).unwrap();
        assert_eq!(leaf.value, Fq::from(100u64));

        // Value exactly 100 (membership case)
        let (_, leaf) = tree.find_low_leaf(Fq::from(100u64)).unwrap();
        assert_eq!(leaf.value, Fq::from(100u64));
    }

    /// Test FQ_MAX is correctly computed as field modulus - 1
    #[test]
    fn test_fq_max_is_field_modulus_minus_one() {
        // FQ_MAX = 0 - 1 in field arithmetic = p - 1
        let fq_max = *FQ_MAX;

        // Adding 1 should wrap to 0
        let fq_max_plus_one = fq_max + Fq::from(1u64);
        assert_eq!(fq_max_plus_one, Fq::from(0u64));

        // FQ_MAX should be the largest value
        assert!(fq_less_than(&Fq::from(0u64), &fq_max));
        assert!(fq_less_than(&Fq::from(u64::MAX), &fq_max));
    }

    /// Test non-membership proof for value near FQ_MAX boundary
    #[test]
    fn test_non_membership_near_fq_max() {
        let tree = IndexedMerkleTree::new();

        // Value just below FQ_MAX should have valid non-membership proof
        let near_max = *FQ_MAX - Fq::from(1u64);
        let (pos, leaf, path) = tree.non_membership_proof(near_max).unwrap();

        // Should use sentinel (only leaf in empty tree)
        assert_eq!(pos, 0);
        assert_eq!(leaf.value, Fq::from(0u64));
        assert_eq!(leaf.next_value, *FQ_MAX);

        // Value must be in gap
        assert!(fq_less_than(&leaf.value, &near_max));
        assert!(fq_less_than(&near_max, &leaf.next_value));

        // Verify auth path
        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            pos,
            &leaf,
            &path,
            root,
            DEFAULT_DEPTH
        ));
    }

    /// Test that FQ_MAX itself cannot have a non-membership proof
    /// (it equals sentinel.next_value, so it's not strictly less than)
    #[test]
    fn test_fq_max_no_non_membership_proof() {
        let tree = IndexedMerkleTree::new();

        // FQ_MAX should fail non-membership proof because value >= next_value
        let result = tree.non_membership_proof(*FQ_MAX);
        assert!(result.is_err());
    }

    /// Test strict inequality: value must be < next_value, not <=
    #[test]
    fn test_strict_inequality_in_gap() {
        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(100u64)).unwrap();

        // Get the leaf for value 100
        let leaf = tree.get_leaf(1).unwrap();
        // leaf.next_value should be FQ_MAX

        // Trying to get non-membership proof for next_value should fail
        // because value >= next_value violates strict inequality
        let result = tree.non_membership_proof(leaf.next_value);
        assert!(result.is_err());
    }

    /// Test that fq_less_than works correctly for large values (> (p-1)/2)
    #[test]
    fn test_fq_less_than_large_values() {
        let half_modulus = *FQ_MAX / Fq::from(2u64);
        let large_a = half_modulus + Fq::from(1u64);
        let large_b = half_modulus + Fq::from(2u64);

        // Both values are > (p-1)/2
        assert!(fq_less_than(&half_modulus, &large_a));
        assert!(fq_less_than(&large_a, &large_b));
        assert!(fq_less_than(&large_b, &*FQ_MAX));

        // Ensure transitivity
        assert!(fq_less_than(&Fq::from(0u64), &half_modulus));
        assert!(fq_less_than(&half_modulus, &*FQ_MAX));
    }

    /// Test non-membership with values in the upper half of the field
    #[test]
    fn test_non_membership_upper_field_half() {
        let tree = IndexedMerkleTree::new();

        // Value in upper half of field
        let half_modulus = *FQ_MAX / Fq::from(2u64);
        let upper_value = half_modulus + Fq::from(1000u64);

        let (pos, leaf, path) = tree.non_membership_proof(upper_value).unwrap();

        // Should be in sentinel's gap
        assert_eq!(pos, 0);
        assert!(fq_less_than(&leaf.value, &upper_value));
        assert!(fq_less_than(&upper_value, &leaf.next_value));

        // Verify path
        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            pos,
            &leaf,
            &path,
            root,
            DEFAULT_DEPTH
        ));
    }

    /// Test that adjacent values don't create invalid gaps
    #[test]
    fn test_adjacent_values_no_gap() {
        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(100u64)).unwrap();
        tree.insert(Fq::from(101u64)).unwrap();

        let leaf_100 = tree.get_leaf(1).unwrap();
        assert_eq!(leaf_100.value, Fq::from(100u64));
        assert_eq!(leaf_100.next_value, Fq::from(101u64));
    }

    /// Test sentinel covers entire range in empty tree
    #[test]
    fn test_sentinel_covers_full_range() {
        let tree = IndexedMerkleTree::new();
        let sentinel = tree.get_leaf(0).unwrap();

        // Sentinel: value=0, next_value=FQ_MAX
        assert_eq!(sentinel.value, Fq::from(0u64));
        assert_eq!(sentinel.next_value, *FQ_MAX);

        // Any value 0 < x < FQ_MAX should have valid non-membership proof
        for v in [1u64, 1000, u64::MAX / 2, u64::MAX] {
            let value = Fq::from(v);
            let result = tree.non_membership_proof(value);
            assert!(result.is_ok(), "Should have proof for value {}", v);
        }
    }
}
