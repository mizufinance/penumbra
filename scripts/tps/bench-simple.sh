#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Simple persistent TPS workflow (single snapshot + persistent corpora lineage).

Commands:
  prepare   Build/fund/register, build initial corpora, save base snapshot.
  run       Restore base snapshot and run regulated+unregulated benchmark.
  append    Restore base snapshot, append to both corpora, re-save base snapshot.
  refresh   Rebuild corpora from seed snapshot and replace base snapshot.
  verify    Restore base snapshot and verify corpus compatibility against observer.
  status    Show current corpus counts, snapshot presence, and lineage metadata.

Examples:
  ./scripts/tps/bench-simple.sh prepare
  ./scripts/tps/bench-simple.sh run --offered-tps 1,2 --steady-blocks 20
  ./scripts/tps/bench-simple.sh append --count 4
  ./scripts/tps/bench-simple.sh refresh
  ./scripts/tps/bench-simple.sh verify --verify-scenario regulated
  ./scripts/tps/bench-simple.sh status

Common options:
  --seed-snapshot <name>         default: local-quick-fixed
  --base-snapshot <name>         default: local-tps-persistent
  --asset-unreg <denom_or_id>    default: upenumbra
  --asset-reg <denom_or_id>      default: test_usd
  --asset-amount <base_units>    default: 1000
  --fee-amount <base_units>      default: 1000
  --fund-chunk-size <n>          default: 16
  --skip-build                   prepare/refresh: skip cargo build
  --skip-funding                 prepare/append/refresh: skip fixture+funding

Prepare/Refresh options:
  --unreg-count <n>              default: 12
  --reg-count <n>                default: 12

Run options:
  --run-label <label>            default: persistent-bench
  --offered-tps <csv>            default: 1
  --repeats <n>                  default: 1
  --warmup-blocks <n>            default: 2
  --steady-blocks <n>            default: 8
  --target-block-time-ms <n>     default: 500
  --submit-workers <n>           default: 1
  --max-inflight <n>             default: 2
  --max-height-drift <n>         default: 200
  --auto-refresh                 refresh automatically if drift gate fails

Append options:
  --append-count <n>             default: 4
  --count <n>                    alias for --append-count

Verify options:
  --verify-scenario <name>       one of: both|regulated|unregulated (default: both)

Notes:
  - This script keeps corpus validity by enforcing one lineage:
    build/append only from restored base snapshot, then re-save that same snapshot.
  - Sidecar metadata is written to:
    corpus/unregulated/lineage.json and corpus/regulated/lineage.json
  - It does not run git commands.
USAGE
}

die() {
  echo "Error: $*" >&2
  exit 1
}

require_pos_int() {
  local flag="$1"
  local value="$2"
  if ! [[ "$value" =~ ^[0-9]+$ ]] || [[ "$value" -eq 0 ]]; then
    die "${flag} must be a positive integer"
  fi
}

require_arg_value() {
  local flag="$1"
  local argc="$2"
  local next="${3:-}"
  if [[ "$argc" -lt 2 || -z "$next" || "$next" == -* ]]; then
    die "${flag} requires a value"
  fi
}

range_csv() {
  local start="$1"
  local end="$2"
  if [[ "$start" -gt "$end" ]]; then
    echo ""
    return 0
  fi
  local out="$start"
  local i
  for (( i=start+1; i<=end; i++ )); do
    out="${out},${i}"
  done
  echo "$out"
}

manifest_tx_count() {
  local manifest_path="$1"
  sed -n 's/^[[:space:]]*"tx_count":[[:space:]]*\([0-9][0-9]*\),\{0,1\}$/\1/p' "$manifest_path" | head -n1
}

required_case_txs() {
  local offered_tps="$1"
  local warmup_blocks="$2"
  local steady_blocks="$3"
  local target_block_time_ms="$4"
  local blocks=$(( warmup_blocks + steady_blocks ))
  local numerator=$(( offered_tps * blocks * target_block_time_ms * 20 ))
  echo $(( (numerator + 9999) / 10000 ))
}

required_total_txs() {
  local offered_tps_csv="$1"
  local repeats="$2"
  local warmup_blocks="$3"
  local steady_blocks="$4"
  local target_block_time_ms="$5"

  local total=0
  local rate
  local per_case
  local rates=()
  IFS=',' read -r -a rates <<< "$offered_tps_csv"
  for rate in "${rates[@]}"; do
    rate="${rate// /}"
    [[ -z "$rate" ]] && continue
    if ! [[ "$rate" =~ ^[0-9]+$ ]] || [[ "$rate" -eq 0 ]]; then
      die "--offered-tps entries must be positive integers"
    fi
    per_case="$(required_case_txs "$rate" "$warmup_blocks" "$steady_blocks" "$target_block_time_ms")"
    total=$(( total + per_case * repeats ))
  done
  echo "$total"
}

wait_for_port() {
  local port="$1"
  local attempts="${2:-60}"
  local delay="${3:-1}"
  local i
  for i in $(seq 1 "$attempts"); do
    if nc -z -w1 127.0.0.1 "$port" 2>/dev/null; then
      return 0
    fi
    sleep "$delay"
  done
  return 1
}

current_height() {
  local out h
  out="$(curl -sf http://127.0.0.1:16657/status 2>/dev/null || true)"
  if [[ -z "$out" ]]; then
    echo ""
    return 0
  fi
  if command -v jq >/dev/null 2>&1; then
    h="$(printf '%s' "$out" | jq -r '.result.sync_info.latest_block_height // empty' 2>/dev/null || true)"
  else
    h="$(printf '%s' "$out" | sed -n 's/.*"latest_block_height"[[:space:]]*:[[:space:]]*"\{0,1\}\([0-9][0-9]*\)"\{0,1\}.*/\1/p' | head -n1)"
  fi
  if [[ "$h" =~ ^[0-9]+$ ]]; then
    echo "$h"
  else
    echo ""
  fi
}

meta_path_for() {
  local scenario="$1"
  case "$scenario" in
    unregulated) echo "$UNREG_META" ;;
    regulated) echo "$REG_META" ;;
    *) die "unknown scenario for metadata: $scenario" ;;
  esac
}

read_meta_number() {
  local file="$1"
  local key="$2"
  [[ -f "$file" ]] || { echo ""; return 0; }
  sed -n "s/^[[:space:]]*\"${key}\":[[:space:]]*\([0-9][0-9]*\),\{0,1\}$/\1/p" "$file" | head -n1
}

read_meta_string() {
  local file="$1"
  local key="$2"
  [[ -f "$file" ]] || { echo ""; return 0; }
  sed -n "s/^[[:space:]]*\"${key}\":[[:space:]]*\"\(.*\)\",\{0,1\}$/\1/p" "$file" | head -n1
}

write_lineage_metadata() {
  local scenario="$1"
  local source_indexes="$2"
  local operation="$3"
  local corpus_dir manifest tx_count meta_file h ts

  case "$scenario" in
    unregulated)
      corpus_dir="$UNREG_CORPUS_DIR"
      ;;
    regulated)
      corpus_dir="$REG_CORPUS_DIR"
      ;;
    *)
      die "unknown scenario: $scenario"
      ;;
  esac

  manifest="$corpus_dir/manifest.json"
  [[ -f "$manifest" ]] || die "missing manifest for metadata write: $manifest"
  tx_count="$(manifest_tx_count "$manifest")"
  [[ -n "$tx_count" ]] || die "cannot parse tx_count while writing metadata: $manifest"

  h="$(current_height)"
  [[ -n "$h" ]] || die "cannot read current chain height while writing metadata"
  ts="$(date +%s)"

  meta_file="$(meta_path_for "$scenario")"
  cat > "$meta_file" <<JSON
{
  "scenario": "$scenario",
  "base_snapshot": "$BASE_SNAPSHOT",
  "operation": "$operation",
  "built_height": $h,
  "built_at": $ts,
  "tx_count": $tx_count,
  "source_indexes": "$source_indexes"
}
JSON
}

ensure_local_up() {
  if nc -z -w1 127.0.0.1 16657 2>/dev/null && nc -z -w1 127.0.0.1 8080 2>/dev/null; then
    return 0
  fi

  echo "Services not reachable after restore, starting local node manually..."
  pkill pd 2>/dev/null || true
  pkill cometbft 2>/dev/null || true
  sleep 1

  mkdir -p "$REPO_ROOT/tmp"
  nohup cometbft start --home "$HOME/.penumbra/network_data/node0/cometbft" \
    > "$REPO_ROOT/tmp/bench-simple-cometbft.log" 2>&1 &
  sleep 2
  nohup "$PD_BIN" start --home "$HOME/.penumbra/network_data/node0/pd" \
    --cometbft-addr http://127.0.0.1:16657 \
    > "$REPO_ROOT/tmp/bench-simple-pd.log" 2>&1 &

  wait_for_port 16657 90 1 || die "cometbft did not become ready on 16657"
  wait_for_port 8080 90 1 || die "pd did not become ready on 8080"
}

source_env() {
  if [[ ! -f "$ENV_FILE" ]]; then
    die "missing env file: $ENV_FILE"
  fi
  # shellcheck disable=SC1090
  source "$ENV_FILE"
}

restore_snapshot_and_services() {
  local name="$1"
  "$SNAPSHOT_RESTORE_SCRIPT" --name "$name"
  ensure_local_up
  source_env
}

verify_corpora() {
  local scenario="${1:-both}"

  case "$scenario" in
    both|all)
      "$TPS_BIN" corpus verify --corpus "$UNREG_CORPUS_DIR" --observer "$OBSERVER_URL" >/dev/null
      "$TPS_BIN" corpus verify --corpus "$REG_CORPUS_DIR" --observer "$OBSERVER_URL" >/dev/null
      ;;
    regulated)
      "$TPS_BIN" corpus verify --corpus "$REG_CORPUS_DIR" --observer "$OBSERVER_URL" >/dev/null
      ;;
    unregulated)
      "$TPS_BIN" corpus verify --corpus "$UNREG_CORPUS_DIR" --observer "$OBSERVER_URL" >/dev/null
      ;;
    *)
      die "unknown verify scenario: $scenario (expected: both|regulated|unregulated)"
      ;;
  esac
}

ensure_run_summary_header() {
  if [[ ! -f "$RUN_SUMMARY_CSV" ]]; then
    cat > "$RUN_SUMMARY_CSV" <<'CSV'
timestamp,label,offered_tps,repeats,warmup_blocks,steady_blocks,target_block_time_ms,submit_workers,max_inflight,run_exit,overall_status,unreg_rows,unreg_peak_committed_tps,unreg_status,reg_rows,reg_peak_committed_tps,reg_status
CSV
  fi
}

append_minimal_run_summary() {
  local start_ts="$1"
  local run_exit="$2"
  local stats total_rows
  local u_rows u_peak u_status
  local r_rows r_peak r_status
  local now offered_safe label_safe overall_status

  ensure_run_summary_header

  if [[ ! -f "$TPS_RESULTS_CSV" ]]; then
    total_rows="0"
    u_rows="0"
    u_peak="na"
    u_status="na"
    r_rows="0"
    r_peak="na"
    r_status="na"
  else
    stats="$(awk -F',' -v label="$RUN_LABEL" -v start_ts="$start_ts" '
BEGIN {
  u_peak=-1;
  r_peak=-1;
}
NR==1 {
  for (i = 1; i <= NF; i++) {
    if ($i == "label") col_label = i;
    else if ($i == "scenario") col_scenario = i;
    else if ($i == "committed_tps") col_committed_tps = i;
    else if ($i == "run_status") col_run_status = i;
    else if ($i == "timestamp") col_timestamp = i;
  }
  next;
}
col_label > 0 && col_scenario > 0 && col_committed_tps > 0 && col_run_status > 0 && col_timestamp > 0 && $(col_label) == label && ($(col_timestamp) + 0) >= start_ts {
  total++;
  scenario=$(col_scenario);
  status=$(col_run_status);
  committed_tps=$(col_committed_tps) + 0;
  if (scenario == "unregulated") {
    u_rows++;
    if (u_peak < 0 || committed_tps > u_peak) u_peak = committed_tps;
    if (status != "ok") u_invalid++;
  } else if (scenario == "regulated") {
    r_rows++;
    if (r_peak < 0 || committed_tps > r_peak) r_peak = committed_tps;
    if (status != "ok") r_invalid++;
  }
}
END {
  if (u_peak < 0) u_peak_out = "na"; else u_peak_out = sprintf("%.6f", u_peak);
  if (r_peak < 0) r_peak_out = "na"; else r_peak_out = sprintf("%.6f", r_peak);
  if (u_rows == 0) u_status = "na"; else if (u_invalid == 0) u_status = "ok"; else u_status = "invalid";
  if (r_rows == 0) r_status = "na"; else if (r_invalid == 0) r_status = "ok"; else r_status = "invalid";
  printf "%d|%d|%s|%s|%d|%s|%s", total, u_rows, u_peak_out, u_status, r_rows, r_peak_out, r_status;
}
' "$TPS_RESULTS_CSV")"

    IFS='|' read -r total_rows u_rows u_peak u_status r_rows r_peak r_status <<< "$stats"

    total_rows="${total_rows:-0}"
    u_rows="${u_rows:-0}"
    u_peak="${u_peak:-na}"
    u_status="${u_status:-na}"
    r_rows="${r_rows:-0}"
    r_peak="${r_peak:-na}"
    r_status="${r_status:-na}"
  fi

  overall_status="invalid"
  if [[ "$run_exit" -eq 0 && "$total_rows" -gt 0 && "$u_status" != "invalid" && "$r_status" != "invalid" ]]; then
    overall_status="ok"
  fi

  now="$(date +%s)"
  offered_safe="${OFFERED_TPS_CSV//,/;}"
  label_safe="${RUN_LABEL//,/;}"

  echo "${now},${label_safe},${offered_safe},${REPEATS},${WARMUP_BLOCKS},${STEADY_BLOCKS},${TARGET_BLOCK_TIME_MS},${SUBMIT_WORKERS},${MAX_INFLIGHT},${run_exit},${overall_status},${u_rows},${u_peak},${u_status},${r_rows},${r_peak},${r_status}" >> "$RUN_SUMMARY_CSV"
  echo "Updated $RUN_SUMMARY_CSV"
}

ensure_lineage_not_stale() {
  local cur_h meta_h drift meta_file
  cur_h="$(current_height)"
  [[ -n "$cur_h" ]] || die "cannot read current height for drift check"

  for scenario in unregulated regulated; do
    meta_file="$(meta_path_for "$scenario")"
    if [[ ! -f "$meta_file" ]]; then
      echo "missing lineage metadata: $meta_file"
      return 10
    fi
    meta_h="$(read_meta_number "$meta_file" built_height)"
    if [[ -z "$meta_h" ]]; then
      echo "invalid lineage metadata (missing built_height): $meta_file"
      return 10
    fi

    if [[ "$cur_h" -ge "$meta_h" ]]; then
      drift=$(( cur_h - meta_h ))
    else
      drift=0
    fi

    if [[ "$drift" -gt "$MAX_HEIGHT_DRIFT" ]]; then
      echo "${scenario} corpus drift too high: current_height=${cur_h}, built_height=${meta_h}, drift=${drift}, max=${MAX_HEIGHT_DRIFT}"
      return 10
    fi
  done

  return 0
}

write_run_config() {
  local label="$1"
  cat > "$RUN_CONFIG_OUT" <<YAML
label: "${label}"
pd_endpoints:
  - "${OBSERVER_URL}"
observer_endpoint: "${OBSERVER_URL}"
profile: "regression"
target_block_time_ms: ${TARGET_BLOCK_TIME_MS}
scenarios:
  - name: "unregulated"
    corpus_dir: "${UNREG_CORPUS_DIR}"
    offered_tps: [${OFFERED_TPS_CSV}]
    repeats: ${REPEATS}
    warmup_blocks: ${WARMUP_BLOCKS}
    steady_blocks: ${STEADY_BLOCKS}
    submit_workers: ${SUBMIT_WORKERS}
    max_inflight: ${MAX_INFLIGHT}
  - name: "regulated"
    corpus_dir: "${REG_CORPUS_DIR}"
    offered_tps: [${OFFERED_TPS_CSV}]
    repeats: ${REPEATS}
    warmup_blocks: ${WARMUP_BLOCKS}
    steady_blocks: ${STEADY_BLOCKS}
    submit_workers: ${SUBMIT_WORKERS}
    max_inflight: ${MAX_INFLIGHT}
stability:
  max_reject_rate_pct: 100.0
  max_p95_latency_ms: 600000
  max_backlog_growth_pct: 100000.0
  min_steady_commits: 1
YAML
}

check_binaries() {
  local missing=0
  if [[ ! -x "$PCLI_BIN" ]]; then
    echo "Missing executable: $PCLI_BIN" >&2
    missing=1
  fi
  if [[ ! -x "$PD_BIN" ]]; then
    echo "Missing executable: $PD_BIN" >&2
    missing=1
  fi
  if [[ ! -x "$TPS_BIN" ]]; then
    echo "Missing executable: $TPS_BIN" >&2
    missing=1
  fi
  if ! command -v cometbft >/dev/null 2>&1; then
    echo "Missing cometbft in PATH" >&2
    missing=1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Missing cargo in PATH" >&2
    missing=1
  fi
  if [[ "$missing" -ne 0 ]]; then
    die "build required tools first"
  fi
}

build_prepare_corpora() {
  local unreg_indexes reg_indexes
  unreg_indexes="$(range_csv 0 $((UNREG_COUNT - 1)))"
  reg_indexes="$(range_csv 0 $((REG_COUNT - 1)))"

  if [[ "$SKIP_FUNDING" -eq 0 ]]; then
    if [[ "$UNREG_COUNT" -gt 1 ]]; then
      local unreg_fund
      unreg_fund="$(range_csv 1 $((UNREG_COUNT - 1)))"
      "$FUND_SCRIPT" \
        --pd-url "$OBSERVER_URL" \
        --wallet-home "$ALICE_HOME" \
        --indexes "$unreg_fund" \
        --asset "$ASSET_UNREG" \
        --asset-amount "$ASSET_AMOUNT" \
        --fee-amount "$FEE_AMOUNT" \
        --chunk-size "$FUND_CHUNK_SIZE"
    fi

    "$PREPARE_REGULATED_SCRIPT" \
      --pd-url "$OBSERVER_URL" \
      --wallet-home "$ALICE_HOME" \
      --asset "$ASSET_REG" \
      --address-count "$REG_COUNT" \
      --wait-seconds 1 \
      --allow-existing

    if [[ "$REG_COUNT" -gt 1 ]]; then
      local reg_fund
      reg_fund="$(range_csv 1 $((REG_COUNT - 1)))"
      "$FUND_SCRIPT" \
        --pd-url "$OBSERVER_URL" \
        --wallet-home "$ALICE_HOME" \
        --indexes "$reg_fund" \
        --asset "$ASSET_REG" \
        --asset-amount "$ASSET_AMOUNT" \
        --fee-amount "$FEE_AMOUNT" \
        --chunk-size "$FUND_CHUNK_SIZE"
    fi
  fi

  rm -rf "$UNREG_CORPUS_DIR" "$REG_CORPUS_DIR"
  mkdir -p "$CORPUS_ROOT"

  "$BUILD_CORPUS_SCRIPT" \
    --scenario unregulated \
    --pd-url "$OBSERVER_URL" \
    --wallet-home "$ALICE_HOME" \
    --asset "$ASSET_UNREG" \
    --count "$UNREG_COUNT" \
    --source-indexes "$unreg_indexes" \
    --to-address "$BOB_ADDRESS" \
    --source-label persistent \
    --notes "prepare-${UNREG_COUNT}" \
    --out "$UNREG_CORPUS_DIR"

  "$BUILD_CORPUS_SCRIPT" \
    --scenario regulated \
    --pd-url "$OBSERVER_URL" \
    --wallet-home "$ALICE_HOME" \
    --asset "$ASSET_REG" \
    --count "$REG_COUNT" \
    --source-indexes "$reg_indexes" \
    --to-address "$ALICE_ADDRESS" \
    --source-label persistent \
    --notes "prepare-${REG_COUNT}" \
    --out "$REG_CORPUS_DIR"

  write_lineage_metadata "unregulated" "$unreg_indexes" "prepare"
  write_lineage_metadata "regulated" "$reg_indexes" "prepare"
}

append_corpora() {
  local unreg_manifest reg_manifest
  local cur_unreg cur_reg
  local unreg_start unreg_end reg_start reg_end
  local unreg_indexes reg_indexes

  unreg_manifest="$UNREG_CORPUS_DIR/manifest.json"
  reg_manifest="$REG_CORPUS_DIR/manifest.json"
  [[ -f "$unreg_manifest" ]] || die "missing unregulated manifest: $unreg_manifest"
  [[ -f "$reg_manifest" ]] || die "missing regulated manifest: $reg_manifest"

  cur_unreg="$(manifest_tx_count "$unreg_manifest")"
  cur_reg="$(manifest_tx_count "$reg_manifest")"
  [[ -n "$cur_unreg" ]] || die "cannot parse unregulated tx_count"
  [[ -n "$cur_reg" ]] || die "cannot parse regulated tx_count"

  unreg_start="$cur_unreg"
  unreg_end=$(( cur_unreg + APPEND_COUNT - 1 ))
  reg_start="$cur_reg"
  reg_end=$(( cur_reg + APPEND_COUNT - 1 ))

  unreg_indexes="$(range_csv "$unreg_start" "$unreg_end")"
  reg_indexes="$(range_csv "$reg_start" "$reg_end")"

  if [[ "$SKIP_FUNDING" -eq 0 ]]; then
    "$FUND_SCRIPT" \
      --pd-url "$OBSERVER_URL" \
      --wallet-home "$ALICE_HOME" \
      --indexes "$unreg_indexes" \
      --asset "$ASSET_UNREG" \
      --asset-amount "$ASSET_AMOUNT" \
      --fee-amount "$FEE_AMOUNT" \
      --chunk-size "$FUND_CHUNK_SIZE"

    "$PREPARE_REGULATED_SCRIPT" \
      --pd-url "$OBSERVER_URL" \
      --wallet-home "$ALICE_HOME" \
      --asset "$ASSET_REG" \
      --address-count $((reg_end + 1)) \
      --wait-seconds 1 \
      --allow-existing

    "$FUND_SCRIPT" \
      --pd-url "$OBSERVER_URL" \
      --wallet-home "$ALICE_HOME" \
      --indexes "$reg_indexes" \
      --asset "$ASSET_REG" \
      --asset-amount "$ASSET_AMOUNT" \
      --fee-amount "$FEE_AMOUNT" \
      --chunk-size "$FUND_CHUNK_SIZE"
  fi

  "$BUILD_CORPUS_SCRIPT" \
    --append \
    --scenario unregulated \
    --pd-url "$OBSERVER_URL" \
    --wallet-home "$ALICE_HOME" \
    --asset "$ASSET_UNREG" \
    --count "$APPEND_COUNT" \
    --source-indexes "$unreg_indexes" \
    --to-address "$BOB_ADDRESS" \
    --source-label persistent-append \
    --notes "append-${APPEND_COUNT}" \
    --out "$UNREG_CORPUS_DIR"

  "$BUILD_CORPUS_SCRIPT" \
    --append \
    --scenario regulated \
    --pd-url "$OBSERVER_URL" \
    --wallet-home "$ALICE_HOME" \
    --asset "$ASSET_REG" \
    --count "$APPEND_COUNT" \
    --source-indexes "$reg_indexes" \
    --to-address "$ALICE_ADDRESS" \
    --source-label persistent-append \
    --notes "append-${APPEND_COUNT}" \
    --out "$REG_CORPUS_DIR"

  write_lineage_metadata "unregulated" "$unreg_indexes" "append"
  write_lineage_metadata "regulated" "$reg_indexes" "append"
}

prepare_or_refresh() {
  local mode="$1"
  local mode_label="$mode"

  case "$mode" in
    prepare) mode_label="Prepare" ;;
    refresh) mode_label="Refresh" ;;
  esac

  if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p penumbra-sdk-bench -p pcli -p pd
  fi

  restore_snapshot_and_services "$SEED_SNAPSHOT"
  build_prepare_corpora
  verify_corpora
  "$SNAPSHOT_SAVE_SCRIPT" --name "$BASE_SNAPSHOT"

  echo "${mode_label} complete."
  echo "  base snapshot: $BASE_SNAPSHOT"
  echo "  corpora: $UNREG_CORPUS_DIR, $REG_CORPUS_DIR"
}

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SNAPSHOT_RESTORE_SCRIPT="$REPO_ROOT/scripts/tps/snapshot-restore.sh"
SNAPSHOT_SAVE_SCRIPT="$REPO_ROOT/scripts/tps/snapshot-save.sh"
BUILD_CORPUS_SCRIPT="$REPO_ROOT/scripts/tps/build-corpus.sh"
FUND_SCRIPT="$REPO_ROOT/scripts/tps/fund-sources.sh"
PREPARE_REGULATED_SCRIPT="$REPO_ROOT/scripts/tps/prepare-regulated-fixture.sh"

CORPUS_ROOT="$REPO_ROOT/crates/bench/benches/compliance/tps/corpus"
UNREG_CORPUS_DIR="$CORPUS_ROOT/unregulated"
REG_CORPUS_DIR="$CORPUS_ROOT/regulated"
UNREG_META="$UNREG_CORPUS_DIR/lineage.json"
REG_META="$REG_CORPUS_DIR/lineage.json"
RUN_CONFIG_OUT="$REPO_ROOT/tmp/tps-persistent.config.yaml"
ENV_FILE="$REPO_ROOT/tmp/compliance-demo.env"
OBSERVER_URL="http://127.0.0.1:8080"
TPS_RESULTS_CSV="$REPO_ROOT/crates/bench/benches/compliance/tps/tps.csv"
RUN_SUMMARY_CSV="$REPO_ROOT/crates/bench/benches/compliance/tps/run_summary.csv"

PCLI_BIN="${PCLI_BIN:-$REPO_ROOT/target/release/pcli}"
PD_BIN="${PD_BIN:-$REPO_ROOT/target/release/pd}"
TPS_BIN="${TPS_BIN:-$REPO_ROOT/target/release/compliance_tps}"

SEED_SNAPSHOT="local-quick-fixed"
BASE_SNAPSHOT="local-tps-persistent"
ASSET_UNREG="upenumbra"
ASSET_REG="test_usd"
ASSET_AMOUNT="1000"
FEE_AMOUNT="1000"
FUND_CHUNK_SIZE="16"
UNREG_COUNT="12"
REG_COUNT="12"
APPEND_COUNT="4"
RUN_LABEL="persistent-bench"
OFFERED_TPS_CSV="1"
REPEATS="1"
WARMUP_BLOCKS="2"
STEADY_BLOCKS="8"
TARGET_BLOCK_TIME_MS="500"
SUBMIT_WORKERS="1"
MAX_INFLIGHT="2"
MAX_HEIGHT_DRIFT="200"
VERIFY_SCENARIO="both"
AUTO_REFRESH=0
SKIP_BUILD=0
SKIP_FUNDING=0

if [[ $# -lt 1 ]]; then
  usage
  exit 1
fi

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

COMMAND="$1"
shift

while [[ $# -gt 0 ]]; do
  case "$1" in
    --seed-snapshot) require_arg_value "$1" "$#" "${2-}"; SEED_SNAPSHOT="$2"; shift 2 ;;
    --base-snapshot) require_arg_value "$1" "$#" "${2-}"; BASE_SNAPSHOT="$2"; shift 2 ;;
    --asset-unreg) require_arg_value "$1" "$#" "${2-}"; ASSET_UNREG="$2"; shift 2 ;;
    --asset-reg) require_arg_value "$1" "$#" "${2-}"; ASSET_REG="$2"; shift 2 ;;
    --asset-amount) require_arg_value "$1" "$#" "${2-}"; ASSET_AMOUNT="$2"; shift 2 ;;
    --fee-amount) require_arg_value "$1" "$#" "${2-}"; FEE_AMOUNT="$2"; shift 2 ;;
    --fund-chunk-size) require_arg_value "$1" "$#" "${2-}"; FUND_CHUNK_SIZE="$2"; shift 2 ;;
    --unreg-count) require_arg_value "$1" "$#" "${2-}"; UNREG_COUNT="$2"; shift 2 ;;
    --reg-count) require_arg_value "$1" "$#" "${2-}"; REG_COUNT="$2"; shift 2 ;;
    --append-count|--count) require_arg_value "$1" "$#" "${2-}"; APPEND_COUNT="$2"; shift 2 ;;
    --run-label) require_arg_value "$1" "$#" "${2-}"; RUN_LABEL="$2"; shift 2 ;;
    --offered-tps) require_arg_value "$1" "$#" "${2-}"; OFFERED_TPS_CSV="$2"; shift 2 ;;
    --repeats) require_arg_value "$1" "$#" "${2-}"; REPEATS="$2"; shift 2 ;;
    --warmup-blocks) require_arg_value "$1" "$#" "${2-}"; WARMUP_BLOCKS="$2"; shift 2 ;;
    --steady-blocks) require_arg_value "$1" "$#" "${2-}"; STEADY_BLOCKS="$2"; shift 2 ;;
    --target-block-time-ms) require_arg_value "$1" "$#" "${2-}"; TARGET_BLOCK_TIME_MS="$2"; shift 2 ;;
    --submit-workers) require_arg_value "$1" "$#" "${2-}"; SUBMIT_WORKERS="$2"; shift 2 ;;
    --max-inflight) require_arg_value "$1" "$#" "${2-}"; MAX_INFLIGHT="$2"; shift 2 ;;
    --max-height-drift) require_arg_value "$1" "$#" "${2-}"; MAX_HEIGHT_DRIFT="$2"; shift 2 ;;
    --verify-scenario) require_arg_value "$1" "$#" "${2-}"; VERIFY_SCENARIO="$2"; shift 2 ;;
    --auto-refresh) AUTO_REFRESH=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --skip-funding) SKIP_FUNDING=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown arg: $1" ;;
  esac
done

require_pos_int "--asset-amount" "$ASSET_AMOUNT"
require_pos_int "--fee-amount" "$FEE_AMOUNT"
require_pos_int "--fund-chunk-size" "$FUND_CHUNK_SIZE"
require_pos_int "--unreg-count" "$UNREG_COUNT"
require_pos_int "--reg-count" "$REG_COUNT"
require_pos_int "--append-count" "$APPEND_COUNT"
require_pos_int "--repeats" "$REPEATS"
require_pos_int "--warmup-blocks" "$WARMUP_BLOCKS"
require_pos_int "--steady-blocks" "$STEADY_BLOCKS"
require_pos_int "--target-block-time-ms" "$TARGET_BLOCK_TIME_MS"
require_pos_int "--submit-workers" "$SUBMIT_WORKERS"
require_pos_int "--max-inflight" "$MAX_INFLIGHT"
require_pos_int "--max-height-drift" "$MAX_HEIGHT_DRIFT"

[[ "$MAX_INFLIGHT" -ge "$SUBMIT_WORKERS" ]] || die "--max-inflight must be >= --submit-workers"
case "$VERIFY_SCENARIO" in
  both|regulated|unregulated) ;;
  *) die "--verify-scenario must be one of: both|regulated|unregulated" ;;
esac

[[ -f "$SNAPSHOT_RESTORE_SCRIPT" ]] || die "missing script: $SNAPSHOT_RESTORE_SCRIPT"
[[ -f "$SNAPSHOT_SAVE_SCRIPT" ]] || die "missing script: $SNAPSHOT_SAVE_SCRIPT"
[[ -f "$BUILD_CORPUS_SCRIPT" ]] || die "missing script: $BUILD_CORPUS_SCRIPT"
[[ -f "$FUND_SCRIPT" ]] || die "missing script: $FUND_SCRIPT"
[[ -f "$PREPARE_REGULATED_SCRIPT" ]] || die "missing script: $PREPARE_REGULATED_SCRIPT"

check_binaries

case "$COMMAND" in
  prepare)
    prepare_or_refresh "prepare"
    ;;

  refresh)
    prepare_or_refresh "refresh"
    ;;

  run)
    restore_snapshot_and_services "$BASE_SNAPSHOT"
    verify_corpora

    if ! ensure_lineage_not_stale; then
      if [[ "$AUTO_REFRESH" -eq 1 ]]; then
        echo "Drift gate failed; auto-refreshing corpus + base snapshot..."
        prepare_or_refresh "refresh"
        restore_snapshot_and_services "$BASE_SNAPSHOT"
        verify_corpora
        ensure_lineage_not_stale || die "drift gate still failing after auto-refresh"
      else
        die "corpus lineage drift gate failed; run './scripts/tps/bench-simple.sh refresh'"
      fi
    fi

    required_total="$(required_total_txs "$OFFERED_TPS_CSV" "$REPEATS" "$WARMUP_BLOCKS" "$STEADY_BLOCKS" "$TARGET_BLOCK_TIME_MS")"
    unreg_cur="$(manifest_tx_count "$UNREG_CORPUS_DIR/manifest.json")"
    reg_cur="$(manifest_tx_count "$REG_CORPUS_DIR/manifest.json")"
    [[ -n "$unreg_cur" ]] || die "cannot parse unregulated tx_count"
    [[ -n "$reg_cur" ]] || die "cannot parse regulated tx_count"
    if [[ "$unreg_cur" -lt "$required_total" ]]; then
      die "unregulated corpus too small ($unreg_cur < $required_total); run append first"
    fi
    if [[ "$reg_cur" -lt "$required_total" ]]; then
      die "regulated corpus too small ($reg_cur < $required_total); run append first"
    fi

    write_run_config "$RUN_LABEL"
    run_started_at="$(date +%s)"
    set +e
    "$TPS_BIN" run --config "$RUN_CONFIG_OUT"
    run_exit="$?"
    set -e
    append_minimal_run_summary "$run_started_at" "$run_exit"

    if [[ "$run_exit" -ne 0 ]]; then
      die "benchmark run failed with exit code ${run_exit} (summary appended: $RUN_SUMMARY_CSV)"
    fi

    echo "Benchmark run complete."
    echo "  config: $RUN_CONFIG_OUT"
    echo "  results: $TPS_RESULTS_CSV"
    echo "  summary: $RUN_SUMMARY_CSV"
    ;;

  append)
    restore_snapshot_and_services "$BASE_SNAPSHOT"
    append_corpora
    verify_corpora
    "$SNAPSHOT_SAVE_SCRIPT" --name "$BASE_SNAPSHOT"

    new_unreg="$(manifest_tx_count "$UNREG_CORPUS_DIR/manifest.json")"
    new_reg="$(manifest_tx_count "$REG_CORPUS_DIR/manifest.json")"
    echo "Append complete and snapshot updated."
    echo "  unregulated tx_count: $new_unreg"
    echo "  regulated tx_count:   $new_reg"
    echo "  base snapshot:        $BASE_SNAPSHOT"
    ;;

  verify)
    restore_snapshot_and_services "$BASE_SNAPSHOT"
    verify_corpora "$VERIFY_SCENARIO"
    echo "Corpus verify complete."
    echo "  scenario: $VERIFY_SCENARIO"
    echo "  observer: $OBSERVER_URL"
    ;;

  status)
    base_snapshot_dir="$REPO_ROOT/crates/bench/benches/compliance/tps/snapshots/$BASE_SNAPSHOT"
    seed_snapshot_dir="$REPO_ROOT/crates/bench/benches/compliance/tps/snapshots/$SEED_SNAPSHOT"
    unreg_count="missing"
    reg_count="missing"

    if [[ -f "$UNREG_CORPUS_DIR/manifest.json" ]]; then
      unreg_count="$(manifest_tx_count "$UNREG_CORPUS_DIR/manifest.json")"
    fi
    if [[ -f "$REG_CORPUS_DIR/manifest.json" ]]; then
      reg_count="$(manifest_tx_count "$REG_CORPUS_DIR/manifest.json")"
    fi

    echo "seed_snapshot_exists=$([[ -d "$seed_snapshot_dir" ]] && echo yes || echo no)"
    echo "base_snapshot_exists=$([[ -d "$base_snapshot_dir" ]] && echo yes || echo no)"
    echo "unregulated_tx_count=${unreg_count}"
    echo "regulated_tx_count=${reg_count}"

    if [[ -f "$UNREG_META" ]]; then
      echo "unregulated_built_height=$(read_meta_number "$UNREG_META" built_height)"
      echo "unregulated_built_at=$(read_meta_number "$UNREG_META" built_at)"
      echo "unregulated_source_indexes=$(read_meta_string "$UNREG_META" source_indexes)"
    else
      echo "unregulated_meta=missing"
    fi
    if [[ -f "$REG_META" ]]; then
      echo "regulated_built_height=$(read_meta_number "$REG_META" built_height)"
      echo "regulated_built_at=$(read_meta_number "$REG_META" built_at)"
      echo "regulated_source_indexes=$(read_meta_string "$REG_META" source_indexes)"
    else
      echo "regulated_meta=missing"
    fi

    cur_h="$(current_height)"
    echo "current_height=${cur_h:-unknown}"
    ;;

  *)
    die "unknown command: $COMMAND (expected: prepare|run|append|refresh|verify|status)"
    ;;
esac
