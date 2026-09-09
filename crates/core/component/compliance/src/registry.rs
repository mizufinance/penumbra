use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use decaf377::Fq;
use decaf377_rdsa::{SpendAuth, VerificationKey};
use futures::StreamExt;
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::ensure_nonidentity_spend_auth_key;
use shieldd_sdk_proto::{DomainType as _, StateReadProto, StateWriteProto};
use shieldd_sdk_tct::StateCommitment;
use std::collections::BTreeMap;

use crate::{
    event, indexed_tree,
    indexed_tree::{
        FqOrdKey, IndexedLeaf, IndexedMerkleTree, InsertResult, FQ_MAX, IMT_ZERO_HASHES,
    },
    params::StateReadExt as _,
    state_key,
    structs::{AssetPolicy, ComplianceLeaf, MerklePath, UserAssetStatus, UserAssetStatusAction},
    tree::{QuadTree, ZERO_HASHES},
};

fn ensure_regulated_asset_id(asset_id: asset::Id, is_regulated: bool) -> Result<()> {
    anyhow::ensure!(
        !is_regulated || asset_id != *shieldd_sdk_asset::BASE_ASSET_ID,
        "the base fee asset cannot be registered as regulated"
    );
    Ok(())
}

fn root_from_auth_path(
    mut position: u64,
    mut current: StateCommitment,
    path: &[[StateCommitment; 3]],
    hash_children: fn(
        StateCommitment,
        StateCommitment,
        StateCommitment,
        StateCommitment,
    ) -> StateCommitment,
) -> StateCommitment {
    for siblings in path {
        let children = match position % 4 {
            0 => [current, siblings[0], siblings[1], siblings[2]],
            1 => [siblings[0], current, siblings[1], siblings[2]],
            2 => [siblings[0], siblings[1], current, siblings[2]],
            3 => [siblings[0], siblings[1], siblings[2], current],
            _ => unreachable!(),
        };
        current = hash_children(children[0], children[1], children[2], children[3]);
        position /= 4;
    }
    current
}

// Note: QuadTree is still used for the user tree. Asset tree has been migrated to IMT.

/// Maximum number of blocks the RPC will search backwards for a recorded anchor.
pub const MAX_ANCHOR_SEARCH_DEPTH_BLOCKS: u64 = 10;

/// Maximum allowed drift between target_timestamp and block timestamp (±30 minutes).
pub const MAX_TIMESTAMP_DRIFT_SECS: u64 = 1_800;

/// Verify a target timestamp against a signed consensus block timestamp.
pub fn check_timestamp_freshness(
    target_timestamp: u64,
    consensus_timestamp_unix: i64,
) -> Result<()> {
    let block_timestamp = u64::try_from(consensus_timestamp_unix).map_err(|_| {
        anyhow::anyhow!("consensus block timestamp {consensus_timestamp_unix} must be nonnegative")
    })?;
    anyhow::ensure!(target_timestamp != 0, "target_timestamp must be nonzero");
    let diff = target_timestamp.abs_diff(block_timestamp);
    anyhow::ensure!(
        diff <= MAX_TIMESTAMP_DRIFT_SECS,
        "target_timestamp {target_timestamp} is {diff}s from block time {block_timestamp}, exceeds ±{MAX_TIMESTAMP_DRIFT_SECS}s"
    );
    Ok(())
}

/// Require the current mutable user-status and asset-policy roots.
pub fn validate_compliance_anchor_facts(
    user_anchor: &StateCommitment,
    current_user_anchor: &StateCommitment,
    asset_anchor: &StateCommitment,
    current_asset_anchor: &StateCommitment,
) -> Result<()> {
    anyhow::ensure!(
        user_anchor == current_user_anchor,
        "user compliance anchor does not match the current user compliance root"
    );
    anyhow::ensure!(
        asset_anchor == current_asset_anchor,
        "asset compliance anchor does not match the current asset compliance root"
    );
    Ok(())
}

// Re-export bincode for serialization
use bincode;

pub use crate::enrichment::AssetProofData;

/// A user-registry mutation admitted after the signed user grant is verified.
pub struct UserGrantAdmission {
    leaf: ComplianceLeaf,
}

impl UserGrantAdmission {
    /// Verify the complete signed grant and bind its leaf for durable insertion.
    pub(crate) fn verify(
        action: &crate::structs::MsgRegisterUser,
        policy: &AssetPolicy,
        current_unix: u64,
        chain_id: &str,
    ) -> Result<Self> {
        policy.validate_regulated()?;
        let grant = action
            .grant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing user registration grant"))?;
        anyhow::ensure!(
            grant.body.leaf == action.leaf,
            "user registration grant leaf does not match action leaf"
        );
        action.leaf.validate_registration(policy.ring.ring_pk)?;
        action
            .capability_certificate
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing Orbis capability certificate"))?
            .verify(&action.leaf, policy, chain_id)?;
        anyhow::ensure!(
            action.leaf.status == UserAssetStatus::Active,
            "user registrations must start active"
        );
        anyhow::ensure!(
            grant.body.policy_id == policy.ring.policy_id,
            "user registration grant policy_id does not match asset policy"
        );
        anyhow::ensure!(
            current_unix <= grant.body.valid_until_unix,
            "user registration grant expired"
        );
        let authority_vk = policy.registration_authority_vk.as_ref().ok_or_else(|| {
            anyhow::anyhow!("regulated asset policy missing registration authority")
        })?;
        grant.verify(authority_vk)?;
        Ok(Self {
            leaf: action.leaf.clone(),
        })
    }
}

/// A user-registry mutation admitted from validated genesis content.
pub struct GenesisUserAdmission {
    leaf: ComplianceLeaf,
}

impl GenesisUserAdmission {
    pub(crate) fn validate(
        registration: &crate::genesis::GenesisUserRegistration,
        policy: &AssetPolicy,
        chain_id: &str,
    ) -> Result<Self> {
        policy.validate_regulated()?;
        registration
            .capability_certificate
            .verify(&registration.leaf, policy, chain_id)?;
        anyhow::ensure!(
            registration.leaf.status == UserAssetStatus::Active,
            "genesis compliance users must start active"
        );
        Ok(Self {
            leaf: registration.leaf.clone(),
        })
    }
}

/// An asset-registry mutation admitted after the registrar grant is verified.
pub struct AssetGrantAdmission {
    asset_id: asset::Id,
    policy: AssetPolicy,
    is_regulated: bool,
}

impl AssetGrantAdmission {
    /// Verify the registrar grant and construct the exact admitted policy.
    pub(crate) fn verify(
        action: &crate::structs::MsgRegisterAsset,
        registrar_authorized: bool,
        current_unix: u64,
    ) -> Result<Self> {
        ensure_regulated_asset_id(action.asset_id, action.is_regulated)?;
        action.validate_authorization_keys()?;
        let grant = action
            .asset_registration_grant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing asset registration grant"))?;
        anyhow::ensure!(
            grant.body == action.registration_grant_body(grant.body.valid_until_unix),
            "asset registration grant body does not match action"
        );
        grant.verify()?;
        anyhow::ensure!(
            registrar_authorized,
            "asset registration grant signed by unauthorized registrar"
        );
        anyhow::ensure!(
            current_unix <= grant.body.valid_until_unix,
            "asset registration grant expired"
        );

        let policy = if action.is_regulated {
            let dk_pub = action.dk_pub.ok_or_else(|| {
                anyhow::anyhow!("regulated assets require a detection key (dk_pub)")
            })?;
            let registration_authority_vk = action.registration_authority_vk.ok_or_else(|| {
                anyhow::anyhow!("regulated assets require registration_authority_vk")
            })?;
            let seizure_authority_vk = action
                .seizure_authority_vk
                .ok_or_else(|| anyhow::anyhow!("regulated assets require seizure_authority_vk"))?;
            let daily_volume_limit = action.daily_volume_limit.unwrap_or(u128::MAX);
            let ring_pk = action
                .ring_pk
                .ok_or_else(|| anyhow::anyhow!("regulated assets require ring_pk"))?;
            AssetPolicy::new(
                dk_pub,
                daily_volume_limit,
                action.allowed_ibc_routes.clone(),
                action.ibc_origin.clone(),
                action.ring_id.clone(),
                ring_pk,
                action.policy_id.clone(),
                action.permission.clone(),
                action.resource.clone(),
            )
            .with_registration_authority(registration_authority_vk)
            .with_seizure_authority(seizure_authority_vk)
        } else {
            anyhow::ensure!(
                action.dk_pub.is_none(),
                "unregulated assets cannot set dk_pub"
            );
            anyhow::ensure!(
                action.daily_volume_limit.is_none(),
                "unregulated assets cannot set a compliance daily_volume_limit"
            );
            anyhow::ensure!(
                action.allowed_ibc_routes.is_empty(),
                "unregulated assets cannot set allowed IBC routes"
            );
            anyhow::ensure!(
                action.ibc_origin.is_none(),
                "unregulated assets cannot set IBC origin"
            );
            anyhow::ensure!(
                action.ring_pk.is_none()
                    && action.ring_id.is_empty()
                    && action.policy_id.is_empty()
                    && action.permission.is_empty()
                    && action.resource.is_empty(),
                "unregulated assets cannot set Orbis configuration"
            );
            anyhow::ensure!(
                action.registration_authority_vk.is_none() && action.seizure_authority_vk.is_none(),
                "unregulated assets cannot set compliance authorities"
            );
            AssetPolicy::default_unregulated()
        };
        if action.is_regulated {
            policy.validate_regulated()?;
        } else {
            policy.validate_crypto_keys()?;
        }
        Ok(Self {
            asset_id: action.asset_id,
            policy,
            is_regulated: action.is_regulated,
        })
    }

    pub(crate) fn policy(&self) -> &AssetPolicy {
        &self.policy
    }
}

/// An asset-registry mutation admitted from validated genesis content.
pub struct GenesisAssetAdmission {
    asset_id: asset::Id,
    policy: AssetPolicy,
    is_regulated: bool,
}

impl GenesisAssetAdmission {
    pub(crate) fn validate(
        asset_id: asset::Id,
        policy: AssetPolicy,
        is_regulated: bool,
    ) -> Result<Self> {
        ensure_regulated_asset_id(asset_id, is_regulated)?;
        anyhow::ensure!(
            asset_id.0 != Fq::from(0u64),
            "genesis asset ID zero is reserved"
        );
        if is_regulated {
            anyhow::ensure!(
                policy.registration_authority_vk.is_some(),
                "regulated genesis asset requires a registration authority"
            );
            anyhow::ensure!(
                policy.seizure_authority_vk.is_some(),
                "regulated genesis asset requires a seizure authority"
            );
            policy.validate_regulated()?;
        } else {
            anyhow::ensure!(
                policy == AssetPolicy::default_unregulated(),
                "unregulated genesis asset must use the canonical default policy"
            );
        }
        Ok(Self {
            asset_id,
            policy,
            is_regulated,
        })
    }
}

/// Compact proof-service index for one registered compliance leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserLeafRecord {
    /// Position of the authenticated leaf in the compliance user tree.
    pub position: u64,
    /// Full leaf data needed to construct a user proof.
    pub leaf: ComplianceLeaf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredUserLeafRecord {
    position: u64,
    leaf: Vec<u8>,
}

fn encode_user_leaf_record(record: &UserLeafRecord) -> Result<Vec<u8>> {
    bincode::serialize(&StoredUserLeafRecord {
        position: record.position,
        leaf: record.leaf.encode_to_vec(),
    })
    .context("encode compliance user record")
}

fn decode_user_leaf_record(bytes: &[u8]) -> Result<UserLeafRecord> {
    let stored: StoredUserLeafRecord =
        bincode::deserialize(bytes).context("decode stored compliance user record envelope")?;
    Ok(UserLeafRecord {
        position: stored.position,
        leaf: ComplianceLeaf::decode(stored.leaf.as_slice())
            .context("decode stored compliance user leaf")?,
    })
}

fn decode_commitment(bytes: Vec<u8>) -> Result<StateCommitment> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow::anyhow!(
            "stored compliance tree commitment must be 32 bytes, got {}",
            bytes.len()
        )
    })?;
    Ok(StateCommitment(Fq::from_bytes_checked(&bytes).map_err(
        |_| anyhow::anyhow!("stored compliance tree commitment is not a field element"),
    )?))
}

fn encode_asset_id(asset_id: asset::Id) -> Vec<u8> {
    asset_id.0.to_bytes().to_vec()
}

fn decode_asset_id(bytes: Vec<u8>) -> Result<asset::Id> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow::anyhow!("stored asset id must be 32 bytes, got {}", bytes.len())
    })?;
    Ok(asset::Id(Fq::from_bytes_checked(&bytes).map_err(|_| {
        anyhow::anyhow!("stored asset id is not a field element")
    })?))
}

fn encode_position(position: u64) -> Vec<u8> {
    position.to_be_bytes().to_vec()
}

fn decode_position(bytes: Vec<u8>) -> Result<u64> {
    let bytes: [u8; 8] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow::anyhow!("stored tree position must be 8 bytes, got {}", bytes.len())
    })?;
    Ok(u64::from_be_bytes(bytes))
}

fn asset_value_desc_key(value: Fq) -> String {
    state_key::tree_storage::asset_value_desc(FqOrdKey::descending_bytes(value))
}

fn parse_node_key(key: &[u8], prefix: &str) -> Result<(u8, u64)> {
    let key = std::str::from_utf8(key)?;
    let suffix = key.strip_prefix(prefix).unwrap_or(key);
    let (level, position) = suffix
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("compliance tree node key missing position: {key}"))?;
    Ok((level.parse()?, position.parse()?))
}

fn parse_leaf_key(key: &[u8], prefix: &str) -> Result<u64> {
    let key = std::str::from_utf8(key)?;
    let suffix = key.strip_prefix(prefix).unwrap_or(key);
    Ok(suffix.parse()?)
}

/// Extension trait for reading compliance registry state.
#[async_trait]
pub trait ComplianceRegistryRead: StateRead {
    /// Load user-tree nodes from nonverifiable storage.
    async fn load_user_tree_nodes(&self) -> Result<BTreeMap<u64, StateCommitment>> {
        let mut nodes = BTreeMap::new();
        let stream = self.nonverifiable_range_raw(
            Some(state_key::tree_storage::user_node_prefix().as_bytes()),
            Vec::new()..,
        )?;
        futures::pin_mut!(stream);
        while let Some((key, bytes)) = stream.next().await.transpose()? {
            let (level, position) =
                parse_node_key(&key, state_key::tree_storage::user_node_prefix())?;
            nodes.insert(
                QuadTree::packed_node_key(level, position),
                decode_commitment(bytes)?,
            );
        }
        Ok(nodes)
    }

    /// Reconstruct the user compliance tree from nonverifiable storage.
    async fn reconstruct_user_tree(&self) -> Result<QuadTree> {
        let nodes = self.load_user_tree_nodes().await?;
        if nodes.is_empty() {
            Ok(QuadTree::new())
        } else {
            QuadTree::try_from_sparse_nodes(crate::tree::DEFAULT_DEPTH, nodes)
        }
    }

    /// Load asset IMT nodes from nonverifiable storage.
    async fn load_asset_imt_nodes(&self) -> Result<BTreeMap<u64, StateCommitment>> {
        let mut nodes = BTreeMap::new();
        let stream = self.nonverifiable_range_raw(
            Some(state_key::tree_storage::asset_node_prefix().as_bytes()),
            Vec::new()..,
        )?;
        futures::pin_mut!(stream);
        while let Some((key, bytes)) = stream.next().await.transpose()? {
            let (level, position) =
                parse_node_key(&key, state_key::tree_storage::asset_node_prefix())?;
            nodes.insert(
                IndexedMerkleTree::packed_node_key(level, position),
                decode_commitment(bytes)?,
            );
        }
        Ok(nodes)
    }

    /// Load asset IMT leaves from nonverifiable storage.
    async fn load_asset_imt_leaves(&self) -> Result<BTreeMap<u64, IndexedLeaf>> {
        let mut leaves = BTreeMap::new();
        let stream = self.nonverifiable_range_raw(
            Some(state_key::tree_storage::asset_leaf_prefix().as_bytes()),
            Vec::new()..,
        )?;
        futures::pin_mut!(stream);
        while let Some((key, bytes)) = stream.next().await.transpose()? {
            let position = parse_leaf_key(&key, state_key::tree_storage::asset_leaf_prefix())?;
            leaves.insert(position, bincode::deserialize(&bytes)?);
        }
        Ok(leaves)
    }

    /// Reconstruct the asset indexed Merkle tree from nonverifiable storage.
    async fn reconstruct_asset_tree(&self) -> Result<IndexedMerkleTree> {
        let nodes = self.load_asset_imt_nodes().await?;
        let leaves = self.load_asset_imt_leaves().await?;
        if leaves.is_empty() {
            anyhow::ensure!(
                nodes.is_empty(),
                "asset IMT has stored nodes without stored leaves"
            );
            anyhow::ensure!(
                self.get_asset_count().await? == 0,
                "asset IMT count is nonzero but no leaves are stored"
            );
            Ok(IndexedMerkleTree::new())
        } else {
            let leaf_count = self.get_asset_count().await?;
            IndexedMerkleTree::try_from_sparse_parts(
                crate::tree::DEFAULT_DEPTH,
                nodes,
                leaves,
                leaf_count,
            )
        }
    }

    /// Get the asset IMT root hash.
    async fn get_asset_imt_root(&self) -> Result<StateCommitment> {
        self.get(state_key::asset_imt_root())
            .await?
            .context("asset IMT is missing its committed root")
    }

    async fn read_user_node(&self, level: u8, position: u64) -> Result<StateCommitment> {
        anyhow::ensure!(
            level <= crate::tree::DEFAULT_DEPTH,
            "user tree level {level} exceeds depth {}",
            crate::tree::DEFAULT_DEPTH
        );
        self.nonverifiable_get_raw(state_key::tree_storage::user_node(level, position).as_bytes())
            .await?
            .map(decode_commitment)
            .transpose()
            .map(|node| node.unwrap_or(ZERO_HASHES[level as usize]))
    }

    async fn read_asset_node(&self, level: u8, position: u64) -> Result<StateCommitment> {
        anyhow::ensure!(
            level <= crate::tree::DEFAULT_DEPTH,
            "asset tree level {level} exceeds depth {}",
            crate::tree::DEFAULT_DEPTH
        );
        self.nonverifiable_get_raw(state_key::tree_storage::asset_node(level, position).as_bytes())
            .await?
            .map(decode_commitment)
            .transpose()
            .map(|node| node.unwrap_or(IMT_ZERO_HASHES[level as usize]))
    }

    async fn read_asset_leaf(&self, position: u64) -> Result<IndexedLeaf> {
        if let Some(bytes) = self
            .nonverifiable_get_raw(state_key::tree_storage::asset_leaf(position).as_bytes())
            .await?
        {
            return Ok(bincode::deserialize(&bytes)?);
        }
        if position == 0 && self.get_asset_count().await? <= 1 {
            return Ok(IndexedLeaf::with_default_policy(Fq::from(0u64), 0, *FQ_MAX));
        }
        anyhow::bail!("missing asset IMT leaf at position {position}")
    }

    async fn read_user_auth_path_direct(&self, position: u64) -> Result<Vec<[StateCommitment; 3]>> {
        let max_leaves = QuadTree::max_leaves_for_depth(crate::tree::DEFAULT_DEPTH);
        anyhow::ensure!(
            position < max_leaves,
            "Position {position} exceeds maximum leaves {max_leaves} for depth {}",
            crate::tree::DEFAULT_DEPTH
        );

        let mut path = Vec::with_capacity(crate::tree::DEFAULT_DEPTH as usize);
        let mut current_position = position;
        for level in 0..crate::tree::DEFAULT_DEPTH {
            let child_index = (current_position % 4) as usize;
            let base_position = (current_position / 4) * 4;
            let children = [
                self.read_user_node(level, base_position).await?,
                self.read_user_node(level, base_position + 1).await?,
                self.read_user_node(level, base_position + 2).await?,
                self.read_user_node(level, base_position + 3).await?,
            ];
            path.push(match child_index {
                0 => [children[1], children[2], children[3]],
                1 => [children[0], children[2], children[3]],
                2 => [children[0], children[1], children[3]],
                3 => [children[0], children[1], children[2]],
                _ => unreachable!(),
            });
            current_position /= 4;
        }
        Ok(path)
    }

    async fn read_asset_auth_path_direct(
        &self,
        position: u64,
    ) -> Result<Vec<[StateCommitment; 3]>> {
        let max_leaves = QuadTree::max_leaves_for_depth(crate::tree::DEFAULT_DEPTH);
        anyhow::ensure!(
            position < max_leaves,
            "Position {position} exceeds maximum leaves {max_leaves} for depth {}",
            crate::tree::DEFAULT_DEPTH
        );

        let mut path = Vec::with_capacity(crate::tree::DEFAULT_DEPTH as usize);
        let mut current_position = position;
        for level in 0..crate::tree::DEFAULT_DEPTH {
            let child_index = (current_position % 4) as usize;
            let base_position = (current_position / 4) * 4;
            let children = [
                self.read_asset_node(level, base_position).await?,
                self.read_asset_node(level, base_position + 1).await?,
                self.read_asset_node(level, base_position + 2).await?,
                self.read_asset_node(level, base_position + 3).await?,
            ];
            path.push(match child_index {
                0 => [children[1], children[2], children[3]],
                1 => [children[0], children[2], children[3]],
                2 => [children[0], children[1], children[3]],
                3 => [children[0], children[1], children[2]],
                _ => unreachable!(),
            });
            current_position /= 4;
        }
        Ok(path)
    }

    async fn read_asset_position_by_value(&self, value: Fq) -> Result<Option<u64>> {
        self.nonverifiable_get_raw(asset_value_desc_key(value).as_bytes())
            .await?
            .map(decode_position)
            .transpose()
    }

    async fn read_low_asset_position(&self, value: Fq) -> Result<Option<u64>> {
        let start = hex::encode(FqOrdKey::descending_bytes(value)).into_bytes();
        let stream = self.nonverifiable_range_raw(
            Some(state_key::tree_storage::asset_value_desc_prefix().as_bytes()),
            start..,
        )?;
        futures::pin_mut!(stream);
        match stream.next().await.transpose()? {
            Some((_key, bytes)) => Ok(Some(decode_position(bytes)?)),
            None => {
                if self.get_asset_count().await? <= 1 {
                    Ok(Some(0))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Verify that compliance trees materialized in NV storage match committed roots.
    async fn verify_committed_tree_roots(&self) -> Result<()> {
        let user_root = self.get_user_tree_root().await?;
        let asset_root = self.get_asset_imt_root().await?;
        anyhow::ensure!(
            self.reconstruct_user_tree().await?.root() == user_root,
            "compliance user tree root mismatch"
        );
        anyhow::ensure!(
            self.reconstruct_asset_tree().await?.root() == asset_root,
            "compliance asset IMT root mismatch"
        );
        Ok(())
    }

    /// Get proof data for an asset using the IMT.
    ///
    /// For regulated assets: returns membership proof (exact match).
    /// For unregulated assets: returns non-membership proof (gap).
    async fn get_asset_proof_data(&self, asset_id: asset::Id) -> Result<AssetProofData> {
        let value = asset_id.0;
        anyhow::ensure!(
            value != Fq::from(0u64),
            "asset value zero is reserved for sentinel leaf"
        );
        let is_regulated = self.is_asset_regulated(asset_id).await?;
        let root = self.get_asset_imt_root().await?;

        if is_regulated {
            let position = self
                .read_asset_position_by_value(value)
                .await?
                .ok_or_else(|| anyhow::anyhow!("regulated asset has no IMT membership leaf"))?;
            let indexed_leaf = self.read_asset_leaf(position).await?;
            anyhow::ensure!(
                indexed_leaf.value == value,
                "asset value index inconsistent: key for {:?} points to leaf {:?} at position {position}",
                value.to_bytes(),
                indexed_leaf.value.to_bytes()
            );
            let path = self.read_asset_auth_path_direct(position).await?;
            anyhow::ensure!(
                IndexedMerkleTree::verify_auth_path(
                    position,
                    &indexed_leaf,
                    &path,
                    root,
                    crate::tree::DEFAULT_DEPTH
                ),
                "asset IMT direct membership path does not verify against committed root"
            );
            return Ok(AssetProofData {
                indexed_leaf,
                position,
                auth_path: MerklePath::from_auth_path(path),
                is_regulated,
            });
        }

        anyhow::ensure!(
            self.read_asset_position_by_value(value).await?.is_none(),
            "unregulated asset has an explicit IMT leaf; unregulated proofs require non-membership"
        );

        let position = self
            .read_low_asset_position(value)
            .await?
            .ok_or_else(|| anyhow::anyhow!("asset value index has no predecessor for target"))?;
        let indexed_leaf = self.read_asset_leaf(position).await?;
        let value_key = FqOrdKey::from(value);
        anyhow::ensure!(
            FqOrdKey::from(indexed_leaf.value) < value_key
                && value_key < FqOrdKey::from(indexed_leaf.next_value),
            "asset value index inconsistent: target {:?} not in leaf gap [{:?}, {:?}) at position {position}",
            value.to_bytes(),
            indexed_leaf.value.to_bytes(),
            indexed_leaf.next_value.to_bytes()
        );
        let path = self.read_asset_auth_path_direct(position).await?;
        anyhow::ensure!(
            IndexedMerkleTree::verify_auth_path(
                position,
                &indexed_leaf,
                &path,
                root,
                crate::tree::DEFAULT_DEPTH
            ),
            "asset IMT direct non-membership path does not verify against committed root"
        );
        Ok(AssetProofData {
            indexed_leaf,
            position,
            auth_path: MerklePath::from_auth_path(path),
            is_regulated,
        })
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

    /// Return whether a verification key is authorized to register compliance assets.
    async fn is_compliance_registrar(&self, vk: &VerificationKey<SpendAuth>) -> Result<bool> {
        ensure_nonidentity_spend_auth_key(vk, "compliance registrar key")?;
        Ok(self
            .get_raw(&state_key::compliance_registrar_vk(vk))
            .await?
            .is_some())
    }

    /// Get the compliance policy for an asset.
    ///
    /// Returns the issuer's detection key and daily_volume_limit for flagged transfers.
    /// Returns `None` if no policy is set (asset uses default behavior).
    async fn get_asset_policy(&self, asset_id: asset::Id) -> Result<Option<AssetPolicy>> {
        let key = state_key::asset_policy(&asset_id);
        match self.get_raw(&key).await? {
            Some(bytes) => Ok(Some(AssetPolicy::from_bytes(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn get_ibc_origin_asset_id(&self, base_denom: &str) -> Result<Option<asset::Id>> {
        match self
            .get_raw(&state_key::ibc_origin_asset(base_denom))
            .await?
        {
            Some(bytes) => Ok(Some(decode_asset_id(bytes)?)),
            None => Ok(None),
        }
    }

    /// Fast regulated-asset check for action gating.
    ///
    /// This is intentionally cheaper than `get_asset_proof_data`: it avoids IMT
    /// deserialization and path construction, and relies on the invariant that
    /// every regulated asset has an `asset_policy` entry.
    async fn is_asset_regulated(&self, asset_id: asset::Id) -> Result<bool> {
        let key = state_key::asset_policy(&asset_id);
        Ok(self.get_raw(&key).await?.is_some())
    }

    /// Get the user tree root hash.
    async fn get_user_tree_root(&self) -> Result<StateCommitment> {
        self.get(state_key::user_tree_root())
            .await?
            .context("user tree is missing its committed root")
    }

    /// Get an authentication path for a user at the given position.
    async fn get_user_auth_path(&self, position: u64) -> Result<Vec<[StateCommitment; 3]>> {
        self.read_user_auth_path_direct(position).await
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
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<Option<u64>> {
        Ok(self
            .get_user_leaf_record(address, asset_id)
            .await?
            .map(|record| record.position))
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
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<Option<ComplianceLeaf>> {
        Ok(self
            .get_user_leaf_record(address, asset_id)
            .await?
            .map(|record| record.leaf))
    }

    /// Load and authenticate the consensus record for a registered user.
    async fn get_user_leaf_record(
        &self,
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<Option<UserLeafRecord>> {
        let key = state_key::user_leaf_record(address, &asset_id);
        let Some(bytes) = self.get_raw(&key).await? else {
            return Ok(None);
        };
        let record =
            decode_user_leaf_record(&bytes).context("invalid stored compliance user record")?;
        anyhow::ensure!(
            record.leaf.address == *address && record.leaf.asset_id == asset_id,
            "stored compliance user record does not match its index key"
        );
        let stored_commitment = self.read_user_node(0, record.position).await?;
        anyhow::ensure!(
            stored_commitment == record.leaf.commit(),
            "stored compliance user record does not match the committed user tree"
        );
        Ok(Some(record))
    }

    async fn get_user_asset_position(
        &self,
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<Option<u64>> {
        self.get_proto(&state_key::user_asset_position(address, &asset_id))
            .await
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
    /// Returns `Ok(true)` if the indexed leaf matches the committed tree position,
    /// `Ok(false)` if not found.
    async fn verify_compliance_leaf(&self, leaf: &ComplianceLeaf) -> Result<bool> {
        Ok(self
            .get_user_leaf(&leaf.address, leaf.asset_id)
            .await?
            .is_some_and(|stored| stored == *leaf))
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

    /// Get the user tree anchor at a specific block height.
    async fn get_user_anchor_by_height(&self, height: u64) -> Result<Option<StateCommitment>> {
        self.get(&state_key::anchor::user_anchor_by_height(height))
            .await
    }

    /// Require the current user-status and asset-policy roots.
    async fn validate_compliance_anchors(
        &self,
        user_anchor: &StateCommitment,
        asset_anchor: &StateCommitment,
    ) -> Result<()> {
        let current_user_anchor = self.get_user_tree_root().await?;
        let current_asset_anchor = self.get_asset_imt_root().await?;
        validate_compliance_anchor_facts(
            user_anchor,
            &current_user_anchor,
            asset_anchor,
            &current_asset_anchor,
        )
    }
}

impl<T: StateRead + ?Sized> ComplianceRegistryRead for T {}

/// Internal durable registry operations.
#[async_trait]
trait ComplianceRegistryRawWrite: StateWrite + ComplianceRegistryRead {
    /// Persist touched user-tree nodes to nonverifiable storage.
    fn put_user_tree_nodes(&mut self, nodes: &[(u8, u64, StateCommitment)]) {
        for &(level, position, hash) in nodes {
            let key = state_key::tree_storage::user_node(level, position).into_bytes();
            if hash.0 == ZERO_HASHES[level as usize].0 {
                self.nonverifiable_delete(key);
            } else {
                self.nonverifiable_put_raw(key, hash.0.to_bytes().to_vec());
            }
        }
    }

    /// Persist touched asset-IMT nodes to nonverifiable storage.
    fn put_asset_imt_nodes(&mut self, nodes: &[(u8, u64, StateCommitment)]) {
        for &(level, position, hash) in nodes {
            let key = state_key::tree_storage::asset_node(level, position).into_bytes();
            if hash.0 == IMT_ZERO_HASHES[level as usize].0 {
                self.nonverifiable_delete(key);
            } else {
                self.nonverifiable_put_raw(key, hash.0.to_bytes().to_vec());
            }
        }
    }

    /// Persist an asset-IMT leaf to nonverifiable storage.
    fn put_asset_imt_leaf(&mut self, position: u64, leaf: &IndexedLeaf) -> Result<()> {
        self.nonverifiable_put_raw(
            state_key::tree_storage::asset_leaf(position).into_bytes(),
            bincode::serialize(leaf)?,
        );
        self.nonverifiable_put_raw(
            asset_value_desc_key(leaf.value).into_bytes(),
            encode_position(position),
        );
        Ok(())
    }

    async fn compute_user_path_updates(
        &self,
        updates: &[(u64, StateCommitment)],
    ) -> Result<Vec<(u8, u64, StateCommitment)>> {
        let mut overlay: BTreeMap<(u8, u64), StateCommitment> = BTreeMap::new();
        let mut touched =
            Vec::with_capacity(updates.len() * (crate::tree::DEFAULT_DEPTH as usize + 1));

        for &(position, leaf_hash) in updates {
            let mut current_position = position;
            let mut current_hash = leaf_hash;
            overlay.insert((0, current_position), current_hash);
            touched.push((0, current_position, current_hash));

            for level in 0..crate::tree::DEFAULT_DEPTH {
                let parent_position = current_position / 4;
                let base_position = parent_position * 4;
                let child_index = (current_position % 4) as usize;
                let mut children = [
                    overlay
                        .get(&(level, base_position))
                        .copied()
                        .unwrap_or(self.read_user_node(level, base_position).await?),
                    overlay
                        .get(&(level, base_position + 1))
                        .copied()
                        .unwrap_or(self.read_user_node(level, base_position + 1).await?),
                    overlay
                        .get(&(level, base_position + 2))
                        .copied()
                        .unwrap_or(self.read_user_node(level, base_position + 2).await?),
                    overlay
                        .get(&(level, base_position + 3))
                        .copied()
                        .unwrap_or(self.read_user_node(level, base_position + 3).await?),
                ];
                children[child_index] = current_hash;
                current_hash =
                    QuadTree::hash_children(children[0], children[1], children[2], children[3]);
                current_position = parent_position;
                overlay.insert((level + 1, current_position), current_hash);
                touched.push((level + 1, current_position, current_hash));
            }
        }

        Ok(touched)
    }

    async fn authenticate_user_leaf(&self, position: u64, leaf: StateCommitment) -> Result<()> {
        let path = self.read_user_auth_path_direct(position).await?;
        let committed = self.get_user_tree_root().await?;
        anyhow::ensure!(
            QuadTree::verify_auth_path(
                position,
                leaf,
                &path,
                committed,
                crate::tree::DEFAULT_DEPTH,
            ),
            "user tree path at position {position} does not authenticate to the committed root"
        );
        Ok(())
    }

    async fn compute_asset_path_updates(
        &self,
        updates: &[(u64, StateCommitment)],
    ) -> Result<Vec<(u8, u64, StateCommitment)>> {
        let mut overlay: BTreeMap<(u8, u64), StateCommitment> = BTreeMap::new();
        let mut touched =
            Vec::with_capacity(updates.len() * (crate::tree::DEFAULT_DEPTH as usize + 1));

        for &(position, leaf_hash) in updates {
            let mut current_position = position;
            let mut current_hash = leaf_hash;
            overlay.insert((0, current_position), current_hash);
            touched.push((0, current_position, current_hash));

            for level in 0..crate::tree::DEFAULT_DEPTH {
                let parent_position = current_position / 4;
                let base_position = parent_position * 4;
                let child_index = (current_position % 4) as usize;
                let mut children = [
                    overlay
                        .get(&(level, base_position))
                        .copied()
                        .unwrap_or(self.read_asset_node(level, base_position).await?),
                    overlay
                        .get(&(level, base_position + 1))
                        .copied()
                        .unwrap_or(self.read_asset_node(level, base_position + 1).await?),
                    overlay
                        .get(&(level, base_position + 2))
                        .copied()
                        .unwrap_or(self.read_asset_node(level, base_position + 2).await?),
                    overlay
                        .get(&(level, base_position + 3))
                        .copied()
                        .unwrap_or(self.read_asset_node(level, base_position + 3).await?),
                ];
                children[child_index] = current_hash;
                current_hash = IndexedMerkleTree::hash_children(
                    children[0],
                    children[1],
                    children[2],
                    children[3],
                );
                current_position = parent_position;
                overlay.insert((level + 1, current_position), current_hash);
                touched.push((level + 1, current_position, current_hash));
            }
        }

        Ok(touched)
    }

    async fn authenticate_asset_leaf(&self, position: u64, leaf: StateCommitment) -> Result<()> {
        let path = self.read_asset_auth_path_direct(position).await?;
        let authenticated =
            root_from_auth_path(position, leaf, &path, IndexedMerkleTree::hash_children);
        let committed = self.get_asset_imt_root().await?;
        anyhow::ensure!(
            authenticated == committed,
            "asset tree path at position {position} does not authenticate to the committed root"
        );
        Ok(())
    }

    async fn ensure_asset_tree_initialized(&mut self) -> Result<()> {
        if self.get_asset_count().await? > 0 {
            let committed = self
                .get::<StateCommitment>(state_key::asset_imt_root())
                .await?
                .context("initialized asset IMT is missing its committed root")?;
            let materialized_root = self.read_asset_node(crate::tree::DEFAULT_DEPTH, 0).await?;
            anyhow::ensure!(
                materialized_root == committed,
                "asset IMT committed root does not match its root node"
            );
            return Ok(());
        }
        anyhow::ensure!(
            self.load_asset_imt_nodes().await?.is_empty()
                && self.load_asset_imt_leaves().await?.is_empty(),
            "uninitialized asset IMT has durable nodes or leaves"
        );
        let sentinel = IndexedLeaf::with_default_policy(Fq::from(0u64), 0, *FQ_MAX);
        let touched_nodes = self
            .compute_asset_path_updates(&[(0, sentinel.commit())])
            .await?;
        let root = touched_nodes
            .last()
            .map(|(_, _, root)| *root)
            .ok_or_else(|| anyhow::anyhow!("asset IMT initialization produced no root"))?;
        self.put_asset_imt_leaf(0, &sentinel)?;
        self.put_asset_imt_nodes(&touched_nodes);
        self.put(state_key::asset_imt_root().to_string(), root);
        self.put_proto(state_key::asset_count().to_string(), 1u64);
        Ok(())
    }

    /// Read and cache an asset policy for this state delta.
    #[cfg(test)]
    async fn get_asset_policy_cached(
        &mut self,
        asset_id: asset::Id,
    ) -> Result<Option<AssetPolicy>> {
        let mut policies: BTreeMap<asset::Id, Option<AssetPolicy>> = self
            .object_get(state_key::cache::cached_asset_policies())
            .unwrap_or_default();

        if let Some(policy) = policies.get(&asset_id) {
            return Ok(policy.clone());
        }

        let policy = self.get_asset_policy(asset_id).await?;
        policies.insert(asset_id, policy.clone());
        self.object_put(state_key::cache::cached_asset_policies(), policies);
        Ok(policy)
    }

    /// Add a compliance leaf for a user.
    ///
    /// This registers a user's address compliance key (ACK) for a regulated asset.
    /// Compliance leaves are current authorization facts; revocation needs an
    /// explicit state machine rather than deletion or archival from this tree.
    /// The leaf is committed and added to the user tree at the next available position.
    ///
    /// # Arguments
    /// * `leaf` - The compliance leaf containing address, ACK, and asset_id
    ///
    /// # Returns
    /// The position in the user tree where the leaf was added.
    async fn add_compliance_leaf(&mut self, leaf: ComplianceLeaf) -> Result<u64> {
        // This is the durable-state boundary, so intrinsic leaf validity must not
        // depend on every caller having passed through MsgRegisterUser.
        leaf.validate()?;
        anyhow::ensure!(
            leaf.status == UserAssetStatus::Active,
            "new compliance leaves must start active"
        );
        anyhow::ensure!(
            leaf.asset_id.0 != Fq::from(0u64),
            "compliance leaf asset ID zero is reserved"
        );

        anyhow::ensure!(
            self.get_user_asset_position(&leaf.address, leaf.asset_id)
                .await?
                .is_none(),
            "compliance leaf is already registered for address and asset"
        );
        let record = UserLeafRecord {
            position: self.get_user_count().await?,
            leaf: leaf.clone(),
        };
        let encoded_record = encode_user_leaf_record(&record)?;

        // Load the current user count (this will be our position)
        let position = record.position;
        let max_leaves = QuadTree::max_leaves_for_depth(crate::tree::DEFAULT_DEPTH);
        anyhow::ensure!(
            position < max_leaves,
            "compliance user tree is full: {position}/{max_leaves} leaves at depth {}",
            crate::tree::DEFAULT_DEPTH
        );

        // Calculate the leaf commitment
        let commitment = leaf.commit();
        self.authenticate_user_leaf(position, ZERO_HASHES[0])
            .await?;

        // Increment the user count
        let new_count = position + 1;

        // Save touched path nodes, count, and root.
        let touched_nodes = self
            .compute_user_path_updates(&[(position, commitment)])
            .await?;
        let root = touched_nodes
            .last()
            .map(|(_, _, root)| *root)
            .ok_or_else(|| anyhow::anyhow!("user tree update produced no root"))?;
        self.put_user_tree_nodes(&touched_nodes);
        self.put_proto(state_key::user_count().to_string(), new_count);
        self.put(state_key::user_tree_root().to_string(), root);

        // Store the typed position/leaf record in consensus state and authenticate
        // it against the user-tree root on every read.
        let leaf_record_key = state_key::user_leaf_record(&leaf.address, &leaf.asset_id);
        self.put_raw(leaf_record_key, encoded_record);
        self.put_proto(
            state_key::user_asset_position(&leaf.address, &leaf.asset_id),
            position,
        );

        Ok(position)
    }

    async fn change_user_asset_status(
        &mut self,
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
        action: UserAssetStatusAction,
        source_height: u64,
    ) -> Result<event::EventUserAssetStatusChanged> {
        anyhow::ensure!(
            self.is_asset_regulated(asset_id).await?,
            "cannot change user status for unregulated asset {asset_id}"
        );
        let record = self
            .get_user_leaf_record(address, asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user is not registered for asset {asset_id}"))?;
        let previous_status = record.leaf.status;
        let previous_commitment = record.leaf.commit();
        let mut leaf = record.leaf;
        leaf.apply_status_action(action, source_height)?;
        self.write_user_asset_status(record.position, previous_status, previous_commitment, leaf)
            .await
    }

    async fn seize_frozen_user_asset(
        &mut self,
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
    ) -> Result<event::EventUserAssetStatusChanged> {
        anyhow::ensure!(
            self.is_asset_regulated(asset_id).await?,
            "cannot seize user status for unregulated asset {asset_id}"
        );
        let record = self
            .get_user_leaf_record(address, asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user is not registered for asset {asset_id}"))?;
        let previous_status = record.leaf.status;
        let previous_commitment = record.leaf.commit();
        let mut leaf = record.leaf;
        leaf.status = leaf.status.seize()?;
        leaf.validate_lifecycle()?;
        self.write_user_asset_status(record.position, previous_status, previous_commitment, leaf)
            .await
    }

    async fn write_user_asset_status(
        &mut self,
        position: u64,
        previous_status: UserAssetStatus,
        previous_commitment: StateCommitment,
        leaf: ComplianceLeaf,
    ) -> Result<event::EventUserAssetStatusChanged> {
        previous_status.validate_transition(leaf.status)?;
        leaf.validate_lifecycle()?;
        let commitment = leaf.commit();
        self.authenticate_user_leaf(position, previous_commitment)
            .await?;

        let touched_nodes = self
            .compute_user_path_updates(&[(position, commitment)])
            .await?;
        let root = touched_nodes
            .last()
            .map(|(_, _, root)| *root)
            .ok_or_else(|| anyhow::anyhow!("user status update produced no root"))?;
        self.put_user_tree_nodes(&touched_nodes);
        self.put(state_key::user_tree_root().to_string(), root);

        let key = state_key::user_leaf_record(&leaf.address, &leaf.asset_id);
        self.put_raw(
            key,
            encode_user_leaf_record(&UserLeafRecord {
                position,
                leaf: leaf.clone(),
            })?,
        );

        Ok(event::EventUserAssetStatusChanged {
            position,
            commitment,
            leaf,
            previous_status,
        })
    }

    /// Register an asset in the IMT.
    ///
    /// This method is idempotent: if the regulated asset is already present, it
    /// returns `Ok(None)`. Unregulated assets are not inserted; they are proven
    /// by non-membership in the IMT.
    async fn register_asset_in_imt(
        &mut self,
        asset_id: asset::Id,
        policy: AssetPolicy,
        is_regulated: bool,
    ) -> Result<Option<indexed_tree::InsertResult>> {
        if !is_regulated {
            tracing::debug!(?asset_id, "unregulated asset uses IMT non-membership");
            return Ok(None);
        }
        ensure_regulated_asset_id(asset_id, true)?;
        policy.validate_regulated()?;

        self.ensure_asset_tree_initialized().await?;
        let value = asset_id.0;
        if value == Fq::from(0u64) {
            anyhow::bail!("IMT insert failed: zero value is reserved for sentinel leaf");
        }

        // A present IMT leaf must have the regulated policy written alongside it.
        if let Some(position) = self.read_asset_position_by_value(value).await? {
            anyhow::ensure!(
                self.get_asset_policy(asset_id).await?.is_some(),
                "regulated asset IMT leaf exists without its policy"
            );
            tracing::debug!(?asset_id, position, "asset already in IMT, skipping");
            return Ok(None);
        }

        let low_leaf_position = self
            .read_low_asset_position(value)
            .await?
            .ok_or_else(|| anyhow::anyhow!("asset value index has no predecessor for insert"))?;
        let low_leaf = self.read_asset_leaf(low_leaf_position).await?;
        let value_key = FqOrdKey::from(value);
        anyhow::ensure!(
            FqOrdKey::from(low_leaf.value) < value_key
                && value_key < FqOrdKey::from(low_leaf.next_value),
            "IMT insert failed: value {:?} not in gap [{:?}, {:?})",
            value.to_bytes(),
            low_leaf.value.to_bytes(),
            low_leaf.next_value.to_bytes()
        );

        let leaf_count = self.get_asset_count().await?.max(1);
        let max_leaves = QuadTree::max_leaves_for_depth(crate::tree::DEFAULT_DEPTH);
        anyhow::ensure!(
            leaf_count < max_leaves,
            "IMT insert failed: tree is full ({leaf_count}/{max_leaves} leaves, depth {})",
            crate::tree::DEFAULT_DEPTH
        );

        let new_position = leaf_count;
        let indexed_leaf =
            IndexedLeaf::from_policy(value, low_leaf.next_index, low_leaf.next_value, &policy);
        let updated_low_leaf = IndexedLeaf {
            value: low_leaf.value,
            next_index: new_position,
            next_value: value,
            params: low_leaf.params.clone(),
            ring: low_leaf.ring.clone(),
        };
        let result = InsertResult {
            position: new_position,
            indexed_leaf: indexed_leaf.clone(),
            low_leaf_position,
            updated_low_leaf: updated_low_leaf.clone(),
        };

        self.authenticate_asset_leaf(result.low_leaf_position, low_leaf.commit())
            .await?;
        self.authenticate_asset_leaf(result.position, IMT_ZERO_HASHES[0])
            .await?;

        self.set_ibc_origin_asset(asset_id, &policy).await?;

        // Save touched leaves and paths.
        self.put_asset_imt_leaf(result.position, &result.indexed_leaf)?;
        self.put_asset_imt_leaf(result.low_leaf_position, &result.updated_low_leaf)?;
        let touched_nodes = self
            .compute_asset_path_updates(&[
                (result.position, result.indexed_leaf.commit()),
                (result.low_leaf_position, result.updated_low_leaf.commit()),
            ])
            .await?;
        let root = touched_nodes
            .last()
            .map(|(_, _, root)| *root)
            .ok_or_else(|| anyhow::anyhow!("asset IMT insert produced no root"))?;
        self.put_asset_imt_nodes(&touched_nodes);
        self.put(state_key::asset_imt_root().to_string(), root);

        self.set_asset_policy(asset_id, policy)?;

        // Update the persisted asset count
        let new_count = leaf_count + 1;
        self.put_proto(state_key::asset_count().to_string(), new_count);

        tracing::debug!(
            ?asset_id,
            result.position,
            new_count,
            is_regulated,
            "registered asset in IMT"
        );
        Ok(Some(result))
    }

    /// Register a regulated asset in the IMT with the given policy.
    ///
    /// Convenience wrapper for `register_asset_in_imt` with `is_regulated = true`.
    #[cfg(test)]
    async fn register_regulated_asset(
        &mut self,
        asset_id: asset::Id,
        policy: AssetPolicy,
    ) -> Result<Option<indexed_tree::InsertResult>> {
        self.register_asset_in_imt(asset_id, policy, true).await
    }

    /// Set the compliance policy for an asset.
    ///
    /// Stores the issuer's detection key and daily_volume_limit for flagged transfers.
    fn set_asset_policy(&mut self, asset_id: asset::Id, policy: AssetPolicy) -> Result<()> {
        let key = state_key::asset_policy(&asset_id);
        self.put_raw(key, policy.to_bytes()?);
        if let Some(mut policies) = self.object_get::<BTreeMap<asset::Id, Option<AssetPolicy>>>(
            state_key::cache::cached_asset_policies(),
        ) {
            policies.insert(asset_id, Some(policy));
            self.object_put(state_key::cache::cached_asset_policies(), policies);
        }
        Ok(())
    }

    async fn set_ibc_origin_asset(
        &mut self,
        asset_id: asset::Id,
        policy: &AssetPolicy,
    ) -> Result<()> {
        let Some(origin) = &policy.params.ibc_origin else {
            return Ok(());
        };
        let key = state_key::ibc_origin_asset(&origin.base_denom);
        if let Some(existing) = self.get_raw(&key).await? {
            let existing = decode_asset_id(existing)?;
            anyhow::ensure!(
                existing == asset_id,
                "regulated IBC origin base denom {} is already registered for asset {}",
                origin.base_denom,
                existing
            );
        }
        self.put_raw(key, encode_asset_id(asset_id));
        Ok(())
    }

    #[cfg(any(test, feature = "test-helpers"))]
    async fn replace_asset_ibc_policy(
        &mut self,
        asset_id: asset::Id,
        expected_route_policy_hash: [u8; 32],
        allowed_ibc_routes: Vec<crate::structs::IbcRoute>,
    ) -> Result<IndexedLeaf> {
        self.ensure_asset_tree_initialized().await?;
        let mut policy = self
            .get_asset_policy(asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("asset is not regulated"))?;
        let current_hash = indexed_tree::route_policy_to_fq(&policy.params).to_bytes();
        anyhow::ensure!(
            current_hash == expected_route_policy_hash,
            "asset IBC route policy hash did not match expected current policy"
        );
        policy.replace_allowed_ibc_routes(allowed_ibc_routes);

        let position = self
            .read_asset_position_by_value(asset_id.0)
            .await?
            .ok_or_else(|| anyhow::anyhow!("regulated asset missing from asset IMT"))?;
        let current_leaf = self.read_asset_leaf(position).await?;
        anyhow::ensure!(
            current_leaf.value == asset_id.0,
            "asset value index inconsistent during IBC policy update"
        );
        let updated_leaf = IndexedLeaf::from_policy(
            asset_id.0,
            current_leaf.next_index,
            current_leaf.next_value,
            &policy,
        );
        self.authenticate_asset_leaf(position, current_leaf.commit())
            .await?;
        self.put_asset_imt_leaf(position, &updated_leaf)?;
        let touched_nodes = self
            .compute_asset_path_updates(&[(position, updated_leaf.commit())])
            .await?;
        let root = touched_nodes
            .last()
            .map(|(_, _, root)| *root)
            .ok_or_else(|| anyhow::anyhow!("asset IMT policy update produced no root"))?;
        self.put_asset_imt_nodes(&touched_nodes);
        self.put(state_key::asset_imt_root().to_string(), root);
        self.set_asset_policy(asset_id, policy)?;
        Ok(updated_leaf)
    }

    /// Store a compliance registrar verification key.
    fn put_compliance_registrar(&mut self, vk: VerificationKey<SpendAuth>) {
        self.put_raw(state_key::compliance_registrar_vk(&vk), vec![1]);
    }

    // ========== Historical Anchor Storage ==========

    /// Record the current compliance tree anchors at the given block height.
    ///
    /// This should be called at the end of each block to store the append-only
    /// user root used by historical compliance proofs. The mutable asset-policy
    /// root is emitted for synchronization but is never retained as admissible
    /// proof history.
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

        // Emit anchor event for local sync
        self.record_proto(event::compliance_anchor(height, user_anchor, asset_anchor));

        tracing::debug!(
            height,
            ?user_anchor,
            ?asset_anchor,
            "recorded compliance anchors"
        );

        let anchor_retention_blocks = self
            .get_compliance_params()
            .await?
            .anchor_validation_window_blocks
            .saturating_add(MAX_ANCHOR_SEARCH_DEPTH_BLOCKS);

        if let Some(cutoff_height) = height.checked_sub(anchor_retention_blocks + 1) {
            let start_height = self
                .get_proto::<u64>(state_key::anchor::pruned_through_height())
                .await?
                .map_or(0, |height| height.saturating_add(1));

            if start_height <= cutoff_height {
                for expired_height in start_height..=cutoff_height {
                    if let Some(expired_user_anchor) =
                        self.get_user_anchor_by_height(expired_height).await?
                    {
                        self.delete(state_key::anchor::user_anchor_by_height(expired_height));
                        if self.check_user_anchor(&expired_user_anchor).await?
                            == Some(expired_height)
                        {
                            self.delete(state_key::anchor::user_anchor_lookup(
                                &expired_user_anchor,
                            ));
                        }
                    }
                }

                self.put_proto(
                    state_key::anchor::pruned_through_height().to_string(),
                    cutoff_height,
                );
            }
        }

        Ok(())
    }

    // ========== Pending Registrations for CompactBlock ==========

    /// Buffer a user registration event for inclusion in the CompactBlock.
    ///
    /// This should be called when a user is registered during transaction processing.
    /// The events are accumulated and drained when building the CompactBlock.
    fn record_pending_user_registration(&mut self, event: event::EventUserRegistered) {
        let key = state_key::pending_user_registrations();
        let mut pending: Vec<event::EventUserRegistered> = self.object_get(key).unwrap_or_default();
        pending.push(event);
        self.object_put(key, pending);
    }

    fn record_pending_user_status_change(&mut self, event: event::EventUserAssetStatusChanged) {
        let key = state_key::pending_user_status_changes();
        let mut pending: Vec<event::EventUserAssetStatusChanged> =
            self.object_get(key).unwrap_or_default();
        pending.push(event);
        self.object_put(key, pending);
    }

    fn emit_user_status_change(&mut self, event: event::EventUserAssetStatusChanged) {
        self.record_proto(event::user_asset_status_changed(
            event.position,
            event.commitment,
            event.leaf.clone(),
            event.previous_status,
        ));
        self.record_pending_user_status_change(event);
    }

    /// Buffer an asset registration event for inclusion in the CompactBlock.
    ///
    /// This should be called when an asset is registered during transaction processing.
    /// The events are accumulated and drained when building the CompactBlock.
    fn record_pending_asset_registration(&mut self, event: event::EventAssetRegistered) {
        let key = state_key::pending_asset_registrations();
        let mut pending: Vec<event::EventAssetRegistered> =
            self.object_get(key).unwrap_or_default();
        pending.push(event);
        self.object_put(key, pending);
    }

    /// Emit an asset registration event proto and buffer it for the CompactBlock.
    fn emit_asset_registered(&mut self, event: event::EventAssetRegistered) {
        self.record_proto(event::asset_registered(
            event.asset_id,
            event.is_regulated,
            event.position,
            event.indexed_leaf.clone(),
            event.low_leaf_position,
            event.updated_low_leaf.clone(),
            event.asset_policy.clone(),
        ));
        self.record_pending_asset_registration(event);
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

    fn pending_user_status_changes(&mut self) -> Vec<event::EventUserAssetStatusChanged> {
        let key = state_key::pending_user_status_changes();
        let result = self.object_get(key).unwrap_or_default();
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

impl<T: StateWrite + ?Sized> ComplianceRegistryRawWrite for T {}

#[derive(Clone, Debug)]
pub struct NoteSeizureLifecycle {
    pub leaf: ComplianceLeaf,
    pub previous_status: UserAssetStatus,
    pub status_change: Option<event::EventUserAssetStatusChanged>,
}

/// Component lifecycle operations that do not admit registry facts.
#[async_trait]
pub(crate) trait ComplianceRegistryComponentWrite:
    StateWrite + ComplianceRegistryRead
{
    async fn initialize_trees(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.load_user_tree_nodes().await?.is_empty() && self.get_user_count().await? == 0,
            "new compliance user tree has existing state"
        );
        self.put(
            state_key::user_tree_root().to_string(),
            QuadTree::new().root(),
        );
        self.put_proto(state_key::user_count().to_string(), 0u64);
        <Self as ComplianceRegistryRawWrite>::ensure_asset_tree_initialized(self).await
    }

    fn admit_genesis_compliance_registrar(&mut self, vk: VerificationKey<SpendAuth>) -> Result<()> {
        ensure_nonidentity_spend_auth_key(&vk, "compliance registrar key")?;
        <Self as ComplianceRegistryRawWrite>::put_compliance_registrar(self, vk);
        Ok(())
    }

    async fn finish_block_compliance_anchors(&mut self, height: u64) -> Result<()> {
        <Self as ComplianceRegistryRawWrite>::record_compliance_anchors(self, height).await
    }

    fn publish_user_registration(&mut self, event: event::EventUserRegistered) {
        <Self as ComplianceRegistryRawWrite>::record_pending_user_registration(self, event.clone());
        self.record_proto(event::user_registered(
            event.position,
            event.commitment,
            event.leaf,
        ));
    }

    fn publish_asset_registration(&mut self, event: event::EventAssetRegistered) {
        <Self as ComplianceRegistryRawWrite>::emit_asset_registered(self, event);
    }
}

impl<T: StateWrite + ?Sized> ComplianceRegistryComponentWrite for T {}

/// Authorized compliance-registry state transitions and non-registry buffers.
///
/// Registry mutations require an admission value produced by the corresponding
/// grant, genesis, or governance validation path. Raw tree and policy mutators
/// are intentionally not part of this public trait.
#[async_trait]
pub trait ComplianceRegistryWrite: StateWrite + ComplianceRegistryRead {
    /// Persist a user registration admitted by a verified user grant.
    async fn register_user_with_grant(&mut self, admission: UserGrantAdmission) -> Result<u64> {
        <Self as ComplianceRegistryRawWrite>::add_compliance_leaf(self, admission.leaf).await
    }

    /// Persist a user registration admitted during genesis validation.
    async fn register_genesis_user(&mut self, admission: GenesisUserAdmission) -> Result<u64> {
        <Self as ComplianceRegistryRawWrite>::add_compliance_leaf(self, admission.leaf).await
    }

    async fn apply_user_status_action(
        &mut self,
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
        action: UserAssetStatusAction,
        source_height: u64,
    ) -> Result<event::EventUserAssetStatusChanged> {
        let event = <Self as ComplianceRegistryRawWrite>::change_user_asset_status(
            self,
            address,
            asset_id,
            action,
            source_height,
        )
        .await?;
        <Self as ComplianceRegistryRawWrite>::emit_user_status_change(self, event.clone());
        Ok(event)
    }

    /// Admit one note seizure against the exact current freeze generation.
    ///
    /// The first admitted note makes the leaf terminally seized. Further notes
    /// from that same generation remain admissible without another tree write.
    async fn admit_note_seizure(
        &mut self,
        address: &shieldd_sdk_keys::Address,
        asset_id: asset::Id,
        freeze_generation: u64,
        frozen_since_height: u64,
    ) -> Result<NoteSeizureLifecycle> {
        anyhow::ensure!(
            self.is_asset_regulated(asset_id).await?,
            "cannot seize a note for unregulated asset {asset_id}"
        );
        let current = self
            .get_user_leaf(address, asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user is not registered for asset {asset_id}"))?;
        anyhow::ensure!(
            current.freeze_generation == freeze_generation
                && current.frozen_since_height == frozen_since_height,
            "note seizure does not match the current freeze generation"
        );
        match current.status {
            UserAssetStatus::Active => {
                anyhow::bail!("active user asset cannot admit a note seizure")
            }
            UserAssetStatus::Frozen => {
                let event = <Self as ComplianceRegistryRawWrite>::seize_frozen_user_asset(
                    self, address, asset_id,
                )
                .await?;
                <Self as ComplianceRegistryRawWrite>::emit_user_status_change(self, event.clone());
                Ok(NoteSeizureLifecycle {
                    leaf: event.leaf.clone(),
                    previous_status: event.previous_status,
                    status_change: Some(event),
                })
            }
            UserAssetStatus::Seized => Ok(NoteSeizureLifecycle {
                leaf: current,
                previous_status: UserAssetStatus::Seized,
                status_change: None,
            }),
        }
    }

    /// Persist an asset registration admitted by a verified registrar grant.
    async fn register_asset_with_grant(
        &mut self,
        admission: AssetGrantAdmission,
    ) -> Result<Option<indexed_tree::InsertResult>> {
        <Self as ComplianceRegistryRawWrite>::register_asset_in_imt(
            self,
            admission.asset_id,
            admission.policy,
            admission.is_regulated,
        )
        .await
    }

    /// Persist an asset registration admitted during genesis validation.
    async fn register_genesis_asset(
        &mut self,
        admission: GenesisAssetAdmission,
    ) -> Result<Option<indexed_tree::InsertResult>> {
        <Self as ComplianceRegistryRawWrite>::register_asset_in_imt(
            self,
            admission.asset_id,
            admission.policy,
            admission.is_regulated,
        )
        .await
    }

    /// Drain user-registration events during compact-block construction.
    fn pending_user_registrations(&mut self) -> Vec<event::EventUserRegistered> {
        <Self as ComplianceRegistryRawWrite>::pending_user_registrations(self)
    }

    fn pending_user_status_changes(&mut self) -> Vec<event::EventUserAssetStatusChanged> {
        <Self as ComplianceRegistryRawWrite>::pending_user_status_changes(self)
    }

    /// Drain asset-registration events during compact-block construction.
    fn pending_asset_registrations(&mut self) -> Vec<event::EventAssetRegistered> {
        <Self as ComplianceRegistryRawWrite>::pending_asset_registrations(self)
    }

    /// Insert a user leaf in test/benchmark state without a signed grant.
    #[cfg(any(test, feature = "test-helpers"))]
    async fn test_only_add_compliance_leaf(&mut self, leaf: ComplianceLeaf) -> Result<u64> {
        <Self as ComplianceRegistryRawWrite>::add_compliance_leaf(self, leaf).await
    }

    #[cfg(any(test, feature = "test-helpers"))]
    async fn test_only_replace_asset_ibc_policy(
        &mut self,
        asset_id: asset::Id,
        expected_hash: [u8; 32],
        routes: Vec<crate::IbcRoute>,
    ) -> Result<IndexedLeaf> {
        <Self as ComplianceRegistryRawWrite>::replace_asset_ibc_policy(
            self,
            asset_id,
            expected_hash,
            routes,
        )
        .await
    }

    /// Insert an asset policy in test/benchmark state without a registrar grant.
    #[cfg(any(test, feature = "test-helpers"))]
    async fn test_only_register_asset(
        &mut self,
        asset_id: asset::Id,
        policy: AssetPolicy,
        is_regulated: bool,
    ) -> Result<Option<indexed_tree::InsertResult>> {
        <Self as ComplianceRegistryRawWrite>::register_asset_in_imt(
            self,
            asset_id,
            policy,
            is_regulated,
        )
        .await
    }
}

impl<T: StateWrite + ?Sized> ComplianceRegistryWrite for T {}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
