# Roadmap


## 1. Governance


| Item |
|------|
| Defra integration |
| Asset registration verification |
| User registration verification |
| IBC channel whitelist tests |


## 2. Security


| Item | Description |
|------|-------------|
| ZK & ciphertext analysis | In-depth analysis of circuit security and ciphertext format. Verify no information leaks, proof soundness, encryption correctness. |

---

## 3. Testing & Benchmarks

Validates correctness and performance of everything above.

### 3.1 Smoke Tests

| Item | Description |
|------|-------------|
| Registration smoke tests | Asset + user registration flows in end-to-end smoke test suite |
| Transfer smoke tests | Regulated + unregulated asset transfers through full devnet |
| Scanning smoke tests | Issuer scanning, detection, flagging validation end-to-end |
| Edge case smoke tests | Unregistered users, stale timestamps, invalid anchors, blocked action types |

### 3.2 Benchmarks

| Item | Description |
|------|-------------|
| Action overhead | Compliance cost per action type (Spend, Output) vs vanilla |
| Simple tx baseline | Spend + Output baseline transaction timing |
| Proof verification | Validator overhead for compliance proof verification |
| Proof generation | Client overhead for compliance proof generation |
| Registry sync | Compliance tree sync cost (events, local storage) |
| Scanner throughput | Transactions scanned per second, ECDH operations per block |
| Load testing | Scale testing with millions of registrations, high tx throughput scanning |
| Orbis throughput | How many ACK per user can it handle? |

---

## 4. Improvements

Modular items that don't affect cross-cutting code. Can be worked on
independently after the foundation is stable.

| Item | Description |
|------|-------------|
| Scanner speedup | Optimize compliance scanning performance (batch ECDH, parallel processing) |
| Scanner persistence | Support historical scanning (resume from checkpoint) and live listening mode |
| UX review | Analyze registration, TX flows, scanning for UX improvements |
| Comment & variable review | Look for outdated or inaccurate comments, naming inconsistencies |
| Private user leaf | Implement private user leaf — register without revealing ACK publicly. Requires RPC calls to fetch leaf info for counterparties. |
| Update registry | Ability to update existing leaves (change threshold, rotate DK, update ACK) |
| Tree pruning / archival | Strategy for QuadTree growth (currently unbounded). Archive old leaves, prune historical anchors beyond retention window. Is fixed tree hieght ok? |
| Multiple ACK support | Find a way to get a UCK for decrypting multiple ACK with Orbis (using MPC and PRE). Or explore alternatives: per-ACK tags, Fuzzy Message Detection (FMD), or similar scheme. Currently MCK = UCK, so all ACKs derive from the same key. Orbis must brute-force ACK matching (try every registered ACK) when re-encrypting. Limit number of ACK per user?|

---

## 5. Future

Priority less.

| Item | Description |
|------|-------------|
| Whitelist for compliance threshold | Whitelist specific addresses to bypass flagging threshold (e.g., known institutional addresses). |
| Swap / DEX support | Extend compliance ciphertexts to cover Swap actions. Requires solving the unknown-counterparty problem for DEX fills. |
| Key rotation protocol | Protocol for rotating UCK, ACK, DK without breaking historical access. |
| Asset types | Support for RWA, NFT, ERC20-bridged assets with compliance. |
| User revocation / de-registration | Protocol for removing a user's registration (e.g., sanctions, KYC expiry). Requires governance approval flow. Maybe a blacklist? Something to freeze assets |
| IP hiding | Transaction propagation privacy via Nym, Tor, or mixnet integration. Prevents network-level correlation of transactions to IP addresses. |
|Granularity receiver sender access|
