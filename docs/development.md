# Development and verification

Use `nix develop` for the repository toolchain, or install the Rust version in
`rust-toolchain.toml`, Go from `tools/gnark/go.mod`, and a CGO-capable C compiler.
Direct host integration tests use temporary storage. Bankd owns the live localnet and mobile/admin/audit smoke workflows.
Voluntary disclosure proving uses the [local gnark backend](disclosure.md#local-proving-artifacts).

| Command | Coverage |
| --- | --- |
| `just check` | Native compilation, formatting, and focused aggregation invariants |
| `just test` | Ordinary Rust tests; ignored tests are excluded |
| `just go-check` | Gnark Go formatting, compilation, tests, and vet |
| `just gnark-proof-tests` | Fast witness, statement, and Go checks |
| `just note-seizure-proof-tests` | Real seizure proofs and host state transitions |
| `just gnark-proof-tests-slow` | Real release-mode proofs using both library and daemon transports |
| `just snarkpack-slow` | Release-mode oracle and two-way aggregation interoperability |
| `just snarkpack-dos-gate` | Release latency and bounded-size rejection gate |
| `just proto-check` | Deterministic Rust/Go generation and schema closure |
| `just features-check` | Independent native crate feature builds |
| `just wasm-check` | Supported domain crates without component features on WASM |
| `cargo test -p shieldd-sdk-app-tests --tests -- --test-threads=1` | Host lifecycle, transfers, wallet planning, sweep, and storage query proofs |

## Real proof tests

Many Rust proof-generating unit tests are explicitly ignored. Ordinary app
integration tests also build real transactions and can require staged prover
artifacts; Go tests include both solver checks and explicit real proofs. `just gnark-proof-tests-slow`
selects only these tests in release mode and validates their prerequisites.
It exercises Transfer, both NoteReshape families, the shared withdrawal proof and host withdrawal caller, and
daemon-backed NoteSeizure. Missing artifacts or transports fail the command.
Fixture-blessing tests remain separate and are never selected by this command.

## Scanner

`cargo test -p shieldd-sdk-compliance --lib` covers atomic block persistence,
restart/replay, reorg rollback, bounded invalid outcomes, and audit validation.
The transaction crate's
`compliance_scanner_transaction_id_matches_canonical_transaction_id` test checks
scanner output identities against `Transaction::id()`.

Scanner databases use a schema guard. Recreate incompatible development state;
there is no migration or version-adoption path.

## Orbis

`just orbis-integration-up` builds the offline tools and starts the pinned
Orbis/Vera Compose stack. `just orbis-integration-setup-ring /tmp/orbis-state.json`
creates the test ring and policy. Use `just orbis-integration-down` for cleanup.
The retained Docker workflow requires Docker Compose v2.

## Builds and features

Use `just proto` to regenerate bindings and `just proto-check` to verify them.
`proto/codegen.json` lists retained roots; imports form the Rust/Go schema closure.
The generator checks the pinned protoc version and compiles its generated Go output.

Native proof construction is opt-in with `prover` on shielded-pool and transaction.
`bundled-proving-keys` includes proving; external artifacts need `prover` alone.
Compliance `scanner` enables SQLite and worker dependencies without `component`.
View `rpc` enables its historical-witness RPC adapter. Isolated builds matter:
workspace feature unification can conceal missing feature declarations.

[Embedded artifacts](embedded-artifacts.md) defines native/prover/audit staging
and relocation. [AGENTS.md](../AGENTS.md) defines resource limits, prototype
contracts and the rule to pause heavy verification after a resource interruption.
Run one heavy job at a time with Rust/Rayon/Go parallelism bounded at two;
run expensive proof tests serially.

Benchmarks, fuzz campaigns and fixture generation are explicit developer tasks.
Keep independent reference/property cases and regression seeds. Fixture blessing
changes frozen vectors and is separate from correctness verification. Do not infer
real proof, release, platform or vendor coverage from an ordinary workspace run.

`just gnark-profile` runs opt-in constraint diagnostics. `just gnark-bless-seizure`
regenerates the frozen seizure witness; ordinary Go tests cannot write it.
Rust witness blessing remains explicitly ignored and selected by exact name.
