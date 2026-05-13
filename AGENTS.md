# Penumbra Engineering Instructions

New product, no backwards-compat. Prefer the correct design over legacy shims.
Delete obsolete paths; do not keep aliases, flags, or half-finished abstractions.

## Workflow

- Discuss goal, risks, and shape before writing a detailed plan.
- Ask when design intent is unclear. Make scope explicit before refactors >5 files.
- Stop and re-evaluate when something goes sideways; do not push through.
- Follow impact through every affected layer: circuits, domain, storage, services, CLI, tests, docs.
- If the same error hits twice, research 3-5 fixes and pick the best — do not flail.

## Architecture

- **Typed domain records** carry facts and events across boundaries. No untyped tuples or maps for cross-boundary data.
- **Pure helpers** for parsing, validation, classification, projection — side-effect-free, unit-testable in isolation.
- **Durable state at the edge** via existing `StateRead` / `StateWrite` patterns. Core logic takes these as inputs; it does not own connections, files, or RPC clients.
- **Provider traits only for real external effects** (RPC, network, MPC, filesystem). Do not introduce traits for internal indirection or speculative future swaps.
- **Durable state is a spine, not a handoff.** Workers, validators, projectors, exporters communicate through replayable typed records on shared storage.
- **Canonical identifiers only.** Do not invent synthetic IDs or hashes in production paths; if code mirrors canonical logic, add a parity test.
- **Explicit state machines.** Define legal states, transitions, and terminal conditions in code and tests.
- **Validate before completing downstream work.** Rows, objects, proofs, and external responses must not reach a completed state until prerequisites are checked.
- **Persist useful failures, bound attacker-controlled growth.**
- **Delete replaced flows.** Do not preserve compatibility surfaces.

## Verification

- Never mark work complete without proving it.
- Bug fixes: reproducing test first, then fix.
- Run focused tests after each meaningful section; relevant full checks before final handoff.
- Say explicitly whether prover/release-gated tests were actually run.

## Style

- Modularity and simplicity over cleverness.
- Drop redundant module/crate names from function names.
- Standard crypto abbreviations fine: `ss`, `ct`, `pt`, `esk`, `epk`, `dk`, `fq`.
- Docs succinct and factual: module ≤5-8 lines, public type ≤1-3, function ≤1-2 unless real protocol nuance.
- Document ownership, invariants, inputs, outputs, failure modes. Do not restate names or history.
- Define docs once; reference elsewhere.
