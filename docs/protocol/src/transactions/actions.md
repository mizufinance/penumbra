# Action Reference

This page lists the Shieldd actions used by the target Bankd integration.

| Action | Description |
| ------ | ----------- |
| `shielded_pool.v1.Transfer` | Moves shielded value between addresses and may produce sender-owned change. |
| `shielded_pool.v1.NoteReshape` | Reshapes sender-owned notes using the 1→8 and 8→1 padded proof families. |
| `shielded_pool.v1.ShieldedHostWithdrawal` | Burns shielded value into a proof-bound host transfer or host execution effect and may produce sender-owned change. |

Bankd drives deposits, asset and user registration, freeze/unfreeze, and
withdrawal settlement through typed host operations. IBC actions are outside
the Bankd host action surface.
