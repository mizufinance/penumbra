# Nullifier history

Status: protocol version 2 is implemented for interoperability testing. The
checked-in proving keys are unsafe and must not be activated.

## Model

Validators keep current and previous nullifier-generation trees. A generation
contains 30 application epochs and uses a depth-20 quaternary Poseidon377
indexed tree. Retired generations are committed in order by
`archived_history_head`.

Each real input exposes whether its note is recent or old. An old input proves
nonmembership across the complete retired prefix, without revealing its source
generation.

The public window is:

```text
NullifierWindow {
    protocol_version,
    current_generation,
    recent_position_floor,
    archived_generation_count,
    archived_history_head,
}
```

At rollover, consensus verifies both live trees, retires the previous tree,
updates the ordered history commitment, promotes the current tree, and creates
an empty current tree. Export and physical pruning are node-local work.

## Proofs and storage

A BLS12-377 Groth16 proof covers one retired generation. A BW6-761 Groth16 proof
compresses ten consecutive generation proofs. Wallets update one generation at
a time and compress each full ten-proof chunk.

Full nodes store immutable compressed generation packs. A pack binds the
generation index, root, SCT interval, canonical leaves, and checksum. Expanded
tree records may be pruned only after the pack is durable, reconstructed, and
root-verified.

The main ownership boundaries are:

- `crates/core/component/sct`: generation state and witness packs
- `crates/view`: durable wallet cache and update worker
- `crates/core/transaction`: proof bundles, authorization binding, and gas
- `crates/core/app`: cryptographic and current-window validation
- `crates/crypto/proof-params`: proof decoding and verification keys
- `tools/gnark/cmd/historicalproofspike`: unsafe interoperability fixtures

## Activation requirements

Production activation requires ceremony-backed keys for both curves, a
configured prover, canonical release artifacts, gas calibration, and
independent review of tree ordering, gap constraints, history chaining,
recursive inputs, and encodings. Formal evidence belongs in
`mizufinance/shieldd-security`, pinned to the activating Shieldd commit.

Required tests cover lower and upper gaps, zero and maximum nullifiers, path
ordering, roots, indices, SCT intervals, history heads, proof ordering, flags,
and trailing bytes. Full nodes must serve identical witnesses after pack
reconstruction and restart.
