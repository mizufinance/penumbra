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
}

/// Extract compliance ciphertexts from a transaction.
pub fn extract_ciphertexts(tx: &ProtoTransaction) -> Result<Vec<(usize, Vec<u8>)>> {
    Ok(extract_ciphertexts_full(tx)?
        .into_iter()
        .map(|e| (e.action_index, e.compliance_ciphertext))
        .collect())
}

/// Extract transfer compliance ciphertexts from a transaction.
pub fn extract_ciphertexts_full(tx: &ProtoTransaction) -> Result<Vec<ExtractedCiphertext>> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action;

    let actions = match &tx.body {
        Some(body) => &body.actions,
        None => return Ok(vec![]),
    };

    let mut results = Vec::new();
    for (idx, action) in actions.iter().enumerate() {
        match &action.action {
            Some(Action::Transfer(transfer)) => {
                if let Some(body) = &transfer.body {
                    for output in &body.outputs {
                        if !output.compliance_ciphertext.is_empty() {
                            results.push(ExtractedCiphertext {
                                action_index: idx,
                                compliance_ciphertext: output.compliance_ciphertext.clone(),
                            });
                        }
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
    use penumbra_sdk_proto::core::component::shielded_pool::v1::{
        Consolidate, ConsolidateBody, Split, SplitBody, Transfer, TransferBody, TransferOutputBody,
    };
    use penumbra_sdk_proto::core::transaction::v1::TransactionBody;
    use penumbra_sdk_proto::core::transaction::v1::{action::Action, Action as ActionProto};

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

    #[test]
    fn test_extract_ciphertexts_full_only_reads_transfer_outputs() {
        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![
                    ActionProto {
                        action: Some(Action::Split(Split {
                            body: Some(SplitBody::default()),
                            ..Default::default()
                        })),
                    },
                    ActionProto {
                        action: Some(Action::Transfer(Transfer {
                            body: Some(TransferBody {
                                outputs: vec![
                                    TransferOutputBody::default(),
                                    TransferOutputBody {
                                        compliance_ciphertext: vec![1, 2, 3, 4],
                                        ..Default::default()
                                    },
                                ],
                                ..Default::default()
                            }),
                            ..Default::default()
                        })),
                    },
                    ActionProto {
                        action: Some(Action::Consolidate(Consolidate {
                            body: Some(ConsolidateBody::default()),
                            ..Default::default()
                        })),
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };

        let extracted = extract_ciphertexts_full(&tx).expect("transfer extraction should succeed");
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].action_index, 1);
        assert_eq!(extracted[0].compliance_ciphertext, vec![1, 2, 3, 4]);
    }
}
