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
use serde::Deserialize;

const NODE1_ENDPOINT: &str = "http://127.0.0.1:50051";
const NODE2_ENDPOINT: &str = "http://127.0.0.1:50052";
const NODE3_ENDPOINT: &str = "http://127.0.0.1:50053";
const NODE1_CONTAINER: &str = "orbis-integration-node-1";
const NODE2_CONTAINER: &str = "orbis-integration-node-2";
const NODE3_CONTAINER: &str = "orbis-integration-node-3";
const ORBIS_NAMESPACE: &str = "orbis";

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
    Run,
    Seed,
    Verify,
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

#[derive(Debug, Deserialize)]
struct DetectedTxRef {
    is_flagged: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let repo = RepoPaths::discover()?;

    match args.command {
        CommandKind::Run => run_full_flow(&repo).await,
        CommandKind::Seed => seed(&repo).await,
        CommandKind::Verify => verify(&repo).await,
    }
}

async fn run_full_flow(repo: &RepoPaths) -> Result<()> {
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
    let env = DemoEnv::load(&repo.env_file)?;
    ensure_demo_gnark_libs(repo)?;
    wait_for_tcp("127.0.0.1:8080", 30, Duration::from_secs(1))?;
    wait_for_tcp("127.0.0.1:50051", 60, Duration::from_secs(2))?;
    wait_for_tcp("127.0.0.1:50052", 60, Duration::from_secs(2))?;
    wait_for_tcp("127.0.0.1:50053", 60, Duration::from_secs(2))?;

    let node1 = OrbisClient::new(NODE1_ENDPOINT);
    let node2 = OrbisClient::new(NODE2_ENDPOINT);
    let node3 = OrbisClient::new(NODE3_ENDPOINT);

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
    fs::write(
        &repo.ring_info_file,
        format!(
            "RING_PK={}\nRING_ID={}\nRING_PEER_IDS={},{},{}\n",
            ring.ring_pk_hex, ring.ring_id, info1.peer_id, info2.peer_id, info3.peer_id
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
    let env = DemoEnv::load(&repo.env_file)?;
    let ring_info = DemoEnv::load(&repo.ring_info_file)?;
    let issuer = DemoEnv::load(&repo.issuer_dk_file)?;
    let node1 = OrbisClient::new(NODE1_ENDPOINT);
    let node2 = OrbisClient::new(NODE2_ENDPOINT);
    let node3 = OrbisClient::new(NODE3_ENDPOINT);
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

    for (tier, suffix) in [("default", "audit"), ("extension", "ext-audit")] {
        for (user_name, address_key) in [("Alice", "ALICE_ADDRESS"), ("Bob", "BOB_ADDRESS")] {
            let audit_file = repo
                .tmp
                .join(format!("{}-{}.json", user_name.to_lowercase(), suffix));
            run_command(
                Command::new(&repo.orbis_audit_bin)
                    .current_dir(&repo.root)
                    .arg("--input")
                    .arg(&repo.detected_file)
                    .arg("--dk-hex")
                    .arg(issuer.get("REGULATED_DK")?)
                    .arg("--node")
                    .arg(env.get("PENUMBRA_NODE_PD_URL")?)
                    .arg("--output")
                    .arg(&audit_file)
                    .arg("--tier")
                    .arg(tier)
                    .arg("--sender-address")
                    .arg(env.get(address_key)?)
                    .arg("--known-address")
                    .arg(env.get("ALICE_ADDRESS")?)
                    .arg("--known-address")
                    .arg(env.get("BOB_ADDRESS")?)
                    .arg("--known-address")
                    .arg(env.get("CHARLIE_ADDRESS")?)
                    .arg("--orbis-endpoint")
                    .arg(NODE1_ENDPOINT)
                    .arg("--ring-pk-hex")
                    .arg(ring_info.get("RING_PK")?)
                    .arg("--ring-id")
                    .arg(ring_info.get("RING_ID")?),
            )?;

            let audit_count = count_json_array(&audit_file)?;
            if audit_count > 0 {
                run_pcli(
                    repo,
                    &env,
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
                )?;
            }
        }
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

fn capture_orbis_logs(repo: &RepoPaths) -> Result<()> {
    let output = command_output(
        Command::new(repo.root.join("scripts/orbis-stack.sh"))
            .current_dir(&repo.root)
            .arg("logs"),
    )?;
    fs::write(repo.tmp.join("orbis-docker.log"), output.stdout)
        .with_context(|| "failed to write tmp/orbis-docker.log".to_string())?;
    Ok(())
}

fn ensure_demo_gnark_libs(repo: &RepoPaths) -> Result<()> {
    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let gnark_dir = repo.root.join("tools/gnark");
    let expected = [
        gnark_dir.join(format!("libpenumbra_gnark_transfer.{ext}")),
        gnark_dir.join(format!("libpenumbra_gnark_split.{ext}")),
        gnark_dir.join(format!("libpenumbra_gnark_consolidate.{ext}")),
        gnark_dir.join(format!("libpenumbra_gnark_shielded_ics20_withdrawal.{ext}")),
    ];
    if expected.iter().all(|path| path.is_file()) {
        return Ok(());
    }

    let builds = [
        ("./cmd/splitlib", format!("libpenumbra_gnark_split.{ext}")),
        (
            "./cmd/transferlib",
            format!("libpenumbra_gnark_transfer.{ext}"),
        ),
        (
            "./cmd/consolidatelib",
            format!("libpenumbra_gnark_consolidate.{ext}"),
        ),
        (
            "./cmd/shieldedics20withdrawallib",
            format!("libpenumbra_gnark_shielded_ics20_withdrawal.{ext}"),
        ),
    ];
    for (pkg, output) in builds {
        run_command(
            Command::new("go")
                .current_dir(&gnark_dir)
                .env("CGO_ENABLED", "1")
                .arg("build")
                .arg("-buildmode=c-shared")
                .arg("-o")
                .arg(output)
                .arg(pkg),
        )?;
    }
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
            String::from_utf8_lossy(&output.stderr)
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
    let output = command_output(command)?;
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "command failed with status {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
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
    if arg.chars().all(|ch| ch.is_ascii_alphanumeric() || "/._:-".contains(ch)) {
        arg.to_string()
    } else {
        format!("{arg:?}")
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
