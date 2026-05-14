#!/bin/bash
# Shared utilities for compliance test scripts.
# Source this file: source "$(dirname "$0")/lib/common.sh"

# --- Repo-local tmp directory for all artifacts ---
COMPLIANCE_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPLIANCE_TMP="${COMPLIANCE_TMP:-$COMPLIANCE_REPO_ROOT/tmp}"
COMPLIANCE_STACK_HOME="${PENUMBRA_ORBIS_HOME:-$COMPLIANCE_TMP/penumbra-home}"
COMPLIANCE_NETWORK_DATA_DIR="${COMPLIANCE_STACK_HOME}/network_data"
mkdir -p "$COMPLIANCE_TMP"

PENUMBRA_PD_GRPC_PORT="${PENUMBRA_PD_GRPC_PORT:-8080}"
PENUMBRA_COMETBFT_RPC_PORT="${PENUMBRA_COMETBFT_RPC_PORT:-16657}"
PENUMBRA_COMETBFT_P2P_PORT="${PENUMBRA_COMETBFT_P2P_PORT:-16656}"
PENUMBRA_POSTGRES_PORT="${PENUMBRA_POSTGRES_PORT:-5432}"
PENUMBRA_PCLIENTD_PORT_BASE="${PENUMBRA_PCLIENTD_PORT_BASE:-18081}"
PENUMBRA_NODE_PD_URL="${PENUMBRA_NODE_PD_URL:-http://127.0.0.1:$PENUMBRA_PD_GRPC_PORT}"
PENUMBRA_NODE_CMT_URL="${PENUMBRA_NODE_CMT_URL:-http://127.0.0.1:$PENUMBRA_COMETBFT_RPC_PORT}"

export COMPLIANCE_TMP
export PENUMBRA_PD_GRPC_PORT
export PENUMBRA_COMETBFT_RPC_PORT
export PENUMBRA_COMETBFT_P2P_PORT
export PENUMBRA_POSTGRES_PORT
export PENUMBRA_PCLIENTD_PORT_BASE
export PENUMBRA_NODE_PD_URL
export PENUMBRA_NODE_CMT_URL

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

    export PENUMBRA_GNARK_TRANSFER_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libpenumbra_gnark_transfer.${ext}"
    export PENUMBRA_GNARK_TRANSFER_ARTIFACT_DIR="$COMPLIANCE_REPO_ROOT/tools/gnark/artifacts/transfer"

    export PENUMBRA_GNARK_SPLIT_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libpenumbra_gnark_split.${ext}"
    export PENUMBRA_GNARK_SPLIT_ARTIFACT_DIR="$COMPLIANCE_REPO_ROOT/tools/gnark/artifacts/split1x4"
    export PENUMBRA_GNARK_SPLIT1X4_ARTIFACT_DIR="$PENUMBRA_GNARK_SPLIT_ARTIFACT_DIR"

    export PENUMBRA_GNARK_CONSOLIDATE_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libpenumbra_gnark_consolidate.${ext}"
    export PENUMBRA_GNARK_CONSOLIDATE_ARTIFACT_DIR="$COMPLIANCE_REPO_ROOT/tools/gnark/artifacts/consolidate2x1"
    export PENUMBRA_GNARK_CONSOLIDATE2X1_ARTIFACT_DIR="$PENUMBRA_GNARK_CONSOLIDATE_ARTIFACT_DIR"

    export PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB="$COMPLIANCE_REPO_ROOT/tools/gnark/libpenumbra_gnark_shielded_ics20_withdrawal.${ext}"
    export PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR="$COMPLIANCE_REPO_ROOT/tools/gnark/artifacts/shielded_ics20_withdrawal"
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
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libpenumbra_gnark_split.$(gnark_lib_ext)" ./cmd/splitlib
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libpenumbra_gnark_transfer.$(gnark_lib_ext)" ./cmd/transferlib
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libpenumbra_gnark_consolidate.$(gnark_lib_ext)" ./cmd/consolidatelib
        CGO_ENABLED=1 go build -buildmode=c-shared -o "libpenumbra_gnark_shielded_ics20_withdrawal.$(gnark_lib_ext)" ./cmd/shieldedics20withdrawallib
    )
}

ensure_demo_gnark_libs() {
    local ext
    ext="$(gnark_lib_ext)"
    local needs_rebuild=0
    local lib_path

    for spec in \
        "split:penumbra_gnark_split_init" \
        "transfer:penumbra_gnark_transfer_init" \
        "consolidate:penumbra_gnark_consolidate_init" \
        "shielded_ics20_withdrawal:penumbra_gnark_shielded_ics20_withdrawal_init"
    do
        local family="${spec%%:*}"
        local symbol="${spec#*:}"
        lib_path="$COMPLIANCE_REPO_ROOT/tools/gnark/libpenumbra_gnark_${family}.${ext}"
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
        "split:penumbra_gnark_split_init" \
        "transfer:penumbra_gnark_transfer_init" \
        "consolidate:penumbra_gnark_consolidate_init" \
        "shielded_ics20_withdrawal:penumbra_gnark_shielded_ics20_withdrawal_init"
    do
        local family="${spec%%:*}"
        local symbol="${spec#*:}"
        lib_path="$COMPLIANCE_REPO_ROOT/tools/gnark/libpenumbra_gnark_${family}.${ext}"
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

is_tcp_port_in_use() {
    local port="$1"

    if command -v nc >/dev/null 2>&1; then
        nc -z 127.0.0.1 "$port" >/dev/null 2>&1
        return $?
    fi

    (echo > /dev/tcp/127.0.0.1/"$port") >/dev/null 2>&1
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
        log_error "Free the conflicting ports or run ./scripts/penumbra-down.sh and ./scripts/orbis-stack.sh down"
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
        if command -v nc >/dev/null 2>&1; then
            if nc -z -w1 127.0.0.1 "$port" 2>/dev/null; then
                return 0
            fi
        elif (echo > /dev/tcp/127.0.0.1/"$port") >/dev/null 2>&1; then
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

docker_compose_flavor() {
    if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
        printf 'docker-compose-v2\n'
        return 0
    fi
    if command -v docker-compose >/dev/null 2>&1; then
        printf 'docker-compose-v1\n'
        return 0
    fi
    log_error "docker compose not found"
    return 1
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

ensure_orbis_runtime_checkout() {
    local repo_url="${ORBIS_RUNTIME_REPO:-https://github.com/sourcenetwork/orbis-rs.git}"
    local ref="${ORBIS_RUNTIME_REF:-60be66516c9e0c1fe88fadf7dc8577a3d489004d}"
    local default_checkout_dir="$COMPLIANCE_TMP/orbis-rs"
    local checkout_dir="${ORBIS_RUNTIME_CONTEXT:-$default_checkout_dir}"
    local current_ref=""

    if [ -n "${ORBIS_RUNTIME_CONTEXT:-}" ]; then
        if [ ! -f "$checkout_dir/docker/Dockerfile" ]; then
            log_error "ORBIS_RUNTIME_CONTEXT is set but $checkout_dir/docker/Dockerfile is missing"
            return 1
        fi
        export ORBIS_RUNTIME_CONTEXT="$checkout_dir"
        return 0
    fi

    command -v git >/dev/null 2>&1 || {
        log_error "git not found in PATH; cannot prepare Orbis runtime checkout"
        return 1
    }

    if [ -d "$checkout_dir/.git" ]; then
        current_ref="$(git -C "$checkout_dir" rev-parse HEAD 2>/dev/null || true)"
    fi
    if [ "$current_ref" = "$ref" ] && [ -f "$checkout_dir/docker/Dockerfile" ]; then
        export ORBIS_RUNTIME_CONTEXT="$checkout_dir"
        return 0
    fi

    rm -rf "$default_checkout_dir"
    mkdir -p "$(dirname "$checkout_dir")"

    log_info "Preparing Orbis runtime checkout at $checkout_dir ($ref)"
    run_quiet git clone --filter=blob:none "$repo_url" "$checkout_dir" || {
        log_error "Failed to clone Orbis runtime repo from $repo_url"
        return 1
    }
    run_quiet git -C "$checkout_dir" checkout "$ref" || {
        log_error "Failed to checkout Orbis runtime ref $ref"
        return 1
    }

    if [ ! -f "$checkout_dir/docker/Dockerfile" ]; then
        log_error "Expected Orbis Dockerfile missing at $checkout_dir/docker/Dockerfile"
        return 1
    fi

    export ORBIS_RUNTIME_CONTEXT="$checkout_dir"
}

run_orbis_compose() {
    local compose_file="$1"
    shift
    local flavor
    flavor="$(docker_compose_flavor)" || return 1
    local project_name="${ORBIS_COMPOSE_PROJECT_NAME:-penumbra-orbis-integration}"
    case "$flavor" in
        docker-compose-v2)
            docker compose -p "$project_name" -f "$compose_file" "$@"
            ;;
        docker-compose-v1)
            docker-compose -p "$project_name" -f "$compose_file" "$@"
            ;;
    esac
}

wait_for_orbis_stack() {
    wait_for_url "http://127.0.0.1:${ORBIS_SOURCEHUB_RPC_PORT:-26657}/status" 60 2 || return 1
    wait_for_tcp_port "${ORBIS_NODE1_GRPC_PORT:-50051}" 60 2 || return 1
    wait_for_tcp_port "${ORBIS_NODE2_GRPC_PORT:-50052}" 60 2 || return 1
    wait_for_tcp_port "${ORBIS_NODE3_GRPC_PORT:-50053}" 60 2 || return 1
}

wait_for_penumbra_stack() {
    wait_for_penumbra "$PENUMBRA_COMETBFT_RPC_PORT" 45 2 5 || return 1
    wait_for_tcp_port "$PENUMBRA_PD_GRPC_PORT" 30 1 || return 1
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

# --- Wait for Penumbra node to be fully ready (blocks producing) ---
wait_for_penumbra() {
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
            log_error "Penumbra did not reach height $min_height within $((max_attempts * interval))s"
            return 1
        fi
        echo "    ... waiting for Penumbra height >= $min_height ($attempt/$max_attempts)"
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
    if [ "${PENUMBRA_ORBIS_USE_DEMO_GNARK:-0}" != "1" ]; then
        return 0
    fi

    log_info "PENUMBRA_ORBIS_USE_DEMO_GNARK=1 enabled; validating demo gnark runtimes"
    ensure_demo_gnark_libs
    export_demo_gnark_env
}

maybe_enable_demo_gnark_env
