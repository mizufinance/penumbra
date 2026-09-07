# Compliance Reference

Technical lookup material for the current transfer compliance surface.
See `flow.md` for the end-to-end lifecycle.

## Transfer Wire Format

Only the receiver `TransferOutputBody` carries compliance bytes. Transfer
inputs and the change output must not carry compliance data.

```text
TransferComplianceCiphertext: 704 bytes
  0..128    four compressed EPKs
             sender_core, sender_ext, output_core, output_ext
  128..256  four canonical Fq c2 values in the same order
  256..320  sender-core and output-core key confirmations
  320..448  four-Fq detection ciphertext
  448..480  sender_core ciphertext: one Fq
  480..576  sender_ext ciphertext: three Fq
  576..608  output_core ciphertext: one Fq
  608..704  output_ext ciphertext: three Fq

TransferComplianceMetadata: 264 bytes
  0..32     ring_id_hash Fq
  32..64    policy_id_hash Fq
  64..96    resource_hash Fq
  96..128   permission_hash Fq
  128..136  target_timestamp u64 little-endian
  136..168  sender_core_salt Fq
  168..200  sender_ext_salt Fq
  200..232  output_core_salt Fq
  232..264  output_ext_salt Fq
```

Every Fq and compressed point must decode canonically. Metadata timestamp zero
is invalid. Tier labels are not serialized; fixed ordering is the tier domain.

The transport has no upload bundle, encrypted seed envelope, public shared
point, DLEQ challenge, or DLEQ response.

After decryption, the four detection words are:

```text
0  asset_id
1  detection_salt
2  is_flagged
3  reserved_zero
```

The flag is canonical boolean and word 3 is exactly zero. Word 0 is the exact
asset ID. Detection carries no slot, role permutation, derivation, address
fragment, or index.

## Transfer Key And Address Validity

The Transfer relation has three explicit one-row Decaf identity exclusions:

```text
authorization_key.x             != 0
sender.diversified_generator.x  != 0
receiver.diversified_generator.x != 0
```

For the on-curve companion-Edwards points consumed by the surrounding gadgets,
`x = 0` is exactly the Decaf identity class. These rows match Rust's
full-viewing-key and address allocation rules. In particular, the sender row
prevents identity-DTK ownership aliasing, and the receiver row prevents a
malicious proof from creating a note with that ambiguous owner.

## Transfer Public Statement

The fixed 2x2 Transfer statement has 53 Fq fields. Its hash uses the
`shieldd.shielded_pool.transfer.public_input_hash.statement` domain.

```text
 0       anchor
 1..2    receiver note and recovery-capsule commitments
 3..4    change note and recovery-capsule commitments
 5       balance commitment
  6..7    fixed sender/receiver routing tags
  8       routing parameter-set identifier
  9       recent position floor
 10       daily-volume transition nullifier
 11       daily-volume successor or padding commitment
 12       selected UTC day start
 13       proof context
 14..19   two (nullifier, randomized verification key, history-required bit) triples
 20..21   asset and compliance anchors
 22..25   detection ciphertext
 26..28   sender_core: EPK, c2, one ciphertext word
 29..33   sender_ext: EPK, c2, three ciphertext words
 34..36   output_core: EPK, c2, one ciphertext word
 37..41   output_ext: EPK, c2, three ciphertext words
 42       target_timestamp
 43..44   sender-core and output-core key confirmations
 45..48   ring, policy, resource, and permission hashes
 49..52   sender-core, sender-ext, output-core, and output-ext salts
```

The exact tail append order is:

```text
target_timestamp,
sender_core_key_confirmation,
output_core_key_confirmation,
ring_id_hash,
policy_id_hash,
resource_hash,
permission_hash,
sender_core_salt,
sender_ext_salt,
output_core_salt,
output_ext_salt
```

The metadata timestamp is not appended twice: its serialized value must equal
the statement's existing `target_timestamp`. The authoritative builders are
`transfer_statement_fields` in Rust and `buildTransferStatementFields` /
`ReconstructedTransferStatementFieldsFromWitness` in Go.

## Effective Policy Selection

| Value | Regulated | Unregulated |
| --- | --- | --- |
| `ring_pk` | registered asset leaf | fixed unregulated sink ring point |
| `dk_pub` | registered asset leaf | fixed unregulated sink DK point |
| daily volume limit | registered limit | authenticated gap-predecessor limit; ignored outside regulated membership |
| four policy hashes | registered strings | hash of the empty string |

The circuit constrains `is_regulated` to exact asset-tree membership or a valid
canonical non-membership gap. Encryption checks run in both branches.
Registry admission rejects identity `dk_pub` and `ring_pk` values before a
policy can be committed to the asset tree.

An external regulated ordinary Transfer is unflagged only when it proves a
real transition whose checked candidate is at most the authenticated daily
volume limit. The fixed padding branch flags the transfer. Unregulated,
self-transfer, and fee-funding contexts remain unflagged.

For regulated assets, audit-tier shared secrets select ACK when unflagged and
issuer DK when flagged. Detection always uses the selected DK. Each tier has an
independent randomizer and EPK. Honest construction derives them from one
fresh private CSPRNG nonce root per Transfer action. That root must not be
reused across sibling Transfers or fee funding; its uniqueness and entropy are
native privacy premises, not public circuit facts.

## Registry Trees

| Tree | Purpose | Shape |
| --- | --- | --- |
| Compliance tree | `(address, asset) -> ComplianceLeaf` | arity 4, depth 16 |
| Asset tree | regulated policy membership and unregulated gap | indexed tree |

Consensus requires the exact current mutable asset-policy and user-status
roots. Large node
materialization is nonverifiable storage checked against those committed
roots.

`ComplianceLeaf` is

```text
PoseidonHash7(
  "shieldd.compliance.leaf",
  diversified_generator_fq,
  transmission_key_fq,
  asset_id,
  compressed_capk,
  compressed_rnk_dh_pk,
  Poseidon(rnk),
  packed_lifecycle
)
```

The address encodings must be canonical. `capk` is the nonidentity capsule
capability. The registration certificate binds both `rnk_dh_pk = ring_sk * G_d`
and `Poseidon(rnk)`. The wallet derives the RNK through static DH; the
corresponding production Orbis release API is not implemented.
The leaf exposes only its Poseidon commitment. The packed lifecycle injectively
contains status, freeze generation, and frozen-since height. Asset ID zero is
reserved for the indexed-tree sentinel and cannot be registered or used as a
Transfer or Withdrawal action asset.

## Scanner Types And Tables

```rust
BlockRef { height, block_hash, parent_hash, block_time_unix }
TxRef { block, tx_index, tx_hash }
ActionRef { tx, action_index }
OutputRef { action, output_index }
ExtractedComplianceCiphertext { output_ref, routing_tags, raw_bytes, metadata_bytes }
```

| Table | Purpose |
| --- | --- |
| `scanner_blocks` | canonical block identity and scan status |
| `scanner_ciphertexts` | accepted ciphertext and metadata bytes |
| `scanner_detections` | DK-detected private outputs and audit status |
| `scanner_invalid_ciphertexts` | bounded malformed rows |
| `scanner_invalid_ciphertext_summaries` | overflow counts |
| `scanner_clear_flows` | public shield/withdraw projections |
| `scanner_sync` | replay cursor |
| `compliance_evidence_objects` | canonical evidence bytes |
| `audit_rows` | normalized audit projection |
| `audit_decryption_failures` | bounded decryption failures |
| `audit_evidence_failures` | bounded evidence failures |

## Scanner evidence

`ComplianceEvidenceObject` contains:

```text
transfer object type
OutputRef and block identity
asset id, flag, detection salt
TransferComplianceCiphertext
TransferComplianceMetadata
SHA-256 payload hash
```

It deliberately excludes release material, DH shared points, and standalone DLEQ
proofs. `validate_and_save_evidence_object` verifies the payload hash, metadata
shape, accepted ciphertext/metadata byte equality, and persisted detection
facts before advancing the row to `evidence_valid`.

## Audit Boundary

Flagged transfers can be completed by issuer-DK decryption after evidence
validation. Scanner evidence never publishes seed-opening material. The
privileged note-seizure host call uses a capsule-specific point and DLEQ proof;
see [`enforcement-and-seizure.md`](enforcement-and-seizure.md).

Transfer statements, user transactions, and scanner evidence do not contain
capsule-release material.

## Circuit Implementation Boundary

The deployed Transfer and shielded-withdrawal proving systems are the gnark
circuits under `tools/gnark/`. No second circuit architecture is supported.

## Restrictions

- Flagging discloses only the current external regulated Transfer. Real
  accumulator transitions advance only undisclosed receiver volume.
- Equality with `daily_volume_limit` remains undisclosed; a greater candidate
  uses the disclosed padding branch.
- Note reshapes carry no transfer audit ciphertext, but regulated reshapes prove
  the owner leaf is `Active` under the exact current compliance root.
- Asset policies and registrations are immutable.
- Channel whitelist enforcement is first-hop only.
- Cross-tier randomizer/EPK independence is mandatory.
- Metadata belongs only to the receiver output.
- Capsule-release material must remain outside user transactions and scanner evidence.

## Source Map

| Component | Location |
| --- | --- |
| Ciphertext construction | `crates/core/component/compliance/src/transfer.rs` |
| Metadata codec | `crates/core/component/compliance/src/decode_object.rs` |
| Native crypto | `crates/core/component/compliance/src/crypto.rs` |
| Transfer circuit | `tools/gnark/internal/circuits/transfer_circuit.go` |
| Witness ABI | `tools/gnark/internal/abi/transfer_witness_binary.go` |
| Rust statement builder | `crates/core/component/shielded-pool/src/public_input_hash.rs` |
| Scanner | `crates/core/component/compliance/src/scanner/` |
| Evidence and audit | `crates/core/component/compliance/src/evidence.rs`, `audit.rs` |
| Formal verification | [`shieldd-formal`](https://github.com/mizufinance/shieldd-formal) |
