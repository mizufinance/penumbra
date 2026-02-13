# CLAUDE.md - Penumbra Development Guide

## What is Penumbra?

Penumbra is a fully shielded zone for the Cosmos ecosystem — private transactions, staking, swaps, and market-making without broadcasting personal information. This is the Mizu Finance fork, extended to support bankd as an IBC counterparty via a multi-client light client registry.

**Version:** 2.1.0. Rust 1.83, Edition 2021, MIT/Apache-2.0 licensed.

## Build & Test Commands

```sh
cargo check --release --all-targets --all-features   # Type-check everything
cargo build --release --all-features --all-targets    # Full release build
cargo nextest run --release                           # Run tests (preferred)
cargo fmt --all                                       # Format
cargo fmt --all -- --check                            # Check formatting
just check                                            # CI: check + fmt (warnings = errors)
just test                                             # Unit tests via nextest
just proto                                            # Regenerate proto types
just build                                            # Release build
just dev                                              # Local devnet (solo validator + metrics)
just smoke                                            # Smoke test suite
```

### IBC-Specific Development

```sh
cargo check -p penumbra-sdk-ibc                       # Type-check IBC crate
cargo test -p penumbra-sdk-ibc                        # IBC unit tests
cargo nextest run --release -p penumbra-sdk-ibc       # IBC tests via nextest
```

## Project Structure

```
bin/
  pcli/              CLI client (wallet, transactions, queries)
  pclientd/          Client daemon (view service + custody)
  pd/                Full node (ABCI application + Tendermint)
  pindexer/          Event indexer (ABCI events → Postgres)
  pmonitor/          Monitoring tool
  elcuity/           Compliance tooling

crates/
  cnidarium-component/  Component trait system (ActionHandler, Component)

  core/
    app/             Main ABCI application (App struct, init_chain, begin_block, end_block)
    app-tests/       Integration tests for the full application
    asset/           Asset types, denominations, metadata
    keys/            Key management, addresses, identity
    num/             Numeric types (Amount, fixed-point)
    transaction/     Transaction builder, actions, plans
    txhash/          Transaction hashing, effect hashes

    component/       13 modular protocol components:
      ibc/           *** IBC protocol — our primary focus ***
      shielded-pool/ Privacy pool (shielded notes, nullifiers)
      sct/           Shielded commitment tree (epochs, block timestamps)
      stake/         Staking, validator management, delegation
      dex/           Decentralized exchange (AMM, LP positions)
      fee/           Fee handling and gas metering
      governance/    On-chain governance, proposals, voting
      auction/       Auction mechanism
      community-pool/ Community pool management
      compact-block/ Compact block summaries for light clients
      compliance/    Privacy-preserving compliance (detection keys)
      distributions/ Validator reward distributions
      funding/       Funding pool logic

  crypto/
    decaf377-fmd/    Forgetful message detection
    decaf377-frost/  Threshold signatures (FROST)
    decaf377-ka/     Key agreement
    eddy/            Edwards curve crypto
    tct/             Tiered commitment tree (Merkle tree for notes)
    proof-params/    zk-SNARK proof parameters
    proof-setup/     zk-SNARK trusted setup

  proto/             Generated protobuf types (penumbra-sdk-proto crate)
  view/              State synchronization and viewing service
  wallet/            Wallet functionality
  custody/           Custodial key management

  test/              Test utilities
    mock-client/     Mock IBC client
    mock-consensus/  Mock consensus engine
    mock-tendermint-proxy/

  util/
    tendermint-proxy/  Tendermint RPC proxy
    cometindex/        CometBFT event indexing

proto/               Proto source files
  penumbra/          Penumbra-specific proto definitions
  rust-vendored/     Vendored external protos (cosmos, ibc, ics23)

tools/
  proto-compiler/    Custom Rust proto build tool
```

## Architecture Overview

### Component System

Penumbra uses a modular component architecture. Each protocol feature (IBC, staking, DEX, etc.) is a self-contained component with standardized lifecycle hooks:

```
                     ABCI Application (pd)
                            |
              +-------------+-------------+
              |             |             |
         Component::    Component::   Component::
         begin_block    end_block     end_epoch
              |             |             |
    +---------+---------+---+---+---------+---------+
    |         |         |       |         |         |
   IBC     Stake      DEX     Fee    Shielded   Governance
                                      Pool
```

**Two core traits** (from `cnidarium-component`):

```rust
// Lifecycle hooks called at block boundaries
trait Component {
    async fn begin_block(state, begin_block);
    async fn end_block(state, end_block);
    async fn end_epoch(state) -> Result<()>;
}

// Three-phase transaction validation + execution
trait ActionHandler {
    async fn check_stateless(&self, ctx) -> Result<()>;     // Parallel, no state
    async fn check_historical(&self, state) -> Result<()>;  // Parallel, read-only
    async fn check_and_execute(&self, state) -> Result<()>;  // Sequential, read-write
}
```

### State Management (cnidarium)

cnidarium is Penumbra's state storage engine — a JMT (Jellyfish Merkle Tree) backed by RocksDB. All component state flows through two traits:

```rust
trait StateRead: Send + Sync {
    async fn get<T: DomainType>(&self, key: &str) -> Result<Option<T>>;
    async fn get_proto<T: prost::Message>(&self, key: &str) -> Result<Option<T>>;
    fn prefix_raw(&self, prefix: &str) -> impl Stream<Item = (String, Vec<u8>)>;
}

trait StateWrite: StateRead {
    fn put<T: DomainType>(&mut self, key: String, value: T);
    fn put_proto<T: prost::Message>(&mut self, key: String, value: T);
    fn delete(&mut self, key: String);
}
```

**Extension trait pattern** — components extend StateRead/StateWrite with domain-specific methods:

```rust
#[async_trait]
trait ClientStateReadExt: StateRead {
    async fn get_client_state(&self, client_id: &ClientId) -> Result<TendermintClientState>;
    async fn get_client_status(&self, client_id: &ClientId, time: Time) -> ClientStatus;
    async fn get_verified_consensus_state(&self, height: &Height, client_id: &ClientId)
        -> Result<TendermintConsensusState>;
    // ...
}
// Auto-implemented for all types implementing StateRead
impl<T: StateRead + ?Sized> ClientStateReadExt for T {}
```

This pattern is used throughout — `ChannelStateReadExt`, `ConnectionStateReadExt`, `ConsensusStateWriteExt`, etc.

### IBC State Storage

IBC state lives under the `ibc-data/` substore prefix. All IBC paths follow the ibc-go convention:

```
ibc-data/clients/{client_id}/clientType           → String
ibc-data/clients/{client_id}/clientState          → TendermintClientState
ibc-data/clients/{client_id}/consensusStates/{h}  → TendermintConsensusState
ibc-data/connections/{conn_id}                    → ConnectionEnd
ibc-data/channelEnds/ports/{port}/channels/{ch}   → ChannelEnd
ibc-data/commitments/ports/{port}/channels/{ch}/sequences/{seq}  → packet commitment
ibc-data/receipts/ports/{port}/channels/{ch}/sequences/{seq}     → receipt
ibc-data/acks/ports/{port}/channels/{ch}/sequences/{seq}         → ack commitment
```

Additional Penumbra-internal state (not in IBC commitment prefix):
```
penumbra_consensus_states/{height}                → own ConsensusState
penumbra_verified_heights/{client_id}/verified_heights → VerifiedHeights
client_processed_times/{client_id}/{height}       → u64 (nanos)
client_processed_heights/{client_id}/{height}     → Height
ibc_client_counter                                → ClientCounter
```

### Proof System

Penumbra uses a **two-level JMT** for IBC proofs (unlike bankd's single-level):

```rust
// Two proof specs — one for the IBC substore, one for the root store
pub static IBC_PROOF_SPECS: Lazy<Vec<ics23::ProofSpec>> =
    Lazy::new(|| vec![vendored::ics23_spec(), vendored::ics23_spec()]);
```

The JMT proof spec (vendored from the `jmt` crate in `prefix.rs`):
- Leaf: SHA256 hash, `JMT::LeafNode` domain separator, no length prefix
- Inner: SHA256 hash, 16-byte prefix (from `JMT::IntrnalNode`), child_size 32
- Max depth: 64, prehash_key_before_comparison: true

## IBC Component Deep Dive

**Crate:** `penumbra-sdk-ibc` (at `crates/core/component/ibc/`)

### Key Limitation: Tendermint-Only

The IBC component currently hardcodes `07-tendermint` everywhere. This is the primary thing our work changes:

- `client.rs` — `get_client_state()` returns `TendermintClientState` directly
- `client.rs` — `put_client()` takes `TendermintClientState` directly
- `client.rs` — `next_tendermint_state()` computes next state from `TendermintHeader`
- `ics02_validation.rs` — 8 functions that check/extract Tendermint types from `Any`
- `proof_verification.rs` — uses `TendermintClientState` and `TendermintConsensusState` directly
- `msg_handler/update_client.rs` — calls Tendermint-specific verification
- `msg_handler/create_client.rs` — assumes Tendermint client type
- `msg_handler/misbehavior.rs` — assumes Tendermint misbehavior type

### IBC Message Flow

```
Transaction Action
    └── IbcRelay enum (18 variants)
         └── ActionHandler::check_stateless  (format validation)
         └── ActionHandler::check_historical (state consistency, read-only)
         └── ActionHandler::check_and_execute (proof verification + state writes)
              └── MsgHandler trait dispatch
                   ├── check_stateless<AH>()     — type/format validation
                   └── try_execute<S, AH, HI>()  — state changes + app callbacks
```

The `IbcRelay` enum wraps all 18 IBC message types:

```rust
pub enum IbcRelay {
    CreateClient(MsgCreateClient),      // ICS-02
    UpdateClient(MsgUpdateClient),
    UpgradeClient(MsgUpgradeClient),
    SubmitMisbehavior(MsgSubmitMisbehaviour),
    ConnectionOpenInit(MsgConnectionOpenInit),  // ICS-03
    ConnectionOpenTry(MsgConnectionOpenTry),
    ConnectionOpenAck(MsgConnectionOpenAck),
    ConnectionOpenConfirm(MsgConnectionOpenConfirm),
    ChannelOpenInit(MsgChannelOpenInit),        // ICS-04
    ChannelOpenTry(MsgChannelOpenTry),
    ChannelOpenAck(MsgChannelOpenAck),
    ChannelOpenConfirm(MsgChannelOpenConfirm),
    ChannelCloseInit(MsgChannelCloseInit),
    ChannelCloseConfirm(MsgChannelCloseConfirm),
    RecvPacket(MsgRecvPacket),
    Acknowledgement(MsgAcknowledgement),
    Timeout(MsgTimeout),
    Unknown(Any),  // Fallback for unrecognized types
}
```

### MsgHandler Trait

Each message type implements this internal trait:

```rust
#[async_trait]
pub(crate) trait MsgHandler {
    async fn check_stateless<AH: AppHandlerCheck>(&self) -> Result<()>;
    async fn try_execute<S: StateWrite, AH: AppHandlerCheck + AppHandlerExecute, HI: HostInterface>(
        &self, state: S,
    ) -> Result<()>;
}
```

The `HI: HostInterface` bound provides chain-specific context:

```rust
#[async_trait]
pub trait HostInterface {
    async fn get_chain_id<S: StateRead>(state: S) -> Result<String>;
    async fn get_revision_number<S: StateRead>(state: S) -> Result<u64>;
    async fn get_block_height<S: StateRead>(state: S) -> Result<u64>;
    async fn get_block_timestamp<S: StateRead>(state: S) -> Result<tendermint::Time>;
}
```

### AppHandler Trait (IBC Application Callbacks)

IBC applications (like ICS-20 transfer) implement this trait to receive channel and packet events:

```rust
trait AppHandlerCheck: Send + Sync {
    async fn chan_open_init_check(state, msg) -> Result<()>;
    async fn chan_open_try_check(state, msg) -> Result<()>;
    // ... all channel lifecycle events
    async fn recv_packet_check(state, msg) -> Result<()>;
    async fn timeout_packet_check(state, msg) -> Result<()>;
    async fn acknowledge_packet_check(state, msg) -> Result<()>;
}

trait AppHandlerExecute: Send + Sync {
    async fn chan_open_init_execute(state, msg);
    // ... all channel lifecycle events
    async fn recv_packet_execute(state, msg) -> Result<()>;
    async fn timeout_packet_execute(state, msg) -> Result<()>;
    async fn acknowledge_packet_execute(state, msg) -> Result<()>;
}

trait AppHandler: AppHandlerCheck + AppHandlerExecute {}
```

### begin_block: Saving Own Consensus State

At each block start, the IBC component saves Penumbra's own consensus state for future counterparty verification:

```rust
// ibc_component.rs — Ibc::begin_block()
let cs = TendermintConsensusState::new(
    MerkleRoot { hash: begin_block.header.app_hash },
    begin_block.header.time,
    begin_block.header.next_validators_hash,
);
state.put_penumbra_sdk_consensus_state(height, cs);
```

This is what counterparty chains verify when they submit proofs against Penumbra.

### Client State Machine (client.rs)

```rust
pub enum ClientStatus { Active, Frozen, Expired, Unknown, Unauthorized }
```

**`next_tendermint_state()`** — computes next client state after header verification:
1. If a stored consensus state at this height conflicts → freeze client
2. If timestamp is not monotonically increasing relative to adjacent heights → freeze client
3. Otherwise → update client state with new header

**`get_client_status()`** — checks if client is usable:
1. Can we find the client type? → Unknown if not
2. Can we find the client state? → Unknown if not
3. Is the client frozen? → Frozen
4. Has the trusting period expired since latest consensus state? → Expired
5. Otherwise → Active

### ics02_validation.rs (What We're Replacing)

8 Tendermint-specific functions that are the primary target for multi-client refactoring:

```rust
pub fn is_tendermint_header_state(header: &Any) -> bool;
pub fn is_tendermint_consensus_state(consensus_state: &Any) -> bool;
pub fn is_tendermint_client_state(client_state: &Any) -> bool;
pub fn is_tendermint_misbehavior(misbehavior: &Any) -> bool;
pub fn get_tendermint_misbehavior(misbehavior: Any) -> Result<TendermintMisbehavior>;
pub fn get_tendermint_header(header: Any) -> Result<TendermintHeader>;
pub fn get_tendermint_consensus_state(consensus_state: Any) -> Result<TendermintConsensusState>;
pub fn get_tendermint_client_state(client_state: Any) -> Result<TendermintClientState>;
```

Plus `validate_penumbra_sdk_client_state()` which validates Tendermint client parameters (frozen, chain_id, revision, height, proof_specs, trust_threshold, unbonding_period).

### proof_verification.rs

Merkle proof verification for all IBC object types. Uses `ics23::verify_membership` / `verify_non_membership` with `IBC_PROOF_SPECS`. Currently typed to `TendermintClientState` — needs to dispatch by client type for multi-client support.

### Packet Commitments

Following ibc-go's pattern (SHA256-based):
```rust
// packet commitment = SHA256(timeout_timestamp || revision_number || revision_height || SHA256(data))
// ack commitment = SHA256(ack_bytes)
```

## Proto System

### Proto Source

Proto definitions live at `proto/penumbra/penumbra/core/component/ibc/v1/ibc.proto`. Vendored external protos (cosmos, ibc, ics23) live at `proto/rust-vendored/`.

### IBC Light Client Protos

Existing pattern at `proto/rust-vendored/ibc/lightclients/tendermint/v1/tendermint.proto`. New bankd light client protos go at `proto/rust-vendored/ibc/lightclients/bankd/v1/bankd.proto`.

### Code Generation

```sh
# Full proto regeneration (from repo root)
just proto
# Which runs: ./deployments/scripts/protobuf-codegen
```

This pulls deps from Buf Schema Registry, then runs `tools/proto-compiler/` to generate Rust types into `crates/proto/src/gen/`.

Generated types live in the `penumbra-sdk-proto` crate:
```
crates/proto/src/gen/
  penumbra.core.component.ibc.v1.rs        (auto-generated Prost types)
  penumbra.core.component.ibc.v1.serde.rs  (JSON serialization)
  ibc.core.client.v1.rs                     (IBC client types)
  ibc.lightclients.tendermint.v1.rs         (Tendermint LC types)
  // bankd LC types would go here after proto build
```

## Key Dependencies

```toml
# IBC Protocol
ibc-types = "0.15.1"      # Domain types (client, connection, channel, packet)
                           # Only concrete Tendermint types — no client traits
ibc-proto = "0.51.1"      # Protobuf definitions for IBC messages
ics23 = "0.12"             # Merkle proof verification

# State Storage
cnidarium = "0.83"         # JMT-backed state (StateRead, StateWrite, StateDelta)

# Tendermint
tendermint = "0.40.3"      # Block headers, Time, validators
tendermint-light-client-verifier = "0.40.3"  # LC verification engine

# Penumbra Internal
penumbra-sdk-proto          # Generated proto types
penumbra-sdk-sct            # Epochs, block timestamps (EpochManager, clock)
penumbra-sdk-asset           # Asset types and denominations

# Serialization
prost = "0.13.4"            # Protocol buffers
serde = "1"                 # JSON/TOML serialization

# Async
async-trait                 # Async trait methods
tokio = "1"                 # Async runtime
futures = "0.3"             # Stream utilities
```

### ibc-types v0.15.1 Limitations

ibc-types provides **only concrete Tendermint types** — there are no client trait abstractions. ADR-004 proposed trait objects but never shipped. This is why we build our own dispatch via `AnyClientState`/`AnyConsensusState` enums rather than using upstream trait infrastructure.

Key types from ibc-types:
- `ibc_types::lightclients::tendermint::{ClientState, ConsensusState, Header, Misbehaviour}`
- `ibc_types::core::client::{ClientId, ClientType, Height}`
- `ibc_types::core::connection::{ConnectionEnd, ConnectionId}`
- `ibc_types::core::channel::{ChannelEnd, ChannelId, PortId, Packet}`
- `ibc_types::core::commitment::{MerkleRoot, MerklePrefix, MerkleProof}`
- `ibc_types::DomainType` — trait for proto ↔ domain type conversions

## IBC Component File Map

| File | Lines | Purpose |
|------|-------|---------|
| **Root module** | | |
| `lib.rs` | 28 | Feature-gated exports, IbcRelay, IBC_PROOF_SPECS |
| `ibc_action.rs` | 292 | IbcRelay enum (18 variants) + proto conversions |
| `ibc_token.rs` | 68 | IBC token denomination handling |
| `params.rs` | 50 | IBCParameters (enabled flags) |
| `prefix.rs` | 67 | IBC_SUBSTORE_PREFIX, IBC_PROOF_SPECS, vendored JMT ics23_spec |
| `genesis.rs` | 37 | Genesis initialization |
| **Component module** | | |
| `ibc_component.rs` | 78 | Ibc struct: init_chain, begin_block (saves own consensus state) |
| `client.rs` | 742 | Client state machine, next_tendermint_state, ClientStatus, StateRead/WriteExt |
| `client_counter.rs` | 88 | ClientCounter, VerifiedHeights |
| `client_recovery.rs` | 264 | Hard-fork client recovery (fork-specific) |
| `connection.rs` | 89 | Connection state read/write |
| `connection_counter.rs` | 26 | Connection counter |
| `channel.rs` | 268 | Channel state, sequences, capabilities |
| `packet.rs` | 313 | Packet commitments (SHA256), ack commitments |
| `proof_verification.rs` | 486 | ics23 proof verification for all IBC object types |
| `ics02_validation.rs` | 160 | Tendermint type checking + counterparty client validation |
| `app_handler.rs` | 64 | AppHandlerCheck + AppHandlerExecute traits |
| `host_interface.rs` | 10 | HostInterface trait (chain_id, height, timestamp) |
| `msg_handler.rs` | 39 | MsgHandler trait definition |
| `view.rs` | 30 | IBCParameters read/write |
| `state_key.rs` | 32 | State key format strings |
| `ibc_action_with_handler.rs` | 33 | IbcRelayWithHandlers wrapper |
| `metrics.rs` | 30 | Prometheus metrics |
| **Message handlers** (`msg_handler/`) | | |
| `create_client.rs` | 91 | MsgCreateClient |
| `update_client.rs` | 271 | MsgUpdateClient (header verification + state update) |
| `upgrade_client.rs` | 150 | MsgUpgradeClient |
| `misbehavior.rs` | 191 | MsgSubmitMisbehaviour (client freezing) |
| `connection_open_init.rs` | 89 | ConnectionOpenInit |
| `connection_open_try.rs` | 222 | ConnectionOpenTry (proof verification) |
| `connection_open_ack.rs` | 243 | ConnectionOpenAck (proof verification) |
| `connection_open_confirm.rs` | 132 | ConnectionOpenConfirm |
| `channel_open_init.rs` | 123 | ChannelOpenInit |
| `channel_open_try.rs` | 141 | ChannelOpenTry |
| `channel_open_ack.rs` | 135 | ChannelOpenAck |
| `channel_open_confirm.rs` | 120 | ChannelOpenConfirm |
| `channel_close_init.rs` | 87 | ChannelCloseInit |
| `channel_close_confirm.rs` | 125 | ChannelCloseConfirm |
| `recv_packet.rs` | 162 | RecvPacket (proof + app callback) |
| `acknowledgement.rs` | 139 | Acknowledgement |
| `timeout.rs` | 158 | Timeout |

## Testing

### Test Infrastructure

```rust
// Unit tests use cnidarium::StateDelta for in-memory state
let state = Arc::new(StateDelta::new(()));

// MockHost implements HostInterface for tests
struct MockHost {}
impl HostInterface for MockHost {
    async fn get_chain_id(_state) -> Result<String> { Ok("mock_chain_id".into()) }
    async fn get_block_height(state) -> Result<u64> { state.get_block_height().await }
    // ...
}

// MockAppHandler implements AppHandler (all methods return Ok)
struct MockAppHandler {}
impl AppHandlerCheck for MockAppHandler { /* all Ok */ }
impl AppHandlerExecute for MockAppHandler { /* all no-op */ }
```

### Running Tests

```sh
cargo test -p penumbra-sdk-ibc          # IBC crate tests
cargo nextest run --release             # Full workspace tests
just smoke                              # Smoke test suite (process-compose)
```

### Test Data

Real IBC messages from Cosmos Hub are used as test fixtures:
- `component/test/create_client.msg` — base64 MsgCreateClient (Stargaze LC on Cosmos Hub)
- `component/test/update_client_1.msg` — first update
- `component/test/update_client_2.msg` — second update

## Code Conventions

- `#![deny(clippy::unwrap_used)]` — no unwrap in production code
- `async_trait` for all async trait methods
- `anyhow::Result` for error handling throughout
- Feature-gated component code (`#[cfg(feature = "component")]`)
- Extension traits auto-implemented via `impl<T: StateRead + ?Sized> FooExt for T {}`
- State keys as plain strings, not typed enums
- Domain types ↔ proto conversions via `DomainType` trait
- `tracing::instrument` on component lifecycle methods

## Build Profiles

```toml
[profile.release]   # Used for all test/build commands
# Default Cargo release settings
```

Workspace resolver = "2". Proto compiler excluded from workspace (`tools/proto-compiler`).
