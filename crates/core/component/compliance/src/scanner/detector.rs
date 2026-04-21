//! Privacy-preserving detection of transfer compliance ciphertexts.
//!
//! This module lets an issuer use its DetectionKey to identify transactions
//! involving a regulated asset without decrypting the flagged-only tiers.

use anyhow::Result;
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;

use super::sync::extract_ciphertexts_full;
use crate::issuer_keys::DetectionKey;
use crate::transfer::TransferComplianceCiphertext;

/// Information about a detected transaction containing the target asset.
#[derive(Clone, Debug)]
pub struct DetectedCiphertext {
    pub height: u64,
    pub tx_index: usize,
    pub action_index: usize,
    pub salt: decaf377::Fq,
    pub ciphertext: TransferComplianceCiphertext,
    pub asset_id: asset::Id,
    pub is_flagged: bool,
}

/// Scan a transaction for ciphertexts matching a specific asset.
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
    let ciphertexts = extract_ciphertexts_full(tx)?;
    let mut matches = 0;

    for extracted in ciphertexts {
        let ciphertext =
            match TransferComplianceCiphertext::from_bytes(&extracted.compliance_ciphertext) {
                Ok(ct) => ct,
                Err(_) => continue,
            };

        let (detected_asset_id, is_flagged, salt) = match detection_key.try_decrypt_detection(
            &ciphertext.sender_core_epk,
            &ciphertext.sender_core_epk,
            &ciphertext.detection_tag,
            &target_asset_id,
        ) {
            Ok(result) => result,
            Err(_) => continue,
        };

        matches += 1;
        callback(DetectedCiphertext {
            height,
            tx_index,
            action_index: extracted.action_index,
            salt,
            ciphertext,
            asset_id: detected_asset_id,
            is_flagged,
        })?;
    }

    Ok(matches)
}

/// Batch scan multiple transactions for a specific asset.
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
        total_matches += scan_transaction(
            detection_key,
            target_asset_id,
            &tx,
            height,
            tx_index,
            &mut callback,
        )?;
    }

    Ok(total_matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_compliance_scalar;
    use crate::test_helpers::make_address;
    use crate::transfer::encrypt_transfer;
    use penumbra_sdk_asset::Value;
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_proto::core::component::shielded_pool::v1::{
        Transfer, TransferBody, TransferOutputBody,
    };
    use penumbra_sdk_proto::core::transaction::v1::{
        action::Action, Action as ActionProto, TransactionBody,
    };
    use rand_core::OsRng;

    fn derive_ack(
        ring_pk: &decaf377::Element,
        address: &penumbra_sdk_keys::Address,
    ) -> decaf377::Element {
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = decaf377::Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    fn make_transfer_tx(ciphertext_bytes: Vec<u8>) -> ProtoTransaction {
        ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![ActionProto {
                    action: Some(Action::Transfer(Transfer {
                        body: Some(TransferBody {
                            outputs: vec![TransferOutputBody {
                                compliance_ciphertext: ciphertext_bytes,
                                ..Default::default()
                            }],
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

    fn make_ciphertext(
        dk_pub: &decaf377::Element,
        ring_pk: &decaf377::Element,
        sender_address: &penumbra_sdk_keys::Address,
        receiver_address: &penumbra_sdk_keys::Address,
        asset_id: asset::Id,
        amount: Amount,
        is_flagged: bool,
        salt: decaf377::Fq,
    ) -> TransferComplianceCiphertext {
        encrypt_transfer(
            &mut OsRng,
            &derive_ack(ring_pk, sender_address),
            &derive_ack(ring_pk, receiver_address),
            dk_pub,
            receiver_address,
            sender_address,
            Value { amount, asset_id },
            is_flagged,
            salt,
        )
        .unwrap()
        .ciphertext
    }

    #[test]
    fn test_scan_transaction_finds_matching_asset() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(11);
        let receiver_address = make_address(21);
        let target_asset = asset::Id(decaf377::Fq::from(9999u64));

        let ciphertext = make_ciphertext(
            &dk_pub,
            &ring_pk,
            &sender_address,
            &receiver_address,
            target_asset,
            Amount::from(123u128),
            false,
            decaf377::Fq::from(0u64),
        );
        let tx = make_transfer_tx(ciphertext.to_bytes());

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
        assert_eq!(scan_result.unwrap(), 1);
        assert_eq!(detected_count, 1);
    }

    #[test]
    fn test_scan_transaction_detects_flagged() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(12);
        let receiver_address = make_address(22);
        let target_asset = asset::Id(decaf377::Fq::from(8888u64));

        let ciphertext = make_ciphertext(
            &dk_pub,
            &ring_pk,
            &sender_address,
            &receiver_address,
            target_asset,
            Amount::from(1_000_000u128),
            true,
            decaf377::Fq::from(1u64),
        );
        let tx = make_transfer_tx(ciphertext.to_bytes());

        let mut detected_flagged = false;
        let scan_result = scan_transaction(&dk, target_asset, &tx, 100, 0, |detected| {
            detected_flagged = detected.is_flagged;
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert!(detected_flagged);
    }

    #[test]
    fn test_scan_transaction_ignores_different_asset() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(13);
        let receiver_address = make_address(23);

        let ciphertext = make_ciphertext(
            &dk_pub,
            &ring_pk,
            &sender_address,
            &receiver_address,
            asset::Id(decaf377::Fq::from(5555u64)),
            Amount::from(999u128),
            false,
            decaf377::Fq::from(2u64),
        );
        let tx = make_transfer_tx(ciphertext.to_bytes());

        let target_asset = asset::Id(decaf377::Fq::from(7777u64));
        let mut callback_called = false;
        let scan_result = scan_transaction(&dk, target_asset, &tx, 200, 0, |_| {
            callback_called = true;
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 0);
        assert!(!callback_called);
    }

    #[test]
    fn test_scan_transaction_with_wrong_detection_key() {
        let dk1 = DetectionKey::demo();
        let dk1_pub = dk1.public_key();
        let dk2 = DetectionKey::from_seed(&[99u8; 32]);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(14);
        let receiver_address = make_address(24);
        let target_asset = asset::Id(decaf377::Fq::from(1111u64));

        let ciphertext = make_ciphertext(
            &dk1_pub,
            &ring_pk,
            &sender_address,
            &receiver_address,
            target_asset,
            Amount::from(456u128),
            false,
            decaf377::Fq::from(3u64),
        );
        let tx = make_transfer_tx(ciphertext.to_bytes());

        let mut callback_called = false;
        let scan_result = scan_transaction(&dk2, target_asset, &tx, 300, 0, |_| {
            callback_called = true;
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 0);
        assert!(!callback_called);
    }

    #[test]
    fn test_scan_transactions_batch() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(15);
        let receiver_address = make_address(25);
        let target_asset = asset::Id(decaf377::Fq::from(3333u64));

        let mut txs = Vec::new();
        for i in 0..3 {
            let asset_id = if i == 1 {
                asset::Id(decaf377::Fq::from(4444u64))
            } else {
                target_asset
            };
            let ciphertext = make_ciphertext(
                &dk_pub,
                &ring_pk,
                &sender_address,
                &receiver_address,
                asset_id,
                Amount::from((100 + i) as u128),
                false,
                decaf377::Fq::from(i as u64),
            );
            txs.push((1000 + i as u64, i, make_transfer_tx(ciphertext.to_bytes())));
        }

        let mut detected = Vec::new();
        let scan_result = scan_transactions(&dk, target_asset, txs, |d| {
            detected.push(d);
            Ok(())
        });

        assert!(scan_result.is_ok());
        assert_eq!(scan_result.unwrap(), 2);
        assert_eq!(detected.len(), 2);
        assert_eq!(detected[0].height, 1000);
        assert_eq!(detected[1].height, 1002);
    }
}
