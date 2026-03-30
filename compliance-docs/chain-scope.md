# Chain Scope

This is a lightweight Penumbra deployment scoped to shielded transfers only.
Most application logic — staking, governance, DeFi, liquidity — lives on BankD.
This chain's sole purpose is to provide a shielded transfer layer with compliance
visibility for regulated assets.

## Enabled Actions

| Action | Purpose |
|--------|---------|
| `Spend` | Consume a shielded note |
| `Output` | Create a shielded note |
| `IbcRelay` | IBC light client and channel lifecycle (inbound and outbound) |
| `Ics20Withdrawal` | Transfer tokens out via IBC |
| `ValidatorDefinition` | Validator registration (permissionless, no rewards) |
| `ProposalSubmit` | Submit a governance proposal (parameter change, upgrade, IBC freeze) |
| `ProposalWithdraw` | Cancel a pending proposal |
| `ValidatorVote` | Validator votes on governance proposals |
| `ProposalDepositClaim` | Reclaim governance deposit after resolution |
| `ComplianceRegisterAsset` | Register a regulated asset with its issuer policy |
| `ComplianceRegisterUser` | Register a user address for a regulated asset |
| `AggregateBundle` | Validator-submitted proof aggregation (internal, not user-facing) |

## Disabled Actions

| Action | Reason |
|--------|--------|
| `Swap` | DEX lives on BankD |
| `SwapClaim` | DEX lives on BankD |
| `PositionOpen` | DEX lives on BankD |
| `PositionClose` | DEX lives on BankD |
| `PositionWithdraw` | DEX lives on BankD |
| `ActionDutchAuctionSchedule` | DEX lives on BankD |
| `ActionDutchAuctionEnd` | DEX lives on BankD |
| `ActionDutchAuctionWithdraw` | DEX lives on BankD |
| `Delegate` | No staking rewards on this chain |
| `Undelegate` | No staking rewards on this chain |
| `UndelegateClaim` | No staking rewards on this chain |
| `DelegatorVote` | No delegators (staking disabled) |
| `CommunityPoolSpend` | No community pool on this chain |
| `CommunityPoolOutput` | No community pool on this chain |
| `CommunityPoolDeposit` | No community pool on this chain |
| `ActionLiquidityTournamentVote` | DEX-dependent, lives on BankD |
| `ProposalPayload::CommunityPoolSpend` | No community pool on this chain |

## Relationship to BankD

BankD is the primary application chain. It handles:
- Staking and validator rewards
- DEX and liquidity positions
- Dutch auctions
- Community pool
- Full on-chain governance with delegator voting

This chain connects to BankD via IBC. Tokens flow in via `IbcRelay` / ICS-20
and are shielded here for private transfers. Tokens flow back to BankD via
`Ics20Withdrawal`. Compliance enforcement applies only while tokens are on
this chain.

## Validator Set

Validators register via `ValidatorDefinition` (permissionless). There are no
staking rewards. The chain is intended to move to a proof-of-authority model
where the validator set is permissioned — this is deferred to a future phase.
