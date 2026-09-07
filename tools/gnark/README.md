# gnark proof runtime

Shieldd proves these Groth16 families over BLS12-377:

- `transfer`
- `note_reshape1x8`
- `note_reshape8x1`
- `shielded_ics20_withdrawal`
- `note_seizure`

The Rust client sends binary witnesses to a bundled shared library or a prover
daemon. Both transports use the same family registry, ABI, artifacts, and
Arkworks-compatible proof encoding.

## Commands

From the repository root:

```bash
just go-check
just gnark-proof-tests
just gnark-proof-tests-slow
```

The slow suite exercises release-mode proof generation. CI runs the matching
`ci-*` commands under `nix develop`.

`gnarkctl` owns setup, proving, replay, verification benchmarks, and circuit
exports. Run it without a command for the current command list.

## Circuit export boundary

Formal verification belongs to
[`mizufinance/shieldd-formal`](https://github.com/mizufinance/shieldd-formal).
That repository pins a Shieldd commit and obtains deterministic circuit inputs
with:

```bash
go run ./cmd/gnarkctl export-circuit \
  --circuit transfer \
  --sr1cs-out /tmp/transfer.sr1cs \
  --manifest-out /tmp/transfer.json
```

`export-wiring-transcript` emits the corresponding semantic operation trace.
Shieldd exports runtime circuit data; it contains no proof specifications,
generated theorem sources, verifier tooling, evidence, or formal CI gates.

## Runtime configuration

Each family uses an artifact directory and one transport:

- `SHIELDD_GNARK_TRANSFER_ARTIFACT_DIR`
- `SHIELDD_GNARK_TRANSFER_LIB` or `SHIELDD_GNARK_TRANSFER_DAEMON`
- `SHIELDD_GNARK_NOTE_RESHAPE_ARTIFACT_DIR`
- `SHIELDD_GNARK_NOTE_RESHAPE_LIB` or `SHIELDD_GNARK_NOTE_RESHAPE_DAEMON`
- `SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR`
- `SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB` or
  `SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_DAEMON`
- `SHIELDD_GNARK_NOTE_SEIZURE_ARTIFACT_DIR`
- `SHIELDD_GNARK_NOTE_SEIZURE_DAEMON`

These variables select proving inputs and transports. Consensus verification
uses verifying keys compiled from the bundled artifacts.

## Artifacts

Canonical artifacts live in `tools/gnark/artifacts`. Fetch only the required
bundle with `scripts/proof_artifacts.py`; verified SR1CS files are cached under
`.cache/proof-artifacts` by default.

When a circuit changes, regenerate its artifact set, then run:

```bash
just go-test
cargo check -p shieldd-sdk-shielded-pool
cargo check -p shieldd-sdk-proof-aggregation
```

The development-only historical nullifier generator is
`cmd/historicalproofspike`. Its checked-in keys and samples are unsafe fixtures,
not deployable proving material.
