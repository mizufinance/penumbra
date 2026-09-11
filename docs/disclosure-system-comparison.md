# Disclosure systems and a minimal Shieldd design

The recommended next experiment is ordinary commitment-opening receipts plus one dedicated gnark circuit over BLS12-377 for selectively revealed note fields, amount predicates, and bounded totals across selected transactions. Keep payment acceptance checks, public signatures, and ordinary metadata handling outside that circuit. This is a recommendation, not a measured performance result.

Midnight, Prividium, Canton, and the other systems below solve different combinations of privacy, evidence, and access control. Their designs do not establish that a general encrypted-memo proof is necessary for voluntary payment disclosure. The strongest transferable ideas are explicit disclosure boundaries, narrow payment receipts, authenticated metadata openings, and separately governed access to records.

The comparison uses documentation available on September 10, 2026. Ethsystems source references are pinned to commit `92ce83852d4a49a0ba9cf67c7f66adc686423e32`. Product documentation describes supported architecture; it is not an independent deployment audit or benchmark. For prior prover measurements and alternative backend analysis, see [disclosure-alternatives.md](disclosure-alternatives.md).

## Comparison

| System | Privacy and disclosure mechanism | Who must be trusted with private data or policy? | Useful lesson for Shieldd |
| --- | --- | --- | --- |
| Ethsystems shielded pool | Encrypted notes, spending/viewing key separation; parent deposit circuit checks KYC membership | Viewing-key recipients; attesters determine eligibility | Separate read access from spending; do not equate a broad viewing key with per-field disclosure |
| Midnight | Private witnesses and application-specific ZK circuits; explicit disclosure annotations | Private-state holder and any prover given witnesses; credential issuers for external facts | Prove the requested predicate and explicitly define every revealed value |
| ZKsync Prividium | Private validium with operator-held state and authenticated, permissioned RPC access | Operator has full visibility; administrators govern access and operator controls data availability | Useful model for a supervised query service, not operator-blind wallet receipts |
| Canton/Daml | Transaction views distributed to entitled parties; contract observers and explicit off-ledger disclosure | Participants hosting the relevant parties and application authorization | Scope access to records and workflows; distinguish disclosure from spending authorization |
| Zama | FHE computation, contract ACLs, threshold decryption through a gateway | Threshold KMS and execution infrastructure; application controls access | A reference for governed decryption and access records, not a small replacement disclosure prover |
| RAILGUN | Viewing keys plus separate Private Proofs of Innocence infrastructure | Key recipients for viewing; list-provider classifications for screening | Payment evidence and source-screening evidence are different artifacts |
| Privacy Pools | Withdrawal proves membership in an approved association set | ASP determines which deposits qualify | Express screening as membership in a named, versioned set |
| Zcash ZIP 311 | Draft selected-output disclosure and spending-authority evidence | Disclosed key recipients; normal chain validation | Precise payment/control semantics and challenge binding |
| Monero | Distinct transaction, spend, and reserve proofs | Normal chain validation; verifier interprets the precise proof type | Keep payment, control, and unspent reserves separate |
| zkLedger | Ledger-wide commitments, audit tokens, and maintained aggregate evidence | Protocol-specific ledger and participant structure | Completeness requires dedicated evidence structure |
| SD-JWT | Issuer-signed salted field hashes, selected openings, optional holder binding | Issuer attests facts | Metadata field disclosure often needs hashes and signatures, not a SNARK |

Sources for each row and its limitations are detailed below.

## Ethsystems

The parent specification uses a Poseidon commitment to token, amount, owner key, and salt. Its audit mechanism grants viewing keys; the specification explicitly describes those keys as exposing the associated transaction history. Its circuit inventory contains deposit, transfer, and withdrawal circuits, rather than a dedicated post-payment selective-field disclosure circuit. The specification's KYC check gates deposits; it explicitly says revocation does not freeze existing in-pool notes.[^1]

The linked extension adds PIR for wallet tree reads and epoch nullifiers with recursive proofs. Its README identifies a research PoC and states that its deposit circuit omits KYC enforcement. These features address read privacy and state growth, rather than making disclosure cheaper.[^2]

There is also a useful implementation detail: the current encryption adapter uses `sealring`, with recipient binding in its KDF and a key-commitment tag. This is more specific than merely using ChaCha20-Poly1305. It should inform any future ciphertext-opening review, but adopting it would change Shieldd encryption and is unnecessary for the first note-opening experiment.[^3]

The general requirements seek selective audit access without exposing uninvolved parties. That aspiration is broader than the concrete viewing-key mechanism. Do not treat the PoC as evidence that all its institutional requirements are already enforced.[^4]

## Midnight

Midnight is closest conceptually to private predicates. Compact distinguishes private witness data from public outputs and requires explicit acknowledgement before potentially private values reach disclosure points. Even a comparison result is a disclosure. `disclose(x)` is a compiler annotation: it permits a value to be exposed; it neither encrypts that value to an auditor nor creates a scoped grant. Writing the value to public ledger state exposes it publicly.[^5]

The application chooses the claims, authenticates their inputs, and defines policy. Midnight's security documentation explicitly warns that witness implementations are outside the circuit and cannot be trusted without validation. A supplied balance therefore needs to be checked against authenticated state or a commitment; proving a predicate on an arbitrary user-supplied balance establishes little.[^6]

For Shieldd, the transferable design is a small circuit checking an accepted commitment and revealing only the requested fields or predicate. There is no need to port Compact or deploy another blockchain to obtain that property. A compliance credential also needs issuer authentication, subject binding, and policy freshness; the word “ZK” alone supplies none of those.

The reviewed sources document language and application primitives, not a universal ready-made payment-disclosure package with complete-history guarantees. No comparative proving-time claim is justified from these pages.

## ZKsync Prividium

Prividium operates a private ZKsync validium inside institutional infrastructure. State and transaction data stay off-chain; Ethereum receives state roots and validity proofs. Authenticated requests pass through a proxy and permissioning API, with contract/function rules and roles configured by administrators.[^7]

The crucial trust boundary is explicit in ZKsync's own documentation: the chain operator has full visibility and controls data availability. Proofs constrain state-transition correctness, while confidentiality against unauthorized users depends on the private infrastructure and access rules. Withholding data remains a different risk from forging a valid state transition.[^8]

Its deployment guidance places the sequencer, prover, and complete state database in private network tiers.[^9] Thus a fast auditor query can simply be an authorized database-backed read. That cost is not comparable to a wallet producing a portable ZK statement from data hidden from the operator.

Prividium is a useful reference for #192's authentication, roles, and supervised query workflow. Copying its trust model into #332 would substantially change the product: Bankd operators would become readers of the underlying private data. That is unnecessary for voluntary receipts. Its commercial licensing is another consideration if evaluating adoption, rather than borrowing design ideas.[^7]

## Canton, Zama, and policy proofs

Canton's privacy model distributes relevant transaction views to stakeholders and other entitled parties. Visibility follows the contract model and transaction consequences, so application composition affects what counterparties learn.[^10] Explicit contract disclosure permits off-ledger sharing with non-stakeholders, allowing the disclosed contract to be used during submission without adding every recipient as a permanent observer.[^11]

This suggests separating a portable record from ongoing observer access. It also cautions against automatically revealing the list of all disclosure recipients to each recipient. Canton contract disclosure is not a generic ZK proof of a hidden payment amount.

Zama uses encrypted computation and contract-defined decryption permissions. Its documented architecture combines coprocessors, a gateway, and threshold KMS; the litepaper also describes Nitro enclaves for KMS execution. Access requests are part of the mediated system, making it relevant to auditable release workflows. It is a much larger infrastructure choice than a local receipt, and neither FHE nor an ACL changes the fact that recipients can retain plaintext once released.[^12]

RAILGUN distinguishes viewing access from Private Proofs of Innocence. PPOI propagates evidence connecting funds to deposits outside selected bad-activity lists through separate infrastructure. Its documentation places that machinery alongside the underlying spending contracts.[^13] Privacy Pools instead proves a withdrawal's association with a set approved by an ASP, with public recovery available through ragequit when approval is unavailable.[^14]

Both are useful patterns for later source-of-funds screening. Such a proof means eligibility under a specified dataset and policy; it does not establish that funds are universally lawful or that a legal identity is known. Neither is needed to prove the amount of a selected Shieldd note.

Aztec provides another client-side proof and key-separation reference. Its detailed current key documentation marks outgoing viewing keys as reserved, whereas some overview pages describe them as operational. The detailed reference should govern expectations; this comparison does not assume a complete outgoing audit facility.[^15]

## Payment evidence and standards

There is no single general disclosure standard requiring every field below in every receipt. Three distinct requirements are often conflated: proving payment facts, exchanging identity information where a regulated workflow requires it, and demonstrating an institution's ongoing compliance controls.

Zcash ZIP 311 is a draft with unresolved Orchard and encoding work. It nevertheless provides useful semantics: selected output disclosure, challenge-bound spending-authority evidence, and chain inclusion checked by the caller.[^16] Monero's documented transaction proof, spend proof, and reserve proof are distinct APIs; a transaction proof does not establish that its outputs remain spendable.[^17]

SD-JWT is a concrete standards-track example of selective metadata disclosure using salted hashes and an issuer signature. It supports holder binding with an audience and nonce. It does not supply private arithmetic or prove that an assertion is true.[^18]

Identity and payment-message requirements depend on jurisdiction, role, and transfer type. FATF's virtual-asset guidance explicitly allows required information to be exchanged separately from the transfer itself. Its Recommendation 16 guidance was still undergoing further consultation in June 2026. This is a reason to support separately authenticated business records, not to hard-code a purported universal legal schema into a note circuit.[^19]

## Capability inventory

“Core” below means recommended for #332, not legally mandatory in every disclosure. Capabilities are available on request; they should not all be disclosed by default.

| Capability | Priority | Proposed evidence and meaning |
| --- | --- | --- |
| Chain, canonical transaction, action and output reference | Core | Every item identifies exactly which accepted commitment it describes |
| Acceptance, block/height, verification source | Core | Chosen-node result separate from cryptographic verification; missing data remains unconfirmed |
| Amount in raw units and canonical asset ID | Core | Full opening or selective proof; ticker/decimals need independently trusted asset information |
| Recipient | Core | Prove committed address components; distinguish address ownership from legal identity |
| Threshold and inclusive range | Core | Private bounded amount satisfies the disclosed parameters |
| Exact selected-output total and total predicates | Core | Unique outputs, same asset, checked arithmetic; allow selections across transactions |
| Sender spending-authority control | Core capability, requested when relevant | Fresh custody signature under the accepted ordinary Transfer's mandatory real input key |
| Request audience and challenge | Core | Bind proof/authorization to the request; does not prevent forwarding revealed facts |
| Explicit field/key preview and evidence classification | Core | Users see all exposed fields and decryption capabilities before export |
| Importable, independently verifiable receipt | Core | Typed package with supported version and pinned circuit identity; no spendable-note import |
| Full memo or payload-key access | Optional disclosure mode | Explicit decryption access, including transaction-wide memo consequences |
| Purpose, invoice reference, document digest | Core attachment support; optional content | Mark unsigned context, signed statement, or payment-committed evidence distinctly |
| Signed counterparty acknowledgement | Useful addition | Evidence a specified party acknowledged receipt; separate from an accepted output |
| Selected metadata fields with hidden remainder | Useful addition | Independently salted field commitments or a separate metadata proof |
| Refund/reversal linkage, fees, accounting tags, tax export | Useful addition | Canonical related references and clearly attributed business context |
| Legal identity, institution/region, KYC credentials | Policy-dependent | Issuer-authenticated credentials with subject linkage, validity and revocation semantics |
| Screening / source-of-funds eligibility | Separate compliance feature | Named provider, list/root, policy version and evaluation time |
| Current unspent reserves or balance | Separate proof feature | Spending control plus current ledger evidence; a historical opening is insufficient |
| Complete account activity, liabilities or solvency | Separate architecture | Completeness and coverage evidence, including off-ledger limitations |
| Federal/regional/bank scope, grants, expiry and revocation | #192 | Authorized query/release mechanism with explicit enforcement assumptions |
| Audit of disclosure access | #192 | Record mediated releases; cannot observe every later read of exported plaintext |

Proof packages that expose canonical transaction references are linkable through those references. Private predicates minimize values disclosed, but do not make two receipts for the same public transaction unlinkable. Repeated threshold questions can also reveal an amount incrementally; previews and authorization should treat each result as information disclosed.

Shieldd already has regulated-asset policy membership, user lifecycle checks, proof-bound compliance ciphertexts, and daily undisclosed-volume logic. These should not be rebuilt as part of voluntary disclosure. Its enforcement documentation also explicitly distinguishes implemented Shieldd verification from unimplemented production ACP/Orbis and Bankd integration boundaries.[^20]

## Metadata openings

Metadata is useful for matching a payment to an invoice, identifying document bytes, or attaching an issuer's assertion. It is not necessary for proving the payment's amount, asset, or recipient. The current #332 scope asks for supporting metadata, but does not require all metadata to remain hidden inside a universal claim program.[^21]

The present implementation hashes a structured document and salt under a domain separator and puts the resulting commitment inside the encrypted memo. A full opening is straightforward: reveal the exact document and salt and recompute that hash. Ordinary native cryptography is sufficient when the verifier can authenticate the intended memo and recover its commitment.[^22]

There are two independent problems:

1. **Document membership:** Does the disclosed data open the commitment? A whole-document hash requires the whole document and salt. To reveal fields independently without ZK, commit each field separately, with fresh salt and an unambiguous encoding of the field name, value, and output association. A short authenticated list of field digests is enough; a Merkle tree is optional for compact proofs over large documents.
2. **Payment binding:** Was that commitment part of this accepted payment? A commitment copied into an export does not answer this. Because the current commitment is inside ciphertext, the verifier needs authenticated decryption or a proof of the ciphertext-to-commitment relationship.

A future authenticated public metadata root would make selective hash openings cheap, but choosing where to anchor it is a separate transaction-format design decision. No suitable new public field should be assumed. Storing per-field digests in the existing encrypted memo still leaves the decryption relationship to establish.

An alternative is a later signature over the payment references and metadata. This is simple and useful, provided it is labelled as a later signed statement. It does not retroactively establish payment-time commitment. Similarly, an invoice digest proves document identity, not delivery of goods, contractual performance, or the truth of its contents.

Do not shortcut authentication by accepting an arbitrary memo key solely because an AEAD tag verifies. ChaCha20-Poly1305 is not intrinsically key-committing; intended-key binding needs its own analysis.[^23] The existing explicit payload-key path may be reused with its validation and privacy preview, rather than inventing a weaker opening format.

Recommendation: retain simple attachments, optional signatures, and full committed-document opening where the user accepts the existing decryption scope. Defer selected metadata inside a hidden memo from the first dedicated circuit. If that capability is later essential, measure its separate relation or redesign its authenticated commitment placement.

## One Groth16 circuit or PLONK

One fixed circuit can cover all the core private note claims. Public selectors choose which fields to reveal and which amount comparisons to evaluate. Bounds, asset choices, and requested values are inputs, not reasons to generate new keys. Slots support up to a fixed number of selected outputs from any number of transactions within that capacity. Active-slot constraints, padding rules, integer ranges, and aggregate arithmetic must all be enforced.

Outside the circuit, the verifier maps each public commitment to an accepted canonical output, rejects duplicate selections, checks chain/reference bindings, and verifies any public custody signature. Inside it, the circuit checks the exact existing note commitment, selected fields, and arithmetic. This avoids proving transaction parsing, RPC behavior, and public signature verification privately when no witness needs hiding.

The circuit must also bind the request context through a constrained construction, and the verifier must reconstruct the exact public statement. Merely adding an unused public input named `challenge` is insufficient. Recipient matching must use canonical address derivation, with validity requirements reviewed against Shieldd's accepted-note relation.

| Question | Groth16 | gnark PLONK with KZG |
| --- | --- | --- |
| Different thresholds or reveal masks? | Same circuit and keys | Same circuit and keys |
| New circuit shape or larger capacity? | New circuit-specific setup artifacts | New per-circuit keys derived from a sufficiently large SRS |
| Trusted setup management | Circuit-specific setup; production needs a justified ceremony | Reusable universal SRS within its curve and capacity |
| One verification key for arbitrary circuits? | No | No |
| Existing Shieldd field/gadget reuse? | Yes, BLS12-377 | Same field supported; validate the PLONK compilation |
| Which is faster here? | Unmeasured | Unmeasured |

gnark's pinned backends expose both schemes and their distinct setup paths.[^24] PLONK reduces repeated ceremony work, not the need to distribute and pin circuit-specific proving/verification keys. An Ethereum BN254 SRS cannot simply be reused for BLS12-377. Groth16 should remain the first experiment because the existing native-field gadget is available and the initial relation is narrow.[^25]

Bounded batching needs an explicit limit. A package can contain multiple proofs, but splitting a hidden grand-total claim into chunks requires either revealing chunk totals or an additional mechanism linking hidden partial sums. Do not promise unlimited private totals with one fixed circuit. Begin with one modest capacity, measure partially filled and full batches, and decide whether padding cost warrants another size or PLONK. Recursion is unnecessary for the first experiment.

## Recommended next experiments

1. Implement and measure full commitment-opening receipts for explicitly selected outputs across multiple accepted transactions. Reveal the note blinding and committed fields, never the note seed by default. Recompute the existing commitment and reuse acceptance checks. Add the optional fresh spending-authority signature outside ZK.
2. Build one bounded gnark BLS12-377 disclosure circuit using the existing note-commitment gadget. Cover masks, comparisons, ranges, and selected-output totals. Measure one output, partially filled batches, and full batches. Report cold/warm time, memory, key size, proof/package size, and verification time without promising a latency beforehand.
3. Retain simple metadata attachments and correctly labelled full openings. Keep encrypted-memo processing out of the initial circuit. Test proof outputs for accidental seeds, keys, hidden fields, and unrelated metadata.
4. Compare PLONK on the same relation if circuit evolution or batch sizing makes setup management a material burden. Do not evaluate it using a different field or a different claim and attribute all differences to the proving system.
5. Reuse the receipt SDK for #192 later. Borrow Prividium's role/query concepts and Canton's record scoping, while preserving Shieldd's intended confidentiality boundary. A managed release system must authorize and durably record release before returning sensitive data. Threshold release or a TEE may enforce that boundary; neither is required for wallet receipt creation.

Complete-history auditing remains separate. zkLedger obtains it through purpose-built ledger commitments and maintained audit evidence, not by summing a wallet-selected list.[^26] No application code, prover benchmark, or release-gated test was run as part of this comparison.

## Sources

[^1]: Ethsystems, [parent shielded-pool specification](https://github.com/ethsystems/pocs/blob/92ce83852d4a49a0ba9cf67c7f66adc686423e32/pocs/private-payment/shielded-pool/SPEC.md), draft v0.1.0; key derivation, revocation, and circuits.
[^2]: Ethsystems, [extension README](https://github.com/ethsystems/pocs/blob/92ce83852d4a49a0ba9cf67c7f66adc686423e32/pocs/private-payment/shielded-pool-extension/README.md), status and implementation shortcuts.
[^3]: Ethsystems, [note encryption adapter](https://github.com/ethsystems/pocs/blob/92ce83852d4a49a0ba9cf67c7f66adc686423e32/pocs/private-payment/shielded-pool/src/lib/crypto/encryption.rs).
[^4]: Ethsystems, [private-payment requirements](https://github.com/ethsystems/pocs/blob/92ce83852d4a49a0ba9cf67c7f66adc686423e32/pocs/private-payment/REQUIREMENTS.md).
[^5]: Midnight, [Explicit disclosure in Compact](https://docs.midnight.network/compact/reference/explicit-disclosure), current documentation.
[^6]: Midnight, [Smart contract security](https://docs.midnight.network/compact/smart-contract-security), witness validation and commitment primitives.
[^7]: ZKsync, [Prividium architecture](https://docs.zksync.io/zk-stack/prividium/architecture) and [overview](https://docs.zksync.io/zk-stack/prividium/overview).
[^8]: ZKsync, [ZKsync Chains](https://docs.zksync.io/zk-stack/zk-chains), controlled data availability and operator visibility.
[^9]: ZKsync, [Prividium deployment model](https://docs.zksync.io/zk-stack/prividium/deployment).
[^10]: Digital Asset, [Compose choices](https://docs.digitalasset.com/build/3.4/tutorials/smart-contracts/compose.html), privacy and divulgence.
[^11]: Digital Asset, [Explicit Contract Disclosure](https://docs.digitalasset.com/build/3.4/sdlc-howtos/applications/develop/explicit-contract-disclosure.html).
[^12]: Zama, [Confidential Blockchain Protocol Litepaper](https://docs.zama.org/protocol/zama-protocol-litepaper), ACL, threshold decryption, and KMS architecture; product claims and roadmap are distinguished from independent validation.
[^13]: RAILGUN, [Private Proofs of Innocence](https://docs.railgun.org/wiki/assurance/private-proofs-of-innocence) and [Assurance Suite](https://docs.railgun.org/wiki/assurance/railgun-assurance-suite).
[^14]: Privacy Pools, [What is Privacy Pools?](https://docs.privacypools.com/), ASP architecture and ragequit.
[^15]: Aztec, [Keys](https://docs.aztec.network/developers/docs/foundational-topics/accounts/keys); compare [wallet overview](https://docs.aztec.network/participate/basics/wallets).
[^16]: Jack Grigg / Zcash, [ZIP 311: Zcash Payment Disclosures](https://zips.z.cash/zip-0311), Draft.
[^17]: Monero, [Wallet RPC documentation](https://docs.getmonero.org/rpc-library/wallet-rpc/), transaction, spend, and reserve proof methods.
[^18]: D. Fett, K. Yasuda, B. Campbell / IETF, [RFC 9901: Selective Disclosure for JSON Web Tokens](https://www.rfc-editor.org/rfc/rfc9901.pdf), November 2025.
[^19]: FATF, [Virtual-assets interpretive-note statement](https://www.fatf-gafi.org/en/publications/Fatfrecommendations/Regulation-virtual-assets-interpretive-note.html), 2019; [Recommendation 16 guidance consultation](https://www.fatf-gafi.org/en/publications/Fatfrecommendations/R16-Public-Consultation-June-2026.html), June 2026. These establish design context, not a jurisdiction-specific legal determination.
[^20]: Shieldd, [compliance flow](compliance/flow.md), [reference](compliance/reference.md), and [enforcement implementation boundaries](compliance/enforcement-and-seizure.md).
[^21]: Bankd, [#332: Enhance voluntary disclosure](https://github.com/mizufinance/bankd/issues/332) and [#192: selective supervisory access](https://github.com/mizufinance/bankd/issues/192).
[^22]: Shieldd, [metadata commitment implementation](../crates/disclosure/src/claims.rs) and [note commitment](../crates/core/component/shielded-pool/src/note.rs).
[^23]: Julia Len, Paul Grubbs, Thomas Ristenpart, [Partitioning Oracle Attacks](https://www.usenix.org/conference/usenixsecurity21/presentation/len), USENIX Security 2021.
[^24]: Consensys, gnark v0.15.0 [Groth16 backend](https://github.com/Consensys/gnark/blob/v0.15.0/backend/groth16/groth16.go) and [PLONK backend](https://github.com/Consensys/gnark/blob/v0.15.0/backend/plonk/plonk.go).
[^25]: Shieldd, [existing commitment gadget](../tools/gnark/internal/circuits/shared_core.go) and [commitment vector test](../tools/gnark/internal/primitives/crypto_primitives_test.go).
[^26]: Neha Narula, Willy Vasquez, Madars Virza, [zkLedger: Privacy-Preserving Auditing for Distributed Ledgers](https://www.usenix.org/system/files/conference/nsdi18/nsdi18-narula.pdf), NSDI 2018.
