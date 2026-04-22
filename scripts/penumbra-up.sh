#!/bin/bash
# Start Penumbra infra (pd + cometbft + wallets + pclientd) as a one-shot setup step.
#
# Artifacts:
#   - tmp/compliance-demo.env
#   - tmp/penumbra-pids.txt
#   - tmp/pd.log
#   - tmp/cometbft.log
#   - tmp/*-pclientd.log
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

repo_root="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
PCLI="${PCLI:-$repo_root/target/release/pcli}"
PCLIENTD="${PCLIENTD:-$repo_root/target/release/pclientd}"
PD="${PD:-$repo_root/target/release/pd}"
PENUMBRA_NODE_PD_URL="${PENUMBRA_NODE_PD_URL:-http://localhost:8080}"
export PENUMBRA_NODE_PD_URL

ALICE_HOME="$COMPLIANCE_TMP/alice-wallet"
BOB_HOME="$COMPLIANCE_TMP/bob-wallet"
CHARLIE_HOME="$COMPLIANCE_TMP/charlie-wallet"
UNREGISTERED_HOME="$COMPLIANCE_TMP/unregistered-wallet"
ALICE_PCLIENTD_HOME="$COMPLIANCE_TMP/alice-pclientd"
BOB_PCLIENTD_HOME="$COMPLIANCE_TMP/bob-pclientd"
CHARLIE_PCLIENTD_HOME="$COMPLIANCE_TMP/charlie-pclientd"
UNREGISTERED_PCLIENTD_HOME="$COMPLIANCE_TMP/unregistered-pclientd"
ALICE_VIEW_URL="http://127.0.0.1:18081"
BOB_VIEW_URL="http://127.0.0.1:18082"
CHARLIE_VIEW_URL="http://127.0.0.1:18083"
UNREGISTERED_VIEW_URL="http://127.0.0.1:18084"

ENV_FILE="$COMPLIANCE_TMP/compliance-demo.env"
PID_FILE="$COMPLIANCE_TMP/penumbra-pids.txt"

print_banner "Penumbra Infra Bring-Up" "pd + cometbft + wallets + persistent view daemons"

log_info "Checking dependencies..."
for bin in "$PCLI" "$PCLIENTD" "$PD"; do
    [ ! -f "$bin" ] && log_error "$(basename "$bin") not found at $bin" && exit 1
done
for bin in cometbft jq; do
    command -v "$bin" >/dev/null 2>&1 || { log_error "$bin not found in PATH"; exit 1; }
done
log_success "All dependencies found"

log_info "Resetting previous Penumbra state..."
kill_tracked_pids "$PID_FILE"
pkill pd 2>/dev/null || true
pkill cometbft 2>/dev/null || true
sleep 2

rm -rf \
    ~/.penumbra/network_data \
    "$ALICE_HOME" "$BOB_HOME" "$CHARLIE_HOME" "$UNREGISTERED_HOME" \
    "$ALICE_PCLIENTD_HOME" "$BOB_PCLIENTD_HOME" "$CHARLIE_PCLIENTD_HOME" "$UNREGISTERED_PCLIENTD_HOME"
rm -f "$ENV_FILE" "$PID_FILE"

log_info "Initializing Alice wallet..."
echo | "$PCLI" --home "$ALICE_HOME" init soft-kms generate >/dev/null 2>&1
ALICE_ADDRESS=$("$PCLI" --home "$ALICE_HOME" view address 0)

log_info "Generating Penumbra network..."
run_quiet "$PD" network generate \
    --chain-id penumbra-local-devnet \
    --epoch-duration 302400 \
    --proposal-voting-blocks 50 \
    --gas-price-simple 1000 \
    --timeout-commit 500ms \
    --tendermint-rpc-bind 0.0.0.0:16657 \
    --tendermint-p2p-bind 0.0.0.0:16656 \
    --validators-input-file "$repo_root/testnets/validators-single.json" \
    --allocation-address "$ALICE_ADDRESS"

log_info "Starting pd..."
"$PD" start --home ~/.penumbra/network_data/node0/pd \
    --cometbft-addr http://127.0.0.1:16657 > "$COMPLIANCE_TMP/pd.log" 2>&1 &
PD_PID=$!
echo "PD_PID=$PD_PID" >> "$PID_FILE"

log_info "Starting cometbft..."
cometbft start --home ~/.penumbra/network_data/node0/cometbft > "$COMPLIANCE_TMP/cometbft.log" 2>&1 &
COMETBFT_PID=$!
echo "COMETBFT_PID=$COMETBFT_PID" >> "$PID_FILE"

log_info "Waiting for Penumbra infra..."
wait_for_penumbra_stack
log_success "Penumbra infra ready"

log_info "Initializing remaining wallets..."
echo | "$PCLI" --home "$BOB_HOME" init soft-kms generate >/dev/null 2>&1
echo | "$PCLI" --home "$CHARLIE_HOME" init soft-kms generate >/dev/null 2>&1
echo | "$PCLI" --home "$UNREGISTERED_HOME" init soft-kms generate >/dev/null 2>&1

log_info "Starting persistent wallet view daemons..."
configure_wallet_view_service "ALICE" "$ALICE_HOME" "$ALICE_PCLIENTD_HOME" 18081 "$PCLI" "$PCLIENTD" "$PID_FILE"
configure_wallet_view_service "BOB" "$BOB_HOME" "$BOB_PCLIENTD_HOME" 18082 "$PCLI" "$PCLIENTD" "$PID_FILE"
configure_wallet_view_service "CHARLIE" "$CHARLIE_HOME" "$CHARLIE_PCLIENTD_HOME" 18083 "$PCLI" "$PCLIENTD" "$PID_FILE"
configure_wallet_view_service "UNREGISTERED" "$UNREGISTERED_HOME" "$UNREGISTERED_PCLIENTD_HOME" 18084 "$PCLI" "$PCLIENTD" "$PID_FILE"

log_info "Syncing wallets through persistent view daemons..."
run_quiet "$PCLI" --home "$ALICE_HOME" view sync
run_quiet "$PCLI" --home "$BOB_HOME" view sync
run_quiet "$PCLI" --home "$CHARLIE_HOME" view sync
run_quiet "$PCLI" --home "$UNREGISTERED_HOME" view sync

BOB_ADDRESS=$("$PCLI" --home "$BOB_HOME" view address 0)
CHARLIE_ADDRESS=$("$PCLI" --home "$CHARLIE_HOME" view address 0)
UNREGISTERED_ADDRESS=$("$PCLI" --home "$UNREGISTERED_HOME" view address 0)

log_info "Prefunding Bob and Charlie for fee-bearing demo flows..."
run_quiet "$PCLI" --home "$ALICE_HOME" tx transfer --to "$BOB_ADDRESS" 1000000upenumbra
run_quiet "$PCLI" --home "$ALICE_HOME" tx transfer --to "$CHARLIE_ADDRESS" 1000000upenumbra
run_quiet "$PCLI" --home "$ALICE_HOME" view sync
run_quiet "$PCLI" --home "$BOB_HOME" view sync
run_quiet "$PCLI" --home "$CHARLIE_HOME" view sync

cat > "$ENV_FILE" <<EOF
export ALICE_HOME="$ALICE_HOME"
export BOB_HOME="$BOB_HOME"
export CHARLIE_HOME="$CHARLIE_HOME"
export UNREGISTERED_HOME="$UNREGISTERED_HOME"
export ALICE_ADDRESS="$ALICE_ADDRESS"
export BOB_ADDRESS="$BOB_ADDRESS"
export CHARLIE_ADDRESS="$CHARLIE_ADDRESS"
export UNREGISTERED_ADDRESS="$UNREGISTERED_ADDRESS"
export PCLI="$PCLI"
export PCLIENTD="$PCLIENTD"
export PENUMBRA_NODE_PD_URL="$PENUMBRA_NODE_PD_URL"
export ALICE_PCLIENTD_HOME="$ALICE_PCLIENTD_HOME"
export BOB_PCLIENTD_HOME="$BOB_PCLIENTD_HOME"
export CHARLIE_PCLIENTD_HOME="$CHARLIE_PCLIENTD_HOME"
export UNREGISTERED_PCLIENTD_HOME="$UNREGISTERED_PCLIENTD_HOME"
export ALICE_VIEW_URL="$ALICE_VIEW_URL"
export BOB_VIEW_URL="$BOB_VIEW_URL"
export CHARLIE_VIEW_URL="$CHARLIE_VIEW_URL"
export UNREGISTERED_VIEW_URL="$UNREGISTERED_VIEW_URL"
EOF

log_success "Penumbra setup complete"
echo "  Env:  $ENV_FILE"
echo "  PIDs: $PID_FILE"
echo "  Next: ./scripts/orbis-stack.sh up"
