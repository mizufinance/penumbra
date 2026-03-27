# TPS Deployment Paths

This document captures the simplified deployment flow used by TPS benchmarking.

## Path A: Local Unregulated Ready Snapshot (single-validator)

Purpose:
- deterministic corpus reuse with no benchmark-time funding;
- prebuilt unregulated corpus aligned with a dedicated funded base state;
- fast restore-and-run iteration.

Entry point:
- `./poc/scripts/tps/fixtures.sh seed`
- `./poc/scripts/tps/fixtures.sh ready`
- `./poc/scripts/tps/run-stage-bench.sh`

Typical loop:

```bash
./poc/scripts/tps/fixtures.sh seed --wallet-home ... --asset ... --count 1000 --to-address ... --out ...
./poc/scripts/tps/fixtures.sh ready --wallet-home ... --asset ... --count 1000 --to-address ... --out ...
./poc/scripts/tps/run-stage-bench.sh mempool --corpus ...
./poc/scripts/tps/run-stage-bench.sh builder single --corpus ...
```

Notes:
- `fixtures.sh seed` creates the funded genesis-backed seed snapshot;
- `fixtures.sh ready` pays the proof-build cost once and saves the ready snapshot;
- `run-stage-bench.sh` runs the retained stage-study binaries directly.

## Path B: External Validator Campaign (4v+)

Purpose:
- measure committed TPS on multi-validator deployments;
- compare regulated vs unregulated behavior under realistic networking.

Recommended config template:
- `config.real-4v.example.yaml`

Command:

```bash
cargo run --release --manifest-path poc/Cargo.toml -p penumbra-sdk-compliance-bench --bin compliance_tps -- \
  run --config poc/crates/compliance-bench/benches/compliance/tps/config.real-4v.example.yaml
```

Prerequisites:
- all `pd_endpoints` and `observer_endpoint` are reachable;
- corpora referenced in the config were built against that same target chain.

## Integration Direction

- Keep `compliance_tps` as the load/measurement engine.
- Keep orchestration in shell scripts under `poc/scripts/tps`.
- Feed run metadata (`run_id`, profile, git revision, host label) into external dashboards if needed.
