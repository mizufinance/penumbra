# Transfer Circuit Constraint Checklist

These are transfer-circuit constraints. Compliance depends on them, but does not
own them.

## Spend Constraints

- Each real spend note commitment matches the spent note fields.
- Each real spend state path resolves to the claimed anchor.
- Each real spend nullifier is derived from the nullifier key, note commitment,
  and state position.
- Dummy spends use synthetic dummy nullifiers and dummy randomized verification
  keys.
- Randomized verification keys bind to the spend authorization key and action
  randomizer for real spends.
- Spent note transmission keys bind to the IVK decomposition.

## Output Constraints

- Each output note commitment matches the created note fields.
- Output 0 is the receiver output and is nonzero.
- Output 1 is sender-owned change when present.
- Output assets match the shared transfer asset.
- Regulated output recipient leaves resolve to the compliance anchor.

## Balance And Statement Constraints

- Net value balance is computed from all input and output amounts for the shared
  asset.
- The action balance commitment matches the computed net balance and action
  blinding.
- Public statement hash binds anchor, balance commitment, asset/compliance
  anchors, target timestamp, nullifiers/RKs, output commitments, and compliance
  public fields.

