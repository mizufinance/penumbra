use {
    self::common::BuilderExt,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    penumbra_sdk_app::{
        genesis::{self, AppState, Content},
        server::consensus::Consensus,
    },
    penumbra_sdk_governance::{
        change::ParameterChange, proposal_state::State as ProposalState, Proposal, ProposalSubmit,
        StateReadExt as _,
    },
    penumbra_sdk_keys::test_keys,
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_proto::DomainType,
    penumbra_sdk_shielded_pool::OutputPlan,
    penumbra_sdk_transaction::{
        memo::MemoPlaintext, plan::MemoPlan, ActionPlan, TransactionParameters, TransactionPlan,
    },
    rand_core::OsRng,
    std::ops::Deref,
    tap::Tap,
};

mod common;

fn proposal_transaction_plan(proposal: ProposalSubmit) -> TransactionPlan {
    let proposal_nft_value = proposal.proposal_nft_value();

    TransactionPlan {
        actions: vec![
            ActionPlan::ProposalSubmit(proposal),
            OutputPlan::new(
                &mut OsRng,
                proposal_nft_value,
                test_keys::ADDRESS_0.deref().clone(),
            )
            .into(),
        ],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
        )),
        detection_data: None,
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default())
}

/// Exercises that normal governance proposals still work while community pool spend payloads are disabled.
#[tokio::test]
async fn app_can_disable_community_pool_spends() -> anyhow::Result<()> {
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;

    let app_state = AppState::Content(Content {
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        governance_content: penumbra_sdk_governance::genesis::Content {
            governance_params: penumbra_sdk_governance::params::GovernanceParameters {
                proposal_deposit_amount: 0_u32.into(),
                ..Default::default()
            },
        },
        ..genesis::Content::default()
    });

    let mut test_node = {
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_penumbra_auto_app_state(app_state)?
            .init_chain(consensus)
            .await
    }?;

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    let mut parameter_change_plan = proposal_transaction_plan(ProposalSubmit {
        proposal: Proposal {
            id: 0,
            title: "parameter change stays enabled".to_owned(),
            description: "prove governance proposals can still enter voting".to_owned(),
            payload: penumbra_sdk_governance::ProposalPayload::ParameterChange(ParameterChange {
                changes: vec![],
                preconditions: vec![],
            }),
        },
        deposit_amount: 0_u32.into(),
    });
    let parameter_change_tx = client
        .witness_auth_build_with_compliance(&mut parameter_change_plan, storage.latest_snapshot())
        .await?;

    test_node
        .block()
        .with_data(vec![parameter_change_tx.encode_to_vec()])
        .execute()
        .await?;

    assert_eq!(
        storage.latest_snapshot().proposal_state(0).await?,
        Some(ProposalState::Voting),
        "parameter change proposals should still be accepted"
    );

    let mut disabled_payload_plan = proposal_transaction_plan(ProposalSubmit {
        proposal: Proposal {
            id: 1,
            title: "community pool spend stays disabled".to_owned(),
            description: "prove disabled proposal payloads are rejected".to_owned(),
            payload: penumbra_sdk_governance::ProposalPayload::CommunityPoolSpend {
                transaction_plan: TransactionPlan::default().encode_to_vec(),
            },
        },
        deposit_amount: 0_u32.into(),
    });
    let disabled_payload_tx = client
        .witness_auth_build_with_compliance(&mut disabled_payload_plan, storage.latest_snapshot())
        .await?;

    let err = test_node
        .block()
        .with_data(vec![disabled_payload_tx.encode_to_vec()])
        .execute()
        .await
        .expect_err("community pool spend proposals should be rejected");

    assert!(
        err.to_string().contains(
            "proposal payload disabled in lightweight transfer-only phase: CommunityPoolSpend"
        ),
        "unexpected error: {err:?}"
    );
    assert_eq!(
        storage.latest_snapshot().proposal_state(1).await?,
        None,
        "rejected community pool spend proposals should not enter governance state"
    );

    Ok(())
        .tap(|_| drop(test_node))
        .tap(|_| drop(storage))
        .tap(|_| drop(guard))
}
