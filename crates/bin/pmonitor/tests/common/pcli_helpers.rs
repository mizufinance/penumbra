//! Convenience methods for wrangling `pcli` CLI invocations,
//! for use in integration testing.

use anyhow::{Context, Result};
use penumbra_sdk_keys::{address::Address, FullViewingKey};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str::FromStr;

/// Initialize a new pcli wallet at the target directory.
/// Discards the generated seed phrase.
pub fn pcli_init_softkms(pcli_binary: &Path, pcli_home: &Path) -> Result<()> {
    let status = Command::new(pcli_binary)
        .args([
            "--home",
            pcli_home
                .to_str()
                .expect("can convert wallet path to string"),
            "init",
            "--grpc-url",
            "http://127.0.0.1:8080",
            "soft-kms",
            "generate",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .expect("pcli init stdin")
                .write_all(b"")?;
            child.wait()
        })
        .context("failed to execute `pcli init soft-kms generate`")?;
    anyhow::ensure!(status.success(), "`pcli init` failed with status {status}");
    Ok(())
}

/// Convenience method for looking up `address 0` from
/// pcli wallet stored at `pcli_home`.
pub fn pcli_view_address(pcli_binary: &Path, pcli_home: &Path) -> Result<Address> {
    let output = Command::new(pcli_binary)
        .args(["--home", pcli_home.to_str().unwrap(), "view", "address"])
        .output()
        .context("failed to retrieve address from pcli wallet")?;
    anyhow::ensure!(
        output.status.success(),
        "`pcli view address` failed with status {}",
        output.status
    );

    // Convert output to String, to trim trailing newline.
    let mut a = String::from_utf8_lossy(&output.stdout).to_string();
    if a.ends_with('\n') {
        a.pop();
    }
    Address::from_str(&a).with_context(|| format!("failed to convert str to Address: '{}'", a))
}

/// Perform a `pcli migrate balance` transaction from the wallet at `pcli_home`,
/// transferring funds to the destination `FullViewingKey`.
pub fn pcli_migrate_balance(
    pcli_binary: &Path,
    pcli_home: &Path,
    fvk: &FullViewingKey,
) -> Result<()> {
    let mut child = Command::new(pcli_binary)
        .args([
            "--home",
            pcli_home
                .to_str()
                .expect("can convert wallet path to string"),
            "migrate",
            "balance",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to execute `pcli migrate balance`")?;
    use std::io::Write;
    child
        .stdin
        .take()
        .expect("pcli migrate stdin")
        .write_all(fvk.to_string().as_bytes())?;
    let status = child.wait().context("wait for `pcli migrate balance`")?;
    anyhow::ensure!(
        status.success(),
        "`pcli migrate balance` failed with status {status}"
    );
    Ok(())
}

/// Register an asset as unregulated in the compliance registry.
/// This must be called before transfers of the asset can be made.
#[allow(dead_code)]
pub fn pcli_register_asset_unregulated(
    pcli_binary: &Path,
    pcli_home: &Path,
    asset: &str,
) -> Result<()> {
    let status = Command::new(pcli_binary)
        .args([
            "--home",
            pcli_home
                .to_str()
                .expect("can convert wallet path to string"),
            "tx",
            "compliance",
            "register-asset",
            asset,
            "--unregulated",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to execute `pcli tx compliance register-asset`")?;
    anyhow::ensure!(
        status.success(),
        "`pcli tx compliance register-asset` failed with status {status}"
    );
    Ok(())
}
