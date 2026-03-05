#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Save a local TPS debug snapshot (chain state + wallets + corpus).

Required:
  --name <snapshot_name>

Optional:
  --stop-local         stop pd/cometbft before saving (default)
  --no-stop-local      do not stop local processes before saving

The snapshot is written to:
  crates/bench/benches/compliance/tps/snapshots/<snapshot_name>/
EOF
}

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

NAME=""
STOP_LOCAL=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name) NAME="$2"; shift 2 ;;
    --stop-local) STOP_LOCAL=1; shift ;;
    --no-stop-local) STOP_LOCAL=0; shift ;;
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
NETWORK_DATA_DIR="$HOME/.penumbra/network_data"
CORPUS_DIR="$REPO_ROOT/crates/bench/benches/compliance/tps/corpus"
ENV_FILE="$COMPLIANCE_TMP/compliance-demo.env"

if [[ ! -d "$NETWORK_DATA_DIR/node0" ]]; then
  echo "Missing network data at $NETWORK_DATA_DIR/node0" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "Missing environment file at $ENV_FILE" >&2
  echo "Run scripts/setup-penumbra.sh or scripts/tps/bench-simple.sh prepare first." >&2
  exit 1
fi
if [[ ! -d "$CORPUS_DIR/unregulated" || ! -d "$CORPUS_DIR/regulated" ]]; then
  echo "Missing corpus directories under $CORPUS_DIR" >&2
  echo "Generate corpora first (for example: scripts/tps/bench-simple.sh prepare)." >&2
  exit 1
fi

if [[ "$STOP_LOCAL" -eq 1 ]]; then
  echo "Stopping local pd/cometbft before snapshot..."
  pkill pd 2>/dev/null || true
  pkill cometbft 2>/dev/null || true
  sleep 1
  rm -f "$COMPLIANCE_TMP/tps-pd.pid" "$COMPLIANCE_TMP/tps-cometbft.pid"
fi

rm -rf "$SNAPSHOT_DIR"
mkdir -p "$SNAPSHOT_DIR"

echo "Saving network data..."
tar -C "$HOME/.penumbra" -czf "$SNAPSHOT_DIR/network_data.tgz" network_data

echo "Saving wallets and env..."
tar -C "$COMPLIANCE_TMP" -czf "$SNAPSHOT_DIR/wallets.tgz" \
  alice-wallet \
  bob-wallet \
  charlie-wallet \
  unregistered-wallet \
  compliance-demo.env

echo "Saving corpus..."
mkdir -p "$SNAPSHOT_DIR/corpus"
cp -R "$CORPUS_DIR/unregulated" "$SNAPSHOT_DIR/corpus/unregulated"
cp -R "$CORPUS_DIR/regulated" "$SNAPSHOT_DIR/corpus/regulated"

ts="$(date +%s)"
escaped_name="$(json_escape "$NAME")"
cat > "$SNAPSHOT_DIR/metadata.json" <<EOF
{
  "name": "$escaped_name",
  "timestamp": $ts,
  "network_data": "network_data.tgz",
  "wallets": "wallets.tgz",
  "corpus_unregulated": "corpus/unregulated",
  "corpus_regulated": "corpus/regulated"
}
EOF

echo "Snapshot saved: $SNAPSHOT_DIR"
