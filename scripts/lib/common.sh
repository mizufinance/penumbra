#!/bin/bash
# Shared utilities for compliance test scripts.
# Source this file: source "$(dirname "$0")/lib/common.sh"

# --- Repo-local tmp directory for all artifacts ---
COMPLIANCE_TMP="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/tmp"
mkdir -p "$COMPLIANCE_TMP"

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
    local url="http://localhost:${cometbft_port}/status"

    for attempt in $(seq 1 "$max_attempts"); do
        local height
        height=$(curl -sf "$url" 2>/dev/null \
            | jq -r '.result.sync_info.latest_block_height' 2>/dev/null || echo "0")
        if [ "$height" -gt 0 ] 2>/dev/null; then
            return 0
        fi
        if [ "$attempt" -eq "$max_attempts" ]; then
            log_error "Penumbra did not produce a block within $((max_attempts * interval))s"
            return 1
        fi
        echo "    ... waiting for first block ($attempt/$max_attempts)"
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
