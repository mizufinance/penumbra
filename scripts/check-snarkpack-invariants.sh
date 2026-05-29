#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "snarkpack invariant failed: $*" >&2
  exit 1
}

markdown_field() {
  local row="$1"
  local index="$2"
  printf '%s\n' "$row" | awk -F'|' -v idx="$index" '{
    field = $idx
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", field)
    print field
  }'
}

is_comparison_level() {
  case "$1" in
    '`penumbra-byte`'|'`abstract-trace`'|'`filecoin-shape`'|'`penumbra-local`'|'`assumption`') return 0 ;;
    *) return 1 ;;
  esac
}

is_deviation_class() {
  case "$1" in
    '`mechanical`'|'`performance`'|'`security-binding`'|'`semantic`') return 0 ;;
    *) return 1 ;;
  esac
}

is_evidence_status() {
  case "$1" in
    '`proved`'|'`refined`'|'`composed`'|'`assumed`'|'`open`'|'`proved-equivalent`') return 0 ;;
    *) return 1 ;;
  esac
}

hash_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

hash_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  else
    shasum -a 256 | awk '{print $1}'
  fi
}

formal_proof_stamp() {
  {
    hash_file scripts/snarkpack-formal.sh | awk '{print $1 "  scripts/snarkpack-formal.sh"}'
    hash_file crates/crypto/proof-aggregation/formal/snarkpack/toolchain.toml \
      | awk '{print $1 "  crates/crypto/proof-aggregation/formal/snarkpack/toolchain.toml"}'
    for file in crates/crypto/proof-aggregation/formal/snarkpack/fstar/*.fst; do
      printf '%s  %s\n' "$(hash_file "$file")" "$file"
    done
  } | hash_stdin
}

check_reference_crate_boundary() {
  local manifest="crates/crypto/proof-aggregation-reference/Cargo.toml"
  local crate_dir="crates/crypto/proof-aggregation-reference"

  [[ -f "$manifest" ]] || return

  rg -F '"crates/crypto/proof-aggregation-reference"' Cargo.toml >/dev/null \
    || fail "reference crate must be listed as a workspace member"
  rg -n '^publish = false$' "$manifest" >/dev/null \
    || fail "reference crate must be marked publish = false"
  rg -n '^penumbra-sdk-proof-aggregation = ' "$manifest" >/dev/null \
    || fail "reference crate must use the public proof-aggregation crate boundary"

  local forbidden_deps
  forbidden_deps="$(
    rg -n 'ark-ip-proofs|ark-inner-products|src/ipp|proof-aggregation/src' "$manifest" "$crate_dir/src" || true
  )"
  if [[ -n "$forbidden_deps" ]]; then
    echo "$forbidden_deps" >&2
    fail "reference crate must not depend on or import production internals"
  fi

  local production_reference_imports
  production_reference_imports="$(
    rg -n 'penumbra-sdk-proof-aggregation-reference|proof_aggregation_reference|proof-aggregation-reference' \
      Cargo.toml crates \
      | rg -v '^Cargo.toml:|^crates/crypto/proof-aggregation-reference/|^crates/crypto/proof-aggregation-fuzz/' || true
  )"
  if [[ -n "$production_reference_imports" ]]; then
    echo "$production_reference_imports" >&2
    fail "production crates must not depend on the dev-only reference oracle"
  fi
}

check_fuzz_crate_boundary() {
  local manifest="crates/crypto/proof-aggregation-fuzz/Cargo.toml"
  local crate_dir="crates/crypto/proof-aggregation-fuzz"

  [[ -f "$manifest" ]] || return

  rg -F '"crates/crypto/proof-aggregation-fuzz"' Cargo.toml >/dev/null \
    || fail "fuzz crate must be listed as a workspace member"
  rg -n '^publish = false$' "$manifest" >/dev/null \
    || fail "fuzz crate must be marked publish = false"
  rg -n '^cargo-fuzz = true$' "$manifest" >/dev/null \
    || fail "fuzz crate must be marked as a cargo-fuzz package"
  rg -n '^libfuzzer-sys = ' "$manifest" >/dev/null \
    || fail "fuzz crate must depend on libfuzzer-sys"
  rg -n '^penumbra-sdk-proof-aggregation = ' "$manifest" >/dev/null \
    || fail "fuzz crate must use the public proof-aggregation crate boundary"
  rg -n '^penumbra-sdk-proof-aggregation-reference = ' "$manifest" >/dev/null \
    || fail "fuzz crate must use the reference oracle crate as a dev-only boundary"
  rg -n '^penumbra-sdk-proof-aggregation-trace-schema = ' "$manifest" >/dev/null \
    || fail "fuzz crate must use the shared trace schema boundary"

  local forbidden_deps
  forbidden_deps="$(
    rg -n 'ark-ip-proofs|ark-inner-products|src/ipp|proof-aggregation/src' "$manifest" "$crate_dir" || true
  )"
  if [[ -n "$forbidden_deps" ]]; then
    echo "$forbidden_deps" >&2
    fail "fuzz crate must not depend on or import production internals"
  fi
}

check_trace_schema() {
  local manifest="crates/crypto/proof-aggregation-trace-schema/Cargo.toml"
  local schema="crates/crypto/proof-aggregation-trace-schema/src/lib.rs"

  [[ -f "$manifest" ]] || fail "trace schema crate is missing"
  [[ -f "$schema" ]] || fail "trace schema source is missing"

  rg -F '"crates/crypto/proof-aggregation-trace-schema"' Cargo.toml >/dev/null \
    || fail "trace schema crate must be listed as a workspace member"
  rg -n '^penumbra-sdk-proof-aggregation-trace-schema[[:space:]]*=' Cargo.toml >/dev/null \
    || fail "trace schema crate must be a workspace dependency"
  if rg -n '^\[dependencies\]' "$manifest"; then
    fail "trace schema crate must remain dependency-free"
  fi
  rg -n '^#!\[no_std\]$' "$schema" >/dev/null \
    || fail "trace schema crate must be no_std"
  rg -n '^extern crate alloc;$' "$schema" >/dev/null \
    || fail "trace schema crate may use alloc, but not std"
  rg -n '^penumbra-sdk-proof-aggregation-trace-schema[[:space:]]*=' \
    crates/crypto/proof-aggregation/Cargo.toml >/dev/null \
    || fail "production proof-aggregation crate must depend on trace schema"
  rg -n '^penumbra-sdk-proof-aggregation-trace-schema[[:space:]]*=' \
    crates/crypto/proof-aggregation-reference/Cargo.toml >/dev/null \
    || fail "reference crate must depend on trace schema"

  local tmpdir spec_rows schema_rows
  tmpdir="$(mktemp -d)"
  spec_rows="$tmpdir/spec"
  schema_rows="$tmpdir/schema"
  awk -F'|' '/^\| `[^`]+` \|/ {
    id = $2
    level = $4
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", id)
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", level)
    gsub(/`/, "", id)
    gsub(/`/, "", level)
    print id "|" level
  }' docs/snarkpack/ripp-spec.md | sort > "$spec_rows"
  awk '
  /TracePolicy \{/ { id = ""; level = "" }
  /spec_row_id:/ {
    id = $0
    sub(/.*spec_row_id: "/, "", id)
    sub(/".*/, "", id)
  }
  /primary_level: TraceComparisonLevel::/ {
    level = $0
    sub(/.*primary_level: TraceComparisonLevel::/, "", level)
    sub(/[,} ].*/, "", level)
  }
  /^[[:space:]]*\},/ && id != "" {
    if (level == "PenumbraByte") level = "penumbra-byte"
    else if (level == "AbstractTrace") level = "abstract-trace"
    else if (level == "FilecoinShape") level = "filecoin-shape"
    else if (level == "PenumbraLocal") level = "penumbra-local"
    else level = "invalid:" level
    print id "|" level
  }' "$schema" | sort > "$schema_rows"
  if [[ ! -s "$schema_rows" ]]; then
    rm -rf "$tmpdir"
    fail "trace schema has no trace policies"
  fi
  if ! diff -u "$spec_rows" "$schema_rows"; then
    rm -rf "$tmpdir"
    fail "trace schema policies must match ripp-spec.md Spec Row Index"
  fi
  rm -rf "$tmpdir"

  rg -n 'MissingBytePayload' "$schema" >/dev/null \
    || fail "trace schema must reject penumbra-byte events without byte payloads"
  rg -n 'MissingAbstractPayload' "$schema" >/dev/null \
    || fail "trace schema must reject abstract-trace events without abstract payloads"
  rg -n 'MissingFilecoinBugClass' "$schema" >/dev/null \
    || fail "trace schema must reject filecoin-shape events without bug-class markers"
}

if rg -n "TranscriptPhase" crates/crypto/proof-aggregation docs/snarkpack; then
  fail "dead TranscriptPhase API must not reappear"
fi

if rg -n "deserialize_compressed_unchecked" crates/crypto/proof-aggregation crates/core/app crates/bench; then
  fail "unchecked aggregate deserialization must not be production-accessible"
fi

if rg -n "USE_UNCHECKED_AGGREGATE_DESERIALIZATION" crates/crypto/proof-aggregation crates/core/app crates/bench; then
  fail "unchecked aggregate deserialization switch must not be retained"
fi

if rg -n "thread_local!|static CHALLENGE_CONTEXT|static CHALLENGE_TRACE" crates/crypto/proof-aggregation/src/ipp/ip_proofs/src; then
  fail "challenge binding and tracing must not use thread-local fallback state"
fi

challenge_rs=crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs
if rg -n "struct ChallengeContext\\(|impl Default for ChallengeContext|Default.*ChallengeContext|ChallengeContext.*Default" "$challenge_rs"; then
  fail "ChallengeContext must not have a tuple constructor or Default implementation"
fi

challenge_context_footguns="$(
  rg -n "ChallengeContext::default|ChallengeContext\\(\\[" \
    crates/crypto/proof-aggregation/src \
    crates/core/app \
    crates/bench || true
)"
if [[ -n "$challenge_context_footguns" ]]; then
  echo "$challenge_context_footguns" >&2
  fail "ChallengeContext must not be default-constructed or tuple-constructed"
fi

unexpected_challenge_context_fns="$(
  sed -n '/impl ChallengeContext {/,/^}/p' "$challenge_rs" \
    | rg -n "pub fn" \
    | rg -v "from_statement_digest|as_bytes" || true
)"
if [[ -n "$unexpected_challenge_context_fns" ]]; then
  echo "$unexpected_challenge_context_fns" >&2
  fail "ChallengeContext must expose only from_statement_digest and as_bytes"
fi

direct_digest_sites="$(
  rg -n "D::digest|Digest::digest" crates/crypto/proof-aggregation/src/ipp/ip_proofs/src \
    | grep -v "src/ipp/ip_proofs/src/challenge.rs" || true
)"
if [[ -n "$direct_digest_sites" ]]; then
  echo "$direct_digest_sites" >&2
  fail "Fiat-Shamir challenges must use challenge::challenge_digest"
fi

duplicate_codec_sites="$(
  rg -n "fn (encode_.*statement|statement_.*encode|decode_.*aggregate_proof|encode_.*aggregate_proof|decode_wrapped|encode_wrapped)" \
    crates/crypto/proof-aggregation/src \
    | rg -v "crates/crypto/proof-aggregation/src/(statement.rs|aggregate_proof_wrapper.rs):" || true
)"
if [[ -n "$duplicate_codec_sites" ]]; then
  echo "$duplicate_codec_sites" >&2
  fail "statement and aggregate-proof encoding/decoding must stay in the canonical modules"
fi

if rg -n "\\badmit\\b|--admit_smt_queries" crates/crypto/proof-aggregation/formal scripts/snarkpack-formal.sh; then
  fail "formal proofs must not use unrecorded admits or --admit_smt_queries"
fi

if rg -n "assume val impl_u32__is_power_of_two" scripts/snarkpack-formal.sh >/dev/null; then
  rg -n "impl_u32__is_power_of_two" docs/snarkpack/formal-handoff.md >/dev/null \
    || fail "hax power-of-two support assumption must be recorded in formal-handoff.md"
fi

if rg -n "assume val impl__starts_with" scripts/snarkpack-formal.sh >/dev/null; then
  rg -n "impl__starts_with" docs/snarkpack/formal-handoff.md >/dev/null \
    || fail "hax slice starts_with support assumption must be recorded in formal-handoff.md"
fi

hax_targets=crates/crypto/proof-aggregation/formal/snarkpack/hax-targets.txt
hax_boundary=crates/crypto/proof-aggregation/formal/snarkpack/hax-extraction-boundary.md
if [[ -f "$hax_targets" ]]; then
  [[ -f "$hax_boundary" ]] || fail "hax extraction boundary metadata is missing"
  while IFS= read -r target; do
    [[ -z "$target" || "$target" =~ ^# ]] && continue
    rg -F "| \`$target\` |" "$hax_boundary" >/dev/null \
      || fail "hax target $target is missing extraction-boundary metadata"
  done < "$hax_targets"
fi

if rg -n "assume val" scripts/snarkpack-formal.sh >/dev/null; then
  while IFS= read -r assumed_symbol; do
    [[ -z "$assumed_symbol" ]] && continue
    rg -F "$assumed_symbol" "$hax_boundary" >/dev/null \
      || fail "hax assume val $assumed_symbol lacks extraction-boundary metadata"
  done < <(sed -n 's/^assume val \([^:]*\):.*/\1/p' scripts/snarkpack-formal.sh)
fi

ripp_scope=crates/crypto/proof-aggregation/formal/snarkpack/ripp-refinement-scope.txt
ripp_map=docs/snarkpack/ripp-refinement.md
if [[ -f "$ripp_scope" ]]; then
  [[ -f "$ripp_map" ]] || fail "RIPP refinement map is missing"
  while IFS= read -r symbol_id; do
    [[ -z "$symbol_id" || "$symbol_id" =~ ^# ]] && continue
    file="${symbol_id%%:*}"
    symbol="${symbol_id#*:}"
    leaf="${symbol##*::}"
    [[ -f "$file" ]] || fail "RIPP refinement scoped file $file does not exist"
    rg -n "\\b${leaf}\\b" "$file" >/dev/null \
      || fail "RIPP refinement scoped symbol $symbol_id does not exist"
    rg -F "| \`$symbol_id\` |" "$ripp_map" >/dev/null \
      || fail "RIPP refinement scoped symbol $symbol_id is missing from $ripp_map"
  done < "$ripp_scope"

  while IFS= read -r mapped_symbol; do
    [[ -z "$mapped_symbol" ]] && continue
    grep -Fx "$mapped_symbol" "$ripp_scope" >/dev/null \
      || fail "RIPP refinement map contains unscoped symbol $mapped_symbol"
  done < <(sed -n 's/^| `\([^`]*\)` |.*/\1/p' "$ripp_map")
fi

ripp_spec=docs/snarkpack/ripp-spec.md
if [[ -f "$ripp_spec" ]]; then
  duplicate_spec_rows="$(
    sed -n 's/^| `\([^`]*\)` |.*/\1/p' "$ripp_spec" | sort | uniq -d
  )"
  if [[ -n "$duplicate_spec_rows" ]]; then
    echo "$duplicate_spec_rows" >&2
    fail "RIPP spec contains duplicate spec row ids"
  fi

  while IFS= read -r row; do
    spec_id="$(printf '%s\n' "$row" | sed -n 's/^| `\([^`]*\)` |.*/\1/p')"
    [[ -z "$spec_id" ]] && continue
    primary_level="$(markdown_field "$row" 4)"
    required_evidence="$(markdown_field "$row" 5)"
    is_comparison_level "$primary_level" \
      || fail "RIPP spec row $spec_id has invalid primary comparison level $primary_level"
    [[ -n "$required_evidence" && "$required_evidence" != "pending" ]] \
      || fail "RIPP spec row $spec_id lacks required evidence"
  done < <(sed -n '/^| `[^`]*` |/p' "$ripp_spec")
fi

adaptation_scope=crates/crypto/proof-aggregation/formal/snarkpack/adaptation-scope.txt
adaptation_map=docs/snarkpack/adaptation-register.md
if [[ -f "$adaptation_scope" ]]; then
  [[ -f "$adaptation_map" ]] || fail "SnarkPack adaptation register is missing"
  filecoin_normative_pin_status="$(
    grep -F "| \`filecoin.normative-pin\` |" "$adaptation_map" \
      | awk -F'|' '{ field = $9; gsub(/^[[:space:]]+|[[:space:]]+$/, "", field); print field }'
  )"
  [[ -n "$filecoin_normative_pin_status" ]] \
    || fail "SnarkPack adaptation register must include filecoin.normative-pin"

  duplicate_adaptation_scope_rows="$(
    sed -n 's/^\([^#|][^|]*\)|.*/\1/p' "$adaptation_scope" | sort | uniq -d
  )"
  if [[ -n "$duplicate_adaptation_scope_rows" ]]; then
    echo "$duplicate_adaptation_scope_rows" >&2
    fail "SnarkPack adaptation scope contains duplicate ids"
  fi

  while IFS='|' read -r adaptation_id target extra; do
    [[ -z "$adaptation_id" || "$adaptation_id" =~ ^# ]] && continue
    [[ -z "${extra:-}" ]] || fail "adaptation scope row $adaptation_id has too many fields"
    [[ -n "$target" ]] || fail "adaptation scope row $adaptation_id is missing a target"

    row_count="$(
      grep -F "| \`$adaptation_id\` |" "$adaptation_map" | wc -l | tr -d ' '
    )"
    [[ "$row_count" == "1" ]] \
      || fail "adaptation scope row $adaptation_id must appear exactly once in adaptation-register.md"

    if [[ "$target" == external:* ]]; then
      external_ref="${target#external:}"
      rg -F "$external_ref" "$adaptation_map" >/dev/null \
        || fail "adaptation external reference $external_ref is not documented"
    else
      [[ "$target" == *:* ]] || fail "adaptation target $target must be file:pattern"
      file="${target%%:*}"
      pattern="${target#*:}"
      [[ -f "$file" ]] || fail "adaptation scoped file $file does not exist"
      rg -F "$pattern" "$file" >/dev/null \
        || fail "adaptation scoped pattern $pattern is missing from $file"
    fi
  done < "$adaptation_scope"

  duplicate_adaptation_map_rows="$(
    sed -n 's/^| `\([^`]*\)` |.*/\1/p' "$adaptation_map" | sort | uniq -d
  )"
  if [[ -n "$duplicate_adaptation_map_rows" ]]; then
    echo "$duplicate_adaptation_map_rows" >&2
    fail "SnarkPack adaptation register contains duplicate ids"
  fi

  while IFS= read -r row; do
    adaptation_id="$(printf '%s\n' "$row" | sed -n 's/^| `\([^`]*\)` |.*/\1/p')"
    [[ -z "$adaptation_id" ]] && continue
    grep -F "${adaptation_id}|" "$adaptation_scope" >/dev/null \
      || fail "adaptation register contains unscoped row $adaptation_id"

    primary_level="$(markdown_field "$row" 5)"
    deviation_class="$(markdown_field "$row" 6)"
    required_evidence="$(markdown_field "$row" 8)"
    status="$(markdown_field "$row" 9)"
    reviewer="$(markdown_field "$row" 10)"
    review_date="$(markdown_field "$row" 11)"

    is_comparison_level "$primary_level" \
      || fail "adaptation row $adaptation_id has invalid comparison level $primary_level"
    is_deviation_class "$deviation_class" \
      || fail "adaptation row $adaptation_id has invalid deviation class $deviation_class"
    [[ -n "$required_evidence" && "$required_evidence" != "pending" ]] \
      || fail "adaptation row $adaptation_id lacks required evidence"
    is_evidence_status "$status" \
      || fail "adaptation row $adaptation_id has invalid status $status"

    if [[ "$status" != '`open`' ]] \
      && [[ "$deviation_class" == '`security-binding`' || "$deviation_class" == '`semantic`' ]]; then
      [[ -n "$reviewer" && "$reviewer" != "pending" && -n "$review_date" && "$review_date" != "pending" ]] \
        || fail "reviewed security-binding/semantic adaptation row $adaptation_id needs reviewer and date"
    fi

    if [[ "$primary_level" == '`filecoin-shape`' && "$adaptation_id" != "filecoin.normative-pin" ]] \
      && [[ "$filecoin_normative_pin_status" != '`refined`' && "$filecoin_normative_pin_status" != '`proved`' && "$filecoin_normative_pin_status" != '`proved-equivalent`' ]] \
      && [[ "$status" != '`open`' ]]; then
      fail "filecoin-shape adaptation row $adaptation_id cannot leave open until filecoin.normative-pin is reviewed"
    fi
  done < <(sed -n '/^| `[^`]*` |/p' "$adaptation_map")
fi

check_reference_crate_boundary
check_fuzz_crate_boundary
check_trace_schema

expected_stamp="$(formal_proof_stamp)"
recorded_stamp="$(
  sed -n 's/^Proof artifact stamp: sha256:\([0-9a-f]\{64\}\)$/\1/p' \
    docs/snarkpack/formal-handoff.md
)"
if [[ -z "$recorded_stamp" ]]; then
  fail "formal-handoff.md must record the proof artifact SHA256 stamp"
fi
if [[ "$recorded_stamp" != "$expected_stamp" ]]; then
  echo "expected proof artifact stamp: sha256:$expected_stamp" >&2
  echo "recorded proof artifact stamp: sha256:$recorded_stamp" >&2
  fail "formal proof files changed without restamping formal-handoff.md"
fi

rg -n "preflight_aggregate_verify" crates/crypto/proof-aggregation/src/backend.rs >/dev/null \
  || fail "backend verifier must pass through typed aggregate preflight"
rg -n "decode_wrapped_aggregate_proof" crates/crypto/proof-aggregation/src/preflight.rs >/dev/null \
  || fail "typed aggregate preflight must decode aggregate proof wrappers"
rg -n "statement.statement_digest\\(\\)" crates/crypto/proof-aggregation/src/preflight.rs >/dev/null \
  || fail "wrapper decode must compare against the recomputed statement digest in preflight"

echo "snarkpack invariants ok"
