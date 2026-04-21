use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn tx_help_exposes_only_reduced_surface_commands() {
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args(["tx", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"(?m)^\s+transfer\s").unwrap())
        .stdout(predicate::str::is_match(r"(?m)^\s+consolidate\s").unwrap())
        .stdout(predicate::str::is_match(r"(?m)^\s+split\s").unwrap())
        .stdout(predicate::str::is_match(r"(?m)^\s+shielded-ics20-withdrawal\s").unwrap())
        .stdout(predicate::str::is_match(r"(?m)^\s+proposal\s").unwrap())
        .stdout(predicate::str::is_match(r"(?m)^\s+compliance\s").unwrap())
        .stdout(
            predicate::str::is_match(r"(?m)^\s+send(?:\s|$)")
                .unwrap()
                .not(),
        )
        .stdout(
            predicate::str::is_match(r"(?m)^\s+send-multi(?:\s|$)")
                .unwrap()
                .not(),
        )
        .stdout(
            predicate::str::is_match(r"(?m)^\s+sweep(?:\s|$)")
                .unwrap()
                .not(),
        )
        .stdout(
            predicate::str::is_match(r"(?m)^\s+withdraw(?:\s|$)")
                .unwrap()
                .not(),
        );
}

#[test]
fn init_help_does_not_expose_spend_terminology() {
    let mut cmd = Command::cargo_bin("pcli").unwrap();
    cmd.args(["init", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("view-only"))
        .stdout(predicate::str::contains("validator-governance-subkey"))
        .stdout(predicate::str::contains("soft-kms"))
        .stdout(
            predicate::str::is_match(r"(?m)^\s+spend(?:\s|$)")
                .unwrap()
                .not(),
        );
}
