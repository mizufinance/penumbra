#!/bin/bash
# Shared utilities for compliance test scripts.
# Source this file: source "$(dirname "$0")/lib/common.sh"

# --- Repo-local tmp directory for all artifacts ---
COMPLIANCE_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPLIANCE_TMP="${COMPLIANCE_TMP:-$COMPLIANCE_REPO_ROOT/tmp}"
COMPLIANCE_STACK_HOME="${SHIELDD_ORBIS_HOME:-$COMPLIANCE_TMP/shieldd-home}"
COMPLIANCE_NETWORK_DATA_DIR="${COMPLIANCE_STACK_HOME}/network_data"
ORBIS_RUNTIME_FILE="$COMPLIANCE_TMP/orbis-runtime.json"
mkdir -p "$COMPLIANCE_TMP"

SHIELDD_PD_GRPC_PORT="${SHIELDD_PD_GRPC_PORT:-8080}"
SHIELDD_COMETBFT_RPC_PORT="${SHIELDD_COMETBFT_RPC_PORT:-16657}"
SHIELDD_COMETBFT_P2P_PORT="${SHIELDD_COMETBFT_P2P_PORT:-16656}"
SHIELDD_POSTGRES_PORT="${SHIELDD_POSTGRES_PORT:-5432}"
SHIELDD_PCLIENTD_PORT_BASE="${SHIELDD_PCLIENTD_PORT_BASE:-18081}"
SHIELDD_NODE_PD_URL="${SHIELDD_NODE_PD_URL:-http://127.0.0.1:$SHIELDD_PD_GRPC_PORT}"
SHIELDD_NODE_CMT_URL="${SHIELDD_NODE_CMT_URL:-http://127.0.0.1:$SHIELDD_COMETBFT_RPC_PORT}"

export COMPLIANCE_TMP
export ORBIS_RUNTIME_FILE
export SHIELDD_PD_GRPC_PORT
export SHIELDD_COMETBFT_RPC_PORT
export SHIELDD_COMETBFT_P2P_PORT
export SHIELDD_POSTGRES_PORT
export SHIELDD_PCLIENTD_PORT_BASE
export SHIELDD_NODE_PD_URL
export SHIELDD_NODE_CMT_URL

gnark_lib_ext() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "dylib"
    elif [[ "$OSTYPE" == "linux"* ]]; then
        echo "so"
    else
        echo "dylib"
    fi
}

export_demo_gnark_env() {
    local ext
    ext="$(gnark_lib_ext)"

    export SHIELDD_GNARK_TRANSFER_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libshieldd_gnark_transfer.${ext}"
    export SHIELDD_GNARK_TRANSFER_ARTIFACT_DIR="$COMPLIANCE_REPO_ROOT/tools/gnark/artifacts/transfer"

    export SHIELDD_GNARK_NOTE_RESHAPE_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libshieldd_gnark_note_reshape.${ext}"

    export SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libshieldd_gnark_shielded_ics20_withdrawal.${ext}"
    export SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR="$COMPLIANCE_REPO_ROOT/tools/gnark/artifacts/shielded_ics20_withdrawal"
}

export_compliance_rust_log() {
    if [ -z "${RUST_LOG:-}" ]; then
        export RUST_LOG="info"
    fi
}

export_compliance_rust_log

gnark_symbol_grep() {
    local lib_path="$1"
    local symbol="$2"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        nm -gU "$lib_path" 2>/dev/null | grep -q "$symbol"
    else
        nm -D --defined-only "$lib_path" 2>/dev/null | grep -q "$symbol"
    fi
}

validate_demo_gnark_lib() {
    local lib_path="$1"
    local symbol="$2"

    [ -f "$lib_path" ] || return 1

    if command -v python3 >/dev/null 2>&1; then
        python3 - "$lib_path" >/dev/null 2>&1 <<'PY'
import ctypes
import sys

ctypes.CDLL(sys.argv[1])
PY
    else
        gnark_symbol_grep "$lib_path" "$symbol" || return 1
    fi

    gnark_symbol_grep "$lib_path" "$symbol"
}

build_demo_gnark_libs() {
    command -v go >/dev/null 2>&1 || {
        log_error "go not found in PATH; cannot rebuild demo gnark libraries"
        return 1
    }

    (
        cd "$COMPLIANCE_REPO_ROOT/tools/gnark"
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libshieldd_gnark_note_reshape.$(gnark_lib_ext)" ./cmd/note_reshapelib
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libshieldd_gnark_transfer.$(gnark_lib_ext)" ./cmd/transferlib
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libshieldd_gnark_shielded_ics20_withdrawal.$(gnark_lib_ext)" ./cmd/shieldedics20withdrawallib
    )
}

ensure_demo_gnark_libs() {
    local ext
    ext="$(gnark_lib_ext)"
    local needs_rebuild=0
    local lib_path

    for spec in \
        "note_reshape:shieldd_gnark_note_reshape_init" \
        "transfer:shieldd_gnark_transfer_init" \
        "shielded_ics20_withdrawal:shieldd_gnark_shielded_ics20_withdrawal_init"
    do
        local family="${spec%%:*}"
        local symbol="${spec#*:}"
        lib_path="$COMPLIANCE_REPO_ROOT/tools/gnark/libshieldd_gnark_${family}.${ext}"
        if ! validate_demo_gnark_lib "$lib_path" "$symbol"; then
            log_warning "demo gnark runtime is missing or invalid: $lib_path"
            needs_rebuild=1
        fi
    done

    if [ "$needs_rebuild" -eq 1 ]; then
        log_info "Rebuilding demo gnark shared libraries..."
        build_demo_gnark_libs || {
            log_error "Failed to rebuild demo gnark shared libraries"
            return 1
        }
    fi

    for spec in \
        "note_reshape:shieldd_gnark_note_reshape_init" \
        "transfer:shieldd_gnark_transfer_init" \
        "shielded_ics20_withdrawal:shieldd_gnark_shielded_ics20_withdrawal_init"
    do
        local family="${spec%%:*}"
        local symbol="${spec#*:}"
        lib_path="$COMPLIANCE_REPO_ROOT/tools/gnark/libshieldd_gnark_${family}.${ext}"
        validate_demo_gnark_lib "$lib_path" "$symbol" || {
            log_error "demo gnark runtime failed validation: $lib_path"
            return 1
        }
    done

    log_success "Demo gnark runtimes validated"
}

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[OK]${NC} $*"; }
log_warning() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $*"; }

is_tcp_host_port_open() {
    local host="$1"
    local port="$2"

    if command -v nc >/dev/null 2>&1; then
        nc -z -w1 "$host" "$port" >/dev/null 2>&1
        return $?
    fi

    (echo > /dev/tcp/"$host"/"$port") >/dev/null 2>&1
}

is_tcp_port_in_use() {
    local port="$1"
    is_tcp_host_port_open 127.0.0.1 "$port"
}

ensure_ports_available() {
    local has_conflict=0
    local port

    for port in "$@"; do
        if ! is_tcp_port_in_use "$port"; then
            continue
        fi

        log_error "TCP port $port is already in use"
        if command -v lsof >/dev/null 2>&1; then
            lsof -nP -iTCP:"$port" -sTCP:LISTEN >&2 || true
        fi
        has_conflict=1
    done

    if [ "$has_conflict" -eq 1 ]; then
        log_error "Free the conflicting ports or run ./scripts/shieldd-down.sh and ./scripts/orbis-stack.sh down"
        return 1
    fi
}

# --- Test counters ---
PASSED=0
FAILED=0
pass() { echo -e "  ${GREEN}PASS${NC}: $1"; PASSED=$((PASSED + 1)); }
fail() { echo -e "  ${RED}FAIL${NC}: $1"; FAILED=$((FAILED + 1)); }
print_results() { echo ""; echo "=== Results: $PASSED passed, $FAILED failed ==="; }

# --- Run command silently (show output only on failure) ---
run_quiet() {
    local tmpfile
    tmpfile=$(mktemp)
    if ! "$@" >"$tmpfile" 2>&1; then
        log_error "Command failed: $*"
        cat "$tmpfile" >&2
        rm -f "$tmpfile"
        return 1
    fi
    rm -f "$tmpfile"
}

# --- Validate hex string: charset + length ---
validate_hex() {
    local name="$1" val="$2" expected="$3"
    if ! [[ "$val" =~ ^[0-9a-fA-F]+$ ]]; then
        log_error "$name contains non-hex characters"
        return 1
    fi
    local actual=${#val}
    if [ "$actual" -ne "$expected" ]; then
        log_error "$name has $actual hex chars, expected $expected"
        return 1
    fi
    return 0
}

# --- Wait for HTTP URL to respond ---
wait_for_url() {
    local url="$1"
    local max_attempts="${2:-30}"
    local interval="${3:-2}"
    for attempt in $(seq 1 "$max_attempts"); do
        if curl -sf "$url" >/dev/null 2>&1; then
            return 0
        fi
        if [ "$attempt" -eq "$max_attempts" ]; then
            log_error "Timed out waiting for $url"
            return 1
        fi
        echo "    ... waiting ($attempt/$max_attempts)"
        sleep "$interval"
    done
}

# --- Wait for gRPC port ---
wait_for_grpc() {
    local port="$1"
    local max_attempts="${2:-30}"
    local interval="${3:-2}"
    for attempt in $(seq 1 "$max_attempts"); do
        if is_tcp_host_port_open 127.0.0.1 "$port"; then
            return 0
        fi
        if [ "$attempt" -eq "$max_attempts" ]; then
            log_error "Timed out waiting for port $port"
            return 1
        fi
        echo "    ... waiting ($attempt/$max_attempts)"
        sleep "$interval"
    done
}

wait_for_tcp_port() {
    local port="$1"
    local max_attempts="${2:-30}"
    local interval="${3:-2}"
    wait_for_grpc "$port" "$max_attempts" "$interval"
}

extract_toml_string() {
    local file="$1"
    local key="$2"

    awk -F'"' -v key="$key" '$1 == key " = " { print $2; exit }' "$file"
}

set_pcli_view_url() {
    local config_path="$1"
    local view_url="$2"
    local tmpfile
    tmpfile=$(mktemp)

    awk -v view_url="$view_url" '
        BEGIN { updated = 0 }
        /^view_url = / {
            print "view_url = \"" view_url "\""
            updated = 1
            next
        }
        /^grpc_url = / {
            print
            if (!updated) {
                print "view_url = \"" view_url "\""
                updated = 1
            }
            next
        }
        { print }
        END {
            if (!updated) {
                print "view_url = \"" view_url "\""
            }
        }
    ' "$config_path" > "$tmpfile"

    mv "$tmpfile" "$config_path"
}

configure_wallet_view_service() {
    local wallet_name="$1"
    local wallet_home="$2"
    local daemon_home="$3"
    local bind_port="$4"
    local pcli_bin="$5"
    local pclientd_bin="$6"
    local pid_file="$7"
    local config_path="$wallet_home/config.toml"
    local fvk
    local grpc_url
    local view_url="http://127.0.0.1:${bind_port}"
    local daemon_log="$COMPLIANCE_TMP/${wallet_name}-pclientd.log"
    local daemon_pid

    fvk="$(extract_toml_string "$config_path" "full_viewing_key")"
    grpc_url="$(extract_toml_string "$config_path" "grpc_url")"

    if [ -z "$fvk" ] || [ -z "$grpc_url" ]; then
        log_error "failed to read wallet config for $wallet_name from $config_path"
        return 1
    fi

    rm -rf "$daemon_home"
    mkdir -p "$daemon_home"

    printf '%s\n' "$fvk" | "$pclientd_bin" --home "$daemon_home" init \
        --view \
        --grpc-url "$grpc_url" \
        --bind-addr "127.0.0.1:${bind_port}" >/dev/null

    set_pcli_view_url "$config_path" "$view_url"

    "$pclientd_bin" --home "$daemon_home" start > "$daemon_log" 2>&1 &
    daemon_pid=$!
    echo "${wallet_name}_PCLIENTD_PID=$daemon_pid" >> "$pid_file"

    wait_for_tcp_port "$bind_port" 30 1

    for attempt in $(seq 1 30); do
        if ! kill -0 "$daemon_pid" 2>/dev/null; then
            log_error "$wallet_name pclientd exited early"
            tail -n 50 "$daemon_log" >&2 || true
            return 1
        fi

        if "$pcli_bin" --home "$wallet_home" view balance >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done

    log_error "$wallet_name pclientd did not become ready"
    tail -n 50 "$daemon_log" >&2 || true
    return 1
}

require_docker_compose() {
    if ! docker compose version >/dev/null 2>&1; then
        log_error "Docker Compose v2 is required"
        return 1
    fi
}

docker_daemon_ready() {
    command -v docker >/dev/null 2>&1 || return 1
    docker info >/dev/null 2>&1
}

ensure_docker_daemon() {
    docker_daemon_ready && return 0
    log_error "Docker daemon is not running"
    log_error "Start Docker Desktop or your local Docker service, then rerun the command"
    return 1
}

orbis_pinned_rev_from_cargo() {
    local cargo_toml="$COMPLIANCE_REPO_ROOT/crates/util/orbis-client/Cargo.toml"
    if [ ! -f "$cargo_toml" ]; then
        log_error "Cannot derive Orbis pin: $cargo_toml not found"
        return 1
    fi
    # All three orbis-rs git deps must share one rev.
    local pins
    pins="$(grep -oE 'orbis-rs", rev = "[0-9a-f]{40}"' "$cargo_toml" | grep -oE '[0-9a-f]{40}')"
    local pin_count
    pin_count="$(printf '%s\n' "$pins" | grep -c '^[0-9a-f]')"
    if [ "$pin_count" -ne 3 ]; then
        log_error "Expected exactly three Orbis Cargo rev pins, found $pin_count"
        return 1
    fi
    local revs
    revs="$(printf '%s\n' "$pins" | sort -u)"
    local rev_count
    rev_count="$(printf '%s\n' "$revs" | grep -c '^[0-9a-f]')"
    if [ "$rev_count" -ne 1 ]; then
        log_error "Orbis Cargo.toml rev pins are inconsistent: $revs"
        return 1
    fi
    printf '%s' "$revs"
}

# Load the pinned integration runtime and verify that the Orbis image's source
# revision matches all three Cargo git dependencies. Vera is built locally from
# its pinned source revision because there is no matching published image.
ensure_orbis_images() {
    local lock_file="$COMPLIANCE_REPO_ROOT/deployments/orbis/images.lock.json"
    if [ ! -f "$lock_file" ]; then
        log_error "Orbis image lock not found: $lock_file"
        return 1
    fi
    if ! command -v jq >/dev/null 2>&1; then
        log_error "jq not found; cannot load Orbis image lock"
        return 1
    fi
    if ! jq -e '
        .schema_version == 1
        and (.orbis.source_revision | strings | test("^[0-9a-f]{40}$"))
        and (.orbis.crypto | strings | length > 0)
        and (.orbis.image | strings | test("^ghcr\\.io/sourcenetwork/orbis-rs@sha256:[0-9a-f]{64}$"))
        and (.vera.source_revision | strings | test("^[0-9a-f]{40}$"))
    ' "$lock_file" >/dev/null; then
        log_error "Invalid Orbis image lock: $lock_file"
        return 1
    fi

    local cargo_rev lock_rev
    cargo_rev="$(orbis_pinned_rev_from_cargo)" || return 1
    lock_rev="$(jq -r '.orbis.source_revision' "$lock_file")"
    if [ "$cargo_rev" != "$lock_rev" ]; then
        log_error "Orbis image revision $lock_rev does not match Cargo revision $cargo_rev"
        return 1
    fi

    if [ "${CI:-}" = "true" ] \
        && { [ "${ORBIS_IMAGE+x}" = "x" ] || [ "${VERA_IMAGE+x}" = "x" ] || [ "${VERA_REF+x}" = "x" ]; }; then
        log_error "CI may not override the pinned Orbis/Vera integration runtime"
        return 1
    fi

    export ORBIS_IMAGE="${ORBIS_IMAGE:-$(jq -r '.orbis.image' "$lock_file")}"
    export VERA_REF="${VERA_REF:-$(jq -r '.vera.source_revision' "$lock_file")}"
    export VERA_IMAGE="${VERA_IMAGE:-shieldd-vera:local}"
}

orbis_compose_project_name() {
    printf '%s\n' "${ORBIS_COMPOSE_PROJECT_NAME:-shieldd-orbis-integration}"
}

run_orbis_compose() {
    local compose_file="$1"
    shift
    require_docker_compose || return 1
    local project_name
    project_name="$(orbis_compose_project_name)"
    docker compose -p "$project_name" -f "$compose_file" "$@"
}

orbis_published_port() {
    local compose_file="$1"
    local service="$2"
    local container_port="$3"
    local binding
    local port

    binding="$(run_orbis_compose "$compose_file" port "$service" "$container_port" | tail -n 1)" || return 1
    port="${binding##*:}"
    if [[ ! "$port" =~ ^[0-9]+$ ]] || [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
        log_error "Invalid published port for $service:$container_port: $binding"
        return 1
    fi
    printf '%s\n' "$port"
}

write_orbis_runtime_config() {
    local compose_file="$1"
    local vera_rpc_port vera_rest_port vera_grpc_port
    local node1_port node2_port node3_port
    local runtime_tmp

    vera_rpc_port="$(orbis_published_port "$compose_file" vera 26657)" || return 1
    vera_rest_port="$(orbis_published_port "$compose_file" vera 1317)" || return 1
    vera_grpc_port="$(orbis_published_port "$compose_file" vera 9090)" || return 1
    node1_port="$(orbis_published_port "$compose_file" node1 50051)" || return 1
    node2_port="$(orbis_published_port "$compose_file" node2 50051)" || return 1
    node3_port="$(orbis_published_port "$compose_file" node3 50051)" || return 1

    runtime_tmp="$(mktemp "${ORBIS_RUNTIME_FILE}.tmp.XXXXXX")"
    if ! jq -n \
        --arg vera_rpc "http://127.0.0.1:$vera_rpc_port" \
        --arg vera_rest "http://127.0.0.1:$vera_rest_port" \
        --arg vera_grpc "http://127.0.0.1:$vera_grpc_port" \
        --arg node1 "http://127.0.0.1:$node1_port" \
        --arg node2 "http://127.0.0.1:$node2_port" \
        --arg node3 "http://127.0.0.1:$node3_port" \
        '{
            vera_rpc: $vera_rpc,
            vera_rest: $vera_rest,
            vera_grpc: $vera_grpc,
            node1: $node1,
            node2: $node2,
            node3: $node3
        }' > "$runtime_tmp"
    then
        rm -f "$runtime_tmp"
        return 1
    fi
    chmod 600 "$runtime_tmp"
    mv "$runtime_tmp" "$ORBIS_RUNTIME_FILE"
    log_info "Orbis runtime endpoints: $(jq -c . "$ORBIS_RUNTIME_FILE")"
}

orbis_compose_service_container_id() {
    local service="$1"
    local project_name
    local container_ids

    project_name="$(orbis_compose_project_name)"
    container_ids="$(docker ps -aq \
        --filter "label=com.docker.compose.project=$project_name" \
        --filter "label=com.docker.compose.service=$service")"
    if [[ "$container_ids" == *$'\n'* ]]; then
        log_error "Multiple $service containers found for project $project_name" >&2
        return 1
    fi
    printf '%s\n' "$container_ids"
}

orbis_container_ipv4() {
    local service="$1"
    local container_id
    local address

    container_id="$(orbis_compose_service_container_id "$service")" || return 1
    if [ -z "$container_id" ]; then
        log_error "No $service container found" >&2
        return 1
    fi
    address="$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$container_id")" || return 1
    if [[ ! "$address" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
        log_error "Invalid container IPv4 address for $service: $address" >&2
        return 1
    fi
    printf '%s\n' "$address"
}

resolve_orbis_node_endpoint() {
    local service="$1"
    local published_endpoint="$2"
    local published_port="${published_endpoint##*:}"
    local container_ip
    local attempt

    if [[ ! "$published_port" =~ ^[0-9]+$ ]]; then
        log_error "Invalid published endpoint for $service: $published_endpoint" >&2
        return 1
    fi

    for ((attempt = 1; attempt <= 30; attempt++)); do
        if is_tcp_host_port_open 127.0.0.1 "$published_port"; then
            printf '%s\n' "$published_endpoint"
            return 0
        fi

        container_ip="$(orbis_container_ipv4 "$service")" || return 1
        if is_tcp_host_port_open "$container_ip" 50051; then
            log_warning "$service published endpoint is unavailable; using routed container endpoint" >&2
            printf 'http://%s:50051\n' "$container_ip"
            return 0
        fi
        sleep 1
    done

    log_error "No host-routable production endpoint found for $service" >&2
    return 1
}

resolve_orbis_runtime_node_endpoints() {
    local published1 published2 published3
    local node1 node2 node3
    local runtime_tmp

    published1="$(jq -r '.node1' "$ORBIS_RUNTIME_FILE")"
    published2="$(jq -r '.node2' "$ORBIS_RUNTIME_FILE")"
    published3="$(jq -r '.node3' "$ORBIS_RUNTIME_FILE")"
    node1="$(resolve_orbis_node_endpoint node1 "$published1")" || return 1
    node2="$(resolve_orbis_node_endpoint node2 "$published2")" || return 1
    node3="$(resolve_orbis_node_endpoint node3 "$published3")" || return 1

    runtime_tmp="$(mktemp "${ORBIS_RUNTIME_FILE}.tmp.XXXXXX")"
    if ! jq \
        --arg node1 "$node1" \
        --arg node2 "$node2" \
        --arg node3 "$node3" \
        '.node1 = $node1 | .node2 = $node2 | .node3 = $node3' \
        "$ORBIS_RUNTIME_FILE" > "$runtime_tmp"
    then
        rm -f "$runtime_tmp"
        return 1
    fi
    chmod 600 "$runtime_tmp"
    mv "$runtime_tmp" "$ORBIS_RUNTIME_FILE"
    log_info "Resolved Orbis runtime endpoints: $(jq -c . "$ORBIS_RUNTIME_FILE")"
}

wait_for_orbis_funder() {
    local compose_file="$1"
    local container_id
    local state
    local attempt

    for ((attempt = 1; attempt <= 180; attempt++)); do
        container_id="$(orbis_compose_service_container_id orbis-funder)" || return 1
        if [ -n "$container_id" ]; then
            state="$(docker inspect --format '{{.State.Status}} {{.State.ExitCode}}' "$container_id")" || return 1
            case "$state" in
                "exited 0")
                    log_success "Orbis funder completed"
                    return 0
                    ;;
                exited\ *)
                    log_error "Orbis funder failed: $state"
                    run_orbis_compose "$compose_file" logs orbis-funder >&2 || true
                    return 1
                    ;;
                dead\ *)
                    log_error "Orbis funder entered an unrecoverable state: $state"
                    run_orbis_compose "$compose_file" logs orbis-funder >&2 || true
                    return 1
                    ;;
            esac
        fi
        if [ $((attempt % 15)) -eq 0 ]; then
            log_info "Waiting for Orbis funder ($attempt/180)"
        fi
        sleep 2
    done

    log_error "Timed out waiting for Orbis funder"
    run_orbis_compose "$compose_file" logs orbis-funder >&2 || true
    return 1
}

wait_for_orbis_node_production() {
    local service="$1"
    local marker='Server is ready to accept connections'
    local container_id
    local inspection
    local state
    local started_at
    local attempt

    for ((attempt = 1; attempt <= 120; attempt++)); do
        container_id="$(orbis_compose_service_container_id "$service")" || return 1
        if [ -n "$container_id" ]; then
            inspection="$(docker inspect --format '{{.State.Status}} {{.State.ExitCode}} {{.State.StartedAt}}' "$container_id")" || return 1
            state="${inspection% *}"
            started_at="${inspection##* }"
            case "$state" in
                exited\ *|dead\ *)
                    log_error "$service stopped before production readiness: $state"
                    docker logs "$container_id" >&2 || true
                    return 1
                    ;;
            esac
            if (set +o pipefail; docker logs --since "$started_at" "$container_id" 2>&1 | grep -Fq "$marker"); then
                log_success "$service production server ready"
                return 0
            fi
        fi
        if [ $((attempt % 15)) -eq 0 ]; then
            log_info "Waiting for $service production server ($attempt/120)"
        fi
        sleep 2
    done

    log_error "Timed out waiting for $service production server"
    if [ -n "$container_id" ]; then
        docker logs "$container_id" >&2 || true
    fi
    return 1
}

wait_for_orbis_stack() {
    local compose_file="$1"
    local vera_rpc

    if ! jq -e '
        [.vera_rpc, .vera_rest, .vera_grpc, .node1, .node2, .node3]
        | all(type == "string" and length > 0)
    ' "$ORBIS_RUNTIME_FILE" >/dev/null; then
        log_error "Invalid Orbis runtime endpoint file: $ORBIS_RUNTIME_FILE"
        return 1
    fi

    vera_rpc="$(jq -r '.vera_rpc' "$ORBIS_RUNTIME_FILE")"

    wait_for_url "$vera_rpc/status" 60 2 || return 1
    wait_for_orbis_funder "$compose_file" || return 1
    wait_for_orbis_node_production node1 || return 1
    wait_for_orbis_node_production node2 || return 1
    wait_for_orbis_node_production node3 || return 1
    resolve_orbis_runtime_node_endpoints || return 1
}

wait_for_shieldd_stack() {
    wait_for_shieldd "$SHIELDD_COMETBFT_RPC_PORT" 45 2 5 || return 1
    wait_for_tcp_port "$SHIELDD_PD_GRPC_PORT" 30 1 || return 1
}

kill_tracked_pids() {
    local pid_file="$1"

    [ -f "$pid_file" ] || return 0

    while IFS='=' read -r _ pid; do
        [ -n "${pid:-}" ] || continue
        kill "$pid" 2>/dev/null || true
    done < "$pid_file"

    while IFS='=' read -r _ pid; do
        [ -n "${pid:-}" ] || continue
        wait "$pid" 2>/dev/null || true
    done < "$pid_file"

    rm -f "$pid_file"
}

# --- Wait for Shieldd node to be fully ready (blocks producing) ---
wait_for_shieldd() {
    local cometbft_port="${1:-16657}"
    local max_attempts="${2:-45}"
    local interval="${3:-2}"
    local min_height="${4:-1}"
    local url="http://localhost:${cometbft_port}/status"

    for attempt in $(seq 1 "$max_attempts"); do
        local height
        height=$(curl -sf "$url" 2>/dev/null \
            | jq -r '.result.sync_info.latest_block_height' 2>/dev/null || echo "0")
        if [ "$height" -ge "$min_height" ] 2>/dev/null; then
            return 0
        fi
        if [ "$attempt" -eq "$max_attempts" ]; then
            log_error "Shieldd did not reach height $min_height within $((max_attempts * interval))s"
            return 1
        fi
        echo "    ... waiting for Shieldd height >= $min_height ($attempt/$max_attempts)"
        sleep "$interval"
    done
}

# --- Active polling for PRE status ---
poll_pre_status() {
    local expected="$1"; shift
    local max_attempts=5
    local interval=3
    local output=""
    local rc=0
    for attempt in $(seq 1 $max_attempts); do
        set +e
        output=$("$@" 2>&1)
        rc=$?
        set -e
        if [ "$expected" = "deny" ]; then
            if [ $rc -ne 0 ] || echo "$output" | grep -qi "error\|denied\|fail\|unauthorized"; then
                echo "$output"
                return 0
            fi
        elif [ "$expected" = "allow" ]; then
            if [ $rc -eq 0 ] && echo "$output" | grep -q "Decrypted Secret:"; then
                echo "$output"
                return 0
            fi
        fi
        echo "     polling ($attempt/$max_attempts)..." >&2
        sleep $interval
    done
    echo "$output"
    return 1
}

# --- Load env file ---
load_env() {
    local env_file="${1:-$COMPLIANCE_TMP/compliance-demo.env}"
    if [ ! -f "$env_file" ]; then
        log_error "Environment file not found: $env_file"
        log_error "Run `just orbis-integration-up` first"
        exit 1
    fi
    source "$env_file"
}

# --- Demo output helpers ---
print_banner() {
    local title="$1"
    local subtitle="${2:-}"
    local width=72
    local border
    border=$(printf '═%.0s' $(seq 1 $width))
    echo ""
    echo "╔${border}╗"
    printf "║  %-$((width - 2))s║\n" "$title"
    if [ -n "$subtitle" ]; then
        printf "║  %-$((width - 2))s║\n" "$subtitle"
    fi
    echo "╚${border}╝"
    echo ""
}

print_state_banner() {
    local state="$1"
    local description="$2"
    local width=72
    local border
    border=$(printf '═%.0s' $(seq 1 $width))
    echo ""
    echo "╔${border}╗"
    printf "║  %-$((width - 2))s║\n" "STATE ${state}: ${description}"
    echo "╚${border}╝"
    echo ""
}

print_phase() {
    local title="$1"
    local width=72
    local line
    line=$(printf '─%.0s' $(seq 1 $width))
    echo ""
    echo "$line"
    echo "  $title"
    echo "$line"
    echo ""
}

maybe_enable_demo_gnark_env() {
    if [ "${SHIELDD_ORBIS_USE_DEMO_GNARK:-0}" != "1" ]; then
        return 0
    fi

    log_info "SHIELDD_ORBIS_USE_DEMO_GNARK=1 enabled; validating demo gnark runtimes"
    ensure_demo_gnark_libs
    export_demo_gnark_env
}

maybe_enable_demo_gnark_env
