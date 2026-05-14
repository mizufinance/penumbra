# Compliance Reference

Technical lookup material for compliance. See `flow.md` for the end-to-end
walkthrough.

## Transfer Wire Format

The receiver `TransferOutputBody.compliance_ciphertext` carries the unified
transfer compliance ciphertext. Transfer inputs and change outputs must carry
empty compliance bytes.

```text
TransferComplianceCiphertext: 576 bytes
  0..128    4 EPKs: sender_core, sender_ext, output_core, output_ext
  128..256  4 C2 envelopes, one per audit tier
  256..320  detection tier: asset id + flag, salt
  320..352  encrypted sender_core amount
  352..448  encrypted sender_ext receiver address
  448..480  encrypted output_core amount
  480..576  encrypted output_ext sender address

TransferComplianceDleqProofs: 256 bytes
  4 * (challenge, response)
```

The legacy IBC memo type remains in `compliance/src/ibc.rs`, but it is not the
current transfer compliance wire.

## Registry Trees

| Tree | Purpose | Notes |
|------|---------|-------|
| QuadTree | `(address, asset) -> ComplianceLeaf` | Arity 4, depth 16, Poseidon377 |
| Indexed asset tree | asset regulation status | Membership for regulated assets, non-membership gap proof for unregulated assets |

Both trees emit historical anchors per block. Clients cache tree state locally;
issuer scanning uses a separate scanner DB and does not share wallet sync
tables.

Tree roots are committed in the app-state JMT. Tree nodes and leaves are stored
in nonverifiable storage as deterministic materialization and are checked
against committed roots at node readiness.

The asset tree remains an IMT because regulated-asset proofs need both
membership and non-membership. The nullifier set uses a dedicated JMT-style
sparse tree instead because nullifier insertion is validator-executed, not
proved inside a circuit.

## Scanner References

```rust
BlockRef { height, block_hash, parent_hash, block_time_unix }
TxRef { block, tx_index, tx_hash }
ActionRef { tx, action_index }
OutputRef { action, output_index }
ExtractedComplianceCiphertext { output_ref, raw_bytes, upload_bundle_bytes }
```

`tx_hash` must match Penumbra `TransactionId`. If a helper computes it outside
the transaction crate, keep the transaction-crate parity test mandatory.

The scanner DB schema is not migration-compatible with earlier prototype DBs.
Delete and rebuild old local scanner DBs.

## Scanner DB Tables

| Table | Purpose |
|-------|---------|
| `scanner_blocks` | committed block identity and scan status |
| `scanner_ciphertexts` | raw extracted output ciphertexts and screening status |
| `scanner_detections` | DK-detected private transfer outputs and audit status |
| `scanner_invalid_ciphertexts` | first capped malformed ciphertext rows per block |
| `scanner_invalid_ciphertext_summaries` | overflow count for invalid rows above cap |
| `scanner_clear_flows` | public shield/withdraw rows |
| `scanner_sync` | single-row height/hash cursor |
| `audit_rows` | normalized ledger projection |
| `audit_address_aliases` | optional labels for UI/reporting |
| `audit_row_audits` | idempotent subject audit marks |
| `audit_decryption_failures` | failed issuer-DK or Orbis decrypt attempts |
| `audit_evidence_failures` | evidence build/validation/import failures |
| `audit_orbis_receipts` | stored PRE receipt JSON |
| `compliance_evidence_objects` | canonical encrypted evidence object bytes |

`commit_block` atomically writes block identity, raw ciphertexts, screening
results, detections, invalid summaries, clear flows, audit projections, and
sync state. Reorg handling compares live parent hash to stored `height - 1`,
rolls back to a common ancestor, and replays.

## Scanner Boundaries

- `ScannerStore`: async storage boundary. SQLite is current; Postgres or remote
  stores should not change worker logic.
- `ComplianceScreener`: pure parse + detection-tier DK decrypt. No persistence,
  Orbis, ACP, audit, or chain I/O.
- `AuditAdviceProvider`: policy/ring/label lookup boundary. SourceHub, Orbis,
  ACP, and caches stay outside scanner logic.

## Evidence And Audit Status

`ComplianceEvidenceObject` is the canonical encrypted evidence payload for a
detected transfer:

```text
output ref
asset id, flag, detection salt
transfer ciphertext
transfer DLEQ bundle
public tier decode objects
optional Orbis upload-bundle hash
payload hash
```

`AuditValidationInput` checks payload hash, canonical tier order, ciphertext and
proof byte consistency, tier DLEQ validation against `ring_pk`, and upload
bundle validation when present.

Valid audit states:

```text
pending
evidence_valid
evidence_invalid
decrypt_failed
audit_complete
```

Allowed transitions:

```text
pending -> evidence_valid
pending -> evidence_invalid
evidence_invalid -> evidence_valid
evidence_valid -> decrypt_failed
decrypt_failed -> audit_complete
evidence_valid -> audit_complete
audit_complete -> audit_complete
```

Both flagged issuer-DK decrypt and unflagged Orbis PRE import require
`evidence_valid`.

## Transfer DLEQ

Each audit tier has an in-circuit Chaum-Pedersen proof binding the tier to
Penumbra metadata:

```text
S  = r * ACK
R  = k * G
R' = k * ACK
M  = Poseidon(policy_id_hash, resource_hash, permission_hash, tier, timestamp, salt)
c  = Poseidon(ACK, EPK, S, R, R', M)
s  = k + c * r
```

Verifier reconstruction:

```text
R       = s * G   - c * EPK
R'      = s * ACK - c * S
c_check = Poseidon(ACK, EPK, S, R, R', M)
```

Tier constants:

| Tier | Constant |
|------|----------|
| sender_core | 1 |
| sender_ext | 2 |
| output_core | 3 |
| output_ext | 4 |

The DLEQ binds ACK/EPK/shared-point metadata. `C2` correctness remains a
Penumbra circuit property. Current Orbis PRE validates the encrypted-seed
upload package and its policy metadata; it does not consume the transfer DLEQ
directly.

## Restrictions

- Flagging is per transfer receiver amount: `amount >= threshold`.
- Split and consolidate do not carry compliance ciphertexts.
- Registrations and asset policies are immutable.
- Channel whitelist enforcement is first-hop only.
- No key rotation is currently defined.
- Cross-tier independence is mandatory: independent EPK/randomness per tier.
- Orbis PRE operates on one encrypted-seed object per tier.
- Transfer-circuit constraints that compliance assumes are tracked separately in
  `../transfer-circuit/constraint-checklist.md`.

## Source Files

| Component | Location |
|-----------|----------|
| Transfer ciphertext/DLEQ | `crates/core/component/compliance/src/transfer.rs` |
| Crypto helpers | `crates/core/component/compliance/src/crypto.rs` |
| Compliance circuits | `crates/core/component/compliance/src/r1cs.rs` |
| Registry/trees | `crates/core/component/compliance/src/registry.rs`, `tree.rs`, `indexed_tree.rs` |
| Scanner | `crates/core/component/compliance/src/scanner/` |
| Evidence/audit | `crates/core/component/compliance/src/evidence.rs`, `audit.rs`, `audit_validation.rs` |
| Transfer planning/proofs | `crates/core/component/shielded-pool/src/transfer/` |
| Local compliance sync | `crates/view/src/storage/compliance.rs`, `crates/view/src/client_compliance.rs` |
| Audit bridge | `crates/bin/orbis-audit/src/main.rs` |
