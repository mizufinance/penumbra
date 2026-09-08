mod proof_acceptance_tests;

use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::Arc;

use crate::test_support::{TestHost, TEST_CHAIN_ID};
use anyhow::{anyhow, Context, Result};
use ark_ff::Zero;
use ark_serialize::CanonicalSerialize;
use cnidarium::{ArcStateDeltaExt as _, StateDelta, StateRead, StateWrite, TempStorage};
use decaf377::{Fq, Fr};
use decaf377_rdsa as rdsa;
use futures::StreamExt as _;
use proptest::prelude::*;
use prost::bytes::Bytes;
use rand_core::OsRng;
use sha2::Digest as _;
use shieldd_sdk_asset::{asset, Value, BASE_ASSET_DENOM, BASE_ASSET_ID};
use shieldd_sdk_compact_block::StatePayload;
use shieldd_sdk_compliance::genesis::{GenesisUserRegistration, NativeAssetRegistration};
use shieldd_sdk_compliance::registry::ComplianceRegistryWrite as _;
use shieldd_sdk_compliance::structs::{
    OrbisCapabilityCertificate, UserRegistrationGrant, UserRegistrationGrantBody,
};
use shieldd_sdk_compliance::{
    derive_regulated_nullifier_key, AssetPolicy, ComplianceLeaf, MsgRegisterUser,
};
use shieldd_sdk_fee::Fee;
use shieldd_sdk_keys::{test_keys, Address};
use shieldd_sdk_mock_client::MockClient;
use shieldd_sdk_num::Amount;
#[cfg(feature = "orbis-dev-srs")]
use shieldd_sdk_proof_aggregation::srs_id;
use shieldd_sdk_proof_aggregation::{
    app_verify_family_code, AggregateBundle, AppVerifyCallId, DevSrs, FamilyAggregate,
    ProofFamilyId, AGGREGATE_PROTOCOL_VERSION, DEFAULT_DEV_SRS_ID,
};
use shieldd_sdk_proof_params::batch::BatchItem;
use shieldd_sdk_proto::DomainType;
use shieldd_sdk_sct::component::clock::{EpochManager as _, EpochRead as _};
use shieldd_sdk_sct::component::tree::{SctManager as _, SctRead as _};
use shieldd_sdk_sct::component::StateWriteExt as _;
use shieldd_sdk_sct::epoch::Epoch;
use shieldd_sdk_sct::nullifier_generation::{
    empty_history_head, NullifierWindow, PROTOCOL_VERSION,
};
use shieldd_sdk_sct::params::SctParameters;
use shieldd_sdk_sct::{CommitmentSource, Nullifier};
use shieldd_sdk_shielded_pool::component::NoteManager as _;
use shieldd_sdk_shielded_pool::test_proof_helpers::proof_test_helpers::build_transfer_action_and_public_without_proof;
use shieldd_sdk_shielded_pool::{genesis::Allocation, ShieldedInputPlan, ShieldedOutputPlan};
use shieldd_sdk_tct as tct;
use shieldd_sdk_transaction::{
    memo::{MemoCiphertext, MemoPlaintext, MEMO_CIPHERTEXT_LEN_BYTES},
    plan::MemoPlan,
    Action, ActionPlan, Transaction, TransactionParameters, TransactionPlan,
};
use shieldd_sdk_txhash::AuthorizingData;
use tendermint::Time;

use super::{BatchCandidate, BatchPreparation, BatchVerdict, PrepareBlockLocalState};
use crate::action_handler::transaction::{
    prepare_candidate_read, prepare_candidate_read_blocking, supports_parallel_prepare,
    HistoricalCheckContext,
};

use crate::action_handler::AppActionHandler;
use crate::app::CheckTxSharedContext;
use crate::app::ProposalArtifactSidecar;
use crate::app::{candidate_digest_from_hashes, CandidateEnvelope};
use crate::genesis::{AppState, Content};
use crate::stateless_cache::{CacheEntry, StatelessCache, TxArtifact};
use crate::SUBSTORE_PREFIXES;

use super::{
    AggregateBundleFamilyEstimate, App, BlockSctAppendLog, BlockTxIndexingMode, StateReadExt,
    AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES, AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
};

fn test_nullifier_window() -> NullifierWindow {
    NullifierWindow {
        protocol_version: PROTOCOL_VERSION,
        current_generation: 0,
        recent_position_floor: 0,
        archived_generation_count: 0,
        archived_history_head: empty_history_head(),
    }
}

const SRS_ID_MISMATCH: &str = if cfg!(feature = "orbis-dev-srs") {
    "Orbis integration SnarkPack SRS id mismatch"
} else {
    "test/fuzz SnarkPack SRS id mismatch"
};

#[cfg(feature = "orbis-dev-srs")]
#[test]
fn orbis_dev_srs_selects_only_the_insecure_integration_fixture() -> Result<()> {
    let srs = super::shipping_srs()?;
    assert!(!srs.is_registered());
    assert_eq!(srs_id(&srs), DEFAULT_DEV_SRS_ID);

    let selected = super::shipping_srs_for_id(&DEFAULT_DEV_SRS_ID)?;
    assert!(!selected.is_registered());
    assert_eq!(srs_id(&selected), DEFAULT_DEV_SRS_ID);

    let error = super::shipping_srs_for_id(&[0u8; 32])
        .expect_err("integration fixture must reject every other SRS id");
    assert!(error
        .to_string()
        .contains("Orbis integration SnarkPack SRS id mismatch"));

    Ok(())
}

fn rolled_up_payload(value: u64) -> StatePayload {
    StatePayload::RolledUp {
        source: CommitmentSource::transaction(),
        commitment: tct::StateCommitment(Fq::from(value)),
    }
}

#[tokio::test]
async fn failed_transaction_drops_all_staged_effects() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut base_state = StateDelta::new(storage.latest_snapshot());
    shieldd_sdk_sct::nullifier_tree::initialize(&mut base_state).await?;
    let mut state = Arc::new(base_state);
    let nullifier = Nullifier(Fq::from(71u64));
    let source = CommitmentSource::Transaction {
        id: Some([7u8; 32]),
    };
    let payload = shieldd_sdk_shielded_pool::NotePayload {
        note_commitment: tct::StateCommitment(Fq::from(72u64)),
        ..shieldd_sdk_shielded_pool::NotePayload::dummy()
    };
    let unrelated_effect_key = "fv/transaction/staged-effect".to_string();

    let execution_result: Result<()> = async {
        let mut state_tx = state
            .try_begin_transaction()
            .expect("test state must have unique ownership");
        state_tx.put_block_height(42);
        state_tx
            .nullify_all(std::slice::from_ref(&nullifier), source.clone())
            .await?;
        state_tx.add_note_payload(payload, source).await;
        state_tx.put_raw(unrelated_effect_key.clone(), vec![1u8]);

        assert_eq!(
            state_tx
                .pending_nullifiers()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![nullifier]
        );
        assert_eq!(state_tx.pending_note_payloads().len(), 1);
        assert_eq!(
            state_tx.get_raw(unrelated_effect_key.as_str()).await?,
            Some(vec![1u8])
        );

        Err(anyhow!("later action failed"))
    }
    .await;

    assert!(execution_result.is_err());
    assert!(state.pending_nullifiers().is_empty());
    assert!(state.pending_note_payloads().is_empty());
    assert!(!shieldd_sdk_sct::nullifier_tree::is_spent(Arc::as_ref(&state), nullifier).await?);
    assert_eq!(state.get_raw(unrelated_effect_key.as_str()).await?, None);

    Ok(())
}

#[test]
fn proposal_tx_count_policy_is_fixed_at_boundary() {
    let mut candidates = vec![Bytes::new(); super::MAX_BLOCK_TX_COUNT + 1];
    super::truncate_prepare_candidates(&mut candidates);
    assert_eq!(candidates.len(), super::MAX_BLOCK_TX_COUNT);
    assert!(super::process_proposal_tx_count_allowed(
        super::MAX_BLOCK_TX_COUNT
    ));
    assert!(!super::process_proposal_tx_count_allowed(
        super::MAX_BLOCK_TX_COUNT + 1
    ));
}

#[test]
fn proposal_payload_size_policy_is_fixed_at_boundary() {
    assert_eq!(super::prepare_proposal_payload_limit(-1), 0);
    assert_eq!(super::prepare_proposal_payload_limit(0), 0);
    assert_eq!(
        super::prepare_proposal_payload_limit(super::MAX_BLOCK_TXS_PAYLOAD_BYTES as i64),
        super::MAX_BLOCK_TXS_PAYLOAD_BYTES as u64
    );
    assert_eq!(
        super::prepare_proposal_payload_limit(super::MAX_BLOCK_TXS_PAYLOAD_BYTES as i64 + 1),
        super::MAX_BLOCK_TXS_PAYLOAD_BYTES as u64
    );
    assert!(super::process_proposal_payload_size_allowed(
        super::MAX_BLOCK_TXS_PAYLOAD_BYTES
    ));
    assert!(!super::process_proposal_payload_size_allowed(
        super::MAX_BLOCK_TXS_PAYLOAD_BYTES + 1
    ));
}

#[test]
fn proposal_nullifier_count_policy_is_fixed_at_boundary() {
    assert!(super::block_nullifier_count_allowed(
        super::MAX_BLOCK_NULLIFIER_COUNT
    ));
    assert!(!super::block_nullifier_count_allowed(
        super::MAX_BLOCK_NULLIFIER_COUNT + 1
    ));
    assert!(!super::block_nullifier_count_allowed(usize::MAX));
}

#[test]
fn proposal_transaction_size_policy_is_fixed_at_boundary() {
    assert!(super::transaction_size_allowed(
        super::MAX_TRANSACTION_SIZE_BYTES
    ));
    assert!(!super::transaction_size_allowed(
        super::MAX_TRANSACTION_SIZE_BYTES + 1
    ));
}

#[test]
fn proof_worker_concurrency_is_bounded_for_all_hardware_sizes() {
    assert_eq!(App::proof_family_ids().len(), 4);
    assert_eq!(super::MAX_CONCURRENT_AGGREGATE_SEGMENTS, 2);
    assert_eq!(super::MAX_CONCURRENT_AGGREGATE_VERIFY_CALLS, 4);
    assert!(super::MAX_CONCURRENT_AGGREGATE_SEGMENTS * App::proof_family_ids().len() <= 8);
}

#[tokio::test]
async fn structured_join_drain_waits_for_siblings_after_error() {
    let sibling_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sibling_finished_for_task = sibling_finished.clone();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async { Err::<(), anyhow::Error>(anyhow!("injected early failure")) });
    tasks.spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(25));
        sibling_finished_for_task.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok::<(), anyhow::Error>(())
    });

    let result = super::drain_joinset_results(&mut tasks, "injected task panic").await;
    assert!(result.is_err());
    assert!(
        sibling_finished.load(std::sync::atomic::Ordering::SeqCst),
        "drain must await sibling work before returning the first error"
    );
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn oversized_checktx_bytes_reject_before_decode_or_cache() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut app = App::new(storage.latest_snapshot());
    let cache = StatelessCache::new();
    let maximum_transaction_size = super::MAX_TRANSACTION_SIZE_BYTES;
    let oversized = vec![
        0xff;
        maximum_transaction_size
            .checked_add(1)
            .context("maximum transaction size must fit in usize")?
    ];

    let error = app
        .deliver_tx_bytes(&oversized, Some(&cache))
        .await
        .expect_err("oversized CheckTx bytes must reject before decoding or cache admission");
    assert_eq!(
        error.to_string(),
        format!(
            "transaction size {} exceeds maximum {maximum_transaction_size}",
            oversized.len()
        ),
        "oversized CheckTx rejection must come from the pre-decode size guard"
    );

    Ok(())
}

#[tokio::test]
async fn artifact_extraction_cannot_bypass_action_stateless_checks() -> Result<()> {
    let action_anchor = tct::Tree::default().root();
    let balance_commitment = shieldd_sdk_asset::Balance::default().commit(Fr::from(1u64));
    let inputs = (0..8)
        .map(|index| shieldd_sdk_shielded_pool::NoteReshapeInputBody {
            nullifier: Nullifier(Fq::from(10u64 + index)),
            rk: rdsa::VerificationKey::from(rdsa::SigningKey::<rdsa::SpendAuth>::from(Fr::from(
                20u64 + index,
            ))),
            encrypted_backref: shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                [u8::try_from(index + 1).expect("small index"); 48],
            )
            .expect("fixed-size encrypted backref"),
            history_required: false,
        })
        .collect();
    let note_reshape = shieldd_sdk_shielded_pool::NoteReshape {
        body: shieldd_sdk_shielded_pool::NoteReshapeBody {
            family_id: shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne,
            anchor: action_anchor,
            balance_commitment,
            inputs,
            outputs: vec![shieldd_sdk_shielded_pool::NoteReshapeOutputBody {
                note_payload: shieldd_sdk_shielded_pool::NotePayload {
                    note_commitment: tct::StateCommitment(Fq::from(30u64)),
                    ..shieldd_sdk_shielded_pool::NotePayload::dummy()
                },
                wrapped_memo_key: shieldd_sdk_keys::symmetric::WrappedMemoKey([31u8; 48]),
                ovk_wrapped_key: shieldd_sdk_keys::symmetric::OvkWrappedKey([32u8; 48]),
            }],
            routing_tag: Default::default(),
            routing_parameter_set_id: Fq::from(0u64),
            asset_anchor: tct::StateCommitment(Fq::from(0u64)),
            compliance_anchor: tct::StateCommitment(Fq::from(0u64)),
        },
        auth_sigs: vec![[0u8; 64].into(); 8],
        proof: shieldd_sdk_shielded_pool::NoteReshapeProof::default(),
    };
    let mut invalid_auth = Transaction {
        transaction_body: shieldd_sdk_transaction::TransactionBody {
            actions: vec![Action::NoteReshape(note_reshape)],
            memo: Some(MemoCiphertext([0u8; MEMO_CIPHERTEXT_LEN_BYTES])),
            nullifier_window: Some(test_nullifier_window()),
            ..Default::default()
        },
        anchor: action_anchor,
        ..Default::default()
    };
    let binding_signing_key = rdsa::SigningKey::<rdsa::Binding>::from(Fr::from(1u64));
    invalid_auth.binding_sig =
        binding_signing_key.sign_deterministic(invalid_auth.auth_hash().as_bytes());

    let mut mismatched_anchor = invalid_auth.clone();
    mismatched_anchor.anchor = tct::Root(tct::structure::Hash::new(Fq::from(987_654u64)));
    let error = match App::build_tx_artifacts_extracted_for_stage_public(
        "artifact_stateless_regression_anchor",
        &[Arc::new(mismatched_anchor)],
    )
    .await
    {
        Ok(_) => panic!("artifact extraction must enforce action/context anchor equality"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("body anchor does not match transaction anchor"),
        "unexpected anchor rejection: {error:#}"
    );

    let error = match App::build_tx_artifacts_extracted_for_stage_public(
        "artifact_stateless_regression_auth",
        &[Arc::new(invalid_auth)],
    )
    .await
    {
        Ok(_) => panic!("artifact extraction must verify spend authorization signatures"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("auth signature 0 failed to verify"),
        "unexpected authorization rejection: {error:#}"
    );

    Ok(())
}

#[test]
fn fee_funding_extraction_rejects_identity_randomized_key() {
    let (mut transfer, _, context) = build_transfer_action_and_public_without_proof(true);
    transfer.body.proof_context = shieldd_sdk_shielded_pool::TransferProofContext::FeeFunding;
    transfer.body.volume_accumulator =
        shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding();
    let identity_sk = rdsa::SigningKey::<rdsa::SpendAuth>::from(Fr::from(0u64));
    transfer.body.inputs[0].rk = rdsa::VerificationKey::from(identity_sk.clone());
    let different_message = b"different fee funding authorization hash";
    assert_ne!(&different_message[..], context.effect_hash.as_ref());
    transfer.auth_sigs[0] = identity_sk.sign_deterministic(different_message);
    transfer.body.inputs[0]
        .rk
        .verify(context.effect_hash.as_ref(), &transfer.auth_sigs[0])
        .expect("the pinned RDSA primitive admits identity keys across messages");
    let fee_funding = shieldd_sdk_transaction::FeeFunding { transfer };

    let error = match super::extract_fee_funding_proof_item(&fee_funding, &context) {
        Ok(_) => panic!("fee funding must use the shared identity-RK rejection"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("randomized spend key 0 must not be identity"),
        "unexpected rejection reason: {error:#}"
    );
}

async fn delete_nv_prefix<S>(state: &mut S, prefix: &[u8]) -> Result<()>
where
    S: StateRead + StateWrite + ?Sized,
{
    let mut keys = Vec::new();
    {
        let stream = state.nonverifiable_prefix_raw(prefix);
        futures::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            let (key, _) = item?;
            keys.push(key);
        }
    }
    for key in keys {
        state.nonverifiable_delete(key);
    }
    Ok(())
}

async fn setup_test_txs(tx_count: usize) -> Result<(TempStorage, TestHost, Vec<Vec<u8>>)> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;

    let allocations: Vec<Allocation> = std::iter::repeat(Allocation {
        raw_amount: 1_000_000u128.into(),
        raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
        address: test_keys::ADDRESS_0.to_owned(),
    })
    .take(tx_count)
    .collect();

    let app_state_bytes = serde_json::to_vec(&AppState::Content(Content {
        chain_id: TEST_CHAIN_ID.to_string(),
        shielded_pool_content: shieldd_sdk_shielded_pool::genesis::Content {
            allocations,
            ..Default::default()
        },
        ..Default::default()
    }))?;

    let initial_time = tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?;
    let mut test_node = TestHost::new(
        storage.as_ref().clone(),
        serde_json::from_slice(&app_state_bytes)?,
        initial_time,
    )
    .await?;
    test_node.execute(Vec::new()).await?;

    let client = Arc::new(
        MockClient::new(test_keys::SPEND_KEY.clone())
            .with_sync_to_storage(&storage)
            .await?,
    );

    let notes: Vec<_> = client
        .notes
        .values()
        .filter(|note| {
            note.asset_id() == *BASE_ASSET_ID
                && note.address() == test_keys::ADDRESS_0.deref().clone()
        })
        .cloned()
        .take(tx_count)
        .collect();
    let mut txs = Vec::with_capacity(tx_count);
    for note in notes {
        let spend = ShieldedInputPlan::new(
            &mut OsRng,
            note.clone(),
            client
                .position(note.commit())
                .ok_or_else(|| anyhow!("note position was unknown to mock client"))?,
        );
        let send_amount = Amount::from(1u64);
        let change_amount = note.amount() - send_amount;
        let output = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: send_amount,
                asset_id: note.asset_id(),
            },
            test_keys::ADDRESS_1.deref().clone(),
        );
        let change = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: change_amount,
                asset_id: note.asset_id(),
            },
            note.address(),
        );

        let intent = shieldd_sdk_mock_client::TransactionIntent {
            actions: vec![shieldd_sdk_mock_client::TransferIntent {
                spends: vec![spend.into()],
                outputs: vec![output.into(), change.into()],
                value_blinding: Fr::from(1u64),
            }
            .into()],
            memo: Some(MemoPlan::new(
                &mut OsRng,
                MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
            )),
            fee_funding: None,
            transaction_parameters: TransactionParameters {
                chain_id: TEST_CHAIN_ID.to_string(),
                ..Default::default()
            },
            nullifier_window: Some(test_nullifier_window()),
        };

        let tx = client
            .witness_auth_build(
                &client
                    .complete_intent(intent, storage.latest_snapshot())
                    .await?,
            )
            .await?;
        txs.push(tx.encode_to_vec());
    }

    Ok((storage, test_node, txs))
}

#[tokio::test]
async fn regulated_genesis_note_transfers_through_host_and_compact_block() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let authority_vk = rdsa::VerificationKey::from(test_keys::SPEND_KEY.spend_auth_key());
    let regulated_denom = "wregulated_usd";
    let regulated_asset_id = asset::REGISTRY.parse_unit(regulated_denom).id();
    let native_asset = NativeAssetRegistration {
        asset_id: regulated_asset_id,
        is_regulated: true,
        dk_pub: Some(decaf377::Element::GENERATOR.vartime_compress().0),
        registration_authority_vk: Some(authority_vk),
        seizure_authority_vk: Some(authority_vk),
        ring_pk: Some(decaf377::Element::GENERATOR.vartime_compress().0),
        ring_id: "test-ring".to_owned(),
        policy_id: "test-policy".to_owned(),
        permission: "read".to_owned(),
        resource: "document".to_owned(),
    };
    let policy = native_asset.asset_policy()?;
    let make_leaf = |address: Address| {
        let rnk_dh_pk = address.diversified_generator().clone();
        let rnk = derive_regulated_nullifier_key(
            test_keys::FULL_VIEWING_KEY.incoming(),
            &address,
            regulated_asset_id,
            decaf377::Element::GENERATOR,
            rnk_dh_pk,
        )?;
        ComplianceLeaf::registered_from_rnk(
            address,
            regulated_asset_id,
            decaf377::Element::GENERATOR,
            rnk_dh_pk,
            rnk,
        )
    };
    let genesis_leaf = make_leaf(test_keys::ADDRESS_0.deref().clone())?;
    let runtime_leaf = make_leaf(test_keys::ADDRESS_1.deref().clone())?;
    let app_state_bytes = serde_json::to_vec(&AppState::Content(Content {
        chain_id: TEST_CHAIN_ID.to_string(),
        compliance_content: shieldd_sdk_compliance::genesis::Content {
            native_assets: vec![native_asset],
            user_registrations: vec![GenesisUserRegistration {
                capability_certificate: OrbisCapabilityCertificate::sign_for_test(
                    TEST_CHAIN_ID,
                    &genesis_leaf,
                    &policy,
                    decaf377::Fr::from(1u64),
                )?,
                leaf: genesis_leaf,
            }],
            ..Default::default()
        },
        shielded_pool_content: shieldd_sdk_shielded_pool::genesis::Content {
            allocations: vec![
                Allocation {
                    raw_amount: 1_000_000u128.into(),
                    raw_denom: regulated_denom.to_string(),
                    address: test_keys::ADDRESS_0.deref().clone(),
                },
                Allocation {
                    raw_amount: 1_000_000u128.into(),
                    raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
                    address: test_keys::ADDRESS_0.deref().clone(),
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    }))?;

    let mut test_node = TestHost::new(
        storage.as_ref().clone(),
        serde_json::from_slice(&app_state_bytes)?,
        tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?,
    )
    .await?;
    test_node.execute(Vec::new()).await?;

    let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;
    let grant_body = UserRegistrationGrantBody {
        leaf: runtime_leaf.clone(),
        policy_id: "test-policy".to_owned(),
        valid_until_unix: 4_102_444_800,
        nonce: vec![1u8; 16],
    };
    let registration = MsgRegisterUser {
        leaf: runtime_leaf.clone(),
        capability_certificate: Some(OrbisCapabilityCertificate::sign_for_test(
            TEST_CHAIN_ID,
            &runtime_leaf,
            &policy,
            decaf377::Fr::from(1u64),
        )?),
        grant: Some(UserRegistrationGrant {
            signature: test_keys::SPEND_KEY
                .spend_auth_key()
                .sign(OsRng, &grant_body.signing_bytes()),
            body: grant_body,
        }),
    };
    let registration_plan = TransactionPlan {
        actions: vec![ActionPlan::from(registration)],
        memo: None,
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TEST_CHAIN_ID.to_string(),
            ..Default::default()
        },
        nullifier_window: None,
    };
    let registration_tx = client.witness_auth_build(&registration_plan).await?;
    test_node
        .execute(vec![registration_tx.encode_to_vec()])
        .await?;
    client.sync_to_latest(storage.latest_snapshot()).await?;
    let note = client
        .notes
        .values()
        .find(|note| {
            note.asset_id() == regulated_asset_id
                && note.address() == test_keys::ADDRESS_0.deref().clone()
        })
        .cloned()
        .context("regulated genesis note must be recoverable")?;
    let spent_commitment = note.commit();
    let position = client
        .position(note.commit())
        .context("regulated genesis note position must be known")?;
    let spend = ShieldedInputPlan::new(&mut OsRng, note.clone(), position);
    let fee_note = client
        .notes
        .values()
        .find(|note| {
            note.asset_id() == *BASE_ASSET_ID
                && note.address() == test_keys::ADDRESS_0.deref().clone()
        })
        .cloned()
        .context("base genesis note must be recoverable for fee funding")?;
    let fee_position = client
        .position(fee_note.commit())
        .context("base genesis note position must be known")?;
    let fee_spend = ShieldedInputPlan::new(&mut OsRng, fee_note.clone(), fee_position);
    let fee_change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: fee_note.amount(),
            asset_id: *BASE_ASSET_ID,
        },
        test_keys::ADDRESS_0.deref().clone(),
    );
    let fee_funding_transfer = shieldd_sdk_mock_client::TransferIntent {
        spends: vec![fee_spend],
        outputs: vec![fee_change],
        value_blinding: Fr::from(2u64),
    };
    let send_amount = Amount::from(100u64);
    let output = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: send_amount,
            asset_id: regulated_asset_id,
        },
        test_keys::ADDRESS_1.deref().clone(),
    );
    let change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: note.amount() - send_amount,
            asset_id: regulated_asset_id,
        },
        test_keys::ADDRESS_0.deref().clone(),
    );
    let intent = shieldd_sdk_mock_client::TransactionIntent {
        actions: vec![shieldd_sdk_mock_client::TransferIntent {
            spends: vec![spend],
            outputs: vec![output, change],
            value_blinding: Fr::from(1u64),
        }
        .into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
        )),
        fee_funding: Some(fee_funding_transfer),
        transaction_parameters: TransactionParameters {
            chain_id: TEST_CHAIN_ID.to_string(),
            ..Default::default()
        },
        nullifier_window: Some(test_nullifier_window()),
    };
    let plan = client
        .complete_intent(intent, storage.latest_snapshot())
        .await?;
    let tx_bytes = client.witness_auth_build(&plan).await?.encode_to_vec();

    let cache = StatelessCache::new();
    let mut mempool_app = App::new(storage.latest_snapshot());
    mempool_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
    mempool_app
        .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
        .await?;

    test_node.execute(Vec::new()).await?;
    let mut recheck_app = App::new(storage.latest_snapshot());
    recheck_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
    recheck_app
        .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
        .await
        .context("regulated transfer must remain valid during next-block mempool recheck")?;

    let proposal = BatchPreparation {
        txs: vec![tx_bytes.into()],
        max_tx_bytes: 1024 * 1024,
        height: 4,
    };
    let mut batch_app = App::new(storage.latest_snapshot());
    let (prepared, sidecar) = batch_app.prepare_batch(proposal, Some(&cache), false).await;
    assert_eq!(
        prepared.txs.len(),
        2,
        "proposal must include the regulated transfer and aggregate bundle"
    );
    let mut validator = App::new(storage.latest_snapshot());
    let verdict = validator
        .validate_batch(
            BatchCandidate {
                txs: prepared.txs.clone(),
                height: 4,
            },
            Some(&cache),
            sidecar.as_ref(),
            false,
        )
        .await;
    assert!(matches!(verdict, BatchVerdict::Accept));
    test_node
        .execute(
            prepared
                .txs
                .into_iter()
                .take(1)
                .map(|bytes| bytes.to_vec())
                .collect(),
        )
        .await?;
    client.sync_to_latest(storage.latest_snapshot()).await?;
    assert!(
        client.spent_note(&spent_commitment),
        "committed regulated transfer nullifier must be visible in the compact block"
    );

    Ok(())
}

async fn candidate_envelope_from_fixture_txs(
    storage: &TempStorage,
    txs: &[Vec<u8>],
) -> Result<CandidateEnvelope> {
    let decoded = txs
        .iter()
        .enumerate()
        .map(|(index, tx_bytes)| {
            Transaction::decode(tx_bytes.as_slice())
                .map(Arc::new)
                .with_context(|| format!("decoding fixture tx ordinal {index}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let verified_artifacts = App::build_tx_artifacts_for_stage("app_test", &decoded).await?;
    let artifacts = verified_artifacts
        .iter()
        .map(|artifact| artifact.extracted())
        .collect::<Vec<_>>();
    let segment_tx_counts = vec![decoded.len()];
    let (bundle, _segment_tx_counts) =
        App::build_exact_segmented_aggregate_bundle_for_artifacts_public(
            &artifacts,
            &segment_tx_counts,
        )
        .await?;
    let sidecar =
        ProposalArtifactSidecar::build(&artifacts, decoded.len(), segment_tx_counts.clone())?;
    let bundle_tx =
        App::build_aggregate_bundle_tx_for_snapshot_public(storage.latest_snapshot(), bundle)
            .await?;
    let tx_hashes = txs
        .iter()
        .map(|tx_bytes| sha2::Sha256::digest(tx_bytes).into())
        .collect::<Vec<[u8; 32]>>();

    Ok(CandidateEnvelope {
        txs: txs.to_vec(),
        tx_hashes: tx_hashes.clone(),
        aggregate_bundle_tx_bytes: Some(bundle_tx.encode_to_vec()),
        sidecar: sidecar.to_record(),
        segment_tx_counts,
        block_tx_count: txs.len(),
        total_payload_bytes: txs.iter().map(Vec::len).sum(),
        candidate_digest: candidate_digest_from_hashes(&tx_hashes),
        source_builder_label: "app_test".to_string(),
    })
}

async fn aggregate_fixture(
    tx_count: usize,
) -> Result<(
    TempStorage,
    Vec<Arc<TxArtifact>>,
    AggregateBundle,
    Transaction,
)> {
    let (storage, _node, txs) = setup_test_txs(tx_count).await?;
    let decoded = txs
        .iter()
        .map(|tx_bytes| Transaction::decode(tx_bytes.as_slice()).map(Arc::new))
        .collect::<Result<Vec<_>, _>>()?;
    let verified_artifacts = App::build_tx_artifacts_for_stage("app_test", &decoded).await?;
    let artifacts = verified_artifacts
        .iter()
        .map(|artifact| artifact.extracted())
        .collect::<Vec<_>>();
    let segment_tx_counts = vec![decoded.len()];
    let (bundle, _) = App::build_exact_segmented_aggregate_bundle_for_artifacts_public(
        &artifacts,
        &segment_tx_counts,
    )
    .await?;
    let bundle_tx = App::build_aggregate_bundle_tx_for_snapshot_public(
        storage.latest_snapshot(),
        bundle.clone(),
    )
    .await?;

    Ok((storage, artifacts, bundle, bundle_tx))
}

fn aggregate_verify_test_item(family_id: ProofFamilyId, value: u64) -> BatchItem {
    let arity = super::proof_verification_key_for_family(family_id)
        .vk
        .gamma_abc_g1
        .len()
        - 1;
    BatchItem {
        proof: ark_groth16::Proof {
            a: Default::default(),
            b: Default::default(),
            c: Default::default(),
        },
        public_inputs: vec![Fq::from(value); arity],
    }
}

fn aggregate_verify_test_artifact(entries: Vec<(ProofFamilyId, BatchItem)>) -> Arc<TxArtifact> {
    let bundle = AggregateBundle {
        version: AGGREGATE_PROTOCOL_VERSION,
        srs_id: DEFAULT_DEV_SRS_ID.to_vec(),
        families: Vec::new(),
    };
    let total_proof_count = entries.len();
    let mut proof_items = BTreeMap::new();
    for (family_id, item) in entries {
        proof_items
            .entry(family_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    Arc::new(TxArtifact {
        tx: Arc::new(aggregate_bundle_shape_test_tx(bundle, 5)),
        proof_items,
        spend_nullifiers: Vec::new(),
        anchor_pairs: Vec::new(),
        total_proof_count,
        historical_validation: None,
    })
}

#[test]
fn aggregate_expected_segments_preserve_segment_and_family_order() {
    let transfer = ProofFamilyId::Transfer;
    let note_reshape =
        ProofFamilyId::NoteReshape(shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne);
    let artifacts = vec![
        aggregate_verify_test_artifact(vec![
            (note_reshape, aggregate_verify_test_item(note_reshape, 11)),
            (transfer, aggregate_verify_test_item(transfer, 12)),
        ]),
        aggregate_verify_test_artifact(vec![
            (transfer, aggregate_verify_test_item(transfer, 21)),
            (note_reshape, aggregate_verify_test_item(note_reshape, 22)),
        ]),
    ];

    let segments = App::expected_aggregate_verify_segments(
        &artifacts,
        &[
            shieldd_sdk_proof_aggregation::AppVerifySegmentRange {
                segment_index: 0,
                start: 0,
                end: 1,
            },
            shieldd_sdk_proof_aggregation::AppVerifySegmentRange {
                segment_index: 1,
                start: 1,
                end: 2,
            },
        ],
    );
    let ids = segments
        .iter()
        .enumerate()
        .map(|(order_index, segment)| super::AggregateVerifyCallId {
            order_index,
            segment_index: segment.segment_index,
            family_index: segment.family_index,
            family_id: segment.family_id,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            super::AggregateVerifyCallId {
                order_index: 0,
                segment_index: 0,
                family_index: 0,
                family_id: transfer,
            },
            super::AggregateVerifyCallId {
                order_index: 1,
                segment_index: 0,
                family_index: 1,
                family_id: note_reshape,
            },
            super::AggregateVerifyCallId {
                order_index: 2,
                segment_index: 1,
                family_index: 0,
                family_id: transfer,
            },
            super::AggregateVerifyCallId {
                order_index: 3,
                segment_index: 1,
                family_index: 1,
                family_id: note_reshape,
            },
        ]
    );
    assert_eq!(segments[0].items[0].public_inputs[0], Fq::from(12u64));
    assert_eq!(segments[1].items[0].public_inputs[0], Fq::from(11u64));
    assert_eq!(segments[2].items[0].public_inputs[0], Fq::from(21u64));
    assert_eq!(segments[3].items[0].public_inputs[0], Fq::from(22u64));
}

#[test]
fn aggregate_verify_planner_preserves_segment_order_and_checks_counts() -> Result<()> {
    let family_id = ProofFamilyId::Transfer;
    let expected_segments = vec![
        super::AggregateExpectedVerifySegment {
            segment_index: 0,
            family_index: 0,
            family_id,
            items: vec![aggregate_verify_test_item(family_id, 1)],
            debug_rows: Vec::new(),
        },
        super::AggregateExpectedVerifySegment {
            segment_index: 1,
            family_index: 0,
            family_id,
            items: vec![aggregate_verify_test_item(family_id, 2)],
            debug_rows: Vec::new(),
        },
    ];
    let bundle = AggregateBundle {
        version: AGGREGATE_PROTOCOL_VERSION,
        srs_id: DEFAULT_DEV_SRS_ID.to_vec(),
        families: vec![
            FamilyAggregate {
                family_id,
                real_count: 1,
                padded_count: 1,
                aggregate_proof: vec![1],
            },
            FamilyAggregate {
                family_id,
                real_count: 1,
                padded_count: 1,
                aggregate_proof: vec![2],
            },
        ],
    };

    let plan = App::plan_aggregate_bundle_verification(
        &bundle,
        expected_segments.clone(),
        shieldd_sdk_proof_aggregation::DevSrs::default(),
    )?;
    assert_eq!(
        plan.calls.iter().map(|call| call.id).collect::<Vec<_>>(),
        vec![
            super::AggregateVerifyCallId {
                order_index: 0,
                segment_index: 0,
                family_index: 0,
                family_id,
            },
            super::AggregateVerifyCallId {
                order_index: 1,
                segment_index: 1,
                family_index: 0,
                family_id,
            },
        ]
    );
    assert_eq!(plan.calls[0].padded_public_inputs[0][0], Fq::from(1u64));
    assert_eq!(plan.calls[1].padded_public_inputs[0][0], Fq::from(2u64));
    assert_eq!(
        plan.calls[0].shipping_call,
        shieldd_sdk_proof_aggregation::AppVerifyShippingCall {
            id: AppVerifyCallId {
                order_index: 0,
                segment_index: 0,
                family_index: 0,
                family: app_verify_family_code(family_id),
            },
            bundle_family: app_verify_family_code(family_id),
            expected_real_count: 1,
            bundle_real_count: 1,
            expected_padded_count: 1,
            bundle_padded_count: 1
        }
    );

    let mut missing_family = bundle.clone();
    missing_family.families.pop();
    let family_count_error = App::plan_aggregate_bundle_verification(
        &missing_family,
        expected_segments.clone(),
        shieldd_sdk_proof_aggregation::DevSrs::default(),
    )
    .err()
    .expect("missing family must reject");
    assert!(family_count_error
        .to_string()
        .contains("aggregate bundle family count mismatch"));

    let mut wrong_order = bundle.clone();
    wrong_order.families[0].family_id =
        ProofFamilyId::NoteReshape(shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne);
    let order_error = App::plan_aggregate_bundle_verification(
        &wrong_order,
        expected_segments.clone(),
        shieldd_sdk_proof_aggregation::DevSrs::default(),
    )
    .err()
    .expect("wrong family order must reject");
    assert!(order_error
        .to_string()
        .contains("aggregate family ordering mismatch"));

    let mut wrong_real_count = bundle.clone();
    wrong_real_count.families[0].real_count = 2;
    let real_count_error = App::plan_aggregate_bundle_verification(
        &wrong_real_count,
        expected_segments.clone(),
        shieldd_sdk_proof_aggregation::DevSrs::default(),
    )
    .err()
    .expect("wrong real count must reject");
    assert!(real_count_error
        .to_string()
        .contains("aggregate real_count mismatch"));

    let mut wrong_padded_count = bundle;
    wrong_padded_count.families[1].padded_count = 2;
    let padded_count_error = App::plan_aggregate_bundle_verification(
        &wrong_padded_count,
        expected_segments,
        shieldd_sdk_proof_aggregation::DevSrs::default(),
    )
    .err()
    .expect("wrong padded count must reject");
    assert!(padded_count_error
        .to_string()
        .contains("aggregate padded_count mismatch"));

    Ok(())
}

#[test]
fn aggregate_verify_plan_header_rejects_incomplete_segment_coverage() {
    let bundle = AggregateBundle {
        version: AGGREGATE_PROTOCOL_VERSION,
        srs_id: DEFAULT_DEV_SRS_ID.to_vec(),
        families: Vec::new(),
    };
    let tx = aggregate_bundle_shape_test_tx(bundle.clone(), 5);
    let artifact = Arc::new(TxArtifact {
        tx: Arc::new(tx),
        proof_items: BTreeMap::new(),
        spend_nullifiers: Vec::new(),
        anchor_pairs: Vec::new(),
        total_proof_count: 1,
        historical_validation: None,
    });

    let error = App::validate_aggregate_verify_plan_inputs(
        &[artifact],
        &bundle,
        Some(&[0]),
        &DevSrs::default(),
    )
    .expect_err("incomplete segment coverage must reject");
    assert!(error
        .to_string()
        .contains("aggregate segment coverage mismatch"));
}

#[test]
fn aggregate_verify_reducer_is_order_independent_and_rejects_exact_calls() -> Result<()> {
    let family_id = ProofFamilyId::Transfer;
    let expected = vec![
        super::AggregateVerifyCallId {
            order_index: 0,
            segment_index: 0,
            family_index: 0,
            family_id,
        },
        super::AggregateVerifyCallId {
            order_index: 1,
            segment_index: 1,
            family_index: 0,
            family_id,
        },
    ];

    let reduction = App::reduce_aggregate_verify_outcomes(
        &expected,
        vec![
            super::AggregateVerifyCallResult {
                id: expected[1],
                accepted: true,
            },
            super::AggregateVerifyCallResult {
                id: expected[0],
                accepted: true,
            },
        ],
    )?;
    reduction.acceptance_result()?;

    let rejected = App::reduce_aggregate_verify_outcomes(
        &expected,
        vec![
            super::AggregateVerifyCallResult {
                id: expected[0],
                accepted: true,
            },
            super::AggregateVerifyCallResult {
                id: expected[1],
                accepted: false,
            },
        ],
    )?;
    let rejection = rejected
        .acceptance_result()
        .expect_err("one rejected call must reject the bundle");
    assert!(rejection
        .to_string()
        .contains("segment=1 family_index=0 family=Transfer"));

    let duplicate_error = App::reduce_aggregate_verify_outcomes(
        &expected,
        vec![
            super::AggregateVerifyCallResult {
                id: expected[0],
                accepted: true,
            },
            super::AggregateVerifyCallResult {
                id: expected[0],
                accepted: true,
            },
        ],
    )
    .expect_err("duplicate outcomes must reject");
    assert!(duplicate_error
        .to_string()
        .contains("aggregate verification outcome identity mismatch"));

    let missing_error = App::reduce_aggregate_verify_outcomes(
        &expected,
        vec![super::AggregateVerifyCallResult {
            id: expected[0],
            accepted: true,
        }],
    )
    .expect_err("missing outcomes must reject");
    assert!(missing_error
        .to_string()
        .contains("aggregate verification outcome count mismatch"));

    Ok(())
}

#[test]
fn aggregate_verify_join_rejection_guard_is_fail_closed() -> Result<()> {
    super::require_no_rejected_joined_calls(Vec::new())?;

    let rejected = AppVerifyCallId {
        order_index: 3,
        segment_index: 5,
        family_index: 7,
        family: app_verify_family_code(ProofFamilyId::Transfer),
    };
    let error = super::require_no_rejected_joined_calls(vec![rejected])
        .expect_err("a retained rejected join must fail after reducer acceptance");
    assert!(error
        .to_string()
        .contains("join retained 1 rejected call(s)"));
    Ok(())
}

#[tokio::test]
async fn aggregate_bundle_rejects_wrong_real_count() -> Result<()> {
    let (_storage, artifacts, mut bundle, _bundle_tx) = aggregate_fixture(1).await?;
    App::verify_aggregate_bundle_for_artifacts_raw(&artifacts, &bundle, Some(&[1])).await?;
    bundle.families[0].real_count += 1;
    assert!(
        App::verify_aggregate_bundle_for_artifacts_raw(&artifacts, &bundle, Some(&[1]))
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn async_verifier_outcome_retains_its_exact_shipping_input() -> Result<()> {
    let (_storage, artifacts, bundle, _bundle_tx) = aggregate_fixture(1).await?;
    let ranges = App::validate_aggregate_verify_plan_inputs(
        &artifacts,
        &bundle,
        Some(&[1]),
        &DevSrs::default(),
    )?;
    let expected_segments = App::expected_aggregate_verify_segments(&artifacts, &ranges);
    let mut plan = App::plan_aggregate_bundle_verification(
        &bundle,
        expected_segments,
        shieldd_sdk_proof_aggregation::DevSrs::default(),
    )?;
    let call = plan.calls.remove(0);
    let expected_id = call.id;
    let expected_shipping_call = call.shipping_call;
    let expected_statement = call.statement.clone();
    let expected_wrapped_proof = call.aggregate.aggregate_proof.clone();
    let mut expected_padded_public_inputs =
        Vec::with_capacity(expected_statement.padded_public_inputs().len());
    for row in expected_statement.padded_public_inputs() {
        let mut serialized_row = Vec::with_capacity(row.len());
        for field in row {
            let mut bytes = Vec::new();
            field.serialize_compressed(&mut bytes)?;
            serialized_row.push(bytes);
        }
        expected_padded_public_inputs.push(serialized_row);
    }
    let expected_public_input_arity = u32::try_from(
        expected_padded_public_inputs
            .first()
            .context("aggregate statement must retain one padded public-input row")?
            .len(),
    )?;

    let outcome = App::execute_aggregate_verify_call(call)?;
    let shipping_result = outcome.shipping_verification.shipping_result();

    assert_eq!(outcome.id, expected_id);
    assert_eq!(shipping_result.input.call, expected_shipping_call);
    assert_eq!(
        shipping_result.input.protocol_version,
        AGGREGATE_PROTOCOL_VERSION
    );
    assert_eq!(
        shipping_result.input.family,
        app_verify_family_code(expected_id.family_id)
    );
    assert_eq!(shipping_result.input.srs_id, expected_statement.srs_id());
    assert_eq!(
        shipping_result.input.vk_digest,
        expected_statement.vk_digest()
    );
    assert_eq!(
        shipping_result.input.real_count,
        expected_statement.real_count()
    );
    assert_eq!(
        shipping_result.input.padded_count,
        expected_statement.padded_count()
    );
    assert_eq!(
        shipping_result.input.public_input_arity,
        expected_public_input_arity
    );
    assert_eq!(
        shipping_result.input.padded_public_inputs,
        expected_padded_public_inputs
    );
    assert_eq!(
        shipping_result.input.canonical_statement_bytes,
        expected_statement.canonical_bytes()
    );
    assert_eq!(
        shipping_result.input.statement_digest,
        expected_statement.statement_digest()
    );
    assert_eq!(
        shipping_result.input.wrapped_proof_bytes,
        expected_wrapped_proof
    );
    assert_eq!(
        shipping_result.input.challenge_context,
        expected_statement.challenge_context().as_bytes()
    );
    assert_eq!(
        shipping_result.result.accepted,
        outcome
            .shipping_verification
            .shipping_result()
            .result
            .accepted
    );
    assert_eq!(
        outcome.result()?.accepted,
        outcome
            .shipping_verification
            .shipping_result()
            .result
            .accepted
    );
    Ok(())
}

#[tokio::test]
async fn latest_snapshot_supports_parallel_reads() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let snapshot = storage.latest_snapshot();
    let mut tasks = tokio::task::JoinSet::new();

    for _ in 0..4 {
        let snapshot = snapshot.clone();
        tasks.spawn(async move {
            let _ = snapshot.get_raw("parallel.snapshot.read").await?;
            Ok::<(), anyhow::Error>(())
        });
    }

    while let Some(result) = tasks.join_next().await {
        result??;
    }

    Ok(())
}

#[tokio::test]
async fn prepare_candidate_read_supports_unregulated_fixture_txs() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(2).await?;
    let snapshot = Arc::new(storage.latest_snapshot());
    let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

    for tx_bytes in txs {
        let tx = Arc::new(Transaction::decode(tx_bytes.as_slice())?);
        assert!(
            supports_parallel_prepare(Arc::as_ref(&tx)),
            "fixture tx should stay on the supported transfer fast path"
        );

        let prepared = prepare_candidate_read(
            tx.clone(),
            snapshot.clone(),
            historical_context.clone(),
            false,
        )
        .await?;

        assert_eq!(prepared.spend_nullifiers.len(), 2);
        assert_eq!(
            prepared.sct_payloads.len(),
            3,
            "fixture transfer should create receiver, change, and accumulator payloads",
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn prepare_candidate_read_blocking_matches_async_fast_path() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(2).await?;
    let snapshot = Arc::new(storage.latest_snapshot());
    let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

    for tx_bytes in txs {
        let tx = Arc::new(Transaction::decode(tx_bytes.as_slice())?);
        assert!(supports_parallel_prepare(Arc::as_ref(&tx)));

        let prepared_async = prepare_candidate_read(
            tx.clone(),
            snapshot.clone(),
            historical_context.clone(),
            false,
        )
        .await?;
        let tx_for_blocking = tx;
        let snapshot_for_blocking = Arc::as_ref(&snapshot).clone();
        let context_for_blocking = historical_context.clone();
        let handle = tokio::runtime::Handle::current();
        let prepared_blocking = tokio::task::spawn_blocking(move || {
            prepare_candidate_read_blocking(
                tx_for_blocking,
                snapshot_for_blocking,
                context_for_blocking,
                false,
                handle,
            )
        })
        .await??;

        assert_eq!(
            prepared_async.spend_nullifiers,
            prepared_blocking.spend_nullifiers
        );
        assert_eq!(
            prepared_async.sct_payloads.len(),
            prepared_blocking.sct_payloads.len()
        );
        assert_eq!(
            prepared_async
                .sct_payloads
                .iter()
                .map(|payload| *payload.commitment())
                .collect::<Vec<_>>(),
            prepared_blocking
                .sct_payloads
                .iter()
                .map(|payload| *payload.commitment())
                .collect::<Vec<_>>()
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checktx_fast_path_matches_standard_path() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let tx = Arc::new(Transaction::decode(
        txs.first().expect("fixture transaction").as_slice(),
    )?);
    let artifact = App::build_tx_artifact_for_stage("app_test", tx.clone()).await?;
    assert!(supports_parallel_prepare(Arc::as_ref(&tx)));
    let shared_context = Arc::new(CheckTxSharedContext::load(&storage.latest_snapshot()).await?);

    let mut standard_app = App::new(storage.latest_snapshot());
    tx.check_historical(standard_app.state.clone()).await?;
    let standard_events = standard_app
        .execute_tx_checked_historical(artifact.clone())
        .await?;

    let mut fast_app = App::new(storage.latest_snapshot());
    fast_app.set_checktx_shared_context(shared_context);
    let fast_events = fast_app.execute_checktx_fast(artifact, false).await?;

    let mut standard_rendered = standard_events
        .iter()
        .map(|event| format!("{event:?}"))
        .collect::<Vec<_>>();
    standard_rendered.sort();
    let mut fast_rendered = fast_events
        .iter()
        .map(|event| format!("{event:?}"))
        .collect::<Vec<_>>();
    fast_rendered.sort();
    assert_eq!(standard_rendered, fast_rendered);

    Ok(())
}

#[tokio::test]
async fn process_candidate_envelope_accepts_valid_fixture() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(2).await?;
    let envelope = candidate_envelope_from_fixture_txs(&storage, &txs).await?;
    let mut app = App::new(storage.latest_snapshot());
    let starting_generation =
        shieldd_sdk_sct::nullifier_tree::generation_state(Arc::as_ref(&app.state)).await?;

    let verdict = app.process_candidate_envelope(&envelope, None).await?;
    assert!(matches!(verdict, BatchVerdict::Accept));
    assert_eq!(app.state.pending_nullifiers().len(), 4);
    assert!(!app.state.nullifier_block_is_materialized());
    assert_eq!(
        shieldd_sdk_sct::nullifier_tree::generation_state(Arc::as_ref(&app.state))
            .await?
            .current_root,
        starting_generation.current_root,
        "ProcessProposal should reuse delivery semantics without building a disposable tree",
    );

    Ok(())
}

#[tokio::test]
async fn ensure_aggregate_bundle_tx_shape_rejects_memo_fee_and_extra_action() -> Result<()> {
    let (_storage, _artifacts, bundle, bundle_tx) = aggregate_fixture(1).await?;

    let mut with_memo = bundle_tx.clone();
    with_memo.transaction_body.memo = Some(MemoCiphertext([0; MEMO_CIPHERTEXT_LEN_BYTES]));
    let memo_error =
        App::ensure_aggregate_bundle_tx_shape(&with_memo).expect_err("memo must be rejected");
    assert!(memo_error
        .to_string()
        .contains("aggregate bundle tx must not contain a memo"));

    let mut with_fee = bundle_tx.clone();
    with_fee.transaction_body.transaction_parameters.fee =
        Fee::from_staking_token_amount(1u64.into());
    let fee_error =
        App::ensure_aggregate_bundle_tx_shape(&with_fee).expect_err("nonzero fee must fail");
    assert!(fee_error
        .to_string()
        .contains("aggregate bundle tx must have zero fee"));

    let mut with_extra_action = bundle_tx.clone();
    with_extra_action
        .transaction_body
        .actions
        .push(Action::AggregateBundle(bundle));
    let shape_error = App::ensure_aggregate_bundle_tx_shape(&with_extra_action)
        .expect_err("multiple actions must fail aggregate bundle shape validation");
    assert!(shape_error
        .to_string()
        .contains("aggregate bundle tx must contain exactly one aggregate bundle action"));

    Ok(())
}

fn aggregate_bundle_shape_test_tx(bundle: AggregateBundle, mode: u8) -> Transaction {
    let mut tx = Transaction {
        transaction_body: shieldd_sdk_transaction::TransactionBody {
            actions: vec![Action::AggregateBundle(bundle.clone())],
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                chain_id: "shieldd-test-chain".to_owned(),
                fee: Fee::default(),
            },
            fee_funding: None,
            memo: None,
            nullifier_window: None,
            historical_nullifier_proofs: Vec::new(),
        },
        binding_sig: [0; 64].into(),
        anchor: shieldd_sdk_tct::Root(shieldd_sdk_tct::structure::Hash::zero()),
    };

    match mode % 5 {
        0 => tx.transaction_body.actions.clear(),
        1 => {
            tx.transaction_body.memo = Some(MemoCiphertext([0; MEMO_CIPHERTEXT_LEN_BYTES]));
        }
        2 => {
            tx.transaction_body.transaction_parameters.fee =
                Fee::from_staking_token_amount(1u64.into());
        }
        3 => tx
            .transaction_body
            .actions
            .push(Action::AggregateBundle(bundle)),
        _ => {
            let binding_signing_key = rdsa::SigningKey::from(Fr::zero());
            let auth_hash = tx.transaction_body.auth_hash();
            tx.binding_sig = binding_signing_key.sign_deterministic(auth_hash.as_bytes());
        }
    }

    tx
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn ensure_aggregate_bundle_tx_shape_do_not_panic(
        mode in 0u8..5,
        version in any::<u32>(),
        srs_id in prop::collection::vec(any::<u8>(), 0usize..=64),
        aggregate_proof in prop::collection::vec(any::<u8>(), 0usize..=1024),
        real_count in any::<u32>(),
        padded_count in any::<u32>(),
    ) {
        let bundle = AggregateBundle {
            version,
            srs_id,
            families: vec![shieldd_sdk_proof_aggregation::FamilyAggregate {
                family_id: ProofFamilyId::Transfer,
                real_count,
                padded_count,
                aggregate_proof,
            }],
        };
        let tx = aggregate_bundle_shape_test_tx(bundle, mode);
        let result = App::ensure_aggregate_bundle_tx_shape(&tx);

        match mode % 5 {
            0 | 3 => prop_assert!(
                result
                    .expect_err("aggregate action shape mutation must reject")
                    .to_string()
                    .contains("exactly one aggregate bundle action")
            ),
            1 => prop_assert!(
                result
                    .expect_err("memo mutation must reject")
                    .to_string()
                    .contains("must not contain a memo")
            ),
            2 => prop_assert!(
                result
                    .expect_err("fee mutation must reject")
                    .to_string()
                    .contains("must have zero fee")
            ),
            _ => {
                let _ = result;
            }
        }
    }
}

#[tokio::test]
async fn aggregate_bundle_verification_rejects_bad_version_srs_and_family_count() -> Result<()> {
    let (_storage, artifacts, bundle, _bundle_tx) = aggregate_fixture(1).await?;

    let mut bad_version = bundle.clone();
    bad_version.version += 1;
    let version_error =
        App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &bad_version, None)
            .await
            .expect_err("bad version must fail verification");
    assert!(version_error
        .to_string()
        .contains("unsupported aggregate bundle version"));

    let mut bad_srs = bundle.clone();
    bad_srs.srs_id[0] ^= 0x01;
    let srs_error =
        App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &bad_srs, None)
            .await
            .expect_err("bad SRS id must fail verification");
    assert!(srs_error.to_string().contains(SRS_ID_MISMATCH));

    let mut empty_families = bundle.clone();
    empty_families.families.clear();
    let empty_error =
        App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &empty_families, None)
            .await
            .expect_err("empty family list must fail verification");
    assert!(empty_error
        .to_string()
        .contains("aggregate bundle family count mismatch"));

    let mut extra_family = bundle.clone();
    extra_family.families.push(extra_family.families[0].clone());
    let family_count_error =
        App::verify_aggregate_bundle_for_artifacts_raw_public(&artifacts, &extra_family, None)
            .await
            .expect_err("extra family entries must fail verification");
    assert!(family_count_error
        .to_string()
        .contains("aggregate bundle family count mismatch"));

    Ok(())
}

#[tokio::test]
async fn aggregate_bundle_verification_rejects_bad_srs_id_before_srs_setup() -> Result<()> {
    let mut wrong_full_length_srs_id = DEFAULT_DEV_SRS_ID.to_vec();
    wrong_full_length_srs_id[0] ^= 0x01;

    for (srs_id, expected_error) in [
        (vec![0; 3], SRS_ID_MISMATCH),
        (wrong_full_length_srs_id, SRS_ID_MISMATCH),
    ] {
        let bundle = AggregateBundle {
            version: AGGREGATE_PROTOCOL_VERSION,
            srs_id,
            families: vec![FamilyAggregate {
                family_id: ProofFamilyId::Transfer,
                real_count: 1,
                padded_count: 1,
                aggregate_proof: vec![0xaa, 0xbb],
            }],
        };
        let tx = aggregate_bundle_shape_test_tx(bundle.clone(), 5);
        let artifact = Arc::new(TxArtifact {
            tx: Arc::new(tx),
            proof_items: BTreeMap::new(),
            spend_nullifiers: Vec::new(),
            anchor_pairs: Vec::new(),
            total_proof_count: 1,
            historical_validation: None,
        });

        let started = std::time::Instant::now();
        let result =
            App::verify_aggregate_bundle_for_artifacts_raw(&[artifact], &bundle, None).await;
        let elapsed = started.elapsed();
        let error = result.err().expect("bad SRS id must fail before SRS setup");

        assert!(error.to_string().contains(expected_error));
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "bad SRS id rejection took {elapsed:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn execute_validated_candidate_envelope_profiled_skips_proposal_validation() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let spent_nullifiers = Transaction::decode(txs[0].as_slice())?
        .spent_nullifiers()
        .collect::<Vec<_>>();
    let envelope = candidate_envelope_from_fixture_txs(&storage, &txs).await?;

    let mut preflight_app = App::new(storage.latest_snapshot());
    let verdict = preflight_app
        .process_candidate_envelope(&envelope, None)
        .await?;
    assert!(matches!(verdict, BatchVerdict::Accept));

    let mut execution_only = envelope.clone();
    execution_only.tx_hashes.clear();
    execution_only.candidate_digest = [0; 32];

    let mut app = App::new(storage.latest_snapshot());
    let profile = app
        .execute_validated_candidate_envelope_profiled(&execution_only, storage.as_ref().clone())
        .await?;
    assert_eq!(profile.block_tx_count, 1);
    assert!(profile.deliver_txs_wall_ms > 0.0);
    let committed = storage.latest_snapshot();
    for nullifier in spent_nullifiers {
        assert!(shieldd_sdk_sct::nullifier_tree::is_spent(&committed, nullifier).await?);
    }
    shieldd_sdk_sct::nullifier_tree::verify_committed_roots(&committed).await?;

    Ok(())
}

#[tokio::test]
async fn checktx_shared_context_caches_historical_context_for_snapshot() -> Result<()> {
    let (storage, _node, _txs) = setup_test_txs(1).await?;
    let snapshot = storage.latest_snapshot();
    let shared_context = CheckTxSharedContext::load(&snapshot).await?;
    let direct_context = HistoricalCheckContext::load(&snapshot).await?;

    assert_eq!(
        shared_context.historical_check_context.chain_id,
        direct_context.chain_id
    );
    assert_eq!(
        shared_context.historical_check_context.block_height,
        direct_context.block_height
    );
    assert_eq!(
        shared_context.historical_check_context.block_timestamp,
        direct_context.block_timestamp
    );
    assert_eq!(
        shared_context
            .historical_check_context
            .discovery_grace_period_blocks,
        direct_context.discovery_grace_period_blocks
    );
    assert_eq!(
        shared_context
            .historical_check_context
            .previous_discovery_parameters,
        direct_context.previous_discovery_parameters
    );
    assert_eq!(
        shared_context
            .historical_check_context
            .current_discovery_parameters,
        direct_context.current_discovery_parameters
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checktx_cache_hit_and_miss_match_for_supported_tx() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let tx_bytes = txs.first().expect("fixture transaction").clone();
    let cache = StatelessCache::new();
    let shared_context = Arc::new(CheckTxSharedContext::load(&storage.latest_snapshot()).await?);

    let mut miss_app = App::new(storage.latest_snapshot());
    miss_app.set_checktx_shared_context(shared_context.clone());
    let miss_events = miss_app.deliver_tx_bytes(&tx_bytes, Some(&cache)).await?;
    let hash: [u8; 32] = sha2::Sha256::digest(&tx_bytes).into();
    assert!(matches!(
        cache.get(&hash, &tx_bytes),
        Some(CacheEntry::FullyVerified(_))
    ));

    let mut hit_app = App::new(storage.latest_snapshot());
    hit_app.set_checktx_shared_context(shared_context);
    let hit_events = hit_app.deliver_tx_bytes(&tx_bytes, Some(&cache)).await?;
    assert!(matches!(
        cache.get(&hash, &tx_bytes),
        Some(CacheEntry::FullyVerified(_))
    ));
    assert_eq!(miss_events, hit_events);

    Ok(())
}

#[tokio::test]
async fn prepared_reads_are_blind_to_same_block_nullifier_conflicts() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let tx = Arc::new(Transaction::decode(
        txs.first().expect("fixture transaction").as_slice(),
    )?);
    let artifact = App::build_tx_artifact_for_stage("app_test", tx.clone()).await?;
    let snapshot = Arc::new(storage.latest_snapshot());
    let historical_context = HistoricalCheckContext::load(Arc::as_ref(&snapshot)).await?;

    let prepared_first = prepare_candidate_read(
        tx.clone(),
        snapshot.clone(),
        historical_context.clone(),
        false,
    )
    .await?;
    let prepared_second =
        prepare_candidate_read(tx.clone(), snapshot, historical_context, false).await?;

    anyhow::ensure!(
        !prepared_first.spend_nullifiers.is_empty(),
        "fixture tx should exercise committed nullifier checks"
    );
    anyhow::ensure!(
        !prepared_second.spend_nullifiers.is_empty(),
        "fixture tx should exercise committed nullifier checks"
    );

    let mut app = App::new(storage.latest_snapshot());
    let mut block_state = PrepareBlockLocalState::default();

    let first_nullifier_count = prepared_first
        .spend_nullifiers
        .len()
        .saturating_add(prepared_first.volume_nullifiers.len());
    app.apply_prepared_prepare_candidate(artifact.clone(), prepared_first, &mut block_state)
        .await?;
    assert_eq!(
        block_state.remaining_nullifier_capacity,
        super::MAX_BLOCK_NULLIFIER_COUNT - first_nullifier_count
    );
    let err = app
        .apply_prepared_prepare_candidate(artifact, prepared_second, &mut block_state)
        .await
        .expect_err("serial apply should resolve duplicate nullifiers in the same proposal");

    assert!(
        err.to_string()
            .contains("already spent earlier in this proposal"),
        "unexpected error: {err:#}"
    );

    Ok(())
}

#[tokio::test]
async fn batched_nullify_matches_repeated_nullify_and_preserves_pending_order() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let snapshot = storage.latest_snapshot();

    let nullifiers = vec![
        Nullifier(Fq::from(11u64)),
        Nullifier(Fq::from(12u64)),
        Nullifier(Fq::from(13u64)),
    ];
    let source = CommitmentSource::Transaction {
        id: Some([7u8; 32]),
    };

    let mut repeated = StateDelta::new(snapshot.clone());
    repeated.put_block_height(42);
    shieldd_sdk_sct::nullifier_tree::initialize(&mut repeated).await?;
    for nullifier in &nullifiers {
        repeated.nullify(*nullifier, source.clone()).await?;
    }

    let mut batched = StateDelta::new(snapshot);
    batched.put_block_height(42);
    shieldd_sdk_sct::nullifier_tree::initialize(&mut batched).await?;
    batched.nullify_all(&nullifiers, source).await?;

    assert_eq!(repeated.pending_nullifiers(), batched.pending_nullifiers());

    for nullifier in &nullifiers {
        assert_eq!(
            repeated.is_nullifier_spent(*nullifier).await?,
            batched.is_nullifier_spent(*nullifier).await?,
        );
    }

    repeated.materialize_nullifier_block().await?;
    batched.materialize_nullifier_block().await?;
    assert_eq!(
        shieldd_sdk_sct::nullifier_tree::generation_state(&repeated)
            .await?
            .current_root,
        shieldd_sdk_sct::nullifier_tree::generation_state(&batched)
            .await?
            .current_root,
    );

    Ok(())
}

#[tokio::test]
async fn app_readiness_requires_initialized_state() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    assert!(!App::is_ready(storage.latest_snapshot()).await);
    Ok(())
}

async fn readiness_state(storage: &TempStorage) -> StateDelta<cnidarium::Snapshot> {
    let mut state = StateDelta::new(storage.latest_snapshot());
    <shieldd_sdk_compliance::Compliance as cnidarium_component::Component>::init_chain(
        &mut state,
        Some(&Default::default()),
    )
    .await;
    state
}

#[tokio::test]
async fn app_readiness_fails_on_corrupted_nullifier_tree_nv() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut state = readiness_state(&storage).await;
    shieldd_sdk_sct::nullifier_tree::insert_batch(&mut state, [Nullifier(Fq::from(91u64))]).await?;
    storage.commit(state).await?;
    assert!(App::is_ready(storage.latest_snapshot()).await);

    let mut corrupt = StateDelta::new(storage.latest_snapshot());
    let tree = shieldd_sdk_sct::nullifier_tree::generation_state(&corrupt)
        .await?
        .current_tree;
    let mut stream = corrupt.nonverifiable_prefix_raw(
        &shieldd_sdk_sct::state_key::nullifier_generations::tree_node_prefix(tree),
    );
    let mut keys = Vec::new();
    while let Some(item) = stream.next().await {
        let (key, _) = item?;
        keys.push(key);
    }
    drop(stream);
    for key in keys {
        corrupt.nonverifiable_delete(key);
    }
    storage.commit(corrupt).await?;

    assert!(!App::is_ready(storage.latest_snapshot()).await);

    Ok(())
}

#[tokio::test]
async fn app_readiness_fails_on_corrupted_sct_nv() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut state = readiness_state(&storage).await;
    shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
    state.put_sct_params(SctParameters {
        epoch_duration: 10,
        sct_anchor_retention_blocks: 100,
    });
    state.put_block_height(1);
    state.put_block_timestamp(1, Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?);
    state.put_epoch_by_height(
        1,
        Epoch {
            index: 0,
            start_height: 0,
        },
    );

    let mut tree = tct::Tree::new();
    tree.insert(
        tct::Witness::Forget,
        tct::StateCommitment::try_from([11u8; 32])?,
    )?;
    let block_root = tree.end_block()?;
    state.write_sct(1, tree, block_root, None).await;
    storage.commit(state).await?;
    assert!(App::is_ready(storage.latest_snapshot()).await);

    let mut corrupt = StateDelta::new(storage.latest_snapshot());
    delete_nv_prefix(
        &mut corrupt,
        shieldd_sdk_sct::state_key::tree::incremental_prefix().as_bytes(),
    )
    .await?;
    storage.commit(corrupt).await?;

    assert!(!App::is_ready(storage.latest_snapshot()).await);

    Ok(())
}

#[tokio::test]
async fn app_readiness_fails_on_corrupted_compliance_nv() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut state = readiness_state(&storage).await;
    shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
    state
        .test_only_add_compliance_leaf(ComplianceLeaf::registered_for_test(
            Address::dummy(&mut rand::thread_rng()),
            asset::Id(Fq::from(123u64)),
        ))
        .await?;
    state
        .test_only_register_asset(
            asset::Id(Fq::from(456u64)),
            AssetPolicy::for_test(
                decaf377::Element::GENERATOR,
                u128::MAX,
                decaf377::Element::GENERATOR,
            ),
            true,
        )
        .await?;
    storage.commit(state).await?;
    assert!(App::is_ready(storage.latest_snapshot()).await);

    let mut corrupt = StateDelta::new(storage.latest_snapshot());
    delete_nv_prefix(
        &mut corrupt,
        shieldd_sdk_compliance::state_key::tree_storage::user_node_prefix().as_bytes(),
    )
    .await?;
    storage.commit(corrupt).await?;

    assert!(!App::is_ready(storage.latest_snapshot()).await);

    Ok(())
}

#[tokio::test]
async fn deferred_sct_log_reserves_contiguous_positions() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let snapshot = storage.latest_snapshot();
    let mut log = BlockSctAppendLog::default();

    let first = log
        .reserve_positions(&snapshot, vec![rolled_up_payload(1), rolled_up_payload(2)])
        .await?;
    let second = log
        .reserve_positions(&snapshot, vec![rolled_up_payload(3)])
        .await?;

    assert_eq!(first[0].0, tct::Position::from(0u64));
    assert_eq!(first[1].0, tct::Position::from(1u64));
    assert_eq!(second[0].0, tct::Position::from(2u64));

    Ok(())
}

#[tokio::test]
async fn deferred_sct_log_materializes_into_tree_and_pending_payloads() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut app = App::new(storage.latest_snapshot());
    let mut state_tx = StateDelta::new(app.state.clone());

    app.pending_sct_append_log.append_positioned(vec![
        (tct::Position::from(0u64), rolled_up_payload(10)),
        (tct::Position::from(1u64), rolled_up_payload(11)),
    ]);

    app.materialize_pending_sct_append_log(&mut state_tx)
        .await?;

    let pending = state_tx.pending_rolled_up_payloads();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].0, tct::Position::from(0u64));
    assert_eq!(pending[1].0, tct::Position::from(1u64));
    assert_eq!(
        state_tx.get_sct().await.position(),
        Some(tct::Position::from(2u64))
    );

    Ok(())
}

#[tokio::test]
async fn deferred_sct_log_returns_error_on_position_drift() -> Result<()> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut app = App::new(storage.latest_snapshot());
    let mut state_tx = StateDelta::new(app.state.clone());

    state_tx
        .add_sct_commitment(
            tct::StateCommitment(Fq::from(99u64)),
            CommitmentSource::transaction(),
        )
        .await?;
    app.pending_sct_append_log
        .append_positioned(vec![(tct::Position::from(0u64), rolled_up_payload(100))]);

    let err = app
        .materialize_pending_sct_append_log(&mut state_tx)
        .await
        .expect_err("position drift should return an explicit error");
    assert!(err.to_string().contains("position drifted"));

    Ok(())
}

#[tokio::test]
async fn checktx_no_index_does_not_record_tx_log_entries_on_app_fork() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let tx_bytes = txs
        .into_iter()
        .next()
        .expect("fixture should return one tx");

    let mut app = App::new(storage.latest_snapshot());
    app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
    let cache = StatelessCache::new();
    app.deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
        .await?;

    let height = app.state.get_block_height().await?;
    let tx_log = app.state.transactions_by_height(height).await?;
    assert!(
        tx_log.transactions.is_empty(),
        "checktx app fork should not stage tx-log entries in NoIndex mode"
    );
    assert!(
        app.deferred_block_transactions.is_empty(),
        "NoIndex mode should not accumulate deferred tx-log entries"
    );

    Ok(())
}

#[tokio::test]
async fn deferred_batch_persists_full_tx_log_by_block_end() -> Result<()> {
    let (storage, mut node, txs) = setup_test_txs(2).await?;
    let expected_hashes = txs
        .iter()
        .map(|tx| hex::encode(sha2::Sha256::digest(tx.as_slice())))
        .collect::<Vec<_>>();

    node.execute(txs).await?;

    let snapshot = storage.latest_snapshot();
    let height = snapshot.get_block_height().await?;
    let tx_log = snapshot.transactions_by_height(height).await?;
    assert_eq!(tx_log.transactions.len(), 2);

    let actual_hashes = tx_log
        .transactions
        .into_iter()
        .map(Transaction::try_from)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|tx| hex::encode(sha2::Sha256::digest(tx.encode_to_vec().as_slice())))
        .collect::<Vec<_>>();

    assert_eq!(actual_hashes, expected_hashes);

    Ok(())
}

#[tokio::test]
async fn prepare_proposal_reuses_fully_verified_checktx_cache_entries() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let tx_bytes = txs
        .into_iter()
        .next()
        .expect("fixture should return one tx");
    let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
    let cache = StatelessCache::new();

    let mut mempool_app = App::new(storage.latest_snapshot());
    mempool_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
    mempool_app
        .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
        .await?;

    let extracted = match cache.get(&tx_hash, &tx_bytes) {
        Some(CacheEntry::FullyVerified(artifact)) => artifact.extracted(),
        _ => anyhow::bail!("expected fully verified cache entry after CheckTx"),
    };
    assert!(!extracted.proof_items.is_empty());
    assert!(extracted.has_matching_historical_validation(storage.latest_snapshot().version()));
    assert_eq!(
        extracted
            .historical_validation
            .map(|stamp| stamp.snapshot_version),
        Some(storage.latest_snapshot().version()),
        "CheckTx should stamp the cache entry with the validated snapshot version"
    );

    let mut proposer = App::new(storage.latest_snapshot());
    proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    let proposal = BatchPreparation {
        txs: vec![tx_bytes.clone().into()],
        max_tx_bytes: 1024 * 1024,
        height: 1,
    };

    let (prepared, _) = proposer.prepare_batch(proposal, Some(&cache), false).await;
    assert_eq!(
        prepared.txs.len(),
        2,
        "proposal should include user tx plus aggregate bundle"
    );

    match cache.get(&tx_hash, &tx_bytes) {
        Some(CacheEntry::FullyVerified(_)) => {}
        _ => anyhow::bail!("expected fully verified cache entry after PrepareProposal"),
    }

    Ok(())
}

#[tokio::test]
async fn prepare_proposal_verifies_and_upgrades_extracted_cache_entry() -> Result<()> {
    let (storage, _node, txs) = setup_test_txs(1).await?;
    let tx_bytes = txs
        .into_iter()
        .next()
        .expect("fixture should return one tx");
    let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
    let cache = StatelessCache::new();

    let tx = Arc::new(Transaction::decode_canonical(tx_bytes.as_slice())?);
    let mut extracted = App::build_tx_artifacts_extracted_for_stage_public(
        "test_extracted_seed",
        std::slice::from_ref(&tx),
    )
    .await?;
    let extracted = extracted
        .pop()
        .context("single extracted transaction artifact missing")?;
    cache.insert_extracted(tx_bytes.as_slice(), extracted.clone())?;
    assert!(!extracted.proof_items.is_empty());

    let mut proposer = App::new(storage.latest_snapshot());
    proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    let proposal = BatchPreparation {
        txs: vec![tx_bytes.clone().into()],
        max_tx_bytes: 1024 * 1024,
        height: 1,
    };

    let (prepared, _) = proposer.prepare_batch(proposal, Some(&cache), false).await;
    assert_eq!(
        prepared.txs.len(),
        2,
        "proposal should include the user transaction and aggregate bundle"
    );

    match cache.get(&tx_hash, tx_bytes.as_slice()) {
        Some(CacheEntry::FullyVerified(_)) => {}
        _ => anyhow::bail!("expected fully verified cache entry after PrepareProposal"),
    }

    Ok(())
}

#[tokio::test]
async fn prepare_proposal_does_not_reuse_stale_historical_validation_stamp() -> Result<()> {
    let (storage, mut node, txs) = setup_test_txs(1).await?;
    let tx_bytes = txs
        .into_iter()
        .next()
        .expect("fixture should return one tx");
    let cache = StatelessCache::new();

    let mut mempool_app = App::new(storage.latest_snapshot());
    mempool_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
    mempool_app
        .deliver_tx_bytes(tx_bytes.as_slice(), Some(&cache))
        .await?;

    let hash: [u8; 32] = sha2::Sha256::digest(&tx_bytes).into();
    let cached = match cache.get(&hash, &tx_bytes) {
        Some(CacheEntry::FullyVerified(artifact)) => artifact.extracted(),
        _ => anyhow::bail!("CheckTx must cache the verified transaction"),
    };
    assert!(cached.has_matching_historical_validation(storage.latest_snapshot().version()));
    node.execute(Vec::new()).await?;
    assert!(!cached.has_matching_historical_validation(storage.latest_snapshot().version()));

    let mut proposer = App::new(storage.latest_snapshot());
    proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    let proposal = BatchPreparation {
        txs: vec![tx_bytes.into()],
        max_tx_bytes: 1024 * 1024,
        height: 2,
    };

    let (prepared, _) = proposer.prepare_batch(proposal, Some(&cache), false).await;
    assert_eq!(
        prepared.txs.len(),
        2,
        "proposal should still include the user tx and aggregate bundle after re-validation"
    );

    Ok(())
}

#[test]
fn aggregate_bundle_size_estimate_is_monotonic() {
    let chain_id = "shieldd-test";
    let small = vec![
        AggregateBundleFamilyEstimate {
            family_id: ProofFamilyId::Transfer,
            real_count: 8,
            padded_count: 8,
            aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
        },
        AggregateBundleFamilyEstimate {
            family_id: ProofFamilyId::NoteReshape(
                shieldd_sdk_shielded_pool::NOTE_RESHAPE_FAMILY_SPECS[0].id,
            ),
            real_count: 8,
            padded_count: 8,
            aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
        },
    ];
    let large = vec![
        AggregateBundleFamilyEstimate {
            family_id: ProofFamilyId::Transfer,
            real_count: 256,
            padded_count: 256,
            aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
        },
        AggregateBundleFamilyEstimate {
            family_id: ProofFamilyId::NoteReshape(
                shieldd_sdk_shielded_pool::NOTE_RESHAPE_FAMILY_SPECS[0].id,
            ),
            real_count: 256,
            padded_count: 256,
            aggregate_proof_bytes: AGGREGATE_PROOF_ESTIMATE_BYTES_OTHER,
        },
    ];

    let small_size = App::estimate_aggregate_bundle_tx_size_bytes(chain_id, &small);
    let large_size = App::estimate_aggregate_bundle_tx_size_bytes(chain_id, &large);
    assert!(
        large_size >= small_size,
        "larger family counts should not estimate a smaller bundle"
    );
}

#[test]
fn selected_prefix_respects_reduced_target_size() {
    let prefix_payload_bytes = vec![100_000, 250_000, 400_000, 550_000];
    let bundle_bytes = 96_000usize;
    let prefix_len = App::select_prefix_len_with_bundle_budget(
        &prefix_payload_bytes,
        600_000,
        AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
        bundle_bytes,
    );

    assert_eq!(prefix_len, 3);
    assert!(
        prefix_payload_bytes[prefix_len - 1] + bundle_bytes as u64
            <= 600_000 - AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES
    );
}

#[test]
fn fallback_prefix_drops_tail_after_exact_bundle_miss() {
    let prefix_payload_bytes = vec![300_000, 600_000, 900_000];
    let initial_prefix_len = App::select_prefix_len_with_bundle_budget(
        &prefix_payload_bytes,
        1_000_000,
        AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
        80_000,
    );
    assert_eq!(initial_prefix_len, 3);

    let fallback_prefix_len = App::select_prefix_len_with_bundle_budget(
        &prefix_payload_bytes,
        1_000_000,
        AGGREGATE_BUNDLE_SIZE_SAFETY_MARGIN_BYTES,
        140_000,
    )
    .min(initial_prefix_len.saturating_sub(1));

    assert_eq!(fallback_prefix_len, 2);
}
