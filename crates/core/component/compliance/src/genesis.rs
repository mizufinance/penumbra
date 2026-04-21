//! Genesis configuration for the compliance component.
//!
//! This module defines the genesis content structure for configuring
//! compliance asset entries at chain initialization.
//!
//! The IMT always contains a structural sentinel leaf and the protocol also
//! seeds the neutral base asset as an explicit unregulated entry. Additional
//! regulated assets may be configured here, while other unregulated assets
//! continue to use IMT non-membership proofs.

use penumbra_sdk_asset::asset;
use serde::{Deserialize, Serialize};

/// Genesis content for the compliance component.
///
/// This allows configuring additional compliance asset entries at genesis.
/// The IMT already contains a structural sentinel and a seeded unregulated base
/// asset; entries listed here are added on top of that baseline.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Content {
    /// Native assets to register explicitly at genesis.
    pub native_assets: Vec<NativeAssetRegistration>,
}

/// Registration configuration for a native asset at genesis.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NativeAssetRegistration {
    /// The asset ID to register.
    pub asset_id: asset::Id,
    /// Whether this asset is regulated (requires compliance proofs).
    pub is_regulated: bool,
    /// Issuer detection key (required if is_regulated is true).
    /// Encoded as 32-byte compressed curve point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dk_pub: Option<[u8; 32]>,
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
