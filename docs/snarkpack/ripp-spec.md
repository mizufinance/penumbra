# SnarkPack RIPP Implementation Spec

Status: review specification for the Penumbra-owned RIPP/SnarkPack backend.
This is not a mechanized proof. It is the checklist used to review
`docs/snarkpack/ripp-refinement.md`.

In this document, "RIPP" means the local proof stack under
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src`: GIPA, TIPA,
TIPA-with-structured-scalar-message, and the Groth16 aggregation adapter.

## What Is Formally Verified Today

The current hax/F* work proves implementation-boundary properties, not RIPP
algorithm correctness.

Mechanically proved rows today:

- `validate_counts` rejects zero real count.
- wrapper decode rejects oversized bytes before parsing or exposing the inner
  proof range.

Composed/type-checked rows today:

- `ChallengeContext` has no production default, tuple constructor, or TLS
  fallback.
- aggregate backend verification receives only typed preflighted proof bytes.
- app-level aggregate bundle flow reaches typed aggregate preflight.

Open F* rows include statement encoding injectivity, digest reduction, full
validation iff lemmas, padding non-malleability, wrapper round trip/digest
mismatch, and challenge preimage injectivity. RIPP/GIPA/TIPA algorithm fidelity
is not formally proved; it is covered by the refinement review map.

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

## Fiat-Shamir Challenge Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs`.

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

## GIPA Spec

Implementation file:
`crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/gipa.rs`.

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

Those are tracked as assumptions or research-track obligations in
`docs/snarkpack/formal-handoff.md` and
`docs/snarkpack/formal-research-plan.md`.
