//! Shared helpers for flow benchmarks.
//!
//! Provides compliance enrichment and TransactionPlan construction.

#![allow(dead_code)]

use std::ops::Deref;

use decaf377::Fr;
use penumbra_sdk_asset::{asset, Value, STAKING_TOKEN_ASSET_ID};
use penumbra_sdk_compliance::{ComplianceLeaf, IndexedMerkleTree, MerklePath, QuadTree};
use penumbra_sdk_fee::Fee;
use penumbra_sdk_keys::{test_keys, Address};
use penumbra_sdk_shielded_pool::{Note, OutputPlan, SpendPlan};
use penumbra_sdk_tct as tct;
use penumbra_sdk_transaction::{
    plan::{CluePlan, DetectionDataPlan, TransactionPlan},
    TransactionParameters, WitnessData,
};
use rand_core::OsRng;

/// Enrich a SpendPlan with valid compliance data.
/// Adapted from penumbra-sdk-app test helpers.
pub fn enrich_spend_for_test<R: rand_core::RngCore + rand_core::CryptoRng>(
    rng: &mut R,
    spend: &mut SpendPlan,
    _sender_address: &Address,
) {
    let asset_id = spend.note.asset_id();

    // Create IMT non-membership proof (unregulated asset)
    let imt = IndexedMerkleTree::new();
    let (position, indexed_leaf, auth_path) = imt
        .non_membership_proof(asset_id.0)
        .expect("can generate non-membership proof");
    let asset_anchor = tct::StateCommitment(imt.root().0);
    let asset_path = MerklePath::from_auth_path(auth_path);

    spend.asset_anchor = asset_anchor;
    spend.asset_path = asset_path;
    spend.asset_position = position;
    spend.asset_indexed_leaf = indexed_leaf;

    spend
        .set_compliance_details(rng)
        .expect("can set compliance details");

    // Build user tree from the compliance_leaf that set_compliance_details created
    let user_leaf = spend.compliance_leaf.clone().unwrap();
    let mut user_tree = QuadTree::new();
    user_tree
        .update(0, user_leaf.commit())
        .expect("can update tree");
    let compliance_anchor = tct::StateCommitment(user_tree.root().0);
    let user_auth_path = user_tree.auth_path(0).expect("can get auth path");
    let compliance_path = MerklePath::from_auth_path(user_auth_path);

    spend.compliance_anchor = compliance_anchor;
    spend.compliance_path = compliance_path;
    spend.compliance_position = 0;
}

/// Enrich an OutputPlan with valid compliance data.
/// Adapted from penumbra-sdk-app test helpers.
pub fn enrich_output_for_test<R: rand_core::RngCore + rand_core::CryptoRng>(
    rng: &mut R,
    output: &mut OutputPlan,
    sender_address: &Address,
    asset_id: asset::Id,
) {
    let imt = IndexedMerkleTree::new();
    let (position, indexed_leaf, auth_path) = imt
        .non_membership_proof(asset_id.0)
        .expect("can generate non-membership proof");
    let asset_anchor = tct::StateCommitment(imt.root().0);
    let asset_path = MerklePath::from_auth_path(auth_path);

    output.asset_anchor = asset_anchor;
    output.asset_path = asset_path;
    output.asset_position = position;
    output.asset_indexed_leaf = indexed_leaf;

    // Create leaves with real d (matching what the circuit derives)
    let recv_b_d_fq = output
        .dest_address
        .diversified_generator()
        .vartime_compress_to_field();
    let recv_d = penumbra_sdk_compliance::derive_compliance_scalar(recv_b_d_fq);
    let recipient_leaf = ComplianceLeaf {
        address: output.dest_address.clone(),
        asset_id,
        d: recv_d,
    };

    let send_b_d_fq = sender_address
        .diversified_generator()
        .vartime_compress_to_field();
    let send_d = penumbra_sdk_compliance::derive_compliance_scalar(send_b_d_fq);
    let sender_leaf = ComplianceLeaf {
        address: sender_address.clone(),
        asset_id,
        d: send_d,
    };

    output
        .set_compliance_details(
            rng,
            &recipient_leaf,
            sender_leaf,
            Fr::from(0u64), // tx_blinding_nonce
        )
        .expect("can set compliance details");

    let user_leaf = output.compliance_leaf.clone().unwrap();
    let mut user_tree = QuadTree::new();
    user_tree
        .update(0, user_leaf.commit())
        .expect("can update tree");
    let compliance_anchor = tct::StateCommitment(user_tree.root().0);
    let user_auth_path = user_tree.auth_path(0).expect("can get auth path");
    let compliance_path = MerklePath::from_auth_path(user_auth_path);

    output.compliance_anchor = compliance_anchor;
    output.compliance_path = compliance_path;
    output.compliance_position = 0;
}

/// Everything needed to build a transaction.
pub struct PreparedPlan {
    pub plan: TransactionPlan,
    pub witness_data: WitnessData,
}

/// Build a simple transfer: 1 spend + 1 output.
/// Returns a plan with valid compliance enrichment, plus witness data and SCT.
pub fn build_simple_transfer() -> PreparedPlan {
    let value = Value {
        amount: 100u64.into(),
        asset_id: *STAKING_TOKEN_ASSET_ID,
    };
    let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);

    let mut sct = tct::Tree::new();
    // Add some padding so positions are non-trivial
    for _ in 0..5 {
        let random_note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        sct.insert(tct::Witness::Keep, random_note.commit())
            .unwrap();
    }
    sct.insert(tct::Witness::Keep, note.commit()).unwrap();
    let auth_path = sct.witness(note.commit()).unwrap();

    let mut spend = SpendPlan::new(&mut OsRng, note.clone(), auth_path.position());
    let mut output = OutputPlan::new(&mut OsRng, value, test_keys::ADDRESS_1.deref().clone());

    enrich_spend_for_test(&mut OsRng, &mut spend, &test_keys::ADDRESS_0);
    enrich_output_for_test(
        &mut OsRng,
        &mut output,
        &test_keys::ADDRESS_0,
        value.asset_id,
    );

    let plan = TransactionPlan {
        transaction_parameters: TransactionParameters {
            expiry_height: 0,
            fee: Fee::default(),
            chain_id: "".into(),
        },
        actions: vec![spend.into(), output.into()],
        detection_data: Some(DetectionDataPlan {
            clue_plans: vec![CluePlan::new(
                &mut OsRng,
                test_keys::ADDRESS_1.deref().clone(),
                1.try_into().unwrap(),
            )],
        }),
        memo: None,
    };

    let witness_data = WitnessData {
        anchor: sct.root(),
        state_commitment_proofs: plan
            .spend_plans()
            .map(|s| (s.note.commit(), sct.witness(s.note.commit()).unwrap()))
            .collect(),
    };

    PreparedPlan { plan, witness_data }
}

/// Build a multi-spend transaction: 4 spends + 1 output.
pub fn build_multi_spend_4x1() -> PreparedPlan {
    let value = Value {
        amount: 25u64.into(),
        asset_id: *STAKING_TOKEN_ASSET_ID,
    };

    let notes: Vec<_> = (0..4)
        .map(|_| Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value))
        .collect();

    let mut sct = tct::Tree::new();
    for note in &notes {
        sct.insert(tct::Witness::Keep, note.commit()).unwrap();
    }

    let mut spends = Vec::new();
    for note in &notes {
        let auth_path = sct.witness(note.commit()).unwrap();
        let mut spend = SpendPlan::new(&mut OsRng, note.clone(), auth_path.position());
        enrich_spend_for_test(&mut OsRng, &mut spend, &test_keys::ADDRESS_0);
        spends.push(spend);
    }

    let total_value = Value {
        amount: 100u64.into(),
        asset_id: *STAKING_TOKEN_ASSET_ID,
    };
    let mut output = OutputPlan::new(
        &mut OsRng,
        total_value,
        test_keys::ADDRESS_1.deref().clone(),
    );
    enrich_output_for_test(
        &mut OsRng,
        &mut output,
        &test_keys::ADDRESS_0,
        total_value.asset_id,
    );

    let mut actions: Vec<_> = spends.into_iter().map(|s| s.into()).collect();
    actions.push(output.into());

    let plan = TransactionPlan {
        transaction_parameters: TransactionParameters {
            expiry_height: 0,
            fee: Fee::default(),
            chain_id: "".into(),
        },
        actions,
        detection_data: Some(DetectionDataPlan {
            clue_plans: vec![CluePlan::new(
                &mut OsRng,
                test_keys::ADDRESS_1.deref().clone(),
                1.try_into().unwrap(),
            )],
        }),
        memo: None,
    };

    let witness_data = WitnessData {
        anchor: sct.root(),
        state_commitment_proofs: plan
            .spend_plans()
            .map(|s| (s.note.commit(), sct.witness(s.note.commit()).unwrap()))
            .collect(),
    };

    PreparedPlan { plan, witness_data }
}

/// Build a simple transfer with unenriched plans (for benchmarking enrichment separately).
pub fn build_simple_transfer_unenriched() -> (SpendPlan, OutputPlan, tct::Tree) {
    let value = Value {
        amount: 100u64.into(),
        asset_id: *STAKING_TOKEN_ASSET_ID,
    };
    let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);

    let mut sct = tct::Tree::new();
    for _ in 0..5 {
        let random_note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        sct.insert(tct::Witness::Keep, random_note.commit())
            .unwrap();
    }
    sct.insert(tct::Witness::Keep, note.commit()).unwrap();
    let auth_path = sct.witness(note.commit()).unwrap();

    let spend = SpendPlan::new(&mut OsRng, note, auth_path.position());
    let output = OutputPlan::new(&mut OsRng, value, test_keys::ADDRESS_1.deref().clone());

    (spend, output, sct)
}

/// Build a multi-spend with unenriched plans.
pub fn build_multi_spend_4x1_unenriched() -> (Vec<SpendPlan>, OutputPlan, tct::Tree) {
    let value = Value {
        amount: 25u64.into(),
        asset_id: *STAKING_TOKEN_ASSET_ID,
    };

    let notes: Vec<_> = (0..4)
        .map(|_| Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value))
        .collect();

    let mut sct = tct::Tree::new();
    for note in &notes {
        sct.insert(tct::Witness::Keep, note.commit()).unwrap();
    }

    let spends: Vec<_> = notes
        .iter()
        .map(|note| {
            let auth_path = sct.witness(note.commit()).unwrap();
            SpendPlan::new(&mut OsRng, note.clone(), auth_path.position())
        })
        .collect();

    let total_value = Value {
        amount: 100u64.into(),
        asset_id: *STAKING_TOKEN_ASSET_ID,
    };
    let output = OutputPlan::new(
        &mut OsRng,
        total_value,
        test_keys::ADDRESS_1.deref().clone(),
    );

    (spends, output, sct)
}
