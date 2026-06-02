//! Small memory-budget sub-case bench: prepared verifier-G2 line-table reuse.
//!
//! Measures the *ceiling* on caching `G2Prepared::from` for the fixed verifier
//! G2 points (`vk.{beta,gamma,delta}_g2` + `v_srs.h`, 4 points) that every
//! verify currently rebuilds. Ceiling = `4 * t_prepare / t_verify`; reuse can
//! never beat it. See optimization-playbook.md §11.
//!
//! Harness-free (`harness = false`): manual `Instant` timing, prints a table.
//! Run: `cargo run --release -p penumbra-sdk-bench --bench snarkpack_prepared_g2`
//! Work floor: prefix with `RAYON_NUM_THREADS=1`.

use ark_ec::pairing::Pairing;
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use ark_std::UniformRand;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_aggregation::{
    aggregate_family, pad_items_to_power_of_two, srs_id, verify_family_aggregate,
    AggregateStatement, DevSrs, ProofFamilyId, AGGREGATE_PROTOCOL_VERSION,
};
use penumbra_sdk_proof_params::batch::BatchItem;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use std::path::PathBuf;
use std::time::Instant;

type G2 = <Bls12_377 as Pairing>::G2;
type G2Prepared = <Bls12_377 as Pairing>::G2Prepared;

/// Number of fixed verifier-G2 points whose `G2Prepared::from` line tables a
/// memory budget could cache once: `vk.{beta,gamma,delta}_g2` (PPE) + `v_srs.h`
/// (g1-side KZG). Grounded in code inspection — see playbook §11.
const FIXED_G2_PREPARES_PER_VERIFY: usize = 4;

#[derive(Clone)]
struct SquareCircuit {
    x: Option<Fq>,
}

impl ConstraintSynthesizer<Fq> for SquareCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> Result<(), SynthesisError> {
        let x = FpVar::new_witness(cs.clone(), || {
            self.x.ok_or(SynthesisError::AssignmentMissing)
        })?;
        let x_sq = &x * &x;
        let public = FpVar::new_input(cs, || {
            let x = self.x.ok_or(SynthesisError::AssignmentMissing)?;
            Ok(x * x)
        })?;
        x_sq.enforce_equal(&public)?;
        Ok(())
    }
}

/// Shared committed Groth16 proof corpus (same dir as the `snarkpack` bench).
fn corpus_path(count: usize) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/corpus/snarkpack"))
        .join(format!("items_{count}.bin"))
}

fn load_or_generate_items(count: usize) -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
    let path = corpus_path(count);
    if let Ok(bytes) = std::fs::read(&path) {
        let mut cur = &bytes[..];
        let pvk = PreparedVerifyingKey::<Bls12_377>::deserialize_uncompressed_unchecked(&mut cur)
            .expect("corpus pvk decodes");
        let raw = Vec::<(Proof<Bls12_377>, Vec<Fq>)>::deserialize_uncompressed_unchecked(&mut cur)
            .expect("corpus items decode");
        let items = raw
            .into_iter()
            .map(|(proof, public_inputs)| BatchItem {
                proof,
                public_inputs,
            })
            .collect();
        return (pvk, items);
    }

    let mut rng = ChaCha20Rng::seed_from_u64((count as u64) + 9);
    let pk = Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
        SquareCircuit {
            x: Some(Fq::from(1u64)),
        },
        &mut rng,
    )
    .expect("setup");
    let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();
    let items: Vec<BatchItem> = (0..count)
        .map(|_| {
            let x = Fq::rand(&mut rng);
            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(
                &pk,
                SquareCircuit { x: Some(x) },
                &mut rng,
            )
            .expect("prove");
            BatchItem {
                proof,
                public_inputs: vec![x * x],
            }
        })
        .collect();

    let mut buf = Vec::new();
    pvk.serialize_uncompressed(&mut buf).expect("pvk ser");
    let raw: Vec<(Proof<Bls12_377>, Vec<Fq>)> = items
        .iter()
        .map(|i| (i.proof.clone(), i.public_inputs.clone()))
        .collect();
    raw.serialize_uncompressed(&mut buf).expect("items ser");
    std::fs::create_dir_all(path.parent().unwrap()).expect("corpus dir");
    std::fs::write(&path, buf).expect("persist corpus");
    (pvk, items)
}

struct Fixture {
    count: usize,
    pvk: PreparedVerifyingKey<Bls12_377>,
    statement: AggregateStatement,
    aggregate_proof: Vec<u8>,
}

fn build_fixture(count: usize, srs: &DevSrs) -> Fixture {
    let (pvk, items) = load_or_generate_items(count);
    let padded_items =
        pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
    let padded_public_inputs = padded_items
        .iter()
        .map(|item| item.public_inputs.clone())
        .collect::<Vec<_>>();
    let statement = AggregateStatement::new(
        AGGREGATE_PROTOCOL_VERSION,
        ProofFamilyId::Transfer,
        srs_id(srs),
        &pvk,
        count as u32,
        &padded_public_inputs,
    )
    .expect("statement");
    let aggregate_proof =
        aggregate_family(&statement, &pvk, &padded_items, srs).expect("aggregate");
    Fixture {
        count,
        pvk,
        statement,
        aggregate_proof,
    }
}

/// Median of `iters` timed runs of `f`, in milliseconds.
fn median_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    let mut samples: Vec<f64> = (0..iters)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

fn main() {
    let threads = std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "default".into());
    println!("snarkpack prepared-G2 ceiling bench (RAYON_NUM_THREADS={threads})\n");

    // Per-G2Prepared::from cost. Point-independent in cost, so a random G2 is a
    // faithful stand-in for the fixed verifier points.
    let mut rng = ChaCha20Rng::seed_from_u64(1);
    let points: Vec<G2> = (0..1024).map(|_| G2::rand(&mut rng)).collect();
    // warmup
    for p in &points {
        std::hint::black_box(G2Prepared::from(*p));
    }
    let prep_iters = 50;
    let total = median_ms(prep_iters, || {
        for p in &points {
            std::hint::black_box(G2Prepared::from(*p));
        }
    });
    let t_prepare_ms = total / points.len() as f64;
    println!("per-G2Prepared::from : {:.6} ms", t_prepare_ms);
    println!(
        "fixed prepares/verify : {FIXED_G2_PREPARES_PER_VERIFY}  -> cacheable {:.6} ms/verify\n",
        t_prepare_ms * FIXED_G2_PREPARES_PER_VERIFY as f64
    );

    let srs = DevSrs::default();
    let counts = [1usize, 2, 4, 8, 64, 1024, 2048];

    println!(
        "{:>4}  {:>12}  {:>14}  {:>10}",
        "n", "verify (ms)", "cacheable (ms)", "ceiling %"
    );
    for count in counts {
        let fx = build_fixture(count, &srs);
        // warmup
        verify_family_aggregate(&fx.statement, &fx.pvk, &fx.aggregate_proof, &srs).expect("verify");
        let t_verify = median_ms(25, || {
            verify_family_aggregate(&fx.statement, &fx.pvk, &fx.aggregate_proof, &srs)
                .expect("verify");
        });
        let cacheable = t_prepare_ms * FIXED_G2_PREPARES_PER_VERIFY as f64;
        let ceiling_pct = 100.0 * cacheable / t_verify;
        println!(
            "{:>4}  {:>12.3}  {:>14.4}  {:>9.3}%",
            fx.count, t_verify, cacheable, ceiling_pct
        );
    }
    println!(
        "\nCeiling = upper bound on prepared-G2 reuse (line-table builds removed).\n\
         Real win is strictly less. Per playbook §4a/§11 this is opportunistic only."
    );
}
