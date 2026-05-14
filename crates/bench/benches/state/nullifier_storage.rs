use std::collections::{BTreeMap, HashMap};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use penumbra_sdk_sct::{
    component::tree::SctRead as _, nullifier_tree, NullificationInfo, Nullifier,
};

fn configured_sizes() -> Vec<usize> {
    std::env::var("PENUMBRA_NULLIFIER_BENCH_SIZES")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.parse().expect("invalid nullifier bench size"))
                .collect()
        })
        .unwrap_or_else(|| vec![10_000])
}

fn nullifier(index: usize) -> Nullifier {
    Nullifier(decaf377::Fq::from(index as u64 + 1))
}

fn nullifier_key(index: usize) -> [u8; 32] {
    nullifier(index).0.to_bytes()
}

fn info(index: usize) -> NullificationInfo {
    NullificationInfo {
        id: [index as u8; 32],
        spend_height: index as u64,
    }
}

fn bench_nullifier_storage(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("nullifier_storage_lookup");

    for size in configured_sizes() {
        let storage = runtime.block_on(cnidarium::TempStorage::new()).unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        let mut flat = HashMap::with_capacity(size);
        let mut ordered = BTreeMap::new();
        runtime
            .block_on(nullifier_tree::initialize(&mut state))
            .unwrap();

        for index in 0..size {
            let nf = nullifier(index);
            let key = nullifier_key(index);
            let info = info(index);
            runtime
                .block_on(nullifier_tree::insert_batch(&mut state, [(nf, info)]))
                .unwrap();
            flat.insert(key, info);
            ordered.insert(key, info);
        }

        let hit_index = size / 2;
        let hit_key = nullifier_key(hit_index);
        let hit_nf = nullifier(hit_index);
        let miss_key = nullifier_key(size + 1);

        group.bench_with_input(
            BenchmarkId::new("dedicated_jmt_hit", size),
            &hit_nf,
            |b, nf| b.iter(|| runtime.block_on(state.spend_info(*nf)).unwrap()),
        );

        group.bench_with_input(
            BenchmarkId::new("flat_hash_hit", size),
            &hit_key,
            |b, key| b.iter(|| flat.get(key)),
        );

        group.bench_with_input(
            BenchmarkId::new("flat_ordered_hit", size),
            &hit_key,
            |b, key| b.iter(|| ordered.get(key)),
        );
        group.bench_with_input(
            BenchmarkId::new("flat_hash_miss", size),
            &miss_key,
            |b, key| b.iter(|| flat.get(key)),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_nullifier_storage);
criterion_main!(benches);
