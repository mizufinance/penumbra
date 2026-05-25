Penumbra is a privacy-focused network for shielded value transfer, validator
management, governance, compliance, and IBC interoperability.

The current chain surface is intentionally reduced:

- shielded actions:
  - `Transfer`
  - `Split`
  - `Consolidate`
  - `ShieldedIcs20Withdrawal`
- infrastructure and control actions:
  - `IbcRelay`
  - `ValidatorDefinition`
  - `ProposalSubmit`
  - `ValidatorVote`
  - `ComplianceRegisterAsset`
  - `ComplianceRegisterUser`
  - `AggregateBundle`

Removed product families such as DEX, auctions, community-pool actions,
staking/delegation economics, and legacy governance deposit flows are not part
of the active protocol surface.

This website renders the work-in-progress protocol specification for Penumbra.

Press `s` or use the magnifying glass icon for full-text search.

If you're interested in technical discussion about the project, why not

- join [the discord](https://discord.gg/hKvkrqa3zC),
- check out [the repo and issue tracker](https://github.com/mizufinance/penumbra),
- view the [roadmap goals](https://github.com/orgs/mizufinance/projects),
- or [follow the project on Twitter](https://twitter.com/penumbrazone) for updates.

## Private Transactions

Penumbra records all value in a single multi-asset shielded pool based on the
Zcash Sapling design, while using a reduced action set tailored to shielded
transfers, note management, and outbound ICS-20 withdrawals. Inbound IBC
transfers shield value as it moves into the zone, while outbound transfers use
the dedicated `ShieldedIcs20Withdrawal` path.

Unlike Zcash, Penumbra has no notion of transparent transactions or a
transparent value pool; instead, inbound IBC transfers are analogous to `t2z`
Zcash transactions, outbound IBC transfers are analogous to `z2t` Zcash
transactions, and the entire Cosmos ecosystem functions analogously to
Zcash's transparent pool.

Unlike account-based chains, Penumbra centers the note model rather than
transparent user balances. Validators retain long-lived identity because they
own validator definitions and participate in consensus and governance.

## Governance

Penumbra supports on-chain governance through proposal submission and validator
voting. The current governance surface is validator-managed and does not include
legacy proposal deposits, withdrawal/claim flows, or delegation-based voting.
