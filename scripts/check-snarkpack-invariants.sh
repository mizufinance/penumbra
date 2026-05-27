#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "snarkpack invariant failed: $*" >&2
  exit 1
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

if rg -n "thread_local!|static CHALLENGE_CONTEXT|static CHALLENGE_TRACE" crates/crypto/proof-aggregation/vendor/ripp/ip_proofs/src; then
  fail "vendored challenge binding and tracing must not use thread-local fallback state"
fi

direct_digest_sites="$(
  rg -n "D::digest|Digest::digest" crates/crypto/proof-aggregation/vendor/ripp/ip_proofs/src \
    | grep -v "vendor/ripp/ip_proofs/src/challenge.rs" || true
)"
if [[ -n "$direct_digest_sites" ]]; then
  echo "$direct_digest_sites" >&2
  fail "vendored Fiat-Shamir challenges must use challenge::challenge_digest"
fi

rg -n "decode_wrapped_aggregate_proof" crates/crypto/proof-aggregation/src/backend.rs >/dev/null \
  || fail "backend verifier must decode aggregate proof wrappers"
rg -n "statement.statement_digest\\(\\)" crates/crypto/proof-aggregation/src/backend.rs >/dev/null \
  || fail "wrapper decode must compare against the recomputed statement digest"

echo "snarkpack invariants ok"
