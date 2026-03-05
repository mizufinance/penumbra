#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  cat <<'USAGE'
Deprecated: use scripts/tps/bench-simple.sh.

Recommended commands:
  ./scripts/tps/bench-simple.sh prepare
  ./scripts/tps/bench-simple.sh run
  ./scripts/tps/bench-simple.sh append --count 4
  ./scripts/tps/bench-simple.sh refresh
  ./scripts/tps/bench-simple.sh status

This wrapper now forwards to:
  ./scripts/tps/bench-simple.sh run
USAGE
  exit 0
fi

echo "[deprecated] scripts/tps/run-tps.sh -> scripts/tps/bench-simple.sh run" >&2
exec "$SCRIPT_DIR/bench-simple.sh" run "$@"
