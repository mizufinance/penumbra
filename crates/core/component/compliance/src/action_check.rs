use anyhow::Result;
use async_trait::async_trait;
use cnidarium::StateRead;
use penumbra_sdk_asset::asset;

use crate::registry::ComplianceRegistryRead;

#[async_trait]
pub trait RegulatedAssetCheck: StateRead {
    async fn ensure_not_regulated(&self, asset_id: asset::Id, action_name: &str) -> Result<()> {
        let proof_data = self.get_asset_proof_data(asset_id).await?;
        if proof_data.is_regulated {
            anyhow::bail!(
                "Regulated assets cannot be used in {} actions. Asset {} is regulated.",
                action_name,
                asset_id
            );
        }
        Ok(())
    }

    async fn ensure_assets_not_regulated(
        &self,
        asset_ids: &[asset::Id],
        action_name: &str,
    ) -> Result<()> {
        for asset_id in asset_ids {
            self.ensure_not_regulated(*asset_id, action_name).await?;
        }
        Ok(())
    }
}

impl<T: StateRead + ?Sized> RegulatedAssetCheck for T {}
