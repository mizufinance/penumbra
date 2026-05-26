# Vendored RIPP Upstream Log

This directory vendors the Arkworks RIPP crates used by the Penumbra
SnarkPack prototype:

- `ip_proofs`
- `dh_commitments`
- `inner_products`

Upstream repository: `https://github.com/arkworks-rs/ripp`

## Local Ownership

The vendored code is treated as Penumbra-owned implementation code for this
prototype. There is no compatibility requirement with the upstream crate API.
Security fixes and performance work should change local code directly, but this
file must be updated when behavior diverges from upstream.

## Required Divergence Entries

For each local change to vendored code, add:

- upstream commit or crate version used as the base, when known
- changed files
- reason for the change
- security impact
- benchmark impact, if relevant
- tests or fuzz targets that cover the change

## Pre-Policy Local Change Summary

Status: pre-policy summary. These entries describe known local divergence from
upstream before this log existed. They are not a complete audited change log.
Entries added after this file must follow `Required Divergence Entries`.

- The vendored crates are upgraded to Arkworks `0.5` dependencies.
- The SnarkPack path is wired for Penumbra BLS12-377 aggregation.
- Profiling buckets were added for aggregate build and verify stages.
- Proving code includes prepared-SRS and dynamic `G2Prepared` reuse work.
- Pairing helpers expose finer-grained timing for normalize, prepare, Miller
  loop, and final exponentiation stages.
- GIPA rescale thresholds and Rayon use are tuned for local builder benchmarks.

## Verification Obligations

Changes under this directory require at least:

- `cargo test -p penumbra-sdk-proof-aggregation --lib` or a focused equivalent
- differential tests against legacy batch verification for affected families
- mutation tests for transcript and public input binding when challenge code
  changes
- release-mode benchmark comparison when pairing, MSM, GIPA, or threading code
  changes

Any transcript or Fiat-Shamir challenge change also requires updating
`docs/snarkpack/security.md` and the transcript test vectors.
