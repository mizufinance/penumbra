use penumbra_sdk_asset::asset;

use super::types::{DetectionEvent, ExtractedComplianceCiphertext, InvalidCiphertext};
use crate::{issuer_keys::DetectionKey, transfer::TransferComplianceCiphertext};

#[derive(Clone)]
pub struct ComplianceScreener {
    detection_key: DetectionKey,
    target_asset_id: asset::Id,
}

#[derive(Clone, Debug)]
pub enum ScreeningResult {
    Irrelevant,
    Detected(DetectionEvent),
    InvalidCiphertext(InvalidCiphertext),
}

impl ComplianceScreener {
    pub fn new(detection_key: DetectionKey, target_asset_id: asset::Id) -> Self {
        Self {
            detection_key,
            target_asset_id,
        }
    }

    pub fn screen(&self, extracted: ExtractedComplianceCiphertext) -> ScreeningResult {
        let ciphertext = match TransferComplianceCiphertext::from_bytes(&extracted.raw_bytes) {
            Ok(ciphertext) => ciphertext,
            Err(error) => {
                return ScreeningResult::InvalidCiphertext(InvalidCiphertext {
                    output_ref: extracted.output_ref,
                    reason: error.to_string(),
                    raw_bytes: extracted.raw_bytes,
                })
            }
        };

        let (asset_id, is_flagged, salt, sender_slot_id, receiver_slot_id) =
            match self.detection_key.try_decrypt_detection(
                &ciphertext.sender_core_epk,
                &ciphertext.sender_core_epk,
                &ciphertext.detection_tag,
                &self.target_asset_id,
            ) {
                Ok(result) => result,
                Err(_) => return ScreeningResult::Irrelevant,
            };

        ScreeningResult::Detected(DetectionEvent {
            output_ref: extracted.output_ref,
            asset_id,
            is_flagged,
            salt,
            sender_slot_id,
            receiver_slot_id,
            ciphertext,
            raw_bytes: extracted.raw_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_compliance_scalar;
    use crate::test_helpers::make_address;
    use crate::transfer::encrypt_transfer;
    use penumbra_sdk_asset::Value;
    use penumbra_sdk_num::Amount;
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

    fn make_extracted(raw_bytes: Vec<u8>, asset_id: asset::Id) -> ExtractedComplianceCiphertext {
        use penumbra_sdk_txhash::TransactionId;

        let block = super::super::types::BlockRef {
            height: 100,
            block_hash: [1u8; 32],
            parent_hash: [2u8; 32],
            block_time_unix: Some(1234),
        };
        let tx = super::super::types::TxRef {
            block,
            tx_index: 0,
            tx_hash: TransactionId([3u8; 32]),
        };
        let action = super::super::types::ActionRef {
            tx,
            action_index: 0,
        };
        let output_ref = super::super::types::OutputRef {
            action,
            output_index: 0,
        };
        let _ = asset_id;
        ExtractedComplianceCiphertext {
            output_ref,
            raw_bytes,
            upload_bundle_bytes: None,
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
            0,
            0,
            salt,
        )
        .unwrap()
        .ciphertext
    }

    #[test]
    fn screener_detects_matching_asset() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(11);
        let receiver_address = make_address(21);
        let asset_id = asset::Id(decaf377::Fq::from(9999u64));

        let ciphertext = make_ciphertext(
            &dk_pub,
            &ring_pk,
            &sender_address,
            &receiver_address,
            asset_id,
            Amount::from(123u128),
            true,
            decaf377::Fq::from(7u64),
        );
        let screener = ComplianceScreener::new(dk, asset_id);

        match screener.screen(make_extracted(ciphertext.to_bytes(), asset_id)) {
            ScreeningResult::Detected(event) => {
                assert_eq!(event.asset_id, asset_id);
                assert!(event.is_flagged);
            }
            other => panic!("expected detection, got {other:?}"),
        }
    }

    #[test]
    fn screener_ignores_wrong_asset_or_key() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(12);
        let receiver_address = make_address(22);
        let asset_id = asset::Id(decaf377::Fq::from(1111u64));
        let other_asset = asset::Id(decaf377::Fq::from(2222u64));
        let ciphertext = make_ciphertext(
            &dk_pub,
            &ring_pk,
            &sender_address,
            &receiver_address,
            asset_id,
            Amount::from(123u128),
            false,
            decaf377::Fq::from(8u64),
        );

        let wrong_asset_screener = ComplianceScreener::new(dk.clone(), other_asset);
        assert!(matches!(
            wrong_asset_screener.screen(make_extracted(ciphertext.to_bytes(), other_asset)),
            ScreeningResult::Irrelevant
        ));

        let wrong_key_screener =
            ComplianceScreener::new(DetectionKey::from_seed(&[99u8; 32]), asset_id);
        assert!(matches!(
            wrong_key_screener.screen(make_extracted(ciphertext.to_bytes(), asset_id)),
            ScreeningResult::Irrelevant
        ));
    }

    #[test]
    fn screener_reports_invalid_ciphertext() {
        let asset_id = asset::Id(decaf377::Fq::from(3333u64));
        let screener = ComplianceScreener::new(DetectionKey::demo(), asset_id);
        match screener.screen(make_extracted(vec![1, 2, 3], asset_id)) {
            ScreeningResult::InvalidCiphertext(invalid) => {
                assert_eq!(invalid.raw_bytes, vec![1, 2, 3]);
                assert!(invalid.reason.contains("transfer compliance ciphertext"));
            }
            other => panic!("expected invalid ciphertext, got {other:?}"),
        }
    }
}
