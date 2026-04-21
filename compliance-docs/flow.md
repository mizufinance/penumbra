# Compliance Flow

Penumbra compliance adds selective regulatory visibility for regulated assets.
Issuers can detect and audit transactions involving their assets, while
unregulated assets retain full vanilla privacy. The system uses threshold MPC
(Orbis), tiered encryption, and zero-knowledge proofs.

External systems: 
**Orbis** (MPC,PRE)
**Defra** (DefraDB storage for KYC),
**SourceHub** (Cosmoschain — ACP policies, bulletin board, verify Defra updates).

---

## 1. Ring Creation (DKG)

The issuer initiates distributed key generation with Orbis to create a
threshold-shared signing key for the asset.

```
Issuer → Orbis DKG
  → ring_pk = sk_ring × G   (public, stored on-chain)
  → sk_ring                  (threshold-shared across Orbis MPC nodes, never leaves Orbis)
```

`ring_pk` is the aggregate public key of the Orbis ring. `sk_ring` is used for
FROST signing (user registration) and PRE (decryption). One ring per
issuer/asset.

---

## 2. Asset Registration

The issuer creates a policy on SourceHub and registers the asset on Penumbra.

```
1. Issuer creates policy_id on SourceHub (ACP policy)
   → Defines KYC requirements, permissions, resources

2. Issuer registers ring on SourceHub bulletin
   → {ring_id, policy_id, permission, resource}

3. Issuer submits RegisterAsset tx to Penumbra
   → IMT inserts IndexedLeaf (sorted linked list)
   → AssetPolicy stored (immutable):
     {dk_pub, ring_pk, threshold, allowed_channels, policy_id}
```

```bash
pcli tx compliance register-asset <asset_id> --regulated \
  --dk-pub-hex <hex> --threshold <base_units> \
  [--allowed-channels channel-0,channel-3]
```

**Asset Registry (IMT)**: Indexed Merkle Tree with sorted linked list. It always
contains a structural zero-value sentinel and the protocol also seeds the
neutral base asset as an explicit unregulated entry. Additional regulated assets
are inserted explicitly. Membership proof: `leaf.value == asset_id`.
Non-membership (gap) proof: `low.value < asset_id < low.next_value`. Both use
identical circuit structure — validators cannot distinguish them.

**IBC channel whitelist**: Empty = IBC blocked entirely. Non-empty = only listed
channels allowed. Unregulated = no restrictions. First-hop enforcement only.
Immutable after registration.

**Chain action surface** for regulated assets: `Transfer`, `Consolidate`,
`Split`, `ProposalSubmit`, `ValidatorVote`, `ValidatorDefinition`,
`ShieldedIcs20Withdrawal` (subject to channel whitelist), and compliance
registration actions. DEX, staking, community-pool, delegator-vote, and
proposal-withdraw/deposit-claim actions are removed from this chain.

Unregistered assets default to unregulated.

---

## 3. User Registration

Users register per (address, asset) pair after completing KYC with Defra.
Both sender and receiver must be registered for a regulated transfer.

### ACK Derivation

Single derivation scalar per address:

```
d = SHA256("elgamal-derivation-v1\0\0" || b_d_fq)   per-address scalar
ACK = d × ring_pk                                    per-address, publicly computable
```

`d` is stored in the ComplianceLeaf. ACK is not stored — it is derivable from
`ring_pk` (in AssetPolicy) and `B_d` (from the address). Different addresses
produce different `d` values and therefore different ACKs. Different assets use
different `ring_pk`, so ACKs differ across assets even for the same address.

Transfer uses sender and receiver ACKs across its transfer compliance bundle.
`Split` and `Consolidate` do not carry compliance ciphertext.

For unregulated assets, Penumbra uses two fixed protocol sink public keys:
one for issuer detection routing and one for ring/ACK routing. Both are
domain-separated hash-to-curve points with no corresponding private key, so
the issuer ciphertext keeps the same wire shape but is effectively undecryptable.
IMT non-membership proof still proves unregulated status.

### ZK Proof of Storage

Registration requires a zero-knowledge proof that the user's `B_d` is stored
in a KYC document linked to the ring's `policy_id`, without revealing the
document identity.

```
Public:  B_d, policy_id, sourcehub_state_root
Private: doc_id, merkle_paths

Proves:
  1. B_d exists in document(doc_id) in DefraDB
  2. (policy_id, "kyc", doc_id, "verified") exists in ACP state on SourceHub
```

The `doc_id` stays hidden — no one can link multiple `B_d` values to the same
KYC document.

### Flow

```
1. Alice completes KYC with Defra
   → Defra stores KYC in DefraDB → doc_id (private, only Alice and Defra know it)

2. Alice appends B_d to her KYC document
   → Defra generates ZK proof: B_d exists in a doc under policy_id (doc_id hidden)
   → Defra publishes proof on SourceHub

3. Alice publishes (B_d, policy_id) on the Orbis bulletin
   → Orbis verifies proof on SourceHub
   → Orbis FROST-signs the bulletin post

4. Alice submits RegisterUser tx to Penumbra
   → Penumbra verifies FROST sig against ring_pk (from AssetPolicy)
   → ComplianceLeaf {address, asset_id} stored in QuadTree
```

```bash
pcli tx compliance register-user <asset_id> --address-index <index>
```

**User Registry (QuadTree)**: Arity-4, depth-16, Poseidon377 hashing. Max ~4
billion leaves. Both registries emit historical anchors per block (like SCT), so
proofs remain valid across tree updates.

**Trust chain**: Defra verifies KYC → generates ZK proof → publishes on
SourceHub. Orbis verifies proof → FROST-signs bulletin post. Penumbra verifies
FROST sig → stores leaf.

| Scenario | Result |
|----------|--------|
| Asset not registered | Unregulated. IMT non-membership proof. Full privacy. |
| User not registered | Transaction rejected. |
| Sender registered, receiver not | Transaction rejected (transfer compliance check fails). |

---

## 4. Transfer

The user runs the same command as vanilla Penumbra:

```bash
pcli tx transfer --to <recipient> <amount><asset>
```

No compliance-specific flags needed. The planner detects whether the asset is
regulated and handles compliance automatically.

### Planner

1. Fetches sender + receiver ComplianceLeaf from QuadTree
2. Looks up `dk_pub`, `ring_pk`, `threshold` from AssetPolicy (IMT)
3. Derives ACKs: `ack_receiver = d_receiver × ring_pk`, `ack_sender = d_sender × ring_pk`
4. Computes `is_flagged = (amount >= threshold)`
5. Generates the transfer compliance ephemeral scalars and EPKs

### Ciphertext Construction

Transfer compliance uses the transfer ciphertext and transfer DLEQ bundle.
The receiver leg carries the compliance bytes; sender-owned change outputs carry none.

### 4-Tier Encryption

| Tier | Content | Encrypted To (unflagged) | Encrypted To (flagged) |
|------|---------|--------------------------|------------------------|
| Detection | asset_id + flag + salt | DK_pub (always) | DK_pub (always) |
| Core | amount + self address | ACK via ElGamal | DK_pub |
| Extension | counterparty address | ack_receiver via ElGamal | DK_pub |
| Sext | sender's counterparty | ack_sender via ElGamal | DK_pub |

Each non-detection tier uses an ElGamal envelope (c2) containing the encrypted
seed, plus stream-cipher-encrypted data. The issuer unlocks c2 via Orbis PRE
to recover the stream cipher seed.

**Flagging**: When amount >= threshold, all tiers encrypt to DK_pub directly.
The issuer decrypts flagged transactions without Orbis.

Each transfer compliance tier is bound independently with its own salt and DLEQ
statement material.

### DLEQ Proof

Each ciphertext carries an in-circuit DLEQ proof binding it to policy metadata.
The proof is computed inside the SNARK and output as `(c, s)` per tier.

```
S    = r × ACK
R    = k × G
R'   = k × ACK
M    = Poseidon(policy_id_hash, resource_hash, permission_hash, tier, target_timestamp, salt)
c    = Poseidon(ACK, EPK, S, R, R', M)    Fiat-Shamir challenge
s    = k + c × r                           response

Public outputs: (c, s) per tier
```

`salt` is random, encrypted in the detection tier (only issuer's DK can decrypt).
`target_timestamp` is Unix UTC seconds, set by client to `now()`, validator
enforces ±1 hour of block time.

Transfer exposes one DLEQ proof per transfer compliance tier.

For unregulated assets, DLEQ uses zeroed policy fields — valid but useless
(no Orbis ring exists).

### ZK Proofs and Validation

**Transfer compliance circuit** validates: QuadTree membership,
IMT membership/non-membership, encryption correctness, flag correctness,
per-tier DLEQ proofs, and binding to `{policy_id, permission, resource}`.

**Split** and **Consolidate** do not participate in compliance encryption or
binding checks.

**Stateful checks** (validator): compliance_anchor in history, asset_anchor in
history, target_timestamp within ±1hr of block time, and FROST signature valid
for `ComplianceRegisterUser`.

Ciphertexts are stored on-chain after broadcast.

---

## 5. Detection Scanning

The issuer scans the chain using their static detection key (DK). No Orbis
involvement.

```
For each compliance ciphertext:
1. Read EPK from ciphertext (on G)
2. Compute S = DK × EPK
3. Derive seed, decrypt detection tier
4. If valid asset_id: match found
5. Check flag bit: is_flagged = (plaintext >> 252) & 1
```

```bash
pcli tx compliance scan --dk-hex <hex> --scan-asset-id <id> \
  --node <url> --start-height <height>
```

Detection tier is always encrypted to DK_pub, so the issuer always gets:
asset_id, flag status, and salt.

**Flagged** (amount >= threshold): All tiers encrypted to DK_pub. Issuer
decrypts everything directly. Done.

**Unflagged** (amount < threshold): Only detection tier decryptable. Issuer
stores transaction references (block height, tx hash, action index, EPK values)
for decryption via governance + Orbis PRE.

---

## 6. Decryption (Governance + Orbis PRE)

For unflagged transactions, the issuer must obtain governance approval before
Orbis will re-encrypt. Each tier requires a separate PRE call (independent r
prevents cross-tier derivation).

### Step 1: Governance Grant

```
Governance grants ACP permission on SourceHub:
  → (issuer, user_address, [core/ext/sext], scope)
  → Stored as ACP relationship
```

### Step 2: Issuer Setup + PRE Request

One-time setup per (user, ring): issuer stores a dummy secret via Orbis
`store_secret` → receives `enc_cmt_orbis`. This is reused for all PRE calls
for that user.

```
Issuer posts PRE request to Orbis:
  → adjusted_pk = pk_issuer + EPK_tier - enc_cmt_orbis   (see Step 4)
  → B_d (from user's ComplianceLeaf)
  → tier label ("core", "ext", or "sext")
  → (c, s) from the DLEQ proof on-chain
  → M (metadata hash, computed by issuer using own policy fields + decrypted salt)
  → ACP permission link
  → object_id (from store_secret)
```

### Step 3: Orbis Verification

```
ACP verifies: permission exists and is in scope
Orbis verifies: policy_id matches ring, ACP permission is valid
Orbis verifies DLEQ:
  S = d × sk_ring × EPK                              (MPC, same computation as PRE)
  ACK = d × ring_pk
  R_rec  = s × G   - c × EPK       must equal R
  R'_rec = s × ACK - c × S         must equal R'
  c_check = Poseidon(ACK, EPK, S, R_rec, R'_rec, M)  must equal c
```

DLEQ verification is free — Orbis already computes S as part of PRE.

### Step 4: PRE (Adjusted Reader Key)

Chain EPKs are already on-chain and cannot be encrypted through the Orbis
`encrypt_secret` pipeline. The **adjusted reader key trick** bridges them:

```
d        = SHA256("elgamal-derivation-v1\0\0" || b_d_fq)
EPK_tier = r_tier × G                                     (from ciphertext)

Issuer computes adjusted reader key:
  adjusted_pk = pk_issuer + EPK_tier - enc_cmt_orbis

Each Orbis node computes:
  d × sk_i × (adjusted_pk + enc_cmt_orbis)
= d × sk_i × (pk_issuer + EPK_tier)                       (enc_cmt_orbis cancels)

Threshold combination:
  xnc_cmt = d × sk_ring × (pk_issuer + EPK_tier)
```

Orbis uses `--xnc-only` mode: it returns `xnc_cmt` without attempting AES
decryption (which would fail since the dummy secret doesn't match the chain
ciphertext).

For core/ext: use receiver's `d`. For sext: use sender's `d`.

### Step 5: Issuer Recovers Seed

```
ACK = d × ring_pk                                    (public, derivable)

P = xnc_cmt - sk_issuer × ACK
  = d × sk_ring × (pk_issuer + EPK_tier) - sk_issuer × d × ring_pk
  = d × sk_ring × pk_issuer + d × sk_ring × EPK_tier - d × sk_ring × sk_issuer × G
  = d × sk_ring × EPK_tier                           (pk_issuer terms cancel)
  = r_tier × ACK                                      (matches C2 encryption)

seed = C2 - P.compress()
```

**One Orbis call per (user, tier).** Cross-tier isolation is both
cryptographically enforced (DLP on independent r) and policy-enforced (ACP
governance per tier).

### Access Summary

| Tier | Content | Flagged | Unflagged |
|------|---------|---------|-----------|
| Detection | asset_id + flag + salt | Direct (DK) | Direct (DK) |
| Core | amount + self address | Direct (DK) | Governance + Orbis PRE |
| Extension | counterparty address | Direct (DK) | Governance + Orbis PRE |
| Sext | sender's counterparty | Direct (DK) | Governance + Orbis PRE (EPK_3, sender's d) |
