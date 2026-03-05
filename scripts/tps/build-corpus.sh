#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Build or append a compliance_tps corpus from offline tx JSON files, or generate them first with pcli.

Required:
  --pd-url <grpc_url>
  --wallet-home <pcli_home>
  --asset <asset_denom_or_id>
  --count <tx_count>
  --out <corpus_dir>

Required for rebuild mode (default):
  --scenario regulated|unregulated

Generation mode (if --tx-json-dir is omitted):
  --to-address <penumbra_address>            required in generation mode

Optional:
  --append                                 append into existing corpus instead of rebuilding it
  --tx-json-dir <dir>                        reuse existing offline tx JSON files
  --asset-kind <label>                       defaults to --asset
  --value <typed_value>                      defaults to 1<asset>
  --source <index>                           defaults to 0
  --source-indexes <csv>                     optional source index list (round-robin)
  --fee-tier <tier>                          defaults to low
  --source-label <label>                     defaults to local
  --chain-id <id>                            defaults to unknown
  --genesis-hash <hash>                      defaults to unknown
  --notes <text>                             defaults to empty
  --keep-tx-json                             keep auto-generated JSON tx files

Environment:
  PCLI_BIN=target/release/pcli
  TPS_BIN=target/release/compliance_tps
EOF
}

SCENARIO=""
PD_URL=""
WALLET_HOME=""
ASSET=""
COUNT=""
OUT_DIR=""
TO_ADDRESS=""
TX_JSON_DIR=""
ASSET_KIND=""
VALUE=""
SOURCE="0"
SOURCE_INDEXES=""
FEE_TIER="low"
SOURCE_LABEL="local"
CHAIN_ID="unknown"
GENESIS_HASH="unknown"
NOTES=""
KEEP_TX_JSON=0
APPEND=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scenario) SCENARIO="$2"; shift 2 ;;
    --pd-url) PD_URL="$2"; shift 2 ;;
    --wallet-home) WALLET_HOME="$2"; shift 2 ;;
    --asset) ASSET="$2"; shift 2 ;;
    --count) COUNT="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --to-address) TO_ADDRESS="$2"; shift 2 ;;
    --tx-json-dir) TX_JSON_DIR="$2"; shift 2 ;;
    --asset-kind) ASSET_KIND="$2"; shift 2 ;;
    --value) VALUE="$2"; shift 2 ;;
    --source) SOURCE="$2"; shift 2 ;;
    --source-indexes) SOURCE_INDEXES="$2"; shift 2 ;;
    --fee-tier) FEE_TIER="$2"; shift 2 ;;
    --source-label) SOURCE_LABEL="$2"; shift 2 ;;
    --chain-id) CHAIN_ID="$2"; shift 2 ;;
    --genesis-hash) GENESIS_HASH="$2"; shift 2 ;;
    --notes) NOTES="$2"; shift 2 ;;
    --keep-tx-json) KEEP_TX_JSON=1; shift ;;
    --append) APPEND=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$PD_URL" || -z "$WALLET_HOME" || -z "$ASSET" || -z "$COUNT" || -z "$OUT_DIR" ]]; then
  echo "Missing required args" >&2
  usage
  exit 1
fi
if [[ "$APPEND" -eq 0 && -z "$SCENARIO" ]]; then
  echo "--scenario is required unless --append is used" >&2
  exit 1
fi
if [[ -n "$SCENARIO" && "$SCENARIO" != "regulated" && "$SCENARIO" != "unregulated" ]]; then
  echo "--scenario must be regulated|unregulated" >&2
  exit 1
fi
if ! [[ "$COUNT" =~ ^[0-9]+$ ]] || [[ "$COUNT" -eq 0 ]]; then
  echo "--count must be a positive integer" >&2
  exit 1
fi

PCLI_BIN="${PCLI_BIN:-target/release/pcli}"
TPS_BIN="${TPS_BIN:-target/release/compliance_tps}"
if [[ ! -x "$PCLI_BIN" ]]; then
  echo "Missing executable: $PCLI_BIN" >&2
  echo "Build first: cargo build --release -p pcli" >&2
  exit 1
fi
if [[ ! -x "$TPS_BIN" ]]; then
  echo "Missing executable: $TPS_BIN" >&2
  echo "Build first: cargo build --release -p penumbra-sdk-bench" >&2
  exit 1
fi

if [[ -z "$ASSET_KIND" ]]; then
  ASSET_KIND="$ASSET"
fi
if [[ -z "$VALUE" ]]; then
  VALUE="1${ASSET}"
fi

AUTO_TMP_DIR=""
cleanup_temp() {
  if [[ -n "$AUTO_TMP_DIR" && -d "$AUTO_TMP_DIR" && "$KEEP_TX_JSON" -eq 0 ]]; then
    rm -rf "$AUTO_TMP_DIR"
  fi
}
trap cleanup_temp EXIT

if [[ -z "$TX_JSON_DIR" ]]; then
  if [[ -z "$TO_ADDRESS" ]]; then
    echo "--to-address is required when generating tx JSON files" >&2
    exit 1
  fi

  source_indexes=()
  if [[ -z "$SOURCE_INDEXES" ]]; then
    source_indexes=("$SOURCE")
  else
    IFS=',' read -r -a raw_indexes <<< "$SOURCE_INDEXES"
    for idx in "${raw_indexes[@]}"; do
      idx="${idx// /}"
      [[ -z "$idx" ]] && continue
      if ! [[ "$idx" =~ ^[0-9]+$ ]]; then
        echo "--source-indexes must be a comma-separated list of non-negative integers" >&2
        exit 1
      fi
      source_indexes+=("$idx")
    done
    if [[ "${#source_indexes[@]}" -eq 0 ]]; then
      echo "--source-indexes did not contain any usable index" >&2
      exit 1
    fi
  fi
  echo "Using source indexes: ${source_indexes[*]}"
  if [[ "${#source_indexes[@]}" -lt "$COUNT" ]]; then
    echo "WARNING: source index pool (${#source_indexes[@]}) is smaller than tx count ($COUNT); sources will be reused."
    echo "         For conflict-safe corpora, use at least one source index per expected submitted tx."
  fi

  AUTO_TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tps-corpus-json.XXXXXX")"
  TX_JSON_DIR="$AUTO_TMP_DIR"
  echo "Generating $COUNT offline tx JSON files into $TX_JSON_DIR"
  for i in $(seq 1 "$COUNT"); do
    tx_file="$TX_JSON_DIR/tx-$(printf '%06d' "$i").json"
    src_idx="${source_indexes[$(( (i - 1) % ${#source_indexes[@]} ))]}"
    "$PCLI_BIN" \
      --home "$WALLET_HOME" \
      --grpc-url "$PD_URL" \
      tx --offline "$tx_file" send \
      --to "$TO_ADDRESS" \
      --source "$src_idx" \
      --fee-tier "$FEE_TIER" \
      "$VALUE"
    if (( i % 100 == 0 )); then
      echo "  generated $i/$COUNT"
    fi
  done
fi

mkdir -p "$OUT_DIR"

if [[ "$APPEND" -eq 1 ]]; then
  "$TPS_BIN" corpus append \
    --json-dir "$TX_JSON_DIR" \
    --corpus "$OUT_DIR" \
    --asset-kind "$ASSET_KIND" \
    --source-label "$SOURCE_LABEL" \
    --notes "$NOTES"
else
  "$TPS_BIN" corpus pack \
    --json-dir "$TX_JSON_DIR" \
    --out "$OUT_DIR" \
    --scenario "$SCENARIO" \
    --asset-kind "$ASSET_KIND" \
    --source-label "$SOURCE_LABEL" \
    --chain-id "$CHAIN_ID" \
    --genesis-hash "$GENESIS_HASH" \
    --notes "$NOTES"
fi

"$TPS_BIN" corpus verify --corpus "$OUT_DIR" --observer "$PD_URL"

echo "Corpus ready: $OUT_DIR"
