# Orbis person-scoped audits and key management

## Recommendation

Keep Orbis, the intermediary, and private DefraDB. Change the compliance keys so that Alice’s key and Bob’s key have no publicly computable scalar relationship. Keep their private keys shared among Orbis nodes and continue returning only transaction-specific PRE results, encrypted to the auditor.

The simplest implementation baseline is an independent distributed key generation (DKG) for each registered compliance address/key version, using the existing committee. This adds key-management work at registration, rather than expensive work to every payment proof or every scanned transaction. It does not require a new standalone service or a separate committee for every address.

Pseudorandom secret sharing (PRSS) is the most relevant next candidate if registration volume makes individual DKG ceremonies expensive. It can produce independent, reproducible secret shares from an initial distributed setup. Its operational integration—especially malicious-node consistency, membership changes, and old-key recovery—needs a concrete design before choosing it over DKG. This is an established research direction, not a proposal to hash ordinary Shamir shares independently.[^1]

Keep the three separately authorized master wrappings for amount, sender, and receiver. Once subject keys are independent of the master key, sharing an ephemeral key between a field’s subject and master wrapping no longer enables the demonstrated public-ratio conversion. That removes this particular objection to reuse; it is not a general security proof of the complete encryption construction.

Do not export an address’s long-term secret key. Even a securely independent Alice secret would let its recipient decrypt subsequent Alice ciphertexts without further Orbis authorization or collection records.

## Required behavior and trust boundary

An Alice-authorized auditor must be able to submit transactions without already knowing whether Alice participated. Alice ciphertexts yield the permitted information; unrelated ciphertexts do not become decryptable, including when the auditor knows another party’s address and modifies the client.

General audits must independently permit amount, sender address, or receiver address without requiring an initial address. The three capabilities must remain separate. Flagged transactions remain under the issuer’s detection key, with issuer disclosure following the separate push path.

Payment creation should remain noninteractive after the required compliance registration is available. No individual Orbis node, intermediary, or database operator should obtain a reusable compliance private key. This requires a specified corruption threshold; a coalition reaching the decryption threshold can inherently bypass the protocol’s intended authorization.

The collection path remains:

```text
Accepted transaction ciphertext
  → intermediary
  → ACP-authorized Orbis nodes
  → encrypted transaction-specific result
  → intermediary stores it in DefraDB
  → authorized auditor retrieves, verifies, and decrypts locally
```

There is no TEE in this design. The intermediary operator is trusted to store results before releasing them through supported interfaces. Neither this routing rule nor Defra permissions can retract downloaded decryption material or prove that every later local read was logged. The objective is a durable record of each supported release, with the released capability unable to unlock unrelated transactions.

## What the current code does

The inspected Shieldd code derives a public scalar from the address using SHA-512 and multiplies the ring public key by that scalar. Write the ring secret as `x`, its public key as `P = xG`, and Alice’s public address-derived scalar as `d_A`. Her compliance public key is:

```text
Q_A = d_A P = d_A xG
```

For ciphertext ephemeral public key `R = rG`, the corresponding shared point is `d_A xR`. Orbis’s Decaf PRE implementation computes shares of `d_A x(Y + R)`, where `Y = yG` is the auditor’s transport public key. The auditor removes `yQ_A` and recovers `d_A xR`.[^L1][^L2]

This is a shared curve point, not the master private scalar. Nevertheless, the auditor knows `d_A`, so can compute:

```text
master shared point for this R = (d_A xR) / d_A
Bob shared point for this R   = (d_B / d_A) · (d_A xR)
```

The conversion applies to the same submitted ephemeral key. One response does not decrypt every unrelated transaction automatically. The problem becomes a person-scope violation when the auditor can submit Bob’s transaction under an Alice scan authorization and then convert the returned point. Reused master wrappings make that result useful even without knowing Bob’s address.[^L3]

A modified result might fail the official PRE proof verifier. That does not protect confidentiality: an adversarial auditor can ignore that verifier and use the transformed point directly to decrypt.

These equations establish a limitation of the primitive under the proposed scan behavior. They do not, by themselves, establish that every older Orbis endpoint was exploitable. The old document path also checked object-specific authorization and encryption binding. Historical exploitability requires tracing those authorization boundaries separately.

## Option 1: independent threshold keys at registration

Generate Alice’s compliance key `a` through DKG, and Bob’s key `b` independently. Publish `A = aG` and `B = bG` through authenticated compliance registration. Nodes retain shares; no registrar reconstructs either scalar.

An Alice request now produces a verifiable, auditor-encrypted result for `aR`. If `R` belongs to a Bob ciphertext, the required point is `bR`. Knowing `A`, `B`, and `aR` does not expose the public multiplier used in the current attack. This relies on the usual discrete-log/Diffie–Hellman assumptions and on the full PRE protocol maintaining its input and transport-key checks; it is not a replacement for a protocol security review.

Crucially, nodes do not have to identify whether Alice participated before answering. They select Alice’s registered threshold key because the authorization is for Alice, process the canonical ciphertext, and leave successful opening to the auditor. This preserves the desired scan behavior.

Shieldd already stores `capk` in its compliance leaf. The Transfer circuit binds that public key into the authenticated leaf and uses it for encryption. In the inspected circuit, it does not recompute the address-to-key derivation. Thus replacing how the registered public key is obtained need not add a secret KDF or new scalar multiplication to each Transfer proof.[^L1]

Registration does need to change. The present Rust constructors and `validate_registration` explicitly enforce `capk = d × ring_pk`. Replace that condition with authenticated evidence that the correct committee provisioned the specified key for the address, asset/ring scope, and key version. Accepting an arbitrary user-chosen key would allow a user to evade compliance encryption. Preserve the separate regulated-nullifier-key rules rather than inadvertently changing them alongside `capk`.

Orbis needs address-key records under its committee, public verification commitments, key selection in PRE requests, and lifecycle support. Its current encrypted `RingShareBundle` and refresh machinery are useful building blocks, but they are currently organized around ring keys; multi-address management is not already complete.[^L4]

This trades public offline derivation for authenticated public-key retrieval at registration. The general master key no longer mathematically derives the subject scalars; the master wrappings provide general decryption instead. That is compatible with the three-wrapping design, but should be an explicit design decision.

### Costs and limitations

- Payment proof: no additional derivation constraints are inherently required solely to replace the registered public key. This is a static observation, not a measured claim that every integration change is free.
- Ciphertext: no additional subject-key fields are inherently required; the compliance leaf already carries the key. Keep the separately planned master wrappings.
- Audit: the same class of threshold PRE operation per selected ciphertext/field, using different stored shares. No new round of generic MPC is required just to isolate Alice from Bob.
- Registration: a DKG ceremony, agreement, and durable key provisioning per address/key version.
- Storage and refresh: proportional to the number of retained address keys, with recovery and membership changes covering historical keys too.

For scale intuition only, one 32-byte secret scalar share plus `t` compressed 32-byte polynomial commitments is approximately `32(1+t)` bytes per address per node before identifiers, serialization, replication, and operational metadata. At threshold two and one million addresses, that narrow component is approximately 96 MB per node. This excludes DKG transcripts and does not estimate ceremony time. Lifecycle traffic may matter more than raw scalar storage.

## Option 2: reproducible independent keys with PRSS or threshold derivation

Secret-keyed derivation can produce unrelated child scalars, unlike a public address multiplier. Standard KDF guidance supports deriving distinct secret material from a secret root and domain-separated context. It does not specify how to perform the computation while preserving Orbis’s threshold secrecy.[^2]

Two implementation families are relevant:

1. Evaluate a suitable secret-keyed function with malicious-secure MPC and retain its output as shares. Publish only the derived public key during registration.
2. Use PRSS to generate pseudorandom Shamir shares directly from a committee’s distributed setup and an agreed address/key context. Authenticate the resulting public key before registration.

PRSS is particularly relevant because its published construction generates new shared pseudorandom values with local computation after setup. The classic construction has setup size that grows combinatorially with committee parameters and is intended principally for relatively small groups. It does not make authenticated registration, public commitments, or dynamic membership automatic.[^1]

For the basic replicated-seed construction and a `t`-of-`n` threshold, the number of subset seeds is `choose(n, t−1)`; a node holds `choose(n−1, t−1)` of them. A 2-of-3 setup therefore has three subset seeds, with two at each node. This is a structural count, not a complete Orbis protocol or a performance estimate.

Do not give every node a secret derivation multiplier that relates all subject keys to the master. A single node leaking that multiplier to an auditor could restore cross-subject conversion. Likewise, `Hash(share_i, address)` does not generally produce shares of `Hash(master_secret, address)`.

PRSS’s difficult operational question is preserving historical keys while refreshing or replacing its long-lived setup. Replacing all setup seeds generally changes generated keys. Keeping old seeds indefinitely complicates protection against compromises accumulated over time. A design must explicitly handle old-key recovery, committee changes, backup, and proactive security; “derive on demand” is insufficient by itself.

**Assessment:** the strongest optimization candidate for independent address keys. Choose between it and per-address DKG using actual committee size, address-registration rate, and refresh requirements. Neither changes the basic wallet encryption or per-ciphertext PRE equations once the public key is registered.

## Option 3: keep current derivation, change what Orbis releases

The public relationship is dangerous because the auditor obtains the shared point. Another solution is to compute a non-convertible result while that point remains secret inside the committee.

### A nonlinear decryption mask

Conceptually, nodes could jointly compute a domain-separated cryptographic KDF of the subject shared point and release only the resulting mask, encrypted to the auditor. Alice’s mask for Bob’s ephemeral key should not let the auditor calculate Bob’s mask. However, sender encryption must use exactly that mask, and the committee must never expose the underlying point through intermediate messages.

This is not ordinary Orbis PRE followed by a client-side hash. The client already has the point in that construction. Nor can nodes hash their partial points independently and interpolate the hashes.

The current child wrapping uses a field encoding of the shared point to mask a payload seed. Returning an unverified candidate seed would expose that encoding through the public wrapping equation. Replacing the mask with a proper nonlinear KDF, or verifying the candidate before release, is therefore material to this approach.[^L5]

### Match verification inside the committee

A second form keeps the existing ciphertext and privately checks that the requested subject key actually opens the relevant accepted ciphertext before returning usable decryption material. A mismatch returns no such material.

That check must use an authentication/confirmation relation that is actually constrained by the accepted transaction. CORE and EXT do not necessarily expose identical confirmation structure; an amount confirmation cannot simply be assumed to authenticate any address ciphertext. The precise permitted field and any required companion check must be designed and reviewed.

Both variants require secure distributed nonlinear computation, private output delivery, and a corruption model consistent with Orbis’s threshold. Frameworks such as MP-SPDZ provide multiple malicious-security protocols, but choosing one may change quorum assumptions, communication, and abort behavior. Distributed symmetric encryption research demonstrates related constructions, not a drop-in implementation for Shieldd’s exact curve and Poseidon formulas.[^3][^4]

**Assessment:** technically credible and worth retaining if public address derivation is essential. It adds work to every scanned item, including nonmatches, and introduces a substantially larger cryptographic protocol than independent key provisioning. No source found establishes its runtime for this application. It is not the first implementation recommendation.

## Option 4: vetKeys, identity-based encryption, and hierarchical encryption

ICP’s vetKeys is the closest deployed design reference found. Its interface derives verifiable key material through threshold participation, encrypting delivery to the client’s transport key. The client, rather than the coordinating application, obtains the raw derived material. Its public implementation is useful to study without implying a dependency on ICP’s deployed network.[^5][^6]

The vetKD paper builds on verifiably encrypted threshold BLS and Boneh–Franklin identity-based encryption. A key uses a hash-to-group input rather than a publicly known scalar multiple of a fixed generator. This is the important distinction from the current derivation; merely changing the hash used to produce `d_A` would not achieve it.[^7]

For this audit workflow, releasing a vetKey whose identity is only “Alice” would still be too broad. A usable key must be limited to a particular encryption and permitted field, or kept inside the committee. Identity/context design also needs to preserve participant privacy and bind the identity to the accepted payment. Do not introduce a circular encryption input by requiring the final transaction ID before constructing ciphertext that contributes to that ID.

Pairing-based encryption is not a direct substitution for Decaf377 ECDH. Orbis already containing another curve backend would not make the wallet ciphertexts, Transfer constraints, threshold protocol, or verifier interoperable. Circuit-native encryption correctness, recipient anonymity, chosen-ciphertext security, and field separation would all need a concrete construction and new cost analysis.

Hierarchical identity-based encryption can also provide genuine parent/child decryption capabilities. BBG HIBE gives compact ciphertexts, while anonymous HIBE addresses the additional requirement that ciphertexts not reveal their target identity. These works show that master decryption without a visible address is possible; the impossibility claim applies to the present derivation, not to all public-key encryption.[^8][^9]

However, issuing an Alice hierarchy key to the auditor would again allow unlogged future decryption. The root must remain threshold protected and results must remain limited to individual encryptions. HIBE does not automatically provide Orbis’s delegated authorization, encrypted delivery, or Defra recording.

**Assessment:** strong long-term reference, especially if the project wants a reusable threshold key-management platform. Larger payment-cryptography changes make it less attractive than using the already registered compliance public keys.

## Other systems and approaches

| Approach | Relevant lesson | Why it is not an immediate replacement |
|---|---|---|
| TACo | Nodes independently evaluate conditions and issue decryption fragments; storage is separate. | Its documented per-ciphertext conditions do not automatically establish that a hidden transaction participant is Alice. That relationship still needs cryptographic enforcement. |
| Shutter | Threshold identity-based key release supports noninteractive encryption followed by controlled release. | Its commit/reveal use releases identity decryption keys. Private auditor delivery and sufficiently narrow identities require adaptation. |
| Conditional PRE | Published constructions cryptographically restrict re-encryption to conditions. | A condition label must be bound to the real hidden participant and accepted ciphertext. Adding a tier string to current PRE is not an implementation of conditional PRE. |
| Standard OPRF/VOPRF | Hash-to-group inputs avoid the known scalar relationship of `H(address)G`. | An OPRF output is not automatically a compatible ECDH private scalar or an existing-ciphertext decryption result. Threshold issuance and output scope remain necessary. |
| Standard secret KDF or hardened wallet derivation | Secret-dependent child keys can avoid public conversion. | A single party computing the root-based derivation would weaken threshold secrecy. Wallet derivation standards do not supply the needed distributed protocol. |
| General functional encryption, attribute-based encryption, FHE | Can express richer access or computation. | No reviewed candidate offers a smaller integration for three fields and address-scoped scans; these introduce much broader cryptographic changes. |

TACo, Shutter, conditional PRE, and OPRF descriptions are supported by their respective primary documentation, paper, and standard.[^10][^11][^12][^13] The suitability judgments are analysis of their fit to this workflow, not claims that those systems have the Orbis vulnerability.

## Comparison

| Candidate | Alice scans arbitrary transactions | Extra payment-proof work | Extra work location | Main unresolved cost |
|---|---|---|---|---|
| Independent address keys via DKG | Yes, under reviewed PRE and independent-key assumptions | None inherent to replacing registered `capk` | Registration and key lifecycle | Many ceremonies and historical-key refresh |
| Independent address keys via PRSS | Yes, if provisioning and consistency are secured | None inherent to replacing registered `capk` | Distributed setup, registration, lifecycle | Setup size, malicious consistency, rotation |
| Secret KDF through MPC at registration | Yes, if output remains shared | No need to prove KDF in each Transfer | Registration | MPC complexity and throughput |
| Nonlinear output computation in Orbis | Potentially, with a reviewed complete protocol | Mask variant changes encryption constraints | Every scanned ciphertext | Network rounds, nonlinear computation, private outputs |
| Private match gate in Orbis | Potentially, with a sound field-specific confirmation | May preserve existing encryption | Every scanned ciphertext | Confirmation coverage and MPC |
| vetKD / anonymous IBE or HIBE | Can be designed for it | Substantial encryption-circuit changes | Encryption and threshold release | Concrete anonymous scheme and circuit costs |
| Exact transaction/value ACP approvals | Does not implement the required Alice-discovery guarantee by itself | Little or none | Authorization | Changes the product requirement |

Fresh master ephemeral keys alone, different mode names, a different public scalar hash, or ordinary transport encryption are not fixes for the Alice-to-Bob conversion. Returning Alice’s existing scalar is worse: since `x_A = d_A x`, anyone receiving it can recover `x = x_A / d_A` directly. An independently generated Alice scalar avoids that escalation but still bypasses subsequent per-transaction collection.

## Integration implications and verification

The preferred design keeps service ownership unchanged. Orbis provisions and manages subject key shares and performs PRE. Shieldd registration authenticates the public keys; the wallet uses the registered keys in the existing encryption constraints. ACP authorizes named-person or general field requests. Bankd retrieves accepted ciphertext, submits bounded requests, and stores exact encrypted results in DefraDB. The auditor verifies and decrypts locally.

Every participating node must select its key from authenticated request context and verify the canonical ciphertext association. Protect the reader-key proof of possession and the existing safeguards against chosen transport keys and arbitrary points. Bind the address/key version, mode, tier, ring, ciphertext reference, auditor key, and authorization context. The signature/PRE proof records the scope; independent keys enforce the particular confidentiality boundary that labels alone cannot enforce.

Separate private-key **resharing**, which preserves a public key, from **rotation**, which creates a new public key. Historical ciphertexts need historical keys or a deliberately designed replacement wrapping. A new committee must not silently lose access to retained evidence, and a removed node must not remain able to combine old material with later compromises without considering the proactive-security model.

Keep master audit permissions separate from subject permissions. With independent subject keys, the current same-field master/subject EPK reuse is not subject to the demonstrated public-ratio attack. Nevertheless, preserve independently randomized encryption across independently protected fields; inventory every use of the released shared point, including issuer detection and any equivalent duplicate ciphertexts. Authorization cannot be narrower than the information the released material actually unlocks.

Before implementation acceptance, require a real adversarial test in which an Alice grant is used against a valid Bob transaction with Bob’s address known. The modified client must fail to obtain Bob’s plaintext, without relying on official-client checks. Repeat against all master wrappings and all three field permissions. Include collusion with fewer than the threshold number of nodes, substituted registered keys, corrupted shares, reader-key attacks, replay, key versions, cross-ring requests, and flagged transactions.

Also test registration crash recovery, conflicting provisioning attempts, public-key agreement, historical-key recovery, and committee resharing. Confirm that no reusable subject scalar appears in application output, logs, Defra records, or transport messages. Test collection storage failure and per-item batch status without pretending that downloaded information can be revoked.

## Evidence limits and decision

The integration notes report an 800-byte ciphertext, including 96 bytes for the three master wrappings, and 176,187 Transfer constraints versus 163,396 before the addition. They report five development Groth16 proofs taking 3.434–3.482 seconds each. Those are reported implementation measurements, not measurements of independent-key registration, PRSS, or MPC alternatives, and they are not a security endorsement of the current person-scoped path.[^L3]

This assessment used static inspection of Orbis commit `986d19c53c6902db9ec310f6b98dec7d83d995e0`, Shieldd worktree commit `ac267e9bdf50d5ea0f65b79fa16f4017fe528b60`, and Bankd’s embedded Shieldd commit `e94f1e3de46f281959bebed1e3c11bada1b6336c`, together with the cited local integration notes. Working-tree content can differ from those commits. No new proof generation, builds, live integrations, or production security certification are represented here.

The recommended next design is **independent registered subject keys, existing ciphertext-specific Orbis PRE, and the three master wrappings**. Use ordinary DKG as the reference implementation design. Assess PRSS before committing to large-scale per-address provisioning; it is the most directly relevant way to reduce that cost without adding work to the Transfer circuit. Reserve distributed nonlinear output computation for a requirement to preserve public derivation, and vetKD/HIBE for a broader encryption redesign.

## Sources

[^1]: Ronald Cramer, Ivan Damgård, Yuval Ishai. [Share conversion, pseudorandom secret-sharing and applications to secure distributed computing](https://www.iacr.org/archive/tcc2005/3378_342/3378_342.pdf), TCC 2005. Local generation of Shamir shares, replicated setup, and size limitations. Protocol application and operation counts above are analysis.
[^2]: NIST. [SP 800-108 Rev. 1, Recommendation for Key Derivation Using Pseudorandom Functions](https://csrc.nist.gov/pubs/sp/800/108/r1/upd1/final), updated 2024. Secret-keyed KDF background; not a threshold implementation specification.
[^3]: [MP-SPDZ official repository](https://github.com/data61/MP-SPDZ). Supported MPC security models and protocols; no Shieldd performance claim follows from their availability.
[^4]: Shashank Agrawal, Payman Mohassel, Pratyay Mukherjee, Peter Rindal. [DiSE: Distributed Symmetric-key Encryption](https://csrc.nist.gov/CSRC/media/Events/NTCW19/papers/paper-AMMR.pdf), NIST-hosted paper. Distributed encryption background.
[^5]: DFINITY. [VetKeys developer documentation](https://docs.internetcomputer.org/concepts/vetkeys/). Threshold, encrypted, verifiable key delivery and context/input semantics.
[^6]: DFINITY. [vetKeys libraries and examples](https://github.com/dfinity/vetkeys). Official implementation reference.
[^7]: Andrea Cerulli, Aisling Connolly, Gregory Neven, Franz-Stefan Preiss, Victor Shoup. [vetKeys: How a Blockchain Can Keep Many Secrets](https://internetcomputer.org/whitepapers/vetKeys_%20How%20a%20Blockchain%20Can%20Keep%20Many%20Secrets.pdf), 2023. In particular Sections 4 and following on verifiably encrypted threshold BLS and vetKD.
[^8]: Dan Boneh, Xavier Boyen, Eu-Jin Goh. [Hierarchical Identity Based Encryption with Constant Size Ciphertext](https://crypto.stanford.edu/~dabo/papers/shibe.pdf), EUROCRYPT 2005. Parent/child encryption capabilities and compact ciphertext construction.
[^9]: Xavier Boyen, Brent Waters. [Anonymous Hierarchical Identity-Based Encryption (Without Random Oracles)](https://ai.stanford.edu/~xb/crypto06a/anonymoushibe.pdf), CRYPTO 2006. Identity anonymity in hierarchical encryption.
[^10]: TACo. [How Threshold Access Control Works](https://docs.taco.build/getting-started/access-control). Per-ciphertext conditions, node checks, and client decryption.
[^11]: Shutter. [How Shutter API Works](https://docs.shutter.network/docs/protocol/api/how_it_works), and its linked official API reference. Threshold key-release workflow; documentation itself identifies the repository README as the current API reference.
[^12]: [A Provably Secure Conditional Proxy Re-Encryption Scheme without Pairing](https://eprint.iacr.org/2019/1135), 2019. Evidence that conditional PRE is a distinct cryptographic construction; no deployment or audit claim.
[^13]: IETF. [RFC 9497: Oblivious Pseudorandom Functions Using Prime-Order Groups](https://www.rfc-editor.org/rfc/rfc9497.html), 2023; [RFC 9380: Hashing to Elliptic Curves](https://www.rfc-editor.org/rfc/rfc9380.html), 2023. Hash-to-group and OPRF definitions.
[^L1]: Shieldd local source: [address derivation](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/crypto.rs:25), [registration](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/structs.rs:291), and [Transfer compliance leaf and key use](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/tools/gnark/internal/circuits/transfer_circuit.go:773). Equivalent registration and key-use structure checked in Bankd’s embedded Shieldd.
[^L2]: Orbis local source: [Decaf PRE equations](/Users/antoinecyr/Documents/Source/orbis-disclosure-integration/crates/crypto/src/decaf377/pre.rs:541).
[^L3]: Bankd local [direct PRE integration notes](/Users/antoinecyr/Documents/Source/bankd-disclosure-integration/infra/disclosure-audit/direct-pre.md). Reported test results and explicit person-scope limitation.
[^L4]: Orbis local [encrypted ring share storage and refresh state](/Users/antoinecyr/Documents/Source/orbis-disclosure-integration/bin/orbis-node/src/ring_state.rs:24).
[^L5]: Shieldd local [Transfer encryption constraints](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/tools/gnark/internal/compliance/transfer_encryption.go) and [shared-point constraints](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/tools/gnark/internal/compliance/spend_shared.go).
