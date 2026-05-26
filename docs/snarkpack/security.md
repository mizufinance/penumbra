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

Status: mixed. The aggregate-bundle pipeline isolation is implemented; the full
transcript binding target is not.

Penumbra aggregates already-valid Groth16 proofs into internal
`AggregateBundle` transactions. The aggregate bundle is accepted only through
the proposal aggregation pipeline. It is not a user-facing action and must not
execute through generic action handling.

Implemented enforcement:

- aggregate-bundle transaction shape:
  `crates/core/app/src/app/mod.rs:2194`
- aggregate bundle must be last and unique in `ProcessProposal`:
  `crates/core/app/src/app/mod.rs:4209`
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

Status: target, with the implemented app-level invariants cited in `Scope`.

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

Status: target. The current backend digest is family-domain-separated only. It
does not yet bind the complete aggregate statement listed below.

Every aggregate statement has one canonical encoding. The transcript preimage
for Fiat-Shamir challenges must bind at least:

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
- all aggregate proof public messages in verifier order

Every field is length-prefixed or fixed-width encoded. Distinct aggregate
statements must not have the same transcript preimage.

The legacy prover/verifier phase domain API was deleted as a bug-class
reduction. It looked like phase domain separation, but the backend digest path
never consumed it, so it created a false sense of transcript binding. The next
transcript builder must add active statement binding rather than reintroducing
unused phase labels.

## Padding

Status: implemented for current padding behavior; transcript binding of the
padding rule is target work.

Aggregation inputs are padded to the next power of two by repeating the final
real proof and its public inputs. Verification recomputes the padded public
inputs from the proposal artifacts. `real_count` and `padded_count` are checked
against those recomputed values before aggregate verification.

Implemented enforcement:

- proof padding repeats the final proof:
  `crates/crypto/proof-aggregation/src/padding.rs:12`
- verifier public-input padding repeats the final public input:
  `crates/crypto/proof-aggregation/src/padding.rs:37`
- aggregate `real_count` and `padded_count` are checked against recomputed
  inputs: `crates/core/app/src/app/mod.rs:2350`,
  `crates/core/app/src/app/mod.rs:2358`

The padding rule is part of the aggregate statement. Changing the rule requires
a new aggregate version.

## Verification Matrix

Status: target. Some shape tests exist today; the full mutation/property matrix
is not implemented.

For each generated valid aggregate fixture, tests should mutate one field at a
time and assert rejection:

- family or family variant
- SRS identifier
- valid proof from a different SRS
- verifying key digest
- aggregate proof bytes
- public input value
- public input order
- same proof with a different padded public-input vector and consistent count
- real count
- padded count
- padding duplicate
- family order
- segment counts
- aggregate bundle transaction shape

Differential property tests use legacy batch verification as the oracle:

- random valid batch: legacy batch verify accepts and SnarkPack verify accepts
- mutate one proof or public input: legacy batch verify rejects and SnarkPack
  verify rejects
- legacy accepts but SnarkPack rejects: integration bug
- legacy rejects but SnarkPack accepts: security bug

## Fuzzing Targets

Status: target.

Fuzz these surfaces with panic detection and resource limits:

- `AggregateBundle` proto/domain decoding
- aggregate proof byte deserialization
- transcript statement encoding
- app-level aggregate bundle validation
- malformed proposal envelopes containing aggregate bundles

The expected fuzz result is either a valid accepted aggregate built from valid
artifacts or a bounded error.

## Benchmarking

Status: target measurement plan. CI failure thresholds should live in
`docs/snarkpack/bench-thresholds.md` once release-mode baselines are collected.

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

Status: target. No hax extraction is implemented yet.

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

Status: target.

This phase is complete when:

- the statement-binding spec is implemented by typed transcript code
- mutation and property tests cover the verification matrix
- fuzz targets run in CI smoke and nightly modes
- invalid inputs are bounded and panic-free
- release benchmarks establish accepted thresholds
- hax extraction covers the transcript encoder invariants, with Lean 4 used
  where the backend supports the required Rust subset
