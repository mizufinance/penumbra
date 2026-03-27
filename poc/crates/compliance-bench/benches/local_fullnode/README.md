# Local Full-Node Correlation Benchmark

`local_fullnode_flow` bridges the gap between:

- `lookahead_builder_lab`: pure-slack builder-only architecture measurement
- `compliance_tps`: real local node throughput execution

It reuses the existing local `pd + CometBFT` workflow from
[bench-simple.sh](/Users/antoinecyr/Documents/Source/penumbra/scripts/tps/bench-simple.sh),
then emits benchmark-style CSVs so the local runtime can be compared against a
synthetic pre-consensus reference.

## Why This Bench Exists

Use this bench to answer:

- how much of the local runtime matches the synthetic optimization signal
- how large the remaining runtime gap is above the app-only path
- whether future engine changes improve or worsen that gap

This category is intentionally separate from the builder-only lab.

## Current Runtime Finding

This bench now serves mainly as a correlation and handoff surface, not the primary bottleneck
finder.

Current grounded conclusion:

- the synthetic-to-real gap is not primarily a `CheckTx` problem anymore
- at low load, local runtime is dominated by block cadence / Comet scheduling
- at higher load, the remaining real work is in:
  - `prepare_proposal`
  - `deliver_tx + end_block + commit`
  - repeated runtime work between mempool admission, proposal construction, and final execution

For actual runtime diagnosis, use the reusable snapshot path plus
[run-under-microscope.sh](/Users/antoinecyr/Documents/Source/penumbra/scripts/tps/run-under-microscope.sh).

## Current Adapter

- engine: `comet_local`
- driver: `bench_simple`

The reporting shape is engine-neutral so a future Gordian-backed adapter can
reuse the same category and correlation fields.

## Outputs

- `benches/compliance/local_fullnode/local_fullnode.csv`
- `benches/compliance/local_fullnode/sections.csv`
- `benches/compliance/local_fullnode/sections/runtime.csv`
- `benches/compliance/local_fullnode/sections/latency.csv`
- `benches/compliance/local_fullnode/sections/correlation.csv`
- `benches/compliance/flows.csv`

## Example Run

```bash
BENCH_VERSION=local \
BENCH_LOCAL_FULLNODE_OFFERED_TPS=20 \
BENCH_LOCAL_FULLNODE_WARMUP_BLOCKS=2 \
cargo bench -p penumbra-sdk-bench --bench local_fullnode_flow
```

The default runtime scenario is `unregulated`.

Useful overrides:

- `BENCH_LOCAL_FULLNODE_SCRIPT`
- `BENCH_LOCAL_FULLNODE_TARGET_BLOCK_TIME_MS`
- `BENCH_LOCAL_FULLNODE_SUBMIT_WORKERS`
- `BENCH_LOCAL_FULLNODE_MAX_INFLIGHT`
- `BENCH_LOCAL_FULLNODE_AUTO_REFRESH`
- `BENCH_LOCAL_FULLNODE_SYNTHETIC_TX_COUNT`
- `BENCH_LOCAL_FULLNODE_SYNTHETIC_MODE`
- `BENCH_LOCAL_FULLNODE_SYNTHETIC_CONCURRENCY`
- `BENCH_LOCAL_FULLNODE_SYNTHETIC_INDEXING_MODE`

## Current Starting Point

If you are starting a new runtime session:

1. do not begin with more synthetic optimization work
2. build or restore `local-unreg-10000-ready`
3. run the microscope on that artifact
4. prioritize only runtime-path changes
