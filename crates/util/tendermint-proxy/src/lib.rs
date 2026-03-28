#![deny(clippy::unwrap_used)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

//! Facilities for proxying gRPC requests to an upstream Tendermint/CometBFT RPC.
//!
//! Most importantly, this crate provides [`TendermintProxy`], which implements Penumbra's
//! [`tendermint_proxy`][proxy-proto] RPC.
//!
//! [proxy-proto]: https://buf.build/penumbra-zone/penumbra/docs/main:penumbra.util.tendermint_proxy.v1

mod tendermint_proxy;

use tendermint_rpc::HttpClient;

/// Implements service traits for Tonic gRPC services.
///
/// The fields of this struct are the configuration and data
/// necessary to the gRPC services.
#[derive(Clone)]
pub struct TendermintProxy {
    /// Address of upstream Tendermint server to proxy requests to.
    tendermint_url: url::Url,
    /// Reused Tendermint RPC client for front-door proxy requests.
    client: HttpClient,
}

impl TendermintProxy {
    /// Returns a new [`TendermintProxy`].
    pub fn new(tendermint_url: url::Url) -> Self {
        let client = HttpClient::new(tendermint_url.as_ref())
            .expect("tendermint rpc URL should be validated before proxy creation");
        Self {
            tendermint_url,
            client,
        }
    }
}

impl std::fmt::Debug for TendermintProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TendermintProxy")
            .field("tendermint_url", &self.tendermint_url)
            .finish()
    }
}
