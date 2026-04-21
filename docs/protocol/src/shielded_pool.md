# Multi-Asset Shielded Pool

Penumbra records value in a single, *multi-asset shielded pool*. Value is
recorded in _notes_, which hold a typed quantity of value together with the
keys required to spend or detect that value.

Notes are never published on chain directly. Instead, the chain stores opaque
_commitments_ to notes in the [state commitment tree](./sct.md), and shielded
actions prove correct note spending and note creation without revealing the
underlying plaintext values.

The supported shielded actions are:

- `Transfer`
- `Split`
- `Consolidate`
- `ShieldedIcs20Withdrawal`

To prevent double-spending, each spent note yields a unique _nullifier_. The
chain rejects any transaction that attempts to reuse a nullifier. New note
commitments are inserted into the state commitment tree and later become
spendable inputs to supported shielded actions.

The note structure is described in [*Note Plaintexts*](./shielded_pool/note_plaintexts.md).
Note commitments are described in [*Note Commitments*](./shielded_pool/note_commitments.md).
