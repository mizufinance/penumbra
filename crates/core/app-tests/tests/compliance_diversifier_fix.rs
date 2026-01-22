//! Test that compliance enrichment correctly handles multiple spends with different diversifiers.
//!
//! This test verifies the fix for the diversifier mismatch bug where all spends were using
//! the first spend's address, causing EPK computation failures when notes have different diversifiers.

use {
    self::common::BuilderExt,
    anyhow::anyhow,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    penumbra_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
    },
    penumbra_sdk_keys::{keys::AddressIndex, test_keys},
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_proto::DomainType,
    penumbra_sdk_sct::component::tree::SctRead as _,
    penumbra_sdk_shielded_pool::{genesis::Allocation, OutputPlan, SpendPlan},
    penumbra_sdk_transaction::{
        memo::MemoPlaintext, plan::MemoPlan, TransactionParameters, TransactionPlan,
    },
    rand_core::OsRng,
    std::ops::Deref,
    tap::{Tap, TapFallible},
    tracing::info,
};

mod common;

/// Test that compliance enrichment works correctly when spending notes from multiple addresses
/// (different diversifiers from the same FVK).
///
/// This exercises the fix where each spend uses its own note's address for compliance lookup
/// rather than using the first spend's address for all spends.
#[tokio::test]
async fn compliance_enrichment_handles_multiple_diversifiers() -> anyhow::Result<()> {
    // Install a test logger, acquire some temporary storage.
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;

    // Create two addresses from same FVK with different diversifiers.
    let address_0 = test_keys::ADDRESS_0.deref().clone();
    let address_1 = test_keys::FULL_VIEWING_KEY
        .payment_address(AddressIndex::from(1u32))
        .0;
    let recipient = test_keys::ADDRESS_1.deref().clone();

    // Create genesis with allocations to BOTH addresses.
    let mut test_node = {
        let mut content =
            genesis::Content::default().with_chain_id(TestNode::<()>::CHAIN_ID.to_string());

        // Set custom allocations to both addresses
        content.shielded_pool_content.allocations = vec![
            Allocation {
                raw_amount: 1000u128.into(),
                raw_denom: "upenumbra".to_string(),
                address: address_0.clone(),
            },
            Allocation {
                raw_amount: 1000u128.into(),
                raw_denom: "upenumbra".to_string(),
                address: address_1.clone(),
            },
        ];

        let app_state = AppState::Content(content);
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_penumbra_auto_app_state(app_state)?
            .init_chain(consensus)
            .await
            .tap_ok(|e| info!(hash = %e.last_app_hash_hex(), "finished init chain"))?
    };

    // Initial sync to discover notes.
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?
        .tap(|c| info!(notes = %c.notes.len(), "initial sync"));

    // Find notes at each address.
    let note_0 = client
        .notes
        .values()
        .find(|n| n.address() == address_0)
        .cloned()
        .ok_or_else(|| anyhow!("no note at address_0"))?;
    let note_1 = client
        .notes
        .values()
        .find(|n| n.address() == address_1)
        .cloned()
        .ok_or_else(|| anyhow!("no note at address_1"))?;

    // CRITICAL: Verify different diversifiers - this is what we're testing.
    assert_ne!(
        note_0.address().diversifier(),
        note_1.address().diversifier(),
        "notes must have different diversifiers to test the fix"
    );

    let asset_id = note_0.asset_id();

    // NOTE: With IMT design, unregulated assets (like staking token) don't need compliance setup.
    // The enrichment automatically returns synthetic proofs for unregulated assets.

    // Create transaction with TWO spends from different addresses.
    let total_value = penumbra_sdk_asset::Value {
        amount: (note_0.amount() + note_1.amount()).into(),
        asset_id,
    };

    let mut plan = TransactionPlan {
        actions: vec![
            // Spend from address_0 (diversifier A)
            SpendPlan::new(
                &mut OsRng,
                note_0.clone(),
                client
                    .position(note_0.commit())
                    .ok_or_else(|| anyhow!("note_0 position unknown"))?,
            )
            .into(),
            // Spend from address_1 (diversifier B) - DIFFERENT diversifier!
            SpendPlan::new(
                &mut OsRng,
                note_1.clone(),
                client
                    .position(note_1.commit())
                    .ok_or_else(|| anyhow!("note_1 position unknown"))?,
            )
            .into(),
            // Output the combined value
            OutputPlan::new(&mut OsRng, total_value, recipient.clone()).into(),
        ],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(address_0.clone()),
        )),
        detection_data: None,
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    // BUILD WITH COMPLIANCE - this is where the fix is exercised!
    // Before fix: would fail because spend_1 uses address_0's diversifier
    // After fix: each spend uses its own address
    let tx = client
        .witness_auth_build_with_compliance(&mut plan, storage.latest_snapshot())
        .await?;

    // Capture pre-tx snapshot for nullifier checks.
    let pre_tx_snapshot = storage.latest_snapshot();

    // Execute transaction.
    test_node
        .block()
        .with_data(vec![tx.encode_to_vec()])
        .execute()
        .await?;
    let post_tx_snapshot = storage.latest_snapshot();

    // Verify BOTH nullifiers spent (proves both spends succeeded).
    for nf in tx.spent_nullifiers() {
        assert!(pre_tx_snapshot.spend_info(nf).await?.is_none());
        assert!(
            post_tx_snapshot.spend_info(nf).await?.is_some(),
            "nullifier {:?} should be spent after tx execution",
            nf
        );
    }

    // Verify output created.
    client.sync_to_latest(post_tx_snapshot).await?;
    let output_nc = tx
        .outputs()
        .next()
        .expect("tx has output")
        .body
        .note_payload
        .note_commitment
        .clone();
    assert!(client.notes.contains_key(&output_nc));

    // Free our temporary storage.
    drop(storage);
    drop(guard);

    Ok(())
}
