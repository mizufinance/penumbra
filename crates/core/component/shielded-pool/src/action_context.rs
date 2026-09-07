use anyhow::{ensure, Context, Result};
use decaf377::{Fq, Fr};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::{
    AssetPolicy, ComplianceLeaf, IndexedLeaf, IndexedMerkleTree, MerklePath, QuadTree,
    DEFAULT_DEPTH,
};
use shieldd_sdk_keys::Address;
use shieldd_sdk_proto::core::component::shielded_pool::v1 as pb;
use shieldd_sdk_tct::StateCommitment;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::AssetWitness", into = "pb::AssetWitness")]
pub struct AssetWitness {
    pub asset_id: asset::Id,
    pub root: StateCommitment,
    pub leaf: IndexedLeaf,
    pub position: u64,
    pub path: MerklePath,
    pub is_regulated: bool,
}

impl From<AssetWitness> for pb::AssetWitness {
    fn from(value: AssetWitness) -> Self {
        Self {
            asset_id: Some(value.asset_id.into()),
            root: Some(value.root.into()),
            leaf: Some(value.leaf.into()),
            position: value.position,
            path: Some(value.path.into()),
            is_regulated: value.is_regulated,
        }
    }
}

impl TryFrom<pb::AssetWitness> for AssetWitness {
    type Error = anyhow::Error;
    fn try_from(value: pb::AssetWitness) -> Result<Self> {
        Ok(Self {
            asset_id: value
                .asset_id
                .context("missing AssetWitness.asset_id")?
                .try_into()?,
            root: value
                .root
                .context("missing AssetWitness.root")?
                .try_into()?,
            leaf: value
                .leaf
                .context("missing AssetWitness.leaf")?
                .try_into()?,
            position: value.position,
            path: value
                .path
                .context("missing AssetWitness.path")?
                .try_into()?,
            is_regulated: value.is_regulated,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::UserWitness", into = "pb::UserWitness")]
pub struct UserWitness {
    pub leaf: ComplianceLeaf,
    pub position: u64,
    pub path: MerklePath,
}

impl From<UserWitness> for pb::UserWitness {
    fn from(value: UserWitness) -> Self {
        Self {
            leaf: Some(value.leaf.into()),
            position: value.position,
            path: Some(value.path.into()),
        }
    }
}

impl TryFrom<pb::UserWitness> for UserWitness {
    type Error = anyhow::Error;
    fn try_from(value: pb::UserWitness) -> Result<Self> {
        Ok(Self {
            leaf: value.leaf.context("missing UserWitness.leaf")?.try_into()?,
            position: value.position,
            path: value.path.context("missing UserWitness.path")?.try_into()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::ActionWitness", into = "pb::ActionWitness")]
pub struct ActionWitness {
    pub asset: AssetWitness,
    pub user_root: StateCommitment,
    pub sender: UserWitness,
}

impl From<ActionWitness> for pb::ActionWitness {
    fn from(value: ActionWitness) -> Self {
        Self {
            asset: Some(value.asset.into()),
            user_root: Some(value.user_root.into()),
            sender: Some(value.sender.into()),
        }
    }
}

impl TryFrom<pb::ActionWitness> for ActionWitness {
    type Error = anyhow::Error;
    fn try_from(value: pb::ActionWitness) -> Result<Self> {
        Ok(Self {
            asset: value
                .asset
                .context("missing ActionWitness.asset")?
                .try_into()?,
            user_root: value
                .user_root
                .context("missing ActionWitness.user_root")?
                .try_into()?,
            sender: value
                .sender
                .context("missing ActionWitness.sender")?
                .try_into()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::TransferContext", into = "pb::TransferContext")]
pub struct TransferContext {
    pub witness: ActionWitness,
    pub recipient: UserWitness,
    pub policy: Option<AssetPolicy>,
    pub timestamp: u64,
    pub nonce: Fr,
}

impl From<TransferContext> for pb::TransferContext {
    fn from(value: TransferContext) -> Self {
        Self {
            witness: Some(value.witness.into()),
            recipient: Some(value.recipient.into()),
            policy: value.policy.map(Into::into),
            timestamp: value.timestamp,
            nonce: value.nonce.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::TransferContext> for TransferContext {
    type Error = anyhow::Error;
    fn try_from(value: pb::TransferContext) -> Result<Self> {
        Ok(Self {
            witness: value
                .witness
                .context("missing TransferContext.witness")?
                .try_into()?,
            recipient: value
                .recipient
                .context("missing TransferContext.recipient")?
                .try_into()?,
            policy: value.policy.map(TryInto::try_into).transpose()?,
            timestamp: value.timestamp,
            nonce: crate::compliance_helpers::parse_tx_blinding_nonce(&value.nonce)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::NoteReshapeContext", into = "pb::NoteReshapeContext")]
pub struct NoteReshapeContext {
    pub witness: ActionWitness,
    pub nonce: Fr,
}

impl From<NoteReshapeContext> for pb::NoteReshapeContext {
    fn from(value: NoteReshapeContext) -> Self {
        Self {
            witness: Some(value.witness.into()),
            nonce: value.nonce.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::NoteReshapeContext> for NoteReshapeContext {
    type Error = anyhow::Error;
    fn try_from(value: pb::NoteReshapeContext) -> Result<Self> {
        Ok(Self {
            witness: value
                .witness
                .context("missing NoteReshapeContext.witness")?
                .try_into()?,
            nonce: crate::compliance_helpers::parse_tx_blinding_nonce(&value.nonce)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::WithdrawalContext", into = "pb::WithdrawalContext")]
pub struct WithdrawalContext {
    pub witness: ActionWitness,
    pub timestamp: u64,
    pub nonce: Fr,
}

impl From<WithdrawalContext> for pb::WithdrawalContext {
    fn from(value: WithdrawalContext) -> Self {
        Self {
            witness: Some(value.witness.into()),
            timestamp: value.timestamp,
            nonce: value.nonce.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::WithdrawalContext> for WithdrawalContext {
    type Error = anyhow::Error;
    fn try_from(value: pb::WithdrawalContext) -> Result<Self> {
        Ok(Self {
            witness: value
                .witness
                .context("missing WithdrawalContext.witness")?
                .try_into()?,
            timestamp: value.timestamp,
            nonce: crate::compliance_helpers::parse_tx_blinding_nonce(&value.nonce)?,
        })
    }
}

impl ActionWitness {
    pub fn validate(&self, asset_id: asset::Id, sender: &Address) -> Result<()> {
        ensure!(
            self.asset.asset_id == asset_id,
            "action asset witness does not match notes"
        );
        let asset_path = auth_path(&self.asset.path, self.asset.position)?;
        ensure!(
            IndexedMerkleTree::verify_auth_path(
                self.asset.position,
                &self.asset.leaf,
                &asset_path,
                self.asset.root,
                DEFAULT_DEPTH
            ),
            "invalid asset witness root or path"
        );
        if self.asset.is_regulated {
            ensure!(
                self.asset.leaf.value == asset_id.0,
                "regulated asset witness must prove membership"
            );
        } else {
            ensure!(
                self.asset.leaf.value < asset_id.0
                    && (self.asset.leaf.next_value == Fq::from(0u64)
                        || asset_id.0 < self.asset.leaf.next_value),
                "unregulated asset witness must prove non-membership"
            );
        }
        self.validate_user(&self.sender, sender)
    }

    pub fn validate_user(&self, witness: &UserWitness, address: &Address) -> Result<()> {
        ensure!(
            witness.leaf.asset_id == self.asset.asset_id && witness.leaf.address == *address,
            "user witness asset or address mismatch"
        );
        let path = auth_path(&witness.path, witness.position)?;
        if self.asset.is_regulated {
            ensure!(
                QuadTree::verify_auth_path(
                    witness.position,
                    witness.leaf.commit(),
                    &path,
                    self.user_root,
                    DEFAULT_DEPTH
                ),
                "invalid user witness root or path"
            );
        } else {
            ensure!(
                witness.leaf
                    == ComplianceLeaf::synthetic_unregulated(address.clone(), self.asset.asset_id),
                "unregulated user witness must use the canonical synthetic leaf"
            );
        }
        Ok(())
    }
}

fn auth_path(path: &MerklePath, position: u64) -> Result<Vec<[StateCommitment; 3]>> {
    path.validate()?;
    ensure!(
        position < (1u64 << (2 * DEFAULT_DEPTH)),
        "witness position outside tree"
    );
    path.layers
        .iter()
        .map(|layer| {
            let mut siblings = [StateCommitment(Fq::from(0u64)); 3];
            for (index, bytes) in layer.siblings.iter().enumerate() {
                siblings[index] = StateCommitment(
                    Fq::from_bytes_checked(bytes.as_slice().try_into()?)
                        .map_err(|_| anyhow::anyhow!("noncanonical witness sibling"))?,
                );
            }
            Ok(siblings)
        })
        .collect()
}
