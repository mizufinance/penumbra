use prost::bytes::Bytes;

/// Candidates and payload budget supplied by the host for batch selection.
pub struct BatchPreparation {
    pub height: u64,
    pub txs: Vec<Bytes>,
    pub max_tx_bytes: i64,
}

/// Ordered transactions, including any aggregate bundle, selected for verification.
pub struct BatchCandidate {
    pub height: u64,
    pub txs: Vec<Bytes>,
}

pub struct PreparedBatch {
    pub txs: Vec<Bytes>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchVerdict {
    Accept,
    Reject,
}
