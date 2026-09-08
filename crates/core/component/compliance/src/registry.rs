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

    // ========== IBC Compliance Metadata ==========

    /// Retrieve IBC compliance metadata for an ICS-20 transfer.
    ///
    /// Returns the compliance metadata that was bridged via ICS-20 from the sending chain.
    async fn get_ibc_compliance_metadata(
        &self,
        channel_id: &str,
        packet_seq: u64,
    ) -> Result<Option<crate::ibc::IbcComplianceMetadata>> {
        use shieldd_sdk_proto::core::component::compliance::v1 as pb;
        let key = state_key::ibc_compliance_metadata(channel_id, packet_seq);
        match self.get_raw(&key).await? {
            Some(bytes) => {
                let proto: pb::IbcComplianceMetadata =
                    shieldd_sdk_proto::Message::decode(bytes.as_slice()).map_err(|e| {
                        anyhow::anyhow!("failed to decode IBC compliance metadata: {e}")
                    })?;
                let meta = crate::ibc::IbcComplianceMetadata::from_proto_public(proto)?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
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

    // ========== IBC Compliance Metadata Storage ==========

    /// Store IBC compliance metadata for an ICS-20 transfer.
    ///
    /// Called during ICS-20 packet receive when the memo contains compliance data.
    fn store_ibc_compliance_metadata(
        &mut self,
        channel_id: &str,
        packet_seq: u64,
        metadata: &crate::ibc::IbcComplianceMetadata,
    ) {
        use shieldd_sdk_proto::Message as _;
        let key = state_key::ibc_compliance_metadata(channel_id, packet_seq);
        let proto = metadata.to_proto_public();
        let bytes = proto.encode_to_vec();
        self.put_raw(key, bytes);
        tracing::debug!(channel_id, packet_seq, "stored IBC compliance metadata");
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

    /// Store validated ICS-20 compliance metadata.
    fn store_ibc_compliance_metadata(
        &mut self,
        channel_id: &str,
        packet_seq: u64,
        metadata: &crate::ibc::IbcComplianceMetadata,
    ) {
        <Self as ComplianceRegistryRawWrite>::store_ibc_compliance_metadata(
            self, channel_id, packet_seq, metadata,
        );
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
mod tests {
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
    async fn test_add_compliance_leaf() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();

        // Create a dummy compliance leaf
        let leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rand::thread_rng()),
            asset::Id(Fq::from(1u64)),
        );

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
                let commitment = leaf.commit();
                let position = state.add_compliance_leaf(leaf).await.unwrap();
                user_positions.push((position, commitment));

                let reconstructed = state.reconstruct_user_tree().await.unwrap();
                assert_eq!(
                    state.get_user_tree_root().await.unwrap(),
                    reconstructed.root()
                );
                let direct_path = state.get_user_auth_path(position).await.unwrap();
                let reconstructed_path = reconstructed.auth_path(position).unwrap();
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
                let (position, leaf, path) =
                    reconstructed.non_membership_proof(asset_id.0).unwrap();
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
    async fn test_multiple_leaves() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();

        let mut rng = rand::thread_rng();

        // Add multiple leaves
        for i in 0..5 {
            let leaf = ComplianceLeaf::registered_for_test(
                Address::dummy(&mut rng),
                asset::Id(Fq::from(i as u64 + 1)),
            );
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
        let count_before = imt.leaf_count();

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
        let leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(100u64)),
        );

        // Before adding, verification should fail
        let verified = state.verify_compliance_leaf(&leaf).await.unwrap();
        assert!(!verified, "Leaf should not be verified before being added");

        // Add the leaf to the registry
        state.add_compliance_leaf(leaf.clone()).await.unwrap();

        // After adding, verification should succeed
        let verified = state.verify_compliance_leaf(&leaf).await.unwrap();
        assert!(verified, "Leaf should be verified after being added");

        // Create a different leaf with same asset but different wallet
        let different_leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(100u64)),
        );

        // Different leaf should not verify
        let verified = state.verify_compliance_leaf(&different_leaf).await.unwrap();
        assert!(!verified, "Different leaf should not be verified");
    }

    #[tokio::test]
    async fn test_leaf_json_serialization() {
        let mut rng = rand::thread_rng();

        // Create a compliance leaf
        let original_leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(200u64)),
        );

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
        let user_leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(300u64)),
        );

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
        let new_leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(999u64)),
        );
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
    async fn test_imt_register_regulated_asset() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();

        let asset_id = asset::Id(Fq::from(12345u64));

        // Register regulated asset
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

        // Check the asset is in the IMT
        let tree = state.reconstruct_asset_tree().await.unwrap();
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
    async fn regulated_asset_identity_keys_fail_before_tree_mutation() {
        let identity_authority =
            VerificationKey::from(&SigningKey::<SpendAuth>::from(Fr::from(0u64)));
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
    async fn test_imt_register_idempotent() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();

        let asset_id = asset::Id(Fq::from(12345u64));

        // Register twice - should be idempotent
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

        let tree = state.reconstruct_asset_tree().await.unwrap();
        assert_eq!(tree.leaf_count(), 2); // sentinel + 1 asset (not 3)
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
        let leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(100u64)),
        );
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
        let leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(100u64)),
        );
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
        let leaf = ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rng),
            asset::Id(Fq::from(9999u64)),
        );
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
    async fn test_anchor_within_window_accepted() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.initialize_trees().await.unwrap();
        put_test_compliance_params(&mut state);

        // Set initial height and record anchor
        state.put_block_height(1);
        state.record_compliance_anchors(1).await.unwrap();
        let user_anchor = state.get_user_tree_root().await.unwrap();
        let asset_anchor = state.get_asset_imt_root().await.unwrap();

        // Advance to height within the configured validation window.
        let new_height = 1 + TEST_ANCHOR_WINDOW_BLOCKS / 2;
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

        state.put_compliance_params(ComplianceParameters {
            anchor_validation_window_blocks: 10,
        });
        state.put_block_height(12);

        state
            .validate_compliance_anchors(&user_anchor, &asset_anchor)
            .await
            .expect("history-retention policy must not invalidate current roots");
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
            check_timestamp_freshness((CURRENT as u64) - MAX_TIMESTAMP_DRIFT_SECS, CURRENT,)
                .is_ok()
        );
        assert!(
            check_timestamp_freshness((CURRENT as u64) + MAX_TIMESTAMP_DRIFT_SECS, CURRENT,)
                .is_ok()
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
}
