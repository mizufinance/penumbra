#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
REMOTE_DIR="${POC_REMOTE_DIR:-~/penumbra-poc}"
REMOTE_HOST=""

usage() {
  cat <<'EOF'
Usage:
  run-stage-bench-remote.sh --host <user@host> [--remote-dir <dir>] -- <stage> [args...]

Examples:
  run-stage-bench-remote.sh --host acyrntoine@136.119.222.139 -- mempool --corpus ...
  run-stage-bench-remote.sh --host acyrntoine@136.119.222.139 -- builder single --corpus ...

EOF
}

sync_repo() {
  rsync -az --delete \
    --exclude '.git/' \
    --exclude 'target/' \
    --exclude 'poc/target/' \
    --exclude 'tmp/' \
    "$REPO_ROOT/" "$REMOTE_HOST:$REMOTE_DIR/"
}

remote_bin_for_stage() {
  case "$1" in
    builder) printf 'builder_lab' ;;
    mempool) printf 'mempool_lab' ;;
    validation) printf 'validation_lab' ;;
    execution) printf 'execution_lab' ;;
    proof) printf 'proof_lab' ;;
    *) return 1 ;;
  esac
}

main() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --host)
        REMOTE_HOST="$2"
        shift 2
        ;;
      --remote-dir)
        REMOTE_DIR="$2"
        shift 2
        ;;
      --)
        shift
        break
        ;;
      -h|--help|help)
        usage
        exit 0
        ;;
      *)
        printf 'unknown option: %s\n' "$1" >&2
        usage
        exit 1
        ;;
    esac
  done

  [[ -n "$REMOTE_HOST" ]] || {
    printf '--host is required\n' >&2
    exit 1
  }
  [[ $# -ge 1 ]] || {
    printf 'missing stage command\n' >&2
    usage
    exit 1
  }

  local stage="$1"
  shift
  local bin
  bin="$(remote_bin_for_stage "$stage")" || {
    printf 'unsupported stage: %s\n' "$stage" >&2
    exit 1
  }
  local remote_args=""
  if [[ $# -gt 0 ]]; then
    remote_args="$(printf ' %q' "$@")"
  fi

  sync_repo

  ssh "$REMOTE_HOST" "cd $REMOTE_DIR && cargo build --release --manifest-path poc/Cargo.toml -p penumbra-sdk-bench --bin $bin && ./poc/target/release/$bin$remote_args"
}

main "$@"
