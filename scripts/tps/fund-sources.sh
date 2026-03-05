#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Fund a set of wallet source indexes for TPS corpus generation.

Required:
  --pd-url <grpc_url>
  --wallet-home <pcli_home>
  --indexes <csv>                      source address indexes to fund
  --asset <asset_denom_or_id>          asset to fund for payload spends
  --asset-amount <base_units>          amount of --asset per index

Optional:
  --fee-asset <asset_denom_or_id>      default: upenumbra
  --fee-amount <base_units>            amount of fee asset per index (default: 1000000)
  --from-source <index>                funding source index (default: 0)
  --fee-tier <tier>                    default: low
  --chunk-size <n>                     outputs per funding tx batch (default: 16)
  --skip-sync                          skip view sync after each chunk

Environment:
  PCLI_BIN=target/release/pcli
EOF
}

PD_URL=""
WALLET_HOME=""
INDEXES_CSV=""
ASSET=""
ASSET_AMOUNT=""
FEE_ASSET="upenumbra"
FEE_AMOUNT="1000000"
FROM_SOURCE="0"
FEE_TIER="low"
CHUNK_SIZE="16"
SKIP_SYNC=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pd-url) PD_URL="$2"; shift 2 ;;
    --wallet-home) WALLET_HOME="$2"; shift 2 ;;
    --indexes) INDEXES_CSV="$2"; shift 2 ;;
    --asset) ASSET="$2"; shift 2 ;;
    --asset-amount) ASSET_AMOUNT="$2"; shift 2 ;;
    --fee-asset) FEE_ASSET="$2"; shift 2 ;;
    --fee-amount) FEE_AMOUNT="$2"; shift 2 ;;
    --from-source) FROM_SOURCE="$2"; shift 2 ;;
    --fee-tier) FEE_TIER="$2"; shift 2 ;;
    --chunk-size) CHUNK_SIZE="$2"; shift 2 ;;
    --skip-sync) SKIP_SYNC=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$PD_URL" || -z "$WALLET_HOME" || -z "$INDEXES_CSV" || -z "$ASSET" || -z "$ASSET_AMOUNT" ]]; then
  echo "Missing required args" >&2
  usage
  exit 1
fi
if ! [[ "$ASSET_AMOUNT" =~ ^[0-9]+$ ]] || [[ "$ASSET_AMOUNT" -eq 0 ]]; then
  echo "--asset-amount must be a positive integer" >&2
  exit 1
fi
if ! [[ "$FEE_AMOUNT" =~ ^[0-9]+$ ]] || [[ "$FEE_AMOUNT" -eq 0 ]]; then
  echo "--fee-amount must be a positive integer" >&2
  exit 1
fi
if ! [[ "$CHUNK_SIZE" =~ ^[0-9]+$ ]] || [[ "$CHUNK_SIZE" -eq 0 ]]; then
  echo "--chunk-size must be a positive integer" >&2
  exit 1
fi
if ! [[ "$FROM_SOURCE" =~ ^[0-9]+$ ]]; then
  echo "--from-source must be a non-negative integer" >&2
  usage
  exit 1
fi

if [[ "$FEE_ASSET" != "$ASSET" && "$CHUNK_SIZE" -gt 16 ]]; then
  echo "Chunk size ${CHUNK_SIZE} is too high for dual-asset funding; capping to 16 to avoid max-tx-size failures."
  CHUNK_SIZE=16
fi

PCLI_BIN="${PCLI_BIN:-target/release/pcli}"
if [[ ! -x "$PCLI_BIN" ]]; then
  echo "Missing executable: $PCLI_BIN" >&2
  echo "Build first: cargo build --release -p pcli" >&2
  exit 1
fi

unique_indexes=()
IFS=',' read -r -a raw_indexes <<< "$INDEXES_CSV"
for idx in "${raw_indexes[@]}"; do
  idx="${idx// /}"
  [[ -z "$idx" ]] && continue
  if ! [[ "$idx" =~ ^[0-9]+$ ]]; then
    echo "--indexes must be a comma-separated list of non-negative integers" >&2
    exit 1
  fi
  unique_indexes+=("$idx")
done
if [[ "${#unique_indexes[@]}" -eq 0 ]]; then
  echo "--indexes did not contain any usable index" >&2
  exit 1
fi

# Deduplicate while preserving order, and skip funding source index itself.
deduped=()
seen="|"
for idx in "${unique_indexes[@]}"; do
  [[ "$idx" == "$FROM_SOURCE" ]] && continue
  if [[ "$seen" == *"|$idx|"* ]]; then
    continue
  fi
  seen="${seen}${idx}|"
  deduped+=("$idx")
done

if [[ "${#deduped[@]}" -eq 0 ]]; then
  echo "No indexes to fund after filtering source index ${FROM_SOURCE}"
  exit 0
fi

echo "Funding ${#deduped[@]} source indexes from index=${FROM_SOURCE}"
echo "  payload asset: ${ASSET_AMOUNT}${ASSET}"
echo "  fee asset:     ${FEE_AMOUNT}${FEE_ASSET}"
echo "  chunk size:    ${CHUNK_SIZE}"

"$PCLI_BIN" --home "$WALLET_HOME" --grpc-url "$PD_URL" view sync >/dev/null

pos=0
batch_no=0
while [[ "$pos" -lt "${#deduped[@]}" ]]; do
  outputs=()
  end=$(( pos + CHUNK_SIZE ))
  [[ "$end" -gt "${#deduped[@]}" ]] && end="${#deduped[@]}"
  for (( i=pos; i<end; i++ )); do
    idx="${deduped[$i]}"
    addr="$("$PCLI_BIN" --home "$WALLET_HOME" view address "$idx")"
    outputs+=(--output "${ASSET_AMOUNT}${ASSET}:${addr}")
    if [[ "$FEE_ASSET" != "$ASSET" ]]; then
      outputs+=(--output "${FEE_AMOUNT}${FEE_ASSET}:${addr}")
    fi
  done

  "$PCLI_BIN" \
    --home "$WALLET_HOME" \
    --grpc-url "$PD_URL" \
    tx send-multi \
    --source "$FROM_SOURCE" \
    --fee-tier "$FEE_TIER" \
    "${outputs[@]}"

  batch_no=$(( batch_no + 1 ))
  echo "  funded batch ${batch_no}: $((end - pos)) indexes"

  if [[ "$SKIP_SYNC" -eq 0 ]]; then
    "$PCLI_BIN" --home "$WALLET_HOME" --grpc-url "$PD_URL" view sync >/dev/null
  fi

  pos="$end"
done

if [[ "$SKIP_SYNC" -eq 0 ]]; then
  "$PCLI_BIN" --home "$WALLET_HOME" --grpc-url "$PD_URL" view sync >/dev/null
fi

echo "Funding complete."
