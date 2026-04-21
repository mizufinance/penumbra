# Governance

Penumbra uses validator-managed on-chain governance.

- proposals are introduced with `ProposalSubmit`
- validators vote transparently with `ValidatorVote`
- proposal lifecycle is simplified to submission, voting, and finalization
- proposal deposit, withdrawal, claim, and proposal-NFT flows are not part of
  the supported surface

Supported proposal payloads are the reduced governance set implemented by the
runtime, including signaling, emergency, parameter-change, and supported IBC
client control proposals where enabled by the chain.

Governance parameter changes are constrained to the surviving module set.
