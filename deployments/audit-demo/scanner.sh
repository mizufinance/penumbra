#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

LOG_FILE="$DEMO_DIR_ABS/scanner.log"
touch "$LOG_FILE"
exec >> "$LOG_FILE" 2>&1

write_health() {
  local running="$1"
  local message="$2"
  local last_height="${3:-}"
  jq -n \
    --argjson running "$running" \
    --arg message "$message" \
    --arg lastHeight "$last_height" \
    '{
      running: $running,
      message: $message,
      lastHeight: (if $lastHeight == "" then null else ($lastHeight | tonumber) end),
      updatedAt: now | todate
    }' > "$SCANNER_HEALTH_FILE"
}

hash_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum
  else
    shasum -a 256
  fi
}

prepare_audit_cache() {
  local detected="$DEMO_DIR_ABS/detected-txs.json"
  local marker="$DEMO_DIR_ABS/audit-cache-prepared.sha256"
  local checksum count
  if [ ! -f "$detected" ] || [ ! -f "$STATE_FILE" ]; then
    return
  fi
  count="$(jq '[.detected[]? | select(.is_flagged != true)] | length' "$detected" 2>/dev/null || echo 0)"
  if [ "$count" = "0" ]; then
    return
  fi
  checksum="$(
    {
      printf '%s\n' 'prepare-v3' | hash_stdin
      jq -c '[.detected[]? | select(.is_flagged != true) | {height, tx_hash, action_index}] | sort_by(.height, .tx_hash, .action_index)' "$detected" | hash_stdin
      jq -r '.users[]?.addresses[]?.address' "$STATE_FILE" | sort | hash_stdin
    } | hash_stdin | awk '{print $1}'
  )"
  if [ -f "$marker" ] && [ "$(cat "$marker")" = "$checksum" ]; then
    return
  fi

  load_dk
  local args=(
    --prepare-only
    --input "$DEMO_DIR/detected-txs.json"
    --dk-hex "$DK_HEX"
    --node "$PENUMBRA_GRPC"
    --output "$DEMO_DIR/audit-cache-prepare.json"
    --timings-json "$DEMO_DIR/audit-cache-prepare-timings.json"
    --object-cache "$DEMO_DIR/orbis-object-cache.json"
    --tier default
    --orbis-endpoint "$ORBIS_ENDPOINT"
  )
  local address
  while IFS= read -r address; do
    [ -n "$address" ] && args+=(--subject-address "$address")
  done < <(jq -r '.users[]?.addresses[]?.address' "$STATE_FILE")
  while IFS= read -r address; do
    [ -n "$address" ] && args+=(--known-address "$address")
  done < <(jq -r '.users[]?.addresses[]?.address' "$STATE_FILE")

  if run_orbis_audit_locked "${args[@]}"; then
    echo "$checksum" > "$marker"
  fi
}

init_state_file
if ! jq -e '.setup.initialized == true' "$STATE_FILE" >/dev/null; then
  write_health false "Audit setup is not ready"
  exit 1
fi
load_dk

write_health true "Scanner running"

refresh_loop() {
  while true; do
    refresh_outputs || true
    prepare_audit_cache || true
    last_height="$(jq -r '.last_height // empty' "$DEMO_DIR_ABS/scanner-state.json" 2>/dev/null || true)"
    write_health true "Scanner running" "$last_height"
    sleep 2
  done
}

refresh_loop &
refresh_pid="$!"

cleanup() {
  kill "$refresh_pid" 2>/dev/null || true
  write_health false "Scanner stopped"
}
trap cleanup EXIT HUP INT TERM

pcli_home "$(wallet_home alice)" tx compliance scan \
  --node "$PENUMBRA_GRPC" \
  --dk-hex "$DK_HEX" \
  --scan-asset-id "$ASSET" \
  --output "$DEMO_DIR/detected-txs.json" \
  --state-file "$DEMO_DIR/scanner-state.json" \
  --issuer-db "$ISSUER_DB" \
  --merge-output \
  --follow
