# Client Benchmarks

The client category measures the transaction construction and proving workload.

## Benches

| Bench name | Command | Output |
|---|---|---|
| `client_flow` | `cargo bench --bench client_flow` | `client/client.csv` + `client/sections.csv` + `client/sections/{enrich,authorize,tx_build}.csv` |
| `client_crypto` | `cargo bench --bench client_crypto` | `client/crypto.csv` |

## Flow Outputs

`client_flow` supports profile/report/version env controls from the compliance top-level README.

`client.csv` contains top-level client flow KPIs (`total`, `prove`) for the selected suite.
`sections.csv` contains the per-section overview across `enrich`, `authorize`, and `tx_build`.

Per-section files contain section-level rows:
- `client/sections/enrich.csv`
- `client/sections/authorize.csv`
- `client/sections/tx_build.csv`

Cross-category flow overview:
- `flows.csv` (contains `category=client` rows from this bench)

`BENCH_SUITE=regression` runs one canonical variant per section.
`BENCH_SUITE=complete` adds expanded flow variants (for example serial mode and multi-spend scenario).
