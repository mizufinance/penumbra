use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_compliance::{decrypt_full_flagged, TransferComplianceCiphertext};
use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::core::app::v1::{
    query_service_client::QueryServiceClient as AppQueryServiceClient, TransactionsByHeightRequest,
};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetStatusRequest,
};
use penumbra_sdk_proto::{DomainType, Message};
use penumbra_sdk_transaction::{ActionPlan, Transaction, TransactionPlan};
use penumbra_sdk_view::{NoteManager, TransferPlanningResult};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tonic::transport::Channel;
use tracing::info;
use url::Url;

use super::FeeTier;

/// A detected transaction reference from the scan command.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedTxRef {
    /// Block height where the transaction was found.
    pub height: u64,
    /// Transaction hash (hex-encoded).
    pub tx_hash: String,
    /// Index of the action within the transaction.
    pub action_index: usize,
    /// Asset ID detected in the transfer (bech32 format).
    pub asset_id: String,
    /// Whether the transfer is flagged (threshold exceeded).
    pub is_flagged: bool,
}

/// Scan output format for JSON serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanOutput {
    /// Information about the scan operation.
    pub scan_info: ScanInfo,
    /// List of detected transactions.
    pub detected: Vec<DetectedTxRef>,
}

/// Metadata about a scan operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanInfo {
    /// Type of key used for scanning.
    pub key_type: String,
    /// Starting block height.
    pub start_height: u64,
    /// Ending block height.
    pub end_height: u64,
    /// Timestamp when scan was performed.
    pub scan_time: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScannerState {
    pub last_height: u64,
}

/// Compliance-related transaction commands.
#[derive(Debug, clap::Subcommand)]
pub enum ComplianceCmd {
    /// Register an asset's regulation status in the compliance registry.
    ///
    /// This marks whether an asset requires compliance (regulated) or not (unregulated).
    /// Once registered, an asset's status cannot be changed in this demo version.
    RegisterAsset {
        /// The asset ID to register (e.g., "uusdc" or a full asset ID).
        asset_id: String,
        /// Mark this asset as regulated (requires compliance ciphertexts).
        #[clap(long)]
        regulated: bool,
        /// Mark this asset as unregulated (no compliance required).
        #[clap(long, conflicts_with = "regulated")]
        unregulated: bool,
        /// Issuer's detection key public (hex, 64 chars = 32 bytes).
        /// When set with --threshold, enables issuer-side flagged transfer decryption.
        #[clap(long)]
        dk_pub_hex: Option<String>,
        /// Amount threshold for flagging, in BASE units (u128).
        /// Transfers at or above this amount are encrypted to the issuer's DK.
        /// For an asset with exponent 6 (like USDC), 500 display units = 500_000_000 base units.
        /// For an asset with exponent 18, 500 display units = 500_000_000_000_000_000_000 base units.
        #[clap(long)]
        threshold: Option<u128>,
        /// Orbis ring public key (hex, 64 chars = 32 bytes compressed).
        /// In production, this comes from the Orbis DKG ceremony.
        #[clap(long)]
        ring_pk_hex: Option<String>,
        /// Orbis ring identifier.
        #[clap(long, default_value = "")]
        ring_id: String,
        /// Orbis policy identifier used for PRE authorization.
        #[clap(long, default_value = "")]
        policy_id: String,
        /// Orbis permission name used for PRE authorization.
        #[clap(long, default_value = "")]
        permission: String,
        /// Orbis resource name used for PRE authorization.
        #[clap(long, default_value = "")]
        resource: String,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },

    /// Register your wallet's compliance key for a regulated asset.
    ///
    /// This allows you to transact with regulated assets by publishing your
    /// compliance viewing key on-chain.
    RegisterUser {
        /// The asset ID to register for (e.g., "uusdc").
        asset_id: String,
        /// Penumbra address to register. If omitted, derives the address from
        /// this wallet using --address-index.
        #[clap(long)]
        address: Option<String>,
        /// Address index to register (default: 0).
        /// Each address has a different ACK for privacy. When --address is
        /// provided, this index is only used as the fee funding source.
        #[clap(long, default_value = "0")]
        address_index: u32,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },

    /// Scan the chain for regulated asset transfers (detection-only).
    ///
    /// This command performs detection-only scanning, identifying which transactions
    /// contain compliance ciphertexts that can be decrypted with the provided key.
    /// It outputs a list of transaction references that can be passed to the
    /// `decrypt` command for full decryption. Provide the issuer's DK (detection key).
    Scan {
        /// The URL of the pd gRPC endpoint (e.g., http://localhost:8080).
        #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
        node: Url,

        /// Start scanning from this block height.
        #[clap(long, default_value = "1")]
        start_height: u64,

        /// Stop scanning at this block height (default: latest).
        #[clap(long)]
        end_height: Option<u64>,

        /// Issuer's detection key (64 hex chars = 32 bytes).
        /// Use for scanning all transfers of a threshold asset.
        #[clap(long)]
        dk_hex: Option<String>,

        /// The asset ID this DK corresponds to (required when using --dk-hex).
        /// The detection key is per-asset, so the scanner needs to know which asset.
        #[clap(long)]
        scan_asset_id: Option<String>,

        /// Output file for detected TX list (JSON format).
        #[clap(long, default_value = "/tmp/detected_txs.json")]
        output: PathBuf,

        /// Keep scanning as new blocks are produced.
        #[clap(long)]
        follow: bool,

        /// Scanner resume state file. Stores the last successfully scanned height.
        #[clap(long)]
        state_file: Option<PathBuf>,

        /// Poll interval for --follow mode.
        #[clap(long, default_value = "2000")]
        poll_interval_ms: u64,

        /// Import detected transfers into the issuer ledger after each scan.
        #[clap(long)]
        issuer_db: Option<PathBuf>,

        /// Merge newly detected transfers into the existing output file.
        #[clap(long)]
        merge_output: bool,
    },

    /// Decrypt previously detected transactions.
    ///
    /// Takes a list of transaction references (from the scan command) and decrypts
    /// them using the issuer's DK for flagged transfers.
    Decrypt {
        /// Path to detected TX list from scan command (JSON format).
        #[clap(long)]
        input: PathBuf,

        /// The URL of the pd gRPC endpoint.
        #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
        node: Url,

        /// Issuer's detection key for flagged transfers (64 hex chars = 32 bytes).
        #[clap(long)]
        dk_hex: Option<String>,
    },

    /// Generate a new issuer detection key pair (DK).
    ///
    /// This command generates a random detection key for asset issuers.
    /// The private DK (dk) is used by the issuer to decrypt flagged transfers.
    /// The public DK (dk_pub) is registered with the asset using --dk-pub-hex.
    ///
    /// Example workflow:
    /// 1. Issuer runs: pcli tx compliance generate-dk
    /// 2. Issuer registers asset: pcli tx compliance register-asset USDC --regulated --dk-pub-hex <DK_PUB> --threshold 10000
    /// 3. Issuer can later scan flagged transfers using their private DK
    GenerateDk,

    /// Issuer surveillance ledger management.
    ///
    /// Manage a SQLite database that tracks detected compliance transfers and their
    /// progressive decryption state. Flagged transfers are decrypted immediately on
    /// import; non-flagged transfers require Orbis PRE audit to decrypt.
    #[clap(subcommand)]
    IssuerDb(IssuerDbCmd),
}

/// Issuer database management subcommands.
#[derive(Debug, clap::Subcommand)]
pub enum IssuerDbCmd {
    /// Initialize a new issuer ledger database.
    Init {
        /// Path to the SQLite database file.
        #[clap(long, default_value = "/tmp/issuer-ledger.db")]
        db: PathBuf,
    },

    /// Import detected transactions from scan output into the ledger.
    ///
    /// Inserts each detected transfer as a row. Flagged transfers are automatically
    /// decrypted using the provided DK and node connection.
    Import {
        /// Path to the SQLite database file.
        #[clap(long, default_value = "/tmp/issuer-ledger.db")]
        db: PathBuf,

        /// Path to scan output JSON (from `pcli tx compliance scan`).
        #[clap(long)]
        scan_output: PathBuf,

        /// Issuer's detection key (64 hex chars = 32 bytes) for decrypting flagged transfers.
        #[clap(long)]
        dk_hex: String,

        /// The URL of the pd gRPC endpoint (for fetching flagged tx ciphertexts).
        #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
        node: Url,
    },

    /// Display the issuer ledger as a table.
    Show {
        /// Path to the SQLite database file.
        #[clap(long, default_value = "/tmp/issuer-ledger.db")]
        db: PathBuf,
        /// Emit structured JSON instead of a human-readable table.
        #[clap(long)]
        json: bool,
    },

    /// Update ledger rows with decrypted data from an Orbis PRE audit.
    Update {
        /// Path to the SQLite database file.
        #[clap(long, default_value = "/tmp/issuer-ledger.db")]
        db: PathBuf,

        /// Path to audit output JSON from the compliance audit pipeline.
        #[clap(long)]
        audit_output: PathBuf,

        /// Name of the audited user (prefixed to Via column, e.g. "Alice core").
        #[clap(long)]
        audit_subject: Option<String>,
    },

    /// Register an address alias (maps a Penumbra address to a human-readable name).
    Alias {
        /// Path to the SQLite database file.
        #[clap(long, default_value = "/tmp/issuer-ledger.db")]
        db: PathBuf,

        /// The Penumbra bech32 address to alias.
        #[clap(long)]
        address: String,

        /// Human-readable name for this address.
        #[clap(long)]
        name: String,
    },
}

impl ComplianceCmd {
    /// Determine if this command requires a network sync before executing.
    pub fn offline(&self) -> bool {
        match self {
            ComplianceCmd::RegisterAsset { .. } => false,
            ComplianceCmd::RegisterUser { .. } => false,
            ComplianceCmd::Scan { .. } => true,
            ComplianceCmd::Decrypt { .. } => true,
            ComplianceCmd::GenerateDk => true,
            ComplianceCmd::IssuerDb(_) => true,
        }
    }

    /// Check if this command is a scan command (doesn't create a transaction).
    pub fn is_scan(&self) -> bool {
        matches!(self, ComplianceCmd::Scan { .. })
    }

    /// Check if this command is a decrypt command (doesn't create a transaction).
    pub fn is_decrypt(&self) -> bool {
        matches!(self, ComplianceCmd::Decrypt { .. })
    }

    /// Check if this command is a generate-dk command (doesn't create a transaction).
    pub fn is_generate_dk(&self) -> bool {
        matches!(self, ComplianceCmd::GenerateDk)
    }

    /// Check if this command is an issuer-db command (doesn't create a transaction).
    pub fn is_issuer_db(&self) -> bool {
        matches!(self, ComplianceCmd::IssuerDb(_))
    }

    /// Execute the generate-dk command (pure computation, no network).
    pub fn exec_generate_dk(&self) -> Result<()> {
        match self {
            ComplianceCmd::GenerateDk => {
                use rand_core::OsRng;

                // Generate a random scalar for the detection key
                let dk = decaf377::Fr::rand(&mut OsRng);
                let dk_pub = decaf377::Element::GENERATOR * dk;

                // Serialize to hex
                let dk_hex = hex::encode(dk.to_bytes());
                let dk_pub_hex = hex::encode(dk_pub.vartime_compress().0);

                println!("=== Issuer Detection Key Generation ===");
                println!();
                println!("Private key (keep secret, use for scanning flagged transfers):");
                println!("  DK (hex): {}", dk_hex);
                println!();
                println!("Public key (use when registering asset):");
                println!("  DK_pub (hex): {}", dk_pub_hex);
                println!();
                println!("To register an asset with threshold flagging:");
                println!(
                    "  pcli tx compliance register-asset <ASSET> --regulated --dk-pub-hex {} --threshold <AMOUNT>",
                    dk_pub_hex
                );

                Ok(())
            }
            _ => anyhow::bail!("exec_generate_dk called on wrong command"),
        }
    }

    /// Execute the scan command directly (detection-only, doesn't create a transaction).
    /// This command doesn't require wallet initialization - only a gRPC connection.
    pub async fn exec_scan(&self) -> Result<()> {
        match self {
            ComplianceCmd::Scan {
                node,
                start_height,
                end_height,
                dk_hex,
                scan_asset_id,
                output,
                follow,
                state_file,
                poll_interval_ms,
                issuer_db,
                merge_output,
            } => {
                // Determine key type and parse key
                let (key_type_str, issuer_dk) = if let Some(hex) = dk_hex {
                    let dk = parse_dk_from_hex(hex)?;
                    ("issuer_dk".to_string(), Some(dk))
                } else {
                    anyhow::bail!("Must provide --dk-hex (issuer detection key)");
                };

                // Parse scan_asset_id (required when using --dk-hex)
                let expected_asset_id: Option<asset::Id> = if let Some(ref id_str) = scan_asset_id {
                    Some(Self::parse_asset_id(id_str)?)
                } else {
                    None
                };
                if issuer_dk.is_some() && expected_asset_id.is_none() {
                    anyhow::bail!(
                        "--scan-asset-id is required when using --dk-hex (DK is per-asset)"
                    );
                }

                // Connect to node directly (no wallet required)
                let channel = connect_to_node(node).await?;
                loop {
                    let end = if let Some(h) = end_height {
                        *h
                    } else {
                        get_latest_height(channel.clone()).await?
                    };
                    let effective_start = state_file
                        .as_ref()
                        .and_then(|path| File::open(path).ok())
                        .and_then(|file| serde_json::from_reader::<_, ScannerState>(file).ok())
                        .map(|state| state.last_height.saturating_add(1).max(*start_height))
                        .unwrap_or(*start_height);

                    if effective_start > end {
                        if !*follow {
                            println!("No new blocks to scan.");
                            break;
                        }
                        sleep(Duration::from_millis(*poll_interval_ms)).await;
                        continue;
                    }

                    eprintln!("Scanning blocks {} to {} ...", effective_start, end);

                    let mut detected_txs: Vec<DetectedTxRef> = Vec::new();
                    let mut total_outputs = 0u64;
                    let mut total_detected = 0u64;
                    let mut total_flagged = 0u64;

                    // Scan each block
                    for height in effective_start..=end {
                        // Fetch transactions for this block
                        let transactions = fetch_transactions(channel.clone(), height).await?;

                        for (tx_idx, tx) in transactions.iter().enumerate() {
                            let tx_hash = Transaction::decode(tx.encode_to_vec().as_slice())
                                .with_context(|| {
                                    format!(
                                        "failed to decode transaction at height {} index {}",
                                        height, tx_idx
                                    )
                                })?
                                .id()
                                .to_string();

                            if let Some(ref body) = tx.body {
                                for (action_idx, action) in body.actions.iter().enumerate() {
                                    // Count all output actions
                                    if let Some(
                                    penumbra_sdk_proto::core::transaction::v1::action::Action::Transfer(
                                        transfer,
                                    ),
                                ) = action.action.as_ref()
                                {
                                    if let Some(body) = transfer.body.as_ref() {
                                        total_outputs += body.outputs.len() as u64;
                                    }
                                }

                                    if let Some(ciphertext) = extract_compliance_ciphertext(action)
                                    {
                                        let detection_result = if let Some(ref dk) = issuer_dk {
                                            use penumbra_sdk_compliance::issuer_keys::DetectionKey;
                                            let dk_obj = DetectionKey::new(*dk);
                                            let expected = expected_asset_id.as_ref().unwrap();
                                            match dk_obj.try_decrypt_detection(
                                                &ciphertext.sender_core_epk,
                                                &ciphertext.sender_core_epk,
                                                &ciphertext.detection_tag,
                                                expected,
                                            ) {
                                                Ok((asset_id, is_flagged, _salt)) => {
                                                    Some((asset_id, is_flagged))
                                                }
                                                Err(_) => None,
                                            }
                                        } else {
                                            None
                                        };

                                        if let Some((asset_id, is_flagged)) = detection_result {
                                            total_detected += 1;
                                            if is_flagged {
                                                total_flagged += 1;
                                            }
                                            detected_txs.push(DetectedTxRef {
                                                height,
                                                tx_hash: tx_hash.clone(),
                                                action_index: action_idx,
                                                asset_id: asset_id.to_string(),
                                                is_flagged,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        // Progress indicator every 100 blocks
                        if height % 100 == 0 {
                            info!(height, "scanning progress...");
                        }
                    }

                    eprintln!(
                        "Scanned {} outputs, {} detected.",
                        total_outputs, total_detected
                    );

                    // Build output
                    let scan_output = ScanOutput {
                        scan_info: ScanInfo {
                            key_type: key_type_str.clone(),
                            start_height: effective_start,
                            end_height: end,
                            scan_time: {
                                use std::time::{SystemTime, UNIX_EPOCH};
                                let duration = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default();
                                format!("{}s", duration.as_secs())
                            },
                        },
                        detected: detected_txs.clone(),
                    };

                    let scan_output = if *merge_output {
                        merge_scan_output(output, scan_output)?
                    } else {
                        scan_output
                    };

                    // Write to file
                    let json = serde_json::to_string_pretty(&scan_output)?;
                    let mut file = File::create(output)?;
                    file.write_all(json.as_bytes())?;

                    if let Some(path) = state_file {
                        let state = ScannerState { last_height: end };
                        let json = serde_json::to_string_pretty(&state)?;
                        let mut file = File::create(path)?;
                        file.write_all(json.as_bytes())?;
                    }

                    if let Some(db) = issuer_db {
                        IssuerDbCmd::Import {
                            db: db.clone(),
                            scan_output: output.clone(),
                            dk_hex: dk_hex.clone().expect("validated above"),
                            node: node.clone(),
                        }
                        .exec()
                        .await?;
                    }

                    let non_flagged = total_detected - total_flagged;
                    println!(
                        "\nDetected {} transfers ({} flagged, {} normal).",
                        detected_txs.len(),
                        total_flagged,
                        non_flagged
                    );
                    println!("Results saved to: {}", output.display());

                    if !*follow {
                        break;
                    }
                    sleep(Duration::from_millis(*poll_interval_ms)).await;
                }

                Ok(())
            }
            _ => anyhow::bail!("exec_scan called on non-scan command"),
        }
    }

    /// Execute the decrypt command directly (doesn't create a transaction).
    pub async fn exec_decrypt(&self) -> Result<()> {
        match self {
            ComplianceCmd::Decrypt {
                input,
                node,
                dk_hex,
            } => {
                // Load detected transactions from file
                let file = File::open(input).context("Failed to open input file")?;
                let reader = BufReader::new(file);
                let scan_output: ScanOutput =
                    serde_json::from_reader(reader).context("Failed to parse scan output JSON")?;

                println!(
                    "Loaded {} detected transactions from {}",
                    scan_output.detected.len(),
                    input.display()
                );

                // Determine key type and parse key
                let (key_type, issuer_dk) = if let Some(hex) = dk_hex {
                    let dk = parse_dk_from_hex(hex)?;
                    ("issuer_dk", Some(dk))
                } else {
                    anyhow::bail!("Must provide --dk-hex (issuer detection key)");
                };

                println!("Decrypting with key type: {}", key_type);

                // Connect to node
                let channel = connect_to_node(node).await?;

                let mut decrypted_count = 0u64;

                for tx_ref in &scan_output.detected {
                    // For issuer decryption, skip non-flagged transfers
                    if issuer_dk.is_some() && !tx_ref.is_flagged {
                        println!(
                            "Skipping non-flagged TX at height {} action {} (encrypted to user)",
                            tx_ref.height, tx_ref.action_index
                        );
                        continue;
                    }

                    // Fetch the transaction
                    let transactions = fetch_transactions(channel.clone(), tx_ref.height).await?;

                    // Find the specific transaction
                    for tx in &transactions {
                        if let Some(ref body) = tx.body {
                            if tx_ref.action_index < body.actions.len() {
                                let action = &body.actions[tx_ref.action_index];
                                if let Some(ciphertext) = extract_compliance_ciphertext(action) {
                                    // Parse asset_id from the scan output
                                    let asset_id: asset::Id = tx_ref
                                        .asset_id
                                        .parse()
                                        .context("invalid asset_id in scan output")?;

                                    // Decrypt based on key type
                                    let result = if let Some(ref dk) = issuer_dk {
                                        decrypt_full_flagged(dk, &ciphertext, asset_id)
                                    } else {
                                        Ok(None)
                                    };

                                    match result {
                                        Ok(Some(data)) => {
                                            decrypted_count += 1;
                                            println!("─────────────────────────────────────────────────────────");
                                            println!(
                                                "📋 Decrypted Transfer at height {}",
                                                tx_ref.height
                                            );
                                            println!("   Action index: {}", tx_ref.action_index);
                                            println!("   Asset: {}", data.asset_id);
                                            println!("   Amount: {}", data.amount);
                                            println!("   Flagged: {}", tx_ref.is_flagged);
                                        }
                                        Ok(None) => {
                                            println!(
                                                "Failed to decrypt TX at height {} action {} (wrong key?)",
                                                tx_ref.height, tx_ref.action_index
                                            );
                                        }
                                        Err(e) => {
                                            println!(
                                                "Error decrypting TX at height {} action {}: {}",
                                                tx_ref.height, tx_ref.action_index, e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                println!(
                    "\nDecryption complete. Successfully decrypted {} transfers.",
                    decrypted_count
                );

                Ok(())
            }
            _ => anyhow::bail!("exec_decrypt called on non-decrypt command"),
        }
    }

    /// Execute an issuer-db subcommand.
    pub async fn exec_issuer_db(&self) -> Result<()> {
        match self {
            ComplianceCmd::IssuerDb(cmd) => cmd.exec().await,
            _ => anyhow::bail!("exec_issuer_db called on non-issuer-db command"),
        }
    }

    /// Create the transaction plan for this compliance command.
    pub async fn plan(
        &self,
        app: &mut crate::App,
        gas_prices: penumbra_sdk_fee::GasPrices,
    ) -> Result<TransactionPlan> {
        match self {
            ComplianceCmd::RegisterAsset {
                asset_id,
                regulated,
                unregulated,
                dk_pub_hex,
                threshold,
                ring_pk_hex,
                ring_id,
                policy_id,
                permission,
                resource,
                fee_tier,
            } => {
                // Determine regulation status
                let is_regulated = if *regulated {
                    true
                } else if *unregulated {
                    false
                } else {
                    anyhow::bail!("Must specify either --regulated or --unregulated");
                };

                // Parse asset ID
                let asset_id = Self::parse_asset_id(asset_id)?;

                // Parse dk_pub - REQUIRED for regulated assets
                let dk_pub = if let Some(hex_str) = dk_pub_hex {
                    let bytes =
                        hex::decode(hex_str).context("invalid dk_pub_hex: must be valid hex")?;
                    if bytes.len() != 32 {
                        anyhow::bail!("dk_pub_hex must be exactly 64 hex chars (32 bytes)");
                    }
                    let arr: [u8; 32] = bytes.try_into().unwrap();
                    Some(
                        decaf377::Encoding(arr)
                            .vartime_decompress()
                            .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?,
                    )
                } else if is_regulated {
                    anyhow::bail!(
                        "--dk-pub-hex is required for regulated assets. \
                        Generate one with: pcli tx compliance generate-dk"
                    );
                } else {
                    None
                };

                // Parse ring_pk (from Orbis DKG)
                let ring_pk = if let Some(hex_str) = ring_pk_hex {
                    let bytes =
                        hex::decode(hex_str).context("invalid ring_pk_hex: must be valid hex")?;
                    if bytes.len() != 32 {
                        anyhow::bail!("ring_pk_hex must be exactly 64 hex chars (32 bytes)");
                    }
                    let arr: [u8; 32] = bytes.try_into().unwrap();
                    Some(
                        decaf377::Encoding(arr)
                            .vartime_decompress()
                            .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?,
                    )
                } else {
                    None
                };

                // Create the registration message
                let msg = MsgRegisterAsset {
                    asset_id,
                    is_regulated,
                    dk_pub,
                    threshold: *threshold,
                    allowed_channels: vec![],
                    ring_pk,
                    ring_id: ring_id.clone(),
                    policy_id: policy_id.clone(),
                    permission: permission.clone(),
                    resource: resource.clone(),
                };

                // Build transaction plan
                let mut note_manager = NoteManager::new(rand_core::OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());

                match note_manager
                    .plan_actions_with_transfer_funding(
                        app.view(),
                        penumbra_sdk_keys::keys::AddressIndex::new(0),
                        vec![ActionPlan::from(msg)],
                    )
                    .await
                    .context("can't build transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => Ok(transaction_plan),
                    TransferPlanningResult::NeedsMaintenance {
                        maintenance_plan, ..
                    } => {
                        anyhow::bail!(
                            "compliance registration requires note maintenance first: {:?}",
                            maintenance_plan
                        );
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!("insufficient balance for compliance registration fees");
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }

            ComplianceCmd::RegisterUser {
                asset_id,
                address,
                address_index,
                fee_tier,
            } => {
                // Parse asset ID
                let asset_id = Self::parse_asset_id(asset_id)?;

                // Get user's full viewing key
                let fvk = app.config.full_viewing_key.clone();

                // Get address at specified index, or register an explicitly
                // supplied address while using the selected index for funding.
                let address_index = penumbra_sdk_keys::keys::AddressIndex::new(*address_index);
                let address = match address {
                    Some(address) => address.parse().context("invalid Penumbra address")?,
                    None => {
                        let (address, _detection_key) = fvk.payment_address(address_index);
                        address
                    }
                };

                // Create compliance leaf for this address and asset
                use penumbra_sdk_compliance::{derive_compliance_scalar, ComplianceLeaf};
                let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                let d = derive_compliance_scalar(b_d_fq);
                let leaf = ComplianceLeaf::new(address, asset_id, d);

                // Create registration message (signature is empty for now - filled during tx build)
                let msg = MsgRegisterUser {
                    leaf,
                    signature: vec![],
                };

                // Build transaction plan
                let mut note_manager = NoteManager::new(rand_core::OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());

                match note_manager
                    .plan_actions_with_transfer_funding(
                        app.view(),
                        address_index,
                        vec![ActionPlan::from(msg)],
                    )
                    .await
                    .context("can't build transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => Ok(transaction_plan),
                    TransferPlanningResult::NeedsMaintenance {
                        maintenance_plan, ..
                    } => {
                        anyhow::bail!(
                            "compliance registration requires note maintenance first: {:?}",
                            maintenance_plan
                        );
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!("insufficient balance for compliance registration fees");
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }

            ComplianceCmd::Scan { .. } => {
                anyhow::bail!("Scan command doesn't create a transaction - use exec_scan instead")
            }

            ComplianceCmd::GenerateDk => {
                anyhow::bail!("GenerateDk command doesn't create a transaction - use exec_generate_dk instead")
            }

            ComplianceCmd::Decrypt { .. } => {
                anyhow::bail!(
                    "Decrypt command doesn't create a transaction - use exec_decrypt instead"
                )
            }

            ComplianceCmd::IssuerDb(_) => {
                anyhow::bail!(
                    "IssuerDb command doesn't create a transaction - use exec_issuer_db instead"
                )
            }
        }
    }

    /// Helper to parse asset ID from string.
    /// Accepts either a full asset ID or a unit name like "penumbra" or "upenumbra".
    fn parse_asset_id(asset_str: &str) -> Result<asset::Id> {
        // Try to parse as a full asset ID first
        if let Ok(asset_id) = asset_str.parse() {
            return Ok(asset_id);
        }
        // Fall back to parsing as a unit name from the registry
        Ok(asset::REGISTRY.parse_unit(asset_str).id())
    }
}

/// Parse issuer Detection Key (DK) from hex string (32 bytes).
fn parse_dk_from_hex(hex: &str) -> Result<decaf377::Fr> {
    let bytes = hex::decode(hex).context("invalid hex string for DK")?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "DK must be exactly 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(decaf377::Fr::from_le_bytes_mod_order(&arr))
}

/// Extract transfer compliance ciphertext from an action.
fn extract_compliance_ciphertext(
    action: &penumbra_sdk_proto::core::transaction::v1::Action,
) -> Option<TransferComplianceCiphertext> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action as ActionEnum;

    let action_inner = action.action.as_ref()?;

    let cc_bytes = match action_inner {
        ActionEnum::Transfer(transfer) => {
            let body = transfer.body.as_ref()?;
            let output = body
                .outputs
                .iter()
                .find(|output| !output.compliance_ciphertext.is_empty())?;
            &output.compliance_ciphertext
        }
        _ => return None,
    };

    if cc_bytes.is_empty() {
        return None;
    }

    match TransferComplianceCiphertext::from_bytes(cc_bytes) {
        Ok(ct) => Some(ct),
        Err(e) => {
            eprintln!(
                "scan: ciphertext deserialization failed ({} bytes): {}",
                cc_bytes.len(),
                e
            );
            None
        }
    }
}

/// Connect to a Penumbra node directly (no wallet required).
async fn connect_to_node(node_url: &Url) -> Result<Channel> {
    let endpoint = tonic::transport::Endpoint::from_shared(node_url.to_string())
        .context("invalid node URL")?
        .timeout(std::time::Duration::from_secs(30));

    endpoint
        .connect()
        .await
        .context(format!("failed to connect to node at {}", node_url))
}

/// Get the latest block height from the chain using TendermintProxy.
async fn get_latest_height(channel: Channel) -> Result<u64> {
    let mut client = TendermintProxyServiceClient::new(channel);

    let response = client
        .get_status(GetStatusRequest {})
        .await
        .context("failed to query node status")?;

    let sync_info = response
        .into_inner()
        .sync_info
        .ok_or_else(|| anyhow::anyhow!("missing sync_info in status response"))?;

    Ok(sync_info.latest_block_height)
}

/// Fetch all transactions at a given height.
async fn fetch_transactions(
    channel: Channel,
    height: u64,
) -> Result<Vec<penumbra_sdk_proto::core::transaction::v1::Transaction>> {
    let mut client = AppQueryServiceClient::new(channel);

    let request = TransactionsByHeightRequest {
        block_height: height,
    };

    let response = client
        .transactions_by_height(request)
        .await
        .context("failed to fetch transactions")?;

    // TransactionsByHeightResponse contains Transaction proto messages directly
    Ok(response.into_inner().transactions)
}

fn merge_scan_output(output: &PathBuf, mut next: ScanOutput) -> Result<ScanOutput> {
    let Ok(file) = File::open(output) else {
        return Ok(next);
    };
    let prior_value: serde_json::Value = serde_json::from_reader(BufReader::new(file))
        .context("failed to parse existing scan output JSON for merge")?;
    let prior_detected = if prior_value.get("scan_info").is_some() {
        serde_json::from_value::<ScanOutput>(prior_value)
            .context("failed to parse existing scan output JSON for merge")?
            .detected
    } else {
        serde_json::from_value::<Vec<DetectedTxRef>>(
            prior_value
                .get("detected")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        )
        .context("failed to parse existing detected refs for merge")?
    };
    let mut seen = std::collections::HashSet::new();
    let mut detected = Vec::new();
    for tx_ref in prior_detected.into_iter().chain(next.detected.into_iter()) {
        if seen.insert((tx_ref.height, tx_ref.tx_hash.clone(), tx_ref.action_index)) {
            detected.push(tx_ref);
        }
    }
    detected.sort_by_key(|tx_ref| (tx_ref.height, tx_ref.action_index));
    next.detected = detected;
    Ok(next)
}

/// A single decrypted entry from the compliance audit pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub height: u64,
    pub action_index: usize,
    pub amount: String,
    pub self_address: String,
    pub counterparty: String,
    pub decrypted_via: String,
}

/// Structured issuer ledger row for machine consumers.
#[derive(Clone, Debug, Serialize)]
pub struct LedgerRow {
    pub height: i64,
    pub tx_hash: String,
    pub action_index: i64,
    pub asset_id: String,
    pub is_flagged: bool,
    pub amount: Option<String>,
    pub self_address: Option<String>,
    pub counterparty: Option<String>,
    pub decrypted_at: Option<String>,
    pub decrypted_via: Option<String>,
    pub self_alias: Option<String>,
    pub counterparty_alias: Option<String>,
}

impl IssuerDbCmd {
    pub async fn exec(&self) -> Result<()> {
        match self {
            IssuerDbCmd::Init { db } => {
                let conn = rusqlite::Connection::open(db)
                    .context("failed to create issuer ledger database")?;

                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS compliance_ledger (
                        id              INTEGER PRIMARY KEY AUTOINCREMENT,
                        height          INTEGER NOT NULL,
                        tx_hash         TEXT NOT NULL,
                        action_index    INTEGER NOT NULL,
                        asset_id        TEXT NOT NULL,
                        is_flagged      BOOLEAN NOT NULL,
                        amount          TEXT,
                        self_address    TEXT,
                        counterparty    TEXT,
                        decrypted_at    TEXT,
                        decrypted_via   TEXT,
                        UNIQUE(height, action_index)
                    );
                    CREATE TABLE IF NOT EXISTS address_aliases (
                        transmission_key_hex TEXT PRIMARY KEY,
                        name                 TEXT NOT NULL
                    );",
                )
                .context("failed to create tables")?;

                println!("Issuer ledger initialized at: {}", db.display());
                Ok(())
            }

            IssuerDbCmd::Import {
                db,
                scan_output,
                dk_hex,
                node,
            } => {
                // Parse DK for flagged tx decryption
                let dk = parse_dk_from_hex(dk_hex)?;

                // Load scan output
                let file = File::open(scan_output).context("failed to open scan output file")?;
                let reader = BufReader::new(file);
                let scan: ScanOutput =
                    serde_json::from_reader(reader).context("failed to parse scan output JSON")?;

                let conn = rusqlite::Connection::open(db)
                    .context("failed to open issuer ledger database")?;

                // Connect to node for fetching flagged tx ciphertexts
                let channel = connect_to_node(node).await?;

                let mut inserted = 0u64;
                let mut decrypted = 0u64;

                for tx_ref in &scan.detected {
                    // Insert the row with NULLs for undecrypted fields
                    conn.execute(
                        "INSERT OR IGNORE INTO compliance_ledger \
                         (height, tx_hash, action_index, asset_id, is_flagged) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![
                            tx_ref.height as i64,
                            tx_ref.tx_hash,
                            tx_ref.action_index as i64,
                            tx_ref.asset_id,
                            tx_ref.is_flagged,
                        ],
                    )?;
                    inserted += 1;

                    // Auto-decrypt flagged transactions
                    if tx_ref.is_flagged {
                        let transactions =
                            fetch_transactions(channel.clone(), tx_ref.height).await?;

                        for tx in &transactions {
                            if let Some(ref body) = tx.body {
                                if tx_ref.action_index < body.actions.len() {
                                    let action = &body.actions[tx_ref.action_index];
                                    if let Some(ct) = extract_compliance_ciphertext(action) {
                                        let asset_id = tx_ref
                                            .asset_id
                                            .parse()
                                            .context("invalid asset_id in scan output")?;
                                        if let Some(data) =
                                            decrypt_full_flagged(&dk, &ct, asset_id)?
                                        {
                                            let now = chrono_now();
                                            conn.execute(
                                                "UPDATE compliance_ledger SET \
                                                 amount = ?1, self_address = ?2, \
                                                 counterparty = ?3, decrypted_at = ?4, \
                                                 decrypted_via = 'flagged' \
                                                 WHERE height = ?5 AND action_index = ?6",
                                                rusqlite::params![
                                                    data.amount.value().to_string(),
                                                    hex::encode(
                                                        data.receiver_address.transmission_key
                                                    ),
                                                    hex::encode(
                                                        data.sender_address.transmission_key
                                                    ),
                                                    now,
                                                    tx_ref.height as i64,
                                                    tx_ref.action_index as i64,
                                                ],
                                            )?;
                                            decrypted += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                println!(
                    "Imported {} transfers ({} flagged auto-decrypted).",
                    inserted, decrypted
                );
                Ok(())
            }

            IssuerDbCmd::Show { db, json } => {
                let conn = rusqlite::Connection::open(db)
                    .context("failed to open issuer ledger database")?;

                // Load address aliases for display
                let aliases: std::collections::HashMap<String, String> = {
                    let mut map = std::collections::HashMap::new();
                    if let Ok(mut alias_stmt) =
                        conn.prepare("SELECT transmission_key_hex, name FROM address_aliases")
                    {
                        if let Ok(rows) = alias_stmt.query_map([], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                        }) {
                            for row in rows.flatten() {
                                map.insert(row.0, row.1);
                            }
                        }
                    }
                    map
                };

                let resolve_alias = |hex: &str| -> String {
                    if let Some(name) = aliases.get(hex) {
                        name.clone()
                    } else {
                        truncate_hex(hex, 12)
                    }
                };

                let fit_cell = |value: String, width: usize| -> String {
                    let count = value.chars().count();
                    if count <= width {
                        value
                    } else if width <= 1 {
                        "…".to_string()
                    } else {
                        let trimmed: String = value.chars().take(width - 1).collect();
                        format!("{trimmed}…")
                    }
                };

                let format_via = |via: String| -> String {
                    match via.as_str() {
                        "flagged" => "issuer-dk".to_string(),
                        other => other.to_string(),
                    }
                };

                let mut stmt = conn.prepare(
                    "SELECT height, tx_hash, action_index, asset_id, is_flagged, \
                     amount, self_address, counterparty, decrypted_at, decrypted_via \
                     FROM compliance_ledger ORDER BY height, action_index",
                )?;

                let rows = stmt
                    .query_map([], |row| {
                        let self_address = row.get::<_, Option<String>>(6)?;
                        let counterparty = row.get::<_, Option<String>>(7)?;
                        let self_alias = self_address
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .and_then(|s| aliases.get(s).cloned());
                        let counterparty_alias = counterparty
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .and_then(|s| aliases.get(s).cloned());
                        Ok(LedgerRow {
                            height: row.get::<_, i64>(0)?,
                            tx_hash: row.get::<_, String>(1)?,
                            action_index: row.get::<_, i64>(2)?,
                            asset_id: row.get::<_, String>(3)?,
                            is_flagged: row.get::<_, bool>(4)?,
                            amount: row.get::<_, Option<String>>(5)?,
                            self_address,
                            counterparty,
                            decrypted_at: row.get::<_, Option<String>>(8)?,
                            decrypted_via: row.get::<_, Option<String>>(9)?,
                            self_alias,
                            counterparty_alias,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;

                if *json {
                    println!("{}", serde_json::to_string_pretty(&rows)?);
                    return Ok(());
                }

                // Header
                println!(
                    "{:<8} {:<5} {:<10} {:<8} {:>12} {:<14} {:<14} {:<14} {:<16}",
                    "Height", "Idx", "Action", "Flag", "Amount", "From", "To", "Via", "Decrypted"
                );
                println!("{}", "-".repeat(110));

                // Collect all rows for two-pass display (spend→output inference)
                struct DisplayRow {
                    height: i64,
                    action_idx: i64,
                    is_flagged: bool,
                    has_amount: bool,
                    amount_str: String,
                    self_str: String,
                    cp_str: String,
                    via_str: String,
                    when_str: String,
                }

                let mut display_rows: Vec<DisplayRow> = Vec::new();
                for row in rows {
                    let dash = "---".to_string();
                    display_rows.push(DisplayRow {
                        height: row.height,
                        action_idx: row.action_index,
                        is_flagged: row.is_flagged,
                        has_amount: row.amount.is_some(),
                        amount_str: row
                            .amount
                            .as_deref()
                            .map(|s| format_display_amount(s))
                            .map(|s| fit_cell(s, 12))
                            .unwrap_or_else(|| dash.clone()),
                        self_str: row
                            .self_address
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .map(|s| resolve_alias(s))
                            .map(|s| fit_cell(s, 14))
                            .unwrap_or_else(|| dash.clone()),
                        cp_str: row
                            .counterparty
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .map(|s| resolve_alias(s))
                            .map(|s| fit_cell(s, 14))
                            .unwrap_or_else(|| dash.clone()),
                        via_str: row
                            .decrypted_via
                            .map(format_via)
                            .map(|s| fit_cell(s, 14))
                            .unwrap_or_else(|| dash.clone()),
                        when_str: row
                            .decrypted_at
                            .as_deref()
                            .map(|s| format_timestamp(s))
                            .unwrap_or_else(|| dash.clone()),
                    });
                }

                let mut count = 0u64;
                let mut decrypted_count = 0u64;
                let mut flagged_count = 0u64;
                for r in &display_rows {
                    if r.is_flagged {
                        flagged_count += 1;
                    }
                    if r.has_amount {
                        decrypted_count += 1;
                    }

                    println!(
                        "{:<8} {:<5} {:<10} {:<8} {:>12} {:<14} {:<14} {:<14} {:<16}",
                        r.height,
                        r.action_idx,
                        "TRANSFER",
                        if r.is_flagged { "FLAGGED" } else { "" },
                        r.amount_str,
                        r.cp_str,
                        r.self_str,
                        r.via_str,
                        r.when_str,
                    );
                    count += 1;
                }

                println!("{}", "-".repeat(110));
                println!(
                    "Total: {} transfers | {} decrypted | {} flagged | {} encrypted",
                    count,
                    decrypted_count,
                    flagged_count,
                    count - decrypted_count
                );
                Ok(())
            }

            IssuerDbCmd::Update {
                db,
                audit_output,
                audit_subject,
            } => {
                // Load audit output from orbis-sim
                let file = File::open(audit_output).context("failed to open audit output file")?;
                let reader = BufReader::new(file);
                let entries: Vec<AuditEntry> =
                    serde_json::from_reader(reader).context("failed to parse audit output JSON")?;

                let conn = rusqlite::Connection::open(db)
                    .context("failed to open issuer ledger database")?;

                // Build set of known addresses from alias table for extension validation.
                // Sender-side PRE has no auth tag, so ~50% of wrong-key decryptions produce
                // valid curve points but garbage addresses. Reject unknown addresses.
                let known_addresses: std::collections::HashSet<String> = {
                    let mut set = std::collections::HashSet::new();
                    if let Ok(mut stmt) =
                        conn.prepare("SELECT transmission_key_hex FROM address_aliases")
                    {
                        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                            for row in rows.flatten() {
                                set.insert(row);
                            }
                        }
                    }
                    set
                };

                let mut updated = 0u64;
                let mut skipped = 0u64;
                let now = chrono_now();

                for entry in &entries {
                    // Validate extension entries: sender-side PRE has no auth tag,
                    // so ~50% of wrong-key decryptions produce garbage addresses.
                    // Skip entries where self_address is unknown (likely false positive).
                    if entry.decrypted_via.ends_with("extension")
                        && !entry.counterparty.is_empty()
                        && !known_addresses.contains(&entry.self_address)
                    {
                        skipped += 1;
                        continue;
                    }

                    // Prefix via with audit subject name (e.g. "core" → "Alice core")
                    let via = match &audit_subject {
                        Some(subject) => format!("{} {}", subject, entry.decrypted_via),
                        None => entry.decrypted_via.clone(),
                    };

                    let is_core_only = entry.counterparty.is_empty();

                    if is_core_only {
                        // Core-only audit: set amount + self_address on rows not yet decrypted
                        let changes = conn.execute(
                            "UPDATE compliance_ledger SET \
                             amount = ?1, self_address = ?2, \
                             decrypted_at = ?3, decrypted_via = ?4 \
                             WHERE height = ?5 AND action_index = ?6 AND amount IS NULL",
                            rusqlite::params![
                                entry.amount,
                                entry.self_address,
                                now,
                                via,
                                entry.height as i64,
                                entry.action_index as i64,
                            ],
                        )?;
                        updated += changes as u64;
                    } else {
                        // Full audit: try upgrading a core-only row (add counterparty)
                        // Only update via when counterparty is actually being set.
                        let changes = conn.execute(
                            "UPDATE compliance_ledger SET \
                             self_address = ?1, counterparty = ?2, decrypted_at = ?3, decrypted_via = ?4 \
                             WHERE height = ?5 AND action_index = ?6 \
                             AND amount IS NOT NULL AND (counterparty IS NULL OR counterparty = '')",
                            rusqlite::params![
                                entry.self_address,
                                entry.counterparty,
                                now,
                                via,
                                entry.height as i64,
                                entry.action_index as i64,
                            ],
                        )?;

                        if changes == 0 {
                            // No prior core-only row; do a first-time full insert
                            let changes = conn.execute(
                                "UPDATE compliance_ledger SET \
                                 amount = ?1, self_address = ?2, counterparty = ?3, \
                                 decrypted_at = ?4, decrypted_via = ?5 \
                                 WHERE height = ?6 AND action_index = ?7 AND amount IS NULL",
                                rusqlite::params![
                                    entry.amount,
                                    entry.self_address,
                                    entry.counterparty,
                                    now,
                                    via,
                                    entry.height as i64,
                                    entry.action_index as i64,
                                ],
                            )?;
                            updated += changes as u64;
                        } else {
                            updated += changes as u64;
                        }
                    }
                }

                println!(
                    "Updated {} rows from audit ({} entries, {} false positives skipped).",
                    updated,
                    entries.len(),
                    skipped
                );
                Ok(())
            }

            IssuerDbCmd::Alias { db, address, name } => {
                let addr: Address = address.parse().context("invalid Penumbra address")?;
                let tk_hex = hex::encode(addr.transmission_key().0);

                let conn = rusqlite::Connection::open(db)
                    .context("failed to open issuer ledger database")?;

                conn.execute(
                    "INSERT OR REPLACE INTO address_aliases (transmission_key_hex, name) \
                     VALUES (?1, ?2)",
                    rusqlite::params![tk_hex, name],
                )?;

                println!("Alias set: {} -> {}...", name, &tk_hex[..16]);
                Ok(())
            }
        }
    }
}

fn truncate_hex(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", secs)
}

/// Convert a base-unit amount string to display units (exponent 18).
fn format_display_amount(raw: &str) -> String {
    let val: u128 = match raw.parse() {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    let exp: u128 = 1_000_000_000_000_000_000; // 10^18
    let whole = val / exp;
    let frac = val % exp;
    if frac == 0 {
        format!("{}", whole)
    } else {
        // Show up to 6 decimal places, trimming trailing zeros
        let frac_scaled = frac / 1_000_000_000_000; // keep 6 digits
        let s = format!("{}.{:06}", whole, frac_scaled);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Format a Unix epoch-seconds timestamp into "YYYY-MM-DD HH:MM".
fn format_timestamp(epoch_str: &str) -> String {
    let secs: u64 = match epoch_str.parse() {
        Ok(v) => v,
        Err(_) => return epoch_str.to_string(),
    };
    // Manual UTC conversion (no chrono dependency needed)
    let s = secs;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Days since 1970-01-01 to Y-M-D
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        year, month, day, hours, minutes
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
