# Voluntary disclosure

`shieldd-sdk-disclosure` implements selected-output claims in one RISC Zero guest.
It reuses Shieldd's note commitments, address parsing, payload encryption and
memo decryption. It does not alter payment proofs or consensus. A selected-output
total describes exactly the listed outputs, not complete account activity.
A single request can select outputs across transactions and block heights on
the same chain; proof generation is not restricted to one transaction.

See [prior work and performance measurements](disclosure-research.md) for the
proof-system comparison, applicable optimizations, and benchmark limitations.
The selected release's security model still cautions that its mathematical
zero-knowledge argument is incomplete. This prototype does not establish that
privacy guarantee; successful verification establishes the claimed execution.

The host verifies native succinct RISC Zero receipts against its compiled guest identity.
Development receipts are disabled. Package-supplied identities are never trusted.
Receipt verification and node-confirmed acceptance are separate: a recipient must
query their chosen Bankd node's committed `TransactionsByHeight` data and chain
parameters before treating a disclosure as fully verified. This is trust in that
chosen node, not an independent consensus or state-membership proof.

## SDK

`prepare` selects private note witnesses from wallet-owned transactions.
`evaluate` is pure claim logic shared by the host and guest. `prove` runs a local
`r0vm` child process; it never selects a remote proving backend. `inspect` describes
unverified claims and decryption capabilities. `verify` checks the receipt or
explicit payload keys; `confirm_acceptance` checks independently fetched blocks.
Only selected witnesses are passed to the prover. No spending key is included.

Amounts use canonical unsigned 128-bit decimal strings. Comparisons disclose the
asset as well as the predicate. Sums reject duplicates, mixed assets and overflow.
Request bindings include chain, canonical transaction ID, height, action, output,
claim parameters, and optional recipient and challenge. Spending control requires
a fresh challenge supplied by the recipient; the verifier must compare the
request with the request they intended, rather than trusting package context.
CLI verification and import require `--request request.json` for spending-control
claims and compare the complete request, including recipient and challenge.

`Storage::prepare_disclosure_metadata` stores a fresh salt and an output-associated
metadata document and returns a memo plan containing its domain-separated hash.
Attach this memo before authorizing the payment. `Storage::build_transaction`
(with the view crate’s `prover` feature) retains authority before returning the
built transaction for submission. Builders managed by another SDK can call
`Storage::retain_disclosure_authority` with their final transaction and plan.
It checks the plan against the built transaction and stores each ordinary
Transfer's first input authorization randomizer. These operations do not reserve
notes or change spend availability. Missing historical witnesses fail explicitly.

The software and encrypted custody signers verify the randomized authority key
before signing the domain-separated request. Externally generated signatures can
be supplied in `DisclosureWitness`. Control means possession of the payment's
spending authority, not legal identity or proof of who initiated the payment.
Selected outputs sharing an authority reuse one signature within the request;
the guest verifies identical key/signature pairs once, with no reuse across challenges.

Payment-committed metadata appears in each output's `metadata`. Later attachments
appear separately in `context`: a document digest and optional signature bind the
attachment to this statement. An unsigned context item is not authenticated; a
commitment or valid signature does not establish the truth of its contents.

## CLI

Build with `cargo build --release -p pcli --features disclosure-prover`. The pinned release
is RISC Zero 3.0.6; install its `r0vm` and the RISC Zero Rust 1.97.0 guest toolchain
using `rzup`. The guest has a committed lockfile and builds with locked resolution.
Verification requires the compiled guest identity but does not run a prover.

```
cargo install rzup --version 0.5.2 --locked --no-default-features --features cli,install
rzup install rust 1.97.0
rzup install r0vm 3.0.6
```

```
pcli disclosure export --wallet sender.sqlite --request request.json --output proof.json
pcli disclosure inspect proof.json
pcli disclosure verify proof.json --node http://localhost:9090
pcli disclosure import proof.json --node http://localhost:9090 --output receipts/payment.json
```

The export preview lists the request and revealed values before proving. The
worker runs as a separate local process, reports activity, and is cancelled with
Ctrl-C. The package is published atomically only after successful local receipt
verification. An ephemeral result file separates packages from prover diagnostics
and is removed when the job finishes or is cancelled. Existing files are never overwritten. Inspection and verification
work without a wallet configuration. Import stores a receipt file and does not
insert spendable notes into the recipient's wallet.

`--payload-keys` explicitly grants decryption of the selected notes and the whole
transaction memo, including its return address. It is a broader capability than
a selective proof. Proof-only packages contain neither payload keys nor note
seeds. Hidden metadata documents and salts remain in wallet storage.

A request names `outputs` explicitly. Each entry has `reference` (transaction ID,
height, `action: {"Body": 0}` or `"FeeFunding"`, and output index), disclosure flags
`amount`, `asset`, `recipient`, `memo`, `spending_control`, optional `predicate`,
and `metadata_fields`. The top-level request has `version: 1`, `chain_id`, optional
`recipient`, `challenge`, and `total`. All optional fields use JSON `null` when
absent. Predicates include `GreaterThan`, `LessThan`, `AtLeast`, `AtMost`, and
`InclusiveRange` with `lower` and `upper`; totals have `reveal` and `predicate`.

## Verification commands

Run only one build, suite or prover at a time with `CARGO_BUILD_JOBS=2` and
`RAYON_NUM_THREADS=2`. The real proof test is explicitly ignored by ordinary tests:

```
cargo test -p shieldd-sdk-disclosure --features proof --test claims -- --test-threads=2
cargo test --release -p shieldd-sdk-disclosure --features prover --test claims real_note_and_hidden_memo_proof -- --ignored --nocapture --test-threads=1
```

The real test prints elapsed proof time and package size. Measure process memory
with the host's process tools. The guest uses the existing Decaf377 implementation
with checked RISC Zero field-arithmetic acceleration. Native receipt generation
can take minutes; no Groth16 wrapping is used.

The end-to-end test builds and accepts a real payment, then runs export,
inspection, verification and import from separate wallet directories. Its gRPC
adapter serves the committed test host snapshot; it does not launch Bankd.
Stage native payment libraries as described in [Embedded artifacts](embedded-artifacts.md),
set `SHIELDD_ARTIFACT_ROOT`, and build the release CLI above before running:

```
SHIELDD_PCLI_BIN="$PWD/target/release/pcli" \
SHIELDD_DISCLOSURE_TEST_RECEIPT=/tmp/disclosure-receipt.json \
cargo test --release -p shieldd-sdk-app-tests --features disclosure-e2e --test disclosure export_import_between_wallet_directories -- --ignored --nocapture --test-threads=1

SHIELDD_DISCLOSURE_TEST_RECEIPT=/tmp/disclosure-receipt.json \
cargo test -p shieldd-sdk-disclosure --features proof --test claims saved_real_receipt_rejects_wrong_guest_and_tampering -- --ignored --test-threads=1
```

The saved-receipt check verifies the same real receipt and rejects another guest
identity, unsupported versions, changed claims and malformed evidence without
generating another proof.
