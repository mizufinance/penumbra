use std::env;

use anyhow::{anyhow, Result};
use orbis_common::blockchain::{ChainConfig, SourceHubClient, TxSigner, TEST_ACCOUNT_HEX_KEY};

pub const NODE1_ENDPOINT: &str = "http://127.0.0.1:50051";
pub const NODE2_ENDPOINT: &str = "http://127.0.0.1:50052";
pub const NODE3_ENDPOINT: &str = "http://127.0.0.1:50053";
pub const NODE1_DIAL_HOST: &str = "node1";
pub const NODE2_DIAL_HOST: &str = "node2";
pub const NODE3_DIAL_HOST: &str = "node3";

pub use penumbra_orbis_client::ORBIS_NAMESPACE;
pub const ORBIS_RESOURCE: &str = "document";
pub const ORBIS_PERMISSION: &str = "read";
pub const ORBIS_POLICY_MARSHAL_TYPE_YAML: i32 = 1;
pub const ORBIS_POLICY_YAML: &str = r#"
name: test-policy
resources:
  - name: document
    relations:
      - name: creator
        types:
          - actor
      - name: reader
        types:
          - actor
    permissions:
      - name: read
        expr: creator + reader
      - name: write
        expr: creator
"#;

pub const DEFAULT_COMPLIANCE_DEV_REGISTRAR_SK_HEX: &str =
    "0100000000000000000000000000000000000000000000000000000000000000";
pub const DEFAULT_COMPLIANCE_DEV_REGISTRAR_VK_HEX: &str =
    "0800000000000000000000000000000000000000000000000000000000000000";
pub const DEFAULT_COMPLIANCE_DEV_AUTHORITY_SK_HEX: &str =
    "0200000000000000000000000000000000000000000000000000000000000000";
pub const DEFAULT_COMPLIANCE_DEV_AUTHORITY_VK_HEX: &str =
    "b2ecf9b9082d6306538be73b0d6ee741141f3222152da78685d6596efc8c1506";
pub const DEFAULT_COMPLIANCE_GRANT_VALID_UNTIL_UNIX: &str = "4102444800";

fn env_or_default(env_key: &str, default: &str) -> String {
    match env::var(env_key) {
        Ok(s) if !s.is_empty() => s,
        _ => default.to_string(),
    }
}

pub fn node_endpoint(env_key: &str, default: &str) -> String {
    env_or_default(env_key, default)
}

pub fn node_dial_host(env_key: &str, default: &str) -> String {
    env_or_default(env_key, default)
}

pub fn node_endpoints() -> (String, String, String) {
    (
        node_endpoint("ORBIS_NODE1_ENDPOINT", NODE1_ENDPOINT),
        node_endpoint("ORBIS_NODE2_ENDPOINT", NODE2_ENDPOINT),
        node_endpoint("ORBIS_NODE3_ENDPOINT", NODE3_ENDPOINT),
    )
}

fn sourcehub_chain_config() -> ChainConfig {
    ChainConfig::builder()
        .chain_id(env::var("ORBIS_SOURCEHUB_CHAIN_ID").ok())
        .rpc_url(sourcehub_url(
            "ORBIS_SOURCEHUB_RPC",
            "ORBIS_SOURCEHUB_RPC_PORT",
        ))
        .rest_url(sourcehub_url(
            "ORBIS_SOURCEHUB_REST",
            "ORBIS_SOURCEHUB_REST_PORT",
        ))
        .grpc_url(sourcehub_url(
            "ORBIS_SOURCEHUB_GRPC",
            "ORBIS_SOURCEHUB_GRPC_PORT",
        ))
        .denom(env::var("ORBIS_SOURCEHUB_DENOM").ok())
        .build()
}

fn sourcehub_url(url_key: &str, port_key: &str) -> Option<String> {
    env::var(url_key)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            env::var(port_key)
                .ok()
                .filter(|s| !s.is_empty())
                .map(|port| format!("http://127.0.0.1:{port}"))
        })
}

pub async fn sourcehub_client() -> Result<SourceHubClient> {
    let config = sourcehub_chain_config();
    let signer = TxSigner::from_hex_key(TEST_ACCOUNT_HEX_KEY, config.clone())
        .map_err(|e| anyhow!("failed to create demo SourceHub signer: {}", e))?;
    SourceHubClient::with_signer(config, signer)
        .await
        .map_err(|e| anyhow!("failed to create signed SourceHub client: {}", e))
}

pub fn compliance_dev_env() -> Vec<(&'static str, String)> {
    vec![
        (
            "COMPLIANCE_DEV_REGISTRAR_SK_HEX",
            process_env_or_default(
                "COMPLIANCE_DEV_REGISTRAR_SK_HEX",
                DEFAULT_COMPLIANCE_DEV_REGISTRAR_SK_HEX,
            ),
        ),
        (
            "COMPLIANCE_DEV_REGISTRAR_VK_HEX",
            process_env_or_default(
                "COMPLIANCE_DEV_REGISTRAR_VK_HEX",
                DEFAULT_COMPLIANCE_DEV_REGISTRAR_VK_HEX,
            ),
        ),
        (
            "COMPLIANCE_DEV_AUTHORITY_SK_HEX",
            process_env_or_default(
                "COMPLIANCE_DEV_AUTHORITY_SK_HEX",
                DEFAULT_COMPLIANCE_DEV_AUTHORITY_SK_HEX,
            ),
        ),
        (
            "COMPLIANCE_DEV_AUTHORITY_VK_HEX",
            process_env_or_default(
                "COMPLIANCE_DEV_AUTHORITY_VK_HEX",
                DEFAULT_COMPLIANCE_DEV_AUTHORITY_VK_HEX,
            ),
        ),
    ]
}

pub fn process_env_or_default(key: &str, default: &str) -> String {
    env_or_default(key, default)
}

#[cfg(test)]
mod tests {
    use super::node_dial_host;
    use std::env;

    #[test]
    fn node_dial_host_can_be_env_configured() {
        let key = "ORBIS_NODE_DIAL_HOST_TEST";
        env::set_var(key, "custom-node-1");
        let host = node_dial_host(key, "node1");
        env::remove_var(key);
        assert_eq!(host, "custom-node-1");
    }

    #[test]
    fn node_dial_host_empty_env_uses_default() {
        let key = "ORBIS_NODE_DIAL_HOST_EMPTY_TEST";
        env::set_var(key, "");
        let host = node_dial_host(key, "node1");
        env::remove_var(key);
        assert_eq!(host, "node1");
    }
}
