# Phase 1: Compliance Boundary MVP - Context

**Gathered:** 2026-05-12
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase delivers one implementation refactor inside `crates/core/component/compliance/src/`. It must evaluate both major candidate areas, select the highest-payoff boundary cleanup from source evidence, implement one or two refactors if justified, and prove behavior with tests and CI.

The phase is not a design memo. It should produce code changes that make compliance ownership boundaries clearer without expanding the feature surface or adding abstraction for its own sake.

</domain>

<decisions>
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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning State
- `.planning/PROJECT.md` — Project scope, core value, constraints, and deferred areas.
- `.planning/REQUIREMENTS.md` — v1 requirements, v2 deferred requirements, and out-of-scope boundaries.
- `.planning/ROADMAP.md` — Phase 1 goal, MVP mode, success criteria, and requirement coverage.
- `.planning/STATE.md` — Current project state and deferred items.

### Codebase Maps
- `.planning/codebase/ARCHITECTURE.md` — Existing architecture, component boundaries, state-machine patterns, and anti-patterns.
- `.planning/codebase/CONCERNS.md` — Compliance registry/scanner fragility, candidate concerns, deferred security issues, and verification gaps.
- `.planning/codebase/CONVENTIONS.md` — Rust naming, module, error-handling, testing, and boundary conventions.

### Candidate Source Areas
- `crates/core/component/compliance/src/registry.rs` — Registry read/write traits, tree persistence, policy lookup, anchors, counts, and proof data.
- `crates/core/component/compliance/src/component/state.rs` — Compliance component lifecycle, genesis registration, and action handlers for `MsgRegisterUser` and `MsgRegisterAsset`.
- `crates/core/component/compliance/src/audit.rs` — Audit row updates, export/import, decryption, evidence failure recording, scanner health, and direct SQLite access.
- `crates/core/component/compliance/src/evidence.rs` — Evidence object domain construction and parsing.
- `crates/core/component/compliance/src/audit_validation.rs` — Existing example of focused validation returning typed status.
- `crates/core/component/compliance/src/scanner/mod.rs` — Scanner module facade and reference boundary shape.
- `crates/core/component/compliance/src/scanner/types.rs` — Scanner typed records reference.
- `crates/core/component/compliance/src/scanner/screener.rs` — Scanner pure classification reference.
- `crates/core/component/compliance/src/scanner/storage.rs` — Scanner store edge reference.
- `crates/core/component/compliance/src/scanner/sync.rs` — Scanner extraction/projection reference.
- `crates/core/component/compliance/src/scanner/worker.rs` — Scanner orchestration reference.
- `crates/core/component/compliance/src/scanner/advice.rs` — External advice provider reference.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scanner/types.rs`: Shows how scanner facts are carried as typed records instead of ad hoc tuples.
- `scanner/screener.rs`: Provides a compact example of pure screening logic separate from storage.
- `scanner/storage.rs`: Shows a concrete store edge around SQLite persistence.
- `audit_validation.rs`: Provides an existing typed validation-status pattern in compliance.
- `state_key.rs`: Existing place for durable compliance state key helpers.

### Established Patterns
- Component state effects should stay under feature-gated component modules and use `StateRead` / `StateWrite`.
- Durable state keys should come from component-owned `state_key.rs` functions, not ad hoc strings.
- Proto types should be converted at boundaries; domain structs should carry validated data through core logic.
- Tests should be behavior-oriented and named for the invariant or workflow they protect.

### Integration Points
- Registry/state candidate: `registry.rs`, `component/state.rs`, `component/rpc.rs`, tests inside `component/state.rs`, and any callers of `ComplianceRegistryRead` / `ComplianceRegistryWrite`.
- Audit/export candidate: `audit.rs`, `scanner/storage.rs`, `crates/bin/orbis-audit/src/main.rs`, `crates/bin/orbis-integration/src/main.rs`, and `crates/bin/pcli/src/command/tx/compliance.rs`.
- Verification should include compliance crate tests and broader checks from `justfile` when feasible.

</code_context>

<specifics>
## Specific Ideas

The desired outcome is “simple feature, done in one phase.” The refactor should apply the scanner’s clear-boundary architecture to the rest of compliance where it helps, while avoiding complexity. The user explicitly wants the boundary that benefits most, not necessarily the smallest boundary.

Recommended boundary-shape preference captured from discussion:
1. Typed records.
2. Pure validation/projection/classification helpers.
3. State access kept at the edge through existing state traits.
4. Provider traits only for real external effects.

</specifics>

<deferred>
## Deferred Ideas

- Registration authorization for `MsgRegisterUser` and `MsgRegisterAsset` remains deferred.
- Regulated asset `allowed_channels` enforcement remains deferred.
- Registry storage scaling changes such as node/index persistence remain deferred unless directly necessary for the selected refactor.
- Non-compliance refactors remain deferred.

</deferred>

---

*Phase: 1-Compliance Boundary MVP*
*Context gathered: 2026-05-12*
