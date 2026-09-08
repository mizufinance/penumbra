#!/bin/bash
# Remove only stale CI resources with validated Orbis integration ownership.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

COMPOSE_FILE="$COMPLIANCE_REPO_ROOT/deployments/orbis/docker-compose.yml"
PROJECT_PATTERN='^orbis-[0-9]+-[0-9]+$'
TMP_PATTERN='^/tmp/orbis-[0-9]+-[0-9]+$'

ensure_docker_daemon
ensure_orbis_images

projects="$(
    {
        docker ps -a \
            --filter label=com.docker.compose.project \
            --format '{{.Label "com.docker.compose.project"}}'
        docker network ls \
            --filter label=com.docker.compose.project \
            --format '{{.Label "com.docker.compose.project"}}'
        docker volume ls \
            --filter label=com.docker.compose.project \
            --format '{{.Label "com.docker.compose.project"}}'
    } | sort -u
)"

while IFS= read -r project; do
    [ -n "$project" ] || continue
    if [[ "$project" =~ $PROJECT_PATTERN ]]; then
        log_info "Removing owned stale Compose project: $project"
        ORBIS_COMPOSE_PROJECT_NAME="$project" \
            run_orbis_compose "$COMPOSE_FILE" down -v --remove-orphans
    fi
done <<< "$projects"

log_success "Owned stale Orbis integration resources cleaned"
