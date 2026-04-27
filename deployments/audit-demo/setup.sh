#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/common.sh"

LOG_FILE="$DEMO_DIR_ABS/setup.log"
touch "$LOG_FILE"
exec > >(tee -a "$LOG_FILE") 2>&1

fail() {
  local code=$?
  release_orbis_lock || true
  write_status failed setup "Audit setup failed with exit code $code"
  exit "$code"
}
trap fail ERR

setup_asset() {
  if jq -e '.setup.assetRegistered == true' "$STATE_FILE" >/dev/null; then
    echo "Regulated asset already registered"
  else
    penumbra_bin orbis-integration setup-ring --output-json "$DEMO_DIR/ring.json"

    pcli_home "$(wallet_home alice)" tx compliance generate-dk | tee "$DEMO_DIR_ABS/dk.txt"
    DK_HEX="$(sed -n 's/.*DK (hex): //p' "$DEMO_DIR_ABS/dk.txt" | head -1)"
    DK_PUB_HEX="$(sed -n 's/.*DK_pub (hex): //p' "$DEMO_DIR_ABS/dk.txt" | head -1)"
    RING_PK_HEX="$(jq -r '.ringPkHex' "$DEMO_DIR_ABS/ring.json")"
    RING_ID="$(jq -r '.ringId' "$DEMO_DIR_ABS/ring.json")"
    POLICY_ID="$(jq -r '.policyId' "$DEMO_DIR_ABS/ring.json")"
    RESOURCE="$(jq -r '.resource' "$DEMO_DIR_ABS/ring.json")"
    PERMISSION="$(jq -r '.permission' "$DEMO_DIR_ABS/ring.json")"

    pcli_home "$(wallet_home alice)" tx compliance register-asset "$ASSET" --regulated \
      --dk-pub-hex "$DK_PUB_HEX" \
      --threshold "$THRESHOLD" \
      --ring-pk-hex "$RING_PK_HEX" \
      --ring-id "$RING_ID" \
      --policy-id "$POLICY_ID" \
      --resource "$RESOURCE" \
      --permission "$PERMISSION" \
      | tee "$DEMO_DIR_ABS/register-asset.out"

    jq \
      --slurpfile ring "$DEMO_DIR_ABS/ring.json" \
      --arg dkHex "$DK_HEX" \
      --arg dkPubHex "$DK_PUB_HEX" \
      '.ring = $ring[0]
        | .issuer = { dkHex: $dkHex, dkPubHex: $dkPubHex }
        | .setup.assetRegistered = true
        | .setup.updatedAt = (now | todate)' \
      "$STATE_FILE" > "$STATE_FILE.tmp"
    mv "$STATE_FILE.tmp" "$STATE_FILE"
    append_event setup "Registered regulated BRL asset"
  fi

  if [ ! -f "$DEMO_DIR_ABS/issuer-ledger.db" ]; then
    pcli_home "$(wallet_home alice)" tx compliance issuer-db init --db "$ISSUER_DB"
  fi
}

register_default_user() {
  local name="$1"
  local mode="$2"
  local slug
  slug="$(slugify "$name")"
  if user_exists "$slug"; then
    echo "$name already registered"
    return
  fi

  init_wallet "$slug" "$mode"
  sync_wallet "$slug"

  local index
  for index in "${ADDRESS_INDEXES[@]}"; do
    fund_fee_index "$slug" "$index"
  done
  sync_wallet "$slug"

  local addresses_json="[]"
  local addresses=()
  for index in "${ADDRESS_INDEXES[@]}"; do
    local address register_file
    address="$(address_for "$slug" "$index")"
    register_file="$DEMO_DIR_ABS/register-$slug-$index.out"
    pcli_home "$(wallet_home "$slug")" tx compliance register-user "$ASSET" --address-index "$index" \
      | tee "$register_file"
    addresses+=("$address")
    addresses_json="$(jq \
      --argjson addresses "$addresses_json" \
      --arg index "$index" \
      --arg address "$address" \
      -n '$addresses + [{ index: ($index | tonumber), address: $address }]')"
  done

  local user_json
  user_json="$(jq -n \
    --arg name "$name" \
    --arg slug "$slug" \
    --arg home "$DEMO_DIR/wallets/$slug" \
    --argjson addresses "$addresses_json" \
    '{
      name: $name,
      slug: $slug,
      home: $home,
      addresses: $addresses,
      default: true,
      createdAt: now | todate
    }')"
  add_user_state "$user_json"
  alias_addresses "$name" "${addresses[@]}"
  append_event user "Registered $name"
}

main() {
  write_status running setup "Initializing audit setup"
  init_state_file

  init_wallet alice alice
  sync_wallet alice
  setup_asset
  register_default_user Alice alice
  register_default_user Bob generated
  register_default_user Charlie charlie

  update_state '.setup.initialized = true | .setup.updatedAt = (now | todate)'
  refresh_outputs
  write_status complete setup "Audit setup ready"
}

main "$@"
