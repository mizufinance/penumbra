# gnark Compatibility Memo

This memo captures the implemented Phase 0 and Phase 0.5 checks for the
`spend`-first gnark prototype.

## Verdict

- `BLS12-377` `R1CS -> Groth16` in gnark: `supported`
- Android-buildable Go shared library flow: `not yet implemented in this spike`
- `decaf377` companion twisted-Edwards curve constants in `gnark-crypto`: `supported`
- `decaf377` quotient-group semantics needed by the current spike (`compress_to_field`): `implemented and vector-validated`
- `poseidon377` exact `hash_7` semantics: `implemented and vector-validated`
- spend statement-hash layer over the real Rust public fixture: `implemented and vector-validated`
- gnark proof/VK/public translation into Rust/Arkworks verification for the statement-hash slice: `implemented`
- broader Decaf encoding/quotient behavior beyond `compress_to_field`: `still manual work if the spend port needs it`

## What was actually validated

1. A tiny gnark circuit compiles, runs setup, proves, and verifies on
   `BLS12-377` using `Groth16`.
2. `gnark-crypto`'s `bls12-377/twistededwards` companion curve matches
   Penumbra's companion curve constants:
   - `A = -1`
   - `D = 3021`
   - subgroup order matches `decaf377::Fr`
3. Penumbra reference vectors are generated directly from Rust for:
   - spend statement-hash domain labels
   - exact `poseidon377::hash_7` inputs/output plus `RATE_7_PARAMS`
   - multiple Decaf `compress_to_field` cases over real quotient-group points
4. gnark's native hash registry does not provide `POSEIDON377`.
5. A custom gnark `poseidon377` gadget now reproduces the exported Rust `hash_7`
   vector exactly.
6. A custom gnark `decaf377::compress_to_field` gadget now reproduces the
   exported Rust vectors exactly.
7. A custom gnark DLEQ verifier fragment now reproduces Penumbra's
   `verify_dleq_r1cs` behavior on a deterministic Rust-native fixture, rejects
   wrong metadata when regulated, and skips challenge enforcement when
   unregulated.
8. A deterministic regulated spend fixture is exported directly from Rust with
   the exact 17 flattened statement fields used by the current spend circuit.
9. A gnark `spend-statement-hash` circuit proves over those 17 fields on
   `BLS12-377` and exports explicit proof/VK/public JSON.
10. The exported gnark proof and VK translate back into Arkworks objects and
    verify through the existing Rust `SpendProof::verify` flow.
11. Negative cases fail cleanly:
    - wrong statement hash
    - wrong DLEQ-bound public field
    - malformed proof/VK coordinates

## Implication

The gnark path is not blocked by the outer proof system or the outer curve.
The first Penumbra-specific cryptography gate is now cleared for:

- exact `poseidon377::hash_7`
- the minimal `decaf377` quotient gadget currently needed by the spike:
  `compress_to_field`
- host-side proof/VK/public translation back into Rust verification for the
  statement-hash slice

The next blocker is not primitive equivalence or statement-hash verification.
It is porting the remaining spend slices on top of the current host-only base:
the core spend integrity checks, Merkle path behavior, and the full compliance
spend path.
