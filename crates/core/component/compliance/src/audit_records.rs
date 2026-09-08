//! Typed audit/export records and pure audit row classification.
//!

use serde::{Deserialize, Serialize};

use crate::audit_status::{AuditStatus, FlowType};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditDetectedRef {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: u32,
    #[serde(default)]
    pub output_index: u32,
    pub asset_id: String,
    pub is_flagged: bool,
    #[serde(default)]
    pub routing_tags: Option<[u32; 2]>,
    #[serde(default = "private_transfer_flow_type")]
    pub flow_type: FlowType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditScanExport {
    pub scan_info: serde_json::Value,
    pub detected: Vec<AuditDetectedRef>,
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
    pub routing_tags: Option<[u32; 2]>,
    pub flow_type: FlowType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditRoutingSelector {
    precision: u8,
    prefix: u32,
}

impl AuditRoutingSelector {
    pub fn new(precision: u8, prefix: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(precision <= 32, "routing precision must be at most 32 bits");
        let selector = Self { precision, prefix };
        anyhow::ensure!(
            prefix & !selector.mask() == 0,
            "routing selector has non-zero unused bits"
        );
        Ok(selector)
    }

    fn mask(self) -> u32 {
        match self.precision {
            0 => 0,
            32 => u32::MAX,
            bits => (1u32 << bits) - 1,
        }
    }

    fn matches(self, tag: u32) -> bool {
        tag & self.mask() == self.prefix
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditSubjectRegistration {
    pub asset_id: String,
    pub selector: AuditRoutingSelector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditSubjectRole {
    Sender,
    Receiver,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditSubjectCandidate {
    pub detected: AuditDetectedRef,
    pub roles: Vec<AuditSubjectRole>,
}

/// Selects subject work locally, applying the decrypted asset before routing prefixes.
pub fn filter_subject_candidates(
    detections: &[AuditDetectedRef],
    registrations: &[AuditSubjectRegistration],
) -> Vec<AuditSubjectCandidate> {
    detections
        .iter()
        .filter_map(|detected| {
            let tags = detected.routing_tags?;
            let matched = registrations
                .iter()
                .filter(|registration| registration.asset_id == detected.asset_id)
                .any(|registration| {
                    tags.into_iter()
                        .any(|tag| registration.selector.matches(tag))
                });

            matched.then(|| AuditSubjectCandidate {
                detected: detected.clone(),
                roles: vec![AuditSubjectRole::Sender, AuditSubjectRole::Receiver],
            })
        })
        .collect()
}

pub fn detected_ref_from_row_parts(row: DetectedRefRowParts) -> AuditDetectedRef {
    AuditDetectedRef {
        height: row.height,
        tx_hash: hex::encode(row.tx_hash),
        action_index: row.action_index,
        output_index: row.output_index,
        asset_id: row.asset_id,
        is_flagged: row.is_flagged,
        routing_tags: row.routing_tags,
        flow_type: row.flow_type,
    }
}

fn private_transfer_flow_type() -> FlowType {
    FlowType::PrivateTransfer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_ref_projection_preserves_fields_and_hex_encodes_tx_hash() {
        let detected = detected_ref_from_row_parts(DetectedRefRowParts {
            height: 42,
            tx_hash: vec![0xab, 0xcd, 0x01],
            action_index: 7,
            output_index: 3,
            asset_id: "asset".to_owned(),
            is_flagged: true,
            routing_tags: Some([11, 22]),
            flow_type: FlowType::PrivateTransfer,
        });

        assert_eq!(detected.height, 42);
        assert_eq!(detected.tx_hash, "abcd01");
        assert_eq!(detected.action_index, 7);
        assert_eq!(detected.output_index, 3);
        assert_eq!(detected.asset_id, "asset");
        assert!(detected.is_flagged);
        assert_eq!(detected.routing_tags, Some([11, 22]));
        assert_eq!(detected.flow_type, FlowType::PrivateTransfer);
    }

    fn selector(prefix: u32) -> AuditRoutingSelector {
        AuditRoutingSelector::new(12, prefix).unwrap()
    }

    fn detected(asset_id: &str, tags: [u32; 2]) -> AuditDetectedRef {
        AuditDetectedRef {
            height: 1,
            tx_hash: "01".to_owned(),
            action_index: 0,
            output_index: 0,
            asset_id: asset_id.to_owned(),
            is_flagged: false,
            routing_tags: Some(tags),
            flow_type: FlowType::PrivateTransfer,
        }
    }

    fn registration(asset_id: &str, prefix: u32) -> AuditSubjectRegistration {
        AuditSubjectRegistration {
            asset_id: asset_id.to_owned(),
            selector: selector(prefix),
        }
    }

    #[test]
    fn subject_filter_marks_both_trial_roles_for_any_tag_hit() {
        let alice = registration("asset-a", 0x123);
        for (label, detection) in [
            ("first_tag", detected("asset-a", [0xa123, 0xb456])),
            ("second_tag", detected("asset-a", [0xe888, 0xf123])),
        ] {
            let candidates = filter_subject_candidates(&[detection], std::slice::from_ref(&alice));
            assert_eq!(candidates.len(), 1, "{label}");
            assert_eq!(
                candidates[0].roles,
                [AuditSubjectRole::Sender, AuditSubjectRole::Receiver],
                "{label}"
            );
        }
    }

    #[test]
    fn subject_filter_rejects_wrong_asset_or_prefix() {
        let alice = registration("asset-a", 0x123);
        let wrong_prefix = filter_subject_candidates(
            &[detected("asset-a", [0xa456, 0xb789])],
            std::slice::from_ref(&alice),
        );
        assert!(wrong_prefix.is_empty());
        let wrong_asset = filter_subject_candidates(
            &[detected("asset-b", [0xa123, 0xb456])],
            std::slice::from_ref(&alice),
        );
        assert!(wrong_asset.is_empty());
    }

    #[test]
    fn subject_filter_applies_asset_before_each_registered_address_selector() {
        let registrations = [
            registration("asset-a", 0x123),
            registration("asset-b", 0x456),
        ];
        let detections = [
            detected("asset-a", [0xa123, 0xb999]),
            detected("asset-b", [0xc456, 0xd999]),
            detected("asset-a", [0xe456, 0xf999]),
        ];
        let candidates = filter_subject_candidates(&detections, &registrations);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].detected.asset_id, "asset-a");
        assert_eq!(candidates[1].detected.asset_id, "asset-b");
    }
}
