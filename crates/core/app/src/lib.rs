#![deny(clippy::unwrap_used)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use once_cell::sync::Lazy;

pub static SUBSTORE_PREFIXES: Lazy<Vec<String>> = Lazy::new(|| {
    vec![
        penumbra_sdk_ibc::IBC_SUBSTORE_PREFIX.to_string(),
        COMETBFT_SUBSTORE_PREFIX.to_string(),
    ]
});

/// The substore prefix used for storing historical CometBFT block data.
pub static COMETBFT_SUBSTORE_PREFIX: &'static str = "cometbft-data";

pub mod app_version;
pub use app_version::APP_VERSION;

pub mod event;
pub mod genesis;
pub mod params;

cfg_if::cfg_if! {
    if #[cfg(feature="component")] {
        pub mod app;
        pub mod block_tx_indexing;
        pub mod metrics;
        pub mod rpc;
        pub mod server;
        pub mod stateless_cache;

        mod action_handler;
        mod penumbra_host_chain;

        pub use crate::{
            action_handler::AppActionHandler, app::StateWriteExt,
            metrics::register_metrics, penumbra_host_chain::PenumbraHost,
        };

        /// Temporary compat wrapper for duplicate trait impls
        pub struct Compat<'a, T>(&'a T);
    }
}
