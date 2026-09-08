use std::{
    env,
    ffi::OsStr,
    fs,
    io::Write,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use command::{collect_args, command_output, format_captured_output, render_args};
use demo_auth::dkg_signer;
use demo_config::{
    node_dial_host, vera_client, OrbisEndpoints, NODE1_DIAL_HOST, NODE2_DIAL_HOST, NODE3_DIAL_HOST,
    ORBIS_PERMISSION, ORBIS_POLICY_MARSHAL_TYPE_YAML, ORBIS_POLICY_YAML, ORBIS_RESOURCE,
    ORBIS_RING_POLICY_RESOURCE,
};
use demo_state::{
    now_string, read_json, write_json, AuditDemoState, RingState, StatusDocument, UserState,
};
use orbis_common::blockchain::{orbis::WhitelistTarget, VeraClient};
use serde::Deserialize;
use shieldd_orbis_client::{NodeInfo, OrbisClient};

mod command;
mod demo_auth;
mod demo_config;
mod demo_state;

#[derive(Parser, Debug)]
#[clap(
    name = "orbis-integration",
    about = "Typed Shieldd <-> Orbis integration flow"
)]
struct Args {
    #[clap(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Set up an Orbis ring and policy for an already running Orbis stack.
    SetupRing {
        /// Path to write ring/policy details as JSON.
        #[clap(long)]
        output_json: PathBuf,
    },
    /// Initialize the bankD Orbis legal-audit demo against a running local stack.
    AuditDemo {
        #[clap(subcommand)]
        command: AuditDemoCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AuditDemoCommand {
    /// Initialize deterministic demo identities and an Orbis/Vera ring only.
    PocSetup,
}

#[derive(Debug)]
struct RepoPaths {
    orbis_runtime_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        CommandKind::SetupRing { output_json } => {
            let repo = RepoPaths::discover()?;
            let endpoints = repo.orbis_endpoints()?;
            setup_ring(&output_json, &endpoints).await
        }
        CommandKind::AuditDemo { command } => {
            let repo = RepoPaths::discover()?;
            let endpoints = repo.orbis_endpoints()?;
            let demo = AuditDemo::from_env(endpoints)?;
            match command {
                AuditDemoCommand::PocSetup => demo.poc_setup().await,
            }
        }
    }
}

async fn setup_ring(output_json: &Path, endpoints: &OrbisEndpoints) -> Result<()> {
    let (node1_endpoint, node2_endpoint, node3_endpoint) = endpoints.node_endpoints();
    for endpoint in [&node1_endpoint, &node2_endpoint, &node3_endpoint] {
        wait_for_tcp_endpoint(endpoint, 60, Duration::from_secs(2))?;
    }

    let node1 = OrbisClient::new(node1_endpoint)?;
    let node2 = OrbisClient::new(node2_endpoint)?;
    let node3 = OrbisClient::new(node3_endpoint)?;

    let info1 = wait_for_node_info(&node1, "node1").await?;
    let info2 = wait_for_node_info(&node2, "node2").await?;
    let info3 = wait_for_node_info(&node3, "node3").await?;

    let vera = vera_client(endpoints).await?;
    let (node_keys, node_routes) = orbis_node_routes(&info1, &info2, &info3)?;
    let policy_id = OrbisClient::add_policy(
        &vera,
        ORBIS_POLICY_YAML,
        ORBIS_POLICY_MARSHAL_TYPE_YAML,
        ORBIS_RESOURCE,
        ORBIS_PERMISSION,
    )
    .await?;
    OrbisClient::register_object(&vera, &policy_id, ORBIS_RING_POLICY_RESOURCE, &policy_id).await?;
    authorize_orbis_nodes_for_policy(&vera, &node_routes, &policy_id).await?;
    let dkg_signer = dkg_signer();
    let dkg = node1
        .start_dkg(2, &node_keys, &vera, &policy_id, &dkg_signer)
        .await?;
    eprintln!(
        "orbis-integration: DKG session started for ring {}: {} ({})",
        dkg.ring_id, dkg.session_id, dkg.status
    );
    eprintln!("orbis-integration: DKG message: {}", dkg.message);

    let ring = wait_for_ring(&vera, &dkg.ring_id).await?;
    let output = RingState {
        ring_pk_hex: ring.ring_pk_hex,
        ring_id: ring.ring_id,
        policy_id,
        resource: ORBIS_RESOURCE.to_string(),
        permission: ORBIS_PERMISSION.to_string(),
    };

    if let Some(parent) = output_json.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(output_json, serde_json::to_string_pretty(&output)?)
        .with_context(|| format!("failed to write {}", output_json.display()))?;
    println!("{}", output_json.display());
    Ok(())
}

fn wait_for_tcp(addr: &str, attempts: usize, interval: Duration) -> Result<()> {
    for _ in 0..attempts {
        if TcpStream::connect(addr).is_ok() {
            return Ok(());
        }
        thread::sleep(interval);
    }
    bail!("timed out waiting for TCP service at {addr}");
}

fn wait_for_tcp_endpoint(endpoint: &str, attempts: usize, interval: Duration) -> Result<()> {
    let without_scheme = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    let addr = without_scheme
        .split('/')
        .next()
        .filter(|addr| !addr.is_empty())
        .ok_or_else(|| anyhow!("invalid endpoint: {endpoint}"))?;
    wait_for_tcp(addr, attempts, interval)
}

async fn wait_for_node_info(client: &OrbisClient, label: &str) -> Result<NodeInfo> {
    let mut last_error = None;
    for _ in 0..60 {
        match client.query_node_info().await {
            Ok(info) => return Ok(info),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("timed out waiting for {label} info endpoint")))
        .with_context(|| format!("timed out waiting for {label} info endpoint"))
}

fn orbis_node_routes(
    info1: &NodeInfo,
    info2: &NodeInfo,
    info3: &NodeInfo,
) -> Result<(Vec<String>, Vec<(String, String)>)> {
    let routes = vec![
        (
            info1.node_key.clone(),
            docker_peer_id(
                info1,
                &node_dial_host("ORBIS_NODE1_DIAL_HOST", NODE1_DIAL_HOST),
            )?,
        ),
        (
            info2.node_key.clone(),
            docker_peer_id(
                info2,
                &node_dial_host("ORBIS_NODE2_DIAL_HOST", NODE2_DIAL_HOST),
            )?,
        ),
        (
            info3.node_key.clone(),
            docker_peer_id(
                info3,
                &node_dial_host("ORBIS_NODE3_DIAL_HOST", NODE3_DIAL_HOST),
            )?,
        ),
    ];
    let node_keys = routes
        .iter()
        .map(|(node_key, _)| node_key.clone())
        .collect();
    Ok((node_keys, routes))
}

async fn authorize_orbis_nodes_for_policy(
    client: &VeraClient,
    routes: &[(String, String)],
    policy_id: &str,
) -> Result<()> {
    for (node_key, peer_id) in routes {
        wait_for_vera_node_info(client, node_key).await?;
        whitelist_orbis_node_for_policy(client, node_key, peer_id, policy_id).await?;
    }
    Ok(())
}

async fn wait_for_vera_node_info(client: &VeraClient, node_key: &str) -> Result<()> {
    let mut last_error = None;
    // 180 * 2s = 6 min. On-chain NodeInfo registration lags node startup by the
    // full funder round-trip (wait for keys -> wait for first block -> fund ->
    // node observes its funded account -> registers). The old self-funding image
    // registered immediately, so 2 min was enough; the prebuilt-image + funder
    // topology needs a larger budget or this races and flakes.
    for _ in 0..180 {
        match client.orbis_read_node_info(node_key).await {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {
                last_error = Some(anyhow!("Orbis NodeInfo {node_key} not found on Vera"));
            }
            Err(error) => {
                last_error = Some(anyhow!("failed to read Orbis NodeInfo {node_key}: {error}"));
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(last_error
        .unwrap_or_else(|| anyhow!("timed out waiting for Orbis NodeInfo {node_key} on Vera")))
    .with_context(|| format!("timed out waiting for Orbis NodeInfo {node_key} on Vera"))
}

/// A single controller-issued NodeInfo transaction. Vera split the former
/// `MsgUpdateNodeInfo` into per-field messages, so refreshing a node's
/// docker-routed peer id and whitelisting its policy are now distinct txs.
enum NodeTx<'a> {
    UpdatePeerId { peer_id: &'a str },
    WhitelistPolicy { policy_id: &'a str },
}

/// Each Orbis node self-registers its NodeInfo on spinup. Here the controller
/// (test account) rewrites the peer id to the docker-routed value and whitelists
/// the freshly created ACP policy onto the node.
async fn whitelist_orbis_node_for_policy(
    client: &VeraClient,
    node_key: &str,
    peer_id: &str,
    policy_id: &str,
) -> Result<()> {
    send_orbis_node_tx(client, node_key, NodeTx::UpdatePeerId { peer_id }).await?;
    send_orbis_node_tx(client, node_key, NodeTx::WhitelistPolicy { policy_id }).await
}

async fn send_orbis_node_tx(client: &VeraClient, node_key: &str, tx: NodeTx<'_>) -> Result<()> {
    let mut attempt = 0u32;
    loop {
        let (label, outcome) = match &tx {
            NodeTx::UpdatePeerId { peer_id } => (
                "update peer id",
                client.orbis_update_node_peer_id(node_key, peer_id).await,
            ),
            NodeTx::WhitelistPolicy { policy_id } => (
                "whitelist policy",
                client
                    .orbis_add_node_to_whitelist(
                        node_key,
                        WhitelistTarget::PolicyId(policy_id.to_string()),
                    )
                    .await,
            ),
        };
        match outcome {
            Ok(result) if result.code == 0 => return Ok(()),
            Ok(result) => {
                if attempt < 30 && is_transient_vera_tx_error(&result.log) {
                    attempt += 1;
                    let _ = client.resync_nonce().await;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                bail!(
                    "Orbis NodeInfo {label} tx failed for {node_key}: code={} log={}",
                    result.code,
                    result.log
                )
            }
            Err(error) => {
                let msg = error.to_string();
                if attempt < 30 && is_transient_vera_tx_error(&msg) {
                    attempt += 1;
                    let _ = client.resync_nonce().await;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                return Err(anyhow!(
                    "failed to {label} on Orbis NodeInfo {node_key}: {error}"
                ));
            }
        }
    }
}

fn is_transient_vera_tx_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("sequence mismatch")
        || lower.contains("account not found")
        || lower.contains("issuedidfromaccountaddr")
}

async fn wait_for_ring(
    client: &VeraClient,
    ring_id: &str,
) -> Result<shieldd_orbis_client::RingInfo> {
    let mut last_error = None;
    for _ in 0..60 {
        match OrbisClient::get_ring(client, ring_id).await {
            Ok(ring) => return Ok(ring),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("timed out waiting for Orbis ring {ring_id}")))
        .with_context(|| format!("timed out waiting for Orbis ring {ring_id}"))
}

fn docker_peer_id(info: &NodeInfo, dial_host: &str) -> Result<String> {
    let (peer_id, socket_addr) = info
        .p2p_address
        .split_once('@')
        .ok_or_else(|| anyhow!("unexpected p2p address format: {}", info.p2p_address))?;
    let (_, port) = socket_addr
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("missing port in p2p address: {}", info.p2p_address))?;
    Ok(format!("{peer_id}@{dial_host}:{port}"))
}

#[derive(Debug, Clone)]
struct AuditDemo {
    root: PathBuf,
    demo_dir: PathBuf,
    demo_dir_rel: String,
    status_file: PathBuf,
    state_file: PathBuf,
    orbis_endpoints: OrbisEndpoints,
}

impl AuditDemo {
    const ALICE_PHRASE: &'static str = "wealth flavor believe regret funny network recall kiss grape useless pepper cram hint member few certain unveil rather brick bargain curious require crowd raise";
    const CHARLIE_PHRASE: &'static str = "comfort ten front cycle churn burger oak absent rice ice urge result art couple benefit cabbage frequent obscure hurry trick segment cool job debate";

    async fn poc_setup(&self) -> Result<()> {
        self.init_state_file()?;
        self.write_status(
            "running",
            "poc-setup",
            "Initializing demo identities and Orbis/Vera ring",
        )?;
        for (name, slug, phrase) in [
            ("Alice", "alice", Some(Self::ALICE_PHRASE)),
            ("Bob", "bob", None),
            ("Charlie", "charlie", Some(Self::CHARLIE_PHRASE)),
        ] {
            self.init_wallet(slug, phrase)?;
            let address = self.address_for(slug, 0)?;
            let mut user = UserState::new(name, slug, self.wallet_home_rel(slug), address)?;
            if slug == "charlie" {
                user.add_address(1, self.address_for(slug, 1)?)?;
            }
            self.update_state(|state| {
                state.users.retain(|user| user.slug != slug);
                state.users.push(user);
            })?;
        }
        let ring_file = self.demo_dir.join("ring.json");
        setup_ring(&ring_file, &self.orbis_endpoints).await?;
        let ring = self.read_json::<RingState>(&ring_file)?;
        self.update_state(|state| {
            state.ring = Some(ring);
            state.setup.initialized = true;
            state.setup.updated_at = Some(now_string());
        })?;
        self.write_status(
            "complete",
            "poc-setup",
            "Demo identities and Orbis/Vera ring ready",
        )
    }

    fn from_env(orbis_endpoints: OrbisEndpoints) -> Result<Self> {
        let root = env::current_dir().context("failed to resolve current directory")?;
        let demo_dir_rel =
            env::var("DEMO_DIR").unwrap_or_else(|_| ".localnet/audit-demo".to_string());
        let demo_dir = root.join(&demo_dir_rel);
        fs::create_dir_all(demo_dir.join("wallets"))
            .with_context(|| format!("failed to create {}", demo_dir.display()))?;
        Ok(Self {
            status_file: demo_dir.join("status.json"),
            state_file: demo_dir.join("state.json"),
            root,
            demo_dir,
            demo_dir_rel,
            orbis_endpoints,
        })
    }

    fn init_wallet(&self, slug: &str, phrase: Option<&str>) -> Result<()> {
        let home = self.wallet_home_abs(slug);
        fs::create_dir_all(&home)?;
        if home.join("config.toml").exists() {
            return Ok(());
        }
        let mut child = self
            .pcli_command(slug)
            .args(["init", "soft-kms"])
            .arg(if phrase.is_some() {
                "import-phrase"
            } else {
                "generate"
            })
            .stdin(Stdio::piped())
            .spawn()
            .context("failed to start pcli wallet init")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(phrase.unwrap_or("").as_bytes())?;
            stdin.write_all(b"\n")?;
        }
        let status = child
            .wait()
            .context("failed to wait for pcli wallet init")?;
        if status.success() {
            Ok(())
        } else {
            bail!("pcli wallet init failed with status {status}")
        }
    }

    fn address_for(&self, slug: &str, index: u64) -> Result<String> {
        let output = self.capture_pcli(slug, ["view", "address", &index.to_string()])?;
        Ok(output
            .lines()
            .last()
            .unwrap_or(output.trim())
            .trim()
            .to_string())
    }

    fn write_status(&self, state: &str, step: &str, message: &str) -> Result<()> {
        write_json(
            &self.status_file,
            &StatusDocument::new(state, step, message),
        )
    }

    fn init_state_file(&self) -> Result<()> {
        if self.state_file.exists() {
            return Ok(());
        }
        write_json(&self.state_file, &AuditDemoState::new())
    }

    fn state(&self) -> Result<AuditDemoState> {
        self.read_json(&self.state_file)
    }

    fn update_state<F>(&self, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut AuditDemoState),
    {
        let mut state = self.state()?;
        mutate(&mut state);
        write_json(&self.state_file, &state)
    }

    fn wallet_home_rel(&self, slug: &str) -> String {
        format!("{}/wallets/{slug}", self.demo_dir_rel)
    }

    fn wallet_home_abs(&self, slug: &str) -> PathBuf {
        self.root.join(self.wallet_home_rel(slug))
    }

    fn pcli_command(&self, slug: &str) -> Command {
        let mut command = Command::new("pcli");
        command
            .current_dir(&self.root)
            .env("HOME", "/home/shieldd")
            .env("SHIELDD_PCLI_HOME", self.wallet_home_abs(slug));
        command
    }

    fn capture_pcli<I, S>(&self, slug: &str, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = collect_args(args);
        eprintln!("orbis-integration audit-demo: pcli {}", render_args(&args));
        let output = command_output(self.pcli_command(slug).args(args))?;
        if !output.status.success() {
            bail!(
                "pcli command failed with status {}:\n{}",
                output.status,
                format_captured_output(&output)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn read_json<T: for<'de> Deserialize<'de>>(&self, path: impl AsRef<Path>) -> Result<T> {
        read_json(path.as_ref())
    }
}

impl RepoPaths {
    fn discover() -> Result<Self> {
        let root = env::var_os("BANKD_REPO_ROOT")
            .map(PathBuf::from)
            .or_else(|| env::current_dir().ok().and_then(find_repo_root))
            .or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../..")
                    .canonicalize()
                    .ok()
            })
            .context("failed to locate repo root")?;
        let tmp = env::var("COMPLIANCE_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(|_| root.join("tmp"));
        fs::create_dir_all(&tmp).with_context(|| format!("failed to create {}", tmp.display()))?;

        Ok(Self {
            orbis_runtime_file: tmp.join("orbis-runtime.json"),
        })
    }

    fn orbis_endpoints(&self) -> Result<OrbisEndpoints> {
        OrbisEndpoints::load_or_env(&self.orbis_runtime_file)
    }
}

fn find_repo_root(start: PathBuf) -> Option<PathBuf> {
    let ancestors = start.ancestors().collect::<Vec<_>>();
    ancestors
        .iter()
        .find(|candidate| {
            candidate
                .join("deployments/orbis/docker-compose.yml")
                .is_file()
        })
        .or_else(|| {
            ancestors
                .iter()
                .find(|candidate| candidate.join("infra/docker-compose.yml").is_file())
        })
        .copied()
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_peer_id_rewrites_host_only() {
        let info = NodeInfo {
            public_address: "vera1abc".to_string(),
            node_key: "node-key".to_string(),
            peer_id: "peerid".to_string(),
            p2p_address: "peerid@127.0.0.1:4001".to_string(),
        };

        let peer = docker_peer_id(&info, "node1").expect("peer id should rewrite");
        assert_eq!(peer, "peerid@node1:4001");
    }

    #[test]
    fn repo_discovery_prefers_the_shieldd_checkout_over_a_bankd_parent() {
        let shieldd = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("Shieldd checkout should exist");
        assert_eq!(find_repo_root(shieldd.clone()), Some(shieldd));
    }
}
