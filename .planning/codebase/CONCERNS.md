# Codebase Concerns

**Analysis Date:** 2026-05-12

## Tech Debt

**IBC authorization and validation gaps:**
- Issue: Several IBC channel and packet handlers carry explicit capability-authentication TODOs, and some proof paths check frozen clients without checking expiration.
- Files: `crates/core/component/ibc/src/component/msg_handler/channel_open_init.rs`, `crates/core/component/ibc/src/component/msg_handler/channel_open_try.rs`, `crates/core/component/ibc/src/component/msg_handler/channel_close_init.rs`, `crates/core/component/ibc/src/component/msg_handler/channel_close_confirm.rs`, `crates/core/component/ibc/src/component/msg_handler/recv_packet.rs`, `crates/core/component/ibc/src/component/proof_verification.rs`
- Impact: Channel-open/close and packet execution behavior is spread across handlers and easy to change inconsistently. Missing close authorization is explicitly called out as allowing spurious channel closes.
- Fix approach: Add a narrow channel capability/authorization trait in `crates/core/component/ibc/src/component/`, call it from every channel/packet handler, and add cross-handler app tests under `crates/core/app-tests/tests/common/ibc_tests/`.

**Compliance registry authorization is incomplete:**
- Issue: User registration accepts `MsgRegisterUser` without proving ownership of the registered address, and asset registration accepts `MsgRegisterAsset` without governance or issuer authorization.
- Files: `crates/core/component/compliance/src/component/state.rs`
- Impact: Registry state can be mutated by actions whose authorization is not bound to the address or issuer authority named by the message.
- Fix approach: Bind `MsgRegisterUser` to a signature over the leaf commitment and bind `MsgRegisterAsset` to governance or issuer authority before `check_and_execute` writes registry state.

**Regulated asset channel policy is stored but not enforced:**
- Issue: `AssetPolicy.allowed_channels` exists, but shielded ICS-20 withdrawal checking bypasses regulated-asset channel allowlist enforcement.
- Files: `crates/core/component/compliance/src/structs.rs`, `crates/core/component/shielded-pool/src/component/transfer.rs`
- Impact: A regulated asset policy can advertise IBC channel constraints that the withdrawal path does not enforce.
- Fix approach: Load `AssetPolicy` in `withdrawal_check_cached`, reject regulated withdrawals whose `source_channel` is not allowed, and cover allowed/blocked channels in `crates/core/app-tests/tests/ics23_transfer.rs`.

**Fixed-point R1CS arithmetic is partial:**
- Issue: Division uses an `ibig::UBig` stub and the intended 384-bit by 256-bit division implementation contains `todo!()` calls. R1CS subtraction and equality checks are also incomplete.
- Files: `crates/core/num/src/fixpoint/div.rs`, `crates/core/num/src/fixpoint.rs`
- Impact: Circuit code that needs fixed-point division, subtraction, or equality can panic or depend on a non-final implementation.
- Fix approach: Replace `stub_div_rem_u384_by_u256` with the native limb algorithm, implement subtraction/equality gadgets, and keep the existing proptest identity checks as parity tests.

**R1CS allocation modes are incomplete across asset/key primitives:**
- Issue: Several `AllocVar` implementations support only the allocation mode used by existing circuits and panic for other valid modes.
- Files: `crates/core/asset/src/balance.rs`, `crates/core/asset/src/balance/commitment.rs`, `crates/core/keys/src/keys/fvk/r1cs.rs`, `crates/core/keys/src/keys/nullifier.rs`, `crates/core/component/sct/src/nullifier.rs`, `crates/crypto/tct/src/r1cs.rs`
- Impact: New circuits can fail at runtime when they allocate constants, public inputs, or witnesses through shared primitives.
- Fix approach: Implement all `AllocationMode` branches or replace unsupported branches with `SynthesisError` paths that fail without panicking.

**Ledger custody support is incomplete:**
- Issue: Ledger custody implements transfer authorization, FVK export, and address confirmation, but validator and governance authorization RPCs panic via `unimplemented!()`.
- Files: `crates/custody-ledger-usb/src/lib.rs`, `crates/custody-ledger-usb/src/device.rs`
- Impact: A client using the generic custody service can crash the Ledger service by calling unsupported validator or proposal endpoints.
- Fix approach: Return typed `tonic::Status::unimplemented` responses until Ledger APDUs exist, then add hardware-backed implementations and integration tests around `crates/custody-ledger-usb/`.

**Governance parameter validation is centralized and partial:**
- Issue: Parameter-change validation is performed in `AppParameters` rather than component domain conversions, upgrade-plan proposals have no stateful checks, and chain-id writes are ignored when app parameters are persisted.
- Files: `crates/core/app/src/params/change.rs`, `crates/core/app/src/action_handler/actions/submit.rs`, `crates/core/app/src/app/mod.rs`
- Impact: Adding new parameters requires updating central validation code and can silently omit component-specific invariants.
- Fix approach: Move validation into each component's domain type and require governance proposal tests for every new parameter or upgrade-plan field.

**Protocol query APIs ignore pagination:**
- Issue: Several ABCI/gRPC query paths decode pagination requests but return full result sets with `pagination: None`.
- Files: `crates/core/app/src/server/info.rs`, `crates/core/component/shielded-pool/src/component/rpc/transfer_query.rs`, `crates/core/component/shielded-pool/src/component/rpc/bank_query.rs`, `crates/bin/pcli/src/command/query/ibc_query.rs`
- Impact: Query cost grows with clients, channels, connections, denoms, and supply entries; CLI users cannot page large result sets.
- Fix approach: Implement shared pagination helpers for state prefix iteration, then wire pcli to request pages rather than `pagination: None`.

**Large modules concentrate unrelated responsibilities:**
- Issue: Some stateful modules are large enough that consensus, mempool, RPC, test fixtures, and planning logic are difficult to review in one pass.
- Files: `crates/core/app/src/app/mod.rs`, `crates/view/src/service.rs`, `crates/view/src/note_manager.rs`, `crates/core/component/compliance/src/registry.rs`, `crates/core/component/compliance/src/indexed_tree.rs`, `crates/core/app/src/local_mempool.rs`
- Impact: Cross-cutting edits have high regression risk because invariants are enforced by local ordering and many `expect(...)` calls rather than small typed interfaces.
- Fix approach: Extract state-machine submodules around narrow contracts and move fixture-heavy tests into dedicated `tests/` or `test_support` modules.

**POC code duplicates production mempool logic:**
- Issue: `poc/crates/preconsensus/src/local_mempool.rs` tracks a variant of `crates/core/app/src/local_mempool.rs` but the `poc` workspace is excluded from the root workspace.
- Files: `Cargo.toml`, `crates/core/app/src/local_mempool.rs`, `poc/crates/preconsensus/src/local_mempool.rs`
- Impact: Preconsensus/mempool fixes can drift between the production crate and the POC crate without root workspace checks catching it.
- Fix approach: Delete the POC copy if obsolete, or extract shared mempool logic into a workspace crate and make `poc` opt into the same test surface.

## Known Bugs

**`pd export --prune` panics:**
- Symptoms: Passing `--prune` to the export command reaches `unimplemented!("storage pruning is unimplemented (for now)")`.
- Files: `crates/bin/pd/src/main.rs`, `crates/bin/pd/src/cli.rs`
- Trigger: Run `pd export --prune ...`.
- Workaround: Export without `--prune`.

**Unsupported migrations panic instead of returning a CLI error:**
- Symptoms: Migration variants not handled by the new framework reach `unimplemented!("the specified migration is unimplemented")`.
- Files: `crates/bin/pd/src/migrate.rs`
- Trigger: Invoke a migration variant outside `Mainnet3ToMainnet4` or `NoOp`.
- Workaround: Use only implemented migration variants.

**Ledger custody unsupported RPCs panic:**
- Symptoms: `authorize_validator_definition`, `authorize_validator_vote`, and `authorize_proposal_submit` panic when called.
- Files: `crates/custody-ledger-usb/src/lib.rs`
- Trigger: Use a Ledger custody service for validator or governance proposal authorization.
- Workaround: Use `crates/custody/src/soft_kms.rs` or return `Status::unimplemented` before exposing these RPCs.

**Bank total supply reports incomplete values:**
- Symptoms: Non-IBC assets are included with a hardcoded zero supply, while IBC balances are accumulated from escrow state.
- Files: `crates/core/component/shielded-pool/src/component/rpc/bank_query.rs`
- Trigger: Call the Cosmos bank `total_supply` query for chain-native assets.
- Workaround: Use domain-specific shielded-pool accounting instead of the Cosmos bank compatibility query.

**pclientd reflection advertises unavailable IBC services:**
- Symptoms: IBC services appear in gRPC reflection but are not added to the pclientd proxy service set.
- Files: `crates/bin/pclientd/src/lib.rs`
- Trigger: Use reflection-driven clients against pclientd and call reflected IBC services.
- Workaround: Connect directly to `pd` for IBC queries.

## Security Considerations

**Compliance registration authority:**
- Risk: Address ownership and asset issuer/governance authority are not checked before compliance registry mutations.
- Files: `crates/core/component/compliance/src/component/state.rs`, `crates/core/component/compliance/src/structs.rs`
- Current mitigation: User registration is limited to regulated assets, duplicate registration is idempotent, and regulated asset registration requires `dk_pub`.
- Recommendations: Add signature/domain separation for user leaves, define the asset-registration authority model, and add failing unauthorized-registration tests.

**Regulated asset IBC policy bypass:**
- Risk: `allowed_channels` can be set on asset policy but is not enforced by withdrawal checking.
- Files: `crates/core/component/compliance/src/structs.rs`, `crates/core/component/shielded-pool/src/component/transfer.rs`
- Current mitigation: None in the withdrawal path.
- Recommendations: Enforce the policy before packet creation and test empty, matching, and mismatched allowlists.

**IBC channel capability authentication:**
- Risk: Channel close/open/packet handlers do not authenticate application channel capabilities; close handlers explicitly note that this can allow spurious channel closure.
- Files: `crates/core/component/ibc/src/component/msg_handler/channel_close_init.rs`, `crates/core/component/ibc/src/component/msg_handler/channel_close_confirm.rs`, `crates/core/component/ibc/src/component/msg_handler/channel_open_init.rs`, `crates/core/component/ibc/src/component/msg_handler/channel_open_try.rs`, `crates/core/component/ibc/src/component/msg_handler/recv_packet.rs`, `crates/core/component/ibc/src/component/msg_handler/acknowledgement.rs`, `crates/core/component/ibc/src/component/msg_handler/timeout.rs`
- Current mitigation: Handlers restrict ports to `transfer` and perform channel/connection/proof checks.
- Recommendations: Add a channel capability owner check and verify unauthorized close/recv/ack/timeout messages fail in app tests.

**IBC client validation and proof freshness:**
- Risk: Client-state validation checks chain id, revision, proof specs, and trust threshold, but does not fully validate unbonding-period policy or upgrade path; channel and packet proof verification paths also omit expiration checks in some helpers.
- Files: `crates/core/component/ibc/src/component/ics02_validation.rs`, `crates/core/component/ibc/src/component/proof_verification.rs`, `crates/core/component/ibc/src/component/msg_handler/update_client.rs`
- Current mitigation: `MsgUpdateClient` checks expiration, frozen clients are rejected, proof heights are verified, and client recovery checks unbonding/upgrade-path equality.
- Recommendations: Centralize active-client validation so every proof verifier checks frozen and expired status, and make upgrade path/unbonding policy explicit chain parameters.

**Sensitive wallet data in debug/trace logs:**
- Risk: View-client code logs full note collections, balance results, requested commitments, and witness data at trace/debug levels.
- Files: `crates/view/src/client.rs`, `crates/view/src/service.rs`, `crates/bin/pclientd/src/main.rs`
- Current mitigation: Default `RUST_LOG` filter is `info`, and pclientd binds to `127.0.0.1:8081` by default.
- Recommendations: Keep wallet-bearing structures out of debug/trace logs or redact note contents before logging; do not enable trace logs on shared systems.

**Public web access to node gRPC:**
- Risk: `pd` applies permissive CORS to the merged gRPC/frontend router.
- Files: `crates/bin/pd/src/main.rs`, `crates/bin/pd/src/cli.rs`
- Current mitigation: Default non-HTTPS gRPC bind is `127.0.0.1:8080`; public exposure requires explicit bind/auto-HTTPS configuration.
- Recommendations: Keep state-changing or wallet-specific APIs out of `pd`, and require explicit CORS review before adding non-public endpoints to the merged router.

## Performance Bottlenecks

**Unpaginated full-result queries:**
- Problem: Query handlers collect all matching state into memory before responding.
- Files: `crates/core/app/src/server/info.rs`, `crates/core/component/shielded-pool/src/component/rpc/transfer_query.rs`, `crates/core/component/shielded-pool/src/component/rpc/bank_query.rs`
- Cause: Pagination request fields are ignored and streams are collected into `Vec<_>`.
- Improvement path: Add bounded state-prefix pagination and return `next_key`/`total` metadata where the protobuf supports it.

**View note selection filters in application code:**
- Problem: Spendable-note selection performs SQL queries with broad filters, then filters account matches and amount cutoffs in Rust.
- Files: `crates/view/src/storage.rs`
- Cause: Address account grouping and "up to amount" logic are not expressed in SQL; the SQLite pool is also capped at one connection to avoid lock errors.
- Improvement path: Add normalized account columns/indexes, implement amount-limited SQL queries, and keep one-connection SQLite access behind an async queue if lock avoidance remains required.

**Compliance registry tree updates serialize whole trees:**
- Problem: User and asset registration load, clone/cache, update, serialize, and write entire Merkle/IMT structures.
- Files: `crates/core/component/compliance/src/registry.rs`, `crates/core/component/compliance/src/tree.rs`, `crates/core/component/compliance/src/indexed_tree.rs`
- Cause: Trees are stored as serialized blobs plus object-store caches; `IndexedMerkleTree::find_low_leaf` linearly scans leaves on misses.
- Improvement path: Persist tree nodes and value indexes as typed state records, keep hot caches bounded to a block, and benchmark registration cost at high leaf counts.

**Compliance leaf verification is linear:**
- Problem: `verify_compliance_leaf` scans every user position to find a commitment.
- Files: `crates/core/component/compliance/src/registry.rs`
- Cause: Reverse lookup is keyed by `(address, asset_id)`, not by leaf commitment.
- Improvement path: Add a `commitment -> position` index and use it for verification/proof generation.

**Large local build artifacts slow filesystem-wide scans:**
- Problem: Local `target/`, `poc/target/`, and `tmp/` directories are large enough to dominate naive `find`, `rg`, and backup operations.
- Files: `target/`, `poc/target/`, `tmp/`
- Cause: Release/debug build outputs and temporary integration artifacts are present in the workspace.
- Improvement path: Exclude these paths from codebase tooling and keep repo scans based on `git ls-files` or explicit source roots.

## Fragile Areas

**Consensus app state machine:**
- Files: `crates/core/app/src/app/mod.rs`
- Why fragile: The file owns check-tx/proposal/block/end-block/commit flows and relies on many invariant `expect(...)` calls around Arc uniqueness, component state, compact block finalization, and proof-family lookups.
- Safe modification: Change one consensus phase at a time, add a focused app-test under `crates/core/app-tests/tests/`, and run `just ci-check` plus the relevant `cargo nextest` package slice.
- Test coverage: App tests cover many happy paths, but invariant failures in rare block/mempool/proposal interleavings need targeted regression tests.

**IBC protocol handlers:**
- Files: `crates/core/component/ibc/src/component/msg_handler/`, `crates/core/component/ibc/src/component/proof_verification.rs`, `crates/core/component/ibc/src/component/packet.rs`
- Why fragile: Connection, channel, packet, timeout, and client-update rules are split across many files, with duplicated active-client/proof validation concerns.
- Safe modification: Add helper functions for shared validation and drive changes through cross-chain tests in `crates/core/app-tests/tests/common/ibc_tests/`.
- Test coverage: Existing IBC app-test scaffolding exercises transfers, but TODO-marked authorization and pagination behavior lacks direct negative tests.

**Compliance registry and scanner:**
- Files: `crates/core/component/compliance/src/component/state.rs`, `crates/core/component/compliance/src/registry.rs`, `crates/core/component/compliance/src/indexed_tree.rs`, `crates/core/component/compliance/src/scanner/worker.rs`
- Why fragile: Compliance combines on-chain registry state, custom Merkle/IMT implementations, compact-block events, RPC proofs, and scanner persistence.
- Safe modification: Preserve typed replayable records for registrations and scanner events; add parity tests for tree roots and proof verification before changing storage layout.
- Test coverage: Unit tests cover tree mechanics and basic registration paths, but authorization, channel policy, and high-cardinality registry performance need tests.

**Proof aggregation backend:**
- Files: `crates/crypto/proof-aggregation/src/lib.rs`, `crates/crypto/proof-aggregation/src/backend.rs`, `crates/crypto/proof-aggregation/vendor/ripp/`
- Why fragile: Consensus proof aggregation depends on locally vendored SnarkPack/RIPP code with internal TODOs and unwraps in generic proof helpers.
- Safe modification: Keep all backend changes behind `AggregationBackend`, run `just gnark-proof-tests` and the slow proof tests before merging proof-family changes.
- Test coverage: Backend tests cover aggregate acceptance/rejection and family/count matching; vendored helper edge cases remain hard to audit.

**View planning and sync:**
- Files: `crates/view/src/note_manager.rs`, `crates/view/src/service.rs`, `crates/view/src/storage.rs`, `crates/view/src/worker.rs`
- Why fragile: Planning, maintenance transactions, note selection, witness assembly, storage, and sync health live across large modules with async boundaries.
- Safe modification: Keep pure planning changes in `crates/view/src/note_manager.rs`, storage changes in `crates/view/src/storage.rs`, and service/RPC changes in `crates/view/src/service.rs`; add tests for resume tokens and partial-funding flows.
- Test coverage: Unit tests cover many planner cases, but worker liveness is only a TODO in `crates/view/src/service.rs`.

**Ledger USB APDU integration:**
- Files: `crates/custody-ledger-usb/src/device.rs`, `crates/custody-ledger-usb/src/lib.rs`
- Why fragile: APDU request construction uses fixed buffers, external Ledger app specs, first-device selection, and long device exchange timeouts.
- Safe modification: Validate payload lengths without `assert!`, surface device/protocol errors through `Status`, and isolate APDU encoding into testable pure functions.
- Test coverage: No local hardware or mock-ledger tests were detected under `crates/custody-ledger-usb/`.

## Scaling Limits

**State commitment tree insertion per block:**
- Current capacity: The TCT supports 281,474,976,710,656 total commitments, but per-block insertion can fail when the current block is full.
- Limit: Note insertion paths assume SCT insertion cannot fail because commitment-per-block budgeting is not implemented.
- Scaling path: Budget commitments before transaction acceptance/proposal construction and return consensus errors instead of panicking.

**Compliance registry tree blobs:**
- Current capacity: `QuadTree` and `IndexedMerkleTree` use default depth 16, with `1u64 << (depth * 2)` maximum leaves.
- Limit: Blob serialization and linear low-leaf scans become the limiting factor before theoretical tree capacity.
- Scaling path: Store tree nodes/indexes separately and add benchmarks around `add_compliance_leaf`, `register_asset_in_imt`, and `get_asset_proof_data`.

**Mempool memory:**
- Current capacity: Default mempool storage allows `1 << 30` bytes and `usize::MAX` transactions, with fee eviction disabled by default.
- Limit: Byte capacity prevents unbounded transaction bytes, but transaction count and index overhead can still grow until byte eviction activates.
- Scaling path: Set explicit transaction-count defaults, enable fee-based eviction for production, and expose mempool pressure metrics.

**Query response size:**
- Current capacity: Several query APIs return all rows in one response and pcli asks for no pagination.
- Limit: Large state sets can produce slow or oversized responses for clients, channels, denom traces, and supply data.
- Scaling path: Implement pagination in `crates/core/app/src/server/info.rs`, `crates/core/component/shielded-pool/src/component/rpc/`, and `crates/bin/pcli/src/command/query/ibc_query.rs`.

## Dependencies at Risk

**Vendored RIPP/SnarkPack code:**
- Risk: Proof aggregation relies on path dependencies under `crates/crypto/proof-aggregation/vendor/ripp/` rather than normal workspace crates from crates.io.
- Impact: Security review, upstream fixes, and dependency updates require local vendored-code maintenance.
- Migration plan: Track the vendored source revision in documentation, upstream local patches where possible, and keep all vendored use behind `crates/crypto/proof-aggregation/src/backend.rs`.

**Pinned Ledger git dependencies:**
- Risk: `ledger-lib` and `ledger-proto` are pinned to a git revision in workspace dependencies.
- Impact: Ledger transport fixes are not received through normal semver updates, while the local Ledger service has unimplemented RPCs.
- Migration plan: Move to released crates when available or create a documented update procedure with APDU compatibility tests.

**Excluded POC workspace:**
- Risk: `poc/` is excluded from the root workspace and has its own dependency graph.
- Impact: `just ci-check` and workspace tests do not guarantee POC code builds after production API changes.
- Migration plan: Delete obsolete POC code or add explicit CI commands for the POC when it is intentionally maintained.

## Missing Critical Features

**Storage pruning:**
- Problem: `pd export --prune` is exposed by the CLI but not implemented.
- Blocks: Producing pruned state exports for operators.

**Ledger validator/governance support:**
- Problem: Ledger custody cannot authorize validator definitions, validator votes, or governance proposal submissions.
- Blocks: Hardware-wallet users from performing validator/governance workflows through the generic custody interface.

**Compliance authority and allowlist enforcement:**
- Problem: Compliance registry mutations and regulated-asset IBC policies are not fully authorized/enforced.
- Blocks: Treating regulated asset policy as a complete on-chain control surface.

**Complete Cosmos compatibility queries:**
- Problem: Bank and transfer query APIs leave supply, escrow address, denom hash, params, balances, and pagination unimplemented or partial.
- Blocks: Cosmos/IBC tooling that expects full bank/transfer query semantics.

## Test Coverage Gaps

**Compliance authorization and policy enforcement:**
- What's not tested: Unauthorized `MsgRegisterUser`, unauthorized `MsgRegisterAsset`, and regulated withdrawal through disallowed channels.
- Files: `crates/core/component/compliance/src/component/state.rs`, `crates/core/component/shielded-pool/src/component/transfer.rs`, `crates/core/app-tests/tests/ics23_transfer.rs`
- Risk: Registry and IBC policy bypasses can ship because existing tests prove registration works, not that unauthorized registration fails.
- Priority: High

**IBC capability-authentication negatives:**
- What's not tested: Unauthorized channel close/open/ack/recv/timeout paths.
- Files: `crates/core/component/ibc/src/component/msg_handler/`, `crates/core/app-tests/tests/common/ibc_tests/`
- Risk: Handler TODOs remain invisible to CI because valid relayer flows pass.
- Priority: High

**Ledger custody unsupported endpoints:**
- What's not tested: Calling every custody RPC through the Ledger service and verifying graceful errors for unsupported operations.
- Files: `crates/custody-ledger-usb/src/lib.rs`, `crates/custody-ledger-usb/src/device.rs`
- Risk: Generic custody clients can panic the service in workflows that soft KMS supports.
- Priority: Medium

**Pagination and large query behavior:**
- What's not tested: Query responses with enough clients/channels/denoms/supply entries to require pagination.
- Files: `crates/core/app/src/server/info.rs`, `crates/core/component/shielded-pool/src/component/rpc/transfer_query.rs`, `crates/core/component/shielded-pool/src/component/rpc/bank_query.rs`, `crates/bin/pcli/src/command/query/ibc_query.rs`
- Risk: Operators discover response-size and memory issues only after state grows.
- Priority: Medium

**Pruning and migration CLI error handling:**
- What's not tested: `pd export --prune` returning a user-facing error and unsupported migrations failing without panic.
- Files: `crates/bin/pd/src/main.rs`, `crates/bin/pd/src/migrate.rs`, `crates/bin/pd/tests/network_integration.rs`
- Risk: Operator commands fail as panics rather than actionable CLI errors.
- Priority: Medium

---

*Concerns audit: 2026-05-12*
