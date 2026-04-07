use anyhow::{Context, Result};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::PmonitorTestRunner;

#[derive(Clone, Copy)]
pub enum ExpectedAudit {
    Pass,
    Fail,
}

impl PmonitorTestRunner {
    /// Generate a config directory for `pmonitor`, based on input FVKs.
    pub fn initialize_pmonitor(&self) -> Result<()> {
        let status = Command::new(&self.binaries.pmonitor)
            .args([
                "--home",
                self.pmonitor_home()
                    .to_str()
                    .expect("failed to convert pmonitor home to str"),
                "init",
                "--grpc-url",
                "http://127.0.0.1:8080",
                "--fvks",
                self.get_pcli_wallet_fvks_filepath()
                    .context("failed to get wallet fvks")?
                    .to_str()
                    .expect("failed to convert fvks json filepath to str"),
            ])
            .status()
            .context("failed to execute pmonitor init")?;

        anyhow::ensure!(status.success(), "failed to initialize pmonitor: {status}");
        Ok(())
    }

    /// Run `pmonitor audit` based on the pcli wallets and associated FVKs.
    pub fn pmonitor_audit(&self) -> Result<()> {
        let status = Command::new(&self.binaries.pmonitor)
            .args([
                "--home",
                self.pmonitor_home()
                    .to_str()
                    .expect("failed to convert pmonitor home to str"),
                "audit",
            ])
            .status()
            .context("failed to execute pmonitor audit")?;
        anyhow::ensure!(
            status.success(),
            "'pmonitor audit' failed with status {status}"
        );
        Ok(())
    }

    pub fn expect_audit_state(&self, expected: ExpectedAudit, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        let mut last_outcome =
            anyhow::anyhow!("pmonitor audit did not run before the timeout elapsed");

        while start.elapsed() < timeout {
            match (expected, self.pmonitor_audit()) {
                (ExpectedAudit::Pass, Ok(())) | (ExpectedAudit::Fail, Err(_)) => return Ok(()),
                (ExpectedAudit::Pass, Err(error)) => last_outcome = error,
                (ExpectedAudit::Fail, Ok(())) => {
                    last_outcome = anyhow::anyhow!(
                        "pmonitor audit still reports success while waiting for failure"
                    );
                }
            }
            sleep(Duration::from_secs(2));
        }

        match expected {
            ExpectedAudit::Pass => {
                Err(last_outcome.context("timed out waiting for pmonitor audit to pass"))
            }
            ExpectedAudit::Fail => {
                Err(last_outcome.context("timed out waiting for pmonitor audit to fail"))
            }
        }
    }
}
