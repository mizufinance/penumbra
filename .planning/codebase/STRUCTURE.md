# Codebase Structure

**Analysis Date:** 2026-05-12

## Directory Layout

```text
penumbra/
|-- Cargo.toml                 # Production Rust workspace root
|-- Cargo.lock                 # Locked Rust dependency graph
|-- justfile                   # Local and CI task recipes
|-- rust-toolchain.toml        # Rust toolchain pin
|-- README.md                  # Product overview and workspace notes
|-- AGENTS.md                  # Local engineering instructions for agents
|-- CLAUDE.md                  # Local engineering instructions for Claude workflows
|-- .cargo/                    # Cargo configuration
|-- .config/                   # Test/tool configuration
|-- .github/                   # GitHub Actions and templates
|-- .codex/                    # GSD/Codex skills, agents, workflows, references
|-- .planning/                 # GSD planning artifacts and generated codebase maps
|-- assets/                    # Bundled frontend/status zip assets embedded by `pd`
|-- crates/                    # Production Rust crates
|   |-- bin/                   # Runtime binaries: `pd`, `pcli`, `pclientd`, `pindexer`, Orbis tools
|   |-- core/                  # Core protocol/app/component/domain crates
|   |-- crypto/                # Cryptography, proof params, proof aggregation, TCT
|   |-- custody/               # Custody protocol and software/threshold KMS
|   |-- custody-ledger-usb/    # Ledger USB custody service
|   |-- proto/                 # Generated protobuf crate
|   |-- view/                  # Wallet view service, sync worker, SQLite storage
|   |-- wallet/                # Wallet transaction planning/build helpers
|   |-- util/                  # Operational utilities and reusable services
|   |-- test/                  # Shared test support crates
|   |-- bench/                 # Benchmarks
|   |-- bench-support/         # Benchmark support code
|   `-- misc/                  # Miscellaneous tools
|-- proto/                     # Source `.proto` definitions and vendored proto inputs
|-- docs/                      # mdBook/protocol/compliance/rustdoc/protobuf docs
|-- deployments/               # Devnet, container, systemd, smoke-test deployment assets
|-- scripts/                   # Repo-level operational scripts
|-- testnets/                  # Testnet genesis/config inputs
|-- tools/                     # Non-workspace and auxiliary tools, including gnark and proto compiler
|-- poc/                       # Separate non-production Cargo workspace
|-- tmp/                       # Local scratch/runtime state
`-- target/                    # Cargo build output
```

## Directory Purposes

**Root Workspace:**
- Purpose: Build and test the production Rust workspace.
- Contains: `Cargo.toml`, `Cargo.lock`, `justfile`, `rust-toolchain.toml`, `.cargo/config.toml`.
- Key files: `Cargo.toml`, `justfile`, `README.md`.

**`crates/bin`:**
- Purpose: Executable crates and integration CLI tools.
- Contains: `pd`, `pcli`, `pclientd`, `pindexer`, `orbis-audit`, `orbis-integration`.
- Key files: `crates/bin/pd/src/main.rs`, `crates/bin/pcli/src/main.rs`, `crates/bin/pclientd/src/lib.rs`, `crates/bin/pindexer/src/indexer_ext.rs`.

**`crates/core/app`:**
- Purpose: Penumbra ABCI application, app lifecycle, state machine orchestration, query route assembly.
- Contains: `app`, `server`, `rpc`, `action_handler`, `genesis`, `params`, `stateless_cache`.
- Key files: `crates/core/app/src/app/mod.rs`, `crates/core/app/src/server.rs`, `crates/core/app/src/server/consensus.rs`, `crates/core/app/src/rpc.rs`.

**`crates/cnidarium-component`:**
- Purpose: Shared component/action contracts for cnidarium-backed ABCI applications.
- Contains: `Component` and `ActionHandler` traits.
- Key files: `crates/cnidarium-component/src/component.rs`, `crates/cnidarium-component/src/action_handler.rs`.

**`crates/core/component`:**
- Purpose: Chain components that own protocol modules and component state.
- Contains: `compact-block`, `compliance`, `fee`, `governance`, `ibc`, `sct`, `shielded-pool`, `stake`.
- Key files: `crates/core/component/*/src/lib.rs`, `crates/core/component/*/src/state_key.rs`, `crates/core/component/*/src/component*`, `crates/core/component/*/src/event.rs`.

**`crates/core/transaction`:**
- Purpose: Transaction lifecycle modeling.
- Contains: action enums, action plans, transaction structs, views, authorization data, witness data, detection data, fee funding.
- Key files: `crates/core/transaction/src/lib.rs`, `crates/core/transaction/src/action.rs`, `crates/core/transaction/src/plan.rs`, `crates/core/transaction/src/view.rs`.

**`crates/core/asset`, `crates/core/keys`, `crates/core/num`, `crates/core/txhash`:**
- Purpose: Shared protocol domain primitives.
- Contains: asset metadata/value types, address/key types, amount/fixpoint/percentage types, transaction hash/auth/effect types.
- Key files: `crates/core/asset/src/lib.rs`, `crates/core/keys/src/lib.rs`, `crates/core/num/src/lib.rs`, `crates/core/txhash/src/lib.rs`.

**`crates/crypto`:**
- Purpose: Cryptographic primitives, proof parameters, proof aggregation, and commitment tree.
- Contains: `decaf377-fmd`, `decaf377-frost`, `decaf377-ka`, `eddy`, `proof-aggregation`, `proof-params`, `tct`.
- Key files: `crates/crypto/proof-params/src/lib.rs`, `crates/crypto/proof-aggregation/src/lib.rs`, `crates/crypto/tct/src/lib.rs`.

**`crates/proto`:**
- Purpose: Generated Rust protobuf crate and conversion helpers.
- Contains: generated `src/gen/*.rs`, serializers, `DomainType`, cnidarium state helpers, optional boxed gRPC service support.
- Key files: `crates/proto/src/lib.rs`, `crates/proto/src/protobuf.rs`, `crates/proto/src/state.rs`.

**`proto`:**
- Purpose: Source protobuf definitions.
- Contains: Penumbra proto packages under `proto/penumbra/penumbra/**/v1/*.proto` and vendored proto inputs under `proto/rust-vendored`.
- Key files: `proto/penumbra/penumbra/core/transaction/v1/transaction.proto`, `proto/penumbra/penumbra/view/v1/view.proto`, `proto/penumbra/penumbra/custody/v1/custody.proto`.

**`crates/view`:**
- Purpose: View service, compact-block sync worker, note planning, wallet-facing queries, and local SQLite persistence.
- Contains: `client`, `service`, `worker`, `storage`, `sync`, `note_manager`, compliance tree support.
- Key files: `crates/view/src/service.rs`, `crates/view/src/worker.rs`, `crates/view/src/storage.rs`, `crates/view/src/storage/schema.sql`.

**`crates/wallet`:**
- Purpose: Wallet transaction plan helpers and build orchestration.
- Contains: `build_transaction` and wallet planning utilities.
- Key files: `crates/wallet/src/build.rs`, `crates/wallet/src/plan.rs`.

**`crates/custody` and `crates/custody-ledger-usb`:**
- Purpose: Signing service abstractions and implementations.
- Contains: custody client trait, request types, policies, soft KMS, threshold custody, encrypted config, Ledger service.
- Key files: `crates/custody/src/client.rs`, `crates/custody/src/soft_kms.rs`, `crates/custody/src/policy.rs`, `crates/custody-ledger-usb/src/lib.rs`.

**`crates/util`:**
- Purpose: Shared operational libraries used by binaries.
- Contains: `auto-https`, `cometindex`, `orbis-client`, `tendermint-proxy`, `tower-trace`.
- Key files: `crates/util/tendermint-proxy/src/lib.rs`, `crates/util/cometindex/src/lib.rs`, `crates/util/auto-https/src/lib.rs`.

**`crates/test` and `crates/core/app-tests`:**
- Purpose: Shared test harnesses and app integration tests.
- Contains: mock client, mock consensus, mock Tendermint proxy, tracing subscriber, app-level tests.
- Key files: `crates/test/mock-consensus/src/lib.rs`, `crates/test/mock-client/src/lib.rs`, `crates/core/app-tests/Cargo.toml`.

**`tools`:**
- Purpose: Auxiliary tooling outside the main production workspace.
- Contains: gnark Go proving runtime, proto compiler, `picturesque`.
- Key files: `tools/gnark/README.md`, `tools/proto-compiler/README.md`, `tools/picturesque/src/main.rs`.

**`poc`:**
- Purpose: Separate non-production workspace for preconsensus prototypes.
- Contains: nested Cargo workspace and `crates/preconsensus`.
- Key files: `poc/Cargo.toml`, `poc/README.md`, `poc/crates/preconsensus/Cargo.toml`.

**`docs`:**
- Purpose: Protocol, guide, compliance, rustdoc, protobuf, and transfer-circuit docs.
- Contains: mdBook sources and static documentation config.
- Key files: `docs/protocol/src/penumbra.md`, `docs/protocol/src/transactions.md`, `docs/compliance/flow.md`.

**`deployments`, `scripts`, `testnets`:**
- Purpose: Local devnet, CI smoke, container, systemd, testnet, and operational automation.
- Contains: deployment docs, scripts, compose/process-compose config, testnet validators/allocations.
- Key files: `deployments/README.md`, `deployments/scripts/run-local-devnet.sh`, `scripts/penumbra-up.sh`, `testnets/validators-ci.json`.

**`.codex`:**
- Purpose: GSD/Codex workflow layer.
- Contains: project skills, agents, workflow definitions, templates, references, hooks.
- Key files: `.codex/skills/gsd-map-codebase/SKILL.md`, `.codex/agents/gsd-codebase-mapper.md`, `.codex/get-shit-done/workflows/map-codebase.md`.

**`.planning`:**
- Purpose: Generated GSD planning and codebase intelligence artifacts.
- Contains: codebase maps under `.planning/codebase`.
- Key files: `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/STRUCTURE.md`.

## Key File Locations

**Entry Points:**
- `crates/bin/pd/src/main.rs`: Full node process startup, ABCI/gRPC serving, metrics.
- `crates/bin/pcli/src/main.rs`: User CLI startup and command dispatch.
- `crates/bin/pclientd/src/main.rs`: View/custody daemon startup.
- `crates/bin/pclientd/src/lib.rs`: `pclientd` config, init/start/reset/load-registry logic.
- `crates/bin/pindexer/src/main.rs`: Indexer process startup.
- `crates/bin/orbis-audit/src/main.rs`: Orbis audit CLI.
- `crates/bin/orbis-integration/src/main.rs`: Penumbra/Orbis integration CLI.

**Configuration:**
- `Cargo.toml`: Workspace members, exclusions, shared dependencies, release metadata.
- `rust-toolchain.toml`: Rust toolchain selection.
- `.cargo/config.toml`: Cargo target linker/rustflags.
- `.config/nextest.toml`: Nextest configuration.
- `justfile`: Local and CI recipe entry points.
- `deployments/compose/*.yml`: Local deployment compose/process-compose configuration. Do not read or quote secret-bearing values from compose files.

**Core Logic:**
- `crates/core/app/src/app/mod.rs`: State machine lifecycle and transaction execution.
- `crates/core/app/src/server.rs`: ABCI server service assembly.
- `crates/core/app/src/server/consensus.rs`: CometBFT consensus request handling.
- `crates/core/app/src/server/mempool.rs`: Mempool `CheckTx` handling.
- `crates/core/app/src/rpc.rs`: Node gRPC route assembly.
- `crates/cnidarium-component/src/action_handler.rs`: Transaction action validity/execution contract.
- `crates/cnidarium-component/src/component.rs`: Block/epoch component contract.
- `crates/core/component/*/src/component*`: Component-specific app logic.
- `crates/core/component/*/src/state_key.rs`: Component-owned durable state keys.
- `crates/core/transaction/src/lib.rs`: Transaction lifecycle exports.

**Protocol and Wire Types:**
- `proto/penumbra/penumbra/**/v1/*.proto`: Source protobuf definitions.
- `crates/proto/src/lib.rs`: Generated protobuf module tree and re-exports.
- `crates/proto/src/protobuf.rs`: `DomainType` conversion trait.
- `crates/proto/src/gen/*.rs`: Generated Rust protobuf code.

**Client and Wallet:**
- `crates/view/src/service.rs`: View gRPC service and wallet-facing methods.
- `crates/view/src/worker.rs`: Compact block sync loop.
- `crates/view/src/sync.rs`: Compact block scanning.
- `crates/view/src/storage.rs`: SQLite storage interface.
- `crates/view/src/storage/schema.sql`: View DB schema.
- `crates/view/src/note_manager.rs`: Note selection, transfer/consolidate/split/withdrawal planning.
- `crates/wallet/src/build.rs`: Custody, witness, and transaction build orchestration.

**Custody:**
- `crates/custody/src/client.rs`: `CustodyClient` trait.
- `crates/custody/src/soft_kms.rs`: Software KMS implementation.
- `crates/custody/src/policy.rs`: Authorization policy checks.
- `crates/custody/src/threshold.rs`: Threshold custody service.
- `crates/custody-ledger-usb/src/lib.rs`: Ledger custody service.

**Indexing and Operations:**
- `crates/bin/pindexer/src/indexer_ext.rs`: Registers default index views.
- `crates/util/cometindex/src/lib.rs`: Indexer primitives.
- `crates/util/tendermint-proxy/src/lib.rs`: Tendermint RPC proxy.
- `deployments/scripts/*.sh`: CI/local deployment and smoke helpers.
- `scripts/*.sh`: Repo-level Penumbra/Orbis operational scripts.

**Testing:**
- `crates/core/app-tests/tests/*`: App integration tests declared in `crates/core/app-tests/Cargo.toml`.
- `crates/bin/*/tests/*`: Binary integration tests.
- `crates/test/mock-consensus/src/lib.rs`: Mock consensus test harness.
- `crates/test/mock-client/src/lib.rs`: Mock wallet/client harness.
- `crates/*/tests/*`: Crate-specific integration tests.

## Naming Conventions

**Files:**
- Use Rust standard crate layout: `src/lib.rs` for libraries and `src/main.rs` for binaries.
- Use component module names for chain component concepts: `component.rs` or `component/*`, `state_key.rs`, `event.rs`, `genesis.rs`, `params.rs`, `rpc.rs`.
- Use transaction lifecycle file names in `crates/core/transaction/src`: `action.rs`, `plan.rs`, `view.rs`, `transaction.rs`, `witness_data.rs`, `auth_data.rs`.
- Use protobuf package path names under `proto/penumbra/penumbra/<domain>/<name>/v1/*.proto`.
- Use generated protobuf files as `crates/proto/src/gen/<package>.rs` and `crates/proto/src/gen/<package>.serde.rs`.
- Use GSD skill files as `.codex/skills/<gsd-command>/SKILL.md`.

**Directories:**
- Use workspace family directories for ownership: `crates/bin`, `crates/core`, `crates/crypto`, `crates/view`, `crates/wallet`, `crates/custody`, `crates/util`, `crates/test`.
- Use component directory names matching protocol modules: `compact-block`, `compliance`, `fee`, `governance`, `ibc`, `sct`, `shielded-pool`, `stake`.
- Use kebab-case for crate directory names where multiple words are needed, such as `proof-aggregation`, `proof-params`, `decaf377-frost`, `tendermint-proxy`.
- Use snake_case for Rust module filenames and directories inside `src`, such as `state_key.rs`, `action_handler`, `note_manager.rs`, `client_compliance.rs`.

**Crates:**
- Use `penumbra-sdk-*` crate names for reusable SDK crates, as in `crates/core/transaction/Cargo.toml` and `crates/view/Cargo.toml`.
- Use short binary crate names for commands: `pd`, `pcli`, `pclientd`, `pindexer`.
- Keep external-facing or utility crate names explicit: `cnidarium-component`, `cometindex`, `penumbra-orbis-client`.

**Tests:**
- Use crate `tests/` directories for integration tests, for example `crates/bin/pcli/tests` and `crates/core/transaction/tests`.
- Use `#[cfg(test)] mod tests` for unit tests colocated with implementation, for example `crates/view/src/note_manager.rs`.
- Use app-wide integration tests in `crates/core/app-tests`, with explicit `[[test]]` declarations in `crates/core/app-tests/Cargo.toml`.

## Where to Add New Code

**New Chain Component:**
- Primary code: `crates/core/component/<component>/src/lib.rs`
- Component effects: `crates/core/component/<component>/src/component.rs` or `crates/core/component/<component>/src/component/*`
- State keys: `crates/core/component/<component>/src/state_key.rs`
- Events: `crates/core/component/<component>/src/event.rs`
- Genesis/params: `crates/core/component/<component>/src/genesis.rs`, `crates/core/component/<component>/src/params.rs`
- App wiring: `crates/core/app/src/app/mod.rs`, `crates/core/app/src/rpc.rs`, `crates/core/app/Cargo.toml`
- Workspace membership: `Cargo.toml`
- Tests: `crates/core/app-tests/tests/*` and component crate unit/integration tests.

**New Transaction Action:**
- Domain action type: owning component crate under `crates/core/component/<component>/src/<action>.rs` or a submodule.
- Transaction enum: `crates/core/transaction/src/action.rs`
- Plan/view types: `crates/core/transaction/src/plan/action.rs`, `crates/core/transaction/src/view/action_view.rs`
- App action dispatch: `crates/core/app/src/action_handler/actions.rs`
- Component action handler: `crates/core/component/<component>/src/component/action_handler/*`
- Proto schema: `proto/penumbra/penumbra/core/transaction/v1/transaction.proto` and component proto under `proto/penumbra/penumbra/core/component/<component>/v1/*.proto`
- Generated proto integration: `crates/proto/src/lib.rs`, generated files under `crates/proto/src/gen`.
- Tests: component tests plus app tests in `crates/core/app-tests/tests`.

**New Durable Chain State:**
- Define keys first: `crates/core/component/<component>/src/state_key.rs`
- Read/write methods: owning component `component` module, such as `crates/core/component/sct/src/component/*`
- RPC exposure: owning component `component/rpc.rs` and route registration in `crates/core/app/src/rpc.rs`
- Tests: state-key parity and app behavior tests under the component and `crates/core/app-tests`.

**New Node Query or RPC Service:**
- Proto schema: `proto/penumbra/penumbra/<domain>/<service>/v1/*.proto`
- Domain conversions: owning crate plus `crates/proto/src/protobuf.rs` when needed.
- Service implementation: component `component/rpc.rs` or app query server in `crates/core/app/src/rpc/query.rs`.
- Route assembly: `crates/core/app/src/rpc.rs`
- Client proxy if exposed through `pclientd`: `crates/bin/pclientd/src/proxy.rs` and `crates/bin/pclientd/src/lib.rs`.

**New Wallet Planning Flow:**
- Planning logic: `crates/view/src/note_manager.rs`
- View service endpoint: `crates/view/src/service.rs`
- Wallet build path: `crates/wallet/src/build.rs`
- CLI command: `crates/bin/pcli/src/command.rs` and relevant command submodules.
- View storage changes: `crates/view/src/storage/schema.sql` and `crates/view/src/storage.rs`.

**New Custody Policy or Signing Mode:**
- Policy code: `crates/custody/src/policy.rs`
- Request shape: `crates/custody/src/request.rs`
- Service implementation: `crates/custody/src/soft_kms.rs`, `crates/custody/src/threshold.rs`, or `crates/custody-ledger-usb/src/lib.rs`.
- Daemon config/serve wiring: `crates/bin/pclientd/src/lib.rs`
- Tests: `crates/custody/src/*` unit tests and `crates/bin/pclientd/tests`.

**New Index Projection:**
- Index view implementation: `crates/bin/pindexer/src/<area>.rs`
- Registration: `crates/bin/pindexer/src/indexer_ext.rs`
- Shared index primitives: `crates/util/cometindex/src/*`
- Tests: `crates/bin/pindexer/tests` or module tests.

**New CLI Command:**
- `pd`: Add command definitions in `crates/bin/pd/src/cli.rs` and implementation modules under `crates/bin/pd/src/*`.
- `pcli`: Add command/subcommand code under `crates/bin/pcli/src/command.rs` or `crates/bin/pcli/src/command/*`.
- `pclientd`: Add command variants and execution in `crates/bin/pclientd/src/lib.rs`.
- `pindexer`: Extend options in `crates/bin/pindexer/src/lib.rs`.

**New Proof/Gnark Integration:**
- Rust proof family/domain code: `crates/core/component/shielded-pool/src/gnark/*`
- Proving/verifying key registration: `crates/crypto/proof-params/src/lib.rs`
- Aggregation support: `crates/crypto/proof-aggregation/src/*`
- Go runtime/artifacts: `tools/gnark`
- Verification recipes: `justfile`

**New Project Workflow Skill:**
- Skill index: `.codex/skills/<skill-name>/SKILL.md`
- Agent definition if needed: `.codex/agents/<agent-name>.md`, `.codex/agents/<agent-name>.toml`
- Workflow/reference files: `.codex/get-shit-done/workflows/*`, `.codex/get-shit-done/references/*`
- Generated planning output: `.planning/*`

**Utilities:**
- Shared operational helpers: `crates/util/<utility>/src/lib.rs`
- Shell automation: `scripts/*` or `deployments/scripts/*`
- Devnet/deployment config: `deployments/*` and `testnets/*`

## Special Directories

**`.codex`:**
- Purpose: Local GSD/Codex command framework, skills, agents, hooks, references, and templates.
- Generated: Yes
- Committed: Yes

**`.planning`:**
- Purpose: Generated GSD project planning state and codebase maps.
- Generated: Yes
- Committed: Yes when workflow artifacts are intended to be shared.

**`target`:**
- Purpose: Cargo build output.
- Generated: Yes
- Committed: No

**`tmp`:**
- Purpose: Local scratch data, runtime homes, test artifacts, and experiment output.
- Generated: Yes
- Committed: No

**`crates/proto/src/gen`:**
- Purpose: Generated Rust protobuf and serde code consumed by `penumbra-sdk-proto`.
- Generated: Yes
- Committed: Yes

**`proto/rust-vendored`:**
- Purpose: Vendored third-party proto definitions required by code generation.
- Generated: No
- Committed: Yes

**`tools/gnark`:**
- Purpose: Go proving runtime, C-shared/daemon transports, fixtures, and proof artifacts for shielded proof families.
- Generated: Mixed. Source is hand-maintained; artifacts and dynamic libraries are generated inputs/outputs.
- Committed: Yes for source and selected artifacts present in the repository.

**`poc`:**
- Purpose: Separate non-production workspace for prototype work.
- Generated: No
- Committed: Yes

**`deployments/compose`:**
- Purpose: Local/dev deployment composition.
- Generated: No
- Committed: Yes

**`assets`:**
- Purpose: Zip archives embedded by `pd` for minifront and node status routes.
- Generated: Yes
- Committed: Yes

**`.github`:**
- Purpose: GitHub Actions, issue templates, and PR template.
- Generated: No
- Committed: Yes

---

*Structure analysis: 2026-05-12*
