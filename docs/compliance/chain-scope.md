# Chain Scope

This is a lightweight Penumbra deployment with a deliberately smaller action surface.
Most application logic such as staking and DeFi lives on BankD.
This chain's sole purpose is to provide a shielded transfer layer with compliance
visibility for regulated assets.

## Supported Actions

| Action | Purpose |
|--------|---------|
| `Transfer` | Spend up to two notes and create up to two shielded notes |
| `Consolidate` | Merge many notes of one asset into a smaller number of notes |
| `Split` | Split one note into several notes of the same asset |
| `IbcRelay` | IBC light client and channel lifecycle (inbound and outbound) |
| `ShieldedIcs20Withdrawal` | Transfer tokens out via IBC |
| `ValidatorDefinition` | Validator registration (permissionless, no rewards) |
| `ProposalSubmit` | Submit a governance proposal (parameter change, upgrade, IBC freeze) |
| `ValidatorVote` | Validator votes on governance proposals |
| `ComplianceRegisterAsset` | Register a regulated asset with its issuer policy |
| `ComplianceRegisterUser` | Register a user address for a regulated asset |
| `AggregateBundle` | Validator-submitted proof aggregation (internal, not user-facing) |

## Removed Actions

The chain does not expose DEX, staking delegation, community-pool transaction
actions, or the legacy governance withdrawal / deposit-claim flow. Those
surfaces remain on other chains or were deleted as part of the POA governance
and shielded-circuit simplification.

## Relationship to BankD

BankD is the primary application chain. It handles:
- Staking and validator rewards
- DEX and liquidity positions
- Dutch auctions
- Community pool
- Governance features not present on this chain

This chain connects to BankD via IBC. Tokens flow in via `IbcRelay` / ICS-20
and are shielded here for private transfers. Tokens flow back to BankD via
`ShieldedIcs20Withdrawal`. Compliance enforcement applies only while tokens are on
this chain.

## Validator Set

Validators register via `ValidatorDefinition` (permissionless). There are no
staking rewards. The chain is intended to move to a proof-of-authority model
where the validator set is permissioned — this is deferred to a future phase.
