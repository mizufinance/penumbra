#!/bin/bash
# Stop the Penumbra compliance demo stack started by penumbra-up.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

PID_FILE="$COMPLIANCE_TMP/penumbra-pids.txt"

print_banner "Penumbra Infra Teardown"
kill_tracked_pids "$PID_FILE"
log_success "Penumbra infra stopped"
