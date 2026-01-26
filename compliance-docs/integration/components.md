# Component Integration

## Action Types

**Modified** (carry compliance ciphertexts):
- Spend, Output

**Blocked** (reject regulated assets):
- Swap, SwapClaim, Delegate, Undelegate, UndelegateClaim
- IbcRelay, Ics20Withdrawal
- ProposalSubmit/Withdraw/Vote/DepositClaim
- PositionOpen/Close/Withdraw
- CommunityPoolSpend/Deposit/Output

**New** (compliance actions):
- RegisterAsset `{ asset_id, is_regulated }`
- RegisterUser `{ leaf, signature }`

## Transaction Structure

Spend and Output bodies now include:

```rust
compliance_ciphertext: Vec<u8>,       // 256 bytes
target_timestamp: u64,                // Unix timestamp
sender_leaf_hash: Fq,                 // Blinded sender
counterparty_leaf_hash: Fq,           // Blinded counterparty
compliance_anchor: StateCommitment,   // User tree root
asset_anchor: StateCommitment,        // Asset tree root
```

## Validator Verification

**Stateless:**
- Extended ZK proof with compliance inputs

**Stateful:**
- `target_timestamp` within 1 hour of block time
- `compliance_anchor` matches chain state
- `asset_anchor` matches chain state
- Nullifier unspent (Spend only)

## Client State

**Local (SQLite):**
- Pruned SCT, spendable notes, nullifiers
- Compliance trees (planned, like SCT)

**Queried (RPC):**
- Compliance anchors and Merkle proofs
- User leaves and regulation status

## Module Changes

| Module | Changes |
|--------|---------|
| `keys/` | New compliance key types in `cvk.rs` |
| `compliance/` | Registry, crypto, trees, r1cs circuits |
| `shielded-pool/` | Extended Spend/Output plans, proofs, actions |
| `transaction/` | RegisterAsset, RegisterUser actions |
| `view/` | `enrich_with_compliance()`, ViewClient methods |
| `pcli/` | compliance subcommands |

## Source Files

| Component | Location |
|-----------|----------|
| Key hierarchy | `keys/src/keys/cvk.rs` |
| Compliance component | `component/compliance/src/` |
| Spend/Output | `component/shielded-pool/src/{spend,output}/` |
| Planner | `view/src/planner.rs` |
| Client compliance | `view/src/client_compliance.rs` |
