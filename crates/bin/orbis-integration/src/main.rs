use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use penumbra_orbis_client::{NodeInfo, OrbisClient};
use serde::{Deserialize, Serialize};

const NODE1_ENDPOINT: &str = "http://127.0.0.1:50051";
const NODE2_ENDPOINT: &str = "http://127.0.0.1:50052";
const NODE3_ENDPOINT: &str = "http://127.0.0.1:50053";
const NODE1_CONTAINER: &str = "orbis-integration-node-1";
const NODE2_CONTAINER: &str = "orbis-integration-node-2";
const NODE3_CONTAINER: &str = "orbis-integration-node-3";
const ORBIS_NAMESPACE: &str = "orbis";
const ORBIS_RESOURCE: &str = "document";
const ORBIS_PERMISSION: &str = "read";

fn node_endpoint(env_key: &str, default: &str) -> String {
    env::var(env_key).unwrap_or_else(|_| default.to_string())
}

fn node_container(env_key: &str, default: &str) -> String {
    env::var(env_key).unwrap_or_else(|_| default.to_string())
}

fn node_endpoints() -> (String, String, String) {
    (
        node_endpoint("ORBIS_NODE1_ENDPOINT", NODE1_ENDPOINT),
        node_endpoint("ORBIS_NODE2_ENDPOINT", NODE2_ENDPOINT),
        node_endpoint("ORBIS_NODE3_ENDPOINT", NODE3_ENDPOINT),
    )
}

#[derive(Parser, Debug)]
#[clap(
    name = "orbis-integration",
    about = "Typed Penumbra <-> Orbis integration flow"
)]
struct Args {
    #[clap(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Run the full bring-up, seed, verify, and teardown flow.
    Run {
        /// Keep the Penumbra and Orbis stacks running if the flow fails.
        #[clap(long)]
        keep_on_fail: bool,
    },
    /// Seed an already running Penumbra + Orbis stack.
    Seed,
    /// Run the read-only verification phase against a seeded stack.
    Verify,
    /// Set up an Orbis ring and policy for an already running Orbis stack.
    SetupRing {
        /// Path to write ring/policy details as JSON.
        #[clap(long)]
        output_json: PathBuf,
    },
}

#[derive(Debug)]
struct RepoPaths {
    root: PathBuf,
    tmp: PathBuf,
    env_file: PathBuf,
    ring_info_file: PathBuf,
    issuer_dk_file: PathBuf,
    detected_file: PathBuf,
    issuer_db_file: PathBuf,
    orbis_audit_bin: PathBuf,
    pcli_bin: PathBuf,
}

#[derive(Debug)]
struct DemoEnv {
    values: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ScanOutput {
    detected: Vec<DetectedTxRef>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DetectedTxRef {
    height: u64,
    action_index: usize,
    is_flagged: bool,
}

#[derive(Debug, Deserialize)]
struct AuditEntry {
    height: u64,
    action_index: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RingSetupOutput {
    ring_pk_hex: String,
    ring_id: String,
    policy_id: String,
    resource: String,
    permission: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        CommandKind::Run { keep_on_fail } => {
            let repo = RepoPaths::discover()?;
            run_full_flow(&repo, keep_on_fail).await
        }
        CommandKind::Seed => {
            let repo = RepoPaths::discover()?;
            seed(&repo).await
        }
        CommandKind::Verify => {
            let repo = RepoPaths::discover()?;
            verify(&repo).await
        }
        CommandKind::SetupRing { output_json } => setup_ring(&output_json).await,
    }
}

async fn setup_ring(output_json: &Path) -> Result<()> {
    let (node1_endpoint, node2_endpoint, node3_endpoint) = node_endpoints();
    for endpoint in [&node1_endpoint, &node2_endpoint, &node3_endpoint] {
        wait_for_tcp_endpoint(endpoint, 60, Duration::from_secs(2))?;
    }

    let node1 = OrbisClient::new(node1_endpoint);
    let node2 = OrbisClient::new(node2_endpoint);
    let node3 = OrbisClient::new(node3_endpoint);

    let info1 = wait_for_node_info(&node1, "node1").await?;
    let info2 = wait_for_node_info(&node2, "node2").await?;
    let info3 = wait_for_node_info(&node3, "node3").await?;

    node1.register_bulletin_namespace(ORBIS_NAMESPACE).await?;
    for info in [&info1, &info2, &info3] {
        node1
            .add_bulletin_collaborator(ORBIS_NAMESPACE, &info.public_address)
            .await?;
    }

    let peer_ids = vec![
        docker_peer_id(
            &info1,
            &node_container("ORBIS_NODE1_CONTAINER", NODE1_CONTAINER),
        )?,
        docker_peer_id(
            &info2,
            &node_container("ORBIS_NODE2_CONTAINER", NODE2_CONTAINER),
        )?,
        docker_peer_id(
            &info3,
            &node_container("ORBIS_NODE3_CONTAINER", NODE3_CONTAINER),
        )?,
    ];
    let dkg = node1.start_dkg(2, &peer_ids).await?;
    eprintln!(
        "orbis-integration: DKG session started: {} ({})",
        dkg.session_id, dkg.status
    );
    eprintln!("orbis-integration: DKG message: {}", dkg.message);

    let ring = wait_for_latest_ring(&node1).await?;
    let policy_id = node1.add_policy().await?;
    let output = RingSetupOutput {
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

async fn run_full_flow(repo: &RepoPaths, keep_on_fail: bool) -> Result<()> {
    let mut started_penumbra = false;
    let mut started_orbis = false;
    let result = async {
        run_script(repo, "scripts/penumbra-up.sh")?;
        started_penumbra = true;

        run_script_with_args(repo, "scripts/orbis-stack.sh", &["up"])?;
        started_orbis = true;

        seed(repo).await?;
        verify(repo).await?;
        Result::<()>::Ok(())
    }
    .await;

    if result.is_err() {
        let _ = capture_orbis_logs(repo);
        if keep_on_fail {
            eprintln!("orbis-integration: preserving Penumbra and Orbis stacks for debugging");
            return result;
        }
    }
    if started_orbis {
        let _ = run_script_with_args(repo, "scripts/orbis-stack.sh", &["down"]);
    }
    if started_penumbra {
        let _ = run_script(repo, "scripts/penumbra-down.sh");
    }

    result
}

async fn seed(repo: &RepoPaths) -> Result<()> {
    let env = load_required_env(
        &repo.env_file,
        "run `just orbis-integration-up` before `just orbis-integration-seed`",
    )?;
    wait_for_tcp("127.0.0.1:8080", 30, Duration::from_secs(1))?;
    wait_for_tcp("127.0.0.1:50051", 60, Duration::from_secs(2))?;
    wait_for_tcp("127.0.0.1:50052", 60, Duration::from_secs(2))?;
    wait_for_tcp("127.0.0.1:50053", 60, Duration::from_secs(2))?;

    let (node1_endpoint, node2_endpoint, node3_endpoint) = node_endpoints();
    let node1 = OrbisClient::new(node1_endpoint);
    let node2 = OrbisClient::new(node2_endpoint);
    let node3 = OrbisClient::new(node3_endpoint);

    let info1 = wait_for_node_info(&node1, "node1").await?;
    let info2 = wait_for_node_info(&node2, "node2").await?;
    let info3 = wait_for_node_info(&node3, "node3").await?;

    node1.register_bulletin_namespace(ORBIS_NAMESPACE).await?;
    for info in [&info1, &info2, &info3] {
        node1
            .add_bulletin_collaborator(ORBIS_NAMESPACE, &info.public_address)
            .await?;
    }

    let peer_ids = vec![
        docker_peer_id(&info1, NODE1_CONTAINER)?,
        docker_peer_id(&info2, NODE2_CONTAINER)?,
        docker_peer_id(&info3, NODE3_CONTAINER)?,
    ];
    let dkg = node1.start_dkg(2, &peer_ids).await?;
    eprintln!(
        "orbis-integration: DKG session started: {} ({})",
        dkg.session_id, dkg.status
    );
    eprintln!("orbis-integration: DKG message: {}", dkg.message);
    let ring = wait_for_latest_ring(&node1).await?;
    let policy_id = node1.add_policy().await?;
    fs::write(
        &repo.ring_info_file,
        format!(
            "RING_PK={}\nRING_ID={}\nRING_PEER_IDS={},{},{}\nORBIS_POLICY_ID={}\nORBIS_RESOURCE={}\nORBIS_PERMISSION={}\n",
            ring.ring_pk_hex,
            ring.ring_id,
            info1.peer_id,
            info2.peer_id,
            info3.peer_id,
            policy_id,
            ORBIS_RESOURCE,
            ORBIS_PERMISSION
        ),
    )
    .with_context(|| format!("failed to write {}", repo.ring_info_file.display()))?;

    let dk_output = capture_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "generate-dk",
        ],
    )?;
    let regulated_dk = parse_key_value_line(&dk_output, "DK (hex): ")?;
    let regulated_dk_pub = parse_key_value_line(&dk_output, "DK_pub (hex): ")?;
    fs::write(
        &repo.issuer_dk_file,
        format!("REGULATED_DK={regulated_dk}\nREGULATED_DK_PUB={regulated_dk_pub}\n"),
    )
    .with_context(|| format!("failed to write {}", repo.issuer_dk_file.display()))?;

    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "register-asset",
            "regulated_usd",
            "--regulated",
            "--dk-pub-hex",
            &regulated_dk_pub,
            "--threshold",
            "500000000000000000000",
            "--ring-pk-hex",
            &ring.ring_pk_hex,
            "--ring-id",
            &ring.ring_id,
            "--policy-id",
            &policy_id,
            "--resource",
            ORBIS_RESOURCE,
            "--permission",
            ORBIS_PERMISSION,
        ],
    )?;
    sync_wallets(repo, &env, &["ALICE_HOME", "BOB_HOME", "CHARLIE_HOME"])?;

    let alice_address_1 = capture_pcli(
        repo,
        &env,
        ["--home", env.get("ALICE_HOME")?, "view", "address", "1"],
    )?
    .trim()
    .to_string();

    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "register-user",
            "regulated_usd",
        ],
    )?;
    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "transfer",
            "--to",
            &alice_address_1,
            "1000000upenumbra",
        ],
    )?;
    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "register-user",
            "regulated_usd",
            "--address-index",
            "1",
        ],
    )?;
    for who in ["BOB_HOME", "CHARLIE_HOME"] {
        run_pcli(
            repo,
            &env,
            [
                "--home",
                env.get(who)?,
                "tx",
                "compliance",
                "register-user",
                "regulated_usd",
            ],
        )?;
    }
    sync_wallets(
        repo,
        &env,
        &[
            "ALICE_HOME",
            "BOB_HOME",
            "CHARLIE_HOME",
            "UNREGISTERED_HOME",
        ],
    )?;

    let alice_split_note = capture_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "view",
            "notes",
            "--asset",
            "regulated_usd",
            "--largest",
            "--commitment-only",
        ],
    )?
    .trim()
    .to_string();
    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "split",
            "--note-commitment",
            &alice_split_note,
            "400regulated_usd",
            "300regulated_usd",
            "600regulated_usd",
            "998700regulated_usd",
        ],
    )?;
    sync_wallets(repo, &env, &["ALICE_HOME", "BOB_HOME", "CHARLIE_HOME"])?;

    run_transfer(
        repo,
        &env,
        "ALICE_HOME",
        "400regulated_usd",
        env.get("BOB_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "ALICE_HOME",
        "300regulated_usd",
        env.get("CHARLIE_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "BOB_HOME",
        "50regulated_usd",
        env.get("ALICE_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "CHARLIE_HOME",
        "40regulated_usd",
        env.get("ALICE_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "BOB_HOME",
        "100regulated_usd",
        env.get("CHARLIE_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "CHARLIE_HOME",
        "80regulated_usd",
        env.get("BOB_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "BOB_HOME",
        "60regulated_usd",
        env.get("ALICE_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "CHARLIE_HOME",
        "30regulated_usd",
        env.get("ALICE_ADDRESS")?,
    )?;
    run_transfer(
        repo,
        &env,
        "ALICE_HOME",
        "600regulated_usd",
        env.get("BOB_ADDRESS")?,
    )?;
    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("BOB_HOME")?,
            "tx",
            "consolidate",
            "regulated_usd",
            "--family",
            "2x1",
        ],
    )?;
    sync_wallets(repo, &env, &["ALICE_HOME", "BOB_HOME", "CHARLIE_HOME"])?;

    let unregistered = command_output(pcli_command(repo, &env).args([
        "--home",
        env.get("ALICE_HOME")?,
        "tx",
        "transfer",
        "--to",
        env.get("UNREGISTERED_ADDRESS")?,
        "10regulated_usd",
    ]))?;
    if unregistered.status.success() {
        bail!("transfer to unregistered user unexpectedly succeeded");
    }

    run_transfer(
        repo,
        &env,
        "ALICE_HOME",
        "1000test_usd",
        env.get("BOB_ADDRESS")?,
    )?;
    eprintln!("orbis-integration: seed phase completed");
    Ok(())
}

async fn verify(repo: &RepoPaths) -> Result<()> {
    let env = load_required_env(
        &repo.env_file,
        "run `just orbis-integration-up` before `just orbis-integration-verify`",
    )?;
    let ring_info = load_required_env(
        &repo.ring_info_file,
        "run `just orbis-integration-seed` before `just orbis-integration-verify`",
    )?;
    let issuer = load_required_env(
        &repo.issuer_dk_file,
        "run `just orbis-integration-seed` before `just orbis-integration-verify`",
    )?;
    let (node1_endpoint, node2_endpoint, node3_endpoint) = node_endpoints();
    let node1 = OrbisClient::new(node1_endpoint);
    let node2 = OrbisClient::new(node2_endpoint);
    let node3 = OrbisClient::new(node3_endpoint);
    let current_peer_ids = format!(
        "{},{},{}",
        wait_for_node_info(&node1, "node1").await?.peer_id,
        wait_for_node_info(&node2, "node2").await?.peer_id,
        wait_for_node_info(&node3, "node3").await?.peer_id
    );
    if ring_info.get("RING_PEER_IDS")? != current_peer_ids {
        bail!("ring is stale: saved peer IDs do not match current Orbis nodes");
    }

    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "scan",
            "--dk-hex",
            issuer.get("REGULATED_DK")?,
            "--scan-asset-id",
            "regulated_usd",
            "--node",
            env.get("PENUMBRA_NODE_PD_URL")?,
            "--output",
            repo.detected_file.to_str().unwrap(),
        ],
    )?;

    let scan: ScanOutput = serde_json::from_slice(
        &fs::read(&repo.detected_file)
            .with_context(|| format!("failed to read {}", repo.detected_file.display()))?,
    )
    .context("failed to parse detected scan output")?;
    let flagged = scan.detected.iter().filter(|tx| tx.is_flagged).count();
    eprintln!(
        "orbis-integration: detected {} transfer entries ({} flagged)",
        scan.detected.len(),
        flagged
    );

    let _ = fs::remove_file(&repo.issuer_db_file);
    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "issuer-db",
            "init",
            "--db",
            repo.issuer_db_file.to_str().unwrap(),
        ],
    )?;
    run_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "issuer-db",
            "import",
            "--db",
            repo.issuer_db_file.to_str().unwrap(),
            "--scan-output",
            repo.detected_file.to_str().unwrap(),
            "--dk-hex",
            issuer.get("REGULATED_DK")?,
            "--node",
            env.get("PENUMBRA_NODE_PD_URL")?,
        ],
    )?;
    for (name, key) in [
        ("Alice", "ALICE_ADDRESS"),
        ("Bob", "BOB_ADDRESS"),
        ("Charlie", "CHARLIE_ADDRESS"),
    ] {
        run_pcli(
            repo,
            &env,
            [
                "--home",
                env.get("ALICE_HOME")?,
                "tx",
                "compliance",
                "issuer-db",
                "alias",
                "--db",
                repo.issuer_db_file.to_str().unwrap(),
                "--address",
                env.get(key)?,
                "--name",
                name,
            ],
        )?;
    }

    for (user_name, address_key) in [("Alice", "ALICE_ADDRESS"), ("Bob", "BOB_ADDRESS")] {
        let default_audit_file = repo
            .tmp
            .join(format!("{}-audit.json", user_name.to_lowercase()));
        run_orbis_audit(
            repo,
            &env,
            &issuer,
            &repo.detected_file,
            &default_audit_file,
            "default",
            user_name,
            env.get(address_key)?,
        )?;
        update_issuer_db_from_audit(repo, &env, user_name, &default_audit_file)?;

        let extension_input = repo
            .tmp
            .join(format!("{}-ext-input.json", user_name.to_lowercase()));
        write_extension_input(&repo.detected_file, &default_audit_file, &extension_input)?;
        if count_detected_refs(&extension_input)? == 0 {
            eprintln!(
                "orbis-integration: skipping {user_name} extension audit; default decoded no refs"
            );
            continue;
        }

        let extension_audit_file = repo
            .tmp
            .join(format!("{}-ext-audit.json", user_name.to_lowercase()));
        run_orbis_audit(
            repo,
            &env,
            &issuer,
            &extension_input,
            &extension_audit_file,
            "extension",
            user_name,
            env.get(address_key)?,
        )?;
        update_issuer_db_from_audit(repo, &env, user_name, &extension_audit_file)?;
    }

    let show = capture_pcli(
        repo,
        &env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "issuer-db",
            "show",
            "--db",
            repo.issuer_db_file.to_str().unwrap(),
        ],
    )?;
    println!("{show}");
    eprintln!("orbis-integration: verify phase completed");
    Ok(())
}

fn run_orbis_audit(
    repo: &RepoPaths,
    env: &DemoEnv,
    issuer: &DemoEnv,
    input: &Path,
    output: &Path,
    tier: &str,
    user_name: &str,
    subject_address: &str,
) -> Result<()> {
    run_command(
        Command::new(&repo.orbis_audit_bin)
            .current_dir(&repo.root)
            .arg("--input")
            .arg(input)
            .arg("--dk-hex")
            .arg(issuer.get("REGULATED_DK")?)
            .arg("--node")
            .arg(env.get("PENUMBRA_NODE_PD_URL")?)
            .arg("--output")
            .arg(output)
            .arg("--object-cache")
            .arg(repo.tmp.join("orbis-audit-object-cache.json"))
            .arg("--tier")
            .arg(tier)
            .arg("--subject-address")
            .arg(subject_address)
            .arg("--known-address")
            .arg(env.get("ALICE_ADDRESS")?)
            .arg("--known-address")
            .arg(env.get("BOB_ADDRESS")?)
            .arg("--known-address")
            .arg(env.get("CHARLIE_ADDRESS")?)
            .arg("--timings-json")
            .arg(repo.tmp.join(format!(
                "{}-{}-timings.json",
                user_name.to_lowercase(),
                tier
            )))
            .arg("--orbis-endpoint")
            .arg(node_endpoint("ORBIS_NODE1_ENDPOINT", NODE1_ENDPOINT)),
    )
}

fn update_issuer_db_from_audit(
    repo: &RepoPaths,
    env: &DemoEnv,
    user_name: &str,
    audit_file: &Path,
) -> Result<()> {
    let audit_count = count_json_array(audit_file)?;
    if audit_count == 0 {
        return Ok(());
    }
    run_pcli(
        repo,
        env,
        [
            "--home",
            env.get("ALICE_HOME")?,
            "tx",
            "compliance",
            "issuer-db",
            "update",
            "--db",
            repo.issuer_db_file.to_str().unwrap(),
            "--audit-output",
            audit_file.to_str().unwrap(),
            "--audit-subject",
            user_name,
        ],
    )
}

fn write_extension_input(
    detected_file: &Path,
    default_audit_file: &Path,
    output: &Path,
) -> Result<()> {
    let scan: ScanOutput = serde_json::from_slice(
        &fs::read(detected_file)
            .with_context(|| format!("failed to read {}", detected_file.display()))?,
    )?;
    let audit_entries: Vec<AuditEntry> = serde_json::from_slice(
        &fs::read(default_audit_file)
            .with_context(|| format!("failed to read {}", default_audit_file.display()))?,
    )?;
    let refs = audit_entries
        .into_iter()
        .map(|entry| (entry.height, entry.action_index))
        .collect::<std::collections::BTreeSet<_>>();
    let detected = scan
        .detected
        .into_iter()
        .filter(|tx_ref| !tx_ref.is_flagged)
        .filter(|tx_ref| refs.contains(&(tx_ref.height, tx_ref.action_index)))
        .collect::<Vec<_>>();
    let output_json = serde_json::json!({
        "scan_info": {},
        "detected": detected,
    });
    fs::write(output, serde_json::to_vec_pretty(&output_json)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

fn count_detected_refs(path: &Path) -> Result<usize> {
    let scan: ScanOutput = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )?;
    Ok(scan.detected.len())
}

fn capture_orbis_logs(repo: &RepoPaths) -> Result<()> {
    let output = command_output(
        Command::new(repo.root.join("scripts/orbis-stack.sh"))
            .current_dir(&repo.root)
            .arg("logs"),
    )?;
    let mut logs = output.stdout;
    logs.extend_from_slice(&output.stderr);
    fs::write(repo.tmp.join("orbis-docker.log"), logs)
        .with_context(|| "failed to write tmp/orbis-docker.log".to_string())?;
    Ok(())
}

fn sync_wallets(repo: &RepoPaths, env: &DemoEnv, homes: &[&str]) -> Result<()> {
    for home in homes {
        run_pcli(repo, env, ["--home", env.get(home)?, "view", "sync"])?;
    }
    Ok(())
}

fn run_transfer(
    repo: &RepoPaths,
    env: &DemoEnv,
    home_key: &str,
    value: &str,
    to: &str,
) -> Result<()> {
    run_pcli(
        repo,
        env,
        [
            "--home",
            env.get(home_key)?,
            "tx",
            "transfer",
            "--to",
            to,
            value,
        ],
    )?;
    sync_wallets(repo, env, &["ALICE_HOME", "BOB_HOME", "CHARLIE_HOME"])?;
    Ok(())
}

fn run_pcli<I, S>(repo: &RepoPaths, env: &DemoEnv, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = collect_args(args);
    eprintln!("orbis-integration: running pcli {}", render_args(&args));
    run_command(pcli_command(repo, env).args(&args))
}

fn capture_pcli<I, S>(repo: &RepoPaths, env: &DemoEnv, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = collect_args(args);
    eprintln!("orbis-integration: running pcli {}", render_args(&args));
    let output = command_output(pcli_command(repo, env).args(&args))?;
    if !output.status.success() {
        bail!(
            "pcli command failed with status {}:\n{}",
            output.status,
            format_captured_output(&output)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn pcli_command<'a>(repo: &'a RepoPaths, env: &'a DemoEnv) -> Command {
    let mut command = Command::new(&repo.pcli_bin);
    command.current_dir(&repo.root);
    for (key, value) in &env.values {
        command.env(key, value);
    }
    command
}

fn run_script(repo: &RepoPaths, script: &str) -> Result<()> {
    run_script_with_args(repo, script, &[])
}

fn run_script_with_args(repo: &RepoPaths, script: &str, args: &[&str]) -> Result<()> {
    let script_path = repo.root.join(script);
    eprintln!(
        "orbis-integration: running {} {}",
        script_path.display(),
        render_args(args)
    );
    let mut command = Command::new(script_path);
    command.current_dir(&repo.root);
    command.args(args);
    run_command(&mut command)
}

fn run_command(command: &mut Command) -> Result<()> {
    let description = format!("{command:?}");
    let status = command
        .status()
        .with_context(|| format!("failed to run {description}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("command failed with status {status}: {description}")
    }
}

fn command_output(command: &mut Command) -> Result<std::process::Output> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {:?}", command))
}

fn collect_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect()
}

fn render_args<S>(args: &[S]) -> String
where
    S: AsRef<str>,
{
    args.iter()
        .map(|arg| shell_escape(arg.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape(arg: &str) -> String {
    if arg
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._:-".contains(ch))
    {
        arg.to_string()
    } else {
        format!("{arg:?}")
    }
}

fn format_captured_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("stdout:\n{stdout}\n\nstderr:\n{stderr}"),
        (false, true) => format!("stdout:\n{stdout}"),
        (true, false) => format!("stderr:\n{stderr}"),
        (true, true) => String::from("<no captured output>"),
    }
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

async fn wait_for_latest_ring(client: &OrbisClient) -> Result<penumbra_orbis_client::RingInfo> {
    let mut last_error = None;
    for _ in 0..60 {
        match client.get_latest_ring().await {
            Ok(ring) => return Ok(ring),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("timed out waiting for Orbis ring publication")))
        .context("timed out waiting for Orbis ring publication")
}

fn parse_key_value_line(output: &str, prefix: &str) -> Result<String> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix))
        .map(str::to_string)
        .ok_or_else(|| anyhow!("failed to find '{prefix}' in command output"))
}

fn docker_peer_id(info: &NodeInfo, container_name: &str) -> Result<String> {
    let (peer_id, socket_addr) = info
        .p2p_address
        .split_once('@')
        .ok_or_else(|| anyhow!("unexpected p2p address format: {}", info.p2p_address))?;
    let (_, port) = socket_addr
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("missing port in p2p address: {}", info.p2p_address))?;
    Ok(format!("{peer_id}@{container_name}:{port}"))
}

fn count_json_array(path: &Path) -> Result<usize> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    value
        .as_array()
        .map(|items| items.len())
        .ok_or_else(|| anyhow!("expected JSON array in {}", path.display()))
}

fn load_required_env(path: &Path, hint: &str) -> Result<DemoEnv> {
    if !path.exists() {
        bail!("missing {}: {hint}", path.display());
    }
    DemoEnv::load(path)
}

impl RepoPaths {
    fn discover() -> Result<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .context("failed to locate repo root")?;
        let tmp = root.join("tmp");
        fs::create_dir_all(&tmp).with_context(|| format!("failed to create {}", tmp.display()))?;

        Ok(Self {
            env_file: tmp.join("compliance-demo.env"),
            ring_info_file: tmp.join("ring-info.env"),
            issuer_dk_file: tmp.join("issuer-dk.env"),
            detected_file: tmp.join("detected_txs.json"),
            issuer_db_file: tmp.join("issuer-ledger.db"),
            orbis_audit_bin: root.join("target/release/orbis-audit"),
            pcli_bin: root.join("target/release/pcli"),
            root,
            tmp,
        })
    }
}

impl DemoEnv {
    fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut values = BTreeMap::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let normalized = trimmed.strip_prefix("export ").unwrap_or(trimmed);
            let (key, value) = normalized
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid env line in {}: {}", path.display(), trimmed))?;
            let value = value.trim().trim_matches('"').to_string();
            values.insert(key.to_string(), value);
        }
        Ok(Self { values })
    }

    fn get(&self, key: &str) -> Result<&str> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("missing {key} in environment"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_peer_id_rewrites_host_only() {
        let info = NodeInfo {
            public_address: "sourcehub1abc".to_string(),
            peer_id: "peerid".to_string(),
            p2p_address: "peerid@127.0.0.1:4001".to_string(),
        };

        let peer = docker_peer_id(&info, "orbis-node-1").expect("peer id should rewrite");
        assert_eq!(peer, "peerid@orbis-node-1:4001");
    }
}
