//! Selection and local decryption of canonical Transfer compliance ciphertext.
use anyhow::{ensure, Result};
use decaf377::{Element, Fq};
use serde::{Deserialize, Serialize};
use shieldd_sdk_num::Amount;

use crate::{
    crypto::{compliance_stream_block, transfer_key_confirmation},
    scanning::AddressData,
    TransferComplianceCiphertext, TransferComplianceMetadata,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferTier {
    SenderCore,
    SenderExt,
    OutputCore,
    OutputExt,
}

/// Public fields selected from one canonical transaction, never a second encryption.
pub struct TransferTierCiphertext<'a> {
    pub epk: Element,
    pub c2: Fq,
    pub ciphertext: &'a [u8],
    pub salt: Fq,
    pub key_confirmation: Option<Fq>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferAuditData {
    Amount(Amount),
    Counterparty(AddressData),
}

impl TransferTier {
    pub fn select<'a>(
        self,
        ct: &'a TransferComplianceCiphertext,
        metadata: &TransferComplianceMetadata,
    ) -> Result<TransferTierCiphertext<'a>> {
        metadata.validate()?;
        let (epk, c2, ciphertext, salt, key_confirmation): (_, _, &[u8], _, _) = match self {
            Self::SenderCore => (
                ct.sender_core_epk,
                ct.sender_core_c2,
                &ct.encrypted_sender_core,
                metadata.sender_core_salt()?,
                Some(ct.sender_core_key_confirmation),
            ),
            Self::SenderExt => (
                ct.sender_ext_epk,
                ct.sender_ext_c2,
                &ct.encrypted_sender_ext,
                metadata.sender_ext_salt()?,
                None,
            ),
            Self::OutputCore => (
                ct.output_core_epk,
                ct.output_core_c2,
                &ct.encrypted_output_core,
                metadata.output_core_salt()?,
                Some(ct.output_core_key_confirmation),
            ),
            Self::OutputExt => (
                ct.output_ext_epk,
                ct.output_ext_c2,
                &ct.encrypted_output_ext,
                metadata.output_ext_salt()?,
                None,
            ),
        };
        ensure!(epk != Element::default(), "identity transfer ephemeral key");
        Ok(TransferTierCiphertext {
            epk,
            c2,
            ciphertext,
            salt,
            key_confirmation,
        })
    }
}

impl TransferTierCiphertext<'_> {
    /// Caller must first verify PRE against this transaction's EPK and encryption key.
    /// A successful decode alone does not authenticate a PRE result or chain acceptance.
    pub fn decrypt(&self, shared: &Element) -> Result<TransferAuditData> {
        ensure!(
            *shared != Element::default(),
            "identity transfer shared point"
        );
        let seed = self.c2 - shared.vartime_compress_to_field();
        self.decrypt_seed(seed)
    }

    pub(crate) fn decrypt_seed(&self, seed: Fq) -> Result<TransferAuditData> {
        let len: usize = if let Some(confirmation) = self.key_confirmation {
            ensure!(
                transfer_key_confirmation(seed, self.epk.vartime_compress_to_field(), self.salt)
                    == confirmation,
                "transfer key confirmation mismatch"
            );
            16
        } else {
            64
        };
        ensure!(
            self.ciphertext.len() == len.div_ceil(31) * 32,
            "invalid transfer tier length"
        );
        let mut plaintext = Vec::with_capacity(len);
        for (i, chunk) in self.ciphertext.chunks_exact(32).enumerate() {
            let ct = Fq::from_bytes_checked(chunk.try_into()?)
                .map_err(|_| anyhow::anyhow!("noncanonical transfer ciphertext"))?;
            let bytes = (ct - compliance_stream_block(seed, i as u64)).to_bytes();
            let take = 31.min(len - plaintext.len());
            ensure!(
                bytes[take..].iter().all(|b| *b == 0),
                "noncanonical transfer plaintext"
            );
            plaintext.extend_from_slice(&bytes[..take]);
        }
        if len == 16 {
            return Ok(TransferAuditData::Amount(Amount::from_le_bytes(
                plaintext.try_into().expect("checked amount length"),
            )));
        }
        let diversified_generator = decaf377::Encoding(plaintext[..32].try_into()?)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid counterparty diversified generator"))?;
        ensure!(
            diversified_generator != Element::default(),
            "identity counterparty diversified generator"
        );
        let transmission_key = plaintext[32..].try_into()?;
        Ok(TransferAuditData::Counterparty(AddressData {
            diversified_generator,
            transmission_key,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::derive_compliance_scalar, test_helpers::make_address, transfer::encrypt_transfer,
    };
    use decaf377::Fr;
    use rand::{rngs::StdRng, SeedableRng};
    use shieldd_sdk_asset::{asset, Value};

    #[test]
    fn decrypt_original_transfer_tiers_with_selected_key() {
        let sender = make_address(31);
        let receiver = make_address(32);
        let ring_sk = Fr::from(29u64);
        let dk = Fr::from(37u64);
        let ring_pk = Element::GENERATOR * ring_sk;
        let sender_d = Fr::from_le_bytes_mod_order(&derive_compliance_scalar(&sender).to_bytes());
        let receiver_d =
            Fr::from_le_bytes_mod_order(&derive_compliance_scalar(&receiver).to_bytes());
        let amount = Amount::from(u128::MAX);
        let metadata = TransferComplianceMetadata::from_identifiers(
            "ring",
            "policy",
            "resource",
            "read",
            1,
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(4u64),
        );
        for flagged in [false, true] {
            let encrypted = encrypt_transfer(
                StdRng::seed_from_u64(17),
                &ring_pk,
                &(ring_pk * sender_d),
                &(ring_pk * receiver_d),
                &(Element::GENERATOR * dk),
                &receiver,
                &sender,
                Value {
                    amount,
                    asset_id: asset::Id(Fq::from(42u64)),
                },
                flagged,
                Fq::from(5u64),
                metadata.sender_core_salt().unwrap(),
                metadata.output_core_salt().unwrap(),
            )
            .unwrap();
            let decoded =
                TransferComplianceCiphertext::from_bytes(&encrypted.ciphertext.to_bytes()).unwrap();
            for selection in crate::master_wrapping::MasterSelection::ALL {
                let selected = selection.tier().select(&decoded, &metadata).unwrap();
                let shared = selected.epk * if flagged { dk } else { ring_sk };
                let expected = match selection {
                    crate::master_wrapping::MasterSelection::Amount => {
                        TransferAuditData::Amount(amount)
                    }
                    crate::master_wrapping::MasterSelection::Sender => {
                        TransferAuditData::Counterparty(AddressData {
                            diversified_generator: *sender.diversified_generator(),
                            transmission_key: sender.transmission_key().0,
                        })
                    }
                    crate::master_wrapping::MasterSelection::Receiver => {
                        TransferAuditData::Counterparty(AddressData {
                            diversified_generator: *receiver.diversified_generator(),
                            transmission_key: receiver.transmission_key().0,
                        })
                    }
                };
                assert_eq!(
                    selection.decrypt(&decoded, &metadata, &shared).unwrap(),
                    expected
                );
                assert_ne!(decoded.master_wrappings[selection as usize], selected.c2);
                if flagged {
                    assert!(selection
                        .decrypt(&decoded, &metadata, &(selected.epk * ring_sk))
                        .is_err());
                }
                for other in crate::master_wrapping::MasterSelection::ALL {
                    if other != selection {
                        assert!(other.decrypt(&decoded, &metadata, &shared).is_err());
                    }
                }
                let mut changed = decoded.clone();
                changed.master_wrappings[selection as usize] += Fq::from(1u64);
                assert!(selection.decrypt(&changed, &metadata, &shared).is_err());
            }
            for tier in [
                TransferTier::SenderCore,
                TransferTier::SenderExt,
                TransferTier::OutputCore,
                TransferTier::OutputExt,
            ] {
                let selected = tier.select(&encrypted.ciphertext, &metadata).unwrap();
                let sender_tier =
                    matches!(tier, TransferTier::SenderCore | TransferTier::SenderExt);
                let key = if flagged {
                    dk
                } else {
                    ring_sk * if sender_tier { sender_d } else { receiver_d }
                };
                let result = selected.decrypt(&(selected.epk * key)).unwrap();
                let expected = match tier {
                    TransferTier::SenderCore | TransferTier::OutputCore => {
                        TransferAuditData::Amount(amount)
                    }
                    TransferTier::SenderExt | TransferTier::OutputExt => {
                        let address = if sender_tier { &receiver } else { &sender };
                        TransferAuditData::Counterparty(AddressData {
                            diversified_generator: *address.diversified_generator(),
                            transmission_key: address.transmission_key().0,
                        })
                    }
                };
                assert_eq!(result, expected);
                // Actual ordinary ciphertext uses the address-derived key for every tier.
                // Possession of the base ring secret alone does not remove that derivation.
                if !flagged {
                    assert!(selected.decrypt(&(selected.epk * ring_sk)).is_err());
                }
                assert!(selected
                    .decrypt(&(selected.epk * (key + Fr::from(1u64))))
                    .is_err());
                assert!(selected.decrypt(&Element::default()).is_err());
            }
        }
    }
}
