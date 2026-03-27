# Compliance Benchmarks

Surviving benchmark categories:

| Category | Purpose | Entry |
| --- | --- | --- |
| `client` | tx construction and proving workload | `client_flow` |
| `scanner` | detection and decryption workload | `scanner_flow` |
| `validator` | stateless verification kernel costs | `validator_flow` |
| `local_fullnode` | local real-node correlation harness | `local_fullnode_flow` |

TPS and builder labs live in `poc/crates/stage-bench`, not here.

Single TPS handoff doc:

- [tps-scaling.md](compliance-docs/tps-scaling.md)

## Runtime Profiles

| Variable | Values | Default | Effect |
| --- | --- | --- | --- |
| `BENCH_PROFILE` | `quick`, `deep` | `quick` | Scenario matrix + default sampling |
| `BENCH_SUITE` | `complete`, `regression` | `complete` | Coverage |
| `BENCH_WARMUP` | integer | profile default | Override warmup count |
| `BENCH_SAMPLES` | integer | profile default | Override sample count |

## Versioning

| Variable | Values | Default |
| --- | --- | --- |
| `BENCH_VERSION` | `base`, `dev`, `local` | N/A |

## Reporting Model

Flow outputs are written beside each category bench files:

- `benches/compliance/flows.csv`
- `benches/compliance/<category>/<category>.csv`
- `benches/compliance/<category>/sections.csv`
- `benches/compliance/<category>/sections/<section>.csv`

## Example Runs

```bash
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=1 BENCH_SAMPLES=5 cargo bench --manifest-path poc/Cargo.toml -p penumbra-sdk-compliance-bench --bench client_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=1 BENCH_SAMPLES=5 cargo bench --manifest-path poc/Cargo.toml -p penumbra-sdk-compliance-bench --bench scanner_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=1 BENCH_SAMPLES=5 cargo bench --manifest-path poc/Cargo.toml -p penumbra-sdk-compliance-bench --bench validator_flow
BENCH_VERSION=local BENCH_LOCAL_FULLNODE_OFFERED_TPS=20 BENCH_LOCAL_FULLNODE_MIN_STEADY_COMMITS=0 cargo bench --manifest-path poc/Cargo.toml -p penumbra-sdk-compliance-bench --bench local_fullnode_flow
```
