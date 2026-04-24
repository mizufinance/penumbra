#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

require_binaries=0
check_ports_free=0

usage() {
    cat >&2 <<'EOF'
usage: ./scripts/orbis-integration-preflight.sh [--require-binaries] [--check-ports-free]
EOF
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --require-binaries)
            require_binaries=1
            ;;
        --check-ports-free)
            check_ports_free=1
            ;;
        *)
            usage
            ;;
    esac
    shift
done

print_banner "Orbis Integration Preflight" "dependency and local-state checks"

for bin in cometbft curl jq; do
    if ! command -v "$bin" >/dev/null 2>&1; then
        log_error "$bin not found in PATH"
        exit 1
    fi
done
docker_compose_flavor >/dev/null
ensure_docker_daemon
log_success "Runtime dependencies found"

if [ "$require_binaries" -eq 1 ]; then
    for bin in \
        "$COMPLIANCE_REPO_ROOT/target/release/pcli" \
        "$COMPLIANCE_REPO_ROOT/target/release/pclientd" \
        "$COMPLIANCE_REPO_ROOT/target/release/pd" \
        "$COMPLIANCE_REPO_ROOT/target/release/orbis-audit" \
        "$COMPLIANCE_REPO_ROOT/target/release/orbis-integration"
    do
        if [ ! -x "$bin" ]; then
            log_error "required release binary not found: $bin"
            log_error "Run `just orbis-integration-build` first"
            exit 1
        fi
    done
    log_success "Release binaries found"
fi

if [ "$check_ports_free" -eq 1 ]; then
    ensure_ports_available \
        8080 \
        16656 \
        16657 \
        18081 \
        18082 \
        18083 \
        18084 \
        26657 \
        50051 \
        50052 \
        50053
    log_success "Required TCP ports are available"
fi

if [ "${PENUMBRA_ORBIS_USE_DEMO_GNARK:-0}" = "1" ]; then
    log_info "Demo gnark override mode enabled via PENUMBRA_ORBIS_USE_DEMO_GNARK=1"
else
    log_info "Using bundled gnark runtimes from the release binaries"
fi
