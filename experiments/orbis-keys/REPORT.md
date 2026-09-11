# Orbis key derivation: LaKey, vetKeys, and alternatives

## Recommendation

Use **on-demand, secret-shared key derivation with LaKey**, retaining Shieldd's Decaf377 encryption and Orbis's ciphertext-specific PRE. Start integration from the regular REG construction, with REG32 as the first parameterization to assess for production. Do not ship the research fork unchanged or treat its parameter choices as independently approved.

This choice matches the priorities: permanent secret storage does not grow with users, Alice authorization does not become Bob authorization, and the additional work largely belongs in Orbis instead of every wallet proof. The experiments establish useful interoperability and cost evidence. They do not constitute a security audit of a production LaKey–Orbis composition.

The principal price is **registration of authenticated compliance public keys**. A wallet cannot derive these public keys from the master public key alone. An address needs its compliance public keys registered before it can receive a payment using this design. Registration can be part of the existing onboarding process; it need not be a new standalone service.

Keep vetKeys as the alternative if avoiding that registration requirement becomes more important than wallet proving cost. Keep a conventional PRF evaluated in MPC as the alternative if review of LaKey's assumptions or parameters is unsatisfactory. Do not replace threshold protection with a TEE merely to avoid implementing derivation.

## Required behavior

An Alice-authorized auditor must be able to submit arbitrary accepted transaction ciphertexts for scanning. Only the permitted Alice information should become readable. The design cannot assume that Bankd already knows which transactions belong to Alice.

General audits must work without a known address, with amount, sender, and receiver independently authorized. The existing separate general-audit encryptions remain necessary: changing the named-person derivation does not automatically let a master key discover an unknown derivation path.

Neither the master secret nor a reusable Alice private key should leave the committee. Orbis returns ciphertext-specific PRE results encrypted for the authorized reader. The intermediary records those results in private DefraDB before supported interfaces make them available. This remains a trusted-intermediary design; changing key derivation does not prevent a malicious intermediary operator from copying results.

## Measured results

All measurements below are local Apple M4 Pro experiments with 48 GiB RAM, run sequentially. They are individual observations, not statistical performance distributions. The watchdog sampled process-tree memory once per second; short jobs can finish between samples. No measured job increased swap usage. Rust was 1.89.0; the gnark module used Go 1.25.7.

### Wallet proving

Both experiments used Shieldd's pinned gnark versions, real development Groth16 setup/proving/verification, BLS12-377, and `GOMAXPROCS=2`.

| Component | Constraints | Proving time | Serialized proof |
|---|---:|---:|---:|
| One BLS12-381 fixed-root pairing plus G1 subgroup check | 741,963 | 9.311 s | 292 bytes |
| Two hidden 32-level Poseidon377 public-key membership paths | 18,334 | 0.365 s | 244 bytes |

The pairing experiment uses a fixed public root with precomputed lines. It excludes identity hashing, exponent derivation, the remaining IBE encryption relations, and the existing Transfer logic. The membership experiment excludes public-key provisioning and existing encryption logic. These are component costs, **not complete new Transfer proof timings**.

For context, the integration checkpoint reported 176,187 constraints and approximately 3.4–3.5 seconds for its full Transfer proof. That historical measurement was not rerun here. The two new component measurements were made in the same experimental environment and are directly useful for comparing integration costs.

Setup took 174.78 seconds for the pairing component and 4.00 seconds for the membership component. Setup is not a per-payment cost. The pairing job's sampled peak process-tree RSS was approximately 2.05 GiB. Altered public outputs failed verification. Membership tests rejected changed leaves, siblings, roots, and a non-boolean direction selector.

Stock vetKeys therefore has a substantial proving disadvantage in Shieldd's existing field. This is not an impossibility result for every pairing-based design. Registering precomputed pairing values or changing curves would be additional protocol designs; they cannot automatically inherit the stock implementation's security review or the measured costs above.

### LaKey and Orbis

The LaKey experiment used the published MP-SPDZ research fork, a real malicious-secure Shamir backend, separate communicating processes, TLS, and live preprocessing. No fake offline material was used. The original benchmark was explicitly run with fresh secret-key generation enabled.

| Experiment | Committee | MPC statistical setting | Computation time | Aggregate communication |
|---|---|---:|---:|---:|
| Original REG12 program, one derivation including key generation | 2-of-3 | 40 | 0.0549 s | 24.33 MB |
| Four domain-separated REG12 derivations from retained master shares | 2-of-3 | 40 | 0.0603 s | 25.31 MB |
| REG12 master-share refresh plus four derivations | 2-of-3 | 40 | 0.0675 s | 26.69 MB |
| REG32 master-share initialization | 3-of-5 | 128 | 0.0759 s | 56.00 MB |
| Four domain-separated REG32 derivations from retained master shares | 3-of-5 | 128 | 0.1584 s | 113.50 MB |
| REG32 master-share refresh plus four derivations | 3-of-5 | 128 | 0.1806 s | 117.67 MB |

The custom programs operate over the **actual Decaf377 scalar modulus**, not an unrelated convenient prime. The 128 setting applies both to arithmetic statistical masking and to the output-composition slack in that experiment. It does not independently establish 128-bit computational security of the lattice parameterization.

The reported computation timers include live preprocessing but exclude executable startup, TLS setup, and bytecode loading. The watchdog observed approximately 3.06 seconds for the complete REG12 four-key invocation and 4.10 seconds for the REG32 five-process invocation. Its elapsed time includes up to one second of polling delay. A deployed precompiled integration will have different overhead; these numbers must not be advertised as Orbis API latency.

REG12's compiled derivation used eight online VM rounds; REG32 used ten. Actual execution with live preprocessing reported approximately 35 and 48 communication rounds respectively. Wide-area deployment must account for preprocessing, round-trip latency, and bandwidth, not just the localhost timer.

The derived shares were passed into **the existing Orbis Decaf377 PRE implementation pinned at `0d30611`**, with public multiplication-based derivation disabled. Across 32 ephemeral points and four independent scopes, the test verified PRE proofs, reconstructed the authorized shared points, and rejected wrong scope commitments, altered ephemeral points, insufficient shares, and the old public-scalar conversion attempt. That test took approximately 0.24–0.25 seconds for 2-of-3 and 0.35 seconds for 3-of-5, including negative checks. It is not directly comparable with a production PRE latency benchmark.

The four scopes were Alice amount, Bob amount, Alice sender, and general amount. The same ephemeral point was deliberately used across them. Isolation came from independent derived keys, not from assuming different ephemeral points.

The interoperability harness can read every node's synthetic fixture. It deliberately reconstructs the synthetic master in a separate clear reference calculation to catch encoding, modulus, rounding, and matrix-expansion errors. **This is not a distributed Orbis deployment or evidence of production process isolation.** The MPC programs themselves do not reveal their master or derived scalars in public outputs. Production must keep each derived share with its node.

### Storage and refresh

The regular construction retains 512 field-element shares per node: **16 KiB of master scalar data**, or 16,439 bytes with the experiment's file header. It is a fixed secret vector, not a single 32-byte share. Its size is independent of the number of registered users.

Four bounded handoff slots were used for the synthetic PRE test and removed afterward. They are not a proposed persistent per-user store. Public registration records, audit evidence, TLS credentials, bounded preprocessing material, and operational metadata are separate from this master-state measurement.

REG12 keys reproduced exactly after process restart. Same-committee refreshes were tested for both REG12/2-of-3 and REG32/3-of-5: independently generated zero-sharings were added to all master entries, the derived public keys remained unchanged, and PRE continued working. These are functional refresh tests. They do not test changing committee membership, malicious refresh participants, secure erasure, or recovery from stale backups.

## How LaKey fits the intended flow

### Registration

1. Authenticate the address registration and its ring/epoch.
2. Orbis derives secret shares for explicitly separated named-person scopes, such as address plus amount/sender/receiver permission.
3. Nodes publish and validate public commitments to those shares. Only the aggregate public keys and their authenticated address association are registered.
4. Discard transient derived secret shares after the operation. Retain the master shares.

The registered association must be authenticated by the committee and accepted by Shieldd. A Merkle membership proof against a caller-invented root proves nothing useful. The Transfer circuit must bind the registered address and encryption public key to the actual transaction participant, and the verifier must require an accepted registration root. A certificate-based implementation is also possible, but its in-circuit verification cost was not measured here.

Use an existing suitable registration structure if available. Do not add a second independently editable address directory solely for this feature. Public-key records grow with users; confidential node state does not.

### Payment

The wallet obtains the registered public keys and their authentication witness, encrypts with the existing Decaf377 machinery, and proves the association to the relevant transaction address. The secret derivation does not run in the wallet or inside the payment circuit.

Maintain separate general-audit encryptions for amount, sender, and receiver. Domain separation must keep general and named-person keys unrelated. If ephemeral points are reused across fields, those fields must not share the same effective encryption key. The experiment uses independent derived keys per scope and tests that stronger condition directly.

### Audit

Orbis nodes validate ACP authorization, the exact accepted ciphertext, the requested scope, and the reader key. They derive Alice's relevant shares on demand, perform existing ciphertext-specific PRE without applying the old public derivation factor, and discard those transient shares.

One Alice derivation can serve a bounded audit batch containing many ciphertexts. Every ciphertext still needs its own validation, authorization association, PRE result, and recorded evidence. Derivation amortization is not permission to export Alice's scalar or to treat an entire batch as one undifferentiated release.

General audits use the independently provisioned general keys. Their ciphertexts reveal the requested value without requiring the address to be known beforehand. The PRE result goes through the intermediary into DefraDB; the reader retrieves it under the established permissions and decrypts locally.

## Why vetKeys remains a credible alternative

vetKD derives group-valued keys using an identity-dependent hash-to-curve, rather than a publicly known scalar multiple of one master secret. The official client verifies encrypted threshold outputs and supports IBE. The management API is live on ICP mainnet; NCC Group has published a scoped cryptography review. This is stronger directly relevant deployment evidence than was found for LaKey.[^1][^2]

The native experiment used the official `ic-vetkeys 0.9.0` client and a local threshold fixture. It tested 2-of-3, 3-of-5, and 5-of-7 at batches of 1, 8, and 32; wrong subjects, fields, transaction references, chains, modes, reader keys, corrupted ciphertexts, corrupted shares, duplicate indices, insufficient shares, and share refresh were exercised. The fixture used a local Shamir dealer, not ICP's distributed DKG or an ICP mainnet call.

For a 32-byte message, the tested IBE ciphertext is 168 bytes; its encrypted derived key is 192 bytes. Native encryption was approximately 1.5–1.6 ms per item after the first call. At 2-of-3, a batch of 32 used approximately 68 ms to create encrypted shares, 236 ms to verify them, 57 ms to combine them, and 86 ms for official-client verification/decryption. These deliberately straightforward native routines are not optimized replicas of the ICP node service.

To preserve recorded ciphertext-specific releases, the IBE identity must include the encryption's unique context and field as well as the subject and domain. Returning a reusable identity key for just “Alice” would let its holder decrypt future matching ciphertexts without another Orbis request. Use an encryption-time identifier, not the final transaction hash when that would make encryption circular.

The obstacle is the wallet's correctness proof. Stock BLS12-381 arithmetic is expensive in Shieldd's BLS12-377 scalar field. Native IBE timings do not predict this cost. The pairing experiment provides concrete evidence for preferring LaKey under the stated proving-time priority.

## Other alternatives

| Option | Assessment for this system |
|---|---|
| Classic PRSS | Extremely cheap derivation and user-independent storage for small committees. Long-lived refresh and committee replacement remain the principal obstacles. |
| AES/HMAC or another conventional PRF inside MPC | A credible fallback with familiar primitive assumptions; still needs secret-shared outputs, field conversion, malicious-secure execution, and public-key registration. Not benchmarked here. |
| Hydra/Ciminion and other algebraic PRFs | Do not select solely from an old speed table. Published cryptanalysis revises the security margin of some Hydra parameters.[^3] |
| Public additive or multiplicative derivation | Reject for isolated person-scoped PRE. Public transformations preserve the unwanted relation between users' results. |
| Public-output threshold VRF/OPRF used as a scalar key | Does not satisfy the requirement merely by being threshold. Revealing a reusable output can reveal the derived private key; retaining secret-shared output is essential. |
| Independently stored per-user keys | Cryptographically straightforward, but violates the stated permanent-secret-storage requirement. |
| TEE | Operational alternative with hardware/vendor trust. If used, send ciphertext-specific PRE results to it, not reusable master shares. It was not deployed or benchmarked here. |

The classic PRSS experiment used HMAC-SHA-512 with subset seeds and checked threshold reconstruction, identity separation, and the failure of naive seed rotation to preserve historical keys. It is an independently written synthetic reference, not a production PRSS service.

| Committee | Seeds per node | Seed bytes per node | Derive 32 identities for all nodes, serial |
|---|---:|---:|---:|
| 2-of-3 | 2 | 64 | 0.49 ms |
| 3-of-5 | 6 | 192 | 3.96 ms |
| 4-of-7 | 20 | 640 | 22.29 ms |
| 5-of-9 | 70 | 2,240 | 111.05 ms |

These implementation timings include repeated polynomial-factor calculations that could be precomputed. The storage issue is structural: the classic subset construction uses `choose(n−1, t−1)` seeds per node. At 23-of-34 this is 193,536,720 seeds, approximately 6.19 GB per node with 32-byte seeds. Increasing user count does not cause that growth; increasing committee parameters does.

LaKey's paper explicitly targets secret-shared outputs and refreshable distributed key management. Its regular constructions offer a better fit for this long-lived service than the tested classic PRSS construction.[^4] The Web3Auth article discussing LaKey says that its described deployment chose a different, secret-nonce approach. It is not evidence that LaKey itself is operating in production there.[^5]

## Committee size and scale

**A million users does not require a larger committee than a thousand users.** Committee size expresses trust, compromise tolerance, availability, and governance. Audit volume determines processing and networking capacity.

Start integration testing with **3-of-5 independent operators**, rather than one operator running five machines. This is a practical configuration to evaluate, not a security theorem that five nodes are always sufficient. It tolerates two compromised shares for secrecy; three shares suffice for the PRE reconstruction after derivation.

Do not equate the PRE threshold with live MPC availability. The tested malicious Shamir derivation uses the configured committee and does not demonstrate that any three of five can complete derivation while two are offline. A production deployment must either require the configured parties online or implement and test a suitable threshold-MPC/resharing procedure. A hung participant must lead to timeout and retry or explicit failure, not weakened authorization or reconstructed clear keys.

Larger committees may provide better organizational distribution but add duplicated work and communication. Do not grow them automatically with users. First measure realistic concurrent investigations, batch sizes, operator bandwidth, and latency. Sharding into more independent roots changes trust and historical-key management and should be a separate decision.

## Required work before deployment

1. **Review LaKey parameters and composition.** Independently assess REG12/REG32, output distribution, total query volume, zero-key handling, and modern lattice attacks. The prototype uses the paper's dimension 512; the statistical setting does not certify computational strength.
2. **Use a maintained malicious-secure MPC implementation.** The research fork needed platform adaptations. A production integration needs pinned dependencies, authenticated participants, explicit session state, bounded preprocessing, timeouts, and tested abort/recovery. Do not write a new MPC protocol just to remove its dependency.
3. **Build a fixed derivation kernel.** The experiments compile a small fixed set of identities into bytecode. Production must not compile a new program for every address. Nodes must independently derive the public matrix from an agreed canonical identity; callers must never supply arbitrary matrices as trusted PRF inputs.
4. **Authenticate public keys and roots.** Validate commitments from all required participants; bind address, chain, ring, epoch, and field scope. Transfer verification must enforce the accepted association. Wrong roots, substituted keys, and stale epochs need adversarial tests.
5. **Keep shares local.** Replace the synthetic file handoff with node-local integration, preserving secret erasure and authenticated public commitment exchange. The experimental reference process intentionally has more access than any production node should have.
6. **Test refresh and recovery beyond the happy path.** Include stale backups, concurrent sessions, member replacement, interrupted refresh, malicious contributions, and preservation of historical decryption. Share refresh must preserve the master; changing the root is a different operation.
7. **Retain Orbis authorization and recording.** Recheck each accepted ciphertext and field. Derived keys do not replace ACP or DefraDB evidence. Do not cache a release across different readers or authorization contexts merely because the derived key is the same.
8. **Run complete integration and proof tests.** Measure actual Transfer proofs, browser/WASM production, live Orbis/ACP, DefraDB recording, and adversarial network behavior. None of those full-stack acceptance checks is claimed by this experiment.

## Reproducibility and boundaries

The original work was pushed before experimentation. The isolated experiment branch is `codex/orbis-key-experiments`, based on Shieldd `4f02fead03`. Its harnesses do not modify payment acceptance or re-enable the blocked named-person endpoint.

The LaKey source is pinned to MetaMask/MP-SPDZ `7a9efcadf134263a94cd0548456e60011a7d4492`. The native Orbis dependency is pinned in Cargo. The macOS patch and configuration are retained alongside the generated-program driver. `results/` contains exact commands, raw logs, and watchdog summaries. `RUN.md` contains reproduction commands.

The old MPIR build failed on modern compiler diagnostics and ARM assembly selection. The working adaptation uses GMP and current Boost names. A missing local TLS certificate caused an initial process abort and was corrected before retry. A 128-bit setting exposed an unused binary-backend initialization limit; the working build disables unsupported mixed circuits and uses the real arithmetic backend. The LaKey programs use arithmetic operations throughout. Failed attempts remain recorded.

Development Groth16 setup was generated only for the experimental components. Production setup approval, complete Transfer correctness, distributed Orbis service integration, and live ICP calls were not performed. No TEE was introduced. This report selects the next implementation direction; it does not mark the broader disclosure integration complete.

## Sources

[^1]: DFINITY, [VetKeys overview](https://docs.internetcomputer.org/concepts/vetkeys/), live API and architecture; [pinned client implementation](https://github.com/dfinity/vetkeys/blob/36ff80e9f4f4c044ce7eed2980fc9f683f12a2b5/backend/rs/ic_vetkeys/src/utils/mod.rs).
[^2]: NCC Group, [VetKeys Cryptography Review](https://www.nccgroup.com/media/35tjt504/ncc_group_dfinityusaresearch_vetkeys_report_2025-10-07_v10.pdf), October 7, 2025. The review is scoped to its listed implementations and commits, not this Orbis adaptation.
[^3]: Matthias Johann Steiner, [Gröbner Basis Cryptanalysis of Ciminion and Hydra](https://arxiv.org/abs/2405.05040), published in IACR Transactions on Symmetric Cryptology 2025(1), 240–275. This is parameter-specific cryptanalysis, not a claim that every Hydra parameterization is broken.
[^4]: Matthias Geihs and Hart Montgomery, [LaKey: Efficient Lattice-Based Distributed PRFs Enable Scalable Distributed Key Management](https://www.usenix.org/system/files/usenixsecurity24-geihs.pdf), USENIX Security 2024; [research implementation](https://github.com/MetaMask/MP-SPDZ/tree/7a9efcadf134263a94cd0548456e60011a7d4492/Programs/Source).
[^5]: Matthias Geihs and Tin Erispe, [Managing Multiple Keys with MPC-friendly Key Derivation Strategies in Web3Auth](https://blog.web3auth.io/managing-multiple-keys-with-mpc-friendly-key-derivation-strategies-in-web3auth/), March 1, 2024.
