use anyhow::{ensure, Context, Result};
use decaf377::{Element, Encoding, Fr};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::{
    issuer_keys::decrypt_detection, DetectionKey, DleqProof, IssuerDhEvidence,
    TransferComplianceCiphertext,
};

use crate::{
    accepted_audit_ciphertext, decode_audit_ciphertext, AcceptedAuditCiphertext, AcceptedBlock,
    AuditAccess, AuditSelection, DecodedAuditValue,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerEvidence {
    pub shared_point: [u8; 32],
    pub commitment_g: [u8; 32],
    pub commitment_h: [u8; 32],
    pub response: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuerDisclosureKind {
    Issuer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerDisclosure {
    pub kind: IssuerDisclosureKind,
    pub version: u32,
    pub request: IssuerRequest,
    pub detection: IssuerEvidence,
    pub selected: IssuerEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerRequest {
    pub kind: IssuerDisclosureKind,
    pub version: u32,
    pub recipient: Option<String>,
    pub challenge: Option<String>,
    pub selection: AuditSelection,
    pub asset: String,
}

pub fn validate_issuer_request(request: &IssuerRequest) -> Result<()> {
    ensure!(
        request.version == 1 && request.selection.version == 2,
        "unsupported issuer request version"
    );
    ensure!(
        matches!(request.selection.access, AuditAccess::General { .. }),
        "issuer request requires a selected value"
    );
    ensure!(
        !request.selection.chain_id.is_empty()
            && request.selection.chain_id.len() <= 256
            && request.selection.reference.height > 0,
        "invalid issuer transaction reference"
    );
    let asset: asset::Id = request.asset.parse()?;
    ensure!(
        asset.to_string() == request.asset,
        "noncanonical issuer asset"
    );
    for text in [&request.recipient, &request.challenge]
        .into_iter()
        .flatten()
    {
        ensure!(
            !text.is_empty() && text.len() <= 1024,
            "invalid issuer audience or challenge"
        );
    }
    Ok(())
}

fn request_digest(request: &IssuerRequest) -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    validate_issuer_request(request)?;
    let mut hash = Sha256::new();
    hash.update(b"shieldd.disclosure.issuer.request.v1\0");
    hash.update(serde_json::to_vec(request)?);
    Ok(hash.finalize().into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedIssuerDisclosure {
    pub request: IssuerRequest,
    pub selection: AuditSelection,
    pub asset: String,
    pub value: DecodedAuditValue,
    pub flagged: bool,
    /// Detection evidence also permits sender CORE amount decryption on flagged Transfers.
    pub grants_amount_and_detection_access: bool,
}

fn point(bytes: [u8; 32]) -> Result<Element> {
    Encoding(bytes)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid issuer evidence point"))
}

impl From<IssuerDhEvidence> for IssuerEvidence {
    fn from(value: IssuerDhEvidence) -> Self {
        Self {
            shared_point: value.shared_point.vartime_compress().0,
            commitment_g: value.proof.commitment_g.vartime_compress().0,
            commitment_h: value.proof.commitment_h.vartime_compress().0,
            response: value.proof.response.to_bytes(),
        }
    }
}

impl IssuerEvidence {
    fn verify(
        &self,
        asset: asset::Id,
        key: Element,
        epk: Element,
        request: &[u8; 32],
    ) -> Result<Element> {
        IssuerDhEvidence {
            version: 2,
            asset_id: asset.0.to_bytes(),
            ciphertext_epk: epk,
            issuer_dk_pub: key,
            shared_point: point(self.shared_point)?,
            proof: DleqProof {
                commitment_g: point(self.commitment_g)?,
                commitment_h: point(self.commitment_h)?,
                response: Fr::from_bytes_checked(&self.response)
                    .map_err(|_| anyhow::anyhow!("noncanonical issuer evidence scalar"))?,
            },
        }
        .verify_bound_for(asset.0.to_bytes(), key, epk, request)
    }
}

/// Creates selected-value evidence locally; the issuer key never enters the package.
pub fn prepare_issuer_disclosure(
    mut rng: impl RngCore + CryptoRng,
    accepted: &AcceptedAuditCiphertext,
    request: IssuerRequest,
    key: &DetectionKey,
) -> Result<IssuerDisclosure> {
    let binding = request_digest(&request)?;
    ensure!(
        serde_json::to_vec(&request.selection)? == serde_json::to_vec(&accepted.selection)?,
        "issuer request selection mismatch"
    );
    let asset: asset::Id = request.asset.parse()?;
    ensure!(
        matches!(accepted.selection.access, AuditAccess::General { .. }),
        "issuer disclosure requires an explicit value selection"
    );
    let ct = TransferComplianceCiphertext::from_bytes(&accepted.ciphertext)?;
    let (_, flagged, _) =
        key.try_decrypt_detection(&ct.sender_core_epk, &ct.detection_tag, &asset)?;
    ensure!(
        flagged,
        "issuer selected-field disclosure requires a flagged transaction"
    );
    let package = IssuerDisclosure {
        kind: IssuerDisclosureKind::Issuer,
        version: 1,
        request,
        detection: IssuerDhEvidence::prove_bound(
            &mut rng,
            key,
            asset.0.to_bytes(),
            ct.sender_core_epk,
            &binding,
        )?
        .into(),
        selected: IssuerDhEvidence::prove_bound(
            &mut rng,
            key,
            asset.0.to_bytes(),
            point(accepted.epk)?,
            &binding,
        )?
        .into(),
    };
    verify_fields(&package, accepted, asset, key.public_key())?;
    Ok(package)
}

/// The issuer key must come from independently retrieved registered asset policy.
pub fn verify_issuer_disclosure(
    package: &IssuerDisclosure,
    node_chain: &str,
    block: &AcceptedBlock,
    registered_asset: asset::Id,
    registered_issuer_key: Element,
) -> Result<VerifiedIssuerDisclosure> {
    let accepted = accepted_audit_ciphertext(package.request.selection.clone(), node_chain, block)?;
    verify_fields(package, &accepted, registered_asset, registered_issuer_key)
}

fn verify_fields(
    package: &IssuerDisclosure,
    accepted: &AcceptedAuditCiphertext,
    asset: asset::Id,
    key: Element,
) -> Result<VerifiedIssuerDisclosure> {
    ensure!(
        package.version == 1,
        "unsupported issuer disclosure version"
    );
    ensure!(
        matches!(
            package.request.selection.access,
            AuditAccess::General { .. }
        ),
        "issuer disclosure requires an explicit value selection"
    );
    ensure!(
        package.request.asset == asset.to_string(),
        "issuer asset mismatch"
    );
    let binding = request_digest(&package.request)?;
    let ct = TransferComplianceCiphertext::from_bytes(&accepted.ciphertext)?;
    let detection = package
        .detection
        .verify(asset, key, ct.sender_core_epk, &binding)?;
    let (_, flagged, _) =
        decrypt_detection(&detection, &ct.sender_core_epk, &ct.detection_tag, &asset)
            .context("issuer detection evidence does not match accepted ciphertext")?;
    ensure!(
        flagged,
        "issuer disclosure does not establish flagged status"
    );
    let shared = package
        .selected
        .verify(asset, key, point(accepted.epk)?, &binding)?;
    let value = decode_audit_ciphertext(accepted, shared.vartime_compress().0)?;
    Ok(VerifiedIssuerDisclosure {
        request: package.request.clone(),
        selection: package.request.selection.clone(),
        asset: asset.to_string(),
        value,
        flagged,
        grants_amount_and_detection_access: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionRef, AuditKeyField, AuditKeyIdentity, AuditKeyScope, AuditPolicy, MasterSelection,
        OutputRef,
    };
    use decaf377::Fq;
    use shieldd_sdk_compliance::{
        transfer::encrypt_transfer, AuditKeys, TransferComplianceMetadata,
    };
    use shieldd_sdk_keys::keys::{SpendKey, SpendKeyBytes};

    fn fixture(
        flagged: bool,
        selection: MasterSelection,
    ) -> (AcceptedAuditCiphertext, asset::Id, DetectionKey) {
        let sender = SpendKey::try_from(SpendKeyBytes([7; 32]))
            .unwrap()
            .full_viewing_key()
            .payment_address(0u32.into());
        let receiver = SpendKey::try_from(SpendKeyBytes([8; 32]))
            .unwrap()
            .full_viewing_key()
            .payment_address(0u32.into());
        let keys = |n| AuditKeys {
            epoch: 1,
            amount: Element::GENERATOR * Fr::from(n),
            sender: Element::GENERATOR * Fr::from(n + 1),
            receiver: Element::GENERATOR * Fr::from(n + 2),
        };
        let asset = asset::Id(Fq::from(42u64));
        let key = DetectionKey::new(Fr::from(37u64));
        let metadata = TransferComplianceMetadata::from_identifiers(
            "ring",
            "policy",
            "transaction",
            "read",
            1,
            1,
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(4u64),
        );
        let encrypted = encrypt_transfer(
            rand_core::OsRng,
            &keys(101u64),
            &keys(104),
            &keys(107),
            &key.public_key(),
            &receiver,
            &sender,
            shieldd_sdk_asset::Value {
                amount: 500u64.into(),
                asset_id: asset,
            },
            flagged,
            Fq::from(5u64),
            metadata.sender_core_salt().unwrap(),
            metadata.output_core_salt().unwrap(),
        )
        .unwrap();
        let tier = selection
            .tier()
            .select(&encrypted.ciphertext, &metadata)
            .unwrap();
        let accepted = AcceptedAuditCiphertext {
            selection: AuditSelection {
                version: 2,
                chain_id: "chain".into(),
                reference: OutputRef {
                    transaction_id: "ab".repeat(32),
                    height: 1,
                    action: ActionRef::Body(0),
                    output: 0,
                },
                access: AuditAccess::General { value: selection },
                policy: AuditPolicy {
                    ring_id: "ring".into(),
                    policy_id: "policy".into(),
                    resource: "transaction".into(),
                    permission: "read".into(),
                },
            },
            epk: tier.epk.vartime_compress().0,
            wrapping: encrypted.ciphertext.master_wrappings[selection as usize].to_bytes(),
            ciphertext: encrypted.ciphertext.to_bytes().to_vec(),
            metadata,
            identity: AuditKeyIdentity {
                chain: "chain".into(),
                ring: "ring".into(),
                epoch: 1,
                scope: AuditKeyScope::General,
                field: AuditKeyField::Amount,
            },
            object_id: "fixture".into(),
        };
        (accepted, asset, key)
    }

    fn request(accepted: &AcceptedAuditCiphertext, asset: asset::Id) -> IssuerRequest {
        IssuerRequest {
            kind: IssuerDisclosureKind::Issuer,
            version: 1,
            recipient: Some("auditor".into()),
            challenge: Some("fresh-challenge".into()),
            selection: accepted.selection.clone(),
            asset: asset.to_string(),
        }
    }

    #[test]
    fn selected_issuer_fields_require_detection_and_both_dleq_proofs() {
        for selection in MasterSelection::ALL {
            let (accepted, asset, key) = fixture(true, selection);
            let package = prepare_issuer_disclosure(
                rand_core::OsRng,
                &accepted,
                request(&accepted, asset),
                &key,
            )
            .unwrap();
            let facts = verify_fields(&package, &accepted, asset, key.public_key()).unwrap();
            assert!(facts.flagged && facts.grants_amount_and_detection_access);
            if selection == MasterSelection::Amount {
                assert_eq!(
                    facts.value,
                    DecodedAuditValue::Amount {
                        base_units: "500".into()
                    }
                );
            }
            assert!(verify_fields(&package, &accepted, asset, Element::GENERATOR).is_err());
            assert!(verify_fields(
                &package,
                &accepted,
                asset::Id(Fq::from(43u64)),
                key.public_key()
            )
            .is_err());
            let mut altered = package.clone();
            altered.detection.response = [255; 32];
            assert!(verify_fields(&altered, &accepted, asset, key.public_key()).is_err());
            let mut altered = package.clone();
            altered.selected.shared_point = Element::GENERATOR.vartime_compress().0;
            assert!(verify_fields(&altered, &accepted, asset, key.public_key()).is_err());
            let mut altered = accepted.clone();
            altered.ciphertext[0] ^= 1;
            assert!(verify_fields(&package, &altered, asset, key.public_key()).is_err());
            let mut replayed = package.clone();
            replayed.request.challenge = Some("different-challenge".into());
            assert!(verify_fields(&replayed, &accepted, asset, key.public_key()).is_err());
            replayed = package.clone();
            replayed.request.recipient = Some("another-auditor".into());
            assert!(verify_fields(&replayed, &accepted, asset, key.public_key()).is_err());
            let mut altered = package.clone();
            altered.version = 2;
            assert!(verify_fields(&altered, &accepted, asset, key.public_key()).is_err());
            assert!(verify_issuer_disclosure(
                &package,
                "chain",
                &AcceptedBlock {
                    height: 1,
                    transactions: vec![]
                },
                asset,
                key.public_key()
            )
            .is_err());
            assert!(verify_issuer_disclosure(
                &package,
                "wrong",
                &AcceptedBlock {
                    height: 1,
                    transactions: vec![]
                },
                asset,
                key.public_key()
            )
            .is_err());
        }
        let (accepted, asset, key) = fixture(false, MasterSelection::Amount);
        assert!(prepare_issuer_disclosure(
            rand_core::OsRng,
            &accepted,
            request(&accepted, asset),
            &key
        )
        .is_err());
    }
}
