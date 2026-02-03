# Roadmap

## 1. Orbis integration

| Item | Description |
|------|-------------|
| Key derivation update | Separate UK_det and UK_enc (detection isolation) |
| Orbis handoff | Key management integration (currently local MCK from spend seed) |
| Per-address registration | Each address key from Orbis, Verify Orbis signature |
| Asset registration | Verify asset registration |


## 2. Benchmarks

| Item | Description |
|------|-------------|
| Action overhead | Compliance cost per action type |
| Simple tx | Spend + Output baseline |
| Proof verification | Validator overhead |
| Proof generation | Client overhead |
| Tree sync | Compliance sync cost |
| Scanner throughput | Txs per second |

## 3. Improvements

| Item | Description |
|------|-------------|
| Scanner speedup | Optimize compliance scanning performance |
| Scanner persistence | Support historical scanning (resume from checkpoint) and live listening mode |
| ZK & ciphertext | In depth analysis of security and possible optimization |
| UX review | Analyze registration, TX flows, scanning for improvements  |
| Comment & Variable review | Look for outdated or inaccurate comments |
| Private user leaf | Implement private user leaf with calls to fetch leaf info |
| Update registry | Ability to update leaves in the registries |


## 4. Future

| Item | Description |
|------|-------------|
| Whitelist | Whitelist for compliance threshold |
| Swap support | DEX integration |
| Key rotation | Protocol for rotation |
| Asset types | RWA, NFT, ERC20 support |

