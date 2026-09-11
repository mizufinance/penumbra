# Orbis audits with bounded secret-key storage

## Decision summary

Orbis must not retain a private key share for every user. Its persistent secret state should depend on its cryptographic configuration and committee, rather than the number of registered addresses. Derived address keys may exist temporarily during an audit, but must be reproducible from that fixed state and discarded afterwards.

This requirement rules out permanently stored, independently generated address keys as the recommended baseline. Three alternatives deserve consideration:

| Design | Persistent secret storage | Shieldd consequence | Evidence and principal limitation |
|---|---|---|---|
| Secret-shared derivation on demand | Fixed master state, with temporary derived shares | Can retain current Decaf377 encryption and registered public-key field | Published constructions and implementations; a production Orbis integration is not established |
| ICP vetKD with appropriately scoped IBE | Master shares and committee metadata, independent of users | New pairing-based compliance encryption and correctness constraints | Live ICP API, public implementation, published cryptography review; wallet proof cost is the main integration risk |
| Attested TEE receiving transaction-specific PRE | Current master shares remain in Orbis | Can retain existing encryption | Mature enclave building blocks; confidentiality then depends on the enclave and its approved code |

Classic pseudorandom secret sharing (PRSS) solves the per-user storage problem, but is only one way to implement the first row. It has committee-size and long-term refresh complications. It should not be treated as a complete production key-management solution merely because the underlying technique is established.

The most promising no-TEE comparison is therefore **on-demand secret-shared key derivation versus vetKD/IBE**. The former moves additional work to Orbis; the latter has much stronger directly relevant deployment evidence but moves significant work into payment encryption and its proof.

## Committee size does not follow user count

A cryptographic committee need not grow as users join. One committee can serve millions of identities using the same master shares. Its size and threshold are decisions about independent operators, compromise tolerance, and availability. Processing capacity instead depends on registrations, audit requests, scanned ciphertexts, and batching.

For a simple `t`-of-`n` release threshold, fewer than `t` colluding key holders cannot reconstruct the secret, and at least `t` cooperative holders are required for release. These simple counts do not specify every consensus, DKG, or MPC protocol's additional assumptions.

| Example | Key holders needed to release | Maximum colluding holders below threshold | Nonparticipating holders tolerated for threshold release |
|---|---:|---:|---:|
| 2-of-3 | 2 | 1 | 1 |
| 3-of-5 | 3 | 2 | 2 |
| 5-of-7 | 5 | 4 | 2 |

These are examples, not recommendations based on user counts. Three or five operators are only meaningfully independent if they do not share the same controlling organization, credentials, or correlated operational failures. Increasing committee size does not automatically increase throughput: nodes often repeat verification and cryptographic work for the same request.

An appropriate capacity model is `audit batches × ciphertexts per batch × requested fields`, with separate accounting for derivation and PRE. In an on-demand address-key design, Alice's shares can be derived once for a bounded batch and used for its individual PRE operations. The derived shares need not be written to permanent storage.

Public registrations, authorization records, transaction evidence, and audit logs still grow with usage. Bounded **secret-key storage** does not mean the whole system can have no per-user or per-request information. Those existing records can remain in Shieldd, ACP, and DefraDB.

## PRSS: what it solves

PRSS establishes distributed secret seeds, then uses an agreed public input to produce shares of a reproducible pseudorandom value. Applied here, the input could bind the address, ring, key purpose, and version. Nodes regenerate the address-key shares when needed, publish only the corresponding authenticated public key during registration, and discard transient shares after use.

The classic construction is an established result from Cramer, Damgård, and Ishai, published in 2005. It supports generating Shamir shares locally after setup; it is not the invalid operation of hashing each ordinary master share independently.[^1]

The important distinction is that the node retains several setup seeds rather than millions of child shares. The number of setup seeds is independent of user count, but can grow very quickly with committee parameters.

For the basic replicated-seed construction, a node holds `choose(n−1, t−1)` seeds for a `t`-of-`n` threshold. Assuming 32-byte seeds, the following are arithmetic storage estimates only:

| Committee | Seeds per node | Raw seed bytes per node |
|---|---:|---:|
| 2-of-3 | 2 | 64 B |
| 3-of-5 | 6 | 192 B |
| 7-of-10 | 84 | 2,688 B |
| 14-of-20 | 27,132 | 868,224 B |
| 20-of-30 | 20,030,010 | 640,960,320 B |
| 23-of-34 | 193,536,720 | 6,193,175,040 B |

These figures exclude authentication, commitments, backup, and operational state. They describe this construction, not a lower bound on all possible threshold derivation protocols. A million users do not change any row; changing the committee does.

### Implementation and deployment evidence

PRSS is not only theoretical. MPyC includes concrete PRSS code, documentation, and tests. However, MPyC's documented security model is a passive, dishonest-minority model. That is not sufficient evidence that this implementation is suitable for actively malicious Orbis nodes.[^2]

There is also historical deployment evidence for related PRSS-based techniques: the 2008 Danish sugar-beet auction paper describes deployed secure computation, including a PRSS-based input-sharing method. That demonstrates practical use of the technique, not a current, continuously operating threshold key-management service with millions of derived identities.[^3]

No directly verified public evidence was found of the precise proposed service: long-lived, malicious-secure, dynamically refreshable PRSS-based compliance-key derivation at that scale. Availability of PRSS in an MPC library does not establish those additional properties.

### The unresolved lifecycle problem

The same setup must reproduce the same keys to decrypt historical ciphertexts. Simply replacing the PRSS seeds changes the derived keys. Retaining the old seeds forever preserves decryption but complicates protection against compromises accumulated across time and committee replacement.

This concern is not speculative: the LaKey paper explicitly identifies PRSS master-state resharing as problematic for long-lived distributed key management. That is a limitation of the approach considered there, not an impossibility theorem for every PRSS variant.[^4]

Before selecting PRSS, require a concrete malicious-secure setup and public-key consistency protocol, plus an explanation of how existing keys survive refresh and changes of membership without retaining per-user shares. This is more consequential than avoiding a registration ceremony.

## On-demand secret-shared derivation beyond classic PRSS

A distributed PRF can instead derive an address secret inside MPC from fixed shared master state. Its output remains secret-shared. Nodes can use those transient shares in PRE and erase them afterwards. Public-key generation during registration exposes only the public key; the same registered key is recovered on later derivations.

LaKey, published at USENIX Security 2024, specifically investigates scalable distributed key management with secret-shared outputs and refreshable master shares. Its public research implementation is available in the `lattice-prf` branch of the MP-SPDZ fork now under MetaMask. The paper evaluates parameterizations with eight or ten online rounds. Those are research measurements for its configuration, not Orbis latency measurements.[^4][^5]

This provides a concrete candidate rather than an unspecified “hash the master key” suggestion. It is still not evidence that an audited production deployment of this exact construction exists. Repository ownership does not prove product deployment. Its security proof also needs to be distinguished from the full operational protocol for ongoing refresh and recovery.

The application design would be:

```text
Registration:
  shared master state + address context
    → derive transient compliance-key shares
    → authenticate and register the public key
    → erase transient shares

Audit for Alice:
  shared master state + the same Alice context
    → reproduce transient shares
    → perform ciphertext-specific PRE for the authorized batch
    → erase transient shares
```

This meets the storage requirement without changing the encryption arithmetic merely to derive the key. Shieldd's registered `capk` remains the encryption public key. Registration validation must authenticate the new key provisioning instead of enforcing the existing public scalar multiplication.[^L1]

The remaining protocol work includes secure conversion to the required scalar distribution, public verification commitments for PRE, request/session consistency, malicious parties, private-channel authentication, and failures. The MPC's quorum rules must match the actual availability and corruption model; an implementation requiring an honest majority among participating parties cannot automatically be used with any arbitrary threshold-sized subset.

Master state for a PRF construction need not be a single 32-byte elliptic-curve scalar. It can be a fixed larger structure. The relevant property is that it does not grow with registered users, and that refreshing its shares preserves the derived keys.

**Assessment:** closest to the existing wallet design while meeting the corrected storage requirement. Derivation can be amortized over an Alice audit batch, rather than repeated per ciphertext. It deserves a concrete protocol assessment alongside vetKD, with no claim that the research code is already production-ready.

## ICP vetKeys: exact relevance

ICP's vetKD management API is live on mainnet. NCC Group published a cryptography review in October 2025 covering specified node and client implementations. It reported two low and three informational findings, with fixes retested and some residual items explicitly documented. This is materially stronger deployment evidence than the PRSS-derived service considered above, but applies to the reviewed code and scope, not automatically to an Orbis port.[^6][^7]

The inspected current `dfinity/vetkeys` source declares Rust package version 0.9.0. Some overview documentation still names older SDK versions. The research therefore distinguishes the current code snapshot from the older reviewed commits rather than treating all documentation as one version.[^8]

### Why it avoids the public-ratio problem

At a simplified level, vetKD produces a group-valued key:

```text
K_ID = x · HashToGroup(ID)
```

This differs from the current `H(address) · xG`: the public input is hashed to a curve point whose discrete logarithm relative to the generator is not known. Knowing Alice's derived point does not provide the public scalar ratio needed to convert it into Bob's.

The implementation additionally derives a context public key and hashes that public key together with the input. Nodes compute the hash themselves; they do not simply multiply an arbitrary caller-supplied point. Context derivation and input derivation are distinct layers and should be preserved when adapting the protocol.[^9]

Each node uses its master share to create an encrypted contribution. The contributions combine into a key encrypted under the recipient's transport public key. The coordinating application cannot decrypt it. Persistent cryptographic state consists of master shares, public verification information, and the normal committee/key lifecycle—not an entry for every identity.[^9]

### Scope it to each encryption and field

Do not request `K_Alice` and give it to the auditor. That would be a reusable decryption key for every encryption under that identity.

Instead, the application should construct the identity from a canonical encoding of:

```text
chain + key version + encryption reference + mode + field + optional subject
```

For an Alice audit, Orbis inserts Alice from the authorized request and derives the key for each selected encryption and permitted field. On an unrelated transaction, that key does not match the identity used by the sender. Returning it must not enable conversion to the actual recipient's key.

For a general audit, the identity omits the subject and uses the distinct general-audit mode. Separate amount, sender, and receiver identities preserve the three permissions. General access still requires corresponding master/general wrappings; stock IBE does not magically let an address-free request decrypt an arbitrary subject identity without that additional design.

The encryption reference must be available before encrypting. Using the final transaction hash would be circular if it includes the ciphertext. Prefer an appropriate existing pre-encryption commitment or unique transaction material, with explicit canonical association and reuse rules. The final transaction reference can then bind authorization and evidence. This binding requires an actual design; a caller-selected string called “transaction ID” is insufficient.

Participant privacy also requires review. ICP's serialized IBE ciphertext contains no plaintext identity field, but absence of that field alone is not a formal proof of recipient anonymity. The protocol must not publish a subject identifier or a publicly testable lookup artifact as an unintended consequence of integration.

### Encryption and decryption

The stock implementation uses BLS12-381. It encrypts with an identity-derived G1 point, a G2 public key, a bilinear pairing, and exponentiation in the pairing target group. It also uses SHA-256-based operations, HKDF, and SHAKE256. Decryption uses the vetKey with the ciphertext's G2 component, then verifies the encryption consistency condition.[^8]

The wallet can encrypt using public information, without contacting Orbis for each payment. Orbis later derives the correctly scoped key. The encrypted result can follow the existing intermediary → Defra → auditor retrieval flow. Using the protocol in Orbis would not require routing financial data to ICP's hosted network, but it would require porting and operating the corresponding threshold functionality.

## Shieldd consequences of stock vetKeys IBE

### Exact sizes available from code

The inspected IBE format consists of an 8-byte header, a compressed 96-byte G2 point, a 32-byte masked random seed, and a masked message of the original length. Its overhead is therefore **136 bytes**, and wrapping a 32-byte payload key occupies **168 bytes**.[^8]

The encrypted vetKey uses two 48-byte G1 points and one 96-byte G2 point: **192 bytes**. The node's encrypted contribution has the same three-group-element size, before request metadata and transport overhead.[^9]

These are serialized cryptographic objects, not complete transaction or batch sizes. If a straightforward design used four subject wrappers and three general wrappers, seven stock IBE encryptions of 32-byte payload keys would occupy 1,176 bytes for those wrappers alone. Detection ciphertexts, flagged paths, other payloads, references, and proofs remain additional. Reusing components to reduce this count requires separate analysis rather than assuming current ElGamal reuse transfers to IBE.

### Proving work is the significant tradeoff

The current Transfer proof handles Decaf377 operations and Poseidon-based encryption. Stock vetKeys encryption introduces hash-to-curve, pairing/target-group arithmetic, scalar operations, and byte-oriented hashes. For a private subject identity, the proof must establish that the identity corresponds to the actual transaction participant. A wallet cannot simply supply an unchecked precomputed pairing result.

Gnark has BLS12-381 pairing gadgets under its emulated arithmetic packages. These are not the same as the current inexpensive native-field Decaf operations. Gnark's native BLS12-377 pairing gadget runs inside a BW6-761 circuit; choosing BLS12-377 encryption would not automatically make it native inside the existing BLS12-377 proof.[^10]

A straightforward implementation calls the IBE encryption relation for every wrapper. Publicly determined general-audit inputs may permit moving some verification work outside the circuit; hidden subject inputs are more difficult. Other possibilities include an additional encryption proof linked to the payment proof, or a different pairing-friendly proof arrangement. Both enlarge the design and require rigorous binding of identical hidden values.

Consequently, no credible conversion from the current roughly 3.5-second reported proof time to a vetKeys proof time is available from these sources. Ciphertext-size arithmetic is straightforward; full proving time is not. Native IBE encryption benchmarks also do not predict SNARK proving time.

A custom construction using precomputed registered pairing values and ciphertext-specific threshold decryption might reduce some in-circuit pairing work. That would be a new adaptation with its own proof and privacy requirements, not adoption of the reviewed vetKD/IBE protocol unchanged. It should not be presented as an already solved cheap path.

**Assessment:** vetKD strongly fits the master-storage and isolated-release requirements. Stock IBE's correctness proof is the principal reason not to select it unconditionally before evaluating the complete encryption relation.

## TEE option without exporting reusable shares

The proposed enclave can serve as the intermediary, decrypt selected information, and write it to Defra. It does not need the nodes' reusable DKG shares.

Prefer this flow:

```text
Orbis validates authorization and the attested enclave recipient key
  → issues transaction-specific PRE results encrypted to that enclave
  → enclave verifies results and enforces subject/field restrictions
  → enclave sends only permitted facts and evidence to private Defra
```

The distinction matters. Giving a single enclave enough reusable scalar shares reconstructs the master secret inside one hardware boundary. Giving it only per-ciphertext PRE results limits the material exposed by an enclave compromise to results obtained, plus what an attacker can obtain while its credentials remain accepted. It does not make the enclave harmless: current shared points still allow the known conversion within the corresponding ciphertext scope.

The enclave must therefore prevent all shared-point, candidate-key, and unauthorized plaintext output. It must verify a real subject match before releasing a named-person result. CORE and EXT confirmation coverage needs explicit treatment; accepting a plausible amount or a decodable address is not authentication. The nodes still enforce ACP and the approved enclave, rather than granting the enclave an unrestricted master operation by default.

Attestation must bind the receiving encryption key to approved code and fresh request context. AWS Nitro Enclaves provides a concrete example of measurement-based attestation and encrypted secret delivery to an enclave key; implementing analogous checks in Orbis is additional work, not an automatic consequence of using a TEE.[^11]

This keeps the existing payment encryption and avoids persistent derived keys, but adds trust in the hardware vendor, firmware, attestation infrastructure, and enclave implementation. Because Defra receives plaintext, its operators now have access to that plaintext. The enclave can attest to its computation; that does not turn an assertion into an independently verifiable ZK proof.

Storage-before-release also retains a boundary: an ordinary database receives plaintext before acknowledging a commit. Enclave routing alone does not prevent a malicious database operator from observing that plaintext before persistence. Encrypted storage followed by a separate key-release step can change that boundary, but is a different storage/release design. For a trusted private Defra deployment, document the remaining operator assumption instead of claiming every bypass is eliminated.

## Recommended next decision

Remove permanent per-user shares from the proposed architecture. Do not select committee size from a forecast of users; select its trust and availability requirements, then size processing for audit demand.

Prioritize two bounded-state cryptographic options:

1. **On-demand secret-shared derivation**, with LaKey and other suitable distributed PRFs as concrete candidates. This preserves the existing encryption structure and can amortize derivation over an audit batch. Validate malicious security, refresh, scalar compatibility, and transient PRE commitments before treating it as implementable.
2. **vetKD with transaction/field-scoped IBE**, using the audited implementation as the reference. Map one complete encryption relation to Shieldd, including hidden identity binding, and assess its proving cost before extending it to every wrapper. Keep stock behavior as the reference when assessing optimizations.

Classic PRSS is most attractive for a small, relatively stable committee, but should not be the default recommendation until its historical-key and refresh story is resolved. The TEE path is a practical fallback if preserving current proving performance outweighs the additional hardware trust. It should receive transaction-specific encrypted outputs, not reusable master shares.

This is a research assessment, not implementation acceptance. No new circuits, prover runs, benchmarks, or live Orbis/ICP/TEE integrations were executed. The current-code observations and arithmetic estimates do not establish the security or runtime of a combined Shieldd protocol.

## Sources

[^1]: Cramer, Damgård, Ishai. [Share conversion, pseudorandom secret-sharing and applications to secure distributed computing](https://www.iacr.org/archive/tcc2005/3378_342/3378_342.pdf), TCC 2005. Seed counts in this report are calculations for its basic replicated setup.
[^2]: [MPyC README](https://github.com/lschoe/mpyc/blob/master/README.md), [PRSS implementation](https://github.com/lschoe/mpyc/blob/master/mpyc/thresha.py), and [documentation](https://mpyc.readthedocs.io/en/latest/mpyc.html). Public implementation and stated passive-security model.
[^3]: Bogetoft et al. [Secure Multiparty Computation Goes Live](https://www.acsu.buffalo.edu/~mblanton/cse715/sugar-beet-auction.pdf), 2008. Historical deployment, not a current key-management product.
[^4]: Geihs and Montgomery. [LaKey: Efficient Lattice-Based Distributed PRFs Enable Scalable Distributed Key Management](https://www.usenix.org/system/files/usenixsecurity24-geihs.pdf), USENIX Security 2024. Sections 1.4–1.6, 5, and 6 discuss requirements, PRSS limitations, evaluation, and the distributed-key-management model.
[^5]: [LaKey research implementation](https://github.com/MetaMask/MP-SPDZ/tree/lattice-prf/Programs/Source), originally linked by the paper under torusresearch. Public implementation availability does not establish deployment.
[^6]: DFINITY. [VetKeys overview](https://docs.internetcomputer.org/concepts/vetkeys/) and [management-canister specification](https://docs.internetcomputer.org/references/ic-interface-spec/management-canister/). Live mainnet status and API semantics.
[^7]: NCC Group. [VetKeys Cryptography Review](https://www.nccgroup.com/media/35tjt504/ncc_group_dfinityusaresearch_vetkeys_report_2025-10-07_v10.pdf), October 7, 2025. Audited scope, findings, and retest limitations.
[^8]: DFINITY. [Pinned IBE and client implementation](https://github.com/dfinity/vetkeys/blob/36ff80e9f4f4c044ce7eed2980fc9f683f12a2b5/backend/rs/ic_vetkeys/src/utils/mod.rs), particularly `IbeCiphertext`, `IBE_OVERHEAD`, `encrypt`, and `decrypt`; [package version and dependencies](https://github.com/dfinity/vetkeys/blob/36ff80e9f4f4c044ce7eed2980fc9f683f12a2b5/backend/rs/ic_vetkeys/Cargo.toml). Serialized-size totals are calculations from these definitions.
[^9]: DFINITY. [Node-side vetKD implementation](https://github.com/dfinity/ic/blob/master/rs/crypto/internal/crypto_lib/bls12_381/vetkd/src/lib.rs), including `EncryptedKeyShare::create`, `DerivedPublicKey`, and `EncryptedKey`; Cerulli et al., [vetKeys paper](https://internetcomputer.org/whitepapers/vetKeys_%20How%20a%20Blockchain%20Can%20Keep%20Many%20Secrets.pdf), 2023. Node source is a moving branch inspected during this assessment; the audit covers separately identified commits.
[^10]: Consensys. [Gnark algebra packages](https://pkg.go.dev/github.com/consensys/gnark/std/algebra). Emulated BLS12-381 and native BLS12-377/BW6-761 distinction.
[^11]: AWS. [Cryptographic attestation with KMS](https://docs.aws.amazon.com/enclaves/latest/user/kms.html), [attestation measurements](https://docs.aws.amazon.com/enclaves/latest/user/set-up-attestation.html), and [encrypted recipient responses](https://github.com/aws/aws-nitro-enclaves-sdk-c/blob/main/docs/kms-apis/kms-apis.md). Building-block reference, not a ready Orbis integration.
[^L1]: Shieldd local [registered compliance key](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/structs.rs:291) and [Transfer key binding](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/tools/gnark/internal/circuits/transfer_circuit.go:773). Reported existing proof measurements are in [Bankd integration notes](/Users/antoinecyr/Documents/Source/bankd-disclosure-integration/infra/disclosure-audit/direct-pre.md).
