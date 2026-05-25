use {
    self::common::BuilderExt,
    anyhow::Context,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    penumbra_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
    },
    penumbra_sdk_asset::BASE_ASSET_ID,
    penumbra_sdk_keys::{keys::AddressIndex, test_keys},
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_proto::{
        view::v1::{
            view_service_client::ViewServiceClient, view_service_server::ViewServiceServer,
            StatusRequest, StatusResponse,
        },
        DomainType,
    },
    penumbra_sdk_view::{NoteManager, SpendableNoteRecord, TransferPlanningResult, ViewClient},
    tap::{Tap, TapFallible},
};

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn view_server_can_be_served_on_localhost() -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;
    let proxy = penumbra_sdk_mock_tendermint_proxy::TestNodeProxy::new::<Consensus>();

    let mut test_node = {
        let app_state = AppState::Content(
            genesis::Content::default().with_chain_id(TestNode::<()>::CHAIN_ID.to_string()),
        );
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_penumbra_auto_app_state(app_state)?
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
        let make_svc = penumbra_sdk_app::rpc::routes(storage.as_ref(), proxy, false)?
            .into_axum_router()
            .layer(tower_http::cors::CorsLayer::permissive())
            .into_make_service();
        listener.set_nonblocking(true)?;
        let server = axum_server::from_tcp(listener).serve(make_svc);
        tokio::spawn(async { server.await.expect("grpc server returned an error") });
    };
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let view_server = penumbra_sdk_view::ViewServer::load_or_initialize(
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
        assert_eq!(
            status,
            StatusResponse {
                full_sync_height: 10,
                partial_sync_height: 10,
                catching_up: false,
            }
        );
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
    let mut plan = match planning_result {
        TransferPlanningResult::Ready { transaction_plan } => transaction_plan,
        other => anyhow::bail!("expected ready transfer plan, got {other:?}"),
    };

    client.sync_to_latest(storage.latest_snapshot()).await?;
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
        use penumbra_sdk_sct::component::tree::SctRead as _;
        assert!(pre_tx_snapshot.spend_info(nf).await?.is_none());
        assert!(post_tx_snapshot.spend_info(nf).await?.is_some());
    }

    {
        use futures::StreamExt;
        let mut status_stream = ViewClient::status_stream(&mut view_client).await?;
        while let Some(status) = status_stream.next().await.transpose()? {
            tracing::info!(?status, "view client received status stream response");
        }
        let status = view_client.status(StatusRequest {}).await?.into_inner();
        assert_eq!(
            status,
            StatusResponse {
                full_sync_height: 11,
                partial_sync_height: 11,
                catching_up: false,
            }
        );
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
