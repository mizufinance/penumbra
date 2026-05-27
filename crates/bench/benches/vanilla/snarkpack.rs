use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_snark::SNARK;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_aggregation::{
    aggregate_family, pad_items_to_power_of_two, srs_id, verify_family_aggregate,
    AggregateStatement, DevSrs, ProofFamilyId, AGGREGATE_PROTOCOL_VERSION,
};
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

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
    let (pvk, items) = generate_items(count);
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

    Fixture {
        family_id,
        count,
        pvk,
        padded_items,
        statement,
        aggregate_proof,
    }
}

fn snarkpack_bench(c: &mut Criterion) {
    let srs = DevSrs::default();
    let counts = [1usize, 2, 4, 8, 64];
    let families = [
        ProofFamilyId::Transfer,
        ProofFamilyId::Consolidate(penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS[0].id),
        ProofFamilyId::Split(penumbra_sdk_shielded_pool::SPLIT_FAMILY_SPECS[0].id),
        ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
    ];

    let fixtures: Vec<_> = families
        .into_iter()
        .flat_map(|family_id| counts.into_iter().map(move |count| (family_id, count)))
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
}

criterion_group!(benches, snarkpack_bench);
criterion_main!(benches);
