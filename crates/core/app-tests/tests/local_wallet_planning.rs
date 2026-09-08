use anyhow::Context;
use cnidarium::TempStorage;
use shieldd_sdk_app::{
    genesis::{AppState, Content},
    test_support::{TestHost, TEST_CHAIN_ID},
};
use shieldd_sdk_asset::{Value, BASE_ASSET_ID};
use shieldd_sdk_keys::{keys::AddressIndex, test_keys};
use shieldd_sdk_transaction::ActionPlan;
use shieldd_sdk_view::{
    planning_io::PlanningIo, NoteManager, NoteManagerPlanningResult, Storage, StoragePlanningIo,
};
mod common;

#[tokio::test]
async fn local_wallet_plan_preserves_inputs_outputs_context_and_insufficient_balance(
) -> anyhow::Result<()> {
    let chain = TempStorage::new().await?;
    let mut host = TestHost::new(
        chain.as_ref().clone(),
        AppState::Content(Content::default().with_chain_id(TEST_CHAIN_ID.into())),
        tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?,
    )
    .await?;
    host.execute(vec![]).await?;
    let wallet = Storage::initialize(
        None::<&camino::Utf8Path>,
        (*test_keys::FULL_VIEWING_KEY).clone(),
        shieldd_sdk_app::params::AppParameters {
            chain_id: TEST_CHAIN_ID.into(),
            ..Default::default()
        },
    )
    .await?;
    common::scan_latest(&chain, &wallet).await?;
    let mut reader = StoragePlanningIo::new(wallet.clone()).await?;
    assert!(reader.latest_block_timestamp().await? > 0);
    let notes = wallet
        .notes(
            false,
            Some(*BASE_ASSET_ID),
            Some(AddressIndex::default()),
            None,
        )
        .await?;
    let note = notes.first().context("wallet funded at genesis")?;
    let value = Value {
        amount: 1u64.into(),
        asset_id: *BASE_ASSET_ID,
    };
    let mut manager = NoteManager::new(rand_core::OsRng);
    manager.set_gas_prices(wallet.gas_prices().await?);
    let result = manager
        .plan_transfer(
            &mut reader,
            AddressIndex::default(),
            value,
            test_keys::ADDRESS_1.clone(),
        )
        .await?;
    let NoteManagerPlanningResult::Ready {
        transaction_plan: plan,
    } = result
    else {
        anyhow::bail!("expected ready transfer plan: {result:?}");
    };
    assert_eq!(plan.transaction_parameters.chain_id, TEST_CHAIN_ID);
    assert_eq!(
        plan.nullifier_window,
        Some(wallet.nullifier_window().await?)
    );
    let ActionPlan::Transfer(transfer) = &plan.actions[0] else {
        anyhow::bail!("expected transfer");
    };
    assert_eq!(transfer.spends.len(), 1);
    assert_eq!(transfer.spends[0].note.commit(), note.note.commit());
    assert!(transfer
        .outputs
        .iter()
        .any(|o| o.value == value && o.dest_address == test_keys::ADDRESS_1.clone()));
    let inputs: shieldd_sdk_num::Amount = transfer.spends.iter().map(|s| s.note.amount()).sum();
    let outputs: shieldd_sdk_num::Amount = transfer.outputs.iter().map(|o| o.value.amount).sum();
    assert_eq!(inputs, outputs);
    assert!(plan.memo.is_some());
    wallet.witness_plan(&plan).await?;
    assert!(matches!(
        manager
            .plan_transfer(
                &mut reader,
                AddressIndex::default(),
                Value {
                    amount: u64::MAX.into(),
                    asset_id: *BASE_ASSET_ID
                },
                test_keys::ADDRESS_1.clone()
            )
            .await?,
        NoteManagerPlanningResult::InsufficientBalance
    ));
    let issued = reader.address_by_index(AddressIndex::from(7u32)).await?;
    assert!(wallet
        .issued_addresses()
        .await?
        .iter()
        .any(|record| record.address == issued));
    host.execute(vec![]).await?;
    common::scan_latest(&chain, &wallet).await?;
    reader
        .chain_id()
        .await
        .expect_err("reader must not mix wallet heights");
    Ok(())
}
