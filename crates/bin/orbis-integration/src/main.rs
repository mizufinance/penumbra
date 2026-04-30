use std::{
    collections::BTreeMap,
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
use penumbra_orbis_client::{NodeInfo, OrbisClient};
use serde::{Deserialize, Serialize};

const NODE1_ENDPOINT: &str = "http://127.0.0.1:50051";
const NODE2_ENDPOINT: &str = "http://127.0.0.1:50052";
const NODE3_ENDPOINT: &str = "http://127.0.0.1:50053";
const NODE1_DIAL_HOST: &str = "node1";
const NODE2_DIAL_HOST: &str = "node2";
const NODE3_DIAL_HOST: &str = "node3";
const ORBIS_NAMESPACE: &str = "orbis";
const ORBIS_RESOURCE: &str = "document";
const ORBIS_PERMISSION: &str = "read";

fn node_endpoint(env_key: &str, default: &str) -> String {
    env::var(env_key).unwrap_or_else(|_| default.to_string())
}

fn node_dial_host(env_key: &str, default: &str) -> String {
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
    /// Run bankD audit-demo setup/scanner/audit commands against an already running local stack.
    AuditDemo {
        #[clap(subcommand)]
        command: AuditDemoCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AuditDemoCommand {
    /// Idempotently register regulated BRL and issuer demo subjects.
    Setup,
    /// Follow Penumbra blocks and continuously import regulated transfer detections.
    Scanner,
    /// Audit one issuer subject by name.
    AuditUser {
        /// Issuer registry subject: Alice, Bob, or Charlie.
        name: String,
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
    tx_hash: String,
    action_index: usize,
    #[serde(default)]
    asset_id: String,
    is_flagged: bool,
}

#[derive(Debug, Deserialize)]
struct AuditEntry {
    height: u64,
    tx_hash: String,
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
        CommandKind::AuditDemo { command } => match command {
            AuditDemoCommand::Setup => AuditDemo::from_env()?.setup().await,
            AuditDemoCommand::Scanner => AuditDemo::from_env()?.scanner(),
            AuditDemoCommand::AuditUser { name } => AuditDemo::from_env()?.audit_user(&name),
        },
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
            &node_dial_host("ORBIS_NODE1_DIAL_HOST", NODE1_DIAL_HOST),
        )?,
        docker_peer_id(
            &info2,
            &node_dial_host("ORBIS_NODE2_DIAL_HOST", NODE2_DIAL_HOST),
        )?,
        docker_peer_id(
            &info3,
            &node_dial_host("ORBIS_NODE3_DIAL_HOST", NODE3_DIAL_HOST),
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
    let (node1_endpoint, node2_endpoint, node3_endpoint) = node_endpoints();
    wait_for_tcp_endpoint(env.get("PENUMBRA_NODE_PD_URL")?, 30, Duration::from_secs(1))?;
    wait_for_tcp_endpoint(&node1_endpoint, 60, Duration::from_secs(2))?;
    wait_for_tcp_endpoint(&node2_endpoint, 60, Duration::from_secs(2))?;
    wait_for_tcp_endpoint(&node3_endpoint, 60, Duration::from_secs(2))?;

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
            &node_dial_host("ORBIS_NODE1_DIAL_HOST", NODE1_DIAL_HOST),
        )?,
        docker_peer_id(
            &info2,
            &node_dial_host("ORBIS_NODE2_DIAL_HOST", NODE2_DIAL_HOST),
        )?,
        docker_peer_id(
            &info3,
            &node_dial_host("ORBIS_NODE3_DIAL_HOST", NODE3_DIAL_HOST),
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
        .map(|entry| (entry.height, entry.tx_hash, entry.action_index))
        .collect::<std::collections::BTreeSet<_>>();
    let detected = scan
        .detected
        .into_iter()
        .filter(|tx_ref| !tx_ref.is_flagged)
        .filter(|tx_ref| {
            refs.contains(&(tx_ref.height, tx_ref.tx_hash.clone(), tx_ref.action_index))
        })
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

#[derive(Debug, Clone)]
struct AuditDemo {
    root: PathBuf,
    demo_dir: PathBuf,
    demo_dir_rel: String,
    status_file: PathBuf,
    state_file: PathBuf,
    issuer_db_rel: String,
    issuer_db_abs: PathBuf,
    scanner_health_file: PathBuf,
    asset: String,
    threshold: String,
    penumbra_grpc: String,
    orbis_endpoint: String,
}

impl AuditDemo {
    const ALICE_PHRASE: &'static str = "wealth flavor believe regret funny network recall kiss grape useless pepper cram hint member few certain unveil rather brick bargain curious require crowd raise";
    const CHARLIE_PHRASE: &'static str = "decorate bright ozone fork gallery riot bus exhaust worth way bone indoor calm squirrel merry zero scheme cotton until shop any excess stage laundry";

    fn from_env() -> Result<Self> {
        let root = env::current_dir().context("failed to resolve current directory")?;
        let demo_dir_rel =
            env::var("DEMO_DIR").unwrap_or_else(|_| ".localnet/audit-demo".to_string());
        let demo_dir = root.join(&demo_dir_rel);
        fs::create_dir_all(demo_dir.join("wallets"))
            .with_context(|| format!("failed to create {}", demo_dir.display()))?;
        let issuer_db_rel = format!("{demo_dir_rel}/issuer-ledger.db");
        Ok(Self {
            status_file: demo_dir.join("status.json"),
            state_file: demo_dir.join("state.json"),
            issuer_db_abs: demo_dir.join("issuer-ledger.db"),
            scanner_health_file: demo_dir.join("scanner-health.json"),
            root,
            demo_dir,
            demo_dir_rel,
            issuer_db_rel,
            asset: env::var("AUDIT_DEMO_ASSET")
                .unwrap_or_else(|_| "transfer/channel-0/ubrl".to_string()),
            threshold: env::var("AUDIT_DEMO_THRESHOLD").unwrap_or_else(|_| "500000000".to_string()),
            penumbra_grpc: env::var("PENUMBRA_GRPC")
                .or_else(|_| env::var("PENUMBRA_NODE_PD_URL"))
                .unwrap_or_else(|_| {
                    let port =
                        env::var("PENUMBRA_PD_GRPC_PORT").unwrap_or_else(|_| "8080".to_string());
                    format!("http://127.0.0.1:{port}")
                }),
            orbis_endpoint: env::var("ORBIS_ENDPOINT")
                .unwrap_or_else(|_| node_endpoint("ORBIS_NODE1_ENDPOINT", NODE1_ENDPOINT)),
        })
    }

    async fn setup(&self) -> Result<()> {
        self.init_state_file()?;
        self.write_status("running", "setup", "Initializing audit setup")?;
        self.init_wallet("alice", Some(Self::ALICE_PHRASE))?;
        self.sync_wallet("alice")?;
        self.setup_asset().await?;
        self.register_subject("Alice", "alice", Some(Self::ALICE_PHRASE), false)?;
        self.register_subject("Bob", "bob", None, true)?;
        self.register_subject("Charlie", "charlie", Some(Self::CHARLIE_PHRASE), true)?;
        self.update_state(|state| {
            state["setup"]["initialized"] = serde_json::Value::Bool(true);
            state["setup"]["updatedAt"] = now_json();
        })?;
        self.refresh_outputs()?;
        self.write_status("complete", "setup", "Audit setup ready")?;
        Ok(())
    }

    fn scanner(&self) -> Result<()> {
        self.init_state_file()?;
        if !self
            .state_value()?
            .pointer("/setup/initialized")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            self.write_health(false, "Audit setup is not ready", None)?;
            bail!("audit setup is not ready");
        }
        let dk = self.issuer_dk()?;
        self.write_health(true, "Scanner running", None)?;
        let refresh_demo = self.clone();
        let refresh_handle = thread::spawn(move || loop {
            let _ = refresh_demo.refresh_outputs();
            let last_height = refresh_demo
                .demo_dir
                .join("scanner-state.json")
                .exists()
                .then(|| {
                    refresh_demo
                        .read_json(refresh_demo.demo_dir.join("scanner-state.json"))
                        .ok()
                })
                .flatten()
                .and_then(|v| v.get("last_height").and_then(serde_json::Value::as_u64));
            let _ = refresh_demo.write_health(true, "Scanner running", last_height);
            thread::sleep(Duration::from_secs(2));
        });
        let _ = refresh_handle.thread().id();

        let status = self
            .pcli_command("alice")
            .args([
                "tx",
                "compliance",
                "scan",
                "--node",
                &self.penumbra_grpc,
                "--dk-hex",
                &dk,
                "--scan-asset-id",
                &self.asset,
                "--output",
                &format!("{}/detected-txs.json", self.demo_dir_rel),
                "--state-file",
                &format!("{}/scanner-state.json", self.demo_dir_rel),
                "--issuer-db",
                &self.issuer_db_rel,
                "--merge-output",
                "--follow",
            ])
            .status()
            .context("failed to run pcli compliance scanner")?;
        self.write_health(false, "Scanner stopped", None)?;
        if status.success() {
            Ok(())
        } else {
            bail!("pcli compliance scanner failed with status {status}")
        }
    }

    fn audit_user(&self, input_name: &str) -> Result<()> {
        self.init_state_file()?;
        let subject = self
            .subject(input_name)?
            .ok_or_else(|| anyhow!("unknown audit subject: {input_name}"))?;
        self.write_status(
            "running",
            "audit-user",
            &format!("Auditing {}", subject.name),
        )?;
        self.refresh_outputs()?;
        let detected_path = self.demo_dir.join("detected-txs.json");
        if !detected_path.exists() {
            fs::write(&detected_path, br#"{"detected":[]}"#)?;
        }
        let detected_json = self.read_json(&detected_path)?;
        let detected = detected_json
            .get("detected")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let ledger = self.ledger_rows()?;
        self.mark_clear_rows_audited(&subject, &ledger)?;
        self.refresh_outputs()?;
        let ledger = self.ledger_rows()?;
        let default_refs = detected
            .iter()
            .filter(|row| {
                !row.get("is_flagged")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
            .filter(|row| {
                row.get("flow_type").and_then(serde_json::Value::as_str) == Some("private_transfer")
            })
            .filter(|row| {
                let height = row.get("height").and_then(serde_json::Value::as_i64);
                let tx_hash = row.get("tx_hash").and_then(serde_json::Value::as_str);
                let action_index = row.get("action_index").and_then(serde_json::Value::as_i64);
                !ledger.iter().any(|ledger_row| {
                    same_ref(ledger_row, height, tx_hash, action_index)
                        && ledger_row_fully_known(ledger_row)
                }) && !ledger.iter().any(|ledger_row| {
                    same_ref(ledger_row, height, tx_hash, action_index)
                        && alias_matches(ledger_row.get("self_alias"), &subject.name)
                        && !ledger_row
                            .get("amount")
                            .unwrap_or(&serde_json::Value::Null)
                            .is_null()
                })
            })
            .cloned()
            .collect::<Vec<_>>();

        if default_refs.is_empty() {
            self.write_status(
                "complete",
                "audit-user",
                &format!("No new transfers to audit for {}", subject.name),
            )?;
            return Ok(());
        }

        let default_input = self
            .demo_dir
            .join(format!("{}-default-input.json", subject.slug));
        self.write_scan_input(&default_input, default_refs)?;
        let default_output = self
            .demo_dir
            .join(format!("{}-default-audit.json", subject.slug));
        self.run_subject_audit(&subject, "default", &default_input, &default_output)?;
        self.update_issuer_db_from_audit(&subject.name, &default_output)?;

        let default_audit = self.read_json_array(&default_output).unwrap_or_default();
        if default_audit.is_empty() {
            fs::write(
                self.demo_dir
                    .join(format!("{}-extension-audit.json", subject.slug)),
                b"[]",
            )?;
            self.refresh_outputs()?;
            self.write_status(
                "complete",
                "audit-user",
                &format!("Audit complete for {}", subject.name),
            )?;
            return Ok(());
        }

        self.refresh_outputs()?;
        let ledger = self.ledger_rows()?;
        let decoded_refs = default_audit
            .iter()
            .filter_map(|row| {
                Some((
                    row.get("height")?.as_i64()?,
                    row.get("tx_hash")?.as_str()?.to_string(),
                    row.get("action_index")?.as_i64()?,
                ))
            })
            .collect::<std::collections::BTreeSet<_>>();
        let extension_refs = detected
            .iter()
            .filter(|row| {
                !row.get("is_flagged")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
            .filter(|row| {
                row.get("flow_type").and_then(serde_json::Value::as_str) == Some("private_transfer")
            })
            .filter(|row| {
                let Some(height) = row.get("height").and_then(serde_json::Value::as_i64) else {
                    return false;
                };
                let Some(tx_hash) = row.get("tx_hash").and_then(serde_json::Value::as_str) else {
                    return false;
                };
                let Some(action_index) =
                    row.get("action_index").and_then(serde_json::Value::as_i64)
                else {
                    return false;
                };
                decoded_refs.contains(&(height, tx_hash.to_string(), action_index))
                    && !ledger.iter().any(|ledger_row| {
                        same_ref(ledger_row, Some(height), Some(tx_hash), Some(action_index))
                            && alias_matches(ledger_row.get("self_alias"), &subject.name)
                            && ledger_row
                                .get("counterparty_alias")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|alias| !alias.is_empty())
                    })
            })
            .cloned()
            .collect::<Vec<_>>();

        let extension_output = self
            .demo_dir
            .join(format!("{}-extension-audit.json", subject.slug));
        if extension_refs.is_empty() {
            fs::write(&extension_output, b"[]")?;
        } else {
            let extension_input = self
                .demo_dir
                .join(format!("{}-extension-input.json", subject.slug));
            self.write_scan_input(&extension_input, extension_refs)?;
            self.run_subject_audit(&subject, "extension", &extension_input, &extension_output)?;
            self.update_issuer_db_from_audit(&subject.name, &extension_output)?;
        }

        self.refresh_outputs()?;
        self.update_state(|state| {
            let audit = serde_json::json!({
                "userSlug": subject.slug,
                "userName": subject.name,
                "at": now_json(),
            });
            push_json_array(&mut state["audits"], audit);
            state["setup"]["updatedAt"] = now_json();
        })?;
        self.write_status(
            "complete",
            "audit-user",
            &format!("Audit complete for {}", subject.name),
        )?;
        Ok(())
    }

    async fn setup_asset(&self) -> Result<()> {
        let state = self.state_value()?;
        if state
            .pointer("/setup/assetRegistered")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            if !self.issuer_db_abs.exists() {
                self.run_pcli(
                    "alice",
                    [
                        "tx",
                        "compliance",
                        "issuer-db",
                        "init",
                        "--db",
                        &self.issuer_db_rel,
                    ],
                )?;
            }
            return Ok(());
        }

        let ring_file = self.demo_dir.join("ring.json");
        setup_ring(&ring_file).await?;
        let ring = self.read_json(&ring_file)?;
        let dk_output = self.capture_pcli("alice", ["tx", "compliance", "generate-dk"])?;
        let dk_hex = parse_key_value_line(&dk_output, "DK (hex): ")?;
        let dk_pub_hex = parse_key_value_line(&dk_output, "DK_pub (hex): ")?;
        self.run_pcli(
            "alice",
            [
                "tx",
                "compliance",
                "register-asset",
                &self.asset,
                "--regulated",
                "--dk-pub-hex",
                &dk_pub_hex,
                "--threshold",
                &self.threshold,
                "--ring-pk-hex",
                required_str(&ring, "ringPkHex")?,
                "--ring-id",
                required_str(&ring, "ringId")?,
                "--policy-id",
                required_str(&ring, "policyId")?,
                "--resource",
                required_str(&ring, "resource")?,
                "--permission",
                required_str(&ring, "permission")?,
            ],
        )?;
        // The registration transaction is confirmed before the next setup step,
        // but the immediately following user-registration transaction can still
        // hit an app-state race in the local stack. Give the chain one block and
        // refresh Alice's view before registering subjects.
        thread::sleep(Duration::from_secs(2));
        self.sync_wallet("alice")?;
        if !self.issuer_db_abs.exists() {
            self.run_pcli(
                "alice",
                [
                    "tx",
                    "compliance",
                    "issuer-db",
                    "init",
                    "--db",
                    &self.issuer_db_rel,
                ],
            )?;
        }
        self.update_state(|state| {
            state["ring"] = ring;
            state["issuer"] = serde_json::json!({
                "dkHex": dk_hex,
                "dkPubHex": dk_pub_hex,
            });
            state["setup"]["assetRegistered"] = serde_json::Value::Bool(true);
            state["setup"]["updatedAt"] = now_json();
        })?;
        Ok(())
    }

    fn register_subject(
        &self,
        name: &str,
        slug: &str,
        phrase: Option<&str>,
        fund_fee: bool,
    ) -> Result<()> {
        if self.subject(slug)?.is_some() {
            return Ok(());
        }
        self.init_wallet(slug, phrase)?;
        self.sync_wallet(slug)?;
        if fund_fee {
            let address = self.address_for(slug, 0)?;
            self.run_pcli(
                "alice",
                ["tx", "transfer", "--to", &address, "50000upenumbra"],
            )?;
            self.sync_wallet(slug)?;
        }
        let address = self.address_for(slug, 0)?;
        self.run_pcli(
            slug,
            [
                "tx",
                "compliance",
                "register-user",
                &self.asset,
                "--address-index",
                "0",
            ],
        )?;
        self.run_pcli(
            "alice",
            [
                "tx",
                "compliance",
                "issuer-db",
                "alias",
                "--db",
                &self.issuer_db_rel,
                "--address",
                &address,
                "--name",
                name,
            ],
        )?;
        let transparent_address = self.address_for_transparent(slug)?;
        self.run_pcli(
            "alice",
            [
                "tx",
                "compliance",
                "issuer-db",
                "alias",
                "--db",
                &self.issuer_db_rel,
                "--address",
                &transparent_address,
                "--name",
                name,
            ],
        )?;
        self.update_state(|state| {
            let user = serde_json::json!({
                "name": name,
                "slug": slug,
                "home": format!("{}/wallets/{}", self.demo_dir_rel, slug),
                "addresses": [{ "index": 0, "address": address }],
                "default": true,
                "createdAt": now_json(),
            });
            let users = state["users"].as_array_mut();
            if let Some(users) = users {
                users.retain(|user| {
                    user.get("slug").and_then(serde_json::Value::as_str) != Some(slug)
                });
                users.push(user);
            } else {
                state["users"] = serde_json::Value::Array(vec![user]);
            }
            state["setup"]["updatedAt"] = now_json();
        })?;
        Ok(())
    }

    fn init_wallet(&self, slug: &str, phrase: Option<&str>) -> Result<()> {
        let home = self.wallet_home_abs(slug);
        fs::create_dir_all(&home)?;
        if home.join("config.toml").exists() {
            return Ok(());
        }
        let mut child = self
            .pcli_command(slug)
            .args(["init", "--grpc-url", &self.penumbra_grpc, "soft-kms"])
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

    fn sync_wallet(&self, slug: &str) -> Result<()> {
        self.run_pcli(slug, ["view", "sync"])
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

    fn address_for_transparent(&self, slug: &str) -> Result<String> {
        let output = self.capture_pcli(slug, ["view", "address", "0", "--transparent"])?;
        Ok(output
            .lines()
            .last()
            .unwrap_or(output.trim())
            .trim()
            .to_string())
    }

    fn run_subject_audit(
        &self,
        subject: &AuditSubject,
        tier: &str,
        input: &Path,
        output: &Path,
    ) -> Result<()> {
        let mut command = Command::new("orbis-audit");
        command
            .current_dir(&self.root)
            .arg("--input")
            .arg(input)
            .arg("--dk-hex")
            .arg(self.issuer_dk()?)
            .arg("--node")
            .arg(&self.penumbra_grpc)
            .arg("--output")
            .arg(output)
            .arg("--timings-json")
            .arg(
                self.demo_dir
                    .join(format!("{}-{tier}-timings.json", subject.slug)),
            )
            .arg("--object-cache")
            .arg(self.demo_dir.join("orbis-object-cache.json"))
            .arg("--tier")
            .arg(tier)
            .arg("--orbis-endpoint")
            .arg(&self.orbis_endpoint)
            .arg("--subject-address")
            .arg(&subject.address);
        for known in self.subjects()? {
            command.arg("--known-address").arg(known.address);
        }
        self.run_orbis_locked(&mut command)
    }

    fn run_orbis_locked(&self, command: &mut Command) -> Result<()> {
        let lock = self.demo_dir.join("orbis-sourcehub.lock");
        while fs::create_dir(&lock).is_err() {
            thread::sleep(Duration::from_secs(1));
        }
        let result = run_command(command);
        let _ = fs::remove_dir(&lock);
        result
    }

    fn update_issuer_db_from_audit(&self, name: &str, audit_file: &Path) -> Result<()> {
        if self
            .read_json_array(audit_file)
            .unwrap_or_default()
            .is_empty()
        {
            return Ok(());
        }
        self.run_pcli(
            "alice",
            [
                "tx",
                "compliance",
                "issuer-db",
                "update",
                "--db",
                &self.issuer_db_rel,
                "--audit-output",
                audit_file.to_str().unwrap_or_default(),
                "--audit-subject",
                name,
            ],
        )
    }

    fn mark_clear_rows_audited(
        &self,
        subject: &AuditSubject,
        ledger: &[serde_json::Value],
    ) -> Result<()> {
        for row in ledger {
            let flow_type = row.get("flow_type").and_then(serde_json::Value::as_str);
            if !matches!(flow_type, Some("shield" | "withdraw")) {
                continue;
            }
            if !alias_matches(row.get("self_alias"), &subject.name) {
                continue;
            }
            let already_audited = row
                .get("audited_subjects")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|subjects| {
                    subjects
                        .iter()
                        .any(|value| value.as_str() == Some(subject.name.as_str()))
                });
            if already_audited {
                continue;
            }

            let Some(height) = row.get("height").and_then(serde_json::Value::as_i64) else {
                continue;
            };
            let Some(tx_hash) = row.get("tx_hash").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(action_index) = row.get("action_index").and_then(serde_json::Value::as_i64)
            else {
                continue;
            };

            self.run_pcli(
                "alice",
                vec![
                    "tx".to_string(),
                    "compliance".to_string(),
                    "issuer-db".to_string(),
                    "mark-audited".to_string(),
                    "--db".to_string(),
                    self.issuer_db_rel.clone(),
                    "--height".to_string(),
                    height.to_string(),
                    "--tx-hash".to_string(),
                    tx_hash.to_string(),
                    "--action-index".to_string(),
                    action_index.to_string(),
                    "--audit-subject".to_string(),
                    subject.name.clone(),
                ],
            )?;
        }
        Ok(())
    }

    fn refresh_outputs(&self) -> Result<()> {
        self.init_state_file()?;
        if !self.scanner_health_file.exists() {
            self.write_health(false, "Scanner not started", None)?;
        }
        let detected = self.demo_dir.join("detected-txs.json");
        if !detected.exists() {
            fs::write(&detected, br#"{"detected":[]}"#)?;
        }
        let ledger_path = self.demo_dir.join("ledger.json");
        if self.issuer_db_abs.exists() {
            match self.capture_pcli(
                "alice",
                [
                    "tx",
                    "compliance",
                    "issuer-db",
                    "show",
                    "--json",
                    "--db",
                    &self.issuer_db_rel,
                ],
            ) {
                Ok(output) => fs::write(&ledger_path, output)?,
                Err(_) if !ledger_path.exists() => fs::write(&ledger_path, b"[]")?,
                Err(error) => return Err(error),
            }
        } else if !ledger_path.exists() {
            fs::write(&ledger_path, b"[]")?;
        }

        let detected_json = self.read_json(&detected)?;
        let ledger_json = self.read_json_array(&ledger_path).unwrap_or_default();
        let scanner_json = self.read_json(&self.scanner_health_file)?;
        self.update_state(|state| {
            let detected_rows = detected_json
                .get("detected")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let flagged = detected_rows
                .iter()
                .filter(|row| row.get("is_flagged").and_then(serde_json::Value::as_bool).unwrap_or(false))
                .count();
            let audited = ledger_json
                .iter()
                .filter(|row| {
                    !row.get("is_flagged").and_then(serde_json::Value::as_bool).unwrap_or(false)
                        && !row.get("amount").unwrap_or(&serde_json::Value::Null).is_null()
                })
                .count();
            state["scan"] = serde_json::json!({
                "detected": detected_rows,
                "scanTime": detected_json.pointer("/scan_info/scan_time").cloned().unwrap_or(serde_json::Value::Null),
                "detectedCount": detected_rows.len(),
                "flaggedCount": flagged,
                "auditedCount": audited,
            });
            state["scanner"] = scanner_json;
            state["ledgerRows"] = serde_json::Value::Array(ledger_json);
            state["setup"]["updatedAt"] = now_json();
        })
    }

    fn write_scan_input(&self, path: &Path, refs: Vec<serde_json::Value>) -> Result<()> {
        fs::write(
            path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "scan_info": {},
                "detected": refs,
            }))?,
        )
        .with_context(|| format!("failed to write {}", path.display()))
    }

    fn write_status(&self, state: &str, step: &str, message: &str) -> Result<()> {
        self.write_json(
            &self.status_file,
            serde_json::json!({
                "state": state,
                "step": step,
                "message": message,
                "updatedAt": now_json(),
            }),
        )
    }

    fn write_health(&self, running: bool, message: &str, last_height: Option<u64>) -> Result<()> {
        self.write_json(
            &self.scanner_health_file,
            serde_json::json!({
                "running": running,
                "message": message,
                "lastHeight": last_height,
                "updatedAt": now_json(),
            }),
        )
    }

    fn init_state_file(&self) -> Result<()> {
        if self.state_file.exists() {
            return Ok(());
        }
        self.write_json(
            &self.state_file,
            serde_json::json!({
                "setup": {
                    "initialized": false,
                    "assetRegistered": false,
                    "updatedAt": now_json(),
                },
                "endpoints": {
                    "penumbraGrpc": self.penumbra_grpc,
                },
                "asset": {
                    "denom": self.asset,
                    "threshold": self.threshold,
                },
                "users": [],
                "scan": {
                    "detected": [],
                    "detectedCount": 0,
                    "flaggedCount": 0,
                    "auditedCount": 0,
                },
                "scanner": {
                    "running": false,
                    "lastHeight": null,
                    "updatedAt": null,
                },
                "ledgerRows": [],
                "audits": [],
                "events": [],
            }),
        )
    }

    fn state_value(&self) -> Result<serde_json::Value> {
        self.read_json(&self.state_file)
    }

    fn update_state<F>(&self, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut serde_json::Value),
    {
        let mut state = self.state_value()?;
        mutate(&mut state);
        self.write_json(&self.state_file, state)
    }

    fn issuer_dk(&self) -> Result<String> {
        self.state_value()?
            .pointer("/issuer/dkHex")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("issuer DK is missing; run audit-demo setup first"))
    }

    fn subjects(&self) -> Result<Vec<AuditSubject>> {
        Ok(self
            .state_value()?
            .get("users")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(AuditSubject::from_value)
            .collect())
    }

    fn subject(&self, name_or_slug: &str) -> Result<Option<AuditSubject>> {
        let slug = slugify(name_or_slug);
        Ok(self.subjects()?.into_iter().find(|subject| {
            subject.slug == slug || subject.name.eq_ignore_ascii_case(name_or_slug)
        }))
    }

    fn ledger_rows(&self) -> Result<Vec<serde_json::Value>> {
        self.read_json_array(self.demo_dir.join("ledger.json"))
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
            .env("HOME", "/home/penumbra")
            .env("PENUMBRA_PCLI_HOME", self.wallet_home_abs(slug));
        command
    }

    fn run_pcli<I, S>(&self, slug: &str, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = collect_args(args);
        eprintln!("orbis-integration audit-demo: pcli {}", render_args(&args));
        run_command(self.pcli_command(slug).args(args))
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

    fn read_json<P: AsRef<Path>>(&self, path: P) -> Result<serde_json::Value> {
        let path = path.as_ref();
        serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))
    }

    fn read_json_array<P: AsRef<Path>>(&self, path: P) -> Result<Vec<serde_json::Value>> {
        let value = self.read_json(path)?;
        Ok(value.as_array().cloned().unwrap_or_default())
    }

    fn write_json(&self, path: &Path, value: serde_json::Value) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&value)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct AuditSubject {
    name: String,
    slug: String,
    address: String,
}

impl AuditSubject {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            name: value.get("name")?.as_str()?.to_string(),
            slug: value.get("slug")?.as_str()?.to_string(),
            address: value
                .get("addresses")?
                .as_array()?
                .first()?
                .get("address")?
                .as_str()?
                .to_string(),
        })
    }
}

fn now_json() -> serde_json::Value {
    serde_json::Value::String(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string()),
    )
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.chars().take(48).collect()
}

fn required_str<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("missing {key}"))
}

fn push_json_array(target: &mut serde_json::Value, item: serde_json::Value) {
    if let Some(array) = target.as_array_mut() {
        array.push(item);
    } else {
        *target = serde_json::Value::Array(vec![item]);
    }
}

fn same_ref(
    row: &serde_json::Value,
    height: Option<i64>,
    tx_hash: Option<&str>,
    action_index: Option<i64>,
) -> bool {
    row.get("height").and_then(serde_json::Value::as_i64) == height
        && row.get("tx_hash").and_then(serde_json::Value::as_str) == tx_hash
        && row.get("action_index").and_then(serde_json::Value::as_i64) == action_index
}

fn alias_matches(alias: Option<&serde_json::Value>, name: &str) -> bool {
    alias
        .and_then(serde_json::Value::as_str)
        .is_some_and(|alias| alias == name || alias.starts_with(&format!("{name} ")))
}

fn ledger_row_fully_known(row: &serde_json::Value) -> bool {
    let has_amount = !row
        .get("amount")
        .unwrap_or(&serde_json::Value::Null)
        .is_null();
    let has_self = row
        .get("self_alias")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|alias| !alias.is_empty());
    let has_counterparty = row
        .get("counterparty_alias")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|alias| !alias.is_empty());

    match row.get("flow_type").and_then(serde_json::Value::as_str) {
        Some("shield" | "withdraw") => has_amount && has_self,
        _ => has_amount && has_self && has_counterparty,
    }
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
        let tmp = env::var("COMPLIANCE_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(|_| root.join("tmp"));
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

        let peer = docker_peer_id(&info, "node1").expect("peer id should rewrite");
        assert_eq!(peer, "peerid@node1:4001");
    }

    #[test]
    fn node_dial_host_can_be_env_configured() {
        let key = "ORBIS_NODE_DIAL_HOST_TEST";
        env::set_var(key, "custom-node-1");
        let host = node_dial_host(key, "node1");
        env::remove_var(key);
        assert_eq!(host, "custom-node-1");
    }
}
