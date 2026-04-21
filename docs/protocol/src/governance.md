# Governance

Penumbra uses validator-managed on-chain governance. The reduced governance
surface keeps proposal submission and validator voting, and removes the legacy
proposal deposit, withdrawal, claim, and proposal-NFT flows.

## Supported Actions

- `ProposalSubmit` creates a proposal and places it into voting.
- `ValidatorVote` records a validator's transparent vote on that proposal.

## Proposal Lifecycle

The supported lifecycle is:

1. submission
2. voting
3. finalization

There is no supported withdraw, deposit-claim, or proposal-NFT sub-flow.

## Voting

Validator votes are public and attributable to the validator that cast them.
The reduced chain does not use delegator voting, stake-weighted delegation-note
proofs, or stake-era governance accounting.

Votes retain the standard three outcomes:

- `Yes`
- `No`
- `Abstain`

## Proposal Kinds

The exact proposal kinds are determined by the runtime's reduced governance
payload set. Supported classes include:

- signaling proposals
- emergency proposals
- parameter-change proposals over the surviving module set
- IBC client freeze / unfreeze proposals when enabled
