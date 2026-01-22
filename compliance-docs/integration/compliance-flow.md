# Compliance Flow

End-to-end overview: registration → transaction → scanning.

## 1. Asset Registration (Issuer)

```
Issuer → RegisterAsset { asset_id, is_regulated: true }
       → Asset added to IMT (Indexed Merkle Tree)
```

Unregulated assets are NOT registered. Non-membership proof = unregulated.

## 2. User Registration (via Orbis)

```
User → KYC with Orbis
     → Orbis derives UK = Hash(MK, user_id)
     → Orbis computes AK = UK * B_d (per address)
     → RegisterUser { address, AK, asset_id }
     → Compliance leaf added to user tree (QuadTree)
```

Each (address, asset) pair requires separate registration.

## 3. Transaction (Alice → Bob)

```
Alice → Build tx: Spend(alice_note) + Output(bob_addr, amount)
      → Planner enriches with compliance:
         - Fetches AK from registry (public)
         - Derives DK = AK + T * B_d (daily public key)
         - Generates sender ciphertext (encrypted to alice's DK)
         - Generates receiver ciphertext (encrypted to bob's DK)
         - Fetches Merkle proofs (user tree + asset tree)
      → ZK proofs bind compliance data
      → Transaction broadcast
      → Validator verifies proofs + anchors
      → Ciphertexts stored on-chain
```

## 4. Scanning (Issuer)

```
Issuer → Fetches dk (daily scalar) from Orbis
       → Scans blocks: S = dk * EPK
       → try_detect_asset decrypts detection segment
       → Identifies transactions with regulated asset
       → Sends tx list to Orbis for re-encryption
       → Orbis re-encrypts to issuer's static key
       → Issuer decrypts with their private key
```

### Access Tiers

| Tier | Daily Scalar | Access |
|------|--------------|--------|
| Detection | dk_det | Asset ID only |
| Core | dk_det + dk_core | + Amount, self address |
| Full | All dk | + Counterparty address |

## Key Summary

| Key | Type | Holder | Purpose |
|-----|------|--------|---------|
| MK | Scalar | Orbis | Master key |
| UK | Scalar | Orbis | User key |
| AK | Point | Registry (public) | Address key |
| DK | Point | Public | Daily key (encryption) |
| dk | Scalar | Orbis | Daily scalar (decryption) |

## Source Files

| Component | Location |
|-----------|----------|
| Client compliance | `view/src/client_compliance.rs` |
| Planner enrichment | `view/src/planner.rs` |
| Registry | `compliance/src/registry.rs` |
