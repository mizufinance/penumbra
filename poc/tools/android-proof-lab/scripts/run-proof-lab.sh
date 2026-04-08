#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Build, push, and run android_proof_lab on an Android device.

The transfer benchmark compares prover-side cost for a fixed `2x2` unregulated
transfer witness. Each run proves through gnark, translates the returned proof,
then verifies it with the Rust verifier for correctness.

Prerequisites:
  - rustup target add aarch64-linux-android
  - cargo install cargo-ndk
  - Android NDK available to cargo-ndk
  - adb installed and one device connected

Options:
  --bin <name>                 binary name in package android_proof_lab
  --target <triple>            default: aarch64-linux-android
  --abi <abi>                  default: arm64-v8a
  --remote-dir <path>          default: /data/local/tmp/penumbra-bench
  --serial <device-serial>     adb device serial to target
  --mode <debug|release|android-prof>
                               default: release
  --cargo-features <list>      comma-separated cargo features to enable
  --profile-mode <none|stage|simpleperf|both>
                               default: none
  --rayon-threads <N>          set RAYON_NUM_THREADS on device
  --cpu-mask <mask>            run the benchmark via taskset on device
  --repeat <N>                 repeat the benchmark invocation N times
  --output <remote-file>       remote output file path
  --local-output <path>        default: tmp/android_proof_lab.json
  --simpleperf-output <path>   local perf.data path for the first profiled run
  --gnark-lib-local <path>     local path for libpenumbra_gnark_transfer.so
                               default: tmp/gnark/libpenumbra_gnark_transfer.so
  --gnark-artifact-dir-local <path>
                               local gnark transfer artifact dir
                               default: tools/gnark/artifacts/transfer2x2
  --gnark-lib-remote <path>    remote path for libpenumbra_gnark_transfer.so
                               default: <remote-dir>/libpenumbra_gnark_transfer.so
  --gnark-artifact-dir-remote <path>
                               remote gnark transfer artifact dir
                               default: <remote-dir>/gnark-transfer2x2
  --skip-gnark-build           skip Go Android shared-library build for gnark transfer backend
  --skip-build                 skip cargo ndk build
  --skip-push                  skip adb push
  --skip-run                   skip adb shell execution
  --skip-pull                  skip adb pull of result artifacts
  --help                       show this help

All remaining arguments after `--` are passed directly to android_proof_lab.

Examples:
  scripts/android/run-proof-lab.sh \
    --profile-mode stage -- \
    --backend gnark \
    --circuit transfer2x2 \
    --compliance-case unregulated \
    --cold-iterations 3 \
    --warm-iterations 5 \
    --format json

  scripts/android/run-proof-lab.sh \
    --profile-mode both \
    --cpu-mask f0 \
    --simpleperf-output tmp/output-simpleperf.perf.data -- \
    --backend gnark \
    --circuit transfer2x2 \
    --compliance-case unregulated \
    --cold-iterations 3 \
    --warm-iterations 5 \
    --format json
EOF
}

die() {
  echo "Error: $*" >&2
  exit 1
}

require_arg_value() {
  local flag="$1"
  local argc="$2"
  local next="${3:-}"
  if [[ "$argc" -lt 2 || -z "$next" || "$next" == -* ]]; then
    die "${flag} requires a value"
  fi
}

find_host_simpleperf() {
  if command -v simpleperf >/dev/null 2>&1; then
    command -v simpleperf
    return 0
  fi

  local candidates=()
  if [[ -n "${ANDROID_NDK_HOME:-}" ]]; then
    candidates+=(
      "$ANDROID_NDK_HOME/simpleperf/bin/darwin-arm64/simpleperf"
      "$ANDROID_NDK_HOME/simpleperf/bin/darwin-x86_64/simpleperf"
      "$ANDROID_NDK_HOME/simpleperf/bin/darwin/x86_64/simpleperf"
      "$ANDROID_NDK_HOME/simpleperf/bin/darwin/arm64/simpleperf"
      "$ANDROID_NDK_HOME/simpleperf"
      "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/simpleperf"
      "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-arm64/bin/simpleperf"
    )
  fi

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -f "$candidate" && -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

with_index_suffix() {
  local path="$1"
  local idx="$2"
  local repeat="$3"

  if [[ "$repeat" -eq 1 ]]; then
    printf '%s\n' "$path"
    return 0
  fi

  local dir base stem ext
  dir="$(dirname "$path")"
  base="$(basename "$path")"

  if [[ "$base" == *.* ]]; then
    stem="${base%.*}"
    ext=".${base##*.}"
  else
    stem="$base"
    ext=""
  fi

  printf '%s/%s-run%02d%s\n' "$dir" "$stem" "$idx" "$ext"
}

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
POC_ROOT="$REPO_ROOT/poc"
PACKAGE_DIR="$REPO_ROOT/poc/tools/android-proof-lab"
cd "$REPO_ROOT"

BIN_NAME="android_proof_lab"
TARGET="aarch64-linux-android"
ABI="arm64-v8a"
REMOTE_DIR="/data/local/tmp/penumbra-bench"
SERIAL=""
MODE="release"
PACKAGE_NAME="android_proof_lab"
BUILD_FEATURES=""
PROFILE_MODE="none"
RAYON_THREADS=""
CPU_MASK=""
REPEAT=1
OUTPUT=""
LOCAL_OUTPUT=""
SIMPLEPERF_OUTPUT=""
GNARK_LIB_LOCAL="tmp/gnark/libpenumbra_gnark_transfer.so"
GNARK_ARTIFACT_DIR_LOCAL="tools/gnark/artifacts/transfer2x2"
GNARK_LIB_REMOTE=""
GNARK_ARTIFACT_DIR_REMOTE=""
SKIP_GNARK_BUILD=0
SKIP_BUILD=0
SKIP_PUSH=0
SKIP_RUN=0
SKIP_PULL=0
LAB_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) require_arg_value "$1" "$#" "${2-}"; BIN_NAME="$2"; shift 2 ;;
    --target) require_arg_value "$1" "$#" "${2-}"; TARGET="$2"; shift 2 ;;
    --abi) require_arg_value "$1" "$#" "${2-}"; ABI="$2"; shift 2 ;;
    --remote-dir) require_arg_value "$1" "$#" "${2-}"; REMOTE_DIR="$2"; shift 2 ;;
    --serial) require_arg_value "$1" "$#" "${2-}"; SERIAL="$2"; shift 2 ;;
    --mode) require_arg_value "$1" "$#" "${2-}"; MODE="$2"; shift 2 ;;
    --cargo-features) require_arg_value "$1" "$#" "${2-}"; BUILD_FEATURES="$2"; shift 2 ;;
    --profile-mode) require_arg_value "$1" "$#" "${2-}"; PROFILE_MODE="$2"; shift 2 ;;
    --rayon-threads) require_arg_value "$1" "$#" "${2-}"; RAYON_THREADS="$2"; shift 2 ;;
    --cpu-mask) require_arg_value "$1" "$#" "${2-}"; CPU_MASK="$2"; shift 2 ;;
    --repeat) require_arg_value "$1" "$#" "${2-}"; REPEAT="$2"; shift 2 ;;
    --output) require_arg_value "$1" "$#" "${2-}"; OUTPUT="$2"; shift 2 ;;
    --local-output) require_arg_value "$1" "$#" "${2-}"; LOCAL_OUTPUT="$2"; shift 2 ;;
    --simpleperf-output) require_arg_value "$1" "$#" "${2-}"; SIMPLEPERF_OUTPUT="$2"; shift 2 ;;
    --gnark-lib-local) require_arg_value "$1" "$#" "${2-}"; GNARK_LIB_LOCAL="$2"; shift 2 ;;
    --gnark-artifact-dir-local) require_arg_value "$1" "$#" "${2-}"; GNARK_ARTIFACT_DIR_LOCAL="$2"; shift 2 ;;
    --gnark-lib-remote) require_arg_value "$1" "$#" "${2-}"; GNARK_LIB_REMOTE="$2"; shift 2 ;;
    --gnark-artifact-dir-remote) require_arg_value "$1" "$#" "${2-}"; GNARK_ARTIFACT_DIR_REMOTE="$2"; shift 2 ;;
    --skip-gnark-build) SKIP_GNARK_BUILD=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --skip-push) SKIP_PUSH=1; shift ;;
    --skip-run) SKIP_RUN=1; shift ;;
    --skip-pull) SKIP_PULL=1; shift ;;
    -h|--help) usage; exit 0 ;;
    --)
      shift
      LAB_ARGS=("$@")
      break
      ;;
    *)
      die "unknown arg: $1"
      ;;
  esac
done

case "$MODE" in
  debug|release|android-prof) ;;
  *) die "--mode must be one of: debug, release, android-prof" ;;
esac

case "$PROFILE_MODE" in
  none|stage|simpleperf|both) ;;
  *) die "--profile-mode must be one of: none, stage, simpleperf, both" ;;
esac

[[ "$REPEAT" =~ ^[0-9]+$ ]] || die "--repeat must be a positive integer"
(( REPEAT > 0 )) || die "--repeat must be > 0"

if [[ "${#LAB_ARGS[@]}" -eq 0 ]]; then
  LAB_ARGS=(
    --backend gnark
    --circuit transfer2x2
    --compliance-case unregulated
    --cold-iterations 3
    --warm-iterations 5
    --format json
  )
fi

ADB=(adb)
if [[ -n "$SERIAL" ]]; then
  ADB+=( -s "$SERIAL" )
fi

if [[ -z "${ANDROID_NDK_HOME:-}" && -d "/opt/homebrew/share/android-ndk" ]]; then
  export ANDROID_NDK_HOME="/opt/homebrew/share/android-ndk"
fi
if [[ -z "${ANDROID_NDK_ROOT:-}" && -n "${ANDROID_NDK_HOME:-}" ]]; then
  export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
fi

if [[ "$PROFILE_MODE" == "simpleperf" || "$PROFILE_MODE" == "both" ]]; then
  MODE="android-prof"
fi

if [[ -z "$LOCAL_OUTPUT" ]]; then
  LOCAL_OUTPUT="tmp/${BIN_NAME}.json"
fi

resolve_local_path() {
  local path="$1"
  if [[ "$path" = /* ]]; then
    printf '%s\n' "$path"
  else
    printf '%s/%s\n' "$REPO_ROOT" "$path"
  fi
}

detect_gnark_backend() {
  local idx arg next
  for ((idx = 0; idx < ${#LAB_ARGS[@]}; idx++)); do
    arg="${LAB_ARGS[$idx]}"
    next="${LAB_ARGS[$((idx + 1))]:-}"
    if [[ "$arg" == "--backend" && "$next" == "gnark" ]]; then
      return 0
    fi
    if [[ "$arg" == "--backend=gnark" ]]; then
      return 0
    fi
  done
  return 1
}

lab_args_contains_flag() {
  local needle="$1"
  local idx arg next
  for ((idx = 0; idx < ${#LAB_ARGS[@]}; idx++)); do
    arg="${LAB_ARGS[$idx]}"
    next="${LAB_ARGS[$((idx + 1))]:-}"
    if [[ "$arg" == "$needle" ]]; then
      return 0
    fi
    if [[ "$arg" == "${needle}="* ]]; then
      return 0
    fi
    if [[ "$needle" == "--backend" && "$arg" == "--backend" && -n "$next" ]]; then
      return 0
    fi
  done
  return 1
}

find_ndk_clang() {
  local ndk_home="$1"
  local candidates=(
    "$ndk_home/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android24-clang"
    "$ndk_home/toolchains/llvm/prebuilt/darwin-arm64/bin/aarch64-linux-android24-clang"
  )
  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

file_size_bytes() {
  local path="$1"
  wc -c < "$path" | tr -d ' '
}

GNARK_BACKEND=0
if detect_gnark_backend; then
  GNARK_BACKEND=1
fi

if [[ -z "$GNARK_LIB_REMOTE" ]]; then
  GNARK_LIB_REMOTE="$REMOTE_DIR/libpenumbra_gnark_transfer.so"
fi
if [[ -z "$GNARK_ARTIFACT_DIR_REMOTE" ]]; then
  GNARK_ARTIFACT_DIR_REMOTE="$REMOTE_DIR/gnark-transfer2x2"
fi

GNARK_LIB_LOCAL_ABS="$(resolve_local_path "$GNARK_LIB_LOCAL")"
GNARK_ARTIFACT_DIR_LOCAL_ABS="$(resolve_local_path "$GNARK_ARTIFACT_DIR_LOCAL")"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  command -v cargo-ndk >/dev/null 2>&1 || die "cargo-ndk is not installed; run: cargo install cargo-ndk"
  rustup target list --installed | grep -qx "$TARGET" || die "Rust target $TARGET is not installed; run: rustup target add $TARGET"

  CARGO_NDK_CMD=(cargo ndk -t "$ABI" build --bin "$BIN_NAME")
  if [[ -n "$BUILD_FEATURES" ]]; then
    CARGO_NDK_CMD+=(--features "$BUILD_FEATURES")
  fi

  case "$MODE" in
    release)
      (cd "$PACKAGE_DIR" && "${CARGO_NDK_CMD[@]}" --release)
      LOCAL_BIN="$POC_ROOT/target/$TARGET/release/$BIN_NAME"
      ;;
    debug)
      (cd "$PACKAGE_DIR" && "${CARGO_NDK_CMD[@]}")
      LOCAL_BIN="$POC_ROOT/target/$TARGET/debug/$BIN_NAME"
      ;;
    android-prof)
      (cd "$PACKAGE_DIR" && "${CARGO_NDK_CMD[@]}" --profile android-prof)
      LOCAL_BIN="$POC_ROOT/target/$TARGET/android-prof/$BIN_NAME"
      ;;
  esac
else
  case "$MODE" in
    release) LOCAL_BIN="$POC_ROOT/target/$TARGET/release/$BIN_NAME" ;;
    debug) LOCAL_BIN="$POC_ROOT/target/$TARGET/debug/$BIN_NAME" ;;
    android-prof) LOCAL_BIN="$POC_ROOT/target/$TARGET/android-prof/$BIN_NAME" ;;
  esac
fi

[[ -x "$LOCAL_BIN" ]] || die "missing local binary: $LOCAL_BIN"

if [[ "$GNARK_BACKEND" -eq 1 && "$SKIP_GNARK_BUILD" -eq 0 ]]; then
  command -v go >/dev/null 2>&1 || die "go is not installed"
  [[ -n "${ANDROID_NDK_HOME:-}" ]] || die "ANDROID_NDK_HOME is not set and no fallback NDK was found"
  NDK_CLANG="$(find_ndk_clang "$ANDROID_NDK_HOME")" || die "failed to find Android NDK clang for aarch64"
  mkdir -p "$(dirname "$GNARK_LIB_LOCAL_ABS")"
  (
    cd "$REPO_ROOT/tools/gnark"
    export CGO_ENABLED=1
    export GOOS=android
    export GOARCH=arm64
    export CC="$NDK_CLANG"
    go build -buildmode=c-shared -o "$GNARK_LIB_LOCAL_ABS" ./cmd/transferlib
  )
fi

if [[ "$GNARK_BACKEND" -eq 1 ]]; then
  [[ -f "$GNARK_LIB_LOCAL_ABS" ]] || die "missing gnark shared library: $GNARK_LIB_LOCAL_ABS"
  [[ -d "$GNARK_ARTIFACT_DIR_LOCAL_ABS" ]] || die "missing gnark artifact dir: $GNARK_ARTIFACT_DIR_LOCAL_ABS"
fi

HOST_SIMPLEPERF=""
if [[ "$PROFILE_MODE" == "simpleperf" || "$PROFILE_MODE" == "both" ]]; then
  HOST_SIMPLEPERF="$(find_host_simpleperf)" || die "simpleperf is not installed on the host"
fi

if [[ "$SKIP_PUSH" -eq 0 || "$SKIP_RUN" -eq 0 ]]; then
  command -v adb >/dev/null 2>&1 || die "adb is not installed"
  "${ADB[@]}" get-state >/dev/null 2>&1 || die "no Android device available via adb"
fi

if [[ "$SKIP_RUN" -eq 0 && -n "$CPU_MASK" ]]; then
  "${ADB[@]}" shell "command -v taskset >/dev/null 2>&1" || die "taskset is not available on the device"
fi

if [[ "$SKIP_RUN" -eq 0 && ( "$PROFILE_MODE" == "simpleperf" || "$PROFILE_MODE" == "both" ) ]]; then
  "${ADB[@]}" shell "command -v simpleperf >/dev/null 2>&1" || die "simpleperf is not available on the device"
fi

REMOTE_BIN="$REMOTE_DIR/$BIN_NAME"
if [[ -z "$OUTPUT" ]]; then
  OUTPUT="$REMOTE_DIR/${BIN_NAME}.json"
fi

if [[ "$SKIP_PUSH" -eq 0 ]]; then
  "${ADB[@]}" shell "mkdir -p '$REMOTE_DIR'"
  "${ADB[@]}" push "$LOCAL_BIN" "$REMOTE_BIN" >/dev/null
  "${ADB[@]}" shell "chmod 755 '$REMOTE_BIN'"
  if [[ "$GNARK_BACKEND" -eq 1 ]]; then
    echo "gnark lib size: $(file_size_bytes "$GNARK_LIB_LOCAL_ABS") bytes" >&2
    "${ADB[@]}" shell "mkdir -p '$GNARK_ARTIFACT_DIR_REMOTE'"
    "${ADB[@]}" push "$GNARK_LIB_LOCAL_ABS" "$GNARK_LIB_REMOTE" >/dev/null
    for artifact in proving_key.bin verifying_key.json circuit_metadata.json; do
      local_artifact="$GNARK_ARTIFACT_DIR_LOCAL_ABS/$artifact"
      [[ -f "$local_artifact" ]] || die "missing gnark artifact: $local_artifact"
      echo "gnark artifact $artifact size: $(file_size_bytes "$local_artifact") bytes" >&2
      "${ADB[@]}" push "$local_artifact" "$GNARK_ARTIFACT_DIR_REMOTE/$artifact" >/dev/null
    done
  fi
fi

build_remote_binary_cmd() {
  local remote_output="$1"
  local cmd="env BENCH_BUILD_MODE=$(printf '%q' "$MODE")"
  if [[ -n "$RAYON_THREADS" ]]; then
    cmd+=" RAYON_NUM_THREADS=$(printf '%q' "$RAYON_THREADS")"
  fi
  if [[ -n "$CPU_MASK" ]]; then
    cmd+=" BENCH_CPU_MASK=$(printf '%q' "$CPU_MASK")"
  fi

  cmd+=" $(printf '%q' "$REMOTE_BIN")"
  if ! lab_args_contains_flag "--output"; then
    cmd+=" --output $(printf '%q' "$remote_output")"
  fi
  if ! lab_args_contains_flag "--profile-mode"; then
    cmd+=" --profile-mode $(printf '%q' "$PROFILE_MODE")"
  fi

  local arg
  for arg in "${LAB_ARGS[@]}"; do
    cmd+=" $(printf '%q' "$arg")"
  done

  if [[ "$GNARK_BACKEND" -eq 1 ]]; then
    if ! lab_args_contains_flag "--gnark-lib"; then
      cmd+=" --gnark-lib $(printf '%q' "$GNARK_LIB_REMOTE")"
    fi
    if ! lab_args_contains_flag "--gnark-artifact-dir"; then
      cmd+=" --gnark-artifact-dir $(printf '%q' "$GNARK_ARTIFACT_DIR_REMOTE")"
    fi
  fi

  if [[ -n "$CPU_MASK" ]]; then
    cmd="taskset $(printf '%q' "$CPU_MASK") $cmd"
  fi

  printf '%s\n' "$cmd"
}

for run_idx in $(seq 1 "$REPEAT"); do
  remote_output_run="$(with_index_suffix "$OUTPUT" "$run_idx" "$REPEAT")"
  local_output_run="$(with_index_suffix "$LOCAL_OUTPUT" "$run_idx" "$REPEAT")"
  remote_binary_cmd="$(build_remote_binary_cmd "$remote_output_run")"
  use_simpleperf=0

  if [[ "$run_idx" -eq 1 && ( "$PROFILE_MODE" == "simpleperf" || "$PROFILE_MODE" == "both" ) ]]; then
    use_simpleperf=1
    remote_perf_run="${REMOTE_DIR}/${BIN_NAME}-simpleperf.perf.data"
  fi

  if [[ "$SKIP_RUN" -eq 0 ]]; then
    if [[ "$use_simpleperf" -eq 1 ]]; then
      remote_cmd="simpleperf record -g -o $(printf '%q' "$remote_perf_run") -- $remote_binary_cmd"
    else
      remote_cmd="$remote_binary_cmd"
    fi

    "${ADB[@]}" shell "$remote_cmd"
    echo "Remote output: $remote_output_run"
  fi

  if [[ "$SKIP_RUN" -eq 0 && "$SKIP_PULL" -eq 0 ]]; then
    mkdir -p "$(dirname "$local_output_run")"
    "${ADB[@]}" pull "$remote_output_run" "$local_output_run" >/dev/null
    echo "Local output: $local_output_run"

    if [[ "$use_simpleperf" -eq 1 ]]; then
      if [[ -n "$SIMPLEPERF_OUTPUT" ]]; then
        local_perf_run="$SIMPLEPERF_OUTPUT"
      else
        local_perf_run="${local_output_run%.*}.perf.data"
      fi
      local_perf_report="${local_perf_run%.*}.report.txt"

      mkdir -p "$(dirname "$local_perf_run")"
      "${ADB[@]}" pull "$remote_perf_run" "$local_perf_run" >/dev/null
      "$HOST_SIMPLEPERF" report -i "$local_perf_run" --symfs "$(dirname "$LOCAL_BIN")" > "$local_perf_report"
      echo "Local simpleperf data: $local_perf_run"
      echo "Local simpleperf report: $local_perf_report"
    fi
  fi
done
