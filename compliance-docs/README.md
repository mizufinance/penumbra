# Penumbra Compliance System

Privacy-preserving compliance for regulated assets.

## Key Hierarchy

```
MCK (Orbis ring) → UCK = Hash(MK, user_id) → ACK = UCK * B_d → DCK = ACK + T * B_d
```

| Key | Type | Holder | Purpose |
|-----|------|--------|---------|
| MCK | Scalar | Orbis ring | Master key |
| UCK | Scalar | Orbis | User Compliance Key (per user) |
| ACK | Point | Registry (public) | Address key (per address) |
| DCK | Point | Public | Daily key (client encryption) |
| dk | Scalar | Orbis | Daily scalar (decryption) |

## Documentation

| Doc | Content |
|-----|---------|
| [Key Hierarchy](architecture/key-hierarchy.md) | MCK → UCK → ACK → DCK derivation |
| [Registry Design](architecture/registry-design.md) | User tree (QuadTree) + Asset tree (IMT) |
| [Ciphertext Design](architecture/ciphertext-design.md) | 3-tier encryption structure |
| [Roadmap](roadmap/README.md) | Pending work |

## Integration

| Doc | Content |
|-----|---------|
| [Components](integration/components.md) | Modified Spend/Output, blocked actions |
| [Transaction Flow](integration/transaction-flow.md) | End-to-end tx with compliance |
| [Compliance Flow](integration/compliance-flow.md) | Registration → scanning |
| [Orbis Flow](integration/orbis-flow.md) | Key management integration |

## Key Files

| Component | Location |
|-----------|----------|
| Compliance component | `crates/core/component/compliance/src/` |
| Client compliance | `crates/view/src/client_compliance.rs` |
| Planner enrichment | `crates/view/src/planner.rs` |
| Spend/Output plans | `crates/core/component/shielded-pool/src/{spend,output}/plan.rs` |
| POC | `crates/bench/tests/hierarchical_keys_poc.rs` |

## CLI Quick Reference

```bash
# Asset registration
pcli tx compliance register-asset <asset> --regulated
pcli tx compliance register-asset <asset> --unregulated

# User registration
pcli tx compliance register-user <asset>

# Key derivation (UCK = User Compliance Key, held by Orbis)
pcli tx compliance derive-daily-key --uck-hex <hex> --date <day_index>

# Scanning
pcli tx compliance scan --daily-key-hex <hex> --node <url>
```

## Testing

```bash
# Unit tests
cargo test -p penumbra-sdk-compliance --lib

# Integration tests
cargo test -p penumbra-sdk-app-tests --test compliance_full_flow

# POC tests
cargo test -p penumbra-sdk-bench --test hierarchical_keys_poc

# Planner tests
cargo test -p penumbra-sdk-view --lib planner::tests
```
