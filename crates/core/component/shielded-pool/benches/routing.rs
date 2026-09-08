use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, SeedableRng};
use shieldd_sdk_asset::{asset, Value};
use shieldd_sdk_keys::test_keys;
use shieldd_sdk_shielded_pool::{
    discovery::{Parameters, RoutingSelector, RoutingTag},
    ShieldedOutputPlan,
};

const TRANSFERS_PER_SECOND: usize = 5_000;

fn routing_scan(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x524f_5554_4535_4b01);
    let fvk = &test_keys::FULL_VIEWING_KEY;
    let address = fvk.payment_address(0u32.into());
    let selector =
        RoutingSelector::for_address(&address, Parameters::default().regulated_precision);
    let output = ShieldedOutputPlan::new(
        &mut rng,
        Value {
            amount: 10u64.into(),
            asset_id: asset::Cache::with_known_assets()
                .get_unit("ushieldd")
                .expect("known benchmark asset")
                .id(),
        },
        address,
    );
    let witness = shieldd_sdk_compliance::ComplianceLeaf::synthetic_unregulated(
        output.dest_address.clone(),
        output.value.asset_id,
    );
    let (note, recovery_capsule) = output.output_note_and_capsule(witness.capk);
    let payload = note.payload(recovery_capsule);

    let mut state = 0x9e37_79b9u32;
    let tags = (0..TRANSFERS_PER_SECOND)
        .map(|index| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let first = if index % 2_048 == 0 {
                (state & !selector.precision.mask()) | selector.prefix
            } else {
                state
            };
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            [RoutingTag { value: first }, RoutingTag { value: state }]
        })
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("routing_5000_tps");
    group.throughput(Throughput::Elements(TRANSFERS_PER_SECOND as u64));
    group.bench_with_input(
        BenchmarkId::new(
            "prefix_match_and_candidate_trial_decrypt",
            TRANSFERS_PER_SECOND,
        ),
        &tags,
        |b, tags| {
            b.iter(|| {
                let mut found = 0usize;
                for transfer in tags {
                    for tag in transfer {
                        if selector.matches(*tag) {
                            found += usize::from(payload.trial_decrypt(fvk).is_some());
                        }
                    }
                }
                black_box(found)
            });
        },
    );
    group.finish();
}

criterion_group!(benches, routing_scan);
criterion_main!(benches);
