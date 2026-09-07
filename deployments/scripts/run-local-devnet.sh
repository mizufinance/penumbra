#!/usr/bin/env bash
# Dev tooling to spin up a localhost devnet for Shieldd.
set -euo pipefail


repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
source "${repo_root}/scripts/lib/common.sh"
shieldd_devnet_home="${SHIELDD_DEVNET_HOME:-$HOME/.shieldd}"
export SHIELDD_DEVNET_HOME="$shieldd_devnet_home"
network_data_dir="${shieldd_devnet_home}/network_data"
compliance_dev_registrar_vk_hex="${COMPLIANCE_DEV_REGISTRAR_VK_HEX:-0800000000000000000000000000000000000000000000000000000000000000}"
pd_cargo_args=()
case "${SHIELDD_PD_INTEGRATION_DEV_SRS:-0}" in
    0)
        ;;
    1)
        if [[ "${SHIELDD_PRODUCTION:-0}" = "1" ]]; then
            >&2 echo "ERROR: integration dev SRS cannot be enabled in production"
            exit 1
        fi
        pd_cargo_args=(--features orbis-dev-srs)
        ;;
    *)
        >&2 echo "ERROR: SHIELDD_PD_INTEGRATION_DEV_SRS must be 0 or 1"
        exit 1
        ;;
esac
if [[ -z "${SHIELDD_PD_BIN:-}" ]]; then
    cargo build --release --bin pd "${pd_cargo_args[@]}"
    export SHIELDD_PD_BIN="${CARGO_TARGET_DIR:-${repo_root}/target}/release/pd"
fi

# Generate network from latest code, only if network does not already exist.
if [[ -d "$network_data_dir" ]] ; then
    >&2 echo "network data exists locally, reusing it"
else
    "$SHIELDD_PD_BIN" network \
        --network-dir "$network_data_dir" \
        generate \
        --chain-id shieldd-local-devnet \
        --epoch-duration 302400 \
        --gas-price-simple 1000 \
        --compliance-registrar-vk-hex "$compliance_dev_registrar_vk_hex" \
        --timeout-commit 500ms \
        --tendermint-rpc-bind "0.0.0.0:${SHIELDD_COMETBFT_RPC_PORT}" \
        --tendermint-p2p-bind "0.0.0.0:${SHIELDD_COMETBFT_P2P_PORT}" \
        --allocations-input-file deployments/compose/devnet-allocations.csv \
        --validators-input-file testnets/validators-single.json \
        --allocation-address "shieldd1u29dhz4vxgnek6a3vzxlejg0l83wegpu7hgs3yphdvljcnnnh89dvs6lc9hxxw94w464t7lh5x36cxnxyx0"

fi

# Check for interactive terminal session, enable TUI if yes.
if [[ -t 1 ]] ; then
    use_tui="true"
else
    use_tui="false"
fi

# Set a unique API port only when the HTTP control server is enabled.
if [[ "${PC_NO_SERVER:-0}" != "1" && -z "${PC_PORT_NUM:-}" ]] ; then
    export PC_PORT_NUM="8888"
fi

# Run the core fullnode config, plus any additional params passed via `$@`.
process-compose up \
    --ordered-shutdown \
    --tui="$use_tui" \
    --config "${repo_root}/deployments/compose/process-compose.yml" \
    "$@"
