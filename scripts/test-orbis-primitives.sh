#!/bin/bash
# Orbis Crypto Integration Tests
#
# Runs the orbis-test binary against real Orbis nodes, testing:
#   - Distributed Key Generation (DKG) with 3 nodes
#   - FROST threshold signatures (2-of-3) for ACK generation
#   - DLEQ (Discrete Log Equality) proofs for PRE correctness
#   - 3-tier PRE: detection, core (amount+self), extension (counterparty)
#   - Negative tests (unauthorized users, invalid proofs)
#
# Prerequisites:
#   - setup-orbis.sh running (SourceHub + 3 Orbis nodes)
#   - cargo build --release -p orbis-test
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_banner "Orbis Crypto Integration Tests" \
    "DKG, FROST signatures, DLEQ proofs, 3-tier PRE"

# --- Prerequisites ---
print_phase "Prerequisites"

log_info "Checking Orbis nodes..."
for port in 50051 50052 50053; do
    if ! nc -z -w1 127.0.0.1 "$port" 2>/dev/null; then
        log_error "Orbis node on port $port not reachable."
        log_error "Run: ./scripts/setup-orbis.sh"
        exit 1
    fi
done
log_success "3 Orbis nodes ready (ports 50051-50053)"

if ! command -v cli-tool &>/dev/null; then
    log_error "cli-tool not found in PATH."
    log_error "Install from orbis-rs with decaf377 feature."
    exit 1
fi
log_success "cli-tool found"

# --- Build ---
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ORBIS_TEST="$REPO_ROOT/target/release/orbis-test"

if [ ! -f "$ORBIS_TEST" ]; then
    print_phase "Build"
    log_info "Building orbis-test..."
    cargo build --release -p orbis-test --manifest-path "$REPO_ROOT/Cargo.toml"
    log_success "Build complete"
fi

# --- Run ---
print_phase "Running Tests"

echo "  Note: This test is independent of setup-tx.sh/test-orbis-scanning.sh."
echo "  It runs its own DKG and does not affect the compliance demo pipeline."
echo ""
echo "  Test suite covers:"
echo "    - DKG: Distributed key generation across 3 MPC nodes"
echo "    - FROST: Threshold (2-of-3) signature scheme for ACK generation"
echo "    - DLEQ: Discrete log equality proofs ensuring PRE correctness"
echo "    - PRE:  3-tier proxy re-encryption (detection, core, extension)"
echo "    - Negative: Unauthorized access attempts, invalid proofs"
echo ""

exec "$ORBIS_TEST" \
    --orbis-endpoints "http://127.0.0.1:50051,http://127.0.0.1:50052,http://127.0.0.1:50053"
