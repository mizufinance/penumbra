#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BlockTxIndexingMode {
    NoIndex,
    #[default]
    PerTx,
    DeferredBatch,
}

impl BlockTxIndexingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoIndex => "no_index",
            Self::PerTx => "per_tx",
            Self::DeferredBatch => "deferred_batch",
        }
    }
}
