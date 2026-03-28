# gnark Spend Prototype

This directory contains the `spend`-first gnark prototype work for Penumbra.

Current implementation status:

- Phase 0 implemented:
  - confirmed gnark `R1CS -> Groth16` works on `BLS12-377`
  - confirmed `gnark-crypto` exposes the `BLS12-377` twisted-Edwards companion curve
- Phase 0.5 implemented:
  - exported Penumbra reference vectors for `decaf377` companion-curve constants and `poseidon377`
  - verified gnark companion-curve constants match Penumbra's `decaf377` companion curve
  - verified gnark does **not** expose native `poseidon377`; only `MiMC` and `Poseidon2` are registered in the std hash registry
  - implemented an exact gnark `poseidon377` `hash_7` gadget using Penumbra's exported `RATE_7_PARAMS`
  - implemented the minimal gnark `decaf377` quotient gadget currently needed by `spend`: `compress_to_field`
  - validated both gadgets against Penumbra-generated Rust vectors
- Phase 1 spike implemented:
  - exported a deterministic Rust-native DLEQ fixture matching Penumbra's `verify_dleq_r1cs`
  - implemented a standalone gnark DLEQ verifier fragment using the same challenge ordering and truncation rules
  - validated the gnark DLEQ fragment on:
    - a valid Rust fixture
    - a wrong-metadata failure case
    - an unregulated skip case
- Statement-hash proof/translation slice implemented:
  - exported a deterministic regulated spend fixture from the real Rust `SpendProofPublic` path
  - extended the Rust fixture export to include the full regulated `SpendProofPrivate` witness
  - defined and exported a concrete `SpendWitnessV1` binary payload for the Android-facing FFI boundary
  - implemented a strict Go decoder for `SpendWitnessV1`
  - validated that the decoded binary witness matches the Rust-exported fixture on:
    - public inputs
    - state-commitment proof data
    - asset/compliance Merkle paths
    - indexed-leaf data
    - user-leaf data
    - note bytes and key blinding fields
  - implemented the exact gnark spend statement-hash circuit over the 17 flattened statement fields
  - generated a gnark `BLS12-377` Groth16 proof and verifying key for that slice
  - translated the gnark proof and VK into Arkworks-compatible objects in Rust
  - verified the translated gnark proof through the existing Rust `SpendProof::verify` flow
  - validated negative cases:
    - wrong statement hash fails
    - wrong DLEQ-bound public field fails
    - malformed proof/VK coordinates fail cleanly during translation

Current verdict:

- `BLS12-377` Groth16 in gnark: supported
- `decaf377` companion-curve constants: supported
- `decaf377` quotient-group semantics needed for the current spike: partially implemented
- `poseidon377` exact semantics for `hash_7`: implemented
- minimal spend-relevant DLEQ verifier fragment: implemented and vector-validated
- spend statement-hash proof/VK/public translation back into Rust verification: implemented for the statement-hash slice
- full regulated spend witness export and binary Android-boundary payload: implemented and cross-checked between Rust and Go
- host-only verifier comparison flow: implemented for gnark native verification vs Arkworks verification of the same gnark-produced `spend` proof

This means the gnark path is not blocked by the outer proof system or curve choice, and it is no longer blocked on the first primitive and subgadget gates. The statement-hash proof/translation slice is now complete. The next stop point is still before a full `spend` port: keep extending the Rust witness boundary and port the remaining spend slices on top of this verified host-side base.

## Verifier benchmark

The prototype now includes a host-only verifier benchmark that compares:

- gnark native `groth16.Verify(...)`
- Arkworks verification of the exact same gnark-produced `spend` proof through the existing Rust bridge

The benchmark intentionally separates one-time setup from repeated pure verification:

- gnark:
  - `load_or_decode_ms`
  - `prepare_ms`
  - repeated `verify_*` timings
- Arkworks:
  - `load_or_decode_ms`
  - `translate_ms`
  - `prepare_vk_ms`
  - repeated `verify_*` timings

Run the full comparison from this directory:

```bash
./run-verify-bench.sh
```

Useful variants:

```bash
./run-verify-bench.sh --warmup-iterations 5 --measured-iterations 50
./run-verify-bench.sh --out-dir ../../tmp/gnark-spend-prototype/verify-bench-custom
```

The runner:

1. ensures `spendsetup` artifacts exist
2. generates a fresh `spendprove` proof artifact
3. runs the gnark verifier benchmark
4. runs the Rust/Arkworks verifier benchmark
5. writes a combined report under `tmp/gnark-spend-prototype/verify-bench/report.json`

The comparison is host-only. It does not measure proving time and does not include Android runtime overhead.

Files:

- `phase0_test.go`: gnark compatibility and Phase 0.5 tests
- `crypto_primitives_test.go`: exact-match tests for `poseidon377::hash_7` and `decaf377::compress_to_field`
- `poseidon377.go`: gnark implementation of exact Penumbra `poseidon377` `hash_7`
- `decaf377.go`: gnark implementation of the minimal `decaf377` quotient gadget used in this spike
- `dleq.go`: gnark implementation of the minimal spend-relevant DLEQ verifier fragment
- `dleq_test.go`: Rust-fixture-backed tests for the gnark DLEQ verifier fragment
- `statement_hash.go`: exact gnark spend statement-hash gadget
- `statement_hash_test.go`: statement-hash parity and Groth16 round-trip tests
- `witness_binary.go`: strict decoder for the prototype `SpendWitnessV1` Android payload
- `witness_binary_test.go`: cross-checks between the Rust-exported fixture and the decoded binary witness
- `verify_bench.go`: shared verifier benchmark JSON helpers and timing stats
- `cmd/spendhashprove/main.go`: host-only CLI that exports gnark proof/VK/public JSON for the statement-hash slice
- `cmd/gnarkverifybench/main.go`: host-only CLI that benchmarks gnark native verification on a `spendprove` artifact
- `compatibility.md`: explicit Phase 0 / 0.5 verdict
- `artifact-mapping.md`: current Penumbra artifacts vs prototype artifacts
- `run-verify-bench.sh`: prototype-local orchestrator for the gnark-vs-Arkworks verifier comparison
- `vectors/phase05_vectors.json`: reference vectors generated from Penumbra Rust code
- `vectors/spend_fixture.json`: deterministic regulated spend fixture generated from Rust
- `vectors/spend_witness_v1.bin`: deterministic regulated `SpendWitnessV1` payload generated from Rust
- `gnark_spend_proto.rs` (previously `crates/bench/src/bin/gnark_spend_proto.rs`): Rust helper for fixture export and Arkworks-side proof/VK translation verification — standalone, not part of the main workspace bench crate
- `rust-vectors/`: standalone Rust utility that generates the reference vectors
