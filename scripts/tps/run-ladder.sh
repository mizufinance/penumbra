#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  cat <<'USAGE'
Deprecated: use scripts/tps/bench-simple.sh.

Ladder/corpus-growth behavior is now:
  ./scripts/tps/bench-simple.sh append --count <n>
  ./scripts/tps/bench-simple.sh run

This wrapper now forwards to:
  ./scripts/tps/bench-simple.sh run
USAGE
  exit 0
fi

echo "[deprecated] scripts/tps/run-ladder.sh -> scripts/tps/bench-simple.sh run" >&2
exec "$SCRIPT_DIR/bench-simple.sh" run "$@"
