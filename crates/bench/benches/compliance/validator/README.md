# Validator Benchmarks

Validator category focuses on kernel-level stateless verification costs.

## Environment Variables

| Variable | Values | Default |
|---|---|---|
| `BENCH_VERSION` | `base`, `dev`, `local` | N/A |
| `BENCH_PROFILE` | `quick`, `deep` | `quick` |
| `BENCH_SUITE` | `complete`, `regression` | `complete` |
| `BENCH_WARMUP` | integer | profile default |
| `BENCH_SAMPLES` | integer | profile default |
| `BENCH_OUTPUT` | `human`, `json` | `human` |

Note: `BENCH_VERSION` is required and must be explicitly set.

Profile defaults (W=warmup, S=samples):
- `quick`: `W1 S5`
- `deep`: `W3 S20`

## Benches

| Bench name | Command | Output |
|---|---|---|
| `validator_flow` | `cargo bench --bench validator_flow` | `validator/validator.csv` + `validator/sections.csv` + `validator/sections/{binding_sig,spend_path,output_path}.csv` |

TPS ownership is in `node_abci_flow`; `validator_flow` tracks verification kernel behavior.

## Flow Outputs

`BENCH_SUITE=complete` flow KPIs:
- `100/serial`
- `100/parallel`
- `per_tx/serial`
- `per_tx/parallel`
- `1/serial`
- `1/parallel`
- `prove_kpi/single`
- `ratio/parallel_over_serial`

`BENCH_SUITE=regression` flow KPIs:
- `100/parallel`
- `per_tx/parallel`

Flow file:
- `validator/validator.csv`
- `validator/sections.csv` (section overview)

Sections file:
- `validator/sections/binding_sig.csv`
- `validator/sections/spend_path.csv`
- `validator/sections/output_path.csv`

Cross-category flow overview:
- `flows.csv` (contains `category=validator` rows from this bench)

Rows include run metadata:
- `profile`, `run_id`, `timestamp`, `git_rev`, `host_label`
