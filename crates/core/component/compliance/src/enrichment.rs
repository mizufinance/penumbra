//! Transaction plan compliance enrichment trait.
//!
//! This module defines the trait for compliance proof providers. The actual
//! enrichment function lives in the crates that have access to TransactionPlan
//! (view, mock-client) since compliance cannot depend on transaction.

use anyhow::Result;
use async_trait::async_trait;
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use penumbra_sdk_tct::StateCommitment;
use std::collections::BTreeMap;

use crate::{indexed_tree::IndexedLeaf, structs::AssetPolicy, ComplianceLeaf, MerklePath};

/// Result of a batch compliance query, containing all data needed for enrichment.
#[derive(Debug, Clone)]
pub struct BatchComplianceData {
    /// Compliance tree anchor (user tree root)
    pub compliance_anchor: StateCommitment,
    /// Asset tree anchor
    pub asset_anchor: StateCommitment,
    /// Per-asset proof data: (merkle_path, position, indexed_leaf, is_regulated)
    pub asset_proofs: BTreeMap<asset::Id, (MerklePath, u64, IndexedLeaf, bool)>,
    /// Per-(address, asset) user proof data: (merkle_path, position, leaf)
    pub user_proofs: BTreeMap<(Address, asset::Id), (MerklePath, u64, ComplianceLeaf)>,
    /// Per-asset policy data (threshold and issuer DK_pub)
    pub asset_policies: BTreeMap<asset::Id, AssetPolicy>,
}

impl Default for BatchComplianceData {
    fn default() -> Self {
        use decaf377::Fq;
        Self {
            compliance_anchor: StateCommitment(Fq::from(0u64)),
            asset_anchor: StateCommitment(Fq::from(0u64)),
            asset_proofs: BTreeMap::new(),
            user_proofs: BTreeMap::new(),
            asset_policies: BTreeMap::new(),
        }
    }
}

/// Provides compliance proofs and leaves from either a ViewClient (production)
/// or StateRead (tests). This trait abstracts over the data source so the
/// enrichment logic can be shared.
#[async_trait]
pub trait ComplianceProofProvider: Send + Sync {
    /// Get the user compliance tree root (anchor) as StateCommitment.
    async fn get_compliance_anchor(&self) -> Result<StateCommitment>;

    /// Get the asset compliance tree root (anchor) as StateCommitment.
    async fn get_asset_anchor(&self) -> Result<StateCommitment>;

    /// Get asset proof information: (merkle_path, position, indexed_leaf, is_regulated).
    /// For IMT, the indexed_leaf is used for membership/non-membership proofs.
    async fn get_asset_proof(
        &self,
        asset_id: asset::Id,
    ) -> Result<(MerklePath, u64, IndexedLeaf, bool)>;

    /// Get user proof and leaf: (merkle_path, position, leaf).
    /// For unregulated assets, implementations should return a synthetic leaf with BLACK_HOLE_ACK.
    /// Returns error if user is not registered for this asset.
    async fn get_user_proof(
        &self,
        address: &Address,
        asset_id: asset::Id,
    ) -> Result<(MerklePath, u64, ComplianceLeaf)>;

    /// Batch fetch all compliance data for multiple (address, asset) pairs.
    ///
    /// This is more efficient than individual calls because it makes a single
    /// gRPC request and fetches the tree anchors only once.
    ///
    /// The default implementation falls back to individual calls. Implementations
    /// that have access to a batch endpoint (like ViewClient) should override this.
    async fn get_batch_proofs(
        &self,
        queries: &[(Address, asset::Id)],
    ) -> Result<BatchComplianceData> {
        let compliance_anchor = self.get_compliance_anchor().await?;
        let asset_anchor = self.get_asset_anchor().await?;

        let mut asset_proofs = BTreeMap::new();
        let mut user_proofs = BTreeMap::new();

        for (address, asset_id) in queries {
            if !asset_proofs.contains_key(asset_id) {
                let proof = self.get_asset_proof(*asset_id).await?;
                asset_proofs.insert(*asset_id, proof);
            }

            let key = (address.clone(), *asset_id);
            if !user_proofs.contains_key(&key) {
                let proof = self.get_user_proof(address, *asset_id).await?;
                user_proofs.insert(key, proof);
            }
        }

        // Fetch asset policies for regulated assets
        let mut asset_policies = BTreeMap::new();
        for (asset_id, (_, _, _, is_regulated)) in &asset_proofs {
            if *is_regulated {
                if let Some(policy) = self.get_asset_policy(*asset_id).await? {
                    asset_policies.insert(*asset_id, policy);
                }
            }
        }

        Ok(BatchComplianceData {
            compliance_anchor,
            asset_anchor,
            asset_proofs,
            user_proofs,
            asset_policies,
        })
    }

    /// Get the asset policy (threshold and DK_pub) for a regulated asset.
    /// Returns None if the asset has no policy set.
    async fn get_asset_policy(&self, asset_id: asset::Id) -> Result<Option<AssetPolicy>>;
}
