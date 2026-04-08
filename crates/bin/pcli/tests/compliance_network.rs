//! Compliance integration tests against a running devnet.
//!
//! These tests are marked with `#[ignore]` and run as part of `just integration-pcli`
//! (invoked by `just smoke`). They require:
//! - A running devnet (PENUMBRA_NODE_PD_URL env var)
//! - Compliance env vars set by smoke-test.sh (COMPLIANCE_DK_HEX, etc.)
//!
//! Run manually:
//! ```
//! cargo test --package pcli -- --ignored --test compliance_network --test-threads 1
//! ```

use assert_cmd::Command;
use penumbra_sdk_keys::test_keys::{ADDRESS_1_STR, SEED_PHRASE};
use tempfile::{tempdir, TempDir};

const TIMEOUT_COMMAND_SECONDS: u64 = 120;

/// Import the wallet from seed phrase into a temporary directory.
fn load_wallet_into_tmpdir() -> TempDir {
    let tmpdir = tempdir().unwrap();

    let grpc_url = std::env::var("PENUMBRA_NODE_PD_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());

    let mut setup_cmd = Command::cargo_bin("pcli").unwrap();
    setup_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "init",
            "--grpc-url",
            &grpc_url,
            "soft-kms",
            "import-phrase",
        ])
        .write_stdin(SEED_PHRASE)
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    setup_cmd
        .assert()
        .stdout(predicates::str::contains("Writing generated config"));

    tmpdir
}

/// Sync the wallet.
fn sync(tmpdir: &TempDir) {
    let mut sync_cmd = Command::cargo_bin("pcli").unwrap();
    sync_cmd
        .args(["--home", tmpdir.path().to_str().unwrap(), "view", "sync"])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    sync_cmd.assert().success();
}

// ---------------------------------------------------------------------------
// Generate-DK: pure computation, no network needed
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_generate_dk() {
    let tmpdir = load_wallet_into_tmpdir();
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "generate-dk",
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should output hex keys
    assert!(
        stdout.contains("DK (hex):"),
        "generate-dk should output private key"
    );
    assert!(
        stdout.contains("DK_pub (hex):"),
        "generate-dk should output public key"
    );

    // Verify hex format (64 chars = 32 bytes)
    let dk_line = stdout
        .lines()
        .find(|l| l.contains("DK (hex):"))
        .expect("DK line");
    let dk_hex = dk_line.split_whitespace().last().unwrap();
    assert_eq!(dk_hex.len(), 64, "DK should be 64 hex chars (32 bytes)");
    assert!(hex::decode(dk_hex).is_ok(), "DK should be valid hex");
}

// ---------------------------------------------------------------------------
// Unregulated transfer: standard test_usd send (should work without special setup)
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_unregulated_transfer() {
    let tmpdir = load_wallet_into_tmpdir();
    sync(&tmpdir);

    // Send a small amount of test_usd to address 1 (unregulated asset, should just work)
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "send",
        "1test_usd",
        "--to",
        ADDRESS_1_STR,
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    cmd.assert().success();
}

// ---------------------------------------------------------------------------
// Register a new regulated asset (requires network)
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_register_asset() {
    let tmpdir = load_wallet_into_tmpdir();
    sync(&tmpdir);

    // Generate a DK for a second test asset
    let mut dk_cmd = Command::cargo_bin("pcli").unwrap();
    dk_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "compliance",
            "generate-dk",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    let dk_output = dk_cmd.assert().success();
    let dk_stdout = String::from_utf8_lossy(&dk_output.get_output().stdout);

    let dk_pub_hex = dk_stdout
        .lines()
        .find(|l| l.contains("DK_pub (hex):"))
        .and_then(|l| l.split_whitespace().last())
        .expect("should have dk_pub");

    // Register a new asset as regulated
    let mut reg_cmd = Command::cargo_bin("pcli").unwrap();
    reg_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "compliance",
            "register-asset",
            "smoke_test_asset_2",
            "--regulated",
            "--dk-pub-hex",
            dk_pub_hex,
            "--threshold",
            "1000000000000000000000",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    reg_cmd.assert().success();
}

// ---------------------------------------------------------------------------
// Register user for a regulated asset
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_register_user() {
    let tmpdir = load_wallet_into_tmpdir();
    sync(&tmpdir);

    // Register user for test_usd (which was registered as regulated at genesis)
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "register-user",
        "test_usd",
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    cmd.assert().success();
}

// ---------------------------------------------------------------------------
// Detection scan (requires COMPLIANCE_DK_HEX from smoke-test.sh)
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_detection_scan() {
    let dk_hex = match std::env::var("COMPLIANCE_DK_HEX") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("COMPLIANCE_DK_HEX not set, skipping detection scan test");
            return;
        }
    };
    let smoke_asset = std::env::var("COMPLIANCE_SMOKE_ASSET")
        .unwrap_or_else(|_| "smoke_compliance_token".to_string());

    let tmpdir = load_wallet_into_tmpdir();
    let grpc_url = std::env::var("PENUMBRA_NODE_PD_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());

    let output_file = tmpdir.path().join("scan_output.json");

    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "scan",
        "--node",
        &grpc_url,
        "--dk-hex",
        &dk_hex,
        "--scan-asset-id",
        &smoke_asset,
        "--start-height",
        "1",
        "--output",
        output_file.to_str().unwrap(),
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

    // Scan should succeed (may find 0 matches if no transfers happened)
    cmd.assert().success();

    // Output file should exist and be valid JSON
    assert!(output_file.exists(), "scan output file should be created");
    let content = std::fs::read_to_string(&output_file).expect("can read scan output");
    let _: serde_json::Value =
        serde_json::from_str(&content).expect("scan output should be valid JSON");
}
