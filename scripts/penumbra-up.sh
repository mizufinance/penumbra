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
PENUMBRA_DEVNET_HOME="${PENUMBRA_DEVNET_HOME:-$COMPLIANCE_STACK_HOME}"
NETWORK_DATA_DIR="${PENUMBRA_DEVNET_HOME}/network_data"
export PENUMBRA_DEVNET_HOME
export PENUMBRA_NODE_PD_URL
export PENUMBRA_NODE_CMT_URL

ALICE_HOME="$COMPLIANCE_TMP/alice-wallet"
BOB_HOME="$COMPLIANCE_TMP/bob-wallet"
CHARLIE_HOME="$COMPLIANCE_TMP/charlie-wallet"
UNREGISTERED_HOME="$COMPLIANCE_TMP/unregistered-wallet"
ALICE_PCLIENTD_HOME="$COMPLIANCE_TMP/alice-pclientd"
BOB_PCLIENTD_HOME="$COMPLIANCE_TMP/bob-pclientd"
CHARLIE_PCLIENTD_HOME="$COMPLIANCE_TMP/charlie-pclientd"
UNREGISTERED_PCLIENTD_HOME="$COMPLIANCE_TMP/unregistered-pclientd"
ALICE_PCLIENTD_PORT="$PENUMBRA_PCLIENTD_PORT_BASE"
BOB_PCLIENTD_PORT=$((PENUMBRA_PCLIENTD_PORT_BASE + 1))
CHARLIE_PCLIENTD_PORT=$((PENUMBRA_PCLIENTD_PORT_BASE + 2))
UNREGISTERED_PCLIENTD_PORT=$((PENUMBRA_PCLIENTD_PORT_BASE + 3))
ALICE_VIEW_URL="http://127.0.0.1:$ALICE_PCLIENTD_PORT"
BOB_VIEW_URL="http://127.0.0.1:$BOB_PCLIENTD_PORT"
CHARLIE_VIEW_URL="http://127.0.0.1:$CHARLIE_PCLIENTD_PORT"
UNREGISTERED_VIEW_URL="http://127.0.0.1:$UNREGISTERED_PCLIENTD_PORT"
COMPLIANCE_DEV_REGISTRAR_SK_HEX="${COMPLIANCE_DEV_REGISTRAR_SK_HEX:-}"
COMPLIANCE_DEV_REGISTRAR_VK_HEX="${COMPLIANCE_DEV_REGISTRAR_VK_HEX:-}"
COMPLIANCE_DEV_AUTHORITY_SK_HEX="${COMPLIANCE_DEV_AUTHORITY_SK_HEX:-}"
COMPLIANCE_DEV_AUTHORITY_VK_HEX="${COMPLIANCE_DEV_AUTHORITY_VK_HEX:-}"
COMPLIANCE_GRANT_VALID_UNTIL_UNIX="${COMPLIANCE_GRANT_VALID_UNTIL_UNIX:-4102444800}"

ENV_FILE="$COMPLIANCE_TMP/compliance-demo.env"
PID_FILE="$COMPLIANCE_TMP/penumbra-pids.txt"

print_banner "Penumbra Infra Bring-Up" "repo-local devnet + wallets + persistent view daemons"

log_info "Checking dependencies..."
for bin in "$PCLI" "$PCLIENTD" "$PD"; do
    [ ! -x "$bin" ] && log_error "$(basename "$bin") not found at $bin" && exit 1
done
for bin in cometbft jq; do
    command -v "$bin" >/dev/null 2>&1 || { log_error "$bin not found in PATH"; exit 1; }
done
log_success "All dependencies found"

require_env_secret() {
    local name="$1"
    if [ -z "${!name:-}" ]; then
        log_error "$name must be set; scripts/penumbra-up.sh does not provide default secret keys"
        exit 1
    fi
}

is_hex_value() {
    local value="$1"
    [[ "$value" =~ ^[0-9A-Fa-f]+$ ]] && [ $(( ${#value} % 2 )) -eq 0 ]
}

derive_spend_vk_hex() {
    "$PCLI" tx compliance derive-spend-vk --signing-key-hex "$1"
}

validate_or_fill_spend_vk() {
    local sk_var="$1"
    local vk_var="$2"
    local label="$3"
    local sk_hex="${!sk_var}"
    local vk_hex="${!vk_var:-}"
    local derived_vk

    is_hex_value "$sk_hex" || {
        log_error "$sk_var must be even-length hex"
        exit 1
    }

    derived_vk="$(derive_spend_vk_hex "$sk_hex")"
    is_hex_value "$derived_vk" || {
        log_error "failed to derive $label verification key from $sk_var"
        exit 1
    }

    if [ -z "$vk_hex" ]; then
        printf -v "$vk_var" '%s' "$derived_vk"
        return
    fi

    if [ "$(printf '%s' "$derived_vk" | tr '[:upper:]' '[:lower:]')" != "$(printf '%s' "$vk_hex" | tr '[:upper:]' '[:lower:]')" ]; then
        log_error "$label signing key does not match configured verification key"
        log_error "derived=$derived_vk configured=$vk_hex"
        exit 1
    fi
}

require_env_secret COMPLIANCE_DEV_REGISTRAR_SK_HEX
require_env_secret COMPLIANCE_DEV_AUTHORITY_SK_HEX
validate_or_fill_spend_vk COMPLIANCE_DEV_REGISTRAR_SK_HEX COMPLIANCE_DEV_REGISTRAR_VK_HEX "registrar"
validate_or_fill_spend_vk COMPLIANCE_DEV_AUTHORITY_SK_HEX COMPLIANCE_DEV_AUTHORITY_VK_HEX "authority"

log_info "Resetting previous Penumbra state..."
kill_tracked_pids "$PID_FILE"
sleep 2
ensure_ports_available \
    "$PENUMBRA_PD_GRPC_PORT" \
    "$PENUMBRA_COMETBFT_P2P_PORT" \
    "$PENUMBRA_COMETBFT_RPC_PORT" \
    "$ALICE_PCLIENTD_PORT" \
    "$BOB_PCLIENTD_PORT" \
    "$CHARLIE_PCLIENTD_PORT" \
    "$UNREGISTERED_PCLIENTD_PORT"

rm -rf \
    "$PENUMBRA_DEVNET_HOME" \
    "$ALICE_HOME" "$BOB_HOME" "$CHARLIE_HOME" "$UNREGISTERED_HOME" \
    "$ALICE_PCLIENTD_HOME" "$BOB_PCLIENTD_HOME" "$CHARLIE_PCLIENTD_HOME" "$UNREGISTERED_PCLIENTD_HOME"
rm -f "$ENV_FILE" "$PID_FILE"
mkdir -p "$PENUMBRA_DEVNET_HOME"

log_info "Initializing Alice wallet..."
echo | "$PCLI" --home "$ALICE_HOME" init --grpc-url "$PENUMBRA_NODE_PD_URL" soft-kms generate >/dev/null 2>&1
ALICE_ADDRESS=$("$PCLI" --home "$ALICE_HOME" view address 0)

log_info "Generating Penumbra network..."
run_quiet "$PD" network --network-dir "$NETWORK_DATA_DIR" generate \
    --chain-id penumbra-local-devnet \
    --epoch-duration 302400 \
    --proposal-voting-blocks 50 \
    --gas-price-simple 1000 \
    --compliance-registrar-vk-hex "$COMPLIANCE_DEV_REGISTRAR_VK_HEX" \
    --timeout-commit 500ms \
    --tendermint-rpc-bind "0.0.0.0:$PENUMBRA_COMETBFT_RPC_PORT" \
    --tendermint-p2p-bind "0.0.0.0:$PENUMBRA_COMETBFT_P2P_PORT" \
    --validators-input-file "$repo_root/testnets/validators-single.json" \
    --allocation-address "$ALICE_ADDRESS"

log_info "Starting pd..."
"$PD" start --home "$NETWORK_DATA_DIR/node0/pd" \
    --grpc-bind "0.0.0.0:$PENUMBRA_PD_GRPC_PORT" \
    --cometbft-addr "$PENUMBRA_NODE_CMT_URL" > "$COMPLIANCE_TMP/pd.log" 2>&1 &
PD_PID=$!
echo "PD_PID=$PD_PID" >> "$PID_FILE"

log_info "Starting cometbft..."
cometbft start --home "$NETWORK_DATA_DIR/node0/cometbft" > "$COMPLIANCE_TMP/cometbft.log" 2>&1 &
COMETBFT_PID=$!
echo "COMETBFT_PID=$COMETBFT_PID" >> "$PID_FILE"

log_info "Waiting for Penumbra infra..."
wait_for_penumbra_stack
log_success "Penumbra infra ready"

log_info "Initializing remaining wallets..."
echo | "$PCLI" --home "$BOB_HOME" init --grpc-url "$PENUMBRA_NODE_PD_URL" soft-kms generate >/dev/null 2>&1
echo | "$PCLI" --home "$CHARLIE_HOME" init --grpc-url "$PENUMBRA_NODE_PD_URL" soft-kms generate >/dev/null 2>&1
echo | "$PCLI" --home "$UNREGISTERED_HOME" init --grpc-url "$PENUMBRA_NODE_PD_URL" soft-kms generate >/dev/null 2>&1

log_info "Starting persistent wallet view daemons..."
configure_wallet_view_service "ALICE" "$ALICE_HOME" "$ALICE_PCLIENTD_HOME" "$ALICE_PCLIENTD_PORT" "$PCLI" "$PCLIENTD" "$PID_FILE"
configure_wallet_view_service "BOB" "$BOB_HOME" "$BOB_PCLIENTD_HOME" "$BOB_PCLIENTD_PORT" "$PCLI" "$PCLIENTD" "$PID_FILE"
configure_wallet_view_service "CHARLIE" "$CHARLIE_HOME" "$CHARLIE_PCLIENTD_HOME" "$CHARLIE_PCLIENTD_PORT" "$PCLI" "$PCLIENTD" "$PID_FILE"
configure_wallet_view_service "UNREGISTERED" "$UNREGISTERED_HOME" "$UNREGISTERED_PCLIENTD_HOME" "$UNREGISTERED_PCLIENTD_PORT" "$PCLI" "$PCLIENTD" "$PID_FILE"

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
export PENUMBRA_DEVNET_HOME="$PENUMBRA_DEVNET_HOME"
export PENUMBRA_NODE_PD_URL="$PENUMBRA_NODE_PD_URL"
export PENUMBRA_NODE_CMT_URL="$PENUMBRA_NODE_CMT_URL"
export ALICE_PCLIENTD_HOME="$ALICE_PCLIENTD_HOME"
export BOB_PCLIENTD_HOME="$BOB_PCLIENTD_HOME"
export CHARLIE_PCLIENTD_HOME="$CHARLIE_PCLIENTD_HOME"
export UNREGISTERED_PCLIENTD_HOME="$UNREGISTERED_PCLIENTD_HOME"
export ALICE_VIEW_URL="$ALICE_VIEW_URL"
export BOB_VIEW_URL="$BOB_VIEW_URL"
export CHARLIE_VIEW_URL="$CHARLIE_VIEW_URL"
export UNREGISTERED_VIEW_URL="$UNREGISTERED_VIEW_URL"
export COMPLIANCE_DEV_REGISTRAR_SK_HEX="$COMPLIANCE_DEV_REGISTRAR_SK_HEX"
export COMPLIANCE_DEV_REGISTRAR_VK_HEX="$COMPLIANCE_DEV_REGISTRAR_VK_HEX"
export COMPLIANCE_DEV_AUTHORITY_SK_HEX="$COMPLIANCE_DEV_AUTHORITY_SK_HEX"
export COMPLIANCE_DEV_AUTHORITY_VK_HEX="$COMPLIANCE_DEV_AUTHORITY_VK_HEX"
export COMPLIANCE_GRANT_VALID_UNTIL_UNIX="$COMPLIANCE_GRANT_VALID_UNTIL_UNIX"
EOF

log_success "Penumbra setup complete"
echo "  Env:  $ENV_FILE"
echo "  PIDs: $PID_FILE"
echo "  Next: ./scripts/orbis-stack.sh up"
