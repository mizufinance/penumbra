//! Compliance tree benchmarks (v0.1 only).
//!
//! Measures QuadTree (user registrations) and IMT (asset registrations) operations.
//! No vanilla counterpart — these tree structures are new with compliance.
//!
//! Outputs: `benches/compliance/scanner/trees.csv`

use std::path::PathBuf;

use decaf377::Fq;
use penumbra_sdk_asset::asset;
use penumbra_sdk_bench_support::bench_runner;
use penumbra_sdk_compliance::{
    indexed_tree::IndexedMerkleTree, structs::AssetPolicy, tree::QuadTree, ComplianceLeaf,
    DEFAULT_DEPTH,
};
use penumbra_sdk_keys::Address;
use rand::SeedableRng;

type StdRng = rand::rngs::StdRng;

const SEED: u64 = 42;
const WARMUP: usize = 1;
const SAMPLES: usize = 10;
const QUAD_SIZES: &[u64] = &[100, 1_000, 10_000];
const IMT_SIZES: &[u64] = &[50, 500, 5_000];

fn make_dummy_leaf(rng: &mut StdRng, i: u64) -> ComplianceLeaf {
    ComplianceLeaf {
        address: Address::dummy(rng),
        asset_id: asset::Id(Fq::from(i)),
        d: Fq::from(0u64),
    }
}

fn build_quad_tree(rng: &mut StdRng, size: u64) -> QuadTree {
    let mut tree = QuadTree::new();
    for i in 0..size {
        let leaf = make_dummy_leaf(rng, i);
        tree.update(i, leaf.commit()).unwrap();
    }
    tree
}

fn build_imt(size: u64) -> IndexedMerkleTree {
    let mut tree = IndexedMerkleTree::new();
    let dk_pub = decaf377::Element::GENERATOR;
    let ring_pk = decaf377::Element::GENERATOR;
    for i in 1..=size {
        let policy = AssetPolicy::simple(dk_pub, u128::MAX, ring_pk);
        tree.insert(Fq::from(i * 1000), &policy).unwrap();
    }
    tree
}

fn main() {
    let mut results = Vec::new();

    // ===== QuadTree benchmarks =====

    // --- Insert ---
    eprintln!("Benchmarking quad_insert...");
    for &size in QUAD_SIZES {
        eprintln!("  size={}", size);
        let mut rng = StdRng::seed_from_u64(SEED);
        let mut tree = build_quad_tree(&mut rng, size);
        let next_pos = size;
        let leaf = make_dummy_leaf(&mut rng, next_pos);
        let commitment = leaf.commit();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            tree.update(next_pos, commitment).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "quad"),
                ("operation", "insert"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Auth path ---
    eprintln!("Benchmarking quad_auth_path...");
    for &size in QUAD_SIZES {
        eprintln!("  size={}", size);
        let mut rng = StdRng::seed_from_u64(SEED);
        let tree = build_quad_tree(&mut rng, size);
        let mid = size / 2;
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _path = tree.auth_path(mid).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "quad"),
                ("operation", "auth_path"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Verify ---
    eprintln!("Benchmarking quad_verify...");
    for &size in QUAD_SIZES {
        eprintln!("  size={}", size);
        let mut rng = StdRng::seed_from_u64(SEED);
        let tree = build_quad_tree(&mut rng, size);
        let pos = size / 2;
        // Rebuild the specific leaf commitment at pos
        let mut rng2 = StdRng::seed_from_u64(SEED);
        let mut commitment = penumbra_sdk_tct::StateCommitment(Fq::from(0u64));
        for i in 0..=pos {
            let l = make_dummy_leaf(&mut rng2, i);
            if i == pos {
                commitment = l.commit();
            }
        }
        let path = tree.auth_path(pos).unwrap();
        let root = tree.root();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _ok = QuadTree::verify_auth_path(pos, commitment, &path, root, DEFAULT_DEPTH);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "quad"),
                ("operation", "verify"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Root ---
    eprintln!("Benchmarking quad_root...");
    for &size in QUAD_SIZES {
        eprintln!("  size={}", size);
        let mut rng = StdRng::seed_from_u64(SEED);
        let tree = build_quad_tree(&mut rng, size);
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _root = tree.root();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "quad"),
                ("operation", "root"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // ===== IMT benchmarks =====

    // --- Insert ---
    eprintln!("Benchmarking imt_insert...");
    for &size in IMT_SIZES {
        eprintln!("  size={}", size);
        let tree = build_imt(size);
        let dk_pub = decaf377::Element::GENERATOR;
        let ring_pk = decaf377::Element::GENERATOR;
        let policy = AssetPolicy::simple(dk_pub, u128::MAX, ring_pk);
        let new_val = Fq::from((size + 1) * 1000 + 999);
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let mut t = tree.clone();
            t.insert(new_val, &policy).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "imt"),
                ("operation", "insert"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Membership proof ---
    eprintln!("Benchmarking imt_membership...");
    for &size in IMT_SIZES {
        eprintln!("  size={}", size);
        let tree = build_imt(size);
        let mid_val = Fq::from((size / 2) * 1000);
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _proof = tree.membership_proof(mid_val).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "imt"),
                ("operation", "membership"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Non-membership proof ---
    eprintln!("Benchmarking imt_non_membership...");
    for &size in IMT_SIZES {
        eprintln!("  size={}", size);
        let tree = build_imt(size);
        let gap_val = Fq::from((size / 2) * 1000 + 1);
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _proof = tree.non_membership_proof(gap_val).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "imt"),
                ("operation", "non_membership"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Root ---
    eprintln!("Benchmarking imt_root...");
    for &size in IMT_SIZES {
        eprintln!("  size={}", size);
        let tree = build_imt(size);
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _root = tree.root();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[
                ("tree", "imt"),
                ("operation", "root"),
                ("size", &size.to_string()),
            ],
            &times,
            None,
        ));
    }

    // --- Output ---
    bench_runner::output_results(&results);
    let csv_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/compliance/scanner/trees.csv");
    bench_runner::write_csv(&csv_path, &results);
}
