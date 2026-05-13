//! Typed audit/export records and pure audit row classification.
//!
//! Audit/export is the selected MVP boundary for D-01/D-03 because `audit.rs`
//! mixes SQLite effects, export DTO construction, Orbis import eligibility,
//! and failure recording. Registry/state is rejected here because it already
//! uses the component state traits, while its highest-risk storage and security
//! items are deferred.

use serde::{Deserialize, Serialize};

use crate::scanner::types::{
    AUDIT_STATUS_AUDIT_COMPLETE, AUDIT_STATUS_DECRYPT_FAILED, AUDIT_STATUS_EVIDENCE_VALID,
    FLOW_TYPE_PRIVATE_TRANSFER,
};

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
    pub flow_type: String,
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
    pub decrypted_via: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditImportRow {
    pub audit_status: String,
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
    pub flow_type: String,
}

pub fn classify_orbis_import_row(row: Option<AuditImportRow>) -> OrbisImportEligibility {
    match row {
        Some(row)
            if !row.is_flagged
                && (row.audit_status == AUDIT_STATUS_EVIDENCE_VALID
                    || row.audit_status == AUDIT_STATUS_DECRYPT_FAILED
                    || row.audit_status == AUDIT_STATUS_AUDIT_COMPLETE) =>
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

fn private_transfer_flow_type() -> String {
    FLOW_TYPE_PRIVATE_TRANSFER.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::types::{AUDIT_STATUS_EVIDENCE_INVALID, AUDIT_STATUS_PENDING};

    fn row(audit_status: &str, is_flagged: bool) -> AuditImportRow {
        AuditImportRow {
            audit_status: audit_status.to_owned(),
            is_flagged,
        }
    }

    #[test]
    fn unflagged_valid_orbis_statuses_are_eligible() {
        for status in [
            AUDIT_STATUS_EVIDENCE_VALID,
            AUDIT_STATUS_DECRYPT_FAILED,
            AUDIT_STATUS_AUDIT_COMPLETE,
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
            (AUDIT_STATUS_EVIDENCE_VALID, true),
            (AUDIT_STATUS_PENDING, false),
            (AUDIT_STATUS_EVIDENCE_INVALID, false),
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
            flow_type: "private_transfer".to_owned(),
        });

        assert_eq!(detected.height, 42);
        assert_eq!(detected.tx_hash, "abcd01");
        assert_eq!(detected.action_index, 7);
        assert_eq!(detected.output_index, 3);
        assert_eq!(detected.asset_id, "asset");
        assert!(detected.is_flagged);
        assert_eq!(detected.flow_type, "private_transfer");
    }
}
