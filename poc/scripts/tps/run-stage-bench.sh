#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
POC_MANIFEST="$REPO_ROOT/poc/Cargo.toml"
STAGE_PKG="penumbra-sdk-bench"

usage() {
  cat <<'EOF'
Usage:
  run-stage-bench.sh builder <subcommand> [args...]
  run-stage-bench.sh mempool [args...]
  run-stage-bench.sh validation [args...]
  run-stage-bench.sh execution [args...]
  run-stage-bench.sh proof <subcommand> [args...]
EOF
}

run_stage() {
  local bin="$1"
  shift
  cargo run --release --manifest-path "$POC_MANIFEST" -p "$STAGE_PKG" --bin "$bin" -- "$@"
}

main() {
  [[ $# -ge 1 ]] || {
    usage
    exit 1
  }

  local stage="$1"
  shift

  case "$stage" in
    builder)
      [[ $# -ge 1 ]] || {
        printf 'builder requires a subcommand\n' >&2
        exit 1
      }
      run_stage builder_lab "$@"
      ;;
    mempool)
      run_stage mempool_lab "$@"
      ;;
    validation)
      run_stage validation_lab "$@"
      ;;
    execution)
      run_stage execution_lab "$@"
      ;;
    proof)
      [[ $# -ge 1 ]] || {
        printf 'proof requires a subcommand\n' >&2
        exit 1
      }
      run_stage proof_lab "$@"
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      printf 'unknown stage bench: %s\n' "$stage" >&2
      usage
      exit 1
      ;;
  esac
}

main "$@"
