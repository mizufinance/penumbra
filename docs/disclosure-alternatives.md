# Alternatives for Shieldd voluntary disclosure

The first alternative worth trying is a **small, dedicated gnark circuit over BLS12-377 for note facts and amount predicates**. Shieldd already implements the exact Poseidon377 note commitment in that field. A second, simpler path is **ordinary commitment-opening receipts when the amount, asset, and recipient can all be revealed**, accompanied by a normal custody signature when current spending-authority control is requested. Neither requires changing payment consensus or introducing a proving service.[^1][^2][^3]

Hidden-memo metadata should be evaluated separately. Its expensive part is establishing the relationship between the accepted ciphertext and the metadata commitment. A different proof engine does not remove that relationship. TEEs can execute it using ordinary Rust cryptography, but replace proof-system assumptions with hardware, attestation, and deployment trust. They are a conditional option for supervisory access, not a transparent replacement for local cryptographic proofs.

The recommended research direction is therefore to narrow the relation before replacing the prover. Preserve the existing SDK request, transaction-acceptance checks, and local worker model. Benchmark a small commitment circuit first; evaluate metadata as a second relation. No implementation or new prover benchmark is included in this analysis.

## Requirements and measured baseline

Issue #332 covers selected transactions and notes, selective fields, predicates, supporting metadata, and independent verification. Issue #192 adds scoped supervisory access, grants and revocation, and an audit record of disclosure access. These are related interfaces, but different security problems.[^4]

The existing RISC Zero 3.0.6 implementation completed an accepted-payment export in 1,329.05 seconds, with 5.23 GB peak resident memory. Separate receipt verification took 10.93 ms. The serialized receipt was 606,874 bytes; the whole package was 813,157 bytes. This measured workload revealed one committed metadata field while hiding the amount, recipient, memo, and remaining metadata. It was not a real multi-transaction throughput benchmark.[^5]

The following distinctions govern the comparison:

| Claim | What must be established | What does not follow |
| --- | --- | --- |
| Note amount, asset, recipient | Facts open the selected accepted note commitment | Current unspent status, legal identity, or successful delivery of encrypted plaintext |
| Hidden amount predicate | An opening exists with a bounded amount satisfying the predicate | Exact amount or complete account activity |
| Selected-output total | Unique selected commitments open to amounts of the same asset with the claimed total | That no other transactions were omitted |
| Spending-authority control | A fresh request-bound signature verifies under the accepted ordinary Transfer's first input authorization key | Who physically initiated the historical payment |
| Payment-committed metadata | Disclosed fields belong to data committed through the accepted payment | Truth of the assertion or authenticity of an unrelated document |
| Later signed statement | A specified signer authenticated a statement bound to the payment | That the statement existed at payment time |
| Supervisory access | A current grant authorizes access, with the required logging | Erasure of information already received or cryptographic completeness of a user-selected history |

All alternatives still need the chosen-node acceptance check. A valid mathematical proof or hardware attestation can describe an unaccepted transaction. Local verification of evidence must remain separate from confirmation that the canonical transaction and selected public data were accepted on the intended chain.

## Comparison

The rankings below are engineering judgments based on the inspected code and sources, not measured speed rankings.

| Alternative | Main fit | Local ordinary laptop | Main added assumption or cost | Recommendation |
| --- | --- | --- | --- | --- |
| Dedicated gnark Groth16, BLS12-377 | Hidden note facts, predicates, selected totals | Yes | Circuit-specific setup and circuit validation | First proof experiment |
| Native commitment opening plus optional signature | Full amount/asset/recipient disclosure | Yes | Reveals the complete commitment opening | First simple receipt experiment |
| gnark PLONK in the same field | Same arithmetic relation, easier circuit evolution | Yes | Suitable universal SRS; different proving costs | Compare after the small Groth16 relation works |
| Separate metadata certificate and disclosure proof | Repeated disclosure from the same payment | Yes | One-time certificate cost, additional proof composition at the verifier | Second-stage experiment |
| Nitro/SGX/TDX/SEV-SNP attestation | General Rust evaluation; managed supervision | Hardware-dependent; Nitro requires EC2 | Hardware/vendor trust, measured software, operational policy | Conditional, especially for #192 |
| Vega | Repeated selective disclosure over hash-heavy data | Client-side implementation | Preparation, circuit translation, field mismatch | Focused metadata investigation |
| Zcash/Monero-style receipts | Payment evidence and authority semantics | Yes | Their protocols do not implement Shieldd's hidden predicates | Reuse design principles |
| Bulletproofs/Schnorr over a new Pedersen commitment | Range/linear relations on Pedersen values | Yes | Must prove the new commitment matches the accepted Poseidon note | Not the first backend to port |
| zkLedger-style auditing | Complete financial histories | Not a wallet-only replacement | Different ledger structure and continuously maintained evidence | Separate #192 architecture work |
| Another zkVM with a ZK wrapper | Broad reuse of Rust claim logic | Depends on the complete local pipeline | VM/precompile port and final wrapping cost | Fallback investigation |

## Dedicated proofs: strongest immediate candidate

### Why Shieldd is unusually well suited

The note commitment is a domain-separated Poseidon hash of six field elements: note blinding, amount, asset ID, compressed diversified generator, transmission-key field encoding, and recovery commitment. The existing gnark `NoteCommitmentWithCompressedDivGen` calls the matching `Poseidon377Hash6` gadget. The payment circuits already use it, and a vector test compares its output with Shieldd's expected commitment.[^1][^2]

This is native arithmetic over the BLS12-377 scalar field, matching the field used by the existing commitment gadget. There is no reason to begin with BN254 and emulate Shieldd's field. Existing gnark support for BLS12-377 is directly relevant; generic claims that all custom curves require expensive non-native arithmetic would be wrong here.[^2][^6]

A useful static observation illustrates the difference in workload. The current rate-six parameters have width seven, eight full rounds, and 31 partial rounds. That gives 87 applications of the `x^17` S-box. The existing implementation uses five multiplication steps per S-box: **435 steps in the hash core before constant folding**. Constant inputs can reduce the compiled work further. This excludes compiler bookkeeping, range checks, public-input binding, and any additional gadgets. It is neither a compiled constraint count nor a latency measurement, and cannot be divided into zkVM cycles to claim a speedup.[^2]

The relevant inference is structural: proving this small algebraic relation avoids proving millions of general-purpose instructions. A trial is justified without claiming that it will finish in any particular number of milliseconds.

### Minimal relation

The first circuit should establish knowledge of an opening of the selected note commitment, optionally disclose the amount or asset, and enforce a requested comparison or inclusive range. It should use the existing commitment function and constants. A private note blinding can be supplied directly; proving its derivation from `rseed` is unnecessary for a claim explicitly about knowledge of a commitment opening.

That narrower statement matters. It proves the committed facts, not knowledge of the original note seed, possession of spending authority, or consistency of every encrypted payload byte. Those properties should be separate claims when needed. The accepted commitment is bound to the canonical transaction and output by the verifier's node check.

Recipient disclosure can also avoid a private hash-to-curve calculation. If the recipient address is public, the verifier can parse it with Shieldd's existing address implementation and derive the public compressed generator and transmission-key encoding. The circuit equates those with the corresponding opening fields. When the recipient is hidden and no claim is made about its address syntax, the circuit can keep those committed fields private. It must not silently assert proof of a complete address representation if the commitment only establishes the encoded components.[^1]

Spending-authority signatures can be verified outside the circuit because the selected authorization key, signature, and request are not themselves intended to be hidden. That uses the existing custody mechanism and accepted first-input key. Combining a proof and a signature in a package is enough; there is no need to prove signature verification inside another proof merely to package them together.[^3]

The public statement must bind the intended request, not just a commitment. Claim parameters, chain, references, recipient, and challenge need an explicit reviewed binding. An unused public variable, or a field mentioned only in the outer JSON, is not adequate. Tests must establish that context changes cannot be accepted by relabeling evidence. This is a required protocol detail for the experiment, not a reason to import the entire transaction into the circuit.

### Totals and batching

Start with one output to isolate the actual proving cost, then evaluate a small fixed-capacity batch. A batch can cover multiple transactions because each public commitment is independently matched to its transaction; the arithmetic relation does not need to execute those transactions.

Each active amount needs a 128-bit range bound and a non-dummy condition. Duplicate references must be rejected independently of the supplied heights. All selected assets must agree. The sum must preserve the SDK's overflow semantics, rather than relying on finite-field addition: a field equality alone is not an unsigned-integer sum proof. Padding and selectors need explicit constraints if a fixed circuit admits fewer active outputs.

Initially verify several proofs directly when useful. Recursive aggregation and SNARK wrapping are unnecessary before measuring verifier cost and package overhead. They reduce some verification or transport costs; they do not make the original witness computation disappear.

### Setup, size, and maturity

This proposal is a new off-chain disclosure circuit, not a replacement payment circuit. It needs its own pinned verification key. Existing payment keys cannot prove arbitrary new relations. gnark's Groth16 setup documentation explicitly warns that leaked setup randomness can compromise the protocol and points to multiparty setup or alternative schemes. A local setup is sufficient for a disposable benchmark, not a production trust claim.[^7]

A small set of stable circuits limits setup proliferation. Encoding supported comparisons with bounded selectors can avoid generating a key for every threshold. Production ceremony planning should follow evidence that the relation is worth deploying, rather than becoming part of the first benchmark.

Groth16 has a small algebraic proof, but actual package size must be measured. gnark's BLS12-377 serializer includes fields beyond the three core proof points, including its commitment-extension encoding. Quoting a generic “192-byte proof” as the complete exported package would be misleading.[^8]

The inspected repository pins gnark 0.15.0. The prominent 2024 hiding and soundness advisories concerned the Groth16 commitment extension through 0.10.0, with fixes listed from 0.11.0. Those advisories do not establish that current gnark is broken, nor do the fixes audit a new disclosure circuit. Prefer a straightforward relation, constrain all hints, and inspect any commitment or lookup extensions actually introduced.[^9]

### PLONK as the nearby comparison

gnark also supports PLONK on BLS12-377. The existing frontend gadgets can inform both circuits, though the constraint-system builder and setup interface differ. Its universal SRS model can be preferable if disclosure circuits will change frequently. A suitable SRS must match the curve and capacity; a BN254 setup is not interchangeable with BLS12-377.[^6][^10]

The decision is operational as well as numerical: compare first-use key loading, proof generation, verification, memory, and key distribution. There is insufficient Shieldd-specific evidence to declare PLONK faster. Its advantage to investigate is reducing per-circuit ceremony friction while retaining the same native arithmetic.

## Ordinary receipts: remove proving when privacy permits

### Full commitment opening

When a disclosure intentionally reveals amount, asset, and recipient together, the verifier can recompute the accepted commitment from the opening. The package would contain those public facts, the note blinding, recovery commitment, and the necessary canonical address data. No general-purpose proof engine is needed for that equality.

This is a concrete candidate inferred from the code, not an already implemented export mode. `rseed` derives note blinding and the ephemeral encryption secret under different PRF domains. Sharing the blinding therefore does not directly hand over the seed, ephemeral secret, or payload key under the intended PRF assumptions. The experiment must confirm that no convenience serialization includes those secrets and review the disclosure implications of all six opening fields.[^1][^11]

It is broader than selective field disclosure: a normal full opening exposes the amount, asset, and committed recipient components. It cannot prove an amount threshold while hiding the amount. The recovery commitment and blinding are additional disclosed values even though they are not payload keys. The preview must say so.

An opening shows what the note commitment contains. It does not by itself establish that the ciphertext is decryptable by the claimed recipient, that the note remains unspent, or that the presenter controls the spending authority. Add the relevant evidence only when that stronger statement is requested. A request-bound custody signature supplies the existing authority-control claim without a ZK circuit.

### Payload keys remain a separate capability

Explicit payload-key sharing is a different option. It allows native decryption and verification of the selected note, but Shieldd's wrapped memo key also grants access to the transaction-wide memo. Retain that option for parties who want decryption access; do not substitute it for proof-only selective disclosure.[^12]

The practical UI distinction is simple: “reveal this note's facts,” “grant decryption access,” and “prove selected facts privately” are different disclosures. These choices should not be hidden behind interchangeable backend names.

## Metadata: isolate the difficult relation

### What existing hidden memos require

The current metadata format commits to the salted serialized document using SHA-256 and places that commitment in the encrypted transaction memo. The memo uses ChaCha20-Poly1305, with a memo key wrapped for outputs; payload-key derivation uses Blake2b and Decaf key agreement. The memo plaintext is fixed at 512 bytes, while the metadata document may be much larger.[^12][^13]

A hidden-memo proof therefore needs a trustworthy path from the accepted ciphertext to the claimed document commitment. Accepting a prover-supplied memo key and merely checking an AEAD tag is not automatically equivalent to deriving the payment's intended key. AEAD authenticity should not casually be treated as a unique-key commitment; published work establishes this distinction for ChaCha20-Poly1305, among other schemes. This is a reason to validate the proposed shortcut, not a finding that the existing Shieldd disclosure is exploitable. The safe initial relation retains the existing key derivation and wrapped-key binding, unless a separately reviewed argument justifies a narrower statement.[^30]

Dedicated arithmetic circuits may handle that work, but the current repository inspection found reusable Poseidon/Decaf gadgets rather than a complete disclosure-ready Blake2b/ChaCha/Poly1305 and metadata-parsing circuit. This is a materially larger experiment than the note predicate. Putting it in the first circuit would obscure whether the cheap majority of claims is already solved.

### Three useful alternatives

**Reveal the memo when permitted.** If the selected disclosure already supplies enough note data or a payload key to establish the intended memo key, full memo verification can use ordinary computation. An arbitrary memo key alone does not settle the binding concern above. This reveals the whole memo, including its return address, and any additional note data must also be included in the privacy preview. A public metadata commitment still does not allow selected fields to be opened from a single salted whole-document hash without either revealing the document or proving the hash relation. The approach removes the encryption proof, not every metadata proof.

**Certify the memo-to-document link once.** An immutable certificate could prove that a selected accepted payment's intended memo contains document commitment `D`, while keeping the memo and keys private. Later presentations could prove selected fields against `D`. The recipient verifies both pieces and checks the shared commitment and transaction binding; recursion is not intrinsically required.

This is an architectural proposal, not a free optimization. It introduces another proof relation and deliberately reveals a stable document commitment. It may improve repeated presentations, but the first-use certificate cost still counts. Current request-bound proofs cannot simply be reused for new challenges: static payment evidence and fresh authority-control evidence must be separated explicitly, and new predicates still need suitable evidence. Background preparation shifts latency; it does not eliminate computation.

**Use signed selective-disclosure documents for later context.** SD-JWT provides standardized disclosure of salted claim hashes in signed JSON. It is suitable for an issuer's invoice attributes or attestations and can avoid a bespoke metadata format for that class of statement. It does not prove that an amount equals an accepted Shieldd note or that a document was committed at payment time. Signing after the payment must remain labeled as later context.[^14]

For future metadata formats, independently salted field commitments or a document tree could make field openings cheap once a root is authenticated. That changes the metadata commitment design and its privacy properties; it is outside the current single-document-commitment plan. It is worth considering only if document hashing or repeated field presentations is measured as the dominant cost. No new payment-circuit field is inherently required merely to place a root inside the existing memo, but the hidden ciphertext-to-root proof remains.

## TEEs: useful under a different trust model

### What a TEE would actually provide

A trusted execution environment could run the existing pure evaluator with ordinary cryptographic libraries, then attest to a result produced by an approved program. It avoids generating a mathematical proof of every cryptographic operation. Near-native evaluation is the attraction; total latency must include enclave startup, attestation, transport, and policy checks, none of which has been measured for this workload.

The recipient would trust the hardware isolation, vendor attestation root, approved software measurement, configuration, and result-binding protocol. A signature from a protected key alone does not prove that the program validated the claim. The attested code must compute the result itself and bind it to the request and accepted transaction data. This is hardware-backed execution evidence, not zero knowledge in the same cryptographic sense as a SNARK.

For a selected-witness job, a plausible design is to attest an ephemeral encryption/signing key, verify the measurement before sending witnesses, evaluate inside the enclave, and return a signed result tied to the request digest and challenge. The recipient independently performs the node acceptance check. No spending key needs to enter the enclave; custody can provide the ordinary control signature separately. This protocol outline is an analytical proposal requiring its own validation.

### Deployment options

| Platform | Relevant capability | Shieldd implication |
| --- | --- | --- |
| AWS Nitro Enclaves | Isolated EC2 VM, image measurements, signed attestation documents | Practical managed experiment, but remote EC2 execution changes the local-only model |
| Intel SGX | Application enclaves on supported hardware | A local one-shot worker is possible on an appropriate machine, not a portable Mac wallet feature |
| Intel TDX / AMD SEV-SNP | Attested confidential VMs | Reuse more of the software stack, with a larger measured software environment and supported-server requirements |
| Apple Secure Enclave | Protected system services and cryptographic-key operations | Documented APIs do not provide a way to load the Shieldd Rust evaluator as an arbitrary enclave program |

AWS documents Nitro's lack of direct external networking, persistent storage, and interactive access, with communication through the parent instance. Its attestation document can carry public-key, nonce, and application-defined data. Verification relies on the AWS certificate chain and the application protocol, not simply a successful COSE signature check.[^15][^16]

A Nitro prototype could be an ephemeral job rather than a persistent proving server. It still requires a cloud-side execution and transport arrangement. Running it in the user's own account reduces operator dependence but does not remove the AWS root of trust or turn it into local proving.

Intel maintains explicit TCB-recovery and supported-platform guidance. AMD's SEV-SNP memory-aliasing advisory is a concrete example of why firmware and mitigation status matter to attestation policy, not a claim that every current platform is vulnerable. An unqualified “valid quote” must not be treated as timeless evidence from a suitably patched system.[^17][^18]

Apple's Secure Enclave architecture and public key-protection interfaces do not establish arbitrary third-party computation attestation. App authenticity, key protection, and proof that a specific computation ran correctly are different properties. Using a Secure Enclave key to sign a result computed in ordinary process memory would not supply the missing guarantee.[^19]

### Privacy, persistence, and #192

A TEE can hide the selected witness from the surrounding operator under its security assumptions. Side channels, software vulnerabilities, input/output logs, and update policy remain part of the design. Minimize the witness and avoid giving a general supervisory service broad decryption keys merely for convenience. Hardware confidentiality also does not protect against a legitimately authorized recipient redistributing its output.

TEEs are more compelling for #192 because a supervised service can enforce a current grant, constrain a query, and require an audit-log acknowledgment before releasing a result. Revocation can stop future mediated access. It cannot erase prior plaintext or invalidate knowledge already learned from a valid disclosure. Nor does a TEE prove history completeness when its only input is a user-selected subset.

An audited enclave service would need authoritative scope data, freshness and rollback defenses, trustworthy input acquisition, and a failure policy that withholds output when required logging fails. Those are additional service responsibilities; they are not solved by reusing #332's local proof worker. The right decision is to consider TEE supervision separately once the intended trust in operators and hardware vendors is explicit.

## Zcash, Monero, and zkLedger

### Zcash

ZIP 311 is a useful receipt design: it binds transaction references, selected outputs, a message/challenge, and spending authority. It remains a draft with a reference implementation marked TBD and Orchard support left open. Its Sapling output mechanism discloses outgoing cipher keys; it is not an implementation of hidden amount predicates or private memo-field extraction.[^20]

The reusable lesson for Shieldd is to separate payment facts, authority control, and accepted status. Copying ZIP 311's proof or key format would not interoperate with Shieldd. Reusing an existing payment proof also cannot generally reveal a new predicate over its hidden witness; a new relation or deliberately linked commitment is needed.

### Monero

Monero implements transaction proofs, spend proofs, and reserve proofs with distinct meanings. The current wallet RPC reports transaction-proof validity, received amount, pool status, and confirmations; reserve verification reports total and spent amounts. Its documentation warns that transaction evidence alone does not guarantee spendability.[^21]

These are good precedents for small, purpose-specific disclosures. They do not make a generic range proof attach to a Shieldd Poseidon commitment. A new Pedersen commitment to an amount needs a proof that it contains the same amount as the accepted note. Without that bridge, a prover can demonstrate a true range statement about an unrelated value.

### zkLedger

zkLedger's fast audits rely on homomorphic commitments, audit tokens, a columnar ledger with an entry for each participant, and maintained caches. The paper reports audit responses below 10 ms over 100,000 transactions under that architecture. Its completeness comes from covering the participant's ledger column, not from trusting a submitted selection.[^22]

For Shieldd, importing that approach would be a ledger/audit-data redesign. An off-chain index built solely from voluntarily supplied transactions cannot establish that no transaction was omitted. The useful lesson for #192 is to define the authoritative scope and maintain authenticated aggregates if complete-history auditing becomes a requirement. It is not an immediate fix for the current single-payment proof time.

## Other proof systems and credential designs

**Vega is worth a narrow metadata investigation.** Microsoft's current repository reports 92 ms online proving, 23 ms verification, and a 108 KB proof for a roughly 2 KB credential workload without trusted setup. Its lifecycle separately includes preparation, reusable for repeated presentations over the same data. These numbers are not first-use Shieldd estimates.[^23][^24]

The inspected providers expose Pallas, Vesta, P256, T256, and BN254 engines, rather than a ready BLS12-377 scalar-field engine. Reusing Shieldd's Poseidon arithmetic would therefore require field emulation or additional provider work. A hash-heavy metadata relation is a more plausible first investigation than replacing the already-native gnark note gadget. The old `microsoft/Spartan2` URL now redirects to Vega; it should not be counted as an independent current option.[^25]

**Bulletproofs are attractive only after the commitment relationship is solved.** They avoid trusted setup and suit range proofs over their expected commitments. Dalek's implementation uses Ristretto and labels its general R1CS interface experimental. Porting it, proving Poseidon preimages in a different field, or adding a linking proof is more work than trying the existing gnark gadget. A Bulletproof over a freshly invented amount commitment is insufficient evidence about the payment.[^26]

**BBS credentials suit externally signed attestations.** The scheme supports selective disclosure of signed messages and unlinkable signature proofs. Public transaction IDs can still link presentations. An invoice issuer's BBS signature does not prove an invoice was paid, and basic selective disclosure is not an arbitrary numeric predicate engine. The inspected specification remains an Internet-Draft, so it is a candidate for the attestation layer rather than a substitute for note proofs.[^27]

**SP1 with its final ZK wrapper is a valid comparison to investigate if Rust-program reuse remains decisive.** The current security model says individual STARK proofs are not zero knowledge; its Groth16 and PLONK wrapped modes provide the documented ZK path. A fair trial must include local proving and final wrapping, compatible Decaf arithmetic acceleration, and the complete private-witness handling path. Comparing only compressed STARK timings would not answer this question.[^28]

A Groth16 wrapper around the current slow RISC Zero computation is a different proposal from a dedicated Groth16 note circuit. Wrapping may change receipt size or privacy assurance but still requires producing the inner proof. Likewise, a new compiler, binary-field prover, or general-purpose framework should not be prioritized merely because a different hash benchmark is fast. The relation and native-field fit need to justify the port.

RISC Zero remains a measured reference implementation, with the existing qualification that its official security model describes an incomplete mathematical zero-knowledge argument. Successful application tests do not settle that property.[^29]

## Experiments worth authorizing next

These are bounded experiments to decide architecture, not a plan to implement every backend.

| Priority | Experiment | Evidence needed | Reason to continue or stop |
| --- | --- | --- | --- |
| 1 | One-note gnark Groth16 opening/predicate circuit | Real accepted note, existing gadget, real proof and independent verification; cold/warm time, RSS, serialized proof and keys | Continue if it materially removes the VM cost with a small auditable relation |
| 1 | Native full-opening receipt plus optional custody signature | Recompute accepted commitment; confirm no seed/key leakage; document exactly what is revealed | Retain if it covers common full-fact receipts without unacceptable disclosure |
| 2 | Small fixed batch of note predicates/totals | Cross-transaction references, duplicate rejection, integer bounds, scaling and key loading | Continue if proof cost scales usefully; avoid recursion until necessary |
| 2 | Same note relation with gnark PLONK | Equivalent statements and privacy, appropriate SRS, full cost comparison | Choose based on proving cost and setup operations rather than generic backend reputation |
| 3 | Metadata decomposition | Separately measure intended-key/ciphertext validation and document-field proof; account for first-use preparation | Continue if reuse or narrower disclosure eliminates substantial repeated work |
| Conditional | One-shot attested evaluator | Real quote, pinned measurement, witness-channel binding, changed-result rejection, startup and warm latency | Only if hardware/cloud trust and deployment fit are acceptable |
| Conditional | Vega metadata relation | First prove the actual SHA-256/field relation can be represented; count preparation and repeated presentations separately | Stop if field/provider adaptation dominates the likely benefit |

Use the same accepted transaction fixtures and privacy contract for comparable claims. Measure one output and representative batches, distinguishing multiple outputs from one memo from outputs across different memos. Keep request-specific signing, node RPC time, proof generation, and key loading separately visible. Existing resource bounds should apply to any later benchmark.

Every proof experiment needs adversarial cases for wrong commitments, references, chain, claim parameters, challenge, mixed assets, duplicate outputs, amount overflow, and malformed evidence. Metadata experiments additionally need changed salt, fields, memo ciphertext, wrapped key, and key-substitution cases. The goal is evidence that a simpler relation is both correct and useful; another negligible micro-optimization is not a reason to extend the exercise.

The recommended next implementation, if selected, is **the dedicated note circuit and the ordinary opening receipt**, leaving hidden-memo metadata as a separately measured capability. For #192, retain the reusable verification interfaces and investigate mediated access policy independently. That gives each claim an appropriate mechanism without committing the product to a second general-purpose prover or a cloud service prematurely.

## Evidence limits

Repository-specific analysis is pinned to Shieldd commit `2f9cda0e885d0af33fc876ead2c83a18a5d35b46`; the Vega provider inspection is pinned to `c0ee259053cd12eaf43ed71b5cde375452b3ee4d`. External documents were checked in September 2026. The only Shieldd proof timings quoted are from the earlier completed RISC Zero experiment. This pass performed source inspection, literature comparison, and a static arithmetic count; it ran no builds, circuits, TEE deployments, or new proofs. Proposed latencies and security properties require implementation-specific validation.

## Sources

[^1]: Shieldd, [note commitment and address-based opening](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/core/component/shielded-pool/src/note.rs), `commitment` and `commitment_from_address`.
[^2]: Shieldd, [shared gnark note gadget](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/tools/gnark/internal/circuits/shared_core.go), [Poseidon implementation](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/tools/gnark/internal/primitives/poseidon377.go), [parameters](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/tools/gnark/internal/primitives/vectors/phase05_vectors.json), and [commitment vector test](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/tools/gnark/internal/primitives/crypto_primitives_test.go).
[^3]: Shieldd, [disclosure claims](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/disclosure/src/claims.rs) and [accepted-output extraction](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/disclosure/src/transaction.rs).
[^4]: Mizu Finance, [Bankd #332: Enhance voluntary disclosure](https://github.com/mizufinance/bankd/issues/332) and [#192: x/disclosure (selective supervisory access)](https://github.com/mizufinance/bankd/issues/192).
[^5]: Shieldd, [completed RISC Zero measurements and validation](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/docs/disclosure-research.md), September 2026.
[^6]: Consensys, [gnark supported proof systems and curves](https://github.com/Consensys/gnark), and [schemes and curves documentation](https://docs.gnark.consensys.io/Concepts/schemes_curves). Repository version inspected: 0.15.0.
[^7]: Consensys, [gnark 0.15.0 Groth16 API and setup warning](https://github.com/Consensys/gnark/blob/v0.15.0/backend/groth16/groth16.go).
[^8]: Consensys, [gnark 0.15.0 BLS12-377 proof serialization](https://github.com/Consensys/gnark/blob/v0.15.0/backend/groth16/bls12-377/marshal.go).
[^9]: Consensys, [GHSA-9xcg-3q8v-7fq6: commitment hiding](https://github.com/Consensys/gnark/security/advisories/GHSA-9xcg-3q8v-7fq6) and [GHSA-q3hw-3gm4-w5cr: multiple-commitment soundness](https://github.com/Consensys/gnark/security/advisories/GHSA-q3hw-3gm4-w5cr), September 6, 2024; affected and patched versions checked.
[^10]: Consensys, [gnark 0.15.0 PLONK API](https://github.com/Consensys/gnark/blob/v0.15.0/backend/plonk/plonk.go).
[^11]: Shieldd, [domain-separated note-seed derivations](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/core/component/shielded-pool/src/rseed.rs) and [keyed Blake2b PRF](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/core/keys/src/prf.rs).
[^12]: Shieldd, [payload and memo-key encryption](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/core/keys/src/symmetric.rs) and [memo format and decryption](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/core/transaction/src/memo.rs).
[^13]: Shieldd, [metadata commitment and hidden-memo relation](https://github.com/mizufinance/shieldd/blob/2f9cda0e885d0af33fc876ead2c83a18a5d35b46/crates/disclosure/src/claims.rs).
[^14]: D. Fett, K. Yasuda, B. Campbell, [RFC 9901: Selective Disclosure for JSON Web Tokens](https://www.rfc-editor.org/rfc/rfc9901.pdf), IETF Standards Track, November 2025.
[^15]: AWS, [What is Nitro Enclaves?](https://docs.aws.amazon.com/enclaves/latest/user/nitro-enclave.html), deployment and isolation model.
[^16]: AWS, [Verifying the root of trust](https://docs.aws.amazon.com/enclaves/latest/user/verify-root.html) and [Cryptographic attestation](https://docs.aws.amazon.com/enclaves/latest/user/set-up-attestation.html), attestation fields, certificates, and measurements.
[^17]: Intel, [Trusted Computing Base Recovery Attestation](https://www.intel.com/content/www/us/en/developer/topic-technology/software-security-guidance/trusted-computing-base-recovery-attestation.html), supported platforms and TCB guidance.
[^18]: AMD, [AMD-SB-3015: Undermining Integrity Features of SEV-SNP with Memory Aliasing](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-3015.html), mitigation and attestation considerations.
[^19]: Apple, [The Secure Enclave](https://support.apple.com/guide/security/the-secure-enclave-sec59b0b31ff/web) and [Protecting keys with the Secure Enclave](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave), architecture and public API scope.
[^20]: Jack Grigg / Zcash, [ZIP 311: Zcash Payment Disclosures](https://zips.z.cash/zip-0311), current draft and reference-implementation status.
[^21]: Monero Project, [Wallet RPC documentation](https://docs.getmonero.org/rpc-library/wallet-rpc/), `check_tx_proof`, `get_spend_proof`, `get_reserve_proof`, and `check_reserve_proof`. This is the current destination of the deprecated getmonero.org RPC page.
[^22]: Neha Narula, Willy Vasquez, Madars Virza, [zkLedger: Privacy-Preserving Auditing for Distributed Ledgers](https://www.usenix.org/system/files/conference/nsdi18/nsdi18-narula.pdf), NSDI 2018, especially architecture and completeness in sections 1 and 3; [publication page](https://www.usenix.org/conference/nsdi18/presentation/narula).
[^23]: Microsoft, [Vega prover](https://github.com/microsoft/vega-prover), published workload and performance claims; Darya Kaviani and Srinath Setty, [Vega: Low-Latency Zero-Knowledge Proofs over Existing Credentials](https://eprint.iacr.org/2025/2094), IEEE S&P 2026.
[^24]: Microsoft, [Vega proving lifecycle](https://microsoft.github.io/vega-prover/overview/lifecycle.html), setup, preparation, online proving, and verification.
[^25]: Microsoft, [Vega provider implementations](https://github.com/microsoft/vega-prover/blob/c0ee259053cd12eaf43ed71b5cde375452b3ee4d/src/provider/mod.rs), inspected revision; [Spartan2 redirect](https://github.com/microsoft/Spartan2).
[^26]: Dalek Cryptography, [Bulletproofs implementation](https://github.com/dalek-cryptography/bulletproofs), supported group and experimental R1CS status.
[^27]: T. Looker et al., [The BBS Signature Scheme](https://datatracker.ietf.org/doc/draft-irtf-cfrg-bbs-signatures/), Internet-Draft, current tracker and January 2026 revision inspected.
[^28]: Succinct, [SP1 Security Model](https://docs.succinct.xyz/docs/sp1/security/security-model), individual STARK privacy and final Groth16/PLONK modes.
[^29]: RISC Zero, [Cryptographic Security Model](https://dev.risczero.com/api/security-model), version 3.0, zero-knowledge qualification.
[^30]: Julia Len, Paul Grubbs, Thomas Ristenpart, [Partitioning Oracle Attacks](https://www.usenix.org/conference/usenixsecurity21/presentation/len), USENIX Security 2021; Ange Albertini et al., [How to Abuse and Fix Authenticated Encryption Without Key Commitment](https://www.usenix.org/conference/usenixsecurity22/presentation/albertini), USENIX Security 2022.
