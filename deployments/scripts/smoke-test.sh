#!/usr/bin/env bash
# Run smoke test suite, via process-compose config.
set -euo pipefail

pcli_tx_cmd() {
    # pcli prints the transaction plan and asks for an enter press before
    # signing. Smoke tests run non-interactively, so acknowledge that prompt
    # explicitly instead of depending on an attached terminal.
    printf '\n' | "$smoke_pcli_bin" --home "$pcli_test_home" "$@"
}

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"
source "${repo_root}/scripts/lib/common.sh"
smoke_test_dir="$(mktemp -d "${TMPDIR:-/tmp}/shieldd-smoke.XXXXXX")"
temp_root="${TMPDIR:-/tmp}"
temp_root="${temp_root%/}"
devnet_pid=""
# Dev-only compliance keys for the isolated smoke devnet. These are public test
# fixtures and must never be used for production or persistent deployments.
compliance_dev_registrar_sk_hex="${COMPLIANCE_DEV_REGISTRAR_SK_HEX:-0100000000000000000000000000000000000000000000000000000000000000}"
compliance_dev_registrar_vk_hex="${COMPLIANCE_DEV_REGISTRAR_VK_HEX:-0800000000000000000000000000000000000000000000000000000000000000}"
compliance_dev_authority_sk_hex="${COMPLIANCE_DEV_AUTHORITY_SK_HEX:-0200000000000000000000000000000000000000000000000000000000000000}"
compliance_dev_authority_vk_hex="${COMPLIANCE_DEV_AUTHORITY_VK_HEX:-b2ecf9b9082d6306538be73b0d6ee741141f3222152da78685d6596efc8c1506}"
compliance_dev_seizure_authority_sk_hex="${COMPLIANCE_DEV_SEIZURE_AUTHORITY_SK_HEX:-0300000000000000000000000000000000000000000000000000000000000000}"
compliance_dev_seizure_authority_vk_hex="${COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX:-2ebd42dd3a2307083c834e79fb9e787e352dd33e0d719f86ae4adb02fe382409}"
compliance_dev_dk_hex="cbd713af1d345f5b2e0c70268dd8bf71e30b10187cc6fd36b067f6b3a2a30902"
compliance_dev_dk_pub_hex="425347413a07569499089d7c3a19b31f600c712d5aca04b9eddaa0af102b4012"
compliance_dev_ring_pk_hex="0800000000000000000000000000000000000000000000000000000000000000"
compliance_dev_ring_id="shieldd-dev-ring"
compliance_dev_policy_id="shieldd-dev-policy"
compliance_dev_permission="read"
compliance_dev_resource="document"
compliance_grant_valid_until_unix="${COMPLIANCE_GRANT_VALID_UNTIL_UNIX:-4102444800}"
if [ "${SHIELDD_PRODUCTION:-0}" = "1" ]; then
    >&2 echo "ERROR: smoke-test.sh uses dev-only compliance keys and must not run with SHIELDD_PRODUCTION=1"
    exit 1
fi

# The repository intentionally has no production SnarkPack SRS until the
# ceremony/provenance work is complete. This isolated ephemeral devnet opts in
# to the explicitly insecure integration SRS so aggregation remains exercised.
export SHIELDD_PD_INTEGRATION_DEV_SRS=1

# Run the full smoke environment against an isolated Shieldd state directory so
# local developer state does not interfere with the devnet/process-compose paths.
export SHIELDD_DEVNET_HOME="${smoke_test_dir}/shieldd-home"
mkdir -p "${SHIELDD_DEVNET_HOME}"

"${repo_root}/deployments/scripts/warn-about-pd-state"

# Check for dependencies. All of these will be installed automatically
# as part of the nix env.
if ! hash cometbft > /dev/null 2>&1 ; then
    >&2 echo "ERROR: cometbft not found in PATH"
    >&2 echo "See the pd install guide"
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

>&2 echo "Building smoke-test binaries..."
python3 scripts/proof_artifacts.py materialize --bundle runtime
cargo build --release -p pcli -p pclientd --features bundled-proving-keys
smoke_pcli_bin="${smoke_test_dir}/pcli"
cp "${CARGO_TARGET_DIR:-${repo_root}/target}/release/pcli" "$smoke_pcli_bin"
cargo build --release -p pd --features orbis-dev-srs
export SHIELDD_PD_BIN="${smoke_test_dir}/pd"
cp "${CARGO_TARGET_DIR:-${repo_root}/target}/release/pd" "$SHIELDD_PD_BIN"

is_hex_value() {
    local value="$1"
    [[ "$value" =~ ^[0-9A-Fa-f]+$ ]] && [ $(( ${#value} % 2 )) -eq 0 ]
}

require_hex_output() {
    local name="$1"
    local value="$2"
    local raw_output="$3"
    local command_name="$4"

    if [ -z "${value//[[:space:]]/}" ] || ! is_hex_value "$value"; then
        >&2 echo "ERROR: $name from '$command_name' is empty or not even-length hex"
        if [ -n "$raw_output" ]; then
            >&2 echo "=== raw $command_name output ==="
            >&2 printf '%s\n' "$raw_output"
            >&2 echo "=== end raw output ==="
        fi
        exit 1
    fi
}

require_address_output() {
    local name="$1"
    local value="$2"
    local command_name="$3"

    if [ -z "${value//[[:space:]]/}" ] || [[ ! "$value" =~ ^shieldd[a-z0-9]+$ ]]; then
        >&2 echo "ERROR: $name from '$command_name' is empty or not a Shieldd address"
        >&2 echo "  $name=$value"
        exit 1
    fi
}

derive_spend_vk_hex() {
    "$smoke_pcli_bin" tx compliance derive-spend-vk --signing-key-hex "$1" | tail -1
}

validate_dev_spend_key_pair() {
    local label="$1"
    local sk_hex="$2"
    local vk_hex="$3"
    local derived_vk

    derived_vk="$(derive_spend_vk_hex "$sk_hex")"
    require_hex_output "${label}_derived_vk_hex" "$derived_vk" "$derived_vk" "tx compliance derive-spend-vk"
    if [ "$(printf '%s' "$derived_vk" | tr '[:upper:]' '[:lower:]')" != "$(printf '%s' "$vk_hex" | tr '[:upper:]' '[:lower:]')" ]; then
        >&2 echo "ERROR: $label compliance signing key does not match configured verification key"
        >&2 echo "  derived=$derived_vk"
        >&2 echo "  configured=$vk_hex"
        exit 1
    fi
}

validate_dev_spend_key_pair "registrar" "$compliance_dev_registrar_sk_hex" "$compliance_dev_registrar_vk_hex"
validate_dev_spend_key_pair "authority" "$compliance_dev_authority_sk_hex" "$compliance_dev_authority_vk_hex"
validate_dev_spend_key_pair "seizure authority" "$compliance_dev_seizure_authority_sk_hex" "$compliance_dev_seizure_authority_vk_hex"
export COMPLIANCE_DEV_REGISTRAR_VK_HEX="$compliance_dev_registrar_vk_hex"

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
    local exit_status=$?

    {
        if [ -n "${devnet_pid:-}" ]; then
            kill_tree "$devnet_pid"
            wait "$devnet_pid" 2>/dev/null || true
        fi

        if [ "$exit_status" -ne 0 ] && [ -f "${smoke_test_dir:-}/devnet.log" ]; then
            >&2 echo "=== devnet transaction diagnostics ==="
            grep -E \
                'checktx_frontdoor_|tx accepted|tx rejected|candidate_tx_count=[1-9]|included_tx_count=[1-9]|proposal_tx_count=[1-9]|process_proposal_reject_reason|deliver_tx failed|num_(valid|invalid)_txs=[1-9]|view worker error|failed to fetch compact block|recheck.*(reject|fail)|mempool.*(reject|drop)' \
                "${smoke_test_dir}/devnet.log" \
                | tail -500 >&2 \
                || true
            >&2 echo "=== end transaction diagnostics ==="
            >&2 echo "=== devnet log after smoke failure (last 200 lines) ==="
            tail -200 "${smoke_test_dir}/devnet.log" >&2 || true
            >&2 echo "=== end devnet log ==="
        fi

        unset SHIELDD_DEVNET_HOME

        case "${smoke_test_dir:-}" in
            "$temp_root"/shieldd-smoke.*)
                if [ -n "${smoke_test_dir}" ] && [ -d "${smoke_test_dir}" ]; then
                    rm -rf "${smoke_test_dir}"
                fi
                ;;
        esac
    } || true

    return "$exit_status"
}

trap cleanup_smoke EXIT

# Build the wallet before genesis so its regulated allocation can be bound to
# the exact wallet-derived compliance nullifier key.
pcli_test_home="${smoke_test_dir}/pcli-test"
mkdir -p "$pcli_test_home"
echo "comfort ten front cycle churn burger oak absent rice ice urge result art couple benefit cabbage frequent obscure hurry trick segment cool job debate" | \
    "$smoke_pcli_bin" --home "$pcli_test_home" init --grpc-url "$SHIELDD_NODE_PD_URL" soft-kms import-phrase
smoke_addr_0=$("$smoke_pcli_bin" --home "$pcli_test_home" view address 0)
require_address_output "smoke_addr_0" "$smoke_addr_0" "view address 0"
smoke_addr_1=$("$smoke_pcli_bin" --home "$pcli_test_home" view address 1)
require_address_output "smoke_addr_1" "$smoke_addr_1" "view address 1"
smoke_addr_2=$("$smoke_pcli_bin" --home "$pcli_test_home" view address 2)
require_address_output "smoke_addr_2" "$smoke_addr_2" "view address 2"
# Generate deterministic public registration fixtures for this isolated devnet.
compliance_dev_bundle=$(cargo run --quiet --release -p shieldd-sdk-compliance \
    --example dev_orbis_registration --no-default-features --features test-helpers)
IFS=$'\t' read -r \
    compliance_dev_address_0 \
    compliance_dev_rnk_dh_pk_0_hex \
    compliance_dev_rnk_commitment_0_hex \
    compliance_dev_capability_certificate_0_hex \
    compliance_dev_address_1 \
    compliance_dev_rnk_dh_pk_1_hex \
    compliance_dev_rnk_commitment_1_hex \
    compliance_dev_capability_certificate_1_hex \
    compliance_dev_address_2 \
    compliance_dev_rnk_dh_pk_2_hex \
    compliance_dev_rnk_commitment_2_hex \
    compliance_dev_capability_certificate_2_hex <<< "$compliance_dev_bundle"
if [ "$compliance_dev_address_0" != "$smoke_addr_0" ] \
    || [ "$compliance_dev_address_1" != "$smoke_addr_1" ] \
    || [ "$compliance_dev_address_2" != "$smoke_addr_2" ]; then
    >&2 echo "ERROR: dev Orbis fixtures do not match the smoke wallet"
    exit 1
fi
for fixture_name in \
    compliance_dev_rnk_dh_pk_0_hex \
    compliance_dev_rnk_commitment_0_hex \
    compliance_dev_capability_certificate_0_hex \
    compliance_dev_rnk_dh_pk_1_hex \
    compliance_dev_rnk_commitment_1_hex \
    compliance_dev_capability_certificate_1_hex \
    compliance_dev_rnk_dh_pk_2_hex \
    compliance_dev_rnk_commitment_2_hex \
    compliance_dev_capability_certificate_2_hex; do
    require_hex_output "$fixture_name" "${!fixture_name}" "$compliance_dev_bundle" "dev Orbis fixture"
done

compliance_genesis_file="${smoke_test_dir}/compliance-genesis.json"
python3 - \
    "$compliance_genesis_file" \
    "$compliance_dev_dk_pub_hex" \
    "$compliance_dev_authority_vk_hex" \
    "$compliance_dev_seizure_authority_vk_hex" \
    "$compliance_dev_ring_pk_hex" \
    "$compliance_dev_ring_id" \
    "$compliance_dev_policy_id" \
    "$compliance_dev_permission" \
    "$compliance_dev_resource" \
    "$smoke_addr_0" \
    "$compliance_dev_rnk_dh_pk_0_hex" \
    "$compliance_dev_rnk_commitment_0_hex" \
    "$compliance_dev_capability_certificate_0_hex" <<'PY'
import base64
import json
import sys

(
    output,
    dk_pub,
    registration_vk,
    seizure_vk,
    ring_pk,
    ring_id,
    policy_id,
    permission,
    resource,
    address,
    rnk_dh_pk,
    rnk_commitment,
    certificate,
) = sys.argv[1:]

def encoded(value):
    return base64.b64encode(bytes.fromhex(value)).decode("ascii")

genesis = {
    "nativeAssets": [{
        "assetId": {"altBaseDenom": "wregulated_usd"},
        "isRegulated": True,
        "dkPub": encoded(dk_pub),
        "registrationAuthorityVk": {"inner": encoded(registration_vk)},
        "seizureAuthorityVk": {"inner": encoded(seizure_vk)},
        "ringPk": encoded(ring_pk),
        "ringId": ring_id,
        "policyId": policy_id,
        "permission": permission,
        "resource": resource,
    }],
    "users": [{
        "address": address,
        "assetId": {"altBaseDenom": "wregulated_usd"},
        "rnkDhPkHex": rnk_dh_pk,
        "rnkCommitmentHex": rnk_commitment,
        "capabilityCertificateHex": certificate,
    }],
}
with open(output, "w", encoding="utf-8") as destination:
    json.dump(genesis, destination)
PY
export COMPLIANCE_GENESIS_INPUT_FILE="$compliance_genesis_file"

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
    height_response=$(curl -s "${SHIELDD_NODE_CMT_URL}/status" 2>&1) || true
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

# --- Compliance smoke test setup ---
# regulated_usd and its funded user are registered at genesis. Register the receiver,
# then send a transfer so the detection scan has on-chain data to find.
>&2 echo "Setting up compliance smoke test environment..."
user_grant_1_output=$("$smoke_pcli_bin" --home "$pcli_test_home" tx compliance sign-user-grant regulated_usd \
    --address "$smoke_addr_1" \
    --policy-id "$compliance_dev_policy_id" \
    --ring-pk-hex "$compliance_dev_ring_pk_hex" \
    --rnk-dh-pk-hex "$compliance_dev_rnk_dh_pk_1_hex" \
    --rnk-commitment-hex "$compliance_dev_rnk_commitment_1_hex" \
    --registration-authority-sk-hex "$compliance_dev_authority_sk_hex" \
    --valid-until-unix "$compliance_grant_valid_until_unix" \
    2>&1) || {
    >&2 echo "ERROR: tx compliance sign-user-grant failed for smoke_addr_1"
    >&2 printf '%s\n' "$user_grant_1_output"
    exit 1
}
user_grant_1=$(printf '%s\n' "$user_grant_1_output" | tail -1)
require_hex_output "user_grant_1" "$user_grant_1" "$user_grant_1_output" "tx compliance sign-user-grant smoke_addr_1"
pcli_tx_cmd tx compliance register-user regulated_usd \
    --address-index 1 \
    --user-registration-grant-hex "$user_grant_1" \
    --capability-certificate-hex "$compliance_dev_capability_certificate_1_hex"
>&2 echo "  User registered for regulated_usd."

# Send a transfer so the detection scan has something to find
"$smoke_pcli_bin" --home "$pcli_test_home" view sync
>&2 echo "  DEBUG: balance before send:"
"$smoke_pcli_bin" --home "$pcli_test_home" view balance 2>&1 | tee /dev/stderr || true
smoke_addr=$("$smoke_pcli_bin" --home "$pcli_test_home" view address 1)
pcli_tx_cmd tx transfer 100regulated_usd --to "$smoke_addr"
>&2 echo "  Compliance transfer sent."

# Export env vars for integration tests
export COMPLIANCE_DK_HEX="$compliance_dev_dk_hex"
export COMPLIANCE_DK_PUB_HEX="$compliance_dev_dk_pub_hex"
export COMPLIANCE_SMOKE_ASSET="regulated_usd"
export COMPLIANCE_DEV_REGISTRAR_SK_HEX="$compliance_dev_registrar_sk_hex"
export COMPLIANCE_DEV_AUTHORITY_SK_HEX="$compliance_dev_authority_sk_hex"
export COMPLIANCE_DEV_AUTHORITY_VK_HEX="$compliance_dev_authority_vk_hex"
export COMPLIANCE_DEV_SEIZURE_AUTHORITY_VK_HEX="$compliance_dev_seizure_authority_vk_hex"
export COMPLIANCE_DEV_RING_PK_HEX="$compliance_dev_ring_pk_hex"
export COMPLIANCE_GRANT_VALID_UNTIL_UNIX="$compliance_grant_valid_until_unix"
export COMPLIANCE_USER_2_RNK_DH_PK_HEX="$compliance_dev_rnk_dh_pk_2_hex"
export COMPLIANCE_USER_2_RNK_COMMITMENT_HEX="$compliance_dev_rnk_commitment_2_hex"
export COMPLIANCE_USER_2_CAPABILITY_CERTIFICATE_HEX="$compliance_dev_capability_certificate_2_hex"
>&2 echo "  Compliance env vars exported."
>&2 echo "Compliance smoke test setup complete."

# Run the integration tests. Using `just` targets so that the exact
# invocations are easily reusable on the CLI in dev loops.
just integration-pclientd
just integration-pcli
# The pd tests come later, as they need work to have been performed for metrics to be emitted.
just integration-pd
