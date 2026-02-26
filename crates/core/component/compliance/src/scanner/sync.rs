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
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::core::component::sct::v1::Nullifier;
use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;
use serde::{Deserialize, Serialize};

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

/// Extracted ciphertext data from a transaction action.
#[derive(Clone, Debug)]
pub struct ExtractedCiphertext {
    /// The action index within the transaction.
    pub action_index: usize,
    /// The compliance ciphertext bytes.
    pub compliance_ciphertext: Vec<u8>,
    /// Whether this is from a spend (true) or output (false) action.
    pub is_spend: bool,
    /// The sender-encrypted ciphertext (96 bytes, only present on outputs).
    pub sender_ciphertext: Vec<u8>,
}

/// Extract compliance ciphertexts from a transaction.
pub fn extract_ciphertexts(tx: &ProtoTransaction) -> Result<Vec<(usize, Vec<u8>, bool)>> {
    Ok(extract_ciphertexts_full(tx)?
        .into_iter()
        .map(|e| (e.action_index, e.compliance_ciphertext, e.is_spend))
        .collect())
}

/// Extract compliance ciphertexts from a transaction, including sender_ciphertext.
pub fn extract_ciphertexts_full(tx: &ProtoTransaction) -> Result<Vec<ExtractedCiphertext>> {
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
                        results.push(ExtractedCiphertext {
                            action_index: idx,
                            compliance_ciphertext: body.compliance_ciphertext.clone(),
                            is_spend: false,
                            sender_ciphertext: body.sender_ciphertext.clone(),
                        });
                    }
                }
            }
            Some(Action::Spend(spend)) => {
                if let Some(body) = &spend.body {
                    if !body.compliance_ciphertext.is_empty() {
                        results.push(ExtractedCiphertext {
                            action_index: idx,
                            compliance_ciphertext: body.compliance_ciphertext.clone(),
                            is_spend: true,
                            sender_ciphertext: vec![],
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use penumbra_sdk_proto::core::transaction::v1::TransactionBody;

    #[test]
    fn test_extract_empty() {
        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(extract_ciphertexts(&tx).unwrap().len(), 0);
    }
}
