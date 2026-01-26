# Key Hierarchy

## Key Types

| Key | Type | Derivation | Holder | Purpose |
|-----|------|------------|--------|---------|
| MK | Scalar | Random | Orbis ring | Master key |
| UK | Scalar | Hash(MK, user_id) | Orbis | User key (per user) |
| AK | Point | UK * B_d | Registry (public) | Address key (per address) |
| DK | Point | AK + T * B_d | Public | Daily key (for encryption) |
| dk | Scalar | UK + T | Orbis | Daily scalar (for decryption) |

## Derivation Tree

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

Where `T = Hash(key_type_domain, date)` is a public tweak.

## Encryption Flow (Client)

Client knows: AK (from registry), T (public), B_d (from address)

```
1. Derive daily public key:  DK = AK + T * B_d
2. Generate ephemeral:       r (random scalar)
3. Compute EPK:              EPK = r * B_d
4. Compute shared secret:    S = r * DK
5. Encrypt with S
```

## Decryption Flow (Orbis)

Orbis knows: UK (secret)

```
1. Derive daily scalar:      dk = UK + T
2. Compute shared secret:    S = dk * EPK
3. Decrypt with S
```

## Math Equivalence

```
DK = AK + T * B_d
   = UK * B_d + T * B_d
   = (UK + T) * B_d

Sender:  S = r * DK = r * (UK + T) * B_d
Orbis:   S = dk * EPK = (UK + T) * r * B_d
```

Both equal `r * (UK + T) * B_d` ✓

## Key Type Domains

Three separate key types with different domain separators:

| Type | Domain | Encrypts |
|------|--------|----------|
| Detection | `Hash("detection", date)` | Asset ID |
| Core | `Hash("core", date)` | Amount + self address |
| Extension | `Hash("extension", date)` | Counterparty address |

Each produces a different tweak T, resulting in different DK values.

## Multi-Address Support

Single UK decrypts all addresses for that user:
- Different addresses have different B_d (from diversifier)
- `AK1 = UK * B_d1`, `AK2 = UK * B_d2`
- Orbis uses same dk: `S = dk * EPK` works for any address

## Access Tiers

| Tier | Keys Shared | Access |
|------|-------------|--------|
| Detection | dk_det | Asset ID only |
| Core | dk_det + dk_core | + Amount, self address |
| Full | All dk | + Counterparty address |

## Security

- **UK never shared**: Only Orbis holds UK
- **AK is public**: Stored in registry, anyone can derive DK
- **Tree isolation** (future): Detection keys may be derived from a separate UK or MK tree to isolate detection from encryption. This doesn't change the math, only which UK is used for detection vs core/extension.

### Detection Key Scope Issue

Sharing a daily detection scalar (dk_det) currently allows detection of ALL transactions for that user on that day, not just a specific asset. This is because dk_det works across all addresses.

**Mitigation options:**
1. Rethink detection key design
2. We get a detection key per asset. Orbis does reencryption on all flagged tx, most of the reencrypted tx will be noise because there are not of the specific user.
3. Accept full user detection scope - issuer gets dk_det for their registered users only

## Source Files

- Key definitions: `crates/core/keys/src/keys/cvk.rs`
- POC: `crates/bench/tests/hierarchical_keys_poc.rs`
