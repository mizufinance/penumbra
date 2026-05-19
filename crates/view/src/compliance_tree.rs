//! In-memory compliance trees with SQLite persistence.
//!
//! This module provides wrappers around the core compliance tree types
//! (`QuadTree` for users, `IndexedMerkleTree` for assets) that enable
//! local sync and proof generation (following the SCT pattern).
//!
//! The design follows the SCT pattern:
//! - Sync full tree STRUCTURE (all commitments) for path computation
//! - Store full LEAF DATA only for addresses in scope (own + counterparties)

use anyhow::Result;
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
    /// Loads full leaf data including policy (dk_pub, threshold) to ensure
    /// correct tree reconstruction with matching leaf commitments.
    pub fn from_store(store: &mut ComplianceTreeStore<'_, '_>) -> Result<Self> {
        use penumbra_sdk_compliance::indexed_tree::{LeafParams, LeafRing};

        let leaf_count = store.get_asset_tree_leaf_count()?;

        tracing::debug!(leaf_count, "ComplianceAssetTree::from_store: starting load");

        // Check if sentinel (position 0) exists in storage
        // If not, this is a fresh database - return a new tree
        if store.get_asset_leaf(0)?.is_none() {
            tracing::debug!("ComplianceAssetTree::from_store: fresh database, returning new tree");
            // Fresh database, no leaves stored yet - return new tree with sentinel
            return Ok(Self::new());
        }

        let mut tree = IndexedMerkleTree::new();
        let mut inserted_values = Vec::new();

        // Load all leaves with full data (including policy)
        // Position 0 is the sentinel, we load it too
        for pos in 0..leaf_count {
            let leaf_data = store
                .get_asset_leaf(pos)?
                .ok_or_else(|| anyhow::anyhow!("missing asset leaf at position {}", pos))?;

            let value = decaf377::Fq::from_bytes_checked(&leaf_data.value).map_err(|_| {
                anyhow::anyhow!("invalid Fq bytes for asset leaf value at position {}", pos)
            })?;
            let next_value =
                decaf377::Fq::from_bytes_checked(&leaf_data.next_value).map_err(|_| {
                    anyhow::anyhow!(
                        "invalid Fq bytes for asset leaf next_value at position {}",
                        pos
                    )
                })?;
            let dk_pub = decaf377::Encoding(leaf_data.dk_pub)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding at position {}", pos))?;

            let route_policy_hash = decaf377::Fq::from_bytes_checked(&leaf_data.route_policy_hash)
                .map_err(|_| anyhow::anyhow!("invalid route_policy_hash at position {}", pos))?;
            let ring_pk = decaf377::Encoding(leaf_data.ring_pk)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding at position {}", pos))?;
            let ring_id_hash = decaf377::Fq::from_bytes_checked(&leaf_data.ring_id_hash)
                .map_err(|_| anyhow::anyhow!("invalid ring_id_hash at position {}", pos))?;
            let policy_id_hash = decaf377::Fq::from_bytes_checked(&leaf_data.policy_id_hash)
                .map_err(|_| anyhow::anyhow!("invalid policy_id_hash at position {}", pos))?;
            let permission_hash = decaf377::Fq::from_bytes_checked(&leaf_data.permission_hash)
                .map_err(|_| anyhow::anyhow!("invalid permission_hash at position {}", pos))?;
            let resource_hash = decaf377::Fq::from_bytes_checked(&leaf_data.resource_hash)
                .map_err(|_| anyhow::anyhow!("invalid resource_hash at position {}", pos))?;

            let leaf = IndexedLeaf {
                value,
                next_index: leaf_data.next_index,
                next_value,
                params: LeafParams {
                    dk_pub,
                    threshold: leaf_data.threshold,
                    route_policy_hash,
                },
                ring: LeafRing {
                    ring_pk,
                    ring_id_hash,
                    policy_id_hash,
                    permission_hash,
                    resource_hash,
                },
            };

            // Debug: log each loaded leaf
            tracing::debug!(
                position = pos,
                value = ?leaf_data.value,
                next_index = leaf_data.next_index,
                threshold = leaf_data.threshold,
                dk_pub_first_byte = leaf_data.dk_pub[0],
                "ComplianceAssetTree::from_store: loaded leaf"
            );

            tree.load_leaf(pos, leaf);

            // Track non-sentinel values for the inserted_values list
            if pos > 0 {
                inserted_values.push(value);
            }
        }

        // Rebuild all internal hashes from the loaded leaves
        tree.rebuild_hashes();

        let root = tree.root();
        tracing::debug!(
            leaf_count,
            root = ?root.0.to_bytes(),
            "ComplianceAssetTree::from_store: completed load"
        );

        Ok(Self {
            inner: tree,
            inserted_values,
        })
    }

    /// Sync a leaf from an EventAssetRegistered (preserves policy data).
    ///
    /// This is the correct method to use when syncing from CompactBlock events,
    /// as it preserves the full IndexedLeaf data including policy (dk_pub, threshold).
    pub fn sync_from_event(
        &mut self,
        new_leaf: IndexedLeaf,
        new_position: u64,
        updated_low_leaf: IndexedLeaf,
        low_leaf_position: u64,
    ) -> Result<()> {
        self.inner.sync_from_event(
            new_leaf.clone(),
            new_position,
            updated_low_leaf,
            low_leaf_position,
        )?;
        self.inserted_values.push(new_leaf.value);
        Ok(())
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
    /// This saves all indexed leaves from position 0 to current leaf_count.
    /// Uses INSERT OR REPLACE to handle both new and updated leaves.
    pub fn persist(
        &self,
        store: &mut ComplianceTreeStore<'_, '_>,
        _start_position: u64,
    ) -> Result<()> {
        let leaf_count = self.inner.leaf_count();

        // Save all leaves (INSERT OR REPLACE handles updates)
        // This ensures both new leaves AND updated low leaves are persisted correctly.
        for pos in 0..leaf_count {
            let leaf = self.inner.get_leaf(pos).ok_or_else(|| {
                anyhow::anyhow!("missing asset leaf at position {} during persist", pos)
            })?;
            let leaf_data = IndexedLeafData {
                value: leaf.value.to_bytes(),
                next_index: leaf.next_index,
                next_value: leaf.next_value.to_bytes(),
                dk_pub: leaf.params.dk_pub.vartime_compress().0,
                threshold: leaf.params.threshold,
                route_policy_hash: leaf.params.route_policy_hash.to_bytes(),
                ring_pk: leaf.ring.ring_pk.vartime_compress().0,
                ring_id_hash: leaf.ring.ring_id_hash.to_bytes(),
                policy_id_hash: leaf.ring.policy_id_hash.to_bytes(),
                permission_hash: leaf.ring.permission_hash.to_bytes(),
                resource_hash: leaf.ring.resource_hash.to_bytes(),
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

    #[test]
    fn asset_tree_sync_preserves_policy() {
        use decaf377::Fq;
        use penumbra_sdk_compliance::indexed_tree::{LeafParams, LeafRing, FQ_MAX};

        let mut tree = ComplianceAssetTree::new();

        // Create a leaf with non-default policy (simulating a regulated asset)
        let dk_pub = decaf377::Element::GENERATOR; // Non-identity element
        let threshold = 1000u128;

        let new_leaf = IndexedLeaf {
            value: Fq::from(12345u64),
            next_index: 0,
            next_value: FQ_MAX.clone(),
            params: LeafParams {
                dk_pub,
                threshold,
                route_policy_hash: penumbra_sdk_compliance::indexed_tree::string_to_fq(""),
            },
            ring: LeafRing::default(),
        };

        // The sentinel (low leaf) gets updated to point to the new leaf
        let updated_sentinel = IndexedLeaf::with_default_policy(
            Fq::from(0u64),
            1, // Points to new leaf
            Fq::from(12345u64),
        );

        // Sync using the event-based method (preserves policy)
        tree.sync_from_event(
            new_leaf.clone(),
            1, // new position
            updated_sentinel,
            0, // sentinel position
        )
        .unwrap();

        // Verify policy is preserved via membership proof
        let asset_id = penumbra_sdk_asset::asset::Id(new_leaf.value);
        let (position, retrieved_leaf, _path, is_regulated) =
            tree.get_proof_data(asset_id).unwrap();

        assert_eq!(position, 1);
        assert!(is_regulated);
        assert_eq!(retrieved_leaf.params.dk_pub, dk_pub);
        assert_eq!(retrieved_leaf.params.threshold, threshold);
    }
}
