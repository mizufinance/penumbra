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
        "$PENUMBRA_PD_GRPC_PORT" \
        "$PENUMBRA_COMETBFT_P2P_PORT" \
        "$PENUMBRA_COMETBFT_RPC_PORT" \
        "$PENUMBRA_PCLIENTD_PORT_BASE" \
        "$((PENUMBRA_PCLIENTD_PORT_BASE + 1))" \
        "$((PENUMBRA_PCLIENTD_PORT_BASE + 2))" \
        "$((PENUMBRA_PCLIENTD_PORT_BASE + 3))" \
        "${ORBIS_SOURCEHUB_RPC_PORT:-26657}" \
        "${ORBIS_SOURCEHUB_P2P_PORT:-26656}" \
        "${ORBIS_SOURCEHUB_REST_PORT:-1317}" \
        "${ORBIS_SOURCEHUB_GRPC_PORT:-9090}" \
        "${ORBIS_NODE1_GRPC_PORT:-50051}" \
        "${ORBIS_NODE2_GRPC_PORT:-50052}" \
        "${ORBIS_NODE3_GRPC_PORT:-50053}" \
        "${ORBIS_NODE1_METRICS_PORT:-9091}" \
        "${ORBIS_NODE2_METRICS_PORT:-9092}" \
        "${ORBIS_NODE3_METRICS_PORT:-9093}"
    log_success "Required TCP ports are available"
fi

if [ "${PENUMBRA_ORBIS_USE_DEMO_GNARK:-0}" = "1" ]; then
    log_info "Demo gnark override mode enabled via PENUMBRA_ORBIS_USE_DEMO_GNARK=1"
else
    log_info "Using bundled gnark runtimes from the release binaries"
fi
