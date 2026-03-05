# Node ABCI Benchmarks

`node_abci` captures representative cross-pass validator work.

## Bench

| Bench name | Command | Output |
|---|---|---|
| `node_abci_flow` | `cargo bench --bench node_abci_flow` | `node_abci/node_abci.csv` + `node_abci/sections.csv` + `node_abci/sections/{checktx,prepare_proposal,process_proposal,deliver_tx}.csv` + `node_abci/sections/process_proposal_subsections/{tx_decode,tx_stateless,tx_historical}.csv` |

## KPI Ownership

TPS and per-block throughput KPIs live here (not in validator kernel flow benches).

## KPI Model

Flow KPIs (`node_abci.csv`) are top-level:
- `abci_roundtrip latency`
- `block tps`
- `block per_tx_ms` (complete suite only)

Section KPIs (per-section files) are stage-level:
- `checktx` (warm always, cold in complete)
- `prepare_proposal`
- `process_proposal`
- `deliver_tx` (warm always, cold in complete)

## Flow Outputs

Flow file:
- `node_abci/node_abci.csv`
- `node_abci/sections.csv` (section overview)

Sections file:
- `node_abci/sections/checktx.csv`
- `node_abci/sections/prepare_proposal.csv`
- `node_abci/sections/process_proposal.csv`
- `node_abci/sections/deliver_tx.csv`

Process proposal subsections:
- `node_abci/sections/process_proposal_subsections/tx_decode.csv`
- `node_abci/sections/process_proposal_subsections/tx_stateless.csv`
- `node_abci/sections/process_proposal_subsections/tx_historical.csv`

Cross-category flow overview:
- `flows.csv` (contains `category=node_abci` rows from this bench)

Rows include metadata fields:
- `profile`
- `run_id`
- `timestamp`
- `git_rev`
- `host_label`

Process-proposal subsection rows include `tx_decode`, `tx_stateless`, and `tx_historical`.

`BENCH_SUITE=regression` runs one canonical variant per section.
`BENCH_SUITE=complete` enables expanded section variants and block per-tx metric.

For external multi-node TPS campaigns, use the dedicated harness in
`benches/compliance/tps/README.md` (`compliance_tps` binary).
