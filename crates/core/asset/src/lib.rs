#![deny(clippy::unwrap_used)]
#![cfg_attr(docsrs, feature(doc_cfg))]
use once_cell::sync::Lazy;

pub mod asset;
pub mod balance;
mod equivalent_value;
mod estimated_price;
mod value;

pub use balance::Balance;
pub use equivalent_value::EquivalentValue;
pub use estimated_price::EstimatedPrice;
pub use value::{Value, ValueVar, ValueView};

pub static BASE_ASSET_DENOM: Lazy<asset::Metadata> = Lazy::new(|| {
    asset::Cache::with_known_assets()
        .get_unit("upenumbra")
        .expect("unable to get upenumbra denom, which should be hardcoded")
        .base()
});
pub static BASE_ASSET_ID: Lazy<asset::Id> = Lazy::new(|| BASE_ASSET_DENOM.id());

pub static TEST_USD_DENOM: Lazy<asset::Metadata> = Lazy::new(|| {
    asset::Cache::with_known_assets()
        .get_unit("wtest_usd")
        .expect("unable to get wtest_usd denom, which should be hardcoded")
        .base()
});
pub static TEST_USD_ASSET_ID: Lazy<asset::Id> = Lazy::new(|| TEST_USD_DENOM.id());

pub static REGULATED_USD_DENOM: Lazy<asset::Metadata> = Lazy::new(|| {
    asset::Cache::with_known_assets()
        .get_unit("wregulated_usd")
        .expect("unable to get wregulated_usd denom, which should be hardcoded")
        .base()
});
pub static REGULATED_USD_ASSET_ID: Lazy<asset::Id> = Lazy::new(|| REGULATED_USD_DENOM.id());
