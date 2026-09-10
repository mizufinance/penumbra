# Disclosure proof performance and prior work

RISC Zero remains an implementation candidate for extensible disclosure claims,
but its privacy assurance and local proving performance are not independently
settled. Its current security model cautions privacy-critical users that the
zero-knowledge argument remains incomplete. Existing payment-disclosure designs support the proposed
separation between output disclosure, spending-authority control, and ledger
acceptance. They do not provide a drop-in implementation of hidden-memo metadata
claims over Shieldd's existing Decaf377 cryptography.

The useful near-term work is reducing repeated cryptographic operations and
adapting the existing field implementation to RISC Zero's arithmetic facilities.
Changing the payment circuit or introducing remote proving is unnecessary for
these improvements. This assessment uses sources accessed on September 9, 2026.

## Existing disclosure designs

**Zcash ZIP 311** is particularly close to the required semantics. It binds a
disclosure to a transaction, selected output indices, a message or challenge,
and spending-authority signatures. Its verification procedure checks unique
indices and the transaction ID, while the caller establishes mined status.
It explicitly distinguishes spending authority from a particular sender address.
However, it remains a draft, its reference implementation is marked TBD, and
output disclosure uses outgoing cipher keys. It is design precedent, not a
ready implementation of hidden amount predicates or selectively revealed memo
fields. Its method of reproving a spend avoids retaining the original authority
randomizer; that requires a different witness and proof workflow from retaining
the randomizer for a later signature. [1]

**Monero** has implemented transaction and spend proofs. The wallet RPC binds a
transaction proof to a transaction ID, destination, and optional message; the
checker reports proof validity, received amount, pool status, and confirmations.
This supports keeping cryptographic evidence and node acceptance distinct.
Monero's documentation also distinguishes payment evidence from a claim that
outputs remain spendable. Its narrow, purpose-built proof primitives do not
implement arbitrary predicates over Shieldd notes or private memo processing.
The useful reuse is the product semantics and challenge binding, not its curve
code or proof encoding. [2][3]

**zkLedger** addresses a different, larger problem: complete financial auditing.
Its columnar ledger structure prevents participants from silently omitting
transactions, and rolling caches make subsequent queries efficient. Its reported
sub-10-millisecond queries over 100,000 transactions depend on that architecture;
they are not a benchmark for scanning arbitrary encrypted Shieldd transactions
inside a zkVM. Selected-output totals remain accurately described as selected
totals. Complete-history claims would require additional authenticated ledger
structure and are outside this feature. [4]

## RISC Zero versus alternatives

The decisive requirement is private witnesses without a SNARK wrapper. The
current SP1 Hypercube security documentation says its individual STARK proofs
are not zero knowledge and obtains zero knowledge through Groth16 or PLONK.
Thus, replacing RISC Zero with SP1 compressed STARK proofs would not preserve
this requirement. This is a statement about the documented proof modes, not a
claim that all zkVMs have the same limitation. [5]

Succinct also publishes VEIL, a wrapper intended to add zero knowledge to
multilinear interactive proofs. Its current README explicitly labels it an
unaudited proof of concept unsuitable for production. It is relevant research,
but does not overturn the documented privacy limitation of ordinary SP1 STARK
proofs or provide a demonstrated replacement for this guest. [17]

RISC Zero's 3.0.6 receipt documentation explicitly describes its receipt as a
zero-knowledge proof and requires the verifier to supply the expected program
identity. It also warns that receipt structures are recursive: a byte-size cap
alone is insufficient protection for unrestricted recursive binary decoding.
Use a depth-bounded decoder for packages received from other parties. [13]

The receipt API is not sufficient evidence of privacy assurance. RISC Zero's
current version-3.0 security model says it targets perfect zero knowledge but
has not completed the mathematical argument, and urges caution for critical
privacy requirements. Advisory GHSA-5xgj-pmjj-gw49 remains published with all
versions affected and no patched version identified; it reports no known
exploit. This qualifies the earlier recommendation: the prototype can measure
correct execution and public-output minimization, but neither proves the
cryptographic hiding property. Native receipts should not be presented as an
independently established privacy solution on this evidence. [15][16]

Anoma's March 2026 measurements are useful because they include local Apple
M-series proving, resource-transfer logic, and executable benchmark repositories.
They compare RISC Zero 3.0.3 succinct receipts with SP1 5.2.4 compressed proofs.
RISC Zero's reported transfer cases take approximately 30 and 57 seconds, and a
compliance case takes 86 seconds. These workloads use different cryptography and
do not reproduce Shieldd's note or memo operations. The SP1 proof mode also does
not provide an equivalent privacy guarantee. The measurements demonstrate that
local private-resource computation is practical in some cases; they cannot
establish Shieldd's latency or rank the current releases generally. [6]

No ready Decaf377 guest adapter was located in the reviewed primary sources.
RISC Zero's documented accelerated crates include several other curves and a
generic integer implementation, but not Decaf377. Its Arkworks fork was also
inspected: neither the default revision `621be87` nor the inspected experimental
branch `84ae735` exposed a usable RISC Zero syscall integration in `ff/src`.
This is a bounded search result, not evidence that no such implementation exists
anywhere. [7][8]

## Specialized client-side proofs

Microsoft Research's **Vega** is a relevant alternative to the general-purpose
VM approach. Its published credential workload reports 92 ms proving, 23 ms
verification, and a 108 kB proof for a 1,920-byte credential, without trusted
setup. It moves repeated work into rerandomizable preprocessing and folds
uniform hashing steps. These results concern signed credentials, not Shieldd
notes, Decaf377 commitments, or encrypted memos. They demonstrate that local
selective disclosure need not inherently take minutes. [18][19]

Vega separates setup, witness preparation, online proving, and verification.
Its reusable prepared state is specific to the same signed data. The quoted
proving latency must therefore not be treated as the total first-use cost of a
new payment disclosure; no corresponding Shieldd preparation cost has been
measured. [21]

Vega accepts circuit descriptions rather than executing the existing Rust claim
program. Adopting it here would require implementing and validating the relevant
Shieldd cryptography and memo checks as circuits. That is a larger change than
adapting existing field arithmetic inside the zkVM. Its useful immediate lessons
are avoiding full parsing when the claim only needs selected bytes, and reusing
verified work. Rerandomizable proof preprocessing is a potential later direction,
not a feature established by the current SDK or benchmarks. The repository's
warning about its preliminary Python reference should not be misread as an
assessment of the separate Rust prover. [18][20]

## Applicable arithmetic work

RISC Zero officially supports generic modular multiplication and documents how
existing cryptographic crates can call precompiles. This permits an adapter
inside the existing Decaf377 field implementation without replacing the curve,
commitment, address, or signature algorithms. The adapter must preserve
Arkworks' Montgomery representation and canonical field values. The current
experiment checks those representations against native Arkworks and checks the
guest's entire public statement against native claim evaluation. [7]

`risc0-bigint2` 1.4.14 provides checked finite-field arithmetic and additional
unchecked variants. Its existence is a useful alternative to hand-writing field
algorithms, but adopting it still requires checking the exact guest feature
support and measuring its benefit. The public API reviewed supplies modular
multiplication rather than a direct drop-in Arkworks Montgomery operation.
Veridise reviewed a scoped BigInt2 implementation in a December 2024 engagement;
that review does not cover this disclosure guest or its new adapter. [9][10]

The Decaf377 review explains that its square-root implementation uses a table
method derived from Sarkar's algorithm. Here the unexpectedly large cost was
constructing those fixed tables for every fresh guest execution. Replacing
independent exponentiations and inversions with successive powers preserves
the table definitions. A parity test compares every generated entry with its
defining power; the existing square-root property tests exercise the resulting
algorithm. No square-root or curve formula was replaced. [11]

RISC Zero's optimization guide also recommends profiling before changes,
avoiding unnecessary serialization, and considering page traffic and segment
boundaries. These are relevant after the dominant arithmetic work has been
reduced. Its general advice does not establish whether a particular host binary
uses GPU acceleration; the installed prover and its actual call stacks must be
checked. [12]

An upstream report identifies the same Apple Silicon limitation observed locally:
the 3.0 release's segment-prover path falls back to CPU, despite documentation
claiming Metal is enabled. The report links the relevant code changes. Installing
a missing Metal compiler would therefore not accelerate the observed segment
proving path. This is a version-specific limitation, not a general statement
about all RISC Zero releases or recursion backends. [14]

## Local measurements

These measurements use the disclosure guest pinned to RISC Zero 3.0.6 and the
installed 1.97.0 guest toolchain. The fixture selects one output, proves an amount
predicate, reveals one committed metadata field, and keeps the memo and other
metadata hidden. The fixture's transaction reference is synthetic; execution
tests do not establish transaction acceptance.

| Cumulative experiment | Guest cycles | Segments at maximum 2^18 |
| --- | ---: | ---: |
| Original implementation | 132,790,156 | 609 |
| Iterative square-root table construction | 36,871,615 | 172 |
| Field multiplication precompile with native representation conversion | 25,049,501 | 119 |
| Representation conversion through the same precompile | 14,740,881 | 72 |
| Reuse parsed note, ephemeral key, and cached diversified generator | 9,366,247 | 46 |
| Published BigInt2 API with canonical-result check | 9,614,440 | 47 |
| Raw byte input | 8,974,277 | 44 |
| Parse memo return address only when revealed | 8,372,303 | 41 |
| BigInt2 inverse | 8,182,318 | 40 |
| Checked direct integer-to-field conversion | 7,690,251 | 38 |
| Thin LTO and one codegen unit | 6,741,987 | 33 |

The cumulative cycle reduction is about 94.9%. Full LTO measured 6,702,083 cycles,
less than 1% better than thin LTO, so that experiment was not retained.
Profile totals include profiler
accounting that differs from the executor's reported cycle metric; the table
uses the executor metric consistently. Fixtures randomize memo encryption keys,
so individual runs can differ slightly. These are execution measurements,
not proportional predictions of proving latency.

The initial end-to-end baseline used an accepted ordinary Transfer, but its
proof was deliberately stopped after roughly 39 minutes while still proving
execution segments. It did not produce a receipt. Sampled peak prover RSS was
approximately 2.8 GB, with no observed swapping. This is an incomplete baseline,
so an exact baseline-to-optimized proving speedup cannot be reported.

An intermediate two-thread proof with the 9.36-million-cycle guest was also
stopped deliberately after 2,626.6 seconds, still in segment proving. It produced
no receipt; reported peak RSS was 2,889,449,472 bytes with zero swaps. Neither
interrupted run is evidence of successful proof generation or its final latency.

Before optimization, synthetic batches of 1, 2, and 8 outputs required roughly
132.8, 166.2, and 366.5 million cycles. The incremental cost was approximately
33.4 million cycles per output; simply batching did not remove the arithmetic
bottleneck. After optimization, execution of the pinned guest produced:

| Synthetic selected outputs | Note predicate only | Predicate and hidden-memo metadata |
| --- | ---: | ---: |
| 1 | 3,107,644 cycles | 6,742,296 cycles |
| 2 | 5,204,449 cycles | 12,472,636 cycles |
| 8 | 17,788,036 cycles | 46,871,192 cycles |

The incremental costs are approximately 2.10 and 5.73 million cycles per output,
respectively. Note-only claims verify the note commitment without deriving
decryption keys. Memo claims additionally authenticate the encrypted payloads
and check the metadata commitment. These synthetic fixtures use distinct
transaction references, but are not accepted multi-transaction proofs. Full
proving measurements remain necessary to establish throughput.

For synthetic outputs sharing one spending authority, reusing the same
request-bound signature and verifying each identical key/signature pair once
reduced eight-output execution from 45,968,386 to 21,489,996 cycles (about 53%).
The two-output case fell from 12,020,622 to 8,603,214 cycles. Reuse is local to one
request; changed signatures, keys, and challenges remain rejection cases.

With an identical one-output witness, segment limits of 2^18, 2^19, and 2^20
produced 33, 15, and 8 segments respectively, with the same 6,742,639 user cycles
and public statement. Larger segments trade memory for fewer segment boundaries
and recursive joins; these execution counts alone do not measure that tradeoff.
The completed proving measurement uses 2^19 and four Rayon threads on an Apple
M4 Pro with 14 CPU cores and 48 GB RAM.

An independently executed accepted-payment fixture matched native evaluation
at 6,898,405 cycles and 34 segments with the 2^18 limit. Its earlier end-to-end
export failed after 1,819.83 seconds, with 2,813,411,328 bytes peak RSS and zero
swaps. The benchmark had enabled prover diagnostics, exposing a reproduced bug
where inherited stdout contaminated the worker's JSON result. A separate
temporary result file fixes that transport problem. No usable disclosure receipt
was retained from that failed run, so it is not a completed proof benchmark.
A tiny diagnostic program separately produced and verified a real native
succinct receipt in 15.19 seconds; that checks the runtime, not disclosure cost.

The corrected release end-to-end test passed using RISC Zero 3.0.6 and its Rust
1.97.0 guest compiler. It built and accepted a real payment, then exported a
selected committed metadata field while hiding the note amount, recipient,
remaining metadata, and memo. This accepted witness used 6,854,717 user cycles
and 16 execution segments in preflight. Results from the completed run:

| Measurement | Result |
| --- | ---: |
| CLI export, including proof generation and local checks | 1,329.05 seconds (22 minutes 9 seconds) |
| Separate local receipt verification | 10.93 milliseconds |
| Serialized native succinct receipt | 606,874 bytes |
| Complete disclosure package | 813,157 bytes |
| Maximum resident set size (`time -l`) | 5,226,102,784 bytes |
| Swaps | 0 |
| Complete end-to-end test, excluding compilation | 1,351.86 seconds |

Inspection, offline verification, node-confirmed verification, and receipt import
passed from separate wallet directories. The node adapter served a committed
test-host snapshot; this was not a running Bankd deployment. Wallet balances and
sync height remained unchanged. A separate saved-receipt test passed rejection
checks for another guest identity, tampered claims, unsupported versions, and
malformed evidence without generating another proof. This is one accepted-payment proof, not a real
multi-payment proving benchmark. Batch execution measurements above do not
establish batch proving latency. The completed CPU result remains too slow for
interactive disclosure; the cycle reduction must not be reported as a measured
wall-clock speedup against the cancelled or failed baselines.

Focused validation also passed: 17 disclosure SDK tests, five guest-execution
checks, three CLI worker/publication tests, three wallet/custody tests, three
note-encryption tests, six embedded-service tests, and the Bankd keeper and
embedded-client Go packages. Decaf377's 95 native tests and nine RDsa integration
tests passed at the pinned dependency commits. Formatting, diff checks, and
dependency policy passed. The policy documents two non-applicable host dependency
advisories; the integer-library memory-safety fix is included. The
pinned installer built on macOS; fresh hosted Linux CI was not run locally.

## Practical conclusions

Retain one claim program and the selected-output request model. Reuse the
existing cryptography, eliminate repeated work, and measure real proofs before
settling the prover settings. A comparison must include proving, receipt
compression, verification, memory, and receipt size, and must state whether it
uses accepted transactions or synthetic witnesses.

Do not turn key-sharing payment proofs into an apparent substitute for
proof-only disclosure: key sharing changes what the recipient can decrypt.
Likewise, do not claim that a valid control signature identifies a legal person
or that a selected sum covers complete history. Those are semantic boundaries
supported by the existing disclosure literature as well as the local design.

Further worthwhile experiments are arithmetic adapter overhead, fixed-table
initialization, repeated transaction-memo work, input serialization, and segment
sizing within memory limits. They should be retained only when measurements
justify their complexity. A new cryptographic backend, remote service, or
recursive proof composition layer needs evidence of an advantage over these
simpler changes.

## Sources

1. Jack Grigg / Zcash, [ZIP 311: Zcash Payment Disclosures](https://zips.z.cash/zip-0311), draft, accessed September 2026.
2. Monero Project, [Wallet RPC documentation](https://web.getmonero.org/resources/developer-guides/wallet-rpc.html#get_tx_proof), transaction-proof methods, accessed September 2026.
3. Monero Project, [monero-wallet-cli reference](https://docs.getmonero.org/interacting/monero-wallet-cli-reference/), proof semantics, accessed September 2026.
4. Neha Narula, Willy Vasquez, Madars Virza, [zkLedger: Privacy-Preserving Auditing for Distributed Ledgers](https://www.usenix.org/conference/nsdi18/presentation/narula), NSDI 2018.
5. Succinct, [SP1 Security Model](https://docs.succinct.xyz/docs/sp1/security/security-model), Hypercube zero-knowledge section, accessed September 2026.
6. Xuyang / Anoma, [zkVM exploration](https://forum.anoma.net/t/zkvm-exploration/2579), benchmark update March 2, 2026; linked source repositories.
7. RISC Zero, [Precompiles](https://dev.risczero.com/api/zkvm/precompiles), documentation version 3.0.
8. RISC Zero, [Arkworks algebra fork](https://github.com/risc0/arkworks-rs-algebra), revisions `621be87b6660057dc4de037c7630a40645f7b0ae` and `84ae7357f56c7833f864a1d459117466c153a64f` inspected.
9. RISC Zero, [risc0-bigint2 1.4.14 field API](https://docs.rs/risc0-bigint2/1.4.14/risc0_bigint2/field/index.html).
10. Veridise, [BigInt2 Precompile assessment](https://veridise.com/audits-archive/company/risc-zero/bigint2-precompile-2025-03-25/), engagement December 2024, report listed March 25, 2025.
11. NCC Group, [Decaf377 Implementation and Poseidon Parameter Selection Review](https://www.nccgroup.com/media/iybds23k/_ncc_group_penumbralabsinc_report_2022-09-12_v11.pdf), version 1.1, September 12, 2022.
12. RISC Zero, [Guest Optimization Guide](https://dev.risczero.com/api/zkvm/optimization), documentation version 3.0.
13. RISC Zero, [Receipt API](https://docs.rs/risc0-zkvm/latest/risc0_zkvm/struct.Receipt.html), version 3.0.6, identity binding and serialization guidance.
14. RISC Zero issue tracker, [release-3.0 docs incorrectly claim Metal proving is enabled by default, #3753](https://github.com/risc0/risc0/issues/3753), May 16, 2026; corroborated against the installed prover's CPU call stack.
15. RISC Zero, [Cryptographic Security Model](https://dev.risczero.com/api/security-model), version 3.0, zero-knowledge limitations, accessed September 2026.
16. RISC Zero, [GHSA-5xgj-pmjj-gw49: notes on zero-knowledge](https://github.com/risc0/risc0/security/advisories/GHSA-5xgj-pmjj-gw49), July 15, 2024, current advisory checked September 2026.
17. Succinct, [VEIL README](https://github.com/succinctlabs/sp1/blob/main/slop/crates/veil/README.md), experimental status, accessed September 2026.
18. Microsoft, [Vega prover](https://github.com/microsoft/vega-prover), Rust implementation and workload measurements, accessed September 2026.
19. Kaviani and Setty, [Vega: Low-Latency Zero-Knowledge Proofs over Existing Credentials](https://eprint.iacr.org/2025/2094), IEEE S&P 2026, revision September 5, 2026.
20. Microsoft, [The Vega Prover Book](https://microsoft.github.io/vega-prover/), circuit interface and preprocessing design, accessed September 2026.
21. Microsoft, [Vega proving lifecycle](https://microsoft.github.io/vega-prover/overview/lifecycle.html), preparation and online proving boundaries, accessed September 2026.
