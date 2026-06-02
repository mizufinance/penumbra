# SnarkPack Filecoin-to-Penumbra Adaptation Register

Status: open coverage artifact. This document records intentional differences
between Filecoin SnarkPack v2 and Penumbra's local SnarkPack/RIPP backend.

Filecoin v2 is used as the reference for Fiat-Shamir omission/reordering bug
classes and transcript discipline. Penumbra's local spec remains authoritative
for BLS12-377, statement binding, padding, SRS/VK binding, and aggregate-bundle
integration. No cross-curve byte-level equivalence to Filecoin is claimed.

Every non-comment row in
`crates/crypto/proof-aggregation/formal/snarkpack/adaptation-scope.txt` must
appear exactly once in the table below.

No `filecoin-shape` row can leave `open` until `filecoin.normative-pin` is
reviewed. The normative SnarkPack-shape source is Bellperson `v0.21.0`;
rust-fil-proofs `filecoin-proofs-v11.1.0` is production-consumer evidence that
this Bellperson version shipped through Filecoin Network v16 Skyr. Older
Bellperson pins are historical comparison aids only.

Comparison levels:

- `penumbra-byte`: exact Penumbra bytes must match between Penumbra reference
  and optimized paths.
- `abstract-trace`: typed event order, dependencies, and equation roles must
  match the Penumbra spec.
- `filecoin-shape`: abstract dependency shape is compared against Filecoin v2
  bug classes only.
- `penumbra-local`: Penumbra-only integration behavior with no Filecoin
  equivalent.
- `assumption`: accepted dependency, tracked through the assumption register.

Deviation classes:

- `mechanical`: naming, type shape, plumbing, or equivalent representation.
- `performance`: same semantics, different execution strategy.
- `security-binding`: transcript, statement, SRS, VK, domain, hash, or
  public-input binding.
- `semantic`: may change accepted/rejected proofs or verifier equations.

Tests are evidence, not proof or review. A `security-binding` or `semantic`
adaptation cannot leave `open` solely because tests pass.

## Coverage

| id | Filecoin behavior/source | Penumbra behavior/source | primary comparison level | deviation class | security impact | required evidence | status | reviewer | date |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `filecoin.bellperson.repo` | Bellperson repository `https://github.com/filecoin-project/bellperson` | Used only as pinned Filecoin v2 transcript-discipline source material. | `filecoin-shape` | `mechanical` | Prevents drifting to an unpinned Filecoin reference. | Source pin review. | `open` | pending | pending |
| `filecoin.normative-pin` | Bellperson tag `v0.21.0`, peeled commit `62c362fd46ca2139747b8770bae53ce6f1e42bb1`. | Normative source for Filecoin v2 SnarkPack transcript, GIPA/TIPA, and aggregate-verifier shape checks. | `filecoin-shape` | `mechanical` | Prevents ambiguous Filecoin-shape comparisons. | Source pin review plus rust-fil-proofs consumer evidence. | `refined` | user-approved | 2026-05-28 |
| `filecoin.bellperson.v0.21.0` | Bellperson tag `v0.21.0`, peeled commit `62c362fd46ca2139747b8770bae53ce6f1e42bb1`. | Used as the normative SnarkPack-shape source material. | `filecoin-shape` | `mechanical` | Pins transcript and aggregate-verifier shape review to immutable source. | Source pin review. | `refined` | user-approved | 2026-05-28 |
| `filecoin.rust-fil-proofs.v11.1.0` | rust-fil-proofs tag `filecoin-proofs-v11.1.0`, commit `004d7b4244c469e0d9aeebf15f9a81ef60308ba3`, consuming Bellperson `0.21.0`. | Production-consumer evidence for the Filecoin Network v16 Skyr SnarkPack v2 release. | `filecoin-shape` | `mechanical` | Confirms the normative Bellperson pin is tied to the shipped Filecoin proofs integration. | Source pin review. | `refined` | user-approved | 2026-05-28 |
| `filecoin.bellperson.release-branch` | Bellperson release branch commit `ff5f39e43cc62481cc575adae628cb7d1124bce8`. | Historical comparison aid only. | `filecoin-shape` | `mechanical` | Prevents accidental use of a moving branch as normative. | Source pin review. | `open` | pending | pending |
| `filecoin.bellperson.v0.18.2` | Bellperson tag `v0.18.2` peeled commit `c5fa04be1824ceb19a96a36ee1689f9d15b2e864`. | Historical comparison aid only. | `filecoin-shape` | `mechanical` | Prevents older tag-family references from becoming normative by implication. | Source pin review. | `open` | pending | pending |
| `curve.bls12-377.backend` | Filecoin Bellperson uses the Filecoin BLS12-381 stack. | Backend aggregate calls instantiate `Bls12_377`. | `assumption` | `semantic` | Curve swap changes field, group, pairing, and encoding semantics. | Arkworks boundary tests and algebraic assumption signoff. | `open` | pending | pending |
| `curve.bls12-377.srs` | Filecoin SRS material is BLS12-381-specific. | Dev SRS generation and verifier SRS use `Bls12_377`. | `assumption` | `semantic` | SRS group material is curve-specific and consensus-sensitive once productionized. | SRS id tests, serialization tests, and SRS assumption signoff. | `open` | pending | pending |
| `curve.field.public-input` | Filecoin public scalar representation is curve-specific. | Statement and public input rows use `Fq`. | `penumbra-byte` | `semantic` | Public input encoding is statement-binding material. | Statement encoding F* rows and public-input mutation tests. | `open` | pending | pending |
| `hash.statement-digest` | Filecoin transcript binding uses its production hash/framing choices. | Penumbra statement digest uses `STATEMENT_DIGEST_DOMAIN` and SHA-256. | `penumbra-byte` | `security-binding` | Statement collision or ambiguity can bind a proof to the wrong aggregate statement. | Statement encoding injectivity proof and SHA-256 assumption row. | `open` | pending | pending |
| `hash.challenge-context` | Filecoin v2 fixes transcript context omissions in its own framing. | Penumbra derives `ChallengeContext` from a statement digest domain. | `penumbra-byte` | `security-binding` | Missing context would reopen transcript-substitution attacks. | Challenge preimage F* row and constructor invariant guard. | `open` | pending | pending |
| `hash.challenge-preimage` | Filecoin v2 transcript bug class requires complete ordered challenge input. | Penumbra frames domain, stage, context, nonce, and messages. | `penumbra-byte` | `security-binding` | Missing or reorderable challenge bytes can reproduce Fiat-Shamir bugs. | Challenge layout proof and prover/verifier trace parity. | `open` | pending | pending |
| `hash.family-transcript` | Filecoin transcript domains are product-specific. | Penumbra family transcript domains are generated by `transcript_family_domain`. | `penumbra-byte` | `security-binding` | Family collision could verify one family aggregate as another. | Domain uniqueness test and family-mutation tests. | `open` | pending | pending |
| `serialization.aggregate-proof` | Filecoin aggregate proof bytes follow its Bellperson serialization stack. | Penumbra decodes and verifies arkworks compressed aggregate proof bytes. | `penumbra-byte` | `security-binding` | Malformed proof bytes must reject before unsafe or expensive behavior. | Wrapper F* rows, malformed-byte tests, subgroup/serialization tests. | `open` | pending | pending |
| `test-surface.deserialize-fuzz-helper` | Filecoin does not expose a comparable test harness surface. | Penumbra exposes feature-gated aggregate-proof deserialization to the fuzz crate only. | `penumbra-local` | `mechanical` | Fuzzing must exercise the arkworks byte boundary without making the helper part of the default API. | Feature-gated API review, cargo-fuzz build, and fuzz-crate invariant guard. | `open` | pending | pending |
| `serialization.vk-digest` | Filecoin VK material and digesting are product-specific. | Penumbra digests `PreparedVerifyingKey` compressed serialization. | `penumbra-byte` | `security-binding` | Wrong VK binding can verify proofs under the wrong family key. | VK digest mutation test and serialization stability tests. | `open` | pending | pending |
| `serialization.public-input-fields` | Filecoin public input byte layout is curve-specific. | Penumbra serializes each public input field with canonical compressed bytes. | `penumbra-byte` | `security-binding` | Ambiguous field bytes can break statement injectivity. | Statement encoding injectivity proof and field framing tests. | `open` | pending | pending |
| `srs.dev-shape` | Filecoin production SRS shape is not reused. | `DevSrs` bounds the prototype SRS size and identity. | `penumbra-local` | `security-binding` | Unsupported SRS sizes must not reach backend verification. | SRS count tests and preflight checks. | `open` | pending | pending |
| `srs.identity` | Filecoin SRS identity is product-specific. | Penumbra computes `srs_id` from backend, curve, max count, and serialized SRS. | `penumbra-byte` | `security-binding` | Wrong SRS identity can bind proof bytes to different setup material. | SRS id tests and aggregate SRS mismatch tests. | `open` | pending | pending |
| `srs.inner-product` | Filecoin inner-product setup is curve and product specific. | Penumbra slices a generated inner-product SRS for the padded count. | `abstract-trace` | `semantic` | Wrong powers or lengths change verifier equations. | SRS dimension tests and RIPP refinement review. | `open` | pending | pending |
| `statement.canonical-binding` | Filecoin statement material is not Penumbra's app statement. | `AggregateStatement::new` binds version, curve, backend, family, SRS, VK, counts, padding, and public inputs. | `penumbra-byte` | `security-binding` | Any omitted field can permit substitution or replay across families. | Statement F* rows and mutation tests. | `open` | pending | pending |
| `statement.wrapper` | Filecoin aggregate proof envelope is product-specific. | Penumbra wrapper binds the aggregate proof bytes to the statement digest. | `penumbra-byte` | `security-binding` | Unwrapped or mismatched proof bytes can bypass statement binding. | Wrapper F* rows and malformed wrapper tests. | `open` | pending | pending |
| `padding.repeat-final` | Filecoin proof-count/padding conventions are not the Penumbra statement rule. | Penumbra pads by repeating the final real row and binds the rule by domain. | `penumbra-byte` | `semantic` | Non-canonical padding can alter the accepted language or hide omitted proofs. | Padding F* row and repeat-final tests. | `open` | pending | pending |
| `family.identifiers` | Filecoin proof family routing does not match Penumbra action families. | `ProofFamilyId` encodes transfer, consolidate, split, and shielded ICS20 withdrawal families. | `penumbra-local` | `security-binding` | Wrong family routing can verify one action family with another VK. | Family mutation tests and app ordering checks. | `open` | pending | pending |
| `bundle.proto-shape` | Filecoin aggregate bundle wire shape is not reused. | Penumbra `AggregateBundle` protobuf/domain type carries version, SRS id, and family aggregates. | `penumbra-local` | `security-binding` | Malformed bundle metadata can bypass app-level verification. | Proto/domain tests and fuzzing harness. | `open` | pending | pending |
| `app.bundle-routing` | Filecoin block integration is product-specific. | Penumbra verifies aggregate bundles against recomputed proposal artifacts. | `penumbra-local` | `security-binding` | App integration must not accept missing, reordered, or extra family aggregates. | App aggregate bundle tests and segment coverage tests. | `open` | pending | pending |
| `app.action-rejection` | Filecoin action model is not reused. | Penumbra rejects aggregate bundles from generic action handling paths. | `penumbra-local` | `security-binding` | User-submitted aggregate bundles must not execute outside proposal aggregation. | Action-handler rejection tests. | `open` | pending | pending |
| `backend.preflight` | Filecoin backend preflight shape is not reused. | Penumbra backend receives typed preflighted calls only. | `penumbra-local` | `security-binding` | Raw backend calls must not bypass statement, SRS, VK, size, or wrapper checks. | Invariant guard and preflight mutation tests. | `open` | pending | pending |
| `backend.digest-dispatch` | Filecoin transcript digest dispatch is product-specific. | Penumbra dispatches family-specific transcript digests after typed preflight. | `penumbra-byte` | `security-binding` | Wrong digest dispatch can collide family transcript domains. | Domain uniqueness and wrong-family tests. | `open` | pending | pending |
| `ripp.prepared-srs` | Filecoin optimization strategy is not directly reused. | Penumbra has prepared SRS paths for proving. | `abstract-trace` | `semantic` | Prepared views must preserve the same commitment-key equations. | Specialized-vs-generic parity tests plus equation review. | `open` | pending | pending |
| `ripp.shifted-srs-ab` | Filecoin shifted-SRS implementation details are curve/backend-specific. | Penumbra uses a prepared shifted SRS path for AB TIPA. | `abstract-trace` | `semantic` | Shift or inverse-power mismatch changes TIPA equations. | Shifted-SRS parity tests and refinement review. | `open` | pending | pending |
| `ripp.kzg-openings` | Filecoin KZG opening implementation is backend-specific. | Penumbra verifies G1/G2 commitment-key KZG openings through arkworks pairings. | `abstract-trace` | `semantic` | Wrong source group or opening equation breaks reduced relation checks. | KZG equation review and mutation tests. | `open` | pending | pending |
| `ripp.public-input-folding` | Filecoin public input folding does not match Penumbra families. | Penumbra folds public inputs into the Groth16 aggregate equation. | `abstract-trace` | `semantic` | Wrong folding order can accept wrong public inputs. | Public-input mutation tests and refinement review. | `open` | pending | pending |
| `ripp.randomizer` | Filecoin randomizer framing is product-specific. | Penumbra derives aggregate randomizer from statement-bound transcript context. | `penumbra-byte` | `security-binding` | Missing randomizer dependencies can recreate Fiat-Shamir omission bugs. | Challenge trace parity and Filecoin bug-class review. | `open` | pending | pending |
| `arkworks.subgroup-rejection-g1` | Filecoin uses a different curve/encoding implementation stack. | Penumbra relies on arkworks compressed G1 deserialization semantics. | `assumption` | `semantic` | Non-subgroup G1 encodings must not enter verified aggregate material. | G1 subgroup rejection fixtures and arkworks serialization assumption signoff. | `open` | pending | pending |
| `arkworks.subgroup-rejection-g2` | Filecoin uses a different curve/encoding implementation stack. | Penumbra relies on arkworks compressed G2 deserialization semantics. | `assumption` | `semantic` | Non-subgroup G2 encodings must not enter verified aggregate material. | G2 subgroup rejection fixtures and arkworks serialization assumption signoff. | `open` | pending | pending |
| `arkworks.identity-semantics` | Filecoin uses a different curve/encoding implementation stack. | Penumbra relies on arkworks identity point serialization/deserialization semantics. | `assumption` | `semantic` | Identity handling must match the intended verifier equations and serialization rules. | Identity round-trip fixtures for G1/G2 and arkworks serialization assumption signoff. | `open` | pending | pending |
| `arkworks.torsion-rejection` | Filecoin uses a different curve/encoding implementation stack. | Penumbra relies on arkworks subgroup checks rejecting torsion-injection fixtures. | `assumption` | `semantic` | Torsion points must not satisfy aggregate verifier equations through malformed encodings. | Torsion-injection fixtures for G1/G2 and arkworks serialization assumption signoff. | `open` | pending | pending |
| `arkworks.msm-boundary` | Filecoin uses a different curve/MSM implementation stack. | Penumbra relies on arkworks `G::msm` for KZG opening computation. | `assumption` | `semantic` | MSM implementation errors can falsify commitment equations. | MSM boundary tests and arkworks assumption signoff. | `open` | pending | pending |

## Scope Derivation

The adaptation scope is based on a grep pass over these families:

- curve/backend identifiers: `Bls12_377`, `decaf377`, `Fq`, pairing aliases
- hash/transcript calls: `Sha256`, `Digest`, challenge helpers, statement digest
  helpers, family transcript domains
- serialization: `serialize_compressed`, `deserialize_compressed`, canonical
  serialization/deserialization traits
- SRS/VK references: `SRS`, `DevSrs`, `srs_id`, `PreparedVerifyingKey`,
  `vk_digest`, verifier-key digest helpers
- public input and padding paths
- aggregate proof wrapper and app-level aggregate bundle routing

The invariant script checks that every scoped touch point maps to exactly one
register row and that every register row is scoped.
