#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

COMPOSE_FILE="$COMPLIANCE_REPO_ROOT/deployments/orbis/docker-compose.yml"
ACTION="${1:-}"

if [ ! -f "$COMPOSE_FILE" ]; then
    log_error "Orbis compose file not found at $COMPOSE_FILE"
    exit 1
fi

case "$ACTION" in
    up)
        print_banner "Orbis Runtime Bring-Up" "sourcehub + 3 nodes via vendored runtime contract"
        run_orbis_compose "$COMPOSE_FILE" up -d --build
        wait_for_orbis_stack
        log_success "Orbis stack ready"
        ;;
    down)
        print_banner "Orbis Runtime Teardown"
        run_orbis_compose "$COMPOSE_FILE" down -v
        log_success "Orbis stack stopped"
        ;;
    logs)
        run_orbis_compose "$COMPOSE_FILE" logs
        ;;
    ps)
        run_orbis_compose "$COMPOSE_FILE" ps
        ;;
    *)
        echo "usage: $0 {up|down|logs|ps}" >&2
        exit 1
        ;;
esac
