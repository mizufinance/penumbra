//! Batched compliance witnesses for completing wallet plans.

use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::Address;
use shieldd_sdk_tct::StateCommitment;
use std::collections::BTreeMap;

use crate::{indexed_tree::IndexedLeaf, structs::AssetPolicy, ComplianceLeaf, MerklePath};

#[derive(Clone, Debug)]
pub struct ComplianceQuery {
    pub address: Address,
    pub asset_id: asset::Id,
}

/// Proof data for an asset in the indexed asset tree.
#[derive(Clone, Debug)]
pub struct AssetProofData {
    /// The indexed leaf used for membership or non-membership proofs.
    pub indexed_leaf: IndexedLeaf,
    /// Position of the leaf in the tree.
    pub position: u64,
    /// Authentication path from leaf to root.
    pub auth_path: MerklePath,
    /// Whether the asset is regulated according to stored policy metadata.
    pub is_regulated: bool,
}

/// Proof data for a user compliance leaf.
#[derive(Clone, Debug)]
pub struct UserProofData {
    /// Authentication path from leaf to root.
    pub auth_path: MerklePath,
    /// Position of the leaf in the user tree.
    pub position: u64,
    /// Compliance leaf proven by the path.
    pub leaf: ComplianceLeaf,
}

/// Result of a batch compliance query, containing all data needed for enrichment.
#[derive(Debug, Clone)]
pub struct BatchComplianceData {
    /// Compliance tree anchor (user tree root)
    pub compliance_anchor: StateCommitment,
    /// Asset tree anchor
    pub asset_anchor: StateCommitment,
    /// Per-asset proof data.
    pub asset_proofs: BTreeMap<asset::Id, AssetProofData>,
    /// Per-asset policy data for regulated assets.
    pub asset_policies: BTreeMap<asset::Id, AssetPolicy>,
    /// Per-(address, asset) user proof data.
    pub user_proofs: BTreeMap<(Address, asset::Id), UserProofData>,
}

impl Default for BatchComplianceData {
    fn default() -> Self {
        use decaf377::Fq;
        Self {
            compliance_anchor: StateCommitment(Fq::from(0u64)),
            asset_anchor: StateCommitment(Fq::from(0u64)),
            asset_proofs: BTreeMap::new(),
            asset_policies: BTreeMap::new(),
            user_proofs: BTreeMap::new(),
        }
    }
}
