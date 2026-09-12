#![cfg(feature = "disclosure-e2e")]
use anyhow::{ensure, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use decaf377::Fr;
use rand_core::OsRng;
use shieldd_sdk_app::{
    app::StateReadExt,
    genesis::{self, AppState},
    test_support::{TestHost, TEST_CHAIN_ID},
};
use shieldd_sdk_asset::{Value, BASE_ASSET_ID};
use shieldd_sdk_disclosure as disclosure;
use shieldd_sdk_keys::test_keys;
use shieldd_sdk_mock_client::MockClient;
use shieldd_sdk_proto::{core::app::v1 as pb, DomainType};
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_shielded_pool::{ShieldedInputPlan, ShieldedOutputPlan};
use shieldd_sdk_transaction::TransactionParameters;
use shieldd_sdk_view::Storage;
use std::process::Stdio;
use tonic::codegen::*;
mod common;

#[derive(Clone)]
struct CommittedQueries {
    parameters: pb::AppParametersResponse,
    block: pb::TransactionsByHeightResponse,
}
impl tonic::server::NamedService for CommittedQueries {
    const NAME: &'static str = "mizufinance.shieldd.v1.Query";
}
struct Parameters(pb::AppParametersResponse);
impl tonic::server::UnaryService<pb::AppParametersRequest> for Parameters {
    type Response = pb::AppParametersResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
    fn call(&mut self, _: tonic::Request<pb::AppParametersRequest>) -> Self::Future {
        let response = self.0.clone();
        Box::pin(async move { Ok(tonic::Response::new(response)) })
    }
}
struct Transactions(pb::TransactionsByHeightResponse);
impl tonic::server::UnaryService<pb::TransactionsByHeightRequest> for Transactions {
    type Response = pb::TransactionsByHeightResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
    fn call(&mut self, request: tonic::Request<pb::TransactionsByHeightRequest>) -> Self::Future {
        let response = self.0.clone();
        Box::pin(async move {
            if request.into_inner().block_height != response.block_height {
                return Err(tonic::Status::not_found("block unavailable"));
            }
            Ok(tonic::Response::new(response))
        })
    }
}
impl<B> Service<http::Request<B>> for CommittedQueries
where
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;
    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let data = self.clone();
        Box::pin(async move {
            let response = match request.uri().path() {
                "/mizufinance.shieldd.v1.Query/AppParameters" => {
                    tonic::server::Grpc::new(tonic::codec::ProstCodec::default())
                        .unary(Parameters(data.parameters), request)
                        .await
                }
                "/mizufinance.shieldd.v1.Query/TransactionsByHeight" => {
                    tonic::server::Grpc::new(tonic::codec::ProstCodec::default())
                        .unary(Transactions(data.block), request)
                        .await
                }
                _ => http::Response::builder()
                    .status(200)
                    .header("grpc-status", "12")
                    .header("content-type", "application/grpc")
                    .body(tonic::body::empty_body())
                    .unwrap(),
            };
            Ok(response)
        })
    }
}

async fn cli(binary: &str, home: &Utf8Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = tokio::process::Command::new(binary)
        .arg("--home")
        .arg(home)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?
        .wait_with_output()
        .await?;
    ensure!(
        output.status.success(),
        "pcli failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(output.stdout)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "builds an accepted payment and generates a real local Groth16 disclosure"]
async fn export_import_between_wallet_directories() -> Result<()> {
    run_disclosure(true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "builds an accepted payment and verifies its commitment opening without proving"]
async fn accepted_disclosure_opening() -> Result<()> {
    run_disclosure(false).await
}

async fn machine(
    binary: &str,
    home: &Utf8Path,
    package: &disclosure::DisclosurePackage,
    node: &str,
) -> Result<serde_json::Value> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(binary)
        .args([
            "--home",
            home.as_str(),
            "disclosure",
            "verify-machine",
            "--node",
            node,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let input = serde_json::to_vec(
        &serde_json::json!({"version":2,"package":package,"request":package.statement.request}),
    )?;
    child
        .stdin
        .take()
        .context("stdin")?
        .write_all(&input)
        .await?;
    let output = child.wait_with_output().await?;
    ensure!(
        output.status.success(),
        "machine verification process failed"
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn run_disclosure(prove: bool) -> Result<()> {
    if std::env::var_os("BANKD_DISCLOSURE_TEST_BIN").is_some() {
        for name in ["BANKD_AUDIT_CLI", "DEFRA_TEST_BIN", "VERA_TEST_FIXTURE"] {
            let path = std::env::var(name)
                .with_context(|| format!("{name} required for Bankd workflow"))?;
            ensure!(
                std::path::Path::new(&path).is_file(),
                "{name} does not name an existing file"
            );
        }
    }
    if prove {
        for name in ["SHIELDD_DISCLOSURE_ARTIFACTS", "SHIELDD_DISCLOSURE_BACKEND"] {
            let path =
                std::env::var(name).with_context(|| format!("{name} required for proving"))?;
            ensure!(
                std::path::Path::new(&path).exists(),
                "{name} does not exist"
            );
        }
    }
    shieldd_sdk_shielded_pool::gnark::require_proof_test_runtime(
        shieldd_sdk_shielded_pool::gnark::ProofTestFamily::Transfer,
    )?;
    let directory = tempfile::tempdir()?;
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned())
        .map_err(|_| anyhow::anyhow!("non-UTF8 temporary path"))?;
    let sender = root.join("sender");
    let recipient = root.join("recipient");
    std::fs::create_dir(&sender)?;
    std::fs::create_dir(&recipient)?;
    let chain = common::new_storage().await?;
    let mut host = TestHost::new(
        chain.as_ref().clone(),
        AppState::Content(genesis::Content::default().with_chain_id(TEST_CHAIN_ID.into())),
        tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?,
    )
    .await?;
    host.execute(vec![]).await?;
    let wallet_path = sender.join("wallet.sqlite");
    let wallet = Storage::initialize(
        Some(&wallet_path),
        test_keys::FULL_VIEWING_KEY.clone(),
        chain.latest_snapshot().get_app_params().await?,
    )
    .await?;
    common::scan_latest(&chain, &wallet).await?;
    let client = MockClient::new(test_keys::SPEND_KEY.clone())
        .with_sync_to_storage(&chain)
        .await?;
    let note = client
        .notes
        .values()
        .find(|n| n.asset_id() == *BASE_ASSET_ID && n.address() == *test_keys::ADDRESS_0)
        .context("missing genesis note")?
        .clone();
    let spend = ShieldedInputPlan::new(
        &mut OsRng,
        note.clone(),
        client
            .position(note.commit())
            .context("missing input position")?,
    );
    let output = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: 42u64.into(),
            asset_id: note.asset_id(),
        },
        test_keys::ADDRESS_1.clone(),
    );
    let change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: note
                .amount()
                .checked_sub(&42u64.into())
                .context("insufficient genesis note")?,
            asset_id: note.asset_id(),
        },
        note.address(),
    );
    let memo = shieldd_sdk_transaction::memo::MemoPlaintext::new(
        test_keys::ADDRESS_0.clone(),
        "PRIVATE-MEMO".into(),
    )?;
    let intent = shieldd_sdk_mock_client::TransactionIntent {
        nullifier_window: None,
        actions: vec![shieldd_sdk_mock_client::TransferIntent {
            spends: vec![spend],
            outputs: vec![output, change],
            value_blinding: Fr::from(1u64),
        }
        .into()],
        memo: Some(shieldd_sdk_transaction::plan::MemoPlan::new(
            &mut OsRng, memo,
        )),
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: TEST_CHAIN_ID.into(),
            ..Default::default()
        },
    };
    let plan = client
        .complete_intent(intent, chain.latest_snapshot())
        .await?;
    eprintln!("Building and accepting the payment");
    let tx = wallet
        .build_transaction(
            plan.clone(),
            &client.witness_plan(&plan)?,
            &client.authorize_plan(&plan)?,
        )
        .await?;
    let authority = wallet.disclosure_authority(&tx.id().to_string(), 0).await?;
    ensure!(
        authority.randomizer
            == plan
                .spends()
                .next()
                .context("missing spend")?
                .spend
                .randomizer
                .to_bytes(),
        "wrong retained authority"
    );
    host.execute(vec![tx.encode_to_vec()]).await?;
    common::scan_latest(&chain, &wallet).await?;
    let snapshot = chain.latest_snapshot();
    let height = snapshot.get_block_height().await?;
    let accepted = snapshot.transactions_by_height(height).await?;
    ensure!(
        accepted.transactions.len() == 1,
        "payment did not enter accepted transaction log"
    );
    let request = disclosure::DisclosureRequest {
        version: disclosure::VERSION,
        chain_id: TEST_CHAIN_ID.into(),
        recipient: Some("recipient".into()),
        challenge: Some("c67d2eb6f94e495e822c77bbd833236fbfbd4ed37a08452f9724cce95f22c452".into()),
        total: None,
        outputs: vec![disclosure::OutputClaim {
            reference: disclosure::OutputRef {
                transaction_id: tx.id().to_string(),
                height,
                action: disclosure::ActionRef::Body(0),
                output: 0,
            },
            amount: false,
            asset: true,
            recipient: false,
            predicate: Some(disclosure::AmountPredicate::AtLeast("42".into())),
            memo: false,
            spending_control: false,
        }],
    };
    let witness = wallet.prepare_disclosure(request.clone()).await?;
    let statement = disclosure::evaluate(&witness)?;
    let block = disclosure::AcceptedBlock {
        height,
        transactions: vec![tx.clone()],
    };
    disclosure::confirm_acceptance(&statement, TEST_CHAIN_ID, &[block.clone()])?;
    let mut substituted = statement.clone();
    let dummy_key: [u8; 32] = tx
        .transfers()
        .next()
        .context("missing transfer")?
        .body
        .inputs[1]
        .rk
        .into();
    substituted.outputs[0].public.spend_verification_key = Some(dummy_key.to_vec());
    ensure!(
        disclosure::confirm_acceptance(&substituted, TEST_CHAIN_ID, &[block.clone()]).is_err(),
        "dummy authority substitution accepted"
    );
    let mut changed_reference = statement.clone();
    changed_reference.request.outputs[0].reference.output = 1;
    changed_reference.outputs[0].public.reference.output = 1;
    ensure!(
        disclosure::confirm_acceptance(&changed_reference, TEST_CHAIN_ID, &[block]).is_err(),
        "altered output reference accepted"
    );
    if !prove {
        let mut full = witness.clone();
        let claim = &mut full.request.outputs[0];
        claim.amount = true;
        claim.asset = true;
        claim.recipient = true;
        let package = disclosure::export_openings(&full)?;
        disclosure::verify(&package)?;
        let binary = std::env::var("SHIELDD_PCLI_BIN").context("set SHIELDD_PCLI_BIN")?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(CommittedQueries {
                    parameters: pb::AppParametersResponse {
                        app_parameters: Some(snapshot.get_app_params().await?.into()),
                    },
                    block: accepted,
                })
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener)),
        );
        let result = machine(&binary, &recipient, &package, &endpoint).await?;
        ensure!(
            result["status"] == "verified",
            "accepted opening not verified: {result}"
        );
        let mut changed = package.clone();
        changed.statement.request.chain_id = "wrong-chain".into();
        let result = machine(&binary, &recipient, &changed, &endpoint).await?;
        ensure!(
            result["status"] == "rejected" && result["result"]["acceptance"] == "Rejected",
            "chain mismatch not rejected: {result}"
        );
        let mut changed = package.clone();
        changed.statement.request.outputs[0]
            .reference
            .transaction_id = "ff".repeat(32);
        changed.statement.outputs[0].public.reference.transaction_id = "ff".repeat(32);
        let result = machine(&binary, &recipient, &changed, &endpoint).await?;
        ensure!(
            result["status"] == "rejected" && result["result"]["acceptance"] == "Rejected",
            "transaction mismatch not rejected: {result}"
        );
        server.abort();
        let _ = server.await;
        let result = machine(&binary, &recipient, &package, &endpoint).await?;
        ensure!(
            result["status"] == "unresolved",
            "node outage not unresolved: {result}"
        );
        return Ok(());
    }
    let binary = std::env::var("SHIELDD_PCLI_BIN")
        .context("set SHIELDD_PCLI_BIN to pcli built with disclosure-prover")?;
    let request_path = sender.join("request.json");
    std::fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let proof_path = sender.join("proof.json");
    let before_height = wallet.last_sync_height().await?;
    let before_notes = wallet.balances(None, None).await?;
    eprintln!("Exporting real hidden-memo disclosure");
    let started = std::time::Instant::now();
    cli(
        &binary,
        &sender,
        &[
            "disclosure",
            "export",
            "--wallet",
            wallet_path.as_str(),
            "--request",
            request_path.as_str(),
            "--output",
            proof_path.as_str(),
        ],
    )
    .await?;
    let bytes = std::fs::read(&proof_path)?;
    let package = disclosure::decode_package(&bytes)?;
    let disclosure::Evidence::Groth16 {
        proof: receipt_bytes,
        ..
    } = &package.evidence
    else {
        anyhow::bail!("export did not produce a ZK receipt")
    };
    eprintln!(
        "Real CLI disclosure: {:?}, {} receipt bytes, {} package bytes",
        started.elapsed(),
        receipt_bytes.len(),
        bytes.len()
    );
    let public = serde_json::to_string(&package.statement)?;
    ensure!(
        !public.contains("payload_key")
            && !public.contains("rseed")
            && package.statement.outputs[0].amount.is_none()
            && package.statement.outputs[0].recipient.is_none()
            && package.statement.outputs[0].memo.is_none(),
        "unrequested private fields entered the public statement"
    );
    ensure!(
        !String::from_utf8_lossy(&bytes).contains("PRIVATE-MEMO"),
        "private memo leaked"
    );
    ensure!(
        wallet.last_sync_height().await? == before_height
            && wallet.balances(None, None).await? == before_notes,
        "disclosure changed wallet availability"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let (shutdown, stopping) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(CommittedQueries {
                parameters: pb::AppParametersResponse {
                    app_parameters: Some(snapshot.get_app_params().await?.into()),
                },
                block: accepted,
            })
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async {
                    let _ = stopping.await;
                },
            ),
    );
    cli(
        &binary,
        &recipient,
        &["disclosure", "inspect", proof_path.as_str()],
    )
    .await?;
    let offline = cli(
        &binary,
        &recipient,
        &["disclosure", "verify", proof_path.as_str()],
    )
    .await?;
    let offline: disclosure::VerificationResult = serde_json::from_slice(&offline)?;
    ensure!(
        offline.cryptography_verified && !offline.fully_verified(),
        "offline verification incorrectly confirmed acceptance"
    );
    let result = cli(
        &binary,
        &recipient,
        &[
            "disclosure",
            "verify",
            proof_path.as_str(),
            "--node",
            &endpoint,
        ],
    )
    .await?;
    let result: disclosure::VerificationResult = serde_json::from_slice(&result)?;
    ensure!(
        result.fully_verified(),
        "recipient did not confirm acceptance"
    );
    let receipt = recipient.join("receipt.json");
    cli(
        &binary,
        &recipient,
        &[
            "disclosure",
            "import",
            proof_path.as_str(),
            "--node",
            &endpoint,
            "--output",
            receipt.as_str(),
        ],
    )
    .await?;
    ensure!(std::fs::read(&receipt)? == bytes, "import changed receipt");
    ensure!(
        !recipient.join("config.toml").exists() && !recipient.join("wallet.sqlite").exists(),
        "import created a spendable wallet"
    );
    if let Ok(path) = std::env::var("SHIELDD_DISCLOSURE_TEST_RECEIPT") {
        std::fs::write(path, bytes)?;
    }
    if let Ok(helper) = std::env::var("BANKD_DISCLOSURE_TEST_BIN") {
        let transaction_path = sender.join("accepted-transaction.bin");
        std::fs::write(&transaction_path, tx.encode_to_vec())?;
        let status = tokio::process::Command::new(helper)
            .args(["-test.run", "^TestAcceptedDisclosureWorkflow$", "-test.v"])
            .env("BANKD_TEST_DISCLOSURE", proof_path.as_str())
            .env("BANKD_TEST_NODE", &endpoint)
            .env("BANKD_TEST_TRANSACTION", transaction_path.as_str())
            .status()
            .await?;
        ensure!(status.success(), "Bankd audit workflow failed");
    }
    let _ = shutdown.send(());
    server.await??;
    Ok(())
}
