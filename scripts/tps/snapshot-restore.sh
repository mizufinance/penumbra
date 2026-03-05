#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Restore a local TPS debug snapshot (chain state + wallets + corpus), then start pd/cometbft.

Required:
  --name <snapshot_name>

Optional:
  --start-local         start pd/cometbft after restore (default)
  --no-start-local      only restore files, do not start services
EOF
}

NAME=""
START_LOCAL=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name)
      if [[ $# -lt 2 || -z "${2:-}" || "${2:-}" == -* ]]; then
        echo "--name requires a value" >&2
        usage
        exit 1
      fi
      NAME="$2"
      shift 2
      ;;
    --start-local) START_LOCAL=1; shift ;;
    --no-start-local) START_LOCAL=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$NAME" ]]; then
  echo "--name is required" >&2
  usage
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/../lib/common.sh"

SNAPSHOT_DIR="$REPO_ROOT/crates/bench/benches/compliance/tps/snapshots/$NAME"
NETWORK_TGZ="$SNAPSHOT_DIR/network_data.tgz"
WALLETS_TGZ="$SNAPSHOT_DIR/wallets.tgz"
SNAP_CORPUS_DIR="$SNAPSHOT_DIR/corpus"
TARGET_CORPUS_DIR="$REPO_ROOT/crates/bench/benches/compliance/tps/corpus"
PD_BIN="${PD_BIN:-$REPO_ROOT/target/release/pd}"

if [[ ! -f "$NETWORK_TGZ" || ! -f "$WALLETS_TGZ" ]]; then
  echo "Snapshot files missing in $SNAPSHOT_DIR" >&2
  exit 1
fi
if [[ ! -d "$SNAP_CORPUS_DIR/unregulated" || ! -d "$SNAP_CORPUS_DIR/regulated" ]]; then
  echo "Snapshot corpus missing in $SNAPSHOT_DIR/corpus" >&2
  exit 1
fi
if [[ ! -x "$PD_BIN" ]]; then
  echo "Missing pd binary at $PD_BIN" >&2
  echo "Build with: cargo build --release -p pd" >&2
  exit 1
fi
if ! command -v cometbft >/dev/null 2>&1; then
  echo "Missing cometbft binary in PATH" >&2
  exit 1
fi

stop_from_pid_file() {
  local pid_file="$1"
  local pid
  if [[ ! -f "$pid_file" ]]; then
    return 1
  fi
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  rm -f "$pid_file"
  if [[ ! "$pid" =~ ^[0-9]+$ ]]; then
    return 1
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    return 1
  fi
  kill "$pid" 2>/dev/null || true
  return 0
}

echo "Stopping local pd/cometbft..."
stop_from_pid_file "$COMPLIANCE_TMP/tps-pd.pid" || pkill -x pd 2>/dev/null || true
stop_from_pid_file "$COMPLIANCE_TMP/tps-cometbft.pid" || pkill -x cometbft 2>/dev/null || true
sleep 1
rm -f "$COMPLIANCE_TMP/tps-pd.pid" "$COMPLIANCE_TMP/tps-cometbft.pid"

echo "Restoring network data..."
rm -rf "$HOME/.penumbra/network_data"
mkdir -p "$HOME/.penumbra"
tar -C "$HOME/.penumbra" -xzf "$NETWORK_TGZ"

echo "Restoring wallets + env..."
rm -rf "$COMPLIANCE_TMP/alice-wallet" \
       "$COMPLIANCE_TMP/bob-wallet" \
       "$COMPLIANCE_TMP/charlie-wallet" \
       "$COMPLIANCE_TMP/unregistered-wallet"
rm -f "$COMPLIANCE_TMP/compliance-demo.env"
tar -C "$COMPLIANCE_TMP" -xzf "$WALLETS_TGZ"

echo "Restoring corpus..."
rm -rf "$TARGET_CORPUS_DIR"
mkdir -p "$TARGET_CORPUS_DIR"
cp -R "$SNAP_CORPUS_DIR/unregulated" "$TARGET_CORPUS_DIR/unregulated"
cp -R "$SNAP_CORPUS_DIR/regulated" "$TARGET_CORPUS_DIR/regulated"

if [[ "$START_LOCAL" -eq 1 ]]; then
  echo "Starting cometbft..."
  nohup cometbft start --home "$HOME/.penumbra/network_data/node0/cometbft" \
    > "$COMPLIANCE_TMP/cometbft.log" 2>&1 &
  COMET_PID=$!
  echo "$COMET_PID" > "$COMPLIANCE_TMP/tps-cometbft.pid"

  sleep 1

  echo "Starting pd..."
  nohup "$PD_BIN" start --home "$HOME/.penumbra/network_data/node0/pd" \
    --cometbft-addr http://127.0.0.1:16657 \
    > "$COMPLIANCE_TMP/pd.log" 2>&1 &
  PD_PID=$!
  echo "$PD_PID" > "$COMPLIANCE_TMP/tps-pd.pid"

  echo "Waiting for services..."
  wait_for_grpc 16657 45 1
  wait_for_grpc 8080 45 1
  wait_for_penumbra 16657 60 1

  # Ensure both processes survived startup.
  if ! kill -0 "$PD_PID" 2>/dev/null; then
    echo "pd process exited during startup; tailing log:" >&2
    tail -n 120 "$COMPLIANCE_TMP/pd.log" >&2 || true
    exit 1
  fi
  if ! kill -0 "$COMET_PID" 2>/dev/null; then
    echo "cometbft process exited during startup; tailing log:" >&2
    tail -n 120 "$COMPLIANCE_TMP/cometbft.log" >&2 || true
    exit 1
  fi

  # Stronger readiness: require live status endpoint.
  if ! curl -sf http://127.0.0.1:16657/status >/dev/null 2>&1; then
    echo "cometbft status endpoint is not healthy after startup" >&2
    tail -n 120 "$COMPLIANCE_TMP/cometbft.log" >&2 || true
    exit 1
  fi

  # Optional health probe: if pcli + env are present, validate a live pd query.
  PCLI_BIN_DEFAULT="$REPO_ROOT/target/release/pcli"
  PCLI_BIN="${PCLI_BIN:-$PCLI_BIN_DEFAULT}"
  if [[ -x "$PCLI_BIN" && -f "$COMPLIANCE_TMP/compliance-demo.env" ]]; then
    # shellcheck disable=SC1091
    source "$COMPLIANCE_TMP/compliance-demo.env"
    if [[ -n "${ALICE_HOME:-}" ]]; then
      ok=0
      for _ in $(seq 1 30); do
        if "$PCLI_BIN" --home "$ALICE_HOME" --grpc-url http://127.0.0.1:8080 view balance >/dev/null 2>&1; then
          ok=1
          break
        fi
        sleep 1
      done
      if [[ "$ok" -ne 1 ]]; then
        echo "pd health probe failed: pcli view balance did not succeed" >&2
        tail -n 120 "$COMPLIANCE_TMP/pd.log" >&2 || true
        exit 1
      fi
    fi
  fi
fi

echo "Snapshot restored: $SNAPSHOT_DIR"
