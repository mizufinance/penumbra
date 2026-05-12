# Compliance Flow

Penumbra compliance gives issuers selective visibility into regulated-asset
activity while leaving unregulated assets on the normal private path. The chain
still validates asset integrity with Penumbra circuits; Orbis/ACP/Defra are
confidentiality and authorization services, not balance-integrity authorities.

External systems:

- **Orbis**: MPC ring key, encrypted-seed storage, and PRE for authorized audit.
- **Defra**: off-chain KYC document storage.
- **SourceHub**: ACP policies, bulletin board, and Defra proof verification.

For low-level formats, schema, and source files, see `reference.md`.

## Registration

1. **Ring creation**: issuer creates an Orbis ring.

```text
Issuer -> Orbis DKG
  -> ring_pk public on Penumbra
  -> sk_ring threshold-shared inside Orbis
```

2. **Asset registration**: issuer creates SourceHub policy metadata and submits
   `RegisterAsset` on Penumbra.

```text
AssetPolicy {
  dk_pub,
  ring_pk,
  threshold,
  allowed_channels,
  ring_id,
  policy_id,
  permission,
  resource
}
```

Regulated assets are inserted into the indexed asset tree. Unregistered assets
are treated as unregulated through non-membership proofs. Channel whitelist
enforcement is first-hop only and immutable after registration.

3. **User registration**: user completes KYC with Defra, publishes a hidden-doc
   proof through SourceHub/Orbis, then registers a `(address, asset)` compliance
   leaf on Penumbra.

```text
d   = SHA256("elgamal-derivation-v1\0\0" || B_d)
ACK = d * ring_pk
```

`d` is stored in the compliance leaf. `ACK` is derivable from the public address
diversifier and the asset's `ring_pk`; it is not stored.

## Transfer

Users run normal transfers:

```bash
pcli tx transfer --to <recipient> <amount><asset>
```

The planner detects regulated assets and adds the transfer compliance bundle.
Both sender and receiver must have compliance leaves for the regulated asset.

```text
planner:
  fetch sender/receiver compliance leaves
  fetch AssetPolicy
  derive sender/receiver ACKs
  set is_flagged = amount >= threshold
  create one receiver-output compliance ciphertext
```

The receiver output carries a unified transfer compliance ciphertext and a DLEQ
bundle. Inputs and change outputs carry no compliance ciphertext.

| Tier | Content | Unflagged Encryption | Flagged Encryption |
|------|---------|----------------------|--------------------|
| Detection | asset id, flag, salt | `DK_pub` | `DK_pub` |
| Sender core | amount | sender ACK | `DK_pub` |
| Sender ext | receiver address | sender ACK | `DK_pub` |
| Output core | amount | receiver ACK | `DK_pub` |
| Output ext | sender address | receiver ACK | `DK_pub` |

Detection is always issuer-DK decryptable. For flagged transfers, every audit
tier is issuer-DK decryptable. For unflagged transfers, audit tiers require
authorized Orbis PRE.

The transfer circuit owns value/nullifier/note/balance soundness. Compliance
owns asset-policy binding, threshold flag correctness, ciphertext construction,
detection tag correctness, tier metadata, and DLEQ binding. See:

- `docs/compliance/constraint-checklist.md`
- `docs/transfer-circuit/constraint-checklist.md`

## Scanner And Audit Pipeline

The scanner DB is the spine. It is not a stage. Scanning, screening, evidence
validation, decryption, audit projection, and exporters all share keyed rows.

```text
Chain
  -> Scan: extract raw OutputRef ciphertexts and clear public flows
  -> Scanner DB spine
  -> Screen: detection-tier DK decrypt marks detected / irrelevant / invalid
  -> Validate evidence: persisted ciphertext + upload bundle + policy/ring binding
  -> Decrypt audit tiers per detected output:
       flagged:   full-tier issuer DK decrypt
       unflagged: Orbis PRE decrypt
  -> Audit ledger projection
  -> Exporters: audit-demo JSON, reports, Orbis audit input
```

`ComplianceScreener` is pure. It parses transfer ciphertexts and DK-decrypts the
detection tier only. It does not persist, fetch blocks, call Orbis, consult ACP,
or mutate audit state.

```text
ExtractedComplianceCiphertext
  -> Irrelevant
  -> Detected(DetectionEvent)
  -> InvalidCiphertext
```

The scanner is reorg-safe: each block row stores `height`, `block_hash`, and
`parent_hash`. A parent mismatch rolls back to the common ancestor and replays.
Invalid ciphertext persistence is capped per block.

```bash
pcli tx compliance scan run \
  --node <url> \
  --db /path/to/compliance-scanner.db \
  --dk-hex <hex> \
  --scan-asset-id <id>

pcli tx compliance scan catch-up \
  --node <url> \
  --db /path/to/compliance-scanner.db \
  --dk-hex <hex> \
  --scan-asset-id <id>
```

## Audit Branches

Detected private rows start as `pending`. Audit completion requires validated
evidence first.

```text
pending -> evidence_valid
pending -> evidence_invalid
evidence_invalid -> evidence_valid
evidence_valid -> decrypt_failed
decrypt_failed -> audit_complete
evidence_valid -> audit_complete
audit_complete -> audit_complete
```

Forbidden:

```text
pending -> audit_complete
evidence_invalid -> audit_complete
```

### Flagged

If `amount >= threshold`, all tiers are encrypted to `DK_pub`. The issuer can
decrypt locally after evidence validates. Orbis is not used.

### Unflagged

Only the detection tier decrypts locally. Audit tiers require governance/ACP
authorization and Orbis PRE. Each tier has an independent encrypted-seed upload
package and independent PRE path.

```text
ACP grant
  -> Orbis validates stored encrypted-seed package and policy metadata
  -> issuer requests PRE for authorized tier
  -> issuer recovers tier seed
  -> issuer decrypts Penumbra tier payload locally
```

Audit-demo and reports are exporters over the scanner DB. The frontend state
shape remains `scan`, `scanner`, `ledgerRows`, and `audits`; backend state comes
from the DB.
