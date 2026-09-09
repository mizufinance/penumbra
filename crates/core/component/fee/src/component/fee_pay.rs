use anyhow::{ensure, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use shieldd_sdk_asset::Value;
use shieldd_sdk_proto::core::component::fee::v1 as pb;
use shieldd_sdk_proto::state::StateWriteProto as _;

use crate::{Fee, Gas};

use super::view::{StateReadExt, StateWriteExt};

const BLOCK_FEE_PRICE_CACHE_KEY: &str = "shieldd.fee.block_fee_price_cache";

#[derive(Clone, Debug)]
struct BlockFeePriceCache {
    base_gas_prices: crate::GasPrices,
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
                    base_gas_prices: self
                        .get_gas_prices()
                        .await
                        .expect("gas prices must be present in state"),
                };
                self.object_put(BLOCK_FEE_PRICE_CACHE_KEY, cache.clone());
                cache
            };

        ensure!(
            fee.asset_id() == *shieldd_sdk_asset::BASE_ASSET_ID,
            "only base-asset fees are supported, found {}",
            fee.asset_id(),
        );

        let current_gas_prices = fee_price_cache.base_gas_prices;

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

        self.accumulate_fees(base_fee.amount(), tip.amount());

        Ok(())
    }
}

impl<S: StateWrite + ?Sized> FeePay for S {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{component::BlockFees, FeeParameters, GasPrices};

    #[tokio::test]
    async fn payments_accumulate_required_fees_and_tips_only_after_validation() -> Result<()> {
        let storage = cnidarium::TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        state.put_fee_params(FeeParameters {
            fixed_gas_prices: GasPrices {
                block_space_price: 1_000,
                ..Default::default()
            },
        });
        assert_eq!(state.block_fees(), BlockFees::default());
        state
            .pay_fee(Gas::zero(), Fee::from_staking_token_amount(0u64.into()))
            .await?;
        assert_eq!(state.block_fees(), BlockFees::default());
        let gas = Gas {
            block_space: 10,
            ..Gas::zero()
        };
        for payment in [10u64, 12] {
            state
                .pay_fee(gas, Fee::from_staking_token_amount(payment.into()))
                .await?;
        }
        let expected = BlockFees {
            base: 20u64.into(),
            tip: 2u64.into(),
        };
        assert_eq!(state.block_fees(), expected);
        for fee in [
            Fee::from_staking_token_amount(9u64.into()),
            Fee(Value {
                asset_id: shieldd_sdk_asset::asset::Id(decaf377::Fq::from(42u64)),
                amount: 100u64.into(),
            }),
        ] {
            assert!(state.pay_fee(gas, fee).await.is_err());
            assert_eq!(state.block_fees(), expected);
        }
        Ok(())
    }
}
