# Ciphertext Design

## Dual Ciphertext Architecture

Each transfer creates TWO compliance ciphertexts:
1. **Sender ciphertext** (on Spend): Encrypted to sender's keys
2. **Receiver ciphertext** (on Output): Encrypted to receiver's keys

Both contain the same data but encrypted for different parties.

## Wire Format (256 bytes)

| Field | Bytes | Description |
|-------|-------|-------------|
| EPK | 32 | Ephemeral public key `EPK = r * B_d` |
| Detection | 32 | 1 Fq - encrypted asset_id |
| Core | 96 | 3 Fq - encrypted (amount + self address) |
| Extension | 96 | 3 Fq - encrypted counterparty address |

## 3-Tier Encryption

Each segment uses a different daily key (DK):

| Segment | Daily Key | Content |
|---------|-----------|---------|
| Detection | DK_det = AK + T_det * B_d | asset_id |
| Core | DK_core = AK + T_core * B_d | amount + self address |
| Extension | DK_ext = AK + T_ext * B_d | counterparty address |

### Encryption (Client)

Client knows: AK (public), T (public tweak), B_d

```
1. Generate ephemeral: r, EPK = r * B_d
2. Derive daily public keys:
   DK_det = AK + T_det * B_d
   DK_core = AK + T_core * B_d
   DK_ext = AK + T_ext * B_d
3. Compute shared secrets:
   S_det = r * DK_det
   S_core = r * DK_core
   S_ext = r * DK_ext
4. Derive seeds: seed_X = hash(S_X, EPK)
5. Encrypt: C[i] = plaintext[i] + hash(seed_X, i)
```

### Decryption (Orbis)

Orbis knows: UK (secret)

```
1. Derive daily scalars:
   dk_det = UK + T_det
   dk_core = UK + T_core
   dk_ext = UK + T_ext
2. Compute shared secrets:
   S_det = dk_det * EPK
   S_core = dk_core * EPK
   S_ext = dk_ext * EPK
3. Derive seeds: seed_X = hash(S_X, EPK)
4. Decrypt: plaintext[i] = C[i] - hash(seed_X, i)
```

## Access Tiers

| Tier | Daily Scalar Shared | Access |
|------|---------------------|--------|
| Detection | dk_det | Asset ID only |
| Core | dk_det + dk_core | + Amount, self address |
| Full | All dk | + Counterparty address |

## BLACK_HOLE_AK

For unregulated assets:

```rust
BLACK_HOLE_AK = Element::GENERATOR
```

Ciphertext is valid but nobody can decrypt (no corresponding UK).

## Source Files

| Component | Location |
|-----------|----------|
| Encryption | `compliance/src/crypto.rs` |
| Structs | `compliance/src/structs.rs` |
