use {
    anyhow::Context,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    futures::StreamExt,
    penumbra_sdk_app::{
        genesis::{AppState, Content},
        server::consensus::Consensus,
    },
    penumbra_sdk_asset::{BASE_ASSET_DENOM, BASE_ASSET_ID},
    penumbra_sdk_keys::{keys::AddressIndex, test_keys},
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_proto::{
        view::v1::{
            view_service_client::ViewServiceClient, view_service_server::ViewServiceServer,
            StatusRequest,
        },
        DomainType,
    },
    penumbra_sdk_shielded_pool::genesis::Allocation,
    penumbra_sdk_view::ViewClient,
    penumbra_sdk_wallet::plan,
    rand_core::OsRng,
    std::ops::Deref,
    tap::{Tap, TapFallible},
    tokio::time,
};

mod common;

const COUNT: usize = 5;

async fn wait_for_view_sync(
    view_client: &mut ViewServiceClient<ViewServiceServer<penumbra_sdk_view::ViewServer>>,
    min_height: u64,
) -> anyhow::Result<()> {
    let mut status_stream = ViewClient::status_stream(view_client).await?;
    while let Some(status) = status_stream.next().await.transpose()? {
        tracing::info!(?status, "view client received status stream response");
    }
    let status = view_client.status(StatusRequest {}).await?.into_inner();
    assert!(!status.catching_up, "view client should not be catching up");
    assert!(
        status.full_sync_height >= min_height,
        "view client should be synced to at least height {min_height}, got {}",
        status.full_sync_height
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn app_can_sweep_a_collection_of_small_notes() -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber_with_env_filter("info".into());
    let storage = TempStorage::new_with_penumbra_prefixes().await?;
    let proxy = penumbra_sdk_mock_tendermint_proxy::TestNodeProxy::new::<Consensus>();

    let allocations = {
        let dust = Allocation {
            raw_amount: 1_u128.into(),
            raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
            address: test_keys::ADDRESS_0.to_owned(),
        };
        std::iter::repeat(dust).take(COUNT).collect()
    };

    let mut test_node = {
        let content = Content {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
                allocations,
                ..Default::default()
            },
            ..Default::default()
        };
        let app_state = AppState::Content(content);
        let app_state = serde_json::to_vec(&app_state).unwrap();
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .app_state(app_state)
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

    time::sleep(time::Duration::from_millis(50)).await;

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

    wait_for_view_sync(&mut view_client, 10).await?;

    client.sync_to_latest(storage.latest_snapshot()).await?;
    assert_eq!(
        client
            .notes_by_asset(BASE_ASSET_ID.deref().to_owned())
            .count(),
        COUNT,
        "client wallet should have {COUNT} notes before sweeping"
    );

    loop {
        client.sync_to_latest(storage.latest_snapshot()).await?;

        let plans = plan::sweep(&mut view_client, OsRng)
            .await
            .context("constructing sweep plans")?;
        if plans.is_empty() {
            break;
        }
        for plan in plans {
            let tx = client.witness_auth_build(&plan).await?;
            test_node
                .block()
                .with_data(vec![tx.encode_to_vec()])
                .execute()
                .await?;
            client.sync_to_latest(storage.latest_snapshot()).await?;
            wait_for_view_sync(&mut view_client, storage.latest_snapshot().version()).await?;
        }
    }

    let post_sweep_notes = view_client.unspent_notes_by_address_and_asset().await?;
    let final_note_count = post_sweep_notes
        .get(&AddressIndex::from(0))
        .expect("test wallet could not find any notes")
        .get(&*BASE_ASSET_ID)
        .map(Vec::len)
        .unwrap_or_default();
    assert!(
        final_note_count < COUNT,
        "sweep should reduce note count from {COUNT}, got {final_note_count}"
    );

    Ok(())
        .tap(|_| drop(test_node))
        .tap(|_| drop(storage))
        .tap(|_| drop(guard))
}
