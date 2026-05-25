# POC Workspace

This nested workspace contains non-production work centered on the remaining
preconsensus prototype surface:

- `crates/preconsensus`: POC runtime and stage-study support types
- `tools/`: isolated experiments that are not part of the supported bench surface

The older `stage-bench` and `compliance-bench` labs were removed from this
workspace. The retained `scripts/tps/` entrypoints now only explain that those
benches are no longer available here.

The root workspace remains production-oriented. Build this workspace separately with:

```bash
cargo build --workspace --manifest-path poc/Cargo.toml
```

Supported POC entrypoints:

```bash
cargo build --workspace --manifest-path poc/Cargo.toml
```
