use decaf377::Element;
use penumbra_sdk_compliance::OrbisSecretEnvelope;

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

/// Result of storing an encrypted-seed object in Orbis.
#[derive(Debug, Clone)]
pub struct StoreSecretResult {
    pub status: String,
    pub message: String,
    pub created_at: i64,
    pub object_id: String,
    pub ring_id: String,
    pub signature: String,
}

/// Result of asking Orbis to PRE an encrypted-seed object.
#[derive(Debug, Clone)]
pub struct PreResult {
    pub xnc_cmt_hex: String,
    pub secret: OrbisSecretEnvelope,
}
