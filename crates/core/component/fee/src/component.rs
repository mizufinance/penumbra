mod fee_pay;
mod view;

use std::sync::Arc;

use crate::{event::EventBlockFees, genesis, Fee};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::Component;
use shieldd_sdk_proto::state::StateWriteProto as _;
use shieldd_sdk_proto::DomainType as _;
use tracing::instrument;

pub use fee_pay::{clear_block_fee_price_cache, FeePay};
pub use view::{BlockFees, StateReadExt, StateWriteExt};

// Fee component
pub struct FeeComponent {}

#[async_trait]
impl Component for FeeComponent {
    type AppState = genesis::Content;

    #[instrument(name = "fee", skip(state, app_state))]
    async fn init_chain<S: StateWrite>(mut state: S, app_state: Option<&Self::AppState>) {
        match app_state {
            Some(genesis) => {
                genesis
                    .fee_params
                    .validate_base_asset_only()
                    .expect("fee params must use base-asset gas only");
                state.put_fee_params(genesis.fee_params.clone());
            }
            None => { /* perform upgrade specific check */ }
        }
    }

    #[instrument(name = "fee", skip(_state, _begin_block))]
    async fn begin_block<S: StateWrite + 'static>(
        _state: &mut Arc<S>,
        _begin_block: &cnidarium_component::BlockContext,
    ) {
    }

    #[instrument(name = "fee", skip(state, _height))]
    async fn end_block<S: StateWrite + 'static>(state: &mut Arc<S>, _height: u64) {
        let state_ref = Arc::get_mut(state).expect("unique ref in end_block");
        // Grab the total fees and use them to emit an event.
        let fees = state_ref.block_fees();

        state_ref.record_proto(
            EventBlockFees {
                swapped_fee_total: Fee::from_staking_token_amount(fees.base + fees.tip),
                swapped_base_fee_total: Fee::from_staking_token_amount(fees.base),
                swapped_tip_total: Fee::from_staking_token_amount(fees.tip),
            }
            .to_proto(),
        );
    }

    #[instrument(name = "fee", skip(_state))]
    async fn end_epoch<S: StateWrite + 'static>(_state: &mut Arc<S>) -> anyhow::Result<()> {
        Ok(())
    }
}
