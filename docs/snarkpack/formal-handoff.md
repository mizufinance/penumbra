# SnarkPack Formal Handoff

Status: Stage 4 implementation-boundary F* rows are complete for the current
extracted Rust target set. The full SnarkPack campaign remains open until the
RIPP refinement, adaptation, assumption, reference, trace, testing, performance,
and review rows leave `open`.

Evidence statuses:

- `proved`: mechanically checked in F* against hax-extracted executed Rust.
- `refined`: reviewed against the published algorithm, backed by tests and
  signed review.
- `composed`: enforced by Rust types plus proved/refined pieces, tests, and
  invariant guards.
- `assumed`: explicit external/tool/cryptographic assumption with owner,
  rationale, supporting evidence, and removal path.
- `open`: completion blocker.

Pinned tools: hax `v0.3.7`, F* `v2026.05.24`, Rust `1.89`, OCaml `5.1.1`,
Z3 `4.14.1`, OPAM switch `hax-0.3.7`. Any hax/F*/OCaml/Z3/Rust pin change
requires rerunning `just snarkpack-formal`, reviewing generated extraction
diffs and support shims, updating the verification marker, and refreshing the
proof artifact stamp.

Proof artifact stamp: sha256:eace5d0369d45ae632562b9f3d127ab2f7f9970660b26599af6f888bfac43523

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
`composed`. If future changes reopen that row, the digest reduction row is also
reopened. While full statement encoding injectivity is `open`, these dependent
rows must stay `open` or carry an explicit blocked-by-injectivity note before
review:

- statement digest equality reduces to canonical statement equality
- challenge context constructor derives from statement digest
- wrapper digest mismatch rejects before inner exposure, for semantic statement
  binding beyond the wrapper byte check
- aggregate backend receives only preflighted bytes, for the statement-binding
  reduction beyond typed routing
- app-level aggregate composition, for the statement-binding reduction beyond
  recomputed app artifacts

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
| local RIPP implementation maps to intended algorithm | `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src` | proof-relevant RIPP symbols | refinement map plus tests/review | `docs/snarkpack/ripp-refinement.md` | all scoped rows refined/proved-equivalent/assumed | open | n/a | not yet reviewed |

## Assumptions

| Assumption | Owner | Rationale | Why not proved here | Supporting evidence | Removal path | Required signoff | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SHA-256 collision resistance | cryptography lead | Statement digest binding reduces to this after encoding injectivity is proved. | External cryptographic primitive assumption. | standard SHA-256 analysis; fixed domain prefixes | replace primitive or mechanize external proof only in research track | security/crypto | assumed |
| SHA-256 preimage resistance | cryptography lead | Challenge context and wrapper digests use SHA-256-derived commitments. | External cryptographic primitive assumption. | standard SHA-256 analysis; fixed domain prefixes | replace primitive or mechanize external proof only in research track | security/crypto | assumed |
| Domain separation by fixed distinct prefixes | proof-aggregation maintainers | Separate statement digest, challenge context, challenge preimage, VK digest, and wrapper domains. | Reduces to fixed-prefix review plus hash assumptions. | golden-layout tests and invariant review | prove prefix disjointness mechanically if needed | security/crypto | assumed |
| abstract Groth16 soundness | cryptography lead | Aggregate verification ultimately depends on Groth16 proof soundness. | Out of implementation-boundary FV scope. | published Groth16 proofs and existing Penumbra circuit audits | research-grade cryptographic proof project | security/crypto | assumed |
| abstract RIPP/GIPA/TIPA/SnarkPack algebraic soundness | cryptography lead | Local implementation is reviewed against the algorithm, but algebraic soundness is external. | Requires protocol proof over algebraic model. | published SnarkPack/RIPP proof material; `ripp-refinement.md` | Lean/EasyCrypt research track | security/crypto | assumed |
| arkworks field/group/pairing mathematical operation implementations | proof-aggregation maintainers | The implementation calls arkworks arithmetic primitives. | Full library verification is outside this campaign. | upstream tests plus planned boundary property tests | verified arithmetic backend or external audit artifact | security/crypto | assumed |
| arkworks MSM implementation computes intended linear combination | proof-aggregation maintainers | MSM is an implementation-heavy dependency, not a pure algebra axiom. | Full arkworks MSM verification is outside this campaign. | required zero-scalar, identity, and random-vector parity tests | verified MSM or external audit artifact | security/crypto | assumed |
| arkworks serialization and subgroup behavior | proof-aggregation maintainers | SRS, VK, proof bytes, and digests depend on arkworks encoding checks. | Full serialization/subgroup proof is outside this campaign. | required G1/G2 subgroup, torsion, malformed-byte, and round-trip tests | verified serialization backend or external audit artifact | security/crypto | assumed |
| hax extraction preserves modeled Rust semantics for the extracted safe subset | formal verification owner | F* proofs are over hax output. | hax semantic preservation is not proved inside this repo. | `hax-extraction-boundary.md`, pinned versions, invariant guards | upstream hax soundness proof or independent translation validation | security/crypto/formal | assumed |
| `impl_u32__is_power_of_two` shim preserves Rust semantics | formal verification owner | Required because pinned hax support output is not directly accepted by pinned F*. | Compatibility shim, not an implementation property. | `hax-extraction-boundary.md` semantic postcondition | remove when hax/F* support library accepts this definition directly | security/crypto/formal | assumed |
| `impl__starts_with` shim preserves Rust slice semantics | formal verification owner | Required because pinned hax support output is not directly accepted by pinned F*. | Compatibility shim, not an implementation property. | `hax-extraction-boundary.md` semantic postcondition | remove when hax/F* support library accepts this definition directly | security/crypto/formal | assumed |
| recorded hax support shims preserve Rust support-library semantics | formal verification owner | Required because pinned hax support output omits or cannot directly discharge several byte-framing, slice-range, array-conversion, integer-roundtrip, and checked-arithmetic facts. | Compatibility shims, not implementation properties. | `hax-extraction-boundary.md` semantic postconditions for every support shim appended by `scripts/snarkpack-formal.sh` | remove each shim when hax/F* support libraries expose an accepted definition or lemma | security/crypto/formal | assumed |
| decaf377 group, field, and encoding behavior | proof-aggregation maintainers | The production and reference crates depend on decaf377 curve, field, and encoding behavior. | Full decaf377 backend verification is outside this campaign. | boundary tests, subgroup/serialization tests, SRS/VK digest stability tests, and reference/prod parity tests | verified curve/encoding backend or external audit artifact | security/crypto | assumed |

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

## Hax Extraction Discipline

The current extracted target list is
`crates/crypto/proof-aggregation/formal/snarkpack/hax-targets.txt`. Per-target
features, preconditions, arithmetic mode, control-flow forms, panics, unsafe,
and support shims are recorded in
`crates/crypto/proof-aggregation/formal/snarkpack/hax-extraction-boundary.md`.

Unrecorded `assume val`, `admit`, `--admit_smt_queries`, duplicate
formal-only encoders, tuple/default `ChallengeContext` constructors, and
unmapped RIPP refinement symbols are rejected by `just snarkpack-invariants`.

## Research Track

The end-to-end cryptographic proof is a separate research-grade project, not a
larger hax extraction target:

- Lean 4 for the algebraic protocol model: Groth16 aggregation, RIPP, GIPA,
  TIPA, commitments, pairings, and reduction invariants.
- EasyCrypt for Fiat-Shamir/random-oracle games and transcript-binding
  reductions.
- F* via hax for executed Rust implementation-boundary proofs.
- Coq as fallback only if Lean 4 cannot support the algebraic model cleanly.

Open the research track only after the implementation-boundary campaign is
complete or if an external audit/soundness issue requires it earlier.

## Gates

Run `just snarkpack-formal` for the formal gate. It checks the pinned toolchain,
hax extraction, F* module imports, smoke bindings to extracted functions, and
proved rows above. The SnarkPack proof files are checked without
`--admit_smt_queries`.

`just snarkpack-formal` must pass on the clean-image `snarkpack-formal` CI
workflow before this phase is considered reproducible. Keep it out of default
`just check` until it satisfies the default CI runtime policy. If the full
formal gate remains outside that policy, only the fast proved
implementation-boundary subset may enter `just check`; the full proof gate must
remain in nightly CI with excluded rows listed here.
