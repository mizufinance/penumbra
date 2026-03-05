# Compliance Benchmarks

Compliance benchmarking is split into categories so we can keep startup runs fast while preserving deep diagnostics.

## Categories

| Category | Purpose | Main bench |
|---|---|---|
| `client` | TX construction and proving workload | `client_flow` |
| `scanner` | Detection/decryption scanning workload | `scanner_flow` |
| `validator` | Stateless verification kernel costs | `validator_flow` |
| `node_abci` | Cross-pass ABCI flow + throughput (TPS owner) | `node_abci_flow` |

Narrow benches remain available for targeted deep analysis (`*_crypto`, `*_proofs`, `*_verification`, etc.).

## External TPS Harness

For real-cluster consensus throughput (from async broadcast to committed blocks), use:

- `crates/bench/benches/compliance/tps/README.md`
- `cargo run --release -p penumbra-sdk-bench --bin compliance_tps -- ...`

This harness uses prebuilt corpora and does not include proving in the measured window.

## Runtime Profiles

| Variable | Values | Default | Effect |
|---|---|---|---|
| `BENCH_PROFILE` | `quick`, `deep` | `quick` | Scenario matrix + default sampling |
| `BENCH_SUITE` | `complete`, `regression` | `complete` | Coverage (expanded vs canonical) |
| `BENCH_WARMUP` | integer | profile default | Override warmup count |
| `BENCH_SAMPLES` | integer | profile default | Override sample count |

Profile defaults:
- `quick`: warmup `1`, samples `5`
- `deep`: warmup `3`, samples `20`

Regression suite defaults (when `BENCH_SUITE=regression` and no explicit overrides):
- warmup `2`, samples `10`

## Versioning

| Variable | Values | Default |
|---|---|---|
| `BENCH_VERSION` | `base`, `dev`, `local` | N/A |

Bench runs are single-version only. For real branch-to-branch comparisons, run each version from its matching worktree/branch with `BENCH_VERSION` set.
`BENCH_VERSION` is required and has no default; set it to one of `base`, `dev`, or `local`.

## Reporting Model

| Variable | Values | Default | Effect |
|---|---|---|---|
| `BENCH_OUTPUT` | `human`, `json` | `human` | Table or NDJSON stdout |

Flow outputs are written beside each category bench files:
- `benches/compliance/flows.csv` (all categories overview)
- `benches/compliance/<category>/<category>.csv`
- `benches/compliance/<category>/sections.csv`
- `benches/compliance/<category>/sections/<section>.csv`
- `benches/compliance/<category>/sections/<section>_subsections/<subsection>.csv` (only when the category exposes true nested breakdowns)

Each CSV is single-layer (no duplicated rows across files) and includes run metadata fields (`profile`, `run_id`, `timestamp`, `git_rev`, `host_label`).

CSV rows are sorted on write/append by dimensions first, then version (`base`, `dev`, `local`).

## Commands

```bash
# Regression test run (canonical suite)
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench client_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench scanner_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench validator_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench node_abci_flow

# Deep investigation run
BENCH_VERSION=local BENCH_PROFILE=deep cargo bench -p penumbra-sdk-bench --bench node_abci_flow
```
