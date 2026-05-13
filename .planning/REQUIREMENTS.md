# Requirements: Penumbra Compliance Boundary Refactor

**Defined:** 2026-05-12
**Core Value:** Compliance code should be easier to reason about because durable state, pure validation, domain records, and external effects have clear ownership boundaries without added complexity.

## v1 Requirements

Requirements for the one-phase refactor. Each maps to the initial roadmap phase.

### Evidence

- [ ] **EVID-01**: The implementation phase identifies the compliance area or areas with the highest boundary-cleanup payoff using source evidence from `crates/core/component/compliance/src/`.
- [ ] **EVID-02**: The selected target area is justified against at least one rejected alternative, such as registry/state versus audit/export.
- [ ] **EVID-03**: The refactor scope is limited to one or two compliance boundaries unless implementation evidence shows that a broader change is necessary.

### Architecture

- [ ] **ARCH-01**: The selected compliance area separates durable state access from pure validation, classification, projection, or domain-record construction where the current code mixes those concerns.
- [ ] **ARCH-02**: New or moved modules expose narrow, purposeful APIs that match existing Penumbra Rust patterns and do not duplicate scanner names mechanically.
- [ ] **ARCH-03**: The refactor removes or reduces mixed-responsibility code in the selected area without adding compatibility shims, redundant aliases, or speculative provider traits.
- [ ] **ARCH-04**: Existing public behavior is preserved unless a focused failing test demonstrates an existing bug that must be fixed as part of the boundary cleanup.

### Implementation

- [ ] **IMPL-01**: Product code changes are primarily contained within `crates/core/component/compliance/src/`, with caller updates limited to required compile fixes or direct API consumers.
- [ ] **IMPL-02**: The scanner architecture is used as a reference for typed records and effect boundaries, not as a template that forces unnecessary abstractions.
- [ ] **IMPL-03**: Obsolete internal paths created by the refactor are removed rather than preserved as aliases.
- [ ] **IMPL-04**: The resulting module boundaries make the selected compliance flow easier to test directly than before the refactor.

### Verification

- [ ] **VERI-01**: Focused tests cover the selected compliance boundary before and after the refactor so behavior preservation is demonstrated.
- [ ] **VERI-02**: The relevant compliance crate tests pass after implementation.
- [ ] **VERI-03**: Formatting and the narrowest relevant compile/test checks pass, with any unrun broad checks explicitly documented.

## v2 Requirements

Deferred to future work. Tracked but not in the current one-phase roadmap.

### Security

- **SECU-01**: Add address ownership authorization for `MsgRegisterUser`.
- **SECU-02**: Define and enforce the asset-registration authority model for `MsgRegisterAsset`.
- **SECU-03**: Enforce regulated asset `allowed_channels` during shielded ICS-20 withdrawal validation.

### Storage

- **STOR-01**: Replace whole-tree compliance registry blob persistence with typed node/index records if benchmarks show registry scaling requires it.
- **STOR-02**: Add a commitment-to-position index for compliance leaf verification if linear scans become a real bottleneck.

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Full compliance redesign | This project must complete as one phase and should target the highest-payoff boundary only. |
| Protocol semantic changes | The requested feature is architectural cleanup, not changing compliance behavior. |
| Registration authorization fixes | Important, but security semantics need a dedicated phase and threat model. |
| Regulated asset IBC policy enforcement | Important, but it crosses compliance and shielded-pool behavior and is not required for this refactor. |
| Non-compliance refactors | The scanner pattern is the reference point, but the work should stay inside compliance unless a direct caller must change. |
| Abstraction for parity with scanner names | Boundaries must reduce coupling or improve testability; naming symmetry alone is not enough. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| EVID-01 | Phase 1 | Pending |
| EVID-02 | Phase 1 | Pending |
| EVID-03 | Phase 1 | Pending |
| ARCH-01 | Phase 1 | Pending |
| ARCH-02 | Phase 1 | Pending |
| ARCH-03 | Phase 1 | Pending |
| ARCH-04 | Phase 1 | Pending |
| IMPL-01 | Phase 1 | Pending |
| IMPL-02 | Phase 1 | Pending |
| IMPL-03 | Phase 1 | Pending |
| IMPL-04 | Phase 1 | Pending |
| VERI-01 | Phase 1 | Pending |
| VERI-02 | Phase 1 | Pending |
| VERI-03 | Phase 1 | Pending |

**Coverage:**
- v1 requirements: 14 total
- Mapped to phases: 14
- Unmapped: 0

---
*Requirements defined: 2026-05-12*
*Last updated: 2026-05-13 after roadmap creation*
