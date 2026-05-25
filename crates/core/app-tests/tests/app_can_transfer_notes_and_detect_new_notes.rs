use {
    self::common::BuilderExt,
    anyhow::anyhow,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    decaf377::Fr,
    penumbra_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
    },
    penumbra_sdk_asset::{Value, BASE_ASSET_ID},
    penumbra_sdk_keys::test_keys,
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_num::Amount,
    penumbra_sdk_proto::DomainType,
    penumbra_sdk_sct::component::tree::SctRead as _,
    penumbra_sdk_shielded_pool::{ShieldedInputPlan, ShieldedOutputPlan, TransferPlan},
    penumbra_sdk_transaction::{
        memo::MemoPlaintext, plan::MemoPlan, TransactionParameters, TransactionPlan,
    },
    rand_core::OsRng,
    tap::TapFallible,
};

mod common;

#[tokio::test]
async fn app_can_transfer_notes_and_detect_new_notes() -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;
    let mut test_node = {
        let app_state = AppState::Content(
            genesis::Content::default().with_chain_id(TestNode::<()>::CHAIN_ID.to_string()),
        );
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_penumbra_auto_app_state(app_state)?
            .init_chain(consensus)
            .await
            .tap_ok(|e| tracing::info!(hash = %e.last_app_hash_hex(), "finished init chain"))?
    };

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
    let mut spend = spend;
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
    let mut outputs = [output, change];
    common::align_transfer_planning_metadata(std::slice::from_mut(&mut spend), &mut outputs);
    let [output, change] = outputs;
    let transfer = TransferPlan::new(vec![spend], vec![output, change], Fr::from(1u64))?;

    let mut plan = TransactionPlan {
        actions: vec![transfer.into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.clone()),
        )),
        detection_data: None,
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    let tx = client
        .witness_auth_build_with_compliance(&mut plan, storage.latest_snapshot())
        .await?;

    let pre_tx_snapshot = storage.latest_snapshot();
    test_node
        .block()
        .with_data(vec![tx.encode_to_vec()])
        .execute()
        .await?;
    let post_tx_snapshot = storage.latest_snapshot();

    for nf in tx.spent_nullifiers() {
        assert!(pre_tx_snapshot.spend_info(nf).await?.is_none());
        assert!(post_tx_snapshot.spend_info(nf).await?.is_some());
    }

    client.sync_to_latest(post_tx_snapshot).await?;

    for output_nc in tx.state_commitments() {
        assert!(client.notes.contains_key(&output_nc));
    }

    drop(storage);
    drop(guard);

    Ok(())
}
