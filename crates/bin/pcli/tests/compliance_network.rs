//! Compliance integration tests against a running devnet.
//!
//! These tests are marked with `#[ignore]` and run as part of `just integration-pcli`
//! (invoked by `just smoke`). They require:
//! - A running devnet (SHIELDD_NODE_PD_URL env var)
//! - Compliance env vars set by smoke-test.sh (COMPLIANCE_DK_HEX, etc.)
//!
//! Run manually:
//! ```
//! cargo test --package pcli -- --ignored --test compliance_network --test-threads 1
//! ```

use assert_cmd::Command;
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::{
    test_keys::{ADDRESS_1_STR, SEED_PHRASE},
    Address,
};
use shieldd_sdk_proto::core::component::compliance::v1::{
    query_service_client::QueryServiceClient, ComplianceAssetStatusRequest,
    ComplianceUserLeafRequest,
};
use tempfile::{tempdir, TempDir};
use tonic::transport::Channel;

const TIMEOUT_COMMAND_SECONDS: u64 = 120;
const DEFAULT_COMPLIANCE_DEV_REGISTRAR_SK_HEX: &str =
    "0100000000000000000000000000000000000000000000000000000000000000";
const DEFAULT_COMPLIANCE_DEV_AUTHORITY_SK_HEX: &str =
    "0200000000000000000000000000000000000000000000000000000000000000";
const DEFAULT_COMPLIANCE_DEV_AUTHORITY_VK_HEX: &str =
    "b2ecf9b9082d6306538be73b0d6ee741141f3222152da78685d6596efc8c1506";
const DEFAULT_COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX: &str =
    "2ebd42dd3a2307083c834e79fb9e787e352dd33e0d719f86ae4adb02fe382409";
const DEFAULT_COMPLIANCE_DEV_RING_PK_HEX: &str =
    "0800000000000000000000000000000000000000000000000000000000000000";
const DEFAULT_COMPLIANCE_GRANT_VALID_UNTIL_UNIX: &str = "4102444800";
const TEST_RING_ID: &str = "shieldd-smoke-ring-2";
const TEST_POLICY_ID: &str = "shieldd-smoke-policy-2";
const TEST_PERMISSION: &str = "read";
const TEST_RESOURCE: &str = "document";

/// Import the wallet from seed phrase into a temporary directory.
fn load_wallet_into_tmpdir() -> TempDir {
    let tmpdir = tempdir().unwrap();

    let grpc_url =
        std::env::var("SHIELDD_NODE_PD_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());

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
    std::env::var("SHIELDD_NODE_PD_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
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

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn sign_asset_grant(
    tmpdir: &TempDir,
    asset_denom: &str,
    dk_pub_hex: &str,
    daily_volume_limit: &str,
) -> String {
    let registrar_sk = env_or_default(
        "COMPLIANCE_DEV_REGISTRAR_SK_HEX",
        DEFAULT_COMPLIANCE_DEV_REGISTRAR_SK_HEX,
    );
    let authority_vk = env_or_default(
        "COMPLIANCE_DEV_AUTHORITY_VK_HEX",
        DEFAULT_COMPLIANCE_DEV_AUTHORITY_VK_HEX,
    );
    let seizure_authority_vk = env_or_default(
        "COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX",
        DEFAULT_COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX,
    );
    let ring_pk = env_or_default(
        "COMPLIANCE_DEV_RING_PK_HEX",
        DEFAULT_COMPLIANCE_DEV_RING_PK_HEX,
    );
    let valid_until = env_or_default(
        "COMPLIANCE_GRANT_VALID_UNTIL_UNIX",
        DEFAULT_COMPLIANCE_GRANT_VALID_UNTIL_UNIX,
    );
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "sign-asset-grant",
        asset_denom,
        "--regulated",
        "--dk-pub-hex",
        dk_pub_hex,
        "--daily-volume-limit",
        daily_volume_limit,
        "--ring-pk-hex",
        &ring_pk,
        "--ring-id",
        TEST_RING_ID,
        "--policy-id",
        TEST_POLICY_ID,
        "--permission",
        TEST_PERMISSION,
        "--resource",
        TEST_RESOURCE,
        "--registration-authority-vk-hex",
        &authority_vk,
        "--seizure-authority-vk-hex",
        &seizure_authority_vk,
        "--registrar-sk-hex",
        &registrar_sk,
        "--valid-until-unix",
        &valid_until,
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

    let output = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(output)
        .expect("asset grant should be utf8")
        .trim()
        .to_string()
}

fn sign_user_grant(
    tmpdir: &TempDir,
    asset_denom: &str,
    address: Address,
    address_index: u32,
) -> (String, String) {
    let authority_sk = env_or_default(
        "COMPLIANCE_DEV_AUTHORITY_SK_HEX",
        DEFAULT_COMPLIANCE_DEV_AUTHORITY_SK_HEX,
    );
    let valid_until = env_or_default(
        "COMPLIANCE_GRANT_VALID_UNTIL_UNIX",
        DEFAULT_COMPLIANCE_GRANT_VALID_UNTIL_UNIX,
    );
    let ring_pk = env_or_default(
        "COMPLIANCE_DEV_RING_PK_HEX",
        DEFAULT_COMPLIANCE_DEV_RING_PK_HEX,
    );
    let policy_id =
        std::env::var("COMPLIANCE_DEV_POLICY_ID").unwrap_or_else(|_| "shieldd-dev-policy".into());
    let rnk_dh_pk = std::env::var(format!("COMPLIANCE_USER_{address_index}_RNK_DH_PK_HEX"))
        .expect("test requires an Orbis-certified RNK DH key");
    let rnk_commitment = std::env::var(format!(
        "COMPLIANCE_USER_{address_index}_RNK_COMMITMENT_HEX"
    ))
    .expect("test requires the wallet-derived RNK commitment");
    let certificate = std::env::var(format!(
        "COMPLIANCE_USER_{address_index}_CAPABILITY_CERTIFICATE_HEX"
    ))
    .expect("test requires an Orbis capability certificate");
    let address = address.to_string();
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "sign-user-grant",
        asset_denom,
        "--address",
        &address,
        "--policy-id",
        &policy_id,
        "--ring-pk-hex",
        &ring_pk,
        "--rnk-dh-pk-hex",
        &rnk_dh_pk,
        "--rnk-commitment-hex",
        &rnk_commitment,
        "--registration-authority-sk-hex",
        &authority_sk,
        "--valid-until-unix",
        &valid_until,
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

    let output = cmd.assert().success().get_output().stdout.clone();
    let grant = String::from_utf8(output)
        .expect("user grant should be utf8")
        .trim()
        .to_string();
    (grant, certificate)
}

fn query_asset_status(
    asset_denom: &str,
) -> shieldd_sdk_proto::core::component::compliance::v1::ComplianceAssetStatusResponse {
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

fn transfer_with_retry(tmpdir: &TempDir, amount: &str, to: &str) {
    for attempt in 0..2 {
        let mut cmd = Command::cargo_bin("pcli").unwrap();
        cmd.args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "transfer",
            amount,
            "--to",
            to,
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

        let output = cmd.output().expect("transfer command should run");
        if output.status.success() {
            return;
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let transient_broadcast_close = stderr
            .contains("connection closed before message completed")
            || stderr.contains("error broadcasting tx sync: HTTP error");
        if attempt == 0 && transient_broadcast_close {
            std::thread::sleep(std::time::Duration::from_secs(5));
            sync(tmpdir);
            continue;
        }

        panic!(
            "pcli transfer failed after {} attempt(s)\nstdout:\n{}\nstderr:\n{}",
            attempt + 1,
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }
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
    transfer_with_retry(&tmpdir, "1wtest_usd", ADDRESS_1_STR);
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
    let daily_volume_limit = "1000000000000000000000";
    let grant = sign_asset_grant(
        &tmpdir,
        "smoke_test_asset_2",
        dk_pub_hex,
        daily_volume_limit,
    );
    let authority_vk = env_or_default(
        "COMPLIANCE_DEV_AUTHORITY_VK_HEX",
        DEFAULT_COMPLIANCE_DEV_AUTHORITY_VK_HEX,
    );
    let seizure_authority_vk = env_or_default(
        "COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX",
        DEFAULT_COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX,
    );
    let ring_pk = env_or_default(
        "COMPLIANCE_DEV_RING_PK_HEX",
        DEFAULT_COMPLIANCE_DEV_RING_PK_HEX,
    );

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
            "--daily-volume-limit",
            daily_volume_limit,
            "--ring-pk-hex",
            &ring_pk,
            "--ring-id",
            TEST_RING_ID,
            "--policy-id",
            TEST_POLICY_ID,
            "--permission",
            TEST_PERMISSION,
            "--resource",
            TEST_RESOURCE,
            "--registration-authority-vk-hex",
            &authority_vk,
            "--seizure-authority-vk-hex",
            &seizure_authority_vk,
            "--asset-registration-grant-hex",
            &grant,
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
    // Smoke setup already registers addresses 0 and 1 for transfer coverage.
    // Register a fresh address while the funded primary address pays the fee.
    let address_index = 2;
    let address = wallet_address(&tmpdir, address_index);
    let address_string = address.to_string();

    let smoke_asset =
        std::env::var("COMPLIANCE_SMOKE_ASSET").unwrap_or_else(|_| "regulated_usd".to_string());
    let (grant, certificate) =
        sign_user_grant(&tmpdir, &smoke_asset, address.clone(), address_index);

    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args([
        "--home",
        tmpdir.path().to_str().unwrap(),
        "tx",
        "compliance",
        "register-user",
        &smoke_asset,
        "--address",
        &address_string,
        "--user-registration-grant-hex",
        &grant,
        "--capability-certificate-hex",
        &certificate,
    ])
    .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    cmd.assert().success();

    assert!(
        query_user_registration(address, &smoke_asset),
        "registered user should be visible in the compliance registry"
    );
}
