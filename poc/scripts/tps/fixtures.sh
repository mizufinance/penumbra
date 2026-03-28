#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
POC_MANIFEST="$REPO_ROOT/poc/Cargo.toml"
COMPLIANCE_PKG="penumbra-sdk-compliance-bench"
COMPLIANCE_BIN="compliance_tps"
TPS_ROOT_DEFAULT="$REPO_ROOT/poc/crates/stage-bench/benches/compliance/tps"
SNAPSHOT_ROOT="${POC_TPS_SNAPSHOT_ROOT:-$TPS_ROOT_DEFAULT/snapshots}"
CORPUS_ROOT="${POC_TPS_CORPUS_ROOT:-$TPS_ROOT_DEFAULT/corpus}"

usage() {
  cat <<'EOF'
Usage:
  fixtures.sh list
  fixtures.sh seed [build-local args...]
  fixtures.sh ready [build-local args...]
  fixtures.sh fund [build-local args...]
  fixtures.sh regulated [build-local args...]
  fixtures.sh build-corpus [compliance_tps corpus args...]
  fixtures.sh promote-corpus <snapshot> <scenario>
  fixtures.sh save <name> <source_dir>
  fixtures.sh restore <name> <dest_dir>

Notes:
  - seed/ready/fund/regulated are thin presets over:
      compliance_tps corpus build-local
  - promote-corpus copies snapshots/<name>/corpus/<scenario> into the active
      top-level corpus directory at poc/crates/stage-bench/benches/compliance/tps/corpus
  - save/restore copy fixture directories under:
      $POC_TPS_SNAPSHOT_ROOT or poc/crates/stage-bench/benches/compliance/tps/snapshots
EOF
}

run_compliance() {
  cargo run --manifest-path "$POC_MANIFEST" -p "$COMPLIANCE_PKG" --bin "$COMPLIANCE_BIN" -- "$@"
}

build_local_with_scenario() {
  local scenario="$1"
  shift
  run_compliance corpus build-local --scenario "$scenario" "$@"
}

list_fixtures() {
  printf 'Snapshots:\n'
  if [[ -d "$SNAPSHOT_ROOT" ]]; then
    find "$SNAPSHOT_ROOT" -maxdepth 1 -mindepth 1 -type d | sort | while read -r snapshot_dir; do
      local name
      name="${snapshot_dir##*/}"
      printf '  - %s\n' "$name"
      if [[ -d "$snapshot_dir/corpus" ]]; then
        find "$snapshot_dir/corpus" -maxdepth 1 -mindepth 1 -type d | sort | while read -r corpus_dir; do
          printf '      corpus: %s\n' "${corpus_dir##*/}"
        done
      fi
    done
  else
    printf '  (none)\n'
  fi

  printf '\nActive top-level corpus:\n'
  if [[ -d "$CORPUS_ROOT" ]]; then
    find "$CORPUS_ROOT" -maxdepth 1 -mindepth 1 -type d | sort | while read -r corpus_dir; do
      printf '  - %s\n' "${corpus_dir##*/}"
    done
  else
    printf '  (none)\n'
  fi
}

promote_corpus() {
  local snapshot_name="$1"
  local scenario="$2"
  local snapshot_corpus_dir="$SNAPSHOT_ROOT/$snapshot_name/corpus/$scenario"
  local target_dir="$CORPUS_ROOT/$scenario"

  [[ -d "$snapshot_corpus_dir" ]] || {
    printf 'missing embedded corpus: %s\n' "$snapshot_corpus_dir" >&2
    exit 1
  }

  mkdir -p "$CORPUS_ROOT"
  rm -rf "$target_dir"
  cp -R "$snapshot_corpus_dir" "$target_dir"
  printf 'promoted corpus: %s -> %s\n' "$snapshot_corpus_dir" "$target_dir"
}

save_fixture() {
  local name="$1"
  local source_dir="$2"
  local snapshot_dir="$SNAPSHOT_ROOT/$name"
  mkdir -p "$SNAPSHOT_ROOT"
  rm -rf "$snapshot_dir"
  cp -R "$source_dir" "$snapshot_dir"
  printf 'saved fixture: %s -> %s\n' "$source_dir" "$snapshot_dir"
}

restore_fixture() {
  local name="$1"
  local dest_dir="$2"
  local snapshot_dir="$SNAPSHOT_ROOT/$name"
  [[ -d "$snapshot_dir" ]] || {
    printf 'missing snapshot: %s\n' "$snapshot_dir" >&2
    exit 1
  }
  rm -rf "$dest_dir"
  mkdir -p "$(dirname -- "$dest_dir")"
  cp -R "$snapshot_dir" "$dest_dir"
  printf 'restored fixture: %s -> %s\n' "$snapshot_dir" "$dest_dir"
}

main() {
  [[ $# -ge 1 ]] || {
    usage
    exit 1
  }

  local command="$1"
  shift

  case "$command" in
    list)
      list_fixtures
      ;;
    seed)
      build_local_with_scenario "unregulated-seed" "$@"
      ;;
    ready)
      build_local_with_scenario "unregulated-ready" "$@"
      ;;
    fund)
      build_local_with_scenario "funded" "$@"
      ;;
    regulated)
      build_local_with_scenario "regulated" "$@"
      ;;
    build-corpus)
      run_compliance corpus "$@"
      ;;
    promote-corpus)
      [[ $# -eq 2 ]] || {
        usage
        exit 1
      }
      promote_corpus "$1" "$2"
      ;;
    save)
      [[ $# -eq 2 ]] || {
        usage
        exit 1
      }
      save_fixture "$1" "$2"
      ;;
    restore)
      [[ $# -eq 2 ]] || {
        usage
        exit 1
      }
      restore_fixture "$1" "$2"
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      printf 'unknown fixtures subcommand: %s\n' "$command" >&2
      usage
      exit 1
      ;;
  esac
}

main "$@"
