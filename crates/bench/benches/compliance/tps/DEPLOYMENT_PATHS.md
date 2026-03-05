# TPS Deployment Paths

This document captures the simplified deployment flow used by TPS benchmarking.

## Path A: Local Persistent Workflow (single-validator)

Purpose:
- deterministic corpus reuse;
- snapshot/corpus alignment across runs;
- fast local iteration without manual config churn.

Entry point:
- `./scripts/tps/bench-simple.sh`

Typical loop:

```bash
./scripts/tps/bench-simple.sh prepare
./scripts/tps/bench-simple.sh run
./scripts/tps/bench-simple.sh append --count 4
./scripts/tps/bench-simple.sh run
```

Notes:
- `prepare` builds corpora once and saves the base snapshot;
- `run` always restores the base snapshot before benchmarking;
- `append` mutates corpus and re-saves the base snapshot to keep lineage valid.

## Path B: External Validator Campaign (4v+)

Purpose:
- measure committed TPS on multi-validator deployments;
- compare regulated vs unregulated behavior under realistic networking.

Recommended config template:
- `config.real-4v.example.yaml`

Command:

```bash
target/release/compliance_tps run --config crates/bench/benches/compliance/tps/config.real-4v.example.yaml
```

Prerequisites:
- all `pd_endpoints` and `observer_endpoint` are reachable;
- corpora referenced in the config were built against that same target chain.

## Integration Direction

- Keep `compliance_tps` as the load/measurement engine.
- Keep orchestration in shell scripts under `scripts/tps`.
- Feed run metadata (`run_id`, profile, git revision, host label) into external dashboards if needed.
