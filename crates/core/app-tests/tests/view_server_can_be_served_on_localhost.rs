use {
    self::common::BuilderExt,
    anyhow::Context,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    shieldd_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
    },
    shieldd_sdk_asset::BASE_ASSET_ID,
    shieldd_sdk_keys::{keys::AddressIndex, test_keys},
    shieldd_sdk_mock_client::MockClient,
    shieldd_sdk_mock_consensus::TestNode,
    shieldd_sdk_proto::{
        view::v1::{
            view_service_client::ViewServiceClient, view_service_server::ViewServiceServer,
            StatusRequest,
        },
        DomainType,
    },
    shieldd_sdk_view::{NoteManager, NoteManagerPlanningResult, SpendableNoteRecord, ViewClient},
    tap::{Tap, TapFallible},
};

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "expensive: real release-mode Gnark proof generation"]
async fn view_server_can_be_served_on_localhost() -> anyhow::Result<()> {
    shieldd_sdk_shielded_pool::gnark::require_proof_test_runtime(
        shieldd_sdk_shielded_pool::gnark::ProofTestFamily::Transfer,
    )?;
    run_view_server_case(true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_and_rpc_planning_have_equivalent_outcomes() -> anyhow::Result<()> {
    run_view_server_case(false).await
}

async fn run_view_server_case(build_proof: bool) -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_shieldd_prefixes().await?;
    let proxy = shieldd_sdk_mock_tendermint_proxy::TestNodeProxy::new::<Consensus>();

    let mut test_node = {
        let app_state = AppState::Content(
            genesis::Content::default().with_chain_id(TestNode::<()>::CHAIN_ID.to_string()),
        );
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_shieldd_auto_app_state(app_state)?
            .on_block(proxy.on_block_callback())
            .init_chain(consensus)
            .await
            .tap_ok(|e| tracing::info!(hash = %e.last_app_hash_hex(), "finished init chain"))?
    };

    let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?
        .tap(
            |c| tracing::info!(client.notes = %c.notes.len(), "mock client synced to test storage"),
        );

    test_node
        .fast_forward(10)
        .tap(|_| tracing::debug!("fast forwarding past genesis"))
        .await?;

    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let local_addr = listener.local_addr()?;
    let grpc_url = format!("http://{local_addr}")
        .parse::<url::Url>()?
        .tap(|url| tracing::debug!(%url, "parsed grpc url"));

    {
        let make_svc = shieldd_sdk_app::rpc::routes(storage.as_ref(), proxy, false)?
            .into_axum_router()
            .layer(tower_http::cors::CorsLayer::permissive())
            .into_make_service();
        listener.set_nonblocking(true)?;
        let server = axum_server::from_tcp(listener).serve(make_svc);
        tokio::spawn(async { server.await.expect("grpc server returned an error") });
    };
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let view_server = shieldd_sdk_view::ViewServer::load_or_initialize(
        None::<&camino::Utf8Path>,
        None::<&camino::Utf8Path>,
        &*test_keys::FULL_VIEWING_KEY,
        grpc_url,
    )
    .await
    .map(ViewServiceServer::new)
    .context("initializing view server")?;

    let mut view_client = ViewServiceClient::new(view_server);

    {
        use futures::StreamExt;
        let mut status_stream = ViewClient::status_stream(&mut view_client).await?;
        while let Some(status) = status_stream.next().await.transpose()? {
            tracing::info!(?status, "view client received status stream response");
        }
        let status = view_client.status(StatusRequest {}).await?.into_inner();
        assert_eq!(status.sync_height, 10);
        assert!(!status.catching_up);
        assert!(status.latest_block_timestamp > 0);
    }

    let notes = view_client.unspent_notes_by_address_and_asset().await?;
    let staking_notes = notes
        .get(&AddressIndex::default())
        .expect("test wallet could not find any notes")
        .get(&*BASE_ASSET_ID)
        .expect("test wallet did not contain any base-asset notes");
    let SpendableNoteRecord { note, .. } = staking_notes[0].to_owned();

    let gas_prices = ViewClient::gas_prices(&mut view_client).await?;
    let mut note_manager = NoteManager::new(rand_core::OsRng);
    note_manager.set_gas_prices(gas_prices);
    let planning_result = note_manager
        .plan_transfer(
            &mut view_client,
            AddressIndex::default(),
            note.value(),
            test_keys::ADDRESS_1.clone(),
        )
        .await?;
    let plan = match planning_result {
        NoteManagerPlanningResult::Ready { transaction_plan } => transaction_plan,
        other => anyhow::bail!("expected ready transfer plan, got {other:?}"),
    };

    use shieldd_sdk_proto::view::v1::{
        transaction_planner_request::TransferOutput, TransactionPlannerRequest,
    };
    use shieldd_sdk_transaction::{ActionPlan, TransactionPlan};
    let request = TransactionPlannerRequest {
        source: Some(AddressIndex::default().into()),
        outputs: vec![TransferOutput {
            value: Some(note.value().into()),
            address: Some(test_keys::ADDRESS_1.clone().into()),
        }],
        ..Default::default()
    };
    let local_plan: TransactionPlan = view_client
        .transaction_planner(request.clone())
        .await?
        .into_inner()
        .plan
        .context("planner response must contain a complete plan")?
        .try_into()?;
    assert_eq!(
        plan.transaction_parameters.encode_to_vec(),
        local_plan.transaction_parameters.encode_to_vec()
    );
    assert_eq!(plan.nullifier_window, local_plan.nullifier_window);
    let (ActionPlan::Transfer(remote), ActionPlan::Transfer(local)) =
        (&plan.actions[0], &local_plan.actions[0])
    else {
        anyhow::bail!("expected transfer actions from both planning paths");
    };
    assert_eq!(
        remote
            .spends
            .iter()
            .map(|spend| spend.note.commit())
            .collect::<Vec<_>>(),
        local
            .spends
            .iter()
            .map(|spend| spend.note.commit())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        remote
            .outputs
            .iter()
            .map(|output| (output.value, output.dest_address.clone()))
            .collect::<Vec<_>>(),
        local
            .outputs
            .iter()
            .map(|output| (output.value, output.dest_address.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        remote.compliance.witness.asset.root,
        local.compliance.witness.asset.root
    );
    assert_eq!(
        remote.compliance.witness.user_root,
        local.compliance.witness.user_root
    );
    let mut insufficient = request;
    let impossible = shieldd_sdk_asset::Value {
        amount: u64::MAX.into(),
        asset_id: *BASE_ASSET_ID,
    };
    insufficient.outputs[0].value = Some(impossible.into());
    let error = view_client
        .transaction_planner(insufficient)
        .await
        .expect_err("insufficient funds must fail");
    assert!(error.message().contains("insufficient balance"), "{error}");
    assert!(matches!(
        note_manager
            .plan_transfer(
                &mut view_client,
                AddressIndex::default(),
                impossible,
                test_keys::ADDRESS_1.clone()
            )
            .await?,
        NoteManagerPlanningResult::InsufficientBalance
    ));
    if !build_proof {
        return Ok(());
    }

    client.sync_to_latest(storage.latest_snapshot()).await?;
    let tx = client.witness_auth_build(&plan).await?;

    let pre_tx_snapshot = storage.latest_snapshot();
    test_node
        .block()
        .with_data(vec![tx.encode_to_vec()])
        .execute()
        .await?;
    let post_tx_snapshot = storage.latest_snapshot();

    for nf in tx.spent_nullifiers() {
        use shieldd_sdk_sct::component::tree::SctRead as _;
        assert!(!pre_tx_snapshot.is_nullifier_spent(nf).await?);
        assert!(post_tx_snapshot.is_nullifier_spent(nf).await?);
    }

    {
        use futures::StreamExt;
        let mut status_stream = ViewClient::status_stream(&mut view_client).await?;
        while let Some(status) = status_stream.next().await.transpose()? {
            tracing::info!(?status, "view client received status stream response");
        }
        let status = view_client.status(StatusRequest {}).await?.into_inner();
        assert_eq!(status.sync_height, 11);
        assert!(!status.catching_up);
        assert!(status.latest_block_timestamp > 0);
    }

    let post_tx_notes = view_client.unspent_notes_by_address_and_asset().await?;
    assert!(
        post_tx_notes
            .get(&AddressIndex::default())
            .expect("test wallet could not find any notes")
            .get(&*BASE_ASSET_ID)
            .is_none(),
        "source address should not be associated with any base-asset notes after tx"
    );
    assert_eq!(
        post_tx_notes
            .get(&AddressIndex::from(1))
            .expect("test wallet could not find any notes")
            .get(&*BASE_ASSET_ID)
            .map(Vec::len),
        Some(1),
        "destination address should have a base-asset note after tx"
    );

    Ok(())
        .tap(|_| drop(test_node))
        .tap(|_| drop(storage))
        .tap(|_| drop(guard))
}
