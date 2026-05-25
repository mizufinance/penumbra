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
        ensure_docker_daemon
        ensure_orbis_runtime_checkout
        if [ -n "${CI:-}" ]; then
            # On CI, preflight runs ~minutes before bring-up (build happens in between),
            # so a port that was free then may now be held by a leftover container or
            # squatting host process. Forcibly clear the ports we're about to publish.
            for port in \
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
            do
                leftover_ids=$(docker ps -q --filter "publish=$port" 2>/dev/null || true)
                if [ -n "$leftover_ids" ]; then
                    log_warning "Removing container(s) holding port $port: $leftover_ids"
                    echo "$leftover_ids" | xargs -r docker rm -f >/dev/null || true
                fi
                if command -v fuser >/dev/null 2>&1; then
                    fuser -k "${port}/tcp" >/dev/null 2>&1 || true
                fi
            done
        fi
        run_orbis_compose "$COMPOSE_FILE" up -d --build
        wait_for_orbis_stack
        log_success "Orbis stack ready"
        ;;
    down)
        print_banner "Orbis Runtime Teardown"
        if ! docker_daemon_ready; then
            log_warning "Docker daemon is not running; skipping Orbis compose teardown"
            exit 0
        fi
        ensure_orbis_runtime_checkout
        run_orbis_compose "$COMPOSE_FILE" down -v
        log_success "Orbis stack stopped"
        ;;
    logs)
        ensure_docker_daemon
        ensure_orbis_runtime_checkout
        run_orbis_compose "$COMPOSE_FILE" logs
        ;;
    ps)
        ensure_docker_daemon
        ensure_orbis_runtime_checkout
        run_orbis_compose "$COMPOSE_FILE" ps
        ;;
    *)
        echo "usage: $0 {up|down|logs|ps}" >&2
        exit 1
        ;;
esac
