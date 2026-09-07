# Enforcement and seizure

Shieldd is an embedded Bankd subsystem. Bankd owns consensus, canonical host
sources, public custody, and settlement. Shieldd owns private note state,
compliance state, proof verification, nullifiers, and replay-safe host receipts.
They do not jointly produce proofs.

## Capsule release

For capsule EPK `E`, registered capability `C`, and released point `S`, Shieldd
verifies an equality-of-discrete-logs proof for

```text
C = xG
S = xE
```

The proof transcript binds a canonical release ID derived from the exact
capsule, note, registered address and asset, current capability, Orbis policy,
authority instruction, and expiry. It proves the DH relation for `S` under
`C`, not that an ACP granted access. The note proof separately checks the
capsule plaintext and note commitment.

`CapsuleReleaseRequest::release_id` defines the canonical resource ID;
`CapsuleReleaseEvidence` carries the point and proof for that ID.

The intended production sequence is:

1. an authority signs the exact note-seizure instruction;
2. an ACP grants access to that capsule-specific release ID;
3. Orbis returns `S` and the DLEQ proof only after the grant;
4. Shieldd verifies the instruction, release, note proof, and nullifier; and
5. Bankd atomically applies the withdrawal returned by Shieldd.

The reusable `reader_secret` never enters Bankd consensus. Publishing `S` opens
the seized capsule to observers; reusing its capability and EPK would share
that opening with another capsule. Hiding the opening requires a larger ZK
release relation or a threshold attestation over a hidden opening.

## State transition

Each registered `(address, asset_id)` leaf commits the address, asset,
capability, regulated-nullifier derivation data, status, freeze generation, and
freeze height. Legal transitions are:

```text
Active -> Frozen -> Active
Active -> Frozen -> Seized
Seized -> Seized       while consuming more notes from the same freeze
```

Every regulated spend and receive requires an `Active` leaf under the current
root. A new freeze generation invalidates older seizure instructions.

`SeizeNote` verifies the authority signature and expiry, the current frozen
leaf, the capsule-specific DLEQ release, the Groth16 note-membership and opening
proof, the canonical regulated nullifier, and nullifier nonmembership. One
state delta inserts the nullifier, updates the lifecycle, records the audit
effect and receipt, and returns the exact typed Bankd withdrawal. Exact source
replay returns the receipt without another mutation.

## Implementation status

| Capability | Status |
| --- | --- |
| Recovery capsules and note-seizure circuit | Implemented in Shieldd |
| Capsule release request, DLEQ verifier, and host state transition | Implemented in Shieldd |
| Production ACP policy enforcement | Not implemented |
| Orbis capsule-release and address-DH APIs | Not implemented |
| Private capsule locator and operator workflow | Not implemented |
| Bankd seizure admission and atomic settlement | Not implemented |

The Shieldd host method is protocol preparation, not a complete seizure
product. Tests construct release evidence locally; that does not stand in for
ACP or Orbis integration.
