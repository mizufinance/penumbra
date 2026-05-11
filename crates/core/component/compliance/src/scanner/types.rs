use penumbra_sdk_asset::asset;
use penumbra_sdk_txhash::TransactionId;

use crate::transfer::TransferComplianceCiphertext;

pub const FLOW_TYPE_PRIVATE_TRANSFER: &str = "private_transfer";
pub const FLOW_TYPE_SHIELD: &str = "shield";
pub const FLOW_TYPE_WITHDRAW: &str = "withdraw";
pub const DECRYPTED_VIA_ISSUER_DK: &str = "issuer_dk";
pub const DECRYPTED_VIA_ORBIS_PRE: &str = "orbis_pre";
pub const DECRYPTED_VIA_PUBLIC: &str = "public";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockRef {
    pub height: u64,
    pub block_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub block_time_unix: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxRef {
    pub block: BlockRef,
    pub tx_index: u32,
    pub tx_hash: TransactionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRef {
    pub tx: TxRef,
    pub action_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputRef {
    pub action: ActionRef,
    pub output_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedComplianceCiphertext {
    pub output_ref: OutputRef,
    pub raw_bytes: Vec<u8>,
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
