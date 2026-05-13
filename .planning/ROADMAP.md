# Roadmap: Penumbra Compliance Boundary Refactor

## Overview

Deliver a single MVP refactor phase that uses source evidence to select the highest-payoff compliance boundary, applies scanner-style ownership separation only where it reduces coupling, and proves preserved behavior with focused tests and relevant checks.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Compliance Boundary MVP** - Select, refactor, and verify one or two high-payoff compliance boundaries without expanding behavior.

## Phase Details

### Phase 1: Compliance Boundary MVP
**Goal**: Compliance code has clearer ownership boundaries in the highest-payoff selected area while preserving existing behavior.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: EVID-01, EVID-02, EVID-03, ARCH-01, ARCH-02, ARCH-03, ARCH-04, IMPL-01, IMPL-02, IMPL-03, IMPL-04, VERI-01, VERI-02, VERI-03
**Success Criteria** (what must be TRUE):
  1. The implementation records source-backed evidence for the selected compliance boundary and at least one rejected alternative.
  2. The selected compliance flow separates durable state access from pure validation, projection, or domain-record construction through narrow Penumbra-style APIs.
  3. Obsolete internal paths from the refactor are removed, with no compatibility aliases, speculative provider traits, or scanner-name mimicry added.
  4. Focused tests demonstrate preserved behavior for the selected boundary and make the refactored flow easier to exercise directly.
  5. Relevant compliance tests, formatting, and the narrowest useful compile/test checks pass, with any intentionally unrun broad checks documented.
**Plans**: 2 plans
Plans:
- [ ] 01-01-PLAN.md — Select audit/export with source evidence and extract pure typed audit records.
- [ ] 01-02-PLAN.md — Wire the audit facade through the pure boundary and run phase verification.

## Progress

**Execution Order:**
Phases execute in numeric order: 1

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Compliance Boundary MVP | 0/TBD | Not started | - |
