#!/bin/bash
# Start Penumbra infrastructure (pd + cometbft) and create/fund wallets.
# Ctrl+C to stop. Cleanup is automatic via trap.
#
# Prerequisites:
#   - cargo build --release (pcli, pd)
#   - cometbft installed
#   - jq installed
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

repo_root="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
PCLI="${PCLI:-$repo_root/target/release/pcli}"
PD="${PD:-$repo_root/target/release/pd}"
PENUMBRA_NODE_PD_URL="${PENUMBRA_NODE_PD_URL:-http://localhost:8080}"
export PENUMBRA_NODE_PD_URL

ORBIS_CLI="${ORBIS_CLI:-cli-tool}"

ALICE_HOME="$COMPLIANCE_TMP/alice-wallet"
BOB_HOME="$COMPLIANCE_TMP/bob-wallet"
CHARLIE_HOME="$COMPLIANCE_TMP/charlie-wallet"
UNREGISTERED_HOME="$COMPLIANCE_TMP/unregistered-wallet"

ENV_FILE="$COMPLIANCE_TMP/compliance-demo.env"
PID_FILE="$COMPLIANCE_TMP/penumbra-pids.txt"

# --- Dependency checks ---
log_info "Checking dependencies..."
for bin in "$PCLI" "$PD"; do
    [ ! -f "$bin" ] && log_error "$(basename "$bin") not found at $bin" && exit 1
done
for bin in cometbft jq; do
    command -v "$bin" &>/dev/null || { log_error "$bin not found in PATH"; exit 1; }
done
log_success "All dependencies found"

# --- Process tracking ---
ALL_PIDS=()

cleanup() {
    echo ""
    log_info "Shutting down Penumbra..."
    for pid in "${ALL_PIDS[@]+"${ALL_PIDS[@]}"}"; do
        kill "$pid" 2>/dev/null || true
    done
    for pid in "${ALL_PIDS[@]+"${ALL_PIDS[@]}"}"; do
        wait "$pid" 2>/dev/null || true
    done
    rm -f "$PID_FILE"
    log_info "Penumbra cleanup complete."
}
trap cleanup EXIT

track_pid() {
    ALL_PIDS+=("$1")
    echo "$2=$1" >> "$PID_FILE"
}

# ===================================================================
# PENUMBRA DEVNET
# ===================================================================
echo ""
echo "============================================"
echo "  Penumbra Devnet"
echo "============================================"

# Kill stale processes from previous runs
if pgrep -q pd 2>/dev/null; then
    log_warning "Killing stale pd processes..."
    pkill pd 2>/dev/null || true
    sleep 2
fi
if pgrep -q cometbft 2>/dev/null; then
    log_warning "Killing stale cometbft processes..."
    pkill cometbft 2>/dev/null || true
    sleep 2
fi

# Cleanup old state
rm -rf ~/.penumbra/network_data "$ALICE_HOME" "$BOB_HOME" "$CHARLIE_HOME" "$UNREGISTERED_HOME"
rm -f "$ENV_FILE" "$PID_FILE"

# Init Alice wallet (need address for genesis allocation)
log_info "Initializing Alice wallet..."
echo | $PCLI --home "$ALICE_HOME" init soft-kms generate > /dev/null 2>&1
ALICE_ADDRESS=$($PCLI --home "$ALICE_HOME" view address 0)
log_info "Alice: ${ALICE_ADDRESS:0:40}..."

# Generate network
log_info "Generating Penumbra network..."
run_quiet $PD network generate \
    --chain-id penumbra-local-devnet \
    --epoch-duration 302400 \
    --proposal-voting-blocks 50 \
    --gas-price-simple 1000 \
    --timeout-commit 500ms \
    --tendermint-rpc-bind 0.0.0.0:16657 \
    --tendermint-p2p-bind 0.0.0.0:16656 \
    --validators-input-file "$repo_root/testnets/validators-single.json" \
    --allocation-address "$ALICE_ADDRESS"

# Start pd
log_info "Starting pd..."
$PD start --home ~/.penumbra/network_data/node0/pd \
    --cometbft-addr http://127.0.0.1:16657 > "$COMPLIANCE_TMP/pd.log" 2>&1 &
track_pid $! "PD_PID"

# Start cometbft
log_info "Starting cometbft..."
cometbft start --home ~/.penumbra/network_data/node0/cometbft > "$COMPLIANCE_TMP/cometbft.log" 2>&1 &
track_pid $! "COMETBFT_PID"

# Wait for Penumbra to be ready (blocks producing = pd + cometbft connected)
log_info "Waiting for Penumbra node..."
wait_for_penumbra 16657 45 2 5
log_success "Penumbra node ready (pd: http://localhost:8080, cometbft: http://localhost:16657)"

# ===================================================================
# WALLETS
# ===================================================================
echo ""
echo "============================================"
echo "  Create & Fund Wallets"
echo "============================================"

# Init remaining wallets
log_info "Initializing wallets..."
echo | $PCLI --home "$BOB_HOME" init soft-kms generate > /dev/null 2>&1
echo | $PCLI --home "$CHARLIE_HOME" init soft-kms generate > /dev/null 2>&1
echo | $PCLI --home "$UNREGISTERED_HOME" init soft-kms generate > /dev/null 2>&1

# Sync all wallets
log_info "Syncing wallets..."
run_quiet $PCLI --home "$ALICE_HOME" view sync
run_quiet $PCLI --home "$BOB_HOME" view sync
run_quiet $PCLI --home "$CHARLIE_HOME" view sync
run_quiet $PCLI --home "$UNREGISTERED_HOME" view sync

BOB_ADDRESS=$($PCLI --home "$BOB_HOME" view address 0)
CHARLIE_ADDRESS=$($PCLI --home "$CHARLIE_HOME" view address 0)
UNREGISTERED_ADDRESS=$($PCLI --home "$UNREGISTERED_HOME" view address 0)

log_info "Alice:        ${ALICE_ADDRESS:0:40}..."
log_info "Bob:          ${BOB_ADDRESS:0:40}..."
log_info "Charlie:      ${CHARLIE_ADDRESS:0:40}..."
log_info "Unregistered: ${UNREGISTERED_ADDRESS:0:40}..."

log_info "Prefunding Bob and Charlie with base asset for nonzero-fee demo flows..."
run_quiet $PCLI --home "$ALICE_HOME" tx transfer --to "$BOB_ADDRESS" 1000000upenumbra
run_quiet $PCLI --home "$ALICE_HOME" tx transfer --to "$CHARLIE_ADDRESS" 1000000upenumbra
run_quiet $PCLI --home "$ALICE_HOME" view sync
run_quiet $PCLI --home "$BOB_HOME" view sync
run_quiet $PCLI --home "$CHARLIE_HOME" view sync

log_success "Wallets initialized, synced, and fee-funded"

# ===================================================================
# EXPORT ENV
# ===================================================================
cat > "$ENV_FILE" << EOF
export ALICE_HOME="$ALICE_HOME"
export BOB_HOME="$BOB_HOME"
export CHARLIE_HOME="$CHARLIE_HOME"
export UNREGISTERED_HOME="$UNREGISTERED_HOME"
export ALICE_ADDRESS="$ALICE_ADDRESS"
export BOB_ADDRESS="$BOB_ADDRESS"
export CHARLIE_ADDRESS="$CHARLIE_ADDRESS"
export UNREGISTERED_ADDRESS="$UNREGISTERED_ADDRESS"
export PCLI="$PCLI"
export ORBIS_CLI="$ORBIS_CLI"
export PENUMBRA_NODE_PD_URL="$PENUMBRA_NODE_PD_URL"
EOF

echo ""
echo "============================================"
echo "  Penumbra Setup Complete"
echo "============================================"
echo ""
echo "  Services:"
echo "    pd:       http://localhost:8080"
echo "    cometbft: http://localhost:16657"
echo ""
echo "  All artifacts in: $COMPLIANCE_TMP/"
echo "    Logs:    pd.log, cometbft.log"
echo "    Wallets: alice-wallet/, bob-wallet/, charlie-wallet/, unregistered-wallet/"
echo "    Env:     compliance-demo.env"
echo "    PIDs:    penumbra-pids.txt"
echo ""
echo "  Next: also run setup-orbis.sh, then setup-tx.sh + test-orbis-scanning.sh"
echo ""
echo "  Press Ctrl+C to stop."
echo ""

# Stay in foreground
wait "${ALL_PIDS[@]+"${ALL_PIDS[@]}"}"
