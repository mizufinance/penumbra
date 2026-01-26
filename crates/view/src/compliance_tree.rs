//! In-memory compliance trees with SQLite persistence.
//!
//! This module provides wrappers around the core compliance tree types
//! (`QuadTree` for users, `IndexedMerkleTree` for assets) that enable
//! local sync and proof generation (following the SCT pattern).
//!
//! The design follows the SCT pattern:
//! - Sync full tree STRUCTURE (all commitments) for path computation
//! - Store full LEAF DATA only for addresses in scope (own + counterparties)

use anyhow::{Context as _, Result};
use penumbra_sdk_compliance::{
    indexed_tree::{IndexedLeaf, IndexedMerkleTree},
    structs::MerklePath,
    tree::QuadTree,
};
use penumbra_sdk_tct::StateCommitment;

use crate::storage::compliance::{ComplianceTreeStore, IndexedLeafData};

/// In-memory user compliance tree (QuadTree) with SQLite persistence.
///
/// Syncs the full tree structure to enable local auth path computation,
/// but only stores full leaf data for addresses in sync scope.
#[derive(Debug, Clone)]
pub struct ComplianceUserTree {
    inner: QuadTree,
    /// Next position for insertion
    position: u64,
}

impl ComplianceUserTree {
    /// Create a new empty user tree.
    pub fn new() -> Self {
        Self {
            inner: QuadTree::new(),
            position: 0,
        }
    }

    /// Load tree from SQLite storage.
    pub fn from_store(store: &mut ComplianceTreeStore<'_, '_>) -> Result<Self> {
        let mut tree = QuadTree::new();
        let position = store.get_user_tree_position()?;

        // Load all commitments and rebuild tree
        for pos in 0..position {
            let commitment = store
                .get_user_position(pos)?
                .ok_or_else(|| anyhow::anyhow!("missing user commitment at position {}", pos))?;
            tree.update(pos, commitment)?;
        }

        // Load internal hashes (optimization to avoid recomputation)
        // The QuadTree will compute hashes on demand, but loading them
        // speeds up initial path queries
        // Note: For simplicity, we rely on the tree to recompute hashes
        // from the commitments. This is correct but could be optimized.

        Ok(Self {
            inner: tree,
            position,
        })
    }

    /// Insert a new commitment at the next position.
    pub fn insert(&mut self, commitment: StateCommitment) -> Result<u64> {
        let pos = self.position;
        self.inner.update(pos, commitment)?;
        self.position += 1;
        Ok(pos)
    }

    /// Get the current tree root.
    pub fn root(&self) -> StateCommitment {
        self.inner.root()
    }

    /// Get the current position (next insertion point).
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Compute a Merkle authentication path for a position.
    pub fn witness(&self, position: u64) -> Result<MerklePath> {
        let auth_path = self.inner.auth_path(position)?;
        Ok(MerklePath::from_auth_path(auth_path))
    }

    /// Persist tree state to SQLite.
    ///
    /// This saves:
    /// - All commitments from `start_position` to current position
    /// - The current position cursor
    pub fn persist(
        &self,
        store: &mut ComplianceTreeStore<'_, '_>,
        start_position: u64,
    ) -> Result<()> {
        // Save new commitments
        for pos in start_position..self.position {
            let commitment = self.inner.get_leaf(pos).ok_or_else(|| {
                anyhow::anyhow!("missing leaf at position {} during persist", pos)
            })?;
            store.add_user_position(pos, commitment)?;
        }

        // Save position cursor
        store.set_user_tree_position(self.position)?;

        Ok(())
    }
}

impl Default for ComplianceUserTree {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory asset compliance tree (Indexed Merkle Tree) with SQLite persistence.
///
/// Syncs the full IMT for non-membership proofs.
///
/// During sync, we store the raw asset IDs (Fq values) that have been inserted.
/// On load, we replay the inserts to reconstruct the tree with correct structure.
#[derive(Debug, Clone)]
pub struct ComplianceAssetTree {
    inner: IndexedMerkleTree,
    /// Ordered list of asset values that have been inserted (for persistence)
    inserted_values: Vec<decaf377::Fq>,
}

impl ComplianceAssetTree {
    /// Create a new empty asset tree.
    pub fn new() -> Self {
        Self {
            inner: IndexedMerkleTree::new(),
            inserted_values: Vec::new(),
        }
    }

    /// Load tree from SQLite storage.
    ///
    /// We store and load the raw Fq values of inserted assets, then replay
    /// the inserts to reconstruct the tree with correct IMT structure.
    pub fn from_store(store: &mut ComplianceTreeStore<'_, '_>) -> Result<Self> {
        let mut tree = IndexedMerkleTree::new();
        let mut inserted_values = Vec::new();

        let position = store.get_asset_tree_leaf_count()?;

        // Load all leaf values and replay inserts
        // Note: Position 0 is the sentinel, we start from 1
        for pos in 1..position {
            let leaf_data = store
                .get_asset_leaf(pos)?
                .ok_or_else(|| anyhow::anyhow!("missing asset leaf at position {}", pos))?;
            let value = decaf377::Fq::from_bytes_checked(&leaf_data.value)
                .map_err(|_| anyhow::anyhow!("invalid Fq bytes for asset leaf value"))?;
            inserted_values.push(value);
        }

        // Replay all inserts to rebuild the tree with correct structure
        for value in &inserted_values {
            tree.insert(*value)
                .context("failed to replay asset insertion during tree load")?;
        }

        Ok(Self {
            inner: tree,
            inserted_values,
        })
    }

    /// Insert a new asset value into the tree.
    ///
    /// Returns the position where the asset was inserted.
    pub fn insert(&mut self, value: decaf377::Fq) -> Result<u64> {
        let position = self.inner.insert(value)?;
        self.inserted_values.push(value);
        Ok(position)
    }

    /// Check if an asset value is in the tree.
    pub fn contains(&self, value: decaf377::Fq) -> bool {
        self.inner.contains(value)
    }

    /// Get the current tree root.
    pub fn root(&self) -> StateCommitment {
        self.inner.root()
    }

    /// Get the current leaf count (including sentinel).
    pub fn leaf_count(&self) -> u64 {
        self.inner.leaf_count()
    }

    /// Get proof data for an asset (membership or non-membership).
    pub fn get_proof_data(
        &self,
        asset_id: penumbra_sdk_asset::asset::Id,
    ) -> Result<(u64, IndexedLeaf, MerklePath, bool)> {
        let value = asset_id.0;

        if self.inner.contains(value) {
            // Membership proof
            let (position, indexed_leaf, path) = self.inner.membership_proof(value)?;
            Ok((
                position,
                indexed_leaf,
                MerklePath::from_auth_path(path),
                true,
            ))
        } else {
            // Non-membership proof
            let (position, indexed_leaf, path) = self.inner.non_membership_proof(value)?;
            Ok((
                position,
                indexed_leaf,
                MerklePath::from_auth_path(path),
                false,
            ))
        }
    }

    /// Persist tree state to SQLite.
    ///
    /// This saves:
    /// - All indexed leaves (for reconstruction on load)
    /// - The current position cursor
    pub fn persist(
        &self,
        store: &mut ComplianceTreeStore<'_, '_>,
        start_position: u64,
    ) -> Result<()> {
        let leaf_count = self.inner.leaf_count();

        // Save new leaves (position 0 is the sentinel, always present)
        for pos in start_position..leaf_count {
            let leaf = self.inner.get_leaf(pos).ok_or_else(|| {
                anyhow::anyhow!("missing asset leaf at position {} during persist", pos)
            })?;
            let leaf_data = IndexedLeafData {
                value: leaf.value.to_bytes(),
                next_index: leaf.next_index,
                next_value: leaf.next_value.to_bytes(),
            };
            store.add_asset_leaf(pos, leaf_data)?;
        }

        // Save position cursor (leaf count)
        store.set_asset_tree_leaf_count(leaf_count)?;

        Ok(())
    }
}

impl Default for ComplianceAssetTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_tree_insert_and_witness() {
        let mut tree = ComplianceUserTree::new();

        // Insert some commitments
        let c1 = StateCommitment::try_from([1u8; 32]).unwrap();
        let c2 = StateCommitment::try_from([2u8; 32]).unwrap();

        let pos1 = tree.insert(c1).unwrap();
        let pos2 = tree.insert(c2).unwrap();

        assert_eq!(pos1, 0);
        assert_eq!(pos2, 1);
        assert_eq!(tree.position(), 2);

        // Witness should work
        let _path = tree.witness(0).unwrap();
        let _path = tree.witness(1).unwrap();
    }

    #[test]
    fn asset_tree_basics() {
        let tree = ComplianceAssetTree::new();

        // New tree starts with sentinel at position 0, so leaf_count is 1
        assert_eq!(tree.leaf_count(), 1);

        // Root should be computable
        let _root = tree.root();
    }
}
