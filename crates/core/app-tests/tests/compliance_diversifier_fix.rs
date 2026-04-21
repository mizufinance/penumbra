use {
    self::common::BuilderExt,
    anyhow::anyhow,
    cnidarium::{StateDelta, TempStorage},
    common::TempStorageExt as _,
    decaf377::Fr,
    penumbra_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
    },
    penumbra_sdk_asset::asset::REGISTRY,
    penumbra_sdk_compliance::{
        scanning::decrypt_full_flagged, structs::AssetPolicy, ComplianceRegistryWrite,
        DetectionKey, TransferComplianceCiphertext,
    },
    penumbra_sdk_keys::{keys::AddressIndex, symmetric::PayloadKey, test_keys},
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_shielded_pool::{
        genesis::Allocation, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan,
    },
    penumbra_sdk_transaction::{
        memo::MemoPlaintext,
        plan::{ActionPlan, MemoPlan},
        TransactionParameters, TransactionPlan,
    },
    penumbra_sdk_view::enrich_plan_with_compliance,
    rand_core::OsRng,
    std::ops::Deref,
    tap::{Tap, TapFallible},
    tracing::info,
};

mod common;

#[tokio::test]
async fn compliance_enrichment_preserves_sender_diversifier_on_supported_transfer(
) -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;
    let regulated_denom = "test_regulated_asset";
    let regulated_asset_id = REGISTRY
        .parse_denom(regulated_denom)
        .expect("custom test denom should parse as a base denom")
        .id();

    let sender = test_keys::FULL_VIEWING_KEY
        .payment_address(AddressIndex::from(1u32))
        .0;
    let recipient = test_keys::ADDRESS_1.deref().clone();

    let _test_node = {
        let mut content =
            genesis::Content::default().with_chain_id(TestNode::<()>::CHAIN_ID.to_string());
        content.shielded_pool_content.allocations = vec![Allocation {
            raw_amount: 1_000u128.into(),
            raw_denom: regulated_denom.to_string(),
            address: sender.clone(),
        }];

        let app_state = AppState::Content(content);
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_penumbra_auto_app_state(app_state)?
            .init_chain(consensus)
            .await
            .tap_ok(|e| info!(hash = %e.last_app_hash_hex(), "finished init chain"))?
    };

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?
        .tap(|c| info!(notes = %c.notes.len(), "initial sync"));

    let note = client
        .notes
        .values()
        .find(|n| n.address() == sender)
        .cloned()
        .ok_or_else(|| anyhow!("no note at sender address"))?;

    let asset_id = note.asset_id();
    assert_eq!(asset_id, regulated_asset_id);
    let dk = DetectionKey::new(decaf377::Fr::from(88888u64));
    let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(424242u64);

    let mut build_state = StateDelta::new(storage.latest_snapshot());
    build_state
        .register_asset_in_imt(
            asset_id,
            AssetPolicy::simple(dk.public_key(), 1u128, ring_pk),
            true,
        )
        .await?;
    common::register_test_users_for_compliance(
        &mut build_state,
        &[sender.clone(), recipient.clone()],
        &[asset_id],
    )
    .await?;

    let spend = ShieldedInputPlan::new(
        &mut OsRng,
        note.clone(),
        client
            .position(note.commit())
            .ok_or_else(|| anyhow!("sender note position unknown"))?,
    );
    let output = ShieldedOutputPlan::new(&mut OsRng, note.value(), recipient.clone());
    let transfer = TransferPlan::new(vec![spend], vec![output], Fr::from(1u64))?;

    let mut plan = TransactionPlan {
        actions: vec![transfer.into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(sender.clone()),
        )),
        detection_data: None,
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    let provider = penumbra_sdk_mock_client::StateReadComplianceProvider::new(build_state);
    enrich_plan_with_compliance(&mut plan, &provider, &mut OsRng, None).await?;

    let witness_data = client.witness_plan(&plan)?;
    let dummy_payload_key: PayloadKey = [0u8; 32].into();
    let Some(ActionPlan::Transfer(transfer_plan)) = plan.actions.first() else {
        panic!("expected a single transfer plan");
    };
    let body = transfer_plan.transfer_body(&client.fvk, &dummy_payload_key, witness_data.anchor)?;
    let receiver_output = body
        .outputs
        .first()
        .expect("transfer should have a receiver output");
    let ciphertext =
        TransferComplianceCiphertext::from_bytes(&receiver_output.compliance_ciphertext)
            .expect("transfer output should carry a valid compliance ciphertext");
    let decrypted = decrypt_full_flagged(dk.inner(), &ciphertext, asset_id)?
        .expect("flagged transfer compliance should decrypt");

    assert_eq!(
        decrypted.sender_address.transmission_key,
        sender.transmission_key().0,
        "compliance ciphertext should preserve the sender transmission key",
    );
    assert_eq!(
        decrypted.sender_address.diversified_generator,
        *sender.diversified_generator(),
        "compliance ciphertext should preserve the sender diversifier",
    );
    assert_eq!(
        decrypted.receiver_address.transmission_key,
        recipient.transmission_key().0,
        "compliance ciphertext should preserve the receiver transmission key",
    );

    drop(storage);
    drop(guard);

    Ok(())
}
