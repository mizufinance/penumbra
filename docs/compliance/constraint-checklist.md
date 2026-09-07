# Compliance Constraint Checklist

This checklist covers the current compliance surface. General spend, note,
nullifier, and value constraints are tracked in
`docs/transfer-circuit/constraint-checklist.md`.

## Circuit Constraints

### Asset Status And Effective Policy

- Enforced by exact asset-tree membership or canonical non-membership-gap
  verification in `transfer_circuit.go`.
- Regulated values come from the committed asset leaf.
- Asset id zero is rejected because it is the indexed-tree sentinel, not an
  asset policy.
- Policy admission rejects identity detection and ring keys before the asset
  leaf can enter durable registry state.
- Unregulated values select fixed sink ring/DK points and empty-string policy
  hashes. Their authenticated predecessor-leaf daily limit remains in the
  fixed witness but cannot enable accumulation or disclosure.
- `is_regulated` is boolean and cannot be chosen independently of the tree.
- Mutation coverage: regulated/unregulated fixtures, bad gap bounds, wrong
  asset leaf, and selected-policy field changes.

### Sender And Receiver Compliance Leaves

- Transfer rejects identity authorization keys and identity sender/receiver
  diversified generators with three exact one-row Decaf predicates. This
  matches native key/address allocation and prevents identity-DTK ownership
  aliasing; it is not merely an honest-construction precondition.
- Regulated transfers bind the diversified generator, transmission key, asset
  ID, capability, RNK commitment, and lifecycle into compliance-leaf
  commitments under the accepted compliance anchor.
- Native registration rejects a derived `d = 0`, preventing an identity ACK.
- Regulated sender and receiver statuses must both equal `Active`.
- ACK derivation uses the selected ring point and the bound `d`.
- Unregulated transfers keep the same witness shape but gate membership against
  the exact asset-status branch.
- Mutation coverage: leaf fields, paths, positions, anchor, derivation, and ACK
  inputs.

### Daily Volume And Detection

- An eligible real transition proves a canonical day origin or SCT predecessor,
  checked `u128` addition, and `candidate <= daily_volume_limit`.
- Eligible padding means disclosure; ineligible padding remains unflagged.
- An unregulated transfer is never flagged, including when its receiver amount
  is `u128::MAX`.
- UTC midnight alone selects the next day, and fee funding is constrained
  to its statement-bound disabled context.
- Detection encryption is unconditional and uses the selected DK shared secret,
  sender-core EPK, asset ID/flag, and detection salt.
- The exact plaintext order is asset, salt, flag, reserved zero. The flag is
  boolean and no slot, role permutation, derivation, or address index appears.
- The detection ciphertext is part of the public statement.
- Mutation coverage: origin, continuation, exact limit, over-limit candidate,
  successor amount, transition mode, timestamp decomposition, flag, reserved
  word, salt, EPK, and each detection ciphertext word.

### Audit-Tier Encryption

- Four fixed tiers exist in structural order: sender core, sender extension,
  output core, output extension.
- Every tier constrains `EPK = r*G`, the selected ACK/DK shared secret,
  `c2 = seed + compress(shared_secret)`, and every Poseidon stream word.
- These equations are unconditional in both regulated and unregulated branches.
- Each tier uses an independent witness randomizer and EPK.
- Every published tier EPK is constrained outside both Decaf identity
  representatives.
- Honest construction rejection-samples each tier scalar until nonzero.
- Address tiers encrypt the canonical two-field, 64-byte address encoding split
  into 31-byte words. The circuit's native binary decomposition enforces
  reduced field encodings before packing.
- Honest native construction samples one CSPRNG nonce root per Transfer action
  and never reuses it across sibling Transfers or fee funding. The root
  deterministically expands into every salt, seed, and tier randomizer, so this
  is a privacy precondition rather than an R1CS-enforceable invariant.
- Mutation coverage: randomizer, EPK coordinates, c2, seed, plaintext, and every
  ciphertext word for each tier.

### Factored Metadata

- One metadata record binds exactly nine facts: four selected policy hashes,
  `target_timestamp`, and four tier salts. It publishes no subject derivation.
- The four salts are structural tier domains in the fixed tier order.
- The serialized record is exactly 264 bytes: eight canonical Fq encodings plus
  one little-endian u64.
- Metadata timestamp equals the existing transfer target timestamp; it is not a
  second statement field.
- Mutation coverage: every metadata field, noncanonical Fq bytes, zero
  timestamp, truncation, and trailing bytes.

### Public Statement

- Rust and Go reconstruct the same 53-field preimage.
- The statement hash uses the canonical transfer domain.
- The preimage binds the consensus recent-position floor and one
  `history_required` bit per spend.
- The public tail commits both core key confirmations and all eight
  non-duplicate metadata Fq values.
- ABI tests reject malformed headers and wrong vector lengths.
- Differential tests compare native Rust/Go reconstruction, circuit public
  assignment, and statement hash.

## Consensus And Runtime Checks

### Live Context

- `validate_compliance_anchors` requires the exact current mutable asset and
  user-status roots.
- Regulated NoteReshape proves its owner is `Active`; fee funding uses Transfer,
  so a frozen user cannot reshape notes or spend the affected asset for fees.
- `check_timestamp_freshness` bounds target timestamp drift.
- Proof verification precedes handler completion.
- Spend signatures cover a Transfer effect hash containing the exact receiver
  ciphertext and metadata; delegated construction cannot replace those bytes
  after authorization.
- Transaction-wide spend-nullifier insertion, temporary scoped daily-volume
  nullifier insertion, and the binding signature remain external acceptance
  requirements.

### Wire Shape

- Only the receiver output may carry the 704-byte ciphertext and 264-byte
  metadata.
- Inputs and the change output carry neither.
- Point and Fq decoders reject noncanonical values and wrong lengths.
- The wire contains no capsule-release evidence, shared point, or DLEQ proof.

## Scanner And Evidence Checks

### Output Identity

- `BlockRef`, `TxRef`, `ActionRef`, and `OutputRef` are the canonical keys.
- `tx_hash` is the transaction crate's `TransactionId`.
- Reorg rollback uses stored block and parent hashes.

### Persisted Bytes

- `scanner_ciphertexts` stores the exact accepted ciphertext and optional
  metadata bytes.
- `validate_and_save_evidence_object` requires both to match evidence exactly.
- Detection asset, flag, salt, and reserved-zero facts must match the persisted
  detection row.
- Failures are persisted with bounded attacker-controlled reason text.

### Audit Completion

- Audit completion requires `evidence_valid`.
- Flagged rows may complete through issuer-DK tier decryption.
- No scanner capsule-release import workflow is exposed. Unflagged ACK-tier audit therefore
  remains incomplete even when transaction evidence is valid.
