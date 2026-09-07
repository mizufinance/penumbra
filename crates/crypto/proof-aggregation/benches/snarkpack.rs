use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use decaf377::{Bls12_377, Fq};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use shieldd_sdk_proof_aggregation::{
    aggregate_family, aggregate_family_torus, pad_items_to_power_of_two, srs_id,
    verify_family_aggregate, verify_family_aggregate_torus, AggregateStatement, DevSrs,
    ProofFamilyId, AGGREGATE_PROTOCOL_VERSION,
};
use shieldd_sdk_proof_params::batch::BatchItem;
use shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId;
use std::env;
use std::path::PathBuf;

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

#[derive(Clone)]
struct Fixture {
    family_id: ProofFamilyId,
    count: usize,
    pvk: PreparedVerifyingKey<Bls12_377>,
    padded_items: Vec<BatchItem>,
    statement: AggregateStatement,
    aggregate_proof: Vec<u8>,
    torus_aggregate_proof: Vec<u8>,
}

/// Committed Groth16 proof corpus directory. The expensive setup (proving-key
/// generation + per-proof proving) runs once, is serialized here, and every
/// subsequent benchmark run loads from disk instead of re-proving. Keyed by
/// `count` only — the `SquareCircuit` proving key and proofs do not depend on
/// the aggregation family (the family only selects the transcript domain).
fn corpus_path(count: usize) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bench/corpus/snarkpack"
    ))
    .join(format!("items_{count}.bin"))
}

/// Load the proof corpus for `count` from disk, generating and persisting it on
/// first use.
fn load_or_generate_items(count: usize) -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
    let path = corpus_path(count);
    if let Ok(bytes) = std::fs::read(&path) {
        let mut cur = &bytes[..];
        let pvk = PreparedVerifyingKey::<Bls12_377>::deserialize_uncompressed(&mut cur)
            .expect("corpus pvk checked decode");
        let raw = Vec::<(Proof<Bls12_377>, Vec<Fq>)>::deserialize_uncompressed(&mut cur)
            .expect("corpus items checked decode");
        let items = raw
            .into_iter()
            .map(|(proof, public_inputs)| BatchItem {
                proof,
                public_inputs,
            })
            .collect();
        return (pvk, items);
    }

    let (pvk, items) = generate_items(count);
    let mut buf = Vec::new();
    pvk.serialize_uncompressed(&mut buf)
        .expect("pvk serializes");
    let raw: Vec<(Proof<Bls12_377>, Vec<Fq>)> = items
        .iter()
        .map(|item| (item.proof.clone(), item.public_inputs.clone()))
        .collect();
    raw.serialize_uncompressed(&mut buf)
        .expect("items serialize");
    std::fs::create_dir_all(path.parent().expect("corpus dir has parent"))
        .expect("create corpus dir");
    std::fs::write(&path, buf).expect("persist corpus");
    (pvk, items)
}

fn generate_items(count: usize) -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
    let mut rng = ChaCha20Rng::seed_from_u64((count as u64) + 9);
    let setup_circuit = SquareCircuit {
        x: Some(Fq::from(1u64)),
    };
    let pk = Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
        setup_circuit,
        &mut rng,
    )
    .expect("setup should succeed");
    let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();

    let items = (0..count)
        .map(|_| {
            let x = Fq::rand(&mut rng);
            let circuit = SquareCircuit { x: Some(x) };
            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                .expect("proof generation should succeed");

            BatchItem {
                proof,
                public_inputs: vec![x * x],
            }
        })
        .collect();

    (pvk, items)
}

fn build_fixture(family_id: ProofFamilyId, count: usize, srs: &DevSrs) -> Fixture {
    let (pvk, items) = load_or_generate_items(count);
    let padded_items =
        pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
    let padded_public_inputs = padded_items
        .iter()
        .map(|item| item.public_inputs.clone())
        .collect::<Vec<_>>();
    let statement = AggregateStatement::new(
        AGGREGATE_PROTOCOL_VERSION,
        family_id,
        srs_id(srs),
        &pvk,
        count as u32,
        &padded_public_inputs,
    )
    .expect("statement builds");
    let aggregate_proof =
        aggregate_family(&statement, &pvk, &padded_items, srs).expect("aggregation succeeds");
    let torus_aggregate_proof =
        aggregate_family_torus(&statement, &padded_items, srs).expect("aggregation succeeds");

    Fixture {
        family_id,
        count,
        pvk,
        padded_items,
        statement,
        aggregate_proof,
        torus_aggregate_proof,
    }
}

fn benchmark_counts() -> Vec<usize> {
    match env::var("SNARKPACK_BENCH_COUNTS") {
        Ok(value) => {
            let counts = value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(|part| {
                    part.parse::<usize>()
                        .expect("SNARKPACK_BENCH_COUNTS entries must be positive integers")
                })
                .collect::<Vec<_>>();
            assert!(
                !counts.is_empty() && counts.iter().all(|count| *count > 0),
                "SNARKPACK_BENCH_COUNTS must contain at least one positive count"
            );
            counts
        }
        Err(_) => vec![1, 2, 4, 8, 48, 64],
    }
}

fn snarkpack_bench(c: &mut Criterion) {
    let srs = DevSrs::default();
    let counts = benchmark_counts();
    let families = if env::var_os("SNARKPACK_BENCH_TRANSFER_ONLY").is_some() {
        vec![ProofFamilyId::Transfer]
    } else {
        vec![
            ProofFamilyId::Transfer,
            ProofFamilyId::NoteReshape(shieldd_sdk_shielded_pool::NOTE_RESHAPE_FAMILY_SPECS[0].id),
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
        ]
    };

    let fixtures: Vec<_> = families
        .into_iter()
        .flat_map(|family_id| counts.iter().copied().map(move |count| (family_id, count)))
        .map(|(family_id, count)| build_fixture(family_id, count, &srs))
        .collect();

    let mut aggregate_group = c.benchmark_group("snarkpack aggregate");
    for fixture in &fixtures {
        aggregate_group.bench_with_input(
            BenchmarkId::new(format!("{:?}", fixture.family_id), fixture.count),
            fixture,
            |b, fixture| {
                b.iter(|| {
                    let _ = aggregate_family(
                        &fixture.statement,
                        &fixture.pvk,
                        &fixture.padded_items,
                        &srs,
                    )
                    .expect("aggregation succeeds");
                });
            },
        );
    }
    aggregate_group.finish();

    let mut torus_aggregate_group = c.benchmark_group("snarkpack torus aggregate");
    for fixture in &fixtures {
        torus_aggregate_group.bench_with_input(
            BenchmarkId::new(format!("{:?}", fixture.family_id), fixture.count),
            fixture,
            |b, fixture| {
                b.iter(|| {
                    let _ = aggregate_family_torus(&fixture.statement, &fixture.padded_items, &srs)
                        .expect("aggregation succeeds");
                });
            },
        );
    }
    torus_aggregate_group.finish();

    let mut verify_group = c.benchmark_group("snarkpack verify");
    for fixture in &fixtures {
        verify_group.bench_with_input(
            BenchmarkId::new(format!("{:?}", fixture.family_id), fixture.count),
            fixture,
            |b, fixture| {
                b.iter(|| {
                    verify_family_aggregate(
                        &fixture.statement,
                        &fixture.pvk,
                        &fixture.aggregate_proof,
                        &srs,
                    )
                    .expect("verification succeeds");
                });
            },
        );
    }
    verify_group.finish();

    let mut torus_verify_group = c.benchmark_group("snarkpack torus verify exact");
    for fixture in &fixtures {
        torus_verify_group.bench_with_input(
            BenchmarkId::new(format!("{:?}", fixture.family_id), fixture.count),
            fixture,
            |b, fixture| {
                b.iter(|| {
                    verify_family_aggregate_torus(
                        &fixture.statement,
                        &fixture.pvk,
                        &fixture.torus_aggregate_proof,
                        &srs,
                    )
                    .expect("verification succeeds");
                });
            },
        );
    }
    torus_verify_group.finish();
}

criterion_group!(benches, snarkpack_bench);
criterion_main!(benches);
