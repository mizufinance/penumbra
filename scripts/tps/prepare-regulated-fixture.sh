#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Register regulated asset + user compliance keys for TPS regulated corpus creation.

Required:
  --pd-url <grpc_url>
  --wallet-home <pcli_home>
  --asset <asset_id_or_denom>

Optional:
  --threshold <base_units>        default: 500000000
  --dk-pub-hex <hex>              optional issuer detection key public value
  --ring-pk-hex <hex>             optional Orbis ring public key
  --address-indexes <csv>         default: 0
  --address-count <n>             also register 0..n-1
  --fee-tier <tier>               default: low
  --wait-seconds <n>              default: 8
  --allow-existing                treat "already registered" as success
  --dk-out-file <path>            write generated dk/dk_pub material

Environment:
  PCLI_BIN=target/release/pcli
EOF
}

PD_URL=""
WALLET_HOME=""
ASSET=""
THRESHOLD="500000000"
DK_PUB_HEX=""
RING_PK_HEX=""
ADDRESS_INDEXES="0"
ADDRESS_COUNT="0"
FEE_TIER="low"
WAIT_SECONDS="8"
ALLOW_EXISTING=0
DK_OUT_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pd-url) PD_URL="$2"; shift 2 ;;
    --wallet-home) WALLET_HOME="$2"; shift 2 ;;
    --asset) ASSET="$2"; shift 2 ;;
    --threshold) THRESHOLD="$2"; shift 2 ;;
    --dk-pub-hex) DK_PUB_HEX="$2"; shift 2 ;;
    --ring-pk-hex) RING_PK_HEX="$2"; shift 2 ;;
    --address-indexes) ADDRESS_INDEXES="$2"; shift 2 ;;
    --address-count) ADDRESS_COUNT="$2"; shift 2 ;;
    --fee-tier) FEE_TIER="$2"; shift 2 ;;
    --wait-seconds) WAIT_SECONDS="$2"; shift 2 ;;
    --allow-existing) ALLOW_EXISTING=1; shift ;;
    --dk-out-file) DK_OUT_FILE="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$PD_URL" || -z "$WALLET_HOME" || -z "$ASSET" ]]; then
  echo "Missing required args" >&2
  usage
  exit 1
fi
if ! [[ "$WAIT_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "--wait-seconds must be an integer" >&2
  exit 1
fi
if ! [[ "$ADDRESS_COUNT" =~ ^[0-9]+$ ]]; then
  echo "--address-count must be a non-negative integer" >&2
  exit 1
fi

PCLI_BIN="${PCLI_BIN:-target/release/pcli}"
if [[ ! -x "$PCLI_BIN" ]]; then
  echo "Missing executable: $PCLI_BIN" >&2
  echo "Build first: cargo build --release -p pcli" >&2
  exit 1
fi

run_pcli_allow_existing() {
  local output
  set +e
  output="$($PCLI_BIN --home "$WALLET_HOME" --grpc-url "$PD_URL" "$@" 2>&1)"
  local rc=$?
  set -e
  if [[ $rc -eq 0 ]]; then
    printf '%s\n' "$output"
    return 0
  fi
  if [[ "$ALLOW_EXISTING" -eq 1 ]] && [[ "$output" == *"already"* || "$output" == *"exists"* ]]; then
    printf '%s\n' "$output"
    echo "Ignoring existing-state error (--allow-existing)"
    return 0
  fi
  printf '%s\n' "$output" >&2
  return $rc
}

generate_dk_pub_if_missing() {
  if [[ -n "$DK_PUB_HEX" ]]; then
    return 0
  fi

  echo "No --dk-pub-hex provided, generating issuer DK via pcli"
  local dk_out
  dk_out="$($PCLI_BIN --home "$WALLET_HOME" tx compliance generate-dk 2>&1)"
  DK_PUB_HEX="$(echo "$dk_out" | sed -n 's/^[[:space:]]*DK_pub (hex):[[:space:]]*//p' | head -n1)"
  local dk_hex
  dk_hex="$(echo "$dk_out" | sed -n 's/^[[:space:]]*DK (hex):[[:space:]]*//p' | head -n1)"

  if [[ -z "$DK_PUB_HEX" ]]; then
    echo "Failed to parse DK_pub from pcli output" >&2
    echo "$dk_out" >&2
    exit 1
  fi
  if [[ -z "$dk_hex" ]]; then
    echo "Failed to parse DK from pcli output" >&2
    echo "$dk_out" >&2
    exit 1
  fi

  echo "Generated DK_pub: ${DK_PUB_HEX:0:16}..."
  if [[ -n "$DK_OUT_FILE" ]]; then
    mkdir -p "$(dirname "$DK_OUT_FILE")"
    cat > "$DK_OUT_FILE" <<EOF
DK_HEX=${dk_hex}
DK_PUB_HEX=${DK_PUB_HEX}
EOF
    echo "Wrote generated DK material to $DK_OUT_FILE"
  fi
}

generate_dk_pub_if_missing

address_indexes=()
seen="|"
if [[ "$ADDRESS_COUNT" -gt 0 ]]; then
  for (( idx=0; idx<ADDRESS_COUNT; idx++ )); do
    address_indexes+=("$idx")
    seen="${seen}${idx}|"
  done
fi

IFS=',' read -r -a idx_array <<< "$ADDRESS_INDEXES"
for idx in "${idx_array[@]}"; do
  idx="${idx// /}"
  [[ -z "$idx" ]] && continue
  if ! [[ "$idx" =~ ^[0-9]+$ ]]; then
    echo "--address-indexes must be a comma-separated list of non-negative integers" >&2
    exit 1
  fi
  if [[ "$seen" == *"|$idx|"* ]]; then
    continue
  fi
  seen="${seen}${idx}|"
  address_indexes+=("$idx")
done

if [[ "${#address_indexes[@]}" -eq 0 ]]; then
  address_indexes=("0")
fi

echo "Registering regulated asset: $ASSET"
args=(tx compliance register-asset "$ASSET" --regulated --threshold "$THRESHOLD" --fee-tier "$FEE_TIER")
args+=(--dk-pub-hex "$DK_PUB_HEX")
if [[ -n "$RING_PK_HEX" ]]; then
  args+=(--ring-pk-hex "$RING_PK_HEX")
fi
run_pcli_allow_existing "${args[@]}"
sleep "$WAIT_SECONDS"

for idx in "${address_indexes[@]}"; do
  echo "Registering user for asset=$ASSET address_index=$idx"
  run_pcli_allow_existing tx compliance register-user "$ASSET" --address-index "$idx" --fee-tier "$FEE_TIER"
  sleep "$WAIT_SECONDS"
done

echo "Regulated fixture prepared for asset=$ASSET"
