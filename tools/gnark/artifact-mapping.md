# Artifact Mapping

This note describes the current shielded-action artifact boundary between Rust and
`tools/gnark`.

## Canonical transfer artifacts

| Artifact | Current owner | Format |
| --- | --- | --- |
| Public inputs | Rust `TransferProofPublic` | Rust structs, flattened to one statement-hash field |
| Private witness | Rust `TransferProofPrivate` | Rust structs |
| Witness payload | Rust witness encoder | `TransferWitnessV1` binary payload |
| Constraint system | Go / gnark | gnark `R1CS` |
| Proving key | Go / gnark artifact bundle | gnark Groth16 proving key |
| Verifying key | Rust canonical verification path | gnark VK represented as Rust `PreparedVerifyingKey<Bls12_377>` |
| Proof | Go / gnark, consumed by Rust | compressed 192-byte Groth16 proof bytes |

## Current runtime boundary

- Rust constructs canonical transfer witness payloads.
- `tools/gnark` decodes those payloads into gnark assignments.
- gnark proves over the transfer circuit and returns the proof plus claimed
  statement hash in the binary proof-result format.
- Rust parses that proof result, checks the claimed statement hash, and verifies
  through the existing transfer verifier flow.

## Important compatibility rule

The gnark proving side is canonical for `Transfer`, while the Rust side still owns:

- statement-hash construction
- proof byte parsing
- verifier-key preparation
- batch verification and aggregation plumbing

That is why the artifact contract is defined in terms of canonical Rust
transfer proof types and witness payloads, not in terms of Go-only structs.

## Historical note

Older transition docs referred to Arkworks proving keys and legacy shielded
surfaces. Those are no longer the live model:

- transfer uses gnark for proving
- the checked-in gnark artifact bundle is canonical for the active transfer surface
- Arkworks references remain only for explicit comparison tests and legacy
  non-migrated proof families
