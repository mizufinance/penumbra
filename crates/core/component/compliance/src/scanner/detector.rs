//! Privacy-preserving detection of compliance ciphertexts.
//!
//! This module provides functions for detecting transactions involving a specific
//! regulated asset by decrypting ONLY the detection_tag (32 bytes) of
//! compliance ciphertexts.
//!
//! # Security Model
//!
//! The detector uses the issuer's `DetectionKey` (DK) which allows:
//! - Decrypting the detection tier (asset_id + flag)
//! - Identifying all transactions involving the issuer's asset
//! - Seeing which transactions are flagged (threshold exceeded)
//!
//! The issuer cannot decrypt core or extension tiers unless the transaction is flagged.

use anyhow::Result;
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;

use super::sync::extract_ciphertexts_full;
use crate::issuer_keys::DetectionKey;
use crate::structs::ComplianceCiphertext;

/// Information about a detected transaction containing the target asset.
///
/// This struct contains the ciphertext, metadata, and detection-tier data.
/// The ciphertext can later be fully decrypted using the UCK if flagged.
#[derive(Clone, Debug)]
pub struct DetectedCiphertext {
    /// The block height where this ciphertext was found.
    pub height: u64,

    /// The transaction index within the block.
    pub tx_index: usize,

    /// The action index within the transaction.
    pub action_index: usize,

    /// Salt from the detection tier, needed for DLEQ metadata hash recomputation.
    pub salt: decaf377::Fq,

    /// The compliance ciphertext (not fully decrypted, just detection tier).
    pub ciphertext: ComplianceCiphertext,

    /// The detected asset ID (from decrypting the detection tier).
    pub asset_id: asset::Id,

    /// Whether this transaction is flagged (threshold exceeded).
    pub is_flagged: bool,

    /// The sender-encrypted ciphertext (96 bytes, only present on outputs).
    /// Contains (gd_fq, pk_fq, amount_fq) encrypted to sender's ack_orbis.
    pub sender_ciphertext: Vec<u8>,
}

/// Scan a transaction for ciphertexts matching a specific asset.
///
/// Uses the issuer's DetectionKey to decrypt only the detection_tag (32 bytes)
/// to extract asset_id and is_flagged. Calls the callback for each match.
///
/// The `target_asset_id` is required because the Fq sentinel approach needs the
/// expected asset_id to determine the flag. This is correct semantically: DK is
/// per-asset, so the caller always knows which asset they're scanning for.
///
/// # Arguments
/// * `detection_key` - The issuer's DetectionKey for this asset
/// * `target_asset_id` - The asset this DK corresponds to
/// * `tx` - The transaction to scan
/// * `height` - Block height for metadata
/// * `tx_index` - Transaction index for metadata
/// * `callback` - Called for each detected ciphertext
///
/// # Returns
/// The number of matches found.
pub fn scan_transaction<F>(
    detection_key: &DetectionKey,
    target_asset_id: asset::Id,
    tx: &ProtoTransaction,
    height: u64,
    tx_index: usize,
    mut callback: F,
) -> Result<usize>
where
    F: FnMut(DetectedCiphertext) -> Result<()>,
{
    // Use shared extraction logic
    let ciphertexts = extract_ciphertexts_full(tx)?;

    let mut matches = 0;

    for extracted in ciphertexts {
        // Parse ciphertext
        let ciphertext = match ComplianceCiphertext::from_bytes(&extracted.compliance_ciphertext) {
            Ok(ct) => ct,
            Err(_) => continue, // Skip malformed ciphertexts
        };

        // Try to decrypt the detection tier using the issuer's detection key.
        // epk_1 is used for both the curve point and the shared secret derivation.
        let (detected_asset_id, is_flagged, salt) = match detection_key.try_decrypt_detection(
            &ciphertext.epk_1,
            &ciphertext.epk_1,
            &ciphertext.detection_tag,
            &target_asset_id,
        ) {
            Ok(result) => result,
            Err(_) => continue, // Detection failed (wrong key or not this asset)
        };

        matches += 1;

        // Call the user's callback with the detected ciphertext
        callback(DetectedCiphertext {
            height,
            tx_index,
            action_index: extracted.action_index,
            salt,
            ciphertext,
            asset_id: detected_asset_id,
            is_flagged,
            sender_ciphertext: extracted.sender_ciphertext,
        })?;
    }

    Ok(matches)
}

/// Batch scan multiple transactions for a specific asset.
///
/// # Arguments
/// * `detection_key` - The issuer's DetectionKey for this asset
/// * `target_asset_id` - The asset this DK corresponds to
/// * `transactions` - Iterator of (height, tx_index, transaction) tuples
/// * `callback` - Called for each detected ciphertext
pub fn scan_transactions<F, I>(
    detection_key: &DetectionKey,
    target_asset_id: asset::Id,
    transactions: I,
    mut callback: F,
) -> Result<usize>
where
    F: FnMut(DetectedCiphertext) -> Result<()>,
    I: IntoIterator<Item = (u64, usize, ProtoTransaction)>,
{
    let mut total_matches = 0;

    for (height, tx_index, tx) in transactions {
        let matches = scan_transaction(
            detection_key,
            target_asset_id,
            &tx,
            height,
            tx_index,
            &mut callback,
        )?;
        total_matches += matches;
    }

    Ok(total_matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issuer_keys::DetectionKey;
    use crate::test_helpers::{encrypt_test_output, make_address};
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_proto::core::component::shielded_pool::v1::{Output, OutputBody};
    use penumbra_sdk_proto::core::transaction::v1::{
        action::Action, Action as ActionProto, TransactionBody,
    };
    use rand_core::OsRng;

    fn make_output_tx(ciphertext_bytes: Vec<u8>) -> ProtoTransaction {
        ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![ActionProto {
                    action: Some(Action::Output(Output {
                        body: Some(OutputBody {
                            compliance_ciphertext: ciphertext_bytes,
                            ..Default::default()
                        }),
                        ..Default::default()
                    })),
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_scan_transaction_finds_matching_asset() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = decaf377::Fr::rand(&mut OsRng);
        let ring_pk = decaf377::Element::GENERATOR * sk_ring;

        let address = make_address(11);
        let target_asset = asset::Id(decaf377::Fq::from(9999u64));
        let amount = Amount::from(123u128);

        let result = encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &address,
            &address,
            target_asset,
            amount,
            false,
        );
        let tx = make_output_tx(result.ciphertext.to_bytes());

        let mut detected_count = 0;
        let scan_result = scan_transaction(&dk, target_asset, &tx, 100, 0, |detected| {
            detected_count += 1;
            assert_eq!(detected.height, 100);
            assert_eq!(detected.tx_index, 0);
            assert_eq!(detected.action_index, 0);
            assert_eq!(detected.asset_id, target_asset);
            assert!(!detected.is_flagged);
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 1, "Should find exactly 1 match");
        assert_eq!(detected_count, 1, "Callback should be called once");
    }

    #[test]
    fn test_scan_transaction_detects_flagged() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = decaf377::Fr::rand(&mut OsRng);
        let ring_pk = decaf377::Element::GENERATOR * sk_ring;

        let address = make_address(11);
        let counterparty_address = make_address(22);
        let target_asset = asset::Id(decaf377::Fq::from(8888u64));
        let amount = Amount::from(1_000_000u128);

        let result = encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &address,
            &counterparty_address,
            target_asset,
            amount,
            true,
        );
        let tx = make_output_tx(result.ciphertext.to_bytes());

        let mut detected_flagged = false;
        let scan_result = scan_transaction(&dk, target_asset, &tx, 100, 0, |detected| {
            assert!(detected.is_flagged, "Transaction should be flagged");
            detected_flagged = true;
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert!(detected_flagged, "Should detect the flagged transaction");
    }

    #[test]
    fn test_scan_transaction_ignores_different_asset() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = decaf377::Fr::rand(&mut OsRng);
        let ring_pk = decaf377::Element::GENERATOR * sk_ring;

        let address = make_address(12);
        let encrypted_asset = asset::Id(decaf377::Fq::from(5555u64));
        let amount = Amount::from(999u128);

        let result = encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &address,
            &address,
            encrypted_asset,
            amount,
            false,
        );
        let tx = make_output_tx(result.ciphertext.to_bytes());

        // Scan for different asset (7777)
        let target_asset = asset::Id(decaf377::Fq::from(7777u64));
        let mut callback_called = false;

        let scan_result = scan_transaction(&dk, target_asset, &tx, 200, 0, |_| {
            callback_called = true;
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 0, "Should find no matches");
        assert!(!callback_called, "Callback should not be called");
    }

    #[test]
    fn test_scan_transaction_with_wrong_detection_key() {
        let dk1 = DetectionKey::demo();
        let dk1_pub = dk1.public_key();
        let dk2 = DetectionKey::from_seed(&[99u8; 32]);

        let sk_ring = decaf377::Fr::rand(&mut OsRng);
        let ring_pk = decaf377::Element::GENERATOR * sk_ring;

        let address = make_address(13);
        let target_asset = asset::Id(decaf377::Fq::from(1111u64));
        let amount = Amount::from(456u128);

        // Encrypt with dk1's public key
        let result = encrypt_test_output(
            &ring_pk,
            &dk1_pub,
            &address,
            &address,
            target_asset,
            amount,
            false,
        );
        let tx = make_output_tx(result.ciphertext.to_bytes());

        // Try to detect with wrong key (dk2)
        let mut callback_called = false;
        let scan_result = scan_transaction(&dk2, target_asset, &tx, 300, 0, |_| {
            callback_called = true;
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 0, "Wrong key should find no matches");
        assert!(
            !callback_called,
            "Callback should not be called with wrong key"
        );
    }

    #[test]
    fn test_scan_transactions_batch() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = decaf377::Fr::rand(&mut OsRng);
        let ring_pk = decaf377::Element::GENERATOR * sk_ring;

        let address = make_address(14);
        let target_asset = asset::Id(decaf377::Fq::from(3333u64));

        // Create 3 transactions, 2 with target asset, 1 with different asset
        let mut txs = Vec::new();
        for i in 0..3 {
            let asset_id = if i == 1 {
                asset::Id(decaf377::Fq::from(4444u64))
            } else {
                target_asset
            };

            let result = encrypt_test_output(
                &ring_pk,
                &dk_pub,
                &address,
                &address,
                asset_id,
                Amount::from((100 + i) as u128),
                false,
            );
            let tx = make_output_tx(result.ciphertext.to_bytes());
            txs.push((1000 + i as u64, i, tx));
        }

        let mut detected = Vec::new();
        let scan_result = scan_transactions(&dk, target_asset, txs, |d| {
            detected.push(d);
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(
            scan_result.unwrap(),
            2,
            "Should find 2 matches (tx 0 and tx 2)"
        );
        assert_eq!(detected.len(), 2);
        assert_eq!(detected[0].height, 1000);
        assert_eq!(detected[1].height, 1002);
    }

    #[test]
    fn test_scan_with_known_asset_id() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = decaf377::Fr::rand(&mut OsRng);
        let ring_pk = decaf377::Element::GENERATOR * sk_ring;

        let address = make_address(15);
        let asset_id = asset::Id(decaf377::Fq::from(5555u64));
        let amount = Amount::from(100u128);

        let result = encrypt_test_output(
            &ring_pk, &dk_pub, &address, &address, asset_id, amount, false,
        );
        let tx = make_output_tx(result.ciphertext.to_bytes());

        let mut detected_asset = None;
        let scan_result = scan_transaction(&dk, asset_id, &tx, 100, 0, |detected| {
            detected_asset = Some(detected.asset_id);
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 1, "Should find 1 match");
        assert_eq!(
            detected_asset,
            Some(asset_id),
            "Should detect the correct asset"
        );
    }
}
