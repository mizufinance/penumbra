use std::collections::BTreeMap;

use anyhow::{ensure, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use penumbra_sdk_asset::Value;
use penumbra_sdk_proto::core::component::fee::v1 as pb;
use penumbra_sdk_proto::state::StateWriteProto as _;

use crate::{Fee, Gas};

use super::view::{StateReadExt, StateWriteExt};

const BLOCK_FEE_PRICE_CACHE_KEY: &str = "penumbra.fee.block_fee_price_cache";

#[derive(Clone, Debug)]
struct BlockFeePriceCache {
    staking_gas_prices: crate::GasPrices,
    alt_gas_prices: BTreeMap<penumbra_sdk_asset::asset::Id, crate::GasPrices>,
}

pub fn clear_block_fee_price_cache<S: StateWrite>(state: &mut S) {
    state.object_delete(BLOCK_FEE_PRICE_CACHE_KEY);
}

/// Allows payment of transaction fees.
#[async_trait]
pub trait FeePay: StateWrite {
    /// Uses the provided `fee` to pay for `gas_used`, erroring if the fee is insufficient.
    async fn pay_fee(&mut self, gas_used: Gas, fee: Fee) -> Result<()> {
        let fee_price_cache =
            if let Some(cache) = self.object_get::<BlockFeePriceCache>(BLOCK_FEE_PRICE_CACHE_KEY) {
                cache
            } else {
                let cache = BlockFeePriceCache {
                    staking_gas_prices: self
                        .get_gas_prices()
                        .await
                        .expect("gas prices must be present in state"),
                    alt_gas_prices: self
                        .get_alt_gas_prices()
                        .await
                        .expect("alt gas prices must be present in state")
                        .into_iter()
                        .map(|prices| (prices.asset_id, prices))
                        .collect(),
                };
                self.object_put(BLOCK_FEE_PRICE_CACHE_KEY, cache.clone());
                cache
            };

        let current_gas_prices = if fee.asset_id() == *penumbra_sdk_asset::STAKING_TOKEN_ASSET_ID {
            fee_price_cache.staking_gas_prices
        } else {
            fee_price_cache
                .alt_gas_prices
                .get(&fee.asset_id())
                .copied()
                .ok_or_else(|| {
                    anyhow::anyhow!("fee token {} not recognized by the chain", fee.asset_id())
                })?
        };

        // Double check that the gas price assets match.
        ensure!(
            current_gas_prices.asset_id == fee.asset_id(),
            "unexpected mismatch between fee and queried gas prices (expected: {}, found: {})",
            fee.asset_id(),
            current_gas_prices.asset_id,
        );

        // Compute the base fee for the `gas_used`.
        let base_fee = current_gas_prices.fee(&gas_used);

        // The provided fee must be at least the base fee.
        ensure!(
            fee.amount() >= base_fee.amount(),
            "fee must be greater than or equal to the transaction base price (supplied: {}, base: {})",
            fee.amount(),
            base_fee.amount(),
        );

        // Otherwise, the fee less the base fee is the proposer tip.
        let tip = Fee(Value {
            amount: fee.amount() - base_fee.amount(),
            asset_id: fee.asset_id(),
        });

        self.record_proto(pb::EventPaidFee {
            fee: Some(fee.into()),
            base_fee: Some(base_fee.into()),
            gas_used: Some(gas_used.into()),
            tip: Some(tip.into()),
        });

        self.raw_accumulate_base_fee_and_tip(base_fee, tip);

        Ok(())
    }
}

impl<S: StateWrite + ?Sized> FeePay for S {}
