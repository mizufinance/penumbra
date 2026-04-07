#![cfg(feature = "network-integration")]
//! Integration integration testing of `pmonitor` against a local devnet.
//! Sets up various scenarios of genesis allocations, and ensures the tool reports
//! violations as errors.
//!
//! As a convenience to developers, there's a commented-out `sleep` call in the
//! `audit_passes_on_compliant_wallets` test. If enabled, the setup testbed can be interacted with
//! manually, which helps when trying to diagnose behavior of the tool.
use anyhow::Context;
use pcli::config::PcliConfig;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
mod common;
use crate::common::pcli_helpers::{pcli_init_softkms, pcli_migrate_balance, pcli_view_address};
use crate::common::{ExpectedAudit, PmonitorTestRunner};

fn integration_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("pmonitor integration test lock is poisoned")
}

async fn fresh_runner() -> anyhow::Result<PmonitorTestRunner> {
    let runner = PmonitorTestRunner::new();
    runner.create_pcli_wallets()?;
    let _network = runner.start_devnet().await?;
    runner.initialize_pmonitor()?;
    Ok(runner)
}

fn create_empty_wallet(
    runner: &PmonitorTestRunner,
    label: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let wallet_home = runner.wallets_dir()?.join(format!("wallet-{label}"));
    pcli_init_softkms(runner.pcli_binary(), &wallet_home)?;
    Ok(wallet_home)
}

fn send_amount(
    runner: &PmonitorTestRunner,
    from_home: &std::path::Path,
    to_address: &penumbra_sdk_keys::address::Address,
    amount: &str,
) -> anyhow::Result<()> {
    let status = Command::new(runner.pcli_binary())
        .args([
            "--home",
            from_home.to_str().unwrap(),
            "tx",
            "send",
            "--to",
            &to_address.to_string(),
            amount,
        ])
        .status()
        .context("failed to execute pcli send command")?;
    anyhow::ensure!(status.success(), "pcli send command failed: {status}");
    Ok(())
}

#[tokio::test]
/// Tests the simplest happy path for pmonitor: all wallets have genesis balances,
/// they never transferred any funds out, nor migrated balances, so all
/// current balances equal the genesis balances. In this case `pmonitor`
/// should exit 0.
async fn audit_passes_on_compliant_wallets() -> anyhow::Result<()> {
    let _guard = integration_test_lock();
    tracing_subscriber::fmt::try_init().ok();
    let p = fresh_runner().await?;

    // Debugging: uncomment the sleep line below if you want to interact with the pmonitor testbed
    // that was set up already. Use e.g.:
    //
    //   cargo run --bin pmonitor -- --home /tmp/pmonitor-integration-test/pmonitor audit
    //
    // to view the output locally.
    //
    // std::thread::sleep(std::time::Duration::from_secs(3600));

    p.pmonitor_audit()?;
    Ok(())
}

#[tokio::test]
/// Tests another happy path for pmonitor: all wallets have genesis balances,
/// one of the wallets ran `pcli migrate balance` once. This means that all
/// wallets still have their genesis balance, save one, which has the genesis
/// balance minus gas fees. In this case, `pmonitor` should exit 0,
/// because it understood the balance migration and updated the FVK.
async fn audit_passes_on_wallets_that_migrated_once() -> anyhow::Result<()> {
    let _guard = integration_test_lock();
    let p = fresh_runner().await?;
    p.pmonitor_audit()?;

    let alice_pcli_home = create_empty_wallet(&p, "alice")?;
    let alice_pcli_config = PcliConfig::load(
        alice_pcli_home
            .join("config.toml")
            .to_str()
            .expect("failed to convert alice wallet to str"),
    )?;

    // Take the second wallet, and migrate its balance to Alice.
    let migrated_wallet = p.wallets_dir()?.join("wallet-1");
    pcli_migrate_balance(
        p.pcli_binary(),
        &migrated_wallet,
        &alice_pcli_config.full_viewing_key,
    )?;

    // Now re-run the audit tool: it should report OK again, because all we did was migrate.
    p.pmonitor_audit()?;
    Ok(())
}

#[tokio::test]
/// Tests another happy path for pmonitor: all wallets have genesis balances,
/// one of the wallets ran `pcli migrate balance` once, then that receiving
/// wallet ran `pcli migrate balance` itself, so the genesis funds are now
/// two (2) FVKs away from the original account. In this case,
/// `pmonitor` should exit 0, because it understood all balance migrations
/// and updated the FVK in its config file accordingly.
async fn audit_passes_on_wallets_that_migrated_twice() -> anyhow::Result<()> {
    let _guard = integration_test_lock();
    let p = fresh_runner().await?;
    p.pmonitor_audit()
        .context("failed unexpectedly during initial audit run")?;

    let alice_pcli_home = create_empty_wallet(&p, "alice")?;
    let alice_pcli_config = PcliConfig::load(
        alice_pcli_home
            .join("config.toml")
            .to_str()
            .expect("failed to convert alice wallet to str"),
    )?;

    // Take the second wallet, and migrate its balance to Alice.
    let migrated_wallet = p.wallets_dir()?.join("wallet-1");
    pcli_migrate_balance(
        p.pcli_binary(),
        &migrated_wallet,
        &alice_pcli_config.full_viewing_key,
    )?;

    // Now re-run the audit tool: it should report OK again, because all we did was migrate.
    p.pmonitor_audit()
        .context("failed unexpectedly during second audit run")?;

    let bob_pcli_home = create_empty_wallet(&p, "bob")?;
    let bob_pcli_config = PcliConfig::load(
        bob_pcli_home
            .join("config.toml")
            .to_str()
            .expect("failed to convert bob wallet to str"),
    )?;

    // Re-migrate the balance from Alice to Bob.
    pcli_migrate_balance(
        p.pcli_binary(),
        &alice_pcli_home,
        &bob_pcli_config.full_viewing_key,
    )?;

    // Now re-run the audit tool: it should report OK again, confirming that it
    // successfully tracks multiple migratrions.
    p.pmonitor_audit()
        .context("failed unexpectedly during final audit run in test")?;

    Ok(())
}

#[tokio::test]
/// Tests an unhappy path for `pmonitor`: a single wallet has sent all its funds
/// to non-genesis account, via `pcli tx send` rather than `pcli migrate balance`.
/// In this case, `pmonitor` should exit non-zero.
async fn audit_fails_on_misbehaving_wallet_that_sent_funds() -> anyhow::Result<()> {
    let _guard = integration_test_lock();
    let p = fresh_runner().await?;
    p.pmonitor_audit()?;

    let alice_pcli_home = create_empty_wallet(&p, "alice")?;
    let alice_address = pcli_view_address(p.pcli_binary(), &alice_pcli_home)?;
    let misbehaving_wallet = p.wallets_dir()?.join("wallet-1");
    send_amount(&p, &misbehaving_wallet, &alice_address, "999900penumbra")?;
    p.expect_audit_state(ExpectedAudit::Fail, Duration::from_secs(30))?;
    Ok(())
}

#[tokio::test]
/// Tests a happy path for `pmonitor`: a single wallet has sent all its funds
/// to non-genesis account, via `pcli tx send` rather than `pcli migrate balance`,
/// but the receiving wallet then sent those funds back.
/// In this case, `pmonitor` should exit zero.
async fn audit_passes_on_misbehaving_wallet_that_sent_funds_but_got_them_back() -> anyhow::Result<()>
{
    let _guard = integration_test_lock();
    tracing_subscriber::fmt::try_init().ok();
    let p = fresh_runner().await?;
    p.pmonitor_audit()?;

    let alice_pcli_home = create_empty_wallet(&p, "alice")?;
    let alice_address = pcli_view_address(p.pcli_binary(), &alice_pcli_home)?;
    let misbehaving_wallet = p.wallets_dir()?.join("wallet-1");
    send_amount(&p, &misbehaving_wallet, &alice_address, "999900penumbra")?;
    p.expect_audit_state(ExpectedAudit::Fail, Duration::from_secs(30))?;

    let misbehaving_address = pcli_view_address(p.pcli_binary(), &misbehaving_wallet)?;
    send_amount(
        &p,
        &alice_pcli_home,
        &misbehaving_address,
        "999899.99penumbra",
    )?;
    p.expect_audit_state(ExpectedAudit::Pass, Duration::from_secs(30))?;

    Ok(())
}
