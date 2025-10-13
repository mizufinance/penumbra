use anyhow::Result;
use penumbra_sdk_keys::{AssetViewingKey, FullViewingKey};
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::penumbra::core::asset::v1 as pb;

#[derive(Debug, clap::Parser)]
pub struct AssetViewingKeyCmd {
    /// The asset ID to create a viewing key for.
    /// Can be either:
    /// - A bech32m-encoded asset ID (starting with 'passet')
    /// - A raw denomination string (e.g., "upenumbra", "usdc")
    #[clap(long)]
    pub asset_id: String,
}

impl AssetViewingKeyCmd {
    /// Determine if this command requires a network sync before it executes.
    pub fn offline(&self) -> bool {
        true
    }

    pub fn exec(&self, fvk: &FullViewingKey) -> Result<()> {
        // Try parsing as bech32m first, then fall back to raw denom
        let asset_id: asset::Id = if self.asset_id.starts_with("passet") {
            self.asset_id.parse()
                .map_err(|e| anyhow::anyhow!("Failed to parse asset ID: {}", e))?
        } else {
            // Treat as raw denomination
            pb::AssetId {
                alt_base_denom: self.asset_id.clone(),
                ..Default::default()
            }
            .try_into()
            .map_err(|e| anyhow::anyhow!("Failed to derive asset ID from denom '{}': {}", self.asset_id, e))?
        };

        // Show the asset ID being used
        println!("Asset ID: {}", asset_id);
        println!();

        // Create the asset viewing key
        let asset_viewing_key = AssetViewingKey::from_fvk(fvk, asset_id);

        // Print the key in bech32m format
        println!("Asset Viewing Key:");
        println!("{}", asset_viewing_key);

        Ok(())
    }
}
