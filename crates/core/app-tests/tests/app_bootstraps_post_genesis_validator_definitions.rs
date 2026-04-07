use {
    self::common::BuilderExt,
    cnidarium::TempStorage,
    common::TempStorageExt as _,
    decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey},
    penumbra_sdk_app::{
        genesis::{self, AppState, Content},
        server::consensus::Consensus,
    },
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_proto::DomainType,
    penumbra_sdk_stake::{
        component::validator_handler::ValidatorDataRead as _,
        params::{equal_validator_voting_power, StakeParameters},
        validator::{self, State, Validator},
        FundingStreams, GovernanceKey, IdentityKey,
    },
    rand_core::OsRng,
    tap::Tap,
};

mod common;

#[tokio::test]
async fn app_activates_post_genesis_validator_definitions_with_equal_weight() -> anyhow::Result<()>
{
    let guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;

    let app_state = AppState::Content(Content {
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        stake_content: penumbra_sdk_stake::genesis::Content {
            stake_params: StakeParameters::default(),
            ..Default::default()
        },
        ..genesis::Content::default()
    });

    let mut node = {
        let consensus = Consensus::new(storage.as_ref().clone());
        TestNode::builder()
            .single_validator()
            .with_penumbra_auto_app_state(app_state)?
            .init_chain(consensus)
            .await
    }?;

    let client = MockClient::new(penumbra_sdk_keys::test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    let validator_id_sk = SigningKey::<SpendAuth>::new(OsRng);
    let validator_id = IdentityKey(VerificationKey::from(&validator_id_sk).into());
    let validator_consensus_sk = ed25519_consensus::SigningKey::new(OsRng);
    let validator_consensus = validator_consensus_sk.verification_key();

    let new_validator = Validator {
        identity_key: validator_id,
        consensus_key: tendermint::PublicKey::from_raw_ed25519(&validator_consensus.to_bytes())
            .expect("consensus key is valid"),
        governance_key: GovernanceKey(VerificationKey::from(&validator_id_sk)),
        enabled: true,
        sequence_number: 0,
        name: "bootstrap validator".to_string(),
        website: String::default(),
        description: String::default(),
        funding_streams: FundingStreams::default(),
    };

    let plan = {
        use penumbra_sdk_transaction::{ActionPlan, TransactionParameters, TransactionPlan};

        let bytes = new_validator.encode_to_vec();
        let auth_sig = validator_id_sk.sign(OsRng, &bytes);
        let action = ActionPlan::ValidatorDefinition(validator::Definition {
            validator: new_validator.clone(),
            auth_sig,
        });

        TransactionPlan {
            actions: vec![action.into()],
            memo: None,
            detection_data: None,
            transaction_parameters: TransactionParameters {
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                ..Default::default()
            },
        }
        .with_populated_detection_data(OsRng, Default::default())
    };

    node.block()
        .add_tx(client.witness_auth_build(&plan).await?.encode_to_vec())
        .execute()
        .await?;

    let snapshot = storage.latest_snapshot();

    assert_eq!(
        snapshot
            .get_validator_state(&new_validator.identity_key)
            .await?,
        Some(State::Inactive),
        "bootstrapped validators should enter the inactive set immediately"
    );
    assert_eq!(
        snapshot
            .get_validator_power(&new_validator.identity_key)
            .await?,
        Some(equal_validator_voting_power()),
        "post-genesis validators should receive equal voting power"
    );
    assert_eq!(
        snapshot
            .get_validator_pool_size(&new_validator.identity_key)
            .await,
        Some(0u64.into()),
        "post-genesis validators should not receive synthetic delegation pool size"
    );

    use penumbra_sdk_stake::component::ConsensusIndexRead;
    let consensus_set = snapshot.get_consensus_set().await?;
    assert!(
        consensus_set.contains(&new_validator.identity_key),
        "post-genesis validators should be indexed for consensus-set selection"
    );
    for validator_id in consensus_set {
        assert_eq!(
            snapshot.get_validator_power(&validator_id).await?,
            Some(equal_validator_voting_power()),
            "all consensus-set validators should carry equal voting power",
        );
    }

    Ok(())
        .tap(|_| drop(node))
        .tap(|_| drop(storage))
        .tap(|_| drop(guard))
}
