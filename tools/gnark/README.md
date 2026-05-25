# gnark Proof Runtime

This directory contains the gnark proving runtime for Penumbra's supported
shielded proof families:

- `Transfer`
- `Split`
- `Consolidate`
- `ShieldedIcs20Withdrawal`

Legacy standalone single-leg proving flows are not part of the active surface.

## Current Scope

- proving system: Groth16 over `BLS12-377`
- transports:
  - C-shared libraries for the supported families
  - persistent prover daemon
- artifact model:
  - one canonical artifact set per supported family
  - shared runtime loading and witness/proof ABI handling

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

- `PENUMBRA_GNARK_TRANSFER_ARTIFACT_DIR`
- `PENUMBRA_GNARK_TRANSFER_LIB` or `PENUMBRA_GNARK_TRANSFER_DAEMON`
- `PENUMBRA_GNARK_SPLIT_ARTIFACT_DIR`
- `PENUMBRA_GNARK_SPLIT_LIB` or `PENUMBRA_GNARK_SPLIT_DAEMON`
- `PENUMBRA_GNARK_CONSOLIDATE_ARTIFACT_DIR`
- `PENUMBRA_GNARK_CONSOLIDATE_LIB` or `PENUMBRA_GNARK_CONSOLIDATE_DAEMON`
- `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR`
- `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB` or `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_DAEMON`

Verifier-key overrides for Rust verification and aggregation use:

- `PENUMBRA_GNARK_TRANSFER_ARTIFACT_DIR`
- `PENUMBRA_GNARK_SPLIT_ARTIFACT_DIR`
- `PENUMBRA_GNARK_CONSOLIDATE_ARTIFACT_DIR`
- `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR`

The supported families share runtime plumbing where that reduces duplication,
but each family still has its own artifact registry entry and transport target.

## Rust <-> Gnark Boundary

Each supported family exposes a narrow witness/proof boundary. Witness payloads
are encoded into binary messages, proved in gnark, and decoded back into
Arkworks-compatible proof objects on the Rust side.

Transport selection happens in the Rust gnark client layer:

- bundled shared library
- env-configured shared library
- env-configured daemon

The prover runtime above that transport owns client initialization, caching, and
shutdown. Callers only use the family-specific supported APIs.

## Artifact Model

Each supported family has one canonical bundled artifact set checked into
`tools/gnark/artifacts`. Internal padding and witness-shape details are runtime
concerns, not part of the public product surface.

When a supported circuit changes, regenerate the matching artifacts and then
rebuild and test:

```bash
just go-test
cargo check -p penumbra-sdk-shielded-pool
cargo check -p penumbra-sdk-proof-aggregation
```

The family registries and runtime configuration should be generated from the
supported manifests rather than maintained by ad hoc handwritten wiring.

## Verifier benchmark

The repository includes a host-only verifier benchmark that compares gnark
native verification with Arkworks verification of the exact same gnark-produced
proof through the existing Rust bridge.

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

The runner ensures artifacts exist for the selected supported circuit, produces
a fresh proof artifact, runs both verifiers, and writes a combined report under
`tmp/gnark/verify-bench/report.json`.

The comparison is host-only. It does not measure proving time and does not include Android runtime overhead.

Files:

- `phase0_test.go`: gnark compatibility and Phase 0.5 tests
- `crypto_primitives_test.go`: exact-match tests for `poseidon377::hash_7` and `decaf377::compress_to_field`
- `internal/primitives/poseidon377.go`: gnark implementation of exact Penumbra `poseidon377` `hash_7`
- `internal/primitives/decaf377.go`: gnark implementation of the minimal `decaf377` quotient gadget used in this spike
- `internal/compliance/dleq.go`: gnark implementation of the minimal transfer-relevant DLEQ verifier fragment
- `dleq_test.go`: Rust-fixture-backed tests for the gnark DLEQ verifier fragment
- `internal/primitives/statement_hash.go`: exact gnark statement-hash gadgets
  for the supported families
- `internal/primitives/statement_hash_test.go`: statement-hash parity and Groth16 round-trip tests
- `internal/abi/transfer_witness_binary.go`: strict decoder for the canonical `TransferWitnessV1` payload
- `internal/circuits/transfer_circuit.go`: gnark implementation of the shielded-pool transfer circuit
- `internal/compliance/transfer_encryption.go`: gnark helpers for transfer compliance encryption
- `internal/abi/shielded_ics20_withdrawal_witness_binary.go`: strict decoder for the canonical shielded ICS-20 withdrawal witness payload
- `internal/circuits/shielded_ics20_withdrawal_circuit.go`: gnark implementation of the outbound shielded ICS-20 withdrawal circuit
- `internal/artifacts/`: verifier benchmark JSON helpers, metadata, and timing stats
- `cmd/gnarkctl/main.go`: unified host CLI for setup, proving, replay, and verify benchmarking
- `cmd/transferlib/main.go`: C-shared gnark prover for `transfer`
- `cmd/consolidatelib/main.go`: C-shared gnark prover for `consolidate`
- `cmd/splitlib/main.go`: C-shared gnark prover for `split`
- `cmd/proverdaemon/main.go`: long-lived stdin/stdout gnark prover daemon for supported shielded actions
- `compatibility.md`: explicit Phase 0 / 0.5 verdict
- `artifact-mapping.md`: current Penumbra transfer artifact boundary
- `run-verify-bench.sh`: local orchestrator for the gnark-vs-Arkworks verifier comparison
- `internal/primitives/vectors/phase05_vectors.json`: reference vectors generated from Penumbra Rust code
- `internal/primitives/vectors/transfer_witness_v1.bin`: deterministic regulated `TransferWitnessV1` payload generated from Rust
- `rust-vectors/`: standalone Rust utility that generates the reference vectors
