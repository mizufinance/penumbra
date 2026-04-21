#!/usr/bin/env bash
# Run smoke test suite, via process-compose config.
set -euo pipefail


# Fail fast if network dir exists, otherwise `cargo run ...` will block
# for a while, masking the error.
#
# If any network data is present, we shouldn't reuse it: the smoke tests assume
# a fresh devnet has been created specifically for the test run. In the future
# we should make this a temp dir so it can always run regardless of pre-existing state.
repo_root="$(git rev-parse --show-toplevel)"
smoke_test_dir="$(mktemp -d "${TMPDIR:-/tmp}/penumbra-smoke.XXXXXX")"
temp_root="${TMPDIR:-/tmp}"
temp_root="${temp_root%/}"
devnet_pid=""

# Run the full smoke environment against an isolated Penumbra state directory so
# local developer state does not interfere with the devnet/process-compose paths.
export PENUMBRA_DEVNET_HOME="${smoke_test_dir}/penumbra-home"
mkdir -p "${PENUMBRA_DEVNET_HOME}"

"${repo_root}/deployments/scripts/warn-about-pd-state"

# Check for dependencies. All of these will be installed automatically
# as part of the nix env.
if ! hash cometbft > /dev/null 2>&1 ; then
    >&2 echo "ERROR: cometbft not found in PATH"
    >&2 echo "See install guide: https://guide.penumbra.zone/main/pd/build.html"
    exit 1
fi

if ! hash process-compose > /dev/null 2>&1 ; then
    >&2 echo "ERROR: process-compose not found in PATH"
    >&2 echo "Install it via https://github.com/F1bonacc1/process-compose/"
    exit 1
fi

if ! hash grpcurl > /dev/null 2>&1 ; then
    >&2 echo "ERROR: grpcurl not found in PATH"
    >&2 echo "Install it via https://github.com/fullstorydev/grpcurl/"
    exit 1
fi

>&2 echo "Building all test targets before running smoke tests..."
# We want a warm cache before the tests run
cargo build --release --bins

have_postgres_tooling=true
for postgres_tool in postgres psql pg_ctl createdb createuser; do
    if ! hash "$postgres_tool" > /dev/null 2>&1 ; then
        have_postgres_tooling=false
        break
    fi
done


# Recursively terminate a process subtree without requiring external session-management tools.
kill_tree() {
    local pid="$1"
    local child

    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
        kill_tree "$child"
    done

    kill "$pid" 2>/dev/null || true
}

cleanup_smoke() {
    if [ -n "${devnet_pid:-}" ]; then
        kill_tree "$devnet_pid"
        wait "$devnet_pid" 2>/dev/null || true
    fi

    unset PENUMBRA_DEVNET_HOME

    case "${smoke_test_dir:-}" in
        "$temp_root"/penumbra-smoke.*)
            if [ -n "${smoke_test_dir}" ] && [ -d "${smoke_test_dir}" ]; then
                rm -rf "${smoke_test_dir}"
            fi
            ;;
    esac
}

trap cleanup_smoke EXIT

# Reuse existing dev-env script, but keep process-compose in the foreground and
# background the wrapper here so smoke can wait on readiness and clean up the
# resulting process tree on exit.
devnet_args=(
    --no-server
)

if [ "$have_postgres_tooling" = true ]; then
    devnet_args+=(
        --config ./deployments/compose/process-compose-postgres.yml
    )
fi

"${repo_root}/deployments/scripts/run-local-devnet.sh" \
    "${devnet_args[@]}" \
    > "${smoke_test_dir}/devnet.log" 2>&1 &
devnet_pid=$!

# Wait a bit for network to start.
sleep 10

# Wait for the network to be fully ready by checking block height.
# We need at least one block to be produced before the chain state is queryable.
>&2 echo "Waiting for network to produce blocks..."
max_attempts=120
attempt=0
while true; do
    # Query the latest block height via the tendermint RPC
    height_response=$(curl -s http://127.0.0.1:16657/status 2>&1) || true
    # Extract the block height from the JSON response
    height=$(echo "$height_response" | grep -o '"latest_block_height":"[0-9]*"' | grep -o '[0-9]*' | head -1) || true

    if [ -n "$height" ] && [ "$height" -gt 0 ] 2>/dev/null; then
        >&2 echo "  Block height: $height"
        break
    fi

    attempt=$((attempt + 1))
    if [ $attempt -ge $max_attempts ]; then
        >&2 echo "ERROR: Network did not produce blocks within timeout"
        >&2 echo "Status response: $height_response"
        >&2 echo "=== devnet log (last 100 lines) ==="
        tail -100 "${smoke_test_dir}/devnet.log" >&2 || true
        >&2 echo "=== end devnet log ==="
        exit 1
    fi
    >&2 echo "  Waiting for blocks (attempt $attempt/$max_attempts)..."
    sleep 1
done
>&2 echo "Network is producing blocks."
# Wait for a few more blocks to ensure state is fully committed
sleep 10

# Initialize the test wallet used by the smoke compliance setup.
pcli_test_home="${smoke_test_dir}/pcli-test"
mkdir -p "$pcli_test_home"
echo "comfort ten front cycle churn burger oak absent rice ice urge result art couple benefit cabbage frequent obscure hurry trick segment cool job debate" | \
    cargo run --release --bin pcli -- --home "$pcli_test_home" init --grpc-url "http://127.0.0.1:8080" soft-kms import-phrase

# --- Compliance smoke test setup ---
# Use regulated_usd (already allocated in genesis) as the unified regulated token.
# Generate a DK, register the asset and user, then send a transfer so the
# detection scan integration test has on-chain data to find.
>&2 echo "Setting up compliance smoke test environment..."
dk_output=$(cargo run --release --bin pcli -- --home "$pcli_test_home" tx compliance generate-dk 2>&1) || true
dk_hex=$(echo "$dk_output" | grep "DK (hex):" | awk '{print $NF}')
dk_pub_hex=$(echo "$dk_output" | grep "DK_pub (hex):" | awk '{print $NF}')

if [ -n "$dk_hex" ] && [ -n "$dk_pub_hex" ]; then
    >&2 echo "  DK generated successfully."

    # Register regulated_usd as a regulated asset with the generated DK
    cargo run --release --bin pcli -- --home "$pcli_test_home" \
        tx compliance register-asset regulated_usd \
        --regulated --dk-pub-hex "$dk_pub_hex" --threshold 500000000000000000000
    >&2 echo "  regulated_usd registered as regulated asset."

    # Register the test user for regulated_usd
    cargo run --release --bin pcli -- --home "$pcli_test_home" \
        tx compliance register-user regulated_usd
    cargo run --release --bin pcli -- --home "$pcli_test_home" \
        tx compliance register-user regulated_usd --address-index 1
    >&2 echo "  User registered for regulated_usd."

    # Send a transfer so the detection scan has something to find
    cargo run --release --bin pcli -- --home "$pcli_test_home" view sync
    >&2 echo "  DEBUG: balance before send:"
    cargo run --release --bin pcli -- --home "$pcli_test_home" view balance 2>&1 | tee /dev/stderr || true
    smoke_addr=$(cargo run --release --bin pcli -- --home "$pcli_test_home" view address 1)
    cargo run --release --bin pcli -- --home "$pcli_test_home" \
        tx transfer 100regulated_usd --to "$smoke_addr"
    >&2 echo "  Compliance transfer sent."

    # Export env vars for integration tests
    export COMPLIANCE_DK_HEX="$dk_hex"
    export COMPLIANCE_DK_PUB_HEX="$dk_pub_hex"
    export COMPLIANCE_SMOKE_ASSET="regulated_usd"
    >&2 echo "  Compliance env vars exported."
else
    >&2 echo "ERROR: DK generation failed during compliance smoke setup."
    exit 1
fi
>&2 echo "Compliance smoke test setup complete."

bash "${repo_root}/deployments/scripts/check-reduced-surface.sh"

# Export devnet parameters for integration tests.
# Must match values in run-local-devnet.sh.
export UNBONDING_DELAY=201
export PENUMBRA_REDUCED_ACTION_SURFACE=1
export PENUMBRA_NODE_PD_URL="http://127.0.0.1:8080"
export PENUMBRA_NODE_CMT_URL="http://127.0.0.1:16657"

# Run the integration tests. Using `just` targets so that the exact
# invocations are easily reusable on the CLI in dev loops.
just integration-pclientd
just integration-pcli
# The pd tests come later, as they need work to have been performed for metrics to be emitted.
just integration-pd
# Finally, pindexer tests, to make assertions about emitted events.
just integration-pindexer
