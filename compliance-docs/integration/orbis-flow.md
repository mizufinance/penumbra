# Orbis Flow

Key management and re-encryption flow with Orbis.

## Key Hierarchy

```
MK (Orbis ring, scalar)
 │
 └── UK = Hash(MK, user_id)     [Orbis only, scalar]
      │
      ├── AK = UK * B_d          [Registry, public point]
      │    │
      │    └── DK = AK + T * B_d [Public point, for encryption]
      │
      └── dk = UK + T            [Orbis only, scalar, for decryption]
```

See [key-hierarchy.md](../architecture/key-hierarchy.md) for details.

## 1. Issuer Setup

```
Issuer → Creates ring parameters
       → Orbis generates MK
       → Issuer registers asset on-chain
       → Issuer registers static public key with Orbis for re-encryption
```

## 2. User Registration (KYC)

```
User → Completes KYC with Orbis
     → Orbis derives UK = Hash(MK, user_id)
     → KYC data stored (DefraDB TBD)
```

## 3. Address Key Generation

```
User → Requests address key from Orbis (provides B_d)
     → Orbis computes AK = UK * B_d
     → User receives AK for their address
     → User registers AK on-chain
```

Multiple addresses per user:
- `AK_1 = UK * B_d1`
- `AK_2 = UK * B_d2`

Single UK can decrypt all addresses (via dk = UK + T).

## 4. Transaction Encryption (Client)

```
Client → Fetches AK from registry (public)
       → Derives DK = AK + T * B_d (daily public key)
       → Encrypts: S = r * DK
       → Stores EPK = r * B_d in ciphertext
```

## 5. Detection (Issuer Scanning)

```
Issuer → Fetches dk (daily scalar) from Orbis
       → dk = UK + T (Orbis computes this)
       → Scans blocks: S = dk * EPK
       → Decrypts detection segment to identify regulated assets
       → Compiles list of transactions of interest
```

## 6. Re-encryption

```
Issuer → Sends transaction list to Orbis
       → Orbis re-encrypts to issuer's static key
       → Issuer decrypts with their private key
```

## Key Summary

| Key | Type | Holder | Purpose |
|-----|------|--------|---------|
| MK | Scalar | Orbis ring | Master key |
| UK | Scalar | Orbis | User key (per user) |
| AK | Point | Registry (public) | Address key (per address) |
| DK | Point | Public | Daily key (client encryption) |
| dk | Scalar | Orbis | Daily scalar (decryption) |

## Open Items

- KYC data storage (DefraDB?)
- Orbis bulletin integration
- Warrant/audit flow
- Namespace management
