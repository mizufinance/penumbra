use std::path::PathBuf;

use anyhow::{Context, Result};
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_compliance::{
    derive_compliance_scalar, issuer_keys::DetectionKey, ComplianceLeaf, IssuerComplianceWorker,
    SqliteScannerStore,
};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetStatusRequest,
};
use penumbra_sdk_transaction::{ActionPlan, TransactionPlan};
use penumbra_sdk_view::{NoteManager, TransferPlanningResult};
use tonic::transport::Channel;
use url::Url;

use super::FeeTier;

/// Compliance-related transaction commands.
#[derive(Debug, clap::Subcommand)]
pub enum ComplianceCmd {
    /// Register an asset's regulation status in the compliance registry.
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
        #[clap(long)]
        dk_pub_hex: Option<String>,
        /// Amount threshold for flagging, in base units.
        #[clap(long)]
        threshold: Option<u128>,
        /// Orbis ring public key (hex, 64 chars = 32 bytes compressed).
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
    RegisterUser {
        /// The asset ID to register for (e.g., "uusdc").
        asset_id: String,
        /// Penumbra address to register. If omitted, derives the address from
        /// this wallet using --address-index.
        #[clap(long)]
        address: Option<String>,
        /// Address index to register (default: 0).
        #[clap(long, default_value = "0")]
        address_index: u32,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },

    /// Run or catch up the issuer compliance scanner.
    #[clap(subcommand)]
    Scan(ScanCmd),

    /// Generate a new issuer detection key pair.
    GenerateDk,
}

impl ComplianceCmd {
    /// Determine if this command requires a network sync before executing.
    pub fn offline(&self) -> bool {
        match self {
            ComplianceCmd::RegisterAsset { .. } => false,
            ComplianceCmd::RegisterUser { .. } => false,
            ComplianceCmd::Scan(_) => true,
            ComplianceCmd::GenerateDk => true,
        }
    }

    /// Check if this command is a scanner command.
    pub fn is_scan(&self) -> bool {
        matches!(self, ComplianceCmd::Scan(_))
    }

    /// Check if this command is a generate-dk command.
    pub fn is_generate_dk(&self) -> bool {
        matches!(self, ComplianceCmd::GenerateDk)
    }

    /// Execute the persistent issuer scanner.
    pub async fn exec_scan(&self) -> Result<()> {
        let ComplianceCmd::Scan(scan) = self else {
            anyhow::bail!("exec_scan called on non-scan command");
        };

        let (node, db, dk_hex, scan_asset_id, follow) = match scan {
            ScanCmd::Run {
                node,
                db,
                dk_hex,
                scan_asset_id,
            } => (node, db, dk_hex, scan_asset_id, true),
            ScanCmd::CatchUp {
                node,
                db,
                dk_hex,
                scan_asset_id,
            } => (node, db, dk_hex, scan_asset_id, false),
        };

        let detection_key = DetectionKey::new(parse_dk_from_hex(dk_hex)?);
        let target_asset_id = Self::parse_asset_id(scan_asset_id)?;
        let storage = SqliteScannerStore::new(db)
            .with_context(|| format!("failed to open scanner database {}", db.display()))?;
        let channel = connect_to_node(node).await?;
        let (worker, handle) =
            IssuerComplianceWorker::new(detection_key, target_asset_id, storage, channel.clone());

        println!(
            "Starting issuer compliance scanner at height {} (db: {})",
            handle.current_height().saturating_add(1),
            db.display()
        );
        if follow {
            worker.run().await
        } else {
            let end_height = latest_block_height(channel).await?;
            println!("Catching up issuer compliance scanner through height {end_height}");
            worker.catch_up_to_height(end_height).await
        }
    }

    /// Execute the generate-dk command.
    pub fn exec_generate_dk(&self) -> Result<()> {
        match self {
            ComplianceCmd::GenerateDk => {
                let dk = decaf377::Fr::rand(&mut rand_core::OsRng);
                let dk_pub = decaf377::Element::GENERATOR * dk;
                let dk_hex = hex::encode(dk.to_bytes());
                let dk_pub_hex = hex::encode(dk_pub.vartime_compress().0);

                println!("=== Issuer Detection Key Generation ===");
                println!();
                println!("Private key (keep secret, use for scanning):");
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
                let is_regulated = if *regulated {
                    true
                } else if *unregulated {
                    false
                } else {
                    anyhow::bail!("Must specify either --regulated or --unregulated");
                };

                let asset_id = Self::parse_asset_id(asset_id)?;

                let dk_pub = if let Some(hex_str) = dk_pub_hex {
                    Some(parse_decaf377_element(hex_str, "dk_pub_hex")?)
                } else if is_regulated {
                    anyhow::bail!(
                        "--dk-pub-hex is required for regulated assets. \
                        Generate one with: pcli tx compliance generate-dk"
                    );
                } else {
                    None
                };

                let ring_pk = ring_pk_hex
                    .as_ref()
                    .map(|hex_str| parse_decaf377_element(hex_str, "ring_pk_hex"))
                    .transpose()?;

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

                let mut note_manager = NoteManager::new(rand_core::OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());

                plan_with_single_action(
                    &mut note_manager,
                    app,
                    penumbra_sdk_keys::keys::AddressIndex::new(0),
                    ActionPlan::from(msg),
                )
                .await
            }

            ComplianceCmd::RegisterUser {
                asset_id,
                address,
                address_index,
                fee_tier,
            } => {
                let asset_id = Self::parse_asset_id(asset_id)?;
                let fvk = app.config.full_viewing_key.clone();
                let address_index = penumbra_sdk_keys::keys::AddressIndex::new(*address_index);
                let address = match address {
                    Some(address) => address.parse().context("invalid Penumbra address")?,
                    None => {
                        let (address, _detection_key) = fvk.payment_address(address_index);
                        address
                    }
                };

                let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                let d = derive_compliance_scalar(b_d_fq);
                let leaf = ComplianceLeaf::new(address, asset_id, d);
                let msg = MsgRegisterUser {
                    leaf,
                    signature: vec![],
                };

                let mut note_manager = NoteManager::new(rand_core::OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());

                plan_with_single_action(
                    &mut note_manager,
                    app,
                    address_index,
                    ActionPlan::from(msg),
                )
                .await
            }

            ComplianceCmd::Scan(_) => {
                anyhow::bail!("Scan command doesn't create a transaction - use exec_scan instead")
            }

            ComplianceCmd::GenerateDk => {
                anyhow::bail!(
                    "GenerateDk command doesn't create a transaction - use exec_generate_dk instead"
                )
            }
        }
    }

    /// Helper to parse asset ID from string.
    /// Accepts either a full asset ID or a unit name like "penumbra" or "upenumbra".
    fn parse_asset_id(asset_str: &str) -> Result<asset::Id> {
        if let Ok(asset_id) = asset_str.parse() {
            return Ok(asset_id);
        }
        Ok(asset::REGISTRY.parse_unit(asset_str).id())
    }
}

#[derive(Debug, clap::Subcommand)]
pub enum ScanCmd {
    /// Follow the chain continuously, persisting scanner/audit state in SQLite.
    Run {
        /// The URL of the pd gRPC endpoint (e.g., http://localhost:8080).
        #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
        node: Url,

        /// Path to the scanner SQLite database.
        #[clap(long, default_value = "/tmp/compliance-scanner.db")]
        db: PathBuf,

        /// Issuer's detection key (64 hex chars = 32 bytes).
        #[clap(long)]
        dk_hex: String,

        /// The asset ID this DK corresponds to.
        #[clap(long)]
        scan_asset_id: String,
    },

    /// Scan from stored progress to the node's current latest height, then exit.
    CatchUp {
        /// The URL of the pd gRPC endpoint (e.g., http://localhost:8080).
        #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
        node: Url,

        /// Path to the scanner SQLite database.
        #[clap(long, default_value = "/tmp/compliance-scanner.db")]
        db: PathBuf,

        /// Issuer's detection key (64 hex chars = 32 bytes).
        #[clap(long)]
        dk_hex: String,

        /// The asset ID this DK corresponds to.
        #[clap(long)]
        scan_asset_id: String,
    },
}

async fn plan_with_single_action<R>(
    note_manager: &mut NoteManager<R>,
    app: &mut crate::App,
    address_index: penumbra_sdk_keys::keys::AddressIndex,
    action: ActionPlan,
) -> Result<TransactionPlan>
where
    R: rand_core::RngCore + rand_core::CryptoRng,
{
    match note_manager
        .plan_actions_with_transfer_funding(app.view(), address_index, vec![action])
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

fn parse_decaf377_element(hex_str: &str, label: &str) -> Result<decaf377::Element> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid {label}: must be hex"))?;
    if bytes.len() != 32 {
        anyhow::bail!("{label} must be exactly 64 hex chars (32 bytes)");
    }
    let arr: [u8; 32] = bytes.try_into().unwrap();
    decaf377::Encoding(arr)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid {label} encoding"))
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

/// Connect to a Penumbra node directly.
async fn connect_to_node(node_url: &Url) -> Result<Channel> {
    let endpoint = tonic::transport::Endpoint::from_shared(node_url.to_string())
        .context("invalid node URL")?
        .timeout(std::time::Duration::from_secs(30));

    endpoint
        .connect()
        .await
        .with_context(|| format!("failed to connect to node at {node_url}"))
}

async fn latest_block_height(channel: Channel) -> Result<u64> {
    let mut client = TendermintProxyServiceClient::new(channel);
    let status = client
        .get_status(GetStatusRequest {})
        .await
        .context("failed to query node status")?
        .into_inner();
    status
        .sync_info
        .map(|sync_info| sync_info.latest_block_height)
        .ok_or_else(|| anyhow::anyhow!("node status response missing sync_info"))
}
