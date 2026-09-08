//! Command-line interface utilities for the `pd` daemon.

use {
    clap::{Parser, Subcommand},
    std::{net::SocketAddr, path::PathBuf},
    url::Url,
};

#[derive(Debug, Parser)]
#[clap(name = "pd", about = "The Shieldd daemon.", version)]
pub struct Opt {
    /// Command to run.
    #[clap(subcommand)]
    pub cmd: RootCommand,
}

#[derive(Debug, Subcommand)]
pub enum RootCommand {
    /// Starts the Shieldd daemon.
    Start {
        /// The path used to store all `pd`-related data and configuration.
        /// If unset, defaults to ~/.shieldd/network_data/node0/pd.
        #[clap(long, env = "SHIELDD_PD_HOME", display_order = 100)]
        home: Option<PathBuf>,
        /// Bind the ABCI server to this socket.
        ///
        /// The ABCI server is used by Tendermint to drive the application state.
        #[clap(
            short,
            long,
            env = "SHIELDD_PD_ABCI_BIND",
            default_value = "127.0.0.1:26658",
            display_order = 400
        )]
        // TODO: Add support for Unix domain sockets, available in tower-abci >=0.10.0
        abci_bind: SocketAddr,
        /// Bind the gRPC server to this socket.
        ///
        /// The gRPC server supports both grpc (HTTP/2) and grpc-web (HTTP/1.1) clients.
        ///
        /// If `grpc_auto_https` is set, this defaults to `0.0.0.0:443` and uses HTTPS.
        ///
        /// If `grpc_auto_https` is not set, this defaults to `127.0.0.1:8080` without HTTPS.
        #[clap(short, long, env = "SHIELDD_PD_GRPC_BIND", display_order = 201)]
        grpc_bind: Option<SocketAddr>,
        /// If set, serve gRPC using auto-managed HTTPS with this domain name.
        ///
        /// NOTE: This option automatically provisions TLS certificates from
        /// Let's Encrypt and caches them in the `home` directory.  The
        /// production LE CA has rate limits, so be careful using this option
        /// with `pd network unsafe-reset-all`, which will delete the certificates
        /// and force re-issuance, possibly hitting the rate limit. See the
        /// `--acme-staging` option.
        #[clap(long, value_name = "DOMAIN", display_order = 200)]
        grpc_auto_https: Option<String>,
        /// Enable use of the LetsEncrypt ACME staging API (https://letsencrypt.org/docs/staging-environment/),
        /// which is more forgiving of ratelimits. Set this option to `true`
        /// if you're trying out the `--grpc-auto-https` option for the first time,
        /// to validate your configuration, before subjecting yourself to production
        /// ratelimits. This option has no effect if `--grpc-auto-https` is not set.
        #[clap(long, display_order = 201)]
        acme_staging: bool,
        /// Bind the metrics endpoint to this socket.
        #[clap(
            short,
            long,
            env = "SHIELDD_PD_METRICS_BIND",
            default_value = "127.0.0.1:9000",
            display_order = 300
        )]
        metrics_bind: SocketAddr,
        /// The JSON-RPC address of the CometBFT node driving this `pd`
        /// instance.
        ///
        /// This is used to proxy requests from the gRPC server to CometBFT,
        /// so clients only need to connect to one endpoint and don't need to
        /// worry about the peculiarities of CometBFT's JSON-RPC encoding
        /// format.
        #[clap(
            short,
            long,
            env = "SHIELDD_PD_COMETBFT_PROXY_URL",
            default_value = "http://127.0.0.1:26657",
            display_order = 401
        )]
        cometbft_addr: Url,
        /// Enable expensive RPCs, currently a no-op.
        #[clap(short, long, display_order = 500)]
        enable_expensive_rpc: bool,
    },
    /// Generate, join, or reset a network.
    Network {
        /// Path to directory to store output in. Must not exist. Defaults to
        /// ~/.shieldd/network_data".
        #[clap(long)]
        network_dir: Option<PathBuf>,
        #[clap(subcommand)]
        net_cmd: NetworkCommand,
    },

    /// Export the storage state the full node.
    Export {
        /// The home directory of the full node.
        #[clap(long, env = "SHIELDD_PD_HOME", display_order = 100)]
        home: PathBuf,
        /// The directory where the exported node state will be written.
        #[clap(long, display_order = 200)]
        export_directory: PathBuf,
        /// An optional filepath for a compressed archive containing the exported
        /// node state, e.g. ~/pd-backup.tar.gz.
        #[clap(long, display_order = 200)]
        export_archive: Option<PathBuf>,
    },

    /// Build, verify, and optionally prune one retired nullifier generation pack.
    NullifierGenerationPack {
        /// The home directory of the stopped full node.
        #[clap(long, env = "SHIELDD_PD_HOME", display_order = 100)]
        home: PathBuf,
        /// Retired generation index to pack.
        #[clap(long)]
        generation: u64,
        /// Delete expanded tree records after the pack is durably written and verified.
        #[clap(long)]
        prune: bool,
    },

    /// Verify one retired nullifier generation pack without changing node state.
    NullifierGenerationPackVerify {
        /// The home directory of the stopped full node.
        #[clap(long, env = "SHIELDD_PD_HOME", display_order = 100)]
        home: PathBuf,
        /// Retired generation index to verify.
        #[clap(long)]
        generation: u64,
    },
}

#[derive(Debug, Subcommand)]
pub enum NetworkCommand {
    /// Generates a directory structure containing necessary files to create a new
    /// network config from genesis, based on input configuration.
    Generate {
        /// The `timeout_commit` parameter (block interval) to configure Tendermint with.
        #[clap(long)]
        timeout_commit: Option<tendermint::Timeout>,
        /// Number of blocks per epoch.
        #[clap(long)]
        epoch_duration: Option<u64>,
        /// Whether to preserve the chain ID (useful for public networks) or append a random suffix (useful for dev/testing).
        #[clap(long)]
        preserve_chain_id: bool,
        /// Path to CSV file containing initial allocations [default: bundled development configuration].
        #[clap(long, parse(from_os_str))]
        allocations_input_file: Option<PathBuf>,
        /// Shieldd wallet address to include in genesis allocations.
        /// Intended to make dev experience nicer on first run:
        /// generate a wallet, view its address, then generate a devnet
        /// with that address included in the base allocations.
        #[clap(long)]
        allocation_address: Option<shieldd_sdk_keys::Address>,
        #[clap(long, parse(from_os_str))]
        /// Path to JSON file containing initial validator configs [default: bundled development configuration].
        validators_input_file: Option<PathBuf>,
        /// Testnet name [default: bundled development configuration].
        #[clap(long)]
        chain_id: Option<String>,
        /// The fixed gas price for all transactions on the network.
        /// Described as "simple" because the single value will be reused
        /// for all gas price types: block space, compact block space, verification, and execution.
        /// The numeric value is one-thousandths of the base unit of the fee token,
        /// so `--gas-price-simple=1000` means all resources will have a cost of 1ushieldd.
        #[clap(long)]
        gas_price_simple: Option<u64>,
        /// Compliance registrar verification key to authorize asset registration grants.
        #[clap(long = "compliance-registrar-vk-hex")]
        compliance_registrar_vk_hex: Vec<String>,
        /// Path to JSON containing native compliance assets and genesis users.
        #[clap(long, parse(from_os_str))]
        compliance_genesis_input_file: Option<PathBuf>,
        /// Base hostname for a validator's p2p service. If multiple validators
        /// exist in the genesis, e.g. via `--validators-input-file`, then
        /// numeric suffixes are automatically added, e.g. "-0", "-1", etc.
        /// Helpful for when you know the validator DNS names ahead of time,
        /// e.g. in Kubernetes service addresses. These option is most useful
        /// to provide peering on a private network setup. If you plan to expose
        /// the validator P2P services to the internet, see the `--external-addresses` option.
        #[clap(long)]
        peer_address_template: Option<String>,
        /// Public addresses and ports for the Tendermint P2P services of the genesis
        /// validator. Accepts comma-separated values, to support multiple validators.
        /// If `--validators-input-file` is used to increase the number
        /// of validators, and the `--external-addresses` flag is set, then the number of
        /// external addresses must equal the number of validators. See the
        /// `--peer-address-template` flag if you don't plan to expose the network
        /// to public peers.
        #[clap(long)]
        // TODO we should support DNS names here. However, there are complications:
        // https://github.com/tendermint/tendermint/issues/1521
        external_addresses: Option<String>,
        /// When generating Tendermint config, use this socket to bind the Tendermint RPC service.
        #[clap(long, env = "SHIELDD_PD_TM_RPC_BIND", default_value = "0.0.0.0:26657")]
        tendermint_rpc_bind: SocketAddr,
        /// When generating Tendermint config, use this socket to bind the Tendermint P2P service.
        #[clap(long, env = "SHIELDD_PD_TM_P2P_BIND", default_value = "0.0.0.0:26656")]
        tendermint_p2p_bind: SocketAddr,
    },

    /// Like `network generate`, but joins the network to which the specified node belongs.
    /// Requires a URL for the CometBFT RPC for the bootstrap node.
    Join {
        /// URL of the remote CometBFT RPC endpoint for bootstrapping connection.
        #[clap(env = "SHIELDD_PD_JOIN_URL")]
        node: Url,
        /// Optional URL of archived node state in .tar.gz format. The archive will be
        /// downloaded and extracted locally, allowing the node to join a network at a block height
        /// higher than 0. Supports loading the archive from a local file, if set with file scheme
        /// explicitly, e.g. `file:///path/to/archive.tar.gz`.
        #[clap(long, env = "SHIELDD_PD_ARCHIVE_URL")]
        archive_url: Option<Url>,
        /// Human-readable name to identify node on network
        // Default: 'node-#'
        #[clap(long, env = "SHIELDD_PD_TM_EXTERNAL_ADDR")]
        moniker: Option<String>,
        /// Public URL to advertise for this node's Tendermint P2P service.
        /// Setting this option will instruct other nodes on the network to connect
        /// to yours. Must be in the form of a socket, e.g. "1.2.3.4:26656".
        #[clap(long, env = "SHIELDD_PD_TM_EXTERNAL_ADDR")]
        external_address: Option<SocketAddr>,
        /// When generating Tendermint config, use this socket to bind the Tendermint RPC service.
        #[clap(long, env = "SHIELDD_PD_TM_RPC_BIND", default_value = "0.0.0.0:26657")]
        tendermint_rpc_bind: SocketAddr,
        /// When generating Tendermint config, use this socket to bind the Tendermint P2P service.
        #[clap(long, env = "SHIELDD_PD_TM_P2P_BIND", default_value = "0.0.0.0:26656")]
        tendermint_p2p_bind: SocketAddr,
        /// Leave the downloaded archive file on disk after extraction.
        #[clap(long, env = "SHIELDD_PD_LEAVE_ARCHIVE", action)]
        leave_archive: bool,
    },

    /// Reset all `pd` network state. This is a destructive action!
    UnsafeResetAll {},
}
