#!/bin/bash
# Start Orbis infrastructure (SourceHub + 3 Orbis nodes).
# Ctrl+C to stop. Cleanup is automatic via trap.
#
# Prerequisites:
#   - sourcehubd built at ../sourcehub/build/sourcehubd
#   - orbis-node and cli-tool installed with decaf377 features
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

repo_root="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
SOURCEHUB_DIR="$(cd "$repo_root/.." && pwd)/sourcehub"
SOURCEHUB_BIN="$SOURCEHUB_DIR/build/sourcehubd"
ORBIS_CLI="${ORBIS_CLI:-cli-tool}"

# Test mnemonic matching orbis-rs TEST_ACCOUNT_HEX_KEY (used by cli-tool fund)
TEST_MNEMONIC="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"

PID_FILE="$COMPLIANCE_TMP/orbis-pids.txt"

# --- Dependency checks ---
log_info "Checking dependencies..."
[ ! -x "$SOURCEHUB_BIN" ] && log_error "sourcehubd not found at $SOURCEHUB_BIN" && exit 1
for bin in orbis-node $ORBIS_CLI; do
    command -v "$bin" &>/dev/null || { log_error "$bin not found in PATH"; exit 1; }
done
log_success "All dependencies found"

# --- Process tracking ---
ALL_PIDS=()
TEMP_DIRS=()

cleanup() {
    echo ""
    log_info "Shutting down Orbis infrastructure..."
    for pid in "${ALL_PIDS[@]+"${ALL_PIDS[@]}"}"; do
        kill "$pid" 2>/dev/null || true
    done
    pkill orbis-node 2>/dev/null || true
    for pid in "${ALL_PIDS[@]+"${ALL_PIDS[@]}"}"; do
        wait "$pid" 2>/dev/null || true
    done
    for dir in "${TEMP_DIRS[@]+"${TEMP_DIRS[@]}"}"; do
        rm -rf "$dir"
    done
    rm -f "$COMPLIANCE_TMP"/orbis-node{1,2,3}.log
    rm -f "$PID_FILE"
    log_info "Orbis cleanup complete."
}
trap cleanup EXIT

track_pid() {
    ALL_PIDS+=("$1")
    echo "$2=$1" >> "$PID_FILE"
}

# ===================================================================
# SOURCEHUB
# ===================================================================
echo ""
echo "============================================"
echo "  SourceHub"
echo "============================================"

# Always start fresh — stale SourceHub has old rings that break PRE.
if curl -sf http://localhost:26657/status >/dev/null 2>&1; then
    log_warning "Killing stale SourceHub (old rings would break PRE)..."
    pkill sourcehubd 2>/dev/null || true
    sleep 3
fi

SH_HOME=$(mktemp -d)
TEMP_DIRS+=("$SH_HOME")

log_info "Initializing SourceHub chain..."
$SOURCEHUB_BIN init node --chain-id sourcehub-localnet --default-denom uopen \
    --home "$SH_HOME" >/dev/null 2>&1

# Validator account
$SOURCEHUB_BIN keys add validator --keyring-backend test \
    --home "$SH_HOME" >/dev/null 2>&1
VALIDATOR_ADDR=$($SOURCEHUB_BIN keys show validator -a --keyring-backend test \
    --home "$SH_HOME" 2>/dev/null)

# Test faucet account
echo "$TEST_MNEMONIC" | $SOURCEHUB_BIN keys add test --recover --keyring-backend test \
    --home "$SH_HOME" >/dev/null 2>&1
TEST_ADDR=$($SOURCEHUB_BIN keys show test -a --keyring-backend test \
    --home "$SH_HOME" 2>/dev/null)

# Genesis funding
$SOURCEHUB_BIN genesis add-genesis-account "$VALIDATOR_ADDR" 1000000000000000uopen \
    --home "$SH_HOME" >/dev/null 2>&1
$SOURCEHUB_BIN genesis add-genesis-account "$TEST_ADDR" 100000000000uopen \
    --home "$SH_HOME" >/dev/null 2>&1
$SOURCEHUB_BIN genesis gentx validator 100000000000000uopen \
    --chain-id sourcehub-localnet --keyring-backend test \
    --home "$SH_HOME" >/dev/null 2>&1
$SOURCEHUB_BIN genesis collect-gentxs \
    --home "$SH_HOME" >/dev/null 2>&1

# Enable zero-fee and disable bearer auth
GENESIS="$SH_HOME/config/genesis.json"
jq '.app_state.hub.chain_config.allow_zero_fee_txs = true' "$GENESIS" > "$SH_HOME/tmp.json" \
    && mv "$SH_HOME/tmp.json" "$GENESIS"
jq '.app_state.hub.chain_config.ignore_bearer_auth = true' "$GENESIS" > "$SH_HOME/tmp.json" \
    && mv "$SH_HOME/tmp.json" "$GENESIS"

# Enable REST API + gRPC
APP_TOML="$SH_HOME/config/app.toml"
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' 's/^enable = .*/enable = true/' "$APP_TOML"
    sed -i '' 's/^enabled-unsafe-cors = .*/enabled-unsafe-cors = true/' "$APP_TOML"
else
    sed -i 's/^enable = .*/enable = true/' "$APP_TOML"
    sed -i 's/^enabled-unsafe-cors = .*/enabled-unsafe-cors = true/' "$APP_TOML"
fi

log_info "Starting SourceHub..."
$SOURCEHUB_BIN start --home "$SH_HOME" > "$COMPLIANCE_TMP/sourcehub.log" 2>&1 &
track_pid $! "SOURCEHUB_PID"

log_info "Waiting for SourceHub..."
wait_for_url "http://localhost:26657/status" 30 2
log_success "SourceHub ready (http://localhost:26657)"

# ===================================================================
# 3 ORBIS NODES
# ===================================================================
echo ""
echo "============================================"
echo "  3 Orbis Nodes"
echo "============================================"

export ORBIS_PASSWORD="${ORBIS_PASSWORD:-test}"
SAVE_DIR="$PWD"

# Kill stale processes
if pgrep -q orbis-node 2>/dev/null; then
    log_warning "Killing stale orbis-node processes..."
    pkill orbis-node 2>/dev/null || true
    sleep 3
fi

NODE1_DIR=$(mktemp -d) ; TEMP_DIRS+=("$NODE1_DIR")
NODE2_DIR=$(mktemp -d) ; TEMP_DIRS+=("$NODE2_DIR")
NODE3_DIR=$(mktemp -d) ; TEMP_DIRS+=("$NODE3_DIR")

# Step 1: Generate keys on throwaway ports
log_info "Step 1/5: Generating node keys..."
cd "$NODE1_DIR" ; orbis-node --addr 127.0.0.1:50151 > "$COMPLIANCE_TMP/orbis-node1.log" 2>&1 & NODE1_PID=$!
cd "$NODE2_DIR" ; orbis-node --addr 127.0.0.1:50152 > "$COMPLIANCE_TMP/orbis-node2.log" 2>&1 & NODE2_PID=$!
cd "$NODE3_DIR" ; orbis-node --addr 127.0.0.1:50153 > "$COMPLIANCE_TMP/orbis-node3.log" 2>&1 & NODE3_PID=$!
cd "$SAVE_DIR"

for attempt in $(seq 1 15); do
    sleep 2
    HAVE_ALL=true
    for i in 1 2 3; do
        if ! grep -q "Signing key ready" "$COMPLIANCE_TMP/orbis-node${i}.log" 2>/dev/null; then
            HAVE_ALL=false; break
        fi
    done
    if [ "$HAVE_ALL" = true ]; then break; fi
    [ "$attempt" -eq 15 ] && { log_error "Key generation timed out"; exit 1; }
    echo "    ... waiting ($attempt/15)"
done

# Extract addresses
NODE_ADDRS=()
for i in 1 2 3; do
    ADDR=$(grep "Signing key ready" "$COMPLIANCE_TMP/orbis-node${i}.log" 2>/dev/null \
        | grep -o 'source1[a-z0-9]*' | head -1) || true
    [ -z "$ADDR" ] && { log_error "No address for node $i"; exit 1; }
    NODE_ADDRS+=("$ADDR")
done
log_info "  Node 1: ${NODE_ADDRS[0]}"
log_info "  Node 2: ${NODE_ADDRS[1]}"
log_info "  Node 3: ${NODE_ADDRS[2]}"

# Step 2: Stop keygen nodes (avoids account_number caching bug)
log_info "Step 2/5: Stopping keygen nodes..."
kill "$NODE1_PID" "$NODE2_PID" "$NODE3_PID" 2>/dev/null || true
sleep 1
pkill -9 orbis-node 2>/dev/null || true
wait "$NODE1_PID" "$NODE2_PID" "$NODE3_PID" 2>/dev/null || true
sleep 3

# Step 3: Fund nodes
log_info "Step 3/5: Funding nodes..."
for i in 0 1 2; do
    log_info "  Funding node $((i+1)) (${NODE_ADDRS[$i]})..."
    $ORBIS_CLI fund --address "${NODE_ADDRS[$i]}" 2>&1
    sleep 2
done

# Step 4: Start real nodes
log_info "Step 4/5: Starting nodes on real ports..."
> "$COMPLIANCE_TMP/orbis-node1.log" ; > "$COMPLIANCE_TMP/orbis-node2.log" ; > "$COMPLIANCE_TMP/orbis-node3.log"
cd "$NODE1_DIR" ; orbis-node --addr 127.0.0.1:50051 > "$COMPLIANCE_TMP/orbis-node1.log" 2>&1 & NODE1_PID=$!
cd "$NODE2_DIR" ; orbis-node --addr 127.0.0.1:50052 > "$COMPLIANCE_TMP/orbis-node2.log" 2>&1 & NODE2_PID=$!
cd "$NODE3_DIR" ; orbis-node --addr 127.0.0.1:50053 > "$COMPLIANCE_TMP/orbis-node3.log" 2>&1 & NODE3_PID=$!
cd "$SAVE_DIR"

track_pid "$NODE1_PID" "NODE1_PID"
track_pid "$NODE2_PID" "NODE2_PID"
track_pid "$NODE3_PID" "NODE3_PID"

# Wait for gRPC and check nodes are alive
log_info "Waiting for gRPC..."
for attempt in $(seq 1 45); do
    sleep 2
    for pid in $NODE1_PID $NODE2_PID $NODE3_PID; do
        if ! kill -0 "$pid" 2>/dev/null; then
            log_error "orbis-node (PID $pid) died."
            echo "  Node 1 log:" && tail -5 "$COMPLIANCE_TMP/orbis-node1.log"
            echo "  Node 2 log:" && tail -5 "$COMPLIANCE_TMP/orbis-node2.log"
            echo "  Node 3 log:" && tail -5 "$COMPLIANCE_TMP/orbis-node3.log"
            exit 1
        fi
    done
    ALL_UP=true
    for port in 50051 50052 50053; do
        if ! nc -z -w1 127.0.0.1 "$port" 2>/dev/null; then
            ALL_UP=false; break
        fi
    done
    if [ "$ALL_UP" = true ]; then break; fi
    [ "$attempt" -eq 45 ] && { log_error "Orbis gRPC not available after 90s"; exit 1; }
    echo "    ... attempt $attempt/45"
done

# Wait for init transactions
log_info "Waiting for init transactions to commit..."
sleep 10

# Step 5: Bulletin setup
log_info "Step 5/5: Setting up bulletin..."
$ORBIS_CLI register-bulletin-namespace --namespace orbis 2>&1
sleep 3
for i in 0 1 2; do
    log_info "  Adding node $((i+1)) as collaborator..."
    $ORBIS_CLI add-bulletin-collaborator --namespace orbis --collaborator "${NODE_ADDRS[$i]}" 2>&1
    sleep 3
done
log_success "3 Orbis nodes ready (ports 50051-50053)"

echo ""
echo "============================================"
echo "  Orbis Setup Complete"
echo "============================================"
echo ""
echo "  Services:"
echo "    SourceHub: http://localhost:26657"
echo "    Orbis:     ports 50051, 50052, 50053"
echo ""
echo "  All artifacts in: $COMPLIANCE_TMP/"
echo "    Logs: sourcehub.log, orbis-node{1,2,3}.log"
echo "    PIDs: orbis-pids.txt"
echo ""
echo "  Next: run test-orbis-primitives.sh or setup-tx.sh + test-orbis-scanning.sh"
echo "  (setup-tx.sh and test-orbis-scanning.sh also require setup-penumbra.sh)"
echo ""
echo "  Press Ctrl+C to stop."
echo ""

# Stay in foreground
wait "${ALL_PIDS[@]+"${ALL_PIDS[@]}"}"
