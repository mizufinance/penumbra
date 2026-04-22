#!/bin/bash
# Stop the Penumbra compliance demo stack started by penumbra-up.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

PID_FILE="$COMPLIANCE_TMP/penumbra-pids.txt"

print_banner "Penumbra Infra Teardown"
kill_tracked_pids "$PID_FILE"
pkill pclientd 2>/dev/null || true
pkill pd 2>/dev/null || true
pkill cometbft 2>/dev/null || true
log_success "Penumbra infra stopped"
