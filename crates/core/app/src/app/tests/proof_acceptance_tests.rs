use super::*;

use std::str::FromStr as _;
use std::time::Duration;

use ark_groth16::Proof;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use cnidarium::ArcStateDeltaExt as _;
use ibc_types::{
    core::{
        channel::{
            channel::{Order, State as ChannelState},
            ChannelEnd, ChannelId, Counterparty as ChannelCounterparty, Packet, PortId, Version,
        },
        client::{ClientId, Height as IbcHeight},
        commitment::MerkleRoot,
        connection::{
            ChainId, ConnectionEnd, ConnectionId, Counterparty as ConnectionCounterparty,
            State as ConnectionState,
        },
    },
    lightclients::tendermint::{
        client_state::{AllowUpdate, ClientState},
        consensus_state::ConsensusState,
        TrustThreshold,
    },
    timestamp::Timestamp,
};
use shieldd_sdk_compact_block::component::StateReadExt as _;
use shieldd_sdk_ibc::{
    component::{
        ChannelStateReadExt as _, ChannelStateWriteExt as _, ClientStateWriteExt as _,
        ConnectionStateWriteExt as _, ConsensusStateWriteExt as _,
    },
    IBC_PROOF_SPECS,
};
use shieldd_sdk_proto::{StateReadProto as _, StateWriteProto as _};
use shieldd_sdk_sct::component::tree::SctRead as _;
use shieldd_sdk_shielded_pool::{Ics20Withdrawal, Note, NoteReshapeFamilyId};
use tendermint::v0_37::abci::response;
use tokio::sync::OnceCell;

use crate::app::{HostBlock, HostExecution, MAX_BLOCK_TXS_PAYLOAD_BYTES};
use crate::stateless_cache::ProofSlot;
use crate::ShielddHost;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeployedProofFamily {
    Transfer,
    NoteReshape(NoteReshapeFamilyId),
    ShieldedIcs20Withdrawal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixtureProofSlot {
    BodyAction,
    FeeFunding,
}

impl DeployedProofFamily {
    fn label(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::NoteReshape(family) => family.label(),
            Self::ShieldedIcs20Withdrawal => "shielded_ics20_withdrawal",
        }
    }

    fn fixture_binding_blinding(self) -> Fr {
        match self {
            Self::Transfer => Fr::from(1u64),
            Self::NoteReshape(family) => Fr::from(family.get()),
            Self::ShieldedIcs20Withdrawal => Fr::from(29u64),
        }
    }
}

#[derive(Clone)]
struct FamilyFixture {
    family: DeployedProofFamily,
    proof_slot: FixtureProofSlot,
    binding_blinding: Fr,
    tx_bytes: Vec<u8>,
}

impl FamilyFixture {
    fn body_action(family: DeployedProofFamily, tx_bytes: Vec<u8>) -> Self {
        Self {
            family,
            proof_slot: FixtureProofSlot::BodyAction,
            binding_blinding: family.fixture_binding_blinding(),
            tx_bytes,
        }
    }

    fn fee_funding(tx_bytes: Vec<u8>) -> Self {
        Self {
            family: DeployedProofFamily::Transfer,
            proof_slot: FixtureProofSlot::FeeFunding,
            binding_blinding: Fr::from(3u64),
            tx_bytes,
        }
    }

    fn label(&self) -> &'static str {
        match self.proof_slot {
            FixtureProofSlot::BodyAction => self.family.label(),
            FixtureProofSlot::FeeFunding => "transfer_fee_funding",
        }
    }
}

struct FamilyFixtureSet {
    _storage_guard: TempStorage,
    fixtures: Vec<FamilyFixture>,
    fee_funding_fixture: FamilyFixture,
    withdrawal_rollback_tx_bytes: Vec<u8>,
}

static FAMILY_FIXTURES: OnceCell<FamilyFixtureSet> = OnceCell::const_new();

async fn family_fixtures() -> Result<&'static FamilyFixtureSet> {
    FAMILY_FIXTURES
        .get_or_try_init(build_family_fixture_set)
        .await
}

async fn build_family_fixture_set() -> Result<FamilyFixtureSet> {
    let storage = build_fixture_storage().await?;
    let client = Arc::new(
        MockClient::new(test_keys::SPEND_KEY.clone())
            .with_sync_to_storage(&storage)
            .await?,
    );
    let notes = client
        .notes
        .values()
        .filter(|note| {
            note.asset_id() == *BASE_ASSET_ID
                && note.address() == test_keys::ADDRESS_0.deref().clone()
        })
        .cloned()
        .take(FIXTURE_REQUIRED_NOTES)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        notes.len() == FIXTURE_REQUIRED_NOTES,
        "expected {FIXTURE_REQUIRED_NOTES} genesis notes, got {}",
        notes.len()
    );

    let mut note_cursor = 0usize;
    let transfer_note = notes[note_cursor].clone();
    note_cursor += 1;
    let transfer_action = transfer_plan(&client, transfer_note, Fr::from(1u64))?;
    let fee_funding_body_action =
        shieldd_sdk_mock_client::ActionIntent::from(transfer_action.clone());

    let fee_funding_note = notes[note_cursor].clone();
    note_cursor += 1;
    let fee_funding_transfer = transfer_plan(&client, fee_funding_note, Fr::from(2u64))?;

    let mut family_actions = vec![(
        DeployedProofFamily::Transfer,
        shieldd_sdk_mock_client::ActionIntent::from(transfer_action),
    )];

    for family in NoteReshapeFamilyId::ALL {
        let real_input_count = family.min_real_inputs();
        let real_output_count = family.min_real_outputs();
        let family_notes = &notes[note_cursor..note_cursor + real_input_count];
        note_cursor += real_input_count;
        let spends = family_notes
            .iter()
            .cloned()
            .map(|note| spend_plan(&client, note))
            .collect::<Result<Vec<_>>>()?;
        let total_amount = FIXTURE_ALLOCATION_AMOUNT
            .checked_mul(u64::try_from(real_input_count)?)
            .context("summing note reshape input amount")?;
        anyhow::ensure!(
            total_amount % u64::try_from(real_output_count)? == 0,
            "test amount must divide evenly across real outputs"
        );
        let output_amount = total_amount / u64::try_from(real_output_count)?;
        let outputs = (0..real_output_count)
            .map(|_| {
                ShieldedOutputPlan::new(
                    &mut OsRng,
                    Value {
                        amount: Amount::from(output_amount),
                        asset_id: *BASE_ASSET_ID,
                    },
                    test_keys::ADDRESS_0.deref().clone(),
                )
            })
            .collect();
        let action = shieldd_sdk_mock_client::NoteReshapeIntent {
            family_id: family,
            spends: spends,
            outputs: outputs,
            value_blinding: Fr::from(family.get()),
        };
        family_actions.push((
            DeployedProofFamily::NoteReshape(family),
            shieldd_sdk_mock_client::ActionIntent::from(action),
        ));
    }

    let withdrawal_note = notes[note_cursor].clone();
    note_cursor += 1;
    let withdrawal_action = withdrawal_plan(&client, withdrawal_note, Fr::from(29u64))?;
    family_actions.push((
        DeployedProofFamily::ShieldedIcs20Withdrawal,
        shieldd_sdk_mock_client::ActionIntent::from(withdrawal_action),
    ));

    let mut withdrawal_rollback_actions = Vec::with_capacity(2);
    for balance_blinding in [30u64, 31u64] {
        let note = notes[note_cursor].clone();
        note_cursor += 1;
        withdrawal_rollback_actions.push(shieldd_sdk_mock_client::ActionIntent::from(
            withdrawal_plan(&client, note, Fr::from(balance_blinding))?,
        ));
    }
    anyhow::ensure!(
        note_cursor == FIXTURE_REQUIRED_NOTES,
        "fixture note accounting drifted: used {note_cursor}, expected {FIXTURE_REQUIRED_NOTES}"
    );

    let mut fixtures = Vec::with_capacity(family_actions.len());
    for (family, action) in family_actions {
        let plan = shieldd_sdk_mock_client::TransactionIntent {
            actions: vec![action],
            memo: None,
            fee_funding: None,
            transaction_parameters: TransactionParameters {
                chain_id: TestNode::<()>::CHAIN_ID.to_string(),
                ..Default::default()
            },
            nullifier_window: Some(test_nullifier_window()),
        };
        let tx = client
            .witness_auth_build(
                &client
                    .complete_intent(plan, storage.latest_snapshot())
                    .await?,
            )
            .await
            .with_context(|| format!("building {} transaction fixture", family.label()))?;
        fixtures.push(FamilyFixture::body_action(family, tx.encode_to_vec()));
    }

    let fee_funding_plan = shieldd_sdk_mock_client::TransactionIntent {
        actions: vec![fee_funding_body_action],
        memo: None,
        fee_funding: Some(fee_funding_transfer),
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
        nullifier_window: Some(test_nullifier_window()),
    };
    let fee_funding_tx = client
        .witness_auth_build(
            &client
                .complete_intent(fee_funding_plan, storage.latest_snapshot())
                .await?,
        )
        .await
        .context("building fee-funding Transfer transaction fixture")?;
    let fee_funding_fixture = FamilyFixture::fee_funding(fee_funding_tx.encode_to_vec());

    let withdrawal_rollback_plan = shieldd_sdk_mock_client::TransactionIntent {
        actions: withdrawal_rollback_actions,
        memo: None,
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TestNode::<()>::CHAIN_ID.to_string(),
            ..Default::default()
        },
        nullifier_window: Some(test_nullifier_window()),
    };
    let withdrawal_rollback_tx = client
        .witness_auth_build(
            &client
                .complete_intent(withdrawal_rollback_plan, storage.latest_snapshot())
                .await?,
        )
        .await
        .context("building two-withdrawal rollback fixture")?;

    anyhow::ensure!(
        fixtures.len() == 4,
        "acceptance-boundary fixture set must cover all four deployed proof families"
    );

    Ok(FamilyFixtureSet {
        _storage_guard: storage,
        fixtures,
        fee_funding_fixture,
        withdrawal_rollback_tx_bytes: withdrawal_rollback_tx.encode_to_vec(),
    })
}

const FIXTURE_ALLOCATION_AMOUNT: u64 = 1_000_000;
const FIXTURE_REQUIRED_NOTES: usize = 8;

async fn build_fixture_storage() -> Result<TempStorage> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let allocations = std::iter::repeat(Allocation {
        raw_amount: u128::from(FIXTURE_ALLOCATION_AMOUNT).into(),
        raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
        address: test_keys::ADDRESS_0.to_owned(),
    })
    .take(FIXTURE_REQUIRED_NOTES)
    .collect();
    let app_state_bytes = serde_json::to_vec(&AppState::Content(Content {
        chain_id: TestNode::<()>::CHAIN_ID.to_string(),
        shielded_pool_content: shieldd_sdk_shielded_pool::genesis::Content {
            allocations,
            ..Default::default()
        },
        ..Default::default()
    }))?;

    let consensus = Consensus::new(storage.as_ref().clone());
    let initial_time = Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?;
    let mut node = TestNode::builder()
        .single_validator()
        .app_state(app_state_bytes)
        .with_initial_timestamp(initial_time)
        .init_chain(consensus)
        .await?;
    node.block().execute().await?;
    Ok(storage)
}

fn spend_plan(client: &MockClient, note: Note) -> Result<ShieldedInputPlan> {
    let position = client
        .position(note.commit())
        .context("fixture note position is known")?;
    Ok(ShieldedInputPlan::new(&mut OsRng, note, position))
}

fn transfer_plan(
    client: &MockClient,
    note: Note,
    value_blinding: Fr,
) -> Result<shieldd_sdk_mock_client::TransferIntent> {
    let spend = spend_plan(client, note.clone())?;
    let receiver = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: Amount::from(1u64),
            asset_id: note.asset_id(),
        },
        test_keys::ADDRESS_1.deref().clone(),
    );
    let change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: note.amount() - Amount::from(1u64),
            asset_id: note.asset_id(),
        },
        note.address(),
    );

    Ok(shieldd_sdk_mock_client::TransferIntent {
        spends: vec![spend],
        outputs: vec![receiver, change],
        value_blinding: value_blinding,
    })
}

fn withdrawal_plan(
    client: &MockClient,
    note: Note,
    balance_blinding: Fr,
) -> Result<shieldd_sdk_mock_client::WithdrawalIntent<Ics20Withdrawal>> {
    let withdrawal_amount = Amount::from(1u64);
    let withdrawal = Ics20Withdrawal {
        amount: withdrawal_amount,
        denom: BASE_ASSET_DENOM.clone(),
        destination_chain_address: "cosmos1destination".to_owned(),
        return_address: test_keys::ADDRESS_0.deref().clone(),
        timeout_height: IbcHeight::new(1, 10).context("valid withdrawal timeout height")?,
        // Absolute Unix timestamp, minute-rounded and after the fixture chain's
        // 2026-01-01 initial time, so route-valid tests reach packet execution.
        timeout_time: 1_800_000_000_000_000_000,
        source_channel: ChannelId::from_str("channel-0")?,
        ics20_memo: String::new(),
        use_transparent_address: false,
    };
    let spend = spend_plan(client, note.clone())?;
    let change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: note.amount() - withdrawal_amount,
            asset_id: note.asset_id(),
        },
        note.address(),
    );
    Ok(shieldd_sdk_mock_client::WithdrawalIntent {
        spends: vec![spend],
        change_output: Some(change),
        withdrawal: withdrawal,
        value_blinding: balance_blinding,
    })
}

fn mutate_to_decodable_invalid_proof(fixture: &FamilyFixture) -> Result<(Transaction, Vec<u8>)> {
    let mut tx = Transaction::decode(fixture.tx_bytes.as_slice())
        .context("decoding valid family fixture")?;
    let proof_bytes = match fixture.proof_slot {
        FixtureProofSlot::BodyAction => {
            match (&mut tx.transaction_body.actions[..], fixture.family) {
                ([Action::Transfer(action)], DeployedProofFamily::Transfer) => {
                    &mut action.proof.inner
                }
                ([Action::NoteReshape(action)], DeployedProofFamily::NoteReshape(expected)) => {
                    anyhow::ensure!(
                        action.body.family_id == expected,
                        "note reshape fixture family mismatch"
                    );
                    &mut action.proof.inner
                }
                (
                    [Action::ShieldedIcs20Withdrawal(action)],
                    DeployedProofFamily::ShieldedIcs20Withdrawal,
                ) => &mut action.proof.inner,
                _ => anyhow::bail!(
                    "{} fixture did not contain its expected single proof action",
                    fixture.family.label()
                ),
            }
        }
        FixtureProofSlot::FeeFunding => {
            anyhow::ensure!(
                fixture.family == DeployedProofFamily::Transfer,
                "only Transfer may inhabit the fee-funding proof slot"
            );
            &mut tx
                .transaction_body
                .fee_funding
                .as_mut()
                .context("fee-funding fixture omitted its Transfer")?
                .transfer
                .proof
                .inner
        }
    };

    let mut remaining = proof_bytes.as_slice();
    let mut proof = Proof::<decaf377::Bls12_377>::deserialize_compressed(&mut remaining)
        .context("decoding canonical fixture proof")?;
    anyhow::ensure!(remaining.is_empty(), "fixture proof had trailing bytes");
    proof.c = proof.a;
    proof_bytes.clear();
    proof
        .serialize_compressed(proof_bytes)
        .context("serializing canonical invalid proof")?;
    let binding_signing_key = rdsa::SigningKey::<rdsa::Binding>::from(fixture.binding_blinding);
    tx.binding_sig = binding_signing_key.sign_deterministic(tx.auth_hash().as_bytes());

    let invalid_bytes = tx.encode_to_vec();
    Transaction::decode(invalid_bytes.as_slice())
        .context("mutated proof must remain canonically decodable")?;
    Ok((tx, invalid_bytes))
}

async fn process_request(app: &App, tx_bytes: &[u8]) -> Result<request::ProcessProposal> {
    let context = app.benchmark_block_context().await?;
    Ok(request::ProcessProposal {
        txs: vec![tx_bytes.to_vec().into()],
        proposed_last_commit: None,
        misbehavior: Vec::new(),
        hash: Hash::None,
        height: context.height,
        time: context.time,
        next_validators_hash: context.next_validators_hash,
        proposer_address: context.proposer_address,
    })
}

async fn prepare_request(app: &App, tx_bytes: Vec<u8>) -> Result<request::PrepareProposal> {
    let context = app.benchmark_block_context().await?;
    Ok(request::PrepareProposal {
        txs: vec![tx_bytes.into()],
        max_tx_bytes: i64::try_from(MAX_BLOCK_TXS_PAYLOAD_BYTES)?,
        local_last_commit: None,
        misbehavior: Vec::new(),
        height: context.height,
        time: context.time,
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    })
}

fn tx_hash(tx_bytes: &[u8]) -> [u8; 32] {
    sha2::Sha256::digest(tx_bytes).into()
}

fn assert_cache_invalid(cache: &StatelessCache, hash: &[u8; 32], tx_bytes: &[u8], context: &str) {
    assert!(
        matches!(cache.get(hash, tx_bytes), Some(CacheEntry::Invalid)),
        "{context}: failed proof must leave an Invalid cache entry"
    );
}

fn assert_cache_not_promoted(
    cache: &StatelessCache,
    hash: &[u8; 32],
    tx_bytes: &[u8],
    context: &str,
) {
    assert!(
        !matches!(
            cache.get(hash, tx_bytes),
            Some(CacheEntry::FullyVerified(_))
        ),
        "{context}: failed validation must never promote a Groth16-verified cache entry"
    );
}

async fn assert_no_tx_effects(app: &App, tx: &Transaction, context: &str) -> Result<()> {
    assert!(
        app.state.pending_nullifiers().is_empty(),
        "{context}: rejected proof staged nullifiers"
    );
    assert!(
        app.state.pending_note_payloads().is_empty(),
        "{context}: rejected proof staged note payloads"
    );
    for nullifier in tx.spent_nullifiers() {
        assert!(
            !app.state.is_nullifier_spent(nullifier).await?,
            "{context}: rejected proof marked a nullifier spent"
        );
    }
    Ok(())
}

async fn stage_spent_nullifier(app: &mut App, tx: &Transaction) -> Result<[u8; 32]> {
    let nullifier = tx
        .spent_nullifiers()
        .next()
        .context("fixture transaction has a spend nullifier")?;
    let source_id = [0x5au8; 32];
    let mut state_tx = app
        .state
        .try_begin_transaction()
        .context("test app state is uniquely owned")?;
    state_tx
        .nullify_all(
            std::slice::from_ref(&nullifier),
            CommitmentSource::Transaction {
                id: Some(source_id),
            },
        )
        .await?;
    state_tx.apply();
    Ok(source_id)
}

async fn put_open_withdrawal_route(app: &mut App, withdrawal: &Ics20Withdrawal) -> Result<()> {
    let mut state_tx = app
        .state
        .try_begin_transaction()
        .context("test app state is uniquely owned")?;
    let client_id = ClientId::from_str("07-tendermint-0")?;
    let latest_height = IbcHeight::new(1, 1)?;
    let client_state = ClientState::new(
        ChainId::new("counterparty".to_string(), 1),
        TrustThreshold::ONE_THIRD,
        Duration::from_secs(4_000_000_000),
        Duration::from_secs(5_000_000_000),
        Duration::from_secs(5),
        latest_height,
        IBC_PROOF_SPECS.to_vec(),
        vec![],
        AllowUpdate {
            after_expiry: false,
            after_misbehaviour: false,
        },
        None,
    )?;
    state_tx.put_client(&client_id, client_state);
    state_tx
        .put_verified_consensus_state::<ShielddHost>(
            latest_height,
            client_id.clone(),
            ConsensusState::new(
                MerkleRoot {
                    hash: vec![1u8; 32],
                },
                Time::from_unix_timestamp(1, 0)?,
                tendermint::Hash::Sha256([2u8; 32]),
            ),
        )
        .await?;

    let connection_id = ConnectionId::new(0);
    state_tx
        .put_new_connection(
            &connection_id,
            ConnectionEnd {
                state: ConnectionState::Open,
                client_id,
                counterparty: ConnectionCounterparty::default(),
                versions: vec![],
                delay_period: Duration::ZERO,
            },
        )
        .await?;

    let port = PortId::transfer();
    state_tx.put_channel(
        &withdrawal.source_channel,
        &port,
        ChannelEnd {
            state: ChannelState::Open,
            ordering: Order::Unordered,
            remote: ChannelCounterparty::new(port.clone(), Some(ChannelId::new(7))),
            connection_hops: vec![connection_id],
            version: Version::new("ics20-1".to_string()),
            ..ChannelEnd::default()
        },
    );
    state_tx.put_send_sequence(&withdrawal.source_channel, &port, 1);
    state_tx.apply();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn artifact_build_rejects_decodable_invalid_groth16() -> Result<()> {
    let family_set = family_fixtures().await?;

    for fixture in &family_set.fixtures {
        let (invalid_tx, _) = mutate_to_decodable_invalid_proof(fixture)?;
        let error = App::build_tx_artifacts(&[Arc::new(invalid_tx)])
            .await
            .err()
            .expect("artifact construction must reject an invalid proof");
        assert!(
            format!("{error:#}").contains("verification failed"),
            "{}: artifact construction failed for the wrong reason: {error:#}",
            fixture.label()
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn process_proposal_rejects_decodable_invalid_groth16() -> Result<()> {
    let family_set = family_fixtures().await?;

    for fixture in &family_set.fixtures {
        let (invalid_tx, invalid_bytes) = mutate_to_decodable_invalid_proof(fixture)?;
        let hash = tx_hash(&invalid_bytes);
        let cache = StatelessCache::new();
        let mut app = App::new(family_set._storage_guard.latest_snapshot());
        let proposal = process_request(&app, &invalid_bytes).await?;

        let verdict = app
            .process_proposal(proposal, Some(&cache), None, false)
            .await;
        assert!(
            matches!(verdict, response::ProcessProposal::Reject),
            "{}: ProcessProposal accepted a decodable invalid proof",
            fixture.label()
        );
        assert_cache_not_promoted(&cache, &hash, &invalid_bytes, fixture.label());
        assert_no_tx_effects(&app, &invalid_tx, fixture.label()).await?;
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn fee_funding_process_proposal_rejects_invalid_groth16() -> Result<()> {
    let family_set = family_fixtures().await?;
    let fixture = &family_set.fee_funding_fixture;
    let (invalid_tx, invalid_bytes) = mutate_to_decodable_invalid_proof(fixture)?;
    let hash = tx_hash(&invalid_bytes);
    let cache = StatelessCache::new();
    let mut app = App::new(family_set._storage_guard.latest_snapshot());
    let proposal = process_request(&app, &invalid_bytes).await?;

    let verdict = app
        .process_proposal(proposal, Some(&cache), None, false)
        .await;
    assert!(
        matches!(verdict, response::ProcessProposal::Reject),
        "ProcessProposal accepted an invalid fee-funding Transfer proof"
    );
    assert_cache_not_promoted(&cache, &hash, &invalid_bytes, fixture.label());
    assert_no_tx_effects(&app, &invalid_tx, fixture.label()).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn fee_funding_valid_proof_executes_and_persists() -> Result<()> {
    let family_set = family_fixtures().await?;
    let fixture = &family_set.fee_funding_fixture;
    let tx = Transaction::decode(fixture.tx_bytes.as_slice())
        .context("decoding valid fee-funding fixture")?;
    let fee_funding = tx
        .transaction_body
        .fee_funding
        .as_ref()
        .context("fee-funding fixture omitted its Transfer")?;
    let fee_nullifiers = fee_funding
        .transfer
        .body
        .inputs
        .iter()
        .map(|input| input.nullifier)
        .collect::<Vec<_>>();
    let fee_outputs = fee_funding
        .transfer
        .body
        .outputs
        .iter()
        .map(|output| output.note_payload.note_commitment)
        .collect::<Vec<_>>();
    let transaction_outputs = tx.state_commitments().collect::<Vec<_>>();
    assert!(
        fee_outputs
            .iter()
            .all(|commitment| transaction_outputs.contains(commitment)),
        "fee-funding outputs must be part of the transaction's canonical effect iterator"
    );

    let storage_guard = build_fixture_storage().await?;
    let storage = storage_guard.as_ref().clone();
    let mut app = App::new(storage.latest_snapshot());
    let context = app.benchmark_block_context().await?;
    let begin_block = App::begin_block_request_from_context(&context);
    app.begin_block(&begin_block).await;

    let cache = StatelessCache::new();
    app.deliver_tx_bytes(&fixture.tx_bytes, Some(&cache))
        .await
        .context("valid fee-funding proof must execute")?;
    let hash = tx_hash(&fixture.tx_bytes);
    let Some(CacheEntry::FullyVerified(artifact)) = cache.get(&hash, &fixture.tx_bytes) else {
        anyhow::bail!("valid fee-funding delivery did not retain an exact proof capability")
    };
    artifact
        .proof_for_slot(ProofSlot::BodyAction(0))
        .context("body Transfer capability missing after verification")?;
    artifact
        .proof_for_slot(ProofSlot::FeeFunding)
        .context("fee-funding Transfer capability missing after verification")?;

    for nullifier in &fee_nullifiers {
        assert!(
            app.state.is_nullifier_spent(*nullifier).await?,
            "verified fee-funding execution did not stage nullifier {nullifier:?}"
        );
    }

    app.end_block(&request::EndBlock {
        height: i64::try_from(context.height.value())?,
    })
    .await;
    app.commit(storage.clone()).await;

    let committed = storage.latest_snapshot();
    let compact_block: shieldd_sdk_compact_block::CompactBlock = committed
        .compact_block(context.height.value())
        .await?
        .context("committed fee-funding block must retain its compact block")?
        .try_into()?;
    let committed_outputs = compact_block
        .state_payloads
        .iter()
        .map(|payload| *payload.commitment())
        .collect::<Vec<_>>();
    assert_eq!(
        committed_outputs, transaction_outputs,
        "compact block must persist every body and fee-funding output"
    );
    for nullifier in fee_nullifiers {
        assert!(
            committed.is_nullifier_spent(nullifier).await?,
            "fee-funding nullifier {nullifier:?} was not durably committed"
        );
        assert!(
            compact_block.nullifiers.contains(&nullifier),
            "compact block omitted fee-funding nullifier {nullifier:?}"
        );
    }
    let transaction_log = committed
        .transactions_by_height(context.height.value())
        .await?;
    let [logged] = transaction_log.transactions.as_slice() else {
        anyhow::bail!(
            "fee-funding block must persist exactly one transaction, got {}",
            transaction_log.transactions.len()
        )
    };
    let logged = Transaction::try_from(logged.clone())?;
    assert_eq!(
        logged.encode_to_vec(),
        fixture.tx_bytes,
        "persisted transaction must retain the exact fee-funding Transfer"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn prepare_proposal_excludes_decodable_invalid_groth16() -> Result<()> {
    let family_set = family_fixtures().await?;

    for fixture in &family_set.fixtures {
        let (invalid_tx, invalid_bytes) = mutate_to_decodable_invalid_proof(fixture)?;
        let hash = tx_hash(&invalid_bytes);
        let cache = StatelessCache::new();
        let mut app = App::new(family_set._storage_guard.latest_snapshot());
        let proposal = prepare_request(&app, invalid_bytes.clone()).await?;

        let (prepared, _) = app.prepare_proposal(proposal, Some(&cache), false).await;
        assert!(
            prepared
                .txs
                .iter()
                .all(|tx_bytes| tx_bytes.as_ref() != invalid_bytes),
            "{}: PrepareProposal included a transaction with an invalid proof",
            fixture.label()
        );
        assert_cache_not_promoted(&cache, &hash, &invalid_bytes, fixture.label());
        assert_no_tx_effects(&app, &invalid_tx, fixture.label()).await?;
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cold_deliver_rejects_invalid_groth16_without_state_mutation() -> Result<()> {
    let family_set = family_fixtures().await?;

    for fixture in &family_set.fixtures {
        let (invalid_tx, invalid_bytes) = mutate_to_decodable_invalid_proof(fixture)?;
        let hash = tx_hash(&invalid_bytes);
        let cache = StatelessCache::new();
        let mut app = App::new(family_set._storage_guard.latest_snapshot());

        let error = app
            .deliver_tx_bytes(&invalid_bytes, Some(&cache))
            .await
            .expect_err("cold delivery must reject an invalid proof");
        assert!(
            format!("{error:#}").contains("Groth16"),
            "{}: cold delivery failed for the wrong reason: {error:#}",
            fixture.label()
        );
        assert_cache_invalid(&cache, &hash, &invalid_bytes, fixture.label());
        assert_no_tx_effects(&app, &invalid_tx, fixture.label()).await?;
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn deferred_index_records_only_transactions_that_commit() -> Result<()> {
    let family_set = family_fixtures().await?;
    let withdrawal = family_set
        .fixtures
        .iter()
        .find(|fixture| fixture.family == DeployedProofFamily::ShieldedIcs20Withdrawal)
        .context("withdrawal fixture is present")?;

    for mode in [
        BlockTxIndexingMode::PerTx,
        BlockTxIndexingMode::DeferredBatch,
    ] {
        let mut app = App::new(family_set._storage_guard.latest_snapshot());
        app.set_block_tx_indexing_mode(mode);
        let cache = StatelessCache::new();

        let error = app
            .deliver_tx_bytes(&withdrawal.tx_bytes, Some(&cache))
            .await
            .expect_err("withdrawal without an IBC route must reject after proof verification");
        assert!(
            format!("{error:#}").contains("channel channel-0")
                && format!("{error:#}").contains("does not exist"),
            "{mode:?}: withdrawal failed for the wrong reason: {error:#}"
        );
        assert!(
            app.deferred_block_transactions.is_empty(),
            "{mode:?}: rejected transaction escaped into the deferred index buffer"
        );

        app.flush_deferred_block_transactions().await?;
        let height = app.state.get_block_height().await?;
        let tx_log = app.state.transactions_by_height(height).await?;
        assert!(
            tx_log.transactions.is_empty(),
            "{mode:?}: rejected transaction was persisted in the block transaction index"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn proved_withdrawal_accounting_failure_rolls_back_action_effects() -> Result<()> {
    let family_set = family_fixtures().await?;
    let withdrawal_fixture = family_set
        .fixtures
        .iter()
        .find(|fixture| fixture.family == DeployedProofFamily::ShieldedIcs20Withdrawal)
        .context("withdrawal fixture is present")?;
    let tx = Transaction::decode(withdrawal_fixture.tx_bytes.as_slice())
        .context("decode proved withdrawal fixture")?;
    let withdrawal = match &tx.transaction_body.actions[..] {
        [Action::ShieldedIcs20Withdrawal(action)] => action.body.withdrawal.clone(),
        _ => anyhow::bail!("withdrawal fixture has the wrong action shape"),
    };

    let mut app = App::new(family_set._storage_guard.latest_snapshot());
    app.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    put_open_withdrawal_route(&mut app, &withdrawal).await?;

    let balance_key = shieldd_sdk_ibc::component::state_key::ics20_value_balance::by_asset_id(
        &withdrawal.source_channel,
        &withdrawal.denom.id(),
    );
    let mut setup_tx = app
        .state
        .try_begin_transaction()
        .context("test app state is uniquely owned")?;
    setup_tx.put(balance_key.clone(), Amount::from(u128::MAX));
    setup_tx.apply();

    let cache = StatelessCache::new();
    let error = app
        .deliver_tx_bytes(&withdrawal_fixture.tx_bytes, Some(&cache))
        .await
        .expect_err("proved withdrawal must reject escrow overflow after proof-bound effects");
    assert!(
        format!("{error:#}").contains("overflow adding value balance"),
        "proved withdrawal failed for the wrong reason: {error:#}"
    );

    assert_no_tx_effects(&app, &tx, "proved withdrawal accounting failure").await?;
    assert_eq!(
        app.state
            .get::<Amount>(&balance_key)
            .await?
            .context("preexisting escrow balance remains present")?,
        Amount::from(u128::MAX),
        "failed withdrawal must retain the exact pre-transaction escrow balance"
    );
    let port = PortId::transfer();
    assert_eq!(
        app.state
            .get_send_sequence(&withdrawal.source_channel, &port)
            .await?,
        1,
        "failed withdrawal must not allocate a packet sequence"
    );
    assert!(
        app.state
            .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 1)
            .await?
            .is_none(),
        "failed withdrawal must not persist a packet commitment"
    );
    assert!(
        app.deferred_block_transactions.is_empty(),
        "failed withdrawal must not escape into the deferred transaction index"
    );
    app.flush_deferred_block_transactions().await?;
    let height = app.state.get_block_height().await?;
    assert!(
        app.state
            .transactions_by_height(height)
            .await?
            .transactions
            .is_empty(),
        "failed withdrawal must not persist in the transaction index"
    );

    let replacement = Arc::new(StateDelta::new(family_set._storage_guard.latest_snapshot()));
    let state = std::mem::replace(&mut app.state, replacement);
    let state = Arc::try_unwrap(state)
        .map_err(|_| anyhow!("failed withdrawal test retained a state reference"))?;
    let (_, mut changes) = state.flatten();
    assert!(
        changes.take_events().is_empty(),
        "failed withdrawal must not leak staged ABCI events"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn proved_withdrawal_then_later_failure_rolls_back_first_action() -> Result<()> {
    let family_set = family_fixtures().await?;
    let tx = Transaction::decode(family_set.withdrawal_rollback_tx_bytes.as_slice())
        .context("decode proved two-withdrawal rollback fixture")?;
    let withdrawals = tx
        .transaction_body
        .actions
        .iter()
        .map(|action| match action {
            Action::ShieldedIcs20Withdrawal(action) => Ok(action.body.withdrawal.clone()),
            _ => anyhow::bail!("rollback fixture contains a non-withdrawal action"),
        })
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(
        withdrawals.len() == 2,
        "rollback fixture must contain exactly two withdrawals"
    );
    let withdrawal = &withdrawals[0];

    let mut app = App::new(family_set._storage_guard.latest_snapshot());
    app.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
    put_open_withdrawal_route(&mut app, withdrawal).await?;

    let port = PortId::transfer();
    let occupied_packet = Packet {
        sequence: 2u64.into(),
        port_on_a: port.clone(),
        chan_on_a: withdrawal.source_channel.clone(),
        port_on_b: port.clone(),
        chan_on_b: ChannelId::new(7),
        data: b"preexisting packet slot".to_vec(),
        timeout_height_on_b: IbcHeight::new(1, 10)?.into(),
        timeout_timestamp_on_b: Timestamp::from_nanoseconds(60_000_000_000)?,
    };
    let mut setup_tx = app
        .state
        .try_begin_transaction()
        .context("test app state is uniquely owned")?;
    setup_tx.put_packet_commitment(&occupied_packet);
    setup_tx.apply();

    let balance_key = shieldd_sdk_ibc::component::state_key::ics20_value_balance::by_asset_id(
        &withdrawal.source_channel,
        &withdrawal.denom.id(),
    );
    let cache = StatelessCache::new();
    let error = app
        .deliver_tx_bytes(&family_set.withdrawal_rollback_tx_bytes, Some(&cache))
        .await
        .expect_err("the occupied second packet slot must reject after the first withdrawal");
    assert!(
        format!("{error:#}").contains("packet commitment already exists")
            && format!("{error:#}").contains("sequence 2"),
        "proved rollback fixture failed before the later withdrawal: {error:#}"
    );

    assert_no_tx_effects(&app, &tx, "later withdrawal failure").await?;
    assert_eq!(
        app.state
            .get_send_sequence(&withdrawal.source_channel, &port)
            .await?,
        1,
        "the first withdrawal's sequence advance must roll back"
    );
    assert!(
        app.state
            .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 1)
            .await?
            .is_none(),
        "the first withdrawal's packet commitment must roll back"
    );
    assert!(
        app.state
            .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 2)
            .await?
            .is_some(),
        "the preexisting packet commitment must remain"
    );
    assert_eq!(
        app.state.get::<Amount>(&balance_key).await?,
        None,
        "the first withdrawal's escrow accounting must roll back"
    );
    assert!(
        app.deferred_block_transactions.is_empty(),
        "the rejected transaction must not enter the deferred index"
    );

    let replacement = Arc::new(StateDelta::new(family_set._storage_guard.latest_snapshot()));
    let state = std::mem::replace(&mut app.state, replacement);
    let state =
        Arc::try_unwrap(state).map_err(|_| anyhow!("rollback test retained a state reference"))?;
    let (_, mut changes) = state.flatten();
    assert!(
        changes.take_events().is_empty(),
        "the first withdrawal's events must roll back"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn extracted_cache_cannot_bypass_groth16_verification() -> Result<()> {
    let family_set = family_fixtures().await?;

    for fixture in &family_set.fixtures {
        let (invalid_tx, invalid_bytes) = mutate_to_decodable_invalid_proof(fixture)?;
        let hash = tx_hash(&invalid_bytes);
        let cache = StatelessCache::new();
        let mut artifacts = App::build_tx_artifacts_extracted_for_stage_public(
            "preseed_extracted",
            &[Arc::new(invalid_tx.clone())],
        )
        .await
        .with_context(|| format!("{} invalid proof remains extraction-valid", fixture.label()))?;
        let artifact = artifacts
            .pop()
            .context("single extracted transaction artifact missing")?;
        cache.insert_extracted(&invalid_bytes, artifact)?;
        let mut app = App::new(family_set._storage_guard.latest_snapshot());

        app.deliver_tx_bytes(&invalid_bytes, Some(&cache))
            .await
            .expect_err("an Extracted cache hit must still verify Groth16");
        assert_cache_invalid(&cache, &hash, &invalid_bytes, fixture.label());
        assert_no_tx_effects(&app, &invalid_tx, fixture.label()).await?;
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn host_delivery_rejects_invalid_groth16_cold_and_after_checktx() -> Result<()> {
    let family_set = family_fixtures().await?;
    let snapshot = family_set._storage_guard.latest_snapshot();
    let root_before = snapshot.root_hash().await?;
    let next_height = i64::try_from(snapshot.get_block_height().await?.saturating_add(1))?;
    let current_time = snapshot.get_current_block_timestamp().await?;
    let next_time = current_time
        .checked_add(Duration::from_secs(1))
        .unwrap_or(current_time);

    for fixture in &family_set.fixtures {
        let (_, invalid_bytes) = mutate_to_decodable_invalid_proof(fixture)?;
        let hash = tx_hash(&invalid_bytes);

        let checked_cache = Arc::new(StatelessCache::new());
        let mut checked_host = HostExecution::with_cache(
            family_set._storage_guard.as_ref().clone(),
            checked_cache.clone(),
        );
        let check_response = checked_host.check_tx(&invalid_bytes).await?;
        assert_ne!(
            check_response.code,
            0,
            "{}: HostExecution CheckTx accepted an invalid proof",
            fixture.label()
        );
        assert_cache_invalid(&checked_cache, &hash, &invalid_bytes, fixture.label());
        checked_host
            .begin_block(HostBlock {
                height: next_height,
                time: next_time,
            })
            .await?;
        let checked_delivery = checked_host.deliver_tx(&invalid_bytes).await?;
        assert_ne!(
            checked_delivery.code,
            0,
            "{}: HostExecution delivery accepted an invalid proof cached by CheckTx",
            fixture.label()
        );
        assert_cache_invalid(&checked_cache, &hash, &invalid_bytes, fixture.label());

        let cold_cache = Arc::new(StatelessCache::new());
        let mut cold_host = HostExecution::with_cache(
            family_set._storage_guard.as_ref().clone(),
            cold_cache.clone(),
        );
        cold_host
            .begin_block(HostBlock {
                height: next_height,
                time: next_time,
            })
            .await?;
        let cold_delivery = cold_host.deliver_tx(&invalid_bytes).await?;
        assert_ne!(
            cold_delivery.code,
            0,
            "{}: cold HostExecution delivery accepted an invalid proof",
            fixture.label()
        );
        assert_cache_invalid(&cold_cache, &hash, &invalid_bytes, fixture.label());
    }

    assert_eq!(
        family_set
            ._storage_guard
            .latest_snapshot()
            .root_hash()
            .await?,
        root_before,
        "rejected host proofs must not mutate durable state"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cache_promotion_never_exceeds_exact_groth16_attestation() -> Result<()> {
    let family_set = family_fixtures().await?;
    let transfer = family_set
        .fixtures
        .iter()
        .find(|fixture| fixture.family == DeployedProofFamily::Transfer)
        .context("Transfer fixture is present")?;
    let valid_tx = Transaction::decode(transfer.tx_bytes.as_slice())?;
    let valid_hash = tx_hash(&transfer.tx_bytes);

    let process_cache = StatelessCache::new();
    let mut process_app = App::new(family_set._storage_guard.latest_snapshot());
    stage_spent_nullifier(&mut process_app, &valid_tx).await?;
    let proposal = process_request(&process_app, &transfer.tx_bytes).await?;
    let verdict = process_app
        .process_proposal(proposal, Some(&process_cache), None, false)
        .await;
    assert!(matches!(verdict, response::ProcessProposal::Reject));
    assert_cache_not_promoted(
        &process_cache,
        &valid_hash,
        &transfer.tx_bytes,
        "stateful ProcessProposal failure",
    );

    let deliver_cache = StatelessCache::new();
    let mut deliver_app = App::new(family_set._storage_guard.latest_snapshot());
    stage_spent_nullifier(&mut deliver_app, &valid_tx).await?;
    let pending_nullifiers_before = deliver_app.state.pending_nullifiers();
    let pending_note_commitments_before = deliver_app
        .state
        .pending_note_payloads()
        .iter()
        .map(|(_, payload, _)| payload.note_commitment)
        .collect::<Vec<_>>();
    deliver_app
        .deliver_tx_bytes(&transfer.tx_bytes, Some(&deliver_cache))
        .await
        .expect_err("committed-nullifier conflict must reject delivery");
    assert!(
        matches!(
            deliver_cache.get(&valid_hash, &transfer.tx_bytes),
            Some(CacheEntry::FullyVerified(_))
        ),
        "stateful delivery failure may retain only the exact Groth16 attestation"
    );
    assert_eq!(
        deliver_app.state.pending_nullifiers(),
        pending_nullifiers_before,
        "stateful delivery rejection must not stage another nullifier"
    );
    assert_eq!(
        deliver_app
            .state
            .pending_note_payloads()
            .iter()
            .map(|(_, payload, _)| payload.note_commitment)
            .collect::<Vec<_>>(),
        pending_note_commitments_before,
        "stateful delivery rejection must not stage note outputs"
    );

    let (_, invalid_bytes) = mutate_to_decodable_invalid_proof(transfer)?;
    let invalid_hash = tx_hash(&invalid_bytes);
    let proof_cache = StatelessCache::new();
    let mut proof_app = App::new(family_set._storage_guard.latest_snapshot());
    proof_app
        .deliver_tx_bytes(&invalid_bytes, Some(&proof_cache))
        .await
        .expect_err("invalid Groth16 proof must reject delivery");
    assert_cache_not_promoted(&proof_cache, &invalid_hash, &invalid_bytes, "proof failure");

    Ok(())
}
