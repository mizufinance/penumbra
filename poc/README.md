# POC Workspace

This nested workspace contains non-production work from the `recursive-verification` branch:

- `crates/preconsensus`: POC runtime and stage-study support types
- `crates/stage-bench`: mempool, builder, validation, execution, and proof-stage labs
- `crates/compliance-bench`: client, scanner, validator, local-fullnode, and TPS correlation flows
- `tools/`: isolated experiments that are not part of the supported bench surface
- `scripts/tps/`: retained fixture and stage-bench runners

The root workspace remains production-oriented. Build this workspace separately with:

```bash
cargo build --workspace --manifest-path poc/Cargo.toml
```

Supported POC entrypoints:

```bash
poc/scripts/tps/fixtures.sh
poc/scripts/tps/run-stage-bench.sh
poc/scripts/tps/run-stage-bench-remote.sh
```
