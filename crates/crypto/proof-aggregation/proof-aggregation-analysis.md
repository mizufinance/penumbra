# Proof Aggregation Analysis

Role: this is the builder/proof-aggregation deep dive.
Use [tps-scaling.md](/Users/antoinecyr/Documents/Source/penumbra/compliance-docs/tps-scaling.md) for TPS-wide current state,
[tps-ledger.md](/Users/antoinecyr/Documents/Source/penumbra/compliance-docs/tps-ledger.md) for benchmark history,
and [tps-optimization-register.md](/Users/antoinecyr/Documents/Source/penumbra/compliance-docs/tps-optimization-register.md) for tried builder optimizations.

## Scope

The current question is:

- after the threshold / `join` pass failed to move builder throughput, does static-SRS reuse materially reduce the remaining builder wall?

This pass added:

- deeper `cfg_multi_pairing` stage splits:
  - `normalize_batch`
  - prepared conversion
  - Miller loop
  - final exponentiation
- KZG opening stage splits:
  - coefficient build
  - eval / quotient construction
  - opening MSM
- a proving-only `PreparedProvingSrs` cache for static SRS bases
- a local audit of Arkworks MSM dispatch for the proving sizes on this path

The measured case stayed the same:

- warmed strict `1k` builder one-shot
- `1` cold build ignored
- next `3` builds measured
- local `14`-core machine

## Builder-Level Context

The previous retained warmed strict `1k` builder anchor on this branch was:

- `build_wall_ms ~= 1337.92`
- `aggregate_total_ms ~= 1311.87`
- about `747.4 tx/s`

After the retained `tipa_ab` dynamic `G2Prepared` reuse pass, the current warmed strict `1k` builder result is:

- `selected_tx_count = 1000`
- `build_wall_ms ~= 1346.78`
- `aggregate_total_ms ~= 1321.05`
- about `742.5 tx/s`

So this pass does produce a retained end-to-end builder win on the current branch.
Builder is still the active isolated-stage wall.

Important interpretation:

- the one-time prepared-SRS construction is still cheap
- static-base reuse remains structurally correct but secondary
- the new retained win came from reducing duplicate dynamic `G2Prepared` work inside `tipa_ab` rounds
- the remaining builder wall is still dominated by dynamic GIPA commitment math, but the duplicated `G2` preparation tax was real and worth removing

## Dynamic `G2Prepared` Reuse Outcome

What happened in the current warmed strict `1k` rerun:

- `aggregate_backend_prepared_srs_ms ~= 11.08`
- `aggregate_backend_commitment_key_extract_ms ~= 0.00`
- `aggregate_backend_commitment_ms ~= 713.58`
- `aggregate_backend_tipa_ab_ms ~= 2680.01`
- `aggregate_backend_tipa_c_ms ~= 1054.48`
- `aggregate_backend_pairing_prepare_ms ~= 627.67`

What that means:

1. prepared-SRS construction is still not a large wall
2. commitment-key extraction overhead remains effectively zero
3. duplicate dynamic `G2Prepared` work inside `tipa_ab` was large enough to move the retained warm builder anchor
4. the remaining backend cost is still overwhelmingly in dynamic prover work, not static SRS preparation

## Top-Level Backend Breakdown

For the current warmed strict `1k` rerun, the dominant top-level backend buckets were:

| bucket | approximate share of backend-core sum | interpretation |
| --- | --- | --- |
| `tipa_ab` | about `51.5%` | main TIPA proof construction for the `ab` side |
| `tipa_c` | about `20.2%` | TIPA proof construction for the `c` side |
| `commitment` | about `13.7%` | initial pairing commitments (`com_a`, `com_b`, `com_c`) |
| `weighted_a` | about `5.7%` | weighted `a_r` construction |
| `ip_ab` | about `4.4%` | inner-product setup for `ab` |
| `ck_1_r` | about `3.6%` | shifted commitment-key construction |
| `agg_c` | about `1.0%` | aggregate `c` path |
| `prepared_srs` | about `0.2%` | one-time static SRS preparation |
| `consistency_check` | `0.0%` | compiled out in release |

Two conclusions follow:

1. static SRS preparation is not the stage wall
2. the remaining wall is still concentrated in TIPA, then outer commitments, then the other backend setup buckets

## Commitment and Pairing Split

The outer `commitment_ms` bucket now splits as:

- `com_a_ms ~= 305.88`
- `com_b_ms ~= 217.21`
- `com_c_ms ~= 190.50`

The full build-path `cfg_multi_pairing` stage accumulator now reports:

- `pairing_normalize_batch_ms ~= 329.60`
- `pairing_prepare_ms ~= 627.67`
- `pairing_miller_loop_ms ~= 1453.08`
- `pairing_final_exponentiation_ms ~= 262.92`

Interpretation:

- the pairing helper is doing real work in all four stages
- Miller loops remain the largest single pairing stage
- dynamic `G2Prepared` reuse cut the prepare stage hard and also reduced normalize work materially
- Miller loops are now even more clearly the remaining dominant pairing stage

## Deep TIPA / GIPA Breakdown

For `tipa_ab`:

- `tipa_ab_gipa_ms ~= 2612.67`, about `97.5%` of `tipa_ab`
- `tipa_ab_transcript_inverse_ms ~= 0.06`
- `tipa_ab_kzg_challenge_ms ~= 0.07`
- `tipa_ab_kzg_coefficient_build_ms ~= 0.15`
- `tipa_ab_kzg_eval_quotient_ms ~= 0.73`
- `tipa_ab_kzg_opening_msm_ms ~= 66.24`
- `tipa_ab_kzg_opening_ck_a_ms ~= 46.23`
- `tipa_ab_kzg_opening_ck_b_ms ~= 20.97`

For `tipa_c`:

- `tipa_c_gipa_ms ~= 1015.06`, about `96.3%` of `tipa_c`
- `tipa_c_transcript_inverse_ms ~= 1.85`
- `tipa_c_kzg_challenge_ms ~= 0.04`
- `tipa_c_kzg_coefficient_build_ms ~= 0.07`
- `tipa_c_kzg_eval_quotient_ms ~= 0.39`
- `tipa_c_kzg_opening_msm_ms ~= 36.99`
- `tipa_c_kzg_opening_ck_a_ms ~= 37.53`

This keeps the previous conclusion intact:

- transcript inversions are negligible
- transcript challenge generation is negligible
- KZG coefficient and quotient work are negligible
- KZG opening MSM is real, but still secondary to GIPA itself

## Inside GIPA

Across combined `tipa_ab_gipa + tipa_c_gipa`:

- commitment work (`commit_l + commit_r`) is still the largest family by a wide margin
- rescale work remains secondary
- GIPA challenge work is effectively noise

The ordering still does not change:

1. dynamic GIPA commitment construction is still the wall
2. GIPA rescale work is secondary
3. KZG opening MSM is tertiary
4. transcript work is noise

So the next optimization target is still dynamic commitment math, not static SRS reuse and not transcript work.

## Arkworks MSM Audit

The local `ark-ec 0.4.2` sources were checked directly.

Relevant facts:

- `CurveGroup::msm(...)` routes to `VariableBaseMSM::msm_unchecked(...)`
- `VariableBaseMSM::msm_bigint(...)` chooses `msm_bigint_wnaf(...)` when `NEGATION_IS_CHEAP`
- short-Weierstrass groups set `NEGATION_IS_CHEAP = true`

That means the current proving-size MSMs on this path use the Arkworks WNAF-backed variable-base MSM path, not a custom stream-Pippenger path.

Practical implication:

- before considering any MSM-backend rewrite, the next cycle should first reduce dynamic GIPA commitment work structurally
- if a later cycle still lands on MSM kernel limits after that structural cleanup, then an Arkworks-level MSM experiment becomes easier to justify

## What This Means For The Next Pass

The next meaningful optimization work should stay inside the proof-aggregation backend.

The measurements now support these decisions:

1. Do not spend the next cycle on more static-SRS caching work.

- the per-prove cache is cheap
- it did not move builder throughput materially
- it can remain as an internal proving primitive, but it is not the next stage win

2. Do not spend the next cycle on transcript changes.

- transcript inverse, challenge, coefficient-build, and quotient buckets are too small

3. The next cycle should go directly into dynamic GIPA commitment math.

- inspect `commit_l` / `commit_r` and the commitment-helper path they call
- focus on reducing repeated dynamic normalize / prepare / Miller-loop work on folded vectors and keys
- treat KZG opening MSM as the secondary target only after that

4. Do not return to Rayon threshold tuning or `rayon::join` experiments yet.

- those scheduling experiments have already failed to move the retained branch materially

## Practical Next Steps

The next optimization pass should be:

1. inspect dynamic GIPA commitment construction in the vendored commitment helpers and folded-vector path
2. determine whether the remaining dynamic `G1` reuse or other round-local commitment reuse is worth a final low-risk software pass
3. treat KZG opening MSM as a secondary cleanup item, not the primary wall
4. rerun the same warmed strict `1k` builder benchmark after the dynamic commitment pass

## Rejected Experiment

One `tipa_ab`-only experiment was implemented and measured but not retained:

- fused `fold -> batch-normalize -> affine-shadow` round state for the pairing-heavy `tipa_ab` path
- direct affine pairing helper entrypoint so the next round could skip repeated dynamic normalization

Why it was rejected:

- the proof path remained test-correct
- but the warmed strict `1k` builder rerun regressed to:
  - `build_wall_ms ~= 1412.92`
  - `aggregate_total_ms ~= 1387.13`
- that is worse than the retained anchor (`1376.82 / 1350.94`), so the branch keeps the pre-experiment prover path

Interpretation:

- simply moving normalization into the fold step did not reduce enough dynamic commitment cost to offset the extra round-transition work
- the next dynamic-commitment pass should therefore target the commitment helper structure itself more directly, not just when affine conversion happens

## What This Means For Builder Throughput

The builder frontier is still set by proof aggregation.

These passes answered two questions clearly:

- static-SRS reuse is not enough to raise the builder frontier on its own
- dynamic `G2Prepared` reuse inside `tipa_ab` does help and is worth retaining on the current branch

The next raw builder-speed wins should come from:

- either one final low-risk dynamic commitment reuse pass on the `G1` side
- or, if that is judged too marginal, a structural shift away from pure CPU-bound pairing cost
- only after that, any MSM-backend experiment justified by the new post-fix measurements
- more cores or more cadence slack if the goal is simply to absorb the current wall operationally

## Android Proof Generation Follow-Up

Role: this section is about on-device single-proof latency for regulated `spend` and `output`,
not builder aggregation.

### Current Android Sweep

Device: Samsung `SM-G781W`, Android `13`.
Binary: `tools/android-proof-lab`, strict valid proofs only.

Retained Arkworks `0.4` baseline:

- `spend` best observed warm result: `4381.94 ms` at `RAYON_NUM_THREADS=8`
- `output` best observed warm result: `7316.71 ms` at `RAYON_NUM_THREADS=12`
- Rayon auto/default was materially worse than explicit tuning on this device for both circuits

Completed Arkworks `0.5` sweep:

- `spend` best observed warm result: `4334.01 ms` at `RAYON_NUM_THREADS=12`
- `output` best observed warm result: `6769.60 ms` at `RAYON_NUM_THREADS=10`
- Rayon auto/default was still worse than explicit tuning, but less catastrophically so on `output`

The `0.5` sweep artifacts are at:

- [tmp/android-sweeps-0_5/spend](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps-0_5/spend)
- [tmp/android-sweeps-0_5/output](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps-0_5/output)

Important interpretation:

- there is no single fixed thread count to ship blindly across Android
- explicit thread tuning is worth keeping in the benchmark harness
- the current proving wall is inside `create_proof_ms`, not key load or serialization
- the Arkworks `0.5` migration is effectively flat on Android: `spend` improved by about `1.1%`, `output` by about `7.5%`

### Android `simpleperf` Findings

Retained Arkworks `0.4` symbolized `simpleperf` runs were taken with:

- `spend`, `RAYON_NUM_THREADS=8`
- `output`, `RAYON_NUM_THREADS=12`

The top samples in both reports are dominated by:

- `decaf377::fields::fp::u64::wrapper::Fp::mul`
- then `Fp::square`
- then smaller shares in `Fq::mul`, projective group arithmetic, and constraint-system plumbing

The reports are at:

- [spend-t8.perf.report.txt](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps/spend/spend-t8.perf.report.txt)
- [output-t12.perf.report.txt](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps/output/output-t12.perf.report.txt)

Fresh Arkworks `0.5` symbolized `simpleperf` runs were taken with the new best settings:

- `spend`, `RAYON_NUM_THREADS=12`
- `output`, `RAYON_NUM_THREADS=10`

The `0.5` reports are at:

- [spend-t12.perf.report.txt](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps-0_5/spend/spend-t12.perf.report.txt)
- [output-t10.perf.report.txt](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps-0_5/output/output-t10.perf.report.txt)

What this means:

- the device is spending its cycles overwhelmingly in finite-field arithmetic during Groth16 proving
- this is consistent with MSM-heavy prover work
- but this profile alone does not prove that variable-base MSM is the only dominant kernel, because Arkworks Groth16 also spends heavy time in other field-arithmetic-intensive stages
- the migration to Arkworks `0.5` did not materially change the profile shape; summing the `Fp::mul` rows in the `0.5` reports still yields about `43.68%` of sampled cycles for `spend` and `44.96%` for `output`

So the right conclusion is:

- the prover wall is still arithmetic-heavy after the `0.5` cutover
- `arkmsm` was justified as a bounded experiment
- but it should be treated as an experiment against a measured arithmetic-heavy prover wall, not as a proven silver bullet

### `arkmsm` Spike Outcome

A dedicated benchmark-only binary was added at:

- [arkmsm_spike.rs](/Users/antoinecyr/Documents/Source/penumbra/tools/android-proof-lab/src/bin/arkmsm_spike.rs)

What it measures:

- the real Groth16 proving-key `G1` query vectors used by regulated `spend` and `output`
- `a_query`, `b_g1_query`, `h_query`, and `l_query`
- three MSM engines on the same inputs:
  - current Arkworks `0.4`
  - Arkworks `0.3` baseline
  - `arkmsm 0.3.0-alpha.1`

Correctness status:

- all compared outputs matched across the three engines on both host and Android

Host result:

- `arkmsm` beat the Arkworks `0.3` baseline by about `1.63x` to `1.67x` on `spend`
- but current Arkworks `0.4` was still far faster than both

Android result on Samsung `SM-G781W`:

- `arkmsm` beat Arkworks `0.3` by about `1.30x` to `1.49x`
- but current Arkworks `0.4` still beat `arkmsm` by about `2.0x` to `2.9x`

Artifacts:

- host `spend` run:
  - [arkmsm-spike-host-spend.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/arkmsm-spike-host-spend.json)
- Android full run:
  - [arkmsm-spike-android.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps/arkmsm/arkmsm-spike-android.json)

Practical conclusion:

- `arkmsm` is a real improvement over older Arkworks `0.3` MSM
- but it is not competitive with the current Arkworks `0.4` MSM path used by Penumbra today
- so this spike is rejected as a next-step Android prover optimization for the current codebase

### IMP1 / ICICLE Compatibility Check

Current Penumbra proving uses:

- Arkworks Groth16
- `Bls12_377`
- serialized Arkworks `ProvingKey<Bls12_377>` assets

The current mobile-friendly IMP1 / ICICLE interfaces that were checked expect:

- witness files
- `zkey` proving-key files
- proof/public output files

That is a Circom / `snarkjs`-style proving surface, not the current Penumbra asset format.

Practical conclusion:

- IMP1 / ICICLE is not a drop-in replacement for the current Android proving path
- adopting it would require either:
  - a new proving-artifact pipeline that emits compatible witness / `zkey` assets
  - or a lower-level integration into the existing Arkworks / `decaf377` prover internals

### Option A Feasibility: ICICLE Kernel Integration On The Current Arkworks Prover

Role: this section evaluates the bounded "Option A" path:
keep the current Arkworks `0.5` Groth16 prover and try to replace only the
dominant arithmetic kernels with ICICLE, without changing circuits, witness
generation, proving-key format, or verifier logic.

Current target to beat:

- serialized Android `2 spend + 2 output` warm baseline on Samsung `SM-G781W`:
  `23061.50 ms`
- retained artifact:
  [spend2_output2_serial.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/android-sweeps-0_5/spend2_output2_serial.json)
- target ladder:
  - `3x` ideal
  - `2x` good
  - `1.5x` acceptable

#### Phase 0: Backend-Injection Feasibility

The current Penumbra proving call path is:

- `SpendProof::prove` / `OutputProof::prove`
- `Groth16::<Bls12_377, LibsnarkReduction>::create_proof_with_reduction(...)`
- Arkworks `create_proof_with_assignment(...)`
- `E::G1::msm_bigint(...)` and `calculate_coeff(...)`
- `VariableBaseMSM::msm_bigint(...)`

The important local files are:

- [spend/proof.rs](/Users/antoinecyr/Documents/Source/penumbra/crates/core/component/shielded-pool/src/spend/proof.rs)
- [output/proof.rs](/Users/antoinecyr/Documents/Source/penumbra/crates/core/component/shielded-pool/src/output/proof.rs)

The important upstream `0.5` files are:

- [`ark-groth16` prover.rs](/Users/antoinecyr/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ark-groth16-0.5.0/src/prover.rs)
- [`ark-ec` variable-base MSM](/Users/antoinecyr/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ark-ec-0.5.0/src/scalar_mul/variable_base/mod.rs)

Verdict:

- there is no clean Penumbra-local backend plug point today
- the realistic seam is an Arkworks fork or patch, not a benchmark-only feature flag
- the likely options are:
  - fork `ark-groth16`
  - patch `ark-ec`
  - or replace the relevant group/backend types

So Phase 0 fails the "bounded integration seam" test.
Option A is not a clean backend swap on the current stack.

#### Phase 1: Reachability Estimate From Existing Data

This estimate uses only the retained `0.5` stage-profile and `simpleperf`
artifacts. No new instrumentation was needed.

Stage timing says almost all warm proof time is already inside
`create_proof_ms`:

- `spend`: `4333.53 / 4334.01 ms`
- `output`: `6768.86 / 6769.60 ms`

Flat `simpleperf` shares on the retained `0.5` best-thread runs:

| bucket | `spend` | `output` | interpretation |
| --- | --- | --- | --- |
| `Fp::mul` | `43.68%` | `44.96%` | dominant arithmetic bucket |
| `EqGadget::enforce_equal` | `11.57%` | `11.49%` | constraint-synthesis / witness-side cost |
| `FqVarExtension::abs` | `3.61%` | `3.99%` | decaf377-specific gadget cost |
| `Fq::mul` | `2.77%` | `3.10%` | secondary arithmetic bucket |
| visible radix-2 FFT / NTT symbol | `0.08%` | `0.11%` | not a dominant visible flat bucket |

Interpretation:

- the absolute upper bound from the existing profile is already too low for the
  `3x` target
- even if every sampled `Fp::mul` cycle disappeared, the flat-profile ceiling is
  only about:
  - `1.78x` for `spend`
  - `1.82x` for `output`
- ICICLE would only reach the subset of those arithmetic cycles that are
  actually inside replaceable MSM / NTT kernels
- visible FFT / NTT symbols are tiny in the retained flat reports, especially
  compared with constraint-synthesis and generic field work

So Phase 1 fails the Amdahl's-law test for a `3x` CPU-path target.

#### Phase 2: ICICLE Capability Validation

Official ICICLE support checked from the public docs:

| capability | verdict | source interpretation |
| --- | --- | --- |
| `BLS12-377` MSM | supported | ICICLE libraries and Rust MSM docs list `bls12-377` |
| `BLS12-377` G2 MSM | supported | ICICLE libraries docs list `G2 MSM` for `bls12-377` |
| `BLS12-377` NTT | supported | ICICLE libraries and Rust NTT docs list `bls12-377` |
| `BLS12-377` ECNTT | supported | ICICLE libraries and ECNTT docs list `bls12-377` |
| CPU backend on 64-bit CPU | supported | ICICLE docs say the built-in CPU backend is available and any 64-bit CPU is supported |
| Android CPU backend | unclear / inferred | no explicit public ICICLE doc was found for Android CPU deployment; this is inferred from `aarch64` CPU support |
| GPU backend | available but irrelevant to this pass | GPU backends are separately installed and licensed; not part of the CPU-first Option A path |

This means ICICLE capabilities are not the main blocker.
The blockers are the Arkworks seam and reachable speedup ceiling.

#### Phase 3: Benchmark-Only ICICLE CPU Spike

The bounded spike was implemented under:

- [icicle_cpu_spike.rs](/Users/antoinecyr/Documents/Source/penumbra/tools/android-proof-lab/src/bin/icicle_cpu_spike.rs)

The spike benchmarks real Penumbra `BLS12-377` `G1` proving-key query vectors:

- `a_query`
- `b_g1_query`
- `h_query`
- `l_query`

for `spend` and `output`, comparing:

- Arkworks `0.5`
- pure ICICLE MSM kernel time
- ICICLE with cached converted bases
- ICICLE with full scalar and base conversion

The Android build required a local vendored ICICLE patch to make the Rust/CMake
build work under `cargo ndk`:

- [vendor/icicle](/Users/antoinecyr/Documents/Source/penumbra/vendor/icicle)

Host artifacts:

- [tmp/icicle-cpu-spike-host.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/icicle-cpu-spike-host.json)

Host result summary:

- pure ICICLE kernel: `1.25x` to `1.92x`, average about `1.50x`
- cached-base path: `0.92x` to `1.48x`, average about `1.16x`
- full-conversion path: `0.62x` to `0.76x`, average about `0.68x`
- correctness matched for all measured query families

Android artifacts:

- [tmp/icicle-cpu-spike-android.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/icicle-cpu-spike-android.json)
- [tmp/icicle-output-h-3-android.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/icicle-output-h-3-android.json)
- [tmp/icicle-output-l-3-android.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/icicle-output-l-3-android.json)

Android full-sweep summary on the Samsung `SM_G781W`:

- pure ICICLE kernel: `0.17x` to `1.24x`, average about `0.71x`
- cached-base path: `0.43x` to `1.71x`, average about `0.78x`
- full-conversion path: `0.49x` to `1.49x`, average about `0.74x`
- correctness matched for all measured query families

Focused Android reruns with `3` measured iterations on the most relevant
`output` queries:

- `output l_query`: cached-base path about `1.13x`
  - [tmp/icicle-output-l-3-android.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/icicle-output-l-3-android.json)
- `output h_query`: cached-base path about `0.52x`
  - [tmp/icicle-output-h-3-android.json](/Users/antoinecyr/Documents/Source/penumbra/tmp/icicle-output-h-3-android.json)

So the host-side kernel win does not translate cleanly to Android CPU on this
device. There are isolated query-family wins, but not a general or convincing
end-to-end improvement.

#### Final Verdict For Option A

Option A is rejected as the next Android prover acceleration path.

Why:

1. the current Arkworks `0.5` prover still does not expose a bounded backend-injection seam
2. the implemented spike shows that host-side ICICLE CPU wins do not reliably carry over to Android CPU
3. the Android cached-base path is not a consistent `>= 1.5x` win, and the focused reruns show the apparent gains are query-family specific rather than general

What this means operationally:

- do not spend the next cycle on CPU-only ICICLE kernel integration into the
  current Arkworks prover
- keep the spike results as evidence that Android CPU behavior is materially
  worse than host behavior for this backend
- if ICICLE remains interesting, the next variant to investigate is GPU-backed
  integration, not more CPU-only work
