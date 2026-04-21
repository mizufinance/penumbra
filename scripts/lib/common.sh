#!/bin/bash
# Shared utilities for compliance test scripts.
# Source this file: source "$(dirname "$0")/lib/common.sh"

# --- Repo-local tmp directory for all artifacts ---
COMPLIANCE_TMP="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/tmp"
COMPLIANCE_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$COMPLIANCE_TMP"

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

export_demo_gnark_env

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
        log_error "Run scripts/setup-penumbra.sh first"
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
