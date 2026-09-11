use {
    anyhow::anyhow,
    cnidarium::StateDelta,
    decaf377::Fr,
    rand_core::OsRng,
    shieldd_sdk_app::genesis::{self, AppState},
    shieldd_sdk_app::test_support::{TestHost, TEST_CHAIN_ID},
    shieldd_sdk_asset::asset::REGISTRY,
    shieldd_sdk_compliance::{
        scanning::decrypt_full_flagged, structs::AssetPolicy, ComplianceRegistryWrite,
        DetectionKey, TransferComplianceCiphertext, TransferComplianceMetadata,
    },
    shieldd_sdk_keys::{keys::AddressIndex, symmetric::PayloadKey, test_keys},
    shieldd_sdk_mock_client::MockClient,
    shieldd_sdk_shielded_pool::{genesis::Allocation, ShieldedInputPlan, ShieldedOutputPlan},
    shieldd_sdk_transaction::{
        memo::MemoPlaintext,
        plan::{ActionPlan, MemoPlan},
        TransactionParameters,
    },
    shieldd_sdk_view::complete_plan_with_compliance,
    std::ops::Deref,
    tap::Tap,
    tracing::info,
};

mod common;

#[tokio::test]
async fn compliance_enrichment_preserves_sender_diversifier_on_supported_transfer(
) -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = common::new_storage().await?;
    let regulated_denom = "test_regulated_asset";
    let regulated_asset_id = REGISTRY
        .parse_denom(regulated_denom)
        .expect("custom test denom should parse as a base denom")
        .id();

    let sender = test_keys::FULL_VIEWING_KEY.payment_address(AddressIndex::from(1u32));
    let recipient = test_keys::ADDRESS_0.deref().clone();

    let _test_node = {
        let mut content = genesis::Content::default().with_chain_id(TEST_CHAIN_ID.to_string());
        content.shielded_pool_content.allocations = vec![Allocation {
            raw_amount: 1_000u128.into(),
            raw_denom: regulated_denom.to_string(),
            address: sender.clone(),
        }];

        let app_state = AppState::Content(content);
        TestHost::new(
            storage.as_ref().clone(),
            app_state,
            tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?,
        )
        .await?
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
        .test_only_register_asset(
            asset_id,
            AssetPolicy::for_test(dk.public_key(), 1u128, ring_pk),
            true,
        )
        .await?;
    for address in [sender.clone(), recipient.clone()] {
        let rnk_dh_pk = address.diversified_generator().clone();
        let rnk = shieldd_sdk_compliance::derive_regulated_nullifier_key(
            client.fvk.incoming(),
            &address,
            asset_id,
            ring_pk,
            rnk_dh_pk,
        )?;
        let leaf = shieldd_sdk_compliance::ComplianceLeaf::registered_from_rnk(
            address,
            asset_id,
            ring_pk,
            rnk_dh_pk,
            rnk,
            shieldd_sdk_compliance::AuditKeys::test_keys(),
        )?;
        build_state.test_only_add_compliance_leaf(leaf).await?;
    }

    let spend = ShieldedInputPlan::new(
        &mut OsRng,
        note.clone(),
        client
            .position(note.commit())
            .ok_or_else(|| anyhow!("sender note position unknown"))?,
    );
    let output = ShieldedOutputPlan::new(&mut OsRng, note.value(), recipient.clone());
    let transfer = shieldd_sdk_mock_client::TransferIntent {
        spends: vec![spend],
        outputs: vec![output],
        value_blinding: Fr::from(1u64),
    };

    let intent = shieldd_sdk_mock_client::TransactionIntent {
        nullifier_window: Some(
            shieldd_sdk_sct::nullifier_tree::generation_state(&build_state)
                .await?
                .window(),
        ),
        actions: vec![transfer.into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(sender.clone()),
        )),
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TEST_CHAIN_ID.to_string(),
            ..Default::default()
        },
    };

    let provider = shieldd_sdk_mock_client::StateReadComplianceProvider::new(build_state);
    let plan = complete_plan_with_compliance(
        intent,
        |queries| async move {
            provider.get_batch_proofs(&queries).await.map(|compliance| {
                shieldd_sdk_view::CompletionData {
                    compliance,
                    volumes: vec![],
                }
            })
        },
        &mut OsRng,
        Default::default(),
        None,
        true,
    )
    .await?;

    let witness_data = client.witness_plan(&plan)?;
    let dummy_payload_key: PayloadKey = [0u8; 32].into();
    let Some(ActionPlan::Transfer(transfer_plan)) = plan.actions.first() else {
        panic!("expected a single transfer plan");
    };
    let body = transfer_plan.transfer_body(
        &client.fvk,
        &dummy_payload_key,
        witness_data.anchor,
        plan.recent_position_floor()?,
    )?;
    let receiver_output = body
        .outputs
        .first()
        .expect("transfer should have a receiver output");
    let ciphertext =
        TransferComplianceCiphertext::from_bytes(&receiver_output.compliance_ciphertext)
            .expect("transfer output should carry a valid compliance ciphertext");
    let metadata = TransferComplianceMetadata::from_bytes(&receiver_output.compliance_metadata)?;
    let decrypted = decrypt_full_flagged(dk.inner(), &ciphertext, &metadata, asset_id)?
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
