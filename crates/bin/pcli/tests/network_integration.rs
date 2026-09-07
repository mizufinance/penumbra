//! Basic integration testing of `pcli` versus a target testnet.
//!
//! These tests are marked with `#[ignore]`, but can be run with:
//! `cargo test --package pcli -- --ignored --test-threads 1`
//!
//! Tests against the network in the `SHIELDD_NODE_PD_URL` environment variable.
//!
//! Tests assume that the initial state of the test account is after genesis,
//! where no tokens have been delegated, and the address with index 0
//! was distributed the wrapped test asset ([`TEST_ASSET`]).
//!
//! See the latest testnet's `allocations.csv` for the initial allocations to the test validator addresses
//! ([`ADDRESS_0_STR`], [`ADDRESS_1_STR`]).

use assert_cmd::Command;
use predicates::prelude::*;
use regex::Regex;
use serde_json::Value;
use tempfile::{tempdir, NamedTempFile, TempDir};
use url::Url;

use shieldd_sdk_keys::test_keys::{ADDRESS_0_STR, ADDRESS_1_STR, SEED_PHRASE};
use shieldd_sdk_proto::core::transaction::v1::TransactionView as ProtoTransactionView;
use shieldd_sdk_transaction::view::TransactionView;

// The number "1020" is chosen so that this is bigger than u64::MAX
// when accounting for the 10e18 scaling factor from the base denom.
const TEST_ASSET: &str = "1020wtest_usd";

// The maximum amount of time any command is allowed to take before we error.
// Gnark proof generation is slower than Groth16; allow extra headroom on top of sync time.
const TIMEOUT_COMMAND_SECONDS: u64 = 600;

/// Import the wallet from seed phrase into a temporary directory.
fn load_wallet_into_tmpdir() -> TempDir {
    load_wallet_phrase_into_tmpdir(SEED_PHRASE)
}

fn load_wallet_phrase_into_tmpdir(seed_phrase: &str) -> TempDir {
    let tmpdir = tempdir().unwrap();

    let grpc_url: Url = std::env::var("SHIELDD_NODE_PD_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
        .parse()
        .expect("failed to parse SHIELDD_NODE_PD_URL");

    let mut setup_cmd = Command::cargo_bin("pcli").unwrap();
    setup_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "init",
            "--grpc-url",
            &grpc_url.to_string(),
            "soft-kms",
            "import-phrase",
        ])
        .write_stdin(seed_phrase)
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    setup_cmd
        .assert()
        .stdout(predicate::str::contains("Writing generated config"));

    tmpdir
}

#[allow(dead_code)]
fn load_string_to_file(content: String, tmpdir: &TempDir) -> NamedTempFile {
    let mut file = NamedTempFile::new_in(tmpdir.path()).unwrap();
    use std::io::Write;
    write!(file, "{}", content).unwrap();
    file
}

/// Sync the wallet.
fn sync(tmpdir: &TempDir) {
    let mut sync_cmd = Command::cargo_bin("pcli").unwrap();
    sync_cmd
        .args(["--home", tmpdir.path().to_str().unwrap(), "view", "sync"])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    sync_cmd.assert().success();
}

fn balance_for_account(tmpdir: &TempDir, account_index: u32, denom: &str) -> u64 {
    let mut balance_cmd = Command::cargo_bin("pcli").unwrap();
    balance_cmd
        .args(["--home", tmpdir.path().to_str().unwrap(), "view", "balance"])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    let output = balance_cmd.output().expect("unable to fetch balance");
    assert!(
        output.status.success(),
        "pcli view balance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pattern = Regex::new(&format!(
        r"# {}\s+(\d+(?:\.\d+)?){}",
        account_index,
        regex::escape(denom)
    ))
    .expect("balance regex is valid");
    let captures = pattern.captures(&stdout).unwrap_or_else(|| {
        panic!("can find account #{account_index} balance for {denom} in:\n{stdout}")
    });

    captures[1]
        .parse::<f64>()
        .expect("balance amount should parse as number") as u64
}

#[ignore]
#[test]
fn transaction_send_from_addr_0_to_addr_1() {
    tracing_subscriber::fmt::try_init().ok();
    let tmpdir = load_wallet_into_tmpdir();
    sync(&tmpdir);
    let initial_addr_1_test_usd = balance_for_account(&tmpdir, 1, "wtest_usd");

    // Create a memo that we can inspect later, to confirm transaction
    // is viewable post-send.
    let memo_text = "Time is an illusion. Lunchtime doubly so.";

    // Send to self: tokens were distributed to `ADDRESS_0_STR`, in our test
    // we'll send `TEST_ASSET` to `ADDRESS_1_STR` and then check our balance.
    let mut transfer_cmd = Command::cargo_bin("pcli").unwrap();
    transfer_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "transfer",
            TEST_ASSET,
            "--to",
            ADDRESS_1_STR,
            "--memo",
            memo_text,
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));

    // Look up the transaction id from the command output so we can view it,
    // to exercise the `pcli view tx` code.
    let transfer_stdout = transfer_cmd.unwrap().stdout;
    let tx_regex = Regex::new(r"transaction confirmed and detected: ([0-9a-f]{64})").unwrap();
    let s = std::str::from_utf8(&transfer_stdout).unwrap();
    let captures = tx_regex.captures(s);
    let tx_id = &captures
        .and_then(|x| x.get(1))
        .expect("can find transaction id within 'pcli tx transfer' output")
        .as_str();
    sync(&tmpdir);
    let mut view_cmd = Command::cargo_bin("pcli").unwrap();
    view_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "view",
            "tx",
            "--raw",
            tx_id,
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    view_cmd.assert().success();

    // Convert the raw JSON to a protobuf TransactionView, then convert
    // that to a domain type.
    let view_output = view_cmd.output().unwrap();
    let view_stdout: String = std::str::from_utf8(&view_output.stdout)
        .unwrap()
        .to_string();
    let view_json: Value =
        serde_json::from_str(&view_stdout).expect("can parse JSON from 'pcli view tx'");

    let tvp: ProtoTransactionView = serde_json::value::from_value(view_json).unwrap();
    let tv: TransactionView = tvp.try_into().unwrap();

    assert!(matches!(
        &tv.body_view.action_views[0],
        shieldd_sdk_transaction::ActionView::Transfer(_)
    ));

    // Inspect the TransactionView and ensure that we can read the memo text.
    let mv = tv
        .body_view
        .memo_view
        .expect("can find MemoView in TransactionView");
    match mv {
        shieldd_sdk_transaction::MemoView::Visible { plaintext, .. } => {
            tracing::info!(?plaintext, "plaintext memo");
            tracing::info!(?memo_text, "expected memo text");
            assert!(plaintext.text == memo_text);
        }
        shieldd_sdk_transaction::MemoView::Opaque { .. } => {
            panic!("MemoView for transaction was Opaque! We should be able to read this memo.");
        }
    }

    // Now we inspect our wallet balance to ensure the funds were transferred correctly.
    let addr_1_test_usd_after = balance_for_account(&tmpdir, 1, "wtest_usd");
    assert_eq!(addr_1_test_usd_after, initial_addr_1_test_usd + 1020);

    // Cleanup: Send the asset back at the end of the test such that other tests begin
    // from the original state.
    let mut transfer_cmd = Command::cargo_bin("pcli").unwrap();
    transfer_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "transfer",
            TEST_ASSET, // 1020wtest_usd
            "--to",
            ADDRESS_0_STR,
            "--source",
            "1",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    transfer_cmd.assert().success();
}

#[ignore]
#[test]
/// Ensure that the view service can successfully parse all historical
/// transactions submitted above.
fn view_tx_hashes() {
    let tmpdir = load_wallet_into_tmpdir();
    let mut view_cmd = Command::cargo_bin("pcli").unwrap();
    view_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "view",
            "list-transaction-hashes",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    let _view_result = view_cmd
        .assert()
        .try_success()
        .expect("pcli command failed: 'view list-transaction-hashes'");
}
