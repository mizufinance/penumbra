# Proof aggregation optimization

Optimize one measured cost at a time while preserving protocol bytes and
rejection behavior.

## Workflow

1. Record the target workload, command, build mode, and baseline distribution.
2. Profile before choosing a change.
3. Add the smallest benchmark or regression that captures the claimed win.
4. Implement one change and compare identical workloads.
5. Remove experiments that do not earn their complexity.
6. Run runtime invariants, interoperability, byte locks, and affected tests.

Use release builds for performance claims. Report medians and tail latency;
single-run timings are diagnostic only.

## Protocol boundary

The following must remain byte-identical unless the protocol version changes:

- canonical statements and digests;
- Fiat-Shamir stage labels, order, and encodings;
- aggregate proof and wrapper encodings;
- padding and real-count binding; and
- SRS identifiers and family routing.

An intentional wire change requires a version bump, new domain separation,
updated fixtures, both runtime implementations, and corresponding work in
`mizufinance/shieldd-security`.

## High-value targets

- repeated curve preparation and normalization;
- duplicate serialization or hashing;
- avoidable allocations and clones in GIPA rounds;
- reusable SRS-derived tables;
- batch inversions and MSM preparation; and
- early rejection of attacker-controlled sizes and malformed encodings.

Cache only immutable values keyed by canonical identities. Bound every cache
and do not let runtime configuration replace compiled protocol material.

## Validation

Run the narrow benchmark first, then:

```bash
bash scripts/check-snarkpack-runtime-invariants.sh
cargo test -p shieldd-sdk-proof-aggregation
cargo test -p shieldd-sdk-proof-aggregation-reference
```

Also run the relevant slow and release-gated suites for changes to aggregation,
verification, SRS preparation, or encodings. Record whether those suites were
actually run.

Reject a change that weakens malformed-input handling, changes bytes without a
versioned protocol update, or adds an unexplained alternate path.
