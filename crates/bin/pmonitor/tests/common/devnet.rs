use anyhow::Result;
use assert_cmd::Command as AssertCommand;
use process_compose_openapi_client::Client;
use std::fs::write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::{PmonitorTestRunner, PROCESS_COMPOSE_PORT, REPO_ROOT};

impl PmonitorTestRunner {
    fn process_compose_manifest_filepath(&self) -> Result<PathBuf> {
        let manifest_path = self
            .pmonitor_integration_test_dir
            .join("process-compose.yml");

        let manifest = format!(
            r#"---
version: "0.5"

environment:
  - "RUST_BACKTRACE=1"
  - "RUST_LOG=info,network_integration=debug,pclientd=debug,pcli=info,pd=debug,penumbra=debug,penumbra_sdk_app::server::mempool=trace,tower_abci=debug"

log_level: info
is_strict: true
log_location: deployments/logs/dev-env-combined.log

processes:
  pd:
    command: "{pd_binary} start"
    readiness_probe:
      http_get:
        host: 127.0.0.1
        scheme: http
        path: "/"
        port: 8080
      failure_threshold: 2
      initial_delay_seconds: 5
      period_seconds: 5

  cometbft:
    command: cometbft --log_level=debug --home ~/.penumbra/network_data/node0/cometbft start
    readiness_probe:
      http_get:
        host: 127.0.0.1
        scheme: http
        path: "/"
        port: 26657
      failure_threshold: 2
      initial_delay_seconds: 5
      period_seconds: 5
    depends_on:
      pd:
        condition: process_healthy
"#,
            pd_binary = self.binaries.pd.display()
        );

        write(&manifest_path, manifest)?;
        Ok(manifest_path)
    }

    /// Halt any pre-existing local devnet for these integration tests.
    pub fn stop_devnet(&self) -> Result<()> {
        Command::new("process-compose")
            .arg("--help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("process-compose is not available on PATH; activate the nix dev env");

        let result = Command::new("process-compose")
            .env("PC_PORT_NUM", PROCESS_COMPOSE_PORT.to_string())
            .arg("down")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match result {
            Ok(_) => {
                tracing::trace!(
                    "'process-compose down' completed, sleeping briefly during teardown"
                );
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(_) => {
                tracing::trace!(
                    "'process-compose down' failed, presumably no prior network running"
                );
            }
        }
        Ok(())
    }

    /// Create a genesis event for the local devnet, with genesis allocations for all pcli wallets.
    /// This is a *destructive* action, as it removes the contents of the default pd network_data
    /// directory prior to generation.
    pub fn generate_network_data(&self) -> Result<()> {
        let reset_cmd = AssertCommand::new(&self.binaries.pd)
            .args(["network", "unsafe-reset-all"])
            .output();
        assert!(
            reset_cmd.unwrap().status.success(),
            "failed to clear out prior local devnet config"
        );

        let validators_filepath: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "..",
            "..",
            "..",
            "testnets",
            "validators-single.json",
        ]
        .iter()
        .collect();

        let cmd = AssertCommand::new(&self.binaries.pd)
            .args([
                "network",
                "generate",
                "--chain-id",
                "penumbra-devnet-pmonitor",
                "--unbonding-delay",
                "50",
                "--epoch-duration",
                "50",
                "--proposal-voting-blocks",
                "50",
                "--timeout-commit",
                "3s",
                "--gas-price-simple",
                "500",
                "--validators-input-file",
                validators_filepath
                    .to_str()
                    .expect("failed to convert validators filepath to str"),
                "--allocations-input-file",
                &self
                    .generate_genesis_allocations()?
                    .to_str()
                    .expect("failed to convert allocations csv to str"),
            ])
            .output();
        assert!(
            cmd.unwrap().status.success(),
            "failed to generate local devnet config"
        );
        Ok(())
    }

    /// Run a local devnet based on input config.
    pub async fn start_devnet(&self) -> Result<Child> {
        self.stop_devnet()?;
        self.generate_network_data()?;
        let process_compose_manifest_filepath = self.process_compose_manifest_filepath()?;

        let child = Command::new("process-compose")
            .env("PC_PORT_NUM", PROCESS_COMPOSE_PORT.to_string())
            .env("TERM", "dumb")
            .current_dir(REPO_ROOT.as_os_str())
            .args([
                "up",
                "--detached",
                "--tui=false",
                "--config",
                process_compose_manifest_filepath
                    .to_str()
                    .expect("failed to convert process-compose manifest to str"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to execute devnet start cmd");

        poll_for_ready("pd").await?;
        poll_for_ready("cometbft").await?;
        tracing::debug!("all processes ready, devnet is running");
        poll_for_blocks().await?;

        Ok(child)
    }
}

async fn poll_for_ready(process_name: &str) -> Result<()> {
    let client = Client::new(format!("http://localhost:{PROCESS_COMPOSE_PORT}").as_str());
    let timeout = 120;

    for elapsed in 0..timeout {
        if let Ok(response) = client.get_process(process_name).await {
            match response.into_inner().is_ready.as_deref() {
                Some("-") => {
                    tracing::debug!("still waiting for process to be ready: {process_name}")
                }
                Some("Ready") => {
                    tracing::debug!("process '{process_name}' is ready!");
                    return Ok(());
                }
                _ => tracing::warn!("unexpected status for process '{process_name}', waiting..."),
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        if elapsed + 1 == timeout {
            break;
        }
    }

    anyhow::bail!("process '{process_name}' not ready after {timeout} seconds, failing");
}

async fn poll_for_blocks() -> Result<()> {
    let timeout = 120;

    for elapsed in 0..timeout {
        if let Ok(result) = Command::new("curl")
            .args(["-s", "http://127.0.0.1:26657/status"])
            .output()
        {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                if let Some(height_match) = stdout
                    .split("latest_block_height")
                    .nth(1)
                    .and_then(|s| s.split('"').nth(2))
                {
                    if let Ok(height) = height_match.parse::<u64>() {
                        if height > 0 {
                            tracing::debug!(height, "network is producing blocks");
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            return Ok(());
                        }
                    }
                }
            }
        }

        tracing::debug!(
            "waiting for blocks to be produced (attempt {}/{})",
            elapsed + 1,
            timeout
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    anyhow::bail!("network did not produce blocks after {timeout} seconds");
}
