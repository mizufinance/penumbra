use {
    self::common::{BuilderExt, TempStorageExt as _},
    cnidarium::TempStorage,
    common::set_tracing_subscriber,
    decaf377::Fr,
    decaf377_rdsa as rdsa,
    penumbra_sdk_app::{
        app::{App, MAX_BLOCK_TXS_PAYLOAD_BYTES},
        genesis::{AppState, Content},
        server::consensus::{Consensus, ConsensusService},
        stateless_cache::{CacheEntry, StatelessCache},
        SUBSTORE_PREFIXES,
    },
    penumbra_sdk_asset::STAKING_TOKEN_DENOM,
    penumbra_sdk_fee::Fee,
    penumbra_sdk_keys::test_keys,
    penumbra_sdk_mock_client::{MockClient, StateReadComplianceProvider},
    penumbra_sdk_mock_consensus::TestNode,
    penumbra_sdk_proof_aggregation::{srs_id, AggregateBundle, DevSrs, ProofFamilyId},
    penumbra_sdk_proto::DomainType,
    penumbra_sdk_sct::component::clock::EpochRead,
    penumbra_sdk_shielded_pool::{
        genesis::Allocation, Output, OutputPlan, OutputProof, Spend, SpendPlan, SpendProof,
    },
    penumbra_sdk_stake::{validator, FundingStreams, GovernanceKey, IdentityKey},
    penumbra_sdk_transaction::AuthorizationData,
    penumbra_sdk_transaction::{
        memo::MemoPlaintext, plan::MemoPlan, Action, ActionPlan, Transaction, TransactionBody,
        TransactionParameters, TransactionPlan, WitnessData,
    },
    penumbra_sdk_txhash::AuthorizingData as _,
    penumbra_sdk_view::enrich_plan_with_compliance,
    prost::bytes::Bytes,
    rand_core::OsRng,
    sha2::Digest as _,
    std::ops::Deref,
    tendermint::{
        account, block,
        v0_37::abci::{request, response},
        Hash, Time,
    },
};

mod common;

#[derive(Clone)]
struct CanonicalProofs {
    spend: SpendProof,
    output: OutputProof,
}

struct PreparedReusedProofTx {
    plan: TransactionPlan,
    auth_data: AuthorizationData,
    witness_data: WitnessData,
}

async fn setup_proof_txs_with_node(
    n: usize,
) -> anyhow::Result<(TempStorage, TestNode<ConsensusService>, Vec<Vec<u8>>, i64)> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
        raw_amount: 1_000_000u128.into(),
        raw_denom: STAKING_TOKEN_DENOM.deref().base_denom().denom,
        address: test_keys::ADDRESS_0.to_owned(),
    })
    .take(n)
    .collect();

    let content = Content {
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
            allocations,
            ..Default::default()
        },
        ..Default::default()
    };
    let app_state_bytes = serde_json::to_vec(&AppState::Content(content))?;
    let consensus = Consensus::new(storage.as_ref().clone());
    let mut node = TestNode::builder()
        .single_validator()
        .app_state(app_state_bytes)
        .init_chain(consensus)
        .await?;
    node.block().execute().await?;

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    let notes: Vec<_> = client.notes.values().cloned().take(n).collect();
    let mut txs = Vec::with_capacity(n);
    for note in &notes {
        let position = client
            .position(note.commit())
            .expect("note position exists");
        let mut plan = TransactionPlan {
            actions: vec![
                SpendPlan::new(&mut OsRng, note.clone(), position).into(),
                OutputPlan::new(
                    &mut OsRng,
                    note.value(),
                    test_keys::ADDRESS_1.deref().clone(),
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
        .with_populated_detection_data(OsRng, Default::default());

        let tx = client
            .witness_auth_build_with_compliance(&mut plan, storage.latest_snapshot())
            .await?;
        txs.push(tx.encode_to_vec());
    }

    let max_tx_bytes = (MAX_BLOCK_TXS_PAYLOAD_BYTES - 1) as i64;
    Ok((storage, node, txs, max_tx_bytes))
}

async fn setup_proof_txs(n: usize) -> anyhow::Result<(TempStorage, Vec<Vec<u8>>, i64)> {
    let (storage, node, txs, max_tx_bytes) = setup_proof_txs_with_node(n).await?;
    drop(node);
    Ok((storage, txs, max_tx_bytes))
}

async fn setup_reused_invalid_proof_txs(
    n: usize,
) -> anyhow::Result<(TempStorage, Vec<Vec<u8>>, i64)> {
    anyhow::ensure!(n >= 2, "need at least two txs to reuse proofs");

    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
        raw_amount: 1_000_000u128.into(),
        raw_denom: STAKING_TOKEN_DENOM.deref().base_denom().denom,
        address: test_keys::ADDRESS_0.to_owned(),
    })
    .take(n)
    .collect();

    let content = Content {
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
            allocations,
            ..Default::default()
        },
        ..Default::default()
    };
    let app_state_bytes = serde_json::to_vec(&AppState::Content(content))?;
    let consensus = Consensus::new(storage.as_ref().clone());
    let mut node = TestNode::builder()
        .single_validator()
        .app_state(app_state_bytes)
        .init_chain(consensus)
        .await?;
    node.block().execute().await?;

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;
    let fvk = test_keys::SPEND_KEY.full_viewing_key();
    let target_timestamp = storage
        .latest_snapshot()
        .get_current_block_timestamp()
        .await?
        .unix_timestamp() as u64;

    let notes: Vec<_> = client.notes.values().cloned().take(n).collect();
    let mut prepared = Vec::with_capacity(n);
    for note in &notes {
        let position = client
            .position(note.commit())
            .expect("note position exists");
        let mut plan = TransactionPlan {
            actions: vec![
                SpendPlan::new(&mut OsRng, note.clone(), position).into(),
                OutputPlan::new(
                    &mut OsRng,
                    note.value(),
                    test_keys::ADDRESS_1.deref().clone(),
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
        .with_populated_detection_data(OsRng, Default::default());

        let provider = StateReadComplianceProvider::new(storage.latest_snapshot());
        enrich_plan_with_compliance(&mut plan, &provider, &mut OsRng, Some(target_timestamp))
            .await?;

        prepared.push(PreparedReusedProofTx {
            auth_data: client.authorize_plan(&plan)?,
            witness_data: client.witness_plan(&plan)?,
            plan,
        });
    }

    let mut prepared_iter = prepared.into_iter();
    let seed = prepared_iter.next().expect("seed tx exists");
    let seed_tx = seed
        .plan
        .clone()
        .build_concurrent(&fvk, &seed.witness_data, &seed.auth_data)
        .await?;
    let canonical_proofs = capture_canonical_proofs(&seed_tx)?;

    let mut txs = vec![seed_tx.encode_to_vec()];
    for prepared_tx in prepared_iter {
        let tx = build_tx_with_reused_canonical_proofs(prepared_tx, &fvk, &canonical_proofs)?;
        txs.push(tx.encode_to_vec());
    }

    let max_tx_bytes = (MAX_BLOCK_TXS_PAYLOAD_BYTES - 1) as i64;
    drop(node);
    Ok((storage, txs, max_tx_bytes))
}

async fn setup_zero_proof_tx() -> anyhow::Result<(TempStorage, Vec<u8>)> {
    let storage = TempStorage::new_with_penumbra_prefixes().await?;
    let app_state =
        AppState::Content(Content::default().with_chain_id(TestNode::<()>::CHAIN_ID.to_string()));
    let consensus = Consensus::new(storage.as_ref().clone());
    let mut node = TestNode::builder()
        .single_validator()
        .with_penumbra_auto_app_state(app_state)?
        .init_chain(consensus)
        .await?;
    node.block().execute().await?;

    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;

    let validator_id_sk = rdsa::SigningKey::new(OsRng);
    let validator_id = IdentityKey(decaf377_rdsa::VerificationKey::from(&validator_id_sk).into());
    let consensus_sk = ed25519_consensus::SigningKey::new(OsRng);
    let consensus_key = consensus_sk.verification_key();

    let new_validator = penumbra_sdk_stake::validator::Validator {
        identity_key: validator_id,
        consensus_key: tendermint::PublicKey::from_raw_ed25519(&consensus_key.to_bytes())
            .expect("consensus key is valid"),
        governance_key: GovernanceKey(validator_id_sk.into()),
        enabled: true,
        sequence_number: 0,
        name: "aggregate-bundle-test-validator".to_string(),
        website: String::default(),
        description: String::default(),
        funding_streams: FundingStreams::default(),
    };

    let bytes = new_validator.encode_to_vec();
    let auth_sig = validator_id_sk.sign(OsRng, &bytes);
    let plan = TransactionPlan {
        actions: vec![ActionPlan::ValidatorDefinition(validator::Definition {
            validator: new_validator,
            auth_sig,
        })
        .into()],
        memo: None,
        detection_data: None,
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
    }
    .with_populated_detection_data(OsRng, Default::default());

    let tx = client.witness_auth_build(&plan).await?;
    drop(node);
    Ok((storage, tx.encode_to_vec()))
}

fn prepare_request(txs: Vec<Vec<u8>>, max_tx_bytes: i64) -> request::PrepareProposal {
    request::PrepareProposal {
        txs: txs.into_iter().map(Bytes::from).collect(),
        max_tx_bytes,
        local_last_commit: None,
        misbehavior: Vec::new(),
        height: block::Height::from(1u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    }
}

fn process_request(txs: Vec<Vec<u8>>) -> request::ProcessProposal {
    request::ProcessProposal {
        txs: txs.into_iter().map(Bytes::from).collect(),
        proposed_last_commit: None,
        misbehavior: Vec::new(),
        hash: Hash::None,
        height: block::Height::from(1u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    }
}

fn build_bundle_tx(anchor: penumbra_sdk_tct::Root, bundle: AggregateBundle) -> Vec<u8> {
    let mut tx = Transaction {
        transaction_body: TransactionBody {
            actions: vec![Action::AggregateBundle(bundle)],
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                fee: Fee::default(),
            },
            detection_data: None,
            memo: None,
        },
        binding_sig: [0; 64].into(),
        anchor,
    };

    let binding_signing_key = rdsa::SigningKey::from(Fr::from(0u64));
    let auth_hash = tx.transaction_body.auth_hash();
    tx.binding_sig = binding_signing_key.sign_deterministic(auth_hash.as_bytes());
    tx.encode_to_vec()
}

fn capture_canonical_proofs(tx: &Transaction) -> anyhow::Result<CanonicalProofs> {
    let spend = tx
        .spends()
        .next()
        .map(|action| action.proof.clone())
        .ok_or_else(|| anyhow::anyhow!("seed tx missing spend proof"))?;
    let output = tx
        .outputs()
        .next()
        .map(|action| action.proof.clone())
        .ok_or_else(|| anyhow::anyhow!("seed tx missing output proof"))?;

    Ok(CanonicalProofs { spend, output })
}

fn build_tx_with_reused_canonical_proofs(
    prepared_tx: PreparedReusedProofTx,
    fvk: &penumbra_sdk_keys::FullViewingKey,
    canonical_proofs: &CanonicalProofs,
) -> anyhow::Result<Transaction> {
    let memo_key = prepared_tx.plan.memo.as_ref().map(|memo| memo.key.clone());
    let actions = prepared_tx
        .plan
        .actions
        .iter()
        .cloned()
        .map(|action_plan| {
            build_action_with_reused_canonical_proof(
                action_plan,
                fvk,
                &prepared_tx.witness_data,
                memo_key.clone(),
                canonical_proofs,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let tx = prepared_tx
        .plan
        .clone()
        .build_unauth_with_actions(actions, &prepared_tx.witness_data)?;
    prepared_tx.plan.apply_auth_data(&prepared_tx.auth_data, tx)
}

fn build_action_with_reused_canonical_proof(
    action_plan: ActionPlan,
    fvk: &penumbra_sdk_keys::FullViewingKey,
    witness_data: &WitnessData,
    memo_key: Option<penumbra_sdk_keys::PayloadKey>,
    canonical_proofs: &CanonicalProofs,
) -> anyhow::Result<Action> {
    Ok(match action_plan {
        ActionPlan::Spend(spend_plan) => Action::Spend(Spend {
            body: spend_plan.spend_body(fvk, None),
            auth_sig: [0; 64].into(),
            proof: canonical_proofs.spend.clone(),
        }),
        ActionPlan::Output(output_plan) => {
            let dummy_payload_key: penumbra_sdk_keys::PayloadKey = [0u8; 32].into();
            Action::Output(Output {
                body: output_plan.output_body(
                    fvk.outgoing(),
                    memo_key.as_ref().unwrap_or(&dummy_payload_key),
                    None,
                ),
                proof: canonical_proofs.output.clone(),
            })
        }
        other => ActionPlan::build_unauth(other, fvk, witness_data, memo_key)?,
    })
}

#[tokio::test]
async fn process_proposal_accepts_valid_proof_bearing_bundle() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, max_tx_bytes) = setup_proof_txs(2).await?;

    let mut proposer = App::new(storage.latest_snapshot());
    let prepared = proposer
        .prepare_proposal_v2(prepare_request(txs.clone(), max_tx_bytes), None)
        .await;
    assert_eq!(prepared.txs.len(), txs.len() + 1);
    let bundle_tx = Transaction::decode(prepared.txs.last().unwrap().as_ref())?;
    assert!(bundle_tx.is_aggregate_bundle_tx());

    let proposal_txs = prepared
        .txs
        .iter()
        .map(|tx| tx.to_vec())
        .collect::<Vec<_>>();
    let mut validator = App::new(storage.latest_snapshot());
    let response = validator
        .process_proposal(process_request(proposal_txs), None)
        .await;
    assert!(matches!(response, response::ProcessProposal::Accept));

    Ok(())
}

#[tokio::test]
async fn process_proposal_rejects_proof_bearing_proposal_without_bundle() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, _) = setup_proof_txs(1).await?;
    let mut validator = App::new(storage.latest_snapshot());
    let response = validator.process_proposal(process_request(txs), None).await;
    assert!(matches!(response, response::ProcessProposal::Reject));
    Ok(())
}

#[tokio::test]
async fn process_proposal_accepts_zero_proof_proposal_without_bundle() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, tx) = setup_zero_proof_tx().await?;
    let prepared_tx = Transaction::decode(tx.as_ref())?;
    assert!(!prepared_tx.is_aggregate_bundle_tx());

    let mut validator = App::new(storage.latest_snapshot());
    let response = validator
        .process_proposal(process_request(vec![tx]), None)
        .await;
    assert!(matches!(response, response::ProcessProposal::Accept));

    Ok(())
}

#[tokio::test]
async fn process_proposal_rejects_zero_proof_proposal_with_bundle() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, tx) = setup_zero_proof_tx().await?;
    let bundle = AggregateBundle {
        version: 1,
        srs_id: srs_id(&DevSrs::default()).to_vec(),
        families: Vec::new(),
    };
    let anchor = Transaction::decode(tx.as_slice())?.anchor;
    let bundle_tx = build_bundle_tx(anchor, bundle);

    let mut validator = App::new(storage.latest_snapshot());
    let response = validator
        .process_proposal(process_request(vec![tx, bundle_tx]), None)
        .await;
    assert!(matches!(response, response::ProcessProposal::Reject));

    Ok(())
}

#[tokio::test]
async fn process_proposal_rejects_duplicate_or_non_final_bundle() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, max_tx_bytes) = setup_proof_txs(1).await?;
    let mut proposer = App::new(storage.latest_snapshot());
    let prepared = proposer
        .prepare_proposal_v2(prepare_request(txs.clone(), max_tx_bytes), None)
        .await;
    assert_eq!(prepared.txs.len(), 2);
    let user_tx = prepared.txs[0].to_vec();
    let bundle_tx = prepared.txs[1].to_vec();

    let mut validator = App::new(storage.latest_snapshot());
    let duplicate = validator
        .process_proposal(
            process_request(vec![user_tx.clone(), bundle_tx.clone(), bundle_tx.clone()]),
            None,
        )
        .await;
    assert!(matches!(duplicate, response::ProcessProposal::Reject));

    let mut validator = App::new(storage.latest_snapshot());
    let non_final = validator
        .process_proposal(process_request(vec![bundle_tx, user_tx]), None)
        .await;
    assert!(matches!(non_final, response::ProcessProposal::Reject));

    Ok(())
}

#[tokio::test]
async fn process_proposal_rejects_mutated_bundle_metadata() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, max_tx_bytes) = setup_proof_txs(1).await?;
    let mut proposer = App::new(storage.latest_snapshot());
    let prepared = proposer
        .prepare_proposal_v2(prepare_request(txs.clone(), max_tx_bytes), None)
        .await;
    assert_eq!(prepared.txs.len(), 2);

    let user_tx = prepared.txs[0].to_vec();
    let bundle_tx = Transaction::decode(prepared.txs[1].as_ref())?;
    let base_bundle = bundle_tx
        .aggregate_bundle_action()
        .cloned()
        .expect("prepared proposal should contain bundle");
    assert_eq!(
        base_bundle.families.len(),
        2,
        "expected spend/output families"
    );

    let mut cases = Vec::new();

    let mut wrong_srs = base_bundle.clone();
    wrong_srs.srs_id[0] ^= 0x01;
    cases.push(("wrong_srs_id", wrong_srs));

    let mut wrong_real_count = base_bundle.clone();
    wrong_real_count.families[0].real_count += 1;
    cases.push(("wrong_real_count", wrong_real_count));

    let mut wrong_padded_count = base_bundle.clone();
    wrong_padded_count.families[0].padded_count += 1;
    cases.push(("wrong_padded_count", wrong_padded_count));

    let mut wrong_family_order = base_bundle.clone();
    wrong_family_order.families.swap(0, 1);
    cases.push(("wrong_family_order", wrong_family_order));

    let mut wrong_family_id = base_bundle.clone();
    wrong_family_id.families[0].family_id = ProofFamilyId::Swap;
    cases.push(("wrong_family_id", wrong_family_id));

    let mut malformed_aggregate = base_bundle.clone();
    let truncated_len = malformed_aggregate.families[0].aggregate_proof.len() / 2;
    malformed_aggregate.families[0]
        .aggregate_proof
        .truncate(truncated_len);
    cases.push(("malformed_aggregate_proof", malformed_aggregate));

    for (name, bundle) in cases {
        let mutated_bundle_tx = build_bundle_tx(bundle_tx.anchor, bundle);
        let mut validator = App::new(storage.latest_snapshot());
        let response = validator
            .process_proposal(
                process_request(vec![user_tx.clone(), mutated_bundle_tx]),
                None,
            )
            .await;
        assert!(
            matches!(response, response::ProcessProposal::Reject),
            "case {name} should reject"
        );
    }

    Ok(())
}

#[tokio::test]
async fn deliver_tx_rejects_user_submitted_aggregate_bundle() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, tx) = setup_zero_proof_tx().await?;
    let bundle = AggregateBundle {
        version: 1,
        srs_id: srs_id(&DevSrs::default()).to_vec(),
        families: Vec::new(),
    };
    let anchor = Transaction::decode(tx.as_slice())?.anchor;
    let bundle_tx = build_bundle_tx(anchor, bundle);
    let mut app = App::new(storage.latest_snapshot());
    let err = app
        .deliver_tx_bytes(&bundle_tx, None)
        .await
        .expect_err("user-submitted aggregate bundle tx should be rejected");
    assert!(
        err.to_string()
            .contains("Aggregate bundle actions are not permitted"),
        "unexpected error: {err}"
    );

    Ok(())
}

#[tokio::test]
async fn deliver_tx_populates_anchor_safe_artifact_cache() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, _) = setup_proof_txs(1).await?;
    let tx = txs.into_iter().next().expect("one tx");
    let hash: [u8; 32] = sha2::Sha256::digest(tx.as_slice()).into();
    let cache = StatelessCache::new();

    let mut app = App::new(storage.latest_snapshot());
    app.deliver_tx_bytes(&tx, Some(&cache)).await?;

    let artifact = match cache.get(&hash) {
        Some(CacheEntry::Extracted(a)) | Some(CacheEntry::FullyVerified(a)) => a,
        Some(CacheEntry::Invalid) => panic!("expected valid cached artifact"),
        None => panic!("missing cached artifact"),
    };

    assert_eq!(artifact.tx.encode_to_vec(), tx);
    assert_eq!(
        artifact.total_proof_count, 2,
        "expected spend + output proofs"
    );
    assert_eq!(artifact.spend_nullifiers.len(), 1);
    assert!(!artifact.anchor_pairs.is_empty());
    assert!(artifact
        .proof_items
        .get(&ProofFamilyId::Spend)
        .is_some_and(|items| items.len() == 1));
    assert!(artifact
        .proof_items
        .get(&ProofFamilyId::Output)
        .is_some_and(|items| items.len() == 1));

    Ok(())
}

#[tokio::test]
async fn proof_bearing_proposal_accepts_with_warm_artifact_cache() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, max_tx_bytes) = setup_proof_txs(2).await?;
    let cache = StatelessCache::new();

    for tx in &txs {
        let mut mempool_app = App::new(storage.latest_snapshot());
        mempool_app.deliver_tx_bytes(tx, Some(&cache)).await?;
    }

    let mut proposer = App::new(storage.latest_snapshot());
    let prepared = proposer
        .prepare_proposal_v2(prepare_request(txs.clone(), max_tx_bytes), Some(&cache))
        .await;
    assert_eq!(prepared.txs.len(), txs.len() + 1);

    let proposal_txs = prepared
        .txs
        .iter()
        .map(|tx| tx.to_vec())
        .collect::<Vec<_>>();
    let mut validator = App::new(storage.latest_snapshot());
    let response = validator
        .process_proposal(process_request(proposal_txs), Some(&cache))
        .await;
    assert!(matches!(response, response::ProcessProposal::Accept));

    Ok(())
}

#[tokio::test]
async fn reused_invalid_proof_proposal_is_feature_gated() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, max_tx_bytes) = setup_reused_invalid_proof_txs(2).await?;

    let seed_tx = Transaction::decode(txs[0].as_slice())?;
    let reused_tx = Transaction::decode(txs[1].as_slice())?;
    assert_eq!(
        seed_tx
            .spends()
            .next()
            .expect("seed spend")
            .proof
            .to_proto()
            .inner,
        reused_tx
            .spends()
            .next()
            .expect("reused spend")
            .proof
            .to_proto()
            .inner,
    );
    assert_eq!(
        seed_tx
            .outputs()
            .next()
            .expect("seed output")
            .proof
            .to_proto()
            .inner,
        reused_tx
            .outputs()
            .next()
            .expect("reused output")
            .proof
            .to_proto()
            .inner,
    );

    let seed_hash: [u8; 32] = sha2::Sha256::digest(txs[0].as_slice()).into();
    let reused_hash: [u8; 32] = sha2::Sha256::digest(txs[1].as_slice()).into();
    assert_ne!(seed_hash, reused_hash, "proof reuse should keep txs unique");

    let mut proposer = App::new(storage.latest_snapshot());
    let prepared = proposer
        .prepare_proposal_v2(prepare_request(txs.clone(), max_tx_bytes), None)
        .await;

    assert!(
        prepared.txs.len() < txs.len() + 1,
        "invalid reused-proof transactions must not produce a full proposal"
    );

    let mut validator = App::new(storage.latest_snapshot());
    let response = validator.process_proposal(process_request(txs), None).await;
    assert!(matches!(response, response::ProcessProposal::Reject));

    Ok(())
}

#[tokio::test]
async fn warm_artifact_cache_does_not_bypass_historical_revalidation() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, mut node, txs, _) = setup_proof_txs_with_node(1).await?;
    let tx = txs.into_iter().next().expect("one tx");
    let cache = StatelessCache::new();

    let mut mempool_app = App::new(storage.latest_snapshot());
    mempool_app.deliver_tx_bytes(&tx, Some(&cache)).await?;

    node.block().with_data(vec![tx.clone()]).execute().await?;

    let mut app = App::new(storage.latest_snapshot());
    let err = app
        .deliver_tx_bytes(&tx, Some(&cache))
        .await
        .expect_err("spent tx should fail historical revalidation even with a warm artifact");
    let message = err.to_string();
    assert!(
        message.contains("check_stateful failed")
            || message.contains("nullifier")
            || message.contains("spent")
            || message.contains("executing transaction"),
        "unexpected historical revalidation error: {message}"
    );

    Ok(())
}

#[tokio::test]
async fn process_proposal_v2_profiled_accepts_with_warm_validator_cache() -> anyhow::Result<()> {
    let _guard = set_tracing_subscriber();
    let (storage, txs, max_tx_bytes) = setup_proof_txs(2).await?;
    let proposer_cache = StatelessCache::new();
    let validator_cache = StatelessCache::new();

    for tx in &txs {
        let hash: [u8; 32] = sha2::Sha256::digest(tx.as_slice()).into();
        let mut mempool_app = App::new(storage.latest_snapshot());
        mempool_app
            .deliver_tx_bytes_v2(tx, Some(&proposer_cache))
            .await?;
        let artifact = match proposer_cache.get(&hash) {
            Some(CacheEntry::Extracted(a)) | Some(CacheEntry::FullyVerified(a)) => a,
            Some(CacheEntry::Invalid) => panic!("expected valid cached artifact"),
            None => panic!("missing cached artifact"),
        };
        validator_cache.insert_fully_verified(hash, artifact.clone());
    }

    let mut proposer = App::new(storage.latest_snapshot());
    let (prepared, _, sidecar) = proposer
        .prepare_proposal_v2_profiled(
            prepare_request(txs.clone(), max_tx_bytes),
            Some(&proposer_cache),
            true,
        )
        .await;

    let proposal_txs = prepared
        .txs
        .iter()
        .map(|tx| tx.to_vec())
        .collect::<Vec<_>>();
    let mut validator = App::new(storage.latest_snapshot());
    let (response, profile) = validator
        .process_proposal_v2_profiled(
            process_request(proposal_txs),
            Some(&validator_cache),
            sidecar.as_ref(),
            true,
        )
        .await;

    assert!(matches!(response, response::ProcessProposal::Accept));
    assert_eq!(profile.artifact_hit_count, 2);
    assert_eq!(profile.artifact_miss_count, 0);
    assert_eq!(profile.warm_reuse_count, 2);
    assert_eq!(profile.cold_reconstruction_ms, 0.0);

    Ok(())
}
