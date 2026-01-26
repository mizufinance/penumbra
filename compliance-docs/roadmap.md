# Roadmap

## 1. Orbis integration

| Item | Description |
|------|-------------|
| Key derivation update | Separate UK_det and UK_enc (detection isolation) |
| Orbis handoff | Key management integration (currently local MCK from spend seed) |
| Per-address registration | Each address key from Orbis, Verify Orbis signature |
| Asset registration | Verify asset registration |

## 2. Security

| Item | Description |
|------|-------------|
| BLACK_HOLE_ACK audit | Replace Element::GENERATOR with NUMS point (hash-to-curve) |
| ZK circuit verification | Formal verification |
| Randomness | Verify usage of randomness in ciphertext |
| IMT security review | Non-membership soundness |

## 3. Client UX

| Item | Description |
|------|-------------|
| UX review | Analyze registration and TX flows |
| Clear errors | Descriptive failure messages |
| Idempotent registration |  Handle duplicate registrations |

## 4. Benchmarks

| Item | Description |
|------|-------------|
| Action overhead | Compliance cost per action type |
| Simple tx | Spend + Output baseline |
| Proof verification | Validator overhead |
| Proof generation | Client overhead |
| Tree sync | Compliance sync cost |
| Scanner throughput | Txs per second |

## 5. Future

| Item | Description |
|------|-------------|
| Threshold/whitelist | Per-asset limits |
| Swap support | DEX integration |
| Key rotation | Protocol for rotation |
| Asset types | RWA, NFT, ERC20 support |

## 6. Code Quality

| Item | Description |
|------|-------------|
| Scanner speedup | Optimize compliance scanning performance |
| Unify providers | Consolidate ComplianceProofProvider implementations |
| Proto deduplication | Remove duplicate view/compliance proto conversions |
| Error handling audit | Replace `.ok()` in manager.rs with explicit error handling for compliance anchors |
