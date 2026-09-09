# Wallet state and planning

[View](../crates/view/src) consumes host-supplied `WalletBlock` records through
`SyncWorker`. Each record contains compact data, transactions, a timestamp and
an expected SCT root. The worker verifies roots before atomically committing
notes, witnesses, compliance projection, historical state and the new sync height.
A failed projection cannot publish partial wallet state.

`StoragePlanningIo` fixes its read height when created and rejects reads after
the wallet advances. `PlanningIo` supplies the external reads needed to complete
an intent. Issued addresses are durable records: planning and discovery reuse
canonical address indices, including diversifiers beyond the default address.

The wallet retains witnesses for owned notes. Compliance projection uses separate
user and indexed asset trees; their pair is validated against advertised anchors.
See [tree persistence](state.md) for mutation and atomicity rules.

`HistoricalProofWorker` advances replayable caches using a `HistoricalWitnessSource`
and optional prover. It persists blocked, invalid and useful failure states;
work becomes ready only after archive/proof prerequisites validate. View’s optional
`rpc` feature supplies `RpcHistoricalWitnessSource`; hosts may supply another real
external source. [Nullifier history](nullifier-history.md) defines coverage,
archive and pruning requirements.

A daily-volume transfer reserves one confirmed accumulator head per subject/day.
Definite pre-broadcast failure or rejection releases the reservation; ambiguous
broadcast keeps it reserved until confirmation or strict expiry. Confirmation
records the successor and clears the reservation atomically. Recovery verifies
the owner payload’s commitment and transition chain before a head becomes
plannable. Incomplete history cannot open a second budget. See
[the volume relation](compliance/flow.md#daily-volume-state).

`pcli` supports offline initialization, addresses and custody. Bankd owns live
query/submission workflows and mobile/admin smoke tests. See
[development](development.md) for test prerequisites.
