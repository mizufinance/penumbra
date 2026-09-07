# SnarkPack Design

Shieldd aggregates same-verifying-key Groth16 proofs with its local SnarkPack
implementation. There are exactly two proof wire encodings over one shared
statement and transcript: the production v1 full-target encoding and the
optional v2 torus encoding. See [verification.md](verification.md) for runtime
checks.

## Protocol

For `N` proofs `(Aᵢ, Bᵢ, Cᵢ)` under one VK, the prover:

1. commits to the proof vectors with pairing-based inner-product commitments;
2. randomizes each position with a Fiat-Shamir scalar;
3. folds the vectors through a logarithmic GIPA recursion;
4. opens the final folded commitment keys with KZG; and
5. emits one aggregate checked by a final Groth16 pairing-product equation.

GIPA is the halving recursion. TIPA specializes it to the pairing inner product
and KZG key openings. The combined TIPP/MIPP proof folds the AB pairing and C
multiexponentiation relations with one challenge stream.

The transcript stages are `aggregate.randomizer`, `tipp-mipp.x0`,
`tipp-mipp.gipa.round`, `tipp-mipp.final-bridge`, and `tipp-mipp.kzg`.

## Local protocol choices

### BLS12-377

Shieldd uses BLS12-377 to match the proving stack.

### Domain-separated transcript

Blake2b derives the five Fiat-Shamir stages from a length-prefixed, stage- and
nonce-bound preimage. SHA-256 separately binds the VK, canonical statement, and
challenge context. Changing a stage label, field order, encoding, or dependency
is a protocol change.

### Canonical statement binding

The statement binds protocol version, curve and backend identity, SRS identifier,
proof family, VK digest, real and padded counts, repeat-final padding, and the
ordered padded public inputs. Collision resistance relies on SHA-256.

### Repeat-final padding

GIPA requires a power-of-two length. Inputs are padded by repeating the final
real proof and public-input row. The verifier recomputes and checks the exact
padding and preserves the caller-order real prefix.

### Wrapper and preflight

The v1 and v2 wrappers use distinct versioned domains and store the statement
digest plus a bounded inner proof. Proposers and consensus use v1; v2 is an
explicit library, test, and benchmark surface only. The shared
`AGGREGATE_PROTOCOL_VERSION` is a statement/application discriminator, not a
third wire encoding.
Preflight validates size, framing, family, counts, VK/SRS facts, rows, padding,
and strict proof decoding before expensive verification.

### Family and bundle routing

The closed proof-family registry selects the compiled VK. Aggregate bundles are
accepted only through the proposal aggregation pipeline and must be the unique
final bundle in a proposal. They are not generic user actions.

## Deployment SRS boundary

Production fails closed unless an authenticated SRS artifact matches the
compiled registry. Activation requires a reviewed ceremony and verification
evidence.

## Optimization byte-lock

Internal compute changes must preserve semantics and the committed proof and
transcript bytes. Wire changes require a protocol-version bump. Transcript
changes require corresponding updates in `shieldd-formal`. The workflow is in
[optimization-playbook.md](../../crates/crypto/proof-aggregation/optimization-playbook.md).
