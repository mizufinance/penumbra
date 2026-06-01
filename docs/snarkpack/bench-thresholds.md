# SnarkPack Benchmark Thresholds

Status: CI-enforced baseline.

Date: 2026-06-01
Baseline runner: GitHub Actions `ubuntu-24.04`, release-mode Rust in the
`snarkpack-formal` workflow. The local developer Criterion run remains useful
for diagnosis, but the hard gate is `just snarkpack-dos-gate`.

## Commands

- CI gate:
  `just snarkpack-dos-gate`
- size report:
  `cargo test -p penumbra-sdk-proof-aggregation aggregate_proof_size_report --lib -- --ignored --nocapture`
- optional release benchmarks:
  `cargo bench -p penumbra-sdk-bench --bench snarkpack`

## Size Gate

Observed max wrapped aggregate proof bytes: `31,946`

Chosen cap:

- formula: `round_up_to_64k(observed_max_wrapped_valid_proof_bytes * 4)`
- minimum: `64 KiB`
- maximum: `1 MiB`
- result: `131,072` bytes

The cap is enforced at typed preflight before aggregate proof deserialization.
The DoS gate includes an oversized wrapper case with
`MAX_AGGREGATE_PROOF_BYTES + 1` bytes.

## Latency Gate

The enforced release-mode gate is
`snarkpack_dos_gate_valid_and_adversarial_paths_hold_thresholds` in
`crates/crypto/proof-aggregation/src/backend.rs`.

| path | p50 | p95 | p99 | invariant |
| --- | ---: | ---: | ---: | --- |
| valid verify | observed | observed | `<= 1,500 ms` | valid aggregate remains under the CI budget |
| mixed workload | observed | observed | `<= 1,500 ms` | one valid verify plus malformed, wrong-family, wrong-public-input, and oversized rejects |
| malformed wrapper reject | observed | observed | `<= 25 ms` and `<= 0.5 * valid p50` | wrapper shape failure before backend work |
| wrong-family reject | observed | observed | `<= 25 ms` and `<= 0.5 * valid p50` | statement digest mismatch before backend work |
| wrong-public-input reject | observed | observed | `<= 25 ms` and `<= 0.5 * valid p50` | statement digest mismatch before backend work |
| oversized wrapper reject | observed | observed | `<= 25 ms` and `<= 0.5 * valid p50` | size cap failure before backend work |

The asymmetry check is the security property: adversarial inputs must reject
with bounded work and must remain materially cheaper than a valid aggregate
verification. A regression that performs pairing work or full inner aggregate
deserialization before these shape checks makes the gate fail.

## Historical Local Baseline

The 2026-05-26 local release Criterion run is retained only as a calibration
note:

- aggregate transfer 64 proofs: `[253.62 ms, 317.93 ms]`
- aggregate consolidate 64 proofs: `[547.48 ms, 649.62 ms]`
- aggregate split 64 proofs: `[547.96 ms, 647.36 ms]`
- aggregate shielded ICS-20 withdrawal 64 proofs: `[267.38 ms, 343.47 ms]`
- verify transfer 64 proofs: `[69.876 ms, 107.85 ms]`
- verify consolidate 64 proofs: `[66.333 ms, 87.854 ms]`
- verify split 64 proofs: `[71.209 ms, 98.065 ms]`
- verify shielded ICS-20 withdrawal 64 proofs: `[70.095 ms, 100.72 ms]`
