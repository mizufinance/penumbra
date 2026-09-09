use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{StateReadProto, StateWriteProto};

use crate::{params::FeeParameters, state_key, GasPrices};

/// Block-local fee totals, paid exclusively in the base asset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockFees {
    pub base: Amount,
    pub tip: Amount,
}

/// This trait provides read access to fee-related parts of the Shieldd
/// state store.
#[async_trait]
pub trait StateReadExt: StateRead {
    /// Gets the fee parameters from the JMT.
    async fn get_fee_params(&self) -> Result<FeeParameters> {
        self.get(state_key::fee_params())
            .await?
            .ok_or_else(|| anyhow!("Missing FeeParameters"))
    }

    /// Gets the current gas prices for the fee token.
    async fn get_gas_prices(&self) -> Result<GasPrices> {
        let params = self.get_fee_params().await?;
        params.validate_base_asset_only()?;
        Ok(params.fixed_gas_prices)
    }

    /// Returns true if the gas prices have been changed in this block.
    fn gas_prices_changed(&self) -> bool {
        self.object_get::<()>(state_key::gas_prices_changed())
            .is_some()
    }

    /// The accumulated required fees and tips for this block.
    fn block_fees(&self) -> BlockFees {
        self.object_get(state_key::fee_accumulator())
            .unwrap_or_default()
    }
}

impl<T: StateRead + ?Sized> StateReadExt for T {}

#[async_trait]
pub trait StateWriteExt: StateWrite {
    /// Writes the provided fee parameters to the JMT.
    fn put_fee_params(&mut self, params: FeeParameters) {
        self.put(state_key::fee_params().into(), params);
        // This could have changed the gas prices, so mark them as changed.
        self.object_put(state_key::gas_prices_changed(), ());
    }

    fn accumulate_fees(&mut self, base: Amount, tip: Amount) {
        let fees = self.block_fees();
        self.object_put(
            state_key::fee_accumulator(),
            BlockFees {
                base: fees.base + base,
                tip: fees.tip + tip,
            },
        );
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}
