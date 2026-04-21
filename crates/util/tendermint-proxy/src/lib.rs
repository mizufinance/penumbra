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
        let client = build_tendermint_http_client(&tendermint_url);
        Self {
            tendermint_url,
            client,
        }
    }
}

fn build_tendermint_http_client(tendermint_url: &url::Url) -> HttpClient {
    let rpc_url = tendermint_url
        .as_ref()
        .try_into()
        .expect("tendermint rpc URL should be validated before proxy creation");

    // Smoke/devnet connects to a local CometBFT RPC endpoint. Building the reqwest client
    // with system proxy autodetection can panic on macOS in `system-configuration`, so we
    // opt out here and provide the client explicitly.
    let http_client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("tendermint rpc client should build");

    HttpClient::builder(rpc_url)
        .client(http_client)
        .build()
        .expect("tendermint rpc URL should be validated before proxy creation")
}

impl std::fmt::Debug for TendermintProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TendermintProxy")
            .field("tendermint_url", &self.tendermint_url)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::build_tendermint_http_client;

    #[test]
    fn build_tendermint_http_client_for_localhost() {
        let url = "http://127.0.0.1:16657"
            .parse()
            .expect("localhost tendermint URL should parse");
        let _client = build_tendermint_http_client(&url);
    }
}
