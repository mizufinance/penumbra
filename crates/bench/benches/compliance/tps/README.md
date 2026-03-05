# TPS Benchmark (Persistent Corpus Workflow)

This directory uses a single persistent workflow for local consensus TPS benchmarking.

## One Script

Use only:

```bash
./scripts/tps/bench-simple.sh <command>
```

Commands:

- `prepare`: restore seed snapshot, fund/register, build corpora, save base snapshot.
- `run`: restore base snapshot, run regulated + unregulated benchmark.
- `append`: restore base snapshot, append transactions to both corpora, re-save base snapshot.
- `refresh`: rebuild corpora from seed snapshot and replace base snapshot.
- `verify`: restore base snapshot and verify corpus compatibility against observer.
- `status`: print snapshot/corpus/lineage metadata state.

## Default UX

```bash
./scripts/tps/bench-simple.sh prepare
./scripts/tps/bench-simple.sh run
./scripts/tps/bench-simple.sh append --count 4
./scripts/tps/bench-simple.sh verify
./scripts/tps/bench-simple.sh run
```

## Why This Is Reliable

- Single lineage: corpus is always built/appended from the same restored base snapshot.
- Snapshot is re-saved after append, keeping state + corpus aligned.
- Internal config is generated automatically in `tmp/tps-persistent.config.yaml`.
- Drift gate: run checks corpus lineage metadata (`lineage.json`) vs current height and fails fast (or auto-refreshes with `--auto-refresh`).

## Generated Outputs

Benchmark results are written to:

- `crates/bench/benches/compliance/tps/tps.csv`
- `crates/bench/benches/compliance/tps/run_summary.csv` (one minimal row per `bench-simple.sh run`)
- `crates/bench/benches/compliance/tps/profiles.csv`
- `crates/bench/benches/compliance/tps/runs/<run_id>/...`

`run_summary.csv` columns:
- run context: `timestamp`, `label`, `offered_tps`, `repeats`, `warmup_blocks`, `steady_blocks`, `target_block_time_ms`, `submit_workers`, `max_inflight`
- status: `run_exit`, `overall_status`
- per-scenario minimal result: `{unreg,reg}_rows`, `{unreg,reg}_peak_committed_tps`, `{unreg,reg}_status`

Static config templates kept in-repo:

- `crates/bench/benches/compliance/tps/config.example.yaml`
- `crates/bench/benches/compliance/tps/config.real-4v.example.yaml`

Corpus lineage metadata:

- `crates/bench/benches/compliance/tps/corpus/unregulated/lineage.json`
- `crates/bench/benches/compliance/tps/corpus/regulated/lineage.json`

## Notes

- Old orchestrators (`run-tps.sh`, `run-ladder.sh`, `run-corpus-reuse.sh`) are deprecated wrappers.
- This workflow is for local deterministic benchmarking with pre-proved transactions.
