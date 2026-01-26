//! Tests that the Planner correctly enriches multi-spend transactions with compliance data.
//!
//! This test verifies that the batch compliance query system works for transactions
//! with multiple spends, which previously caused hangs due to creating many gRPC connections.

use {
    anyhow::Context,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    penumbra_sdk_app::{
        genesis::{AppState, Content},
        server::consensus::Consensus,
    },
    penumbra_sdk_asset::{STAKING_TOKEN_ASSET_ID, STAKING_TOKEN_DENOM},
    penumbra_sdk_keys::{keys::AddressIndex, test_keys},
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_num::Amount,
    penumbra_sdk_proto::{
        view::v1::{
            view_service_client::ViewServiceClient, view_service_server::ViewServiceServer,
            GasPricesRequest, StatusRequest, StatusResponse,
        },
        DomainType,
    },
    penumbra_sdk_shielded_pool::genesis::Allocation,
    penumbra_sdk_view::{Planner, SpendableNoteRecord, ViewClient},
    std::ops::Deref,
    tap::{Tap, TapFallible},
    tokio::time,
};

mod common;

/// Number of notes to create at genesis for multi-spend testing
const NOTE_COUNT: usize = 4;

/// Test that multiple spends (4 in this case) work correctly with batched compliance queries.
/// This verifies the fix for the sweep test hang - multi-spend enrichment now uses batch queries.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn planner_multi_spend_with_batched_compliance() -> anyhow::Result<()> {
    // Install a test logger, acquire some temporary storage, and start the test node.
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;

    // Instantiate a mock tendermint proxy, which we will connect to the test node.
    let proxy = penumbra_sdk_mock_tendermint_proxy::TestNodeProxy::new::<Consensus>();

    // Define allocations to the test address, as many small notes.
    let allocations = {
        let note = Allocation {
            raw_amount: 1_000_000u128.into(), // 1M per note
            raw_denom: STAKING_TOKEN_DENOM.deref().base_denom().denom,
            address: test_keys::ADDRESS_0.to_owned(),
        };
        std::iter::repeat(note).take(NOTE_COUNT).collect()
    };

    // Start the test node with custom genesis allocations.
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

    // Sync the mock client, using the test wallet's spend key, to the latest snapshot.
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?
        .tap(
            |c| tracing::info!(client.notes = %c.notes.len(), "mock client synced to test storage"),
        );

    // Jump ahead a few blocks.
    test_node
        .fast_forward(10)
        .tap(|_| tracing::debug!("fast forwarding past genesis"))
        .await?;

    // Use port 0 to let the OS assign an available port dynamically
    let make_svc = penumbra_sdk_app::rpc::routes(
        storage.as_ref(),
        proxy,
        false, /*enable_expensive_rpc*/
    )?
    .into_axum_router()
    .layer(tower_http::cors::CorsLayer::permissive())
    .into_make_service()
    .tap(|_| tracing::debug!("initialized rpc service"));

    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let local_addr = listener.local_addr()?;
    let grpc_url = format!("http://{}", local_addr)
        .parse::<url::Url>()?
        .tap(|url| tracing::debug!(%url, "parsed grpc url"));

    // Spawn the server-side view server.
    {
        let server = axum_server::from_tcp(listener).serve(make_svc);
        tokio::spawn(async { server.await.expect("grpc server returned an error") })
            .tap(|_| tracing::debug!("grpc server is running"))
    };

    // Wait for the server to start listening.
    time::sleep(time::Duration::from_secs(1)).await;

    // Spawn the client-side view server
    let view_server = {
        penumbra_sdk_view::ViewServer::load_or_initialize(
            None::<&camino::Utf8Path>,
            None::<&camino::Utf8Path>,
            &*test_keys::FULL_VIEWING_KEY,
            grpc_url,
        )
        .await
        .map(ViewServiceServer::new)
        .context("initializing view server")?
    };

    // Create a view client
    let mut view_client = ViewServiceClient::new(view_server);

    // Sync the view client to the chain.
    {
        use futures::StreamExt;
        let mut status_stream = ViewClient::status_stream(&mut view_client).await?;
        while let Some(status) = status_stream.next().await.transpose()? {
            tracing::info!(?status, "view client received status stream response");
        }
        // Confirm that the status is as expected: synced up to height 10.
        // (Genesis is height 0, fast_forward(10) reaches height 10)
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

    // Get the notes. We expect to have the genesis allocation notes.
    let notes = view_client.unspent_notes_by_address_and_asset().await?;
    let staking_notes = notes
        .get(&AddressIndex::default())
        .expect("test wallet could not find any notes")
        .get(&*STAKING_TOKEN_ASSET_ID)
        .expect("test wallet did not contain any staking tokens");

    // Verify we have at least 4 notes to spend
    assert!(
        staking_notes.len() >= 4,
        "need at least 4 notes for multi-spend test, got {}",
        staking_notes.len()
    );

    // Build a multi-spend transaction using the Planner.
    // This is the key test - Planner.plan() will call enrich_plan_with_compliance
    // which will batch all the compliance queries into a single gRPC call.
    let plan = {
        let gas_prices = view_client
            .gas_prices(GasPricesRequest {})
            .await?
            .into_inner()
            .gas_prices
            .expect("gas prices must be available")
            .try_into()?;

        let mut planner = Planner::new(rand_core::OsRng);
        planner.set_gas_prices(gas_prices);

        // Spend 4 notes explicitly
        let mut total_value = Amount::zero();
        for (i, note_record) in staking_notes.iter().take(4).enumerate() {
            let SpendableNoteRecord { note, position, .. } = note_record.to_owned();
            tracing::info!(i, ?position, value = ?note.value(), "adding spend to plan");
            planner.spend(note.clone(), position);
            total_value = total_value + note.value().amount;
        }

        // Output to ADDRESS_1 (minus some for fees)
        let output_value = penumbra_sdk_asset::Value {
            amount: total_value - Amount::from(1000u64), // leave room for fees
            asset_id: *STAKING_TOKEN_ASSET_ID,
        };
        planner.output(output_value, test_keys::ADDRESS_1.deref().clone());

        // Plan the transaction - this triggers batch compliance enrichment
        tracing::info!("planning multi-spend transaction...");
        planner
            .plan(&mut view_client, AddressIndex::default())
            .await?
    };

    // Verify the plan has 4 spends
    let spend_count = plan
        .actions
        .iter()
        .filter(|a| matches!(a, penumbra_sdk_transaction::plan::ActionPlan::Spend(_)))
        .count();
    assert_eq!(spend_count, 4, "plan should have 4 spends");

    // Build and execute the transaction
    client.sync_to_latest(storage.latest_snapshot()).await?;
    // Use witness_auth_build because Planner.plan() already enriched with compliance
    let tx = client.witness_auth_build(&plan).await?;

    // Verify all spends have compliance data set
    for action in tx.transaction_body.actions.iter() {
        if let penumbra_sdk_transaction::Action::Spend(spend) = action {
            // Check that compliance fields are populated
            assert!(
                !spend.body.compliance_ciphertext.is_empty(),
                "spend compliance ciphertext should be populated"
            );
        }
    }

    // Execute the transaction
    let pre_tx_snapshot = storage.latest_snapshot();
    test_node
        .block()
        .with_data(vec![tx.encode_to_vec()])
        .execute()
        .await?;
    let post_tx_snapshot = storage.latest_snapshot();

    // Check that the nullifiers were spent
    for nf in tx.spent_nullifiers() {
        use penumbra_sdk_sct::component::tree::SctRead as _;
        assert!(pre_tx_snapshot.spend_info(nf).await?.is_none());
        assert!(post_tx_snapshot.spend_info(nf).await?.is_some());
    }

    tracing::info!("multi-spend transaction with batched compliance succeeded!");

    Ok(())
        .tap(|_| drop(test_node))
        .tap(|_| drop(storage))
        .tap(|_| drop(guard))
}
