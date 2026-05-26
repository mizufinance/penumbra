# SnarkPack Benchmark Thresholds

Status: provisional local baseline. These thresholds should be replaced with
CI-hardware baselines before they are used as hard gates.

Date: 2026-05-26
Machine: local developer machine

## Commands

- size report:
  `cargo test -p penumbra-sdk-proof-aggregation aggregate_proof_size_report --lib -- --ignored --nocapture`
- release benchmarks:
  `cargo bench -p penumbra-sdk-bench --bench snarkpack`

## Size Baseline

Observed max wrapped aggregate proof bytes: `31,946`

Chosen cap:

- formula: `round_up_to_64k(observed_max_wrapped_valid_proof_bytes * 4)`
- minimum: `64 KiB`
- maximum: `1 MiB`
- result: `131,072` bytes

Observed wrapped sizes were identical across the sampled transfer,
consolidate, split, and shielded ICS-20 withdrawal families:

- count `1`: `3,146` bytes
- count `2`: `7,946` bytes
- count `4`: `12,746` bytes
- count `8`: `17,546` bytes
- count `64`: `31,946` bytes

## Latency Baselines

The release benchmark command completed locally. Representative Criterion
intervals for the sampled 64-proof cases:

- aggregate transfer: `[253.62 ms, 317.93 ms]`
- aggregate consolidate: `[547.48 ms, 649.62 ms]`
- aggregate split: `[547.96 ms, 647.36 ms]`
- aggregate shielded ICS-20 withdrawal: `[267.38 ms, 343.47 ms]`
- verify transfer: `[69.876 ms, 107.85 ms]`
- verify consolidate: `[66.333 ms, 87.854 ms]`
- verify split: `[71.209 ms, 98.065 ms]`
- verify shielded ICS-20 withdrawal: `[70.095 ms, 100.72 ms]`

Malformed-rejection latency thresholds are not yet fixed. Re-collect p50/p95/p99
numbers on target CI hardware, then add explicit fail thresholds here.
