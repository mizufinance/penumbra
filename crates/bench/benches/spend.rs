use ark_relations::r1cs::{
    ConstraintSynthesizer, ConstraintSystem, OptimizationGoal, SynthesisMode,
};
use decaf377::{Fq, Fr};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_compliance::structs::ComplianceCiphertext;
use penumbra_sdk_proof_params::{DummyWitness, SPEND_PROOF_PROVING_KEY};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::generate_test_data;
use penumbra_sdk_shielded_pool::{SpendCircuit, SpendProof, SpendProofPrivate, SpendProofPublic};
use penumbra_sdk_tct as tct;

use criterion::{criterion_group, criterion_main, Criterion};
use rand_core::OsRng;

fn spend_proving_time(c: &mut Criterion) {
    let mut rng = OsRng;

    // Generate valid test data with compliance encryption
    use penumbra_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::CircuitType;
    let test_data = generate_test_data(&mut rng, 1, 100, false, CircuitType::Spend);

    // Create SCT for spend
    let mut sct = tct::Tree::new();
    let note_commitment = test_data.note.commit();
    sct.insert(tct::Witness::Keep, note_commitment).unwrap();
    let anchor = sct.root();
    let state_commitment_proof = sct.witness(note_commitment).unwrap();

    // Prepare public/private inputs
    let balance_commitment = Balance::from(test_data.value).commit(test_data.balance_blinding);
    let nullifier = Nullifier::derive(
        test_data.fvk.nullifier_key(),
        state_commitment_proof.position(),
        &note_commitment,
    );
    let randomizer = Fr::rand(&mut rng);
    let rk = test_data
        .fvk
        .spend_verification_key()
        .randomize(&randomizer);

    // Create dummy leaf and blinded sender hash
    let dummy_leaf = penumbra_sdk_compliance::ComplianceLeaf::new(
        test_data.address.clone(),
        test_data.note.asset_id(),
        Fq::from(0u64),
    );
    let dummy_nonce = Fr::from(0u64);
    let sender_leaf_hash =
        penumbra_sdk_compliance::blind_sender_leaf(dummy_leaf.commit(), dummy_nonce);

    // Spend uses detection + core only (4 Fqs)
    let ct_obj = ComplianceCiphertext::from_bytes(&test_data.compliance_ciphertext_bytes)
        .expect("can deserialize ciphertext");
    let (epk, c2_core, spend_ciphertext) = ct_obj.to_spend_circuit_public_inputs();

    let public = SpendProofPublic {
        anchor,
        balance_commitment,
        nullifier,
        rk,
        asset_anchor: test_data.asset_anchor,
        compliance_anchor: test_data.compliance_anchor,
        epk,
        c2_core,
        compliance_ciphertext: spend_ciphertext,
        target_timestamp: Fq::from(0u64),
        dleq_c: Fq::from(0u64),
        dleq_s: Fq::from(0u64),
        sender_leaf_hash,
    };

    let private = SpendProofPrivate {
        state_commitment_proof,
        note: test_data.note,
        v_blinding: test_data.balance_blinding,
        spend_auth_randomizer: randomizer,
        ak: *test_data.fvk.spend_verification_key(),
        nk: *test_data.fvk.nullifier_key(),
        asset_path: penumbra_sdk_compliance::MerklePath::default(),
        asset_position: 0,
        asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
            decaf377::Fq::from(0u64),
            0,
            decaf377::Fq::from(0u64),
        ),
        is_regulated: false,
        compliance_path: penumbra_sdk_compliance::MerklePath::default(),
        compliance_position: 0,
        user_leaf: test_data.user_leaf,
        compliance_ephemeral_secret: test_data.ephemeral_secret,
        tx_blinding_nonce: dummy_nonce,
        is_flagged: false,
        salt: decaf377::Fq::from(0u64),
    };

    let r = Fq::rand(&mut rng);
    let s = Fq::rand(&mut rng);

    c.bench_function("spend proving", |b| {
        b.iter(|| {
            let _proof = SpendProof::prove(
                r,
                s,
                &SPEND_PROOF_PROVING_KEY,
                public.clone(),
                private.clone(),
            )
            .expect("can create proof");
        })
    });

    // Also print out the number of constraints.
    let circuit = SpendCircuit::with_dummy_witness();

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

criterion_group!(benches, spend_proving_time);
criterion_main!(benches);
