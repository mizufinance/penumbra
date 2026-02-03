//! Compliance scanning for regulated asset transfers.
//!
//! Note: Detection (asset_id) is now handled by the issuer's DetectionKey.
//! See `detector.rs` for the issuer detection workflow.
//!
//! This module provides user-side decryption of core and extension tiers
//! once the asset is known (from issuer detection or context).

use anyhow::Result;
use decaf377::Element;
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::keys::{DailyKeySet, UserComplianceKey};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::core::component::sct::v1::Nullifier;
use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;
use serde::{Deserialize, Serialize};

use crate::scanning::{decrypt_core, decrypt_extension};
use crate::structs::ComplianceCiphertext;

use super::decrypt::{decrypt_compliance, DecryptedUserData};

/// Partial address from decrypted compliance data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialAddress {
    pub diversified_generator: [u8; 32],
    pub transmission_key: [u8; 32],
}

impl PartialAddress {
    pub fn new(diversified_generator: Element, transmission_key: [u8; 32]) -> Self {
        Self {
            diversified_generator: diversified_generator.vartime_compress().0,
            transmission_key,
        }
    }
}

impl std::fmt::Display for PartialAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "g_d:{:02x}{:02x}..{:02x}{:02x},pk:{:02x}{:02x}..{:02x}{:02x}",
            self.diversified_generator[0],
            self.diversified_generator[1],
            self.diversified_generator[30],
            self.diversified_generator[31],
            self.transmission_key[0],
            self.transmission_key[1],
            self.transmission_key[30],
            self.transmission_key[31],
        )
    }
}

/// A detected transfer of a regulated asset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedTransfer {
    pub height: u64,
    pub action_index: usize,
    pub asset_id: asset::Id,
    pub amount: Amount,
    pub self_address: PartialAddress,
    pub counterparty_address: PartialAddress,
    pub nullifier: Option<Nullifier>,
}

impl std::fmt::Display for DetectedTransfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "height={}, action={}, asset={}, amount={}, self={}, counterparty={}",
            self.height,
            self.action_index,
            self.asset_id,
            self.amount,
            self.self_address,
            self.counterparty_address,
        )
    }
}

/// Extract compliance ciphertexts from a transaction.
/// Returns (action_index, ciphertext_bytes, is_spend) tuples.
pub fn extract_compliance_ciphertexts(
    tx: &ProtoTransaction,
) -> Result<Vec<(usize, Vec<u8>, bool)>> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action;

    let actions = match &tx.body {
        Some(body) => &body.actions,
        None => return Ok(vec![]),
    };

    let mut results = Vec::new();
    for (idx, action) in actions.iter().enumerate() {
        match &action.action {
            Some(Action::Output(output)) => {
                if let Some(body) = &output.body {
                    if !body.compliance_ciphertext.is_empty() {
                        results.push((idx, body.compliance_ciphertext.clone(), false));
                    }
                }
            }
            Some(Action::Spend(spend)) => {
                if let Some(body) = &spend.body {
                    if !body.compliance_ciphertext.is_empty() {
                        results.push((idx, body.compliance_ciphertext.clone(), true));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(results)
}

/// Core scan logic - decrypts ciphertexts and builds DetectedTransfer structs.
///
/// Note: asset_id must be known (from issuer detection or context).
fn scan_transaction_core<F>(
    tx: &ProtoTransaction,
    height: u64,
    asset_id: asset::Id,
    mut decrypt_fn: F,
) -> Result<Vec<DetectedTransfer>>
where
    F: FnMut(&ComplianceCiphertext) -> Result<DecryptedUserData>,
{
    let ciphertexts = extract_compliance_ciphertexts(tx)?;
    let mut detected = Vec::new();

    for (action_index, ciphertext_bytes, _) in ciphertexts {
        let ciphertext = match ComplianceCiphertext::from_bytes(&ciphertext_bytes) {
            Ok(ct) => ct,
            Err(_) => continue,
        };

        let decrypted = match decrypt_fn(&ciphertext) {
            Ok(data) => data,
            Err(_) => continue,
        };

        detected.push(DetectedTransfer {
            height,
            action_index,
            asset_id,
            amount: decrypted.core.amount,
            self_address: PartialAddress::new(
                decrypted.core.self_diversified_generator,
                decrypted.core.self_transmission_key,
            ),
            counterparty_address: PartialAddress::new(
                decrypted.extension.counterparty_diversified_generator,
                decrypted.extension.counterparty_transmission_key,
            ),
            nullifier: None,
        });
    }

    Ok(detected)
}

/// Scan a transaction using UserComplianceKey.
///
/// Note: `asset_id` must be known (from issuer detection or context).
pub fn scan_transaction_for_compliance(
    tx: &ProtoTransaction,
    height: u64,
    uck: &UserComplianceKey,
    date: u64,
    asset_id: asset::Id,
) -> Result<Vec<DetectedTransfer>> {
    scan_transaction_core(tx, height, asset_id, |ct| decrypt_compliance(uck, date, ct))
}

/// Scan multiple transactions using UserComplianceKey.
///
/// Note: `asset_id` must be known (from issuer detection or context).
pub fn scan_transactions_for_compliance<I>(
    transactions: I,
    uck: &UserComplianceKey,
    date: u64,
    asset_id: asset::Id,
) -> Result<Vec<DetectedTransfer>>
where
    I: IntoIterator<Item = (u64, ProtoTransaction)>,
{
    let mut all = Vec::new();
    for (height, tx) in transactions {
        all.extend(scan_transaction_for_compliance(
            &tx, height, uck, date, asset_id,
        )?);
    }
    Ok(all)
}

/// Scan a transaction using pre-derived DailyKeySet.
/// Preferred for production - auditor receives daily keys from issuer.
///
/// Note: `asset_id` must be known (from issuer detection or context).
pub fn scan_transaction_for_compliance_with_daily_keys(
    tx: &ProtoTransaction,
    height: u64,
    daily_keys: &DailyKeySet,
    asset_id: asset::Id,
) -> Result<Vec<DetectedTransfer>> {
    scan_transaction_core(tx, height, asset_id, |ct| {
        // Decrypt using pre-derived daily keys
        let core = decrypt_core(&daily_keys.core, ct)?
            .ok_or_else(|| anyhow::anyhow!("core decryption produced None"))?;
        let extension = decrypt_extension(&daily_keys.extension, ct)?
            .ok_or_else(|| anyhow::anyhow!("extension decryption produced None"))?;
        Ok(DecryptedUserData { core, extension })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::encrypt_compliance_details;
    use crate::issuer_keys::DetectionKey;
    use crate::test_helpers::{make_address, make_test_leaf, make_uck, make_wallet};
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_proto::core::component::shielded_pool::v1::{Output, OutputBody};
    use penumbra_sdk_proto::core::transaction::v1::{
        action::Action, Action as ActionProto, TransactionBody,
    };
    use rand_core::OsRng;

    #[test]
    fn test_extract_empty() {
        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(extract_compliance_ciphertexts(&tx).unwrap().len(), 0);
    }

    #[test]
    fn test_scan_with_known_asset() {
        let uck = make_uck();
        let (ack, self_address) = make_wallet(&uck, 1);
        let counterparty_address = make_address(2);

        let date = 19000u64;
        let asset_id = asset::Id(decaf377::Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let dk = DetectionKey::demo();
        let asset_leaf = make_test_leaf(dk.public_key(), u128::MAX);

        let mut rng = OsRng;
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
        .unwrap();

        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![ActionProto {
                    action: Some(Action::Output(Output {
                        body: Some(OutputBody {
                            compliance_ciphertext: result.ciphertext.to_bytes(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        // Scan with known asset_id
        let detected = scan_transaction_for_compliance(&tx, 100, &uck, date, asset_id).unwrap();

        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].asset_id, asset_id);
        assert_eq!(detected[0].amount, amount);
        assert_eq!(detected[0].height, 100);
    }
}
