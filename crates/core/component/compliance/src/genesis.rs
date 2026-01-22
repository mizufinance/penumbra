//! Genesis configuration for the compliance component.
//!
//! This module defines the genesis content structure for configurable asset
//! registration at chain initialization. Currently uses a simple Rust-native
//! structure that can be extended with proto definitions in the future.

use penumbra_sdk_asset::asset;
use serde::{Deserialize, Serialize};

/// Genesis content for the compliance component.
///
/// This allows configuring which assets are registered at genesis and their
/// regulation status.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Content {
    /// Assets to auto-register at genesis with their regulation status.
    pub native_assets: Vec<NativeAssetRegistration>,
}

/// Registration configuration for a native asset at genesis.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NativeAssetRegistration {
    /// The asset ID to register.
    pub asset_id: asset::Id,
    /// Whether this asset is regulated (requires compliance proofs).
    pub is_regulated: bool,
}

impl Content {
    /// Creates default genesis content with the staking token and test USD
    /// registered as unregulated.
    ///
    /// The staking token must be registered because fee payments require it,
    /// creating a bootstrapping problem if it's not pre-registered.
    /// Test USD is included for testing convenience.
    pub fn with_defaults() -> Self {
        use penumbra_sdk_asset::{STAKING_TOKEN_ASSET_ID, TEST_USD_ASSET_ID};

        Self {
            native_assets: vec![
                NativeAssetRegistration {
                    asset_id: *STAKING_TOKEN_ASSET_ID,
                    is_regulated: false,
                },
                NativeAssetRegistration {
                    asset_id: *TEST_USD_ASSET_ID,
                    is_regulated: false,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_genesis() {
        let content = Content::default();
        assert!(content.native_assets.is_empty());
    }

    #[test]
    fn test_with_defaults_includes_staking_token() {
        use penumbra_sdk_asset::STAKING_TOKEN_ASSET_ID;

        let content = Content::with_defaults();
        assert!(!content.native_assets.is_empty());

        let staking_token = content
            .native_assets
            .iter()
            .find(|a| a.asset_id == *STAKING_TOKEN_ASSET_ID);
        assert!(staking_token.is_some());
        assert!(!staking_token.unwrap().is_regulated);
    }

    #[test]
    fn test_serde_roundtrip() {
        let content = Content::with_defaults();
        let json = serde_json::to_string(&content).unwrap();
        let parsed: Content = serde_json::from_str(&json).unwrap();
        assert_eq!(content.native_assets.len(), parsed.native_assets.len());
    }
}
