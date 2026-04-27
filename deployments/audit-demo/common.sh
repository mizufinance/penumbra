#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(pwd)}"
cd "$REPO_ROOT"

PENUMBRA_IMAGE="${PENUMBRA_IMAGE:-penumbra:bankd-demo}"
DEMO_DIR="${DEMO_DIR:-.localnet/audit-demo}"
DEMO_DIR_ABS="$REPO_ROOT/$DEMO_DIR"
WALLETS_DIR="$DEMO_DIR/wallets"
WALLETS_DIR_ABS="$DEMO_DIR_ABS/wallets"
STATUS_FILE="$DEMO_DIR_ABS/status.json"
STATE_FILE="$DEMO_DIR_ABS/state.json"
ISSUER_DB="$DEMO_DIR/issuer-ledger.db"
SCANNER_HEALTH_FILE="$DEMO_DIR_ABS/scanner-health.json"
LOCK_DIR="$DEMO_DIR_ABS/orbis-sourcehub.lock"

PENUMBRA_GRPC="${PENUMBRA_GRPC:-http://localhost:8080}"
PENUMBRA_GRPC_CONTAINER="${PENUMBRA_GRPC_CONTAINER:-http://pd-node0:8080}"
AUDIT_DOCKER_NETWORK="${AUDIT_DOCKER_NETWORK:-infra_bankd-net}"
ORBIS_ENDPOINT="${ORBIS_ENDPOINT:-http://orbis-node1:50051}"
ORBIS_NODE1_ENDPOINT="${ORBIS_NODE1_ENDPOINT:-$ORBIS_ENDPOINT}"
ORBIS_NODE2_ENDPOINT="${ORBIS_NODE2_ENDPOINT:-http://127.0.0.1:50052}"
ORBIS_NODE3_ENDPOINT="${ORBIS_NODE3_ENDPOINT:-http://127.0.0.1:50053}"
ORBIS_NODE1_CONTAINER="${ORBIS_NODE1_CONTAINER:-orbis-node1}"
ORBIS_NODE2_CONTAINER="${ORBIS_NODE2_CONTAINER:-orbis-node2}"
ORBIS_NODE3_CONTAINER="${ORBIS_NODE3_CONTAINER:-orbis-node3}"
ORBIS_SOURCEHUB_CHAIN_ID="${ORBIS_SOURCEHUB_CHAIN_ID:-sourcehub-localnet}"
ORBIS_SOURCEHUB_RPC="${ORBIS_SOURCEHUB_RPC:-http://sourcehub:26657}"
ORBIS_SOURCEHUB_REST="${ORBIS_SOURCEHUB_REST:-http://sourcehub:1317}"
ORBIS_SOURCEHUB_GRPC="${ORBIS_SOURCEHUB_GRPC:-http://sourcehub:9090}"
ORBIS_SOURCEHUB_DENOM="${ORBIS_SOURCEHUB_DENOM:-uopen}"

ASSET="transfer/channel-0/ubrl"
THRESHOLD="500000000"
CHARLIE_PHRASE="decorate bright ozone fork gallery riot bus exhaust worth way bone indoor calm squirrel merry zero scheme cotton until shop any excess stage laundry"
ALICE_PHRASE="wealth flavor believe regret funny network recall kiss grape useless pepper cram hint member few certain unveil rather brick bargain curious require crowd raise"
DEFAULT_USER_SLUGS=("alice" "bob")
ADDRESS_INDEXES=(0 1)
FEE_FUND_AMOUNT="50000"

mkdir -p "$DEMO_DIR_ABS" "$WALLETS_DIR_ABS"

write_status() {
  local state="$1"
  local step="$2"
  local message="${3:-}"
  jq -n \
    --arg state "$state" \
    --arg step "$step" \
    --arg message "$message" \
    '{
      state: $state,
      step: $step,
      message: $message,
      updatedAt: now | todate
    }' > "$STATUS_FILE"
}

init_state_file() {
  if [ -f "$STATE_FILE" ]; then
    return
  fi
  jq -n \
    --arg penumbraGrpc "$PENUMBRA_GRPC" \
    --arg asset "$ASSET" \
    --arg threshold "$THRESHOLD" \
    '{
      setup: {
        initialized: false,
        assetRegistered: false,
        updatedAt: now | todate
      },
      endpoints: {
        penumbraGrpc: $penumbraGrpc
      },
      asset: {
        denom: $asset,
        threshold: $threshold
      },
      users: [],
      scan: {
        detected: [],
        detectedCount: 0,
        flaggedCount: 0,
        auditedCount: 0
      },
      scanner: {
        running: false,
        lastHeight: null,
        updatedAt: null
      },
      ledgerRows: [],
      audits: [],
      events: []
    }' > "$STATE_FILE"
}

update_state() {
  local filter="$1"
  jq "$filter" "$STATE_FILE" > "$STATE_FILE.tmp"
  mv "$STATE_FILE.tmp" "$STATE_FILE"
}

append_event() {
  local type="$1"
  local message="$2"
  jq --arg type "$type" --arg message "$message" \
    '.events = ((.events // []) + [{ type: $type, message: $message, at: now | todate }])
      | .setup.updatedAt = (now | todate)' \
    "$STATE_FILE" > "$STATE_FILE.tmp"
  mv "$STATE_FILE.tmp" "$STATE_FILE"
}

slugify() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//' \
    | cut -c1-48
}

pcli_home() {
  local home="$1"
  shift
  mkdir -p "$REPO_ROOT/$home"
  if [ "${AUDIT_DEMO_IN_CONTAINER:-false}" = "true" ]; then
    HOME=/home/penumbra PENUMBRA_PCLI_HOME="/work/$home" pcli "$@"
  else
    docker run --rm -i -u "$(id -u):$(id -g)" \
      -e HOME=/home/penumbra \
      -e PENUMBRA_PCLI_HOME=/home/penumbra/.local/share/pcli \
      -v "$REPO_ROOT/$home":/home/penumbra/.local/share/pcli \
      -v "$REPO_ROOT":/work \
      -w /work \
      --network host \
      --entrypoint pcli \
      "$PENUMBRA_IMAGE" "$@"
  fi
}

penumbra_bin() {
  local bin="$1"
  shift
  if [ "${AUDIT_DEMO_IN_CONTAINER:-false}" = "true" ]; then
    ORBIS_NODE1_ENDPOINT="$ORBIS_NODE1_ENDPOINT" \
    ORBIS_NODE2_ENDPOINT="$ORBIS_NODE2_ENDPOINT" \
    ORBIS_NODE3_ENDPOINT="$ORBIS_NODE3_ENDPOINT" \
    ORBIS_NODE1_CONTAINER="$ORBIS_NODE1_CONTAINER" \
    ORBIS_NODE2_CONTAINER="$ORBIS_NODE2_CONTAINER" \
    ORBIS_NODE3_CONTAINER="$ORBIS_NODE3_CONTAINER" \
    ORBIS_SOURCEHUB_CHAIN_ID="$ORBIS_SOURCEHUB_CHAIN_ID" \
    ORBIS_SOURCEHUB_RPC="$ORBIS_SOURCEHUB_RPC" \
    ORBIS_SOURCEHUB_REST="$ORBIS_SOURCEHUB_REST" \
    ORBIS_SOURCEHUB_GRPC="$ORBIS_SOURCEHUB_GRPC" \
    ORBIS_SOURCEHUB_DENOM="$ORBIS_SOURCEHUB_DENOM" \
      "$bin" "$@"
  else
    local network_args=(--network host)
    if [ "$bin" = "orbis-audit" ] || [ "$bin" = "orbis-integration" ]; then
      network_args=(--network "$AUDIT_DOCKER_NETWORK")
    fi
    docker run --rm \
      -e ORBIS_NODE1_ENDPOINT="$ORBIS_NODE1_ENDPOINT" \
      -e ORBIS_NODE2_ENDPOINT="$ORBIS_NODE2_ENDPOINT" \
      -e ORBIS_NODE3_ENDPOINT="$ORBIS_NODE3_ENDPOINT" \
      -e ORBIS_NODE1_CONTAINER="$ORBIS_NODE1_CONTAINER" \
      -e ORBIS_NODE2_CONTAINER="$ORBIS_NODE2_CONTAINER" \
      -e ORBIS_NODE3_CONTAINER="$ORBIS_NODE3_CONTAINER" \
      -e ORBIS_SOURCEHUB_CHAIN_ID="$ORBIS_SOURCEHUB_CHAIN_ID" \
      -e ORBIS_SOURCEHUB_RPC="$ORBIS_SOURCEHUB_RPC" \
      -e ORBIS_SOURCEHUB_REST="$ORBIS_SOURCEHUB_REST" \
      -e ORBIS_SOURCEHUB_GRPC="$ORBIS_SOURCEHUB_GRPC" \
      -e ORBIS_SOURCEHUB_DENOM="$ORBIS_SOURCEHUB_DENOM" \
      -v "$REPO_ROOT":/work \
      -w /work \
      "${network_args[@]}" \
      --entrypoint "$bin" \
      "$PENUMBRA_IMAGE" "$@"
  fi
}

wallet_home() {
  echo "$WALLETS_DIR/$1"
}

init_wallet() {
  local slug="$1"
  local mode="$2"
  local home
  home="$(wallet_home "$slug")"
  mkdir -p "$home"
  if [ -f "$home/config.toml" ]; then
    return
  fi
  if [ "$mode" = "alice" ]; then
    printf '%s\n' "$ALICE_PHRASE" \
      | pcli_home "$home" init --grpc-url "$PENUMBRA_GRPC" soft-kms import-phrase
  elif [ "$mode" = "charlie" ]; then
    printf '%s\n' "$CHARLIE_PHRASE" \
      | pcli_home "$home" init --grpc-url "$PENUMBRA_GRPC" soft-kms import-phrase
  else
    printf '\n' \
      | pcli_home "$home" init --grpc-url "$PENUMBRA_GRPC" soft-kms generate
  fi
}

sync_wallet() {
  pcli_home "$(wallet_home "$1")" view sync
}

address_for() {
  pcli_home "$(wallet_home "$1")" view address "$2" | tail -n 1
}

user_exists() {
  local slug="$1"
  jq -e --arg slug "$slug" '.users[]? | select(.slug == $slug)' "$STATE_FILE" >/dev/null
}

user_exists_by_address() {
  local address="$1"
  jq -e --arg address "$address" '.users[]? | .addresses[]? | select(.address == $address)' "$STATE_FILE" >/dev/null
}

user_name_from_slug() {
  local slug="$1"
  jq -r --arg slug "$slug" '.users[]? | select(.slug == $slug) | .name' "$STATE_FILE"
}

known_user_or_fail() {
  local slug
  slug="$(slugify "$1")"
  if ! user_exists "$slug"; then
    echo "Unknown audit user: $1" >&2
    exit 1
  fi
  echo "$slug"
}

add_user_state() {
  local user_json="$1"
  jq --argjson user "$user_json" \
    '.users = ((.users // []) | map(select(.slug != $user.slug)) + [$user])
      | .setup.updatedAt = (now | todate)' \
    "$STATE_FILE" > "$STATE_FILE.tmp"
  mv "$STATE_FILE.tmp" "$STATE_FILE"
}

alias_addresses() {
  local name="$1"
  shift
  local index=0
  local address
  for address in "$@"; do
    pcli_home "$(wallet_home alice)" tx compliance issuer-db alias \
      --db "$ISSUER_DB" \
      --address "$address" \
      --name "$name address $index"
    index=$((index + 1))
  done
}

fund_fee_index() {
  local slug="$1"
  local index="$2"
  if [ "$slug" = "alice" ] && [ "$index" = "0" ]; then
    return
  fi
  local address
  address="$(address_for "$slug" "$index")"
  pcli_home "$(wallet_home alice)" tx transfer --to "$address" "${FEE_FUND_AMOUNT}upenumbra"
}

load_dk() {
  DK_HEX="$(jq -r '.issuer.dkHex // empty' "$STATE_FILE")"
  if [ -z "$DK_HEX" ]; then
    echo "Issuer DK is missing; audit setup is not complete." >&2
    exit 1
  fi
  export DK_HEX
}

refresh_outputs() {
  init_state_file
  if [ ! -f "$SCANNER_HEALTH_FILE" ]; then
    jq -n '{ running: false, lastHeight: null, updatedAt: now | todate }' > "$SCANNER_HEALTH_FILE"
  fi
  if [ ! -f "$DEMO_DIR_ABS/detected-txs.json" ]; then
    echo '{"detected":[]}' > "$DEMO_DIR_ABS/detected-txs.json"
  fi

  local ledger_file="$DEMO_DIR_ABS/ledger.json"
  local ledger_tmp="$DEMO_DIR_ABS/ledger.$$.$RANDOM.tmp.json"
  if [ -f "$DEMO_DIR_ABS/issuer-ledger.db" ]; then
    if pcli_home "$(wallet_home alice)" tx compliance issuer-db show --json \
      --db "$ISSUER_DB" > "$ledger_tmp"; then
      mv "$ledger_tmp" "$ledger_file"
    else
      rm -f "$ledger_tmp"
      [ -f "$ledger_file" ] || echo "[]" > "$ledger_file"
    fi
  else
    echo "[]" > "$ledger_file"
  fi

  jq \
    --slurpfile detected "$DEMO_DIR_ABS/detected-txs.json" \
    --slurpfile ledger "$ledger_file" \
    --slurpfile scanner "$SCANNER_HEALTH_FILE" \
    '.scan = {
      detected: ($detected[0].detected // []),
      detectedCount: (($detected[0].detected // []) | length),
      flaggedCount: (($detected[0].detected // []) | map(select(.is_flagged == true)) | length),
      auditedCount: ($ledger[0] | map(select(.amount != null and .is_flagged == false)) | length)
    }
    | .scanner = ($scanner[0] // { running: false })
    | .ledgerRows = $ledger[0]
    | .setup.updatedAt = (now | todate)' \
    "$STATE_FILE" > "$STATE_FILE.tmp"
  mv "$STATE_FILE.tmp" "$STATE_FILE"
}

acquire_orbis_lock() {
  while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    sleep 1
  done
}

release_orbis_lock() {
  rmdir "$LOCK_DIR" 2>/dev/null || true
}

run_orbis_audit_locked() {
  acquire_orbis_lock
  set +e
  penumbra_bin orbis-audit "$@"
  local status=$?
  set -e
  release_orbis_lock
  return "$status"
}
