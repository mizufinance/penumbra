# Compliance Reference

Technical specifications and lookup material. See `flow.md` for end-to-end
walkthrough.

---

## Ciphertext Wire Formats

### Transfer Compliance Ciphertext (576 bytes)

Carried on the receiver `TransferOutputBody.compliance_ciphertext` of a
`Transfer` action. Change outputs and `TransferInputBody.compliance_ciphertext`
are empty. Layout (`crates/core/component/compliance/src/transfer.rs`):

```
[  0.. 32] EPK_sender_core     r × G (sender core tier)
[ 32.. 64] EPK_sender_ext      r × G (sender extension tier)
[ 64.. 96] EPK_output_core     r × G (receiver core tier)
[ 96..128] EPK_output_ext      r × G (receiver extension tier)
[128..160] c2_sender_core      ElGamal envelope
[160..192] c2_sender_ext       ElGamal envelope
[192..224] c2_output_core      ElGamal envelope
[224..256] c2_output_ext       ElGamal envelope
[256..320] Detection           2 Fq: (asset_id + flag<<252), salt
[320..352] Encrypted sender_core    1 Fq: amount
[352..448] Encrypted sender_ext     3 Fq: receiver address (sender's view)
[448..480] Encrypted output_core    1 Fq: amount
[480..576] Encrypted output_ext     3 Fq: sender address (receiver's view)
```

4 independent EPKs — one per tier. The single detection tier (encrypted to
`DK_pub`) covers the whole bundle. Both perspectives (sender's view of the
counterparty and receiver's view of the counterparty) are carried, so each
side's daily extension key reveals only their own counterparty data.

### IBC Compliance Metadata

The legacy IBC compliance memo path still exists in
`crates/core/component/compliance/src/ibc.rs`, but it is not the current
transfer compliance wire. Current `Transfer` actions require empty input
compliance bytes and carry the unified transfer compliance ciphertext on the
receiver output.

```
IbcComplianceMetadata {
    compliance_ciphertext: Vec<u8>,
    asset_id: asset::Id,
}
```

---

## Registry Trees

### QuadTree (User Registry)

Maps (address, asset) → ComplianceLeaf.

```rust
ComplianceLeaf { address, asset_id }
```

Commitment: `poseidon_hash_3(domain, g_d, pk_d, asset_id)`

ACK is not stored — derivable from `ring_pk` + `B_d`.

| Property | Value |
|----------|-------|
| Arity | 4 |
| Depth | 16 |
| Max leaves | ~4 billion |
| Hash | Poseidon377 |

### IMT (Asset Registry)

Contains a structural zero-value sentinel plus explicitly inserted asset
entries. The protocol seeds the base asset as an explicit unregulated entry,
regulated assets are inserted explicitly, and other unregulated assets use
non-membership gap proofs.

```rust
IndexedLeaf { value: Fq, next_index: u64, next_value: Fq }
AssetPolicy { dk_pub, ring_pk, threshold, allowed_channels, policy_id }
```

Membership: `leaf.value == asset_id`. Non-membership: `low.value < asset_id < low.next_value`. Both use identical circuit (indistinguishable).

### Historical Anchors

Both trees emit per-block anchors (same pattern as SCT). Bidirectional lookups:
`anchor_by_height(h) → root`, `anchor_lookup(root) → height`. Transactions
reference past tree states, so new registrations don't invalidate in-flight
proofs.

### Local Sync

Clients cache trees locally (like SCT). SQLite tables:
`compliance_user_positions`, `compliance_user_hashes`, `compliance_anchors`.

### Issuer Scanner Store

Issuer scanning uses a separate SQLite store from wallet local sync. The
scanner stores typed chain references:

```rust
BlockRef { height, block_hash, parent_hash, block_time_unix }
TxRef { block, tx_index, tx_hash }
ActionRef { tx, action_index }
OutputRef { action, output_index }
ExtractedComplianceCiphertext { output_ref, raw_bytes }
```

`tx_hash` is the canonical Penumbra `TransactionId`, computed from the same
canonical protobuf transaction bytes as `Transaction::id()`. The transaction
crate has a parity test so scanner hashing changes fail loudly.

SQLite tables:

| Table | Purpose |
|-------|---------|
| `scanner_blocks` | Committed block identity: height, block hash, parent hash, block time, scan status |
| `scanner_ciphertexts` | Raw extracted transfer output ciphertexts keyed by `OutputRef`, plus `screen_status` and `screen_reason` |
| `scanner_detections` | Detected transfer output ciphertexts keyed by height, tx hash, action index, output index |
| `scanner_invalid_ciphertexts` | First 256 malformed ciphertexts per block |
| `scanner_invalid_ciphertext_summaries` | Overflow count for additional malformed ciphertexts in a block |
| `scanner_clear_flows` | Public shield and withdrawal rows extracted from IBC receive and shielded ICS20 withdrawal actions |
| `audit_rows` | Normalized audit ledger projection for private detections and public clear flows |
| `audit_address_aliases` | Optional address/transmission-key labels used by audit-demo and reports |
| `audit_row_audits` | Idempotent subject audit marks for ledger rows |
| `audit_decryption_failures` | Non-destructive record of failed issuer-DK or Orbis decrypt attempts |
| `audit_orbis_receipts` | Stored PRE receipt JSON keyed by output and tier |
| `scanner_sync` | Single-row sync cursor with last height and last block hash |

`commit_block` writes block metadata, raw ciphertexts, screening results,
detections, invalid ciphertexts, invalid summary, clear flows, audit rows, and
sync cursor atomically. Reorg handling compares each live block's parent hash
against the stored hash at `height - 1`; on mismatch, the scanner walks
backward to the common ancestor, rolls back later scanner and audit rows, and
replays from `ancestor + 1`.

Screening is detection-tier DK decryption. Full-tier DK decryption is a
separate audit branch used only for flagged rows. Orbis PRE is the audit branch
for unflagged private rows; it updates the same `audit_rows` key.

---

## Transfer DLEQ

In-circuit Chaum-Pedersen proof binding each transfer tier to the canonical
Penumbra metadata statement. Computed inside the SNARK, output as `(c, s)` per
tier.

### Math (Orbis sign convention)

**Prover (circuit):**

```
S    = r × ACK
R    = k × G
R'   = k × ACK
M    = Poseidon(policy_id_hash, resource_hash, permission_hash, tier, target_timestamp, salt)
c    = Poseidon(ACK, EPK, S, R, R', M)
s    = k + c × r
```

`policy_id_hash`, `resource_hash`, `permission_hash` are reused from IMT leaf
commitment verification (zero additional cost).

**Canonical verifier (Penumbra-side / future public tier object validator):**

```
ACK  = d × ring_pk
R    = s × G   - c × EPK
R'   = s × ACK - c × S
c_check = Poseidon(ACK, EPK, S, R, R', M)
Accept if c_check == c
```

`S` must be supplied alongside the public tier object if the verifier does not
know `sk_ring`. The proof binds the metadata statement to the ACK/EPK relation;
it does not by itself prove that `C2` encrypts a valid seed. `C2` correctness
remains a Penumbra zk-circuit property.

### Current Orbis Stored-Secret Proof

Current Orbis `store_secret` / `start_pre` does not consume the transfer DLEQ
above. It verifies the stored-secret encryption proof for the Orbis-compatible
encrypted-seed object uploaded for each transfer tier. That proof binds the
stored object to Orbis metadata
`{policy_id, resource, permission, tier, timestamp, salt}`, not directly to the
canonical Penumbra transfer-tier statement.

### Tier Binding

| Proof | DLEQ instance | Tier constant |
|-------|---------------|---------------|
| Transfer | sender_core (r_sender_core, ACK_sender) | `Fq::from(1)` |
| Transfer | sender_ext (r_sender_ext, ACK_sender) | `Fq::from(2)` |
| Transfer | output_core (r_output_core, ACK_receiver) | `Fq::from(3)` |
| Transfer | output_ext (r_output_ext, ACK_receiver) | `Fq::from(4)` |

Tier is a circuit constant — Alice cannot lie about it.

### Timestamp Binding

`target_timestamp`: Unix UTC seconds. Client sets `SystemTime::now().as_secs()`.
Validator enforces `|target_timestamp - block_timestamp| <= 3600` (±1 hour)
via `check_timestamp_freshness()`. Prevents replay of old proofs under changed
policies.

### Salt

Random Fq, encrypted in the detection tier (only issuer DK can decrypt).
Included in metadata hash M. Prevents brute-force of M even under full
`sk_ring` compromise.

### Privacy (two layers)

| Layer | Protects against | Mechanism |
|-------|------------------|-----------|
| S-blinding | Public observers | `c = Poseidon(..., S, ...)` — S has 256-bit entropy |
| Salt | `sk_ring` compromise | `M = Poseidon(..., salt)` — salt only recoverable with DK |

### Cost

| Proof | Additional public outputs |
|-------|--------------------------|
| Transfer | +8 Fq (4 × (c, s)) = 256 bytes (`TRANSFER_DLEQ_BYTES`) |
| IBC memo (out-of-circuit) | None — DK-only detection tier |

---

## Restrictions

**Flagging is per-transfer**: A single `is_flagged = (receiver_amount >= threshold)`
is computed once per `Transfer` and applied to the unified compliance bundle on
the receiver output. The flag is based on the actual transfer (receiver) amount,
not on the value of the spent input notes.

**No send/receive distinction**: Issuers see the same data for both sides.

**Defra holds KYC**: KYC data in DefraDB, not on-chain. Issuer knows registered
addresses; KYC-to-identity link is held by Defra only.

**Immutable registrations**: ComplianceLeaf and AssetPolicy cannot be updated.
IBC channel whitelist must be set at registration time.

**IBC first-hop only**: Channel whitelist enforced at withdrawal, not multi-hop.

**No key rotation**: No protocol for rotating compromised ring_pk or DK.

**Cross-tier independence**: Each ACK-tier uses independent ephemeral scalar.
Enforced by ZK circuit. Issuer cannot derive one tier from another.

**Current PRE path is encrypted-seed-object based**: Orbis PRE operates on the
stored encrypted seed object for each transfer tier. It authorizes by stored
object metadata plus request scope, and the PRE request must carry a
`valid_window` whenever the stored object includes a timestamp.

**decaf377 curve**: Orbis supports decaf377 natively — no cross-curve bridge.

---

## Source Files

| Component | Location |
|-----------|----------|
| Encryption / DLEQ | `compliance/src/crypto.rs` |
| R1CS circuits | `compliance/src/r1cs.rs` |
| Data structures | `compliance/src/structs.rs` |
| Registry / trees | `compliance/src/registry.rs`, `tree.rs`, `indexed_tree.rs` |
| Statement hash helpers | `shielded-pool/src/public_input_hash.rs` |
| Transfer action | `shielded-pool/src/transfer/action.rs`, `plan.rs`, `proof.rs`, `compliance.rs` |
| Split / Consolidate actions | `shielded-pool/src/split/`, `shielded-pool/src/consolidate/` (no compliance bytes) |
| Transfer ciphertext / DLEQ | `compliance/src/transfer.rs`, `structs.rs` |
| Issuer scanner refs/extractor/screener/store/worker | `compliance/src/scanner/types.rs`, `sync.rs`, `screener.rs`, `storage.rs`, `worker.rs` |
| Proof aggregation (`AggregateBundle`) | `crates/core/component/proof-aggregation/` |
| View service | `crates/view/src/service.rs` |
| Compliance client | `crates/view/src/client_compliance.rs` |
| Local storage | `view/src/storage/compliance.rs` |
| IBC metadata | `compliance/src/ibc.rs` |
| State keys | `compliance/src/state_key.rs` |
