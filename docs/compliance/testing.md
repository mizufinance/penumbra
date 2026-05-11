# Testing Guide

## Prerequisites

**Recommended for CI parity**: Use nix for the correct toolchain (includes cargo-nextest and the bundled gnark runtime toolchain):
```bash
nix develop
```

**For day-to-day local Rust/gnark work without nix**: Install dependencies manually:
```bash
# cargo-nextest (required for `just test`)
# Note: Requires compatible Rust version - check rust-toolchain.toml
cargo install cargo-nextest

# Go toolchain for tools/gnark and bundled gnark runtime compilation
# plus a CGO-capable C toolchain (clang or gcc)

# For smoke/integration tests: clean network state
pd network unsafe-reset-all
```

## Quick Reference

| Command | Scope | When to Use |
|---------|-------|-------------|
| `cargo test --release -p <crate> --lib` | Single crate | Active development |
| `just test` | All unit tests (nextest) | Before commit |
| `just go-test` | `tools/gnark` Go tests only | Fast circuit/gadget iteration |
| `just go-check` | `tools/gnark` format/build/test/vet | Before commit on gnark changes |
| `just gnark-proof-tests` | Fast gnark inner-loop checks | During transfer/split/consolidate development |
| `just gnark-proof-tests-slow` | End-to-end gnark proof generation | Before PR on shielded-action changes |
| `just smoke` | End-to-end | Before PR (transaction changes) |
| `just integration-pcli` | pcli tests | Before PR (CLI changes) |

## Scanner Core

Use these when changing issuer compliance scanning:

```bash
cargo test -p penumbra-sdk-compliance --lib scanner::
cargo test -p penumbra-sdk-compliance --lib audit::
cargo test -p penumbra-sdk-transaction compliance_scanner_transaction_id_matches_canonical_transaction_id --lib
cargo check -p penumbra-sdk-compliance -p penumbra-sdk-transaction -p pcli -p orbis-audit -p orbis-integration
```

The transaction parity test is mandatory: the scanner-side transaction hash
helper must continue to match `Transaction::id()`.

Smoke the scanner CLI shape with:

```bash
pcli tx compliance scan run --node http://127.0.0.1:8080 --db /tmp/compliance-scanner.db --dk-hex <hex> --scan-asset-id <asset>
pcli tx compliance scan catch-up --node http://127.0.0.1:8080 --db /tmp/compliance-scanner.db --dk-hex <hex> --scan-asset-id <asset>
```

There is no legacy JSON scan-output or issuer-db command surface. Audit-demo
exports frontend-compatible `scan`, `scanner`, `ledgerRows`, and `audits` state
from the scanner DB.

## Standard Preflight

```bash
just ci-preflight
cargo fmt --all
just fmt
just check
just test
just go-check
just gnark-proof-tests
just gnark-proof-tests-slow
just smoke
just proto
```

```bash
# One-shot local Orbis run
just orbis-integration
```
