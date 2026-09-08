use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn transaction_commands_are_discoverable() {
    let mut command = Command::cargo_bin("pcli").unwrap();
    command.args(["tx", "--help"]);
    let mut assertion = command.assert().success();
    for name in ["transfer", "reshape", "withdraw", "compliance"] {
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
