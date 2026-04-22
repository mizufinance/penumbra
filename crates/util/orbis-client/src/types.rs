use decaf377::Element;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEnvelope {
    pub enc_cmt: Vec<u8>,
    pub encrypted_data: Vec<u8>,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedSecret {
    pub encrypted_document: Vec<u8>,
    pub enc_cmt: Vec<u8>,
    pub shared_point: Vec<u8>,
    pub challenge: Vec<u8>,
    pub response: Vec<u8>,
    pub metadata: Vec<u8>,
    pub derived_pk: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct DkgResult {
    pub session_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub public_address: String,
    pub peer_id: String,
    pub p2p_address: String,
}

#[derive(Debug, Clone)]
pub struct RingInfo {
    pub ring_id: String,
    pub ring_pk: Element,
    pub ring_pk_hex: String,
}

#[derive(Debug, Clone)]
pub struct StoreSecretResult {
    pub status: String,
    pub message: String,
    pub created_at: i64,
    pub object_id: String,
    pub ring_id: String,
    pub signature: String,
    pub enc_cmt_hex: String,
}

#[derive(Debug, Clone)]
pub struct PreResult {
    pub xnc_cmt_hex: String,
}
