# SnarkPack RIPP Review Spec

Status: Penumbra-local implementation spec, checked against Filecoin v2
transcript bug classes and used by `docs/snarkpack/ripp-refinement.md`.
This is not a mechanized proof and not an independently invented SnarkPack
specification.

In this document, "RIPP" means the local proof stack under
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src`: GIPA, TIPA,
TIPA-with-structured-scalar-message, and the Groth16 aggregation adapter.

Filecoin SnarkPack v2 is the reference for Fiat-Shamir omission/reordering bug
classes and transcript discipline. The normative SnarkPack-shape source is
Bellperson `v0.21.0` at peeled commit
`62c362fd46ca2139747b8770bae53ce6f1e42bb1`; rust-fil-proofs
`filecoin-proofs-v11.1.0` at commit
`004d7b4244c469e0d9aeebf15f9a81ef60308ba3` is production-consumer evidence
for the Filecoin Network v16 Skyr release. The Penumbra-local spec is
authoritative for BLS12-377, hash/domain choices, statement binding, padding,
SRS/VK binding, and aggregate-bundle integration. No cross-curve byte-level
equivalence to Filecoin is claimed.

## Comparison Levels

Every spec row/event has exactly one primary required comparison level.
Secondary checks can provide evidence, but they do not satisfy the row's gate.
The shared trace policy table in
`crates/crypto/proof-aggregation-trace-schema` must match the Spec Row Index
below.

- `penumbra-byte`: exact bytes hashed or serialized by Penumbra; required
  between Penumbra reference and optimized paths.
- `abstract-trace`: typed event order, labels, challenge dependencies, round
  schedule, object roles, and verifier equation roles.
- `filecoin-shape`: abstract dependency-shape comparison against Filecoin v2
  bug classes only.
- `penumbra-local`: Penumbra-only integration behavior with no Filecoin
  comparison target.

Default primary-level policy:

- Penumbra challenge preimages, statement bytes, wrapper bytes, SRS/VK digest
  inputs, and public-input framing use `penumbra-byte`.
- GIPA/TIPA/Groth16 round structure and verifier equation roles use
  `abstract-trace` unless exact Penumbra bytes are hashed at that step.
- Filecoin-specific bug-class checks use `filecoin-shape`.
- Penumbra-only app integration and aggregate-bundle routing use
  `penumbra-local` evidence in the refinement/adaptation maps.

Rows whose primary level is `abstract-trace` may consume objects whose byte
binding is established by upstream `penumbra-byte` rows. In that case, the
abstract row gates only the equation or event role; the upstream byte rows gate
canonical framing and byte-to-object binding.

## Spec Row Index

| spec_row_id | source basis | primary_required_comparison_level | required evidence |
| --- | --- | --- | --- |
| `fs.context-constructor` | Penumbra adaptation; Filecoin v2 transcript context bug class | `penumbra-byte` | F* challenge-context row; invariant guard against alternate constructors |
| `fs.challenge-preimage` | Penumbra adaptation; Filecoin v2 ordered transcript-input bug class | `penumbra-byte` | F* challenge-preimage row; golden layout test |
| `fs.stage-labels` | Penumbra adaptation; Filecoin v2 domain-separation discipline | `penumbra-byte` | prover/verifier trace parity and stage-label review |
| `fs.filecoin-bug-class` | Bellperson `v0.21.0` Filecoin v2 transcript hardening, with rust-fil-proofs `filecoin-proofs-v11.1.0` as production-consumer evidence | `filecoin-shape` | manual review against pinned Filecoin v2 reference |
| `gipa.input-relation` | paper algebra and local implementation | `abstract-trace` | equation review and unit/property evidence |
| `gipa.round-folding` | paper algebra and local implementation | `abstract-trace` | equation review and prover/verifier trace evidence |
| `gipa.challenge-dependency` | Penumbra challenge helper and Filecoin v2 transcript-input discipline | `penumbra-byte` | trace parity over exact Penumbra challenge bytes |
| `gipa.verifier-folding` | paper algebra and local implementation | `abstract-trace` | equation review and mutation rejection evidence |
| `tipa.srs` | paper algebra and Penumbra SRS adaptation | `abstract-trace` | SRS dimension tests and refinement review |
| `tipa.ab.gipa` | paper algebra and local implementation | `abstract-trace` | GIPA trace/equation evidence |
| `tipa.ab.kzg-challenge` | Penumbra challenge helper and Filecoin v2 transcript-input discipline | `penumbra-byte` | trace parity over exact Penumbra KZG challenge bytes |
| `tipa.ab.kzg-equations` | paper algebra and local implementation | `abstract-trace` | KZG equation review and mutation tests |
| `ssm.power-sequence` | paper algebra and local implementation | `abstract-trace` | structured-power tests and equation review |
| `ssm.kzg-challenge` | Penumbra challenge helper and Filecoin v2 transcript-input discipline | `penumbra-byte` | trace parity over exact Penumbra C-path challenge bytes |
| `ssm.base-equation` | paper algebra and local implementation | `abstract-trace` | equation review and mutation tests |
| `groth16.randomizer` | Penumbra challenge helper and Filecoin v2 final-randomness bug class | `penumbra-byte` | randomizer trace parity and Filecoin bug-class review |
| `groth16.folded-inputs` | Penumbra public-input adaptation and paper algebra; byte binding is covered by `curve.field.public-input` and `serialization.public-input-fields` adaptation rows | `abstract-trace` | public-input mutation tests and equation review |
| `groth16.ppe` | paper algebra and local implementation | `abstract-trace` | PPE mutation tests and equation review |

## What Is Formally Verified Today

The current hax/F* work proves implementation-boundary properties, not RIPP
algorithm correctness.

Mechanically proved rows today:

- `validate_counts` rejects zero real count.
- statement byte-field framing and full `encode_statement` injectivity.
- statement digest equality reduction to canonical statement equality, modulo
  the SHA-256 collision-resistance assumption.
- full count, row-arity, and repeat-final-padding validation iff lemmas.
- wrapper decode rejects oversized bytes before parsing or exposing the inner
  proof range.
- wrapper round trip returns the exact inner range, and digest mismatch rejects
  before successful inner-range exposure.
- challenge context layout and challenge preimage layout/injectivity.

Composed/type-checked rows today:

- `ChallengeContext` has no production default, tuple constructor, or TLS
  fallback.
- aggregate backend verification receives only typed preflighted proof bytes.
- app-level aggregate bundle flow reaches typed aggregate preflight.

There are no remaining open Stage 4 implementation-boundary F* rows for the
current extracted Rust target set. RIPP/GIPA/TIPA algorithm fidelity is not
formally proved; it is covered by the refinement review map.

## Review Method

For each `symbol_id` in `ripp-refinement-scope.txt`, verify four facts:

1. The local function implements the spec step listed here.
2. Prover and verifier use the same transcript inputs, labels, nonce rules, and
   challenge conversion.
3. Any performance specialization is equation-preserving.
4. Any security-binding or semantic deviation is either mechanically
   `proved-equivalent` or explicitly accepted as an `assumed` row by
   security/crypto review.

Do not mark a row `refined` just because tests pass. Tests are evidence for a
review conclusion; they are not a replacement for checking the equations.
Security-binding or semantic rows cannot become `refined` solely from tests.

## Fiat-Shamir Challenge Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs`.

Source basis: Penumbra adaptation plus Filecoin v2 transcript omission and
ordering bug classes.

Primary comparison levels:

- `fs.context-constructor`: `penumbra-byte`
- `fs.challenge-preimage`: `penumbra-byte`
- `fs.stage-labels`: `penumbra-byte`
- `fs.filecoin-bug-class`: `filecoin-shape`

Context:

- `ChallengeContext = SHA256("penumbra.snarkpack.challenge_context.v1\0" ||
  statement_digest)`.
- The context has no public constructor except `from_statement_digest`.

Challenge preimage:

```text
"penumbra.snarkpack.challenge.v1\0"
|| u32_le(stage_label.len())
|| stage_label
|| challenge_context[32]
|| u64_le(nonce)
|| messages
```

Required checks:

- no call site can omit `ChallengeContext`
- no thread-local fallback exists
- nonce starts at `0` and increments only when challenge decoding fails
- stage labels are stable:
  - `aggregate.randomizer`
  - `tipa.ab.gipa.round`
  - `tipa.ab.kzg`
  - `tipa.c.gipa.round`
  - `tipa.c.kzg`
- prover and verifier traces are byte-identical for accepted proofs

Filecoin v2 bug-class checklist:

- no omitted first-round transcript hash
- no omitted final randomness dependency
- no reorderable public messages
- no hidden/default transcript context
- prover and verifier consume equivalent challenge inputs

## GIPA Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs`.

Source basis: paper algebra plus local implementation. Fiat-Shamir dependency
rows additionally import Filecoin v2 transcript discipline.

Primary comparison levels:

- `gipa.input-relation`: `abstract-trace`
- `gipa.round-folding`: `abstract-trace`
- `gipa.challenge-dependency`: `penumbra-byte`
- `gipa.verifier-folding`: `abstract-trace`

Input relation:

```text
IP(a, b) = t
ComA = CommitA(ck_a, a)
ComB = CommitB(ck_b, b)
ComT = CommitT(ck_t, t)
len(a) = len(b) = len(ck_a) = len(ck_b) = power_of_two
```

Prover round for `n > 1`, using local split conventions:

```text
split = n / 2
a1 = a[split..]       a2 = a[..split]
b1 = b[..split]       b2 = b[split..]
ck_a1 = ck_a[..split] ck_a2 = ck_a[split..]
ck_b1 = ck_b[split..] ck_b2 = ck_b[..split]

L = (
  CommitA(ck_a1, a1),
  CommitB(ck_b1, b1),
  CommitT(ck_t, IP(a1, b1))
)

R = (
  CommitA(ck_a2, a2),
  CommitB(ck_b2, b2),
  CommitT(ck_t, IP(a2, b2))
)
```

Challenge conversion:

```text
x = scalar_from_first_128_bits_be(challenge_digest(...))
require x != 0
c = x^-1
c_inv = x
```

The swap is a local convention used to keep one folded side cheap for
multiexponentiation. It is acceptable only if prover and verifier use the same
convention.

Fold:

```text
a'    = c     * a1    + a2
b'    = c_inv * b2    + b1
ck_a' = c_inv * ck_a2 + ck_a1
ck_b' = c     * ck_b1 + ck_b2
```

The prover records `(L, R)` each round, then reverses the proof round list and
challenge transcript before returning.

Verifier:

```text
for proof rounds in reverse proof order:
  recompute c, c_inv from prior transcript value and (L, R)
  ComA = c * L.ComA + ComA + c_inv * R.ComA
  ComB = c * L.ComB + ComB + c_inv * R.ComB
  ComT = c * L.ComT + ComT + c_inv * R.ComT

derive final ck_a_base, ck_b_base from the transcript exponents
verify:
  CommitA(ck_a_base, [a_base]) == ComA
  CommitB(ck_b_base, [b_base]) == ComB
  CommitT(ck_t, [IP(a_base, b_base)]) == ComT
```

Required checks:

- prover and verifier serialize the same prior transcript value and `(L, R)`
  tuple in the same order
- proof round reversal matches verifier iteration
- final commitment-key exponent formulas match the fold equations
- base commitment check recomputes `IP(a_base, b_base)`
- parallel rescale path is equation-identical to sequential rescale

## TIPA Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/mod.rs`.

Source basis: paper algebra plus Penumbra SRS and transcript adaptations.

Primary comparison levels:

- `tipa.srs`: `abstract-trace`
- `tipa.ab.gipa`: `abstract-trace`
- `tipa.ab.kzg-challenge`: `penumbra-byte`
- `tipa.ab.kzg-equations`: `abstract-trace`

SRS:

```text
g_alpha_powers = [g * alpha^i] for i in 0..2n-2
h_beta_powers  = [h * beta^i]  for i in 0..2n-2
g_beta = g * beta
h_alpha = h * alpha
```

TIPA prover:

1. Runs GIPA for the pairing inner-product relation.
2. Gets final commitment keys `(ck_a_final, ck_b_final)` and transcript.
3. Derives KZG challenge from:

```text
first transcript element, if present
|| ck_a_final
|| ck_b_final
```

4. Proves KZG openings showing `ck_a_final` and `ck_b_final` are the
   transcript-derived commitment keys from the SRS.

TIPA verifier:

1. Recomputes GIPA transcript and folded base commitments.
2. Recomputes the KZG challenge from the same transcript/final-key bytes.
3. Verifies the G2 opening for `ck_a_final`.
4. Verifies the G1 opening for `ck_b_final`.
5. Verifies the base inner-product commitment:

```text
CommitA(ck_a_final, [a_base]) == ComA_base
CommitB(ck_b_final, [b_base]) == ComB_base
CommitT(ck_t, [IP(a_base, b_base)]) == ComT_base
```

Required checks:

- generic and specialized pairing paths compute the same proof bytes
- affine/projective KZG opening paths match
- transcript polynomial coefficients match the product-form evaluation
- verifier KZG equations use the correct source group for each final key
- shifted-SRS path accounts for `r_shift` and its inverse consistently

## TIPA With Structured Scalar Message Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/tipa/structured_scalar_message.rs`.

Source basis: paper algebra plus local C-path specialization.

Primary comparison levels:

- `ssm.power-sequence`: `abstract-trace`
- `ssm.kzg-challenge`: `penumbra-byte`
- `ssm.base-equation`: `abstract-trace`

This is the C-path variant where the right message is public structured powers:

```text
b = [1, s, s^2, ..., s^(n-1)]
```

Prover:

1. Runs GIPA with placeholder commitments for the structured right message.
2. Proves one KZG opening for the final left commitment key.
3. Uses stage labels:
   - `tipa.c.gipa.round`
   - `tipa.c.kzg`

Verifier:

1. Recomputes GIPA transcript and folded commitments.
2. Verifies the final left commitment key KZG opening.
3. Computes final structured scalar:

```text
b_base = product_i (1 + transcript_i^-1 * s^(2^i))
```

4. Verifies:

```text
CommitA(ck_a_final, [a_base]) == ComA_base
CommitT(ck_t, [IP(a_base, [b_base])]) == ComT_base
```

Required checks:

- `structured_scalar_power` returns `[1, s, s^2, ...]`
- prover and verifier use inverse transcript elements consistently
- no placeholder commitment value enters a real algebraic check
- C-path stage labels are distinct from AB-path labels

## Groth16 Aggregation Adapter Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/applications/groth16_aggregation.rs`.

Source basis: paper algebra plus Penumbra public-input, family, and transcript
adaptations.

Primary comparison levels:

- `groth16.randomizer`: `penumbra-byte`
- `groth16.folded-inputs`: `abstract-trace`
- `groth16.ppe`: `abstract-trace`

Inputs:

```text
proof_i = (A_i, B_i, C_i)
vk = (alpha_g1, beta_g2, gamma_g2, delta_g2, gamma_abc_g1)
public_inputs_i
```

Prover:

```text
A = [A_i]
B = [B_i]
C = [C_i]

com_a = pairing_inner_product(A, ck_1)
com_b = pairing_inner_product(ck_2, B)
com_c = pairing_inner_product(C, ck_1)

r = Fiat-Shamir(com_a, com_b, com_c)
r_vec = [1, r, r^2, ..., r^(n-1)]
A_r = [A_i * r^i]
ip_ab = sum_i e(A_i * r^i, B_i)
agg_c = sum_i C_i * r^i
ck_1_r = [ck_1_i * r^-i]

tipa_ab proves ip_ab over (A_r, B) and (ck_1_r, ck_2)
tipa_c proves agg_c over (C, r_vec) and ck_1
```

Verifier:

```text
r = Fiat-Shamir(com_a, com_b, com_c)
verify tipa_ab using stage labels tipa.ab.*
verify tipa_c using stage labels tipa.c.*

r_sum = 1 + r + ... + r^(n-1)
folded_inputs_j = sum_i public_inputs_i[j] * r^i
g_ic = gamma_abc_g1[0] * r_sum
     + sum_j gamma_abc_g1[j + 1] * folded_inputs_j

accept iff:
  tipa_ab_valid
  && tipa_c_valid
  && e(alpha_g1 * r_sum, beta_g2)
     * e(g_ic, gamma_g2)
     * e(agg_c, delta_g2)
     == ip_ab
```

Required checks:

- prover and verifier derive `r` from exactly `com_a`, `com_b`, `com_c`
- `r_vec` order matches public-input folding and `agg_c`
- byte binding of public inputs to field elements is covered by upstream
  `penumbra-byte` rows; this section checks the folding equation
- `ck_1_r` uses inverse powers matching the shifted TIPA AB prover
- public input arity was checked before this backend is called
- malformed or mutated TIPA AB, TIPA C, KZG opening, public input, or PPE input
  rejects
- reviewer must decide whether the randomizer must reject `r == 1`, or whether
  the verifier must handle `r_sum = n` for that case

## Minimum Evidence Per Refinement Row

Use this table when filling `docs/snarkpack/ripp-refinement.md`.

| Row class | Required evidence |
| --- | --- |
| mechanical | equation review plus unit or parity test |
| performance | equation review plus specialized-vs-generic parity test |
| security-binding | challenge/message-order review plus trace parity or F* boundary row |
| semantic | equation review plus mutation rejection test; security/crypto signoff |

Rows that cannot meet the required evidence stay `open` or become explicit
`assumed` rows in `formal-handoff.md`.

## What This Spec Does Not Prove

This spec does not prove:

- Groth16 soundness
- SnarkPack/RIPP/GIPA/TIPA algebraic soundness
- Fiat-Shamir/random-oracle security
- arkworks field/group/pairing/MSM/serialization correctness
- hax semantic preservation

Those are tracked as standing assumptions in
`docs/snarkpack/formal-handoff.md`. End-to-end formal verification is out of
scope; algebraic soundness is assumed from the paper and the Filecoin
implementation and probabilistically cross-checked by Lean differential
conformance (security.md Stage 9).
