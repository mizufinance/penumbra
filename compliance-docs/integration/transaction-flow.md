# Transaction Flow

## End-to-End Flow

```
1. USER INITIATES TRANSFER
   pcli tx send <recipient> <amount> <asset>
              │
              ▼
2. PLANNER BUILDS TRANSACTION
   Creates SpendPlan + OutputPlan
   Plans contain compliance fields:
   - compliance_path, compliance_anchor
   - compliance_ciphertext, compliance_leaf
   - asset_path, asset_anchor
              │
              ▼
3. COMPLIANCE DATA (local lookups)
   a) Check local tree for asset regulation status
   b) Lookup Merkle proofs from cached trees
   c) Generate ciphertexts (encrypted to DK = AK + T * B_d)
   d) For unregulated: use BLACK_HOLE_ACK
              │
              ▼
4. PROOF GENERATION
   SpendCircuit + OutputCircuit bind compliance data
   Proves: correct encryption, valid Merkle paths
              │
              ▼
5. TRANSACTION BROADCAST
   Validator verifies proofs + anchors
   Ciphertexts stored in block
              │
              ▼
6. COMPLIANCE SCANNING
   Issuer derives dk = UK + T from Orbis
   try_detect_asset() with detection key on EPK
   If match: request re-encryption from Orbis
```

## Plan Fields

SpendPlan and OutputPlan include:

```rust
compliance_path: MerklePath,
compliance_anchor: StateCommitment,
compliance_ciphertext: Vec<u8>,
compliance_leaf: Option<ComplianceLeaf>,
is_regulated: bool,
counterparty_address: Option<Address>,
counterparty_leaf: Option<ComplianceLeaf>,
asset_path: MerklePath,
asset_anchor: StateCommitment,
asset_indexed_leaf: Option<IndexedLeaf>, // For IMT proofs
```

## Source Files

| Component | Location |
|-----------|----------|
| Planner enrichment | `view/src/planner.rs` |
| Client compliance | `view/src/client_compliance.rs` |
| SpendPlan | `shielded-pool/src/spend/plan.rs` |
| OutputPlan | `shielded-pool/src/output/plan.rs` |
