#![deny(clippy::unwrap_used)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use once_cell::sync::Lazy;

pub static SUBSTORE_PREFIXES: Lazy<Vec<String>> =
    Lazy::new(|| vec![COMETBFT_SUBSTORE_PREFIX.to_string()]);

/// The substore prefix used for historical block transaction data.
pub static COMETBFT_SUBSTORE_PREFIX: &'static str = "cometbft-data";

pub mod app_version;
pub use app_version::APP_VERSION;

pub mod genesis;
pub mod params;

cfg_if::cfg_if! {
    if #[cfg(feature="component")] {
        pub mod app;
        pub mod block_tx_indexing;
        pub mod metrics;
        pub mod nullifier_generation_packs;
        pub mod stateless_cache;
        #[cfg(any(test, feature = "benchmark-helpers"))]
        pub mod test_support;

        mod action_handler;


    }
}
