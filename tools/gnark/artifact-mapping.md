# Artifact Mapping

This note describes the current spend/output artifact boundary between Rust and
`tools/gnark`.

## Canonical spend/output artifacts

| Artifact | Current owner | Format |
| --- | --- | --- |
| Public inputs | Rust `SpendProofPublic` / `OutputProofPublic` | Rust structs, flattened to one statement-hash field |
| Private witness | Rust `SpendProofPrivate` / `OutputProofPrivate` | Rust structs |
| Witness payload | Rust witness encoder | `SpendWitnessV1` / `OutputWitnessV1` binary payload |
| Constraint system | Go / gnark | gnark `R1CS` |
| Proving key | Go / gnark artifact bundle | gnark Groth16 proving key |
| Verifying key | Rust canonical verification path | gnark VK represented as Rust `PreparedVerifyingKey<Bls12_377>` |
| Proof | Go / gnark, consumed by Rust | compressed 192-byte Groth16 proof bytes |

## Current runtime boundary

- Rust constructs canonical spend/output witness payloads.
- `tools/gnark` decodes those payloads into gnark assignments.
- gnark proves over the spend/output circuits and returns the proof plus claimed
  statement hash in the binary proof-result format.
- Rust parses that proof result, checks the claimed statement hash, and verifies
  through the existing spend/output verifier flow.

## Important compatibility rule

The spend/output proving side is gnark-only, but the Rust side still owns:

- statement-hash construction
- proof byte parsing
- verifier-key preparation
- batch verification and aggregation plumbing

That is why the artifact contract is defined in terms of canonical Rust
spend/output proof types and witness payloads, not in terms of Go-only structs.

## Historical note

Older transition docs referred to Arkworks spend/output proving keys and a
`spend`-only prototype. Those are no longer the live model:

- spend and output both use gnark for proving
- the checked-in gnark artifact bundle is canonical for those two families
- Arkworks references remain only for explicit comparison tests and legacy
  non-migrated proof families
