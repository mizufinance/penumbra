# Embedded artifacts

Bankd links the `shieldd` static library through `crates/bin/shieldd/include/shieldd.h`.
Execution methods and protobuf messages are shared contracts. Bankd serves public
queries and executes IBC; Shieldd reads committed snapshots directly. Host
withdrawals use the shared `shielded_withdrawal` proof family and return value
to Bankd for transfer or execution.

Build explicit deliverables from the Shieldd source root:

```sh
python3 scripts/stage_artifacts.py native
python3 scripts/stage_artifacts.py provers
python3 scripts/stage_artifacts.py audit
python3 scripts/stage_artifacts.py verify
```

The output is `target/shieldd`: `include/shieldd.h`, `lib/libshieldd.a`,
`bin/` tools, `lib/gnark/` prover libraries, and `manifest.json`. Native builds
need no prover hydration. Prover builds use the existing Git-backed proving keys;
SR1CS hydration remains an explicit constraint/proof-test operation.

Copy the complete staged directory to any location. Builders discover libraries
relative to their executable, or through `SHIELDD_ARTIFACT_ROOT`. Explicit
family-specific prover overrides remain available. The manifest records the
exact source revision, target platform, deliverable groups, and SHA-256 checksums.
Verify with an independently selected revision before using downloaded artifacts:

```sh
python3 scripts/stage_artifacts.py verify --output /opt/shieldd \
  --revision "$SHIELDD_REVISION" --target x86_64-unknown-linux-gnu
```

Source copied under Bankd without its own Git metadata must receive
`SHIELDD_REVISION` (or `--revision`) from the importing build. The staging script
resolves source files relative to itself, not the enclosing repository root.
Bankd's `shieldd` Docker target exports only native artifacts; `shieldd-provers`
exports the proof builders, and `shieldd-audit` exports audit tooling.

CI compares uninterrupted execution with close/reopen and checkpoint reimport,
checks committed query bytes/proofs, spent markers and nonempty history, rejects
replayed host sources, and compares the next committed root. The same fixture
runs under a nested Bankd source directory to check source relocation.
Bankd owns real transfer/withdrawal integration tests. Manual Rust proof replay
covers ignored release-gated cases; ordinary PR tests do not imply those ran.

The native decoder and minimized regression seeds remain in Shieldd. Campaigns,
corpora, focused decoder proofs, and certification evidence are owned by
[Shieldd Security](https://github.com/mizufinance/shieldd-security).
