# Key Hierarchy

## User Keys

| Key | Derivation | Holder | Purpose |
|-----|------------|--------|---------|
| UCK | Hash(RingMaster, user_id) | Orbis | Per-user secret (scalar) |
| ACK | UCK × B_d | Registry | Per-address public key (point) |
| DCK | dck = UCK + T (scalar), DCK_pub = ACK + T × B_d (point) | Orbis/Public | Daily encryption/decryption |

Where T = Hash(key_type_domain, date) is a public tweak.

## Issuer Keys

| Key | Derivation | Holder | Purpose |
|-----|------------|--------|---------|
| MCK | Random | Orbis | Per-issuer master (scalar) |
| DK | Standalone | Issuer | Per-asset detection (scalar) |

DK is standalone (not derived from MCK) - Orbis shares DK with issuer.

## Encryption

**To User (Core/Extension):**
- Sender: S = r × DCK_pub, EPK = r × B_d
- Orbis: S = dck × EPK

**To Issuer (Detection):**
- Sender: S = r × DK_pub, EPK_G = r × G
- Issuer: S = DK × EPK_G

## Key Types

| Type | Encrypts |
|------|----------|
| Detection | asset_id (to issuer) |
| Core | amount + self address |
| Extension | counterparty address |

## Conditional Encryption

| Flag | Core + Extension encrypted to |
|------|-------------------------------|
| 0 | User's DCK |
| 1 | Issuer's DK |

Issuer always decrypts detection tier. Issuer only decrypts full details when flagged.

## Source Files

- User keys: `crates/core/keys/src/keys/cvk.rs`
- Issuer keys: `crates/core/component/compliance/src/issuer_keys.rs`
