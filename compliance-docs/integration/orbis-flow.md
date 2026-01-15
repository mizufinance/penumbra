# Orbis Flow

End-to-end overview of the Orbis key management and re-encryption flow.

## Key Hierarchy

```
Asset Master Key (AMK) - Orbis only
       │
       ▼ Hash(AMK, user_id), one-way
   User Key (UK) - Orbis only
       │
       ▼ UK + secret_n, linear (one secret per address)
  Address Key (AK) - given to user address
```

Detection keys follow the same hierarchy with a separate Detection Master Key (DMK).

---

## 1. Issuer Setup

Issuer onboards a regulated asset with Orbis.

```
Issuer → Creates ring parameters for asset
       → Orbis generates Asset Master Key (AMK)
       → Orbis generates Detection Master Key (DMK)
       → Issuer registers asset on-chain (Penumbra only)
```

Issuer also registers a static public key with Orbis for re-encryption.

---

## 2. User Registration (KYC) ----- TO REVISIT (DefraDB??)

User completes KYC to transact with the regulated asset.

```
User   → Completes KYC with Orbis
       → Orbis stores KYC information
       → UK = hash(AMK, user_id), no need to store
```

The hash derivation is one-way: UK cannot reveal AMK.

---

## 3. Address Key Generation (With signature)

User requests an address key for their Penumbra address.

```
User   → Requests address key from Orbis
       → Orbis generates secret_n (one per address)
       → Orbis computes: AK = UK + secret_n
       → Orbis (Or DefraDB ---) stores secret_n with KYC data
       → User receives AK for their address
```

Multiple addresses can be generated for the same user:
- `AK_1 = UK + secret_1`
- `AK_2 = UK + secret_2`

Linear derivation allows Orbis to decrypt any AK ciphertext by looking up the corresponding secret. Per-address secrets prevent address linkability (see below).



---

## 4. Address Registration (penumbra-only)

User registers their address key on-chain, with the Orbis signature.

```
User   → Derives Address Compliance Key: ACK = AK * B_d
       → Submits RegisterUser { address, ACK, asset_id }
       → User's compliance leaf added to on-chain registry
```

---

## 5. Detection (Issuer Scanning)

Issuer scans for transactions involving their asset.

```
Issuer → Fetches detection key from Orbis (1 detection key, or 1 per day)
       → Scans on-chain transactions
       → Identifies transactions involving the regulated asset
       → Compiles list of transactions of interest
```

---

## 6. Re-encryption

Issuer requests decryption of identified transactions.

```
Issuer → Sends transaction list to Orbis (Along with list of permissions, derivation path)
       → Orbis re-encrypts data to issuer's static public key
       → Issuer receives re-encrypted ciphertexts
       → Issuer decrypts with their private key
```


---

Points to reconsider/consider
-KYC data (probably not Orbis, most likely defra?)
-Same for address secret, which would be with KYC data
-How does the address key gets generated? We need to ask Orbis to generate an address key from AMK, user_id, and secret_n. user_id and secret_n would be kept in defra db with KYC, the user does not have access to it.
-Audit part/warrant, (just don't know)
-Namespace (Orbis)
-What is Orbis bulletin
-How do we share the secrets (transactions), Orbis fetch them, Issuer sends them to Bulletin or ring directly
-Orbis does not derive any keys at any point in key generatoin. You send key derivation of the public side when you want to decrypt.
