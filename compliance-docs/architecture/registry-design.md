# Registry Design

Two Merkle trees store compliance state on-chain.

## User Registry (QuadTree)

Maps (address, asset) → ComplianceLeaf

```rust
ComplianceLeaf { address, key: ACK, asset_id }
```

Commitment: `poseidon_hash_4(domain, g_d, pk_d, ack, asset_id)`

| Property | Value |
|----------|-------|
| Arity | 4 (quad-tree) |
| Depth | 16 levels |
| Max leaves | ~4 billion |
| Hash | Poseidon377 |

## Asset Registry (Indexed Merkle Tree)

Stores only **regulated** assets. Unregulated = not in tree.

```rust
IndexedLeaf { value: Fq, next_index: u64, next_value: Fq }
```

Leaves form a sorted linked list enabling gap proofs.

### Membership Proof (Regulated)

Asset exists in tree: `leaf.value == asset_id`

### Non-Membership Proof (Unregulated)

Asset falls in gap: `low.value < asset_id < low.next_value`

Both proofs use identical circuit structure (indistinguishable).

## Historical Anchors

Accept past tree roots (like SCT). Bidirectional lookups:
- `anchor_by_height(h)` → root
- `anchor_lookup(root)` → height

Transactions no longer fail if tree changes between proof generation and submission.

## Local Sync

Clients cache trees locally (like SCT) to avoid RPC at tx time.

SQLite tables:
- `compliance_user_positions` - leaf commitments
- `compliance_user_hashes` - internal hashes for auth paths
- `compliance_anchors` - anchors per block height

## Source Files

| Component | Location |
|-----------|----------|
| Registry | `compliance/src/registry.rs` |
| QuadTree | `compliance/src/tree.rs` |
| IMT | `compliance/src/indexed_tree.rs` |
| State keys | `compliance/src/state_key.rs` |
| Local storage | `view/src/storage/compliance.rs` |
