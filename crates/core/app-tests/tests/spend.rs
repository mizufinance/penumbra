mod common;

use self::common::TempStorageExt;
use cnidarium::{ArcStateDeltaExt, StateDelta, TempStorage};
use cnidarium_component::{ActionHandler as _, Component};
use decaf377::{Fq, Fr};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use penumbra_sdk_asset::Value;
use penumbra_sdk_compact_block::component::CompactBlockManager;
use penumbra_sdk_compliance::structs::MerklePath;
use penumbra_sdk_compliance::ComplianceRegistryWrite;
use penumbra_sdk_keys::{keys::NullifierKey, test_keys};
use penumbra_sdk_mock_client::MockClient;
use penumbra_sdk_num::Amount;
use penumbra_sdk_sct::{
    component::{clock::EpochManager, source::SourceContext},
    epoch::Epoch,
};
use penumbra_sdk_shielded_pool::{
    component::ShieldedPool, Note, OutputPlan, SpendPlan, SpendProof, SpendProofPrivate,
    SpendProofPublic,
};
use penumbra_sdk_tct as tct;
use penumbra_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, TransactionParameters, TransactionPlan,
};
use penumbra_sdk_txhash::{EffectHash, EffectingData as _, TransactionContext};
use rand_core::OsRng;
use std::{ops::Deref, sync::Arc};
use tendermint::abci;

/// Test the spend action handler with proper compliance proofs.
///
/// This test registers compliance data, builds a transaction with Groth16 proofs,
/// and verifies the spend action handler accepts the transaction.
#[tokio::test]
async fn spend_happy_path() -> anyhow::Result<()> {
    let storage = TempStorage::new_with_penumbra_prefixes()
        .await?
        .apply_default_genesis()
        .await?;
    let mut state = Arc::new(StateDelta::new(storage.latest_snapshot()));

    let height = 1;

    // Sync the mock client to see genesis notes
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone());
    client.sync_to(0, state.deref()).await?;

    // Get a note to spend
    let note = client.notes.values().next().unwrap().clone();
    let sender_address = note.address();
    let recipient_address = test_keys::ADDRESS_1.deref().clone();
    let asset_id = note.asset_id();

    // 1. Simulate BeginBlock and register compliance
    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_block_height(height);
        state_tx.put_block_timestamp(height, tendermint::Time::now());
        state_tx.put_epoch_by_height(
            height,
            Epoch {
                index: 0,
                start_height: 0,
            },
        );

        // Register the asset as unregulated
        common::register_assets_for_compliance(&mut state_tx, &[asset_id]).await?;
        // Register the users for this asset
        common::register_test_users_for_compliance(
            &mut state_tx,
            &[sender_address.clone(), recipient_address.clone()],
            &[asset_id],
        )
        .await?;

        state_tx.record_compliance_anchors(height).await?;

        state_tx.apply();
    }

    // 2. Create a transaction plan with spend and output
    let mut plan = TransactionPlan {
        actions: vec![
            SpendPlan::new(
                &mut OsRng,
                note.clone(),
                client
                    .position(note.commit())
                    .expect("note should be in mock client's tree"),
            )
            .into(),
            OutputPlan::new(&mut OsRng, note.value(), recipient_address).into(),
        ],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
        )),
        detection_data: None,
        transaction_parameters: TransactionParameters {
            chain_id: "penumbra-test".to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    // Build with compliance enrichment from state
    let tx = client
        .witness_auth_build_with_compliance(&mut plan, state.deref())
        .await?;

    // Create transaction context for verification
    let transaction_context = TransactionContext {
        anchor: client.sct.root(),
        effect_hash: EffectHash(tx.effect_hash().as_ref().try_into().unwrap()),
    };

    // Get the spend action from the transaction
    let tx_body = tx.transaction_body();
    let spend = tx_body
        .actions
        .iter()
        .find_map(|a| {
            if let penumbra_sdk_transaction::Action::Spend(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .expect("transaction should have a spend action");

    // 3. Verify and execute the spend action
    spend.check_stateless(transaction_context).await?;
    spend.check_historical(state.clone()).await?;

    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_mock_source(1u8);
        spend.check_and_execute(&mut state_tx).await?;
        state_tx.apply();
    }

    // 4. Execute EndBlock
    let end_block = abci::request::EndBlock {
        height: height.try_into().unwrap(),
    };
    ShieldedPool::end_block(&mut state, &end_block).await;

    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.finish_block().await.unwrap();
        state_tx.apply();
    }

    Ok(())
}

/// PoC for issue surfaced in zellic audit: https://github.com/penumbra-zone/penumbra/issues/3859
///
/// Test that 0-value spends with invalid proofs cannot be created.
/// This demonstrates that the constraint system catches invalid inputs during proof generation,
/// providing defense-in-depth against attempts to forge spend proofs.
///
/// The test constructs invalid witness data (zero-value note, random keys, empty compliance paths)
/// and verifies that the prover rejects the inputs as unsatisfiable constraints.
#[tokio::test]
async fn invalid_dummy_spend() -> anyhow::Result<()> {
    let storage = TempStorage::new_with_penumbra_prefixes()
        .await?
        .apply_default_genesis()
        .await?;
    let state = Arc::new(StateDelta::new(storage.latest_snapshot()));

    // Sync the mock client to see genesis notes
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone());
    client.sync_to(0, state.deref()).await?;
    let note = client.notes.values().next().unwrap().clone();

    let note_commitment = note.commit();
    let proof = client.sct.witness(note_commitment).unwrap();
    let root = client.sct.root();

    // Create a zero-value note with the same address and asset (the "dummy" note)
    let note_zero_value = Note::from_parts(
        note.address(),
        Value {
            amount: Amount::from(0u64),
            asset_id: note.asset_id(),
        },
        note.rseed(),
    )?;

    // Create dummy compliance data for the bad proof
    let dummy_compliance_anchor = tct::StateCommitment(Fq::from(0u64));
    let dummy_asset_anchor = tct::StateCommitment(Fq::from(0u64));
    let dummy_compliance_leaf = penumbra_sdk_compliance::ComplianceLeaf {
        address: note.address(),
        key: penumbra_sdk_keys::keys::AddressComplianceKey::new(
            *penumbra_sdk_compliance::BLACK_HOLE_ACK,
        ),
        asset_id: note.asset_id(),
    };
    let dummy_merkle_path = MerklePath { layers: vec![] };

    // Construct private witness with invalid/dummy data
    let ak = VerificationKey::<SpendAuth>::try_from([0u8; 32]).unwrap();
    let nk = NullifierKey(Fq::rand(&mut OsRng));

    // Derive a dummy nullifier using the invalid key
    let dummy_nullifier =
        penumbra_sdk_sct::Nullifier::derive(&nk, 0u64.into(), &note_zero_value.commit());

    // Public inputs for the proof - using real anchor but claiming different balance
    let public = SpendProofPublic {
        anchor: root,
        balance_commitment: note_zero_value.value().commit(Fr::rand(&mut OsRng)),
        nullifier: dummy_nullifier,
        rk: ak.clone(),
        asset_anchor: dummy_asset_anchor,
        compliance_anchor: dummy_compliance_anchor,
        compliance_epk: decaf377::Element::default(),
        compliance_epk_g: decaf377::Element::default(),
        compliance_ciphertext: vec![Fq::from(0u64); 11],
        target_timestamp: 0,
        sender_leaf_hash: tct::StateCommitment(Fq::from(0u64)),
        counterparty_leaf_hash: tct::StateCommitment(Fq::from(0u64)),
    };

    let private = SpendProofPrivate {
        state_commitment_proof: proof,
        note: note_zero_value,
        v_blinding: Fr::rand(&mut OsRng),
        spend_auth_randomizer: Fr::rand(&mut OsRng),
        ak,
        nk,
        asset_path: dummy_merkle_path.clone(),
        asset_position: 0,
        asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf {
            value: Fq::from(0u64),
            next_index: 0,
            next_value: penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
            policy: penumbra_sdk_compliance::AssetPolicy::default_unregulated(),
        },
        is_regulated: false,
        compliance_path: dummy_merkle_path,
        compliance_position: 0,
        user_leaf: dummy_compliance_leaf.clone(),
        compliance_ephemeral_secret: Fr::from(0u64),
        counterparty_leaf: dummy_compliance_leaf,
        tx_blinding_nonce: Fr::from(0u64),
        is_flagged: false,
    };

    // Attempt to prove - this should fail because the constraints are unsatisfiable
    let proof_result = SpendProof::prove(
        Fq::rand(&mut OsRng),
        Fq::rand(&mut OsRng),
        &penumbra_sdk_proof_params::SPEND_PROOF_PROVING_KEY,
        public,
        private,
    );

    // The proof should fail to generate because the constraint system catches the invalid inputs.
    // This is good security - we catch forgery attempts at proof generation time.
    assert!(
        proof_result.is_err(),
        "proof generation should fail for invalid dummy spend inputs"
    );
    let err_msg = format!("{:?}", proof_result.unwrap_err());
    assert!(
        err_msg.contains("Unsatisfiable") || err_msg.contains("SynthesisError"),
        "error should be about unsatisfiable constraints, got: {}",
        err_msg
    );

    Ok(())
}

/// Test that attempting to spend the same note twice is rejected.
///
/// This test builds two valid spend transactions for the same note and verifies
/// that the second one fails with a double-spend error.
#[tokio::test]
async fn spend_duplicate_nullifier_previous_transaction() -> anyhow::Result<()> {
    let storage = TempStorage::new_with_penumbra_prefixes()
        .await?
        .apply_default_genesis()
        .await?;
    let mut state = Arc::new(StateDelta::new(storage.latest_snapshot()));

    let height = 1;

    // Sync the mock client to see genesis notes
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone());
    client.sync_to(0, state.deref()).await?;

    // Get a note to spend (will be spent twice)
    let note = client.notes.values().next().unwrap().clone();
    let sender_address = note.address();
    let recipient_address = test_keys::ADDRESS_1.deref().clone();
    let asset_id = note.asset_id();
    let tct_position = client
        .position(note.commit())
        .expect("note should be in mock client's tree");

    // 1. Simulate BeginBlock and register compliance
    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_block_height(height);
        state_tx.put_block_timestamp(height, tendermint::Time::now());
        state_tx.put_epoch_by_height(
            height,
            Epoch {
                index: 0,
                start_height: 0,
            },
        );

        // Register the asset as unregulated
        common::register_assets_for_compliance(&mut state_tx, &[asset_id]).await?;
        // Register the users for this asset
        common::register_test_users_for_compliance(
            &mut state_tx,
            &[sender_address.clone(), recipient_address.clone()],
            &[asset_id],
        )
        .await?;

        state_tx.record_compliance_anchors(height).await?;

        state_tx.apply();
    }

    // 2. Create and execute first spend transaction
    let mut plan1 = TransactionPlan {
        actions: vec![
            SpendPlan::new(&mut OsRng, note.clone(), tct_position).into(),
            OutputPlan::new(&mut OsRng, note.value(), recipient_address.clone()).into(),
        ],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
        )),
        detection_data: None,
        transaction_parameters: TransactionParameters {
            chain_id: "penumbra-test".to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    // Build with compliance enrichment from state
    let tx1 = client
        .witness_auth_build_with_compliance(&mut plan1, state.deref())
        .await?;

    let transaction_context1 = TransactionContext {
        anchor: client.sct.root(),
        effect_hash: EffectHash(tx1.effect_hash().as_ref().try_into().unwrap()),
    };

    // Get the spend action from the first transaction
    let tx1_body = tx1.transaction_body();
    let spend1 = tx1_body
        .actions
        .iter()
        .find_map(|a| {
            if let penumbra_sdk_transaction::Action::Spend(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .expect("transaction should have a spend action");

    // Execute the first spend (should succeed)
    spend1.check_stateless(transaction_context1).await?;
    spend1.check_historical(state.clone()).await?;

    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_mock_source(1u8);
        spend1.check_and_execute(&mut state_tx).await?;
        state_tx.apply();
    }

    // 3. Create second spend transaction of the SAME note (double spend)
    let mut plan2 = TransactionPlan {
        actions: vec![
            SpendPlan::new(&mut OsRng, note.clone(), tct_position).into(),
            OutputPlan::new(&mut OsRng, note.value(), recipient_address).into(),
        ],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
        )),
        detection_data: None,
        transaction_parameters: TransactionParameters {
            chain_id: "penumbra-test".to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    // Build with compliance enrichment from state
    let tx2 = client
        .witness_auth_build_with_compliance(&mut plan2, state.deref())
        .await?;

    let transaction_context2 = TransactionContext {
        anchor: client.sct.root(),
        effect_hash: EffectHash(tx2.effect_hash().as_ref().try_into().unwrap()),
    };

    // Get the spend action from the second transaction
    let tx2_body = tx2.transaction_body();
    let spend2 = tx2_body
        .actions
        .iter()
        .find_map(|a| {
            if let penumbra_sdk_transaction::Action::Spend(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .expect("transaction should have a spend action");

    // Stateless check should pass (proof is valid)
    spend2.check_stateless(transaction_context2).await?;
    // Historical check should pass (anchor is valid)
    spend2.check_historical(state.clone()).await?;

    // 4. Attempt to execute the double spend - this should fail
    let mut state_tx = state.try_begin_transaction().unwrap();
    state_tx.put_mock_source(2u8);
    let result = spend2.check_and_execute(&mut state_tx).await;

    // The double spend should be rejected
    assert!(result.is_err(), "double spend should have been rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("was already spent"),
        "error should mention 'was already spent', got: {}",
        err_msg
    );

    Ok(())
}
