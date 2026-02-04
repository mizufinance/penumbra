use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::decrypt_full;
use penumbra_sdk_compliance::structs::{ComplianceCiphertext, MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_custody::soft_kms::Config as SoftKmsConfig;
use penumbra_sdk_keys::keys::{DailyComplianceKey, DailyKeySet, KeyType, UserComplianceKey};
use penumbra_sdk_proto::core::app::v1::{
    query_service_client::QueryServiceClient as AppQueryServiceClient, TransactionsByHeightRequest,
};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetStatusRequest,
};
use penumbra_sdk_transaction::{ActionPlan, TransactionPlan};
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use tracing::info;
use url::Url;

use super::FeeTier;
use crate::config::CustodyConfig;

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
        /// Amount threshold for flagging (in smallest unit, u128).
        /// Transfers at or above this amount are encrypted to issuer's DK instead of user's daily key.
        #[clap(long)]
        threshold: Option<u128>,
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
        /// Address index to register (default: 0).
        /// Each address has a different ACK for privacy.
        #[clap(long, default_value = "0")]
        address_index: u32,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },

    /// Derive a daily key from a User Compliance Key for a specific date.
    ///
    /// This command is used by Orbis to create time-limited keys that can be
    /// shared with auditors. The auditor can then use the daily key to scan
    /// transactions for that specific date, without having access to the full
    /// User Compliance Key.
    ///
    /// Example workflow:
    /// 1. Orbis runs: pcli tx compliance derive-daily-key --uck-hex <UCK> --date <DAY>
    /// 2. Orbis shares the daily_key_hex with the auditor
    /// 3. Auditor runs: pcli tx compliance scan --daily-key-hex <KEY> --node <URL>
    DeriveDailyKey {
        /// User Compliance Key as hex string (64 hex chars = 32 bytes).
        #[clap(long = "uck-hex")]
        uck_hex: String,

        /// The day index to derive the key for (e.g., 20459 for a specific day).
        /// This is typically computed as: unix_timestamp / 86400
        #[clap(long)]
        date: u64,
    },

    /// Scan the chain for regulated asset transfers (detection-only).
    ///
    /// This command performs detection-only scanning, identifying which transactions
    /// contain compliance ciphertexts that can be decrypted with the provided key.
    /// It outputs a list of transaction references that can be passed to the
    /// `decrypt` command for full decryption.
    ///
    /// For user scanning: provide a daily key set from derive-daily-key.
    /// For issuer scanning: provide the issuer's DK (detection key).
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

        /// User's daily key set (192 hex chars = 96 bytes).
        /// Use for scanning non-threshold assets or below-threshold transfers.
        #[clap(long, group = "key")]
        daily_key_hex: Option<String>,

        /// Issuer's detection key (64 hex chars = 32 bytes).
        /// Use for scanning all transfers of a threshold asset.
        #[clap(long, group = "key")]
        dk_hex: Option<String>,

        /// Output file for detected TX list (JSON format).
        #[clap(long, default_value = "/tmp/detected_txs.json")]
        output: PathBuf,
    },

    /// Decrypt previously detected transactions.
    ///
    /// This command takes a list of transaction references (from the scan command)
    /// and decrypts them using the provided key. Use daily-key-hex for user
    /// decryption or dk-hex for issuer decryption of flagged transfers.
    Decrypt {
        /// Path to detected TX list from scan command (JSON format).
        #[clap(long)]
        input: PathBuf,

        /// The URL of the pd gRPC endpoint.
        #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
        node: Url,

        /// User's full daily key set (192 hex chars = 96 bytes).
        #[clap(long, group = "key")]
        daily_key_hex: Option<String>,

        /// Issuer's detection key for flagged transfers (64 hex chars = 32 bytes).
        #[clap(long, group = "key")]
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
}

impl ComplianceCmd {
    /// Determine if this command requires a network sync before executing.
    pub fn offline(&self) -> bool {
        match self {
            ComplianceCmd::RegisterAsset { .. } => false,
            ComplianceCmd::RegisterUser { .. } => false,
            ComplianceCmd::DeriveDailyKey { .. } => true, // No network needed
            ComplianceCmd::Scan { .. } => true,           // Scanner doesn't need wallet sync
            ComplianceCmd::Decrypt { .. } => true,        // Decrypt doesn't need wallet sync
            ComplianceCmd::GenerateDk => true,            // Pure computation
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

    /// Check if this command is a derive-daily-key command (doesn't create a transaction).
    pub fn is_derive_daily_key(&self) -> bool {
        matches!(self, ComplianceCmd::DeriveDailyKey { .. })
    }

    /// Check if this command is a generate-dk command (doesn't create a transaction).
    pub fn is_generate_dk(&self) -> bool {
        matches!(self, ComplianceCmd::GenerateDk)
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

    /// Execute the derive-daily-key command (pure computation, no network).
    pub fn exec_derive_daily_key(&self) -> Result<()> {
        match self {
            ComplianceCmd::DeriveDailyKey { uck_hex, date } => {
                // Parse the UCK (User Compliance Key) from hex
                let uck = parse_uck_from_hex(uck_hex)?;

                // Derive all three daily keys
                let daily_keys = uck.derive_daily_keys(*date);

                // Output each key type (detection is issuer-only, not user-derivable)
                let core_hex = hex::encode(daily_keys.core.to_bytes());
                let extension_hex = hex::encode(daily_keys.extension.to_bytes());

                // Combined format for decryption (core + extension keys)
                let full_hex = format!("{}{}", core_hex, extension_hex);

                println!("=== Daily Key Derivation ===");
                println!("Date (day index): {}", date);
                println!();
                println!("Individual keys (for selective disclosure):");
                println!("  Core Key:      {}", core_hex);
                println!("  Extension Key: {}", extension_hex);
                println!();
                println!("Combined key (for decryption):");
                println!("  Full Key Set:  {}", full_hex);
                println!();
                println!("Note: Detection is issuer-only (use --issuer-dk-hex for scanning)");
                println!("To decrypt with known asset:");
                println!(
                    "  pcli tx compliance decrypt --daily-key-hex {} --input <file>",
                    full_hex
                );

                Ok(())
            }
            _ => anyhow::bail!("exec_derive_daily_key called on wrong command"),
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
                daily_key_hex,
                dk_hex,
                output,
            } => {
                // Determine key type and parse key
                let (key_type_str, detection_key, issuer_dk) = if let Some(hex) = daily_key_hex {
                    // Warn user that daily keys cannot detect - this is issuer-only
                    eprintln!("Warning: --daily-key-hex cannot detect transactions (detection is issuer-only)");
                    eprintln!("         Use --dk-hex with issuer detection key for scanning.");
                    eprintln!("         Daily keys are for decryption only (use 'decrypt' command instead).");
                    eprintln!();
                    let keys = parse_daily_keys_from_hex(hex)?;
                    ("user_daily_key".to_string(), Some(keys), None)
                } else if let Some(hex) = dk_hex {
                    let dk = parse_dk_from_hex(hex)?;
                    ("issuer_dk".to_string(), None, Some(dk))
                } else {
                    anyhow::bail!("Must provide either --daily-key-hex or --dk-hex");
                };

                // Connect to node directly (no wallet required)
                let channel = connect_to_node(node).await?;
                let end = if let Some(h) = end_height {
                    *h
                } else {
                    get_latest_height(channel.clone()).await?
                };

                println!(
                    "Scanning blocks {} to {} for regulated transfers (detection-only)...",
                    start_height, end
                );
                println!("Key type: {}", key_type_str);

                let mut detected_txs: Vec<DetectedTxRef> = Vec::new();

                // Scan each block
                for height in *start_height..=end {
                    // Fetch transactions for this block
                    let transactions = fetch_transactions(channel.clone(), height).await?;

                    for (tx_idx, tx) in transactions.iter().enumerate() {
                        // Try to detect compliance ciphertexts in this transaction
                        let tx_hash = format!("block{}tx{}", height, tx_idx); // Simplified hash

                        // Extract compliance ciphertexts from actions
                        if let Some(ref body) = tx.body {
                            for (action_idx, action) in body.actions.iter().enumerate() {
                                if let Some(ciphertext) = extract_compliance_ciphertext(action) {
                                    // Detection is issuer-only via DetectionKey
                                    let detection_result = if let Some(ref dk) = issuer_dk {
                                        // Issuer detection with DK
                                        use penumbra_sdk_compliance::issuer_keys::DetectionKey;
                                        let detection_key = DetectionKey::new(*dk);
                                        match detection_key.try_decrypt_detection(
                                            &ciphertext.epk,
                                            &ciphertext.epk_g,
                                            &ciphertext.detection_tag,
                                        ) {
                                            Ok((asset_id, is_flagged)) => {
                                                Some((asset_id, is_flagged))
                                            }
                                            Err(_) => None, // Not encrypted to this DK
                                        }
                                    } else if detection_key.is_some() {
                                        // User daily keys cannot detect - detection is issuer-only
                                        // Skip detection, user must use --issuer-dk-hex for scanning
                                        None
                                    } else {
                                        None
                                    };

                                    if let Some((asset_id, is_flagged)) = detection_result {
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

                // Build output
                let scan_output = ScanOutput {
                    scan_info: ScanInfo {
                        key_type: key_type_str,
                        start_height: *start_height,
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

                // Write to file
                let json = serde_json::to_string_pretty(&scan_output)?;
                let mut file = File::create(output)?;
                file.write_all(json.as_bytes())?;

                println!(
                    "\nScan complete. Detected {} transfers.",
                    detected_txs.len()
                );
                println!("Results saved to: {}", output.display());

                // Print summary
                for tx_ref in &detected_txs {
                    println!(
                        "  Height {}, action {}: {} (flagged: {})",
                        tx_ref.height, tx_ref.action_index, tx_ref.asset_id, tx_ref.is_flagged
                    );
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
                daily_key_hex,
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
                let (key_type, daily_keys, issuer_dk) = if let Some(hex) = daily_key_hex {
                    let keys = parse_daily_keys_from_hex(hex)?;
                    ("user_daily_key", Some(keys), None)
                } else if let Some(hex) = dk_hex {
                    let dk = parse_dk_from_hex(hex)?;
                    ("issuer_dk", None, Some(dk))
                } else {
                    anyhow::bail!("Must provide either --daily-key-hex or --dk-hex");
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
                                    let result = if let Some(ref keys) = daily_keys {
                                        decrypt_full(keys, &ciphertext, asset_id)
                                    } else if let Some(ref dk) = issuer_dk {
                                        // Use issuer decryption
                                        use penumbra_sdk_compliance::decrypt_compliance_details_with_dk;
                                        decrypt_compliance_details_with_dk(dk, &ciphertext)
                                            .map(|data| Some(penumbra_sdk_compliance::FullComplianceData {
                                                asset_id: data.asset_id,
                                                core: penumbra_sdk_compliance::CoreData {
                                                    amount: data.amount,
                                                    self_diversified_generator: data.self_diversified_generator,
                                                    self_transmission_key: data.self_transmission_key,
                                                },
                                                extension: penumbra_sdk_compliance::ExtensionData {
                                                    counterparty_diversified_generator: data.counterparty_diversified_generator,
                                                    counterparty_transmission_key: data.counterparty_transmission_key,
                                                },
                                            }))
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
                                            println!("   Amount: {}", data.core.amount);
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

                // Create the registration message
                let msg = MsgRegisterAsset {
                    asset_id,
                    is_regulated,
                    dk_pub,
                    threshold: *threshold,
                };

                // Build transaction plan
                let mut planner = penumbra_sdk_wallet::plan::Planner::new(rand_core::OsRng);
                planner
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());

                planner.action(ActionPlan::from(msg));

                Ok(planner
                    .plan(app.view(), penumbra_sdk_keys::keys::AddressIndex::new(0))
                    .await
                    .context("can't build transaction")?)
            }

            ComplianceCmd::RegisterUser {
                asset_id,
                address_index,
                fee_tier,
            } => {
                // Parse asset ID
                let asset_id = Self::parse_asset_id(asset_id)?;

                // Get user's full viewing key
                let fvk = app.config.full_viewing_key.clone();

                // Get address at specified index
                let address_index = penumbra_sdk_keys::keys::AddressIndex::new(*address_index);
                let (address, _detection_key) = fvk.payment_address(address_index);

                // Derive user-specific UCK from spend key seed
                // This ensures each user has their own unique UCK for per-user isolation
                let user_uck = match &app.config.custody {
                    CustodyConfig::SoftKms(SoftKmsConfig { spend_key, .. }) => {
                        let seed = spend_key.to_bytes().0;
                        let uck = UserComplianceKey::from_spend_seed(&seed);
                        println!("Derived user-specific UCK from wallet seed");
                        println!("   UCK (hex): {}", hex::encode(uck.to_bytes()));
                        uck
                    }
                    _ => {
                        // Non-SoftKms custody (view-only, threshold, etc.) cannot derive UCK
                        anyhow::bail!(
                            "Cannot derive compliance key: custody type doesn't have spend key. \
                             Compliance registration requires a SoftKms custody configuration."
                        );
                    }
                };

                // Derive ACK for this specific address (maximum privacy - one ACK per address)
                use penumbra_sdk_compliance::ComplianceLeaf;
                let leaf = ComplianceLeaf::new(&user_uck, address, asset_id);

                // Create registration message (signature is empty for now - filled during tx build)
                let msg = MsgRegisterUser {
                    leaf,
                    signature: vec![],
                };

                // Build transaction plan
                let mut planner = penumbra_sdk_wallet::plan::Planner::new(rand_core::OsRng);
                planner
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());

                planner.action(ActionPlan::from(msg));

                Ok(planner
                    .plan(app.view(), address_index)
                    .await
                    .context("can't build transaction")?)
            }

            ComplianceCmd::DeriveDailyKey { .. } => {
                anyhow::bail!("DeriveDailyKey command doesn't create a transaction - use exec_derive_daily_key instead")
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

/// Parse User Compliance Key from hex string.
fn parse_uck_from_hex(hex: &str) -> Result<UserComplianceKey> {
    let bytes = hex::decode(hex).context("invalid hex string for UCK")?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "UCK must be exactly 32 bytes (64 hex chars), got {}",
            bytes.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let fr = decaf377::Fr::from_le_bytes_mod_order(&arr);
    Ok(UserComplianceKey::new(fr))
}

/// Parse DailyKeySet from hex string (64 bytes = 2 keys × 32 bytes each: core + extension).
/// Note: Detection is issuer-only and not part of user daily keys.
fn parse_daily_keys_from_hex(hex: &str) -> Result<DailyKeySet> {
    let bytes = hex::decode(hex).context("invalid hex string for daily key set")?;
    if bytes.len() != 64 {
        anyhow::bail!(
            "Daily key set must be exactly 64 bytes (128 hex chars = 2 keys × 32 bytes), got {} bytes",
            bytes.len()
        );
    }

    let mut core_arr = [0u8; 32];
    let mut extension_arr = [0u8; 32];

    core_arr.copy_from_slice(&bytes[0..32]);
    extension_arr.copy_from_slice(&bytes[32..64]);

    Ok(DailyKeySet {
        core: DailyComplianceKey::from_bytes(&core_arr, KeyType::Core),
        extension: DailyComplianceKey::from_bytes(&extension_arr, KeyType::Extension),
    })
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

/// Extract ComplianceCiphertext from a protobuf Action if present.
fn extract_compliance_ciphertext(
    action: &penumbra_sdk_proto::core::transaction::v1::Action,
) -> Option<ComplianceCiphertext> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action as ActionEnum;

    let action_inner = action.action.as_ref()?;

    // Extract compliance ciphertext from Output actions
    if let ActionEnum::Output(output) = action_inner {
        if let Some(body) = &output.body {
            let cc_bytes = &body.compliance_ciphertext;
            if !cc_bytes.is_empty() {
                // Parse the compliance ciphertext from bytes
                return ComplianceCiphertext::from_bytes(cc_bytes).ok();
            }
        }
    }

    None
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
