#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$ROOT_DIR/../.." && pwd)"

ARTIFACT_DIR="$REPO_DIR/tmp/gnark-spend-prototype/spend"
VERIFY_OUT_DIR="$REPO_DIR/tmp/gnark-spend-prototype/verify-bench"
WITNESS_BIN="$ROOT_DIR/vectors/spend_witness_v1.bin"
FIXTURE_JSON="$ROOT_DIR/vectors/spend_fixture.json"
PROOF_JSON="$VERIFY_OUT_DIR/spendprove_artifacts.json"
GNARK_JSON="$VERIFY_OUT_DIR/gnark_verify.json"
ARKWORKS_JSON="$VERIFY_OUT_DIR/arkworks_verify.json"
COMBINED_JSON="$VERIFY_OUT_DIR/report.json"
WARMUP_ITERATIONS=3
MEASURED_ITERATIONS=20

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --artifact-dir PATH         spend setup artifact directory
  --out-dir PATH              verifier benchmark output directory
  --witness PATH              SpendWitnessV1 binary path
  --fixture PATH              spend fixture JSON path
  --warmup-iterations N       verify warmup iterations (default: 3)
  --measured-iterations N     verify measured iterations (default: 20)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-dir)
      ARTIFACT_DIR="$2"
      shift 2
      ;;
    --out-dir)
      VERIFY_OUT_DIR="$2"
      PROOF_JSON="$VERIFY_OUT_DIR/spendprove_artifacts.json"
      GNARK_JSON="$VERIFY_OUT_DIR/gnark_verify.json"
      ARKWORKS_JSON="$VERIFY_OUT_DIR/arkworks_verify.json"
      COMBINED_JSON="$VERIFY_OUT_DIR/report.json"
      shift 2
      ;;
    --witness)
      WITNESS_BIN="$2"
      shift 2
      ;;
    --fixture)
      FIXTURE_JSON="$2"
      shift 2
      ;;
    --warmup-iterations)
      WARMUP_ITERATIONS="$2"
      shift 2
      ;;
    --measured-iterations)
      MEASURED_ITERATIONS="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "$VERIFY_OUT_DIR" "$ARTIFACT_DIR" "$ROOT_DIR/.gocache" "$ROOT_DIR/.tmp"

export GOCACHE="${GOCACHE:-$ROOT_DIR/.gocache}"
export GOTMPDIR="${GOTMPDIR:-$ROOT_DIR/.tmp}"

if [[ ! -f "$ARTIFACT_DIR/proving_key.bin" || ! -f "$ARTIFACT_DIR/verifying_key.bin" || ! -f "$ARTIFACT_DIR/circuit_metadata.json" ]]; then
  echo "generating spend setup artifacts into $ARTIFACT_DIR" >&2
  (
    cd "$ROOT_DIR"
    go run ./cmd/spendsetup --out-dir "$ARTIFACT_DIR"
  )
fi

echo "generating gnark spend proof artifacts into $PROOF_JSON" >&2
(
  cd "$ROOT_DIR"
  go run ./cmd/spendprove \
    --witness "$WITNESS_BIN" \
    --artifact-dir "$ARTIFACT_DIR" \
    --out "$PROOF_JSON"
)

echo "benchmarking gnark native verification" >&2
(
  cd "$ROOT_DIR"
  go run ./cmd/gnarkverifybench \
    --artifacts "$PROOF_JSON" \
    --out "$GNARK_JSON" \
    --warmup-iterations "$WARMUP_ITERATIONS" \
    --measured-iterations "$MEASURED_ITERATIONS"
)

echo "benchmarking Arkworks verification" >&2
(
  cd "$REPO_DIR"
  cargo build --release -p penumbra-sdk-bench --bin gnark_spend_proto >/dev/null
  target/release/gnark_spend_proto verify-bench \
    --fixture "$FIXTURE_JSON" \
    --artifacts "$PROOF_JSON" \
    --out "$ARKWORKS_JSON" \
    --warmup-iterations "$WARMUP_ITERATIONS" \
    --measured-iterations "$MEASURED_ITERATIONS"
)

echo "writing combined verifier report to $COMBINED_JSON" >&2
python3 - "$PROOF_JSON" "$WITNESS_BIN" "$GNARK_JSON" "$ARKWORKS_JSON" "$COMBINED_JSON" <<'PY'
import json
import pathlib
import sys

proof_path = pathlib.Path(sys.argv[1])
witness_path = pathlib.Path(sys.argv[2])
gnark_path = pathlib.Path(sys.argv[3])
arkworks_path = pathlib.Path(sys.argv[4])
combined_path = pathlib.Path(sys.argv[5])

with proof_path.open() as f:
    artifacts = json.load(f)
with gnark_path.open() as f:
    gnark = json.load(f)
with arkworks_path.open() as f:
    arkworks = json.load(f)

combined = {
    "curve": artifacts["curve"],
    "circuit": artifacts["circuit"],
    "artifact": {
        "claimed_statement_hash": artifacts["claimed_statement_hash"],
        "artifact_json": str(proof_path),
        "witness_bin": str(witness_path),
    },
    "iterations": {
        "warmup": gnark["verify_warmup_iterations"],
        "measured": gnark["verify_measured_iterations"],
    },
    "gnark": gnark,
    "arkworks": arkworks,
    "equivalence_checks": {
        "same_claimed_statement_hash": (
            artifacts["claimed_statement_hash"] == gnark["claimed_statement_hash"] == arkworks["claimed_statement_hash"]
        ),
        "same_curve": gnark["curve"] == arkworks["curve"] == artifacts["curve"],
        "same_circuit": gnark["circuit"] == arkworks["circuit"] == artifacts["circuit"],
        "both_verifiers_succeeded": True,
    },
}

with combined_path.open("w") as f:
    json.dump(combined, f, indent=2)
    f.write("\n")
PY

cat "$COMBINED_JSON"
