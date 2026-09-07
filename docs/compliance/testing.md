# Compliance verification

Use `nix develop` for the repository toolchain, or install the Rust version in
`rust-toolchain.toml`, Go from `tools/gnark/go.mod`, and a CGO-capable C compiler.
The process-compose smoke test creates its own temporary development state.

| Command | Coverage |
| --- | --- |
| `just check` | Native compilation, formatting, and focused aggregation invariants |
| `just test` | Ordinary Rust tests; ignored tests are excluded |
| `just go-check` | Gnark Go formatting, compilation, tests, and vet |
| `just gnark-proof-tests` | Fast witness, statement, and Go checks |
| `just gnark-proof-tests-slow` | Real release-mode proofs using both library and daemon transports |
| `just snarkpack-slow` | Release-mode oracle and two-way aggregation interoperability |
| `just snarkpack-dos-gate` | Release latency and bounded-size rejection gate |
| `just proto-check` | Deterministic Rust/Go generation and schema closure |
| `just features-check` | Independent native crate feature builds |
| `just wasm-check` | Supported domain crates without component features on WASM |
| `just smoke` | Fresh process-compose network, wallet, CLI, and node integration |

## Real proof tests

Proof-generating unit tests are explicitly ignored. `just gnark-proof-tests-slow`
selects only these tests in release mode and validates their prerequisites.
It exercises Transfer, both NoteReshape families, and withdrawal, including both
withdrawal callers. Missing artifacts or transports fail the command.
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

`just orbis-integration-up` builds the binaries and starts Shieldd and the pinned
Orbis/Vera Compose stack. `just orbis-integration-setup-ring /tmp/orbis-state.json`
creates the test ring and policy. Use `just orbis-integration-down` for cleanup.
The retained Docker workflow requires Docker Compose v2.
