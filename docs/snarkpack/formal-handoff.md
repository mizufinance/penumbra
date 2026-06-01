# SnarkPack Formal Handoff

This is the typed evidence ledger. Implementation-boundary F* rows cover the
current extracted Rust target set; the RIPP implementation map is reviewed; no
rows remain `open`. The `assumed` rows are standing external/tool/cryptographic
assumptions with named postconditions and removal paths, not missing
implementation work.

Evidence statuses:

- `proved`: mechanically checked in F* against hax-extracted executed Rust.
- `refined`: reviewed against the published algorithm, backed by tests and
  signed review.
- `composed`: enforced by Rust types plus proved/refined pieces, tests, and
  invariant guards.
- `assumed`: explicit external/tool/cryptographic assumption with owner,
  rationale, supporting evidence, and removal path.
- `open`: completion blocker; none remain in this ledger.

Pinned tools: hax `v0.3.7`, F* `v2026.05.24`, Rust `1.89`, OCaml `5.1.1`,
Z3 `4.14.1`, OPAM switch `hax-0.3.7`. Any hax/F*/OCaml/Z3/Rust pin change
requires rerunning `just snarkpack-formal`, reviewing generated extraction
diffs and support shims, updating the verification marker, and refreshing the
proof artifact stamp.

Proof artifact stamp: sha256:ddb9147693d1b2ef02ecb7c1168feddbbf4cc96426a434dee28326d73613d283

The stamp is the SHA-256 of the committed SnarkPack F* proof files and
`scripts/snarkpack-formal.sh` plus
`crates/crypto/proof-aggregation/formal/snarkpack/toolchain.toml`. It is
checked by `just snarkpack-invariants`.

## Final Implementation Claim

If Penumbra aggregate verification accepts, then the accepted backend call was
produced from recomputed local artifacts, passed verified statement, wrapper,
padding, and challenge preflight, and reached a local RIPP implementation
reviewed against the intended algorithm. Validity then depends only on named
cryptographic, arkworks, hax, and refinement assumptions.

This is a composition claim, not a full mechanized SnarkPack/RIPP/Groth16
soundness theorem.

## Completion Rules

Statement encoding injectivity is `proved` for the current extracted Rust target
set by `lemma_encode_statement_injective`. It cannot be downgraded to
`composed`. Digest reduction, padding canonicality, challenge-preimage
injectivity, wrapper binding, typed aggregate preflight, and app-level aggregate
composition all depend on that proved row plus their named proof/test evidence.
If a future change reopens statement encoding injectivity, those dependent rows
must be re-reviewed and either reopened or given an explicit replacement proof
dependency.

Security-binding or semantic RIPP deviations in
`docs/snarkpack/ripp-refinement.md` are blockers unless mechanically
`proved-equivalent` or explicitly accepted as `assumed` by security/crypto
review. Prose review can support `refined`, but not `proved-equivalent`.

Every assumption row must have an owner, rationale, supporting evidence, removal
path, and security/crypto reviewer signoff. Disputed RIPP deviation
classification defaults to the higher-risk class until resolved.

## Proof And Evidence Index

| Obligation | Rust path | Extracted or evidence target | Backend/evidence | Proof or evidence file | Lemma or row | Status | Tool version | Verification marker |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| statement byte-field framing injectivity | `crates/crypto/proof-aggregation/src/statement.rs` | `StatementFieldBytes`, `StatementPublicInputRow`, `StatementPaddedRows` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/StatementEncodingProofs.fst` | byte-field, row, and field framing injectivity | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| full statement encoding injectivity | `crates/crypto/proof-aggregation/src/statement.rs` | `StatementEncodingInput`, `encode_statement` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/StatementEncodingProofs.fst` | `lemma_encode_statement_injective` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| statement digest equality reduces to canonical statement equality | `crates/crypto/proof-aggregation/src/statement.rs` | `statement_digest`, `encode_statement` | F* corollary plus SHA-256 CR assumption | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/StatementEncodingProofs.fst` | digest reduction modulo SHA-256 collision resistance | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| count validation rejects zero real count | `crates/crypto/proof-aggregation/src/statement.rs` | `validate_counts` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ValidationProofs.fst` | `lemma_validate_counts_rejects_zero` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| count validation branch coverage | `crates/crypto/proof-aggregation/src/statement.rs` | `validate_counts` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ValidationProofs.fst` | bad-count, bad-padding, and success guard lemmas | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| count validation iff | `crates/crypto/proof-aggregation/src/statement.rs` | `validate_counts` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ValidationProofs.fst` | `lemma_validate_counts_iff` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| row arity validation iff | `crates/crypto/proof-aggregation/src/statement.rs` | `validate_row_arity` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ValidationProofs.fst` | `lemma_validate_row_arity_iff_top` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| padding canonicality and bounded non-malleability | `crates/crypto/proof-aggregation/src/statement.rs`; `crates/crypto/proof-aggregation/src/padding.rs` | `validate_repeat_final_padding` and statement binding of `real_count` | F* via hax plus Rust tests | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ValidationProofs.fst` | `lemma_validate_repeat_final_padding_iff` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| wrapper oversize rejects before parsing | `crates/crypto/proof-aggregation/src/aggregate_proof_wrapper.rs` | `decode_wrapped_aggregate_proof_inner_range` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/WrapperProofs.fst` | `lemma_wrapper_rejects_oversize_before_parsing` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| wrapper round trip and exact inner range | `crates/crypto/proof-aggregation/src/aggregate_proof_wrapper.rs` | wrapper encode/decode core | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/WrapperProofs.fst` | `lemma_wrapper_roundtrip` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| wrapper digest mismatch rejects before inner exposure | `crates/crypto/proof-aggregation/src/aggregate_proof_wrapper.rs` | wrapper decode core | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/WrapperProofs.fst` | `lemma_wrapper_digest_mismatch_before_range` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| challenge preimage layout and injectivity | `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs` | `challenge_preimage` | F* via hax | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ChallengePreimageProofs.fst` | `lemma_challenge_preimage_layout`; `lemma_challenge_preimage_injective` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| challenge context constructor derives from statement digest | `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs` | `ChallengeContext::from_statement_digest` | F* via hax plus Rust privacy guard | `crates/crypto/proof-aggregation/formal/snarkpack/fstar/ChallengePreimageProofs.fst`; invariant script | `lemma_challenge_context_preimage_layout`; `lemma_challenge_context_bytes_injective` | proved | hax `v0.3.7`, F* `v2026.05.24` | formal gate passed |
| challenge context has no alternate production constructor | `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs` | `ChallengeContext` privacy and invariant guards | Rust type system plus `just snarkpack-invariants` | `scripts/check-snarkpack-invariants.sh` | no `Default`, tuple constructor, or TLS context | composed | n/a | invariant gate passed |
| aggregate backend receives only preflighted bytes | `crates/crypto/proof-aggregation/src/preflight.rs`; `src/backend.rs` | `VerifiedAggregateBackendCall`, `VerifiedInnerProofBytes` | Rust type system plus invariant guards | `scripts/check-snarkpack-invariants.sh` | raw verifier entrypoints route through typed preflight | composed | n/a | invariant gate passed |
| app-level aggregate composition | `crates/core/app/src/app/mod.rs` | aggregate bundle verification pipeline | Rust tests plus typed backend preflight | `docs/snarkpack/security.md` verification matrix | recomputed statement material reaches typed preflight | composed | n/a | invariant gate passed |
| deterministic and property conformance | `crates/crypto/proof-aggregation/src/backend.rs`; `crates/crypto/proof-aggregation-reference/src/lib.rs` | production/reference/batch oracle parity, Groth16 oracle table, mutation matrices, aggregate determinism | Rust deterministic tests plus proptest | `docs/snarkpack/security.md` verification matrix | clean and mutated aggregate decisions agree across production, reference, and batch oracles | composed | n/a | test and invariant gates passed |
| untrusted-byte fuzz smoke coverage | `crates/crypto/proof-aggregation/src/aggregate_proof_wrapper.rs`; `src/preflight.rs`; `src/backend.rs`; `crates/core/app/src/app/preconsensus.rs`; `crates/core/app/src/app/mod.rs`; `crates/crypto/proof-aggregation-fuzz` | wrapper, preflight, aggregate deserialization, sidecar, aggregate-bundle shape, and proposal-validation byte boundaries | Stable proptest plus cargo-fuzz smoke targets | `just snarkpack-fuzz-smoke`; `scripts/check-snarkpack-invariants.sh` | byte-boundary entrypoints return bounded Ok/Err without panics in smoke coverage | composed | n/a | test, fuzz, and invariant gates passed |
| optimization preserves byte trace or versions the protocol | `crates/crypto/proof-aggregation/src/backend.rs`; `crates/crypto/proof-aggregation-reference/src/lib.rs`; `crates/crypto/proof-aggregation/src/ipp/dh_commitments/src/afgho16/mod.rs`; `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs`; `docs/snarkpack/optimization-playbook.md`; `crates/bench/benches/vanilla/snarkpack.rs` | committed aggregate-byte and PenumbraByte-trace baselines; `msm_keys` final commitment-key recombination; corpus-backed bench plus compile-time `bench-baseline` A/B seam | Rust golden-baseline tests plus unit equivalence; documented playbook process | `aggregate_bytes_match_committed_baseline`; `penumbra_byte_trace_matches_committed_baseline`; `msm_keys_equals_sequential_fold`; committed fixtures under `tests/fixtures/`; `docs/snarkpack/optimization-playbook.md` | aggregate bytes and transcript are locked to a version-tagged baseline; the playbook constrains optimizations to categories 1/2 (never transcript), measured honestly, so a change either preserves bytes or bumps `AGGREGATE_PROTOCOL_VERSION` with an adaptation-register row | composed | n/a | test gates passed |
| local RIPP implementation maps to intended algorithm | `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src` | proof-relevant RIPP symbols | refinement map plus tests/review | `docs/snarkpack/ripp-refinement.md` | all scoped rows refined against `ripp-spec.md` | refined | n/a | ripp refinement reviewed; invariant gate passed |

## Assumptions

| Assumption | Owner | Rationale | Why not proved here | Supporting evidence | Removal path | Required signoff | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SHA-256 collision resistance | cryptography lead | Statement digest binding reduces to this after encoding injectivity is proved. | External cryptographic primitive assumption. | Postcondition: distinct encoded statements do not collide under SHA-256 at the chosen security margins; evidence is standard SHA-256 analysis plus fixed domain prefixes. | replace primitive or obtain external audit evidence; no end-to-end FV is planned for this primitive | security/crypto | assumed |
| SHA-256 preimage resistance | cryptography lead | Challenge context and wrapper digests use SHA-256-derived commitments. | External cryptographic primitive assumption. | Postcondition: attacker cannot choose proof/wrapper/challenge bytes that invert the recorded SHA-256 commitments at the chosen security margins; evidence is standard SHA-256 analysis plus fixed domain prefixes. | replace primitive or obtain external audit evidence; no end-to-end FV is planned for this primitive | security/crypto | assumed |
| Domain separation by fixed distinct prefixes | proof-aggregation maintainers | Separate statement digest, challenge context, challenge preimage, VK digest, and wrapper domains. | Reduces to fixed-prefix review plus hash assumptions. | Postcondition: statement digest, challenge context, challenge preimage, VK digest, and wrapper domains have disjoint fixed prefixes; evidence is golden-layout tests plus invariant review. | prove prefix disjointness mechanically if this becomes proof-critical | security/crypto | assumed |
| abstract Groth16 soundness | cryptography lead | Aggregate verification ultimately depends on Groth16 proof soundness. | Out of implementation-boundary FV scope. | Postcondition: accepted Groth16 proofs satisfy the verified circuits under the published Groth16 assumptions; evidence is published Groth16 proof material and existing Penumbra circuit audits. | standing assumption; replace only with external audit or separate Groth16 proof campaign | security/crypto | assumed |
| abstract RIPP/GIPA/TIPA/SnarkPack algebraic soundness | cryptography lead | Local implementation is reviewed against the algorithm, but algebraic soundness is external. | End-to-end FV out of scope; paper + Filecoin impl assumed sound. | Postcondition: the reviewed local RIPP/GIPA/TIPA equations are sound under the published SnarkPack/RIPP algebraic assumptions; evidence is published proof material, `ripp-refinement.md`, and the implemented Stage 9 Lean differential conformance gate (`just snarkpack-lean-conformance`). | standing assumption; removal requires a separate algebraic proof or external audit; Lean conformance remains supporting evidence, not a proof | security/crypto | assumed |
| arkworks field/group/pairing mathematical operation implementations | proof-aggregation maintainers | The implementation calls arkworks arithmetic primitives. | Full library verification is outside this scope. | Postcondition: arkworks field, group, and pairing operations implement the algebra used by SnarkPack; evidence is upstream tests plus `arkworks_pairing_identity_and_generator_consistency`, `arkworks_g1_g2_compressed_round_trip_and_identity`, and `arkworks_g1_g2_subgroup_and_torsion_rejection`. | verified arithmetic backend or external audit artifact | security/crypto | assumed |
| arkworks MSM implementation computes intended linear combination | proof-aggregation maintainers | MSM is an implementation-heavy dependency, not a pure algebra axiom. | Full arkworks MSM verification is outside this scope. | Postcondition: arkworks MSM returns the same linear combination as the naive fold for the boundary cases used by aggregation; evidence is `arkworks_msm_boundary_zero_scalar_identity_and_random_parity`. | verified MSM or external audit artifact | security/crypto | assumed |
| arkworks serialization and subgroup behavior | proof-aggregation maintainers | SRS, VK, proof bytes, and digests depend on arkworks encoding checks. | Full serialization/subgroup proof is outside this scope. | Postcondition: checked compressed G1/G2 decoding rejects malformed and non-subgroup encodings and round-trips valid/identity encodings; evidence is `arkworks_g1_g2_compressed_round_trip_and_identity`, `arkworks_g1_g2_malformed_compressed_bytes_reject`, and `arkworks_g1_g2_subgroup_and_torsion_rejection`. | verified serialization backend or external audit artifact | security/crypto | assumed |
| hax extraction preserves modeled Rust semantics for the extracted safe subset | formal verification owner | F* proofs are over hax output. | hax semantic preservation is not proved inside this repo. | Postcondition: each extracted safe-subset target preserves the Rust semantics recorded in `hax-extraction-boundary.md`; evidence is pinned versions and invariant guards over the target list and support assumptions. | upstream hax soundness proof or independent translation validation | security/crypto/formal | assumed |
| `impl_u32__is_power_of_two` shim preserves Rust semantics | formal verification owner | Required because pinned hax support output is not directly accepted by pinned F*. | Compatibility shim, not an implementation property. | Postcondition: shim truth value equals Rust `u32::is_power_of_two`; evidence is the semantic postcondition in `hax-extraction-boundary.md`. | remove when hax/F* support library accepts this definition directly | security/crypto/formal | assumed |
| `impl__starts_with` shim preserves Rust slice semantics | formal verification owner | Required because pinned hax support output is not directly accepted by pinned F*. | Compatibility shim, not an implementation property. | Postcondition: shim truth value equals Rust slice `starts_with`; evidence is the semantic postcondition in `hax-extraction-boundary.md`. | remove when hax/F* support library accepts this definition directly | security/crypto/formal | assumed |
| recorded hax support shims preserve Rust support-library semantics | formal verification owner | Required because pinned hax support output omits or cannot directly discharge several byte-framing, slice-range, array-conversion, integer-roundtrip, and checked-arithmetic facts. | Compatibility shims, not implementation properties. | Postcondition: each appended support shim matches the Rust support-library fact named in `hax-extraction-boundary.md`; evidence is the per-shim semantic postconditions for all shims appended by `scripts/snarkpack-formal.sh`. | remove each shim when hax/F* support libraries expose an accepted definition or lemma | security/crypto/formal | assumed |
| decaf377 group, field, and encoding behavior | proof-aggregation maintainers | The production and reference crates depend on decaf377 curve, field, and encoding behavior. | Full decaf377 backend verification is outside this scope. | Postcondition: decaf377 group/field arithmetic and encodings used by aggregation match the backend assumptions; evidence is `decaf377_vk_digest_round_trips_after_serialization`, `srs_id_is_stable`, `reference_srs_matches_public_production_id`, arkworks subgroup/serialization tests, and production/reference parity tests. | verified curve/encoding backend or external audit artifact | security/crypto | assumed |

## Arkworks Boundary Test Obligations

These are evidence obligations, not proofs. They narrow the arkworks
implementation assumptions above.

- compressed G1 deserialization rejects non-subgroup encodings
- compressed G2 deserialization rejects non-subgroup encodings
- identity points round-trip according to arkworks documented semantics
- torsion-injection fixtures reject for G1 and G2
- malformed compressed bytes reject
- valid G1/G2 points serialize and deserialize round trip
- verifying key digest is stable under serialize/deserialize
- SRS id is stable under serialize/deserialize
- MSM with zero scalars matches naive linear combination
- MSM with identity elements matches naive linear combination
- MSM on small random vectors matches naive linear combination

Implemented test evidence:
`arkworks_pairing_identity_and_generator_consistency`,
`arkworks_msm_boundary_zero_scalar_identity_and_random_parity`,
`arkworks_g1_g2_compressed_round_trip_and_identity`,
`arkworks_g1_g2_malformed_compressed_bytes_reject`,
`arkworks_g1_g2_subgroup_and_torsion_rejection`,
`decaf377_vk_digest_round_trips_after_serialization`,
`srs_id_is_stable`, and `reference_srs_matches_public_production_id`.

## Hax Extraction Discipline

The current extracted target list is
`crates/crypto/proof-aggregation/formal/snarkpack/hax-targets.txt`. Per-target
features, preconditions, arithmetic mode, control-flow forms, panics, unsafe,
and support shims are recorded in
`crates/crypto/proof-aggregation/formal/snarkpack/hax-extraction-boundary.md`.

Unrecorded `assume val`, `admit`, `--admit_smt_queries`, duplicate
formal-only encoders, tuple/default `ChallengeContext` constructors, and
unmapped RIPP refinement symbols are rejected by `just snarkpack-invariants`.

## Soundness Assumption And Differential Conformance

End-to-end formal verification is **out of scope**. SnarkPack/RIPP/Groth16
algebraic soundness is a standing assumption, inherited from the published paper
and the Filecoin (Bellperson v0.21.0) implementation, both assumed sound. There
is no Lean algebraic proof and no EasyCrypt Fiat-Shamir proof.

Instead, algebraic/transcript conformance is **exhaustively cross-checked over a
bounded domain** by Stage 9 (security.md): an independent, hand-built Lean model of
the transcript and folding discipline, differentially tested against the Rust by
enumerating every distinct transcript shape (one per power of two up to the SRS
max) rather than sampling. This is evidence, not proof (bounded by the SRS max,
algebra abstract), and is non-blocking. F* via hax remains the executed-Rust
implementation-boundary proof backend and stays a completion blocker.

## Gates

Run `just snarkpack-formal` for the formal gate. It checks the pinned toolchain,
hax extraction, F* module imports, smoke bindings to extracted functions, and
proved rows above. The SnarkPack proof files are checked without
`--admit_smt_queries`.

The clean-image `.github/workflows/snarkpack-formal.yml` job installs the pinned
Z3, F*, and hax versions from `toolchain.toml`, runs `just snarkpack-formal`,
then runs `just snarkpack-invariants`, `just snarkpack-fuzz-smoke`,
`just snarkpack-filecoin-shape`, `just snarkpack-dos-gate`, and
`just snarkpack-lean-conformance`. Keep the full
formal gate out of default `just check` unless it satisfies the default CI
runtime policy; it remains a required SnarkPack workflow gate.
