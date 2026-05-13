use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use decaf377::{Element, Fq, Fr};
use penumbra_orbis_client::OrbisClient;
use penumbra_sdk_compliance::{
    decrypt_orbis_reencrypted_seed, decrypt_tier_bytes, OrbisEncryptedSeedUploadPackage,
    TransferComplianceCiphertext, TransferOrbisUploadBundle,
};
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tonic::transport::Channel;
use url::Url;

#[derive(Parser, Debug)]
#[clap(
    name = "orbis-audit",
    about = "Compliance audit via Orbis PRE for transfer ciphertexts"
)]
struct Args {
    #[clap(long)]
    input: PathBuf,

    #[clap(long)]
    dk_hex: String,

    #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
    node: Url,

    #[clap(long, default_value = "/tmp/alice-audit.json")]
    output: PathBuf,

    #[clap(long, default_value = "default")]
    tier: String,

    #[clap(long)]
    sender_address: Option<String>,

    #[clap(long = "subject-address")]
    subject_addresses: Vec<String>,

    #[clap(long)]
    orbis_endpoint: String,

    #[clap(long = "known-address")]
    known_addresses: Vec<String>,

    #[clap(long)]
    timings_json: Option<PathBuf>,

    #[clap(long)]
    object_cache: Option<PathBuf>,

    #[clap(long)]
    prepare_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ScanOutput {
    scan_info: serde_json::Value,
    detected: Vec<DetectedTxRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DetectedTxRef {
    height: u64,
    tx_hash: String,
    action_index: usize,
    #[serde(default)]
    output_index: usize,
    asset_id: String,
    is_flagged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuditEntry {
    height: u64,
    tx_hash: String,
    action_index: usize,
    #[serde(default)]
    output_index: usize,
    amount: String,
    self_address: String,
    counterparty: String,
    decrypted_via: String,
}

#[derive(Clone)]
struct PackageObjectContext {
    object_id: String,
    package: OrbisEncryptedSeedUploadPackage,
    cache_key: String,
    from_cache: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ObjectCache {
    objects: HashMap<String, CachedObject>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedObject {
    object_id: String,
}

#[derive(Clone)]
struct AuditContext<'a> {
    cli: &'a OrbisClient,
    dk: &'a Fr,
    dk_pub: &'a Element,
    subject_transmission_key_hex: &'a str,
    subject_b_d_bytes: &'a [u8; 32],
    known_transmission_keys: &'a HashSet<String>,
    tier_mode: &'a str,
}

#[derive(Clone, Debug)]
struct AddressData {
    transmission_key_hex: String,
}

#[derive(Clone, Debug)]
struct SubjectData {
    transmission_key_hex: String,
    subject_b_d_bytes: [u8; 32],
}

#[derive(Clone, Debug, Default, Serialize)]
struct AuditTimings {
    tier: String,
    candidate_refs: u64,
    skipped_flagged: u64,
    no_ciphertext: u64,
    transaction_fetch_ms: u128,
    ciphertext_extraction_ms: u128,
    package_store_ms: u128,
    object_registration_ms: u128,
    relationship_setup_ms: u128,
    start_pre_ms: u128,
    seed_decrypt_ms: u128,
    amount_decrypt_ms: u128,
    address_decrypt_ms: u128,
    issuer_db_update_ms: u128,
    pre_calls: u64,
    total_pre_ms: u128,
    avg_pre_ms: u128,
    p95_pre_ms: u128,
    total_ms: u128,
    decrypted: u64,
    object_cache_hits: u64,
    object_cache_misses: u64,
    object_cache_stale: u64,
    subject_mismatch: u64,
    #[serde(skip)]
    pre_call_samples_ms: Vec<u128>,
}

#[derive(Clone, Debug)]
enum TransferMatch {
    Sender {
        amount: Amount,
        receiver: AddressData,
    },
    Receiver {
        amount: Amount,
        sender: AddressData,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let tier_mode = match args.tier.as_str() {
        "default" | "extension" => args.tier.as_str(),
        other => anyhow::bail!("--tier must be 'default' or 'extension', got '{other}'"),
    };

    let dk = parse_fr(&args.dk_hex, "DK")?;
    let dk_pub = Element::GENERATOR * dk;

    let subjects = parse_subjects(&args)?;

    let mut known_transmission_keys = HashSet::new();
    for subject in &subjects {
        known_transmission_keys.insert(subject.transmission_key_hex.clone());
    }
    for address in &args.known_addresses {
        let address: Address = address
            .parse()
            .with_context(|| format!("failed to parse --known-address {address}"))?;
        known_transmission_keys.insert(hex::encode(address.transmission_key().0));
    }

    let cli = OrbisClient::new(args.orbis_endpoint.clone());
    eprintln!(
        "orbis-audit: targets={}...",
        subjects
            .iter()
            .map(|subject| &subject.transmission_key_hex[..16])
            .collect::<Vec<_>>()
            .join(",")
    );

    let file = File::open(&args.input).context("failed to open input file")?;
    let reader = BufReader::new(file);
    let scan: ScanOutput = serde_json::from_reader(reader).context("failed to parse scan JSON")?;
    eprintln!(
        "orbis-audit: Processing {} detected transactions",
        scan.detected.len()
    );

    let channel = connect_to_node(&args.node).await?;
    let total_started = Instant::now();
    let mut object_cache = load_object_cache(args.object_cache.as_ref())?;
    let mut timings = AuditTimings {
        tier: args.tier.clone(),
        ..Default::default()
    };
    let mut results = Vec::new();
    let mut attempted = 0u64;
    let mut decrypted = 0u64;
    let mut no_ciphertext = 0u64;

    for tx_ref in &scan.detected {
        if tx_ref.is_flagged {
            timings.skipped_flagged += 1;
            continue;
        }
        attempted += subjects.len() as u64;
        timings.candidate_refs += subjects.len() as u64;
        let started = Instant::now();
        let transactions = fetch_transactions(channel.clone(), tx_ref.height).await?;
        timings.transaction_fetch_ms += started.elapsed().as_millis();

        for tx in &transactions {
            let Some(body) = tx.body.as_ref() else {
                continue;
            };
            if tx_ref.action_index >= body.actions.len() {
                continue;
            }

            let action = &body.actions[tx_ref.action_index];
            let started = Instant::now();
            let Some((ct, bundle)) = extract_transfer_data(action, tx_ref.output_index) else {
                timings.ciphertext_extraction_ms += started.elapsed().as_millis();
                no_ciphertext += 1;
                timings.no_ciphertext += 1;
                continue;
            };
            timings.ciphertext_extraction_ms += started.elapsed().as_millis();

            for subject in &subjects {
                let ctx = AuditContext {
                    cli: &cli,
                    dk: &dk,
                    dk_pub: &dk_pub,
                    subject_transmission_key_hex: &subject.transmission_key_hex,
                    subject_b_d_bytes: &subject.subject_b_d_bytes,
                    known_transmission_keys: &known_transmission_keys,
                    tier_mode,
                };
                if args.prepare_only {
                    prepare_transfer(&bundle, &ctx, &mut timings, &mut object_cache).await?;
                    continue;
                }
                if let Some(entry) =
                    audit_transfer(tx_ref, &ct, &bundle, &ctx, &mut timings, &mut object_cache)
                        .await?
                {
                    decrypted += 1;
                    results.push(entry);
                }
            }
        }
    }

    let json = serde_json::to_string_pretty(&results)?;
    let mut out_file = File::create(&args.output)?;
    out_file.write_all(json.as_bytes())?;

    eprintln!(
        "orbis-audit: Decrypted {}/{} non-flagged transfers (tier={}).",
        decrypted, attempted, args.tier
    );
    if no_ciphertext > 0 {
        eprintln!(
            "orbis-audit: {} actions had no compliance ciphertext.",
            no_ciphertext
        );
    }
    timings.decrypted = decrypted;
    timings.total_ms = total_started.elapsed().as_millis();
    timings.pre_calls = timings.pre_call_samples_ms.len() as u64;
    timings.total_pre_ms = timings.pre_call_samples_ms.iter().sum();
    timings.avg_pre_ms = if timings.pre_calls == 0 {
        0
    } else {
        timings.total_pre_ms / timings.pre_calls as u128
    };
    timings.p95_pre_ms = percentile(&timings.pre_call_samples_ms, 95);
    if let Some(path) = &args.timings_json {
        let json = serde_json::to_string_pretty(&timings)?;
        let mut out_file = File::create(path)?;
        out_file.write_all(json.as_bytes())?;
    }
    save_object_cache(args.object_cache.as_ref(), &object_cache)?;
    eprintln!(
        "orbis-audit: PRE calls={} total_pre_ms={} avg_pre_ms={} p95_pre_ms={} object_cache_hits={} object_cache_misses={} object_cache_stale={}",
        timings.pre_calls,
        timings.total_pre_ms,
        timings.avg_pre_ms,
        timings.p95_pre_ms,
        timings.object_cache_hits,
        timings.object_cache_misses,
        timings.object_cache_stale
    );
    println!("Results saved to: {}", args.output.display());

    Ok(())
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let mut values = samples.to_vec();
    values.sort_unstable();
    let index = ((values.len() - 1) * percentile) / 100;
    values[index]
}

fn parse_subjects(args: &Args) -> Result<Vec<SubjectData>> {
    let mut subject_addresses = args.subject_addresses.clone();
    if let Some(sender_address) = &args.sender_address {
        subject_addresses.push(sender_address.clone());
    }
    if subject_addresses.is_empty() {
        anyhow::bail!("at least one --subject-address or --sender-address is required");
    }

    let mut seen = HashSet::new();
    let mut subjects = Vec::new();
    for subject_address in subject_addresses {
        let address: Address = subject_address
            .parse()
            .with_context(|| format!("failed to parse subject address {subject_address}"))?;
        let transmission_key_hex = hex::encode(address.transmission_key().0);
        if seen.insert(transmission_key_hex.clone()) {
            let subject_b_d_bytes = address
                .diversified_generator()
                .vartime_compress_to_field()
                .to_bytes();
            subjects.push(SubjectData {
                transmission_key_hex,
                subject_b_d_bytes,
            });
        }
    }
    Ok(subjects)
}

async fn audit_transfer(
    tx_ref: &DetectedTxRef,
    ct: &TransferComplianceCiphertext,
    bundle: &TransferOrbisUploadBundle,
    ctx: &AuditContext<'_>,
    timings: &mut AuditTimings,
    object_cache: &mut ObjectCache,
) -> Result<Option<AuditEntry>> {
    if let Some(candidate) = try_receiver_match(ct, bundle, ctx, timings, object_cache).await? {
        return Ok(Some(candidate_to_entry(tx_ref, candidate, ctx)));
    }
    if let Some(candidate) = try_sender_match(ct, bundle, ctx, timings, object_cache).await? {
        return Ok(Some(candidate_to_entry(tx_ref, candidate, ctx)));
    }
    Ok(None)
}

async fn prepare_transfer(
    bundle: &TransferOrbisUploadBundle,
    ctx: &AuditContext<'_>,
    timings: &mut AuditTimings,
    object_cache: &mut ObjectCache,
) -> Result<()> {
    let TransferOrbisUploadBundle {
        output_core,
        output_ext,
        sender_core,
        sender_ext,
    } = bundle.clone();

    if output_core.derivation_bytes() == *ctx.subject_b_d_bytes {
        let ring_id = output_core.ring_id.clone();
        ensure_package_object(ctx.cli, &ring_id, output_core, timings, object_cache).await?;
        ensure_package_object(ctx.cli, &ring_id, output_ext, timings, object_cache).await?;
    } else {
        timings.subject_mismatch += 1;
    }

    if sender_core.derivation_bytes() == *ctx.subject_b_d_bytes {
        let ring_id = sender_core.ring_id.clone();
        ensure_package_object(ctx.cli, &ring_id, sender_core, timings, object_cache).await?;
        ensure_package_object(ctx.cli, &ring_id, sender_ext, timings, object_cache).await?;
    } else {
        timings.subject_mismatch += 1;
    }

    Ok(())
}

async fn try_receiver_match(
    ct: &TransferComplianceCiphertext,
    bundle: &TransferOrbisUploadBundle,
    ctx: &AuditContext<'_>,
    timings: &mut AuditTimings,
    object_cache: &mut ObjectCache,
) -> Result<Option<TransferMatch>> {
    let ring_id = bundle.output_core.ring_id.clone();
    let TransferOrbisUploadBundle {
        output_core,
        output_ext,
        ..
    } = bundle.clone();
    if output_core.derivation_bytes() != *ctx.subject_b_d_bytes {
        timings.subject_mismatch += 1;
        return Ok(None);
    }
    let output_core_seed = pre_package_seed(
        ctx.cli,
        &ring_id,
        output_core,
        ctx.dk,
        ctx.dk_pub,
        timings,
        object_cache,
    )
    .await?;
    let started = Instant::now();
    let amount = decrypt_amount_with_seed(output_core_seed, &ct.encrypted_output_core)?;
    timings.amount_decrypt_ms += started.elapsed().as_millis();

    let output_ext_seed = pre_package_seed(
        ctx.cli,
        &ring_id,
        output_ext,
        ctx.dk,
        ctx.dk_pub,
        timings,
        object_cache,
    )
    .await?;
    let started = Instant::now();
    let sender = match decrypt_address_with_seed(output_ext_seed, &ct.encrypted_output_ext) {
        Ok(sender) => {
            timings.address_decrypt_ms += started.elapsed().as_millis();
            sender
        }
        Err(_) => {
            timings.address_decrypt_ms += started.elapsed().as_millis();
            return Ok(None);
        }
    };

    if !ctx
        .known_transmission_keys
        .contains(&sender.transmission_key_hex)
    {
        return Ok(None);
    }

    Ok(Some(TransferMatch::Receiver { amount, sender }))
}

async fn try_sender_match(
    ct: &TransferComplianceCiphertext,
    bundle: &TransferOrbisUploadBundle,
    ctx: &AuditContext<'_>,
    timings: &mut AuditTimings,
    object_cache: &mut ObjectCache,
) -> Result<Option<TransferMatch>> {
    let ring_id = bundle.sender_core.ring_id.clone();
    let TransferOrbisUploadBundle {
        sender_core,
        sender_ext,
        ..
    } = bundle.clone();
    if sender_core.derivation_bytes() != *ctx.subject_b_d_bytes {
        timings.subject_mismatch += 1;
        return Ok(None);
    }
    let sender_core_seed = pre_package_seed(
        ctx.cli,
        &ring_id,
        sender_core,
        ctx.dk,
        ctx.dk_pub,
        timings,
        object_cache,
    )
    .await?;
    let started = Instant::now();
    let amount = decrypt_amount_with_seed(sender_core_seed, &ct.encrypted_sender_core)?;
    timings.amount_decrypt_ms += started.elapsed().as_millis();

    let sender_ext_seed = pre_package_seed(
        ctx.cli,
        &ring_id,
        sender_ext,
        ctx.dk,
        ctx.dk_pub,
        timings,
        object_cache,
    )
    .await?;
    let started = Instant::now();
    let receiver = match decrypt_address_with_seed(sender_ext_seed, &ct.encrypted_sender_ext) {
        Ok(receiver) => {
            timings.address_decrypt_ms += started.elapsed().as_millis();
            receiver
        }
        Err(_) => {
            timings.address_decrypt_ms += started.elapsed().as_millis();
            return Ok(None);
        }
    };

    if !ctx
        .known_transmission_keys
        .contains(&receiver.transmission_key_hex)
    {
        return Ok(None);
    }

    Ok(Some(TransferMatch::Sender { amount, receiver }))
}

fn candidate_to_entry(
    tx_ref: &DetectedTxRef,
    candidate: TransferMatch,
    ctx: &AuditContext<'_>,
) -> AuditEntry {
    match (ctx.tier_mode, candidate) {
        ("default", TransferMatch::Receiver { amount, .. })
        | ("default", TransferMatch::Sender { amount, .. }) => AuditEntry {
            height: tx_ref.height,
            tx_hash: tx_ref.tx_hash.clone(),
            action_index: tx_ref.action_index,
            output_index: tx_ref.output_index,
            amount: amount.value().to_string(),
            self_address: ctx.subject_transmission_key_hex.to_string(),
            counterparty: String::new(),
            decrypted_via: "core".to_string(),
        },
        ("extension", TransferMatch::Receiver { amount, sender }) => AuditEntry {
            height: tx_ref.height,
            tx_hash: tx_ref.tx_hash.clone(),
            action_index: tx_ref.action_index,
            output_index: tx_ref.output_index,
            amount: amount.value().to_string(),
            self_address: ctx.subject_transmission_key_hex.to_string(),
            counterparty: sender.transmission_key_hex,
            decrypted_via: "ext".to_string(),
        },
        ("extension", TransferMatch::Sender { amount, receiver }) => AuditEntry {
            height: tx_ref.height,
            tx_hash: tx_ref.tx_hash.clone(),
            action_index: tx_ref.action_index,
            output_index: tx_ref.output_index,
            amount: amount.value().to_string(),
            self_address: ctx.subject_transmission_key_hex.to_string(),
            counterparty: receiver.transmission_key_hex,
            decrypted_via: "ext".to_string(),
        },
        _ => unreachable!("tier already validated"),
    }
}

fn decrypt_amount_with_seed(seed: Fq, encrypted: &[u8]) -> Result<Amount> {
    let plaintext = decrypt_tier_bytes(encrypted, seed, 16);
    let amount_bytes: [u8; 16] = plaintext[..16]
        .try_into()
        .context("transfer amount plaintext must be 16 bytes")?;
    Ok(Amount::from_le_bytes(amount_bytes))
}

fn decrypt_address_with_seed(seed: Fq, encrypted: &[u8]) -> Result<AddressData> {
    let plaintext = decrypt_tier_bytes(encrypted, seed, 64);
    let diversified_generator_bytes: [u8; 32] = plaintext[..32]
        .try_into()
        .context("transfer address diversified generator must be 32 bytes")?;
    let transmission_key: [u8; 32] = plaintext[32..64]
        .try_into()
        .context("transfer address transmission key must be 32 bytes")?;
    decaf377::Encoding(diversified_generator_bytes)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid transfer address diversified generator"))?;
    Ok(AddressData {
        transmission_key_hex: hex::encode(transmission_key),
    })
}

fn load_object_cache(path: Option<&PathBuf>) -> Result<ObjectCache> {
    let Some(path) = path else {
        return Ok(ObjectCache::default());
    };
    if !path.exists() {
        return Ok(ObjectCache::default());
    }
    let file = File::open(path)
        .with_context(|| format!("failed to open object cache {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("failed to parse object cache {}", path.display()))
}

fn save_object_cache(path: Option<&PathBuf>, cache: &ObjectCache) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create object cache dir {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(cache)?;
    let mut file = File::create(path)
        .with_context(|| format!("failed to write object cache {}", path.display()))?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

fn package_cache_key(ring_id: &str, package: &OrbisEncryptedSeedUploadPackage) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(ring_id.as_bytes());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(package)?);
    Ok(hex::encode(hasher.finalize()))
}

async fn ensure_package_object(
    cli: &OrbisClient,
    ring_id: &str,
    package: OrbisEncryptedSeedUploadPackage,
    timings: &mut AuditTimings,
    object_cache: &mut ObjectCache,
) -> Result<PackageObjectContext> {
    package.validate()?;
    let cache_key = package_cache_key(ring_id, &package)?;
    if let Some(cached) = object_cache.objects.get(&cache_key) {
        timings.object_cache_hits += 1;
        return Ok(PackageObjectContext {
            object_id: cached.object_id.clone(),
            package,
            cache_key,
            from_cache: true,
        });
    }

    timings.object_cache_misses += 1;
    let started = Instant::now();
    let stored = cli.store_encrypted_seed_package(ring_id, &package).await?;
    timings.package_store_ms += started.elapsed().as_millis();
    let started = Instant::now();
    cli.register_object(&package.policy_id, &package.resource, &stored.object_id)
        .await?;
    timings.object_registration_ms += started.elapsed().as_millis();
    let started = Instant::now();
    cli.set_relationship(&package.policy_id, &package.resource, &stored.object_id)
        .await?;
    timings.relationship_setup_ms += started.elapsed().as_millis();
    object_cache.objects.insert(
        cache_key.clone(),
        CachedObject {
            object_id: stored.object_id.clone(),
        },
    );
    Ok(PackageObjectContext {
        object_id: stored.object_id.clone(),
        package,
        cache_key: cache_key.clone(),
        from_cache: false,
    })
}

async fn pre_package_seed(
    cli: &OrbisClient,
    ring_id: &str,
    package: OrbisEncryptedSeedUploadPackage,
    reader_sk: &Fr,
    reader_pk: &Element,
    timings: &mut AuditTimings,
    object_cache: &mut ObjectCache,
) -> Result<Fq> {
    let mut object = ensure_package_object(cli, ring_id, package, timings, object_cache).await?;
    let started = Instant::now();
    let mut pre_result = cli
        .start_pre(
            &hex::encode(reader_pk.vartime_compress().0),
            &object.object_id,
            &hex::encode(object.package.derivation_bytes()),
            Some(&object.package.salt),
            Some(object.package.timestamp),
        )
        .await;
    if pre_result.is_err() && object.from_cache {
        timings.object_cache_stale += 1;
        object_cache.objects.remove(&object.cache_key);
        object = ensure_package_object(cli, ring_id, object.package.clone(), timings, object_cache)
            .await?;
        pre_result = cli
            .start_pre(
                &hex::encode(reader_pk.vartime_compress().0),
                &object.object_id,
                &hex::encode(object.package.derivation_bytes()),
                Some(&object.package.salt),
                Some(object.package.timestamp),
            )
            .await;
    }
    let pre = pre_result?;
    let pre_ms = started.elapsed().as_millis();
    timings.start_pre_ms += pre_ms;
    timings.pre_call_samples_ms.push(pre_ms);

    let started = Instant::now();
    let xnc_bytes = hex::decode(&pre.xnc_cmt_hex).context("invalid xnc_cmt hex from PRE")?;
    let xnc_arr: [u8; 32] = xnc_bytes
        .try_into()
        .map_err(|_| anyhow!("xnc_cmt should be 32 bytes"))?;
    let xnc_cmt = decaf377::Encoding(xnc_arr)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid xnc_cmt curve point"))?;
    let seed = decrypt_orbis_reencrypted_seed(&object.package, reader_sk, &xnc_cmt, &pre.secret)?;
    timings.seed_decrypt_ms += started.elapsed().as_millis();
    Ok(seed)
}

fn parse_fr(hex_str: &str, label: &str) -> Result<Fr> {
    let bytes = hex::decode(hex_str).context(format!("invalid hex for {label}"))?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "{label} must be 32 bytes (64 hex chars), got {}",
            bytes.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Fr::from_le_bytes_mod_order(&arr))
}

fn extract_transfer_data(
    action: &penumbra_sdk_proto::core::transaction::v1::Action,
    output_index: usize,
) -> Option<(TransferComplianceCiphertext, TransferOrbisUploadBundle)> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action as ActionEnum;

    let ActionEnum::Transfer(transfer) = action.action.as_ref()? else {
        return None;
    };
    let body = transfer.body.as_ref()?;
    let output = body.outputs.get(output_index)?;
    if output.compliance_ciphertext.is_empty() {
        return None;
    }

    let ct = match TransferComplianceCiphertext::from_bytes(&output.compliance_ciphertext) {
        Ok(ct) => ct,
        Err(error) => {
            eprintln!(
                "orbis-audit: ciphertext deserialization failed ({} bytes): {}",
                output.compliance_ciphertext.len(),
                error
            );
            return None;
        }
    };
    let bundle = match TransferOrbisUploadBundle::from_bytes(&output.orbis_upload_bundle) {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!(
                "orbis-audit: upload bundle deserialization failed ({} bytes): {}",
                output.orbis_upload_bundle.len(),
                error
            );
            return None;
        }
    };
    Some((ct, bundle))
}

async fn connect_to_node(node_url: &Url) -> Result<Channel> {
    let endpoint = tonic::transport::Endpoint::from_shared(node_url.to_string())
        .context("invalid node URL")?
        .timeout(std::time::Duration::from_secs(30));

    endpoint
        .connect()
        .await
        .context(format!("failed to connect to node at {node_url}"))
}

async fn fetch_transactions(
    channel: Channel,
    height: u64,
) -> Result<Vec<penumbra_sdk_proto::core::transaction::v1::Transaction>> {
    use penumbra_sdk_proto::core::app::v1::{
        query_service_client::QueryServiceClient as AppQueryServiceClient,
        TransactionsByHeightRequest,
    };

    let mut client = AppQueryServiceClient::new(channel);
    let request = TransactionsByHeightRequest {
        block_height: height,
    };
    let response = client
        .transactions_by_height(request)
        .await
        .context("failed to fetch transactions")?;
    Ok(response.into_inner().transactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::{Element, Fr};
    use penumbra_sdk_asset::{asset, Value};
    use penumbra_sdk_compliance::transfer::encrypt_transfer;
    use penumbra_sdk_compliance::{derive_compliance_scalar, issuer_keys::DetectionKey};
    use penumbra_sdk_keys::{keys::AddressIndex, test_keys};
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_proto::core::component::shielded_pool::v1::{
        Consolidate, ConsolidateBody, Split, SplitBody, Transfer, TransferBody, TransferOutputBody,
    };
    use penumbra_sdk_proto::core::transaction::v1::action::Action;
    use std::collections::HashSet;

    fn derive_ack(ring_pk: &Element, address: &penumbra_sdk_keys::Address) -> Element {
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    fn make_transfer_ciphertext_bytes() -> Vec<u8> {
        let dk = DetectionKey::new(Fr::from(5u64));
        let dk_pub = dk.public_key();
        let ring_pk = Element::GENERATOR * Fr::from(11u64);
        let sender = test_keys::ADDRESS_0.clone();
        let receiver = test_keys::FULL_VIEWING_KEY
            .payment_address(AddressIndex::from(1u32))
            .0;
        encrypt_transfer(
            &mut rand_core::OsRng,
            &derive_ack(&ring_pk, &sender),
            &derive_ack(&ring_pk, &receiver),
            &dk_pub,
            &receiver,
            &sender,
            Value {
                amount: Amount::from(17u128),
                asset_id: asset::Id(decaf377::Fq::from(77u64)),
            },
            false,
            decaf377::Fq::from(9u64),
        )
        .expect("transfer ciphertext should build")
        .ciphertext
        .to_bytes()
    }

    fn dummy_context<'a>(tier_mode: &'a str, subject: &'a str) -> AuditContext<'a> {
        let cli = Box::leak(Box::new(OrbisClient::new("http://127.0.0.1:8080")));
        let dk = Box::leak(Box::new(Fr::from(3u64)));
        let dk_pub = Box::leak(Box::new(Element::GENERATOR * Fr::from(3u64)));
        let mut known_transmission_keys = HashSet::new();
        known_transmission_keys.insert(subject.to_string());
        let known_transmission_keys = Box::leak(Box::new(known_transmission_keys));

        AuditContext {
            cli,
            dk,
            dk_pub,
            subject_transmission_key_hex: subject,
            subject_b_d_bytes: &[0u8; 32],
            known_transmission_keys,
            tier_mode,
        }
    }

    #[test]
    fn extract_transfer_data_ignores_non_transfer_actions() {
        let split_action = penumbra_sdk_proto::core::transaction::v1::Action {
            action: Some(Action::Split(Split {
                body: Some(SplitBody::default()),
                ..Default::default()
            })),
        };
        let consolidate_action = penumbra_sdk_proto::core::transaction::v1::Action {
            action: Some(Action::Consolidate(Consolidate {
                body: Some(ConsolidateBody::default()),
                ..Default::default()
            })),
        };

        assert!(extract_transfer_data(&split_action, 0).is_none());
        assert!(extract_transfer_data(&consolidate_action, 0).is_none());
    }

    fn make_upload_bundle_bytes() -> Vec<u8> {
        let mut rng = rand_core::OsRng;
        let ring_sk = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * ring_sk;
        let sender = test_keys::ADDRESS_0.clone();
        let receiver = test_keys::FULL_VIEWING_KEY
            .payment_address(AddressIndex::from(1u32))
            .0;
        let sender_b_d = sender.diversified_generator().vartime_compress_to_field();
        let receiver_b_d = receiver.diversified_generator().vartime_compress_to_field();
        let policy_id = "policy-id";
        let resource = "document";
        let permission = "read";
        let ring_id = "ring-id";
        let timestamp = 1_700_000_000;
        let sender_core_salt = decaf377::Fq::from(11u64);
        let sender_ext_salt = decaf377::Fq::from(12u64);
        let output_core_salt = decaf377::Fq::from(13u64);
        let output_ext_salt = decaf377::Fq::from(14u64);

        let bundle = TransferOrbisUploadBundle {
            sender_core: penumbra_sdk_compliance::build_orbis_encrypted_seed_upload_package(
                &mut rng,
                &ring_pk,
                decaf377::Fq::from(21u64),
                penumbra_sdk_compliance::TransferTierMetadataStatement::from_identifiers(
                    sender_b_d,
                    ring_id,
                    policy_id,
                    resource,
                    permission,
                    penumbra_sdk_compliance::TransferTierKind::SenderCore,
                    timestamp,
                    sender_core_salt,
                ),
                ring_id,
                policy_id,
                resource,
                permission,
                penumbra_sdk_compliance::TransferTierKind::SenderCore,
                timestamp,
                sender_core_salt,
            )
            .expect("sender_core package should build"),
            sender_ext: penumbra_sdk_compliance::build_orbis_encrypted_seed_upload_package(
                &mut rng,
                &ring_pk,
                decaf377::Fq::from(22u64),
                penumbra_sdk_compliance::TransferTierMetadataStatement::from_identifiers(
                    sender_b_d,
                    ring_id,
                    policy_id,
                    resource,
                    permission,
                    penumbra_sdk_compliance::TransferTierKind::SenderExt,
                    timestamp,
                    sender_ext_salt,
                ),
                ring_id,
                policy_id,
                resource,
                permission,
                penumbra_sdk_compliance::TransferTierKind::SenderExt,
                timestamp,
                sender_ext_salt,
            )
            .expect("sender_ext package should build"),
            output_core: penumbra_sdk_compliance::build_orbis_encrypted_seed_upload_package(
                &mut rng,
                &ring_pk,
                decaf377::Fq::from(23u64),
                penumbra_sdk_compliance::TransferTierMetadataStatement::from_identifiers(
                    receiver_b_d,
                    ring_id,
                    policy_id,
                    resource,
                    permission,
                    penumbra_sdk_compliance::TransferTierKind::OutputCore,
                    timestamp,
                    output_core_salt,
                ),
                ring_id,
                policy_id,
                resource,
                permission,
                penumbra_sdk_compliance::TransferTierKind::OutputCore,
                timestamp,
                output_core_salt,
            )
            .expect("output_core package should build"),
            output_ext: penumbra_sdk_compliance::build_orbis_encrypted_seed_upload_package(
                &mut rng,
                &ring_pk,
                decaf377::Fq::from(24u64),
                penumbra_sdk_compliance::TransferTierMetadataStatement::from_identifiers(
                    receiver_b_d,
                    ring_id,
                    policy_id,
                    resource,
                    permission,
                    penumbra_sdk_compliance::TransferTierKind::OutputExt,
                    timestamp,
                    output_ext_salt,
                ),
                ring_id,
                policy_id,
                resource,
                permission,
                penumbra_sdk_compliance::TransferTierKind::OutputExt,
                timestamp,
                output_ext_salt,
            )
            .expect("output_ext package should build"),
        };
        bundle.to_bytes().expect("bundle should serialize")
    }

    #[test]
    fn extract_transfer_data_reads_requested_transfer_output() {
        let ciphertext_bytes = make_transfer_ciphertext_bytes();
        let bundle_bytes = make_upload_bundle_bytes();
        let transfer_action = penumbra_sdk_proto::core::transaction::v1::Action {
            action: Some(Action::Transfer(Transfer {
                body: Some(TransferBody {
                    outputs: vec![
                        TransferOutputBody::default(),
                        TransferOutputBody {
                            compliance_ciphertext: ciphertext_bytes.clone(),
                            orbis_upload_bundle: bundle_bytes.clone(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            })),
        };

        let (ciphertext, bundle) =
            extract_transfer_data(&transfer_action, 1).expect("transfer action should expose data");
        assert_eq!(ciphertext.to_bytes(), ciphertext_bytes);
        assert_eq!(
            bundle.to_bytes().expect("bundle should serialize"),
            bundle_bytes
        );
    }

    #[test]
    fn candidate_to_entry_uses_semantic_transfer_rendering() {
        let tx_ref = DetectedTxRef {
            height: 290,
            tx_hash: "tx".to_string(),
            action_index: 1,
            output_index: 2,
            asset_id: "asset".to_string(),
            is_flagged: false,
        };
        let self_tk = "aa".repeat(32);
        let counterparty_tk = "bb".repeat(32);

        let default_ctx = dummy_context("default", &self_tk);
        let default_entry = candidate_to_entry(
            &tx_ref,
            TransferMatch::Sender {
                amount: Amount::from(400u128),
                receiver: AddressData {
                    transmission_key_hex: counterparty_tk.clone(),
                },
            },
            &default_ctx,
        );
        assert_eq!(default_entry.amount, "400");
        assert_eq!(default_entry.output_index, 2);
        assert_eq!(default_entry.self_address, self_tk);
        assert_eq!(default_entry.counterparty, "");
        assert_eq!(default_entry.decrypted_via, "core");

        let extension_ctx = dummy_context("extension", &self_tk);
        let extension_entry = candidate_to_entry(
            &tx_ref,
            TransferMatch::Receiver {
                amount: Amount::from(600u128),
                sender: AddressData {
                    transmission_key_hex: counterparty_tk.clone(),
                },
            },
            &extension_ctx,
        );
        assert_eq!(extension_entry.amount, "600");
        assert_eq!(extension_entry.self_address, self_tk);
        assert_eq!(extension_entry.counterparty, counterparty_tk);
        assert_eq!(extension_entry.decrypted_via, "ext");
    }
}
