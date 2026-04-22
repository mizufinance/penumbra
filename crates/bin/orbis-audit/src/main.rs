use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use decaf377::{Element, Fq, Fr};
use penumbra_orbis_client::OrbisClient;
use penumbra_sdk_compliance::{
    compute_adjusted_reader_pk, decrypt_tier_bytes, derive_compliance_scalar, recover_seed,
    TransferComplianceCiphertext,
};
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use serde::{Deserialize, Serialize};
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
    sender_address: String,

    #[clap(long)]
    orbis_endpoint: String,

    #[clap(long)]
    ring_id: Option<String>,

    #[clap(long)]
    ring_pk_hex: Option<String>,

    #[clap(long = "known-address")]
    known_addresses: Vec<String>,
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
    asset_id: String,
    is_flagged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuditEntry {
    height: u64,
    action_index: usize,
    amount: String,
    self_address: String,
    counterparty: String,
    decrypted_via: String,
}

#[derive(Clone)]
struct OrbisContext {
    object_id: String,
    enc_cmt: Element,
}

#[derive(Clone)]
struct AuditContext<'a> {
    cli: &'a OrbisClient,
    orbis: &'a OrbisContext,
    dk: &'a Fr,
    dk_pub: &'a Element,
    ack: Element,
    b_d_hex: &'a str,
    subject_transmission_key_hex: &'a str,
    known_transmission_keys: &'a HashSet<String>,
    tier_mode: &'a str,
}

#[derive(Clone, Debug)]
struct AddressData {
    transmission_key_hex: String,
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

    let subject_address: Address = args
        .sender_address
        .parse()
        .context("failed to parse --sender-address as Penumbra address")?;
    let subject_transmission_key_hex = hex::encode(subject_address.transmission_key().0);
    let b_d_fq = subject_address
        .diversified_generator()
        .vartime_compress_to_field();
    let b_d_hex = hex::encode(b_d_fq.to_bytes());

    let mut known_transmission_keys = HashSet::new();
    known_transmission_keys.insert(subject_transmission_key_hex.clone());
    for address in &args.known_addresses {
        let address: Address = address
            .parse()
            .with_context(|| format!("failed to parse --known-address {address}"))?;
        known_transmission_keys.insert(hex::encode(address.transmission_key().0));
    }

    let cli = OrbisClient::new(args.orbis_endpoint.clone());
    let (ring_pk, ring_id, orbis_ring_pk_hex) = match (&args.ring_pk_hex, &args.ring_id) {
        (Some(pk_hex), Some(id)) => {
            let bytes = hex::decode(pk_hex).context("invalid --ring-pk-hex")?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow!("--ring-pk-hex must be 32 bytes"))?;
            let pk = decaf377::Encoding(arr)
                .vartime_decompress()
                .map_err(|_| anyhow!("--ring-pk-hex is not a valid curve point"))?;
            (pk, id.clone(), pk_hex.clone())
        }
        _ => {
            let ring = cli.get_latest_ring().await?;
            (ring.ring_pk, ring.ring_id, ring.ring_pk_hex)
        }
    };
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ack = ring_pk * d_fr;

    eprintln!(
        "orbis-audit: ring_pk={}, ring_id={}, target={}...",
        &orbis_ring_pk_hex[..16],
        &ring_id,
        &subject_transmission_key_hex[..16],
    );

    let orbis = setup_orbis(&cli, &orbis_ring_pk_hex, &ring_id, &b_d_hex).await?;

    let file = File::open(&args.input).context("failed to open input file")?;
    let reader = BufReader::new(file);
    let scan: ScanOutput = serde_json::from_reader(reader).context("failed to parse scan JSON")?;
    eprintln!(
        "orbis-audit: Processing {} detected transactions",
        scan.detected.len()
    );

    let channel = connect_to_node(&args.node).await?;
    let ctx = AuditContext {
        cli: &cli,
        orbis: &orbis,
        dk: &dk,
        dk_pub: &dk_pub,
        ack,
        b_d_hex: &b_d_hex,
        subject_transmission_key_hex: &subject_transmission_key_hex,
        known_transmission_keys: &known_transmission_keys,
        tier_mode,
    };

    let mut results = Vec::new();
    let mut attempted = 0u64;
    let mut decrypted = 0u64;
    let mut no_ciphertext = 0u64;

    for tx_ref in &scan.detected {
        if tx_ref.is_flagged {
            continue;
        }
        attempted += 1;
        let transactions = fetch_transactions(channel.clone(), tx_ref.height).await?;

        for tx in &transactions {
            let Some(body) = tx.body.as_ref() else {
                continue;
            };
            if tx_ref.action_index >= body.actions.len() {
                continue;
            }

            let action = &body.actions[tx_ref.action_index];
            let Some(ct) = extract_transfer_ciphertext(action) else {
                no_ciphertext += 1;
                continue;
            };

            if let Some(entry) = audit_transfer(tx_ref, &ct, &ctx).await? {
                decrypted += 1;
                results.push(entry);
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
    println!("Results saved to: {}", args.output.display());

    Ok(())
}

async fn audit_transfer(
    tx_ref: &DetectedTxRef,
    ct: &TransferComplianceCiphertext,
    ctx: &AuditContext<'_>,
) -> Result<Option<AuditEntry>> {
    if let Some(candidate) = try_receiver_match(ct, ctx).await? {
        return Ok(Some(candidate_to_entry(tx_ref, candidate, ctx)));
    }
    if let Some(candidate) = try_sender_match(ct, ctx).await? {
        return Ok(Some(candidate_to_entry(tx_ref, candidate, ctx)));
    }
    Ok(None)
}

async fn try_receiver_match(
    ct: &TransferComplianceCiphertext,
    ctx: &AuditContext<'_>,
) -> Result<Option<TransferMatch>> {
    let output_core = orbis_pre_for_epk(
        ctx.cli,
        ctx.orbis,
        ctx.dk_pub,
        &ct.output_core_epk,
        ctx.b_d_hex,
    )
    .await?;
    let output_core_seed = recover_seed(&output_core, ctx.dk, &ctx.ack, &ct.output_core_c2);
    let amount = decrypt_amount_with_seed(output_core_seed, &ct.encrypted_output_core)?;

    let output_ext = orbis_pre_for_epk(
        ctx.cli,
        ctx.orbis,
        ctx.dk_pub,
        &ct.output_ext_epk,
        ctx.b_d_hex,
    )
    .await?;
    let output_ext_seed = recover_seed(&output_ext, ctx.dk, &ctx.ack, &ct.output_ext_c2);
    let sender = match decrypt_address_with_seed(output_ext_seed, &ct.encrypted_output_ext) {
        Ok(sender) => sender,
        Err(_) => return Ok(None),
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
    ctx: &AuditContext<'_>,
) -> Result<Option<TransferMatch>> {
    let sender_core = orbis_pre_for_epk(
        ctx.cli,
        ctx.orbis,
        ctx.dk_pub,
        &ct.sender_core_epk,
        ctx.b_d_hex,
    )
    .await?;
    let sender_core_seed = recover_seed(&sender_core, ctx.dk, &ctx.ack, &ct.sender_core_c2);
    let amount = decrypt_amount_with_seed(sender_core_seed, &ct.encrypted_sender_core)?;

    let sender_ext = orbis_pre_for_epk(
        ctx.cli,
        ctx.orbis,
        ctx.dk_pub,
        &ct.sender_ext_epk,
        ctx.b_d_hex,
    )
    .await?;
    let sender_ext_seed = recover_seed(&sender_ext, ctx.dk, &ctx.ack, &ct.sender_ext_c2);
    let receiver = match decrypt_address_with_seed(sender_ext_seed, &ct.encrypted_sender_ext) {
        Ok(receiver) => receiver,
        Err(_) => return Ok(None),
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
            action_index: tx_ref.action_index,
            amount: amount.value().to_string(),
            self_address: ctx.subject_transmission_key_hex.to_string(),
            counterparty: String::new(),
            decrypted_via: "core".to_string(),
        },
        ("extension", TransferMatch::Receiver { amount, sender }) => AuditEntry {
            height: tx_ref.height,
            action_index: tx_ref.action_index,
            amount: amount.value().to_string(),
            self_address: ctx.subject_transmission_key_hex.to_string(),
            counterparty: sender.transmission_key_hex,
            decrypted_via: "ext".to_string(),
        },
        ("extension", TransferMatch::Sender { amount, receiver }) => AuditEntry {
            height: tx_ref.height,
            action_index: tx_ref.action_index,
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

async fn setup_orbis(
    cli: &OrbisClient,
    ring_pk_hex: &str,
    ring_id: &str,
    derivation_hex: &str,
) -> Result<OrbisContext> {
    eprintln!("orbis-audit: Setting up Orbis ACP...");
    let policy_id = cli.add_policy().await?;
    eprintln!("orbis-audit: policy_id={policy_id}");

    let store_secret = cli
        .store_secret(ring_pk_hex, ring_id, &policy_id, derivation_hex)
        .await?;
    let object_id = store_secret.object_id;
    let enc_cmt_hex = store_secret.enc_cmt_hex;
    eprintln!("orbis-audit: object_id={object_id}");

    cli.register_object(&policy_id, &object_id).await?;
    cli.set_relationship(&policy_id, &object_id).await?;
    eprintln!("orbis-audit: ACP configured");

    let enc_cmt_bytes = hex::decode(&enc_cmt_hex).context("invalid enc_cmt hex")?;
    let enc_cmt_arr: [u8; 32] = enc_cmt_bytes
        .try_into()
        .map_err(|_| anyhow!("enc_cmt should be 32 bytes"))?;
    let enc_cmt = decaf377::Encoding(enc_cmt_arr)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid enc_cmt curve point"))?;
    eprintln!(
        "orbis-audit: enc_cmt={}",
        hex::encode(&enc_cmt.vartime_compress().0[..8])
    );

    Ok(OrbisContext { object_id, enc_cmt })
}

async fn orbis_pre_for_epk(
    cli: &OrbisClient,
    ctx: &OrbisContext,
    pk_issuer: &Element,
    epk_chain: &Element,
    derivation_hex: &str,
) -> Result<Element> {
    let adjusted_pk = compute_adjusted_reader_pk(pk_issuer, epk_chain, &ctx.enc_cmt);
    let adjusted_pk_hex = hex::encode(adjusted_pk.vartime_compress().0);
    let xnc_hex = cli
        .pre_xnc_only(&adjusted_pk_hex, &ctx.object_id, derivation_hex)
        .await?
        .xnc_cmt_hex;

    let xnc_bytes = hex::decode(&xnc_hex).context("invalid xnc_cmt hex from PRE")?;
    let xnc_arr: [u8; 32] = xnc_bytes
        .try_into()
        .map_err(|_| anyhow!("xnc_cmt should be 32 bytes"))?;
    decaf377::Encoding(xnc_arr)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid xnc_cmt curve point"))
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

fn extract_transfer_ciphertext(
    action: &penumbra_sdk_proto::core::transaction::v1::Action,
) -> Option<TransferComplianceCiphertext> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action as ActionEnum;

    let ActionEnum::Transfer(transfer) = action.action.as_ref()? else {
        return None;
    };
    let body = transfer.body.as_ref()?;
    let output = body
        .outputs
        .iter()
        .find(|output| !output.compliance_ciphertext.is_empty())?;

    match TransferComplianceCiphertext::from_bytes(&output.compliance_ciphertext) {
        Ok(ct) => Some(ct),
        Err(error) => {
            eprintln!(
                "orbis-audit: ciphertext deserialization failed ({} bytes): {}",
                output.compliance_ciphertext.len(),
                error
            );
            None
        }
    }
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
    use penumbra_sdk_compliance::issuer_keys::DetectionKey;
    use penumbra_sdk_compliance::transfer::encrypt_transfer;
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
        let orbis = Box::leak(Box::new(OrbisContext {
            object_id: "object".to_string(),
            enc_cmt: Element::GENERATOR,
        }));
        let dk = Box::leak(Box::new(Fr::from(3u64)));
        let dk_pub = Box::leak(Box::new(Element::GENERATOR * Fr::from(3u64)));
        let mut known_transmission_keys = HashSet::new();
        known_transmission_keys.insert(subject.to_string());
        let known_transmission_keys = Box::leak(Box::new(known_transmission_keys));

        AuditContext {
            cli,
            orbis,
            dk,
            dk_pub,
            ack: Element::GENERATOR,
            b_d_hex: "00",
            subject_transmission_key_hex: subject,
            known_transmission_keys,
            tier_mode,
        }
    }

    #[test]
    fn extract_transfer_ciphertext_ignores_non_transfer_actions() {
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

        assert!(extract_transfer_ciphertext(&split_action).is_none());
        assert!(extract_transfer_ciphertext(&consolidate_action).is_none());
    }

    #[test]
    fn extract_transfer_ciphertext_reads_first_non_empty_transfer_output() {
        let ciphertext_bytes = make_transfer_ciphertext_bytes();
        let transfer_action = penumbra_sdk_proto::core::transaction::v1::Action {
            action: Some(Action::Transfer(Transfer {
                body: Some(TransferBody {
                    outputs: vec![
                        TransferOutputBody::default(),
                        TransferOutputBody {
                            compliance_ciphertext: ciphertext_bytes.clone(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            })),
        };

        let extracted = extract_transfer_ciphertext(&transfer_action)
            .expect("transfer action should expose a valid ciphertext");
        assert_eq!(extracted.to_bytes(), ciphertext_bytes);
    }

    #[test]
    fn candidate_to_entry_uses_semantic_transfer_rendering() {
        let tx_ref = DetectedTxRef {
            height: 290,
            tx_hash: "tx".to_string(),
            action_index: 1,
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
