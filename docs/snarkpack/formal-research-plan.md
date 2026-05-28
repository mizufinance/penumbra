# SnarkPack End-To-End Formal Research Plan

Status: research-grade proof plan. This is separate from the hax/F*
implementation-boundary proof gate in `docs/snarkpack/formal-handoff.md`.

## Backend Decision

Use a split proof stack:

- F* via hax: executed Rust implementation-boundary proofs.
- Lean 4: algebraic protocol model for Groth16 aggregation, RIPP, GIPA, TIPA,
  pairings, commitments, and reduction invariants.
- EasyCrypt: Fiat-Shamir/random-oracle game proof and transcript-binding games.
- Coq: fallback only if Lean 4 cannot support the algebraic model cleanly.

Lean 4 and EasyCrypt tool versions are not pinned yet because the research
project has not started. The first research milestone must pin exact tool
versions and add a clean CI gate before any theorem is marked proved.

## End-To-End Theorem

Target theorem:

```text
If Penumbra aggregate verification accepts a family aggregate, then every real
underlying Groth16 proof verifies against the statement bound by the aggregate,
unless one of the named cryptographic assumptions fails.
```

The theorem composes these layers:

1. Rust wrapper/statement validation accepts only one canonical statement.
2. Fiat-Shamir challenges are bound to that statement.
3. The non-interactive transcript corresponds to an accepting interactive
   transcript in the random-oracle model.
4. RIPP/GIPA/TIPA verification implies the reduced algebraic relations.
5. Groth16 aggregation verification implies every real proof verifies.
6. App-level aggregate-bundle verification calls the verified aggregate path.

## Stages

### Stage R0: Research Toolchain

- Install and pin Lean 4, Lake, EasyCrypt, Why3/SMT dependencies, and any math
  libraries used by the model.
- Add `just snarkpack-research-formal` and a clean-image CI workflow.
- Record tool versions and proof artifact hashes in this document.

Gate: clean CI can typecheck empty Lean/EasyCrypt project skeletons.

### Stage R1: Protocol Model

- Define finite fields, source groups, target group, pairings, scalar
  arithmetic, multi-scalar multiplication, commitments, and verifier equations.
- Model only the algebra required by SnarkPack; do not model arkworks internals.
- Define Rust-to-spec mapping tables for statements, public inputs, SRS
  material, proof objects, and verifier outputs.

Gate: the model typechecks and every verifier object has a spec counterpart.

### Stage R2: RIPP/GIPA/TIPA Algebraic Proof

- Prove GIPA folding preserves the claimed inner-product relation.
- Prove TIPA reduction preserves the pairing-product relation.
- Prove structured-scalar-message reductions preserve the clear-vector relation.
- Prove KZG opening checks bind the reduced commitments under the stated
  commitment assumptions.

Gate: Lean theorem `ripp_accepts_implies_reduced_relation`.

### Stage R3: Groth16 Aggregation Proof

- Model Groth16 verifier equations and the SnarkPack aggregation relation.
- Prove accepted aggregate relations imply every real Groth16 proof relation,
  excluding padded copies according to the Penumbra padding rule.
- Keep Groth16 proof-system soundness as a named cryptographic assumption unless
  a separate Groth16 soundness development is imported.

Gate: Lean theorem `aggregate_accepts_implies_all_real_groth16_verify`.

### Stage R4: Fiat-Shamir Random-Oracle Proof

- Model the interactive protocol and non-interactive Fiat-Shamir transform in
  EasyCrypt.
- Prove fixed domain labels, statement-derived challenge context, nonce, and
  message order define the random-oracle queries.
- Prove accepted non-interactive proofs correspond to accepting interactive
  transcripts except with random-oracle/SHA-256 failure probability.

Gate: EasyCrypt theorem `fs_accepts_implies_interactive_accepts`.

### Stage R5: Composition

- Compose F* implementation-boundary proofs, Lean algebraic proofs, and
  EasyCrypt Fiat-Shamir proof into the end-to-end theorem.
- Each cross-tool boundary must be a named assumption or a checked translation
  lemma. No implicit translation assumptions.

Gate: final proof index row
`penumbra_aggregate_accepts_implies_all_real_proofs_verify` is marked proved or
has only explicitly approved cryptographic assumptions.

## Required Assumptions

- SHA-256 collision resistance.
- SHA-256 preimage resistance where needed by statement digest usage.
- Random-oracle model for Fiat-Shamir.
- Pairing-friendly group laws for BLS12-377.
- Correctness of arkworks field, group, pairing, serialization, and MSM
  implementations relative to the Lean algebraic model.
- Groth16 soundness, unless imported as a separate verified development.
- KZG/commitment binding assumptions used by RIPP/TIPA.
- hax extraction preserves the modeled Rust semantics for the extracted safe
  subset.

## Non-Goals

- Proving arkworks implementation correctness from source.
- Proving BLS12-377 curve arithmetic implementation correctness.
- Proving SRS ceremony correctness.
- Replacing runtime tests, fuzzing, benchmarks, or external audit.
