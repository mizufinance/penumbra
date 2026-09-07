# Shieldd inside Bankd

Shieldd is Bankd's private execution subsystem. Bankd and Shieldd share one
validator set and one atomic block lifecycle; Shieldd is not intended to run as
an independent product chain.

Bankd owns consensus, application accounts, issuer and compliance authority,
deposits, withdrawals, and transparent minting. Shieldd owns shielded notes,
nullifiers, proof verification, compliance commitments, and encrypted audit
records. Bankd calls those capabilities through a typed host interface whose
source is the canonical Bankd transaction location.

The shielded action surface includes `Transfer`, `NoteReshape`, and host
withdrawal. Regulated actions bind the current asset-policy and user-status
roots. A status change therefore invalidates proofs created against the prior
user root.

Compliance transitions are per `(address, asset_id)`. Bankd can freeze and
unfreeze a pair. Its privileged `SeizeNote` call consumes one real frozen note,
emits the same canonical nullifier an owner spend would use, and advances the
pair to terminal `Seized`. The capsule discovery, ACP/Orbis release, and Bankd
settlement workflow is not implemented here yet.

This book describes cryptographic and state-machine internals. The Bankd host
boundary defines the target deployment architecture.
