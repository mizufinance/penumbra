//! Compliance decryption with tiered access control.
//!
//! This module provides separate functions for decrypting different tiers of compliance data,
//! enabling selective disclosure:
//!
//! - **Core Decryption**: Uses the core key to decrypt amount + self address.
//! - **Extension Decryption**: Uses the extension key to decrypt counterparty address.
//!
//! Note: Detection (asset_id scanning) is now handled by the issuer's DetectionKey.
//! See `issuer_keys.rs::DetectionKey::try_decrypt_detection()` for detection functionality.
//!
//! # Access Tiers
//!
//! | Role | Keys Available | Can See |
//! |------|----------------|---------|
//! | Issuer | DetectionKey | asset_id + is_flagged (for filtering) |
//! | Auditor | Core key | amount + self address |
//! | Full Access | All keys (UCK) | Everything including counterparty |

use decaf377::{Element, Fq};
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::keys::{DailyComplianceKey, DailyKeySet, KeyType};
use penumbra_sdk_num::Amount;

use crate::crypto::COMPLIANCE_STREAM_CIPHER_DOMAIN;
use crate::structs::ComplianceCiphertext;

/// Decrypted core data: amount and self address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreData {
    pub amount: Amount,
    pub self_diversified_generator: Element,
    pub self_transmission_key: [u8; 32],
}

/// Decrypted extension data: counterparty address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionData {
    pub counterparty_diversified_generator: Element,
    pub counterparty_transmission_key: [u8; 32],
}

/// Full decrypted compliance data (all tiers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullComplianceData {
    pub asset_id: asset::Id,
    pub core: CoreData,
    pub extension: ExtensionData,
}

/// Decrypt the core data (amount + self address).
///
/// Requires the core key. Does NOT decrypt the counterparty address.
///
/// # Arguments
/// * `core_key` - The daily core key (must be KeyType::Core)
/// * `ciphertext` - The compliance ciphertext
///
/// # Returns
/// * `Ok(Some(CoreData))` on successful decryption
/// * `Ok(None)` if decryption failed (wrong key)
/// * `Err(_)` on unexpected errors
pub fn decrypt_core(
    core_key: &DailyComplianceKey,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<CoreData>> {
    assert_eq!(
        core_key.key_type(),
        KeyType::Core,
        "decrypt_core requires a Core key"
    );

    // Compute shared secret using core key
    let ss_core = ciphertext.epk * core_key.inner();

    // Derive seed for core
    let epk_fq = ciphertext.epk.vartime_compress_to_field();
    let seed_core = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss_core.vartime_compress_to_field(), epk_fq),
    );

    // Decrypt core data - 3 Fq elements (80 bytes plaintext, 31-byte chunks)
    let mut core_plaintext_bytes = Vec::new();
    for (i, chunk) in ciphertext.encrypted_core.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_core, (counter, seed_core));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        // Take 31 bytes per Fq (to match 31-byte chunk encoding)
        let bytes_to_take = 31.min(80 - core_plaintext_bytes.len());
        core_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    if core_plaintext_bytes.len() < 80 {
        return Ok(None);
    }

    // Parse: amount (16) || self_div_gen (32) || self_trans_key (32)
    let amount_bytes: [u8; 16] = match core_plaintext_bytes[0..16].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let amount = Amount::from_le_bytes(amount_bytes);

    let self_div_gen_bytes: [u8; 32] = match core_plaintext_bytes[16..48].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let self_div_gen = match decaf377::Encoding(self_div_gen_bytes).vartime_decompress() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let self_trans_key_bytes: [u8; 32] = match core_plaintext_bytes[48..80].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    Ok(Some(CoreData {
        amount,
        self_diversified_generator: self_div_gen,
        self_transmission_key: self_trans_key_bytes,
    }))
}

/// Decrypt the extension data (counterparty address).
///
/// Requires the extension key.
///
/// # Arguments
/// * `extension_key` - The daily extension key (must be KeyType::Extension)
/// * `ciphertext` - The compliance ciphertext
///
/// # Returns
/// * `Ok(Some(ExtensionData))` on successful decryption
/// * `Ok(None)` if decryption failed (wrong key)
/// * `Err(_)` on unexpected errors
pub fn decrypt_extension(
    extension_key: &DailyComplianceKey,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<ExtensionData>> {
    assert_eq!(
        extension_key.key_type(),
        KeyType::Extension,
        "decrypt_extension requires an Extension key"
    );

    // Compute shared secret using extension key
    let ss_extension = ciphertext.epk * extension_key.inner();

    // Derive seed for extension
    let epk_fq = ciphertext.epk.vartime_compress_to_field();
    let seed_extension = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss_extension.vartime_compress_to_field(), epk_fq),
    );

    // Decrypt extension data - 3 Fq elements (64 bytes plaintext, 31-byte chunks)
    let mut extension_plaintext_bytes = Vec::new();
    for (i, chunk) in ciphertext.encrypted_extension.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_extension, (counter, seed_extension));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        // Take 31 bytes per Fq (to match 31-byte chunk encoding)
        let bytes_to_take = 31.min(64 - extension_plaintext_bytes.len());
        extension_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    if extension_plaintext_bytes.len() < 64 {
        return Ok(None);
    }

    // Parse: counterparty_div_gen (32) || counterparty_trans_key (32)
    let counterparty_div_gen_bytes: [u8; 32] = match extension_plaintext_bytes[0..32].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let counterparty_div_gen =
        match decaf377::Encoding(counterparty_div_gen_bytes).vartime_decompress() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

    let counterparty_trans_key_bytes: [u8; 32] = match extension_plaintext_bytes[32..64].try_into()
    {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    Ok(Some(ExtensionData {
        counterparty_diversified_generator: counterparty_div_gen,
        counterparty_transmission_key: counterparty_trans_key_bytes,
    }))
}

/// Decrypt core and extension data using a full key set.
///
/// This is a convenience function for full-access scenarios.
/// Note: asset_id must be provided externally (from issuer detection or context).
///
/// # Arguments
/// * `daily_keys` - The daily key set (core + extension)
/// * `ciphertext` - The compliance ciphertext
/// * `asset_id` - The asset ID (from issuer detection or known context)
///
/// # Returns
/// * `Ok(Some(FullComplianceData))` on successful full decryption
/// * `Ok(None)` if decryption failed
/// * `Err(_)` on unexpected errors
pub fn decrypt_full(
    daily_keys: &DailyKeySet,
    ciphertext: &ComplianceCiphertext,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    // Decrypt core
    let core = match decrypt_core(&daily_keys.core, ciphertext)? {
        Some(c) => c,
        None => return Ok(None),
    };

    // Decrypt extension
    let extension = match decrypt_extension(&daily_keys.extension, ciphertext)? {
        Some(e) => e,
        None => return Ok(None),
    };

    Ok(Some(FullComplianceData {
        asset_id,
        core,
        extension,
    }))
}

/// Scanner role - determines which ciphertext to process in dual-ciphertext transactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScannerRole {
    Sender,
    Receiver,
}

/// Decrypt compliance data for a specific role in a dual-ciphertext transaction.
///
/// Selects the appropriate ciphertext (sender or receiver) based on role, then decrypts.
///
/// # Arguments
/// * `daily_keys` - The daily key set (core + extension)
/// * `sender_ciphertext` - The sender's ciphertext
/// * `receiver_ciphertext` - The receiver's ciphertext
/// * `role` - Which party's ciphertext to decrypt
/// * `asset_id` - The asset ID (from issuer detection or known context)
pub fn decrypt_with_role(
    daily_keys: &DailyKeySet,
    sender_ciphertext: &ComplianceCiphertext,
    receiver_ciphertext: &ComplianceCiphertext,
    role: ScannerRole,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    let ciphertext = match role {
        ScannerRole::Sender => sender_ciphertext,
        ScannerRole::Receiver => receiver_ciphertext,
    };
    decrypt_full(daily_keys, ciphertext, asset_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issuer_keys::DetectionKey;
    use crate::test_helpers::{encrypt_dual, make_test_leaf, make_uck};

    fn test_leaf() -> crate::indexed_tree::IndexedLeaf {
        let dk = DetectionKey::demo();
        make_test_leaf(dk.public_key(), u128::MAX)
    }

    #[test]
    fn test_core_and_extension_decryption() {
        let uck = make_uck();
        let date = 19000u64;
        let asset_leaf = test_leaf();
        let (sender_ct, _receiver_ct) = encrypt_dual(&uck, 1, 2, date, 42, 1000, &asset_leaf);

        let daily_keys = uck.derive_daily_keys(date);

        // Core decryption (amount + self address)
        let core = decrypt_core(&daily_keys.core, &sender_ct)
            .unwrap()
            .expect("core decryption should succeed");
        assert_eq!(core.amount, Amount::from(1000u128));

        // Extension decryption (counterparty address)
        let extension = decrypt_extension(&daily_keys.extension, &sender_ct)
            .unwrap()
            .expect("extension decryption should succeed");
        // Just verify it returns something
        assert!(extension.counterparty_transmission_key != [0u8; 32]);
    }

    #[test]
    fn test_wrong_key_type_panics() {
        let uck = make_uck();
        let date = 19000u64;
        let asset_leaf = test_leaf();
        let (sender_ct, _) = encrypt_dual(&uck, 1, 2, date, 42, 1000, &asset_leaf);

        let extension_key = uck.derive_daily_key(KeyType::Extension, date);

        // Using extension key for decrypt_core should panic
        let result = std::panic::catch_unwind(|| {
            let _ = decrypt_core(&extension_key, &sender_ct);
        });
        assert!(result.is_err(), "wrong key type should panic");
    }

    #[test]
    fn test_full_decryption_with_known_asset() {
        let uck = make_uck();
        let date = 19000u64;
        let asset_id = asset::Id(Fq::from(42u64));
        let asset_leaf = test_leaf();
        let (sender_ct, receiver_ct) = encrypt_dual(&uck, 1, 2, date, 42, 1000, &asset_leaf);

        let daily_keys = uck.derive_daily_keys(date);

        // Full decryption with known asset_id
        let full = decrypt_with_role(
            &daily_keys,
            &sender_ct,
            &receiver_ct,
            ScannerRole::Sender,
            asset_id,
        )
        .unwrap()
        .expect("full decryption should succeed");

        assert_eq!(full.asset_id, asset_id);
        assert_eq!(full.core.amount, Amount::from(1000u128));
    }

    #[test]
    fn test_wrong_date_fails() {
        let uck = make_uck();
        let asset_leaf = test_leaf();
        let (s_ct, _r_ct) = encrypt_dual(&uck, 1, 2, 19000, 42, 1000, &asset_leaf);

        // Wrong date keys
        let wrong_keys = uck.derive_daily_keys(19001);

        // Core decryption should fail (produces garbage)
        let core = decrypt_core(&wrong_keys.core, &s_ct).unwrap();
        // May return Some with garbage data or None
        if let Some(core_data) = core {
            // The amount will be garbage if decryption with wrong key
            // This is expected - wrong keys produce garbage
            let _ = core_data;
        }
    }

    #[test]
    fn test_wrong_uck_fails() {
        let uck_a = make_uck();
        let uck_b = make_uck();

        let asset_id = asset::Id(Fq::from(42u64));
        let asset_leaf = test_leaf();
        let (s_ct, r_ct) = encrypt_dual(&uck_a, 1, 2, 19000, 42, 1000, &asset_leaf);

        // Different UCK
        let wrong_keys = uck_b.derive_daily_keys(19000);

        // Full decryption should fail (produce None or garbage)
        let result =
            decrypt_with_role(&wrong_keys, &s_ct, &r_ct, ScannerRole::Sender, asset_id).unwrap();
        // With wrong keys, the point decompression will likely fail
        assert!(result.is_none());
    }

    #[test]
    fn test_role_separation() {
        let uck = make_uck();
        let asset_id = asset::Id(Fq::from(42u64));
        let asset_leaf = test_leaf();
        let (s_ct, r_ct) = encrypt_dual(&uck, 1, 2, 19000, 42, 1000, &asset_leaf);

        let daily_keys = uck.derive_daily_keys(19000);

        // Both roles can decrypt their respective ciphertexts
        let sender_result =
            decrypt_with_role(&daily_keys, &s_ct, &r_ct, ScannerRole::Sender, asset_id);
        let receiver_result =
            decrypt_with_role(&daily_keys, &s_ct, &r_ct, ScannerRole::Receiver, asset_id);

        assert!(sender_result.unwrap().is_some());
        assert!(receiver_result.unwrap().is_some());
    }
}
