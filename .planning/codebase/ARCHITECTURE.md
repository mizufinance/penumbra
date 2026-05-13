<!-- refreshed: 2026-05-12 -->
# Architecture

**Analysis Date:** 2026-05-12

## System Overview

```text
+-------------------------------------------------------------+
|                      Runtime Entry Points                    |
+------------------+------------------+-----------------------+
|   Full node      |   Wallet/client  |    Index/audit tools   |
|  `crates/bin/pd` | `pcli`,          | `pindexer`, `orbis-*`  |
|                  | `pclientd`       | `crates/bin/*`         |
+--------+---------+--------+---------+----------+------------+
         |                  |                     |
         v                  v                     v
+-------------------------------------------------------------+
|                 Protocol and Service Boundary                |
| `crates/proto`, `proto/penumbra`, Tonic/gRPC, ABCI, CometBFT |
+--------+----------------------------------------------------+
         |
         v
+-------------------------------------------------------------+
|                    Application State Machine                 |
| `crates/core/app`, `crates/cnidarium-component`              |
+------------------+------------------+-----------------------+
| Block hooks      | Transaction      | Query/RPC assembly     |
| `Component`      | `ActionHandler`  | `rpc::routes`          |
+--------+---------+--------+---------+----------+------------+
         |                  |                     |
         v                  v                     v
+-------------------------------------------------------------+
|                     Domain Component Crates                  |
| `crates/core/component/*`, `crates/core/transaction`,        |
| `crates/core/asset`, `crates/core/keys`, `crates/core/txhash`|
+--------+----------------------------------------------------+
         |
         v
+-------------------------------------------------------------+
| Storage, Crypto, Proofs, and External Edges                  |
| `cnidarium::Storage`, `crates/view/src/storage/schema.sql`,  |
| `crates/crypto/*`, `tools/gnark`, `crates/util/*`            |
+-------------------------------------------------------------+
```

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| Root workspace | Defines the production Cargo workspace and excludes `poc/` and `tools/proto-compiler` from root builds | `Cargo.toml` |
| `pd` binary | Starts the full node, opens RocksDB-backed `cnidarium::Storage`, runs ABCI and gRPC servers, exposes metrics and bundled frontend assets | `crates/bin/pd/src/main.rs` |
| ABCI server | Builds the `tower_abci` server with consensus, mempool, info, and snapshot services over shared storage/cache | `crates/core/app/src/server.rs` |
| Consensus actor | Dispatches CometBFT ABCI requests into app lifecycle methods: init, prepare/process proposal, begin/deliver/end/commit | `crates/core/app/src/server/consensus.rs` |
| Mempool actor | Runs concurrent `CheckTx` against isolated app forks and shared stateless verification cache | `crates/core/app/src/server/mempool.rs` |
| App state machine | Owns block lifecycle sequencing, transaction execution, component hooks, proposal aggregation, and commits to storage | `crates/core/app/src/app/mod.rs` |
| Component traits | Defines block-level `Component` and per-action `ActionHandler` contracts for component crates | `crates/cnidarium-component/src/lib.rs` |
| Component crates | Own chain modules for compact blocks, compliance, fee, governance, IBC, SCT, shielded pool, and validators | `crates/core/component/*/src/lib.rs` |
| Transaction model | Defines `TransactionPlan`, `ActionPlan`, `Transaction`, `Action`, `TransactionView`, authorization, detection data, and witness data | `crates/core/transaction/src/lib.rs` |
| Proto layer | Contains generated protobuf wire types and `DomainType` conversion contracts; validation belongs in domain conversions | `crates/proto/src/lib.rs` |
| View service | Syncs private wallet state from compact blocks and exposes wallet-facing gRPC/query methods | `crates/view/src/service.rs` |
| View worker | Streams compact blocks from `pd`, scans with a full viewing key, maintains in-memory trees, and records SQLite state | `crates/view/src/worker.rs` |
| Wallet builder | Asks custody for authorization, asks view for witnesses, then builds transactions from plans | `crates/wallet/src/build.rs` |
| Custody service | Signs transaction plans and validator/governance messages behind policy checks | `crates/custody/src/soft_kms.rs` |
| `pcli` | CLI frontend for wallet/view/query/transaction/validator/threshold commands | `crates/bin/pcli/src/main.rs` |
| `pclientd` | Long-running view/custody daemon that proxies node queries and serves Tonic/Tonic-Web services | `crates/bin/pclientd/src/lib.rs` |
| `pindexer` | Runs `cometindex` PostgreSQL-backed views for block, stake, governance, supply, IBC, and insights events | `crates/bin/pindexer/src/main.rs` |
| Orbis tools | Runs compliance audit and integration flows against Orbis PRE services and Penumbra scan exports | `crates/bin/orbis-audit/src/main.rs`, `crates/bin/orbis-integration/src/main.rs` |
| Gnark runtime | Provides Go/Groth16 proving runtimes and Rust bridge artifacts for supported shielded proof families | `tools/gnark/README.md` |
| GSD tooling | Defines project-local planning, mapping, execution, review, and workflow skills consumed by Codex/GSD | `.codex/skills/gsd-map-codebase/SKILL.md` |

## Pattern Overview

**Overall:** Modular Rust workspace with a componentized ABCI state machine, generated protobuf boundary, and sidecar client/indexer/tooling processes.

**Key Characteristics:**
- Use crate boundaries for ownership: runtime binaries live in `crates/bin/*`, reusable protocol/domain logic in `crates/core/*`, cryptography in `crates/crypto/*`, and wallet/custody/view services in `crates/view`, `crates/wallet`, and `crates/custody`.
- Keep server-side state effects feature-gated behind each component crate's `component` feature. Client-safe transaction/domain types are exported from component crate roots such as `crates/core/component/shielded-pool/src/lib.rs`.
- Treat protobuf as a wire format only. Convert through `TryFrom`/`From` and `DomainType` before business validation or persistence, following `crates/proto/src/lib.rs` and `crates/proto/src/protobuf.rs`.
- Persist chain state through `cnidarium::Storage` and component-owned `state_key.rs` functions. View state uses SQLite via `crates/view/src/storage.rs` and `crates/view/src/storage/schema.sql`.
- Route all transaction validity through `ActionHandler` phases: stateless, historical read-only, then sequential write execution in `crates/cnidarium-component/src/action_handler.rs`.

## Layers

**Runtime Binaries:**
- Purpose: Own process startup, CLI parsing, tracing, concrete network/storage clients, and service serving.
- Location: `crates/bin/*`
- Contains: `pd`, `pcli`, `pclientd`, `pindexer`, `orbis-audit`, `orbis-integration`.
- Depends on: Reusable crates in `crates/core/*`, `crates/view`, `crates/wallet`, `crates/custody`, `crates/util/*`.
- Used by: Operators, clients, CI, integration flows, and local devnet scripts.

**ABCI Application:**
- Purpose: Adapt CometBFT ABCI requests to Penumbra state transitions and queries.
- Location: `crates/core/app`
- Contains: `App`, ABCI server wiring, mempool/consensus actors, proposal aggregation, app params/genesis, RPC route assembly.
- Depends on: `cnidarium`, `cnidarium-component`, component crates with `component` feature, `tendermint`, `tower_abci`, `tonic`.
- Used by: `crates/bin/pd/src/main.rs`, app integration tests in `crates/core/app-tests`.

**Component Domain Modules:**
- Purpose: Own protocol-specific action types, state keys, events, params, genesis content, RPC, and component hooks.
- Location: `crates/core/component/*`
- Contains: `compact-block`, `compliance`, `fee`, `governance`, `ibc`, `sct`, `shielded-pool`, `stake`.
- Depends on: `penumbra-sdk-proto`, shared domain crates, crypto crates, and optionally `cnidarium`/`tonic` under feature gates.
- Used by: `crates/core/app`, `crates/core/transaction`, wallet/view/custody clients.

**Core Domain Types:**
- Purpose: Model assets, keys, amounts, transactions, action plans/views, hashes, and app-independent protocol data.
- Location: `crates/core/asset`, `crates/core/keys`, `crates/core/num`, `crates/core/transaction`, `crates/core/txhash`
- Contains: Pure domain structs/enums, serialization conversion impls, transaction building primitives.
- Depends on: Crypto/proto crates and standard Rust dependencies.
- Used by: Components, app, view, wallet, custody, tests, and CLI commands.

**Protocol Wire Layer:**
- Purpose: Provide generated protobuf types, Tonic clients/servers, reflection descriptors, and domain conversion helpers.
- Location: `crates/proto`, `proto/penumbra`
- Contains: `.proto` sources, generated `src/gen/*.rs`, `DomainType`, `StateReadProto`, `StateWriteProto`, serializers.
- Depends on: `prost`, `tonic`, `cnidarium` optionally, Tendermint/IBC proto crates.
- Used by: Every service boundary and persistence point that stores encoded domain data.

**Client/Wallet/Custody:**
- Purpose: Maintain private wallet state, plan notes/actions, generate witnesses, authorize plans, and build/broadcast transactions.
- Location: `crates/view`, `crates/wallet`, `crates/custody`, `crates/custody-ledger-usb`
- Contains: `ViewServer`, `Worker`, `Storage`, `NoteManager`, `CustodyClient`, `SoftKms`, threshold and Ledger custody support.
- Depends on: `penumbra-sdk-proto`, `penumbra-sdk-transaction`, component domain crates, `tonic`, SQLite/r2d2.
- Used by: `pcli`, `pclientd`, integration tests, wallet-facing flows.

**Crypto and Proofs:**
- Purpose: Own FMD, FROST, key agreement, flow encryption, proof aggregation, proving/verifying parameters, and tiered commitment tree logic.
- Location: `crates/crypto/*`, `tools/gnark`
- Contains: `decaf377-fmd`, `decaf377-frost`, `decaf377-ka`, `eddy`, `proof-aggregation`, `proof-params`, `tct`, Go gnark runtime.
- Depends on: Arkworks, decaf377, Groth16/SnarkPack, proof artifacts.
- Used by: Shielded-pool actions, compliance, wallet planning/building, consensus validation.

**Indexing and Operational Utilities:**
- Purpose: Project chain events to Postgres, proxy CometBFT RPC, manage auto-HTTPS, trace tower services, and talk to Orbis.
- Location: `crates/util/*`, `crates/bin/pindexer`, `deployments`, `scripts`
- Contains: `cometindex`, `tendermint-proxy`, `auto-https`, `tower-trace`, `orbis-client`, devnet/deployment scripts.
- Depends on: `sqlx`, `tendermint-rpc`, `tonic`, `reqwest`, `rustls`.
- Used by: `pd`, `pclientd`, `pindexer`, Orbis integration, local devnet workflows.

**Project Workflow Tooling:**
- Purpose: Store GSD command skills, agent definitions, workflow references, and generated planning artifacts.
- Location: `.codex`, `.planning`
- Contains: `.codex/skills/*/SKILL.md`, `.codex/agents/*`, `.codex/get-shit-done/*`, `.planning/codebase/*`.
- Depends on: Local Codex/GSD conventions rather than product runtime crates.
- Used by: GSD commands such as `$gsd-map-codebase`, `$gsd-plan-phase`, `$gsd-execute-phase`.

## Data Flow

### Full Node Startup and Serving

1. `pd` parses `RootCommand::Start`, initializes tracing/TLS, resolves home paths, and opens `cnidarium::Storage` (`crates/bin/pd/src/main.rs:59`, `crates/bin/pd/src/main.rs:117`).
2. `pd` checks app version and halt readiness before serving (`crates/bin/pd/src/main.rs:122`, `crates/bin/pd/src/main.rs:136`).
3. `pd` spawns the ABCI server from `penumbra_sdk_app::server::new(storage.clone()).listen_tcp(abci_bind)` (`crates/bin/pd/src/main.rs:143`).
4. `pd` builds app/component/storage/IBC/query/reflection gRPC routes through `penumbra_sdk_app::rpc::routes` and merges frontend/status Axum routes (`crates/bin/pd/src/main.rs:149`, `crates/core/app/src/rpc.rs:117`).
5. `pd` serves gRPC through `axum_server`, optionally with ACME auto-HTTPS, and exposes Prometheus metrics (`crates/bin/pd/src/main.rs:188`, `crates/bin/pd/src/main.rs:205`).

### ABCI Request Path

1. `server::new` constructs consensus and mempool actors with shared `Storage` and `StatelessCache` (`crates/core/app/src/server.rs:27`, `crates/core/app/src/server.rs:49`).
2. `Consensus::run` receives `InitChain`, `PrepareProposal`, `ProcessProposal`, `BeginBlock`, `DeliverTx`, `EndBlock`, and `Commit` messages from `tower_abci` (`crates/core/app/src/server/consensus.rs:116`).
3. `PrepareProposal` and `ProcessProposal` run against isolated `App::new(storage.latest_snapshot())` forks to avoid corrupting finalized state (`crates/core/app/src/server/consensus.rs:254`, `crates/core/app/src/server/consensus.rs:317`).
4. `BeginBlock` calls `App::begin_block`, which applies app parameter changes and invokes component `begin_block` hooks in sequence (`crates/core/app/src/server/consensus.rs:347`, `crates/core/app/src/app/mod.rs:4439`).
5. `DeliverTx` decodes bytes through `Transaction::decode`, uses stateless cache when provided, then runs transaction/action validation and execution (`crates/core/app/src/server/consensus.rs:366`, `crates/core/app/src/app/mod.rs:4464`).
6. `EndBlock` flushes deferred transaction indexing, materializes SCT append logs, invokes component `end_block` and optional `end_epoch` hooks, then returns validator updates (`crates/core/app/src/app/mod.rs:5933`, `crates/core/app/src/server/consensus.rs:395`).
7. `Commit` extracts the accumulated `StateDelta`, commits it to `cnidarium::Storage`, and returns the app hash to CometBFT (`crates/core/app/src/app/mod.rs:6045`, `crates/core/app/src/server/consensus.rs:428`).

### Transaction Build and Submit Path

1. `pcli` parses commands, initializes local app clients, syncs the view service for online commands, and dispatches transaction/view/query handlers (`crates/bin/pcli/src/main.rs:24`, `crates/bin/pcli/src/main.rs:67`).
2. Wallet code creates a `TransactionPlan` through view/note planning code (`crates/view/src/service.rs:429`, `crates/view/src/note_manager.rs`).
3. `build_transaction` asks custody for `AuthorizationData`, asks view for `WitnessData`, then builds the `Transaction` from the plan (`crates/wallet/src/build.rs:10`, `crates/wallet/src/build.rs:23`, `crates/wallet/src/build.rs:34`).
4. `SoftKms` enforces configured policies before signing transaction, validator definition, validator vote, or proposal submit requests (`crates/custody/src/soft_kms.rs:39`, `crates/custody/src/soft_kms.rs:119`).
5. The view service can broadcast a built transaction through the node-facing Tendermint proxy path (`crates/view/src/service.rs:407`).

### View Synchronization Path

1. `ViewServer::new` opens a Tonic channel to `pd`, constructs a `Worker`, and spawns exactly one worker task per shared server state (`crates/view/src/service.rs:122`, `crates/view/src/service.rs:139`).
2. `Worker::new` loads the full viewing key, in-memory SCT, compliance trees, error slot, and sync-height watch channel from SQLite storage (`crates/view/src/worker.rs:66`, `crates/view/src/worker.rs:87`).
3. `Worker::sync` streams compact blocks from `CompactBlockQueryService`, buffers the stream, and processes blocks in height order (`crates/view/src/worker.rs:310`, `crates/view/src/worker.rs:321`).
4. Blocks with scan-relevant payloads call `scan_block`, fetch matching full transactions, record counterparties/assets, and commit to SQLite storage (`crates/view/src/worker.rs:391`, `crates/view/src/sync.rs:27`, `crates/view/src/worker.rs:528`).
5. View RPC methods such as `witness` hold a read lock over the in-memory SCT so all auth paths share one anchor (`crates/view/src/service.rs:1299`, `crates/view/src/service.rs:1305`).

### Index and Audit Flow

1. `pindexer` starts `cometindex::Indexer`, attaches default Penumbra app views, and runs against configured CometBFT/event storage (`crates/bin/pindexer/src/main.rs:5`, `crates/bin/pindexer/src/indexer_ext.rs:5`).
2. App views project block, validator set, governance, supply, IBC, and insights indexes (`crates/bin/pindexer/src/lib.rs:5`, `crates/bin/pindexer/src/indexer_ext.rs:7`).
3. Orbis audit tools consume scan exports, fetch node transactions, run PRE object/package workflows, and emit audit JSON (`crates/bin/orbis-audit/src/main.rs:178`, `crates/bin/orbis-integration/src/main.rs:153`).

**State Management:**
- Chain state is a `cnidarium::Storage` JMT/RocksDB store opened by `pd` and mutated through `StateDelta` in `crates/core/app/src/app/mod.rs`.
- Component state keys are public API functions under each `crates/core/component/*/src/state_key.rs`.
- View state is SQLite with schema in `crates/view/src/storage/schema.sql`, accessed through `crates/view/src/storage.rs`.
- Proposal/mempool artifact reuse is in-memory and bounded through `StatelessCache` in `crates/core/app/src/stateless_cache.rs`.
- GSD planning state is file-backed under `.planning`, with codebase maps in `.planning/codebase`.

## Key Abstractions

**`App`:**
- Purpose: Stateful ABCI application over a snapshot plus accumulated `StateDelta`.
- Examples: `crates/core/app/src/app/mod.rs`, `crates/core/app/src/server/consensus.rs`.
- Pattern: Construct from latest snapshot for each execution context; mutate state through begin/deliver/end/commit lifecycle.

**`Component`:**
- Purpose: Block-level state machine hooks for component crates.
- Examples: `crates/cnidarium-component/src/component.rs`, `crates/core/component/sct/src/component/sct.rs`, `crates/core/component/shielded-pool/src/component/shielded_pool.rs`.
- Pattern: Implement `init_chain`, `begin_block`, `end_block`, and `end_epoch`; keep state-effects code under feature-gated `component` modules.

**`ActionHandler`:**
- Purpose: Per-action transaction validity and execution contract.
- Examples: `crates/cnidarium-component/src/action_handler.rs`, `crates/core/app/src/action_handler/actions.rs`, `crates/core/component/shielded-pool/src/component/action_handler/transfer.rs`.
- Pattern: Put parallelizable checks in `check_stateless` or carefully justified `check_historical`; keep sequential writes in `check_and_execute`.

**`DomainType`:**
- Purpose: Tie domain structs to protobuf structs with explicit encode/decode conversion boundaries.
- Examples: `crates/proto/src/protobuf.rs`, `crates/proto/src/lib.rs`.
- Pattern: Proto messages parse wire bytes; domain conversions validate shape before app or wallet logic uses the data.

**`TransactionPlan` / `Transaction` / `Action`:**
- Purpose: Model the transaction lifecycle from planning through authorized shielded execution and wallet-facing views.
- Examples: `crates/core/transaction/src/lib.rs`, `crates/core/transaction/src/action.rs`, `crates/core/transaction/src/plan.rs`.
- Pattern: Add actions to transaction plan/action/view enums and dispatch them through app/component action handlers.

**`ViewServer` / `Worker` / `Storage`:**
- Purpose: Maintain private wallet state, scan compact blocks, generate witnesses, and answer wallet-facing queries.
- Examples: `crates/view/src/service.rs`, `crates/view/src/worker.rs`, `crates/view/src/storage.rs`.
- Pattern: Long-running worker syncs from `pd`; service methods read shared SQLite/in-memory trees and surface Tonic errors.

**`CustodyClient` / `SoftKms`:**
- Purpose: Abstract transaction authorization behind async custody protocol and policy checks.
- Examples: `crates/custody/src/client.rs`, `crates/custody/src/soft_kms.rs`, `crates/custody/src/policy.rs`.
- Pattern: Wallet code requests authorization; custody implementations decide and sign without trusting the caller.

**`StatelessCache`:**
- Purpose: Share decoded/extracted/proof-verified transaction artifacts across mempool, proposer, and validator passes.
- Examples: `crates/core/app/src/stateless_cache.rs`, `crates/core/app/src/server.rs`.
- Pattern: Bounded in-memory cache keyed by SHA-256 of raw tx bytes with valid/invalid/extracted tiers.

**`cometindex::Indexer`:**
- Purpose: Project ABCI events into queryable Postgres-backed app views.
- Examples: `crates/util/cometindex/src/lib.rs`, `crates/bin/pindexer/src/indexer_ext.rs`.
- Pattern: Add an `AppView` implementation and register it in `with_default_penumbra_app_views`.

## Entry Points

**Full Node:**
- Location: `crates/bin/pd/src/main.rs`
- Triggers: `pd start`, network generation/migration commands under `crates/bin/pd/src/cli.rs`.
- Responsibilities: Own node process wiring, storage, ABCI listener, gRPC/HTTP server, metrics, Tendermint proxy, migration/network helpers.

**Command Line Client:**
- Location: `crates/bin/pcli/src/main.rs`
- Triggers: User CLI commands.
- Responsibilities: Initialize local config, connect to view/custody services, sync before online commands, dispatch wallet/query/validator/threshold flows.

**View/Custody Daemon:**
- Location: `crates/bin/pclientd/src/main.rs`, `crates/bin/pclientd/src/lib.rs`
- Triggers: `pclientd init`, `pclientd start`, `pclientd reset`, `pclientd load-registry`.
- Responsibilities: Serve view/custody gRPC, proxy node query services, hold local SQLite view DB, optionally expose custody signing.

**Indexer:**
- Location: `crates/bin/pindexer/src/main.rs`
- Triggers: `pindexer` CLI.
- Responsibilities: Run `cometindex`, register Penumbra app views, project ABCI events.

**Orbis Compliance Tooling:**
- Location: `crates/bin/orbis-audit/src/main.rs`, `crates/bin/orbis-integration/src/main.rs`
- Triggers: Audit/integration CLI commands and just recipes.
- Responsibilities: Run PRE setup, scanner/audit demo flows, fetch transactions, import/export compliance scan state.

**Production Workspace:**
- Location: `Cargo.toml`
- Triggers: `cargo build --workspace`, `cargo test`, `just check`.
- Responsibilities: Defines shipped workspace members and shared dependency versions.

**POC Workspace:**
- Location: `poc/Cargo.toml`
- Triggers: `cargo build --workspace --manifest-path poc/Cargo.toml`.
- Responsibilities: Isolated non-production preconsensus prototype workspace.

**Proof Runtime:**
- Location: `tools/gnark`, `crates/crypto/proof-params/src/lib.rs`, `crates/core/component/shielded-pool/src/gnark`
- Triggers: Rust proof generation/verification, `just go-check`, `just gnark-proof-tests`.
- Responsibilities: Bridge Rust witnesses/proofs to gnark Groth16 proving runtimes for supported shielded proof families.

## Architectural Constraints

- **Threading:** Runtime services are Tokio async processes. `pd` uses `#[tokio::main]` and `tower_actor` actors for consensus/mempool (`crates/bin/pd/src/main.rs`, `crates/core/app/src/server.rs`). `ViewServer` spawns one `Worker` task and shares state with `Arc<RwLock<_>>` (`crates/view/src/service.rs`, `crates/view/src/worker.rs`).
- **Global state:** Keep global state limited to constants, lazy configuration, proof keys, metrics, and bounded in-memory caches. Examples include `SUBSTORE_PREFIXES` in `crates/core/app/src/lib.rs`, `SCHEMA_HASH` in `crates/view/src/storage.rs`, proving keys in `crates/crypto/proof-params/src/lib.rs`, and `StatelessCache` instances passed by caller in `crates/core/app/src/server.rs`.
- **Feature gates:** Server-side component effects use `feature = "component"` in crates such as `crates/core/app/src/lib.rs` and `crates/core/component/shielded-pool/src/lib.rs`. Client/wasm-safe domain types must not require Tokio/cnidarium server features.
- **State keys:** Durable chain state keys are component-owned public APIs under `state_key.rs` files, for example `crates/core/component/sct/src/state_key.rs` and `crates/core/component/governance/src/state_key.rs`.
- **Wire/domain boundary:** Use generated protobuf types only at transport/storage edges and convert to domain types through `DomainType`/`TryFrom`, as required by `crates/proto/src/lib.rs`.
- **Circular imports:** Not detected in the mapper scan. Workspace layering relies on Cargo crate boundaries in `Cargo.toml` and component feature gates instead of cross-crate cyclic ownership.
- **Secrets:** Environment files and credential material are not part of architecture docs. Do not read `.env*`, credential files, or local generated key material.
- **GSD artifacts:** `.planning/codebase/*.md` is generated project context for GSD commands. Product runtime crates do not depend on `.planning` or `.codex`.

## Anti-Patterns

### Component Effects Outside Feature-Gated Component Modules

**What happens:** Adding chain-state reads/writes, RPC servers, or async service dependencies to a component crate's client-safe root modules.
**Why it's wrong:** Transaction/domain crates depend on component crates without `component`; adding effects there breaks wasm/client-safe reuse and expands dependency surfaces.
**Do this instead:** Put stateful code in the feature-gated `component` tree and export pure domain types from `lib.rs`, following `crates/core/component/shielded-pool/src/lib.rs` and `crates/cnidarium-component/src/lib.rs`.

### Proto Types as Business Objects

**What happens:** Passing generated `pb` structs deep into app, wallet, or component logic.
**Why it's wrong:** Generated types only parse wire format; validation and invariants live in domain conversions.
**Do this instead:** Convert at the boundary with `TryFrom`/`From` and implement `DomainType`, following `crates/proto/src/lib.rs` and `crates/proto/src/protobuf.rs`.

### Ad Hoc Durable State Keys

**What happens:** Creating raw state key strings inside handlers or service methods.
**Why it's wrong:** State keys are public chain-state APIs and need consistent prefixes for proofs, queries, migrations, and tests.
**Do this instead:** Add or reuse a function in the owning component's `state_key.rs`, such as `crates/core/component/sct/src/state_key.rs` or `crates/core/component/compliance/src/state_key.rs`.

### Bypassing Transaction Validation Phases

**What happens:** Performing expensive or parallelizable validity work only during sequential state execution, or doing historical checks that are unsafe against prior state.
**Why it's wrong:** It reduces proposer/mempool parallelism and can introduce TOCTOU bugs.
**Do this instead:** Use `ActionHandler::check_stateless`, justified `check_historical`, and `check_and_execute` exactly as described in `crates/cnidarium-component/src/action_handler.rs`.

## Error Handling

**Strategy:** Use `anyhow::Result` at binary/internal orchestration boundaries, convert service failures to `tonic::Status` at gRPC boundaries, and map transaction admission failures to ABCI response codes/logs.

**Patterns:**
- CLI and process startup return `anyhow::Result` or `ExitCode`, for example `crates/bin/pd/src/main.rs` and `crates/bin/pcli/src/main.rs`.
- ABCI consensus handles fallible `PrepareProposal`/`ProcessProposal` by returning empty/reject responses, while `DeliverTx` returns code `1` and error log on transaction rejection (`crates/core/app/src/server/consensus.rs`).
- Invariant failures that indicate corrupted state or impossible ABCI phases use `expect`, for example app commit and init-chain paths in `crates/core/app/src/app/mod.rs` and `crates/core/app/src/server/consensus.rs`.
- gRPC services convert domain/storage errors to `tonic::Status` in `crates/core/app/src/rpc/query.rs` and `crates/view/src/service.rs`.
- View worker errors are persisted in an `error_slot` and surfaced through service health checks (`crates/view/src/service.rs`, `crates/view/src/worker.rs`).

## Cross-Cutting Concerns

**Logging:** Use `tracing`, request spans, and `EnvFilter`. Node startup configures tracing and metrics context in `crates/bin/pd/src/main.rs`; ABCI/gRPC request spans use `crates/util/tower-trace/src/lib.rs` and `crates/core/app/src/server.rs`.

**Metrics:** `pd` registers Prometheus exporter in `crates/bin/pd/src/main.rs`; app and component metrics are registered through `crates/core/app/src/metrics.rs`, `crates/bin/pd/src/metrics.rs`, and component metrics modules such as `crates/core/component/governance/src/metrics.rs`.

**Validation:** Transaction validity is split across stateless, historical, and sequential execution phases in `crates/cnidarium-component/src/action_handler.rs`; protobuf validation is at `DomainType` conversions in `crates/proto/src/protobuf.rs`; app lifecycle validation is in `crates/core/app/src/app/mod.rs`.

**Authentication:** Spend authorization is externalized to custody clients and services in `crates/custody/src/client.rs`, `crates/custody/src/soft_kms.rs`, and `crates/custody/src/policy.rs`. View services operate from full viewing keys and do not imply spend authority unless custody is configured in `pclientd`.

**Persistence:** Chain persistence uses `cnidarium::Storage` in `pd`; private wallet persistence uses SQLite schema in `crates/view/src/storage/schema.sql`; index projections use Postgres through `cometindex` in `crates/util/cometindex`.

**External Services:** CometBFT is accessed through ABCI and Tendermint RPC proxy code in `crates/util/tendermint-proxy`; Orbis PRE flows use `crates/util/orbis-client` and `crates/bin/orbis-*`; proof generation can cross the Rust/Go gnark boundary through `tools/gnark`.

---

*Architecture analysis: 2026-05-12*
