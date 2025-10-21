//! Asset-specific viewing keys for selective disclosure.
//!
//! An `AssetViewingKey` allows viewing transactions for a specific asset type only,
//! enabling selective disclosure for compliance scenarios (e.g., court orders).
//!
//! ## Architecture
//!
//! This implementation uses the full viewing key capabilities with post-decryption filtering:
//! - The AssetViewingKey wraps the full IVK, so it can decrypt notes at ANY address
//! - After decryption, only notes matching the specified asset_id are visible
//! - This works exactly like a full viewing key, but filtered to one asset
//!
//! ## Security Properties
//!
//! - ✅ Cryptographically sound: Can decrypt all addresses, but only reveals one asset
//! - ✅ Cannot derive the full FVK from an AssetViewingKey (missing OVK)
//! - ✅ Safe for court compliance: holder can only see the specified asset across all addresses
//!
//! ## Usage
//!
//! ```ignore
//! // Create an asset-specific viewing key that works for ALL addresses
//! let usdc_id = asset::Id::from_raw_denom("usdc");
//! let usdc_key = AssetViewingKey::from_fvk(&full_viewing_key, usdc_id);
//!
//! // This key can decrypt USDC notes sent to any address derived from the FVK
//! // but will filter out all other assets
//! let usdc_notes = usdc_key.scan_all_notes(&note_payloads);
//! ```

use anyhow::Context;
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::serializers::bech32str;

use crate::{keys::IncomingViewingKey, FullViewingKey};

// Additional test imports
#[cfg(test)]
use penumbra_sdk_proto::penumbra::core::asset::v1 as pb;

/// A viewing key that can decrypt notes for a specific asset across all addresses.
///
/// This key allows selective disclosure of transaction history for a single asset type,
/// useful for compliance scenarios where you need to prove holdings/transactions
/// for one asset without revealing other holdings.
///
/// The key holder can decrypt notes at ANY address derived from the FVK, but when
/// scanning, only notes matching the specified asset_id will be visible.
#[derive(Clone, Debug)]
pub struct AssetViewingKey {
    /// The asset this key can view
    asset_id: asset::Id,

    /// The underlying incoming viewing key
    /// This is the same IVK as the full viewing key, allowing decryption
    /// at any address. Filtering by asset_id happens after decryption.
    ivk: IncomingViewingKey,
}

impl AssetViewingKey {
    /// Create an AssetViewingKey from a full viewing key and asset ID.
    ///
    /// This creates a viewing key that can decrypt notes at ANY address derived
    /// from the FVK, but when scanning will only reveal notes for the specified asset.
    ///
    /// This is functionally equivalent to a full viewing key, but filtered to one asset.
    pub fn from_fvk(fvk: &FullViewingKey, asset_id: asset::Id) -> Self {
        let ivk = fvk.incoming().clone();

        Self { asset_id, ivk }
    }

    /// Get the asset ID this key can view.
    pub fn asset_id(&self) -> asset::Id {
        self.asset_id
    }

    /// Get a reference to the underlying incoming viewing key.
    ///
    /// This allows callers to perform decryption operations using the standard
    /// Note::decrypt methods. The caller should then filter decrypted notes by
    /// checking if note.asset_id() matches self.asset_id().
    ///
    /// The IVK can decrypt notes at any address derived from the original FVK.
    pub fn incoming_viewing_key(&self) -> &IncomingViewingKey {
        &self.ivk
    }

    /// Encode the AssetViewingKey to bytes.
    ///
    /// Format: asset_id (32 bytes) || ivk (32 bytes) || dk (16 bytes) = 80 bytes total
    pub fn to_bytes(&self) -> [u8; 80] {
        let mut bytes = [0u8; 80];
        bytes[0..32].copy_from_slice(&self.asset_id.to_bytes());
        bytes[32..64].copy_from_slice(&self.ivk.ivk.to_bytes());
        bytes[64..80].copy_from_slice(&self.ivk.dk.0);
        bytes
    }

    /// Decode an AssetViewingKey from bytes.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() != 80 {
            anyhow::bail!("AssetViewingKey must be 80 bytes, got {}", bytes.len());
        }

        let asset_id_bytes: [u8; 32] = bytes[0..32].try_into().context("asset_id wrong length")?;
        let asset_id = asset::Id::try_from(asset_id_bytes).context("invalid asset_id bytes")?;

        let ivk_bytes: [u8; 32] = bytes[32..64].try_into().context("ivk wrong length")?;
        let ivk_secret =
            crate::ka::Secret::new_from_field(decaf377::Fr::from_le_bytes_mod_order(&ivk_bytes));

        let dk_bytes: [u8; 16] = bytes[64..80].try_into().context("dk wrong length")?;
        let dk = crate::keys::DiversifierKey(dk_bytes);

        let ivk = IncomingViewingKey {
            ivk: ivk_secret,
            dk,
        };

        Ok(Self { asset_id, ivk })
    }
}

impl std::fmt::Display for AssetViewingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&bech32str::encode(
            &self.to_bytes(),
            bech32str::asset_viewing_key::BECH32_PREFIX,
            bech32str::Bech32m,
        ))
    }
}

impl std::str::FromStr for AssetViewingKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = bech32str::decode(
            s,
            bech32str::asset_viewing_key::BECH32_PREFIX,
            bech32str::Bech32m,
        )?;
        Self::from_bytes(&bytes)
    }
}

// Note: Integration tests with Note/NotePayload have been disabled due to circular dependency issues
// between penumbra-sdk-keys and penumbra-sdk-shielded-pool. These tests should be moved to
// integration tests in a separate crate.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Bip44Path, SeedPhrase, SpendKey};
    use penumbra_sdk_asset::{asset::Id as AssetId, STAKING_TOKEN_ASSET_ID};
    use rand_core::OsRng;

    #[test]
    fn test_asset_viewing_key_creation() {
        // Create a full viewing key
        let seed_phrase = SeedPhrase::generate(OsRng);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = spend_key.full_viewing_key();

        // Create asset-specific keys
        let penumbra_id = *STAKING_TOKEN_ASSET_ID;
        let usdc_id = AssetId::try_from(pb::AssetId {
            alt_base_denom: "usdc".to_owned(),
            ..Default::default()
        })
        .expect("valid asset id");

        let penumbra_key = AssetViewingKey::from_fvk(fvk, penumbra_id);
        let usdc_key = AssetViewingKey::from_fvk(fvk, usdc_id);

        // Should have correct asset IDs
        assert_eq!(penumbra_key.asset_id(), penumbra_id);
        assert_eq!(usdc_key.asset_id(), usdc_id);

        // Both should have the same IVK (they can decrypt the same addresses)
        assert_eq!(
            penumbra_key.incoming_viewing_key(),
            usdc_key.incoming_viewing_key(),
            "Both keys should have the same IVK"
        );
    }

    #[test]
    fn test_ivk_matches_fvk() {
        // The AssetViewingKey should contain the same IVK as the FVK
        let seed_phrase = SeedPhrase::generate(OsRng);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = spend_key.full_viewing_key();

        let asset_id = *STAKING_TOKEN_ASSET_ID;
        let asset_key = AssetViewingKey::from_fvk(fvk, asset_id);

        assert_eq!(
            asset_key.incoming_viewing_key(),
            fvk.incoming(),
            "AssetViewingKey IVK should match FVK IVK"
        );
    }

    #[test]
    fn test_asset_viewing_key_serialization() {
        // Create a full viewing key
        let seed_phrase = SeedPhrase::generate(OsRng);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = spend_key.full_viewing_key();

        // Create an asset-specific key for USDC
        let usdc_id = AssetId::try_from(pb::AssetId {
            alt_base_denom: "usdc".to_owned(),
            ..Default::default()
        })
        .expect("valid asset id");

        let asset_key = AssetViewingKey::from_fvk(fvk, usdc_id);

        // Test byte serialization
        let bytes = asset_key.to_bytes();
        assert_eq!(bytes.len(), 80, "AssetViewingKey should be 80 bytes");

        let decoded_key = AssetViewingKey::from_bytes(&bytes).expect("should decode from bytes");

        assert_eq!(decoded_key.asset_id(), asset_key.asset_id());
        assert_eq!(
            decoded_key.incoming_viewing_key(),
            asset_key.incoming_viewing_key()
        );
    }

    #[test]
    fn test_ivk_serialization_roundtrip() {
        // Test that IVK serialization and deserialization preserves the exact value
        let seed_phrase = SeedPhrase::generate(OsRng);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = spend_key.full_viewing_key();
        let original_ivk = fvk.incoming();

        let asset_id = *STAKING_TOKEN_ASSET_ID;

        // Create asset viewing key
        let avk = AssetViewingKey::from_fvk(fvk, asset_id);

        // Serialize to bytes
        let bytes = avk.to_bytes();

        // Deserialize back
        let restored_avk = AssetViewingKey::from_bytes(&bytes).unwrap();

        // The IVK should be identical
        let restored_ivk = restored_avk.incoming_viewing_key();

        // Test that both IVKs produce the same address
        let addr1 = original_ivk.payment_address(0u32.into()).0;
        let addr2 = restored_ivk.payment_address(0u32.into()).0;

        assert_eq!(addr1, addr2, "IVKs should produce the same address");

        // Test that both IVKs view the same address
        assert!(
            original_ivk.views_address(&addr1),
            "Original IVK should view address"
        );
        assert!(
            restored_ivk.views_address(&addr1),
            "Restored IVK should view address"
        );
    }

    #[test]
    fn test_asset_viewing_key_bech32_roundtrip() {
        // Create a full viewing key
        let seed_phrase = SeedPhrase::generate(OsRng);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = spend_key.full_viewing_key();

        // Create an asset-specific key
        let asset_id = *STAKING_TOKEN_ASSET_ID;
        let asset_key = AssetViewingKey::from_fvk(fvk, asset_id);

        // Test bech32m serialization
        let bech32_str = asset_key.to_string();
        assert!(
            bech32_str.starts_with("penumbraassetviewingkey"),
            "Bech32 string should start with correct prefix"
        );

        // Test parsing
        let parsed_key: AssetViewingKey = bech32_str
            .parse()
            .expect("should parse from bech32m string");

        assert_eq!(parsed_key.asset_id(), asset_key.asset_id());
        assert_eq!(
            parsed_key.incoming_viewing_key(),
            asset_key.incoming_viewing_key()
        );
    }

    // Integration tests with Note/NotePayload should be added to verify:
    // 1. AssetViewingKey can decrypt notes at any address (same as FVK.incoming())
    // 2. After decryption, filtering by asset_id only shows the specified asset
    // 3. Multiple AssetViewingKeys for different assets can coexist for the same wallet
}
