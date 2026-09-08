use std::future::Future;

use anyhow::{Context, Result};
use async_trait::async_trait;
use shieldd_sdk_compliance::{
    AssetProofData, BatchComplianceData, ComplianceLeaf, ComplianceQuery, MerklePath, UserProofData,
};
use shieldd_sdk_keys::{keys::AddressIndex, Address};
use shieldd_sdk_proto::{core::component::compliance::v1 as pb, view::v1::NotesRequest};

use crate::{
    planning_io::PlanningIo, AddressPurpose, ComplianceAssetTree, ComplianceUserTree,
    IssuedAddress, SpendableNoteRecord, Storage,
};

/// Plans against one wallet height; advancing sync invalidates this reader.
pub struct StoragePlanningIo {
    storage: Storage,
    height: u64,
    timestamp: u64,
    users: ComplianceUserTree,
    assets: ComplianceAssetTree,
}

impl StoragePlanningIo {
    pub async fn new(storage: Storage) -> Result<Self> {
        let height = storage
            .last_sync_height()
            .await?
            .context("wallet has not scanned genesis")?;
        let reader = Self {
            height,
            timestamp: storage.block_timestamp().await?,
            users: storage.compliance_user_tree().await?,
            assets: storage.compliance_asset_tree().await?,
            storage,
        };
        reader.ensure_height().await?;
        Ok(reader)
    }

    async fn ensure_height(&self) -> Result<()> {
        anyhow::ensure!(
            self.storage.last_sync_height().await? == Some(self.height),
            "wallet advanced during planning; create a new planning reader"
        );
        Ok(())
    }

    async fn read<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        self.ensure_height().await?;
        let value = future.await?;
        self.ensure_height().await?;
        Ok(value)
    }

    async fn local_compliance(&self, queries: Vec<ComplianceQuery>) -> Result<BatchComplianceData> {
        let mut data = BatchComplianceData {
            compliance_anchor: self.users.root(),
            asset_anchor: self.assets.root(),
            ..Default::default()
        };
        for ComplianceQuery { address, asset_id } in queries {
            let (position, indexed_leaf, auth_path, is_regulated) =
                self.assets.get_proof_data(asset_id)?;
            data.asset_proofs.insert(
                asset_id,
                AssetProofData {
                    position,
                    indexed_leaf,
                    auth_path,
                    is_regulated,
                },
            );
            if is_regulated {
                data.asset_policies.insert(
                    asset_id,
                    self.storage
                        .get_asset_policy(&asset_id)
                        .await?
                        .context("missing regulated asset policy")?,
                );
            }
            let proof = match self
                .storage
                .get_compliance_leaf_data(&address, &asset_id)
                .await?
            {
                Some(stored) => {
                    let leaf: ComplianceLeaf = pb::ComplianceLeaf {
                        address: Some(address.clone().into()),
                        asset_id: Some(asset_id.into()),
                        capk: stored.capk.to_vec(),
                        rnk_dh_pk: stored.rnk_dh_pk.to_vec(),
                        rnk_commitment: stored.rnk_commitment.to_vec(),
                        status: pb::UserAssetStatus::from(stored.status) as i32,
                        freeze_generation: stored.freeze_generation,
                        frozen_since_height: stored.frozen_since_height,
                    }
                    .try_into()?;
                    anyhow::ensure!(
                        leaf.commit() == stored.commitment
                            && self.users.commitment(stored.position) == Some(stored.commitment),
                        "stored compliance leaf does not match planning snapshot"
                    );
                    UserProofData {
                        leaf,
                        position: stored.position,
                        auth_path: self.users.witness(stored.position)?,
                    }
                }
                None if !is_regulated && self.users.root() == ComplianceUserTree::new().root() => {
                    UserProofData {
                        leaf: ComplianceLeaf::synthetic_unregulated(address.clone(), asset_id),
                        position: 0,
                        auth_path: MerklePath::default(),
                    }
                }
                None => anyhow::bail!(
                    "compliance leaf unavailable locally; obtain host data before planning"
                ),
            };
            data.user_proofs.insert((address, asset_id), proof);
        }
        Ok(data)
    }
}

#[async_trait]
impl PlanningIo for StoragePlanningIo {
    async fn latest_block_timestamp(&mut self) -> Result<u64> {
        self.ensure_height().await?;
        Ok(self.timestamp)
    }
    async fn volume_accumulator_recovery(
        &mut self,
        subject: decaf377::Fq,
        day_start: u64,
    ) -> Result<crate::storage::VolumeAccumulatorRecovery> {
        self.read(self.storage.volume_accumulator_recovery(subject, day_start))
            .await
    }
    async fn chain_id(&mut self) -> Result<String> {
        Ok(self.read(self.storage.app_params()).await?.chain_id)
    }
    async fn nullifier_window(
        &mut self,
    ) -> Result<shieldd_sdk_sct::nullifier_generation::NullifierWindow> {
        self.read(self.storage.nullifier_window()).await
    }
    async fn discovery_parameters(
        &mut self,
    ) -> Result<shieldd_sdk_shielded_pool::discovery::Parameters> {
        self.read(self.storage.discovery_parameters()).await
    }
    async fn notes(&mut self, request: NotesRequest) -> Result<Vec<SpendableNoteRecord>> {
        self.read(self.storage.notes(
            request.include_spent,
            request.asset_id.map(TryInto::try_into).transpose()?,
            request.address_index.map(TryInto::try_into).transpose()?,
            request.amount_to_spend.map(TryInto::try_into).transpose()?,
        ))
        .await
    }
    async fn address_by_index(&mut self, index: AddressIndex) -> Result<Address> {
        self.ensure_height().await?;
        let fvk = self.storage.full_viewing_key().await?;
        self.read(self.storage.record_issued_address(IssuedAddress {
            address_index: index,
            address: fvk.payment_address(index),
            purpose: AddressPurpose::General,
            birth_height: self.height,
            retired_height: None,
        }))
        .await
    }
    async fn index_by_address(&mut self, address: Address) -> Result<Option<AddressIndex>> {
        Ok(self
            .read(self.storage.full_viewing_key())
            .await?
            .address_index(&address))
    }
    async fn compliance_data(
        &mut self,
        queries: Vec<ComplianceQuery>,
    ) -> Result<BatchComplianceData> {
        self.read(self.local_compliance(queries)).await
    }
}
