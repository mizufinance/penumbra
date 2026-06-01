# SnarkPack Verification & Testing Plan

The verification layers and the remaining work to complete the hardening phase.
This is the consolidated companion to `security.md` (campaign order and status)
and `formal-handoff.md` (the typed evidence ledger).

The campaign is layered defense: no single layer proves the system; each catches
a different bug class. The layers below run from "closest to the cryptographic
math" down to "closest to the raw bytes and ops." Each says what it checks, why
it matters (the bug class), its state, and how it is or should be implemented.

End-to-end formal verification is out of scope (Scope Lock, 2026-06-01).
Algebraic soundness is a standing assumption — the SnarkPack paper and the
Filecoin (Bellperson v0.21.0) implementation are assumed sound — and is
exhaustively cross-checked over its bounded transcript-shape domain, not proved,
by Layer 9.

## Part A — The verification layers

### 1. Boundary formal verification (hax → F*)

- What: extracts the executed Rust at the implementation boundary — statement
  encoder (`statement.rs`), wrapper framing, count/arity validation, padding
  canonicality, Fiat-Shamir challenge preimage — into F* and mechanically proves
  statement-encoding injectivity (distinct statements ⇒ distinct transcript
  preimage), digest reduction (a digest collision reduces to a SHA-256
  collision), padding canonicality, and bounded non-malleability.
- Why: the load-bearing binding property — a malicious proposer must not craft
  two different statements that hash to the same challenge. It is a proof over
  the real byte-producing code, so it cannot drift from what ships.
- State: complete for the current extracted target set. Statement-encoding
  injectivity, digest reduction, padding canonicality, and challenge-preimage
  injectivity are mechanically proved in the F* artifact set.
- Implementation: hax extraction → F* lemmas under
  `crates/crypto/proof-aggregation/formal/snarkpack/fstar/`, gated by
  `just snarkpack-formal`, content-stamped by the invariants gate.

### 2. Independent reference path (a second implementation)

- What: a dev-only, non-published crate `proof-aggregation-reference` re-implements
  the slow prover/verifier (aggregate/TIPA/GIPA/KZG/verifier-SRS codecs) from
  scratch using only public APIs; an invariant gate forbids importing production
  `src/ipp` internals, `ark-ip-proofs`, or `ark-inner-products`. Runs 3-way
  parity: production-prover/reference-verifier, reference-prover/production-verifier,
  reference/reference.
- Why: an independently written second implementation catches common-mode bugs a
  test sharing the production code would miss. The Rust-level independent oracle.
- State: implemented; feeds Layers 3-6.

### 3. Differential oracle tests (integration-level soundness/completeness)

- What: the aggregate's accept/reject must agree with per-proof Groth16 verify and
  with legacy batch verify across all parity families and counts
  (`snarkpack_matches_single_and_batch_groth16_oracles`,
  `snarkpack_property_matches_legacy_batch_oracle`).
- Why: catches the aggregate accepting what the underlying Groth16 would reject
  (soundness) or rejecting valid proofs (completeness) at the integration seam.
- State: implemented.
- Limitation: both sides share the arkworks lineage, so a shared algebraic bug
  passes both. That gap is Layer 9's job.

### 4. Trace instrumentation + trace equivalence

- What: production and reference emit structured `TraceEvent`s through the
  dependency-free `proof-aggregation-trace-schema` crate; the schema policy table
  must match the Spec Row Index in `ripp-spec.md`, and instrumentation must not
  re-decide levels from call shape (`production_and_reference_traces_match_declared_levels`).
- Why: verifies the two paths perform the same sequence/structure of transcript
  operations, not just the same final answer. The structural spine the byte
  baseline and mutation matrices hang off.
- State: implemented.

### 5. Byte-equivalence golden baselines

- What: two committed, version-tagged golden artifacts — an aggregate-proof byte
  baseline (`aggregate_bytes_match_committed_baseline`) and a PenumbraByte
  transcript-trace baseline (`penumbra_byte_trace_matches_committed_baseline`).
  Both regenerate deterministically from fixed `(family, count, seed)` vectors and
  fail on any drift; each version tag must equal `AGGREGATE_PROTOCOL_VERSION`.
- Why: makes silently changing the wire/transcript bytes impossible — "preserve
  bytes vs version the protocol" becomes a mechanical gate. This locks the (frozen)
  optimization loop and any refactor.
- State: implemented.
- Scope: byte equivalence is Penumbra-reference vs Penumbra-optimized only. There
  is deliberately no cross-curve byte equivalence to Filecoin — BLS12-381 vs
  BLS12-377 makes it impossible. This is why we do not "test against Filecoin"
  directly.

### 6. Mutation matrices (the threat model, executed)

- What: an input-mutant matrix mutates each binding field (VK digest, public-input
  value, public-input order, padding, counts, SRS id) and asserts the verifier
  rejects; a verifier-mutant matrix builds deliberately broken verifiers that
  omit/reorder challenge inputs and asserts they reject valid proofs (each
  Fiat-Shamir step is load-bearing). Coverage assertions force both matrices to
  cover every Penumbra byte-trace row (`mutation_matrices_cover_penumbra_byte_trace_rows`).
- Why: directly targets the SnarkPack v2 bug classes — a field that looks bound but
  is not, and a transcript step that does not matter. The most threat-model-aligned
  layer.
- State: implemented.

### 7. Filecoin-shape static check

- What: `scripts/check-snarkpack-filecoin-shape.sh` clones pinned Bellperson
  v0.21.0 and greps the prover/verifier/transcript source to confirm the transcript
  labels, ordering, V2 branch, and domain/nonce binding we modeled on still exist.
- Why: pins our claimed Fiat-Shamir discipline to the audited reference and fails
  if that reference drifts.
- State: implemented, but review-grade and static — it never executes Bellperson
  or compares behavior. Layer 9 is its executable upgrade.

### 8. Fuzzing (malformed-input robustness)

- What: stable proptests (in-gate smoke) plus cargo-fuzz/libFuzzer targets in the
  non-published `proof-aggregation-fuzz` crate over every byte boundary — wrapper
  decode, preflight, aggregate-proof deserialize, sidecar decode, bundle shape,
  proposal validation. Invariant: valid-accept or bounded-error; never panic,
  never unbounded allocation, never expensive work before cheap shape checks.
- Why: catches malformed-input handling bugs, panics, and DoS-via-malformed-bytes.
  The proposer is adversarial and submits arbitrary bytes.
- State: minimized corpora are committed for all six original byte-boundary
  targets plus the Layer 9 conformance target. The smoke gate seeds from the
  corpus through a temporary copy, and the 2026-06-01 coverage-guided baseline is
  recorded in `docs/snarkpack/fuzz-corpus-baseline.md`.
- Finding closed: proposal validation previously generated full default SRS-id
  material before rejecting malformed SRS ids; the regression
  `aggregate_bundle_verification_rejects_bad_srs_id_before_srs_setup` and
  checked-in `DEFAULT_DEV_SRS_ID` keep those rejects cheap.

### 9. Lean differential conformance

- What: an independent, hand-built Lean model of the transcript + folding
  discipline (FS label sequence, challenge derivation, GIPA/TIPA fold order,
  padding), derived from `ripp-spec.md` and the paper — not transliterated from the
  Rust, or it is circular — compiled to an executable oracle and differentially
  tested against the Rust. Pairing/field arithmetic stays abstract and `assumed`.
- Why: the only independent algebraic/transcript oracle. Every other behavioral
  layer (3-6) shares the arkworks lineage, so a bug in the equations themselves
  passes all of them; Lean is derived from the paper/Filecoin discipline, so it can
  falsify that shared-bug class. This strengthens, but does not remove, the
  standing algebraic-soundness assumption.
- State: implemented as evidence, not proof. The dev-only
  `proof-aggregation-lean-conformance` crate compiles the hand-written Lean model
  to an executable oracle, runs Rust-vs-Lean structural trace tests, and exposes
  `just snarkpack-lean-conformance`.
- Implementation: `SnarkpackOracle.lean` emits spec-row keyed event shapes; Rust
  fixtures compare public trace-schema event shapes against them. The transcript
  shape is fully determined by `padded_count = next power of two of the real
  count` (the only count-dependent part is the GIPA round count = log₂), so the
  domain of distinct shapes is **finite and small** — one per power of two up to
  the SRS max (2¹⁵ = 16 shapes) × 4 families. It is therefore **exhaustively
  enumerated, not fuzzed**: the always-on smoke test covers round depths 0..=5
  plus padding representatives, and `lean_oracle_matches_all_shapes_to_max`
  (release-gated, `#[ignore]`) covers every shape up to the SRS max. This is
  certainty over the bounded shape domain — superseding the earlier
  coverage-guided `lean_conformance` fuzz target, which sampled a domain small
  enough to enumerate outright. It remains bounded (≤ 2¹⁵) and structural
  (algebra abstract), so it is still evidence, not a soundness proof.

### 10. Performance / DoS-asymmetry gates

- What: fixed CI latency thresholds (p50/p95/p99 under realistic mixed proposals)
  plus valid-vs-invalid-path benchmarks proving a malformed/adversarial aggregate
  is rejected cheaply with bounded verifier work.
- Why: turns the "reject cheaply before expensive work" invariant into an enforced
  gate — closes the algorithmic-DoS asymmetry.
- State: implemented as a release-mode CI gate. `bench-thresholds.md` records the
  GitHub Actions `ubuntu-24.04` baseline and thresholds.
- Implementation: `just snarkpack-dos-gate` runs
  `snarkpack_dos_gate_valid_and_adversarial_paths_hold_thresholds`, covering
  malformed wrapper, wrong-family, wrong-public-input, oversized, valid, and
  mixed-proposal paths. The `snarkpack-formal` workflow runs the gate.

### 11. Assumption register — governance

- What: the evidence ledger in `formal-handoff.md`: every fact is typed
  `proved / refined / composed / assumed / open`; each `assumed` row needs a
  recorded postcondition + removal path; coverage is invariant-gated.
- Why: not a test — it makes "what is proven vs assumed" auditable and honest and
  prevents an assumption from silently widening.
- State: 13 proved / 1 refined / 6 composed / 13 assumed / 0 open, matching
  `formal-handoff.md` as of 2026-06-01.
- Left: no P1 ledger rows remain open; future work is evidence strengthening
  outside the P1 completion gate.

## Part B — What we deliberately do not verify (standing assumptions)

Locked 2026-06-01: SnarkPack/RIPP/Groth16 algebraic soundness (assumed from the
paper + Filecoin implementation), arkworks field/group/pairing/MSM correctness,
SHA-256 collision/preimage resistance, the random-oracle model for Fiat-Shamir,
BLS12-377 group laws, and hax semantic preservation. End-to-end FV is out of
scope. Layer 9 is the only one that cross-checks the algebraic/transcript
assumption (exhaustively over its bounded shape domain); the rest are
external-audit-or-replace.

## Part C — Remaining-work plan

P0 — Scope Lock — done (2026-06-01).

P1 — critical path to completion, closed by the current ledger:

| Item | Closes layer | Detail |
|---|---|---|
| Stale prose reconciled with the ledger | 1, 11 | Statement injectivity, digest reduction, padding canonicality, and challenge-preimage injectivity are `proved`; the ledger has no `open` rows. |
| Clean-image formal CI + arkworks boundary property tests | 1, 11 | The `snarkpack-formal` workflow uses pinned hax/F*/Z3 versions and the arkworks/decaf377 boundary tests are named in the assumption register. |
| RIPP-mapping review (`ripp-refinement.md`) | 1, 11 | Every scoped symbol is `refined` with code line and spec-row evidence. |
| DoS-asymmetry + perf gate | 10 | `just snarkpack-dos-gate` enforces fixed size, latency, and cheap-rejection thresholds in CI. |
| Assumption-register finalization | 11 | The 13 `assumed` rows each have a postcondition and removal path; rows naming backend tests cite implemented tests. |

P2 — parallel, evidence-strengthening, non-blocking:

| Item | Closes layer | Detail |
|---|---|---|
| Fuzz corpus expansion | 8 | Minimized corpora committed for each target; smoke seeds from the corpus; baseline and finding triage recorded in `fuzz-corpus-baseline.md`. |
| Lean differential conformance | 9 | Independent executable oracle implemented. Hand-derived Lean transcript/folding model, differentially tested against the Rust by exhaustive enumeration of the finite shape domain (powers of two to the SRS max), not fuzzing. |

P0-final — Final manual review: timeboxed review of spec, adaptation register,
reference path, F* proof index, test/fuzz evidence, and assumptions, once P1 is
green. Touches all layers.

## Sequencing

P1 no longer starts with an injectivity proof: that proof is already present in
`StatementEncodingProofs.fst` and the dependent digest/padding/challenge rows are
closed in `formal-handoff.md`. The live completion path is evidence maintenance:
keep the RIPP refinement map exact, keep the DoS gate in CI, keep assumption rows
narrow, and keep the formal workflow reproducible from pinned tools.

Fuzz expansion and Lean differential are P2 evidence-strengthening work. They are
implemented as non-blocking evidence: the standing algebraic-soundness assumption
remains, but it is now backed by a live paper-derived Lean conformance oracle that
exhaustively enumerates the finite transcript-shape domain, plus coverage-guided
byte-boundary fuzzing.

Critical path to phase-complete: no P1 formal rows remain open; final manual
review is the remaining campaign-level governance checkpoint.

## Completion definition

- All implementation-boundary rows in `formal-handoff.md` are `proved`;
  RIPP-mapping rows `refined`/`proved-equivalent`/`assumed`; adaptation rows
  coverage-checked; composition rows `composed`.
- No `open` rows remain; statement-encoding injectivity and digest reduction
  mechanically proved; padding canonicality + bounded non-malleability proved.
- No raw verifier bypass remains.
- Benchmark/DoS thresholds hold in CI after proof-driven refactors.
- Every assumption reviewed and narrowly scoped.

Layer 9 (Lean differential) is not in the completion gate — it strengthens the
standing algebraic-soundness assumption but the campaign completes without it.
