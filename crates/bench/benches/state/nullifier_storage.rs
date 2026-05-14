use std::collections::{BTreeMap, HashMap};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use penumbra_sdk_sct::{state_key, NullificationInfo, Nullifier};

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

struct TinyBloom {
    bits: Vec<u64>,
    mask: usize,
}

impl TinyBloom {
    fn new(entries: usize) -> Self {
        let bit_count = entries.next_power_of_two().max(64) * 16;
        Self {
            bits: vec![0; bit_count / 64],
            mask: bit_count - 1,
        }
    }

    fn indexes(key: &[u8; 32], mask: usize) -> [usize; 3] {
        let a = u64::from_le_bytes(key[0..8].try_into().unwrap()) as usize;
        let b = u64::from_le_bytes(key[8..16].try_into().unwrap()) as usize;
        let c = u64::from_le_bytes(key[16..24].try_into().unwrap()) as usize;
        [a & mask, b & mask, c & mask]
    }

    fn insert(&mut self, key: &[u8; 32]) {
        for index in Self::indexes(key, self.mask) {
            self.bits[index / 64] |= 1u64 << (index % 64);
        }
    }

    fn may_contain(&self, key: &[u8; 32]) -> bool {
        Self::indexes(key, self.mask)
            .iter()
            .all(|index| self.bits[index / 64] & (1u64 << (index % 64)) != 0)
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
        let mut bloom = TinyBloom::new(size);

        for index in 0..size {
            let nf = nullifier(index);
            let key = nullifier_key(index);
            let info = info(index);
            state.put(state_key::nullifier_set::spent_nullifier_lookup(&nf), info);
            flat.insert(key, info);
            ordered.insert(key, info);
            bloom.insert(&key);
        }

        let hit_index = size / 2;
        let hit_key = nullifier_key(hit_index);
        let hit_nf = nullifier(hit_index);
        let miss_key = nullifier_key(size + 1);

        group.bench_with_input(BenchmarkId::new("jmt_hit", size), &hit_nf, |b, nf| {
            b.iter(|| {
                runtime
                    .block_on(state.get::<NullificationInfo>(
                        &state_key::nullifier_set::spent_nullifier_lookup(nf),
                    ))
                    .unwrap()
            })
        });

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
            BenchmarkId::new("bloom_then_flat_miss", size),
            &miss_key,
            |b, key| b.iter(|| bloom.may_contain(key).then(|| flat.get(key)).flatten()),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_nullifier_storage);
criterion_main!(benches);
