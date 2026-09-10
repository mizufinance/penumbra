# Voluntary transaction disclosure

Disclosure is an off-chain wallet operation. It never reserves notes or modifies
payment proofs, commitments, consensus, or spendable balances. Compliance audit
pulls through ACP/Orbis are a separate, deferred mode.

| Evidence | What the recipient obtains |
| --- | --- |
| Payload keys | Selected note plaintext, including its seed, and transaction-wide memo decryption capability |
| Full openings | Amount, asset, recipient and commitment blinding/recovery values, without note seeds or payload keys |
| Groth16 | Selected public fields and explicit true/false predicate results |

Full openings require all three field selectors. Memo revelation uses payload-key
exports. There is no structured metadata framework. An asset identifier and a
recipient address do not themselves establish legal identity; KYC linkage belongs
to the recipient's trusted records.

## Requests and claims

`DisclosureRequest` selects canonical transaction ID, accepted height, action,
and output index, with a chain ID and optional audience (`recipient`) and
challenge. Action references are `{"Body": 0}` or `"FeeFunding"`.
Each selection specifies `amount`, `asset`, `recipient`, `memo`,
`spending_control`, and optional `predicate`.

Predicates are `GreaterThan`, `LessThan`, `AtLeast`, `AtMost`, and
`InclusiveRange`. Thresholds and amounts are canonical unsigned decimal strings
in asset base units. Predicate outputs always include their asset and a boolean
result. Invalid proofs or unavailable inputs never mean false.

An optional `total: {"reveal": false, "predicate": {"AtLeast": "500"}}`
operates over every explicitly selected output, including selections from multiple
transactions. All selected outputs must have one asset. These are **selected-output
totals**, not complete wallet or account activity. Duplicate references and u128
overflow are rejected. A single proof supports at most 32 outputs; larger hidden
totals are rejected, never silently split.

## SDK and acceptance

The SDK provides `prepare`, `inspect`, `verify`, `prove`, `export_openings`, and
`export_payload_keys`. Wallet storage, custody, node access and processes stay
outside commitment evaluation. `inspect` describes unverified claims.

`verify` checks cryptography only. `confirm_acceptance` additionally compares
selected public data with the chosen node's committed `TransactionsByHeight`
records and chain identity. A cryptographically valid disclosure without that
check is not fully verified. `verify_candidates` checks encrypted transaction
bytes obtained from Shinzo without treating indexer records as execution evidence.

Bankd exposes `mizufinance.shieldd.v1.Query/TransactionsByHeight` through the
embedded Shieldd adapter. An indexed success code, database CID, or indexer
signature alone does not authenticate successful Shieldd execution.

## CLI

```sh
pcli disclosure capabilities
pcli disclosure export --wallet wallet.sqlite --request request.json --output disclosure.json --openings
pcli disclosure export --wallet wallet.sqlite --request request.json --output disclosure.json --payload-keys
pcli disclosure export --wallet wallet.sqlite --request request.json --output disclosure.json
pcli disclosure inspect disclosure.json
pcli disclosure verify disclosure.json --node http://localhost:9090
pcli disclosure import disclosure.json --node http://localhost:9090 --output receipt.json
```

Inspection, verification and capabilities require no wallet configuration.
Verification accepts `-` to read a bounded package from stdin. `--request` checks
an expected request and is required when spending control is claimed.
`--transactions` takes a JSON array of base64 canonical transaction bytes for
candidate-content checks, before the chosen-node acceptance check.

Export previews fields and capabilities. The prover runs in a separate local
process with private stdin and a restrictive temporary result file. Cancellation
kills its process group. A package is published atomically only after verification;
existing files are never overwritten. Import stores the original verified receipt
without adding spendable notes.

## Spending-authority control

Software and encrypted custody sign a domain-separated request under the ordinary
Transfer's mandatory real first input authorization key. The wallet retains its
randomizer before returning a built outgoing transaction for submission. The
spending key remains in custody. External signatures can be supplied through the
SDK. Historical payments without retained randomizers report an unavailable witness.

Verification checks this signature separately from ZK and compares the key with
the accepted transaction. It proves current control of that spending authority,
not legal identity or who initiated the historical payment. Requesters must issue
fresh challenges; replaying a stored proof does not demonstrate fresh control.

## Local proving artifacts

The fixed circuit uses gnark Groth16 over BLS12-377 and the existing Poseidon377
note-commitment gadget. It constrains selectors, inactive padding, field revelation,
128-bit amounts, totals, predicates and a statement-context hash. The verifier
reconstructs all public inputs and rejects unknown circuit or key identities.

For a development build:

```sh
cd tools/gnark
GOMAXPROCS=1 go build -p 1 -o /tmp/shieldd-disclosure-backend ./cmd/disclosure
/tmp/shieldd-disclosure-backend setup-development /tmp/disclosure-artifacts
cd ../..
CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 cargo build -p pcli --features disclosure-prover,development-disclosure-artifacts
export SHIELDD_DISCLOSURE_BACKEND=/tmp/shieldd-disclosure-backend
export SHIELDD_DISCLOSURE_ARTIFACTS=/tmp/disclosure-artifacts
```

Artifacts come from local configuration, never submitted packages. Development
setup is explicitly labelled. Release builds reject it even if the development
feature is enabled. Production ZK acceptance remains disabled until an approved
ceremony and its keys are pinned. Full openings and payload-key verification do
not depend on proving artifacts.

The backend caps Go concurrency at two and respects a lower GOMAXPROCS setting. Run only one heavy verification job at
a time, with the resource checks in [development.md](development.md).

## Verification fixtures

The real proof test is `cargo test -p shieldd-sdk-disclosure --features
prover,development-artifacts --test claims real_proofs -- --ignored --nocapture
--test-threads=1`, using the configured development backend/artifacts. It covers
one, eight and 32 selected outputs, revealed fields, true/false predicates and
public-statement mutations. Run it with `GOMAXPROCS=1 RAYON_NUM_THREADS=1` when
using the one-worker resource bound.

The release app tests `accepted_disclosure_opening` and
`export_import_between_wallet_directories` require a payment proving runtime.
Set `SHIELDD_GNARK_TRANSFER_LIB` to the library built from `tools/gnark/cmd/transferlib`
and `SHIELDD_GNARK_TRANSFER_ARTIFACT_DIR` to `tools/gnark/artifacts/transfer`.
The latter test uses `SHIELDD_PCLI_BIN` pointing to an explicitly enabled debug
prover: release builds correctly reject development disclosure keys.

For the combined private audit workflow, build Bankd's disclosure test binary with
`go test -p 1 -tags auditdevelopment -c -o /tmp/bankd-disclosure-workflow ./supervisor/disclosure`,
and its CLI with `go build -p 1 -tags auditdevelopment -o /tmp/bankd-disclosure-audit ./cmd/disclosure-audit`.
Set `BANKD_DISCLOSURE_TEST_BIN` and `BANKD_AUDIT_CLI` to those binaries, and supply
`DEFRA_TEST_BIN` plus `VERA_TEST_FIXTURE`. The release app test
passes its real accepted transaction and running query endpoint to that test.
The fixture creates separate private/index databases and uses real Vera grants.
It does not upgrade or write to existing Shinzo deployments.

Measured on native macOS ARM64 with one worker and development setup artifacts:

| Selected outputs | Proving and local verification | Package bytes |
| --- | --- | --- |
| 1, including amount/recipient revelation | 81.90 s | 4,535 |
| 8, hidden amounts with a selected-output total predicate | 81.94 s | 29,246 |
| 32, hidden amounts with a selected-output total predicate | 83.02 s | 114,397 |

The separate wallet workflow produced a 244-byte Groth16 receipt. Package sizes
also include public transaction material and requests. The three-proof suite
reached 1,052,360,704 bytes maximum RSS with zero swaps. The fixed circuit has
approximately constant proving cost across these batch sizes; these measurements
do not establish production setup approval or query completeness.
