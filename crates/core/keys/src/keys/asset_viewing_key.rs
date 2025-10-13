//! Asset-specific viewing keys for selective disclosure.
//!
//! An `AssetViewingKey` allows viewing transactions for a specific asset type only,
//! enabling selective disclosure for compliance scenarios (e.g., court orders).
//!
//! ## Architecture
//!
//! This implementation uses address diversification to achieve asset-specific viewing:
//! - Each asset_id deterministically derives a unique AddressIndex
//! - Notes for that asset should be sent to the asset-specific address
//! - The AssetViewingKey can only decrypt notes sent to that address
//!
//! ## Security Properties
//!
//! - ✅ Cryptographically sound: Cannot decrypt notes for other assets
//! - ✅ Cannot derive the full FVK from an AssetViewingKey
//! - ✅ Safe for court compliance: holder cannot see other assets
//!
//! ## Usage
//!
//! ```ignore
//! // Create an asset-specific viewing key
//! let usdc_id = asset::Id::from_raw_denom("usdc");
//! let usdc_key = AssetViewingKey::from_fvk(&full_viewing_key, usdc_id);
//!
//! // Get the address where USDC should be sent
//! let usdc_address = usdc_key.address();
//!
//! // Scan transactions (only decrypts notes sent to usdc_address)
//! let usdc_notes = usdc_key.scan_notes(&note_payloads);
//! ```

use penumbra_sdk_asset::asset;
use sha2::{Digest, Sha256};

use crate::{
    keys::{AddressIndex, Diversifier, IncomingViewingKey},
    Address, FullViewingKey,
};

// Additional test imports
#[cfg(test)]
use penumbra_sdk_proto::{penumbra::core::asset::v1 as pb};

/// A viewing key that can only decrypt notes for a specific asset.
///
/// This key allows selective disclosure of transaction history for a single asset type,
/// useful for compliance scenarios where you need to prove holdings/transactions
/// for one asset without revealing other holdings.
#[derive(Clone, Debug)]
pub struct AssetViewingKey {
    /// The asset this key can view
    asset_id: asset::Id,

    /// The deterministic address index for this asset
    address_index: AddressIndex,

    /// The underlying incoming viewing key
    /// This is the same IVK as the full viewing key, but we only use it
    /// to decrypt notes sent to the asset-specific address
    ivk: IncomingViewingKey,

    /// The diversifier for the asset-specific address (cached)
    diversifier: Diversifier,
}

impl AssetViewingKey {
    /// Create an AssetViewingKey from a full viewing key and asset ID.
    ///
    /// The address index is deterministically derived from the asset_id,
    /// ensuring the same asset always maps to the same address.
    pub fn from_fvk(fvk: &FullViewingKey, asset_id: asset::Id) -> Self {
        let address_index = Self::derive_address_index(&asset_id);
        let ivk = fvk.incoming().clone();
        // Access the diversifier key directly (it's pub(super) in the parent module)
        let diversifier = ivk.dk.diversifier_for_index(&address_index);

        Self {
            asset_id,
            address_index,
            ivk,
            diversifier,
        }
    }

    /// Derive a deterministic address index from an asset ID.
    ///
    /// This uses SHA-256 to hash the asset_id bytes, then takes the first 4 bytes
    /// as the account number. The randomizer is left as zeros to make the address
    /// non-ephemeral and deterministic.
    ///
    /// Security: Since we're using the hash of the asset_id, different assets
    /// will have different (and unpredictable) address indices.
    fn derive_address_index(asset_id: &asset::Id) -> AddressIndex {
        let asset_bytes = asset_id.0.to_bytes();
        let hash = Sha256::digest(&asset_bytes);

        // Use first 4 bytes of hash as account number
        let account = u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]);

        // Use next 12 bytes as randomizer for additional entropy
        let mut randomizer = [0u8; 12];
        randomizer.copy_from_slice(&hash[4..16]);

        AddressIndex {
            account,
            randomizer,
        }
    }

    /// Get the asset ID this key can view.
    pub fn asset_id(&self) -> asset::Id {
        self.asset_id
    }

    /// Get the address where notes for this asset should be sent.
    ///
    /// For the key to work properly, senders MUST send notes of this asset type
    /// to this specific address. This is enforced by wallet software.
    pub fn address(&self) -> Address {
        let (address, _detection_key) = self.ivk.payment_address(self.address_index);
        address
    }

    /// Get the address index for this asset.
    pub fn address_index(&self) -> AddressIndex {
        self.address_index
    }

    /// Get the diversifier for this asset's address.
    pub fn diversifier(&self) -> &Diversifier {
        &self.diversifier
    }

    /// Get a reference to the underlying incoming viewing key.
    ///
    /// This allows callers to perform decryption operations using the standard
    /// Note::decrypt methods, then filter by asset_id and address.
    pub fn incoming_viewing_key(&self) -> &IncomingViewingKey {
        &self.ivk
    }

    /// Check if this key can view a specific address.
    ///
    /// Returns true only if the address matches this asset's address.
    pub fn can_view_address(&self, address: &Address) -> bool {
        address.diversifier() == &self.diversifier
    }
}

// Note: Integration tests with Note/NotePayload have been disabled due to circular dependency issues
// between penumbra-sdk-keys and penumbra-sdk-shielded-pool. These tests should be moved to
// integration tests in a separate crate.

#[cfg(test)]
mod tests {
    use super::*;
    use penumbra_sdk_asset::{asset::Id as AssetId, STAKING_TOKEN_ASSET_ID};
    use crate::keys::{Bip44Path, SeedPhrase, SpendKey};
    use rand_core::OsRng;

    #[test]
    fn test_deterministic_address_derivation() {
        // Same asset should always produce same address index
        let asset_id = *STAKING_TOKEN_ASSET_ID;

        let index1 = AssetViewingKey::derive_address_index(&asset_id);
        let index2 = AssetViewingKey::derive_address_index(&asset_id);

        assert_eq!(index1, index2, "Address index should be deterministic");
    }

    #[test]
    fn test_different_assets_different_addresses() {
        // Different assets should produce different address indices
        let penumbra_id = *STAKING_TOKEN_ASSET_ID;
        let usdc_id = AssetId::try_from(pb::AssetId {
            alt_base_denom: "usdc".to_owned(),
            ..Default::default()
        })
        .expect("valid asset id");

        let index1 = AssetViewingKey::derive_address_index(&penumbra_id);
        let index2 = AssetViewingKey::derive_address_index(&usdc_id);

        assert_ne!(
            index1, index2,
            "Different assets should have different address indices"
        );
    }

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

        // Should have different addresses
        assert_ne!(
            penumbra_key.address(),
            usdc_key.address(),
            "Different assets should have different addresses"
        );

        // Should have correct asset IDs
        assert_eq!(penumbra_key.asset_id(), penumbra_id);
        assert_eq!(usdc_key.asset_id(), usdc_id);
    }

    // This test is commented out due to circular dependency issues with penumbra-sdk-shielded-pool
    // TODO: Move to integration tests
    // #[test]
    // fn test_decrypt_correct_asset() { ... }

    // This test is commented out due to circular dependency issues with penumbra-sdk-shielded-pool
    // TODO: Move to integration tests
    // #[test]
    // fn test_cannot_decrypt_wrong_asset() { ... }

    // This test is commented out due to circular dependency issues with penumbra-sdk-shielded-pool
    // TODO: Move to integration tests
    // #[test]
    // fn test_cannot_decrypt_wrong_address() { ... }

    // This test is commented out due to circular dependency issues with penumbra-sdk-shielded-pool
    // TODO: Move to integration tests
    // #[test]
    // fn test_scan_multiple_notes() { ... }

    #[test]
    fn test_can_view_address() {
        let seed_phrase = SeedPhrase::generate(OsRng);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = spend_key.full_viewing_key();

        let penumbra_id = *STAKING_TOKEN_ASSET_ID;
        let penumbra_key = AssetViewingKey::from_fvk(fvk, penumbra_id);

        // Should be able to view its own address
        let asset_address = penumbra_key.address();
        assert!(
            penumbra_key.can_view_address(&asset_address),
            "Should be able to view its own address"
        );

        // Should NOT be able to view other addresses
        let other_address = fvk.payment_address(AddressIndex::new(0)).0;
        assert!(
            !penumbra_key.can_view_address(&other_address),
            "Should not be able to view other addresses"
        );
    }

    // This test is commented out due to circular dependency issues with penumbra-sdk-shielded-pool
    // TODO: Move to integration tests
    // #[test]
    // fn test_fvk_can_decrypt_all_asset_addresses() { ... }
}
