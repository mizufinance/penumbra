use {
    self::common::BuilderExt,
    anyhow::anyhow,
    cnidarium::{ArcStateDeltaExt, StateDelta, TempStorage},
    cnidarium_component::ActionHandler as _,
    common::TempStorageExt as _,
    penumbra_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
    },
    penumbra_sdk_asset::asset,
    penumbra_sdk_community_pool::{CommunityPoolDeposit, StateReadExt},
    penumbra_sdk_compliance::ComplianceRegistryWrite,
    penumbra_sdk_keys::test_keys,
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_num::Amount,
    penumbra_sdk_sct::component::{clock::EpochManager as _, source::SourceContext as _},
    penumbra_sdk_shielded_pool::SpendPlan,
    penumbra_sdk_transaction::{TransactionParameters, TransactionPlan},
    penumbra_sdk_txhash::{EffectHash, EffectingData as _, TransactionContext},
    rand_core::OsRng,
    std::{collections::BTreeMap, ops::Deref, sync::Arc},
    tap::{Tap, TapFallible},
    tracing::info,
};

mod common;

/// Exercises that the app can deposit a note into the community pool.
#[tokio::test]
async fn app_can_deposit_into_community_pool() -> anyhow::Result<()> {
    // Install a test logger, and acquire some temporary storage.
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;

    // Define our application state, and start the test node.
    let _test_node = {
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

    // Get state for compliance registration
    let mut state = Arc::new(StateDelta::new(storage.latest_snapshot()));

    // Sync the mock client, using the test wallet's spend key, to the latest snapshot.
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone());
    client.sync_to(0, state.deref()).await?;
    info!(client.notes = %client.notes.len(), "mock client synced to test storage");

    // Take one of the test wallet's notes, and prepare to deposit it in the community pool.
    let note = client
        .notes
        .values()
        .cloned()
        .next()
        .ok_or_else(|| anyhow!("mock client had no note"))?;

    let sender_address = note.address();
    let asset_id = note.asset_id();

    // Register asset and user for compliance (required for spend proofs)
    let height = 1u64;
    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_block_height(height);
        state_tx.put_block_timestamp(height, tendermint::Time::now());
        state_tx.put_epoch_by_height(
            height,
            penumbra_sdk_sct::epoch::Epoch {
                index: 0,
                start_height: 0,
            },
        );

        common::register_assets_for_compliance(&mut state_tx, &[asset_id]).await?;
        common::register_test_users_for_compliance(
            &mut state_tx,
            &[sender_address.clone()],
            &[asset_id],
        )
        .await?;

        state_tx.record_compliance_anchors(height).await?;

        state_tx.apply();
    }

    // Create a community pool transaction.
    let mut plan = {
        let value = note.value();
        let spend = SpendPlan::new(
            &mut OsRng,
            note.clone(),
            client
                .position(note.commit())
                .ok_or_else(|| anyhow!("input note commitment was unknown to mock client"))?,
        )
        .into();
        let deposit = CommunityPoolDeposit { value }.into();
        TransactionPlan {
            actions: vec![spend, deposit],
            // Now fill out the remaining parts of the transaction needed for verification:
            memo: None,
            detection_data: None, // We'll set this automatically below
            transaction_parameters: TransactionParameters {
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                ..Default::default()
            },
        }
    };
    plan.populate_detection_data(OsRng, Default::default());
    let tx = client
        .witness_auth_build_with_compliance(&mut plan, state.deref())
        .await?;

    info!("Transaction built successfully with compliance proofs");

    // Create transaction context for verification
    let transaction_context = TransactionContext {
        anchor: client.sct.root(),
        effect_hash: EffectHash(tx.effect_hash().as_ref().try_into().unwrap()),
    };

    // Get pre-tx balance from state
    let pre_tx_balance: BTreeMap<asset::Id, Amount> = state.community_pool_balance().await?;
    let id = note.asset_id();
    let pre_tx_amount = pre_tx_balance.get(&id).copied().unwrap_or_default();

    // Get the transaction body to access actions
    let tx_body = tx.transaction_body();

    // Verify and execute spend action
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

    spend.check_stateless(transaction_context).await?;
    info!("Spend proof verified successfully");

    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_mock_source(1u8);
        spend.check_and_execute(&mut state_tx).await?;
        state_tx.apply();
    }
    info!("Spend action executed successfully");

    // Verify and execute deposit action
    let deposit = tx_body
        .actions
        .iter()
        .find_map(|a| {
            if let penumbra_sdk_transaction::Action::CommunityPoolDeposit(d) = a {
                Some(d)
            } else {
                None
            }
        })
        .expect("transaction should have a deposit action");

    deposit.check_stateless(()).await?;
    {
        let mut state_tx = state.try_begin_transaction().unwrap();
        deposit.check_and_execute(&mut state_tx).await?;
        state_tx.apply();
    }
    info!("CommunityPoolDeposit action executed successfully");

    // Assert that the community pool balance looks correct for the deposited asset id
    {
        type Balance = BTreeMap<asset::Id, Amount>;

        let post_tx_balance: Balance = state.community_pool_balance().await?;
        let post_tx_amount = post_tx_balance.get(&id).copied().unwrap_or_default();

        assert_eq!(
            pre_tx_amount + note.amount(),
            post_tx_amount,
            "community pool balance should include the deposited note"
        );

        let count_other_assets_in_pool = |balance: &Balance| {
            balance
                .into_iter()
                // Skip the amount for our note's asset id.
                .filter(|(&entry_id, _)| entry_id != id)
                .map(|(_, &amount)| amount)
                .sum::<Amount>()
        };
        assert_eq!(
            count_other_assets_in_pool(&pre_tx_balance),
            count_other_assets_in_pool(&post_tx_balance),
            "other community pool balance amounts should not have changed"
        );
    }

    info!("All assertions passed - deposit into community pool works correctly with compliance");

    // Free our temporary storage.
    Ok(())
        .tap(|_| drop(_test_node))
        .tap(|_| drop(storage))
        .tap(|_| drop(guard))
}
