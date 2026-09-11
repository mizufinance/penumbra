#![cfg(feature = "disclosure-e2e")]
use anyhow::{ensure, Context, Result};
use decaf377::{Encoding, Fq, Fr};
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use rand_core::OsRng;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use shieldd_sdk_app::{
    app::{HostBlock, StateReadExt},
    genesis::{self, AppState},
    test_support::{TestHost, TEST_CHAIN_ID},
};
use shieldd_sdk_asset::{asset::REGISTRY, Value};
use shieldd_sdk_compliance::{
    structs::{
        AssetRegistrationGrant, OrbisCapabilityCertificate, UserRegistrationGrant,
        UserRegistrationGrantBody,
    },
    AuditKeys, ComplianceLeaf, DetectionKey, MsgRegisterAsset, MsgRegisterUser,
};
use shieldd_sdk_disclosure::{
    AuditAccess, AuditPolicy, AuditRegistration, AuditRegistrationRequest, AuditSelection,
    MasterSelection, OutputRef,
};
use shieldd_sdk_keys::test_keys;
use shieldd_sdk_mock_client::{ActionIntent, MockClient, TransactionIntent, TransferIntent};
use shieldd_sdk_proto::{
    core::component::compliance::v1 as cpb,
    execution_client::v1::{DepositRequest, HostSource},
    DomainType,
};
use shieldd_sdk_shielded_pool::{ShieldedInputPlan, ShieldedOutputPlan};
use shieldd_sdk_transaction::{plan::ActionPlan, TransactionParameters};
use std::{path::PathBuf, process::Stdio};
use tokio::io::AsyncWriteExt;
mod common;
#[path = "common/queries.rs"]
mod queries;

#[derive(Deserialize)]
struct Fixture {
    root: PathBuf,
    policy: String,
    ring: String,
    shieldd_node: String,
}
#[derive(Deserialize)]
struct Key {
    public_key: [u8; 32],
    evaluations: Json,
}
fn keys(records: &[Key]) -> Result<AuditKeys> {
    ensure!(records.len() == 3, "three registered fields required");
    let point = |i: usize| {
        Encoding(records[i].public_key)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid registered point"))
    };
    Ok(AuditKeys {
        epoch: 1,
        amount: point(0)?,
        sender: point(1)?,
        receiver: point(2)?,
    })
}
async fn helper(fixture: &str, request: Json) -> Result<Json> {
    let mut child = tokio::process::Command::new("python3")
        .arg(std::env::var("BANKD_ORBIS_LIVE_HELPER")?)
        .args([
            "--fixture",
            fixture,
            "--orbis-bin",
            &std::env::var("ORBIS_AUDIT_TEST_BINARY")?,
            "--shieldd-bin",
            &std::env::var("SHIELDD_PCLI_BIN")?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().context("missing helper stdin")?;
    stdin.write_all(&serde_json::to_vec(&request)?).await?;
    drop(stdin);
    let output = child.wait_with_output().await?;
    ensure!(output.status.success(), "live registration helper failed");
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires five live Orbis nodes, Vera, initialized LaKey and real Transfer proving"]
async fn accepted_transaction_through_live_orbis() -> Result<()> {
    run_audit(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires live certified registrations and accepted issuer disclosure"]
async fn accepted_issuer_disclosure() -> Result<()> {
    run_audit(true).await
}

async fn run_audit(issuer_only: bool) -> Result<()> {
    shieldd_sdk_shielded_pool::gnark::require_proof_test_runtime(
        shieldd_sdk_shielded_pool::gnark::ProofTestFamily::Transfer,
    )?;
    let fixture_path = std::env::var("ORBIS_LIVE_FIXTURE")?;
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(&fixture_path)?)?;
    let general: Vec<Key> =
        serde_json::from_slice(&std::fs::read(fixture.root.join("general.json"))?)?;
    let ring: Json = serde_json::from_slice(&std::fs::read(fixture.root.join("ring.json"))?)?;
    let ring_pk = Encoding(
        hex::decode(ring["ring_pk"].as_str().context("ring key missing")?)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("ring key size"))?,
    )
    .vartime_decompress()
    .map_err(|_| anyhow::anyhow!("ring point invalid"))?;
    let authority = SigningKey::<SpendAuth>::from(Fr::from(19001u64));
    let authority_vk = VerificationKey::from(&authority);
    let issuer = DetectionKey::new(Fr::from(19002u64));
    let denom = "live_orbis_asset";
    let asset = REGISTRY.parse_denom(denom).context("fixture asset")?.id();
    let mut registration = MsgRegisterAsset {
        audit_certificate: None,
        asset_registration_grant: None,
        daily_volume_limit: None,
        allowed_ibc_routes: vec![],
        ibc_origin: None,
        audit_keys: Some(keys(&general)?),
        asset_id: asset,
        is_regulated: true,
        dk_pub: Some(issuer.public_key()),
        registration_authority_vk: Some(authority_vk),
        seizure_authority_vk: Some(authority_vk),
        ring_pk: Some(ring_pk),
        ring_id: fixture.ring.clone(),
        policy_id: fixture.policy.clone(),
        permission: "read".into(),
        resource: "shieldd_audit".into(),
    };
    let mut content = genesis::Content::default().with_chain_id(TEST_CHAIN_ID.into());
    content
        .compliance_content
        .compliance_registrar_vk
        .push(authority_vk);
    let storage = common::new_storage().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let time = tendermint::Time::from_unix_timestamp(now.try_into()?, 0)?;
    let mut host =
        TestHost::new(storage.as_ref().clone(), AppState::Content(content), time).await?;
    host.execute(vec![]).await?;
    let listener = tokio::net::TcpListener::bind(
        fixture
            .shieldd_node
            .strip_prefix("http://")
            .context("fixture endpoint")?,
    )
    .await?;
    let server = tonic::transport::Server::builder()
        .add_service(queries::CommittedQueries(storage.as_ref().clone()))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener));
    let server = tokio::spawn(server);
    let mut client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;
    let intent = |actions| TransactionIntent {
        actions,
        nullifier_window: None,
        memo: None,
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TEST_CHAIN_ID.into(),
            ..Default::default()
        },
    };
    let body = registration.registration_grant_body(now + 3600);
    registration.asset_registration_grant = Some(AssetRegistrationGrant {
        registrar_vk: authority_vk,
        signature: authority.sign(OsRng, &body.signing_bytes()),
        body,
    });
    let request = AuditRegistrationRequest {
        version: 1,
        chain_id: TEST_CHAIN_ID.into(),
        registration: AuditRegistration::General {
            action: registration.clone(),
        },
    };
    let result = helper(&fixture_path, json!({"kind":"certify", "registration":request,
        "evaluations": general.iter().map(|record| record.evaluations.clone()).collect::<Vec<_>>()})).await?;
    let signature: Vec<u8> = serde_json::from_value(result["certificate"].clone())?;
    ensure!(signature.len() == 64, "general certificate signature size");
    registration.audit_certificate = Some(OrbisCapabilityCertificate::try_from(
        cpb::OrbisCapabilityCertificate {
            chain_id: TEST_CHAIN_ID.into(),
            r_point: signature[..32].to_vec(),
            response: signature[32..].to_vec(),
        },
    )?);
    let plan = client
        .complete_intent(
            intent(vec![ActionIntent::Complete(
                ActionPlan::ComplianceRegisterAsset(registration),
            )]),
            storage.latest_snapshot(),
        )
        .await?;
    let transaction = client.witness_auth_build(&plan).await?;
    host.execute(vec![transaction.encode_to_vec()]).await?;
    let mut actions = vec![];
    let mut person_keys = vec![];
    for (index, address) in [test_keys::ADDRESS_0.clone(), test_keys::ADDRESS_1.clone()]
        .into_iter()
        .enumerate()
    {
        let records: Vec<Key> = serde_json::from_value(
            helper(
                &fixture_path,
                json!({"kind":"keys", "scope":{"kind":"person","identity":address.to_vec()}}),
            )
            .await?,
        )?;
        let rnk_dh_pk = address.diversified_generator().clone();
        let rnk = shieldd_sdk_compliance::derive_regulated_nullifier_key(
            client.fvk.incoming(),
            &address,
            asset,
            ring_pk,
            rnk_dh_pk,
        )?;
        let leaf = ComplianceLeaf::registered_from_rnk(
            address,
            asset,
            ring_pk,
            rnk_dh_pk,
            rnk,
            keys(&records)?,
        )?;
        let body = UserRegistrationGrantBody {
            leaf: leaf.clone(),
            policy_id: fixture.policy.clone(),
            valid_until_unix: now + 3600,
            nonce: vec![index as u8 + 1; 32],
        };
        let grant = UserRegistrationGrant {
            signature: authority.sign(OsRng, &body.signing_bytes()),
            body,
        };
        let mut action = MsgRegisterUser {
            leaf,
            grant: Some(grant),
            capability_certificate: None,
        };
        let request = AuditRegistrationRequest {
            version: 1,
            chain_id: TEST_CHAIN_ID.into(),
            registration: AuditRegistration::Person {
                action: action.clone(),
            },
        };
        let result = helper(&fixture_path, json!({"kind":"certify", "registration":request,
            "evaluations": records.iter().map(|record| record.evaluations.clone()).collect::<Vec<_>>()})).await?;
        let signature: Vec<u8> = serde_json::from_value(result["certificate"].clone())?;
        ensure!(signature.len() == 64, "certificate signature size");
        action.capability_certificate = Some(OrbisCapabilityCertificate::try_from(
            cpb::OrbisCapabilityCertificate {
                chain_id: TEST_CHAIN_ID.into(),
                r_point: signature[..32].to_vec(),
                response: signature[32..].to_vec(),
            },
        )?);
        actions.push(ActionIntent::Complete(ActionPlan::ComplianceRegisterUser(
            action,
        )));
        person_keys.push(records);
    }
    let plan = client
        .complete_intent(intent(actions), storage.latest_snapshot())
        .await?;
    let registrations = client.witness_auth_build(&plan).await?;
    host.execute(vec![registrations.encode_to_vec()]).await?;
    host.execution
        .begin_block(HostBlock {
            height: 4,
            time: time
                .checked_add(std::time::Duration::from_secs(3))
                .context("time overflow")?,
        })
        .await?;
    host.execution
        .deposit(DepositRequest {
            denom: denom.into(),
            amount: "42".into(),
            recipient: test_keys::ADDRESS_0.to_string(),
            source: Some(HostSource {
                height: 4,
                tx_hash: vec![19; 32],
                tx_index: 0,
                msg_index: 0,
            }),
        })
        .await?;
    host.execution.end_block(4).await?;
    host.execution.commit().await?;
    client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&storage)
        .await?;
    let note = client
        .notes
        .values()
        .find(|note| note.asset_id() == asset)
        .context("regulated deposit missing")?
        .clone();
    let spend = ShieldedInputPlan::new(
        &mut OsRng,
        note.clone(),
        client.position(note.commit()).context("note position")?,
    );
    let output = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: 42u64.into(),
            asset_id: asset,
        },
        test_keys::ADDRESS_1.clone(),
    );
    let mut plan = client
        .complete_intent(
            intent(vec![TransferIntent {
                spends: vec![spend],
                outputs: vec![output],
                value_blinding: Fr::from(1u64),
            }
            .into()]),
            storage.latest_snapshot(),
        )
        .await?;
    let ActionPlan::Transfer(transfer) = plan.actions.first_mut().context("missing transfer")?
    else {
        anyhow::bail!("expected transfer plan");
    };
    ensure!(
        transfer
            .compliance
            .witness
            .policy
            .as_ref()
            .context("missing policy")?
            .params
            .daily_volume_limit
            >= 42,
        "fixture exceeds private volume limit"
    );
    if !issuer_only {
        transfer.volume_accumulator = shieldd_sdk_shielded_pool::VolumeAccumulatorPlan::origin(
            shieldd_sdk_shielded_pool::VolumeAccumulatorState {
                subject: shieldd_sdk_shielded_pool::VolumeAccumulatorState::subject(
                    &test_keys::ADDRESS_0,
                    asset,
                ),
                day_start: shieldd_sdk_shielded_pool::select_accumulator_day(
                    transfer.compliance.timestamp,
                ),
                undisclosed_volume: 42,
                blinding: Fq::rand(&mut OsRng),
            },
        );
    }
    if let Ok(script) = std::env::var("BANKD_BROWSER_WITNESS_TEST") {
        let witness = client.witness_plan(&plan)?;
        let ActionPlan::Transfer(transfer) = &plan.actions[0] else {
            unreachable!()
        };
        let paths = transfer
            .spends
            .iter()
            .map(|spend| {
                witness
                    .state_commitment_proofs
                    .get(&spend.note.commit())
                    .cloned()
                    .context("missing spend proof")
            })
            .collect::<Result<Vec<_>>>()?;
        let expected = transfer.transfer_witness_payload(
            &client.fvk,
            paths,
            witness.anchor,
            plan.recent_position_floor()?,
        )?;
        let input = json!({"plan":plan.encode_to_vec(),"action":plan.actions[0].encode_to_vec(),
            "fvk":client.fvk.encode_to_vec(),"witness":witness.encode_to_vec(),"expected":expected});
        let mut private = tempfile::NamedTempFile::new_in(&fixture.root)?;
        std::io::Write::write_all(&mut private, &serde_json::to_vec(&input)?)?;
        let status = tokio::process::Command::new("node")
            .arg(script)
            .arg(private.path())
            .status()
            .await?;
        ensure!(status.success(), "browser/native Transfer witness mismatch");
    }
    let transaction = client.witness_auth_build(&plan).await?;
    host.execute_block(
        HostBlock {
            height: 5,
            time: time
                .checked_add(std::time::Duration::from_secs(3))
                .context("time overflow")?,
        },
        vec![transaction.encode_to_vec()],
    )
    .await?;
    let accepted = storage.latest_snapshot().transactions_by_height(5).await?;
    ensure!(
        accepted.transactions.len() == 1,
        "transaction acceptance missing"
    );
    let mut selections: Vec<_> = [
        MasterSelection::Amount,
        MasterSelection::Sender,
        MasterSelection::Receiver,
    ]
    .into_iter()
    .map(|value| AuditSelection {
        version: 2,
        chain_id: TEST_CHAIN_ID.into(),
        reference: OutputRef {
            height: 5,
            transaction_id: transaction.id().to_string(),
            action: shieldd_sdk_disclosure::ActionRef::Body(0),
            output: 0,
        },
        access: AuditAccess::General { value },
        policy: AuditPolicy {
            ring_id: fixture.ring.clone(),
            policy_id: fixture.policy.clone(),
            resource: "shieldd_audit".into(),
            permission: "read".into(),
        },
    })
    .collect();
    use shieldd_sdk_disclosure::{DecodedAuditValue, TransferTier};
    let amount = DecodedAuditValue::Amount {
        base_units: "42".into(),
    };
    let components = |address: &shieldd_sdk_keys::Address| DecodedAuditValue::AddressComponents {
        diversified_generator: address.diversified_generator().vartime_compress().0,
        transmission_key: address.transmission_key().0,
    };
    let sender = components(&test_keys::ADDRESS_0);
    let receiver = components(&test_keys::ADDRESS_1);
    let mut expected = vec![amount.clone(), sender.clone(), receiver.clone()];
    let mut evaluations: Vec<_> = general
        .iter()
        .map(|key| json!({"evaluations":key.evaluations}))
        .collect();
    for (person, field, tier, value) in [
        (0, 0, TransferTier::SenderCore, amount.clone()),
        (0, 2, TransferTier::SenderExt, receiver),
        (1, 0, TransferTier::OutputCore, amount),
        (1, 1, TransferTier::OutputExt, sender),
    ] {
        let mut selection = selections[0].clone();
        selection.access = AuditAccess::NamedPerson {
            tier,
            address: [
                test_keys::ADDRESS_0.to_string(),
                test_keys::ADDRESS_1.to_string(),
            ][person]
                .clone(),
        };
        selections.push(selection);
        evaluations.push(json!({"evaluations":person_keys[person][field].evaluations}));
        expected.push(value);
    }
    std::fs::write(
        fixture.root.join("accepted-keys.json"),
        serde_json::to_vec(&evaluations)?,
    )?;
    std::fs::write(
        fixture.root.join("accepted-expected.json"),
        serde_json::to_vec(&expected)?,
    )?;
    let package = fixture.root.join("accepted-selections.json");
    std::fs::write(&package, serde_json::to_vec(&selections)?)?;
    let issuer_file = fixture.root.join("accepted-issuer.json");
    if issuer_only {
        use shieldd_sdk_disclosure::{
            accepted_audit_ciphertext, prepare_issuer_disclosure, AcceptedBlock,
            IssuerDisclosureKind, IssuerRequest,
        };
        let block = AcceptedBlock {
            height: 5,
            transactions: vec![transaction.clone()],
        };
        let mut packages = vec![];
        for selection in &selections[..3] {
            let accepted = accepted_audit_ciphertext(selection.clone(), TEST_CHAIN_ID, &block)?;
            let request = IssuerRequest {
                kind: IssuerDisclosureKind::Issuer,
                version: 1,
                recipient: None,
                challenge: None,
                selection: selection.clone(),
                asset: asset.to_string(),
            };
            packages.push(prepare_issuer_disclosure(
                OsRng, &accepted, request, &issuer,
            )?);
        }
        std::fs::write(&issuer_file, serde_json::to_vec(&packages)?)?;
    }
    let runner = std::env::var("BANKD_ORBIS_TEST_BIN")?;
    let status = tokio::process::Command::new(runner)
        .args([
            "-test.run",
            "^TestAcceptedOrbisCollectionWorkflow$",
            "-test.v",
        ])
        .env("BANKD_TEST_SELECTIONS", &package)
        .env("BANKD_TEST_NODE", &fixture.shieldd_node)
        .env(
            "BANKD_TEST_ISSUER_DISCLOSURES",
            if issuer_only {
                issuer_file.as_os_str()
            } else {
                std::ffi::OsStr::new("")
            },
        )
        .status()
        .await?;
    ensure!(status.success(), "Bankd live collection workflow failed");
    server.abort();
    Ok(())
}
