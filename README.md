# Shieldd

Shieldd is Bankd’s embedded private execution subsystem, forked from Penumbra.
It owns shielded notes, nullifiers, proof verification, compliance commitments,
and compact wallet/auditor data. Bankd owns consensus, issuer authorization,
asset accounting, escrow settlement, public queries, and transaction submission.
Both are active prototypes.

Bankd calls `HostExecution` through the `shieldd` static library’s C ABI. Shieldd
also provides wallet planning and synchronization libraries, an issuer scanner,
native proof builders, and offline `pcli` custody/key tools. IBC relay and ICS20
withdrawal execution remain implemented in Shieldd.

Regulated participation uses asset policy and per-address/per-asset lifecycle
commitments. Fees use the base asset. The `SeizeNote` host call verifies recovery
capsule proofs; capsule location, ACP/Orbis release orchestration, and Bankd seizure
settlement remain incomplete. See [enforcement and seizure](docs/compliance/enforcement-and-seizure.md).

Start with [the code and documentation map](docs/README.md), then use
[development](docs/development.md) for commands or
[embedded artifacts](docs/embedded-artifacts.md) for Bankd integration.
Engineering rules live in [AGENTS.md](AGENTS.md).
