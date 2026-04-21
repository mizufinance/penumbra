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
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::{
    test_keys::{ADDRESS_1_STR, SEED_PHRASE},
    Address,
};
use penumbra_sdk_proto::core::component::compliance::v1::{
    query_service_client::QueryServiceClient, ComplianceAssetStatusRequest,
    ComplianceUserLeafRequest,
};
use tempfile::{tempdir, TempDir};
use tonic::transport::Channel;

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

fn grpc_url() -> String {
    std::env::var("PENUMBRA_NODE_PD_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
}

fn wallet_address(tmpdir: &TempDir, index: u32) -> Address {
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "view",
        "address",
        &index.to_string(),
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

    let output = cmd.assert().success().get_output().stdout.clone();
    let address = String::from_utf8(output).expect("address output should be utf8");
    address.trim().parse().expect("address output should parse")
}

fn query_asset_status(
    asset_denom: &str,
) -> penumbra_sdk_proto::core::component::compliance::v1::ComplianceAssetStatusResponse {
    let asset_id = asset::REGISTRY.parse_unit(asset_denom).id();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        QueryServiceClient::<Channel>::connect(grpc_url())
            .await
            .expect("view service should be reachable")
            .compliance_asset_status(ComplianceAssetStatusRequest {
                asset_id: Some(asset_id.into()),
            })
            .await
            .expect("asset status query should succeed")
            .into_inner()
    })
}

fn query_user_registration(address: Address, asset_denom: &str) -> bool {
    let asset_id = asset::REGISTRY.parse_unit(asset_denom).id();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        QueryServiceClient::<Channel>::connect(grpc_url())
            .await
            .expect("view service should be reachable")
            .compliance_user_leaf(ComplianceUserLeafRequest {
                address: Some(address.into()),
                asset_id: Some(asset_id.into()),
            })
            .await
            .expect("user leaf query should succeed")
            .into_inner()
            .is_registered
    })
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
// Unregulated transfer: standard wrapped test asset send (should work without special setup)
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_unregulated_transfer() {
    let tmpdir = load_wallet_into_tmpdir();
    sync(&tmpdir);

    // Send a small amount of the wrapped test asset to address 1.
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "transfer",
        "1wtest_usd",
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

    // Wait for the registration to be committed and visible in a new snapshot.
    std::thread::sleep(std::time::Duration::from_secs(5));

    let status = query_asset_status("smoke_test_asset_2");
    assert!(
        status.is_registered,
        "registered asset should be visible in view service"
    );
    assert!(
        status.is_regulated,
        "registered asset should be marked regulated"
    );
}

// ---------------------------------------------------------------------------
// Register user for the regulated smoke asset provisioned by smoke-test.sh
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_register_user() {
    let tmpdir = load_wallet_into_tmpdir();
    sync(&tmpdir);
    let address = wallet_address(&tmpdir, 0);

    let smoke_asset =
        std::env::var("COMPLIANCE_SMOKE_ASSET").unwrap_or_else(|_| "regulated_usd".to_string());

    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "register-user",
        &smoke_asset,
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    cmd.assert().success();

    assert!(
        query_user_registration(address, &smoke_asset),
        "registered user should be visible in the compliance registry"
    );
}

// ---------------------------------------------------------------------------
// Detection scan (requires COMPLIANCE_DK_HEX from smoke-test.sh)
// ---------------------------------------------------------------------------

#[ignore]
#[test]
fn compliance_detection_scan() {
    let dk_hex = std::env::var("COMPLIANCE_DK_HEX")
        .expect("COMPLIANCE_DK_HEX must be set by the smoke harness");
    let smoke_asset =
        std::env::var("COMPLIANCE_SMOKE_ASSET").unwrap_or_else(|_| "regulated_usd".to_string());

    let tmpdir = load_wallet_into_tmpdir();
    let grpc_url = grpc_url();

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

    cmd.assert().success();

    // Output file should exist and be valid JSON
    assert!(output_file.exists(), "scan output file should be created");
    let content = std::fs::read_to_string(&output_file).expect("can read scan output");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("scan output should be valid JSON");
    let detected = parsed["detected"]
        .as_array()
        .expect("scan output should contain a detected array");

    assert!(
        !detected.is_empty(),
        "scan should detect at least one regulated transfer for the smoke asset"
    );
    let expected_asset_id = asset::REGISTRY.parse_unit(&smoke_asset).id().to_string();
    assert!(
        detected
            .iter()
            .any(|entry| entry["asset_id"].as_str() == Some(expected_asset_id.as_str())),
        "scan output should include the requested smoke asset (expected asset_id: {})",
        expected_asset_id
    );
}
