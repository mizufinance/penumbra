#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

require_binaries=0

usage() {
    cat >&2 <<'EOF'
usage: ./scripts/orbis-integration-preflight.sh [--require-binaries]
EOF
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --require-binaries)
            require_binaries=1
            ;;
        *)
            usage
            ;;
    esac
    shift
done

print_banner "Orbis Integration Preflight" "dependency and local-state checks"

for bin in curl jq; do
    if ! command -v "$bin" >/dev/null 2>&1; then
        log_error "$bin not found in PATH"
        exit 1
    fi
done
require_docker_compose
ensure_docker_daemon
log_success "Runtime dependencies found"

if [ "$require_binaries" -eq 1 ]; then
    for bin in \
        "$COMPLIANCE_REPO_ROOT/target/release/orbis-integration"
    do
        if [ ! -x "$bin" ]; then
            log_error "required release binary not found: $bin"
            log_error 'Run just orbis-integration-build first'
            exit 1
        fi
    done
    log_success "Release binaries found"
fi
