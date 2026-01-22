# Roadmap

## 1. Local Sync

| Item | Description |
|------|-------------|
| Tree sync via events | Subscribe to registration events, update local cache |
| Path caching | Store auth paths in SQLite (tables exist, need impl) |
| Anchor refresh | Lightweight RPC for roots before submission |
| Offline TX building | Use cached paths + fresh anchor |

## 2. Orbis integration

| Item | Description |
|------|-------------|
| Key derivation update | Separate UK_det and UK_enc (detection isolation) |
| Orbis handoff | Key management integration (currently local MCK from spend seed) |
| Per-address registration | Each address key from Orbis, Verify Orbis signature |
| Asset registration | Verify asset registration |

## 3. Security

| Item | Description |
|------|-------------|
| BLACK_HOLE_ACK audit | Verify usage patterns |
| ZK circuit verification | Formal verification |
| Randomness | Verify usage of randomness in ciphertext |
| IMT security review | Non-membership soundness |


## 4. Client UX

| Item | Description |
|------|-------------|
| UX review | Analyze registration and TX flows |
| Clear errors | Descriptive failure messages |
| Idempotent registration |  Handle duplicate registrations |

## 5. Benchmarks

| Item | Description |
|------|-------------|
| Action overhead | Compliance cost per action type |
| Simple tx | Spend + Output baseline |
| Proof verification | Validator overhead |
| Proof generation | Client overhead |
| Tree sync | Compliance sync cost |
| Scanner throughput | Txs per second |

## 6. Future

| Item | Description |
|------|-------------|
| Threshold/whitelist | Per-asset limits |
| Swap support | DEX integration |
| Key rotation | Protocol for rotation |
| Asset types | RWA, NFT, ERC20 support |
