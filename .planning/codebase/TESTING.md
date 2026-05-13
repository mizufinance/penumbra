# Testing Patterns

**Analysis Date:** 2026-05-12

## Test Framework

**Runner:**
- Rust unit and integration tests use Cargo's built-in test harness and `cargo-nextest`; workspace commands are defined in `justfile`.
- CI runs `just ci-test`, which prefers `cargo nextest run --cargo-profile ci` and falls back to `cargo test --release --no-fail-fast`, as defined in `justfile` and used by `.github/workflows/rust.yml`.
- Go gnark tests use the standard Go `testing` package plus gnark helpers under `tools/gnark/`; commands are `just go-test`, `just go-vet`, and `just go-check` in `justfile`.
- Config: `.config/nextest.toml` exists but is empty; Cargo workspace config is in `Cargo.toml`; the Rust toolchain is pinned in `rust-toolchain.toml`.

**Assertion Library:**
- Rust uses standard `assert!`, `assert_eq!`, `matches!`, `Result` returns, and panics with concrete messages, as in `crates/core/component/compliance/src/audit_validation.rs` and `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`.
- CLI tests use `assert_cmd` and `predicates`, as in `crates/bin/pcli/tests/cli_surface.rs`, `crates/bin/pcli/tests/network_integration.rs`, and `crates/bin/pd/tests/network_integration.rs`.
- Parameterized Rust tests use `rstest`, as in `crates/bin/pd/tests/network_integration.rs` and `crates/bin/pindexer/tests/network_integration.rs`.
- Property tests use `proptest` and `proptest-derive`, as in `crates/test/tct-property-test/tests/witness.rs`, `crates/crypto/decaf377-ka/tests/proptests.rs`, and `crates/core/asset/src/asset.rs`.
- Go circuit tests use `github.com/consensys/gnark/test`, as in `tools/gnark/internal/circuits/family_test.go` and `tools/gnark/internal/compliance/dleq_test.go`.

**Run Commands:**
```bash
cargo test --release -p <crate> --lib
just test
just ci-test
just check
just go-test
just go-check
just gnark-proof-tests
just gnark-proof-tests-slow
just smoke
just integration-pcli
just integration-pclientd
just integration-pd
just integration-pindexer
just orbis-integration
just ci-preflight
```

## Test File Organization

**Location:**
- Put fast unit tests inline under `#[cfg(test)] mod tests` in the implementation module, as in `crates/core/component/compliance/src/audit_validation.rs`, `crates/core/asset/src/asset.rs`, and `crates/view/src/compliance_tree.rs`.
- Put crate integration tests in `tests/*.rs`, as in `crates/bin/pcli/tests/cli_surface.rs`, `crates/bin/pclientd/tests/network_integration.rs`, and `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`.
- Put shared integration helpers in `tests/common`, as in `crates/core/app-tests/tests/common/mod.rs`.
- Put property-test-only coverage in dedicated crates when it spans a domain surface, as in `crates/test/tct-property-test/tests/witness.rs` and `crates/test/tct-property-test/tests/simulate/`.
- Put Go gnark package tests beside Go packages under `tools/gnark/internal/**`, as in `tools/gnark/internal/primitives/statement_hash_test.go` and `tools/gnark/internal/circuits/transfer_metamorphic_test.go`.
- Put Criterion benchmarks under `benches/`, as in `crates/bench/benches/vanilla/nullifier_derivation.rs` and `crates/crypto/decaf377-fmd/benches/fmd.rs`.

**Naming:**
- Name Rust test functions by expected behavior or invariant, such as `tampered_tx_hash_is_invalid_evidence` in `crates/core/component/compliance/src/audit_validation.rs` and `transaction_send_from_addr_0_to_addr_1` in `crates/bin/pcli/tests/network_integration.rs`.
- Use `network_integration.rs`, `testnet.rs`, or explicit behavior names for tests that require external services, as in `crates/bin/pd/tests/network_integration.rs`, `crates/bin/pclientd/tests/testnet.rs`, and `crates/misc/measure/tests/testnet.rs`.
- Keep proptest regression seeds in `proptest-regressions/` beside the owning crate, as in `crates/crypto/tct/proptest-regressions`, `crates/core/num/proptest-regressions`, and `crates/wallet/proptest-regressions`.

**Structure:**
```text
crates/<crate>/src/**/*.rs               # implementation plus inline #[cfg(test)] unit tests
crates/<crate>/tests/*.rs                # crate-level integration tests
crates/<crate>/tests/common/*.rs         # shared integration helpers
crates/<crate>/proptest-regressions/**   # committed proptest failure seeds
crates/<crate>/benches/*.rs              # Criterion benchmarks
tools/gnark/internal/**/*_test.go        # Go gnark unit/circuit tests
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> AuditValidationInput {
        let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        AuditValidationInput {
            evidence,
            upload_bundle: Some(bundle),
            ring_pk,
        }
    }

    #[test]
    fn missing_upload_bundle_is_reported_without_panic() {
        let mut input = valid_input();
        input.upload_bundle = None;
        assert_eq!(
            validate_audit_evidence(input),
            AuditValidationStatus::MissingUploadBundle
        );
    }
}
```

The pattern above is from `crates/core/component/compliance/src/audit_validation.rs`: keep fixtures local, mutate one field per test, and assert the classified result.

**Patterns:**
- Use `#[tokio::test] async fn ...() -> anyhow::Result<()>` for async app and service tests, as in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs` and `crates/bin/pclientd/tests/network_integration.rs`.
- Use `#[ignore]` for tests that require a live network, daemon, external binary, local ports, or special proving setup, as in `crates/bin/pcli/tests/network_integration.rs`, `crates/bin/pd/tests/network_integration.rs`, and `crates/core/transaction/tests/generate_transaction_signing_test_vectors.rs`.
- Use `#[rstest]` and `#[case(...)]` for table-like parameterization, as in metrics checks in `crates/bin/pd/tests/network_integration.rs` and SQL null checks in `crates/bin/pindexer/tests/network_integration.rs`.
- Use `proptest!` for algebraic, serialization, tree, and numeric invariants, as in `crates/test/tct-property-test/tests/witness.rs`, `crates/core/num/src/fixpoint.rs`, and `crates/crypto/decaf377-ka/tests/proptests.rs`.
- Return `anyhow::Result<()>` from integration tests so `?` can carry context, as in `crates/bin/pindexer/tests/network_integration.rs` and `crates/core/app-tests/tests/mock_consensus_block_proving.rs`.
- Explicitly drop tracing guards or temporary storage when tests need deterministic teardown, as in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`.

## Mocking

**Framework:** In-repo mocks and process-level test doubles; no `mockall` or similar mocking framework detected.

**Patterns:**
```rust
let storage = TempStorage::new_with_penumbra_prefixes().await?;
let consensus = Consensus::new(storage.as_ref().clone());
let mut test_node = TestNode::builder()
    .single_validator()
    .with_penumbra_auto_app_state(app_state)?
    .init_chain(consensus)
    .await?;
let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
    .with_sync_to_storage(&storage)
    .await?;
```

This integration pattern appears in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs` and relies on helpers from `crates/test/mock-consensus/src/lib.rs`, `crates/test/mock-client/src/lib.rs`, and `crates/core/app-tests/tests/common/mod.rs`.

**What to Mock:**
- Use `penumbra-sdk-mock-consensus` from `crates/test/mock-consensus/src/lib.rs` for consensus-driven app tests instead of running CometBFT.
- Use `penumbra-sdk-mock-client` from `crates/test/mock-client/src/lib.rs` for wallet scanning, witness generation, and client-side note state in app tests.
- Use `TempStorage` and `tests/common` extensions from `crates/core/app-tests/tests/common/mod.rs` for isolated storage and state setup.
- Use subprocesses with `assert_cmd` for CLI behavior so the real binary surface is exercised, as in `crates/bin/pcli/tests/cli_surface.rs` and `crates/bin/pclientd/tests/network_integration.rs`.
- Use local HTTP/Postgres/daemon endpoints for smoke and indexer tests; `crates/bin/pindexer/tests/network_integration.rs` reads `PENUMBRA_POSTGRES_PORT` and `PENUMBRA_NODE_CMT_URL`.

**What NOT to Mock:**
- Do not mock canonical cryptographic, transaction, or proof encodings when parity matters. Use vectors and round trips from `crates/core/transaction/tests/signing_test_vectors/`, `crates/core/component/compliance/testdata/orbis_decaf377_dleq_fixture.json`, and `tools/gnark/internal/primitives/`.
- Do not replace CLI tests with direct function calls when command visibility or output is the contract; follow `crates/bin/pcli/tests/cli_surface.rs`.
- Do not mock gnark circuit semantics when the circuit constraint system is the target; use gnark `test.NewAssert` in `tools/gnark/internal/circuits/family_test.go`.

## Fixtures and Factories

**Test Data:**
```rust
let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
let tmpdir = tempfile::tempdir().unwrap();
let grpc_url: Url = std::env::var("PENUMBRA_NODE_PD_URL")
    .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
    .parse()
    .expect("failed to parse PENUMBRA_NODE_PD_URL");
```

Use fixture factories for valid baseline data and mutate only the field under test, following `crates/core/component/compliance/src/audit_validation.rs`. Use tempdirs and env-var defaults for CLI/network tests, following `crates/bin/pcli/tests/network_integration.rs`.

**Location:**
- Reusable Rust test keys live under `penumbra_sdk_keys::test_keys`, used from `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs` and `crates/bin/pcli/tests/network_integration.rs`.
- Compliance fixtures and helpers live in `crates/core/component/compliance/src/evidence.rs`, `crates/core/component/compliance/src/lib.rs` (`test_helpers`), and `crates/core/component/compliance/testdata/orbis_decaf377_dleq_fixture.json`.
- Transaction signing vectors live in `crates/core/transaction/tests/signing_test_vectors/` and are generated/checked by `crates/core/transaction/tests/generate_transaction_signing_test_vectors.rs`.
- Go gnark fixtures are loaded through helpers in `tools/gnark/internal/primitives/` and exercised by `tools/gnark/internal/circuits/family_test.go`.
- Proptest regressions live under crate-local `proptest-regressions/` directories such as `crates/crypto/tct/proptest-regressions` and `crates/core/component/stake/proptest-regressions`.

## Coverage

**Requirements:** No coverage threshold or coverage tool is enforced. Searches found no `llvm-cov`, `tarpaulin`, `grcov`, or Codecov configuration in `Cargo.toml`, `justfile`, `.github/`, or `docs/`.

**View Coverage:**
```bash
# Not configured by this repo.
# Use targeted tests plus CI parity commands instead:
just check
just test
just go-check
just gnark-proof-tests
just smoke
```

`docs/compliance/testing.md` explicitly describes mandatory semantic checks for compliance/gnark work and treats proof-generating release tests as special coverage when proving keys are available.

## Test Types

**Unit Tests:**
- Inline Rust unit tests validate pure parsing, formatting, validation, and conversion logic. Examples: `crates/core/asset/src/asset.rs`, `crates/core/component/compliance/src/audit_validation.rs`, `crates/core/component/compliance/src/tree.rs`, and `crates/core/transaction/src/memo.rs`.
- Go unit tests validate gnark helpers and package behavior, as in `tools/gnark/internal/artifacts/artifacts_test.go` and `tools/gnark/internal/primitives/statement_hash_test.go`.

**Integration Tests:**
- Rust crate integration tests under `tests/` exercise binary surfaces, app state transitions, network-facing behavior, and storage. Examples: `crates/core/app-tests/tests/*.rs`, `crates/bin/pcli/tests/*.rs`, `crates/bin/pclientd/tests/network_integration.rs`, `crates/bin/pd/tests/network_integration.rs`, and `crates/bin/pindexer/tests/network_integration.rs`.
- `crates/core/app-tests/Cargo.toml` disables auto-discovery with `autotests = false` and explicitly lists the active app integration tests.

**E2E Tests:**
- Local smoke tests are run by `just smoke` in `justfile` and `.github/workflows/smoke.yml`; they use `deployments/scripts/smoke-test.sh`.
- Orbis end-to-end tests are run by `just orbis-integration` in `justfile` and `.github/workflows/orbis-integration.yml`; they build `pcli`, `pclientd`, `pd`, `orbis-audit`, and `orbis-integration`.
- Network integration commands in `justfile` run ignored tests for `pcli`, `pclientd`, `pd`, and `pindexer`, using package-specific test files under `crates/bin/*/tests/`.

**Property Tests:**
- Use `proptest` for generated inputs and invariant preservation, as in `crates/test/tct-property-test/tests/witness.rs`, `crates/crypto/decaf377-ka/tests/proptests.rs`, and `crates/core/num/src/fixpoint.rs`.
- Commit regression seeds in `proptest-regressions/` directories, as in `crates/crypto/tct/proptest-regressions/storage/deserialize.txt`.

**Proof and Circuit Tests:**
- Fast gnark validation is `just gnark-proof-tests-fast` in `justfile`, which runs Go checks plus focused Rust library tests for `penumbra-sdk-shielded-pool`.
- Slow proof-generation coverage is `just gnark-proof-tests-slow` in `justfile`, which runs release-mode Rust tests with bundled proving keys.
- Go circuit mutation and assignment tests live under `tools/gnark/internal/circuits/` and `tools/gnark/internal/compliance/`.

**Benchmarks:**
- Criterion benchmarks live in `crates/bench/benches/vanilla/*.rs` and `crates/crypto/decaf377-fmd/benches/fmd.rs`; bench configuration is in `crates/bench/Cargo.toml` and `crates/crypto/decaf377-fmd/Cargo.toml`.

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn app_can_transfer_notes_and_detect_new_notes() -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;
    // exercise storage, consensus, client sync, and transaction execution
    drop(storage);
    drop(guard);
    Ok(())
}
```

Use this shape for async app/service tests, following `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`. Keep setup explicit and return `anyhow::Result<()>`.

**Error Testing:**
```rust
let mut input = valid_input();
input.evidence.tier_objects[0].dleq_challenge_bytes[0] ^= 1;
assert!(matches!(
    validate_audit_evidence(input),
    AuditValidationStatus::InvalidEvidence(_)
));
```

Use classified assertions for domain validation failures, following `crates/core/component/compliance/src/audit_validation.rs`. Use `assert!(result.is_err())` for simple parser/validation failures, as in `crates/core/asset/src/asset.rs`.

**CLI Testing:**
```rust
let mut cmd = Command::cargo_bin("pcli").unwrap();
cmd.args(["tx", "--help"]);
cmd.assert()
    .success()
    .stdout(predicate::str::contains("compliance"));
```

Use `assert_cmd::Command::cargo_bin` and `predicates` for command shape and output tests, following `crates/bin/pcli/tests/cli_surface.rs`.

**Go Circuit Testing:**
```go
assert := test.NewAssert(t)
assert.CheckCircuit(
    family.circuit(),
    test.WithCurves(ecc.BLS12_377),
    test.WithBackends(backend.GROTH16),
    test.WithValidAssignment(family.assignment(t)),
)
```

Use gnark's circuit assertions for valid and invalid assignments, following `tools/gnark/internal/circuits/family_test.go` and `tools/gnark/internal/compliance/dleq_test.go`.

## CI and Verification Surfaces

- Rust CI is defined in `.github/workflows/rust.yml`; it runs lint/check, feature checks, nextest, Go gnark checks, and gnark-backed Rust proof tests.
- Protobuf lint and generated-code drift checks are defined in `.github/workflows/buf-pull-request.yml` and use `proto/buf.yaml`, `proto/buf.gen.yaml`, and `deployments/scripts/protobuf-codegen`.
- Docs lint builds rustdocs through `.github/workflows/docs-lint.yml` and `deployments/scripts/rust-docs`.
- Smoke integration is defined in `.github/workflows/smoke.yml` and runs `just smoke`.
- Orbis integration is defined in `.github/workflows/orbis-integration.yml` and runs `just orbis-integration`.

---

*Testing analysis: 2026-05-12*
