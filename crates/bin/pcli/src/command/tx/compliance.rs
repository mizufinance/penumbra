use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::structs::{
    AssetRegistrationGrant, AssetRegistrationGrantBody, IbcAssetOrigin, IbcRoute, MsgRegisterAsset,
    MsgRegisterUser, UserRegistrationGrant, UserRegistrationGrantBody,
};
use penumbra_sdk_compliance::{
    derive_compliance_scalar, issuer_keys::DetectionKey, ComplianceLeaf, IssuerComplianceWorker,
    RpcAuditAdviceProvider, SqliteScannerStore, TendermintProxyBlockIdentityProvider,
};
use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetStatusRequest,
};
use penumbra_sdk_proto::DomainType;
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
        /// Registration-authority verification key for regulated user grants, hex-encoded.
        #[clap(long)]
        registration_authority_vk_hex: Option<String>,
        /// Allowed direct IBC route: <local_channel,connection_id,counterparty_channel>.
        #[clap(long = "allowed-ibc-route")]
        allowed_ibc_routes: Vec<String>,
        /// Base denom for a regulated external IBC origin.
        #[clap(long)]
        ibc_origin_base_denom: Option<String>,
        /// Origin route: <local_channel,connection_id,counterparty_channel>.
        #[clap(long)]
        ibc_origin_route: Option<String>,
        /// Asset registration grant, hex-encoded protobuf bytes.
        #[clap(long)]
        asset_registration_grant_hex: String,
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
        /// User registration grant, hex-encoded protobuf bytes.
        #[clap(long)]
        user_registration_grant_hex: String,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },

    /// Run or catch up the issuer compliance scanner.
    #[clap(subcommand)]
    Scan(ScanCmd),

    /// Generate a new issuer detection key pair.
    GenerateDk,

    /// Sign an asset registration grant.
    SignAssetGrant {
        /// The asset ID authorized by this grant.
        asset_id: String,
        /// Mark this asset as regulated.
        #[clap(long)]
        regulated: bool,
        /// Mark this asset as unregulated.
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
        /// Registration-authority verification key for regulated user grants, hex-encoded.
        #[clap(long)]
        registration_authority_vk_hex: Option<String>,
        /// Allowed direct IBC route: <local_channel,connection_id,counterparty_channel>.
        #[clap(long = "allowed-ibc-route")]
        allowed_ibc_routes: Vec<String>,
        /// Base denom for a regulated external IBC origin.
        #[clap(long)]
        ibc_origin_base_denom: Option<String>,
        /// Origin route: <local_channel,connection_id,counterparty_channel>.
        #[clap(long)]
        ibc_origin_route: Option<String>,
        /// Registrar signing key authorized in genesis, hex-encoded.
        #[clap(long)]
        registrar_sk_hex: String,
        /// Grant expiration as Unix seconds.
        #[clap(long)]
        valid_until_unix: u64,
    },

    /// Sign a user registration grant.
    SignUserGrant {
        /// The asset ID authorized by this grant.
        asset_id: String,
        /// Penumbra address authorized by this grant.
        #[clap(long)]
        address: Address,
        /// SourceHub policy ID bound to this grant.
        #[clap(long, default_value = "")]
        policy_id: String,
        /// Registration-authority signing key for this asset, hex-encoded.
        #[clap(long)]
        registration_authority_sk_hex: String,
        /// Grant expiration as Unix seconds.
        #[clap(long)]
        valid_until_unix: u64,
    },

    /// Derive a spend verification key from a spend signing key.
    DeriveSpendVk {
        /// Spend signing key, hex-encoded.
        #[clap(long)]
        signing_key_hex: String,
    },
}

impl ComplianceCmd {
    /// Determine if this command requires a network sync before executing.
    pub fn offline(&self) -> bool {
        match self {
            ComplianceCmd::RegisterAsset { .. } => false,
            ComplianceCmd::RegisterUser { .. } => false,
            ComplianceCmd::Scan(_) => true,
            ComplianceCmd::GenerateDk => true,
            ComplianceCmd::SignAssetGrant { .. } => true,
            ComplianceCmd::SignUserGrant { .. } => true,
            ComplianceCmd::DeriveSpendVk { .. } => true,
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

    pub fn is_sign_grant(&self) -> bool {
        matches!(
            self,
            ComplianceCmd::SignAssetGrant { .. }
                | ComplianceCmd::SignUserGrant { .. }
                | ComplianceCmd::DeriveSpendVk { .. }
        )
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
        let (worker, handle) = IssuerComplianceWorker::new(
            detection_key,
            target_asset_id,
            Arc::new(storage),
            Arc::new(TendermintProxyBlockIdentityProvider::new(channel.clone())),
            Arc::new(RpcAuditAdviceProvider::new(channel.clone())),
            channel.clone(),
        )
        .await?;

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

    /// Execute offline grant-signing commands.
    pub fn exec_sign_grant(&self) -> Result<()> {
        match self {
            ComplianceCmd::SignAssetGrant {
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
                registration_authority_vk_hex,
                allowed_ibc_routes,
                ibc_origin_base_denom,
                ibc_origin_route,
                registrar_sk_hex,
                valid_until_unix,
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
                    anyhow::bail!("--dk-pub-hex is required for regulated assets");
                } else {
                    None
                };
                let ring_pk = ring_pk_hex
                    .as_ref()
                    .map(|hex_str| parse_decaf377_element(hex_str, "ring_pk_hex"))
                    .transpose()?;
                let registration_authority_vk = registration_authority_vk_hex
                    .as_ref()
                    .map(|hex_str| parse_spend_vk(hex_str, "registration_authority_vk_hex"))
                    .transpose()?;
                if is_regulated && registration_authority_vk.is_none() {
                    anyhow::bail!(
                        "--registration-authority-vk-hex is required for regulated assets"
                    );
                }
                let allowed_ibc_routes = Self::parse_ibc_routes(allowed_ibc_routes, is_regulated)?;
                let ibc_origin = Self::parse_ibc_origin(
                    ibc_origin_base_denom.as_deref(),
                    ibc_origin_route.as_deref(),
                    is_regulated,
                )?;
                let registrar_sk = parse_spend_sk(registrar_sk_hex, "registrar_sk_hex")?;
                let body = AssetRegistrationGrantBody {
                    asset_id,
                    is_regulated,
                    dk_pub,
                    threshold: *threshold,
                    allowed_ibc_routes,
                    ibc_origin,
                    ring_pk,
                    ring_id: ring_id.clone(),
                    policy_id: policy_id.clone(),
                    permission: permission.clone(),
                    resource: resource.clone(),
                    registration_authority_vk,
                    valid_until_unix: *valid_until_unix,
                };
                let grant = AssetRegistrationGrant {
                    signature: registrar_sk.sign(rand_core::OsRng, &body.signing_bytes()),
                    registrar_vk: VerificationKey::from(&registrar_sk),
                    body,
                };
                println!("{}", hex::encode(grant.encode_to_vec()));
                Ok(())
            }
            ComplianceCmd::SignUserGrant {
                asset_id,
                address,
                policy_id,
                registration_authority_sk_hex,
                valid_until_unix,
            } => {
                let asset_id = Self::parse_asset_id(asset_id)?;
                let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                let d = derive_compliance_scalar(b_d_fq);
                let leaf = ComplianceLeaf::new(address.clone(), asset_id, d);
                let mut nonce = vec![0u8; 16];
                rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
                let authority_sk = parse_spend_sk(
                    registration_authority_sk_hex,
                    "registration_authority_sk_hex",
                )?;
                let body = UserRegistrationGrantBody {
                    leaf,
                    policy_id: policy_id.clone(),
                    valid_until_unix: *valid_until_unix,
                    nonce,
                };
                let grant = UserRegistrationGrant {
                    signature: authority_sk.sign(rand_core::OsRng, &body.signing_bytes()),
                    body,
                };
                println!("{}", hex::encode(grant.encode_to_vec()));
                Ok(())
            }
            ComplianceCmd::DeriveSpendVk { signing_key_hex } => {
                let signing_key = parse_spend_sk(signing_key_hex, "signing_key_hex")?;
                let vk = VerificationKey::from(&signing_key);
                println!("{}", hex::encode(vk.to_bytes()));
                Ok(())
            }
            _ => anyhow::bail!("exec_sign_grant called on non-grant command"),
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
                registration_authority_vk_hex,
                allowed_ibc_routes,
                ibc_origin_base_denom,
                ibc_origin_route,
                asset_registration_grant_hex,
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
                let registration_authority_vk = registration_authority_vk_hex
                    .as_ref()
                    .map(|hex_str| parse_spend_vk(hex_str, "registration_authority_vk_hex"))
                    .transpose()?;
                if is_regulated && registration_authority_vk.is_none() {
                    anyhow::bail!(
                        "--registration-authority-vk-hex is required for regulated assets"
                    );
                }
                let allowed_ibc_routes =
                    Self::parse_ibc_routes(allowed_ibc_routes, is_regulated)?;
                let ibc_origin = Self::parse_ibc_origin(
                    ibc_origin_base_denom.as_deref(),
                    ibc_origin_route.as_deref(),
                    is_regulated,
                )?;
                let asset_registration_grant =
                    decode_asset_registration_grant(asset_registration_grant_hex)
                        .context("invalid --asset-registration-grant-hex")?;

                let msg = MsgRegisterAsset {
                    asset_id,
                    is_regulated,
                    dk_pub,
                    threshold: *threshold,
                    allowed_ibc_routes,
                    ibc_origin,
                    ring_pk,
                    ring_id: ring_id.clone(),
                    policy_id: policy_id.clone(),
                    permission: permission.clone(),
                    resource: resource.clone(),
                    registration_authority_vk,
                    asset_registration_grant: Some(asset_registration_grant),
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
                user_registration_grant_hex,
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
                let grant = decode_user_registration_grant(user_registration_grant_hex)
                    .context("invalid --user-registration-grant-hex")?;
                let msg = MsgRegisterUser {
                    leaf,
                    grant: Some(grant),
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

            ComplianceCmd::SignAssetGrant { .. }
            | ComplianceCmd::SignUserGrant { .. }
            | ComplianceCmd::DeriveSpendVk { .. } => anyhow::bail!(
                "offline compliance helper commands don't create transactions - use exec_sign_grant instead"
            ),
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

    fn parse_ibc_routes(route_specs: &[String], is_regulated: bool) -> Result<Vec<IbcRoute>> {
        if !is_regulated && !route_specs.is_empty() {
            anyhow::bail!("--allowed-ibc-route is only valid for regulated assets");
        }
        route_specs
            .iter()
            .map(|spec| Self::parse_ibc_route(spec))
            .collect()
    }

    fn parse_ibc_origin(
        base_denom: Option<&str>,
        route: Option<&str>,
        is_regulated: bool,
    ) -> Result<Option<IbcAssetOrigin>> {
        match (base_denom, route) {
            (None, None) => Ok(None),
            (Some(_), None) | (None, Some(_)) => {
                anyhow::bail!(
                    "--ibc-origin-base-denom and --ibc-origin-route must be provided together"
                )
            }
            (Some(base_denom), Some(route)) => {
                if !is_regulated {
                    anyhow::bail!("IBC origin is only valid for regulated assets");
                }
                Ok(Some(IbcAssetOrigin {
                    route: Self::parse_ibc_route(route)?,
                    base_denom: base_denom.to_string(),
                }))
            }
        }
    }

    fn parse_ibc_route(spec: &str) -> Result<IbcRoute> {
        let parts = spec.split(',').collect::<Vec<_>>();
        anyhow::ensure!(
            parts.len() == 3 && parts.iter().all(|part| !part.is_empty()),
            "IBC route must be <local_channel,connection_id,counterparty_channel>"
        );
        Ok(IbcRoute::transfer(parts[0], parts[1], parts[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multiple_ibc_routes_defaults_transfer_ports() {
        let specs = vec![
            "channel-0,connection-0,channel-7".to_string(),
            "channel-1,connection-1,channel-8".to_string(),
        ];

        let routes = ComplianceCmd::parse_ibc_routes(&specs, true).unwrap();

        assert_eq!(
            routes,
            vec![
                IbcRoute::transfer("channel-0", "connection-0", "channel-7"),
                IbcRoute::transfer("channel-1", "connection-1", "channel-8"),
            ]
        );
    }

    #[test]
    fn parse_ibc_origin_requires_base_denom_and_route() {
        assert!(ComplianceCmd::parse_ibc_origin(Some("ubank"), None, true).is_err());
        assert!(ComplianceCmd::parse_ibc_origin(
            None,
            Some("channel-0,connection-0,channel-7"),
            true
        )
        .is_err());

        let origin = ComplianceCmd::parse_ibc_origin(
            Some("ubank"),
            Some("channel-0,connection-0,channel-7"),
            true,
        )
        .unwrap()
        .unwrap();
        assert_eq!(origin.base_denom, "ubank");
        assert_eq!(
            origin.route,
            IbcRoute::transfer("channel-0", "connection-0", "channel-7")
        );
    }

    #[test]
    fn parse_ibc_route_rejects_chain_id_shape() {
        assert!(
            ComplianceCmd::parse_ibc_route("channel-0,connection-0,bankd-1,channel-7").is_err()
        );
    }

    #[test]
    fn parse_ibc_policy_args_reject_unregulated_assets() {
        let specs = vec!["channel-0,connection-0,channel-7".to_string()];
        assert!(ComplianceCmd::parse_ibc_routes(&specs, false).is_err());
        assert!(ComplianceCmd::parse_ibc_origin(
            Some("ubank"),
            Some("channel-0,connection-0,channel-7"),
            false
        )
        .is_err());
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

fn parse_spend_vk(hex_str: &str, label: &str) -> Result<VerificationKey<SpendAuth>> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid {label}: must be hex"))?;
    if bytes.len() != 32 {
        anyhow::bail!("{label} must be exactly 64 hex chars (32 bytes)");
    }
    VerificationKey::<SpendAuth>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("invalid {label} encoding"))
}

fn parse_spend_sk(hex_str: &str, label: &str) -> Result<SigningKey<SpendAuth>> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid {label}: must be hex"))?;
    if bytes.len() != 32 {
        anyhow::bail!("{label} must be exactly 64 hex chars (32 bytes)");
    }
    SigningKey::<SpendAuth>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("invalid {label} encoding"))
}

fn decode_asset_registration_grant(hex_str: &str) -> Result<AssetRegistrationGrant> {
    let bytes = hex::decode(hex_str).context("value must be hex-encoded protobuf bytes")?;
    AssetRegistrationGrant::decode(bytes.as_slice())
}

fn decode_user_registration_grant(hex_str: &str) -> Result<UserRegistrationGrant> {
    let bytes = hex::decode(hex_str).context("value must be hex-encoded protobuf bytes")?;
    UserRegistrationGrant::decode(bytes.as_slice())
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
