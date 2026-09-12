use anyhow::{ensure, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use shieldd_sdk_disclosure as sdk;
use shieldd_sdk_proto::core::app::v1 as pb;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    process::Stdio,
};
use tokio::io::AsyncWriteExt;

#[derive(Debug, clap::Subcommand)]
pub enum DisclosureCmd {
    /// Machine adapter protocol and supported package identity.
    Capabilities,
    /// Resolve canonical compliance ciphertext from a chosen node, without a wallet.
    AuditCiphertext {
        selection: Utf8PathBuf,
        #[clap(long)]
        node: String,
        /// Candidate canonical transaction bytes as a JSON array of base64 strings.
        #[clap(long)]
        transactions: Option<Utf8PathBuf>,
    },
    /// Decode a chosen-node ciphertext using a shared point already verified through Orbis.
    AuditDecode {
        request: Utf8PathBuf,
        #[clap(long)]
        node: String,
    },
    /// Reconstruct certificate bytes from a registration grant and chosen-node policy.
    AuditRegistration {
        request: Utf8PathBuf,
        #[clap(long)]
        node: String,
    },
    /// Create issuer evidence from private JSON stdin; never pass the issuer secret as an argument.
    IssuerCreate {
        #[clap(long)]
        node: String,
        #[clap(long)]
        output: Utf8PathBuf,
    },
    /// Verify issuer evidence against accepted ciphertext and registered issuer policy.
    IssuerVerify {
        package: Utf8PathBuf,
        #[clap(long)]
        node: String,
    },
    /// Validate a request without loading wallet or custody configuration.
    ValidateRequest { request: Utf8PathBuf },
    /// Export explicitly selected claims using a local background prover.
    Export {
        #[clap(long)]
        wallet: Utf8PathBuf,
        #[clap(long)]
        request: Utf8PathBuf,
        #[clap(long)]
        output: Utf8PathBuf,
        /// Give note decryption and transaction-wide memo access instead of a ZK proof.
        #[clap(long)]
        payload_keys: bool,
        /// Reveal all committed note fields and blinding without proving.
        #[clap(long, conflicts_with = "payload-keys")]
        openings: bool,
    },
    /// Describe unverified claims and decryption capabilities without loading a wallet.
    Inspect { package: Utf8PathBuf },
    /// Verify a bounded machine request from private stdin.
    VerifyMachine {
        #[clap(long)]
        node: String,
    },
    /// Verify cryptography and optionally acceptance using a chosen Bankd gRPC endpoint.
    Verify {
        package: Utf8PathBuf,
        #[clap(long)]
        node: Option<String>,
        #[clap(long)]
        request: Option<Utf8PathBuf>,
        /// Candidate canonical transaction bytes as a JSON array of base64 strings.
        #[clap(long)]
        transactions: Option<Utf8PathBuf>,
    },
    /// Validate and store a receipt; disclosed notes never become spendable wallet notes.
    Import {
        package: Utf8PathBuf,
        #[clap(long)]
        node: String,
        #[clap(long)]
        request: Option<Utf8PathBuf>,
        #[clap(long)]
        output: Utf8PathBuf,
    },
    #[clap(hide = true)]
    Worker {
        #[clap(long)]
        output: Utf8PathBuf,
    },
}

#[derive(serde::Serialize)]
struct SharedFields<'a> {
    reference: &'a sdk::OutputRef,
    amount: &'a Option<String>,
    asset: &'a Option<String>,
    recipient: &'a Option<String>,
    memo: &'a Option<sdk::DisclosedMemo>,
    predicate_result: Option<bool>,
}
#[derive(serde::Serialize)]
struct SharingPreview<'a> {
    outputs: Vec<SharedFields<'a>>,
    selected_output_total: &'a Option<sdk::DisclosedTotal>,
}
fn sharing_preview(statement: &sdk::DisclosureStatement) -> SharingPreview<'_> {
    SharingPreview {
        outputs: statement
            .outputs
            .iter()
            .map(|o| SharedFields {
                reference: &o.public.reference,
                amount: &o.amount,
                asset: &o.asset,
                recipient: &o.recipient,
                memo: &o.memo,
                predicate_result: o.predicate_result,
            })
            .collect(),
        selected_output_total: &statement.selected_output_total,
    }
}

fn report_rejection<T>(result: Result<T>) -> Result<T> {
    if result.is_err() {
        println!("{}", serde_json::json!({"status":"rejected"}));
    }
    result
}

fn read_bounded(path: &Utf8Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if path.as_str() == "-" {
        std::io::stdin()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
    } else {
        std::fs::File::open(path)?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
    }
    ensure!(bytes.len() <= limit, "file exceeds size limit");
    Ok(bytes)
}

fn publish(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_str().is_empty())
        .unwrap_or(Utf8Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}

fn check_request(package: &sdk::DisclosurePackage, expected: Option<&Utf8Path>) -> Result<()> {
    let request = expected
        .map(|path| -> Result<sdk::DisclosureRequest> {
            Ok(serde_json::from_slice(&read_bounded(
                path,
                sdk::MAX_DOCUMENT_BYTES,
            )?)?)
        })
        .transpose()?;
    check_expected(package, request.as_ref())
}

fn check_expected(
    package: &sdk::DisclosurePackage,
    expected: Option<&sdk::DisclosureRequest>,
) -> Result<()> {
    if let Some(request) = expected {
        sdk::validate_request(request)?;
        ensure!(
            &package.statement.request == request,
            "disclosure does not match the recipient's request"
        );
    } else {
        ensure!(
            package
                .statement
                .request
                .outputs
                .iter()
                .all(|c| !c.spending_control),
            "spending-control verification requires --request with the recipient's fresh challenge"
        );
    }
    Ok(())
}

async fn accepted_blocks(
    node: &str,
    heights: BTreeSet<u64>,
) -> Result<(String, Vec<sdk::AcceptedBlock>)> {
    let channel = tonic::transport::Endpoint::from_shared(node.to_owned())?
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .connect()
        .await?;
    let mut client =
        tonic::client::Grpc::new(channel).max_decoding_message_size(sdk::MAX_PACKAGE_BYTES);
    client.ready().await?;
    let response: tonic::Response<pb::AppParametersResponse> = client
        .unary(
            tonic::Request::new(pb::AppParametersRequest {}),
            tonic::codegen::http::uri::PathAndQuery::from_static(
                "/mizufinance.shieldd.v1.Query/AppParameters",
            ),
            tonic::codec::ProstCodec::default(),
        )
        .await?;
    let chain_id = response
        .into_inner()
        .app_parameters
        .context("node omitted chain parameters")?
        .chain_id;
    let mut blocks = Vec::new();
    for height in heights {
        client.ready().await?;
        let response: tonic::Response<pb::TransactionsByHeightResponse> = client
            .unary(
                tonic::Request::new(pb::TransactionsByHeightRequest {
                    block_height: height,
                }),
                tonic::codegen::http::uri::PathAndQuery::from_static(
                    "/mizufinance.shieldd.v1.Query/TransactionsByHeight",
                ),
                tonic::codec::ProstCodec::default(),
            )
            .await?;
        let response = response.into_inner();
        ensure!(
            response.block_height == height,
            "node returned wrong height"
        );
        blocks.push(sdk::AcceptedBlock {
            height,
            transactions: response
                .transactions
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>>>()?,
        });
    }
    Ok((chain_id, blocks))
}

#[derive(Debug)]
struct AcceptanceMismatch;
impl std::fmt::Display for AcceptanceMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("accepted transaction data mismatch")
    }
}
impl std::error::Error for AcceptanceMismatch {}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineVerification {
    version: u32,
    package: sdk::DisclosurePackage,
    request: Option<sdk::DisclosureRequest>,
    #[serde(default)]
    transactions: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum MachineStatus {
    Verified,
    Rejected,
    Unresolved,
    Unavailable,
}

#[derive(serde::Serialize)]
struct MachineResult {
    protocol: u32,
    package_version: u32,
    circuit: &'static str,
    development_artifacts: bool,
    status: MachineStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<sdk::VerificationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    statement: Option<sdk::DisclosureStatement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'static str>,
}

async fn verify_machine(input: &[u8], node: &str) -> MachineResult {
    let mut response = MachineResult {
        protocol: 2,
        package_version: sdk::VERSION,
        circuit: sdk::CIRCUIT_ID,
        development_artifacts: cfg!(all(
            feature = "development-disclosure-artifacts",
            debug_assertions
        )),
        status: MachineStatus::Rejected,
        result: None,
        statement: None,
        method: None,
    };
    let verified = (|| -> Result<(sdk::DisclosurePackage, sdk::VerificationResult)> {
        use base64::Engine;
        let input: MachineVerification = serde_json::from_slice(input)?;
        ensure!(input.version == 2, "unsupported machine protocol");
        let package = input.package;
        ensure!(
            package.version == sdk::VERSION,
            "unsupported disclosure version"
        );
        sdk::validate_request(&package.statement.request)?;
        check_expected(&package, input.request.as_ref())?;
        let candidates = input
            .transactions
            .iter()
            .map(|b| {
                let raw = base64::engine::general_purpose::STANDARD.decode(b)?;
                shieldd_sdk_transaction::Transaction::decode_canonical(&raw)
            })
            .collect::<Result<Vec<_>>>()?;
        if !candidates.is_empty() {
            sdk::verify_candidates(&package.statement, &candidates)?;
        }
        let result = sdk::verify(&package)?;
        Ok((package, result))
    })();
    let (package, mut result) = match verified {
        Ok(value) => value,
        Err(error) => {
            if error.is::<sdk::VerificationUnavailable>() {
                response.status = MachineStatus::Unavailable;
            }
            return response;
        }
    };
    response.status = match acceptance(&package, node).await {
        Ok(accepted) => {
            result.acceptance = accepted;
            MachineStatus::Verified
        }
        Err(error) if error.is::<AcceptanceMismatch>() => {
            result.acceptance = sdk::Acceptance::Rejected;
            MachineStatus::Rejected
        }
        Err(_) => MachineStatus::Unresolved,
    };
    response.result = Some(result);
    response.statement = Some(package.statement);
    response.method = Some(match package.evidence {
        sdk::Evidence::Groth16 { .. } => "Groth16",
        sdk::Evidence::Openings { .. } => "Openings",
        sdk::Evidence::PayloadKeys { .. } => "PayloadKeys",
    });
    response
}

async fn acceptance(package: &sdk::DisclosurePackage, node: &str) -> Result<sdk::Acceptance> {
    let heights = package
        .statement
        .request
        .outputs
        .iter()
        .map(|c| c.reference.height)
        .collect();
    let (chain_id, blocks) = accepted_blocks(node, heights).await?;
    sdk::confirm_acceptance(&package.statement, &chain_id, &blocks)
        .map_err(|_| AcceptanceMismatch.into())
}

async fn asset_policy(
    node: &str,
    asset: shieldd_sdk_asset::asset::Id,
) -> Result<shieldd_sdk_compliance::AssetPolicy> {
    use shieldd_sdk_proto::core::component::compliance::v1 as cpb;
    let channel = tonic::transport::Endpoint::from_shared(node.to_owned())?
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .connect()
        .await?;
    let mut client =
        tonic::client::Grpc::new(channel).max_decoding_message_size(sdk::MAX_DOCUMENT_BYTES);
    client.ready().await?;
    let response: tonic::Response<cpb::ComplianceAssetStatusResponse> = client
        .unary(
            tonic::Request::new(cpb::ComplianceAssetStatusRequest {
                asset_id: Some(asset.into()),
            }),
            tonic::codegen::http::uri::PathAndQuery::from_static(
                "/mizufinance.shieldd.v1.Query/ComplianceAssetStatus",
            ),
            tonic::codec::ProstCodec::default(),
        )
        .await?;
    let response = response.into_inner();
    ensure!(
        response.is_registered && response.is_regulated,
        "registered regulated asset unavailable"
    );
    let asset_id: shieldd_sdk_asset::asset::Id = response
        .asset_id
        .context("node omitted asset identity")?
        .try_into()?;
    ensure!(asset_id == asset, "node returned wrong asset identity");
    response
        .asset_policy
        .context("node omitted asset policy")?
        .try_into()
}

impl DisclosureCmd {
    pub async fn exec(&self, home: &Utf8Path) -> Result<()> {
        match self {
            Self::VerifyMachine { node } => {
                let input = zeroize::Zeroizing::new(read_bounded(
                    Utf8Path::new("-"),
                    sdk::MAX_PACKAGE_BYTES,
                )?);
                println!(
                    "{}",
                    serde_json::to_string(&verify_machine(&input, node).await)?
                );
            }
            Self::IssuerCreate { node, output } => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct PrivateRequest {
                    request: sdk::IssuerRequest,
                    issuer_secret: [u8; 32],
                }
                let bytes = zeroize::Zeroizing::new(read_bounded(
                    Utf8Path::new("-"),
                    sdk::MAX_DOCUMENT_BYTES,
                )?);
                let mut request: PrivateRequest = serde_json::from_slice(&bytes)?;
                let secret = zeroize::Zeroizing::new(request.issuer_secret);
                use zeroize::Zeroize;
                request.issuer_secret.zeroize();
                let mut key = shieldd_sdk_compliance::DetectionKey::new(
                    decaf377::Fr::from_bytes_checked(&secret)
                        .map_err(|_| anyhow::anyhow!("invalid issuer secret"))?,
                );
                let result = async {
                    let asset: shieldd_sdk_asset::asset::Id = request.request.asset.parse()?;
                    let policy = asset_policy(node, asset).await?;
                    ensure!(key.public_key() == policy.params.dk_pub, "issuer key does not match registered asset");
                    let (chain, blocks) = accepted_blocks(node, [request.request.selection.reference.height].into_iter().collect()).await?;
                    let block = blocks.first().context("accepted block unavailable")?;
                    let accepted = sdk::accepted_audit_ciphertext(request.request.selection.clone(), &chain, block)?;
                    eprintln!("Issuer evidence shares the selected value plus asset, flagged status, detection salt, and amount decryption access.");
                    let package = sdk::prepare_issuer_disclosure(rand_core::OsRng, &accepted, request.request, &key)?;
                    let preview = sdk::verify_issuer_disclosure(&package, &chain, block, asset, policy.params.dk_pub)?;
                    eprintln!("{}", serde_json::to_string_pretty(&preview)?);
                    publish(output, &serde_json::to_vec(&package)?)
                }.await;
                key.0.zeroize();
                result?;
            }
            Self::IssuerVerify { package, node } => {
                let parsed = serde_json::from_slice::<sdk::IssuerDisclosure>(&read_bounded(
                    package,
                    sdk::MAX_DOCUMENT_BYTES,
                )?);
                let package = match parsed {
                    Ok(package) if package.version == 1 => package,
                    _ => {
                        println!(r#"{{"status":"rejected"}}"#);
                        anyhow::bail!("invalid issuer disclosure");
                    }
                };
                let asset: shieldd_sdk_asset::asset::Id = match package.request.asset.parse() {
                    Ok(asset) => asset,
                    Err(_) => {
                        println!(r#"{{"status":"rejected"}}"#);
                        anyhow::bail!("invalid issuer asset");
                    }
                };
                let policy = asset_policy(node, asset).await?;
                let (chain, blocks) = accepted_blocks(
                    node,
                    [package.request.selection.reference.height]
                        .into_iter()
                        .collect(),
                )
                .await?;
                match sdk::verify_issuer_disclosure(
                    &package,
                    &chain,
                    blocks.first().context("accepted block unavailable")?,
                    asset,
                    policy.params.dk_pub,
                ) {
                    Ok(facts) => println!("{}", serde_json::to_string(&facts)?),
                    Err(error) => {
                        println!(r#"{{"status":"rejected"}}"#);
                        return Err(error);
                    }
                }
            }
            Self::AuditRegistration { request, node } => {
                let request: sdk::AuditRegistrationRequest =
                    serde_json::from_slice(&read_bounded(request, sdk::MAX_DOCUMENT_BYTES)?)?;
                let (chain, _) = accepted_blocks(node, BTreeSet::new()).await?;
                let policy = match &request.registration {
                    sdk::AuditRegistration::Person { action } => {
                        Some(asset_policy(node, action.leaf.asset_id).await?)
                    }
                    sdk::AuditRegistration::General { .. } => None,
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();
                let statement =
                    sdk::prepare_audit_registration(&request, &chain, now, policy.as_ref())?;
                println!("{}", serde_json::to_string(&statement)?);
            }
            Self::AuditDecode { request, node } => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct DecodeRequest {
                    selection: sdk::AuditSelection,
                    shared_point: [u8; 32],
                }
                let request: DecodeRequest =
                    serde_json::from_slice(&read_bounded(request, sdk::MAX_DOCUMENT_BYTES)?)?;
                let (chain, blocks) = accepted_blocks(
                    node,
                    [request.selection.reference.height].into_iter().collect(),
                )
                .await?;
                let accepted = sdk::accepted_audit_ciphertext(
                    request.selection,
                    &chain,
                    blocks.first().context("accepted block unavailable")?,
                )?;
                let decoded = sdk::decode_audit_ciphertext(&accepted, request.shared_point)?;
                println!("{}", serde_json::to_string(&decoded)?);
            }
            Self::AuditCiphertext {
                selection,
                node,
                transactions,
            } => {
                let bytes = read_bounded(selection, sdk::MAX_DOCUMENT_BYTES)?;
                let selection: sdk::AuditSelection = report_rejection((|| {
                    let selection: sdk::AuditSelection = serde_json::from_slice(&bytes)?;
                    ensure!(
                        selection.version == 2 && selection.reference.height > 0,
                        "invalid audit selection"
                    );
                    selection.access.key_scope()?;
                    Ok(selection)
                })())?;
                let (chain, blocks) =
                    accepted_blocks(node, [selection.reference.height].into_iter().collect())
                        .await?;
                if let Some(path) = transactions {
                    use base64::Engine;
                    let encoded: Vec<String> =
                        serde_json::from_slice(&read_bounded(path, sdk::MAX_PACKAGE_BYTES)?)?;
                    ensure!(encoded.len() == 1, "expected one indexed audit transaction");
                    let raw = base64::engine::general_purpose::STANDARD.decode(&encoded[0])?;
                    report_rejection(sdk::verify_audit_candidate(
                        &selection,
                        blocks.first().context("accepted block unavailable")?,
                        &raw,
                    ))?;
                }
                let accepted = report_rejection(sdk::accepted_audit_ciphertext(
                    selection,
                    &chain,
                    blocks.first().context("accepted block unavailable")?,
                ))?;
                println!("{}", serde_json::to_string(&accepted)?);
            }
            Self::ValidateRequest { request } => {
                let bytes = read_bounded(request, sdk::MAX_DOCUMENT_BYTES)?;
                let kind: serde_json::Value = serde_json::from_slice(&bytes)?;
                if kind.get("kind").and_then(|k| k.as_str()) == Some("issuer") {
                    let request: sdk::IssuerRequest = serde_json::from_slice(&bytes)?;
                    sdk::validate_issuer_request(&request)?;
                    println!("{}", serde_json::to_string(&request)?);
                } else {
                    let request: sdk::DisclosureRequest = serde_json::from_slice(&bytes)?;
                    sdk::validate_request(&request)?;
                    println!("{}", serde_json::to_string(&request)?);
                }
            }
            Self::Capabilities => println!(
                "{}",
                serde_json::json!({"protocol":1,"package_version":sdk::VERSION,"audit_ciphertext_version":2,"audit_registration_version":1,"issuer_disclosure_version":1,"circuit":sdk::CIRCUIT_ID,"development_artifacts":cfg!(all(feature="development-disclosure-artifacts",debug_assertions))})
            ),
            Self::Inspect { package } => {
                let package = sdk::decode_package(&read_bounded(package, sdk::MAX_PACKAGE_BYTES)?)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&sdk::inspect(&package)?)?
                );
            }
            Self::Verify {
                package,
                node,
                request,
                transactions,
            } => {
                let package = sdk::decode_package(&read_bounded(package, sdk::MAX_PACKAGE_BYTES)?)?;
                check_request(&package, request.as_deref())?;
                if let Some(path) = transactions {
                    use base64::Engine;
                    let encoded: Vec<String> =
                        serde_json::from_slice(&read_bounded(path, sdk::MAX_PACKAGE_BYTES)?)?;
                    let txs = encoded
                        .iter()
                        .map(|b| {
                            let raw = base64::engine::general_purpose::STANDARD.decode(b)?;
                            shieldd_sdk_transaction::Transaction::decode_canonical(&raw)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    sdk::verify_candidates(&package.statement, &txs)?;
                }
                let mut result = sdk::verify(&package)?;
                if let Some(node) = node {
                    match acceptance(&package, node).await {
                        Ok(acceptance) => result.acceptance = acceptance,
                        Err(error) => {
                            if error.is::<AcceptanceMismatch>() {
                                result.acceptance = sdk::Acceptance::Rejected;
                            }
                            println!("{}", serde_json::to_string_pretty(&result)?);
                            return Err(error).context(
                                "cryptography verified; node acceptance could not be confirmed",
                            );
                        }
                    }
                }
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            Self::Import {
                package,
                node,
                output,
                request,
            } => {
                let bytes = read_bounded(package, sdk::MAX_PACKAGE_BYTES)?;
                let package = sdk::decode_package(&bytes)?;
                check_request(&package, request.as_deref())?;
                let mut result = sdk::verify(&package)?;
                result.acceptance = acceptance(&package, node).await?;
                ensure!(result.fully_verified(), "receipt is not fully verified");
                publish(output, &bytes)?;
                println!("Imported verified disclosure receipt: {output}");
            }
            Self::Export {
                wallet,
                request,
                output,
                payload_keys,
                openings,
            } => {
                ensure!(!output.exists(), "output already exists");
                let request =
                    serde_json::from_slice(&read_bounded(request, sdk::MAX_DOCUMENT_BYTES)?)?;
                let storage = shieldd_sdk_view::Storage::load(wallet).await?;
                let mut witness = storage.prepare_disclosure(request).await?;
                eprintln!(
                    "Disclosure request: {}",
                    serde_json::to_string_pretty(&witness.request)?
                );
                if *payload_keys {
                    eprintln!("Shares decryption of every selected note (amount, asset, recipient and note seed) and the entire transaction memo, including its return address.");
                } else if *openings {
                    eprintln!("Shares all committed note fields and blinding, without note seeds or decryption keys.");
                } else {
                    eprintln!("Shares selected fields and explicit predicate results using a local Groth16 proof.");
                }
                if witness.request.outputs.iter().any(|c| c.spending_control) {
                    use crate::config::{CustodyConfig, PcliConfig};
                    let config = PcliConfig::load(home.join("config.toml"))?;
                    ensure!(
                        config.full_viewing_key == storage.full_viewing_key().await?,
                        "custody and wallet differ"
                    );
                    let mut signatures = BTreeMap::<[u8; 32], Vec<u8>>::new();
                    for (claim, output) in witness.request.outputs.iter().zip(&mut witness.outputs)
                    {
                        if !claim.spending_control {
                            continue;
                        }
                        let sdk::ActionRef::Body(action) = claim.reference.action else {
                            anyhow::bail!("control requires an ordinary Transfer")
                        };
                        let authority = storage
                            .disclosure_authority(&claim.reference.transaction_id, action)
                            .await?;
                        let key: [u8; 32] = output
                            .public
                            .spend_verification_key
                            .as_deref()
                            .context("not an ordinary Transfer")?
                            .try_into()?;
                        let signature = if let Some(signature) = signatures.get(&key) {
                            signature.clone()
                        } else {
                            let signature = match &config.custody {
                            CustodyConfig::SoftKms(c) => shieldd_sdk_custody::soft_kms::SoftKms::new(c.clone()).sign_disclosure(&witness.request, &output.public, authority.randomizer)?,
                            CustodyConfig::Encrypted(c) => c.clone().sign_disclosure(&rpassword::prompt_password("Custody password: ")?, &witness.request, &output.public, authority.randomizer)?,
                            _ => anyhow::bail!("disclosure signing unavailable for this custody; use an external signature through the SDK"),
                            };
                            signatures.insert(key, signature.clone());
                            signature
                        };
                        output.control_signature = Some(signature);
                    }
                }
                let statement = sdk::evaluate(&witness)?;
                eprintln!(
                    "Fields to share: {}",
                    serde_json::to_string_pretty(&sharing_preview(&statement))?
                );
                drop(storage);
                let package = if *payload_keys {
                    sdk::export_payload_keys(&witness)?
                } else if *openings {
                    sdk::export_openings(&witness)?
                } else {
                    run_worker(&witness, &std::env::current_exe()?).await?
                };
                sdk::verify(&package)?;
                publish(output, &serde_json::to_vec(&package)?)?;
                println!("Exported disclosure: {output}");
            }
            Self::Worker { output } => {
                #[cfg(feature = "disclosure-prover")]
                {
                    let mut bytes = Vec::new();
                    std::io::stdin()
                        .take(sdk::MAX_WITNESS_BYTES as u64 + 1)
                        .read_to_end(&mut bytes)?;
                    let package = sdk::prove(&sdk::decode_witness(&bytes)?)?;
                    let bytes = serde_json::to_vec(&package)?;
                    ensure!(bytes.len() <= sdk::MAX_PACKAGE_BYTES, "package too large");
                    std::fs::write(output, bytes)?;
                }
                #[cfg(not(feature = "disclosure-prover"))]
                {
                    let _ = output;
                    anyhow::bail!(
                        "build pcli with --features disclosure-prover to generate proofs"
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
struct WorkerGroup {
    pid: u32,
    active: bool,
}

#[cfg(unix)]
impl Drop for WorkerGroup {
    fn drop(&mut self) {
        if self.active {
            // This group was created for this job; it contains no unrelated processes.
            unsafe {
                libc::kill(-(self.pid as i32), libc::SIGKILL);
            }
        }
    }
}

async fn run_worker(
    witness: &sdk::DisclosureWitness,
    executable: &std::path::Path,
) -> Result<sdk::DisclosurePackage> {
    #[cfg(not(unix))]
    anyhow::bail!("local disclosure proving currently requires macOS or Linux");
    let input = serde_json::to_vec(witness)?;
    ensure!(input.len() <= sdk::MAX_WITNESS_BYTES, "witness too large");
    // A separate result file keeps inherited prover stdout out of the package channel.
    let result_file = tempfile::NamedTempFile::new()?;
    let result_path = Utf8Path::from_path(result_file.path()).context("non-UTF8 result path")?;
    let mut command = tokio::process::Command::new(executable);
    command
        .args(["disclosure", "worker", "--output"])
        .arg(result_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .env_remove("RUST_LOG")
        .kill_on_drop(true);
    // All descendants belong to this job, so cancellation also stops the native prover.
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn()?;
    #[cfg(unix)]
    let mut group = WorkerGroup {
        pid: child.id().context("worker pid unavailable")?,
        active: true,
    };
    let mut stdin = child.stdin.take().context("worker input unavailable")?;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    let work = async move {
        stdin.write_all(&input).await?;
        drop(stdin);
        ensure!(child.wait().await?.success(), "local prover failed");
        let bytes = read_bounded(result_path, sdk::MAX_PACKAGE_BYTES)?;
        sdk::decode_package(&bytes)
    };
    tokio::pin!(work);
    loop {
        tokio::select! {
            result = &mut work => {
                #[cfg(unix)]
                if result.is_ok() { group.active = false; }
                return result;
            },
            _ = interval.tick() => eprintln!("Local proof running; Ctrl-C cancels the job."),
            signal = tokio::signal::ctrl_c() => {
                signal?;
                anyhow::bail!("disclosure job cancelled; no package published");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[test]
    fn export_modes_parse_and_conflict() {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Args {
            #[clap(subcommand)]
            command: DisclosureCmd,
        }
        let base = [
            "pcli",
            "export",
            "--wallet",
            "wallet.sqlite",
            "--request",
            "request.json",
            "--output",
            "package.json",
        ];
        assert!(Args::try_parse_from(base).is_ok());
        for mode in ["--openings", "--payload-keys"] {
            assert!(Args::try_parse_from(base.into_iter().chain([mode])).is_ok());
        }
        let error = Args::try_parse_from(base.into_iter().chain(["--openings", "--payload-keys"]))
            .err()
            .expect("incompatible exports accepted");
        assert_eq!(error.kind(), clap::ErrorKind::ArgumentConflict);
    }

    use super::*;

    #[test]
    fn publication_is_atomic_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("receipt.json")).unwrap();
        publish(&path, b"verified receipt").unwrap();
        assert!(publish(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"verified receipt");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prover_stdout_does_not_corrupt_worker_result() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("worker");
        let request = sdk::DisclosureRequest {
            version: sdk::VERSION,
            chain_id: "test".into(),
            recipient: None,
            challenge: None,
            total: None,
            outputs: vec![sdk::OutputClaim {
                reference: sdk::OutputRef {
                    transaction_id: "ab".repeat(32),
                    height: 1,
                    action: sdk::ActionRef::Body(0),
                    output: 0,
                },
                amount: true,
                asset: false,
                recipient: false,
                predicate: None,
                memo: false,
                spending_control: false,
            }],
        };
        let package = sdk::DisclosurePackage {
            version: sdk::VERSION,
            statement: sdk::DisclosureStatement {
                request: request.clone(),
                outputs: vec![],
                selected_output_total: None,
            },
            evidence: sdk::Evidence::Openings {
                openings: vec![],
                control_signatures: vec![],
            },
        };
        std::fs::write(
            &executable,
            b"#!/bin/sh\ncat >/dev/null\nprintf 'prover diagnostic\\n'\ncp \"$0.package\" \"$4\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("worker.package"),
            serde_json::to_vec(&package).unwrap(),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = run_worker(
            &sdk::DisclosureWitness {
                request,
                outputs: vec![],
            },
            &executable,
        )
        .await
        .unwrap();
        assert_eq!(result.statement, package.statement);
        // Successful transport never makes a worker's unverified evidence acceptable.
        assert!(sdk::verify(&result).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_suppresses_dumps_and_cancels_process_group() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("worker");
        std::fs::write(
            &executable,
            b"#!/bin/sh\n[ -z \"${RUST_LOG+x}\" ] || exit 1\nprintf '%s' $$ > \"$0.pid\"\nsleep 30 &\nwait\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let witness = sdk::DisclosureWitness {
            request: sdk::DisclosureRequest {
                version: 1,
                chain_id: "test".into(),
                recipient: None,
                challenge: None,
                outputs: vec![],
                total: None,
            },
            outputs: vec![],
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            run_worker(&witness, &executable),
        )
        .await;
        assert!(result.is_err());
        let pid: i32 = std::fs::read_to_string(dir.path().join("worker.pid"))
            .unwrap()
            .parse()
            .unwrap();
        for _ in 0..50 {
            if unsafe { libc::kill(-pid, 0) } != 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("cancelled worker group is still alive");
    }
}
