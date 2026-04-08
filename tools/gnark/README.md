# gnark Proof Runtime

This directory contains the gnark prover work for Penumbra's shielded-pool
`spend`, `output`, and generic transfer circuits, plus the runtime transports
used by the Rust integration.

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
- full `spend` witness export and binary Android-boundary payload: implemented and cross-checked between Rust and Go
- full `output` witness export and binary Android-boundary payload: implemented in Rust and decoded by gnark
- full gnark `spend` proving path through the Rust bridge: implemented
- full gnark `output` proving path through the Rust bridge: implemented
- generic gnark `transfer(n_in, n_out)` proving path through the Rust bridge:
  implemented for the manifest-supported families
- runtime transports:
  - shared library (`cmd/spendlib`, `cmd/outputlib`, `cmd/transferlib`): implemented
  - persistent daemon (`cmd/proverdaemon`): implemented
- host-only verifier comparison flow: implemented for gnark native verification vs Arkworks verification of the same gnark-produced `spend` proof

This means the gnark path is not blocked by the outer proof system or curve
choice. Both shielded-pool circuits now have production-shaped witness
packages, gnark circuit implementations, Rust proof translation, and two
runtime transports over the same witness/proof ABI.

## Local Verification

From the repository root, the supported local verification commands are:

```bash
just go-check
just check
just gnark-proof-tests
just gnark-proof-tests-slow
```

`just gnark-proof-tests` is the fast inner-loop suite. Use
`just gnark-proof-tests-slow` for the end-to-end release-mode proof-generation
path.

CI runs the corresponding `ci-*` wrappers under `nix develop`, while local
development can use the plain commands directly.

## Rust Integration Environment

Rust proving selects the gnark backend when the corresponding artifact
directory is configured and one transport is selected for that circuit:

- `PENUMBRA_GNARK_SPEND_ARTIFACT_DIR`
- `PENUMBRA_GNARK_SPEND_LIB` or `PENUMBRA_GNARK_SPEND_DAEMON`
- `PENUMBRA_GNARK_OUTPUT_ARTIFACT_DIR`
- `PENUMBRA_GNARK_OUTPUT_LIB` or `PENUMBRA_GNARK_OUTPUT_DAEMON`
- `PENUMBRA_GNARK_TRANSFER1X1_ARTIFACT_DIR`
- `PENUMBRA_GNARK_TRANSFER1X2_ARTIFACT_DIR`
- `PENUMBRA_GNARK_TRANSFER2X1_ARTIFACT_DIR`
- `PENUMBRA_GNARK_TRANSFER2X2_ARTIFACT_DIR`
- `PENUMBRA_GNARK_TRANSFER_LIB` or `PENUMBRA_GNARK_TRANSFER_DAEMON`

Verifier-key overrides for Rust verification and aggregation use:

- `PENUMBRA_GNARK_SPEND_ARTIFACT_DIR`
- `PENUMBRA_GNARK_OUTPUT_ARTIFACT_DIR`
- `PENUMBRA_GNARK_TRANSFER{NXM}_ARTIFACT_DIR`

Transfers use one generic runtime library and one generic circuit implementation,
but each supported `n_in x n_out` transfer family still has its own setup,
verifying key, proving key, and artifact directory.

## Rust <-> Gnark Boundary

The transfer proving boundary is intentionally narrow:

- Rust semantic types:
  - `TransferProofPublic`
  - `TransferProofPrivate`
- Rust witness payload:
  - `TransferWitnessV1`
  - encoded to one binary witness byte slice
- Gnark proof result payload:
  - one binary proof-result byte slice
  - decoded back into an Arkworks-compatible `TransferProof`

Family metadata always comes from the generated `TransferFamilyId` registry and
the transfer-family manifest, not from handwritten per-family logic in the Rust
or Go proving path.

Transport selection happens in one place in the Rust transfer gnark client:

- bundled shared library
- env-configured shared library
- env-configured daemon

The transfer prover runtime above that transport owns client initialization,
caching, and shutdown. Callers only use the generic transfer proving API.

## Adding Transfer Families

The handwritten source of truth for supported transfer families is:

```bash
tools/gnark/transfer_families.json
```

Adding a new family such as `3x3` should require:

1. Add a manifest entry with `id`, `label`, `artifact_name`, `n_in`, `n_out`,
   and `bundled_lib_basename`.
2. Regenerate transfer-family bindings:

```bash
cd tools/gnark
GOCACHE=/tmp/penumbra-go-cache go run ./cmd/gen-transfer-families
```

3. Generate setup artifacts and keys for the new family:

```bash
cd tools/gnark
GOCACHE=/tmp/penumbra-go-cache go run ./cmd/gnarkctl setup \
  --circuit transfer3x3 \
  --out-dir artifacts/transfer3x3
```

4. Copy the artifacts into the bundled proof-params tree:

```bash
cp -R tools/gnark/artifacts/transfer3x3 \
  crates/crypto/proof-params/src/gen/gnark/transfer3x3
```

5. Rebuild and test:

```bash
just go-test
cargo check -p penumbra-sdk-shielded-pool
cargo check -p penumbra-sdk-proof-aggregation
```

The transfer circuit implementation itself is generic over `(n_in, n_out)`.
What changes per family is the generated registry wiring and the per-shape
artifact set. New families should not require handwritten Rust or Go source
edits outside the manifest.

## Verifier benchmark

The repository includes a host-only verifier benchmark that compares:

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
./run-verify-bench.sh --out-dir ../../tmp/gnark/verify-bench-custom
```

The runner:

1. ensures `gnarkctl setup --circuit spend` artifacts exist
2. generates a fresh `gnarkctl prove --circuit spend` proof artifact
3. runs the gnark verifier benchmark
4. runs the Rust/Arkworks verifier benchmark
5. writes a combined report under `tmp/gnark/verify-bench/report.json`

The comparison is host-only. It does not measure proving time and does not include Android runtime overhead.

Files:

- `phase0_test.go`: gnark compatibility and Phase 0.5 tests
- `crypto_primitives_test.go`: exact-match tests for `poseidon377::hash_7` and `decaf377::compress_to_field`
- `internal/primitives/poseidon377.go`: gnark implementation of exact Penumbra `poseidon377` `hash_7`
- `internal/primitives/decaf377.go`: gnark implementation of the minimal `decaf377` quotient gadget used in this spike
- `internal/compliance/dleq.go`: gnark implementation of the minimal spend-relevant DLEQ verifier fragment
- `dleq_test.go`: Rust-fixture-backed tests for the gnark DLEQ verifier fragment
- `internal/primitives/statement_hash.go`: exact gnark spend/output/transfer statement-hash gadgets
- `statement_hash_test.go`: statement-hash parity and Groth16 round-trip tests
- `internal/abi/witness_binary.go`: strict decoder for the `SpendWitnessV1` witness payload
- `witness_binary_test.go`: cross-checks between the Rust-exported fixture and the decoded binary witness
- `internal/abi/output_witness_binary.go`: strict decoder for `OutputWitnessV1`
- `internal/abi/transfer_witness_binary.go`: strict decoder for the generic `TransferWitnessV1` payload
- `internal/circuits/output_circuit.go`: gnark implementation of the shielded-pool `output` circuit
- `internal/circuits/transfer_circuit.go`: generic gnark implementation of the shielded-pool `transfer(n_in, n_out)` circuit
- `internal/compliance/output_compliance.go`: gnark helpers for output-side compliance encryption and DLEQ checks
- `internal/artifacts/`: verifier benchmark JSON helpers, metadata, and timing stats
- `cmd/gnarkctl/main.go`: unified host CLI for setup, proving, replay, and verify benchmarking
- `cmd/spendlib/main.go`: C-shared gnark prover for `spend`
- `cmd/outputlib/main.go`: C-shared gnark prover for `output`
- `cmd/transferlib/main.go`: C-shared gnark prover for generic transfer families
- `cmd/proverdaemon/main.go`: long-lived stdin/stdout gnark prover daemon for `spend`, `output`, or `transfer`
- `compatibility.md`: explicit Phase 0 / 0.5 verdict
- `artifact-mapping.md`: current Penumbra spend/output artifact boundary
- `run-verify-bench.sh`: local orchestrator for the gnark-vs-Arkworks verifier comparison
- `vectors/phase05_vectors.json`: reference vectors generated from Penumbra Rust code
- `vectors/spend_fixture.json`: deterministic regulated spend fixture generated from Rust
- `vectors/spend_witness_v1.bin`: deterministic regulated `SpendWitnessV1` payload generated from Rust
- `gnark_spend_proto.rs` (previously `crates/bench/src/bin/gnark_spend_proto.rs`): Rust helper for fixture export and Arkworks-side proof/VK translation verification — standalone, not part of the main workspace bench crate
- `rust-vectors/`: standalone Rust utility that generates the reference vectors
