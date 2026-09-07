# Private daily volume accumulator

Issue: [bankd#311](https://github.com/mizufinance/bankd/issues/311)

Reference: [`ethsystems/pocs` private-payment design](https://github.com/ethsystems/pocs/blob/92ce83852d4a49a0ba9cf67c7f66adc686423e32/pocs/private-payment/shielded-pool-compliance/SPEC.md)

## Design

Each ACP-approved address and regulated asset has one private accumulator for
undisclosed outbound volume. ACP permits one live regulated address per KYC
identity; this design does not assume multiple unlinkable regulated addresses.

The accumulator applies only to external receiver value in an ordinary
`Transfer`. Deposits, withdrawals, note reshapes, self-transfers, and fee
funding do not participate. The asset policy commits a `daily_volume_limit`.

For a tracked transfer, the circuit proves:

```text
candidate = prior_undisclosed_volume + receiver_amount
candidate <= daily_volume_limit
successor_undisclosed_volume = candidate
```

Addition is checked in `u128`; overflow rejects the proof. Equality remains
undisclosed. If the candidate is greater than the limit, the wallet uses the
fixed padding branch and the existing issuer ciphertext discloses that
transaction. The accumulator head does not advance. A later transaction may
remain private when its candidate undisclosed volume fits within the allowance.

A caller may request voluntary disclosure. It uses the same padding branch,
does not read or advance the head, and can be submitted concurrently when it
uses independent value notes. This intentionally means disclosed volume is not
part of the later undisclosed-volume calculation.

The issuer ciphertext contains only the current transfer. It does not contain
the cumulative volume. No FHE is used: the prover knows the opening and proves
the private arithmetic directly.

## UTC day

The accounting period is a fixed UTC day, independent of Shieldd's SCT epoch:

```text
utc_day_start = timestamp - timestamp % 86400
day_start = utc_day_start
```

Consensus accepts a target timestamp only within ±30 minutes of signed block
time. This lets a proof built shortly before midnight remain valid after
midnight while keeping it charged to the day containing its target timestamp.

Each selected day has an independent deterministic origin. A lost head blocks
tracked transfers only for that selected day; voluntary disclosure remains
available, and the next day can start from its fresh origin.

## Private state transition

The committed state is:

```text
VolumeAccumulatorState {
  subject = H(approved_address, asset_id),
  day_start,
  undisclosed_volume,
  blinding,
}
```

The first transition derives an origin nullifier from the wallet nullifier key,
subject, and day. A continuation proves SCT membership of the predecessor and
uses its positioned nullifier. Reusing an origin or predecessor therefore
forks onto the same scoped nullifier and consensus rejects the second branch.

Every ordinary Transfer has one fixed accumulator slot:

- eligible tracked transfer: real successor commitment and transition
  nullifier;
- disclosed, self, or unregulated transfer: independently derived padding
  commitment and nullifier;
- fee funding: proof-bound canonical disabled slot and no SCT insertion.

The fixed shape hides whether an ordinary transfer advanced a real
accumulator. The public day is UTC-aligned but does not identify the private
subject. Fresh state blindings and positioned nullifiers prevent public
address/asset linkage.

## Consensus storage and growth

Accumulator commitments reuse the SCT. Accumulator nullifiers do not enter the
global historical nullifier tree. Consensus keeps them in a separate
day-prefixed KV set and deletes a selected day's entries only after
`day_start + 24h + 30m`; the boundary itself remains accepted.

Consequences:

- global nullifier-tree growth: **0%**;
- temporary KV entries: one per ordinary Transfer, retained for about one day;
- common main-plus-fee SCT commitments: four to five, **+25%**;
- Transfer circuit constraints: 129,821 to 142,571, about **+9.8%**;
- Groth16 proof byte length remains fixed, although proving time and key size
  can grow with the circuit.

This is deliberately simpler than adding another nullifier tree, generation
rollover, historical witness service, and compact-block root.

## Wallet state and recovery

The wallet stores one confirmed head per subject and selected day, plus at most
one pending reservation. A tracked proof reserves the head atomically. A
second tracked proof fails while it is reserved; the caller may wait or request
voluntary disclosure.

The reservation is released after a definite pre-broadcast failure or node
rejection. An ambiguous broadcast remains reserved until confirmation or
strict expiry. Confirmation atomically records the successor and clears the
reservation.

Each Transfer carries a fixed 108-byte owner-encrypted accumulator payload.
The key is derived from the existing outgoing viewing key and successor
commitment; the 92-byte plaintext carries the real/padding marker, subject,
day, volume, and blinding. A scanner verifies the commitment and
transition chain before making a recovered head plannable. Missing or broken
history is marked incomplete rather than silently opening a second budget.

## Comparison with ethsystems

Ethsystems uses an epoch velocity state with an explicit sequence and derives
nullifiers from `(spending_key, epoch, sequence)`. Shieldd reuses its existing
positioned SCT nullifier construction for continuations and needs no public or
private transaction count. Both designs serialize exact accumulator updates;
Shieldd uses voluntary disclosure as the minimal burst path instead of lanes or
parallel budgets.

User-side recursive or SnarkPack aggregation does not solve this shared-head
dependency by itself.

## Risks and controls

| Risk | Control |
| --- | --- |
| Two proofs fork one head | Wallet reservation plus scoped origin/predecessor nullifier uniqueness |
| Lost or incomplete head opens a second daily budget | Recovery-chain status; only complete heads are plannable; new origin only on a new selected day |
| Midnight proof remains valid briefly | Exact target day plus matching nullifier-retention grace |
| Fee funding bypass is reused by a body Transfer | Proof context is statement-bound and checked against transaction location |
| Amount or cumulative volume wraps | Circuit and native checked `u128` addition |
| Padding leaks disclosure through action shape | Real and padding use the same ordinary-Transfer wire slot and ciphertext length |
| Owner payload creates a new cryptographic subsystem | Existing OVK KDF plus authenticated encryption; no DH, FHE, or SNARK encryption |
| Temporary state grows without bound | Day markers and deterministic pruning after the acceptance buffer |
| Policy/key changes invalidate semantics | Deferred to [shieldd#38](https://github.com/mizufinance/shieldd/issues/38) |

Activation requires `mizufinance/shieldd-formal` to pin the activating Shieldd
commit and update its accumulator and withdrawal models.
