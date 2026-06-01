# Codex Brief — P2 (Evidence-Strengthening, Non-Blocking)

Source of truth: `docs/snarkpack/verification-plan.md` (Layers 8 and 9),
`docs/snarkpack/security.md` (Stage 8 continued, Stage 9), `docs/snarkpack/ripp-spec.md`
(the spec the Lean model is derived from).

P2 strengthens the **standing algebraic-soundness assumption** (paper + Filecoin
assumed sound). It is **explicitly excluded from the campaign completion gate** —
do not block P1 on it, and do not let P2 work change protocol bytes, transcript
semantics, or production code paths. P2 is oracles and corpora, not protocol
changes.

## Hard rules

- **Production behavior is frozen by P2.** No category-2 or category-3 changes. No
  edits to `statement.rs` / `transcript.rs` / `challenge.rs` semantics, no byte /
  trace / version drift. P2 adds test oracles and corpora only.
- **The Lean model must be derived from the spec/paper, NOT transliterated from the
  Rust.** A model copied from the implementation shares its bugs and proves
  nothing. Derive from `ripp-spec.md` + the SnarkPack/RIPP paper + the Filecoin v2
  transcript discipline. Pairing/field arithmetic stays abstract / `assumed`.
- **Fuzzers must stay bounded.** Targets must never panic, never allocate
  unbounded, never do expensive work before cheap shape checks. A new finding is a
  bug to file + a minimized corpus entry, not a silent catch.
- **Prototype contract policy** (CLAUDE.md): new crates are dev-only, non-published;
  no shims/aliases. If a fuzz finding requires a production fix, that fix follows
  the P1 rules (reproducing test first, no transcript changes).

## Task A — Fuzz corpus expansion (Layer 8)

Today: bounded smoke proptests + a cargo-fuzz scaffold
(`crates/crypto/proof-aggregation-fuzz`, 6 targets: `wrapper_inner_range`,
`preflight_aggregate_verify`, `deserialize_aggregate_proof`, `sidecar_decoding`,
`aggregate_bundle_shape`, `proposal_validation`). The smoke gate runs
`SNARKPACK_FUZZ_RUNS` (default 16) per target — that is a CI smoke check, not real
fuzzing.

Goal: sustained, coverage-guided corpora that actually explore the byte space.

- Run each of the 6 targets under cargo-fuzz / libFuzzer for **sustained sessions**
  (hours, coverage-guided), well beyond the 16-run smoke. Use seed corpora from
  real valid artifacts where possible so the fuzzer starts past the shape checks.
- **Retain and minimize** the resulting corpora outside the smoke gate: commit a
  minimized corpus per target (`cargo fuzz cmin`) so coverage is reproducible and
  CI smoke seeds from it. Keep the committed corpus small; store the large raw
  corpus out of the gate.
- For every crash / hang / OOM: minimize (`cargo fuzz tmin`), file it, add a
  reproducing unit test, and (if it is a production bug) fix under the P1 rules.
- Confirm the invariant on each target: valid-accept **or** bounded-error; never
  panic; never unbounded allocation; never expensive work before cheap shape
  checks. Where a target can be driven into expensive-work-before-shape-check, that
  is a finding.
- Acceptance: each target has a committed minimized corpus with recorded coverage;
  the smoke gate seeds from it; any finding has a minimized reproducer + test;
  documented "runs clean for N hours" baseline per target.

## Task B — Lean differential conformance (Layer 9 — the real gap)

This is the only oracle that can catch a bug in the **shared arkworks algebra**
(where production, the reference crate, and the Groth16 oracle could all be wrong
the same way). It is a larger investment; it is evidence, not proof; it does not
gate completion.

Build it in three stages — do not skip to fuzzing before the model is differentially
sound on seeded vectors.

1. **Hand-build the Lean model** of the transcript + folding discipline: the FS
   label sequence, challenge derivation, GIPA/TIPA fold order, and padding —
   **derived from `ripp-spec.md` and the paper**, not from the Rust. Pairing and
   field arithmetic are abstract / `assumed` (the model checks transcript and
   folding *structure*, not the curve math). Compile it to an **executable oracle**
   ("programmatic extraction" = compile the Lean model to a runnable oracle; there
   is no auto-extraction of the Rust algebra into Lean).
2. **Seeded differential proptest**: run the Rust and the Lean oracle on the same
   `(family, count, seed)` vectors and assert the transcript/fold trace agrees.
   Reuse the existing trace-schema vocabulary (`proof-aggregation-trace-schema`,
   the Spec Row Index in `ripp-spec.md`) so the comparison is structural, not
   stringly-typed. A disagreement is either a Rust bug or a model bug — triage and
   record which.
3. **Graduate to coverage-guided conformance fuzzing**: point Task A's
   coverage-guided machinery at the **conformance property** ("Rust trace == Lean
   oracle trace") instead of just "no panic". This turns the fuzzer from a
   robustness tool into a soundness cross-check.

- Keep the model + oracle in a **dev-only, non-published** crate (mirror the
  `proof-aggregation-reference` arrangement: no importing production internals that
  would make it circular).
- Update `formal-handoff.md`: the "abstract RIPP/GIPA/TIPA/SnarkPack algebraic
  soundness" `assumed` row already names "Stage 9 Lean differential conformance" as
  supporting evidence — once the oracle exists and runs, record it as *implemented*
  supporting evidence (the row stays `assumed`; Lean strengthens it, does not prove
  it). Restamp if you touch any stamped formal artifact.
- Acceptance: an executable paper-derived Lean oracle exists; seeded differential
  proptest passes (or all disagreements are triaged and resolved); the conformance
  fuzz target runs; the handoff row records Lean as live supporting evidence.

## Gates

P2 work must keep all P1 gates green and add no protocol drift:

```sh
cargo test -p penumbra-sdk-proof-aggregation --lib
cargo test -p penumbra-sdk-proof-aggregation-reference --lib
just snarkpack-fuzz-smoke
just snarkpack-filecoin-shape
just snarkpack-invariants
cargo fmt --all -- --check
# new:
just snarkpack-lean-conformance   # (name TBD — created in Task B)
```

## Definition of done for P2

- Each fuzz target has a committed, minimized, coverage-recorded corpus; the smoke
  gate seeds from it; findings (if any) have minimized reproducers + tests.
- An executable, paper-derived Lean transcript/folding oracle exists and is
  differentially tested against the Rust (seeded + coverage-guided).
- `formal-handoff.md` records Lean as live supporting evidence for the algebraic-
  soundness assumption (row stays `assumed`).
- No byte / trace / version drift; production behavior unchanged.

