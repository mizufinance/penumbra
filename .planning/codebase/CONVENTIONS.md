# Coding Conventions

**Analysis Date:** 2026-05-12

## Naming Patterns

**Files:**
- Use Rust `snake_case.rs` module files under crate `src/` trees, as in `crates/core/component/shielded-pool/src/transfer/plan.rs`, `crates/core/component/compliance/src/audit_validation.rs`, and `crates/view/src/note_manager.rs`.
- Put Rust integration tests in `tests/*.rs` with descriptive snake_case names, as in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`, `crates/bin/pcli/tests/cli_surface.rs`, and `crates/bin/pd/tests/network_integration.rs`.
- Put Go gnark tests beside package code as `*_test.go`, as in `tools/gnark/internal/circuits/family_test.go`, `tools/gnark/internal/compliance/dleq_test.go`, and `tools/gnark/internal/artifacts/artifacts_test.go`.
- Use crate package names that identify ownership and layer, such as `penumbra-sdk-shielded-pool` in `crates/core/component/shielded-pool/Cargo.toml`, `penumbra-sdk-app-tests` in `crates/core/app-tests/Cargo.toml`, and binary crates like `pcli` in `crates/bin/pcli/Cargo.toml`.

**Functions:**
- Use Rust `snake_case` for public APIs, helpers, and tests. Examples: `validate_audit_evidence` in `crates/core/component/compliance/src/audit_validation.rs`, `transfer_public_private` in `crates/core/component/shielded-pool/src/transfer/plan.rs`, and `app_can_transfer_notes_and_detect_new_notes` in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`.
- Prefer short names that avoid repeating the crate or module name. `sweep` and `sweep_notes` in `crates/wallet/src/plan.rs` are the local pattern.
- Name tests by the behavior or invariant being checked, as in `missing_upload_bundle_is_reported_without_panic` in `crates/core/component/compliance/src/audit_validation.rs` and `tx_help_exposes_only_reduced_surface_commands` in `crates/bin/pcli/tests/cli_surface.rs`.
- Use Go `TestXxx` functions with table subtests where appropriate, as in `TestCircuitFamiliesRejectWrongStatementHash` in `tools/gnark/internal/circuits/family_test.go` and `TestDoubleBaseScalarMulMatchesNaiveImplementation` in `tools/gnark/internal/compliance/dleq_test.go`.

**Variables:**
- Use Rust `snake_case` for locals and fields, with short protocol abbreviations where established. Examples include `fvk`, `asset_id`, `memo_key`, and `state_commitment_proofs` in `crates/core/component/shielded-pool/src/transfer/plan.rs`.
- Use deterministic ordered collections for persistent or serialized data. Prefer `BTreeMap` and `BTreeSet`, following `clippy.toml`, `crates/wallet/src/plan.rs`, `crates/test/mock-client/src/lib.rs`, and `crates/core/component/compliance/src/tree.rs`.
- Use `HashMap` or `HashSet` only when stable iteration is not part of the contract or the algorithm requires it, as in threshold/FROST state in `crates/custody/src/threshold/dkg.rs` and cache/index state in `crates/core/app/src/local_mempool.rs`.

**Types:**
- Use `UpperCamelCase` for Rust structs, enums, traits, and enum variants, as in `TransferPlan` in `crates/core/component/shielded-pool/src/transfer/plan.rs`, `AuditValidationStatus` in `crates/core/component/compliance/src/audit_validation.rs`, and `MockClient` in `crates/test/mock-client/src/lib.rs`.
- Use `SCREAMING_SNAKE_CASE` for constants, as in `PADDED_TRANSFER_INPUTS` exported from `crates/core/component/shielded-pool/src/lib.rs` and `TEST_ASSET` in `crates/bin/pcli/tests/network_integration.rs`.
- Encode state/result classifications as explicit enums instead of strings, following `AuditValidationStatus` in `crates/core/component/compliance/src/audit_validation.rs` and `TransferPlanningResult` usage in `crates/wallet/src/plan.rs`.

## Code Style

**Formatting:**
- Use `cargo fmt --all` from `just fmt` in `justfile`. No `rustfmt.toml` is present, so use standard rustfmt output.
- Use the pinned Rust toolchain from `rust-toolchain.toml` (`channel = "1.89"`) and keep `rustfmt` available through that toolchain.
- Use `gofmt` for all Go code under `tools/gnark/`; `just go-fmt` and `just go-fmt-check` in `justfile` are the local entry points.
- Keep Cargo manifests readable and grouped by role. Workspace dependencies live in `Cargo.toml`; crate-specific features and dev dependencies live in files like `crates/bin/pcli/Cargo.toml` and `crates/core/component/shielded-pool/Cargo.toml`.

**Linting:**
- `just check` in `justfile` runs `RUSTFLAGS="-D warnings" cargo check --release --all-targets --all-features --target-dir=target/check` and `cargo fmt --all -- --check`.
- `clippy.toml` disallows `std::collections::HashMap` and documents the stable-order preference for `BTreeMap`; it also permits unwraps in tests with `allow-unwrap-in-tests = true`.
- Many crates deny production unwraps with `#![deny(clippy::unwrap_used)]`, including `crates/core/app/src/lib.rs`, `crates/core/component/shielded-pool/src/lib.rs`, `crates/core/transaction/src/lib.rs`, `crates/view/src/lib.rs`, and `crates/bin/pcli/src/lib.rs`.
- Keep `#[allow(...)]` scoped to the smallest module or item. Examples include generated/proto allowances in `crates/proto/src/lib.rs`, protocol shape allowances in `crates/crypto/tct/src/internal/complete/node/children.rs`, and targeted `dead_code` allowances in `crates/core/app-tests/tests/common/mod.rs`.

## Import Organization

**Order:**
1. Standard library imports first, either as direct `use std::...` or a grouped `use std::{...}` block, as in `crates/wallet/src/plan.rs`, `crates/bin/pcli/tests/network_integration.rs`, and `crates/bin/pd/src/main.rs`.
2. External crates next, such as `anyhow`, `tracing`, `tokio`, `serde`, `proptest`, and `assert_cmd`, as in `crates/core/component/compliance/src/audit_validation.rs` and `crates/bin/pcli/tests/cli_surface.rs`.
3. Workspace crates next, using explicit `penumbra_sdk_*` paths, as in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs` and `crates/wallet/src/plan.rs`.
4. Local imports last, using `crate::`, `super::`, or `self::`, as in `crates/core/component/shielded-pool/src/transfer/plan.rs` and `crates/core/app-tests/tests/common/mod.rs`.

**Path Aliases:**
- No TypeScript-style path aliases are used. Rust dependencies are centralized through `[workspace.dependencies]` in `Cargo.toml` and referenced with `{ workspace = true }` in crate manifests like `crates/bin/pd/Cargo.toml`.
- Use workspace crate names rather than relative cross-crate paths. Examples: `penumbra_sdk_transaction` in `crates/wallet/src/plan.rs`, `penumbra_sdk_mock_consensus` in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`, and `penumbra_sdk_proto` in `crates/bin/pd/tests/network_integration.rs`.
- Use `as _` for trait imports that only enable methods, as in `common::TempStorageExt as _` and `penumbra_sdk_sct::component::tree::SctRead as _` in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`.

## Error Handling

**Patterns:**
- Use `anyhow::Result<T>` for binary entry points, integration tests, service wiring, and effect-heavy code. Examples: `main` in `crates/bin/pd/src/main.rs`, `transaction_send_flow` in `crates/bin/pclientd/tests/network_integration.rs`, and `sweep` in `crates/wallet/src/plan.rs`.
- Add context at IO, network, storage, parsing, and spawned-task boundaries with `Context` or `with_context`, following `crates/wallet/src/plan.rs`, `crates/bench-support/src/proof_txs.rs`, and `crates/bin/pindexer/tests/network_integration.rs`.
- Use `anyhow::ensure!` and `anyhow::bail!` for validation gates, as in `crates/core/component/shielded-pool/src/transfer/plan.rs`, `crates/core/component/compliance/src/audit_validation.rs`, and `crates/bin/pclientd/tests/network_integration.rs`.
- Use typed domain errors when callers need to classify failures. Examples include `ProofError` exported from `crates/core/component/shielded-pool/src/lib.rs` and `AuditValidationStatus` in `crates/core/component/compliance/src/audit_validation.rs`.
- Convert protobuf/domain boundaries with `TryFrom` and explicit `"missing ..."` or `"malformed ..."` messages, following `TryFrom<pb::TransferPlan>` in `crates/core/component/shielded-pool/src/transfer/plan.rs`.
- Avoid `unwrap()` in production code because crate-level `#![deny(clippy::unwrap_used)]` is common. Use `expect` only for internal invariants with a concrete message, as in `first_spend` in `crates/core/component/shielded-pool/src/transfer/plan.rs`; tests may use `unwrap` for fixture setup, as in `crates/bin/pcli/tests/cli_surface.rs`.

## Logging

**Framework:** `tracing`

**Patterns:**
- Use structured `tracing` fields rather than string interpolation where possible, as in `tracing::info!(?cmd, version = env!("CARGO_PKG_VERSION"), "running command")` in `crates/bin/pd/src/main.rs`.
- Annotate async or effectful operations with `#[tracing::instrument]` or `#[instrument(skip(...))]`, as in `crates/wallet/src/plan.rs`, `crates/test/mock-consensus/src/lib.rs`, and `tools/picturesque/src/cometbft.rs`.
- Initialize binary logging with `tracing_subscriber` and `EnvFilter`, following `crates/bin/pd/src/main.rs` and `tools/picturesque/src/main.rs`.
- Use `penumbra-sdk-test-subscriber` for integration tests that need captured logs, as in `crates/core/app-tests/tests/common/mod.rs` and `crates/test/tracing-subscriber/src/lib.rs`.
- Use `tracing::debug!` for expected branch diagnostics and `tracing::warn!` or `tracing::error!` for operational failures, as in `crates/core/component/shielded-pool/src/component/transfer.rs` and `crates/bin/pd/src/main.rs`.

## Comments

**When to Comment:**
- Add module docs with `//!` for crates, test modules, and integration surfaces, as in `crates/test/mock-consensus/src/lib.rs`, `crates/bin/pcli/tests/network_integration.rs`, and `crates/bin/pd/tests/network_integration.rs`.
- Comment protocol constraints, invariants, and test assumptions. Examples: compliance setup notes in `crates/core/app-tests/tests/common/mod.rs`, binary padding notes in `crates/core/component/shielded-pool/src/transfer/plan.rs`, and devnet assumptions in `crates/bin/pindexer/tests/network_integration.rs`.
- Do not add comments that repeat names. The root guidance in `AGENTS.md` prefers concise factual docs and public API docs that explain ownership, invariants, inputs, outputs, or failure modes.

**JSDoc/TSDoc:**
- Not applicable. No TypeScript source is part of the active repo scan. Use Rustdoc `///` and module docs `//!` for public Rust APIs, as in `crates/test/mock-consensus/src/lib.rs` and `crates/core/component/compliance/src/audit_validation.rs`.
- Use Go comments only where they clarify exported behavior or non-obvious circuit/test assumptions, following `tools/gnark/internal/circuits/family_test.go` and `tools/gnark/internal/compliance/dleq_test.go`.

## Function Design

**Size:** Keep pure validation and transformation helpers focused; split effectful orchestration from shape/invariant checks. Examples: `validate_evidence_shape` and `validate_upload_bundle` in `crates/core/component/compliance/src/audit_validation.rs`, and `validate_invariants`, `transfer_body`, and `transfer_public_private` in `crates/core/component/shielded-pool/src/transfer/plan.rs`.

**Parameters:** Pass stores, clients, providers, and RNGs explicitly rather than constructing hidden globals in core logic. Examples include generic `ViewClient` and `RngCore + CryptoRng` parameters in `crates/wallet/src/plan.rs`, `StateRead`/storage inputs in `crates/test/mock-client/src/lib.rs`, and explicit test storage/client setup in `crates/core/app-tests/tests/app_can_transfer_notes_and_detect_new_notes.rs`.

**Return Values:** Return `Result` at fallible boundaries and return explicit classification values for validation workflows. Examples: `anyhow::Result<Vec<TransactionPlan>>` in `crates/wallet/src/plan.rs`, `Result<(TransferProofPublic, TransferProofPrivate), crate::ProofError>` in `crates/core/component/shielded-pool/src/transfer/plan.rs`, and `AuditValidationStatus` in `crates/core/component/compliance/src/audit_validation.rs`.

## Module Design

**Exports:** Use `src/lib.rs` as the crate facade: keep implementation modules private with `mod`, expose intended API with `pub mod` and `pub use`. Examples: `crates/core/component/shielded-pool/src/lib.rs`, `crates/core/component/compliance/src/lib.rs`, `crates/core/asset/src/lib.rs`, and `crates/view/src/lib.rs`.

**Barrel Files:** Rust crate facades are the barrel pattern. Add new public exports in the owning crate's `src/lib.rs` only when they are intended as part of that crate API, following `crates/core/component/shielded-pool/src/lib.rs` and `crates/core/transaction/src/lib.rs`.

**Feature Gates:** Put optional component, RPC, benchmark, proving-key, and integration behavior behind Cargo features in each crate manifest. Examples: `component`, `bundled-proving-keys`, `download-proving-keys`, and `benchmark-helpers` in `crates/core/component/shielded-pool/Cargo.toml`; `integration-testnet` in `crates/bin/pcli/Cargo.toml`; and `network-integration` in `crates/bin/pindexer/Cargo.toml`.

**Test Helpers:** Keep reusable test machinery in dedicated test crates or `tests/common`. Examples: `crates/test/mock-client/src/lib.rs`, `crates/test/mock-consensus/src/lib.rs`, `crates/test/tracing-subscriber/src/lib.rs`, and `crates/core/app-tests/tests/common/mod.rs`.

## Project Workflow Constraints

- Follow the product-level engineering constraints in `AGENTS.md`: prefer correct design over compatibility shims, keep durable typed state as the integration spine, separate pure domain logic from effects, use typed references and canonical identifiers, and validate before completing downstream work.
- GSD workflow files under `.codex/skills/*/SKILL.md` are orchestration instructions, not product architecture. The mapping workflow in `.codex/skills/gsd-map-codebase/SKILL.md` writes codebase maps under `.planning/codebase/`; product code should stay under the owning `crates/`, `tools/`, `scripts/`, `proto/`, `docs/`, or `deployments/` path.

---

*Convention analysis: 2026-05-12*
