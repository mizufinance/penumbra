# Compliance Benchmarks

Measures the performance impact of the compliance system (v0.1) vs vanilla Penumbra (v0).

## Running

```bash
# All compliance benchmarks
cargo bench -p penumbra-sdk-bench

# Single benchmark
cargo bench -p penumbra-sdk-bench --bench <name>
```

`cargo bench` compiles with the `bench` profile (release optimizations). Results are only meaningful in release mode.

## Machine

Results in this repo were collected on:

- **CPU**: Apple M4 Pro (14-core)
- **RAM**: 48 GB
- **OS**: macOS 15.5

Results vary by machine. When comparing, always re-run all benchmarks on the same hardware.

## Versions

| Version | Description |
|---------|-------------|
| **v0** | Vanilla Penumbra (pre-compliance, tag v2.0.4). Smaller circuits, no ciphertext or DLEQ. |
| **v0.1** | Compliance-enabled. Larger circuits, adds encrypted ciphertexts, DLEQ proofs, and compliance tree witnesses. |

For benchmarks that compare v0 vs v0.1, the v0 baseline uses the vanilla circuit definitions compiled alongside the current code (same proving system, same hardware). Block-level benchmarks label v0 as "estimated" since the current `App` pipeline requires compliance fields.

## CSV Columns

All result CSVs share these stat columns:

| Column | Description |
|--------|-------------|
| `version` | `v0` (vanilla) or `v0.1` (compliance) |
| `mean_ms` | Average wall-clock time across all samples (milliseconds) |
| `median_ms` | Median time (50th percentile) |
| `std_dev_ms` | Standard deviation across samples |
| `samples` | Number of timed iterations. Higher = more reliable. |
| `constraints` | ZK circuit constraint count (R1CS). Empty when not applicable. Larger circuits = slower proving but verification time scales weakly with constraint count. |

Dimension columns (e.g. `circuit`, `operation`, `mode`) vary per benchmark and are documented in each subdirectory README.

## Infrastructure

All compliance benchmarks use a custom `bench_runner` harness (`crates/bench/src/lib.rs`), not Criterion. Each benchmark binary:
1. Runs warmup iterations (not timed)
2. Runs N timed samples, collecting wall-clock milliseconds
3. Computes stats (mean, median, min, max, std_dev)
4. Prints a formatted table to stdout
5. Writes a CSV to the co-located `results/` directory

## Subdirectories

| Directory | Role | Benchmarks |
|-----------|------|------------|
| [`client/`](client/) | Client-side TX building | Crypto primitives, enrichment overhead, end-to-end proof generation |
| [`scanner/`](scanner/) | Issuer-side scanning | Decryption tiers, tree operations, block scanning throughput |
| [`validator/`](validator/) | Validator-side verification | Proof verification, verification pipeline, batch flow, block TPS |
