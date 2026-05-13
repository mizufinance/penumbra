---
phase: 01
reviewers: [gemini, claude, coderabbit, opencode]
reviewed_at: 2026-05-13T01:23:20.532Z
plans_reviewed: [01-01-PLAN.md, 01-02-PLAN.md]
---

# Cross-AI Plan Review — Phase 01

## Gemini Review

Gemini review failed or returned empty output.

---

## Claude Review

# Plan Review: Phase 01 — Compliance Boundary MVP

## 01-01-PLAN.md

### Summary
A well-scoped plan that creates `audit_records.rs` with pure typed helpers and tests, establishing the evidence gate and pure boundary before the facade is rewired. The TDD approach and grep-based acceptance criteria are appropriate for the constraints.

### Strengths
- TDD ordering: tests define expected behavior before implementation
- Evidence documentation baked into Task 1 as a module-level note — satisfies EVID requirements without a separate artifact
- Grep-based negative acceptance criteria prevent scope creep concretely
- No SQLite dependency in the pure module is enforced structurally

### Concerns
- **MEDIUM**: Task 1 and Task 2 have overlapping scope. Task 1 creates helpers and tests; Task 2 moves/defines DTOs and adds more helpers. The boundary between "minimal pure helpers" (T1) and "typed audit records and exports" (T2) is fuzzy — the implementer may do most of T2's work in T1 or duplicate effort.
- **LOW**: The plan says "Move or define the public DTOs `AuditDetectedRef`, `AuditScanExport`, and `OrbisAuditEntry` in `audit_records.rs` if that is the smallest stable shape; otherwise keep the DTOs in `audit.rs`." This conditional makes the plan's `must_haves` artifact assertion (`contains: "AuditDetectedRef"` in `audit_records.rs`) potentially wrong if the implementer decides DTOs stay in `audit.rs`.
- **LOW**: Task 2 doesn't update `audit.rs` imports to consume helpers — it says it does, but the verify command only runs `audit_records` tests, not full `audit::` tests, so broken imports in `audit.rs` wouldn't be caught until Plan 02.

### Suggestions
- Merge Task 1 and Task 2 into a single task. The separation adds ceremony without real value — both modify the same files and the "minimal" vs "full" distinction is artificial.
- If keeping two tasks, make Task 2's verify also run `cargo test -p penumbra-sdk-compliance --features component` to catch import breakage early.
- Resolve the conditional DTO placement before execution: pick one approach in the plan rather than deferring to runtime judgment, since the `must_haves` already assume one outcome.

### Risk Assessment
**LOW** — The plan is conservative and well-constrained. The task overlap is inefficient but not dangerous.

---

## 01-02-PLAN.md

### Summary
Completes the refactor by wiring the facade through pure helpers, removing obsolete paths, and running verification. Structurally sound with appropriate verification depth and threat awareness.

### Strengths
- Explicit grep gates for no-provider-trait and no-SQL-in-pure-module constraints
- Verification task is a separate task with named commands — makes the proof bar concrete
- Threat model correctly identifies import eligibility bypass as the main tampering risk
- `depends_on: [01-01]` correctly sequences the wave

### Concerns
- **MEDIUM**: Task 1's scope is vague on *which* private helpers get removed. The plan says "Remove moved private helper definitions instead of keeping aliases" but doesn't enumerate them. The implementer must read `audit.rs` (1,166 lines) and decide in real time which functions are "moved" vs "kept." A short list of candidate functions (e.g., the inline DTO constructors, eligibility branching) would reduce implementation risk.
- **MEDIUM**: The verify command `cargo test -p penumbra-sdk-compliance --features component audit:: -- --nocapture` uses `audit::` as a filter, which matches test module paths containing `audit::`. If Plan 01 moved tests into `audit_records::tests`, those won't be caught by this filter. The Task 2 verification compensates, but Task 1 could pass with broken `audit_records` tests.
- **LOW**: The plan references `01-01-SUMMARY.md` in `read_first` but that artifact doesn't exist yet at planning time. If Plan 01 deviates from expectations, Plan 02's assumptions about what's in `audit_records.rs` could be wrong.

### Suggestions
- In Task 1, list 3-5 specific private functions or inline patterns in `audit.rs` that should be replaced by `audit_records` calls. This makes the "remove obsolete paths" requirement concrete and reviewable.
- Add `audit_records` to Task 1's verify filter: `cargo test -p penumbra-sdk-compliance --features component -- audit --nocapture` to catch both modules.
- Consider whether `cargo clippy -p penumbra-sdk-compliance --all-features` should be part of Task 2 verification — it catches dead code from removed helpers.

### Risk Assessment
**LOW** — The plan achieves the phase goals. The main risk is implementation ambiguity around which helpers to extract, but the grep gates and existing test coverage provide good safety nets.

---

## Cross-Plan Assessment

**Overall phase risk: LOW.** The two plans together form a clean extract-then-wire sequence. The phase goals (EVID, ARCH, IMPL, VERI requirements) are well-covered.

**One structural suggestion:** The two plans could reasonably be one plan with three tasks (create pure module → wire facade → verify). The wave separation adds session-resumability but also adds overhead (summary artifacts, state updates, re-reading). For a refactor this contained (~3 files, one module extraction), a single plan would be simpler and still satisfy all requirements.

---

## CodeRabbit Review

╔═════════════════════════════════════════════╗
║                                             ║
║           New update available!             ║
║          Run: coderabbit update             ║
║                                             ║
╚═════════════════════════════════════════════╝

Starting CodeRabbit review in plain text mode...

Connecting to review service
Setting up
Analyzing
Reviewing

============================================================================
File: .planning/phases/01-compliance-boundary-mvp/01-DISCUSSION-LOG.md
Line: 66
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/phases/01-compliance-boundary-mvp/01-DISCUSSION-LOG.md at line 66, The heading "the agent's Discretion" has inconsistent capitalization; update that heading text to use title case to match other sections—replace it with "The Agent's Discretion" (or "Agent's Discretion" if you prefer dropping the leading article) so the file's heading style is consistent.



============================================================================
File: AGENTS.md
Line: 16
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @AGENTS.md at line 16, Remove the stray trailing double-quote at the end of the bullet "Don’t fight errors! Whenever you encounter the same error twice, research the web and find 3-5 possible ways to fix it. Then choose the most efficient solution and implement it." so it matches other list items; locate that exact bullet text in AGENTS.md and delete only the extra closing quote character.



============================================================================
File: .planning/config.json
Line: 40
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/config.json at line 40, Replace the unsafe "mode": "yolo" setting with a safer enforcement mode (e.g., "safe" or "strict") so verification and testing aren’t bypassed; update the "mode" value to one that enforces proof-before-complete and automated checks, and ensure it is consistent with the existing flags ("verifier", "plan_check", "ui_safety_gate") so those features are not contradictory—pick a mode that requires reproducing tests and running focused/full checks before marking work complete.



============================================================================
File: .planning/ROADMAP.md
Line: 30 to 42
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/ROADMAP.md around lines 30 - 42, The progress table's "Plans Complete" value is inconsistent with the stated "Plans: 2 plans" and the two listed plans ("01-01-PLAN.md", "01-02-PLAN.md"); update the table cell that currently reads "0/TBD" under the "Plans Complete" column to "0/2" so the progress table reflects the actual plan count.



============================================================================
File: .planning/phases/01-compliance-boundary-mvp/01-PATTERNS.md
Line: 9 to 14
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/phases/01-compliance-boundary-mvp/01-PATTERNS.md around lines 9 - 14, The test entry for "audit_records.rs tests or audit.rs tests" is ambiguous; update the table to either split into two rows (one for crates/core/component/compliance/src/audit_records.rs tests and one for crates/core/component/compliance/src/audit.rs tests) or explicitly state that tests are inline #[cfg(test)] modules within audit_records.rs and audit.rs (or that they live as separate files under tests/) so the "New/Modified File" column maps to a single, specific file for each row; ensure you reference audit_records.rs and audit.rs by name so the classification is unambiguous.



============================================================================
File: .planning/REQUIREMENTS.md
Line: 57
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/REQUIREMENTS.md at line 57, The document currently introduces "This project must complete as one phase" only at the Full compliance redesign row; move or duplicate a concise explanation of the "one phase" constraint into the document header or the "Core Value" statement so readers see the rationale early. Add a short sentence clarifying what "one phase" means (e.g., single-delivery lifecycle, no iterative redesigns) and why it drives excluding "Full compliance redesign" (time/resources/high-payoff boundary), referencing the existing phrase "Full compliance redesign" to link the rationale to that exclusion.



============================================================================
File: .planning/phases/01-compliance-boundary-mvp/01-01-PLAN.md
Line: 56 to 59
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/phases/01-compliance-boundary-mvp/01-01-PLAN.md around lines 56 - 59, The execution_context block contains user-specific absolute paths (/Users/antoinecyr/...) which break portability; change those entries to repository-relative paths using the @ prefix (e.g., replace "/Users/antoinecyr/Documents/Source/penumbra/.codex/get-shit-done/workflows/execute-plan.md" and the templates path with "@/ .codex/get-shit-done/workflows/execute-plan.md" and "@/ .codex/get-shit-done/templates/summary.md" respectively) so the execution_context uses repository-relative paths consistent with other references.



============================================================================
File: .planning/REQUIREMENTS.md
Line: 34
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/REQUIREMENTS.md at line 34, VERI-03 is ambiguous about where "any unrun broad checks" must be documented; update .planning/REQUIREMENTS.md to explicitly state the single canonical location (e.g., require authors to list unrun broad checks in .planning/STATE.md and also summarize them in the PR description) and include a short template/example showing the expected fields (check name, reason not run, and link to any logs); reference VERI-03 by name in the REQUIREMENTS.md change and mention the canonical doc (.planning/STATE.md) and PR description so verifiers know where to look.



============================================================================
File: .planning/REQUIREMENTS.md
Line: 12 to 14
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/REQUIREMENTS.md around lines 12 - 14, EVID-01 and EVID-03 use "area" and "boundary" inconsistently; update the REQUIREMENTS text to define both terms and state how they map (e.g., "area = functional domain; boundary = specific compliance module/namespace"), clarify whether an area may span multiple boundaries or a boundary may contain multiple areas, and modify EVID-01 and EVID-03 to explicitly require that the implementer note the chosen granularity and justify if the selected "area" spans more than one "boundary" (or vice versa) with a short example or mapping to guide scope selection.



============================================================================
File: .planning/STATE.md
Line: 29
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/STATE.md at line 29, Update the imprecise plan count string "Plan: 0 of TBD in current phase" to a concrete value (e.g., "Plan: 0 of 2 in current phase" or "Plan: 0 of 2+ in current phase") in .planning/STATE.md; also make the same replacement where the same pattern appears around the other occurrence (the comment notes it also applies at the other instance) so both lines show a concrete total instead of "TBD".



============================================================================
File: .planning/STATE.md
Line: 5
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/STATE.md at line 5, The frontmatter key "status: executing" and the body string "Status: Ready to execute" are inconsistent; pick the correct overall state and make both places match (e.g., change the frontmatter "status: executing" to "status: ready" if implementation hasn't started, or update the body line "Status: Ready to execute" to "Status: Executing" if work has begun), ensuring the terms exactly match the chosen keyword across the file (.planning/STATE.md) including the frontmatter "status" and the "Status:" line in the body.



============================================================================
File: .planning/PROJECT.md
Line: 5 to 7
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/PROJECT.md around lines 5 - 7, The plan currently locks to a "one-phase implementation refactor" while deferring "deferred target selection" to implementation, creating execution risk; update the PROJECT.md text to either (A) add a short pre-implementation analysis step that finalizes target selection (describe deliverable: chosen target area, size estimate, coupling risks, go/no-go criteria) or (B) relax the one-phase commitment and add an explicit decision gate that allows scope extension if evidence shows the target is too large; reference and change the phrases "one-phase implementation refactor" and "deferred target selection" and add acceptance criteria and a decision owner so reviewers can locate and validate the change.



============================================================================
File: .planning/phases/01-compliance-boundary-mvp/01-RESEARCH.md
Line: 10 to 11
Type: potential_issue

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/phases/01-compliance-boundary-mvp/01-RESEARCH.md around lines 10 - 11, The heading hierarchy is incorrect: change the "## Implementation Decisions" heading to "### Implementation Decisions" so it nests under "### Locked Decisions" (update the heading token for the line containing "Implementation Decisions" to use three hashes).



============================================================================
File: .planning/phases/01-compliance-boundary-mvp/01-02-PLAN.md
Line: 143
Type: refactor_suggestion

Prompt for AI Agent:
Verify each finding against current code. Fix only still-valid issues, skip the rest with a brief reason, keep changes minimal, and validate.

In @.planning/phases/01-compliance-boundary-mvp/01-02-PLAN.md at line 143, The action block currently mixes mandatory and conditional verification commands, causing ambiguity; update the action text to explicitly separate "Required" commands that must be run and pass for task completion (list the three cargo test commands and the two formatting/check commands: cargo test -p penumbra-sdk-compliance --features component audit_records -- --nocapture, cargo test -p penumbra-sdk-compliance --features component audit:: -- --nocapture, cargo test -p penumbra-sdk-compliance --features component, cargo fmt --all -- --check, and cargo check -p penumbra-sdk-compliance --all-targets --all-features) from "Optional / Environment-dependent" commands (e.g. just check or full local CI) with an instruction to document exact reasons if optional commands are not run; also add a clear completion criterion sentence stating that Phase 1 planning/execution progress in .planning/STATE.md should be updated after required commands pass, but the phase should not be marked complete until all plans are finished.



Review completed: 14 findings ✔

---

## OpenCode Review

Let me verify the plans against the actual source code before reviewing.
Now I have a thorough picture of the codebase. Let me review both plans.

---

# Cross-AI Plan Review: Phase 01 Compliance Boundary MVP

## Plan 01-01: Pure audit record boundary helpers

### Summary

Extracts a pure typed-record boundary from `audit.rs` into a new `audit_records.rs` module with focused tests, while documenting the evidence-based selection of audit/export over registry/state. Follows a TDD pattern with three behavior tests and strict boundary constraints (no SQLite in the pure module).

### Strengths

- **Evidence-backed target selection**: Module-level comment documenting audit/export selection and registry/state rejection satisfies EVID-01/EVID-02 cleanly.
- **TDD discipline**: Tests specified before implementation, with concrete acceptance criteria.
- **Strong boundary enforcement**: Grep gates preventing SQLite, `StateRead`/`StateWrite`, provider traits, and deferred security items from leaking into the pure module.
- **Scanner-as-reference, not template**: Uses typed records and pure helpers (matching `audit_validation.rs` style) rather than copying scanner module names.
- **Threat model includes scope-creep blocking**: T-01-03 explicitly transfers registration authorization TODOs to deferred status.

### Concerns

| # | Severity | Issue |
|---|----------|-------|
| C1 | **HIGH** | **Helper API is underspecified.** Task 1 says "Add explicit helper names such as `detected_ref_from_row_parts` and `classify_orbis_import_row` *only if* their arguments are typed." The conditional creates a risk that Plan 01-01 produces no helpers (or trivial ones), leaving Plan 01-02 with nothing to wire. The tests require at least one classification helper and one projection helper — the plan should commit to these explicitly rather than conditionally. |
| C2 | **HIGH** | **DTO ownership ambiguity.** Task 2 says "Move or define the public DTOs in `audit_records.rs` if that is the smallest stable shape; *otherwise keep the DTOs in `audit.rs`*." Keeping DTOs in `audit.rs` forces the pure `audit_records` module to depend on the effectful `audit` module, violating the directional rule that pure layers should not depend on effectful layers. The DTOs (`AuditDetectedRef`, `AuditScanExport`, `OrbisAuditEntry`) are pure data types with no SQLite dependency — they belong in `audit_records.rs`. This must be unconditional. |
| C3 | **MEDIUM** | **Current ineligible statuses are incomplete vs code.** Test 1 states "evidence-valid" as the only eligible status, but `import_orbis_audit_entries` at `audit.rs:290-293` accepts `evidence_valid` *or* `decrypt_failed` *or* `audit_complete`. The test must cover all three to prove behavior preservation. |
| C4 | **MEDIUM** | **`classify_orbis_import_row` signature needs to handle "row not found".** The plan says Test 2 covers "non-evidence-valid row" ineligibility. But `import_orbis_audit_entries` has three branches: eligible, ineligible (status exists but wrong), and ineligible (row not found). The "row not found" case cannot be represented in a pure helper that only takes a status — it needs `Option<status>` to distinguish. The plan should specify `fn classify_orbis_import_row(row: Option<(&str, bool)>) -> ImportEligibility` or similar. |
| C5 | **LOW** | **The `detected_ref_from_row_parts` helper has marginal value.** It's essentially a struct constructor wrapper. Its main value is testability of projection, which is real but small. The plan might be better served by focusing on the import classification (C3/C4) as the primary extraction. |

### Suggestions

- Make DTO movement unconditional: define all three DTOs in `audit_records.rs`, re-export through `lib.rs`, import from `audit.rs`.
- Make the classification helper signature explicit: `fn classify_orbis_import_entry(row: Option<(&str, bool)>) -> OrbisImportEligibility` returning an enum with `Eligible` and `Ineligible { stage, reason }`.
- Extend Test 1 to cover all three eligible statuses: `evidence_valid`, `decrypt_failed`, `audit_complete`.
- Remove the conditional language on helper creation — the tests concretely define what must exist.

---

## Plan 01-02: Wire facade and run verification

### Summary

Wires the effectful `audit.rs` facade through the pure helpers from Plan 01-01, removes obsolete patterns, and runs focused and broad verification. Depends on Plan 01-01 providing the helper surface.

### Strengths

- **Clear dependency ordering**: Correctly depends on 01-01, with no circular task structure.
- **Good grep gates**: Scans for provider traits (`trait.*Audit|struct.*Store`), SQL in pure module (`lock_conn|SELECT`), and compatibility aliases (`pub use.*as|alias`).
- **Verification checklist is thorough**: Focused audit tests, full compliance crate tests, formatting check, check-all-targets, plus documented handling of unrun broad checks.
- **Threat model properly gates security scope**: T-01-08 (deferred auth) is explicitly blocked.

### Concerns

| # | Severity | Issue |
|---|----------|-------|
| C6 | **HIGH** | **"Remove moved private helper definitions" is vague.** What exactly gets removed? The private helpers in `audit.rs` (`record_failure_tx`, `record_evidence_failure_tx`, `scan_info`, `audited_subjects`, `decode_tx_hash`, `now_unix`) are all SQLite-bound — they can't move to `audit_records.rs`. The only code that would be "removed" is the *inlined DTO construction and inline classification branching* now replaced by helper calls. But inline code isn't a "private helper definition." This needs to specify: e.g., "replace the inline eligibility if-chain in `import_orbis_audit_entries` with a call to `classify_orbis_import_entry`." |
| C7 | **MEDIUM** | **No rollback of pure-module additions if dependencies fail.** If Plan 01-01 produced too-small or incompatible helpers, Plan 01-02 has no fallback. The dependency constraint (`depends_on: 01-01`) should include an explicit API contract: the signature of `classify_orbis_import_entry` and the location of DTO types must match what 01-02 expects. |
| C8 | **MEDIUM** | **Facade delegation doesn't actually reduce coupling.** If the wiring task simply replaces inline `AuditDetectedRef { ... }` construction with a call to `detected_ref_from_row_parts(...)` but keeps the SQL query, transaction, and failure recording in the same function, the coupling reduction is minimal. The meaningful boundary is separating the eligibility *decision* from the persistence *effect* — the plan should prioritize this over projection extraction. |
| C9 | **LOW** | **`export_orbis_pending_scan` (line 225) also calls `scan_info(store)` which makes a separate SQL query.** The detection mapping and the scan info call are interleaved in the same function. If the plan only extracts the DTO construction but not the scan_info construct, the facade is barely cleaner. Consider whether `scan_info` should also be exposed from a helper. (Acceptable to defer — it's a separate concern.) |

### Suggestions

- Tighten "obsolete path removal" to name specific code sections: e.g., "replace the inline `AuditDetectedRef { height: ..., tx_hash: ..., ... }` construction in `export_orbis_pending_scan` and `export_detected_refs` with `audit_records::detected_ref_from_row_parts(...)`."
- Add a concrete API contract dependency between plans: 01-01 must export `classify_orbis_import_entry(status: Option<(&str, bool)>) -> OrbisImportEligibility` and `detected_ref_from_row_parts(...) -> AuditDetectedRef`.
- Run `cargo test -p penumbra-sdk-compliance --features component` as the *first* verification step to establish a baseline before changes, then again after.

---

## Risk Assessment

| Risk Level | Justification |
|------------|---------------|
| **MEDIUM** | The plans are structurally sound but have two HIGH-severity issues: underspecified helper API (C1) and ambiguous DTO ownership (C2) that could cause 01-01 to produce unmergeable or incompatible output for 01-02. If both issues are resolved (by making DTO movement unconditional and committing to explicit helper signatures), risk drops to LOW. No architectural over-engineering, no security regressions, and the scope remains tightly constrained to the audit/export boundary. |

## Whether plans achieve phase goals

**Partially.** The plans correctly target audit/export as the highest-payoff boundary and explicitly reject registry/state (EVID-01, EVID-02, EVID-03). They apply typed records and pure helpers per ARCH-01/ARCH-02. They avoid provider traits, scanner-name mimicry, and deferred security work (D-07, D-08, D-09).

However, **the boundary effectiveness depends on execution details that are underspecified**:
- If the pure helpers are only trivial struct constructors, the "clearer ownership" goal is weakly met
- If DTOs remain in `audit.rs`, the module dependency direction is wrong
- If the eligibility classification is extracted properly, the phase delivers real value

**Resolve C1 and C2 (make DTO movement unconditional, commit to explicit helper API signatures) and the plans are ready for execution.** The structural approach, tooling gates, test coverage, and verification plan are all appropriate.

---

## Consensus Summary

The reviewers broadly agree that the audit/export target is the right phase boundary and that the extract-then-wire sequence is directionally sound. The main shared concern is that Plan 01 leaves too much implementation discretion around the exact helper API and DTO ownership. Claude flags this as task overlap and conditional DTO placement; OpenCode raises it as high-risk because keeping pure DTOs in `audit.rs` would weaken the intended dependency direction. CodeRabbit mostly reports documentation and planning hygiene issues rather than core architecture blockers.

### Agreed Strengths

- Audit/export is a well-justified, contained target compared with registry/state.
- The plans preserve the important architectural split: pure record/classification helpers in `audit_records.rs`, SQLite and transaction effects in `audit.rs`.
- Verification is stronger than usual for a small refactor: focused tests, facade tests, crate tests, formatting, and compile checks are all specified.
- Scope constraints are explicit: no provider-trait symmetry, no deferred registration authorization, no regulated IBC policy work.

### Agreed Concerns

- The DTO ownership decision should be made before execution. Reviewers prefer moving `AuditDetectedRef`, `AuditScanExport`, and `OrbisAuditEntry` into `audit_records.rs` unconditionally, then re-exporting them through the existing public surface.
- The import-classification helper should have an explicit API contract. It needs to represent eligible rows, ineligible rows, and missing rows, and preserve the current accepted statuses: `evidence_valid`, `decrypt_failed`, and `audit_complete`.
- Plan 01 task boundaries are somewhat fuzzy. The difference between "minimal pure helpers" and "typed audit records and exports" may cause duplicated work unless the executor treats Plan 01 as one coherent extraction pass.
- Plan 02 should name the inline patterns being replaced more concretely, especially the import eligibility branching and detected-ref DTO construction.
- Some planning/documentation hygiene issues remain: roadmap/state counts, absolute execution-context paths, and ambiguous pattern/test rows.

### Divergent Views

- Claude rates the remaining plan risk as LOW and treats most issues as execution clarity improvements. OpenCode rates risk as MEDIUM until DTO ownership and helper signatures are explicit. CodeRabbit focuses on planning-document consistency and flags some items that may be outside the intended review scope, such as changing YOLO mode.

### Recommended Follow-Up Before Execution

Before running `$gsd-execute-phase 1`, either update the plans directly or run `$gsd-plan-phase 1 --reviews` so the planner incorporates these review findings. The highest-value changes are to make DTO movement unconditional, specify the import-classification enum/signature, and make Plan 02's facade-rewiring targets concrete.
