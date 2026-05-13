---
phase: 01-compliance-boundary-mvp
plan: 01
subsystem: compliance
tags: [rust, compliance, audit, records, orbis]

requires: []
provides:
  - audit_records owns audit/export DTOs and pure Orbis import classification
  - audit facade imports moved DTOs without compatibility aliases
  - crate root re-exports moved audit DTOs behind the component feature
affects: [01-compliance-boundary-mvp, audit-export-boundary]

tech-stack:
  added: []
  patterns: [typed DTO ownership, pure classification, facade re-export]

key-files:
  created:
    - crates/core/component/compliance/src/audit_records.rs
    - crates/core/component/compliance/tests/audit_public_exports.rs
  modified:
    - crates/core/component/compliance/src/audit.rs
    - crates/core/component/compliance/src/lib.rs

key-decisions:
  - "Selected audit/export for the MVP boundary because it mixed SQLite effects, DTO construction, import eligibility, and failure recording."
  - "Rejected registry/state for this plan because its highest-risk work is deferred storage/security scope and it already uses component state traits."
  - "Moved DTO ownership unconditionally into audit_records.rs and re-exported names from the crate root without aliases."

patterns-established:
  - "Pure audit record helpers classify and project row data without SQLite dependencies."
  - "Effectful audit.rs imports DTOs and helper results from audit_records.rs while keeping existing facade functions stable."

requirements-completed: [EVID-01, EVID-02, EVID-03, ARCH-01, ARCH-02, ARCH-03, IMPL-01, IMPL-02, IMPL-03, IMPL-04, VERI-01]

duration: 20min
completed: 2026-05-13
---

# Phase 01 Plan 01: Audit Records Boundary Summary

**Audit/export DTOs and Orbis import eligibility now live in a pure typed record module with focused behavior tests.**

## Performance

- **Duration:** 20 min
- **Started:** 2026-05-13T12:38:00Z
- **Completed:** 2026-05-13T12:58:13Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Created `audit_records.rs` with `AuditDetectedRef`, `AuditScanExport`, `OrbisAuditEntry`, `AuditImportRow`, `OrbisImportEligibility`, `DetectedRefRowParts`, `classify_orbis_import_row`, and `detected_ref_from_row_parts`.
- Moved audit DTO ownership out of `audit.rs`; the facade now imports moved DTOs and uses pure projection/classification helpers while keeping SQLite effects in `audit.rs`.
- Added focused tests for eligible, flagged, invalid-status, missing-row, and detected-ref projection behavior, plus an integration test for crate-root DTO exports.

## Task Commits

1. **Task 1: Create audit_records DTOs and pure API** - `837206af6` (feat)
2. **Task 2: Re-export moved DTOs without compatibility aliases** - `19e6dca31` (test)

**Plan metadata:** committed separately with this summary.

## Files Created/Modified

- `crates/core/component/compliance/src/audit_records.rs` - Owns audit/export DTOs and pure Orbis import classification/projection helpers.
- `crates/core/component/compliance/src/audit.rs` - Imports moved DTOs and routes row projection/import eligibility through the pure module.
- `crates/core/component/compliance/src/lib.rs` - Declares `audit_records` under `component` and re-exports moved DTO names.
- `crates/core/component/compliance/tests/audit_public_exports.rs` - Verifies moved DTO names remain importable from `penumbra_sdk_compliance`.

## Decisions Made

- Audit/export is the selected boundary for this MVP because `audit.rs` mixed durable SQLite effects with DTO construction and import eligibility checks.
- Registry/state remains out of this plan because its meaningful risk is deferred security/storage work, and a small extraction there would be more likely cosmetic.
- No provider traits, compatibility aliases, scanner-name mirror modules, or SQLite access were added to the pure record module.

## Verification

- `cargo test -p penumbra-sdk-compliance --features component audit_records -- --nocapture` - passed.
- `cargo check -p penumbra-sdk-compliance --features component --tests` - passed.
- `cargo test -p penumbra-sdk-compliance --features component --test audit_public_exports -- --nocapture` - passed.
- `rg -n "lock_conn\\(|unchecked_transaction\\(|SELECT |INSERT |UPDATE |StateRead|StateWrite|rusqlite" crates/core/component/compliance/src/audit_records.rs` - no matches.
- `rg -n "registration authorization|allowed_channels|AuditStore|RegistryStore|pub use .* as|compat|alias" crates/core/component/compliance/src/audit_records.rs crates/core/component/compliance/src/audit.rs crates/core/component/compliance/src/lib.rs` - only pre-existing address alias symbols and SQL in `audit.rs`; no new provider, compatibility, or deferred-security surface in `audit_records.rs`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Declared the new module during Task 1**
- **Found during:** Task 1
- **Issue:** Rust would not compile or run `audit_records` tests unless the module was declared from `lib.rs`, which Task 2 originally owned.
- **Fix:** Added the feature-gated `pub mod audit_records` declaration while creating the module, then completed DTO re-export coverage in Task 2.
- **Files modified:** `crates/core/component/compliance/src/lib.rs`
- **Verification:** `cargo test -p penumbra-sdk-compliance --features component audit_records -- --nocapture`
- **Committed in:** `837206af6`

---

**Total deviations:** 1 auto-fixed (Rule 3).
**Impact on plan:** Kept scope unchanged; the declaration was required for the planned tests to execute.

## Issues Encountered

None beyond the module declaration ordering noted above.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None.

## Threat Flags

None.

## Self-Check: PASSED

- Created files exist: `audit_records.rs`, `audit_public_exports.rs`.
- Task commits found: `837206af6`, `19e6dca31`.
- No tracked file deletions were introduced by task commits.

## Next Phase Readiness

Plan 02 can now rewire more of the audit facade through `audit_records.rs` while preserving the crate-root DTO surface established here.

---
*Phase: 01-compliance-boundary-mvp*
*Completed: 2026-05-13*
