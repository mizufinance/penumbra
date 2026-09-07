#![deny(clippy::unwrap_used)]

use std::{collections::HashSet, env, fs, path::PathBuf, str::FromStr, time::Instant};

use anyhow::{anyhow, Context, Result};
use bech32::{ToBase32, Variant};
use clap::Parser;
use decaf377::{Element, Encoding, Fr};
use orbis_authn::JwtSigner;
use orbis_common::blockchain::{ChainConfig, TxSigner, VeraClient, TEST_ACCOUNT_HEX_KEY};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use shieldd_orbis_client::{AcpObjectRef, InlineDocumentInput, OrbisClient};
use shieldd_sdk_compliance::{
    compliance_derivation, decrypt_reencrypted_seed, decrypt_tier_bytes, parse_element,
    validate_decrypted_seed, PocOrbisAccess, PocOrbisAuditBundle, PocOrbisAuditPackage,
    PocOrbisSecretEnvelope, PocOrbisStoredAuditPackage, PocOrbisTier,
};
use shieldd_sdk_keys::Address;
use shieldd_sdk_shielded_pool::discovery::{Precision, RoutingSelector};

#[derive(Parser, Debug)]
#[clap(
    name = "orbis-audit",
    about = "Orbis-only compliance audit POC for current Shieldd transfer tiers"
)]
struct Args {
    #[clap(long)]
    input: Option<PathBuf>,
    #[clap(long)]
    output: PathBuf,
    #[clap(long)]
    descriptors_only: bool,
    /// Register the canonical subject and investigation objects from an enriched transfer bundle.
    #[clap(long)]
    register_bundle: Option<PathBuf>,
    /// Register canonical Orbis document IDs with Vera ACP in one transaction.
    #[clap(long)]
    register_objects: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterBundleInput {
    shieldd_tx_hash: String,
    bankd_tx_hash: String,
    height: u64,
    action_index: u32,
    output_index: u32,
    bundle: PocOrbisAuditBundle,
}

#[derive(Debug, Serialize)]
struct RegisterBundleResult {
    records: Vec<StoredAuditObject>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAuditObject {
    shieldd_tx_hash: String,
    action_index: u32,
    output_index: u32,
    access: PocOrbisAccess,
    tier: String,
    object_id: String,
    ring_id: String,
    policy_id: String,
    resource: String,
    permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    derivation: Option<String>,
    salt: String,
    timestamp: u64,
    effective_pk: String,
    transfer_c2: String,
    tier_ciphertext: String,
    package: PocOrbisAuditPackage,
    output_ref: StoredOutputRef,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredOutputRef {
    height: u64,
    bankd_tx_hash: String,
    action_index: u32,
    output_index: u32,
}

#[derive(Debug, Deserialize)]
struct AuditInput {
    request_id: String,
    #[serde(default)]
    subject_addresses: Vec<String>,
    #[serde(default = "default_precision")]
    routing_precision_bits: u8,
    valid_window: Option<TimestampRange>,
    #[serde(default)]
    objects: Vec<AuditObject>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct TimestampRange {
    start: u64,
    end: u64,
}

#[derive(Debug, Deserialize)]
struct AuditObject {
    output_ref: OutputRef,
    object_id: String,
    package: PocOrbisAuditPackage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterObjectsInput {
    objects: Vec<AcpObjectRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OutputRef {
    height: u64,
    bankd_tx_hash: String,
    action_index: u32,
    output_index: u32,
}

#[derive(Debug, Serialize)]
struct SubjectDescriptors {
    precision_bits: u8,
    addresses: Vec<SubjectDescriptor>,
}

#[derive(Debug, Serialize)]
struct SubjectDescriptor {
    address: String,
    derivation_hex: String,
    routing_prefix: u32,
}

#[derive(Debug, Serialize)]
struct AuditResult {
    request_id: String,
    reader_public_key_hash: String,
    reader_did: String,
    objects: Vec<ObjectResult>,
}

#[derive(Debug, Serialize)]
struct ObjectResult {
    output_ref: OutputRef,
    object_id: String,
    access: PocOrbisAccess,
    tier: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_stage: Option<&'static str>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(bundle) = args.register_bundle.as_ref() {
        anyhow::ensure!(
            args.input.is_none(),
            "--register-bundle conflicts with --input"
        );
        anyhow::ensure!(
            !args.descriptors_only,
            "--register-bundle conflicts with --descriptors-only"
        );
        anyhow::ensure!(
            args.register_objects.is_none(),
            "--register-bundle conflicts with --register-objects"
        );
        return register_bundle(bundle, &args.output).await;
    }
    if let Some(objects) = args.register_objects.as_ref() {
        anyhow::ensure!(
            args.input.is_none(),
            "--register-objects conflicts with --input"
        );
        anyhow::ensure!(
            !args.descriptors_only,
            "--register-objects conflicts with --descriptors-only"
        );
        return register_objects(objects, &args.output).await;
    }
    let input_path = args
        .input
        .as_ref()
        .context("--input is required unless an object registration mode is used")?;
    let input: AuditInput = serde_json::from_slice(
        &fs::read(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", input_path.display()))?;
    anyhow::ensure!(
        !input.request_id.trim().is_empty(),
        "request_id is required"
    );

    if args.descriptors_only {
        return write_json(&args.output, &derive_descriptors(&input)?);
    }
    let audit_objects = canonicalize_audit_objects(input.objects);
    anyhow::ensure!(
        !audit_objects.is_empty(),
        "at least one Orbis object is required"
    );
    if let Some(window) = input.valid_window {
        anyhow::ensure!(window.start <= window.end, "valid window is reversed");
    }

    let reader_sk = request_scalar(&input.request_id);
    let decrypt_sk = reader_sk;
    let reader_pk = Element::GENERATOR * reader_sk;
    let reader_pk_hex = hex::encode(reader_pk.vartime_compress().0);
    let reader_public_key_hash = hex::encode(sha2::Sha256::digest(reader_pk_hex.as_bytes()));
    let jwt_signer = JwtSigner::new();
    let vera = vera_client().await?;
    let orbis = OrbisClient::new(env_or("ORBIS_NODE1_ENDPOINT", "http://127.0.0.1:50051"))?;
    let valid_window = input.valid_window.map(|window| (window.start, window.end));

    let authorization_started = Instant::now();
    let grants = audit_objects
        .iter()
        .map(|object| AcpObjectRef {
            policy_id: object.package.policy_id.clone(),
            resource: object.package.resource.clone(),
            object_id: object.object_id.clone(),
        })
        .collect::<Vec<_>>();
    OrbisClient::authorize_reader(&vera, &grants, &jwt_signer.did_uri).await?;
    println!(
        "[orbis-audit] vera_authorization_ms={} objects={}",
        authorization_started.elapsed().as_millis(),
        grants.len()
    );

    let mut objects = Vec::with_capacity(audit_objects.len());
    let pre_started = Instant::now();
    for object in audit_objects {
        let result = audit_object(
            &orbis,
            &jwt_signer,
            &reader_pk_hex,
            &decrypt_sk,
            valid_window,
            object,
        )
        .await;
        objects.push(match result {
            Ok(result) => result,
            Err(error) => ObjectResult {
                output_ref: error.output_ref,
                object_id: error.object_id,
                access: error.access,
                tier: error.tier,
                status: "decrypt_no_match",
                field: None,
                value: None,
                identity_key: None,
                error: Some(error.source.to_string()),
                failed_stage: Some(error.stage),
            },
        });
    }
    println!(
        "[orbis-audit] orbis_pre_and_decrypt_ms={} objects={}",
        pre_started.elapsed().as_millis(),
        objects.len()
    );

    write_json(
        &args.output,
        &AuditResult {
            request_id: input.request_id,
            reader_public_key_hash,
            reader_did: jwt_signer.did_uri,
            objects,
        },
    )
}

async fn register_bundle(input_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    let input: RegisterBundleInput = serde_json::from_slice(
        &fs::read(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", input_path.display()))?;
    anyhow::ensure!(
        !input.shieldd_tx_hash.is_empty(),
        "shielddTxHash is required"
    );
    anyhow::ensure!(!input.bankd_tx_hash.is_empty(), "bankdTxHash is required");

    let tiers = [
        input.bundle.subject.sender_core,
        input.bundle.subject.sender_ext,
        input.bundle.subject.output_core,
        input.bundle.subject.output_ext,
        input.bundle.investigation.sender_core,
        input.bundle.investigation.sender_ext,
        input.bundle.investigation.output_core,
        input.bundle.investigation.output_ext,
    ];
    let mut records = Vec::with_capacity(tiers.len());
    let mut objects = Vec::with_capacity(tiers.len());
    for package in tiers.into_iter().filter(|package| {
        PocOrbisTier::from_label(&package.tier_label)
            .map(|tier| tier.is_canonical_for_access(package.access))
            .unwrap_or(true)
    }) {
        let object_id = OrbisClient::inline_document_object_id(&inline_document_input(&package))?;
        objects.push(AcpObjectRef {
            policy_id: package.policy_id.clone(),
            resource: package.resource.clone(),
            object_id: object_id.clone(),
        });
        records.push(StoredAuditObject {
            shieldd_tx_hash: input.shieldd_tx_hash.clone(),
            action_index: input.action_index,
            output_index: input.output_index,
            access: package.access,
            tier: package.tier_label.clone(),
            object_id,
            ring_id: package.ring_id.clone(),
            policy_id: package.policy_id.clone(),
            resource: package.resource.clone(),
            permission: package.permission.clone(),
            derivation: package.derivation.as_ref().map(hex::encode),
            salt: package.salt.clone(),
            timestamp: package.timestamp,
            effective_pk: hex::encode(&package.effective_pk),
            transfer_c2: hex::encode(&package.transfer_c2),
            tier_ciphertext: hex::encode(&package.tier_ciphertext),
            package,
            output_ref: StoredOutputRef {
                height: input.height,
                bankd_tx_hash: input.bankd_tx_hash.clone(),
                action_index: input.action_index,
                output_index: input.output_index,
            },
        });
    }
    let vera = vera_client().await?;
    let started = Instant::now();
    OrbisClient::register_objects(&vera, &objects).await?;
    println!(
        "[orbis-audit] vera_object_registration_ms={} objects={}",
        started.elapsed().as_millis(),
        objects.len()
    );
    write_json(output_path, &RegisterBundleResult { records })
}

async fn register_objects(input_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    let input: RegisterObjectsInput = serde_json::from_slice(
        &fs::read(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", input_path.display()))?;
    anyhow::ensure!(
        !input.objects.is_empty(),
        "at least one ACP object is required"
    );
    let vera = vera_client().await?;
    let started = Instant::now();
    OrbisClient::register_objects(&vera, &input.objects).await?;
    println!(
        "[orbis-audit] vera_object_registration_ms={} objects={}",
        started.elapsed().as_millis(),
        input.objects.len()
    );
    write_json(
        output_path,
        &serde_json::json!({ "registered": input.objects.len() }),
    )
}

/// Collapses redundant unrestricted amount packages while retaining backwards
/// compatibility with records that contain only the old `output_core` form.
fn canonicalize_audit_objects(objects: Vec<AuditObject>) -> Vec<AuditObject> {
    let canonical_amount_outputs = objects
        .iter()
        .filter(|object| {
            object.package.access == PocOrbisAccess::Investigation
                && PocOrbisTier::from_label(&object.package.tier_label)
                    == Some(PocOrbisTier::SenderCore)
        })
        .map(|object| output_key(&object.output_ref))
        .collect::<HashSet<_>>();

    objects
        .into_iter()
        .filter(|object| {
            PocOrbisTier::from_label(&object.package.tier_label)
                .map(|tier| tier.is_canonical_for_access(object.package.access))
                .unwrap_or(true)
                || !canonical_amount_outputs.contains(&output_key(&object.output_ref))
        })
        .collect()
}

fn output_key(output_ref: &OutputRef) -> (u64, String, u32, u32) {
    (
        output_ref.height,
        output_ref.bankd_tx_hash.clone(),
        output_ref.action_index,
        output_ref.output_index,
    )
}

fn inline_document_input(package: &PocOrbisAuditPackage) -> InlineDocumentInput {
    InlineDocumentInput {
        encrypted_document: package.encrypted_document.clone(),
        enc_cmt: package.enc_cmt.clone(),
        ring_id: package.ring_id.clone(),
        policy_id: package.policy_id.clone(),
        resource: package.resource.clone(),
        permission: package.permission.clone(),
        shared_point: package.shared_point.clone(),
        challenge: package.orbis_challenge.clone(),
        response: package.orbis_response.clone(),
        tier: Some(package.tier_label.clone()),
        timestamp: Some(package.timestamp),
    }
}

fn derive_descriptors(input: &AuditInput) -> Result<SubjectDescriptors> {
    anyhow::ensure!(
        !input.subject_addresses.is_empty(),
        "subject_addresses are required for descriptor export"
    );
    let precision = Precision::new(input.routing_precision_bits)?;
    let addresses = input
        .subject_addresses
        .iter()
        .map(|raw| {
            let address =
                Address::from_str(raw).with_context(|| format!("invalid subject address {raw}"))?;
            Ok(SubjectDescriptor {
                address: raw.clone(),
                derivation_hex: hex::encode(compliance_derivation(&address)),
                routing_prefix: RoutingSelector::for_address(&address, precision).prefix,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SubjectDescriptors {
        precision_bits: precision.bits(),
        addresses,
    })
}

struct ObjectAuditError {
    output_ref: OutputRef,
    object_id: String,
    access: PocOrbisAccess,
    tier: String,
    stage: &'static str,
    source: anyhow::Error,
}

#[allow(clippy::too_many_arguments)]
async fn audit_object(
    orbis: &OrbisClient,
    jwt_signer: &JwtSigner,
    reader_pk_hex: &str,
    decrypt_sk: &Fr,
    valid_window: Option<(u64, u64)>,
    object: AuditObject,
) -> std::result::Result<ObjectResult, ObjectAuditError> {
    let output_ref = object.output_ref.clone();
    let object_id = object.object_id.clone();
    let access = object.package.access;
    let tier = object.package.tier_label.clone();
    let mut stage = "orbis_pre";
    let result = async {
        let retained_package = PocOrbisStoredAuditPackage::from(&object.package);
        let pre = orbis
            .start_pre(
                reader_pk_hex,
                &object.object_id,
                object.package.derivation.clone(),
                Some(&object.package.salt),
                valid_window,
                inline_document_input(&object.package),
                jwt_signer,
            )
            .await?;
        stage = "decrypt";
        let xnc_cmt = parse_element(&hex::decode(&pre.xnc_cmt_hex)?, "xnc_cmt")?;
        let secret = PocOrbisSecretEnvelope {
            enc_cmt: pre.secret.enc_cmt,
            encrypted_data: pre.secret.encrypted_data,
            nonce: pre.secret.nonce,
        };
        let seed = decrypt_reencrypted_seed(&retained_package, decrypt_sk, &xnc_cmt, &secret)?;
        validate_decrypted_seed(&retained_package, seed)?;
        let (field, plaintext_len) = match object.package.tier_label.as_str() {
            "sender_core" | "output_core" => ("amount", 16),
            "sender_ext" => ("receiver", 64),
            "output_ext" => ("sender", 64),
            tier => return Err(anyhow!("unknown Orbis tier {tier}")),
        };
        let plaintext = decrypt_tier_bytes(&object.package.tier_ciphertext, seed, plaintext_len);
        let (value, identity_key) = if field == "amount" {
            let bytes: [u8; 16] = plaintext
                .try_into()
                .map_err(|_| anyhow!("amount plaintext must be 16 bytes"))?;
            (u128::from_le_bytes(bytes).to_string(), None)
        } else {
            (
                format_compliance_identity(&plaintext)?,
                Some(hex::encode(&plaintext[32..64])),
            )
        };
        Ok(ObjectResult {
            output_ref: object.output_ref,
            object_id: object.object_id,
            access: object.package.access,
            tier: object.package.tier_label,
            status: "decrypt_succeeded",
            field: Some(field),
            value: Some(value),
            identity_key,
            error: None,
            failed_stage: None,
        })
    }
    .await;
    result.map_err(|source| ObjectAuditError {
        output_ref,
        object_id,
        access,
        tier,
        stage,
        source,
    })
}

fn format_compliance_identity(plaintext: &[u8]) -> Result<String> {
    anyhow::ensure!(
        plaintext.len() == 64,
        "decrypted compliance identity must be 64 bytes"
    );
    let diversified_generator: [u8; 32] = plaintext[..32]
        .try_into()
        .context("compliance identity generator must be 32 bytes")?;
    Encoding(diversified_generator)
        .vartime_decompress()
        .map_err(|_| anyhow!("compliance identity generator is invalid"))?;
    bech32::encode("shielddcomp", plaintext.to_base32(), Variant::Bech32m)
        .context("failed to encode compliance identity")
}

fn request_scalar(request_id: &str) -> Fr {
    let mut hasher = Sha512::new();
    hasher.update(b"bankd-orbis-audit-reader");
    hasher.update(request_id.as_bytes());
    let mut scalar = Fr::from_le_bytes_mod_order(&hasher.finalize());
    if scalar == Fr::from(0u64) {
        scalar = Fr::from(1u64);
    }
    scalar
}

async fn vera_client() -> Result<VeraClient> {
    let config = ChainConfig::builder()
        .chain_id(Some(env_or("ORBIS_VERA_CHAIN_ID", "vera-localnet")))
        .rpc_url(Some(env_or("ORBIS_VERA_RPC", "http://127.0.0.1:26657")))
        .rest_url(Some(env_or("ORBIS_VERA_REST", "http://127.0.0.1:1317")))
        .grpc_url(Some(env_or("ORBIS_VERA_GRPC", "http://127.0.0.1:9090")))
        .denom(Some(env_or("ORBIS_VERA_DENOM", "uopen")))
        .build();
    let signer = TxSigner::from_hex_key(TEST_ACCOUNT_HEX_KEY, config.clone())
        .map_err(|error| anyhow!("failed to create Vera demo signer: {error}"))?;
    VeraClient::with_signer(config, signer)
        .await
        .map_err(|error| anyhow!("failed to connect to Vera: {error}"))
}

fn write_json(path: &PathBuf, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn default_precision() -> u8 {
    12
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit_object(height: u64, access: PocOrbisAccess, tier: PocOrbisTier) -> AuditObject {
        AuditObject {
            output_ref: OutputRef {
                height,
                bankd_tx_hash: format!("tx-{height}"),
                action_index: 0,
                output_index: 0,
            },
            object_id: format!("object-{}", tier.label()),
            package: PocOrbisAuditPackage {
                access,
                derivation: None,
                ring_id: String::new(),
                policy_id: String::new(),
                resource: String::new(),
                permission: String::new(),
                tier_label: tier.label().to_owned(),
                timestamp: 0,
                salt: String::new(),
                encrypted_document: br#"{"enc_cmt":[],"encrypted_data":[],"nonce":[]}"#.to_vec(),
                enc_cmt: Vec::new(),
                shared_point: Vec::new(),
                orbis_challenge: Vec::new(),
                orbis_response: Vec::new(),
                effective_pk: Vec::new(),
                metadata_hash: Vec::new(),
                transfer_epk: Vec::new(),
                transfer_c2: Vec::new(),
                transfer_seed_binding: Vec::new(),
                tier_ciphertext: Vec::new(),
            },
        }
    }

    #[test]
    fn reader_key_is_request_scoped_and_deterministic() {
        let first = request_scalar("request-1");
        assert_eq!(first, request_scalar("request-1"));
        assert_ne!(first, request_scalar("request-2"));
    }

    #[test]
    fn reversed_window_is_observable_before_network_work() {
        let window = TimestampRange { start: 20, end: 10 };
        assert!(window.start > window.end);
    }

    #[test]
    fn extended_identity_has_an_unambiguous_non_payment_prefix() {
        let mut plaintext = Vec::from(Element::GENERATOR.vartime_compress().0);
        plaintext.extend_from_slice(&[7u8; 32]);
        let identity = format_compliance_identity(&plaintext).expect("valid identity");
        assert!(identity.starts_with("shielddcomp1"));
        assert_ne!(
            identity.split_once('1').map(|(prefix, _)| prefix),
            Some("shieldd")
        );
    }

    #[test]
    fn canonicalization_only_removes_paired_unrestricted_output_core() {
        let objects = vec![
            audit_object(1, PocOrbisAccess::Investigation, PocOrbisTier::SenderCore),
            audit_object(1, PocOrbisAccess::Investigation, PocOrbisTier::OutputCore),
            audit_object(2, PocOrbisAccess::Investigation, PocOrbisTier::OutputCore),
            audit_object(3, PocOrbisAccess::Subject, PocOrbisTier::SenderCore),
            audit_object(3, PocOrbisAccess::Subject, PocOrbisTier::OutputCore),
        ];

        let canonical = canonicalize_audit_objects(objects);
        assert_eq!(canonical.len(), 4);
        assert!(canonical.iter().any(|object| {
            object.output_ref.height == 1 && object.package.tier_label == "sender_core"
        }));
        assert!(canonical.iter().any(|object| {
            object.output_ref.height == 2 && object.package.tier_label == "output_core"
        }));
        assert_eq!(
            canonical
                .iter()
                .filter(|object| object.output_ref.height == 3)
                .count(),
            2
        );
    }
}
