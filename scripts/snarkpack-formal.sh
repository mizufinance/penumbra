#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FORMAL_DIR="crates/crypto/proof-aggregation/formal/snarkpack"
TOOLCHAIN="$FORMAL_DIR/toolchain.toml"

fail() {
  echo "snarkpack formal failed: $*" >&2
  exit 1
}

read_pin() {
  local key="$1"
  sed -n "s/^${key} = \"\\(.*\\)\"/\\1/p" "$TOOLCHAIN"
}

without_v_prefix() {
  printf '%s' "$1" | sed 's/^v//'
}

require_command() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || fail "$cmd is not installed"
}

require_command cargo
require_command z3

load_opam_switch() {
  local switch
  switch="$(read_pin opam_switch)"
  if command -v hax-engine >/dev/null 2>&1 || [ -z "$switch" ]; then
    return
  fi
  command -v opam >/dev/null 2>&1 || return
  opam switch list --short 2>/dev/null | grep -Fx "$switch" >/dev/null || return
  eval "$(opam env --switch="$switch")"
}

load_opam_switch

if ! command -v cargo-hax >/dev/null 2>&1 && ! cargo hax --version >/dev/null 2>&1; then
  fail "cargo-hax is not installed; expected hax $(read_pin hax)"
fi

require_command hax-engine

if command -v fstar.exe >/dev/null 2>&1; then
  FSTAR=fstar.exe
elif command -v fstar >/dev/null 2>&1; then
  FSTAR=fstar
else
  fail "F* is not installed; expected F* $(read_pin fstar)"
fi

z3 --version | grep -F "$(read_pin z3)" >/dev/null \
  || fail "z3 version mismatch; expected $(read_pin z3)"

hax_version="$(without_v_prefix "$(read_pin hax)")"
fstar_version="$(without_v_prefix "$(read_pin fstar)")"

cargo hax --version | grep -F "version=${hax_version}" >/dev/null \
  || fail "hax version mismatch; expected $(read_pin hax)"

"$FSTAR" --version | grep -F "$fstar_version" >/dev/null \
  || fail "F* version mismatch; expected $(read_pin fstar)"

find_hax_proof_libs() {
  if [ -n "${HAX_PROOF_LIBS_HOME:-}" ] && [ -d "$HAX_PROOF_LIBS_HOME/core" ]; then
    printf '%s\n' "$HAX_PROOF_LIBS_HOME"
    return
  fi

  local candidate
  for candidate in \
    "$HOME/.local/opt/hax-${hax_version}/hax-lib/proof-libs/fstar" \
    "$HOME/.local/opt/hax-v${hax_version}/hax-lib/proof-libs/fstar"; do
    if [ -d "$candidate/core" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  if [ "${SNARKPACK_FORMAL_ALLOW_TMP_HAX_LIBS:-}" = "1" ] && [ "${CI:-}" != "true" ]; then
    for candidate in \
      "/tmp/hax-cargo-hax-v${hax_version}/hax-lib/proof-libs/fstar" \
      "/tmp/hax-cargo-hax-${hax_version}/hax-lib/proof-libs/fstar"; do
      if [ -d "$candidate/core" ]; then
        printf '%s\n' "$candidate"
        return
      fi
    done
  fi

  fail "hax F* proof libraries not found; set HAX_PROOF_LIBS_HOME"
}

prepare_fstar_inputs() {
  local hax_proof_libs="$1"
  local hax_lib_root
  hax_lib_root="$(cd "$hax_proof_libs/../.." && pwd)"
  local hax_lib_extraction="$hax_lib_root/proofs/fstar/extraction"
  [ -d "$hax_lib_extraction" ] || fail "hax-lib F* extraction not found at $hax_lib_extraction"

  GENERATED_DIR="$FORMAL_DIR/.generated"
  FSTAR_SHIMS_DIR="$GENERATED_DIR/fstar-shims"
  FSTAR_HAX_PROOF_LIBS="$GENERATED_DIR/hax-proof-libs"
  FSTAR_HAX_LIB_EXTRACTION="$GENERATED_DIR/hax-lib-extraction"

  rm -rf "$FSTAR_SHIMS_DIR" "$FSTAR_HAX_PROOF_LIBS" "$FSTAR_HAX_LIB_EXTRACTION"
  mkdir -p "$FSTAR_SHIMS_DIR" "$FSTAR_HAX_PROOF_LIBS" "$FSTAR_HAX_LIB_EXTRACTION"
  cp -R "$hax_proof_libs/core" "$FSTAR_HAX_PROOF_LIBS/core"
  cp -R "$hax_proof_libs/rust_primitives" "$FSTAR_HAX_PROOF_LIBS/rust_primitives"
  cp -R "$hax_lib_extraction/." "$FSTAR_HAX_LIB_EXTRACTION/"

  cat > "$FSTAR_SHIMS_DIR/FStar.Mul.fst" <<'FSTAR'
module FStar.Mul
FSTAR

  find "$FSTAR_HAX_PROOF_LIBS" "$FSTAR_HAX_LIB_EXTRACTION" \
    \( -name '*.fst' -o -name '*.fsti' \) \
    -exec perl -0pi -e 's/pred:\s*Type0/pred: Prims.prop/g; s/->\s*Type0;/-> Prims.prop;/g; s/->\s*Type0\)/-> Prims.prop)/g; s/->\s*Type0\n/-> Prims.prop\n/g; s/\(p: Type0\)/(p: Prims.prop)/g; s/\(v__formula: Type0\)/(v__formula: Prims.prop)/g' {} +

  cat >> "$FSTAR_HAX_PROOF_LIBS/core/Core_models.Num.fst" <<'FSTAR'

assume val impl_u32__is_power_of_two: u32 -> bool

assume val impl_u32__to_le_bytes_injective: x: u32 -> y: u32
  -> Lemma (requires impl_u32__to_le_bytes x == impl_u32__to_le_bytes y) (ensures x == y)

assume val impl_u32__from_to_le_bytes: x: u32
  -> Lemma (ensures impl_u32__from_le_bytes (impl_u32__to_le_bytes x) == x)

assume val impl_usize_u32_cast_roundtrip: x: usize
  -> Lemma
      (requires v x <= 4294967295)
      (ensures (cast (cast x <: u32) <: usize) == x)

assume val impl_usize__checked_add_ok: x: usize -> y: usize
  -> Lemma
      (requires v x + v y <= max_usize)
      (ensures impl_usize__checked_add x y == Core_models.Option.Option_Some (x +! y))

assume val impl_u64__to_le_bytes_injective: x: u64 -> y: u64
  -> Lemma (requires impl_u64__to_le_bytes x == impl_u64__to_le_bytes y) (ensures x == y)
FSTAR

  cat >> "$FSTAR_HAX_PROOF_LIBS/core/Core_models.Slice.fst" <<'FSTAR'

assume val impl__starts_with: #v_T:Type0 -> t_Slice v_T -> t_Slice v_T -> bool

assume val impl__starts_with_append: #v_T:Type0 -> prefix:t_Slice v_T -> rest:t_Slice v_T
  -> Lemma (ensures impl__starts_with (FStar.Seq.append prefix rest) prefix == true)

assume val impl__get_middle_append3:
  #v_T:Type0 ->
  prefix:t_Slice v_T ->
  field:t_Slice v_T ->
  suffix:t_Slice v_T ->
  Lemma
    (requires
      Rust_primitives.Integers.v (impl__len prefix) +
      Rust_primitives.Integers.v (impl__len field) +
      Rust_primitives.Integers.v (impl__len suffix) <=
      Rust_primitives.Integers.max_usize)
    (ensures
      impl__get #v_T
        #(Core_models.Ops.Range.t_Range Rust_primitives.Integers.usize)
        (FStar.Seq.append prefix (FStar.Seq.append field suffix))
        ({
            Core_models.Ops.Range.f_start = impl__len prefix;
            Core_models.Ops.Range.f_end = impl__len prefix +! impl__len field
          }
          <:
          Core_models.Ops.Range.t_Range Rust_primitives.Integers.usize)
      ==
      Core_models.Option.Option_Some field)
FSTAR

  cat >> "$FSTAR_HAX_PROOF_LIBS/core/Core_models.Convert.fst" <<'FSTAR'

assume val impl__try_into_array_self_slice:
  #v_T:Type0 ->
  #v_N:Rust_primitives.Integers.usize ->
  bytes:t_Array v_T v_N ->
  Lemma
    (ensures
      f_try_into #(t_Slice v_T)
        #(t_Array v_T v_N)
        #FStar.Tactics.Typeclasses.solve
        (bytes <: t_Slice v_T)
      ==
      Core_models.Result.Result_Ok bytes)
FSTAR
}

pushd crates/crypto/proof-aggregation >/dev/null
echo "snarkpack formal: extracting proof-aggregation statement boundary"
cargo hax into \
  -i '-** +penumbra_sdk_proof_aggregation::statement::StatementFieldBytes +penumbra_sdk_proof_aggregation::statement::StatementPublicInputRow +penumbra_sdk_proof_aggregation::statement::StatementPaddedRows +penumbra_sdk_proof_aggregation::statement::StatementEncodingInput +penumbra_sdk_proof_aggregation::statement::encode_statement +penumbra_sdk_proof_aggregation::statement::validate_counts +penumbra_sdk_proof_aggregation::statement::validate_row_arity +penumbra_sdk_proof_aggregation::statement::validate_repeat_final_padding +penumbra_sdk_proof_aggregation::aggregate_proof_wrapper::encode_wrapped_aggregate_proof +penumbra_sdk_proof_aggregation::aggregate_proof_wrapper::decode_wrapped_aggregate_proof_inner_range' \
  fstar \
  || fail "hax extraction failed for proof-aggregation statement boundary"
popd >/dev/null

pushd crates/crypto/proof-aggregation/src/ipp/ip_proofs >/dev/null
echo "snarkpack formal: extracting ip_proofs challenge boundary"
cargo hax into \
  -i '-** +ark_ip_proofs::challenge::ChallengeContext +ark_ip_proofs::challenge::challenge_context_preimage +ark_ip_proofs::challenge::challenge_preimage' \
  fstar \
  || fail "hax extraction failed for ip_proofs challenge boundary"
popd >/dev/null

prepare_fstar_inputs "$(find_hax_proof_libs)"

FSTAR_FLAGS=(
  --cache_off
  --include "$FSTAR_SHIMS_DIR"
  --include "$FSTAR_HAX_PROOF_LIBS/rust_primitives"
  --include "$FSTAR_HAX_PROOF_LIBS/core"
  --include "$FSTAR_HAX_LIB_EXTRACTION"
  --include "crates/crypto/proof-aggregation/proofs/fstar/extraction"
  --include "crates/crypto/proof-aggregation/src/ipp/ip_proofs/proofs/fstar/extraction"
  --include "$FORMAL_DIR/fstar"
)

for proof in "$FORMAL_DIR"/fstar/*.fst; do
  "$FSTAR" "${FSTAR_FLAGS[@]}" "$proof"
done

echo "snarkpack formal ok"
