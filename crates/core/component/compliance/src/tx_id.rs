use penumbra_sdk_proto::{core::transaction::v1::Transaction as ProtoTransaction, Message};
use penumbra_sdk_txhash::TransactionId;
use sha2::{Digest, Sha256};

/// Compute the scanner's transaction id from canonical Penumbra transaction
/// protobuf bytes.
///
/// This must remain byte-for-byte compatible with
/// `penumbra_sdk_transaction::Transaction::id()`. The transaction crate owns a
/// parity test so future transaction encoding changes fail loudly.
pub fn scanner_transaction_id_from_proto(tx: &ProtoTransaction) -> TransactionId {
    TransactionId(Sha256::digest(tx.encode_to_vec()).into())
}
