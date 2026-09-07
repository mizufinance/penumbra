# Docs index — what is authoritative where

Runtime code and protocol records are authoritative here. Formal specifications,
generated evidence, and their gates live in
[`mizufinance/shieldd-formal`](https://github.com/mizufinance/shieldd-formal)
and certify one exact Shieldd commit.

| Area | Authoritative for | Checked by |
| --- | --- | --- |
| `docs/compliance/` | Compliance flow, enforcement and seizure, wire reference, constraints, and tests. | referenced tests |
| `docs/snarkpack/` | SnarkPack runtime design and verification. | runtime tests and `scripts/check-snarkpack-runtime-invariants.sh` |
| `docs/protocol/` | Protocol background for crypto constructions, keys, and addresses. Do not cite it as implementation evidence. | none |
| `docs/transfer-circuit/` | Transfer-circuit constraint checklist (referenced by `docs/compliance/`). | referenced tests |
| `docs/nullifier-history.md` | Nullifier-generation state, proof, and storage boundaries. | focused runtime and interoperability tests |
| `docs/research/` | Focused engineering notes for active runtime designs. | referenced parity and lifecycle tests |
| `docs/rustdoc/` | CI staging for rustdoc deploy (`notes.yml`), not documentation content. | CI |

The user guide lives in a separate repository:
<https://github.com/mizufinance/guide>.
