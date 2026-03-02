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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ComplianceRegistryWrite;
    use crate::structs::AssetPolicy;
    use cnidarium::TempStorage;
    use decaf377::Fq;

    #[tokio::test]
    async fn ensure_not_regulated_unknown_asset() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let state = cnidarium::StateDelta::new(snapshot);

        // Unknown asset should be treated as unregulated
        let unknown = asset::Id(Fq::from(999999u64));
        state
            .ensure_not_regulated(unknown, "test_action")
            .await
            .expect("unknown asset should pass as unregulated");
    }

    #[tokio::test]
    async fn ensure_not_regulated_rejects_regulated() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let asset_id = asset::Id(Fq::from(42u64));
        state
            .register_regulated_asset(
                asset_id,
                AssetPolicy::simple(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
            )
            .await
            .unwrap();

        let result = state.ensure_not_regulated(asset_id, "swap").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("swap"), "error should mention action name");
        assert!(err.contains("Regulated"), "error should mention regulated");
    }

    #[tokio::test]
    async fn ensure_assets_not_regulated_batch_with_one_regulated() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let unregulated_1 = asset::Id(Fq::from(1u64));
        let regulated = asset::Id(Fq::from(2u64));
        let unregulated_2 = asset::Id(Fq::from(3u64));

        state
            .register_regulated_asset(
                regulated,
                AssetPolicy::simple(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
            )
            .await
            .unwrap();

        // Batch check with one regulated should fail
        let result = state
            .ensure_assets_not_regulated(&[unregulated_1, regulated, unregulated_2], "delegate")
            .await;
        assert!(
            result.is_err(),
            "batch check should fail when any asset is regulated"
        );
    }

    #[tokio::test]
    async fn ensure_assets_not_regulated_all_unregulated() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let state = cnidarium::StateDelta::new(snapshot);

        let assets = vec![
            asset::Id(Fq::from(1u64)),
            asset::Id(Fq::from(2u64)),
            asset::Id(Fq::from(3u64)),
        ];

        state
            .ensure_assets_not_regulated(&assets, "delegate")
            .await
            .expect("all unregulated should pass");
    }
}
