#!/usr/bin/env bash
# Dev tooling to spin up a localhost devnet for Penumbra.
set -euo pipefail


repo_root="$(git rev-parse --show-toplevel)"
source "${repo_root}/scripts/lib/common.sh"
penumbra_devnet_home="${PENUMBRA_DEVNET_HOME:-$HOME/.penumbra}"
export PENUMBRA_DEVNET_HOME="$penumbra_devnet_home"
network_data_dir="${penumbra_devnet_home}/network_data"
# The process-compose file already respects local state and will reuse it.
# "${repo_root}/deployments/scripts/warn-about-pd-state"

>&2 echo "Building binaries from latest code..."
cargo build --release --bin pd
# Also make sure to invoke via `cargo run` so that the process-compose
# spin-up doesn't block on more building/linking.
cargo --quiet run --release --bin pd -- --help > /dev/null

# Generate network from latest code, only if network does not already exist.
if [[ -d "$network_data_dir" ]] ; then
    >&2 echo "network data exists locally, reusing it"
else
    # XXX: Manually Add allocation address.
    cargo run --release --bin pd -- network \
        --network-dir "$network_data_dir" \
        generate \
        --chain-id penumbra-local-devnet \
        --epoch-duration 302400 \
        --proposal-voting-blocks 50 \
        --gas-price-simple 1000 \
        --timeout-commit 500ms \
        --tendermint-rpc-bind "0.0.0.0:${PENUMBRA_COMETBFT_RPC_PORT}" \
        --tendermint-p2p-bind "0.0.0.0:${PENUMBRA_COMETBFT_P2P_PORT}" \
        --allocations-input-file deployments/compose/devnet-allocations.csv \
        --validators-input-file testnets/validators-single.json \
        --allocation-address "penumbra1cvp32r5wp4lfnnww3g3fytxccqnu2xcj0r2qm0sa8ekjdezlm3gzk34qtg2xscqx9r6yrhz24k3l6j88q98rexyp7dnupq66cxllvpp9v0lw0xuqf0yfhv5ksfxzv0m968tmxn"

    # opt in to cometbft abci indexing to postgres
    postgresql_db_url="postgresql://penumbra:penumbra@127.0.0.1:${PENUMBRA_POSTGRES_PORT}/penumbra_cometbft?sslmode=disable"
    sed -i -e "s#^indexer.*#indexer = \"psql\"\\npsql-conn = \"$postgresql_db_url\"#" "$network_data_dir/node0/cometbft/config/config.toml"
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
