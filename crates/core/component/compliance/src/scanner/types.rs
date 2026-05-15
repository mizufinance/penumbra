use penumbra_sdk_asset::asset;

use crate::transfer::TransferComplianceCiphertext;
pub use crate::audit_status::{
    AUDIT_STATUS_AUDIT_COMPLETE, AUDIT_STATUS_DECRYPT_FAILED, AUDIT_STATUS_EVIDENCE_INVALID,
    AUDIT_STATUS_EVIDENCE_VALID, AUDIT_STATUS_PENDING, DECRYPTED_VIA_ISSUER_DK,
    DECRYPTED_VIA_ORBIS_PRE, DECRYPTED_VIA_PUBLIC, DETECTION_STATUS_DETECTED,
    FLOW_TYPE_PRIVATE_TRANSFER, FLOW_TYPE_SHIELD, FLOW_TYPE_WITHDRAW, SCREEN_STATUS_DETECTED,
    SCREEN_STATUS_INVALID, SCREEN_STATUS_IRRELEVANT, SCREEN_STATUS_PENDING,
};
pub use crate::{ActionRef, BlockRef, OutputRef, TxRef};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedComplianceCiphertext {
    pub output_ref: OutputRef,
    pub raw_bytes: Vec<u8>,
    pub upload_bundle_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClearFlowKind {
    Shield,
    Withdraw,
}

impl ClearFlowKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shield => FLOW_TYPE_SHIELD,
            Self::Withdraw => FLOW_TYPE_WITHDRAW,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClearFlowEvent {
    pub output_ref: OutputRef,
    pub kind: ClearFlowKind,
    pub asset_id: asset::Id,
    pub amount: penumbra_sdk_num::Amount,
    pub self_address: Option<String>,
    pub counterparty: Option<String>,
    pub public_address: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DetectionEvent {
    pub output_ref: OutputRef,
    pub asset_id: asset::Id,
    pub is_flagged: bool,
    pub salt: decaf377::Fq,
    pub ciphertext: TransferComplianceCiphertext,
    pub raw_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidCiphertext {
    pub output_ref: OutputRef,
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
