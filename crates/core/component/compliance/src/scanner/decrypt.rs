//! Full decryption of compliance ciphertexts.
//!
//! With the new key hierarchy, detection is handled by the issuer's DetectionKey.
//! User daily keys (Core, Extension) decrypt the amount/addresses when available.
//!
//! For selective disclosure, use the individual decrypt functions in scanning.rs.

use anyhow::{Context, Result};
use penumbra_sdk_keys::keys::UserComplianceKey;

use crate::scanning::{decrypt_core, decrypt_extension, CoreData, ExtensionData};
use crate::structs::ComplianceCiphertext;

/// Decrypted compliance data (without asset_id since detection is issuer-only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecryptedUserData {
    /// Core data (amount + self address)
    pub core: CoreData,
    /// Extension data (counterparty address)
    pub extension: ExtensionData,
}

/// Decrypt core and extension tiers using UserComplianceKey.
///
/// Derives daily keys internally and decrypts the ciphertext.
/// Note: This does NOT decrypt the detection tier (asset_id).
/// Detection is handled by the issuer's DetectionKey.
pub fn decrypt_compliance(
    uck: &UserComplianceKey,
    date: u64,
    ciphertext: &ComplianceCiphertext,
) -> Result<DecryptedUserData> {
    let daily_keys = uck.derive_daily_keys(date);

    // Decrypt core data
    let core = decrypt_core(&daily_keys.core, ciphertext)
        .context("failed to decrypt core data")?
        .ok_or_else(|| anyhow::anyhow!("core decryption produced None"))?;

    // Decrypt extension data
    let extension = decrypt_extension(&daily_keys.extension, ciphertext)
        .context("failed to decrypt extension data")?
        .ok_or_else(|| anyhow::anyhow!("extension decryption produced None"))?;

    Ok(DecryptedUserData { core, extension })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::encrypt_compliance_details;
    use crate::issuer_keys::DetectionKey;
    use crate::test_helpers::{make_address, make_test_leaf, make_uck, make_wallet};
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_num::Amount;

    #[test]
    fn test_decrypt_compliance_roundtrip() {
        let mut rng = rand_core::OsRng;

        // Setup: Create a user compliance key and derive a wallet key
        let uck = make_uck();
        let (ack, self_address) = make_wallet(&uck, 7);

        // Counterparty address
        let counterparty_address = make_address(8);

        // Test data
        let date = 19000u64;
        let asset_id = asset::Id(decaf377::Fq::from(12345u64));
        let amount = Amount::from(999u128);

        // Create asset leaf with detection key
        let dk = DetectionKey::demo();
        let asset_leaf = make_test_leaf(dk.public_key(), u128::MAX);

        // Encrypt
        let result = encrypt_compliance_details(
            &mut rng,
            &ack,
            &self_address,
            date,
            asset_id,
            amount,
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Decrypt using UCK
        let decrypted = decrypt_compliance(&uck, date, &result.ciphertext)
            .expect("decryption should succeed with correct UCK");

        // Verify core data matches
        assert_eq!(decrypted.core.amount, amount, "amount should match");

        // Verify self address components
        assert_eq!(
            decrypted.core.self_diversified_generator,
            *self_address.diversified_generator(),
            "self diversified generator should match"
        );
        assert_eq!(
            decrypted.core.self_transmission_key,
            self_address.transmission_key().0,
            "self transmission key should match"
        );

        // Verify counterparty address components
        assert_eq!(
            decrypted.extension.counterparty_diversified_generator,
            *counterparty_address.diversified_generator(),
            "counterparty diversified generator should match"
        );
        assert_eq!(
            decrypted.extension.counterparty_transmission_key,
            counterparty_address.transmission_key().0,
            "counterparty transmission key should match"
        );
    }

    #[test]
    fn test_decrypt_with_wrong_uck_fails() {
        let mut rng = rand_core::OsRng;

        // Create two different UCKs
        let uck1 = make_uck();
        let uck2 = make_uck();

        let (ack1, self_address) = make_wallet(&uck1, 9);
        let counterparty_address = self_address.clone();

        // Encrypt with uck1
        let date = 19001u64;
        let asset_id = asset::Id(decaf377::Fq::from(54321u64));
        let amount = Amount::from(777u128);

        // Create asset leaf
        let dk = DetectionKey::demo();
        let asset_leaf = make_test_leaf(dk.public_key(), u128::MAX);

        let result = encrypt_compliance_details(
            &mut rng,
            &ack1,
            &self_address,
            date,
            asset_id,
            amount,
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Try to decrypt with uck2 (wrong key) - should fail
        let decrypt_result = decrypt_compliance(&uck2, date, &result.ciphertext);

        // Decryption should fail (point decompression will fail with garbage)
        assert!(decrypt_result.is_err(), "wrong UCK should fail decryption");
    }

    #[test]
    fn test_decrypt_with_wrong_date_fails() {
        let mut rng = rand_core::OsRng;

        let uck = make_uck();
        let (ack, self_address) = make_wallet(&uck, 10);
        let counterparty_address = self_address.clone();

        // Encrypt for date 19000
        let encryption_date = 19000u64;
        let asset_id = asset::Id(decaf377::Fq::from(11111u64));
        let amount = Amount::from(555u128);

        // Create asset leaf
        let dk = DetectionKey::demo();
        let asset_leaf = make_test_leaf(dk.public_key(), u128::MAX);

        let result = encrypt_compliance_details(
            &mut rng,
            &ack,
            &self_address,
            encryption_date,
            asset_id,
            amount,
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Try to decrypt with wrong date (19001 instead of 19000)
        let wrong_date = 19001u64;
        let decrypt_result = decrypt_compliance(&uck, wrong_date, &result.ciphertext);

        // Decryption should fail (wrong keys produce garbage)
        assert!(decrypt_result.is_err(), "wrong date should fail decryption");
    }
}
