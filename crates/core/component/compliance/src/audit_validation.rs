use crate::ComplianceEvidenceObject;

pub struct AuditValidationInput {
    pub evidence: ComplianceEvidenceObject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditValidationStatus {
    Valid,
    InvalidEvidence(String),
}

/// Validate the canonical evidence object committed by an accepted transfer.
///
/// PRE packages are intentionally absent: the Orbis prototype exposes the seed-opening DH
/// point and is not a valid evidence or authorization boundary.
pub fn validate_audit_evidence(input: AuditValidationInput) -> AuditValidationStatus {
    match input.evidence.validate_payload_hash() {
        Ok(()) => AuditValidationStatus::Valid,
        Err(error) => AuditValidationStatus::InvalidEvidence(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_metadata_only_evidence_is_accepted() {
        let (evidence, _) = crate::evidence::tests::valid_evidence_fixture();
        assert_eq!(
            validate_audit_evidence(AuditValidationInput { evidence }),
            AuditValidationStatus::Valid
        );
    }

    #[test]
    fn payload_tampering_is_rejected() {
        let (mut evidence, _) = crate::evidence::tests::valid_evidence_fixture();
        evidence.payload_hash[0] ^= 1;
        assert!(matches!(
            validate_audit_evidence(AuditValidationInput { evidence }),
            AuditValidationStatus::InvalidEvidence(_)
        ));
    }
}
