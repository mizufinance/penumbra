use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn offline_compliance_commands_are_discoverable() {
    let mut command = Command::cargo_bin("pcli").unwrap();
    command.args(["compliance", "--help"]);
    let mut assertion = command.assert().success();
    for name in [
        "generate-dk",
        "sign-asset-grant",
        "sign-user-grant",
        "derive-spend-vk",
    ] {
        assertion =
            assertion.stdout(predicate::str::is_match(format!(r"(?m)^\s+{name}\s")).unwrap());
    }
}

#[test]
fn wallet_initialization_methods_are_discoverable() {
    let mut command = Command::cargo_bin("pcli").unwrap();
    command.args(["init", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("view-only"))
        .stdout(predicate::str::contains("soft-kms"));
}

#[test]
fn generates_detection_key_without_a_wallet_or_network() {
    let output = Command::cargo_bin("pcli")
        .unwrap()
        .args(["compliance", "generate-dk"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.contains("DK_pub (hex):"));
    let key = stdout
        .lines()
        .find(|line| line.contains("DK (hex):"))
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap();
    assert_eq!(hex::decode(key).unwrap().len(), 32);
}
