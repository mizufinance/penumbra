# Testing Guide

## Prerequisites

**Recommended**: Use nix for the correct toolchain (includes cargo-nextest):
```bash
nix develop
```

**Without nix**: Install dependencies manually:
```bash
# cargo-nextest (required for `just test`)
# Note: Requires compatible Rust version - check rust-toolchain.toml
cargo install cargo-nextest

# For smoke/integration tests: clean network state
pd network unsafe-reset-all
```

## Quick Reference

| Command | Scope | When to Use |
|---------|-------|-------------|
| `cargo test --release -p <crate> --lib` | Single crate | Active development |
| `just test` | All unit tests (nextest) | Before commit |
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
```

### 2. Before Commit

Run all unit tests to catch regressions:

```bash
# With nextest (faster, parallel)
just test

# Without nextest (fallback)
cargo test --release --workspace --lib
```

### 3. Before PR

Run the same checks as CI:

```bash
# Formatting (auto-fix)
just fmt

# Linting
just lint

# All unit tests
just test

# End-to-end smoke tests (if you touched transaction flow)
just smoke
```

## CI/CD Pipeline

### rust.yml (Every PR)

| Job | Description |
|-----|-------------|
| `rustfmt` | Code formatting check |
| `clippy` | Linting warnings |
| `features` | Feature flag combinations compile |
| `test` | All unit tests via cargo-nextest |

### smoke.yml (Main/Release Branches)

| Job | Description |
|-----|-------------|
| `smoke` | Full end-to-end smoke tests |
| `pmonitor` | pmonitor integration tests |

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
`PENUMBRA_LIGHTWEIGHT_TRANSFER_ONLY_PHASE=1`. Removed-action integration tests
must check that flag and skip at runtime; `#[ignore]` alone is not sufficient
because the smoke suite runs ignored tests explicitly.

**Note**: Smoke tests expect nix environment. Warning can be ignored if dependencies are installed.

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
