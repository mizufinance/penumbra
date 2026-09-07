# Proof-bound note routing

Shieldd addresses are exactly 48 bytes: a 16-byte diversifier followed by the
32-byte transmission key. Routing derives directly from that transmission key
and does not use fuzzy message detection.

For every address, the stable routing word is

```text
Trunc32(Poseidon("shieldd.discovery.route", transmission_key_s))
```

The hash excludes the asset, regulation class, precision, height, time, epoch,
and address-purpose metadata. Regulated and unregulated selectors are therefore
nested low-bit prefixes of the same word. Application parameters contain the
regulated precision, unregulated precision, and activation height, with
`regulated_precision <= unregulated_precision`. Transactions reveal only the
parameter-set identifier, never the privately selected class precision.

Every public `RoutingTag` is one `u32`. Its low `p` bits equal the address word
and its other bits come from the proof-bound routing nonce and tag slot. A
transfer always publishes two permuted slots. Regulated transfers route the
authenticated sender and receiver; unregulated transfers route the receiver
and either the change owner or a nonce-derived filler. Withdrawals and note
reshapes publish one owner slot. Dummy and filler slots remain in compact blocks.

Compact blocks expose only typed action locations:

```text
RoutingRecord {
  tag, height, transaction_id, action_index, tag_slot
}
```

They do not label sender or receiver roles. A match returns the encrypted note
payloads for that action, which the wallet trial-decrypts. A full compact-block
scan is always a valid recovery path.

## Wallet state and recovery

The view service persists every issued address before returning it, including
the actual randomized address index, its local purpose, birth height, and
optional retirement height. `General` and `Regulated { asset_id }` purposes are
wallet metadata and never affect an address or routing word. Backups should
retain these records; scanning full compact blocks recovers notes when the
metadata is unavailable.

## Remote privacy modes

- **Local full node:** selector matching is local and discloses no search to an
  external provider.
- **Remote full compact blocks:** the client downloads the whole range. This has
  the highest bandwidth cost but reveals no selector or matched position.
- **Remote filtered query:** the provider learns the selectors, height ranges,
  timing, grouping, network identity, and the action positions returned.

Downloading tags and subsequently requesting only matching actions is still a
filtered query: the provider can map the requested positions back to tags. The
base API does not claim PIR, padding, decoys, or anonymity. Private remote
recovery requires downloading the full range or adding one of those mechanisms.

## Issuer audit routing

Detection-key screening authenticates the asset, regulated flag, sender and
receiver routing classes, and the encrypted routing permutation. Slots do not
identify an address and no registered subject selector prefilters ciphertexts.
An authorized target-address history proof submits every required unflagged
core tier in the authenticated range to Orbis for private match/non-match
classification. Routing does not change consensus validity or grant an
external system spending authority.

## Performance baseline

The `routing` Criterion benchmark scans two tags for each of 5,000 transfers and
trial-decrypts matching action payloads. On the August 11, 2026 development
machine, a 10-sample optimized run completed in 2.1051–2.1110 ms, or about
2.37 million transfers per second. This is a local CPU baseline, not a network
or end-to-end full-node throughput claim.
