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
just ci-preflight
# Formatting (auto-fix)
just fmt

# Linting
just check

# All unit tests
just test

# Go runtime and fast gnark proof checks
just go-check
just gnark-proof-tests

# Full slow gnark proof generation checks
just gnark-proof-tests-slow

# End-to-end smoke tests (if you touched transaction flow)
just smoke

just proto

cargo fmt --all 


```bash
# One-shot local run
just orbis-integration
```
