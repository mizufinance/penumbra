use anyhow::ensure;
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::BASE_ASSET_ID;
use shieldd_sdk_proto::shieldd::core::component::fee::v1 as pb;
use shieldd_sdk_proto::DomainType;

use crate::GasPrices;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(try_from = "pb::FeeParameters", into = "pb::FeeParameters")]
pub struct FeeParameters {
    pub fixed_gas_prices: GasPrices,
}

impl FeeParameters {
    pub fn validate_base_asset_only(&self) -> anyhow::Result<()> {
        ensure!(
            self.fixed_gas_prices.asset_id == *BASE_ASSET_ID,
            "only base-asset gas prices are supported, found {}",
            self.fixed_gas_prices.asset_id,
        );
        Ok(())
    }
}

impl DomainType for FeeParameters {
    type Proto = pb::FeeParameters;
}

impl TryFrom<pb::FeeParameters> for FeeParameters {
    type Error = anyhow::Error;

    fn try_from(msg: pb::FeeParameters) -> anyhow::Result<Self> {
        let params = FeeParameters {
            fixed_gas_prices: msg.fixed_gas_prices.unwrap_or_default().try_into()?,
        };
        params.validate_base_asset_only()?;
        Ok(params)
    }
}

impl From<FeeParameters> for pb::FeeParameters {
    fn from(params: FeeParameters) -> Self {
        pb::FeeParameters {
            fixed_gas_prices: Some(params.fixed_gas_prices.into()),
        }
    }
}
