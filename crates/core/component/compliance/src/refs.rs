use penumbra_sdk_txhash::TransactionId;

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
