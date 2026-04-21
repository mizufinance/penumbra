# Validators

Penumbra validators are managed directly through `ValidatorDefinition` actions and
participate in consensus with equal voting power once they are active.

## States

Validators move through a reduced state machine:

* **Defined**: validator metadata has been published on-chain.
* **Inactive**: validator is known to the chain, eligible for the active set, but not currently participating in consensus.
* **Active**: validator is in the consensus set and subject to uptime requirements.
* **Jailed**: validator has been removed from consensus for downtime and must be re-enabled by its operator before it can return.
* **Tombstoned**: validator has been permanently removed for byzantine misbehavior.
* **Disabled**: validator has been manually disabled by its operator.

## Lifecycle

Validators become known to the chain either at genesis or by submitting a
`ValidatorDefinition` action. The chain enforces an `active_validator_limit`
parameter to bound the active set.

State transitions are:

* `ValidatorDefinition` registration creates a validator in **Defined** state.
* An enabled `ValidatorDefinition` moves **Defined** to **Inactive**. There is no bonding, deposit, or stake threshold in the reduced validator model.
* At genesis, validators may start directly in **Active**.
* At the end of each epoch, the chain re-evaluates every validator currently in the consensus index whose state is **Active** or **Inactive**.
* Selection into the active set is deterministic: those eligible validators are sorted by identity key, then the first `active_validator_limit` are marked **Active** and the remainder are marked **Inactive**.
* Validators outside the active set do not enter a separate Pending or Standby state; they remain **Inactive** until a later epoch re-evaluation promotes them.
* **Active** validators that exceed downtime policy move to **Jailed**.
* A **Jailed** validator does not move directly back to **Active**. Its operator must submit an enabled `ValidatorDefinition`, which moves it to **Inactive**; it can then be promoted back into the active set on a later epoch transition.
* **Disabled** validators immediately leave the active rotation. Re-enabling is allowed, but only after the chain-enforced cooldown recorded from the last disable height; re-enabling moves the validator back to **Inactive**, not directly to **Active**.
* **Tombstoned** is permanent. A tombstoned validator cannot transition back to **Defined**, **Inactive**, **Active**, **Jailed**, or **Disabled**.

The relationship between **Defined** and **Inactive** is therefore explicit:
**Defined** means the validator record exists on-chain, while **Inactive** means
the validator is enabled, indexed for consensus-set consideration, and waiting
for the next active-set selection pass.

`Disabled` can be reversed only through a later enabled `ValidatorDefinition`.
`Jailed` likewise returns through **Inactive** after operator action, not by an
automatic unjail path.

Because selection happens at epoch end, slots that free up in the active set
are filled by the next epoch re-evaluation of eligible **Inactive** validators,
again using the deterministic identity-key ordering described above. `Jailed`
and `Tombstoned` validators are not eligible for active-set selection until
they first transition back to an eligible state, and `Tombstoned` never does.

`ValidatorVote` actions allow validators to participate in governance without
re-introducing staking-era delegation or unbonding semantics. The reduced
validator surface has no delegation pools, unbonding queues, or staking rewards.
