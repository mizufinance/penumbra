---
phase: 01-compliance-boundary-mvp
verified: 2026-05-13T13:16:33Z
status: passed
score: 5/5 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 5/6
  gaps_closed:
    - "MVP-mode phase goal is a valid User Story so the required User Flow Coverage verification can be applied."
  gaps_remaining: []
  regressions: []
---

# Phase 1: Compliance Boundary MVP Verification Report

**Phase Goal:** Compliance code has clearer ownership boundaries in the highest-payoff selected area while preserving existing behavior.  
**Verified:** 2026-05-13T13:16:33Z  
**Status:** passed  
**Re-verification:** Yes - after roadmap mode removal in commit `3bfd66fca`

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | The implementation records source-backed evidence for the selected compliance boundary and at least one rejected alternative. | VERIFIED | `audit_records.rs:1-7` records audit/export selection and registry/state rejection; `01-RESEARCH.md` compares audit/export with registry/state using source evidence. |
| 2 | The selected compliance flow separates durable state access from pure validation, projection, or domain-record construction through narrow Penumbra-style APIs. | VERIFIED | `audit_records.rs:16-103` owns DTOs and pure helper decisions; `audit.rs:194-340` keeps SQLite connection, transaction, SQL, status updates, and failure recording in the effectful facade. |
| 3 | Obsolete internal paths from the refactor are removed, with no compatibility aliases, speculative provider traits, or scanner-name mimicry added. | VERIFIED | DTO definitions are owned by `audit_records.rs`; `audit.rs:8-12` and `lib.rs:100-108` re-export/use those names directly. Anti-pattern grep found only pre-existing/domain address-alias names and `GenesisContent` rename, not a new compatibility/provider surface. |
| 4 | Focused tests demonstrate preserved behavior for the selected boundary and make the refactored flow easier to exercise directly. | VERIFIED | `audit_records.rs:121-180` tests eligible statuses, invalid/flagged/missing rows, and detected-ref projection without SQLite. `audit.rs` audit facade tests and `audit_public_exports.rs:7-23` verify preserved behavior/export paths. |
| 5 | Relevant compliance tests, formatting, and the narrowest useful compile/test checks pass, with any intentionally unrun broad checks documented. | VERIFIED | See Behavioral Spot-Checks: focused tests, full compliance crate tests, formatting, and all-target/all-feature compliance check passed in this verification run. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/core/component/compliance/src/audit_records.rs` | Typed audit/export/import DTOs and pure audit record helpers | VERIFIED | Exists, substantive, SDK artifact check passed; owns `AuditDetectedRef`, `AuditScanExport`, `OrbisAuditEntry`, `AuditImportRow`, `OrbisImportEligibility`, `DetectedRefRowParts`, `classify_orbis_import_row`, and `detected_ref_from_row_parts`. |
| `crates/core/component/compliance/src/audit.rs` | Effectful audit facade and SQLite orchestration | VERIFIED | Exists, substantive, SDK artifact check passed; imports helpers and retains `SqliteScannerStore::lock_conn`, SQL, transactions, failure recording, and facade functions. |
| `crates/core/component/compliance/src/lib.rs` | Stable feature-gated public audit exports | VERIFIED | Exists, substantive, SDK artifact check passed; declares `pub mod audit_records` and re-exports DTO names under the component feature. |
| `crates/core/component/compliance/tests/audit_public_exports.rs` | Public export regression tests | VERIFIED | Exists and passes; verifies DTOs remain importable from crate root and `penumbra_sdk_compliance::audit`. |
| `.planning/STATE.md` | GSD phase progress | VERIFIED | SDK artifact check passed for Plan 02 state progress artifact. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `audit.rs` | `audit_records.rs` | DTO imports and pure helper calls | VERIFIED | SDK key-link check passed for Plan 01 and Plan 02 helper wiring; direct evidence at `audit.rs:8-12`, `audit.rs:214`, `audit.rs:262`, and `audit.rs:615`. |
| `audit.rs` | `scanner/storage.rs` | Existing `SqliteScannerStore::lock_conn` calls stay in effectful facade | VERIFIED | Manual grep verifies `audit.rs` retains `lock_conn()` calls while `audit_records.rs` has no `lock_conn`, transactions, SQL, `StateRead`, `StateWrite`, or `rusqlite` matches. The SDK key-link check could not parse the escaped regex pattern, but the code evidence is direct. |
| `lib.rs` | Public audit callers | Unchanged DTO and function names | VERIFIED | SDK key-link check passed; `lib.rs:100-112` exposes `audit_records`, DTOs, `audit`, and existing facade functions. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `audit_records.rs` | `AuditImportRow` | Built in `audit.rs:244-261` from `scanner_detections.audit_status` and `scanner_detections.is_flagged` | Yes | FLOWING |
| `audit_records.rs` | `DetectedRefRowParts` | Built in `audit.rs:207-222` and `audit.rs:607-623` from SQL rows | Yes | FLOWING |
| `audit.rs` | Orbis import failure reason | `classify_orbis_import_row` result at `audit.rs:262-275` | Yes | FLOWING |
| `lib.rs` / `audit.rs` | Public DTO names | Re-export of concrete `audit_records` types | Yes | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Roadmap mode removal closes previous MVP verifier blocker | `gsd-sdk query roadmap.get-phase 1 --raw` plus `git show 3bfd66fca -- .planning/ROADMAP.md` | Phase mode is `null`; commit removes `**Mode:** mvp` | PASS |
| Pure audit record classification/projection tests pass | `cargo test -p penumbra-sdk-compliance --features component audit_records -- --nocapture` | 4 passed | PASS |
| Public DTO export regression passes | `cargo test -p penumbra-sdk-compliance --features component --test audit_public_exports` | 2 passed | PASS |
| Audit facade behavior tests pass | `cargo test -p penumbra-sdk-compliance --features component -- audit --nocapture` | 20 unit tests and 2 integration tests passed; doc tests passed | PASS |
| Relevant compliance crate tests pass | `cargo test -p penumbra-sdk-compliance --features component` | 203 unit tests and 2 integration tests passed; doc tests passed | PASS |
| Formatting passes | `cargo fmt --all -- --check` | exit 0 | PASS |
| Narrow compile check passes | `cargo check -p penumbra-sdk-compliance --all-targets --all-features` | exit 0 | PASS |

### Probe Execution

| Probe | Command | Result | Status |
|-------|---------|--------|--------|
| Conventional probes | `find scripts -path '*/tests/probe-*.sh' -type f` plus plan/summary grep | No probes found or declared | SKIPPED |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| EVID-01 | 01-01 | Identify highest boundary-cleanup payoff using compliance source evidence. | SATISFIED | `01-RESEARCH.md` evaluates audit/export and registry/state; `audit_records.rs:1-7` records selected/rejected boundary. |
| EVID-02 | 01-01 | Justify selected target against at least one rejected alternative. | SATISFIED | Registry/state rejection is documented in `audit_records.rs:3-7` and research candidate tables. |
| EVID-03 | 01-01 | Limit scope to one or two compliance boundaries. | SATISFIED | Product changes are limited to the audit/export boundary and public export tests. |
| ARCH-01 | 01-01, 01-02 | Separate durable state access from pure validation/projection/domain records. | SATISFIED | `audit_records.rs` has pure DTO/helper logic and no SQLite/state-effect matches; `audit.rs` owns durable effects. |
| ARCH-02 | 01-01, 01-02 | Narrow purposeful APIs matching Penumbra patterns. | SATISFIED | API is structs/enums/functions; no new broad provider trait or scanner-name copy was introduced. |
| ARCH-03 | 01-01, 01-02 | Reduce mixed-responsibility code without shims or speculative traits. | SATISFIED | DTO construction and eligibility decisions moved out of `audit.rs`; grep found no `AuditStore`, `RegistryStore`, compatibility shim, or new provider trait in scoped files. |
| ARCH-04 | 01-02 | Preserve public behavior unless a focused failing test proves a bug. | SATISFIED | Existing facade functions remain exported; audit and public-export tests pass. |
| IMPL-01 | 01-01, 01-02 | Product code changes primarily in compliance source. | SATISFIED | Product changes are in `crates/core/component/compliance/src/`; test added under compliance crate tests. |
| IMPL-02 | 01-01, 01-02 | Use scanner architecture as reference, not naming template. | SATISFIED | New module is `audit_records`, and no scanner-name mirror/provider was added. |
| IMPL-03 | 01-01, 01-02 | Remove obsolete internal paths rather than aliases. | SATISFIED | DTO definitions are not duplicated in `audit.rs`; public names are direct re-exports, not compatibility aliases. |
| IMPL-04 | 01-01, 01-02 | Make selected flow easier to test directly. | SATISFIED | `audit_records` tests exercise classification/projection without SQLite setup. |
| VERI-01 | 01-01, 01-02 | Focused tests cover selected boundary before/after. | SATISFIED | Pure helper tests plus audit facade tests cover the selected flow. |
| VERI-02 | 01-02 | Relevant compliance crate tests pass. | SATISFIED | `cargo test -p penumbra-sdk-compliance --features component` passed. |
| VERI-03 | 01-02 | Formatting and narrow compile/test checks pass, with unrun broad checks documented. | SATISFIED | Formatting and all-target/all-feature compliance check passed; no required broad check was left unrun in this verification pass. |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/core/component/compliance/src/lib.rs` | 71 | `pub use genesis::Content as GenesisContent` | INFO | Pre-existing public rename outside the audit/export refactor; not a compatibility alias introduced by this phase. |
| `crates/core/component/compliance/src/audit.rs` | multiple | `alias` in address-alias feature names | INFO | Existing domain feature for address aliases, not a refactor compatibility shim. |

### Human Verification Required

None. The phase is a backend Rust refactor with programmatically checkable code, wiring, tests, formatting, and compile behavior.

### Gaps Summary

No blocking gaps remain. The previous verification gap is resolved: Phase 1 no longer has `mode: mvp` in `ROADMAP.md`, `gsd-sdk query roadmap.get-phase 1 --raw` reports `"mode": null`, and commit `3bfd66fca` removed the MVP mode line. With MVP-mode verification dormant, the roadmap success criteria and plan must-haves are verified against current code.

---

_Verified: 2026-05-13T13:16:33Z_  
_Verifier: the agent (gsd-verifier)_
