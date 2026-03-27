# Artifact Mapping

This prototype keeps the current Rust verifier logic and changes only the proving side.

## Current Penumbra spend artifacts

| Artifact | Current source | Format |
| --- | --- | --- |
| Public inputs | `SpendProofPublic` + statement hash | Rust structs + Arkworks field elements |
| Private witness | `SpendProofPrivate` | Rust structs |
| Proving key | `SPEND_PROOF_PROVING_KEY` | bundled Arkworks `ProvingKey<Bls12_377>` |
| Verifying key | `SPEND_PROOF_VERIFICATION_KEY` | bundled Arkworks `PreparedVerifyingKey<Bls12_377>` |
| Proof | `SpendProof([u8; 192])` | compressed Arkworks Groth16 proof bytes |

## Prototype gnark spend artifacts

| Artifact | Prototype owner | Format |
| --- | --- | --- |
| Witness payload | Rust exporter | prototype-only serialized payload |
| Constraint system | Go / gnark | gnark `R1CS` |
| Proving key | Go / gnark | gnark Groth16 proving key |
| Verifying key | Go / gnark, translated into Rust | gnark Groth16 VK translated to Arkworks-compatible verifier inputs |
| Proof | Go / gnark, translated into Rust | gnark Groth16 proof translated to Arkworks-compatible proof inputs |

## Implemented host-side slice

The prototype now has one end-to-end verified slice:

- witness/public source: `vectors/spend_fixture.json`
- circuit: `spend-statement-hash`
- proof/VK exporter: `cmd/spendhashprove`
- Rust translation/verifier harness: `crates/bench/src/bin/gnark_spend_proto.rs`

For this slice, the Go side emits explicit affine coordinates for:

- proof `A`, `B`, `C`
- VK `alpha_g1`, `beta_g2`, `gamma_g2`, `delta_g2`, `gamma_abc_g1[]`
- public inputs and flattened statement fields

The Rust side then:

- reconstructs Arkworks `Proof<Bls12_377>`
- reconstructs Arkworks `VerifyingKey<Bls12_377>` and `PreparedVerifyingKey`
- serializes the proof into the existing compressed Groth16 byte format
- verifies it through the existing `SpendProof::verify` logic

## Locked prototype decisions

- The prototype is `spend` only.
- The frontend is gnark `R1CS`, not `PLONK`.
- Rust keeps final verification semantics.
- The current checked-in Penumbra spend VK is **not** reused for gnark proofs.
- Translation of gnark proof/public/VK artifacts into the Rust verifier path is mandatory if the prototype advances beyond Phase 0.5.

## Current stop condition

The prototype no longer stops on primitive equivalence or on proof/VK/public translation for the statement-hash slice:

- exact `poseidon377::hash_7` is implemented and vector-validated
- `decaf377::compress_to_field` is implemented and vector-validated
- the gnark statement-hash slice proves and verifies through Rust/Arkworks

The next stop condition is still before a full `spend` port:

- extend the serialized Rust witness boundary beyond the canonical statement-hash fixture
- port the remaining spend slices on top of the current statement-hash and DLEQ base
- keep the final Rust verification milestone unchanged for the full spend circuit
