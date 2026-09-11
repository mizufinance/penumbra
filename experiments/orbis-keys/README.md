# Bounded-state audit key experiments

Read [the findings and recommendation](REPORT.md) and [reproduction commands](RUN.md).

Research harnesses only. These do not change payment acceptance or enable the
blocked named-person endpoint. All secrets generated here are synthetic fixtures.

Required properties: no permanent per-address private shares; Alice-authorized
scanning does not decrypt Bob; amount/sender/receiver remain separately scoped;
no reusable subject key leaves Orbis; historical keys survive share refresh.

Experiments run sequentially on native Apple Silicon with one heavy build or
proof at a time. Timing excludes network unless a transport is explicitly used.

## Cases

1. Native vetKeys IBE with the official Rust client: encrypted threshold output
   interoperability, wrong identity/field/key/ciphertext rejection, batches,
   share refresh, serialization costs. Threshold fixture generation is distinct
   from a production distributed DKG ceremony.
2. Gnark pairing/encryption cost screening over Shieldd's field. Primitive
   counts and proofs are not reported as complete Transfer proofs.
3. Published LaKey programs under a real three-process malicious-secure MPC
   backend, separating setup, preprocessing, online execution, and key generation.
   Public-key benchmark inputs must not be represented as secret key provisioning.
4. On-demand derived shares feeding ciphertext-specific decryption; retain only
   master state between sessions. Evaluate refresh and changing the public input.

Results must distinguish upstream code, locally written experimental adapters,
measured execution, extrapolation, and unresolved security composition.

## Saved checkpoints

- Shieldd research: `b3de9feceb` on `codex/voluntary-disclosure-332`.
- Shieldd integration: `4f02fead03` on `codex/disclosure-integration`.
- Orbis integration: `0d30611` on `codex/disclosure-integration`.
- Bankd integration: `01e0896` on `codex/disclosure-integration`.

Each checkpoint was pushed before experiments. Integration remains incomplete
and the identified public-derivation confidentiality issue remains a blocker.
