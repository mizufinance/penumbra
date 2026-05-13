# Phase 1: Compliance Boundary MVP - Research

**Researched:** 2026-05-13  
**Domain:** Rust compliance component boundary refactor  
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
## Implementation Decisions

### Target Boundary
- **D-01:** Evaluate both `registry/state` and `audit/export` before selecting the implementation target.
- **D-02:** Implement both refactors only if evidence shows both deliver high payoff without making the phase too broad.
- **D-03:** The selected target must be justified against at least one rejected alternative in the implementation notes or plan.

### Boundary Shape
- **D-04:** Prefer typed domain records first when facts cross internal boundaries.
- **D-05:** Prefer pure validation, classification, projection, or domain-record construction helpers before introducing new effect boundaries.
- **D-06:** Keep durable state access at the edge through existing `StateRead` / `StateWrite` and state-key patterns.
- **D-07:** Add provider traits only for real external effects, not for internal indirection or scanner-name symmetry.
- **D-08:** Use the scanner architecture as a reference for ownership separation, not as a template to copy mechanically.

### Behavior Changes
- **D-09:** Do not implement existing TODO/security items such as registration authorization or regulated asset IBC policy enforcement in this phase.
- **D-10:** Behavior-changing cleanup is allowed when it is obviously better, directly related to the selected boundary, and covered by tests.
- **D-11:** Avoid broad semantic changes. If a change looks like a new capability or deferred security feature, leave it for a later phase.

### Verification Bar
- **D-12:** The proof bar is high: focused unit tests, relevant integration tests, and the full local CI/check pipeline should pass.
- **D-13:** If a broad check cannot be run locally, the final handoff must state exactly what was not run and why.
- **D-14:** Tests should prove behavior preservation and make the refactored flow easier to exercise directly.

### the agent's Discretion
The implementation agent may choose the final target boundary after source analysis, provided it evaluates both registry/state and audit/export and records the evidence. The agent may also choose the exact module names and extraction shape, as long as the result follows existing Penumbra patterns and does not add needless indirection.

### Deferred Ideas (OUT OF SCOPE)
- Registration authorization for `MsgRegisterUser` and `MsgRegisterAsset` remains deferred.
- Regulated asset `allowed_channels` enforcement remains deferred.
- Registry storage scaling changes such as node/index persistence remain deferred unless directly necessary for the selected refactor.
- Non-compliance refactors remain deferred.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| EVID-01 | Identify highest boundary-cleanup payoff using compliance source evidence. | Candidate evidence compares `audit.rs` mixed SQLite/export/orchestration functions with `registry.rs` chain-state traits and tests. [VERIFIED: codebase grep] |
| EVID-02 | Justify selected target against at least one rejected alternative. | Recommendation selects `audit/export` and rejects `registry/state` for this phase because registry cleanup risks pulling in deferred storage/security semantics. [VERIFIED: `.planning/REQUIREMENTS.md`; codebase grep] |
| EVID-03 | Limit scope to one or two boundaries. | Recommendation is one boundary only: audit/export. [VERIFIED: `.planning/phases/01-compliance-boundary-mvp/01-CONTEXT.md`] |
| ARCH-01 | Separate durable state access from pure validation/projection/domain records. | Extract typed audit row/export/import record builders while leaving SQLite reads/writes at `SqliteScannerStore`/audit edge. [VERIFIED: `audit.rs`; `scanner/types.rs`; `scanner/storage.rs`] |
| ARCH-02 | Expose narrow APIs matching Penumbra Rust patterns. | Use Rust module extraction plus structs/enums, not broad provider traits. [VERIFIED: `audit_validation.rs`; `scanner/types.rs`] |
| ARCH-03 | Remove mixed-responsibility code without shims or speculative traits. | Move helper logic out of `audit.rs`; keep public facade only for existing exported functions used by binaries. [VERIFIED: `lib.rs`; caller grep] |
| ARCH-04 | Preserve public behavior unless focused failing test proves a bug. | Existing audit tests already cover evidence persistence, Orbis export/import gating, decryption gating, and rollback behavior. [VERIFIED: `audit.rs`] |
| IMPL-01 | Keep product code primarily in compliance source. | Recommended files stay in `crates/core/component/compliance/src/`, with no required binary changes unless names move. [VERIFIED: caller grep] |
| IMPL-02 | Use scanner architecture as reference, not naming template. | Reuse scanner typed record style, not scanner module names. [VERIFIED: `scanner/types.rs`; CONTEXT D-08] |
| IMPL-03 | Remove obsolete internal paths. | Planner should update imports and delete replaced private helper paths; no alias modules. [VERIFIED: AGENTS.md] |
| IMPL-04 | Make selected flow easier to test directly. | Pure projection/import validation helpers can be unit-tested without SQLite setup. [VERIFIED: `audit_validation.rs`; `audit.rs`] |
| VERI-01 | Focused tests cover selected boundary before/after. | Add tests for pure export/import row classification plus retain existing audit SQLite tests. [VERIFIED: `audit.rs`] |
| VERI-02 | Relevant compliance crate tests pass. | Use `cargo test -p penumbra-sdk-compliance --features component`. [VERIFIED: `Cargo.toml`] |
| VERI-03 | Formatting and narrow compile/test checks pass. | Use `cargo fmt --all -- --check`, `cargo check -p penumbra-sdk-compliance --all-targets --all-features`, and document any skipped `just check`/CI. [VERIFIED: `justfile`; `Cargo.toml`] |
</phase_requirements>

## Summary

`audit/export` is the best MVP boundary target. `crates/core/component/compliance/src/audit.rs` is a 1,166-line module that mixes SQLite queries, status transitions, export JSON/domain record construction, Orbis import validation, evidence failure persistence, decryption updates, and tests. [VERIFIED: `wc -l`; `audit.rs`] It already depends on scanner typed records such as `AuditLedgerRow`, `AuditRowKey`, status constants, and `OutputRef`, so extracting pure audit projection/classification helpers can improve ownership without new external traits. [VERIFIED: `scanner/types.rs`; `audit.rs`]

`registry/state` has real concerns, but it is a weaker MVP target. `registry.rs` is larger at 1,894 lines and contains chain-state reads/writes, tree serialization, proof generation, anchor retention, cache state, and many tests. [VERIFIED: `wc -l`; `registry.rs`] The highest-risk registry concerns are incomplete registration authorization and storage scaling/indexing, both explicitly deferred from this phase. [VERIFIED: `.planning/REQUIREMENTS.md`; `.planning/codebase/CONCERNS.md`] A small registry extraction is possible, but it is more likely to be cosmetic or to touch protocol/state semantics. [VERIFIED: `registry.rs`; `.planning/REQUIREMENTS.md`]

**Primary recommendation:** Refactor one boundary: split `audit/export` into typed audit records plus pure projection/import classification helpers, leaving SQLite effects at the existing store/audit edge and preserving the current public facade. [VERIFIED: `audit.rs`; `lib.rs`; `scanner/storage.rs`]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Audit row projection/export | Component domain module | SQLite scanner store | Output rows are domain records; query execution remains at the SQLite edge. [VERIFIED: `audit.rs`; `scanner/types.rs`] |
| Orbis audit import validation | Component domain module | SQLite scanner store | Entry eligibility and status classification can be pure; row updates remain SQLite effects. [VERIFIED: `audit.rs`] |
| Evidence failure persistence | SQLite scanner store edge | Component domain module | Failure reason/stage classification is domain logic, but inserts/updates are store effects. [VERIFIED: `audit.rs`; `scanner/storage.rs`] |
| Registry registration state | Chain state component | Domain structs | Registry writes belong behind `StateRead`/`StateWrite`; domain records such as `ComplianceLeaf` and `AssetPolicy` cross the boundary. [VERIFIED: `registry.rs`; `component/state.rs`; `structs.rs`] |

## Project Constraints (from AGENTS.md)

- Prefer correct design over compatibility shims; remove obsolete paths instead of aliases. [VERIFIED: AGENTS.md]
- Discuss goal, risks, and intended shape before detailed planning for non-trivial work. [VERIFIED: AGENTS.md]
- If a refactor touches more than five files, make scope explicit first. [VERIFIED: AGENTS.md]
- Follow impact through circuits, domain logic, storage, services, CLI, tests, and docs. [VERIFIED: AGENTS.md]
- Treat durable state as typed, replayable records; separate pure domain logic from effects. [VERIFIED: AGENTS.md]
- Use typed references and canonical identifiers; do not invent synthetic IDs in production paths. [VERIFIED: AGENTS.md]
- Keep dependencies explicit and put external systems behind narrow provider traits only when they are real external effects. [VERIFIED: AGENTS.md]
- Never mark work complete without proof; bug fixes need reproducing tests first. [VERIFIED: AGENTS.md]
- Run focused tests after meaningful sections and relevant broad checks before handoff when feasible. [VERIFIED: AGENTS.md]
- Keep comments/docs succinct and factual; do not duplicate docs across files. [VERIFIED: AGENTS.md]
- During GSD work, persist `.planning/` progress incrementally after verified task or wave progress. [VERIFIED: AGENTS.md]

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Rust toolchain | 1.89.0 | Build, format, and test compliance crate. | Repository pins Rust 1.89 and local `rustc`/`cargo` match. [VERIFIED: `rust-toolchain.toml`; `cargo --version`; `rustc --version`] |
| `anyhow` | workspace | Fallible internal/binary-style operations. | Existing compliance audit/registry code uses `anyhow::Result`, `ensure`, and `Context`. [VERIFIED: `audit.rs`; `registry.rs`; `Cargo.toml`] |
| `rusqlite` | 0.32 | Scanner/audit SQLite persistence. | `SqliteScannerStore` and audit functions use rusqlite connections, transactions, and parameters. [VERIFIED: `Cargo.toml`; `scanner/storage.rs`; `audit.rs`] |
| `serde` / `serde_json` | workspace | Audit export/import serialization. | Audit export structs derive serde and JSON export functions return `serde_json::Value`. [VERIFIED: `audit.rs`; `Cargo.toml`] |
| `cnidarium` `StateRead` / `StateWrite` | workspace | Chain-state reads/writes for registry candidate. | Registry traits extend these state traits and component handlers execute over `StateWrite`. [VERIFIED: `registry.rs`; `component/state.rs`; `Cargo.toml`] |
| `async-trait` | workspace | Async extension traits. | Registry read/write traits and component handlers use `#[async_trait]`. [VERIFIED: `registry.rs`; `component/state.rs`; `Cargo.toml`] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | workspace dev dependency | Temporary SQLite files if tests need file-backed stores. | Use only when `:memory:` SQLite is insufficient. [VERIFIED: `Cargo.toml`; `audit.rs` tests use `:memory:`] |
| `tokio` | workspace dev dependency | Async compliance tests. | Needed for scanner store trait tests and registry/component async tests. [VERIFIED: `Cargo.toml`; `audit.rs`; `registry.rs`; `component/state.rs`] |
| `bincode` | workspace | Registry tree blob serialization. | Relevant only if planner touches registry internals. [VERIFIED: `registry.rs`; `Cargo.toml`] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Audit/export extraction | Registry/state extraction | Registry has larger blast radius and deferred storage/security semantics; audit/export is cleaner for pure helper extraction. [VERIFIED: `audit.rs`; `registry.rs`; `.planning/REQUIREMENTS.md`] |
| Pure helper functions | New provider traits | Provider traits would be speculative because SQLite is already represented by `SqliteScannerStore`/`ScannerStore`; this phase should not add internal indirection. [VERIFIED: `scanner/storage.rs`; CONTEXT D-07] |
| Keep all helpers in `audit.rs` | New focused modules under compliance | Leaving everything in `audit.rs` preserves current mixing and does not satisfy ARCH-01/IMPL-04 as strongly. [VERIFIED: `audit.rs`; `.planning/REQUIREMENTS.md`] |

**Installation:** No new packages are recommended. [VERIFIED: `Cargo.toml`]

**Version verification:** This is a Rust workspace refactor using existing dependencies; no npm packages apply. Local tool versions were verified with `cargo --version`, `rustc --version`, and `just --version`. [VERIFIED: terminal]

## Candidate Boundary Evidence

### Selected: `audit/export`

| Evidence | Payoff | Risk |
|----------|--------|------|
| `audit.rs` combines row mutation, export construction, import validation, evidence validation integration, decryption updates, health JSON, and tests. [VERIFIED: `audit.rs`] | Extracting pure helpers reduces mixed responsibility while preserving public functions. [VERIFIED: `audit.rs`; `lib.rs`] | Public functions are used by `orbis-audit`, `orbis-integration`, and scanner storage; keep facade names stable unless compile fixes are intentional. [VERIFIED: caller grep] |
| Existing scanner records already carry typed references and audit rows. [VERIFIED: `scanner/types.rs`] | Use existing `AuditLedgerRow`, `AuditDetectedRef`, `AuditScanExport`, `OrbisAuditEntry`, and `OutputRef` instead of inventing IDs. [VERIFIED: `audit.rs`; `scanner/types.rs`; `refs.rs`] | Do not move SQLite schema ownership out of `scanner/storage.rs`; that is the durable store edge. [VERIFIED: `scanner/storage.rs`] |
| Existing audit tests cover key behavior through SQLite fixtures. [VERIFIED: `audit.rs`] | Add pure unit tests that exercise classification/projection without SQLite. [VERIFIED: `audit_validation.rs` pattern] | Avoid changing status strings or SQL semantics accidentally; they are stored in SQLite rows. [VERIFIED: `scanner/types.rs`; `scanner/storage.rs`] |

### Rejected for MVP: `registry/state`

| Evidence | Why Not This Phase |
|----------|--------------------|
| Registry traits already isolate chain-state access through `ComplianceRegistryRead` and `ComplianceRegistryWrite`. [VERIFIED: `registry.rs`] | The boundary is imperfect but already follows the required state trait edge better than audit/export does. [VERIFIED: `registry.rs`; CONTEXT D-06] |
| State handlers call registry methods such as `add_compliance_leaf` and `register_asset_in_imt` while emitting events. [VERIFIED: `component/state.rs`] | Extracting action planning records is possible, but the main risks are authorization TODOs and storage/index scaling, both deferred. [VERIFIED: `component/state.rs`; `.planning/REQUIREMENTS.md`] |
| Registry tests already cover leaf insertion, asset registration, idempotence, proof data, anchor validation, and round trips. [VERIFIED: `registry.rs`] | Changing tree/storage internals has high regression risk and is not needed to satisfy the MVP if audit/export is selected. [VERIFIED: `registry.rs`; `.planning/codebase/CONCERNS.md`] |

## Architecture Patterns

### System Architecture Diagram

```text
-------------------+      +----------------------+      +----------------------+
| Scanner pipeline  | ---> | SqliteScannerStore   | ---> | audit.rs facade      |
| typed events      |      | durable SQLite edge  |      | public API callers   |
+-------------------+      +----------+-----------+      +----------+-----------+
                                      |                             |
                                      v                             v
                          +----------------------+      +----------------------+
                          | Pure audit helpers   | ---> | Export/import DTOs   |
                          | classify/project     |      | JSON / Orbis entries |
                          +----------+-----------+      +----------------------+
                                     |
                                     v
                          +----------------------+
                          | SQLite write effects |
                          | status/failures      |
                          +----------------------+
```

### Recommended Project Structure

```text
crates/core/component/compliance/src/
├── audit.rs              # public facade plus SQLite transaction orchestration
├── audit_records.rs      # typed audit/export/import record helpers
├── audit_validation.rs   # existing evidence validation status helpers
├── scanner/
│   ├── types.rs          # existing scanner/audit typed records and constants
│   └── storage.rs        # SQLite store edge and schema
└── lib.rs                # keep intended public exports
```

`audit_records.rs` is a suggested name; planner may choose a better Penumbra-style name, but should avoid scanner-name mimicry. [VERIFIED: CONTEXT D-08; CONVENTIONS.md]

### Pattern 1: Pure Classification Before Persistence

**What:** Compute an explicit status or row-update intent in pure code, then apply the result in one SQLite transaction. [VERIFIED: `audit_validation.rs`; `audit.rs`]

**When to use:** Orbis import eligibility, evidence failure stage mapping, export row DTO construction. [VERIFIED: `audit.rs`]

**Example:**

```rust
// Source: crates/core/component/compliance/src/audit_validation.rs
match validate_audit_evidence(input) {
    AuditValidationStatus::Valid => {}
    AuditValidationStatus::InvalidEvidence(reason) => {
        // persistence stays at the caller/store edge
    }
    _ => {}
}
```

### Pattern 2: Keep SQLite Effects at the Store/Audit Edge

**What:** `SqliteScannerStore` owns schema and connection locking; audit functions may orchestrate transactions but should delegate row shape decisions to pure helpers. [VERIFIED: `scanner/storage.rs`; `audit.rs`]

**When to use:** Export/import functions that must query current durable scanner/audit state. [VERIFIED: `audit.rs`]

### Pattern 3: Public Facade Stability Without Compatibility Shims

**What:** Keep `lib.rs` exports for externally used audit functions unless a compile-driven rename is worth the churn; internal moved helpers should not get alias modules. [VERIFIED: `lib.rs`; caller grep; AGENTS.md]

**When to use:** Refactoring private helper functions out of `audit.rs`. [VERIFIED: `audit.rs`]

### Anti-Patterns to Avoid

- **Adding `AuditStore` provider traits:** SQLite is already represented by `SqliteScannerStore` and `ScannerStore`; a new internal trait would be speculative. [VERIFIED: `scanner/storage.rs`; CONTEXT D-07]
- **Moving schema ownership into audit helper modules:** Durable audit tables are initialized in `scanner/storage.rs`; duplicating schema knowledge makes rollback and scanner commits harder to reason about. [VERIFIED: `scanner/storage.rs`]
- **Changing audit status strings:** Status constants are persisted values such as `pending`, `evidence_valid`, `evidence_invalid`, `decrypt_failed`, and `audit_complete`; changing them is a data migration, not a boundary refactor. [VERIFIED: `scanner/types.rs`; `scanner/storage.rs`]
- **Implementing registration auth or allowed-channel policy enforcement:** These are deferred v2 requirements and out of scope. [VERIFIED: `.planning/REQUIREMENTS.md`; CONTEXT D-09]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SQLite connection/transaction wrapper | A new generic audit store abstraction | Existing `SqliteScannerStore::lock_conn` and `ScannerStore` where already used | Existing code owns locking/schema/rollback behavior. [VERIFIED: `scanner/storage.rs`] |
| Audit status model | Ad hoc strings in new code | Existing constants in `scanner/types.rs` | Status strings are stored in SQLite and used across audit functions. [VERIFIED: `scanner/types.rs`; `audit.rs`] |
| Evidence validity logic | Duplicate evidence parsing and DLEQ/hash checks | Existing `validate_audit_evidence` and `AuditValidationStatus` | The validation module already centralizes pure validation and tests. [VERIFIED: `audit_validation.rs`] |
| Transaction/output identity | New synthetic keys | Existing `OutputRef`, `ActionRef`, `TxRef`, `BlockRef`, and `AuditRowKey` | Scanner, evidence, and audit rows already share canonical typed references. [VERIFIED: `refs.rs`; `scanner/types.rs`] |
| Registry storage scaling | Node/index persistence rewrite | Defer unless directly needed | Scaling changes are v2/deferred and risk broad state migration. [VERIFIED: `.planning/REQUIREMENTS.md`; `.planning/codebase/CONCERNS.md`] |

**Key insight:** The useful MVP is not another abstraction layer; it is extracting pure audit decisions and typed row construction from SQLite orchestration so behavior can be tested without a database for every case. [VERIFIED: `audit.rs`; `audit_validation.rs`]

## Common Pitfalls

### Pitfall 1: Cosmetic Registry Extraction

**What goes wrong:** Planner chooses registry/state because the file is largest, then only moves code around without reducing state/effect mixing. [VERIFIED: `wc -l`; `registry.rs`]  
**Why it happens:** Registry risk is real, but much of it is deferred storage/security work. [VERIFIED: `.planning/REQUIREMENTS.md`; `.planning/codebase/CONCERNS.md`]  
**How to avoid:** Select audit/export for MVP, or require registry tasks to name a pure helper and concrete tests before implementation. [VERIFIED: CONTEXT D-05]  
**Warning signs:** New `registry_*` modules that only re-export old methods or mirror scanner names. [VERIFIED: AGENTS.md; CONTEXT D-08]

### Pitfall 2: Provider Traits for Internal SQLite Calls

**What goes wrong:** The refactor adds traits that wrap existing concrete store calls without isolating a real external effect. [VERIFIED: `scanner/storage.rs`; CONTEXT D-07]  
**Why it happens:** Scanner has a `ScannerStore` trait, but copying that shape mechanically violates the phase decision. [VERIFIED: `scanner/storage.rs`; CONTEXT D-08]  
**How to avoid:** Use pure helpers for row/status decisions and keep `SqliteScannerStore` as the concrete edge. [VERIFIED: `audit.rs`; `audit_validation.rs`]  
**Warning signs:** New trait impls with only one implementation and no external service boundary. [ASSUMED]

### Pitfall 3: Accidentally Changing Audit Semantics

**What goes wrong:** Export/import gating, failure recording, or status transitions change while helpers are moved. [VERIFIED: `audit.rs`]  
**Why it happens:** SQL predicates and status constants encode workflow semantics. [VERIFIED: `audit.rs`; `scanner/types.rs`]  
**How to avoid:** Keep focused before/after tests for Orbis export requiring valid evidence, Orbis import requiring valid evidence, flagged decrypt requiring valid evidence, and rollback cleanup. [VERIFIED: `audit.rs` tests]  
**Warning signs:** Test changes that update expected status names or row counts without a behavior-change rationale. [VERIFIED: `audit.rs` tests]

## Code Examples

### Existing Typed Validation Status

```rust
// Source: crates/core/component/compliance/src/audit_validation.rs
pub enum AuditValidationStatus {
    Valid,
    MissingUploadBundle,
    InvalidEvidence(String),
    InvalidOrbisPackage(String),
}
```

### Existing Scanner Typed Row

```rust
// Source: crates/core/component/compliance/src/scanner/types.rs
pub struct AuditLedgerRow {
    pub height: u64,
    pub tx_hash_hex: String,
    pub action_index: u32,
    pub output_index: u32,
    pub flow_type: String,
    pub asset_id: String,
    pub is_flagged: bool,
    pub audited_subjects: Vec<String>,
}
```

### Existing Public Audit Facade

```rust
// Source: crates/core/component/compliance/src/lib.rs
#[cfg(feature = "component")]
pub use audit::{
    decrypt_flagged_rows, export_ledger_rows, export_orbis_pending_scan,
    import_orbis_audit_entries, mark_row_audited, record_address_alias,
};
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Whole audit workflow logic in one audit module | Scanner already has typed records and pure screener; audit validation is already pure and typed | Present in current source | Audit/export should follow existing typed helper pattern. [VERIFIED: `audit.rs`; `scanner/types.rs`; `scanner/screener.rs`; `audit_validation.rs`] |
| Registry asset tree as plain user/asset tree note | Asset tree has migrated to `IndexedMerkleTree`; comments note user tree still uses `QuadTree` | Present in current source | Registry internals are actively specialized and risky to change broadly. [VERIFIED: `registry.rs`; `component/state.rs`] |
| Direct proto/wire types deep in logic | Domain structs and `DomainType` conversions at boundaries | Current convention | Keep new audit records as domain structs, not generated protobufs. [VERIFIED: `.planning/codebase/ARCHITECTURE.md`; `CONVENTIONS.md`] |

**Deprecated/outdated:**
- Compatibility aliases for moved internal modules: forbidden by project instructions. [VERIFIED: AGENTS.md]
- Scanner-name mimicry as architecture: explicitly forbidden by phase decisions. [VERIFIED: CONTEXT D-08]
- Deferred security TODOs as part of this refactor: out of scope. [VERIFIED: `.planning/REQUIREMENTS.md`; CONTEXT D-09]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | New single-implementation provider traits are usually not useful for this phase. | Common Pitfalls | Planner might reject all traits categorically, even if an actual external effect appears during implementation. |

## Open Questions

1. **Should the helper module be named `audit_records.rs`, `audit_projection.rs`, or something else?**
   - What we know: Existing names are short, factual, snake_case, and avoid redundant module names. [VERIFIED: CONVENTIONS.md]
   - What's unclear: The exact extracted helper set is implementation-dependent. [VERIFIED: `audit.rs`]
   - Recommendation: Let the implementer choose after extraction; prefer `audit_records.rs` if the module owns DTO construction, or `audit_flow.rs` if it owns status transitions. [ASSUMED]

2. **Should public audit function signatures change?**
   - What we know: Binaries import existing public functions from the compliance crate. [VERIFIED: caller grep]
   - What's unclear: A signature change may be justified if a helper returns typed intents that simplify callers. [ASSUMED]
   - Recommendation: Preserve public signatures unless compile-driven simplification is clearly smaller than adapter churn. [VERIFIED: AGENTS.md; caller grep]

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Rust `cargo` | Build/test compliance crate | yes | 1.89.0 | None needed. [VERIFIED: terminal] |
| Rust `rustc` | Build/test compliance crate | yes | 1.89.0 | None needed. [VERIFIED: terminal] |
| `just` | Local project checks | yes | 1.43.1 | Run underlying cargo commands directly. [VERIFIED: terminal; `justfile`] |
| `cargo-nextest` | Full CI-style test runner | no | — | `just test` falls back to `cargo test --release --no-fail-fast`; focused `cargo test` works. [VERIFIED: terminal; `justfile`] |

**Missing dependencies with no fallback:** None found. [VERIFIED: terminal; `justfile`]

**Missing dependencies with fallback:**
- `cargo-nextest` is not available in the shell; use focused `cargo test` commands and document if full nextest was not run. [VERIFIED: terminal; `justfile`]

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|------------------|
| V2 Authentication | no for selected audit/export refactor | Do not add registration authorization in this phase; it is deferred. [VERIFIED: `.planning/REQUIREMENTS.md`] |
| V3 Session Management | no | No session-bearing component in selected scope. [VERIFIED: `audit.rs`] |
| V4 Access Control | no for selected refactor | Do not implement asset/user registration authority changes. [VERIFIED: `.planning/REQUIREMENTS.md`; CONTEXT D-09] |
| V5 Input Validation | yes | Preserve `validate_audit_evidence`, Orbis import row eligibility checks, and decode/hash checks. [VERIFIED: `audit_validation.rs`; `audit.rs`] |
| V6 Cryptography | yes, but no new crypto | Reuse existing evidence/ciphertext validation; do not hand-roll crypto or DLEQ checks. [VERIFIED: `audit_validation.rs`; `crypto.rs`] |

### Known Threat Patterns for Compliance Audit Refactor

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Evidence row mismatch accepted as valid | Tampering | Keep persisted ciphertext/upload bundle/detection match checks before marking evidence valid. [VERIFIED: `audit.rs`] |
| Invalid Orbis import completes a row | Tampering | Preserve import eligibility check requiring evidence-valid unflagged detection. [VERIFIED: `audit.rs`] |
| Attacker-controlled failure growth | Denial of Service | Keep bounded/purposeful failure persistence and do not expand failure tables or unbounded blobs. [VERIFIED: AGENTS.md; `audit.rs`] |
| Deferred registration authorization confused with refactor | Elevation of Privilege | Keep registration authorization TODOs out of scope and document as deferred. [VERIFIED: `.planning/REQUIREMENTS.md`; `component/state.rs`] |

## Sources

### Primary (HIGH confidence)

- `AGENTS.md` - project engineering, architecture, verification, and style constraints.
- `.planning/phases/01-compliance-boundary-mvp/01-CONTEXT.md` - locked phase decisions and deferred ideas.
- `.planning/REQUIREMENTS.md` - v1 requirements, v2 deferred security/storage requirements, out-of-scope boundaries.
- `.planning/ROADMAP.md` - phase goal and success criteria.
- `.planning/codebase/ARCHITECTURE.md` - workspace architecture and component boundary patterns.
- `.planning/codebase/CONCERNS.md` - candidate concerns, deferred security/storage risks, test gaps.
- `.planning/codebase/CONVENTIONS.md` - Rust naming, testing, formatting, and boundary conventions.
- `crates/core/component/compliance/src/audit.rs` - selected boundary source and tests.
- `crates/core/component/compliance/src/registry.rs` - rejected boundary source and tests.
- `crates/core/component/compliance/src/component/state.rs` - registry action handlers and deferred TODOs.
- `crates/core/component/compliance/src/scanner/types.rs` - typed scanner/audit records and status constants.
- `crates/core/component/compliance/src/scanner/storage.rs` - SQLite durable scanner/audit store edge.
- `crates/core/component/compliance/src/audit_validation.rs` - pure validation/status example.
- `crates/core/component/compliance/src/lib.rs` - public compliance facade exports.
- `justfile`, `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml` - local verification and toolchain conventions.

### Secondary (MEDIUM confidence)

- None. Research was grounded in project-local source and planning artifacts. [VERIFIED: codebase grep]

### Tertiary (LOW confidence)

- None beyond assumptions logged above. [VERIFIED: assumptions log]

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - existing workspace dependencies and local tool versions were inspected. [VERIFIED: terminal; `Cargo.toml`]
- Architecture: HIGH - recommendation follows current source boundaries, codebase maps, and locked phase decisions. [VERIFIED: source files; planning artifacts]
- Pitfalls: MEDIUM - most are directly visible in source; trait-overuse risk includes one assumption about implementation tendency. [VERIFIED: source files; ASSUMED A1]

**Research date:** 2026-05-13  
**Valid until:** 2026-06-12
