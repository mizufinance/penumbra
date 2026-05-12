# Compliance Constraint Checklist

This checklist covers compliance-owned constraints only. Transfer-circuit
soundness constraints such as nullifiers, randomized spend authorization keys,
note commitments, state anchors, value conservation, and balance commitments are
external invariants the compliance flow assumes; they are tracked separately in
`docs/transfer-circuit/constraint-checklist.md`.

## Circuit Constraints

- Asset policy is bound by the indexed asset tree leaf and asset anchor.
- Regulated transfers use the policy DK public key, ring public key, threshold,
  ring ID hash, policy ID hash, resource hash, and permission hash from that
  leaf.
- Unregulated transfers use the protocol sink DK/ring keys and skip regulated
  DLEQ enforcement.
- The threshold flag equals `receiver_amount >= policy.threshold`.
- The detection tier encrypts asset ID, threshold flag, and detection salt to
  the effective DK public key.
- Sender/output core tiers encrypt receiver amount to their respective shared
  secrets.
- Sender/output extension tiers encrypt the expected counterparty address fields
  to their respective shared secrets.
- ACKs are derived from each subject compliance leaf and the effective ring
  public key.
- Each tier DLEQ binds `(ACK, EPK, shared point)` to the tier metadata hash.
- Tier metadata binds subject `B_d`, policy/resource/permission hashes, tier
  label, target timestamp, and tier salt.

## Scanner And Evidence Constraints

- Scanner rows identify outputs by `(height, block_hash, tx_hash, action_index,
  output_index)`.
- Persisted evidence must match the raw scanner ciphertext bytes for that output.
- Evidence asset ID, threshold flag, and detection salt must match the detected
  DK plaintext.
- Evidence tier objects must appear in canonical order: sender core, sender ext,
  output core, output ext.
- Each tier object must match the transfer ciphertext EPK/C2 and DLEQ proof.
- Upload bundles must validate and their package statements must match evidence
  tier statements.
- Orbis PRE audit import may only complete rows whose evidence is valid.
- Flagged issuer-DK decryption may only complete rows whose evidence is valid.

