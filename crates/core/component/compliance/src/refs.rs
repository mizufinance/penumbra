//! Typed references for compliance tracking.
//!
//! The `refs` module maps chain data into stable block, transaction, action,
//! and output references. `TxRef` carries the canonical `TransactionId` so
//! scanner, evidence, and audit rows use the same transaction identity as the
//! rest of Penumbra.

use penumbra_sdk_txhash::TransactionId;

/// Reference to a block by height, hash, parent hash, and optional Unix
/// timestamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockRef {
    pub height: u64,
    pub block_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub block_time_unix: Option<i64>,
}

/// Lightweight reference to a transaction inside a block.
/// `block` identifies the containing block, `tx_index` its position, and
/// `tx_hash` the canonical transaction id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxRef {
    pub block: BlockRef,
    pub tx_index: u32,
    pub tx_hash: TransactionId,
}

/// Reference to an action inside a transaction.
/// `tx` identifies the containing transaction and `action_index` is the
/// zero-based action position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRef {
    pub tx: TxRef,
    pub action_index: u32,
}

/// Reference to a specific output of an action.
/// `action` identifies the containing action and `output_index` is zero-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputRef {
    pub action: ActionRef,
    pub output_index: u32,
}
