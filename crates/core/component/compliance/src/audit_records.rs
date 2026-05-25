//! Typed audit/export records and pure audit row classification.
//!

use serde::{Deserialize, Serialize};

use crate::audit_status::{AuditStatus, DecryptedVia, FlowType};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditDetectedRef {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: u32,
    #[serde(default)]
    pub output_index: u32,
    pub asset_id: String,
    pub is_flagged: bool,
    #[serde(default = "private_transfer_flow_type")]
    pub flow_type: FlowType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditScanExport {
    pub scan_info: serde_json::Value,
    pub detected: Vec<AuditDetectedRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrbisAuditEntry {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: u32,
    #[serde(default)]
    pub output_index: u32,
    pub amount: String,
    pub self_address: String,
    pub counterparty: String,
    pub decrypted_via: DecryptedVia,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditImportRow {
    pub audit_status: AuditStatus,
    pub is_flagged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrbisImportEligibility {
    Eligible,
    Ineligible { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedRefRowParts {
    pub height: u64,
    pub tx_hash: Vec<u8>,
    pub action_index: u32,
    pub output_index: u32,
    pub asset_id: String,
    pub is_flagged: bool,
    pub flow_type: FlowType,
}

pub fn classify_orbis_import_row(row: Option<AuditImportRow>) -> OrbisImportEligibility {
    match row {
        Some(row)
            if !row.is_flagged
                && (row.audit_status == AuditStatus::EvidenceValid
                    || row.audit_status == AuditStatus::DecryptFailed
                    || row.audit_status == AuditStatus::AuditComplete) =>
        {
            OrbisImportEligibility::Eligible
        }
        Some(row) => OrbisImportEligibility::Ineligible {
            reason: format!(
                "row is not an evidence-valid unflagged detection: {}",
                row.audit_status
            ),
        },
        None => OrbisImportEligibility::Ineligible {
            reason: "detected row not found".to_owned(),
        },
    }
}

pub fn detected_ref_from_row_parts(row: DetectedRefRowParts) -> AuditDetectedRef {
    AuditDetectedRef {
        height: row.height,
        tx_hash: hex::encode(row.tx_hash),
        action_index: row.action_index,
        output_index: row.output_index,
        asset_id: row.asset_id,
        is_flagged: row.is_flagged,
        flow_type: row.flow_type,
    }
}

fn private_transfer_flow_type() -> FlowType {
    FlowType::PrivateTransfer
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(audit_status: AuditStatus, is_flagged: bool) -> AuditImportRow {
        AuditImportRow {
            audit_status,
            is_flagged,
        }
    }

    #[test]
    fn unflagged_valid_orbis_statuses_are_eligible() {
        for status in [
            AuditStatus::EvidenceValid,
            AuditStatus::DecryptFailed,
            AuditStatus::AuditComplete,
        ] {
            assert_eq!(
                classify_orbis_import_row(Some(row(status, false))),
                OrbisImportEligibility::Eligible
            );
        }
    }

    #[test]
    fn flagged_or_invalid_orbis_rows_are_ineligible_with_status_reason() {
        for (status, is_flagged) in [
            (AuditStatus::EvidenceValid, true),
            (AuditStatus::Pending, false),
            (AuditStatus::EvidenceInvalid, false),
        ] {
            assert_eq!(
                classify_orbis_import_row(Some(row(status, is_flagged))),
                OrbisImportEligibility::Ineligible {
                    reason: format!("row is not an evidence-valid unflagged detection: {status}")
                }
            );
        }
    }

    #[test]
    fn missing_orbis_row_is_ineligible_with_missing_reason() {
        assert_eq!(
            classify_orbis_import_row(None),
            OrbisImportEligibility::Ineligible {
                reason: "detected row not found".to_owned()
            }
        );
    }

    #[test]
    fn detected_ref_projection_preserves_fields_and_hex_encodes_tx_hash() {
        let detected = detected_ref_from_row_parts(DetectedRefRowParts {
            height: 42,
            tx_hash: vec![0xab, 0xcd, 0x01],
            action_index: 7,
            output_index: 3,
            asset_id: "asset".to_owned(),
            is_flagged: true,
            flow_type: FlowType::PrivateTransfer,
        });

        assert_eq!(detected.height, 42);
        assert_eq!(detected.tx_hash, "abcd01");
        assert_eq!(detected.action_index, 7);
        assert_eq!(detected.output_index, 3);
        assert_eq!(detected.asset_id, "asset");
        assert!(detected.is_flagged);
        assert_eq!(detected.flow_type, FlowType::PrivateTransfer);
    }

    #[test]
    fn orbis_audit_entries_accept_orbis_pre_decryption_label() {
        for label in ["orbis_pre"] {
            let entry: OrbisAuditEntry = serde_json::from_value(serde_json::json!({
                "height": 42,
                "tx_hash": "abcd",
                "action_index": 0,
                "output_index": 0,
                "amount": "1234",
                "self_address": "receiver",
                "counterparty": "sender",
                "decrypted_via": label,
            }))
            .expect("orbis-audit output should parse");

            assert_eq!(entry.decrypted_via, DecryptedVia::OrbisPre);
            assert_eq!(entry.decrypted_via.as_str(), label);
        }
    }
}
