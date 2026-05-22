//! Basic integration testing of `pcli` versus a target testnet.
//!
//! These tests are marked with `#[ignore]`, but can be run with:
//! `cargo test --package pcli -- --ignored --test-threads 1`
//!
//! Tests against the network in the `PENUMBRA_NODE_PD_URL` environment variable.
//!
//! Tests assume that the initial state of the test account is after genesis,
//! where no tokens have been delegated, and the address with index 0
//! was distributed the wrapped test asset ([`TEST_ASSET`]).
//!
//! See the latest testnet's `allocations.csv` for the initial allocations to the test validator addresses
//! ([`ADDRESS_0_STR`], [`ADDRESS_1_STR`]).

use std::path::PathBuf;

use assert_cmd::Command;
use directories::UserDirs;
use penumbra_sdk_validator::validator::ValidatorToml;
use predicates::prelude::*;
use regex::Regex;
use serde_json::Value;
use tempfile::{tempdir, NamedTempFile, TempDir};
use url::Url;

use penumbra_sdk_keys::test_keys::{ADDRESS_0_STR, ADDRESS_1_STR, SEED_PHRASE};
use penumbra_sdk_proto::core::transaction::v1::TransactionView as ProtoTransactionView;
use penumbra_sdk_transaction::view::TransactionView;

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

    let grpc_url: Url = std::env::var("PENUMBRA_NODE_PD_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
        .parse()
        .expect("failed to parse PENUMBRA_NODE_PD_URL");

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

/// Look up a currently active validator on the testnet.
/// Will return the most bonded, which means the Penumbra Labs CI validator.
fn get_validator(tmpdir: &TempDir) -> String {
    let mut validator_cmd = Command::cargo_bin("pcli").unwrap();
    validator_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "query",
            "validator",
            "list",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    validator_cmd.assert().success();

    // Pull out one of the validators from stdout.
    let stdout_vec = validator_cmd.unwrap().stdout;
    let validator_regex = Regex::new(r"penumbravalid1\w{58}").unwrap();
    let captures = validator_regex.captures(std::str::from_utf8(&stdout_vec).unwrap());

    // We retrieve the first match via index 0, which results in most trusted.
    captures.unwrap()[0].to_string()
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
        penumbra_sdk_transaction::ActionView::Transfer(_)
    ));

    // Inspect the TransactionView and ensure that we can read the memo text.
    let mv = tv
        .body_view
        .memo_view
        .expect("can find MemoView in TransactionView");
    match mv {
        penumbra_sdk_transaction::MemoView::Visible { plaintext, .. } => {
            tracing::info!(?plaintext, "plaintext memo");
            tracing::info!(?memo_text, "expected memo text");
            assert!(plaintext.text == memo_text);
        }
        penumbra_sdk_transaction::MemoView::Opaque { .. } => {
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
fn governance_submit_proposal() {
    let tmpdir = load_wallet_into_tmpdir();

    // Get template for signaling proposal.
    let mut template_cmd = Command::cargo_bin("pcli").unwrap();
    template_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "proposal",
            "template",
            "signaling",
            "--file",
            "proposal.toml",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    template_cmd.assert().success();

    // Submit signaling proposal.
    let mut submit_cmd = Command::cargo_bin("pcli").unwrap();
    submit_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "tx",
            "proposal",
            "submit",
            "--file",
            "proposal.toml",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    submit_cmd.assert().success();

    // Now list the proposals.
    let mut proposals_cmd = Command::cargo_bin("pcli").unwrap();
    proposals_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "query",
            "governance",
            "list-proposals",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    proposals_cmd.assert().success();
}

#[ignore]
#[test]
fn duplicate_consensus_key_forbidden() {
    // Look up validator, so we have known-good data to munge.
    let tmpdir = load_wallet_into_tmpdir();
    let validator = get_validator(&tmpdir);
    let mut query_cmd = Command::cargo_bin("pcli").unwrap();
    query_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "query",
            "validator",
            "definition",
            validator.as_str(),
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    query_cmd.assert().success();
    let validator_def_vec = query_cmd.unwrap().stdout;
    let original_validator_def: ValidatorToml =
        toml::from_str(&String::from_utf8_lossy(&validator_def_vec))
            .expect("can parse validator template as TOML");

    // Get template for promoting our node to validator.
    let mut template_cmd = Command::cargo_bin("pcli").unwrap();
    template_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "validator",
            "definition",
            "template",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    template_cmd.assert().success();
    let template_vec = template_cmd.unwrap().stdout;
    let mut new_validator_def: ValidatorToml =
        toml::from_str(&String::from_utf8_lossy(&template_vec))
            .expect("can parse validator template as TOML");

    // Overwrite randomly generated consensus key with one taken from
    // a real validator.
    new_validator_def.consensus_key = original_validator_def.consensus_key;

    // Write out new, intentionally broken validator definition.
    let validator_filepath = NamedTempFile::new().unwrap();
    std::fs::write(
        &validator_filepath,
        toml::to_string_pretty(&new_validator_def)
            .expect("Could not marshall new validator config as TOML"),
    )
    .expect("Could not overwrite validator config file with new definition");

    // Submit (intentionally broken) validator definition.
    let mut submit_cmd = Command::cargo_bin("pcli").unwrap();
    submit_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "validator",
            "definition",
            "upload",
            "--file",
            validator_filepath.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    submit_cmd.assert().failure();
}

#[ignore]
#[test]
/// Ensures that attempting to modify an existing validator's consensus key fails.
fn mismatched_consensus_key_update_fails() {
    // Use a distinct wallet so the validator template identity does not match the
    // single genesis validator we provision for smoke tests.
    const ALT_SEED_PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // Get template for promoting our node to validator.
    // We use a named tempfile so we can get a filepath for pcli cli.
    let validator_filepath = NamedTempFile::new().unwrap();
    let tmpdir = load_wallet_phrase_into_tmpdir(ALT_SEED_PHRASE);
    let mut template_cmd = Command::cargo_bin("pcli").unwrap();
    template_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "validator",
            "definition",
            "template",
            "--file",
            (validator_filepath.path().to_str().unwrap()),
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    template_cmd.assert().success();
    let template_content = std::fs::read_to_string(&validator_filepath)
        .expect("Could not read initial validator config file");
    let mut new_validator_def: ValidatorToml = toml::from_str(&template_content)
        .expect("Could not parse initial validator template as TOML");

    // Now we retrieve the actual cometbft consensus key from the network data dir.
    // Doing so assumes that the generated data was previously put in place,
    // which is a reasonable assumption in the context of running smoketest suite.
    // Respect PENUMBRA_DEVNET_HOME if set (smoke tests use a temp dir).
    let devnet_home = std::env::var("PENUMBRA_DEVNET_HOME").unwrap_or_else(|_| {
        let userdir = UserDirs::new().unwrap();
        let homedir = userdir.home_dir().to_str().unwrap().to_string();
        format!("{homedir}/.penumbra")
    });
    let tm_key_filepath: PathBuf = [
        &devnet_home,
        "network_data",
        "node0",
        "cometbft",
        "config",
        "priv_validator_key.json",
    ]
    .iter()
    .collect();
    let tm_key_config: Value =
        serde_json::from_str(&std::fs::read_to_string(tm_key_filepath).unwrap())
            .expect("Could not read cometbft key config file");
    let tm_key: tendermint::PublicKey =
        serde_json::value::from_value(tm_key_config["pub_key"].clone())
            .expect("Could not parse cometbft key config file");

    // Modify initial validator definition template to use actual tm key.
    new_validator_def.consensus_key = tm_key;
    // Mark validator definition as "active".
    new_validator_def.enabled = true;

    // Write out revised (and incorrect!) validator definition.
    std::fs::write(
        &validator_filepath,
        toml::to_string_pretty(&new_validator_def)
            .expect("Could not marshall revised validator config as TOML"),
    )
    .expect("Could not overwrite validator config file with revised definition");

    let mut resubmit_cmd = Command::cargo_bin("pcli").unwrap();
    resubmit_cmd
        .args([
            "--home",
            tmpdir.path().to_str().unwrap(),
            "validator",
            "definition",
            "upload",
            "--file",
            validator_filepath.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    // Ensure that command fails.
    resubmit_cmd.assert().failure();
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
            "list-tx-hashes",
        ])
        .timeout(std::time::Duration::from_secs(TIMEOUT_COMMAND_SECONDS));
    let _view_result = view_cmd
        .assert()
        .try_success()
        .expect("pcli command failed: 'view list-tx-hashes'");
}
