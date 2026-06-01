# SnarkPack Security And Verification Plan

Status: this document is the hardening plan. Sections marked `Target` describe
required behavior that is not fully implemented yet. Sections marked
`Implemented` cite the code that currently enforces the invariant.

This document defines the internal hardening target for Penumbra proof
aggregation. It intentionally excludes the production SRS ceremony. The only
SRS requirement in this phase is that the SRS identity is bound into every
aggregate statement so a future production SRS swap is a normal versioned
change.

## Security Campaign Order

Status: build-out largely complete; campaign now in its evidence-closure phase.
Scope **locked** 2026-06-01 (decisions recorded under Scope Lock). Optimization is
**frozen** (see below). End-to-end formal verification is **out of scope** — the
SnarkPack paper and the Filecoin implementation are assumed sound (see Scope Lock).

This is the authoritative order for the SnarkPack security campaign. Each stage
carries its current status; the **Remaining Work** subsection below is the active
plan.

| # | Stage | Status |
|---|---|---|
| 1 | Scope Lock | **locked 2026-06-01** (decisions recorded) |
| 2 | Spec Reframe (`ripp-spec.md`) | drafted; needs review signoff |
| 3 | Adaptation Register (`adaptation-register.md` + scope file) | implemented |
| 4 | Implementation-boundary hax/F* glue | complete for current extracted target set |
| 5 | Penumbra slow reference path (reference crate) | implemented |
| 6 | Trace instrumentation (trace-schema crate) | implemented |
| 7 | Deterministic + property conformance tests | implemented |
| 8 | Coverage fuzzing | minimized corpora landed; smoke seeds from corpus; baseline recorded |
| 9 | Lean differential conformance: implementation vs paper / Filecoin discipline | implemented as non-blocking evidence |
| 10 | Optimization | **frozen** — byte-trace-locked loop, current state accepted as-is |
| 11 | Performance and DoS gates | implemented and CI-gated |
| 12 | Assumption register finalization | narrowed (13 `assumed` rows, each with postcondition + removal path) |
| 13 | Final manual review | **open** |

Stages 1 through 3 are planning artifacts with invariant-backed coverage. They
define the target for later proof, reference-path, testing, fuzzing, and review
work. They do not claim SnarkPack/RIPP/Groth16 algebraic soundness.

### Optimization is frozen

The Stage 10 byte-trace-locked optimization loop and its landed results are
accepted as-is. No further optimization work is planned. Any future optimization
must still obey the byte-trace lock in `docs/snarkpack/optimization-playbook.md`
(preserve the Penumbra byte trace or explicitly version the protocol), but it is
out of the active campaign critical path.

### Remaining Work (active plan)

Everything left to reach campaign completion, in dependency order. P1 evidence
closure is implemented: the formal handoff ledger has no `open` rows, the
DoS/performance gate is CI-wired, and the assumption register is narrowed. Stage
8 corpus expansion and Stage 9 Lean differential conformance are implemented as
non-blocking evidence. Final manual review remains the campaign-level
governance checkpoint. Algebraic
soundness is a **standing assumption** (paper + Filecoin implementation assumed
sound); end-to-end formal verification is not pursued, so there is no
algebraic-soundness proof stage.

The full verification landscape — every testing/verification layer (boundary FV,
independent reference path, differential oracle tests, trace equivalence,
byte-equivalence baselines, mutation matrices, Filecoin-shape check, fuzzing,
Lean differential conformance, DoS gates, assumption register), what each catches,
and the prioritized remaining work — is consolidated in
`docs/snarkpack/verification-plan.md`. This section is the campaign-order view of
the same plan.

**P0 — Scope Lock — DONE (2026-06-01).** Scope decisions are locked: algebraic
soundness is a standing assumption; boundary + implementation F* proofs remain
completion blockers; end-to-end FV is dropped; Lean differential conformance
(Stage 9) is the primary independent-oracle evidence. Recorded in the Scope Lock
section.

**P1 — Formal ledger closure (`formal-handoff.md`) — DONE.**
Statement-encoding injectivity, digest reduction, padding canonicality, and
challenge-preimage injectivity are `proved`. The RIPP mapping row is `refined`;
every scoped RIPP/GIPA/TIPA symbol is classified with file:line evidence and a
`ripp-spec.md` row reference.

**P1 — Performance and DoS gates (Stage 11) — DONE.** `bench-thresholds.md`
records fixed CI thresholds, and `just snarkpack-dos-gate` enforces the
valid-vs-adversarial asymmetry benchmark for malformed wrapper, wrong-family,
wrong-public-input, oversized, valid, and mixed-proposal paths.

**P1 — Assumption register finalization (Stage 12) — DONE.** The 13 `assumed`
rows each record a postcondition and removal path; arkworks/decaf377 rows cite
implemented boundary tests.

**P2 — Fuzzing corpus expansion (Stage 8 continued) — DONE as local corpus
baseline.** Each fuzz target has a committed minimized corpus, and the smoke gate
seeds from it through a temporary copy. `docs/snarkpack/fuzz-corpus-baseline.md`
records coverage-guided baseline runs, minimized coverage, and the
proposal-validation SRS-id cheap-rejection finding/fix.

**P2 — Stage 9: Lean differential conformance — DONE as non-blocking evidence.**
See the dedicated section below. The independent-oracle evidence that our
implementation conforms to the paper / Filecoin transcript discipline is live:
`just snarkpack-lean-conformance` runs the executable Lean oracle and Rust-vs-Lean
trace-shape tests that **exhaustively enumerate** the finite shape domain (one
shape per power of two up to the SRS max). Evidence, not proof (bounded and
structural); the algebraic-soundness row remains `assumed`.

**P0-final — Final manual review (Stage 13).** Timeboxed review of spec,
adaptation register, reference path, F* proof index, test/fuzz evidence, and
assumptions, once P0-P1 are green.

### Stage 9: Lean differential conformance

Goal — show our implementation is **conformant** to the paper / Filecoin
transcript discipline by differentially testing the Rust against an independent
Lean model. The transcript shape is determined by `padded_count = next power of
two of the real count`, so the domain of distinct shapes is finite and small (one
per power of two up to the SRS max) and is **exhaustively enumerated**, not
sampled — certainty over the bounded shape domain. This is the implemented
substitute for the algebraic-soundness proof that was dropped with end-to-end FV:
it does not prove soundness (it is bounded by the SRS max and keeps the algebra
abstract), but within that domain it checks every shape rather than sampling.

Motivation — the independent-oracle gap. Every behavioral test today compares
production-Rust against reference-Rust; both descend from the same arkworks
lineage and implement the same algebraic equations, so a bug in the equations
themselves passes both. The Filecoin-shape harness
(`scripts/check-snarkpack-filecoin-shape.sh`) is a **static source grep** of
pinned Bellperson `v0.21.0`, not an executable oracle, and cross-curve byte
equivalence is impossible (BLS12-381 vs BLS12-377). So the transcript/folding
discipline has no independent *executable* check — only static shape + human
review (`ripp-refinement.md`, `open`).

Implemented deliverable:
- An **independent, hand-built Lean model** of the transcript + folding structure
  (Fiat-Shamir label sequence, challenge derivation, GIPA/TIPA fold order,
  padding) — derived from `ripp-spec.md` and the paper, **not** transliterated
  from the Rust (or it is circular). Pairing/field arithmetic stays abstract and
  `assumed`, matching the existing scope (algebraic soundness out; FS discipline
  in). The model lives at
  `crates/crypto/proof-aggregation-lean-conformance/lean/SnarkpackOracle.lean`.
- A **differential conformance harness**: run the executable Lean model and the
  Rust reference on the same cases and compare the structural transcript/fold
  outputs. Because the shape domain is finite, the dev-only Rust crate
  **enumerates** it rather than fuzzing: an always-on smoke test covers round
  depths 0..=5 (all four families) plus padding representatives, and the
  release-gated `lean_oracle_matches_all_shapes_to_max` (`#[ignore]`) covers every
  shape up to the SRS max (2¹⁵). The earlier `lean_conformance` cargo-fuzz target
  was removed — it sampled a domain small enough to enumerate outright.

Scope discipline:
- This is **evidence, not proof** — it can falsify Lean↔Rust divergence from the
  paper/Filecoin discipline but does not prove equivalence. It strengthens the
  standing RIPP/GIPA/TIPA/SnarkPack algebraic-soundness assumption; there is no
  follow-on end-to-end proof.
- "Programmatic extraction" here means **compiling the Lean model to an executable
  oracle**. There is no automatic extraction of the Rust RIPP algebra into Lean;
  hax extracts Rust→F* only at the implementation boundary. The Lean model is
  written by hand from `ripp-spec.md` and the paper.

Stage 4 keeps the hax/F* glue focused on implementation-boundary rows over
executed Rust. It does not introduce duplicate formal-only encoders. The
formal artifact stamp includes proof files, `scripts/snarkpack-formal.sh`, and
`crates/crypto/proof-aggregation/formal/snarkpack/toolchain.toml`. Status:
complete for the current extracted Rust target set; the formal gate was run and
the proof artifact stamp was refreshed.

Stage 5 uses a separate crate,
`crates/crypto/proof-aggregation-reference`, for the slow Penumbra reference
oracle. That crate is dev-only, non-published, and may use only public
`penumbra-sdk-proof-aggregation` APIs, the shared trace-schema crate, and
ordinary crypto dependencies. The invariant gate rejects direct imports of the
production `src/ipp` internals, `ark-ip-proofs`, or `ark-inner-products`. The
reference prover/verifier, input-mutation matrix, and verifier-mutant matrix
are implemented as normal reference-crate tests.

Stage 6 is spec-first trace instrumentation. The dependency-free
`crates/crypto/proof-aggregation-trace-schema` crate defines the shared trace
schema used by production and reference code. Its policy table must match the
Spec Row Index in `docs/snarkpack/ripp-spec.md`; instrumentation must not
re-decide trace levels from production call shape. The Filecoin-shape harness
re-derives evidence from Bellperson `v0.21.0`; no cross-curve byte equivalence
is claimed.

Stage 7 is implemented as deterministic and seeded property coverage. The
production backend checks Groth16 single-proof, batch, and aggregate agreement
across parity families and count tables. The reference crate checks
production/reference/batch agreement for clean and mutated inputs, declares
the input mutation matrix, and asserts the input plus verifier matrices cover
every Penumbra byte trace row.

Stage 8 is implemented as bounded fuzz smoke coverage plus committed minimized
libFuzzer corpora. Stable proptests cover wrapper decoding, preflight, aggregate
proof deserialization, sidecar decoding, and aggregate bundle transaction shape.
The non-published fuzz crate provides libFuzzer targets for the same byte
boundaries and proposal-validation path, with invariant-gated dependency
boundaries. The 2026-06-01 corpus baseline and clean-run coverage are recorded in
`docs/snarkpack/fuzz-corpus-baseline.md`.

Stage 10 (optimization) is implemented as a byte-trace-locked iterative loop and
is now **frozen**: the landed results are accepted as-is and no further
optimization is planned. The governing rule remains that any future optimization
must preserve the Penumbra byte trace or explicitly version the protocol. Two committed, version-tagged golden
baselines enforce this: an aggregate-proof byte baseline
(`aggregate_bytes_match_committed_baseline`) and a PenumbraByte transcript-trace
baseline (`penumbra_byte_trace_matches_committed_baseline`). Both regenerate
deterministically from fixed `(family, count, seed)` vectors and fail on any
drift. The version tag on each baseline must equal `AGGREGATE_PROTOCOL_VERSION`,
making the choice between "preserve bytes" and "version the protocol" a
mechanical gate rather than a judgment call.

The per-optimization workflow is:

1. Confirm the targeted stage is measured (the `snarkpack verify` / `snarkpack
   aggregate` Criterion groups in `crates/bench/benches/vanilla/snarkpack.rs`).
2. Implement the optimization.
3. Run the byte and trace baselines, the production/reference trace-equivalence
   test, the Groth16 oracle-agreement table, and the mutation matrices. If the
   baselines hold, the change is byte-preserving and lands as a refactor. If a
   baseline fails, either revert to a byte-preserving form, or take the
   version path: bump `AGGREGATE_PROTOCOL_VERSION`, regenerate both baselines via
   their `--ignored` helper tests, and add an `adaptation-register.md` row (with
   a matching `adaptation-scope.txt` entry) documenting the byte change. No
   silent byte changes are permitted.
4. Record the before/after performance delta on the targeted stage.

The first optimization landed under this loop replaces the GIPA verifier's
sequential final commitment-key recombination
(`_compute_final_commitment_keys`) with a variable-base multiexponentiation,
exposed as the overridable `DoublyHomomorphicCommitment::msm_keys` method
(default fold; AFGHO group-backed override). It is byte-preserving by
construction — `msm_keys_equals_sequential_fold` proves the MSM yields the same
group element as the prior fold — so it landed with both baselines unchanged and
`AGGREGATE_PROTOCOL_VERSION` still `1`.

The full search-to-land method — how to find candidates (profiler- and
pattern-driven), classify the change, measure it honestly with the corpus-backed
bench and the compile-time `bench-baseline` A/B seam, apply the win-or-clarity
bar, and run the gate set — is the optimization playbook in
`docs/snarkpack/optimization-playbook.md`, which also carries the ranked
candidate backlog.

Fixed performance thresholds and the denial-of-service asymmetry gate are a
later stage; Stage 10 only requires a repeatable measurement so optimizations
are evidence-backed.

## Scope Lock

Status: locked 2026-06-01. Decisions recorded below; this does not require a
security-firm engagement.

Locked decisions (2026-06-01):
- Algebraic soundness of SnarkPack/RIPP/Groth16 is a **standing assumption** — the
  published paper and the Filecoin (Bellperson v0.21.0) implementation are assumed
  sound. It is not a campaign deliverable and not an `open` blocker.
- **Boundary and implementation F\* proofs stay completion blockers** (statement
  injectivity, digest reduction, padding canonicality) — these are boundary FV
  over executed Rust, not end-to-end.
- **End-to-end formal verification is dropped** — no Lean algebraic proof, no
  EasyCrypt Fiat-Shamir proof, no composed end-to-end theorem.
- **Lean differential conformance (Stage 9) is implemented** as the primary
  independent oracle: exhaustively enumerate the finite transcript-shape domain
  and check the Rust against the paper / Filecoin discipline on every shape.
  Evidence, not proof (bounded + structural); non-blocking.

Campaign claim:

```text
Penumbra implements a Penumbra-local SnarkPack/RIPP backend whose Fiat-Shamir
transcript discipline is checked against Filecoin SnarkPack v2 bug classes.
Penumbra-specific statement, wrapper, padding, challenge, and preflight binding
obligations are proved, refined, composed, or assumed per the evidence taxonomy
in formal-handoff.md. SnarkPack/RIPP/Groth16 algebraic soundness is not
proved; it is assumed from the published paper and the Filecoin implementation,
and exhaustively cross-checked over its bounded transcript-shape domain by Lean
differential conformance (Stage 9).
End-to-end formal verification is out of scope.
```

Evidence statuses are exactly those in `docs/snarkpack/formal-handoff.md`:
`proved`, `refined`, `composed`, `assumed`, and `open`. Scope-lock text must
not weaken open blockers with "where feasible" or equivalent wording.

Source roles:

- Filecoin v2 is the normative reference for Fiat-Shamir omission/reordering
  bug classes and transcript discipline.
- The Penumbra spec is the normative reference for Penumbra curve, hash,
  statement, padding, SRS/VK binding, and app integration.
- The SnarkPack paper is algebraic background, not the production
  implementation oracle.
- The existing Arkworks lineage is provenance and a comparison aid, not the
  production security baseline.

Pinned Filecoin references:

- Bellperson repository: `filecoin-project/bellperson`
- Bellperson tag `v0.21.0` observed at peeled commit
  `62c362fd46ca2139747b8770bae53ce6f1e42bb1`; this is the normative
  SnarkPack-shape source for transcript, GIPA/TIPA, and aggregate-verifier
  comparison.
- rust-fil-proofs tag `filecoin-proofs-v11.1.0` observed at commit
  `004d7b4244c469e0d9aeebf15f9a81ef60308ba3`; this is production-consumer
  evidence that Bellperson `0.21.0` shipped through Filecoin Network v16 Skyr.
- Bellperson release branch `v0-18-release-branch` at
  `ff5f39e43cc62481cc575adae628cb7d1124bce8` and Bellperson tag `v0.18.2` at
  peeled commit `c5fa04be1824ceb19a96a36ee1689f9d15b2e864` are historical
  comparison aids only.

Non-claim:

```text
No cross-curve byte-level equivalence to Filecoin is claimed. Byte-level
transcript equality is required only between Penumbra reference and Penumbra
optimized paths.
```

Preserved hax/F* discipline:

- extracted targets have explicit `requires` and extraction-boundary rows
- every `assume val` has a recorded semantic postcondition and removal path
- no unrecorded admits, `--admit_smt_queries`, or debug-only invariants in
  extracted-code claims
- hax extraction remains over executed Rust, not duplicate formal-only encoders

Preserved formal tool pins:

- hax `v0.3.7`
- F* `v2026.05.24`
- Rust `1.89`
- OCaml `5.1.1`
- Z3 `4.14.1`
- OPAM switch `hax-0.3.7`

Changing these pins requires the formal gate described in
`docs/snarkpack/formal-handoff.md`.

Preserved arkworks assumption split:

- algebraic assumptions: field/group/pairing laws and abstract
  Groth16/RIPP/SnarkPack soundness
- implementation assumptions: arkworks arithmetic, MSM, and serialization
  behavior
- encoding/subgroup checks: compressed G1/G2 rejection, torsion and malformed
  byte rejection, identity semantics, round-trip stability, and VK/SRS digest
  stability

## Scope

Status: implemented for the current prototype hardening pass.

Penumbra aggregates already-valid Groth16 proofs into internal
`AggregateBundle` transactions. The aggregate bundle is accepted only through
the proposal aggregation pipeline. It is not a user-facing action and must not
execute through generic action handling.

Implemented enforcement:

- aggregate-bundle transaction shape:
  `crates/core/app/src/app/mod.rs:2202`
- aggregate bundle must be last and unique in `ProcessProposal`:
  `crates/core/app/src/app/mod.rs:4224`
- aggregate bundle rejected from generic action stateless, historical, and
  execution handling: `crates/core/app/src/action_handler/actions.rs:40`,
  `crates/core/app/src/action_handler/actions.rs:64`,
  `crates/core/app/src/action_handler/actions.rs:88`

The current prototype uses a Penumbra-owned SnarkPack/RIPP implementation
forked from Arkworks RIPP over BLS12-377. The original RIPP code is not treated
as a production-security baseline; audit scope is the full local
implementation. The security goal for this phase is implementation binding and
verification discipline: a malicious proposer must not be able to replace,
reorder, omit, or mismatch any public statement material while still producing
an accepted aggregate.

## Threat Model

Status: implemented for the listed cheap-shape and statement-binding checks;
coverage-guided fuzz corpus baselines are recorded.

Assume a malicious proposer or aggregator can submit arbitrary aggregate bundle
bytes and arbitrary ordinary transactions. The verifier must reject:

- malformed aggregate proof encodings
- aggregate proof bytes above configured limits
- unknown aggregate versions
- unknown or mismatched SRS identities
- wrong proof family or family variant
- wrong verifying key for the family
- omitted, reordered, or mutated public inputs
- wrong real or padded proof count
- non-canonical padding
- missing, extra, or reordered family aggregates
- segment coverage mismatches
- aggregate bundles embedded in user transactions
- aggregate bundles not placed last in a proposal

Invalid inputs must return errors. They must not panic, allocate without a hard
bound, or perform avoidable expensive work before cheap shape checks.

## Statement Binding

Status: implemented for version, curve id, backend id, family, SRS id,
verifying-key digest, counts, padding rule, and ordered padded public inputs.
The aggregate proof wrapper stores only a recomputed statement digest; protobuf
has no second digest field.

Every aggregate statement has one canonical encoding. The Fiat-Shamir
challenge context binds:

- aggregate protocol version
- curve identifier
- backend identifier
- SRS identifier
- proof family and family variant
- verifying key digest
- real proof count
- padded proof count
- canonical padding rule
- ordered padded public inputs
- all aggregate proof public messages in verifier order, through the
  Penumbra-owned challenge helper

Every byte field is length-prefixed, including fixed-width digests; integer
fields are fixed-width little-endian. Distinct aggregate statements must not
have the same transcript preimage.

Implemented enforcement:

- statement constructor and encoder:
  `crates/crypto/proof-aggregation/src/statement.rs`
- aggregate proof wrapper digest check:
  `crates/crypto/proof-aggregation/src/aggregate_proof_wrapper.rs`
- typed aggregate preflight, which recomputes SRS/VK facts and decodes the
  wrapper before SnarkPack verification:
  `crates/crypto/proof-aggregation/src/preflight.rs`
- Penumbra-owned Fiat-Shamir helper:
  `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs`
- explicit prover/verifier challenge trace parity:
  `prover_verifier_challenge_streams_match`

The legacy prover/verifier phase domain API was deleted as a bug-class
reduction. It looked like phase domain separation, but the backend digest path
never consumed it, so it created a false sense of transcript binding. The next
transcript builder must add active statement binding rather than reintroducing
unused phase labels.

## Padding

Status: implemented for current padding behavior and statement binding of the
padding rule.

Aggregation inputs are padded to the next power of two by repeating the final
real proof and its public inputs. Verification recomputes the padded public
inputs from the proposal artifacts. `real_count` and `padded_count` are checked
against those recomputed values before aggregate verification.

Implemented enforcement:

- proof padding repeats the final proof:
  `crates/crypto/proof-aggregation/src/padding.rs:14`
- verifier public-input padding repeats the final public input:
  `crates/crypto/proof-aggregation/src/padding.rs:39`
- aggregate `real_count` and `padded_count` are checked against recomputed
  inputs: `crates/core/app/src/app/mod.rs:2359`,
  `crates/core/app/src/app/mod.rs:2371`

The padding rule is part of the aggregate statement. Changing the rule requires
a new aggregate version.

## Verification Matrix

Status: implemented as focused unit/property coverage for the current pass,
except the alternate-valid-SRS fixture listed as an open item.

| Invariant or mutation | Coverage |
| --- | --- |
| family or family variant | `snarkpack_backend_rejects_wrong_family_id`; `aggregate_bundle_verification_rejects_bad_version_srs_and_family_count` |
| SRS identifier | `statement_rejects_mutated_srs_id`; `aggregate_bundle_verification_rejects_bad_version_srs_and_family_count` |
| valid proof from a different SRS | TODO(snarkpack-srs-v2): add when a second production-style SRS fixture exists |
| verifying key digest | `statement_mismatch_rejects_vk_digest_mutation_before_backend` |
| aggregate proof bytes | `snarkpack_backend_rejects_malformed_aggregate_bytes`; `malformed_aggregate_proof_oversize_rejected_before_deserialization`; `wrapper_rejects_malformed_length` |
| public input value | `snarkpack_backend_rejects_mutated_public_inputs`; `snarkpack_property_matches_legacy_batch_oracle`; `reference_property_matches_production_and_batch_oracles` |
| public input order | `statement_digest_binds_inputs`; `snarkpack_property_matches_legacy_batch_oracle`; `reference_property_matches_production_and_batch_oracles` |
| same proof with different padded public-input vector and consistent count | `snarkpack_backend_rejects_mutated_public_inputs` |
| real count | `statement_rejects_bad_counts`; app aggregate real-count checks |
| padded count | `statement_rejects_bad_padding`; app aggregate padded-count checks |
| padding duplicate | `pads_by_repeating_last_item`; `prepare_verify_inputs_matches_full_padding` |
| family order | app aggregate ordering checks in `verify_aggregate_bundle_for_artifacts_raw_profiled` |
| segment counts | segmented aggregate bundle tests and segment coverage checks in `verify_aggregate_bundle_for_artifacts_raw_profiled` |
| aggregate bundle transaction shape | `ensure_aggregate_bundle_tx_shape_rejects_memo_detection_fee_and_extra_action` |
| prover/verifier challenge stream equality | `prover_verifier_challenge_streams_match` |
| statement canonical field order | `statement_canonical_encoding_layout` |
| independent reference accepts production aggregate | `reference_verifier_accepts_production_prover` |
| production verifier accepts independent reference aggregate | `reference_prover_cross_verifies_with_production` |
| production/reference trace parity | `production_and_reference_traces_match_declared_levels` |
| reference input mutation matrix | `reference_verifier_rejects_required_input_mutations`; `input_mutant_matrix_is_declared_per_byte_binding_row`; `mutation_matrices_cover_penumbra_byte_trace_rows` |
| verifier-mutant challenge omission/reordering sensitivity | `verifier_mutants_reject_valid_proofs`; `verifier_mutant_matrix_is_declared_per_byte_binding_row`; `mutation_matrices_cover_penumbra_byte_trace_rows` |
| aggregate determinism | `aggregation_is_deterministic_for_fixed_inputs` |
| Groth16 oracle agreement | `snarkpack_matches_single_and_batch_groth16_oracles` |
| untrusted byte entrypoints | `wrapper_decode_inner_range_do_not_panic`; `preflight_aggregate_verify_do_not_panic`; `deserialize_aggregate_proof_do_not_panic`; `decode_batch_item_do_not_panic`; `decode_artifact_do_not_panic`; `ensure_aggregate_bundle_tx_shape_do_not_panic`; `just snarkpack-fuzz-smoke` |

Differential property tests use legacy batch verification as the oracle:

- random valid batch: legacy batch verify accepts and SnarkPack verify accepts
- mutate one proof or public input: legacy batch verify rejects and SnarkPack
  verify rejects
- legacy accepts but SnarkPack rejects: integration bug
- legacy rejects but SnarkPack accepts: security bug

## Fuzzing Targets

Status: bounded smoke coverage and committed minimized corpora implemented.

Fuzz these surfaces with panic detection and resource limits:

- `AggregateBundle` proto/domain decoding
- aggregate proof byte deserialization
- transcript statement encoding
- app-level aggregate bundle validation
- malformed proposal envelopes containing aggregate bundles

The expected fuzz result is either a valid accepted aggregate built from valid
artifacts or a bounded error.

Implemented harnesses:

- `decode_wrapped_aggregate_proof_inner_range`
- `preflight_aggregate_verify`
- aggregate proof deserialization through the fuzz-only helper
- sidecar `decode_batch_item` and `decode_artifact`
- aggregate bundle transaction shape
- proposal aggregate bundle validation with bounded artifacts

The 2026-06-01 corpus expansion found and closed one boundedness issue:
malformed SRS ids in proposal validation could force default SRS-id/SRS setup
before cheap rejection. The regression
`aggregate_bundle_verification_rejects_bad_srs_id_before_srs_setup` keeps those
paths below the cheap-rejection budget, and saved slow units replay in
milliseconds against the fixed target.

## Benchmarking

Status: CI-gated. Fixed release-mode thresholds are recorded in
`docs/snarkpack/bench-thresholds.md` and enforced by `just snarkpack-dos-gate`
in `.github/workflows/snarkpack-formal.yml`.

Benchmark both valid and invalid paths in release mode:

- aggregate build by family and proof count
- aggregate verify by family and proof count
- malformed byte rejection
- wrong-family and wrong-input rejection
- proposal validation with and without aggregate bundle
- p50, p95, and p99 latency under realistic mixed proposals

Benchmark regressions are security-relevant when invalid inputs become an
asymmetric denial-of-service path.

## Formal Verification

Status: Stage 4 implementation-boundary F* rows are complete for the current
extracted Rust target set. The RIPP refinement row is reviewed, assumptions are
narrowed, and no formal handoff rows remain `open`. The broader campaign remains
open only for final manual review and non-blocking evidence strengthening. The
authoritative evidence index is `docs/snarkpack/formal-handoff.md`.

Evidence is typed:

- `proved`: mechanically checked in F* against extracted executed Rust
- `refined`: reviewed against the published algorithm with tests and signoff
- `composed`: Rust types plus proved/refined pieces, tests, and invariant
  guards
- `assumed`: explicit external/tool/cryptographic assumption
- `open`: completion blocker

The implementation-boundary target is statement encoding injectivity, wrapper
framing, count/arity validation, padding canonicality, and explicit
Fiat-Shamir challenge preimage binding. The local RIPP implementation is not a
black-box oracle; `docs/snarkpack/ripp-refinement.md` maps proof-relevant RIPP
symbols to the intended algorithm and is reviewed for P1 completion.
Intentional Filecoin-to-Penumbra differences are tracked in
`docs/snarkpack/adaptation-register.md`; the scope file under
`crates/crypto/proof-aggregation/formal/snarkpack/adaptation-scope.txt` keeps
that register coverage-checkable.

End-to-end cryptographic proof is **out of scope**: SnarkPack/RIPP/Groth16
algebraic soundness is assumed from the published paper and the Filecoin
implementation (Scope Lock). F* covers the implementation boundary; Lean is used
only for the Stage 9 differential conformance oracle, not for an algebraic proof.

## Independent Reference And Trace Evidence

Status: implemented as code and tests. These are part of the closed P1 evidence
set; the P2 Lean conformance oracle is now live supporting evidence, and final
manual review remains outside the implementation gate.

The independent reference crate re-derives the dev SRS from the public seed and
checks the resulting SRS id against production. It owns its aggregate proof,
TIPA, GIPA, KZG, and verifier-SRS codecs and implements slow prover/verifier
equations without importing production RIPP internals. The normal
reference-crate test suite exercises production-prover/reference-verifier,
reference-prover/production-verifier, and reference-prover/reference-verifier
parity.

Production and reference emit `TraceEvent`s through
`penumbra-sdk-proof-aggregation-trace-schema`. Penumbra-byte rows compare exact
Penumbra bytes only between Penumbra paths; Filecoin-shape rows are checked by
`just snarkpack-filecoin-shape` against the pinned Bellperson source.

## Completion Criteria

Status: P1 implementation gate closed. Stage 4 implementation-boundary F* rows
are complete for the current extracted Rust target set; RIPP refinement,
clean-image formal CI, arkworks/decaf377 boundary tests, and fixed CI benchmark
gates are in place. Final manual review remains the campaign-level checkpoint.

This phase is complete when:

- all pure implementation-boundary rows in `formal-handoff.md` are `proved`
- RIPP mapping rows are `refined`, `proved-equivalent`, or explicitly
  `assumed`
- Filecoin-to-Penumbra adaptation rows are coverage-checked and either
  `refined`, `proved-equivalent`, or explicitly `assumed`
- app/backend composition rows are `composed`
- every assumption is reviewed and narrowly scoped
- no `open` rows remain
- no raw verifier bypass remains
- statement encoding injectivity is mechanically proved
- digest reduction is mechanically proved modulo SHA-256 collision resistance
- padding canonicality and bounded non-malleability are proved
- benchmark thresholds hold after proof-driven refactors
