use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use cnidarium_component::Component;
use shieldd_sdk_proto::{StateReadProto, StateWriteProto};
use tracing::instrument;

use crate::{epoch::Epoch, genesis, nullifier_tree, params::SctParameters, state_key};

use super::clock::{EpochManager, EpochRead};

pub struct Sct {}

#[async_trait]
impl Component for Sct {
    type AppState = genesis::Content;

    #[instrument(name = "sct_component", skip(state, app_state))]
    async fn init_chain<S: StateWrite>(mut state: S, app_state: Option<&Self::AppState>) {
        match app_state {
            Some(genesis) => {
                state.put_sct_params(genesis.sct_params.clone());
                state.put_block_height(0);
                state.put_epoch_by_height(
                    0,
                    Epoch {
                        index: 0,
                        start_height: 0,
                    },
                );

                // We need to set the epoch for the first block as well, since we set
                // the epoch by height in end_block, and end_block isn't called after init_chain.
                state.put_epoch_by_height(
                    1,
                    Epoch {
                        index: 0,
                        start_height: 0,
                    },
                );
                nullifier_tree::initialize(&mut state)
                    .await
                    .expect("initialize nullifier tree");
            }
            None => { /* no-op until an upgrade occurs */ }
        }
    }

    #[instrument(name = "sct_component", skip(state, begin_block))]
    async fn begin_block<S: StateWrite + 'static>(
        state: &mut Arc<S>,
        begin_block: &cnidarium_component::BlockContext,
    ) {
        let state = Arc::get_mut(state).expect("there's only one reference to the state");
        state.put_block_height(begin_block.height);
        state.put_block_timestamp(begin_block.height, begin_block.time);
        state.object_put(
            state_key::nullifier_generations::pending_block(),
            super::tree::PendingNullifierBlock::default(),
        );
        state.object_delete(state_key::cache::block_materialization());
    }

    #[instrument(name = "sct_component", skip(state, _height))]
    async fn end_block<S: StateWrite + 'static>(state: &mut Arc<S>, _height: u64) {
        use super::tree::SctManager as _;

        Arc::get_mut(state)
            .expect("there's only one reference to the state")
            .materialize_nullifier_block()
            .await
            .expect("materialize block nullifiers");
    }

    #[instrument(name = "sct_component", skip(state))]
    async fn end_epoch<S: StateWrite + 'static>(state: &mut Arc<S>) -> anyhow::Result<()> {
        let state = Arc::get_mut(state).expect("there's only one reference to the state");
        let next_epoch = state
            .get_current_epoch()
            .await?
            .index
            .checked_add(1)
            .ok_or_else(|| anyhow!("application epoch overflow"))?;
        let tree_epoch = u16::try_from(next_epoch)
            .map_err(|_| anyhow!("application epoch exceeds the SCT position range"))?;
        let next_position = u64::from(shieldd_sdk_tct::Position::from((tree_epoch, 0, 0)));

        if let Some(transition) = nullifier_tree::rollover(state, next_epoch, next_position).await?
        {
            state.record_proto(
                shieldd_sdk_proto::shieldd::core::component::sct::v1::EventNullifierGenerationFrozen::from(
                    transition.frozen,
                ),
            );
            if let Some(archived) = transition.archived {
                state.record_proto(
                    shieldd_sdk_proto::shieldd::core::component::sct::v1::EventNullifierGenerationArchived::from(
                        archived,
                    ),
                );
            }
        }
        Ok(())
    }
}

/// This trait provides read access to configuration data for the component.
#[async_trait]
pub trait StateReadExt: StateRead {
    /// Gets the SCT parameters from the state.
    async fn get_sct_params(&self) -> Result<SctParameters> {
        self.get(state_key::config::sct_params())
            .await?
            .ok_or_else(|| anyhow!("Missing SctParameters"))
    }

    /// Fetch the epoch duration parameter (measured in blocks).
    ///
    /// # Errors
    /// Returns an error if the Sct parameters are missing.
    async fn get_epoch_duration_parameter(&self) -> Result<u64> {
        self.get_sct_params()
            .await
            .map(|params| params.epoch_duration)
    }
}

impl<T: StateRead + ?Sized> StateReadExt for T {}

/// This trait provides write access to configuration data for the component.
#[async_trait]
pub trait StateWriteExt: StateWrite {
    fn put_sct_params(&mut self, params: SctParameters) {
        self.put(state_key::config::sct_params().to_string(), params);
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}
