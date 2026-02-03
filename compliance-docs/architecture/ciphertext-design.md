# Ciphertext Design

## Dual Ciphertext Architecture

Each transfer creates TWO compliance ciphertexts:
1. **Sender ciphertext** (on Spend): Encrypted to sender's keys
2. **Receiver ciphertext** (on Output): Encrypted to receiver's keys

Both contain the same data but encrypted for different parties.

## Wire Format (288 bytes)

| Field | Bytes | Description |
|-------|-------|-------------|
| EPK | 32 | Ephemeral public key `EPK = r * B_d` (for user ECDH) |
| EPK_G | 32 | Ephemeral public key `EPK_G = r * G` (for issuer ECDH) |
| Detection | 32 | 1 Fq - encrypted (asset_id + flag bit) to issuer |
| Core | 96 | 3 Fq - encrypted (amount + self address) |
| Extension | 96 | 3 Fq - encrypted counterparty address |

## 3-Tier Encryption

| Segment | Encrypted To | Domain | Content |
|---------|--------------|--------|---------|
| Detection | Issuer's DK_pub | ISSUER_DETECTION_DOMAIN | asset_id with flag in bit 252 |
| Core | User's DCK (unflagged) or DK_pub (flagged) | COMPLIANCE_STREAM_CIPHER_DOMAIN | amount + self address |
| Extension | User's DCK (unflagged) or DK_pub (flagged) | COMPLIANCE_STREAM_CIPHER_DOMAIN | counterparty address |

### Encryption (Client)

Client knows: ACK (user public), DK_pub (issuer public from IMT), B_d, threshold

```
1. Generate ephemeral: r
2. Compute EPKs:
   EPK = r * B_d (for user)
   EPK_G = r * G (for issuer)
3. Compute is_flagged = (amount >= threshold)
4. Compute shared secrets:
   S_issuer = r * DK_pub (for detection, and core/ext if flagged)
   S_core = r * DCK_core (if not flagged)
   S_ext = r * DCK_ext (if not flagged)
5. Detection tier: encrypt (asset_id | flag<<252) with S_issuer
6. Core/Extension: encrypt with S_issuer (flagged) or user keys (unflagged)
```

### Decryption

**Issuer (Detection Tier):**
```
S_issuer = DK * EPK_G
seed = hash(ISSUER_DETECTION_DOMAIN, S_issuer, EPK)
plaintext = ciphertext - hash(seed, 0)
asset_id = plaintext & ~(1<<252)
is_flagged = (plaintext >> 252) & 1
```

**User (Core/Extension, unflagged):**
```
dck = UCK + T_type
S = dck * EPK
seed = hash(COMPLIANCE_STREAM_CIPHER_DOMAIN, S, EPK)
plaintext[i] = ciphertext[i] - hash(seed, i)
```

## Access Tiers

| Tier | Who Can Decrypt | Access |
|------|-----------------|--------|
| Detection | Issuer only | Asset ID + flag status |
| Core (unflagged) | User only | + Amount, self address |
| Core (flagged) | Issuer only | + Amount, self address |
| Extension (unflagged) | User only | + Counterparty address |
| Extension (flagged) | Issuer only | + Counterparty address |

## BLACK_HOLE_ACK

For unregulated assets, encryption uses a NUMS (Nothing-Up-My-Sleeve) point:

```rust
BLACK_HOLE_ACK = hash_to_curve("penumbra.compliance.black_hole_ack")
```

No one knows the discrete log, so ciphertext is effectively a dead letter.

## Source Files

| Component | Location |
|-----------|----------|
| Encryption | `compliance/src/crypto.rs` |
| Structs | `compliance/src/structs.rs` |
