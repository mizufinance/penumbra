#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "snarkpack invariant failed: $*" >&2
  exit 1
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
    for file in crates/crypto/proof-aggregation/formal/snarkpack/fstar/*.fst; do
      printf '%s  %s\n' "$(hash_file "$file")" "$file"
    done
  } | hash_stdin
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
