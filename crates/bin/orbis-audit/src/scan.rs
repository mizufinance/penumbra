use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanOutput {
    pub scan_info: serde_json::Value,
    pub detected: Vec<DetectedTxRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedTxRef {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: usize,
    #[serde(default)]
    pub output_index: usize,
    pub asset_id: String,
    pub is_flagged: bool,
}
