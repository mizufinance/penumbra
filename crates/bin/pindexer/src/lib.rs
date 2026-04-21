pub use cometindex::{AppView, ContextualizedEvent, Indexer, PgPool, PgTransaction};

mod indexer_ext;
pub use indexer_ext::IndexerExt;
pub mod block;
pub mod ibc;
pub mod insights;
mod parsing;
pub mod stake;
pub mod supply;

pub mod governance;

#[derive(clap::Parser, Clone, Debug)]
pub struct Options {
    #[clap(flatten)]
    pub cometindex: cometindex::opt::Options,
}
