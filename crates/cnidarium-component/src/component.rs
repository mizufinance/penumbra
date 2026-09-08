use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use cnidarium::StateWrite;

/// Host-supplied facts needed by execution components at the start of a block.
#[derive(Clone, Copy, Debug)]
pub struct BlockContext {
    pub height: u64,
    pub time: tendermint::Time,
}

/// Stateful lifecycle hooks; implementations must release cloned state before returning.
#[async_trait]
pub trait Component {
    type AppState;

    /// Initializes virgin state or validates a checkpoint when `app_state` is absent.
    async fn init_chain<S: StateWrite>(state: S, app_state: Option<&Self::AppState>);

    /// Begins execution with exclusive state ownership on entry and return.
    async fn begin_block<S: StateWrite + 'static>(state: &mut Arc<S>, block: &BlockContext);

    /// Closes the current block with exclusive state ownership on entry and return.
    async fn end_block<S: StateWrite + 'static>(state: &mut Arc<S>, height: u64);

    /// Applies epoch transitions with exclusive state ownership on entry and return.
    async fn end_epoch<S: StateWrite + 'static>(_state: &mut Arc<S>) -> Result<()> {
        Ok(())
    }
}
