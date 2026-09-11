# Master-key access to Shieldd audit ciphertexts

**A master key can decrypt unknown-child ciphertexts in some encryption schemes. The current Shieldd/Orbis construction does not provide that property.** Changing the derivation hash alone does not supply it. Anonymous hierarchical identity-based encryption provides a concrete counterexample to any claim that the feature is universally impossible, but requires a different encryption system.

For Shieldd, the practical choice is between changing the access rules of existing ciphertext fields, adding a small master-key recovery component, or replacing the compliance encryption scheme. Avoiding a new field is possible if access semantics change; it is not a demonstrated free optimization that preserves every existing property.

This analysis concerns ordinary, unflagged Transfers. Flagged Transfer fields remain encrypted to the issuer detection key, which is independent of the Orbis ring. A general-audit change must not silently give the ordinary ring access to those fields.

**What the current code actually does**

The inspected Shieldd commit is `ac267e9bdf50d5ea0f65b79fa16f4017fe528b60`. Its derivation uses SHA-512 with a domain separator over the full canonical address, reduced to a scalar. The registered compliance public key is the ring public key multiplied by that scalar. The Transfer circuit uses the registered sender/receiver compliance keys for ordinary encryption, and the issuer key for flagged encryption. These are constrained key selections, not merely wallet conventions. [L1–L4]

| Current ordinary field | Plaintext | Encryption public key |
|---|---|---|
| Sender CORE | Amount | Sender address-derived key |
| Sender EXT | Receiver address components | Sender address-derived key |
| Output CORE | Amount | Receiver address-derived key |
| Output EXT | Sender address components | Receiver address-derived key |

Write the master secret as `x`, its public key as `P = xG`, the address-derived scalar as `d`, and fresh encryption randomness as `r`. The child public key is `Q = dP`. The transaction publishes `R = rG`; its encryption uses the shared point `S = rQ = xdR`.

The master secret computes `xR`. It still needs `d` to compute `xdR`. A no-derivation Orbis operation uses multiplier one; it does not discover the missing multiplier. Knowing the child public key also does not by itself solve this: dividing `Q` by known `x` yields `dG`, and computing `rdG` from `rG` and `dG` is the computational Diffie–Hellman problem. This is an algebraic analysis of the inspected implementation, not an impossibility theorem about all encryption. [L1–L4]

The conclusion is conditional on the normal cryptographic hardness assumptions and on the address not already being available through another source. An authority with a finite candidate address list can try those candidates and use the existing CORE confirmation to recognize a match. That is searching for the path, not decrypting independently of it. Addresses are not passwords with a necessarily small search space.

Hierarchical deterministic wallet keys illustrate the same distinction: BIP32 derives a child using a parent extended key **and an index**. Its parent-key property does not mean that an unknown index is unnecessary. BIP32 is not itself an encryption scheme. [1]

The demo's no-address investigation succeeds through an additional encryption under the base ring key. It does not demonstrate no-address decryption of the original address-derived ciphertext. Removing that additional encryption without replacing its capability removes general investigation access. [L5]

**A real cryptographic counterexample**

Boneh–Boyen–Goh HIBE supplies a compact illustration. Its master secret contains `g2^alpha`; a ciphertext includes `A = M · e(g1,g2)^s` and `B = g^s`, where `g1 = g^alpha`. Therefore the master can compute `M = A / e(B,g2^alpha)` without the recipient identity. Child keys require the scheme's additional identity-dependent component. This is a direct algebraic consequence of the published construction, not a proposed Shieldd modification. The paper's ciphertext contains three group elements; “constant size” means independent of hierarchy depth. [2]

The basic example is not sufficient for Shieldd's identity privacy: a construction must also prevent observers from testing candidate identities. Boyen–Waters anonymous HIBE explicitly addresses identity hiding. In its full construction, the master holds `w_hat`, and the ciphertext includes `E = M · Omega^(-r)` and `c0 = g^r`, with `Omega = e(g,w_hat)`. Thus `E · e(c0,w_hat)` recovers the message without the identity. This confirms that **anonymous ciphertexts and master decryption without an identity can coexist**. That construction carries several additional group elements. [3]

Later anonymous HIBE work also achieves ciphertext size independent of hierarchy depth using Type-3 pairings. This is relevant research, but does not establish that its ciphertext is as small as Shieldd's current encoding, or that it is compatible with Orbis's current PRE operations. [4]

These examples concern the master/root. They should not be generalized into a claim that every intermediate parent in every HIBE can decrypt every descendant without knowing its remaining identity components. That behavior must be checked for the chosen construction.

**Why HIBE is more than a derivation change**

Adopting anonymous HIBE would change the compliance key types, ciphertext elements, encryption relation, recipient decryption, and the distributed operations performed by Orbis. Its pairing-based groups are not the current Decaf377 encryption group. Shieldd's use of BLS12-377 for Groth16 does not make its Decaf377 ciphertext a pairing-based HIBE ciphertext.

The cost relevant to this system is not simply a native pairing benchmark. The wallet must prove correct encryption and transaction association in its existing proof system. Some HIBE encryption can use precomputed pairing values, so it is inaccurate to say that all variants necessarily calculate pairings while encrypting. Nevertheless, the circuit must constrain the selected construction's group operations and message-key relationship. Those operations need measurement in the actual circuit, not an assumption that the current gadget can be reused unchanged.

Threshold master decryption may be mathematically distributable, but the current Orbis protocol also protects the auditor's decryption key and verifies its shares. A threshold decryption design that exposes the reconstructed plaintext or usable decryption secret to the intermediary would violate the intended flow. Threshold availability, recipient confidentiality, publicly verifiable shares, replay protection, and malicious-ciphertext handling all require a concrete protocol.

The Rust `hohibe` crate implements BBG HIBE and is a possible research starting point. Its existence is not evidence of an audited anonymous-HIBE implementation, production readiness, or compatibility with the current Orbis interface. No such compatibility was established here. [5]

**Other related schemes and what they do not automatically solve**

| Family | Relevant capability | Limitation for this task |
|---|---|---|
| Anonymous HIBE | Hidden identities with delegated keys; some constructions permit direct master decryption | New ciphertext and cryptographic operations; integration costs unmeasured |
| Escrow ElGamal | One authority can decrypt for many independent recipient keys | The classic Boneh–Franklin escrow decryption equation still uses the recipient public key; a hidden unknown recipient is not automatically resolved |
| Dual-receiver encryption | One message can be decrypted by either of two recipients | Does not automatically provide anonymous identities, arbitrary hierarchy, or current-sized ciphertext |
| Identity recovery encryption | An authorized party can recover a hidden identity | Can add dedicated encrypted identity material; not necessarily a size saving |
| Key-aggregate encryption | Compact delegation for selected ciphertext classes | Compact keys do not imply unknown-identity master decryption with the current ciphertext |
| Ordinary DH/HPKE plus a different KDF | Established encapsulation to a selected recipient key | Does not itself add a parent decryption capability |

The classic escrow example is particularly easy to overread: its escrow equation contains the recipient's public key as an input. It demonstrates global escrow, but not the exact unknown-address capability needed here. [6] Dual-receiver research also emphasizes that both recipients must recover the same plaintext; this is directly relevant to binding any two decryption paths in a payment proof. [7]

Anonymous IBE with identity recovery treats identity recovery as an explicit function. The examined construction includes an encryption of the identity to the recovery authority. It supports the proposed encrypted-path approach conceptually, rather than making its extra information disappear. [8] Key-aggregate encryption addresses compact delegated rights, a different optimization. [9] HPKE distinguishes encapsulation from the subsequent symmetric encryption, but its standard DH construction has no implicit master/child access rule. [10]

**Changing only the algebra can accidentally remove child isolation**

A tempting no-extra-field modification is to publish `R' = rdG` instead of `R = rG`, while keeping `S = r(dP)`. Now the master computes `xR' = S` without knowing the address. This does make the master equation work.

But it effectively makes the encapsulation an ordinary master-key encapsulation: `rd` is simply fresh effective randomness. A holder of a scalar child secret `xd`, knowing its public derivation `d`, can compute `(xd)/d = x` and recover the master secret. Public scalar multiplication therefore does not produce independently delegable child secrets. That scalar-export problem already exists for the present derivation; the current design must keep the ring/derived secrets inside Orbis, rather than distribute them as standalone child private keys.

Even with secrets retained inside Orbis, a purported subject operation on `R'` can cancel any supplied nonzero derivation and return the same master shared point. Merely providing Alice's path no longer makes a different person's ciphertext fail to decrypt. Address restriction must come from independently validated authorization or additional evidence. This is a change in the protection supplied by the ciphertext, not a free hierarchy upgrade.

This algebraic sketch is not a proposed production scheme. It explains why a superficially successful master-decryption test is insufficient: the corresponding wrong-child test matters just as much.

**Option A: reuse the existing address fields**

For ordinary transactions, encrypt the existing sender and receiver address fields directly under the master ring public key, while retaining address-derived keys for amounts. The public key/ciphertext element types can remain the same. Merely changing which key protects the existing fields does not inherently add a ciphertext field.

General audits can then recover the parties without knowing either party first. This is the closest match to the intended statement that the master decrypts both counterparties.

The tradeoff is explicit: the existing counterparty ciphertext is no longer protected by Alice's derived key. An Alice-specific auditor needs ACP-authorized access to a master-key PRE operation for that exact address field. Orbis must establish the transaction's relationship to Alice through evidence it can actually validate. A caller's Alice label or an unverified object association is insufficient. A design that grants only general investigators access to those fields is simpler, but would remove named-person counterparty access and therefore is a product change.

There is also a concrete encoding issue. Current EXT plaintext contains a 32-byte diversified generator and a 32-byte transmission key. Current derivation hashes a 48-byte canonical address containing the original 16-byte diversifier and transmission key, after jumbling. The generator is obtained by hashing the diversifier; it is not an encoding that can simply be reversed to recover that diversifier. [L6]

Consequently, recovering current EXT data does not alone reconstruct the exact current derivation path. Possible designs are: use a trusted KYC mapping to the full address; change the derivation to the already committed address components; or encrypt a reconstructible full address in the existing field. Each needs its corresponding registration/circuit checks. This encoding issue is separate from the parent-key question.

Option A is worth evaluating if avoiding added ciphertext bytes is the priority and the revised authorization model is acceptable. It must not be described as preserving the current child-key restriction on counterparties.

**Option B: encrypt derivation information to the master**

Retain the original fields and add master-readable derivation information. The master operation recovers the information needed for a subsequent child-key PRE operation. This preserves existing named-person encryption and permits a general investigation to start without an address.

There are two potentially useful plaintexts: the canonical address bytes, or the reduced derivation scalar. The scalar is sufficient for the DH calculation, but is not itself an address and does not automatically satisfy an ACP rule expressed in terms of an address. A request protocol would need to accept and validate its connection to the accepted transaction rather than blindly treating caller-supplied scalars as approved paths.

For a design using a fresh Decaf ephemeral point and directly masking one scalar represented in a field element, the raw arithmetic budget is approximately 32 bytes for the point plus 32 for the masked scalar, per party, before additional authentication/framing decisions. This is an encoding estimate for a candidate design, not a reviewed construction or benchmark. Full-address encryption has a different budget. Sender and receiver use different derivations; one scalar does not automatically cover both.

The Transfer proof must bind the added value to the actual registered key and selected party. Encrypting a caller-chosen unrelated path would preserve transaction validity while making the audit fail, unless the new relation is constrained. This would be a new constraint for a new feature, not a correction of the existing seed relation.

Fresh encryption and domain separation are necessary. Reusing an existing shared secret to save another point needs analysis of what the resulting PRE access permits across the fields sharing it. Returning an encrypted path through Defra preserves storage routing; recovering that path may require a second authorized PRE round to obtain the original data.

**Option C: give the master a second wrapping of an existing tier key**

Instead of encrypting the address/path, allow the master to recover the encryption key already used for a selected tier. The encrypted amount/address payload stays single; only access to its key gains another recipient.

In the current arithmetic, a tier stores `C2 = seed + compress(r · child_public_key)`. A candidate second wrapping is `C2_master = seed + compress(r · master_public_key)`, using the already published `R = rG`. Both paths then reach the same tier key and original ciphertext. The master needs neither the derivation nor a reconstructed address.

This candidate adds one 32-byte field element per supported tier when reusing that tier's ephemeral point. Four tiers would add 128 raw bytes to the current 704-byte compliance ciphertext, excluding metadata and transaction/proof framing. This is a size calculation, not a performance measurement or a completed security analysis. Fresh independent randomness would have a larger budget.

The exact scheme must be reviewed for related recipient keys, shared randomness, selective key disclosure, domain separation, and soundness. Research supports randomness reuse for specific multi-recipient constructions, not a blanket rule that reuse is always safe. [11] The circuit must prove that both wrappings recover the same key used by the accepted payload, and that the master recipient is the permitted ring key.

Unlike the discarded off-chain PoC, this would be part of the transaction and enforced by its proof. It would not depend on the sender voluntarily retaining or uploading an unrelated audit bundle. The intermediary would still receive and store only the PRE result; local auditor decryption remains possible.

This option is particularly attractive when preserving named-person access, independent tier permissions, and the current Orbis group matters more than eliminating every additional field. Flagged transactions must retain their issuer-only treatment. Unflagged versus flagged wire shape must not publicly reveal a flag merely through the presence or absence of the new components.

**Comparison and recommendation**

| Option | General audit without an initial address | Preserves current named-person cryptographic access | Additional transaction data | Expected integration impact |
|---|---|---|---|---|
| Current construction | No, unless candidates are available | Yes | None | Already implemented |
| Change derivation hash only | No demonstrated solution | Depends on change | None | Does not resolve missing information |
| Master-encrypt existing EXT fields | Yes for existing address components | Not for EXT; ACP/evidence must replace that restriction | Potentially none | Circuit key selection, address encoding/derivation, authorization |
| Encrypt derivation to master | Yes | Yes | Small encrypted recovery value(s) | Circuit binding plus staged audit flow |
| Second master wrapping per tier | Yes | Yes | Candidate 32 bytes per tier with reviewed EPK reuse | Circuit binding and another supported PRE selection |
| Anonymous HIBE | Yes in identified constructions | Can provide genuine delegated child keys | New ciphertext format; no size advantage established | Largest cryptographic/protocol change |

**Do not replace the compliance cryptosystem with HIBE merely to avoid one small field.** HIBE is a real solution to the abstract requirement, but its extra ciphertext structure, proof integration and Orbis changes must beat a concrete small-overhead baseline to justify adoption.

For a strict no-new-field design, evaluate master encryption of the existing address fields first. Its decisive question is whether the changed enforcement of named-person counterparty access is acceptable. Resolve the address-encoding dependency at the same time.

If existing access semantics must remain, compare encrypted derivations against second master wrappings. The latter is the leading simplicity candidate because it preserves the payload, avoids unknown-path recovery entirely, and does not require a second investigation step. Encrypting the derivation remains valid if identifying parties before selecting further tiers is an intentional workflow requirement.

Before choosing, review the exact candidate relation and measure its incremental bytes and proof cost. Required checks include wrong-child access, unrelated-address authorization, mismatched dual wrappings, flagged transactions, partial tier disclosure, malicious PRE requests, and auditor-only decryption. None of the new schemes has been implemented or benchmarked in this research pass. The earlier current-scheme test is evidence of existing behavior, not a benchmark of these alternatives.

**Sources**

1. Pieter Wuille. [BIP32: Hierarchical Deterministic Wallets](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki), 2012, current specification inspected.
2. Dan Boneh, Xavier Boyen, Eu-Jin Goh. [Hierarchical Identity Based Encryption with Constant Size Ciphertext](https://crypto.stanford.edu/~dabo/papers/shibe.pdf), EUROCRYPT 2005, §3. Master-decryption equation above is derived from the published equations.
3. Xavier Boyen, Brent Waters. [Anonymous Hierarchical Identity-Based Encryption (Without Random Oracles)](https://ai.stanford.edu/~xb/crypto06a/anonymoushibe.pdf), CRYPTO 2006, full version, §5 and Appendix B.2. Master-decryption equation above is derived from Setup and Encrypt.
4. Somindu C. Ramanna, Palash Sarkar. [Anonymous Constant-Size Ciphertext HIBE From Asymmetric Pairings](https://eprint.iacr.org/2012/057), 2012, revised 2013. Abstract establishes anonymity and constant size; a Shieldd implementation cost is not established.
5. [hohibe Rust documentation](https://docs.rs/hohibe/latest/hohibe/), inspected September 2026. Describes a BBG implementation, not an audit or an Orbis integration.
6. Dan Boneh, Matthew Franklin. [Identity-Based Encryption from the Weil Pairing](https://crypto.stanford.edu/~dabo/papers/bfibe.pdf), full version, §7, Escrow ElGamal encryption. The escrow equation explicitly requires the recipient public key.
7. Sherman S. M. Chow, Matthew Franklin, Haibin Zhang. [Practical Dual-Receiver Encryption—Soundness, Complete Non-Malleability, and Applications](https://eprint.iacr.org/2013/858), 2013 / CT-RSA 2014. Abstract and stated soundness property inspected.
8. Xuecheng Ma, Xin Wang, Dongdai Lin. [Anonymous Identity-Based Encryption with Identity Recovery](https://arxiv.org/html/1806.05943), 2018, §§1.2 and 4.
9. Cheng-Kang Chu et al. [Key-Aggregate Cryptosystem for Scalable Data Sharing in Cloud Storage](https://repository.sutd.edu.sg/esploro/outputs/journalArticle/Key-Aggregate-Cryptosystem-for-Scalable-Data-Sharing/9911643009846), IEEE TPDS, 2014. Author-institution publication record and abstract.
10. Barnes et al. [RFC 9180: Hybrid Public Key Encryption](https://www.rfc-editor.org/rfc/rfc9180.html), 2022, §§4 and 7.1.
11. Mihir Bellare, Alexandra Boldyreva, Jessica Staddon. [Multi-Recipient Encryption Schemes: Security Notions and Randomness Re-Use](https://cseweb.ucsd.edu/~mihir/papers/bbs.pdf), full version dated 2021; preliminary PKC 2003 paper.

**Local implementation references**

- L1: [Derivation](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/crypto.rs:25) and [registered compliance public key](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/structs.rs:304).
- L2: [Transfer encryption and selected keys](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/transfer.rs:269).
- L3: [Transfer circuit key selection](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/tools/gnark/internal/circuits/transfer_circuit.go:1592).
- L4: [Orbis derivation and PRE](/Users/antoinecyr/Documents/Source/orbis-disclosure-integration/crates/crypto/src/decaf377/pre.rs:567), inspected worktree commit `986d19c`.
- L5: [Separate subject/investigation demo encryptions](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/shielded-pool/src/transfer/compliance.rs:219).
- L6: [Canonical 48-byte address](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/keys/src/address.rs:169), [diversifier hashing](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/keys/src/keys/diversifier.rs:31), and [EXT plaintext selection](/Users/antoinecyr/Documents/Source/shieldd-disclosure-integration/crates/core/component/compliance/src/transfer.rs:366).
