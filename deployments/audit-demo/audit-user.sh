#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

LOG_FILE="$DEMO_DIR_ABS/audit-user.log"
touch "$LOG_FILE"
exec >> "$LOG_FILE" 2>&1

input_name="${1:?name is required}"

init_state_file
if ! jq -e '.setup.initialized == true' "$STATE_FILE" >/dev/null; then
  write_status failed audit-user "Audit setup is not ready"
  exit 1
fi

load_dk
slug="$(known_user_or_fail "$input_name")"
name="$(user_name_from_slug "$slug")"

write_status running audit-user "Auditing $name"
refresh_outputs

if [ ! -f "$DEMO_DIR_ABS/detected-txs.json" ]; then
  echo '{"detected":[]}' > "$DEMO_DIR_ABS/detected-txs.json"
fi

non_flagged_count="$(jq '[.detected[]? | select(.is_flagged != true)] | length' "$DEMO_DIR_ABS/detected-txs.json")"
if [ "$non_flagged_count" = "0" ]; then
  write_status complete audit-user "No non-flagged transfers to audit for $name"
  exit 0
fi

default_input="$DEMO_DIR_ABS/$slug-default-input.json"
jq --slurpfile ledger "$DEMO_DIR_ABS/ledger.json" --arg name "$name" '
  .detected = ((.detected // []) | map(select(.is_flagged != true) | select(. as $detected |
    ($ledger[0] // [] | any(
      .height == $detected.height
      and .action_index == $detected.action_index
      and ((.self_alias // "") | startswith($name))
      and (.amount != null)
    ) | not)
  )))
' "$DEMO_DIR_ABS/detected-txs.json" > "$default_input"

if [ "$(jq '[.detected[]?] | length' "$default_input")" = "0" ]; then
  write_status complete audit-user "No new transfers to audit for $name"
  exit 0
fi

audit_user_addresses() {
  local tier="$1"
  local input="$2"
  local output="$DEMO_DIR/$slug-$tier-audit.json"
  local audit_node="$PENUMBRA_GRPC"
  if [ "${AUDIT_DEMO_IN_CONTAINER:-false}" != "true" ]; then
    audit_node="$PENUMBRA_GRPC_CONTAINER"
  fi
  local args=(
    --input "$input"
    --dk-hex "$DK_HEX"
    --node "$audit_node"
    --output "$output"
    --timings-json "$DEMO_DIR/$slug-$tier-timings.json"
    --object-cache "$DEMO_DIR/orbis-object-cache.json"
    --tier "$tier"
    --orbis-endpoint "$ORBIS_ENDPOINT"
  )

  while IFS= read -r address; do
    [ -n "$address" ] && args+=(--subject-address "$address")
  done < <(jq -r --arg slug "$slug" '.users[] | select(.slug == $slug) | .addresses[]?.address' "$STATE_FILE")

  while IFS= read -r address; do
    [ -n "$address" ] && args+=(--known-address "$address")
  done < <(jq -r '.users[]?.addresses[]?.address' "$STATE_FILE")

  run_orbis_audit_locked "${args[@]}" | tee "$DEMO_DIR_ABS/$slug-$tier-audit.out"

  if jq -e 'length > 0' "$DEMO_DIR_ABS/$slug-$tier-audit.json" >/dev/null; then
    pcli_home "$(wallet_home alice)" tx compliance issuer-db update \
      --db "$ISSUER_DB" \
      --audit-output "$DEMO_DIR/$slug-$tier-audit.json" \
      --audit-subject "$name $tier"
  fi
}

audit_user_addresses default "$DEMO_DIR/$slug-default-input.json"

default_output="$DEMO_DIR_ABS/$slug-default-audit.json"
extension_input="$DEMO_DIR_ABS/$slug-extension-input.json"
if jq -e 'length > 0' "$default_output" >/dev/null; then
  refresh_outputs
  jq --slurpfile audit "$default_output" '
    ($audit[0] // [] | map({ height, action_index }) | unique) as $refs
    | .detected = ((.detected // []) | map(select(.is_flagged != true) | select(. as $detected |
      any($refs[]; .height == $detected.height and .action_index == $detected.action_index)
    )))
  ' "$DEMO_DIR_ABS/detected-txs.json" \
    | jq --slurpfile ledger "$DEMO_DIR_ABS/ledger.json" --arg name "$name" '
      .detected = ((.detected // []) | map(select(. as $detected |
        ($ledger[0] // [] | any(
          .height == $detected.height
          and .action_index == $detected.action_index
          and ((.self_alias // "") | startswith($name))
          and ((.counterparty_alias // "") != "")
        ) | not)
      )))
    ' > "$extension_input"

  if [ "$(jq '[.detected[]?] | length' "$extension_input")" != "0" ]; then
    audit_user_addresses extension "$DEMO_DIR/$slug-extension-input.json"
  else
    echo "Skipping extension audit because it cannot add counterparty knowledge."
    jq -n '[]' > "$DEMO_DIR_ABS/$slug-extension-audit.json"
  fi
else
  echo "Skipping extension audit because default audit decoded no transfers."
  jq -n '[]' > "$DEMO_DIR_ABS/$slug-extension-audit.json"
fi

refresh_outputs
jq --arg slug "$slug" --arg name "$name" \
  '.audits = ((.audits // []) + [{ userSlug: $slug, userName: $name, at: (now | todate) }])
    | .setup.updatedAt = (now | todate)' \
  "$STATE_FILE" > "$STATE_FILE.tmp"
mv "$STATE_FILE.tmp" "$STATE_FILE"
append_event audit "Audited $name"
write_status complete audit-user "Audit complete for $name"
