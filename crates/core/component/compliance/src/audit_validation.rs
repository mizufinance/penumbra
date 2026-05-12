use anyhow::{bail, ensure, Context, Result};
use decaf377::Element;
use sha2::{Digest, Sha256};

use crate::{
    decode_object::{PublicTransferTierDecodeObject, TransferTierKind},
    ComplianceEvidenceObject, EvidenceObjectType, TransferOrbisUploadBundle,
};

pub struct AuditValidationInput {
    pub evidence: ComplianceEvidenceObject,
    pub upload_bundle: Option<TransferOrbisUploadBundle>,
    pub ring_pk: Element,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditValidationStatus {
    Valid,
    MissingUploadBundle,
    InvalidEvidence(String),
    InvalidOrbisPackage(String),
}

pub fn validate_audit_evidence(input: AuditValidationInput) -> AuditValidationStatus {
    if let Err(error) = validate_evidence_shape(&input.evidence, &input.ring_pk) {
        return AuditValidationStatus::InvalidEvidence(error.to_string());
    }

    match (
        &input.evidence.orbis_upload_bundle_hash,
        &input.upload_bundle,
    ) {
        (Some(_), None) => AuditValidationStatus::MissingUploadBundle,
        (_, Some(bundle)) => match validate_upload_bundle(&input.evidence, bundle) {
            Ok(()) => AuditValidationStatus::Valid,
            Err(error) => AuditValidationStatus::InvalidOrbisPackage(error.to_string()),
        },
        (None, None) => AuditValidationStatus::Valid,
    }
}

fn validate_evidence_shape(evidence: &ComplianceEvidenceObject, ring_pk: &Element) -> Result<()> {
    evidence.validate_payload_hash()?;
    ensure!(
        evidence.object_type == EvidenceObjectType::Transfer,
        "unsupported evidence object type"
    );
    ensure!(
        crate::transfer::TransferComplianceCiphertext::from_bytes(
            &evidence.transfer_ciphertext.to_bytes()
        )
        .is_ok(),
        "transfer ciphertext does not round-trip"
    );
    ensure!(
        crate::transfer::TransferComplianceDleqProofs::from_bytes(
            &evidence.transfer_dleq_bundle.to_bytes()
        )
        .is_ok(),
        "transfer DLEQ bundle does not round-trip"
    );
    ensure!(
        evidence.tier_objects.len() == 4,
        "transfer evidence must contain exactly four tier objects"
    );

    let expected = [
        TransferTierKind::SenderCore,
        TransferTierKind::SenderExt,
        TransferTierKind::OutputCore,
        TransferTierKind::OutputExt,
    ];
    for (tier_object, expected_kind) in evidence.tier_objects.iter().zip(expected) {
        ensure!(
            tier_object.statement.tier == expected_kind,
            "tier objects must be in canonical order"
        );
        validate_tier_matches_transfer(evidence, tier_object, expected_kind).with_context(
            || {
                format!(
                    "tier {} does not match transfer evidence",
                    expected_kind.label()
                )
            },
        )?;
        tier_object
            .validate(ring_pk)
            .with_context(|| format!("tier {} DLEQ validation failed", expected_kind.label()))?;
    }

    Ok(())
}

fn validate_tier_matches_transfer(
    evidence: &ComplianceEvidenceObject,
    tier_object: &PublicTransferTierDecodeObject,
    tier: TransferTierKind,
) -> Result<()> {
    let ciphertext = &evidence.transfer_ciphertext;
    let proofs = &evidence.transfer_dleq_bundle;
    let (epk, c2, proof) = match tier {
        TransferTierKind::SenderCore => (
            ciphertext.sender_core_epk.vartime_compress().0,
            ciphertext.sender_core_c2.to_bytes(),
            &proofs.sender_core,
        ),
        TransferTierKind::SenderExt => (
            ciphertext.sender_ext_epk.vartime_compress().0,
            ciphertext.sender_ext_c2.to_bytes(),
            &proofs.sender_ext,
        ),
        TransferTierKind::OutputCore => (
            ciphertext.output_core_epk.vartime_compress().0,
            ciphertext.output_core_c2.to_bytes(),
            &proofs.output_core,
        ),
        TransferTierKind::OutputExt => (
            ciphertext.output_ext_epk.vartime_compress().0,
            ciphertext.output_ext_c2.to_bytes(),
            &proofs.output_ext,
        ),
    };
    ensure!(tier_object.epk_bytes == epk, "tier EPK mismatch");
    ensure!(tier_object.c2_bytes == c2, "tier c2 mismatch");
    ensure!(
        tier_object.dleq_challenge_bytes == proof.c.to_bytes(),
        "tier DLEQ challenge mismatch"
    );
    ensure!(
        tier_object.dleq_response_bytes == proof.s.to_bytes(),
        "tier DLEQ response mismatch"
    );
    Ok(())
}

fn validate_upload_bundle(
    evidence: &ComplianceEvidenceObject,
    bundle: &TransferOrbisUploadBundle,
) -> Result<()> {
    bundle.validate()?;
    if let Some(expected_hash) = evidence.orbis_upload_bundle_hash {
        let actual_hash: [u8; 32] = Sha256::digest(bundle.to_bytes()?).into();
        ensure!(
            expected_hash == actual_hash,
            "Orbis upload bundle hash mismatch"
        );
    }

    let objects = &evidence.tier_objects;
    ensure!(
        bundle.sender_core.statement == objects[0].statement,
        "sender_core Orbis statement mismatch"
    );
    ensure!(
        bundle.sender_ext.statement == objects[1].statement,
        "sender_ext Orbis statement mismatch"
    );
    ensure!(
        bundle.output_core.statement == objects[2].statement,
        "output_core Orbis statement mismatch"
    );
    ensure!(
        bundle.output_ext.statement == objects[3].statement,
        "output_ext Orbis statement mismatch"
    );

    if objects.is_empty() {
        bail!("missing tier objects");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> AuditValidationInput {
        let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        AuditValidationInput {
            evidence,
            upload_bundle: Some(bundle),
            ring_pk,
        }
    }

    #[test]
    fn validates_complete_evidence_and_upload_bundle() {
        assert_eq!(
            validate_audit_evidence(valid_input()),
            AuditValidationStatus::Valid
        );
    }

    #[test]
    fn missing_upload_bundle_is_reported_without_panic() {
        let mut input = valid_input();
        input.upload_bundle = None;
        assert_eq!(
            validate_audit_evidence(input),
            AuditValidationStatus::MissingUploadBundle
        );
    }

    #[test]
    fn tampered_tx_hash_is_invalid_evidence() {
        let mut input = valid_input();
        input.evidence.output_ref.action.tx.tx_hash = penumbra_sdk_txhash::TransactionId([9u8; 32]);
        assert!(matches!(
            validate_audit_evidence(input),
            AuditValidationStatus::InvalidEvidence(_)
        ));
    }

    #[test]
    fn tampered_action_index_is_invalid_evidence() {
        let mut input = valid_input();
        input.evidence.output_ref.action.action_index += 1;
        assert!(matches!(
            validate_audit_evidence(input),
            AuditValidationStatus::InvalidEvidence(_)
        ));
    }

    #[test]
    fn tampered_salt_is_invalid_evidence() {
        let mut input = valid_input();
        input.evidence.detection_salt += decaf377::Fq::from(1u64);
        assert!(matches!(
            validate_audit_evidence(input),
            AuditValidationStatus::InvalidEvidence(_)
        ));
    }

    #[test]
    fn tampered_tier_label_is_invalid_evidence() {
        let mut input = valid_input();
        input.evidence.tier_objects[0].statement.tier = TransferTierKind::OutputCore;
        input.evidence.payload_hash = ComplianceEvidenceObject::new_transfer(
            input.evidence.output_ref.clone(),
            input.evidence.asset_id,
            input.evidence.is_flagged,
            input.evidence.detection_salt,
            input.evidence.transfer_ciphertext.clone(),
            input.evidence.transfer_dleq_bundle.clone(),
            input.evidence.tier_objects.clone(),
            input.upload_bundle.as_ref(),
        )
        .unwrap()
        .payload_hash;
        assert!(matches!(
            validate_audit_evidence(input),
            AuditValidationStatus::InvalidEvidence(_)
        ));
    }

    #[test]
    fn tampered_dleq_challenge_is_invalid_evidence() {
        let mut input = valid_input();
        input.evidence.tier_objects[0].dleq_challenge_bytes[0] ^= 1;
        assert!(matches!(
            validate_audit_evidence(input),
            AuditValidationStatus::InvalidEvidence(_)
        ));
    }

    #[test]
    fn tampered_orbis_package_metadata_is_invalid_package() {
        let mut input = valid_input();
        input
            .upload_bundle
            .as_mut()
            .unwrap()
            .sender_core
            .policy_id
            .push_str("-tampered");
        assert!(matches!(
            validate_audit_evidence(input),
            AuditValidationStatus::InvalidOrbisPackage(_)
        ));
    }
}
