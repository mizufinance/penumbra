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
| `just go-test` | `tools/gnark` Go tests only | Fast circuit/gadget/transfer-family iteration |
| `just go-check` | `tools/gnark` format/build/test/vet | Before commit on gnark changes |
| `just gnark-proof-tests` | Fast gnark inner-loop checks | During spend/output/transfer development |
| `just gnark-proof-tests-slow` | End-to-end gnark proof generation | Before PR on spend/output/transfer changes |
| `just smoke` | End-to-end | Before PR (transaction changes) |
| `just integration-pcli` | pcli tests | Before PR (CLI changes) |
| `just integration-pmonitor` | pmonitor tests | Before PR (monitoring changes) |

## Development Workflow

### 1. Active Development

Run tests for the specific crate you're modifying:

```bash
# Unit tests for a crate
cargo test --release -p penumbra-sdk-compliance --lib

# Specific test
cargo test --release -p penumbra-sdk-compliance --lib test_name

# With output
cargo test --release -p penumbra-sdk-compliance --lib -- --nocapture

# Fast gnark circuit/gadget/transfer-family loop (Go tests only)
just go-test
```

### 2. Before Commit

Run all unit tests to catch regressions:

```bash
# With nextest (faster, parallel)
just test

# Go-side gnark checks
just go-check

# Fast Go-only circuit iteration
just go-test

# Without nextest (fallback)
cargo test --release --workspace --lib
```

### 3. Before PR

Run the same checks as CI:

```bash
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
```

## CI/CD Pipeline

### rust.yml (Every PR)

| Job | Description |
|-----|-------------|
| `lint` | Rust `cargo check` + formatting check |
| `features` | Feature flag combinations compile |
| `test` | All unit tests via cargo-nextest |
| `go-gnark` | `tools/gnark` format/build/test/vet |
| `gnark-rust` | Bundled gnark spend/output/transfer proof generation |

## Adding a Transfer Family

Transfer proving uses one generic transfer library and one generic
`transfer(n_in, n_out)` circuit implementation, but each supported family still
needs its own proving key, verifying key, and artifact directory.

To add a new family such as `3x3`:

```bash
# 1. Add the new family entry.
$EDITOR tools/gnark/transfer_families.json

# 2. Regenerate transfer-family bindings.
cd tools/gnark
GOCACHE=/tmp/penumbra-go-cache go run ./cmd/gen-transfer-families

# 3. Generate setup artifacts and keys for the new family.
GOCACHE=/tmp/penumbra-go-cache go run ./cmd/gnarkctl setup \
  --circuit transfer3x3 \
  --out-dir artifacts/transfer3x3

# 4. Copy bundled artifacts into proof params.
cd ../..
cp -R tools/gnark/artifacts/transfer3x3 \
  crates/crypto/proof-params/src/gen/gnark/transfer3x3

# 5. Rebuild and test.
just go-test
cargo check -p penumbra-sdk-shielded-pool
cargo check -p penumbra-sdk-proof-aggregation
```

The transfer-family generator now owns the Rust and Go registry wiring. After
this refactor, adding a new supported family should not require handwritten Rust
or Go source changes outside the manifest.

### smoke.yml (Every PR)

| Job | Description |
|-----|-------------|
| `smoke` | Full end-to-end smoke tests with bundled gnark features |
| `pmonitor` | pmonitor integration tests with bundled gnark features |

## Running Smoke Tests Locally

Smoke tests require a clean local devnet state.

```bash
# Clean existing network data (REQUIRED before smoke tests)
pd network unsafe-reset-all

# Build binaries
cargo build --release -p pd -p pcli

# Install cometbft consensus engine
go install github.com/cometbft/cometbft/cmd/cometbft@v0.37.15

# Run smoke tests
just smoke

# Run pmonitor integration tests
just integration-pmonitor
```

The smoke test:
1. Starts a local network with multiple validators
2. Creates wallets and funds them
3. Runs transaction scenarios
4. Validates chain state

For the current lightweight-chain branch, `just smoke` exports
`PENUMBRA_REDUCED_ACTION_SURFACE=1`. Removed-action integration tests
must check that flag and skip at runtime; `#[ignore]` alone is not sufficient
because the smoke suite runs ignored tests explicitly.

**Note**: Smoke tests and devnet orchestration remain nix-based. Normal Rust+gnark development no longer requires nix, but `just smoke` still assumes the nix environment.

## Compliance-Specific Tests

```bash
# Unit tests
cargo test --release -p penumbra-sdk-compliance --lib

# Integration tests
cargo test --release -p penumbra-sdk-app-tests --test compliance_full_flow

# Planner tests
cargo test --release -p penumbra-sdk-view --lib planner::tests
```

### Demo Scripts (Local Devnet)

End-to-end demos on a local devnet with real Orbis nodes.

#### Prerequisites

```bash
# Build Penumbra binaries
cargo build --release -p pcli -p pd -p orbis-audit

# Install external tools (from orbis-rs repo)
#   orbis-node, cli-tool  — with decaf377 feature
#   sourcehubd            — from sourcehub repo
```

#### Scripts Overview

| Command | Requires | Description |
|---------|----------|-------------|
| `./scripts/setup-penumbra.sh` | — | Penumbra devnet: pd + cometbft + wallets (Terminal 1, stays running) |
| `./scripts/setup-orbis.sh` | — | Orbis network: SourceHub + 3 MPC nodes (Terminal 2, stays running) |
| `./scripts/setup-tx.sh` | Both setups running | DKG + registrations + transfers (run once) |
| `./scripts/test-orbis-primitives.sh` | setup-orbis.sh | Orbis crypto tests (DKG, FROST, DLEQ, PRE) |
| `./scripts/test-orbis-scanning.sh` | setup-tx.sh completed | Progressive disclosure demo (rerunnable) |

All artifacts (logs, wallets, keys, scan data) go to `tmp/`.

#### Infrastructure Setup

Start each in a separate terminal:

```bash
# Terminal 1: Penumbra devnet (pd + cometbft + wallets)
./scripts/setup-penumbra.sh

# Terminal 2: Orbis network (SourceHub + 3 MPC nodes)
./scripts/setup-orbis.sh
```

Both scripts stay in the foreground and clean up on Ctrl+C.

#### Transaction Setup (run once)

After both setups are ready:

```bash
./scripts/setup-tx.sh
```

This runs all chain-writing operations once:
1. DKG to establish Orbis ring (threshold 2-of-3)
2. Generates issuer detection key (DK)
3. Registers regulated asset (threshold: 500 display units)
4. Registers users (Alice, Bob, Charlie)
5. Executes regulated transfers (including 1 flagged above threshold)
6. Tests edge cases (unregistered user rejection, unregulated asset transfer)

Keys are saved to `tmp/` so the scanning demo can reuse them.

#### Scanning Demo (rerunnable)

```bash
./scripts/test-orbis-scanning.sh
```

Read-only analysis that can be re-run any number of times against the same chain:
1. Scans chain and populates issuer database
2. Shows **STATE 1**: detection-only (flagged transactions auto-decrypted via DK)
3. Performs Orbis PRE for Alice & Bob (core tier) → **STATE 2**: amounts + self-addresses
4. Performs Orbis PRE for Alice & Bob (extension tier) → **STATE 3**: counterparty addresses

Charlie is deliberately not audited — his non-flagged transactions stay encrypted,
demonstrating that PRE is selective per-user access.

#### Orbis Crypto Tests

```bash
./scripts/test-orbis-primitives.sh
```

Runs the `orbis-test` binary against real Orbis nodes:
- DKG, FROST threshold signatures, DLEQ proofs, 3-tier PRE
- Includes negative tests (unauthorized users, invalid proofs)

#### Quick Start

```bash
# Terminal 1
./scripts/setup-penumbra.sh

# Terminal 2
./scripts/setup-orbis.sh

# Terminal 3 (after both setups are ready)
./scripts/setup-tx.sh
./scripts/test-orbis-scanning.sh

# Re-run scanning demo as many times as needed
./scripts/test-orbis-scanning.sh
```

## Troubleshooting

### Smoke tests fail with "Address already in use"

Kill lingering processes from previous runs:
```bash
# Kill processes on required ports
lsof -ti:8080 -ti:9000 -ti:26657 -ti:26658 | xargs kill -9 2>/dev/null

# Stop process-compose
process-compose down --port 8888 2>/dev/null

# Clean state
pd network unsafe-reset-all
```

### "Network did not produce blocks within timeout"

Check logs at `deployments/logs/dev-env-combined.log` for errors.

Common causes:
- **Port conflicts** - see cleanup commands above
- **Missing dependencies** - smoke tests require prometheus, postgresql, grpcui (use nix)
- **Corrupted network state** - clean with `rm -rf ~/.penumbra/network_data`

### Running integration tests manually (without nix)

If `just smoke` fails due to missing dependencies, run the full integration test flow manually:

```bash
# 1. Clean state
rm -rf ~/.penumbra/network_data /tmp/pcli-test

# 2. Generate network with test allocations
cargo run --release --bin pd -- network generate \
  --chain-id test-local \
  --validators-input-file testnets/validators-single.json \
  --allocations-input-file deployments/compose/devnet-allocations.csv

# 3. Start pd (terminal 1)
cargo run --release --bin pd -- start

# 4. Start cometbft (terminal 2)
cometbft --home ~/.penumbra/network_data/node0/cometbft start

# 5. Verify blocks are produced
curl -s http://127.0.0.1:26657/status | grep latest_block_height
```

Once the network is running, initialize wallet and register compliance:

```bash
# Initialize wallet with test seed phrase
mkdir -p /tmp/pcli-test
echo "comfort ten front cycle churn burger oak absent rice ice urge result art couple benefit cabbage frequent obscure hurry trick segment cool job debate" | \
  cargo run --release --bin pcli -- --home /tmp/pcli-test init --grpc-url "http://127.0.0.1:8080" soft-kms import-phrase

# Verify wallet has funds
cargo run --release --bin pcli -- --home /tmp/pcli-test view balance

# Register test assets for compliance
cargo run --release --bin pcli -- --home /tmp/pcli-test tx compliance register-asset gm --unregulated
cargo run --release --bin pcli -- --home /tmp/pcli-test tx compliance register-user gm
cargo run --release --bin pcli -- --home /tmp/pcli-test tx compliance register-user test_usd
```

Run integration tests:

```bash
# Run specific integration test
cargo test --release --features sct-divergence-check,download-proving-keys \
  --package pcli -- --ignored --test-threads 1 --nocapture \
  transaction_send_from_addr_0_to_addr_1

# List all available integration tests
cargo test --release --features sct-divergence-check,download-proving-keys \
  --package pcli -- --ignored --list

# Cleanup when done
lsof -ti:8080 -ti:26657 -ti:26658 | xargs kill -9 2>/dev/null
```

### Smoke test dependencies (for `just smoke`)

The full smoke test suite requires:
- `cometbft` - CometBFT 0.37.15
- `process-compose` - process orchestration
- `grpcurl` - gRPC CLI
- `prometheus` - metrics (optional, will warn)
- `postgresql` - event indexing (optional, will warn)

All provided by `nix develop`. Without nix, install manually or use the manual pd+cometbft method above.

## orbis-sim (Test Harness)

`orbis-sim` holds `sk_ring` directly for testing without a real Orbis network.

```bash
# Print ring_pk derived from the hardcoded sk_ring
cargo run --release -p orbis-sim -- --derive-ring-pk

# Pass sk_ring explicitly
cargo run --release -p orbis-sim -- --sk-ring-hex <hex>

# Derive b_d from a sender address
cargo run --release -p orbis-sim -- --sender-address <address>
```

## Tips

1. **Run tests in background**: Use `just test &` while continuing development
2. **Watch mode**: Use `cargo watch -x 'test -p <crate>'` for auto-run on save
3. **Filter tests**: `cargo test -p <crate> test_prefix` runs matching tests
4. **Skip slow tests**: `cargo test -p <crate> --lib` skips integration tests
