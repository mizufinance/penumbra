# Penumbra Compliance Boundary Refactor

## What This Is

This project applies the scanner refactor's modular, clear-boundary architecture to the rest of the Penumbra compliance component where the evidence supports it. The goal is a one-phase implementation refactor that improves ownership boundaries in compliance code without expanding the feature surface or introducing abstraction for its own sake.

The work starts from the existing scanner split in `crates/core/component/compliance/src/scanner/` and evaluates nearby compliance areas such as registry/state and audit/export for the highest-payoff boundary cleanup.

## Core Value

Compliance code should be easier to reason about because durable state, pure validation, domain records, and external effects have clear ownership boundaries without added complexity.

## Requirements

### Validated

- ✓ Penumbra has a componentized Rust architecture with protocol modules under `crates/core/component/*` — existing.
- ✓ Compliance owns regulated asset/user registry state, audit evidence, Orbis interoperability, scanner storage, and compliance transaction logic under `crates/core/component/compliance/src/` — existing.
- ✓ The scanner now has clearer internal boundaries through `scanner/types.rs`, `scanner/screener.rs`, `scanner/storage.rs`, `scanner/sync.rs`, `scanner/worker.rs`, and `scanner/advice.rs` — existing.
- ✓ Compliance scanner persistence is behind `ScannerStore` with a concrete `SqliteScannerStore` edge — existing.
- ✓ Compliance has tests around scanner storage, registration behavior, evidence validation, and recent scanner/audit flows — existing.

### Active

- [ ] Identify the one or two compliance areas that benefit most from scanner-style boundary cleanup based on code evidence, not preference.
- [ ] Refactor the selected compliance area(s) so domain types, validation/projection logic, durable state access, and external effects sit behind clearer module boundaries.
- [ ] Avoid new compatibility shims, redundant aliases, speculative provider traits, or broad rewrites that do not reduce coupling.
- [ ] Preserve existing public behavior unless a failing test exposes an existing bug that must be fixed as part of the boundary cleanup.
- [ ] Add or adjust focused tests proving the refactor preserves behavior and improves the selected boundary.

### Out of Scope

- Re-architecting the entire compliance component — this is a one-phase feature, not a broad rewrite.
- Changing compliance protocol semantics — the target is structure and maintainability unless an existing behavior is proven wrong by tests.
- Implementing unrelated security fixes such as registration authorization or IBC policy enforcement — those may be future phases if surfaced during analysis.
- Refactoring non-compliance components — scanner architecture is the reference point, but this project stays inside compliance unless a direct caller needs a small mechanical update.
- Adding abstraction layers only to mirror scanner names — boundaries must remove real coupling or clarify ownership.

## Context

The last commit, `64827a886 Scanner core (#74)`, moved scanner code toward a modular shape with typed records, a store trait, a screener, sync extraction, worker orchestration, and advice providers. That architecture is a useful reference because it separates pure detection/classification from persistence and external service access.

The current compliance module still has several larger mixed-responsibility areas:

- `crates/core/component/compliance/src/registry.rs` combines tree persistence, policy lookup, proof data, anchor recording, count/index helpers, and registry read/write extension traits.
- `crates/core/component/compliance/src/component/state.rs` combines component lifecycle hooks, genesis registration, event recording, and action execution for `MsgRegisterUser` and `MsgRegisterAsset`.
- `crates/core/component/compliance/src/audit.rs` combines audit row updates, export/import JSON shapes, Orbis pending scan export, decryption, evidence failure recording, scanner health, and direct SQLite access through `SqliteScannerStore`.
- `crates/core/component/compliance/src/indexed_tree.rs` and `tree.rs` are large core data structures that may be better left alone unless they are directly involved in the selected boundary.

The codebase map also flagged compliance registry/scanner as fragile because it combines on-chain registry state, custom Merkle/IMT implementations, compact-block events, RPC proofs, and scanner persistence. The likely candidates are therefore registry/state or audit/export, but the implementation phase should decide from evidence after reading call sites and test coverage.

## Constraints

- **Scope**: Complete in one phase — choose the highest-payoff one or two refactors rather than attempting a full compliance redesign.
- **Simplicity**: Do not add complexity — introduce a boundary only when it reduces coupling, removes mixed responsibilities, or makes tests more direct.
- **Architecture**: Follow existing Penumbra patterns — durable state goes through typed replayable records and state-key helpers; pure validation/projection stays separate from effects.
- **Compatibility**: This is a new product branch, not a backwards-compatibility exercise — remove obsolete paths rather than preserve aliases if a path is replaced.
- **Verification**: Prove behavior with focused tests before marking the phase complete.
- **Containment**: Stay primarily in `crates/core/component/compliance/src/`; update binaries or docs only when required by the refactor.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Implement, not just plan | The desired output is working refactor code with tests, not an architecture memo. | — Pending |
| Let evidence decide target area | Registry/state and audit/export both show coupling; the phase should pick based on highest payoff after reading call sites. | — Pending |
| Allow one or two refactors | A single boundary may not cover the best payoff, but broad rewrites are out of scope. | — Pending |
| Use scanner architecture as reference, not template | The scanner split is useful because it clarified ownership; copying names or layers mechanically would add complexity. | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `$gsd-transition`):
1. Requirements invalidated? -> Move to Out of Scope with reason
2. Requirements validated? -> Move to Validated with phase reference
3. New requirements emerged? -> Add to Active
4. Decisions to log? -> Add to Key Decisions
5. "What This Is" still accurate? -> Update if drifted

**After each milestone** (via `$gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-12 after initialization*
