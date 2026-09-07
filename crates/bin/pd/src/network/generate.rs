//! Logic for creating a new testnet configuration.
//! Used for deploying (approximately weekly) testnets
//! for Shieldd.
use crate::network::config::{get_network_dir, NetworkTendermintConfig, ValidatorKeys};
use anyhow::{Context, Result};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use serde::{de, Deserialize};
use shieldd_sdk_app::{
    app::{MAX_BLOCK_TXS_PAYLOAD_BYTES, MAX_EVIDENCE_SIZE_BYTES},
    params::AppParameters,
};
use shieldd_sdk_asset::BASE_ASSET_ID;
use shieldd_sdk_compliance::genesis::Content as ComplianceContent;
use shieldd_sdk_fee::genesis::Content as FeeContent;
use shieldd_sdk_keys::Address;
use shieldd_sdk_sct::genesis::Content as SctContent;
use shieldd_sdk_sct::params::SctParameters;
use shieldd_sdk_shielded_pool::{
    genesis::{self as shielded_pool_genesis, Allocation, Content as ShieldedPoolContent},
    params::ShieldedPoolParameters,
};
use std::{
    fmt,
    fs::File,
    io::Read,
    net::SocketAddr,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tendermint::consensus::params::AbciParams;
use tendermint::{node, public_key::Algorithm, Genesis, Time};
use tendermint_config::net::Address as TendermintAddress;

/// Represents a Shieldd network config, including initial validators
/// and allocations at genesis time.
pub struct NetworkConfig {
    /// The name of the network
    pub name: String,
    /// The Tendermint genesis for initial chain state.
    pub genesis: Genesis<shieldd_sdk_app::genesis::AppState>,
    /// Path to local directory where config files will be written to
    pub network_dir: PathBuf,
    /// Set of validators at genesis. Uses the convenient wrapper type
    /// to generate config files.
    pub network_validators: Vec<NetworkValidator>,
    /// Hostname as string for a validator's p2p service. Will have
    /// numbers affixed to it for each validator, e.g. "-0", "-1", etc.
    pub peer_address_template: Option<String>,
    /// The Tendermint `consensus.timeout_commit` value, controlling how long Tendermint should
    /// wait after committing a block, before starting on the new height. If unspecified, `5s`.
    pub tendermint_timeout_commit: Option<tendermint::Timeout>,
    /// Bind address for the CometBFT RPC service.
    pub tendermint_rpc_bind: SocketAddr,
    /// Bind address for the CometBFT P2P service.
    pub tendermint_p2p_bind: SocketAddr,
}

impl NetworkConfig {
    /// Create a new testnet configuration, optionally customizing the allocations and validator
    /// set. By default, will use the prepared Discord allocations and Shieldd Labs CI validator
    /// configs.
    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        chain_id: &str,
        network_dir: Option<PathBuf>,
        peer_address_template: Option<String>,
        external_addresses: Option<Vec<TendermintAddress>>,
        allocations_input_file: Option<PathBuf>,
        allocation_address: Option<Address>,
        validators_input_file: Option<PathBuf>,
        tendermint_timeout_commit: Option<tendermint::Timeout>,
        epoch_duration: Option<u64>,
        gas_price_simple: Option<u64>,
        compliance_registrar_vk: Vec<VerificationKey<SpendAuth>>,
        tendermint_rpc_bind: SocketAddr,
        tendermint_p2p_bind: SocketAddr,
    ) -> anyhow::Result<NetworkConfig> {
        let external_addresses = external_addresses.unwrap_or_default();

        let network_validators = Self::collect_validators(
            validators_input_file,
            peer_address_template.clone(),
            external_addresses,
        )?;

        let mut allocations = Self::collect_allocations(allocations_input_file)?;

        // Add an extra allocation for a dynamic wallet address.
        if let Some(address) = allocation_address {
            tracing::info!(%address, "adding dynamic allocation to genesis");
            allocations.extend(NetworkAllocation::simple(address));
        }
        let app_state = Self::make_genesis_content(
            chain_id,
            allocations,
            epoch_duration,
            gas_price_simple,
            compliance_registrar_vk,
        )?;
        let mut genesis = Self::make_genesis(app_state)?;
        genesis.validators = network_validators
            .iter()
            .map(NetworkValidator::consensus_validator)
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(NetworkConfig {
            name: chain_id.to_owned(),
            genesis,
            network_dir: get_network_dir(network_dir),
            network_validators,
            peer_address_template,
            tendermint_timeout_commit,
            tendermint_rpc_bind,
            tendermint_p2p_bind,
        })
    }

    /// Read supplied validators or use the bundled development defaults.
    fn collect_validators(
        validators_input_file: Option<PathBuf>,
        peer_address_template: Option<String>,
        external_addresses: Vec<TendermintAddress>,
    ) -> anyhow::Result<Vec<NetworkValidator>> {
        let testnet_validators = if let Some(validators_input_file) = validators_input_file {
            NetworkValidator::from_json(validators_input_file)?
        } else {
            NetworkValidator::from_reader(std::io::Cursor::new(include_str!(
                "defaults/validators.json"
            )))
            .context("parse bundled development validators")?
        };

        if !external_addresses.is_empty() && external_addresses.len() != testnet_validators.len() {
            anyhow::bail!("Number of validators did not equal number of external addresses");
        }

        Ok(testnet_validators
            .into_iter()
            .enumerate()
            .map(|(i, v)| NetworkValidator {
                peer_address_template: peer_address_template.as_ref().map(|t| format!("{t}-{i}")),
                external_address: external_addresses.get(i).cloned(),
                ..v
            })
            .collect())
    }

    /// Read supplied allocations or use the bundled development defaults.
    fn collect_allocations(
        allocations_input_file: Option<PathBuf>,
    ) -> anyhow::Result<Vec<Allocation>> {
        if let Some(ref allocations_input_file) = allocations_input_file {
            Ok(
                NetworkAllocation::from_csv(allocations_input_file.to_path_buf()).with_context(
                    || format!("could not parse allocations file {allocations_input_file:?}"),
                )?,
            )
        } else {
            NetworkAllocation::from_reader(std::io::Cursor::new(include_str!(
                "defaults/allocations.csv"
            )))
            .context("parse bundled development allocations")
        }
    }

    /// Create a full genesis configuration for inclusion in the tendermint
    /// genesis config.
    fn make_genesis_content(
        chain_id: &str,
        allocations: Vec<Allocation>,
        epoch_duration: Option<u64>,
        gas_price_simple: Option<u64>,
        compliance_registrar_vk: Vec<VerificationKey<SpendAuth>>,
    ) -> anyhow::Result<shieldd_sdk_app::genesis::Content> {
        // Look up default app params, so we can fill in defaults.
        let default_app_params = AppParameters::default();

        let gas_price_simple = gas_price_simple.unwrap_or_default();

        let app_state = shieldd_sdk_app::genesis::Content {
            chain_id: chain_id.to_string(),
            fee_content: FeeContent {
                fee_params: shieldd_sdk_fee::params::FeeParameters {
                    fixed_gas_prices: shieldd_sdk_fee::GasPrices {
                        block_space_price: gas_price_simple,
                        compact_block_space_price: gas_price_simple,
                        verification_price: gas_price_simple,
                        execution_price: gas_price_simple,
                        asset_id: *BASE_ASSET_ID,
                    },
                    fixed_alt_gas_prices: vec![],
                },
            },
            compliance_content: ComplianceContent {
                compliance_registrar_vk,
                ..Default::default()
            },
            shielded_pool_content: ShieldedPoolContent {
                shielded_pool_params: ShieldedPoolParameters::default(),
                allocations: allocations.clone(),
            },
            sct_content: SctContent {
                sct_params: SctParameters {
                    epoch_duration: epoch_duration
                        .unwrap_or(default_app_params.sct_params.epoch_duration),
                    sct_anchor_retention_blocks: default_app_params
                        .sct_params
                        .sct_anchor_retention_blocks,
                },
            },
            ..Default::default()
        };
        Ok(app_state)
    }

    /// Build Tendermint genesis data, based on Shieldd initial application state.
    pub(crate) fn make_genesis(
        app_state: shieldd_sdk_app::genesis::Content,
    ) -> anyhow::Result<Genesis<shieldd_sdk_app::genesis::AppState>> {
        // Use now as genesis time
        let genesis_time = Time::from_unix_timestamp(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("expected that time travels linearly in a forward direction")?
                .as_secs() as i64,
            0,
        )
        .context("failed to convert current time into Time")?;

        // Create Tendermint genesis data shared by all nodes
        let genesis = Genesis {
            genesis_time,
            chain_id: app_state
                .chain_id
                .parse::<tendermint::chain::Id>()
                .context("failed to parse chain ID")?,
            initial_height: 0,
            consensus_params: tendermint::consensus::Params {
                abci: AbciParams::default(),
                block: tendermint::block::Size {
                    // 1MB
                    max_bytes: MAX_BLOCK_TXS_PAYLOAD_BYTES as u64,
                    // Set to infinity since a chain running Shieldd won't use
                    // cometbft's notion of gas.
                    max_gas: -1,
                    // Minimum time increment between consecutive blocks.
                    time_iota_ms: 500,
                },
                evidence: tendermint::evidence::Params {
                    // Keep this roughly aligned with the intended evidence retention window.
                    max_age_num_blocks: 130000,
                    // Similarly, we set the max age duration for evidence to be a little over a week.
                    max_age_duration: tendermint::evidence::Duration(Duration::from_secs(650000)),
                    // 30KB
                    max_bytes: MAX_EVIDENCE_SIZE_BYTES as i64,
                },
                validator: tendermint::consensus::params::ValidatorParams {
                    pub_key_types: vec![Algorithm::Ed25519],
                },
                version: Some(tendermint::consensus::params::VersionParams { app: 0 }),
            },
            // always empty in genesis json
            app_hash: tendermint::AppHash::default(),
            app_state: shieldd_sdk_app::genesis::AppState::Content(app_state),
            validators: vec![],
        };
        Ok(genesis)
    }

    /// Generate and write to disk the Tendermint configs for each validator at genesis.
    pub fn write_configs(&self) -> anyhow::Result<()> {
        // Loop over each validator and write its config separately.
        for (n, v) in self.network_validators.iter().enumerate() {
            // Create the directory for this node
            let node_name = format!("node{n}");
            let node_dir = self.network_dir.clone().join(node_name.clone());

            // Each node should include only the IPs for *other* nodes in their peers list.
            let ips_minus_mine: anyhow::Result<Vec<TendermintAddress>> = self
                .network_validators
                .iter()
                .map(|v| v.peering_address())
                .filter(|a| {
                    *a.as_ref().expect("able to get address ref")
                        != v.peering_address()
                            .expect("able to get peering address ref")
                })
                .collect();
            let ips_minus_mine = ips_minus_mine?;
            tracing::debug!(?ips_minus_mine, "Found these peer ips");

            let external_address: Option<TendermintAddress> = v.external_address.as_ref().cloned();
            let mut tm_config = NetworkTendermintConfig::new(
                &node_name,
                ips_minus_mine,
                external_address,
                Some(self.tendermint_rpc_bind),
                Some(self.tendermint_p2p_bind),
            )?;
            if let Some(timeout_commit) = self.tendermint_timeout_commit {
                tm_config.0.consensus.timeout_commit = timeout_commit;
            }
            tm_config.write_config(node_dir, v, &self.genesis)?;
        }
        Ok(())
    }
}

/// Create a new testnet definition, including genesis and at least one
/// validator config. Write all configs to the target testnet dir,
/// defaulting to `~/.shieldd/<chain_id>`.
#[allow(clippy::too_many_arguments)]
pub fn network_generate(
    network_dir: Option<PathBuf>,
    chain_id: &str,
    tendermint_timeout_commit: Option<tendermint::Timeout>,
    epoch_duration: Option<u64>,
    peer_address_template: Option<String>,
    external_addresses: Vec<TendermintAddress>,
    validators_input_file: Option<PathBuf>,
    allocations_input_file: Option<PathBuf>,
    allocation_address: Option<Address>,
    gas_price_simple: Option<u64>,
    compliance_registrar_vk: Vec<VerificationKey<SpendAuth>>,
    tendermint_rpc_bind: SocketAddr,
    tendermint_p2p_bind: SocketAddr,
) -> anyhow::Result<()> {
    tracing::info!(?chain_id, "Generating network config");
    let t = NetworkConfig::generate(
        chain_id,
        network_dir,
        peer_address_template,
        Some(external_addresses),
        allocations_input_file,
        allocation_address,
        validators_input_file,
        tendermint_timeout_commit,
        epoch_duration,
        gas_price_simple,
        compliance_registrar_vk,
        tendermint_rpc_bind,
        tendermint_p2p_bind,
    )?;
    tracing::info!(
        n_validators = t.network_validators.len(),
        chain_id = %t.genesis.chain_id,
        "Writing config files for network"
    );
    t.write_configs()?;
    Ok(())
}

/// Represents initial allocations to the testnet.
#[derive(Debug, Deserialize)]
pub struct NetworkAllocation {
    #[serde(deserialize_with = "string_u128")]
    pub amount: u128,
    pub denom: String,
    pub address: String,
}

impl NetworkAllocation {
    /// Import allocations from a CSV file. The format is simple:
    ///
    ///   amount,denom,address
    ///
    /// Typically these CSV files are generated by Galileo.
    pub fn from_csv(csv_filepath: PathBuf) -> Result<Vec<Allocation>> {
        let allocations_file = File::open(&csv_filepath)
            .with_context(|| format!("cannot open file {csv_filepath:?}"))?;
        Self::from_reader(allocations_file)
    }
    /// Import allocations from a reader object that emits CSV.
    pub fn from_reader(csv_input: impl Read) -> Result<Vec<Allocation>> {
        let mut rdr = csv::Reader::from_reader(csv_input);
        let mut res = vec![];
        for (line, result) in rdr.deserialize().enumerate() {
            let record: NetworkAllocation = result?;
            let record: shielded_pool_genesis::Allocation =
                record.try_into().with_context(|| {
                    format!("invalid allocation in entry {line} of allocations file")
                })?;
            res.push(record);
        }

        if res.is_empty() {
            anyhow::bail!("parsed no entries from allocations input file; is the file valid CSV?");
        }

        Ok(res)
    }
    /// Creates a basic set of genesis [Allocation]s for the provided [Address].
    /// Returns multiple Allocations, so that it's immediately possible to use the DEX,
    /// for basic interactive testing of swap behavior, and compliance system testing.
    /// For more control over precise allocation amounts, use [from_csv].
    pub fn simple(address: Address) -> Vec<Allocation> {
        vec![
            Allocation {
                address: address.clone(),
                raw_denom: "ushieldd".into(),
                // The `ushieldd` base denom is millionths, so `10^6 * n`
                // results in `n` `shieldd` tokens. Split the base allocation
                // across two genesis notes so nonzero-fee base-asset sends can
                // dedicate one note to tx-level fee funding.
                raw_amount: (99_000 * 10u128.pow(6)).into(),
            },
            Allocation {
                address: address.clone(),
                raw_denom: "ushieldd".into(),
                raw_amount: (1_000 * 10u128.pow(6)).into(),
            },
            Allocation {
                address: address.clone(),
                raw_denom: "wtest_usd".into(),
                // 1_000_000 display units (exponent 18)
                raw_amount: (1_000_000u128 * 10u128.pow(18)).into(),
            },
            Allocation {
                address: address.clone(),
                raw_denom: "wregulated_usd".into(),
                // 1_000_000 display units (exponent 18)
                raw_amount: (1_000_000u128 * 10u128.pow(18)).into(),
            },
            Allocation {
                address: address.clone(),
                raw_denom: "unknown_token".into(),
                // 1_000_000 display units (exponent 18)
                raw_amount: (1_000_000u128 * 10u128.pow(18)).into(),
            },
        ]
    }
}

/// Represents testnet validators in configuration files.
#[derive(Deserialize)]
pub struct NetworkValidator {
    pub name: String,
    /// Optional `external_address` field for Tendermint config.
    /// Instructs peers to connect to this node's P2P service
    /// on this address.
    pub external_address: Option<TendermintAddress>,
    pub peer_address_template: Option<String>,
    pub keys: ValidatorKeys,
}

impl NetworkValidator {
    /// Import validator configs from a JSON file.
    pub fn from_json(json_filepath: PathBuf) -> Result<Vec<NetworkValidator>> {
        let validators_file = File::open(&json_filepath)
            .with_context(|| format!("cannot open file {json_filepath:?}"))?;
        Self::from_reader(validators_file)
    }
    /// Import validator configs from a reader object that emits JSON.
    pub fn from_reader(input: impl Read) -> Result<Vec<NetworkValidator>> {
        #[derive(Deserialize)]
        struct RawNetworkValidator {
            pub name: String,
            pub external_address: Option<TendermintAddress>,
            pub peer_address_template: Option<String>,
            pub keys: Option<ValidatorKeys>,
        }

        let raw_validators: Vec<RawNetworkValidator> = serde_json::from_reader(input)?;

        raw_validators
            .into_iter()
            .enumerate()
            .map(|(index, raw)| {
                let keys = match raw.keys {
                    Some(keys) => keys,
                    None => {
                        let mut seed = [0u8; 32];
                        seed[..8].copy_from_slice(&(index as u64).to_le_bytes());
                        ValidatorKeys::from_seed(seed)?
                    }
                };

                Ok(NetworkValidator {
                    name: raw.name,
                    external_address: raw.external_address,
                    peer_address_template: raw.peer_address_template,
                    keys,
                })
            })
            .collect()
    }
    /// Return a URL for Tendermint P2P service for this node.
    ///
    /// In order for the set of genesis validators to communicate with each other,
    /// they must have initial peer information seeded into their Tendermint config files.
    /// If an `external_address` was set, use that. Next, check for a `peer_address_template`.
    /// Finally, fall back to localhost.
    pub fn peering_address(&self) -> anyhow::Result<TendermintAddress> {
        let tm_node_id = node::Id::from(self.keys.node_key_pk.ed25519().expect("ed25519 key"));
        tracing::debug!(?self.name, ?self.external_address, ?self.peer_address_template, "Looking up peering_address");
        let r: TendermintAddress = match &self.external_address {
            // The `external_address` is a TendermintAddress, so unpack as enum to retrieve
            // the host/port info.
            Some(a) => match a {
                TendermintAddress::Tcp {
                    peer_id: _,
                    host,
                    port,
                } => format!("{tm_node_id}@{}:{}", host, port).parse()?,
                // The other enum type is TendermintAddress::Unix, see
                // https://docs.rs/tendermint-config/0.33.0/tendermint_config/index.html
                _ => {
                    anyhow::bail!(
                        "Only TCP format is supported for tendermint addresses: {}",
                        a
                    );
                }
            },
            None => match &self.peer_address_template {
                Some(t) => format!("{tm_node_id}@{t}:26656").parse()?,
                None => format!("{tm_node_id}@127.0.0.1:26656").parse()?,
            },
        };
        Ok(r)
    }

    /// Hardcoded initial state for Tendermint, used for writing configs.
    // Easiest to hardcode since we never change these.
    pub fn initial_state() -> String {
        r#"{
        "height": "0",
        "round": 0,
        "step": 0
    }
    "#
        .to_string()
    }
}

impl NetworkValidator {
    fn consensus_validator(&self) -> anyhow::Result<tendermint::validator::Info> {
        let public_key = self.keys.validator_cons_pk;
        Ok(tendermint::validator::Info {
            address: public_key.into(),
            pub_key: public_key,
            power: 1i64.try_into()?,
            name: (!self.name.is_empty()).then(|| self.name.clone()),
            proposer_priority: 0i64.try_into()?,
        })
    }

    pub fn generate() -> Result<Self> {
        Ok(Self {
            name: "".to_string(),
            external_address: None,
            peer_address_template: None,
            keys: ValidatorKeys::generate()?,
        })
    }
}

impl TryFrom<NetworkAllocation> for shielded_pool_genesis::Allocation {
    type Error = anyhow::Error;

    fn try_from(a: NetworkAllocation) -> anyhow::Result<shielded_pool_genesis::Allocation> {
        Ok(shielded_pool_genesis::Allocation {
            raw_amount: a.amount.into(),
            raw_denom: a.denom.clone(),
            address: Address::from_str(&a.address).with_context(|| {
                format!(
                    "invalid address format in genesis allocations: {}",
                    &a.address
                )
            })?,
        })
    }
}

fn string_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct U128StringVisitor;

    impl<'de> de::Visitor<'de> for U128StringVisitor {
        type Value = u128;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string containing a u128 with optional underscores")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let r = v.replace('_', "");
            r.parse::<u128>().map_err(E::custom)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v as u128)
        }

        fn visit_u128<E>(self, v: u128) -> std::prelude::v1::Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v)
        }
    }

    deserializer.deserialize_any(U128StringVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allocations_from_good_csv() -> anyhow::Result<()> {
        let csv_content = r#"
"amount","denom","address"
"100000","ushieldd","shieldd1u29dhz4vxgnek6a3vzxlejg0l83wegpu7hgs3yphdvljcnnnh89dvs6lc9hxxw94w464t7lh5x36cxnxyx0"
"100000","ushieldd","shieldd1nrvgexa9tq9y242r6s7mqa6qlkfqmu45k9cwyrh66faaj8eupldkw837cjgv8qkfwedsdzmlypseupefutj"
"100000","ushieldd","shieldd1u29dhz4vxgnek6a3vzxlejg0l83wegpu7hgs3yphdvljcnnnh89dvs6lc9hxxw94w464t7lh5x36cxnxyx0"
"100000","ushieldd","shieldd1nrvgexa9tq9y242r6s7mqa6qlkfqmu45k9cwyrh66faaj8eupldkw837cjgv8qkfwedsdzmlypseupefutj"
"#;
        let allos = NetworkAllocation::from_reader(csv_content.as_bytes())?;

        let a1 = &allos[0];
        assert!(a1.raw_denom == "ushieldd");
        assert!(a1.address == Address::from_str("shieldd1u29dhz4vxgnek6a3vzxlejg0l83wegpu7hgs3yphdvljcnnnh89dvs6lc9hxxw94w464t7lh5x36cxnxyx0")?);
        assert!(a1.raw_amount.value() == 100000);

        let a2 = &allos[1];
        assert!(a2.raw_denom == "ushieldd");
        assert!(a2.address == Address::from_str("shieldd1nrvgexa9tq9y242r6s7mqa6qlkfqmu45k9cwyrh66faaj8eupldkw837cjgv8qkfwedsdzmlypseupefutj")?);
        assert!(a2.raw_amount.value() == 100000);

        Ok(())
    }

    #[test]
    fn parse_allocations_from_bad_csv() -> anyhow::Result<()> {
        let csv_content = r#"
"amount","denom","address"\n"100000","ushieldd","shieldd1rqcd3hfvkvc04c4c9vc0ac87lh4y0z8l28k4xp6d0cnd5jc6f6k0neuzp6zdwtpwyfpswtdzv9jzqtpjn5t6wh96pfx3flq2dhqgc42u7c06kj57dl39w2xm6tg0wh4z9uu0qt"\n"100000","ushieldd","shieldd1xq2e9x7uhfzezwunvazdamlxepf4jr5htsuqnzlsahuayyqxjjwg9lk0aytwm6wfj3jy29rv2kdpen57903s8wxv3jmqwj6m6v5jgn6y2cypfd03rke652k8wmavxra7y7yt34"\n"100000","ushieldd","shieldd100zd92fg6x27wc0mlu48cd6phq420u7ep59kzdalg2cq66mjkyl0xr54z0c64gectnj44mv5k2vyjjsz5gyd5gq33a6wnqzvgu2fz7namz7usazsl6p8wza83gcpwt8qrpf98e"\n"100000","ushieldd","shieldd1xap8sgefy9rl2nfvsse0h4y6c25hy2n20ymr5w7hs28m9xemt3tmz88atyulswumc32sv7h937wnfhyct282de66zm75nk6ywq3d4r32p5ju0cnscj2rraesnrxr9lvk8vjqsu"\n
"#;
        let result = NetworkAllocation::from_reader(csv_content.as_bytes());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    /// Generate a config suitable for local testing: no custom address information, no additional
    /// validators at genesis.
    fn generate_devnet_config() -> anyhow::Result<()> {
        let testnet_config = NetworkConfig::generate(
            "test-chain-1234",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            vec![],
            SocketAddr::from_str("0.0.0.0:26657")?,
            SocketAddr::from_str("0.0.0.0:26656")?,
        )?;
        assert_eq!(testnet_config.name, "test-chain-1234");
        assert_eq!(testnet_config.genesis.validators.len(), 2);
        // When no validators_input_file is provided, validators are loaded from
        let shieldd_sdk_app::genesis::AppState::Content(app_state) =
            testnet_config.genesis.app_state
        else {
            panic!("network generation must produce content genesis")
        };
        assert_eq!(
            app_state
                .shielded_pool_content
                .allocations
                .iter()
                .filter(|allocation| allocation.raw_denom.starts_with("udelegation_"))
                .count(),
            0
        );
        Ok(())
    }

    #[test]
    /// Generate a config suitable for a public testnet: custom validators input file,
    /// increasing the default validators from 1 -> 2.
    fn generate_network_config() -> anyhow::Result<()> {
        let ci_validators_filepath =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/network/defaults/validators.json");
        let testnet_config = NetworkConfig::generate(
            "test-chain-4567",
            None,
            Some(String::from("validator.local")),
            None,
            None,
            None,
            Some(ci_validators_filepath),
            None,
            None,
            None,
            vec![],
            SocketAddr::from_str("0.0.0.0:26657")?,
            SocketAddr::from_str("0.0.0.0:26656")?,
        )?;
        assert_eq!(testnet_config.name, "test-chain-4567");
        assert_eq!(testnet_config.genesis.validators.len(), 2);
        let shieldd_sdk_app::genesis::AppState::Content(app_state) =
            testnet_config.genesis.app_state
        else {
            panic!("network generation must produce content genesis")
        };
        assert_eq!(
            app_state
                .shielded_pool_content
                .allocations
                .iter()
                .filter(|allocation| allocation.raw_denom.starts_with("udelegation_"))
                .count(),
            0
        );
        Ok(())
    }
}
