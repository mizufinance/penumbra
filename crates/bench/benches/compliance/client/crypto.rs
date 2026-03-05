//! Compliance crypto primitive benchmarks (dev only).
//!
//! Measures all compliance-specific cryptographic operations.
//! No vanilla counterpart — these operations didn't exist before compliance.
//!
//! Outputs: `benches/compliance/client/crypto.csv`

use std::path::PathBuf;

use decaf377::{Fq, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_bench::bench_runner;
use penumbra_sdk_compliance::{
    compute_dleq_native, compute_metadata_hash, compute_output_dleqs, compute_spend_dleq,
    decrypt_detection_tier, decrypt_flagged_output, decrypt_flagged_spend,
    derive_compliance_scalar, fq_to_challenge_scalar,
    indexed_tree::IndexedMerkleTree,
    issuer_keys::DetectionKey,
    structs::{AssetPolicy, ComplianceCiphertext},
    test_helpers::{self, make_address},
    verify_dleq_native, ComplianceLeaf,
};
use penumbra_sdk_num::Amount;
use rand_core::OsRng;

const WARMUP: usize = 3;
const SAMPLES: usize = 100;

fn main() {
    let mut results = Vec::new();

    // --- Scalar derivation ---
    eprintln!("Benchmarking derive_compliance_scalar...");
    let b_d_fq = Fq::from(12345u64);
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _d = derive_compliance_scalar(b_d_fq);
    });
    results.push(bench_runner::make_result(
        "dev",
        &[("operation", "derive_compliance_scalar")],
        &times,
        None,
    ));

    eprintln!("Benchmarking fq_to_challenge_scalar...");
    let fq = Fq::from(99999u64);
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _cs = fq_to_challenge_scalar(fq);
    });
    results.push(bench_runner::make_result(
        "dev",
        &[("operation", "fq_to_challenge_scalar")],
        &times,
        None,
    ));

    // --- Metadata hash ---
    eprintln!("Benchmarking metadata_hash...");
    let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
        let _h = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1700000000u64),
            Fq::from(42u64),
        );
    });
    results.push(bench_runner::make_result(
        "dev",
        &[("operation", "metadata_hash")],
        &times,
        None,
    ));

    // --- DLEQ proofs ---
    eprintln!("Benchmarking dleq_compute...");
    {
        let mut rng = OsRng;
        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let sk = Fr::rand(&mut rng);
        let ack = decaf377::Element::GENERATOR * sk;
        let epk = decaf377::Element::GENERATOR * r;
        let metadata_hash = Fq::from(42u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "dleq_compute")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking dleq_verify...");
    {
        let mut rng = OsRng;
        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let sk = Fr::rand(&mut rng);
        let ack = decaf377::Element::GENERATOR * sk;
        let epk = decaf377::Element::GENERATOR * r;
        let s_point = ack * r;
        let metadata_hash = Fq::from(42u64);
        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            verify_dleq_native(&ack, &epk, &s_point, &proof.c, &proof.s, metadata_hash).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "dleq_verify")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking spend_dleq...");
    {
        let mut rng = OsRng;
        let r_s = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let sk = Fr::rand(&mut rng);
        let ack = decaf377::Element::GENERATOR * sk;
        let metadata_hash = Fq::from(42u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _proof = compute_spend_dleq(r_s, k, &ack, metadata_hash);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "spend_dleq")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking output_dleqs...");
    {
        let mut rng = OsRng;
        let r_1 = Fr::rand(&mut rng);
        let r_2 = Fr::rand(&mut rng);
        let r_3 = Fr::rand(&mut rng);
        let k_1 = Fr::rand(&mut rng);
        let k_2 = Fr::rand(&mut rng);
        let k_3 = Fr::rand(&mut rng);
        let sk_r = Fr::rand(&mut rng);
        let sk_s = Fr::rand(&mut rng);
        let ack_receiver = decaf377::Element::GENERATOR * sk_r;
        let ack_sender = decaf377::Element::GENERATOR * sk_s;
        let metadata_hash = Fq::from(42u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _proofs = compute_output_dleqs(
                r_1,
                r_2,
                r_3,
                k_1,
                k_2,
                k_3,
                &ack_receiver,
                &ack_sender,
                metadata_hash,
            );
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "output_dleqs")],
            &times,
            None,
        ));
    }

    // --- ECDH shared secret ---
    eprintln!("Benchmarking ecdh_shared_secret...");
    {
        let mut rng = OsRng;
        let r = Fr::rand(&mut rng);
        let sk = Fr::rand(&mut rng);
        let ack = decaf377::Element::GENERATOR * sk;

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _ss = ack * r;
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "ecdh_shared_secret")],
            &times,
            None,
        ));
    }

    // --- Encryption ---
    eprintln!("Benchmarking encrypt_spend...");
    {
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let dk_pub = decaf377::Element::GENERATOR;
        let addr = make_address(1);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result =
                test_helpers::encrypt_test_spend(&ring_pk, &dk_pub, &addr, asset_id, amount, false);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "encrypt_spend")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking encrypt_spend_flagged...");
    {
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let dk_pub = decaf377::Element::GENERATOR;
        let addr = make_address(1);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result =
                test_helpers::encrypt_test_spend(&ring_pk, &dk_pub, &addr, asset_id, amount, true);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "encrypt_spend_flagged")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking encrypt_output...");
    {
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let dk_pub = decaf377::Element::GENERATOR;
        let self_addr = make_address(1);
        let counterparty_addr = make_address(2);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result = test_helpers::encrypt_test_output(
                &ring_pk,
                &dk_pub,
                &self_addr,
                &counterparty_addr,
                asset_id,
                amount,
                false,
            );
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "encrypt_output")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking encrypt_output_flagged...");
    {
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let dk_pub = decaf377::Element::GENERATOR;
        let self_addr = make_address(1);
        let counterparty_addr = make_address(2);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result = test_helpers::encrypt_test_output(
                &ring_pk,
                &dk_pub,
                &self_addr,
                &counterparty_addr,
                asset_id,
                amount,
                true,
            );
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "encrypt_output_flagged")],
            &times,
            None,
        ));
    }

    // --- Decryption ---
    eprintln!("Benchmarking decrypt_detection...");
    {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let self_addr = make_address(1);
        let counterparty_addr = make_address(2);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let result = test_helpers::encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &self_addr,
            &counterparty_addr,
            asset_id,
            amount,
            false,
        );
        let ct = result.ciphertext;
        let epk_1 = ct.epk_1;
        let detection_tag = ct.detection_tag;

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result = decrypt_detection_tier(dk.inner(), &epk_1, &detection_tag, &asset_id);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "decrypt_detection")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking decrypt_flagged_spend...");
    {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let self_addr = make_address(1);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let spend_result =
            test_helpers::encrypt_test_spend(&ring_pk, &dk_pub, &self_addr, asset_id, amount, true);

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result = decrypt_flagged_spend(dk.inner(), &spend_result.ciphertext, &asset_id);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "decrypt_flagged_spend")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking decrypt_flagged_output...");
    {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let self_addr = make_address(1);
        let counterparty_addr = make_address(2);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let output_result = test_helpers::encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &self_addr,
            &counterparty_addr,
            asset_id,
            amount,
            true,
        );

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _result = decrypt_flagged_output(dk.inner(), &output_result.ciphertext, &asset_id);
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "decrypt_flagged_output")],
            &times,
            None,
        ));
    }

    // --- Leaf commitment ---
    eprintln!("Benchmarking leaf_commit...");
    {
        let addr = make_address(1);
        let leaf = ComplianceLeaf {
            address: addr,
            asset_id: asset::Id(Fq::from(1000u64)),
            d: Fq::from(42u64),
        };

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _c = leaf.commit();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "leaf_commit")],
            &times,
            None,
        ));
    }

    eprintln!("Benchmarking indexed_leaf_commit...");
    {
        let dk_pub = decaf377::Element::GENERATOR;
        let ring_pk = decaf377::Element::GENERATOR;
        let policy = AssetPolicy::simple(dk_pub, u128::MAX, ring_pk);

        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(1000u64), &policy).unwrap();
        let leaf = tree.get_leaf(1).unwrap().clone();

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _c = leaf.commit();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "indexed_leaf_commit")],
            &times,
            None,
        ));
    }

    // --- Ciphertext serialization ---
    eprintln!("Benchmarking ciphertext serialization...");
    {
        let ring_pk = decaf377::Element::GENERATOR * Fr::from(999u64);
        let dk_pub = decaf377::Element::GENERATOR;
        let self_addr = make_address(1);
        let counterparty_addr = make_address(2);
        let asset_id = asset::Id(Fq::from(1000u64));
        let amount = Amount::from(100u64);

        let spend_result = test_helpers::encrypt_test_spend(
            &ring_pk, &dk_pub, &self_addr, asset_id, amount, false,
        );
        let output_result = test_helpers::encrypt_test_output(
            &ring_pk,
            &dk_pub,
            &self_addr,
            &counterparty_addr,
            asset_id,
            amount,
            false,
        );

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _bytes = spend_result.ciphertext.to_bytes();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "serialize_spend")],
            &times,
            None,
        ));

        let spend_bytes = spend_result.ciphertext.to_bytes();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _ct = ComplianceCiphertext::from_bytes(&spend_bytes).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "deserialize_spend")],
            &times,
            None,
        ));

        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _bytes = output_result.ciphertext.to_bytes();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "serialize_output")],
            &times,
            None,
        ));

        let output_bytes = output_result.ciphertext.to_bytes();
        let times = bench_runner::run_bench(WARMUP, SAMPLES, || {
            let _ct = ComplianceCiphertext::from_bytes(&output_bytes).unwrap();
        });
        results.push(bench_runner::make_result(
            "dev",
            &[("operation", "deserialize_output")],
            &times,
            None,
        ));
    }

    // --- Output ---
    bench_runner::output_results(&results);
    let csv_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/compliance/client/crypto.csv");
    bench_runner::write_csv(&csv_path, &results);
}
