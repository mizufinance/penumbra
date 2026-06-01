# SnarkPack Fuzz Corpus Baseline

Status: committed minimized corpus baseline.

Date: 2026-06-01
Host: local macOS arm64 developer runner with `cargo +nightly fuzz`.
Scope: `crates/crypto/proof-aggregation-fuzz`.

The smoke gate copies the committed corpus into a temporary directory before
execution, so CI smoke runs seed from it without mutating it. Only the small,
curated, human-named seeds (`empty`, `oversize`, `valid-wrapper`, `wrong-digest`,
…) are committed; the bulk SHA1-named machine-minimized corpus is git-ignored
(`crates/crypto/proof-aggregation-fuzz/.gitignore`) and lives under the local
working tree only — it bloats the repo and regenerates every run. The figures
below describe the local coverage-guided sessions, time-boxed at 61 seconds per
target, run well beyond the 16-run smoke default.

They found one proposal-validation cheap-rejection issue: malformed SRS ids
forced default SRS-id/SRS setup before rejection. The reproducer is covered by
`aggregate_bundle_verification_rejects_bad_srs_id_before_srs_setup`; replayed
slow units now reject in 1-3 ms after `DEFAULT_DEV_SRS_ID` was checked in.

The Lean differential conformance layer is **not** a fuzz target. The transcript-
shape domain is finite (one shape per power of two up to the SRS max), so it is
exhaustively enumerated by `proof-aggregation-lean-conformance`, not sampled —
see `verification-plan.md` Layer 9. The `feature/coverage` columns below describe
machine-fuzzed byte boundaries only.

## Minimized Corpora

| target | committed corpus | minimized features | coverage edges | bytes |
| --- | ---: | ---: | ---: | ---: |
| `wrapper_inner_range` | 9 files | 52 | 52 | 388 |
| `preflight_aggregate_verify` | 3 files | 7,048 | 6,867 | 151 |
| `deserialize_aggregate_proof` | 56 files | 449 | 208 | 23,495 |
| `sidecar_decoding` | 227 files | 2,695 | 1,416 | 71,555 |
| `aggregate_bundle_shape` | 69 files | 2,166 | 1,669 | 469,234 |
| `proposal_validation` | 62 files | 2,425 | 1,881 | 140,238 |

## Clean Baseline Runs

| target | clean duration | runs | final coverage | final features | result |
| --- | ---: | ---: | ---: | ---: | --- |
| `wrapper_inner_range` | 61s (0.017h) | 1,084,302 | 41 | 41 | clean |
| `preflight_aggregate_verify` | 61s (0.017h) | 657,865 | 299 | 288 | clean |
| `deserialize_aggregate_proof` | 61s (0.017h) | 151,302 | 197 | 438 | clean |
| `sidecar_decoding` | 61s (0.017h) | 196,213 | 1,028 | 2,262 | clean |
| `aggregate_bundle_shape` | 61s (0.017h) | 70,952 | 1,350 | 1,804 | clean |
| `proposal_validation` | 61s (0.017h) | 175,343 | 1,355 | 1,816 | clean after SRS-id fix |

No crash, hang, OOM, panic, or sanitizer finding remains in the recorded clean
baseline. Larger raw corpora and logs are intentionally left under `target/`.
