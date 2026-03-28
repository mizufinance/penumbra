# TPS Benchmark

This directory is the strict-only TPS harness.

Valid proofs are the only accepted benchmark policy. Replayable or bypassed proof paths are removed from the harness, scripts, tracked artifacts, and canonical docs.

## Main Entry Points

Scripts live at `scripts/tps/` in the repo root (not under `poc/`):

- `scripts/tps/fixtures.sh`
- `scripts/tps/create-unreg-ready.sh`
- `scripts/tps/run-stage-bench.sh`
- `scripts/tps/run-stage-bench-remote.sh`

## Current Benchmark Split

The retained workflow is stage-separated:

1. mempool
2. building
3. validation
4. execution

Current retained strict anchors:

- mempool direct `1k`: about `998 tx/s`
- builder local warmed `1k`: about `842 tx/s` at `segment_tx_count=200`, `threads=1`
- builder VM `64`-core strict `10k`: about `865 tx/s` at `segment_tx_count=128`, `threads=1`
- validation warmed strict `1k`: about `6191.65 tx/s`

Interpretation:

- mempool is ahead of builder
- validation is ahead of builder
- the active wall is still builder proof aggregation time

## Common Flows

Provision a strict unregulated corpus:

```bash
scripts/tps/create-unreg-ready.sh --count 1000
```

Run stage-bench:

```bash
scripts/tps/run-stage-bench.sh
```

## Document Map

- [tps-scaling.md](compliance-docs/tps-scaling.md)
- [tps-ledger.md](compliance-docs/tps-ledger.md)
- [proof-aggregation-analysis.md](compliance-docs/proof-aggregation-analysis.md)
