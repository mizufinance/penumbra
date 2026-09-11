use anyhow::{Context, Result};
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::structs::{
    AssetRegistrationGrant, AssetRegistrationGrantBody, IbcAssetOrigin, IbcRoute,
    UserRegistrationGrant, UserRegistrationGrantBody,
};
use shieldd_sdk_compliance::ComplianceLeaf;
use shieldd_sdk_keys::{ensure_nonidentity_spend_auth_key, Address};
use shieldd_sdk_proto::DomainType;

/// Compliance-related transaction commands.
#[derive(Debug, clap::Subcommand)]
pub enum ComplianceCmd {
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
        /// Daily undisclosed-volume limit, in base units.
        #[clap(long)]
        daily_volume_limit: Option<u128>,
        /// Orbis ring public key (hex, 64 chars = 32 bytes compressed).
        #[clap(long)]
        ring_pk_hex: Option<String>,
        /// Registered general audit key bundle, hex encoded.
        #[clap(long)]
        audit_keys_hex: Option<String>,
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
        /// Seizure-authority verification key for regulated note seizures, hex-encoded.
        #[clap(long)]
        seizure_authority_vk_hex: Option<String>,
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
        /// Shieldd address authorized by this grant.
        #[clap(long)]
        address: Address,
        /// Vera policy ID bound to this grant.
        #[clap(long)]
        policy_id: String,
        /// Orbis ring public key for the registered asset.
        #[clap(long)]
        ring_pk_hex: String,
        /// Registered person audit key bundle, hex encoded.
        #[clap(long)]
        audit_keys_hex: String,
        /// Orbis ring public key evaluated on the address diversified generator.
        #[clap(long)]
        rnk_dh_pk_hex: String,
        /// Poseidon commitment to the wallet-Orbis regulated nullifier key.
        #[clap(long)]
        rnk_commitment_hex: String,
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
    pub fn exec(&self) -> Result<()> {
        if matches!(self, Self::GenerateDk) {
            self.exec_generate_dk()
        } else {
            self.exec_sign_grant()
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
                daily_volume_limit,
                ring_pk_hex,
                audit_keys_hex,
                ring_id,
                policy_id,
                permission,
                resource,
                registration_authority_vk_hex,
                seizure_authority_vk_hex,
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
                require_regulated_orbis_config(
                    is_regulated,
                    ring_pk.as_ref(),
                    ring_id,
                    policy_id,
                    permission,
                    resource,
                )?;
                let registration_authority_vk = registration_authority_vk_hex
                    .as_ref()
                    .map(|hex_str| parse_spend_vk(hex_str, "registration_authority_vk_hex"))
                    .transpose()?;
                if is_regulated && registration_authority_vk.is_none() {
                    anyhow::bail!(
                        "--registration-authority-vk-hex is required for regulated assets"
                    );
                }
                let seizure_authority_vk = seizure_authority_vk_hex
                    .as_ref()
                    .map(|hex_str| parse_spend_vk(hex_str, "seizure_authority_vk_hex"))
                    .transpose()?;
                if is_regulated && seizure_authority_vk.is_none() {
                    anyhow::bail!("--seizure-authority-vk-hex is required for regulated assets");
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
                    audit_keys: audit_keys_hex
                        .as_ref()
                        .map(|encoded| {
                            shieldd_sdk_compliance::AuditKeys::from_bytes(&hex::decode(encoded)?)
                        })
                        .transpose()?,
                    dk_pub,
                    daily_volume_limit: *daily_volume_limit,
                    allowed_ibc_routes,
                    ibc_origin,
                    ring_pk,
                    ring_id: ring_id.clone(),
                    policy_id: policy_id.clone(),
                    permission: permission.clone(),
                    resource: resource.clone(),
                    registration_authority_vk,
                    seizure_authority_vk,
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
                ring_pk_hex,
                audit_keys_hex,
                rnk_dh_pk_hex,
                rnk_commitment_hex,
                registration_authority_sk_hex,
                valid_until_unix,
            } => {
                let asset_id = Self::parse_asset_id(asset_id)?;
                let ring_pk = parse_decaf377_element(ring_pk_hex, "ring_pk_hex")?;
                let rnk_dh_pk = parse_decaf377_element(rnk_dh_pk_hex, "rnk_dh_pk_hex")?;
                let rnk_commitment = parse_fq(rnk_commitment_hex, "rnk_commitment_hex")?;
                let leaf = ComplianceLeaf::registered(
                    address.clone(),
                    asset_id,
                    ring_pk,
                    rnk_dh_pk,
                    rnk_commitment,
                    shieldd_sdk_compliance::AuditKeys::from_bytes(&hex::decode(audit_keys_hex)?)?,
                )?;
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
    /// Helper to parse asset ID from string.
    /// Accepts either a full asset ID or a unit name like "shieldd" or "ushieldd".
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

    #[test]
    fn authorization_key_parsers_reject_identity() {
        let identity_sk = SigningKey::<SpendAuth>::from(decaf377::Fr::from(0u64));
        let identity_vk = VerificationKey::from(&identity_sk);

        assert!(
            parse_spend_vk(&hex::encode(identity_vk.to_bytes()), "test_vk").is_err(),
            "CLI verification-key parsing must reject identity"
        );
        assert!(
            parse_spend_sk(&hex::encode(decaf377::Fr::from(0u64).to_bytes()), "test_sk").is_err(),
            "CLI signing-key parsing must reject a key deriving identity"
        );
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

fn parse_fq(hex_str: &str, label: &str) -> Result<decaf377::Fq> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid {label}: must be hex"))?;
    if bytes.len() != 32 {
        anyhow::bail!("{label} must be exactly 64 hex chars (32 bytes)");
    }
    let bytes: [u8; 32] = bytes.try_into().unwrap();
    let value = decaf377::Fq::from_bytes_checked(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid {label} field encoding"))?;
    anyhow::ensure!(value != decaf377::Fq::from(0u64), "{label} must be nonzero");
    Ok(value)
}

fn require_regulated_orbis_config(
    is_regulated: bool,
    ring_pk: Option<&decaf377::Element>,
    ring_id: &str,
    policy_id: &str,
    permission: &str,
    resource: &str,
) -> Result<()> {
    if !is_regulated {
        return Ok(());
    }
    anyhow::ensure!(
        ring_pk.is_some(),
        "--ring-pk-hex is required for regulated assets"
    );
    for (flag, value) in [
        ("--ring-id", ring_id),
        ("--policy-id", policy_id),
        ("--permission", permission),
        ("--resource", resource),
    ] {
        anyhow::ensure!(!value.is_empty(), "{flag} is required for regulated assets");
    }
    Ok(())
}

fn parse_spend_vk(hex_str: &str, label: &str) -> Result<VerificationKey<SpendAuth>> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid {label}: must be hex"))?;
    if bytes.len() != 32 {
        anyhow::bail!("{label} must be exactly 64 hex chars (32 bytes)");
    }
    let key = VerificationKey::<SpendAuth>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("invalid {label} encoding"))?;
    ensure_nonidentity_spend_auth_key(&key, label)?;
    Ok(key)
}

fn parse_spend_sk(hex_str: &str, label: &str) -> Result<SigningKey<SpendAuth>> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid {label}: must be hex"))?;
    if bytes.len() != 32 {
        anyhow::bail!("{label} must be exactly 64 hex chars (32 bytes)");
    }
    let key = SigningKey::<SpendAuth>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("invalid {label} encoding"))?;
    ensure_nonidentity_spend_auth_key(&VerificationKey::from(&key), label)?;
    Ok(key)
}
