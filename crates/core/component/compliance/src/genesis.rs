//! Genesis configuration for the compliance component.
//!
//! This module defines the genesis content structure for configuring
//! regulated assets at chain initialization. Unregulated assets do not
//! need registration - they are proven via IMT non-membership proofs.

use penumbra_sdk_asset::asset;
use serde::{Deserialize, Serialize};

/// Genesis content for the compliance component.
///
/// This allows configuring which regulated assets are registered at genesis.
/// Only regulated assets (is_regulated: true) are stored in the IMT.
/// Unregulated assets need no registration.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Content {
    /// Regulated assets to register at genesis.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_genesis() {
        let content = Content::default();
        assert!(content.native_assets.is_empty());
    }

    #[test]
    fn test_serde_roundtrip() {
        let content = Content::default();
        let json = serde_json::to_string(&content).unwrap();
        let parsed: Content = serde_json::from_str(&json).unwrap();
        assert_eq!(content.native_assets.len(), parsed.native_assets.len());
    }
}
