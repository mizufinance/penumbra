#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "snarkpack lean conformance failed: $*" >&2
  exit 1
}

resolve_lean() {
  if [[ -n "${LEAN_BIN:-}" ]]; then
    [[ -x "$LEAN_BIN" ]] || fail "LEAN_BIN is set but not executable: $LEAN_BIN"
    printf '%s\n' "$LEAN_BIN"
    return
  fi
  if command -v lean >/dev/null 2>&1; then
    command -v lean
    return
  fi
  if [[ -x "$HOME/.elan/bin/lean" ]]; then
    printf '%s\n' "$HOME/.elan/bin/lean"
    return
  fi
  fail "Lean is not installed; install elan/Lean 4 or set LEAN_BIN"
}

LEAN="$(resolve_lean)"
expected_version="${SNARKPACK_LEAN_VERSION:-4.11.0}"
actual_version="$("$LEAN" --version)"
if [[ "$actual_version" != *"version $expected_version"* ]]; then
  fail "expected Lean $expected_version, got: $actual_version"
fi

# Absolute path: the writer (cargo run) and reader (cargo test) resolve relative
# paths against different working directories, so pin it to an absolute location.
oracle_json="${SNARKPACK_LEAN_ORACLE_SHAPES:-$ROOT/target/snarkpack-lean-conformance/oracle-shapes.json}"
mkdir -p "$(dirname "$oracle_json")"

LEAN_BIN="$LEAN" cargo run -p penumbra-sdk-proof-aggregation-lean-conformance \
  --bin snarkpack_lean_oracle -- "$oracle_json"

# Smoke band: deterministic exhaustive enumeration over round depths 0..=7 and
# padding representatives, for all families. The transcript-shape domain is finite
# and small, so it is enumerated, not fuzzed. The full sweep to 2^15 is the
# release-gated #[ignore]d test `lean_oracle_matches_all_shapes_to_max`.
LEAN_BIN="$LEAN" SNARKPACK_LEAN_ORACLE_SHAPES="$oracle_json" \
  cargo test -p penumbra-sdk-proof-aggregation-lean-conformance --lib

echo "snarkpack lean conformance ok"
