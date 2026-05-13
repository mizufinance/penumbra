# Phase 1: Compliance Boundary MVP - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-12
**Phase:** 1-Compliance Boundary MVP
**Areas discussed:** Target boundary selection, Boundary shape, Behavior preservation, Proof standard

---

## Target Boundary Selection

| Option | Description | Selected |
|--------|-------------|----------|
| Registry/state | Focus on compliance registration, registry storage, and component state writes. | |
| Audit/export | Focus on audit evidence, export/import, and Orbis-facing workflows. | |
| Both | Evaluate both and implement one or both based on evidence and complexity. | ✓ |

**User's choice:** Both.
**Notes:** User wants the boundary that benefits most, not necessarily the smallest boundary. One or two refactors are acceptable.

---

## Boundary Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Typed records | Carry facts across boundaries as explicit domain records. | ✓ |
| Pure helpers | Separate validation, projection, and classification from effects. | ✓ |
| State edge | Keep durable state access behind existing state traits and state-key helpers. | ✓ |
| Provider traits | Use only for real external effects, not internal indirection. | ✓ |

**User's choice:** Asked for best-practice recommendation.
**Notes:** Recommendation locked: typed records first, pure helpers second, state access at the edge, provider traits only for real external effects.

---

## Behavior Preservation

| Option | Description | Selected |
|--------|-------------|----------|
| Strict behavior preservation | Do not change externally visible semantics. | |
| Allow covered cleanup | Permit obvious behavior-changing cleanup when clearly better and tested. | ✓ |
| Work on TODOs | Implement nearby TODO/security issues during the refactor. | |

**User's choice:** Allow obvious optimizations or cleanup that may change semantics, but do not work on TODOs.
**Notes:** Registration authorization, channel-policy enforcement, and other deferred TODO/security items should not be implemented in this phase.

---

## Proof Standard

| Option | Description | Selected |
|--------|-------------|----------|
| Focused unit tests | Prove extracted logic and preserved behavior at the smallest useful boundary. | ✓ |
| Integration tests | Exercise relevant compliance workflows through existing integration surfaces. | ✓ |
| Full CI/check pipeline | Run the full local CI/check pipeline where feasible. | ✓ |
| Document unrun checks | Explicitly state any broad checks that could not be run. | ✓ |

**User's choice:** Everything: unit tests, integration tests, and all CI/CD pipeline passing.
**Notes:** Final handoff must clearly state any checks that were not run or could not pass locally.

---

## the agent's Discretion

- Choose the final target boundary after evaluating both registry/state and audit/export.
- Choose exact module names and extraction structure if they follow existing Penumbra patterns and keep complexity down.

## Deferred Ideas

- Registration authorization fixes.
- Regulated asset IBC policy enforcement.
- Registry storage scaling/index work unless it is directly necessary for the selected refactor.
