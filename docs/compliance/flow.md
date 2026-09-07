# Compliance Flow

Shieldd, embedded inside Bankd, gives issuers selective visibility into regulated-asset
transfers while preserving the same fixed circuit shape for unregulated
assets. Ledger safety remains a Bankd consensus and Shieldd circuit responsibility.
External policy systems do not authorize spends or establish balance
conservation.

Ordinary Shieldd user transactions exclude release proofs, DH shared points,
and seed-opening material. The privileged `SeizeNote` host request carries a
capsule-specific recovered point and DLEQ proof. See
[`enforcement-and-seizure.md`](enforcement-and-seizure.md) for the release
relation and the unimplemented ACP, Orbis, and Bankd boundaries.

For exact encodings and statement order, see `reference.md`.

## Registration

An issuer creates its policy and ring configuration, then registers an
`AssetPolicy` on Shieldd:

```text
AssetPolicy {
  dk_pub,
  ring_pk,
  daily_volume_limit,
  allowed_ibc_routes,
  ibc_origin,
  ring_id,
  policy_id,
  permission,
  resource
}
```

Regulated assets are members of the indexed asset tree. An unregistered asset
is proved unregulated with a valid non-membership gap. The asset proof must use
the current mutable policy root. User compliance proofs must use the exact
current mutable user tree because each leaf also commits authorization status.

Policy admission rejects identity `dk_pub` and `ring_pk` values before
registration can mutate durable state. This prevents a regulated asset from
selecting a degenerate detection or audit key even if its membership witness is
otherwise valid.

A user registers a `(shielded address, asset)` compliance leaf:

```text
d   = SHA512("elgamal-derivation-v1\0\0" || canonical_address_bytes) reduced mod Fr
ACK = d * ring_pk
```

For regulated participation, ACP permits exactly one live shielded address per
KYC identity. Shieldd's generic diversified-address capability does not admit
additional regulated addresses; only the ACP-approved address may appear in a
regulated user leaf.

The leaf commits to the address encodings, asset ID, capsule capability,
compliance-nullifier-key commitment, and lifecycle. Registration checks the
canonical address, capability derivation, and authorization. A derived `d = 0`
is rejected. The same address may register independently for multiple assets.

Asset id zero is reserved for the indexed-tree sentinel. Registration and both
Transfer and Withdrawal circuits reject it as an action asset, so the sentinel
cannot be presented as an authenticated regulated policy.

## Transfer Construction

The circuit excludes the Decaf identity class for the authorization key, both
distinct address generators, and all four compliance tier EPKs. Each is an
exact `x != 0` row, matching the native full-viewing-key and address gadgets.
These are ownership/classification constraints, not builder hygiene:
identity sender DTK derivation would make the transmission key independent of
the IVK and let one note commitment/path be reopened under distinct nullifier
keys, while an identity receiver generator would let a malicious proof create
such a note. An identity tier EPK would make its shared point independent of
the audit capability and cause every candidate address to pass the same key
confirmation, so it is rejected by the accepted relation.

The transfer planner selects one policy shape:

- Regulated: exact asset-tree membership, registered sender and receiver
  compliance leaves, and the registered policy values.
- Unregulated: exact asset-tree non-membership, fixed sink ring/DK points,
  and the canonical hash of the empty string for each policy identifier. The
  authenticated gap-predecessor daily limit remains present in the leaf
  witness but cannot make an unregulated transfer eligible for accumulation or
  disclosure.

Both branches construct the same detection and audit ciphertext rows. The
encryption equations are unconditional; `is_regulated` selects the effective
keys and policy hashes. For an external regulated ordinary Transfer, the
wallet either proves a real daily-volume transition or selects the fixed
padding branch. Padding sets `is_flagged = true`; a real transition sets it to
false. Self-transfers, unregulated transfers, and fee funding never set it.

For a real transition the circuit proves checked `u128` addition and
`prior_undisclosed_volume + receiver_amount <= daily_volume_limit`. Equality
remains undisclosed. A greater candidate or explicit caller request discloses
only the current transaction and leaves the accumulator head unchanged.

| Tier | Plaintext | Unflagged regulated key | Flagged regulated key |
| --- | --- | --- | --- |
| Detection | asset id; salt; flag; reserved zero | issuer `dk_pub` | issuer `dk_pub` |
| Sender core | amount | sender ACK | issuer `dk_pub` |
| Sender extension | receiver address | sender ACK | issuer `dk_pub` |
| Output core | amount | receiver ACK | issuer `dk_pub` |
| Output extension | sender address | receiver ACK | issuer `dk_pub` |

Unregulated transfers use the selected sink policy. Its ciphertexts remain
well-formed, but no issuer decryptability or capability release is claimed.

The four detection plaintext words are exact:

```text
asset_id
detection_salt
is_flagged
0
```

The flag is boolean and the reserved word is exactly zero. No routing role,
slot, derivation, index, or address fragment is encrypted in detection.

Honest construction samples a fresh private CSPRNG nonce root for each
Transfer action, separate from every sibling Transfer and fee-funding action.
All five salts and the tier seeds/randomizers are deterministically
domain-separated from that root. Reusing one root would repeat EPK and stream
material. Root freshness is a native privacy premise, not a circuit or
consensus check, because the root is private and a malicious creator controls
its own randomness. Each derived tier scalar is rejection-sampled until
nonzero, preventing an identity EPK/shared secret from exposing an honestly
constructed tier.

### Daily volume state

The accounting day is UTC-aligned and independent of the SCT epoch. Every
target timestamp is assigned to the UTC day that contains it. Target timestamps
must remain within ±30 minutes of signed block time, so prior-day nullifiers
remain retained for the same grace after midnight.

The first real transition in a selected day emits a deterministic private
origin nullifier. Later transitions prove SCT membership of the predecessor
commitment and derive its positioned nullifier. Every ordinary Transfer emits
an indistinguishable real or padding commitment; fee funding uses a
proof-bound disabled context and emits neither.

Accumulator nullifiers live in a day-prefixed temporary consensus set rather
than the global historical nullifier tree. Entries are retained through
`day_start + 24h + 30m` and pruned afterward. The owner opening is recovered
from a fixed 108-byte OVK-authenticated payload in the compact block.

Only the receiver output carries compliance data:

```text
TransferOutputBody {
  compliance_ciphertext: 704 bytes
  compliance_metadata:   264 bytes
}
```

Inputs and the change output carry neither field. The metadata is a single
factored record:

```text
ring_id_hash
policy_id_hash
resource_hash
permission_hash
target_timestamp
sender_core_salt
sender_ext_salt
output_core_salt
output_ext_salt
```

The two core key confirmations are carried with the ciphertext. The circuit
binds them to the recovered tier seeds, compressed EPKs, and role-specific
salts. Metadata carries no subject derivation, ACK, or per-ciphertext address
index. The circuit binds every metadata value to its selected policy fact.
Tier identity is structural: the four EPK/c2/ciphertext groups and four salts
always occur in sender-core, sender-extension, output-core, output-extension
order.

## Consensus And Proof Boundary

The transfer circuit proves:

- fixed two-input/two-output shape and dummy-note semantics;
- spend ownership, authorization-key randomization, membership, nullifiers,
  note commitments, value conservation, and balance commitment;
- non-identity authorization key and sender/receiver diversified generators;
- asset membership versus canonical non-membership gap;
- rejection of the asset-tree zero sentinel;
- regulated policy selection and compliance-leaf membership;
- complete compliance leaves and `Active` sender/receiver status;
- daily-volume origin or predecessor validity, checked addition, limit, UTC
  day selection, proof context, and disclosure flag correctness;
- four independent EPK/shared-secret/c2/payload encryption relations;
- detection encryption;
- exact `(asset_id, detection_salt, is_flagged, reserved_zero)` detection
  packing;
- two proof-derived, privately permuted 32-bit routing tags and their complete
  parameter-set identifier;
- the consensus recent-position floor and, for each spend, the exact old-note
  classification `!is_dummy && position < recent_position_floor`;
- canonical address plaintext packing from the two 32-byte Fq encodings into
  31-byte stream words;
- the single 9-field metadata binding; and
- the exact 53-field statement preimage committed under the canonical transfer
  statement-hash domain.

The Rust verifier reconstructs the same 53 fields from typed public data.
Consensus separately checks proof verification, the current asset-policy and
user-status roots, timestamp freshness, spend signatures, transaction-wide
spend-nullifier uniqueness, scoped daily-volume-nullifier uniqueness, and the
binding signature. Transfer's effect hash includes the exact receiver
ciphertext, metadata, accumulator payload, and proof context, so a delegated
builder cannot replace them after the spends are authorized.

## Scanner And Evidence

The scanner extracts only typed public facts:

```text
ExtractedComplianceCiphertext {
  output_ref,
  routing_tags: [u32; 2],
  raw_bytes,
  metadata_bytes
}
```

The scanner DB is the durable spine:

```text
chain output
  -> canonical ciphertext/metadata decode
  -> detection-tier DK screening
  -> persisted detection or bounded invalid row
  -> canonical evidence validation
  -> flagged issuer-DK tier decryption
  -> audit ledger projection
```

`ComplianceScreener` is pure parsing plus detection-key screening. It performs
no persistence, chain fetches, ACP decisions, or release calls. Scanner blocks are
keyed by height/hash/parent hash; a reorg rolls state back to the common
ancestor before replay.

The evidence object contains the output reference, asset/flag/detection facts,
the 704-byte ciphertext, the 264-byte metadata record, and a payload hash. It
contains no capsule-release evidence, shared point, or standalone DLEQ proof. Evidence
validation compares both ciphertext and metadata to the accepted output and
the persisted detection row before an audit can complete.

Valid audit transitions are:

```text
pending -> evidence_valid | evidence_invalid
evidence_valid -> evidence_invalid | decrypt_failed | audit_complete
evidence_invalid -> evidence_valid
decrypt_failed -> audit_complete
audit_complete -> audit_complete
```

## Audit Availability

Flagged regulated transfers encrypt every audit tier to the issuer DK. After
evidence validation, the issuer can decrypt them locally and complete the
audit.

Unflagged regulated tiers encrypt to the sender or receiver ACK. The scanner has
no release import workflow, so those rows cannot currently complete. Scanner
evidence must not publish seed-opening material. `SeizeNote` is a separate
privileged host path.
