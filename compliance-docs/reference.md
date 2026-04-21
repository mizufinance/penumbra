# Compliance Reference

Technical specifications and lookup material. See `flow.md` for end-to-end
walkthrough.

---

## Ciphertext Wire Formats

### Spend (224 bytes)

```
[  0.. 32] EPK          r_s × G
[ 32.. 64] c2_core      ElGamal envelope (core tier seed)
[ 64..128] Detection    2 Fq: (asset_id + flag<<252), salt
[128..224] Core         3 Fq: amount + sender address
```

1 EPK. Detection shares `r_s` with core (safe: DK vs ACK are unrelated key
families).

### Output (544 bytes)

```
[  0.. 32] EPK_1        r_1 × G (detection + receiver core)
[ 32.. 64] EPK_2        r_2 × G (receiver extension)
[ 64.. 96] EPK_3        r_3 × G (sender extension)
[ 96..128] c2_core      ElGamal envelope (core tier seed)
[128..160] c2_ext       ElGamal envelope (extension tier seed)
[160..192] c2_sext      ElGamal envelope (spend extension tier seed)
[192..256] Detection    2 Fq: (asset_id + flag<<252), salt
[256..352] Core         3 Fq: amount + receiver address
[352..448] Output Ext   3 Fq: counterparty address
[448..544] Spend Ext    3 Fq: sender's counterparty info
```

3 independent EPKs. All counterparty data lives on the Output action (both
receiver's and sender's perspectives).

### IBC Compliance Metadata

When a regulated asset crosses IBC, the planner injects compliance metadata
into the ICS-20 memo:

```
IbcComplianceMetadata {
    compliance_ciphertext: Vec<u8>,  // Spend ciphertext (224 bytes)
    asset_id: asset::Id,
}
```

Encoded as base64 protobuf in a JSON memo field. The issuer decrypts directly
with DK — no Merkle proofs needed.

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

---

## DLEQ Proof

In-circuit Chaum-Pedersen proof binding each ciphertext to policy metadata.
Computed inside the SNARK, output as `(c, s)` per tier.

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

**Verifier (Orbis, at PRE time):**

```
S    = d × sk_ring × EPK              (MPC computation)
ACK  = d × ring_pk
R    = s × G   - c × EPK
R'   = s × ACK - c × S
c_check = Poseidon(ACK, EPK, S, R, R', M)
Accept if c_check == c
```

### Tier Binding

| Proof | DLEQ instance | Tier constant |
|-------|---------------|---------------|
| Spend | core | `Fq::from(1)` |
| Output | core (r_1, ACK_receiver) | `Fq::from(1)` |
| Output | ext (r_2, ACK_receiver) | `Fq::from(2)` |
| Output | sext (r_3, ACK_sender) | `Fq::from(3)` |

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

| Proof | Additional constraints | Additional public outputs |
|-------|----------------------|--------------------------|
| Spend | ~4,050 | +2 Fq (c, s) = 64 bytes |
| Output | ~11,350 | +6 Fq (c1, s1, c2, s2, c3, s3) = 192 bytes |

---

## Restrictions

**Flagging is per-note**: Flag triggers when the spent note's value >= threshold,
even if the actual transfer amount is small. The change output inherits the flag.

**No send/receive distinction**: Issuers see the same data for both sides.

**Defra holds KYC**: KYC data in DefraDB, not on-chain. Issuer knows registered
addresses; KYC-to-identity link is held by Defra only.

**Immutable registrations**: ComplianceLeaf and AssetPolicy cannot be updated.
IBC channel whitelist must be set at registration time.

**IBC first-hop only**: Channel whitelist enforced at withdrawal, not multi-hop.

**No key rotation**: No protocol for rotating compromised ring_pk or DK.

**Cross-tier independence**: Each ACK-tier uses independent ephemeral scalar.
Enforced by ZK circuit. Issuer cannot derive one tier from another.

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
| Spend proof | `shielded-pool/src/spend/proof.rs`, `plan.rs`, `action.rs` |
| Output proof | `shielded-pool/src/output/proof.rs`, `plan.rs`, `action.rs` |
| View service | `crates/view/src/service.rs` |
| Compliance client | `crates/view/src/client_compliance.rs` |
| Local storage | `view/src/storage/compliance.rs` |
| IBC metadata | `compliance/src/ibc.rs` |
| State keys | `compliance/src/state_key.rs` |
