use ark_relations::r1cs::{
    ConstraintSynthesizer, ConstraintSystem, OptimizationGoal, SynthesisMode,
};
use decaf377::{Fq, Fr};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_proof_params::{DummyWitness, OUTPUT_PROOF_PROVING_KEY};
use penumbra_sdk_shielded_pool::output::{OutputProofPrivate, OutputProofPublic};
use penumbra_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::{
    generate_test_data, CircuitType,
};
use penumbra_sdk_shielded_pool::{OutputCircuit, OutputProof};

use criterion::{criterion_group, criterion_main, Criterion};
use rand_core::OsRng;

fn output_proving_time(c: &mut Criterion) {
    let mut rng = OsRng;

    // Generate valid test data with compliance encryption
    let test_data = generate_test_data(&mut rng, 1, 100, false, CircuitType::Output);

    let note_commitment = test_data.note.commit();
    let balance_commitment = (-Balance::from(test_data.value)).commit(test_data.balance_blinding);

    // Create dummy leaves and blinded hashes
    let dummy_leaf = penumbra_sdk_compliance::ComplianceLeaf {
        address: test_data.address.clone(),
        asset_id: test_data.note.asset_id(),
        d: Fq::from(0u64),
    };
    let dummy_nonce = Fr::from(0u64);
    let counterparty_leaf_hash =
        penumbra_sdk_compliance::blind_sender_leaf(dummy_leaf.commit(), dummy_nonce);

    let public = OutputProofPublic {
        balance_commitment,
        note_commitment,
        epk_1: test_data.epk_1,
        epk_2: test_data.epk_2.expect("output needs epk_2"),
        epk_3: test_data.epk_3.expect("output needs epk_3"),
        c2_core: test_data.c2_core,
        c2_ext: test_data.c2_ext.expect("output needs c2_ext"),
        c2_sext: test_data.c2_sext.expect("output needs c2_sext"),
        compliance_ciphertext: test_data.compliance_ciphertext,
        asset_anchor: test_data.asset_anchor,
        compliance_anchor: test_data.compliance_anchor,
        target_timestamp: Fq::from(0u64),
        dleq_c_1: Fq::from(0u64),
        dleq_s_1: Fq::from(0u64),
        dleq_c_2: Fq::from(0u64),
        dleq_s_2: Fq::from(0u64),
        dleq_c_3: Fq::from(0u64),
        dleq_s_3: Fq::from(0u64),
        counterparty_leaf_hash,
    };

    let private = OutputProofPrivate {
        note: test_data.note,
        balance_blinding: test_data.balance_blinding,
        asset_path: test_data.asset_path,
        asset_position: test_data.asset_position,
        asset_indexed_leaf: test_data.asset_indexed_leaf,
        is_regulated: false,
        compliance_path: test_data.compliance_path,
        compliance_position: test_data.compliance_position,
        user_leaf: test_data.user_leaf,
        compliance_ephemeral_secret: test_data.ephemeral_secret,
        r_2: Fr::rand(&mut rng),
        r_3: Fr::rand(&mut rng),
        counterparty_leaf: dummy_leaf,
        tx_blinding_nonce: dummy_nonce,
        is_flagged: false,
        salt: decaf377::Fq::from(0u64),
    };

    let r = Fq::rand(&mut rng);
    let s = Fq::rand(&mut rng);

    c.bench_function("output proving", |b| {
        b.iter(|| {
            let _proof = OutputProof::prove(
                r,
                s,
                &OUTPUT_PROOF_PROVING_KEY,
                public.clone(),
                private.clone(),
            )
            .expect("can create proof");
        })
    });

    // Also print out the number of constraints.
    let circuit = OutputCircuit::with_dummy_witness();

    let cs = ConstraintSystem::new_ref();
    cs.set_optimization_goal(OptimizationGoal::Constraints);
    cs.set_mode(SynthesisMode::Setup);

    circuit
        .generate_constraints(cs.clone())
        .expect("can generate constraints");
    cs.finalize();
    let num_constraints = cs.num_constraints();
    println!("Number of constraints: {}", num_constraints);
}

criterion_group!(benches, output_proving_time);
criterion_main!(benches);
