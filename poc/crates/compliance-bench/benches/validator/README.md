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

Profile defaults:
- `quick`: `W1`, `S5`
- `deep`: `W2`, `S10`

Regression suite default:
- `W1`, `S5`

## Benches

| Bench name | Command | Output |
|---|---|---|
| `validator_flow` | `cargo bench --bench validator_flow` | `validator/validator.csv` + `validator/sections.csv` + `validator/sections/{binding_sig,spend_auth_sig,spend_extract,spend_extract.to_batch_item,output_extract,output_extract.ciphertext_parse,output_extract.to_batch_item,extract,legacy_batch_verify,snarkpack_verify,snarkpack_verify.deserialize,snarkpack_verify.tipa_ab,snarkpack_verify.tipa_c}.csv` |

TPS ownership is in the `tps` harness and the synthetic local optimization work is in
`pre_consensus`; `validator_flow` tracks verification kernel behavior only.
Use this bench for verifier-only extraction and aggregate-verify breakdowns, not for
end-to-end block throughput.

Phase-0 section output is intentionally filtered to material buckets. Low-signal
subsections are no longer timed or emitted.

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
- `validator/sections/spend_auth_sig.csv`
- `validator/sections/spend_extract.csv`
- `validator/sections/spend_extract.to_batch_item.csv`
- `validator/sections/output_extract.csv`
- `validator/sections/output_extract.ciphertext_parse.csv`
- `validator/sections/output_extract.to_batch_item.csv`
- `validator/sections/extract.csv`
- `validator/sections/legacy_batch_verify.csv`
- `validator/sections/snarkpack_verify.csv`
- `validator/sections/snarkpack_verify.deserialize.csv`
- `validator/sections/snarkpack_verify.tipa_ab.csv`
- `validator/sections/snarkpack_verify.tipa_c.csv`

Cross-category flow overview:
- `flows.csv` (contains `category=validator` rows from this bench)

Rows include run metadata:
- `profile`, `run_id`, `timestamp`, `git_rev`, `host_label`

## Phase 0 Baseline

```bash
BENCH_VERSION=local \
BENCH_SUITE=regression \
BENCH_WARMUP=1 \
BENCH_SAMPLES=5 \
BENCH_VALIDATOR_BATCH_SIZES=100,200 \
cargo bench -p penumbra-sdk-bench --bench validator_flow
```
