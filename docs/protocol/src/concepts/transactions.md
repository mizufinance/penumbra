# Transactions

Transactions describe an atomic collection of state changes. Each transaction
contains a sequence of supported actions plus the authorization, memo, and
balance-commitment data needed to execute them safely.

The reduced chain supports three main categories of actions:

- shielded note actions:
  - `Transfer`
  - `Split`
  - `Consolidate`
  - `ShieldedIcs20Withdrawal`
- validator and governance actions:
  - `ValidatorDefinition`
  - `ProposalSubmit`
  - `ValidatorVote`
- service and infrastructure actions:
  - `IbcRelay`
  - `ComplianceRegisterAsset`
  - `ComplianceRegisterUser`
  - `AggregateBundle`

Shielded actions consume existing note commitments, reveal nullifiers for spent
notes, and create new note commitments when applicable. The transaction-level
binding signature enforces that the value balance of all actions, together with
fees, nets to zero.
