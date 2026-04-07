# gnark Compatibility Notes

This note records the compatibility work that made the current gnark
spend/output prover possible.

## What is established

- `BLS12-377` `R1CS -> Groth16` proving works in gnark for the Penumbra
  spend/output circuits.
- The required `decaf377` companion-curve constants and quotient behavior used
  by the current circuits have been implemented and validated.
- The required `poseidon377` semantics used by the current circuits have been
  implemented and validated.
- Rust witness export and Go witness decoding agree on the spend/output witness
  transport format.
- gnark-produced spend/output proofs are consumed by the canonical Rust
  verification path.

## What still uses Arkworks

Arkworks references that remain in this repo are comparison or legacy items,
not the canonical spend/output proving path:

- explicit gnark-vs-Arkworks timing tests in Rust
- verifier benchmark helpers under `tools/gnark/run-verify-bench.sh`
- proof families that have not yet migrated to gnark

## Why this file still exists

The original migration work started as a spend-first compatibility effort. The
repo has moved beyond that stage, but it is still useful to keep a short record
of the compatibility assumptions that the current gnark integration relies on.
