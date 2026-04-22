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

# Fast gnark circuit/gadget loop (Go tests only)
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
| `gnark-rust` | Bundled gnark transfer/split/consolidate proof generation |

## Transfer Artifacts

`Transfer` is now one semantic action and one bundled artifact set. The hidden
arity used by the proving implementation remains internal; there is no active
transfer-shape surface to manage in normal development.

### smoke.yml (Every PR)

| Job | Description |
|-----|-------------|
| `smoke` | Full end-to-end smoke tests with bundled gnark features |

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

### Penumbra + Orbis Integration

Penumbra now treats Orbis as an external dependency. The Orbis network and
SourceHub lifecycle come from the vendored runtime contract in
`deployments/orbis/`, and Penumbra owns the typed integration flow on top of
that runtime.

#### Prerequisites

```bash
# Build Penumbra binaries
cargo build --release -p pcli -p pclientd --features bundled-proving-keys
cargo build --release -p pd -p orbis-audit -p orbis-integration
```

#### Script Overview

| Command | Requires | Description |
|---------|----------|-------------|
| `./scripts/penumbra-up.sh` | built `pd` + `pcli` + `pclientd` | Start Penumbra devnet, wallets, and persistent view daemons |
| `./scripts/orbis-stack.sh up` | Docker | Start SourceHub + 3 Orbis nodes from the vendored runtime contract |
| `./target/release/orbis-integration seed` | Penumbra + Orbis up | Run DKG + registrations + split/transfer/consolidate |
| `./target/release/orbis-integration verify` | `seed` completed | Rerunnable progressive-disclosure verification |
| `just orbis-integration` | same prerequisites as above | One-shot CI-style bring-up, seed, verify, teardown |

All artifacts go to `tmp/`.

#### Recommended Local Flow

One-shot CI-style flow:

```bash
just orbis-integration
```

Manual phased flow:

```bash
./scripts/penumbra-up.sh
./scripts/orbis-stack.sh up
./target/release/orbis-integration seed
./target/release/orbis-integration verify
```
`orbis-integration verify` is read-only and can be rerun against the same
seeded chain state any number of times.

#### What The Seed + Verify Phases Cover

`orbis-integration seed` performs:
1. DKG to establish the Orbis ring (threshold 2-of-3)
2. Issuer detection-key generation
3. Regulated asset registration
4. User registration for Alice, Bob, and Charlie
5. One split, regulated transfers, and one consolidate
6. Negative checks for unregistered users and unregulated assets

`orbis-integration verify` performs:
1. Detection-only chain scanning
2. Core-tier PRE for Alice and Bob
3. Extension-tier PRE for Alice and Bob
4. Verification that Charlie remains encrypted because no PRE is requested

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

All provided bycvf `nix develop`. Without nix, install manually or use the manual pd+cometbft method above.

## Tips

1. **Run tests in background**: Use `just test &` while continuing development
2. **Watch mode**: Use `cargo watch -x 'test -p <crate>'` for auto-run on save
3. **Filter tests**: `cargo test -p <crate> test_prefix` runs matching tests
4. **Skip slow tests**: `cargo test -p <crate> --lib` skips integration tests
