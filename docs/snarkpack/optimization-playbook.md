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

**Byte-stability is necessary, not always sufficient.** A passing byte/trace
baseline proves you did not change the *output*, but it cannot vouch for a change
that weakens a *validation or soundness check* (e.g. batched vs per-element
subgroup checks — §8 candidate 1). Such a change can be byte-stable and still
accept invalid proofs; the gates won't catch it. Any change to how elements are
*validated* needs an explicit security review on top of byte stability.

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

## 0.5 Dynamic-core scaling (hard rule)

Parallelism is encouraged and should *maximize* the cores it is given, but it
must **scale dynamically** with the runtime core count. Never assume or hardcode
a static number of cores, a fixed split, or a machine-specific constant.

Production will run a large, not-yet-fixed core count across many machines, and
the natural parallelism unit is also *across aggregations*. So an intra-op
parallel construct must degrade gracefully — never slower than serial — when
cores are scarce or already busy with other aggregations. `rayon` work-stealing
satisfies this by construction; bespoke fixed-thread schemes do not.

## 0.6 Tunable Parameters Register

Any constant that will need calibration once the production machine architecture
is settled — a parallel-vs-serial `n` threshold, chunk size, `rayon::join`
nesting depth, pool size — must be a **named tunable** recorded here, never a
silent magic number on a parallel path.

| Name | Site | Current value | Rationale | Tune when |
|---|---|---|---|---|
| _(none yet — populate as parallel knobs are introduced or surfaced)_ | | | | hardware known |

## 0.7 Parallelization is deferred, not optimized

The current default parallelization stays as-is. **Do not tune it now.** The
allocation strategy — how a fixed core pool is spent — is settled *later*,
against the benchmark matrix (§10), once the production architecture is known.
The matrix must cover three regimes:

- **Throughput** — many aggregations in parallel, each lean (≈1 thread); cores
  saturated by the workload, so intra-op `rayon` yields ~nothing.
- **Latency** — one aggregation across many cores (intra-op `rayon`); the regime
  the current landed parallel stack was benched in.
- **Hybrid** — bounded intra-op parallelism plus across-aggregation scheduling.

Until then, the value of every intra-op `rayon` change is *regime-conditional and
unsettled*: it is **not** counted toward the optimization bar (§4). That bar is
about work reduction, measured at the work floor (§3).

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

### 3c. The work floor is the per-change metric
Run the A/B at `RAYON_NUM_THREADS=1` as well. This is the **work floor** — it
strips scheduling out and shows whether the change removed real work. A
work-reduction optimization must move the work-floor number; a change whose only
gain appears at >1 thread is *parallelization*, which is deferred to the §10
matrix and is **not** quoted as an optimization win (§0.7).

### 3d. Report the cumulative number, not just the per-change delta
The §3b seam compares against the *immediately preceding* state, which is itself
already optimized — a moving reference. Per-change deltas measured this way do
**not** sum to the real total (§4.5). For any landed change, also record the
**cumulative A/B**: full optimized build vs the pristine *origin* baseline. The
cumulative-vs-origin figure is the headline number.

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

### 4a. The 10% rule — estimate before attempting
Do **not** *attempt* an optimization you estimate below **10%** end-to-end at
realistic counts. Estimate first — FE/Miller-loop accounting or op-count
arithmetic is usually enough — and record the estimate before coding. Pursue only
≥10% candidates. A smaller *measured* result is fine to keep if it lands; a
smaller *estimate* is the reason not to start. Pure correctness/clarity/scaling
refactors are still allowed but are labeled as such, not as performance.

## 4.5 Interactions — optimizations against one another

Two optimizations can work against each other:

- **Site conflict.** Two rewrites of the same loop (e.g. parallelize vs MSM-ify)
  are mutually exclusive — implementing one removes the other's premise, and the
  §3b seam can only gate one at a time. A/B them independently, then pick one.
- **Non-additive stacking.** Stacked parallelism shares one thread pool, so
  per-change deltas measured against a moving reference do **not** sum: the
  cumulative win is not the sum of the per-commit figures, and can be far less
  under a core-saturated workload. For any stack of ≥2 related changes, record a
  final cumulative A/B (§3d). A parallel stack must never claim the sum of its
  per-commit numbers as its real win.

## 5. IMPLEMENT — per-category workflow

### Category 1
1. Keep the pre-optimization implementation as a named `*_baseline` reference.
2. Add a unit equivalence test (`optimized == baseline`) at sizes covering
   **every** code path the optimization touches (cf. the both-sided
   `msm_keys_equals_sequential_fold` in `dh_commitments/src/afgho16/mod.rs`).
3. Wire the §3b A/B seam.
4. Implement; confirm the byte + trace baselines pass **unchanged** and the
   version stays 1.
5. **Baseline lifecycle.** Once landed and validated, the optimized code *is* the
   live baseline going forward — the retained `*_baseline` is only a frozen A/B +
   equivalence artifact, compiled solely under `bench-baseline`. Remove the
   `*_baseline` twin unless it remains a useful equivalence oracle; do not hoard
   dead feature-gated paths. Distinguish the *origin baseline* (pristine,
   pre-optimization) from the *prior-stack reference* (the per-change A/B point):
   the headline number is always vs origin (§3d).

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

Each is a *candidate*, not a commitment — each goes through §3–§4 first, including
the §4a 10% estimate.

**Open, clears the 10% bar — deferred pending a security review (not started):**

1. **Batched GT subgroup validation on deserialization.** The measured #1 verify
   hotspot is `deserialize_ms`, **not** pairings. `AggregateProof`
   (`groth16_aggregation.rs`) carries many GT elements — the four top-level
   (`com_a/b/c`, `ip_ab`) plus the `r_commitment_steps` commitments in both
   `tipa_proof_ab` and `tipa_proof_c`, ~2 per round × log₂n rounds (≈16 at n=8,
   ≈28 at n=64). `deserialize_aggregate_proof` (`backend.rs`) calls
   `deserialize_compressed` (Arkworks `Validate::Yes`), which runs a **full
   GT-subgroup exponentiation per element** — the dominant cost. Replace it with
   decode-`Validate::No` + **one randomized batch subgroup check** over all GT
   elements (random rᵢ, test `Π eᵢ^{rᵢ}` is in 𝔾_T; in-subgroup-iff-all w.h.p.),
   paying 1 exponentiation instead of N. Byte-stable (category 1 in bytes —
   validation is orthogonal to the wire/transcript).

   **Measured (2026-06-01, one-off bench since removed, work floor
   `RAYON_NUM_THREADS=1`):** `deserialize_ms` is the dominant verify stage —
   58% of `core_total` at n=1 rising to **129% at n=64** (i.e. larger than all the
   rest of verify combined). A batched randomized subgroup check with **128-bit**
   verifier-local randomizers runs **~2.2–2.4×** faster than per-element checks
   (~0.9 ms per GT subgroup check). But only ~half of `deserialize_ms` is subgroup
   checking — the rest is un-batchable Fp12 decompression/parsing — and 128-bit
   randomizers cap the batchable part at ~2×. Net projected end-to-end:
   **~13–16% verify** (clears the 10% bar, low end). Note: a smaller randomizer
   (64-bit → ~4×) buys more speed but weakens soundness; that trade is part of the
   security review. *Confirm the exact GT-element count and decompression share
   before committing.*

   **Why it is gated, not just landed.** This weakens a *soundness check*
   (per-element → aggregate-probabilistic), so byte-stability is **necessary but
   not sufficient** — it needs an explicit security review. The review must
   establish, before any code lands:
   - **The error bound from the real BLS12-377 𝔽_{q^12}^× order factorization**
     — the smallest cofactor prime ℓ_min sets the per-round soundness error 1/ℓ_min.
     If ℓ_min is small, one batch round is insufficient (need larger randomizers /
     multiple rounds / per-prime handling). *Compute this number first; it gates
     the whole design.* Analyze the **smallest** N (n=1/2), not the typical n=64.
   - **Randomizer sizing & domain** — error ≤ 2⁻¹²⁸ derived per cofactor prime,
     not assumed from "a random 𝔽_r scalar"; CSPRNG, fresh per verification.
   - **Independence** — rᵢ sampled after the proof bytes are fixed, never derived
     from the proof/transcript (else the prover can grind; also would be a
     forbidden category-3 touch). Assert rᵢ never enter `encode_statement`/
     `challenge_preimage`.
   - **Completeness** — every GT element the per-element path validated is in the
     batch; derive the list from the serialization traversal so a future field
     can't silently escape it. Non-GT fields (`agg_c` in G1, any G2) keep their
     own subgroup checks.
   - **Negative tests are the real proof** — plant a small-cofactor-torsion
     component in *each* element position (top-level and a round commitment in each
     sub-proof) and assert the batch **rejects** every one. If you can't construct
     that test, the cofactor structure isn't understood well enough to ship.
   - **Filecoin-shape interaction** — confirm the audited construction's
     "deserialized elements are subgroup-valid" premise is still discharged, now by
     the batch.

**Evaluated and rejected as sub-bar (do not re-attempt without new evidence):**

- **Batch all verify pairing checks into one randomized multi-pairing** (fold the
  two KZG openings, TIPA-AB/C base cases, and the 3-pair PPE with verifier-local
  γᵢ to remove ~4 of 5 final exponentiations). Implemented and **reverted** —
  regressed or tied across n={1,2,4,8,64}. Pairings are not the bottleneck
  (deserialize is, candidate 1), and this matches the literature: compressed/
  randomized pairing checks are SnarkPack's headline FE-saving trick and still did
  not pay on our shapes.
- Narrow 2-KZG-opening merge (saves 1 FE, ~5–8%) — subset of the above.
- Batch-invert the transcript (`ark_ff::batch_inversion`) — transcript is
  log₂n (≤13) elements; field inversions are negligible vs pairings, <1%.
- MSM-ify `fold_public_inputs` `g_ic` and the shifted-`ck_1` build — one-time,
  sized by public inputs not by n; measured below bar.
- Category-2 uncompressed encoding (slower); reusing prepared `ck_a` in TIPA-AB
  (regressed build ~5–9%).

**Parallelization — deferred to the §10 matrix, not optimizations:** the landed
`rayon` stack (rescale folds, round commits, TIPA proofs, verification checks,
KZG checks) is the default pending the benchmark matrix; do not re-tune it here.

**Landed — prepared verifier-G2 reuse in the PPE (§11):** `verify_ppe` now reuses
`PreparedVerifyingKey`'s `alpha_g1_beta_g2` (GT-exp, −1 Miller loop) and the
prepared `-γ`/`-δ` tables instead of re-pairing the raw `vk` G2 points. Provable
work reduction, equivalence-tested (`ppe_optimized_matches_baseline_gt_value`),
byte/trace-stable category 1. Measured −3% at n=1, noise at larger n; landed under
§4's never-slower clause, not the 10% bar. Subsumes the line-table-only idea below.

**Memory-budget SRS precompute — remaining:**
- *Large fixed-base MSM tables for the SRS generators* (windowed comb/Pippenger,
  tens–hundreds of MB): **deferred to the §10 matrix.** Its payoff is
  memory-bandwidth- and regime-bound and does **not** transfer from a single
  developer machine to the production fleet — see §10a.

Architectural TODOs (`tipa/mod.rs`, `structured_scalar_message.rs`) are
out-of-loop refactors, not optimization candidates.

## 9. Worked example (the first optimization)

`_compute_final_commitment_keys` recombines the final GIPA commitment keys
(`Σ xᵢ·ck[i]`). The original code was a sequential fold; it now calls the
commitment trait's `msm_keys` (variable-base MSM for AFGHO keys). Equivalence is
proven by `msm_keys_equals_sequential_fold` (both key sides), the byte and trace
baselines pass unchanged (category 1, version 1), and the `bench-baseline` seam
(`fold_keys_baseline`) lets the verify path be A/B-measured. Honest result:
~1–2% end-to-end at n ≤ 64 (near noise) — it earns its place on
correctness/clarity/scaling, not on a dramatic speedup.

## 10. Parallelization benchmark matrix (to run later)

Parallelization is *not* tuned through the optimization loop (§0.7). Instead,
once the production machine architecture is settled, run this matrix to choose
the default allocation strategy. Do not change parallelization code before then;
only populate the §0.6 register with the knobs the matrix will sweep.

Axes:

- **Regime:** throughput (N aggregations × 1 thread) vs latency (1 aggregation ×
  M threads) vs hybrid (bounded intra-op + across-aggregation scheduling).
- **Core pool:** sweep available-core counts (e.g. 1, 4, 16, 64, 256) — never a
  static assumption.
- **`n` (proofs per aggregation):** {1, 2, 4, 8, 64}.
- **Intra-op knob:** rayon pool size / max-intra-op-threads, off → full.

Metric: aggregate **throughput** (aggs/sec) *and* per-aggregation **latency**
under a *saturated* workload — not idle-bench wall-clock, which flatters intra-op
parallelism by assuming free spare cores. Output: a recommended default (regime +
knob values) per candidate architecture, with the §0.6 register filled in.

### 10a. Big memory budget — MUST be tested once the architecture is known

A *memory budget* is a fixed, declared RAM cap a precomputation cache is allowed
to consume in exchange for cutting repeated compute: you set the ceiling up front
and size the lookup tables to fit under it. For the SRS this means windowed
fixed-base tables (comb/Pippenger) over the fixed SRS generators, so each
scalar-mul becomes table lookups instead of doublings. Bigger budget → bigger
tables → faster fixed-base MSMs → more RAM per worker.

**This is an open task, not a closed decision.** The large-table version is *not*
benchmarked yet and must not be sized or committed on developer-machine numbers,
because its payoff does not transfer:

- **Memory-bandwidth bound.** Tables trade arithmetic for memory lookups; whether
  that wins depends on the machine's memory bandwidth and cache hierarchy relative
  to its multiplier throughput. The *sign* of the result can flip between a laptop
  (fat caches, high unified-memory bandwidth) and a many-core server (many threads
  contending for shared DRAM) — not just the magnitude.
- **Regime-entangled (ties into §0.7/§10).** Tables cost RAM *per concurrent
  worker*. In the **throughput** regime you either replicate them per worker (RAM
  blows up — the budget genuinely binds) or share and serialize access (the
  speedup evaporates). The budget cannot be chosen before the regime is, and the
  regime is settled here, in the §10 matrix.
- **Saturated, not idle.** The marginal MSM speedup must be measured under a
  saturated workload; an idle laptop bench overstates it.

**Action when production hardware is settled:** add the large fixed-base table to
the §10 matrix as an extra axis — **memory budget** {0 (off), small, large}, swept
against the regime × core-pool × `n` axes, measured under saturated load — and
emit a recommended budget per candidate architecture. Until then this stays
unbenched and uncommitted by design. (Record the chosen budget and table sizing in
the §0.6 register once known.)

## 11. Prepared verifier-G2 reuse in the PPE — LANDED (provable work reduction)

The fixed verifier G2 points the PPE pairs against never change across
verifications, yet the original `verify_ppe` re-prepared and re-paired them every
call. The realized optimization reuses the precomputes **already carried in
`ark_groth16::PreparedVerifyingKey`** instead of caching new line tables — so it
goes further than line-table reuse and removes a whole pairing:

- `e(α·r_sum, β) = e(α, β)^{r_sum}` — `pvk.alpha_g1_beta_g2` raised to `r_sum`, a
  GT exponentiation that **removes one Miller loop** entirely.
- `e(g_ic, γ) = e(-g_ic, -γ)` and `e(agg_c, δ) = e(-agg_c, -δ)` — paired against
  `pvk.{gamma,delta}_g2_neg_pc`, the already-prepared `-γ`/`-δ` line tables, so
  neither γ nor δ is re-prepared (and β is no longer prepared at all).

Net per verify: −1 Miller loop, −3 `G2Prepared::from` builds, +1 GT exponentiation
(cheaper than a Miller loop) + 2 already-prepared tables reused. Strictly less
work for the identical GT element ⇒ **provably never slower** (§4 second clause).

**Category 1, byte- and trace-stable.** The change is verifier-side arithmetic
only: same accept/reject decision, no wire or Fiat-Shamir byte touched, version
unchanged. The reference oracle has its own `verify_ppe` (it does not call
production), and the PPE emits no challenge-trace events, so the PenumbraByte trace
baseline is unaffected.

**Equivalence:** `ppe_optimized_matches_baseline_gt_value`
(`backend.rs` tests) asserts the optimized expression equals the three-pairing
form's GT value over random inputs; end-to-end correctness is additionally gated by
`snarkpack_matches_single_and_batch_groth16_oracles` and the byte/trace baselines.
The pre-optimization three-pairing form is retained as `verify_ppe_baseline`
(compiled only under `bench-baseline`) for the §3b A/B seam.

**A/B (2026-06-01, M4 Pro, work floor `RAYON_NUM_THREADS=1`, `snarkpack verify`):**

| n | optimized | origin baseline | Δ |
|---:|---:|---:|---:|
| 1  | 14.74 ms | 15.21 ms | −3.1% |
| 2  | 33.61 | 33.34 | +0.8% (noise) |
| 4  | 46.66 | 45.96 | +1.5% (noise) |
| 8  | 59.14 | 58.31 | +1.4% (noise) |
| 64 | 96.91 | 100.46 | −3.5% |

The PPE is a fixed-cost stage, so the saving (~0.4–0.5 ms) is near-constant —
visible (~3%) at n=1 and lost in measurement noise where verify is larger and
deserialize-dominated (§8 candidate 1). It does **not** clear §4a's 10% bar and was
not *pursued* under it; it was landed under §4's provable-work-reduction clause
because the work was already done and the change is equivalence-tested and never
slower. The larger lever remains §8 candidate 1.

**Ceiling analysis (superseded).** The earlier line-table-only framing — caching
just the 4 fixed-G2 `G2Prepared::from` builds — had a ceiling of ≤4.1% (n=1) → ~1%
(n=64); see `crates/bench/benches/vanilla/snarkpack_prepared_g2.rs`, which measures
per-`G2Prepared::from` cost vs full verify. The landed PPE reuse subsumes it by
also removing the β Miller loop, not merely the prepares.

