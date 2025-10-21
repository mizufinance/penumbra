use anyhow::Result;
use penumbra_sdk_asset::asset::{self, REGISTRY};
use penumbra_sdk_keys::{AssetViewingKey, FullViewingKey};
use penumbra_sdk_proto::penumbra::core::asset::v1 as pb;
use serde::Serialize;

#[derive(Debug, clap::Parser)]
pub struct AssetViewingKeyCmd {
    /// The asset ID to create a viewing key for.
    /// Can be either:
    /// - A bech32m-encoded asset ID (starting with 'passet')
    /// - A raw denomination string (e.g., "upenumbra", "usdc")
    #[clap(long)]
    pub asset_id: String,
}

#[derive(Serialize)]
struct AssetViewingKeyOutput {
    asset_id: String,
    asset_viewing_key: String,
}

impl AssetViewingKeyCmd {
    /// Determine if this command requires a network sync before it executes.
    pub fn offline(&self) -> bool {
        true
    }

    pub fn exec(&self, fvk: &FullViewingKey) -> Result<()> {
        // Try parsing as bech32m first, then fall back to raw denom
        let asset_id: asset::Id = if self.asset_id.starts_with("passet") {
            self.asset_id
                .parse()
                .map_err(|e| anyhow::anyhow!("Failed to parse asset ID: {}", e))?
        } else {
            // Treat as raw denomination - need to resolve to base denomination first
            // Parse the unit to check if it's a display denomination
            let unit = REGISTRY.parse_unit(&self.asset_id);

            // Get the base denomination from the unit
            // ex: test_usd is actually wtest_usd underlying, which is the true asset ID used for the passet
            let base_denom = unit.base();

            // Use the base denomination to derive the asset ID
            pb::AssetId {
                alt_base_denom: base_denom.to_string(),
                ..Default::default()
            }
            .try_into()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to derive asset ID from denom '{}': {}",
                    self.asset_id,
                    e
                )
            })?
        };

        // Create the asset viewing key
        let asset_viewing_key = AssetViewingKey::from_fvk(fvk, asset_id);

        // Create JSON output
        let output = AssetViewingKeyOutput {
            asset_id: asset_id.to_string(),
            asset_viewing_key: asset_viewing_key.to_string(),
        };

        // Print as JSON
        println!("{}", serde_json::to_string_pretty(&output)?);

        Ok(())
    }
}
