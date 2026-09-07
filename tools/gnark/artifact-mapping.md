# gnark artifact mapping

Each deployed circuit family has one directory under `tools/gnark/artifacts`:

| Family | Directory |
| --- | --- |
| Transfer | `transfer/` |
| Note reshape 1→8 | `note_reshape1x8/` |
| Note reshape 8→1 | `note_reshape8x1/` |
| Shielded ICS-20 withdrawal | `shielded_ics20_withdrawal/` |

Every directory contains the circuit metadata, semantic manifest, setup
provenance, proving key, SR1CS, and binary/JSON verifying keys. Metadata binds
the exact circuit label, dimensions, hashes, and key sizes.

Rust family registration is generated from the supported family manifests.
The prover daemon and shared library use the same registry and witness ABI.
Runtime configuration may select a transport and artifact directory but cannot
replace consensus verifying keys.

`historical_generation_indexed/` is an unsafe development fixture for the
historical nullifier prototype. It is not part of the deployed family registry.
