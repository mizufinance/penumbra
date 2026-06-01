# Codex Brief — P1 (Critical Path to Campaign Completion)

Source of truth: `docs/snarkpack/verification-plan.md` (layers), `docs/snarkpack/security.md`
(campaign order), `docs/snarkpack/formal-handoff.md` (the typed evidence ledger).
This brief is the executable task list; the ledger is what "done" means.

## Current state (read before starting)

The formal boundary work is **further along than the prose docs say**. As of
2026-06-01 the `formal-handoff.md` table has exactly **one `open` row** (the
RIPP-mapping review). Statement-encoding injectivity, digest reduction, padding
canonicality, and challenge-preimage injectivity are all **`proved`**
(`lemma_encode_statement_injective` in
`crates/crypto/proof-aggregation/formal/snarkpack/fstar/StatementEncodingProofs.fst`,
no admits). The "7 open rows / injectivity open" language in `security.md` and
`verification-plan.md` is **stale** — Task 0 fixes it.

So P1 is: fix the stale docs, close the one open formal row, turn the provisional
DoS/perf numbers into a hard CI gate, and finalize the assumption register. None
of these are blocked by each other except where noted — parallelize freely.

## Hard rules (non-negotiable)

- **Category 3 is forbidden.** Do not change Fiat-Shamir / transcript semantics.
  Never edit the *behavior* of `statement.rs` (`encode_statement`), `transcript.rs`,
  or `ip_proofs/src/challenge.rs`. Reviewing and documenting them is fine; changing
  what bytes they emit is not.
- **No byte/trace/version drift.** The byte-equivalence and trace baselines and
  `AGGREGATE_PROTOCOL_VERSION` must be unchanged by P1. P1 adds evidence and gates,
  it does not change the protocol.
- **Stamped-file discipline.** If you edit any stamped formal artifact
  (`toolchain.toml`, `fstar/*.fst`, `scripts/snarkpack-formal.sh`), you must
  restamp the `Proof artifact stamp:` line in `formal-handoff.md` and re-run
  `just snarkpack-invariants` until it prints `snarkpack invariants ok`. P1 should
  generally **not** need to touch these.
- **Prototype contract policy** (CLAUDE.md): delete obsolete paths, no aliases /
  shims / migrations / dual paths. Update all in-repo callers to the new design.
- **Prove it.** Bug fix ⇒ reproducing test first. State explicitly whether
  release-gated / prover-gated tests were actually run.

## Tasks

### Task 0 — Reconcile the stale docs with the ledger (do first; cheap)

The ledger is authoritative. Make the prose match it.

- In `security.md` and `verification-plan.md`, correct every claim that statement
  injectivity / digest reduction / padding canonicality are `open`. They are
  `proved`. Update the "7 open rows" / "P1 starts with injectivity" framing to the
  real state: **one** open formal row (RIPP-mapping), plus the DoS gate and
  assumption-register finalization.
- Re-derive the assumption-register counts from the live table
  (`grep -cE "\| <status> \|" docs/snarkpack/formal-handoff.md`) and fix any stale
  tallies in both docs.
- Acceptance: no doc claims a row is `open` that the ledger marks `proved`; the
  open-row list in the docs equals the ledger's `| open |` rows.

### Task 1 — Close the RIPP-mapping open row

The only `open` formal row:
`docs/snarkpack/ripp-refinement.md` — "local RIPP implementation maps to intended
algorithm". This is a **review artifact, not a mechanized proof**.

- Every `symbol_id` listed in
  `crates/crypto/proof-aggregation/formal/snarkpack/ripp-refinement-scope.txt` must
  appear in `ripp-refinement.md` exactly once. Verify the set is complete first.
- For each scoped symbol, review the Penumbra Rust against `ripp-spec.md` and the
  paper, classify the deviation (`mechanical` / `performance` / `security-binding`
  / `semantic`), and move its status from `open` to one of `refined` /
  `proved-equivalent` / `assumed` **with cited evidence** (file:line + spec row).
  - `security-binding` / `semantic` rows need the strongest evidence; do not mark
    these `assumed` without an explicit recorded rationale and owner.
  - Intentional Filecoin→Penumbra differences belong in
    `adaptation-register.md`, referenced from the row — not silently absorbed.
- When all scoped rows are non-`open`, flip the `formal-handoff.md` RIPP-mapping
  row from `open` to its resolved status and update its "current status" cell.
- Acceptance: `grep -c "| open |" docs/snarkpack/formal-handoff.md` returns `0`;
  every symbol in `ripp-refinement-scope.txt` is classified with evidence; the
  invariants gate passes.

### Task 2 — DoS-asymmetry + perf gate (independent of all formal work)

Turn the provisional latency/size numbers into an enforced "reject cheaply"
guarantee. This is the Layer-10 row in `verification-plan.md`.

- `docs/snarkpack/bench-thresholds.md` is a **provisional local baseline**.
  Re-establish thresholds on CI hardware (or document the chosen CI baseline) and
  remove the "provisional" status once they are real.
- Add **valid-vs-adversarial path benches**: prove a malformed / wrong-family /
  wrong-public-input / oversized aggregate is **rejected with bounded work**,
  cheaper than a valid verify. The invariant under test: no expensive work (no
  pairing / no full deserialize subgroup pass) happens before the cheap shape
  checks reject the input. Cover each rejection class.
- Wire a **CI gate** that fails on threshold regression (p50/p95/p99 latency under
  a realistic mixed-proposal workload, the size cap, and the cheap-rejection
  asymmetry). Add the `just` recipe + CI wiring; do not leave it as a manual bench.
- Acceptance: a deliberately slow rejection path (e.g. doing pairing work before a
  shape check) makes the new gate fail; valid-path thresholds hold; the gate runs
  in CI, not just locally.

### Task 3 — Finalize the assumption register

13 `assumed` rows in `formal-handoff.md`. Each must be **narrowed**: a recorded
postcondition + a removal path, and — where the evidence column already *names
required tests* — those tests must actually exist and run. Specifically:

- For the arkworks/decaf377 backend rows (lines for field/group/pairing, MSM,
  serialization/subgroup, decaf377), implement the **boundary property tests** the
  rows promise: MSM zero-scalar / identity / random-vector parity; G1/G2 subgroup,
  torsion, malformed-byte, and round-trip serialization tests. Cite the test names
  back in the row's evidence column.
- For the hax-shim rows, confirm each shim still has a recorded semantic
  postcondition in `hax-extraction-boundary.md` and that the removal path
  ("remove when hax/F* accepts this directly") is current.
- For the SHA-256 / domain-separation / Groth16 / RIPP-algebraic rows, confirm the
  postcondition + removal path read correctly under the **standing-assumption /
  no-end-to-end-FV** policy (these stay `assumed`; just make them honest and
  precise — no widening).
- Acceptance: every `assumed` row has a non-empty, current postcondition + removal
  path; every row that names a required test has that test implemented and passing;
  the register coverage invariant passes.

### Task 4 — Clean-image formal CI (close remaining formal infra)

- Confirm `just snarkpack-formal` (`scripts/snarkpack-formal.sh`) runs the hax→F*
  pipeline **reproducibly from a clean image** with the pinned toolchain
  (`toolchain.toml`), not just on a primed dev box. Wire it into CI if it is not
  already a required check.
- Acceptance: the formal gate is a required CI check and passes from clean; pins in
  `toolchain.toml` match what CI uses; `just snarkpack-invariants` passes.

## Gates to run before handoff (all must be green)

```sh
cargo test -p penumbra-sdk-proof-aggregation --lib
cargo test -p penumbra-sdk-proof-aggregation-reference --lib
just snarkpack-fuzz-smoke
just snarkpack-filecoin-shape
just snarkpack-formal
just snarkpack-invariants
cargo fmt --all -- --check
# plus the new P1 gate:
just snarkpack-dos-gate   # (name TBD — created in Task 2)
```

## Definition of done for P1

- No `open` rows remain in `formal-handoff.md`.
- Docs (`security.md`, `verification-plan.md`) match the ledger — no stale `open`
  claims, correct tallies.
- DoS/perf thresholds are real (not "provisional") and enforced by a CI gate;
  cheap-rejection asymmetry is benched and gated.
- Every `assumed` row is narrowed (postcondition + removal path); every promised
  boundary test exists and passes.
- The formal gate runs reproducibly in CI.
- No byte/trace/version drift; all gates above green.

P2 (fuzz corpus expansion, Lean differential conformance) is **not** in the P1
completion gate — see `docs/snarkpack/codex-brief-p2.md`.
