use anyhow::Result;
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use penumbra_sdk_sct::component::clock::EpochRead;
use penumbra_sdk_tct::StateCommitment;

use crate::{
    event, indexed_tree,
    indexed_tree::{IndexedLeaf, IndexedMerkleTree},
    state_key,
    structs::{AssetPolicy, ComplianceLeaf, MerklePath},
    tree::QuadTree,
};

// Note: QuadTree is still used for the user tree. Asset tree has been migrated to IMT.

/// Maximum age of compliance anchors in blocks (~10 minutes at 6s blocks).
/// Prevents using stale anchors to falsely prove non-membership.
/// Safe because trees are append-only.
pub const MAX_ANCHOR_AGE_BLOCKS: u64 = 100;

// Re-export bincode for serialization
use bincode;

/// Proof data for an asset in the IMT.
/// Contains all information needed for membership or non-membership proofs.
#[derive(Clone, Debug)]
pub struct AssetProofData {
    /// The indexed leaf (for membership or non-membership proof).
    pub indexed_leaf: IndexedLeaf,
    /// Position of the leaf in the IMT.
    pub position: u64,
    /// Authentication path from leaf to root.
    pub auth_path: MerklePath,
    /// Whether the asset is regulated (membership) or not (non-membership).
    pub is_regulated: bool,
}

/// Extension trait for reading compliance registry state.
#[async_trait]
pub trait ComplianceRegistryRead: StateRead {
    /// Get the user compliance tree from state.
    async fn get_user_tree(&self) -> Result<QuadTree> {
        match self.get_raw(state_key::user_tree()).await? {
            Some(bytes) => Ok(bincode::deserialize(&bytes)?),
            None => Ok(QuadTree::new()),
        }
    }

    /// Get the asset Indexed Merkle Tree (IMT) from state.
    async fn get_asset_imt(&self) -> Result<IndexedMerkleTree> {
        match self.get_raw(state_key::asset_imt()).await? {
            Some(bytes) => Ok(bincode::deserialize(&bytes)?),
            None => Ok(IndexedMerkleTree::new()),
        }
    }

    /// Get the asset IMT root hash.
    async fn get_asset_imt_root(&self) -> Result<StateCommitment> {
        let tree = self.get_asset_imt().await?;
        Ok(tree.root())
    }

    /// Get proof data for an asset using the IMT.
    ///
    /// For regulated assets: returns membership proof (exact match).
    /// For unregulated assets: returns non-membership proof (gap).
    async fn get_asset_proof_data(&self, asset_id: asset::Id) -> Result<AssetProofData> {
        let tree = self.get_asset_imt().await?;
        let value = asset_id.0;

        if tree.contains(value) {
            // Regulated asset - membership proof
            let (position, indexed_leaf, path) = tree.membership_proof(value)?;
            Ok(AssetProofData {
                indexed_leaf,
                position,
                auth_path: MerklePath::from_auth_path(path),
                is_regulated: true,
            })
        } else {
            // Unregulated asset - non-membership proof
            let (position, indexed_leaf, path) = tree.non_membership_proof(value)?;
            Ok(AssetProofData {
                indexed_leaf,
                position,
                auth_path: MerklePath::from_auth_path(path),
                is_regulated: false,
            })
        }
    }

    /// Get the current user count (number of registered users).
    async fn get_user_count(&self) -> Result<u64> {
        Ok(self
            .get_proto(state_key::user_count())
            .await?
            .unwrap_or(0u64))
    }

    /// Get the current asset count (number of registered assets).
    async fn get_asset_count(&self) -> Result<u64> {
        Ok(self
            .get_proto(state_key::asset_count())
            .await?
            .unwrap_or(0u64))
    }

    /// Get the compliance policy for an asset.
    ///
    /// Returns the issuer's detection key and threshold for flagged transfers.
    /// Returns `None` if no policy is set (asset uses default behavior).
    async fn get_asset_policy(&self, asset_id: asset::Id) -> Result<Option<AssetPolicy>> {
        let key = state_key::asset_policy(&asset_id);
        match self.get_raw(&key).await? {
            Some(bytes) => Ok(Some(AssetPolicy::from_bytes(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Get the user tree root hash.
    async fn get_user_tree_root(&self) -> Result<StateCommitment> {
        let tree = self.get_user_tree().await?;
        Ok(tree.root())
    }

    /// Get an authentication path for a user at the given position.
    async fn get_user_auth_path(&self, position: u64) -> Result<Vec<[StateCommitment; 3]>> {
        let tree = self.get_user_tree().await?;
        tree.auth_path(position)
    }

    /// Get the position of a user's leaf in the user tree.
    ///
    /// This enables O(1) lookup for generating merkle paths during transaction planning.
    ///
    /// # Arguments
    /// * `address` - The wallet address
    /// * `asset_id` - The asset ID
    ///
    /// # Returns
    /// Returns `Some(position)` if the user is registered for this asset, `None` otherwise.
    async fn get_user_leaf_position(
        &self,
        address: &penumbra_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<Option<u64>> {
        let lookup_key = state_key::user_leaf_position(address, &asset_id);
        self.get_proto(&lookup_key).await
    }

    /// Get the full ComplianceLeaf for a user.
    ///
    /// This retrieves the complete leaf data (including the ACK) that was registered
    /// on-chain. This is needed for proof generation to ensure the leaf used in the
    /// proof matches what was registered.
    ///
    /// # Arguments
    /// * `address` - The wallet address
    /// * `asset_id` - The asset ID
    ///
    /// # Returns
    /// Returns `Some(ComplianceLeaf)` if the user is registered for this asset, `None` otherwise.
    async fn get_user_leaf(
        &self,
        address: &penumbra_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<Option<ComplianceLeaf>> {
        use penumbra_sdk_proto::DomainType;

        let lookup_key = state_key::user_leaf_data(address, &asset_id);
        match self.get_raw(&lookup_key).await? {
            Some(bytes) => {
                // Use proto decoding (ComplianceLeaf implements DomainType)
                let leaf = ComplianceLeaf::decode(bytes.as_slice())?;
                Ok(Some(leaf))
            }
            None => Ok(None),
        }
    }

    /// Verify that a compliance leaf exists on-chain by checking if its commitment
    /// is in the user tree.
    ///
    /// This function is used to verify that a leaf shared off-chain actually exists
    /// in the on-chain registry.
    ///
    /// # Arguments
    /// * `leaf` - The compliance leaf to verify
    ///
    /// # Returns
    /// Returns `Ok(true)` if the leaf's commitment is found in the tree at any position,
    /// `Ok(false)` if not found.
    ///
    /// # Note
    /// This is a linear scan through all user positions. For production, consider
    /// adding a reverse mapping from commitment to position.
    async fn verify_compliance_leaf(&self, leaf: &ComplianceLeaf) -> Result<bool> {
        let tree = self.get_user_tree().await?;
        let user_count = self.get_user_count().await?;
        let target_commitment = leaf.commit();

        // Scan through all positions to find matching commitment
        for position in 0..user_count {
            if let Some(commitment) = tree.get_leaf(position) {
                if commitment.0 == target_commitment.0 {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    // ========== Historical Anchor Validation ==========

    /// Check if a user tree anchor is valid (exists in historical records).
    ///
    /// Returns `Some(height)` if the anchor was recorded at that block height,
    /// `None` if the anchor is unknown.
    async fn check_user_anchor(&self, anchor: &StateCommitment) -> Result<Option<u64>> {
        let key = state_key::anchor::user_anchor_lookup(anchor);
        self.get_proto(&key).await
    }

    /// Check if an asset IMT anchor is valid (exists in historical records).
    ///
    /// Returns `Some(height)` if the anchor was recorded at that block height,
    /// `None` if the anchor is unknown.
    async fn check_asset_anchor(&self, anchor: &StateCommitment) -> Result<Option<u64>> {
        let key = state_key::anchor::asset_anchor_lookup(anchor);
        self.get_proto(&key).await
    }

    /// Get the user tree anchor at a specific block height.
    async fn get_user_anchor_by_height(&self, height: u64) -> Result<Option<StateCommitment>> {
        self.get(&state_key::anchor::user_anchor_by_height(height))
            .await
    }

    /// Get the asset IMT anchor at a specific block height.
    async fn get_asset_anchor_by_height(&self, height: u64) -> Result<Option<StateCommitment>> {
        self.get(&state_key::anchor::asset_anchor_by_height(height))
            .await
    }

    /// Validate that a compliance proof uses valid historical anchors.
    ///
    /// Checks that both anchors:
    /// 1. Exist in historical records
    /// 2. Are not older than MAX_ANCHOR_AGE_BLOCKS from current height
    ///
    /// The age check prevents the "genesis anchor attack" where an attacker
    /// uses an old anchor to prove false non-membership for newly regulated assets.
    ///
    /// Returns `Ok(())` if both anchors are valid and recent, otherwise returns an error.
    async fn validate_compliance_anchors(
        &self,
        user_anchor: &StateCommitment,
        asset_anchor: &StateCommitment,
    ) -> Result<()> {
        let current_height = self.get_block_height().await?;

        // Check user anchor exists and is recent enough
        let user_anchor_height = self.check_user_anchor(user_anchor).await?.ok_or_else(|| {
            anyhow::anyhow!("invalid user compliance anchor: not found in history")
        })?;

        if current_height > user_anchor_height + MAX_ANCHOR_AGE_BLOCKS {
            anyhow::bail!(
                "user compliance anchor too old: height {} is more than {} blocks behind current height {}",
                user_anchor_height,
                MAX_ANCHOR_AGE_BLOCKS,
                current_height
            );
        }

        // Check asset anchor exists and is recent enough
        let asset_anchor_height =
            self.check_asset_anchor(asset_anchor)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("invalid asset compliance anchor: not found in history")
                })?;

        if current_height > asset_anchor_height + MAX_ANCHOR_AGE_BLOCKS {
            anyhow::bail!(
                "asset compliance anchor too old: height {} is more than {} blocks behind current height {}",
                asset_anchor_height,
                MAX_ANCHOR_AGE_BLOCKS,
                current_height
            );
        }

        Ok(())
    }
}

impl<T: StateRead + ?Sized> ComplianceRegistryRead for T {}

/// Extension trait for writing compliance registry state.
#[async_trait]
pub trait ComplianceRegistryWrite: StateWrite + ComplianceRegistryRead {
    /// Add a compliance leaf for a user.
    ///
    /// This registers a user's address compliance key (ACK) for a regulated asset.
    /// The leaf is committed and added to the user tree at the next available position.
    ///
    /// # Arguments
    /// * `leaf` - The compliance leaf containing address, ACK, and asset_id
    ///
    /// # Returns
    /// The position in the user tree where the leaf was added.
    async fn add_compliance_leaf(&mut self, leaf: ComplianceLeaf) -> Result<u64> {
        // Load the current user tree (or create new)
        let mut tree = self.get_user_tree().await?;

        // Load the current user count (this will be our position)
        let position = self.get_user_count().await?;

        // Calculate the leaf commitment
        let commitment = leaf.commit();

        // Update the tree at the next available position
        tree.update(position, commitment)?;

        // Increment the user count
        let new_count = position + 1;

        // Save the updated tree and count
        let tree_bytes = bincode::serialize(&tree)?;
        self.put_raw(state_key::user_tree().to_string(), tree_bytes);
        self.put_proto(state_key::user_count().to_string(), new_count);

        // Store the reverse lookup index for O(1) position retrieval
        let lookup_key = state_key::user_leaf_position(&leaf.address, &leaf.asset_id);
        self.put_proto(lookup_key, position);

        // Store the full leaf data for later retrieval during proof generation
        // Use proto encoding since ComplianceLeaf has serde(try_from/into proto) attributes
        use penumbra_sdk_proto::DomainType;
        let leaf_data_key = state_key::user_leaf_data(&leaf.address, &leaf.asset_id);
        let leaf_bytes = leaf.encode_to_vec();
        self.put_raw(leaf_data_key, leaf_bytes);

        Ok(position)
    }

    /// Register a regulated asset in the IMT.
    ///
    /// Only regulated assets are stored in the IMT. Unregulated status is proven
    /// via non-membership proofs (asset falls in a gap between leaves).
    ///
    /// This method is idempotent - if the asset is already registered, returns None.
    ///
    /// The `dk_pub` is the issuer's detection key - REQUIRED for regulated assets.
    /// The leaf will have a policy with that detection key and threshold = u64::MAX
    /// (nothing flagged by default, threshold can be set separately).
    ///
    /// # Returns
    /// Some(InsertResult) with full insertion data for client sync, or None if already registered.
    async fn register_regulated_asset(
        &mut self,
        asset_id: asset::Id,
        dk_pub: decaf377::Element,
    ) -> Result<Option<indexed_tree::InsertResult>> {
        let mut tree = self.get_asset_imt().await?;

        // Check if already exists - be idempotent
        if let Some(position) = tree.get_position(asset_id.0) {
            tracing::debug!(?asset_id, position, "asset already in IMT, skipping");
            return Ok(None);
        }

        // Insert into the IMT and get full data (policy embedded in leaf)
        let result = tree.insert_with_data(asset_id.0, dk_pub)?;

        // Save the updated tree
        let tree_bytes = bincode::serialize(&tree)?;
        self.put_raw(state_key::asset_imt().to_string(), tree_bytes);

        // Update the persisted asset count
        let new_count = tree.leaf_count();
        self.put_proto(state_key::asset_count().to_string(), new_count);

        tracing::debug!(
            ?asset_id,
            result.position,
            new_count,
            "registered regulated asset in IMT"
        );
        Ok(Some(result))
    }

    /// Save the asset IMT to state.
    async fn put_asset_imt(&mut self, tree: &IndexedMerkleTree) -> Result<()> {
        let tree_bytes = bincode::serialize(tree)?;
        self.put_raw(state_key::asset_imt().to_string(), tree_bytes);
        Ok(())
    }

    /// Set the compliance policy for an asset.
    ///
    /// Stores the issuer's detection key and threshold for flagged transfers.
    fn set_asset_policy(&mut self, asset_id: asset::Id, policy: AssetPolicy) {
        let key = state_key::asset_policy(&asset_id);
        self.put_raw(key, policy.to_bytes());
    }

    // ========== Historical Anchor Storage ==========

    /// Record the current compliance tree anchors at the given block height.
    ///
    /// This should be called at the end of each block to store bidirectional
    /// mappings for both user tree and asset IMT anchors. These mappings enable
    /// validation of historical anchors in compliance proofs.
    async fn record_compliance_anchors(&mut self, height: u64) -> Result<()> {
        // Get current anchors
        let user_anchor = self.get_user_tree_root().await?;
        let asset_anchor = self.get_asset_imt_root().await?;

        // Store user anchor bidirectionally using verifiable storage (matching SCT pattern)
        self.put(
            state_key::anchor::user_anchor_by_height(height),
            user_anchor,
        );
        self.put_proto(state_key::anchor::user_anchor_lookup(&user_anchor), height);

        // Store asset anchor bidirectionally using verifiable storage
        self.put(
            state_key::anchor::asset_anchor_by_height(height),
            asset_anchor,
        );
        self.put_proto(
            state_key::anchor::asset_anchor_lookup(&asset_anchor),
            height,
        );

        // Emit anchor event for local sync
        self.record_proto(event::compliance_anchor(height, user_anchor, asset_anchor));

        tracing::debug!(
            height,
            ?user_anchor,
            ?asset_anchor,
            "recorded compliance anchors"
        );

        Ok(())
    }

    // ========== Pending Registrations for CompactBlock ==========

    /// Buffer a user registration event for inclusion in the CompactBlock.
    ///
    /// This should be called when a user is registered during transaction processing.
    /// The events are accumulated and drained when building the CompactBlock.
    fn record_pending_user_registration(&mut self, event: event::EventUserRegistered) {
        let key = state_key::pending_user_registrations();
        let mut pending: Vec<event::EventUserRegistered> =
            self.object_get(key).unwrap_or_else(|| {
                // Empty vec is the expected initial state - no need to log
                Vec::new()
            });
        pending.push(event);
        self.object_put(key, pending);
    }

    /// Buffer an asset registration event for inclusion in the CompactBlock.
    ///
    /// This should be called when an asset is registered during transaction processing.
    /// The events are accumulated and drained when building the CompactBlock.
    fn record_pending_asset_registration(&mut self, event: event::EventAssetRegistered) {
        let key = state_key::pending_asset_registrations();
        let mut pending: Vec<event::EventAssetRegistered> =
            self.object_get(key).unwrap_or_else(|| {
                // Empty vec is the expected initial state - no need to log
                Vec::new()
            });
        pending.push(event);
        self.object_put(key, pending);
    }

    /// Retrieve and clear all pending user registrations.
    ///
    /// This should be called during CompactBlock finalization to include
    /// user registration events for client sync.
    fn pending_user_registrations(&mut self) -> Vec<event::EventUserRegistered> {
        let key = state_key::pending_user_registrations();
        let result = self.object_get(key).unwrap_or_else(|| {
            // Empty vec is the expected state when no registrations occurred - no need to log
            Vec::new()
        });
        self.object_delete(key);
        result
    }

    /// Retrieve and clear all pending asset registrations.
    ///
    /// This should be called during CompactBlock finalization to include
    /// asset registration events for client sync.
    fn pending_asset_registrations(&mut self) -> Vec<event::EventAssetRegistered> {
        let key = state_key::pending_asset_registrations();
        let result = self.object_get(key).unwrap_or_else(|| {
            // Empty vec is the expected state when no registrations occurred - no need to log
            Vec::new()
        });
        self.object_delete(key);
        result
    }
}

impl<T: StateWrite + ?Sized> ComplianceRegistryWrite for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::QuadTree;
    use cnidarium::TempStorage;
    use decaf377::Fq;
    use penumbra_sdk_keys::{keys::AddressComplianceKey, Address};
    use penumbra_sdk_sct::component::clock::EpochManager;

    #[tokio::test]
    async fn test_add_compliance_leaf() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Create a dummy compliance leaf
        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rand::thread_rng()),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(1u64)),
        };

        // Add the leaf
        state.add_compliance_leaf(leaf.clone()).await.unwrap();

        // Check that user count increased
        let count = state.get_user_count().await.unwrap();
        assert_eq!(count, 1);

        // Check that the tree root changed
        let root = state.get_user_tree_root().await.unwrap();
        assert_ne!(root.0, Fq::from(0u64));
    }

    #[tokio::test]
    async fn test_register_regulated_asset() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let asset_id = asset::Id(Fq::from(123u64));

        // Initially asset is unregulated (not in IMT)
        let proof_before = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(!proof_before.is_regulated);

        // Register as regulated
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();

        // Now asset is regulated (in IMT)
        let proof_after = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(proof_after.is_regulated);
        assert_eq!(proof_after.indexed_leaf.value, asset_id.0);

        // IMT root should have changed
        let empty_imt = IndexedMerkleTree::new();
        let root = state.get_asset_imt_root().await.unwrap();
        assert_ne!(root.0, empty_imt.root().0);
    }

    #[tokio::test]
    async fn test_multiple_leaves() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let mut rng = rand::thread_rng();

        // Add multiple leaves
        for i in 0..5 {
            let leaf = ComplianceLeaf {
                address: Address::dummy(&mut rng),
                key: AddressComplianceKey::new(
                    decaf377::Element::GENERATOR * decaf377::Fr::from(i as u64 + 1),
                ),
                asset_id: asset::Id(Fq::from(i as u64)),
            };
            state.add_compliance_leaf(leaf).await.unwrap();
        }

        // Check that user count is correct
        let count = state.get_user_count().await.unwrap();
        assert_eq!(count, 5);

        // Verify we can get auth paths for each position
        for pos in 0..5 {
            let path = state.get_user_auth_path(pos).await.unwrap();
            assert_eq!(path.len(), 16); // DEFAULT_DEPTH
        }
    }

    #[tokio::test]
    async fn test_asset_duplicate_prevention() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let asset_id = asset::Id(Fq::from(789u64));

        // First registration should succeed
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .expect("First registration should succeed");

        // Verify asset is regulated
        let proof = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(proof.is_regulated);

        // Get IMT leaf count
        let imt = state.get_asset_imt().await.unwrap();
        let count_before = imt.leaf_count();

        // Second registration of same asset should be idempotent (succeed but no change)
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .expect("Duplicate registration should be idempotent");

        // Verify IMT leaf count didn't increase
        let imt = state.get_asset_imt().await.unwrap();
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

        let mut rng = rand::thread_rng();

        // Create a compliance leaf
        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(100u64)),
        };

        // Before adding, verification should fail
        let verified = state.verify_compliance_leaf(&leaf).await.unwrap();
        assert!(!verified, "Leaf should not be verified before being added");

        // Add the leaf to the registry
        state.add_compliance_leaf(leaf.clone()).await.unwrap();

        // After adding, verification should succeed
        let verified = state.verify_compliance_leaf(&leaf).await.unwrap();
        assert!(verified, "Leaf should be verified after being added");

        // Create a different leaf with same asset but different wallet
        let different_leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR * decaf377::Fr::from(2u64)),
            asset_id: asset::Id(Fq::from(100u64)),
        };

        // Different leaf should not verify
        let verified = state.verify_compliance_leaf(&different_leaf).await.unwrap();
        assert!(!verified, "Different leaf should not be verified");
    }

    #[tokio::test]
    async fn test_leaf_json_serialization() {
        let mut rng = rand::thread_rng();

        // Create a compliance leaf
        let original_leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(200u64)),
        };

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

        let mut rng = rand::thread_rng();

        // User creates their compliance leaf (private)
        let user_leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(300u64)),
        };

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

        // Issuer can now use the received_leaf.key (ACK) to encrypt compliance data
        assert_eq!(
            user_leaf.key.inner(),
            received_leaf.key.inner(),
            "ACK should be preserved through sharing"
        );
    }

    #[tokio::test]
    async fn test_verify_with_multiple_leaves() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let mut rng = rand::thread_rng();

        // Add multiple leaves
        let mut leaves = Vec::new();
        for i in 0..5u64 {
            let leaf = ComplianceLeaf {
                address: Address::dummy(&mut rng),
                key: AddressComplianceKey::new(
                    decaf377::Element::GENERATOR * decaf377::Fr::from(i + 1),
                ),
                asset_id: asset::Id(Fq::from(i)),
            };
            state.add_compliance_leaf(leaf.clone()).await.unwrap();
            leaves.push(leaf);
        }

        // All added leaves should verify
        for leaf in &leaves {
            let verified = state.verify_compliance_leaf(leaf).await.unwrap();
            assert!(verified, "All added leaves should verify");
        }

        // A new leaf not in the tree should not verify
        let new_leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(999u64),
            ),
            asset_id: asset::Id(Fq::from(999u64)),
        };
        let verified = state.verify_compliance_leaf(&new_leaf).await.unwrap();
        assert!(!verified, "Non-registered leaf should not verify");
    }

    #[tokio::test]
    async fn test_comprehensive_integration() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        let mut rng = rand::thread_rng();

        // Bridged asset (USDC) - regulated
        let usdc_asset_id = asset::Id(Fq::from(12345u64));
        state
            .register_regulated_asset(usdc_asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();

        let usdc_proof = state.get_asset_proof_data(usdc_asset_id).await.unwrap();
        assert!(usdc_proof.is_regulated);

        let empty_imt = IndexedMerkleTree::new();
        assert_ne!(
            state.get_asset_imt_root().await.unwrap().0,
            empty_imt.root().0
        );

        // Native asset (penumbra) - unregulated (NOT in IMT)
        let penumbra_asset_id = asset::Id(Fq::from(1u64));
        // Don't register - unregulated assets are proven via non-membership
        let penumbra_proof = state.get_asset_proof_data(penumbra_asset_id).await.unwrap();
        assert!(!penumbra_proof.is_regulated);

        // Multiple wallets for same user
        let wallet1 = Address::dummy(&mut rng);
        let ack1 =
            AddressComplianceKey::new(decaf377::Element::GENERATOR * decaf377::Fr::from(101u64));
        let leaf1 = ComplianceLeaf {
            address: wallet1.clone(),
            key: ack1,
            asset_id: usdc_asset_id,
        };

        let leaf2 = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(102u64),
            ),
            asset_id: usdc_asset_id,
        };
        let leaf3 = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(103u64),
            ),
            asset_id: usdc_asset_id,
        };

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
        assert_eq!(received_leaf.key.inner(), ack1.inner());

        // Query unregistered asset - should get non-membership proof
        let unknown_asset = asset::Id(Fq::from(99999u64));
        let unknown_proof = state.get_asset_proof_data(unknown_asset).await.unwrap();
        assert!(!unknown_proof.is_regulated);

        // Authentication paths
        let path = state.get_user_auth_path(0).await.unwrap();
        assert_eq!(path.len(), 16);
        let user_root = state.get_user_tree_root().await.unwrap();
        let tree = state.get_user_tree().await.unwrap();
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
            .register_regulated_asset(dai_asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();
        let leaf1_dai = ComplianceLeaf {
            address: wallet1,
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(201u64),
            ),
            asset_id: dai_asset_id,
        };
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
        let mut rng = rand::thread_rng();

        let wallet1 = Address::dummy(&mut rng);
        let wallet2 = Address::dummy(&mut rng);
        let usdc = asset::Id(Fq::from(12345u64));
        let dai = asset::Id(Fq::from(67890u64));

        let leaf1 = ComplianceLeaf {
            address: wallet1.clone(),
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(101u64),
            ),
            asset_id: usdc,
        };
        let leaf2 = ComplianceLeaf {
            address: wallet1.clone(),
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(102u64),
            ),
            asset_id: dai,
        };
        let leaf3 = ComplianceLeaf {
            address: wallet2.clone(),
            key: AddressComplianceKey::new(
                decaf377::Element::GENERATOR * decaf377::Fr::from(103u64),
            ),
            asset_id: usdc,
        };

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
        let tree = state.get_user_tree().await.unwrap();
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
        let mut rng = rand::thread_rng();

        let wallet = Address::dummy(&mut rng);
        let ack =
            AddressComplianceKey::new(decaf377::Element::GENERATOR * decaf377::Fr::from(42u64));
        let asset_id = asset::Id(Fq::from(12345u64));

        let original_leaf = ComplianceLeaf {
            address: wallet.clone(),
            key: ack,
            asset_id,
        };
        state
            .add_compliance_leaf(original_leaf.clone())
            .await
            .unwrap();

        let fetched_leaf = state
            .get_user_leaf(&wallet, asset_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(original_leaf.key.inner(), fetched_leaf.key.inner());
        assert_eq!(original_leaf.address, fetched_leaf.address);
        assert_eq!(original_leaf.asset_id, fetched_leaf.asset_id);
        assert_eq!(original_leaf.commit().0, fetched_leaf.commit().0);
        assert!(state
            .get_user_leaf(&Address::dummy(&mut rng), asset_id)
            .await
            .unwrap()
            .is_none());
    }

    // ========== IMT Tests ==========

    #[tokio::test]
    async fn test_imt_register_regulated_asset() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let asset_id = asset::Id(Fq::from(12345u64));

        // Register regulated asset
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();

        // Check the asset is in the IMT
        let tree = state.get_asset_imt().await.unwrap();
        assert!(tree.contains(asset_id.0));
        assert_eq!(tree.leaf_count(), 2); // sentinel + 1 asset

        // Check via get_asset_proof_data
        let proof_data = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(proof_data.is_regulated);

        // IMT root should have changed from empty
        let empty_tree = IndexedMerkleTree::new();
        let root = state.get_asset_imt_root().await.unwrap();
        assert_ne!(root.0, empty_tree.root().0);
    }

    #[tokio::test]
    async fn test_imt_register_idempotent() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let asset_id = asset::Id(Fq::from(12345u64));

        // Register twice - should be idempotent
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();

        let tree = state.get_asset_imt().await.unwrap();
        assert_eq!(tree.leaf_count(), 2); // sentinel + 1 asset (not 3)
    }

    #[tokio::test]
    async fn test_imt_get_proof_data_regulated() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let asset_id = asset::Id(Fq::from(12345u64));
        state
            .register_regulated_asset(asset_id, decaf377::Element::GENERATOR)
            .await
            .unwrap();

        // Get proof data for regulated asset
        let proof_data = state.get_asset_proof_data(asset_id).await.unwrap();

        assert!(proof_data.is_regulated);
        assert_eq!(proof_data.indexed_leaf.value, asset_id.0);
        assert_eq!(proof_data.position, 1); // First after sentinel
        assert_eq!(proof_data.auth_path.layers.len(), 16);

        // Verify the path
        let tree = state.get_asset_imt().await.unwrap();
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

        // Register one asset
        let regulated_asset = asset::Id(Fq::from(100u64));
        state
            .register_regulated_asset(regulated_asset, decaf377::Element::GENERATOR)
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

        // Register multiple assets in non-sorted order
        let assets = [
            asset::Id(Fq::from(500u64)),
            asset::Id(Fq::from(100u64)),
            asset::Id(Fq::from(300u64)),
        ];

        for asset_id in &assets {
            state
                .register_regulated_asset(*asset_id, decaf377::Element::GENERATOR)
                .await
                .unwrap();
        }

        let tree = state.get_asset_imt().await.unwrap();
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
        let mut rng = rand::thread_rng();

        // Set block height first (required for validation)
        state.put_block_height(1);

        // Add a user and asset
        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(100u64)),
        };
        state.add_compliance_leaf(leaf).await.unwrap();
        state
            .register_regulated_asset(asset::Id(Fq::from(200u64)), decaf377::Element::GENERATOR)
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

        // Can retrieve by height
        let user_anchor_by_height = state.get_user_anchor_by_height(1).await.unwrap().unwrap();
        let asset_anchor_by_height = state.get_asset_anchor_by_height(1).await.unwrap().unwrap();
        assert_eq!(user_anchor.0, user_anchor_by_height.0);
        assert_eq!(asset_anchor.0, asset_anchor_by_height.0);

        // Can check via lookup
        let user_height = state.check_user_anchor(&user_anchor).await.unwrap();
        let asset_height = state.check_asset_anchor(&asset_anchor).await.unwrap();
        assert_eq!(user_height, Some(1));
        assert_eq!(asset_height, Some(1));
    }

    #[tokio::test]
    async fn test_invalid_anchor_rejected() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

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
    async fn test_historical_anchors_preserved() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        let mut rng = rand::thread_rng();

        // Set initial block height and record anchors at height 1 (empty state)
        state.put_block_height(1);
        state.record_compliance_anchors(1).await.unwrap();
        let anchor_at_1 = state.get_user_tree_root().await.unwrap();

        // Add a user and record at height 2
        state.put_block_height(2);
        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(100u64)),
        };
        state.add_compliance_leaf(leaf).await.unwrap();
        state.record_compliance_anchors(2).await.unwrap();
        let anchor_at_2 = state.get_user_tree_root().await.unwrap();

        // Both anchors should be different
        assert_ne!(anchor_at_1.0, anchor_at_2.0);

        // Both should be valid (current height is 2, both anchors within window)
        let asset_anchor = state.get_asset_imt_root().await.unwrap();
        assert!(state
            .validate_compliance_anchors(&anchor_at_1, &asset_anchor)
            .await
            .is_ok());
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
    async fn test_anchor_too_old_rejected() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        let mut rng = rand::thread_rng();

        // Set initial height and record anchor
        state.put_block_height(1);
        state.record_compliance_anchors(1).await.unwrap();
        let old_user_anchor = state.get_user_tree_root().await.unwrap();
        let old_asset_anchor = state.get_asset_imt_root().await.unwrap();

        // Add something to change the tree roots (so old anchors remain distinct)
        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rng),
            key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
            asset_id: asset::Id(Fq::from(9999u64)),
        };
        state.add_compliance_leaf(leaf).await.unwrap();

        // Advance to height just past the window (MAX_ANCHOR_AGE_BLOCKS + 2)
        let new_height = 1 + MAX_ANCHOR_AGE_BLOCKS + 1;
        state.put_block_height(new_height);
        state.record_compliance_anchors(new_height).await.unwrap();

        // New anchors should be different
        let new_user_anchor = state.get_user_tree_root().await.unwrap();
        assert_ne!(
            old_user_anchor.0, new_user_anchor.0,
            "Anchors should differ after adding leaf"
        );

        // Validation of old anchors should fail (they're too old)
        let result = state
            .validate_compliance_anchors(&old_user_anchor, &old_asset_anchor)
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("too old"),
            "Error should mention 'too old': {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_anchor_within_window_accepted() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Set initial height and record anchor
        state.put_block_height(1);
        state.record_compliance_anchors(1).await.unwrap();
        let user_anchor = state.get_user_tree_root().await.unwrap();
        let asset_anchor = state.get_asset_imt_root().await.unwrap();

        // Advance to height within the window (MAX_ANCHOR_AGE_BLOCKS / 2)
        let new_height = 1 + MAX_ANCHOR_AGE_BLOCKS / 2;
        state.put_block_height(new_height);
        state.record_compliance_anchors(new_height).await.unwrap();

        // Validation of old anchors should succeed (within window)
        let result = state
            .validate_compliance_anchors(&user_anchor, &asset_anchor)
            .await;

        assert!(
            result.is_ok(),
            "Anchors within window should be valid: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_genesis_anchor_attack_prevented() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Genesis: IMT is empty, record anchor at height 0
        state.put_block_height(0);
        state.record_compliance_anchors(0).await.unwrap();
        let genesis_user_anchor = state.get_user_tree_root().await.unwrap();
        let genesis_asset_anchor = state.get_asset_imt_root().await.unwrap();

        // Register USDC as regulated at height 100
        state.put_block_height(100);
        let usdc_id = asset::Id(Fq::from(12345u64));
        state
            .register_regulated_asset(usdc_id, decaf377::Element::GENERATOR)
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
        let attack_height = MAX_ANCHOR_AGE_BLOCKS + 1;
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

        // Should fail because genesis anchor is > MAX_ANCHOR_AGE_BLOCKS blocks old
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("too old"),
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
}
