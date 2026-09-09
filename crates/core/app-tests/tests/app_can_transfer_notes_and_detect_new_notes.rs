use {
    anyhow::anyhow,
    decaf377::Fr,
    rand_core::OsRng,
    shieldd_sdk_app::genesis::{self, AppState},
    shieldd_sdk_app::test_support::{TestHost, TEST_CHAIN_ID},
    shieldd_sdk_asset::{Value, BASE_ASSET_ID},
    shieldd_sdk_keys::test_keys,
    shieldd_sdk_mock_client::MockClient,
    shieldd_sdk_num::Amount,
    shieldd_sdk_proto::DomainType,
    shieldd_sdk_sct::component::tree::SctRead as _,
    shieldd_sdk_shielded_pool::{ShieldedInputPlan, ShieldedOutputPlan},
    shieldd_sdk_transaction::{memo::MemoPlaintext, plan::MemoPlan, TransactionParameters},
};

mod common;

#[tokio::test]
async fn app_can_transfer_notes_and_detect_new_notes() -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = common::new_storage().await?;
    let mut test_node = {
        let app_state =
            AppState::Content(genesis::Content::default().with_chain_id(TEST_CHAIN_ID.to_string()));
        TestHost::new(
            storage.as_ref().clone(),
            app_state,
            tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?,
        )
        .await?
    };

    test_node.execute(Vec::new()).await?;

    let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    let input_note = client
        .notes
        .values()
        .filter(|note| {
            note.asset_id() == *BASE_ASSET_ID && note.address() == test_keys::ADDRESS_0.clone()
        })
        .cloned()
        .next()
        .ok_or_else(|| anyhow!("mock client had no note"))?;
    let spend = ShieldedInputPlan::new(
        &mut OsRng,
        input_note.clone(),
        client
            .position(input_note.commit())
            .ok_or_else(|| anyhow!("input note commitment was unknown to mock client"))?,
    );
    let send_amount = Amount::from(1u64);
    let change_amount = input_note
        .amount()
        .checked_sub(&send_amount)
        .expect("test input note amount must cover the requested send amount");
    let output = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: send_amount,
            asset_id: input_note.asset_id(),
        },
        test_keys::ADDRESS_1.clone(),
    );
    let change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: change_amount,
            asset_id: input_note.asset_id(),
        },
        input_note.address(),
    );
    let transfer = shieldd_sdk_mock_client::TransferIntent {
        spends: vec![spend],
        outputs: vec![output, change],
        value_blinding: Fr::from(1u64),
    };

    let intent = shieldd_sdk_mock_client::TransactionIntent {
        nullifier_window: None,
        actions: vec![transfer.into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.clone()),
        )),
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TEST_CHAIN_ID.to_string(),
            ..Default::default()
        },
    };

    let plan = client
        .complete_intent(intent, storage.latest_snapshot())
        .await?;
    let tx = client.witness_auth_build(&plan).await?;

    let pre_tx_snapshot = storage.latest_snapshot();
    test_node.execute(vec![tx.encode_to_vec()]).await?;
    let post_tx_snapshot = storage.latest_snapshot();

    for nf in tx.spent_nullifiers() {
        assert!(!pre_tx_snapshot.is_nullifier_spent(nf).await?);
        assert!(post_tx_snapshot.is_nullifier_spent(nf).await?);
    }

    client.sync_to_latest(post_tx_snapshot).await?;

    for output_nc in tx.transfers().flat_map(|transfer| {
        transfer
            .body
            .outputs
            .iter()
            .map(|output| output.note_payload.note_commitment)
    }) {
        assert!(client.notes.contains_key(&output_nc));
    }

    drop(storage);
    drop(guard);

    Ok(())
}
