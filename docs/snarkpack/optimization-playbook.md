# SnarkPack Optimization Playbook

How to find, judge, implement, and land optimizations of the Penumbra-owned
RIPP backend without changing protocol bytes silently, without shipping
microbench fiction, and without paying complexity that the measured benefit
does not justify.

This is the durable process behind `security.md` Stage 10. Every optimization
follows it.

## 0. The one rule: optimize in category 1 or 2, never 3

| Category | Touches bytes? | What it is | How to land |
|---|---|---|---|
| **1 — internal compute** | No, by construction | Same group elements / field values, faster math | Default. Byte/trace baselines pass unchanged. |
| **2 — output/wire encoding** | Yes (wire bytes only) | Same transcript + elements, different serialization | Version-bump path (§5). |
| **3 — transcript / Fiat-Shamir input** | Yes (transcript bytes) | A *protocol* change, not an optimization | **Forbidden through this loop.** |

A category-1 optimization computes the identical element, so the on-the-wire
aggregate bytes and the PenumbraByte transcript are unchanged — the golden
baselines pass without anyone thinking about it, and `AGGREGATE_PROTOCOL_VERSION`
stays put.

Category 3 changes what gets hashed for challenges. That voids the Filecoin-shape
inheritance (`scripts/check-snarkpack-filecoin-shape.sh`) — the argument *"our
transcript is byte-shaped like the audited Filecoin/Bellperson SnarkPack,
therefore their soundness analysis applies to us"* — and would require
re-establishing that evidence plus the F\* boundary. It is not an optimization.
**Do not do category 3 under the guise of speed.** The byte/trace baselines make
it impossible to land silently: it shows up as a transcript-byte diff that fails
the gate.

Transcript surface to never touch as an "optimization":
- `encode_statement` — `crates/crypto/proof-aggregation/src/statement.rs`
- `ChallengeContext` / `challenge_preimage` — `crates/crypto/proof-aggregation/src/ipp/ip_proofs/src/challenge.rs`
- family transcript digests — `crates/crypto/proof-aggregation/src/transcript.rs`

## 1. FIND — where to look

Two complementary methods; use both.

### 1a. Profiler-driven (top-down)
The build and verify paths are already instrumented. Read the sub-timings and
attack the largest *measured* stage, not the most obvious one.

- `AggregateVerificationProfile` — `backend.rs` (verify path): `deserialize_ms`,
  `challenge_ms`, `tipa_ab_ms`, `tipa_c_ms`, `public_input_fold_ms`, `ppe_ms`,
  `core_total_ms`.
- `AggregateBuildBackendProfile` — `backend.rs` (build path): per-stage
  `backend_*_ms` fields, including the pairing breakdown
  (`backend_pairing_miller_loop_ms`, `backend_pairing_final_exponentiation_ms`)
  and the per-round GIPA/TIPA/KZG sub-timings.

Pairing cost (Miller loop + final exponentiation) usually dominates verify, so a
multi-pairing **merge** that removes a final-exponentiation beats a scalar-mul
tweak that the profiler can barely see.

### 1b. Pattern-driven (bottom-up)
Grep the RIPP backend (`backend.rs`, `src/ipp/ip_proofs/src/`,
`src/ipp/dh_commitments/src/`) for the recurring category-1 smells:

| Smell | Faster form |
|---|---|
| sequential `fold` / `mul_helper` scalar-mul-then-add over a vector | variable-base MSM (`G::msm`) |
| `.map(\|x\| x.inverse().unwrap())` over a vector | `ark_ff::batch_inversion` |
| N independent `cfg_multi_pairing` checks | one combined multi-pairing (random linear combination) — removes a final-exponentiation |
| `.clone()` on `PairingOutput` / `G1` / `G2` inside a per-round loop | in-place `add_assign` / `mul_assign` |
| independent serial sub-computations | `rayon::join` / `par_iter` |
| challenge powers / inverses recomputed across passes | hoist + reuse |
| leftover `//TODO: Optimization` / `VariableMSM` markers | the marked optimization |

## 2. CLASSIFY — record the category before coding

- Alters any byte that feeds a challenge hash? → **category 3, stop.**
- Changes the serialized aggregate-proof encoding? → **category 2** (§5).
- Same elements, faster compute? → **category 1** (§4; most optimizations).

If unsure between 1 and 3, the test is mechanical: implement, run the byte +
trace baselines. Pass unchanged ⇒ category 1. Fail ⇒ you changed bytes; decide
wire-only (2) vs transcript (3) and act accordingly.

## 3. MEASURE — prove the win is real

Never quote an isolated microbench as an end-to-end result. (The first
optimization showed a 45–90× *microbench* speedup that was ~1–2% — inside the
noise band — end-to-end at realistic batch sizes.)

### 3a. Corpus-backed bench
`crates/bench/benches/vanilla/snarkpack.rs` benchmarks `aggregate_family` and
`verify_family_aggregate` by `(family, count)`. The Groth16 proof corpus is
generated once and committed under `crates/bench/corpus/snarkpack/`; the bench
loads it via `load_or_generate_items` — proofs are **never** regenerated per
run, and the measured closure times only the aggregate/verify call.

### 3b. The compile-time A/B seam
To compare an optimization against its pre-optimization form on the *real*
end-to-end path in the *same release build*, use the `bench-baseline` feature
(compile-time, never a runtime env branch, never on a transcript path):

1. At the optimized call site, gate between the optimized impl and a retained
   `*_baseline` impl with `#[cfg(feature = "bench-baseline")]` (worked example:
   `_compute_final_commitment_keys` / `fold_keys_baseline` in
   `src/ipp/ip_proofs/src/gipa.rs`).
2. Build and run the bench twice and compare medians:

   ```sh
   # optimized (default)
   cargo build --release -p penumbra-sdk-bench --bench snarkpack
   ./target/release/deps/snarkpack-* --bench --warm-up-time 1 --measurement-time 4 "snarkpack verify"

   # pre-optimization baseline
   cargo build --release -p penumbra-sdk-bench --bench snarkpack \
     --features penumbra-sdk-proof-aggregation/bench-baseline
   SNARKPACK... ./target/release/deps/snarkpack-* --bench ... "snarkpack verify"
   ```

   (First ever run populates the corpus; pass the criterion `--bench` flag so it
   measures rather than runs in test mode.)
3. Report medians at realistic counts (n ∈ {1,2,4,8,64}). Flag anything inside
   the noise band as noise.

The retained `*_baseline` fn doubles as the equivalence-test oracle (§4), so the
seam and the correctness proof share one artifact.

## 4. DECIDE — the win-or-clarity bar

Land iff **either**:
- a **measured end-to-end gain above the noise floor** at realistic batch sizes, **or**
- a **clear correctness / clarity / scaling improvement that is provably never
  slower** (equivalence-tested, strictly better asymptotically, expresses intent
  more directly).

**Reject** changes that add non-trivial API surface or indirection for a
near-noise gain with no clarity or scaling case. "Technically faster in a
microbench" is not sufficient. Honest reporting includes "this was noise,
reverting."

## 5. IMPLEMENT — per-category workflow

### Category 1
1. Keep the pre-optimization implementation as a named `*_baseline` reference.
2. Add a unit equivalence test (`optimized == baseline`) at sizes covering
   **every** code path the optimization touches (cf. the both-sided
   `msm_keys_equals_sequential_fold` in `dh_commitments/src/afgho16/mod.rs`).
3. Wire the §3b A/B seam.
4. Implement; confirm the byte + trace baselines pass **unchanged** and the
   version stays 1.

### Category 2 (wire encoding only)
1. Bump `AGGREGATE_PROTOCOL_VERSION` (`statement.rs`).
2. Regenerate both golden baselines via the `--ignored` helpers:
   - `cargo test -p penumbra-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored`
   - `cargo test -p penumbra-sdk-proof-aggregation-reference regenerate_penumbra_byte_trace_baseline -- --ignored`
3. Add one `adaptation-register.md` row (deviation class `performance` or
   `mechanical`) plus the matching `adaptation-scope.txt` entry. The invariants
   script enforces the bijection and field validity.

## 6. VALIDATE — the gate set (all green before done)

- `cargo test -p penumbra-sdk-proof-aggregation --lib` — byte baseline,
  determinism (`aggregation_is_deterministic_for_fixed_inputs`), Groth16 oracle
  agreement (`snarkpack_matches_single_and_batch_groth16_oracles`).
- `cargo test -p penumbra-sdk-proof-aggregation-reference --lib` — trace
  baseline, trace equivalence
  (`production_and_reference_traces_match_declared_levels`), input + verifier
  mutation matrices (`*_mutant_matrix_is_declared_per_byte_binding_row`,
  `mutation_matrices_cover_penumbra_byte_trace_rows`).
- `just snarkpack-fuzz-smoke` — 6 targets, zero crashes.
- `just snarkpack-invariants`, `just snarkpack-filecoin-shape`,
  `just snarkpack-formal` — no regression.
- `cargo fmt --all -- --check`.
- A/B delta recorded in the commit/PR description (not a committed threshold —
  fixed perf thresholds and the DoS gate are a later stage).

## 7. LAND OR REVERT

- Category 1, baselines hold, win-or-clarity bar met → land.
- Measured gain is noise and no clarity/scaling case → revert, and say so.
- Baselines moved unexpectedly → it was not the category you thought; stop and
  re-classify before doing anything else.

## 8. Candidate backlog (ranked, grounded in real sites)

Each is a *candidate*, not a commitment — each goes through §3–§4 first.

1. **Merge the two KZG-opening multi-pairings** — the verifier runs two separate
   2-pair `cfg_multi_pairing` checks (`tipa/mod.rs` ~`998`, ~`1021`); a combined
   4-pair check via random linear combination removes one final-exponentiation,
   the single most expensive verify step. *Highest expected end-to-end win.*
2. **Batch-invert the transcript** — serial `.inverse()` maps at `tipa/mod.rs`
   ~`706`/`847` and `structured_scalar_message.rs` ~`435`; use
   `ark_ff::batch_inversion`. Fold in the redundant `c.inverse()` exponent loop
   in `gipa.rs` `_compute_final_commitment_keys` and `r_shift.inverse()`.
3. **MSM-ify `fold_public_inputs` `g_ic`** — hand-rolled multiexponentiation in
   `groth16_aggregation.rs` (~`714`); use `G1::msm`. (`public_input_fold_ms`.)
4. **MSM-ify the shifted-`ck_1` build** — per-element G2 scalar-mul in
   `groth16_aggregation.rs` (~`535`); variable-base MSM on G2. (`backend_ck_1_r_ms`.)
5. **Clone elimination in the GIPA verifier fold** — `.clone()` on `Fp12` each
   round (`gipa.rs` ~`561`); in-place ops. Likely a *clarity* land, not a
   measured win.
6. **`rayon::join` the four independent rescale-folds** per GIPA round
   (`gipa.rs` ~`423`); judge against rayon overhead at small log(n).

Architectural TODOs (`tipa/mod.rs` ~`32`/`161`/`979`,
`structured_scalar_message.rs` ~`25`) are out-of-loop refactors, not
optimization candidates.

## 9. Worked example (the first optimization)

`_compute_final_commitment_keys` recombines the final GIPA commitment keys
(`Σ xᵢ·ck[i]`). The original code was a sequential fold; it now calls the
commitment trait's `msm_keys` (variable-base MSM for AFGHO keys). Equivalence is
proven by `msm_keys_equals_sequential_fold` (both key sides), the byte and trace
baselines pass unchanged (category 1, version 1), and the `bench-baseline` seam
(`fold_keys_baseline`) lets the verify path be A/B-measured. Honest result:
~1–2% end-to-end at n ≤ 64 (near noise) — it earns its place on
correctness/clarity/scaling, not on a dramatic speedup.
