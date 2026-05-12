# Penumbra Engineering Instructions

This is a new product, not a live backwards-compatible system. Prefer the
correct design over legacy shims. Remove obsolete paths instead of keeping
aliases, compatibility flags, or half-finished abstractions.

## Workflow

- For non-trivial work, discuss the goal, risks, and intended shape before
  writing a detailed plan.
- If design intent is unclear, ask before implementing.
- If a refactor touches more than five files, make the scope explicit first.
- When something goes sideways, stop and re-evaluate instead of pushing through.
- Follow impact through all affected layers: circuits, domain logic, storage,
  services, CLI, tests, and docs.
- Don’t fight errors! Whenever you encounter the same error twice, research the web and find 3-5 possible ways to fix it. Then choose the most efficient solution and implement it."

## Architecture Pattern

- Treat durable state as a spine, not a one-off handoff. Independent workers,
  validators, projectors, and exporters should communicate through typed,
  replayable records.
- Separate pure domain logic from effects. Parsing, classification, validation,
  and projection should be side-effect-free where possible; persistence,
  networking, enrichment, and external services belong at the edges.
- Use typed references and canonical identifiers. Do not invent synthetic IDs or
  hashes in production paths; if code mirrors canonical logic, add parity tests.
- Make state machines explicit. Define valid states, legal transitions, and
  terminal conditions in code and tests.
- Validate before completing downstream work. A row, object, proof, or external
  response should not advance to a completed state until its prerequisites are
  checked.
- Keep dependencies explicit. Core workers should receive stores, providers, and
  clients from their caller; CLI or app wiring owns concrete RPC/filesystem
  implementations.
- Put external systems behind narrow provider traits. Do not let core logic grow
  direct dependencies on policy services, caches, label stores, or network
  clients.
- Persist useful failures, but bound attacker-controlled growth.
- Prefer deleting replaced flows over preserving compatibility surfaces.

## Verification

- Never mark work complete without proving it.
- Bug fixes need a reproducing test first, then the fix.
- Run focused tests after each meaningful section of a multi-step change.
- For broad changes, run the relevant full checks from the local docs or
  justfile before final handoff.
- When proof-generation tests require special keys or release builds, say
  whether they were actually run.

## Style

- Modularity and simplicity over cleverness.
- Drop redundant module/crate names from function names.
- Standard crypto abbreviations are fine: `ss`, `ct`, `pt`, `esk`, `epk`, `dk`,
  `fq`.
- Comments and docs should be succinct and factual. Add public API docs when
  they explain ownership, invariants, inputs, outputs, or failure modes; avoid
  comments that only repeat names or implementation history.
- As a rule of thumb, module docs should stay under 5-8 lines, public type docs
  under 1-3 lines, and function docs under 1-2 lines unless the API has real
  safety or protocol nuance.
- Do not duplicate docs across files. Define once, reference elsewhere.
