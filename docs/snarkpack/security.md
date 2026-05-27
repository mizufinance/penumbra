# SnarkPack Security And Verification Plan

Status: this document is the hardening plan. Sections marked `Target` describe
required behavior that is not fully implemented yet. Sections marked
`Implemented` cite the code that currently enforces the invariant.

This document defines the internal hardening target for Penumbra proof
aggregation. It intentionally excludes the production SRS ceremony. The only
SRS requirement in this phase is that the SRS identity is bound into every
aggregate statement so a future production SRS swap is a normal versioned
change.

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

The current prototype uses a vendored Arkworks/RIPP SnarkPack backend over
BLS12-377. The security goal for this phase is implementation binding and
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
- all aggregate proof public messages in verifier order, through the vendored
  challenge helper

Every byte field is length-prefixed, including fixed-width digests; integer
fields are fixed-width little-endian. Distinct aggregate statements must not
have the same transcript preimage.

Implemented enforcement:

- statement constructor and encoder:
  `crates/crypto/proof-aggregation/src/statement.rs`
- aggregate proof wrapper digest check:
  `crates/crypto/proof-aggregation/src/aggregate_proof_wrapper.rs`
- backend wrapper decode before SnarkPack verification:
  `crates/crypto/proof-aggregation/src/backend.rs`
- vendored Fiat-Shamir helper:
  `crates/crypto/proof-aggregation/vendor/ripp/ip_proofs/src/challenge.rs`
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

Status: formal handoff artifacts exist in `docs/snarkpack/formal-handoff.md`.
No hax extraction is implemented yet.

The first formal target is the transcript and statement encoder, not full
SnarkPack algebraic soundness.

Use hax extraction to F* first for implementation velocity and toolchain risk
reduction. Keep Lean 4 as the preferred Rust-to-Lean target when the extracted
subset is supported, and use Coq as a fallback if it is the lowest-friction
backend for a specific lemma.

The initial hax target is:

- transcript encoding injectivity
- inclusion of all required public statement fields
- fixed challenge input order
- padding and count invariants

EasyCrypt is reserved for later game-based soundness work if the project needs
a cryptographic proof model beyond implementation structure.

## Completion Criteria

Status: partially complete. Formal extraction and fixed CI benchmark gates are
next-phase work.

This phase is complete when:

- the statement-binding spec is implemented by typed transcript code
- mutation and property tests cover the verification matrix
- fuzz targets run in CI smoke and nightly modes
- invalid inputs are bounded and panic-free
- release benchmarks establish accepted thresholds
- hax extraction covers the transcript encoder invariants, with Lean 4 used
  where the backend supports the required Rust subset
