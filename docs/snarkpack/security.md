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

Status: scope lock pending internal signoff.

This is the authoritative order for the SnarkPack security campaign:

1. Scope Lock
2. Spec Reframe
3. Adaptation Register
4. Implementation-boundary hax/F* glue, running in parallel after Stage 1
5. Penumbra slow reference path
6. Trace instrumentation
7. Deterministic conformance tests
8. Property and differential testing
9. Coverage fuzzing
10. Optional advanced research track: Lean/EasyCrypt/spec-guided bug finding,
    non-blocking
11. Optimization
12. Performance and DoS gates
13. Assumption register finalization
14. Final manual review

Stages 1 through 3 are planning artifacts with invariant-backed coverage. They
define the target for later proof, reference-path, testing, fuzzing,
optimization, and review work. They do not claim SnarkPack/RIPP/Groth16
algebraic soundness.

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

## Scope Lock

Status: pending internal signoff; this does not require a security-firm
engagement.

Campaign claim:

```text
Penumbra implements a Penumbra-local SnarkPack/RIPP backend whose Fiat-Shamir
transcript discipline is checked against Filecoin SnarkPack v2 bug classes.
Penumbra-specific statement, wrapper, padding, challenge, and preflight binding
obligations are proved, refined, composed, assumed, or open per the evidence
taxonomy in formal-handoff.md. This campaign does not prove
SnarkPack/RIPP/Groth16 algebraic soundness.
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
long-running fuzzing remains open.

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
| public input value | `snarkpack_backend_rejects_mutated_public_inputs`; `snarkpack_property_matches_legacy_batch_oracle` |
| public input order | `statement_digest_binds_inputs`; `snarkpack_property_matches_legacy_batch_oracle` |
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
| reference input mutation matrix | `reference_verifier_rejects_required_input_mutations` |
| verifier-mutant challenge omission/reordering sensitivity | `verifier_mutants_reject_valid_proofs` |

Differential property tests use legacy batch verification as the oracle:

- random valid batch: legacy batch verify accepts and SnarkPack verify accepts
- mutate one proof or public input: legacy batch verify rejects and SnarkPack
  verify rejects
- legacy accepts but SnarkPack rejects: integration bug
- legacy rejects but SnarkPack accepts: security bug

## Fuzzing Targets

Status: smoke coverage implemented; long-running fuzz harnesses remain open.

Fuzz these surfaces with panic detection and resource limits:

- `AggregateBundle` proto/domain decoding
- aggregate proof byte deserialization
- transcript statement encoding
- app-level aggregate bundle validation
- malformed proposal envelopes containing aggregate bundles

The expected fuzz result is either a valid accepted aggregate built from valid
artifacts or a bounded error.

Open items:

- add long-running `cargo-fuzz` harnesses for aggregate bundle decoding,
  wrapped proof decoding, malformed aggregate verification, and statement
  construction
- wire those harnesses into nightly CI with corpus retention

## Benchmarking

Status: provisional local size threshold is recorded in
`docs/snarkpack/bench-thresholds.md`; release latency gates are still open.

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
extracted Rust target set. The broader verification campaign remains open for
RIPP refinement, Filecoin-to-Penumbra adaptation, assumption review, reference
and trace evidence, boundary tests, performance gates, and final review. The
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
symbols to the intended algorithm and must be reviewed before completion.
Intentional Filecoin-to-Penumbra differences are tracked in
`docs/snarkpack/adaptation-register.md`; the scope file under
`crates/crypto/proof-aggregation/formal/snarkpack/adaptation-scope.txt` keeps
that register coverage-checkable.

The end-to-end cryptographic proof is tracked separately in
`docs/snarkpack/formal-research-plan.md`. That research track uses Lean 4 for
the algebraic protocol model and EasyCrypt for Fiat-Shamir/random-oracle games;
F* remains the implementation-boundary proof backend.

## Independent Reference And Trace Evidence

Status: implemented as code and tests, but not sufficient to complete the full
campaign without the remaining refinement, adaptation, assumption, trace,
boundary-test, performance, and review gates.

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

Status: open. Stage 4 implementation-boundary F* rows are complete for the
current extracted Rust target set; RIPP refinement signoff, clean-image formal
CI, arkworks boundary tests, and fixed CI benchmark gates remain open.

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
