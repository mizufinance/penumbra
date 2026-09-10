use shieldd_sdk_asset::asset;

pub use crate::audit_status::{
    AUDIT_STATUS_AUDIT_COMPLETE, AUDIT_STATUS_DECRYPT_FAILED, AUDIT_STATUS_EVIDENCE_INVALID,
    AUDIT_STATUS_EVIDENCE_VALID, AUDIT_STATUS_PENDING, DECRYPTED_VIA_ISSUER_DK,
    DECRYPTED_VIA_ORBIS_PRE, DECRYPTED_VIA_PUBLIC, FLOW_TYPE_PRIVATE_TRANSFER, FLOW_TYPE_SHIELD,
    FLOW_TYPE_WITHDRAW, SCREEN_STATUS_DETECTED, SCREEN_STATUS_INVALID, SCREEN_STATUS_IRRELEVANT,
    SCREEN_STATUS_PENDING,
};
pub use crate::{ActionRef, BlockRef, ComplianceRecordRef, OutputRef, TxRef};
use crate::{TransferComplianceCiphertext, WithdrawalComplianceCiphertext};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplianceCiphertextKind {
    Transfer,
    Withdrawal,
}

#[derive(Clone, Debug)]
pub enum ComplianceCiphertext {
    Transfer(TransferComplianceCiphertext),
    Withdrawal(WithdrawalComplianceCiphertext),
}

impl ComplianceCiphertext {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Transfer(ciphertext) => ciphertext.to_bytes(),
            Self::Withdrawal(ciphertext) => ciphertext.to_bytes().to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PublicWithdrawalData {
    pub asset_id: asset::Id,
    pub amount: shieldd_sdk_num::Amount,
    pub self_address: Option<String>,
    pub destination: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedComplianceCiphertext {
    pub record_ref: ComplianceRecordRef,
    pub kind: ComplianceCiphertextKind,
    pub routing_tags: [u32; 2],
    pub raw_bytes: Vec<u8>,
    pub metadata_bytes: Option<Vec<u8>>,
    pub public_withdrawal: Option<PublicWithdrawalData>,
}

#[derive(Clone, Debug)]
pub struct DetectionEvent {
    pub record_ref: ComplianceRecordRef,
    pub asset_id: asset::Id,
    pub is_flagged: bool,
    pub salt: decaf377::Fq,
    pub routing_tags: [u32; 2],
    pub ciphertext: ComplianceCiphertext,
    pub raw_bytes: Vec<u8>,
    pub public_withdrawal: Option<PublicWithdrawalData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidCiphertext {
    pub record_ref: ComplianceRecordRef,
    pub reason: String,
    pub raw_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditRowKey {
    pub height: u64,
    #[serde(rename = "tx_hash")]
    pub tx_hash_hex: String,
    pub action_index: u32,
    pub output_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditLedgerRow {
    pub height: u64,
    #[serde(rename = "block_hash")]
    pub block_hash_hex: String,
    pub tx_index: u32,
    #[serde(rename = "tx_hash")]
    pub tx_hash_hex: String,
    pub action_index: u32,
    pub output_index: u32,
    pub flow_type: String,
    pub asset_id: String,
    pub is_flagged: bool,
    pub amount: Option<String>,
    pub self_address: Option<String>,
    pub self_alias: Option<String>,
    pub counterparty_address: Option<String>,
    pub counterparty_alias: Option<String>,
    pub public_address: Option<String>,
    pub decrypted_via: Option<String>,
    pub audited_subjects: Vec<String>,
}

/// A block's classified results, committed together with their evidence outcomes.
#[derive(Clone, Debug)]
pub struct ScannedBlock {
    pub block: BlockRef,
    pub outputs: Vec<ScannedOutput>,
}

impl ScannedBlock {
    pub fn new(block: BlockRef) -> Self {
        Self {
            block,
            outputs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScannedOutput {
    pub ciphertext: ExtractedComplianceCiphertext,
    pub outcome: OutputOutcome,
}

#[derive(Clone, Debug)]
pub enum OutputOutcome {
    Irrelevant,
    Invalid {
        reason: String,
    },
    Detected {
        event: DetectionEvent,
        evidence: CandidateEvidence,
    },
}

#[derive(Clone, Debug)]
pub enum CandidateEvidence {
    Ready(crate::ComplianceEvidenceObject),
    BuildFailure { reason: String },
}

impl CandidateEvidence {
    pub fn from_detection(event: &DetectionEvent, metadata: Option<&[u8]>) -> Self {
        let build = || -> anyhow::Result<crate::ComplianceEvidenceObject> {
            match &event.ciphertext {
                ComplianceCiphertext::Transfer(ciphertext) => {
                    let bytes = metadata.ok_or_else(|| {
                        anyhow::anyhow!("detected output is missing transfer compliance metadata")
                    })?;
                    let metadata = crate::TransferComplianceMetadata::from_bytes(bytes)?;
                    crate::ComplianceEvidenceObject::new_transfer(
                        event.record_ref.output_ref(),
                        event.asset_id,
                        event.is_flagged,
                        event.salt,
                        ciphertext.clone(),
                        metadata,
                    )
                }
                ComplianceCiphertext::Withdrawal(ciphertext) => {
                    let public = event.public_withdrawal.as_ref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "withdrawal compliance record is missing public withdrawal data"
                        )
                    })?;
                    crate::ComplianceEvidenceObject::new_withdrawal(
                        event.record_ref.clone(),
                        event.asset_id,
                        event.is_flagged,
                        ciphertext.clone(),
                        crate::WithdrawalEvidencePublicData {
                            amount: public.amount,
                            self_address: public.self_address.clone(),
                            destination: public.destination.clone(),
                        },
                    )
                }
            }
        };
        match build() {
            Ok(evidence) => Self::Ready(evidence),
            Err(error) => Self::BuildFailure {
                reason: crate::audit::bounded_failure_reason(&error.to_string()),
            },
        }
    }
}
