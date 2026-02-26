//! Orbis PRE audit tool — real Orbis node interaction for compliance audits.
//!
//! Replaces `orbis-sim` with real Orbis PRE via the adjusted reader key trick:
//! sets `reader_pk = pk_issuer + EPK_chain - enc_cmt_orbis` so Orbis computes
//! `d * sk_ring * (pk_issuer + EPK_chain)` — identical to SimulatedOrbis math.
//!
//! Uses `cli-tool pre --xnc-only` to get xnc_cmt without AES decrypt (which
//! would fail since enc_cmt != EPK_chain).

use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use decaf377::{Element, Fr};
use penumbra_sdk_compliance::compute_adjusted_reader_pk;
use penumbra_sdk_compliance::derive_compliance_scalar;
use penumbra_sdk_compliance::orbis::recover_seed;
use penumbra_sdk_compliance::scanning::{
    decrypt_core_with_seed, decrypt_extension_with_seed, decrypt_spend_ext_with_seed,
};
use penumbra_sdk_compliance::structs::ComplianceCiphertext;
use penumbra_sdk_keys::Address;
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use url::Url;

mod cli_tool;

#[derive(Parser, Debug)]
#[clap(
    name = "orbis-audit",
    about = "Compliance audit via real Orbis PRE (replaces orbis-sim)"
)]
struct Args {
    /// Path to detected transactions JSON (from `pcli tx compliance scan`).
    #[clap(long)]
    input: PathBuf,

    /// Issuer's private detection key (64 hex chars = 32 bytes).
    #[clap(long)]
    dk_hex: String,

    /// The URL of the pd gRPC endpoint.
    #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
    node: Url,

    /// Output file for audit results (JSON format).
    #[clap(long, default_value = "/tmp/alice-audit.json")]
    output: PathBuf,

    /// Disclosure tier: "default" (core PRE + sender_ciphertext in one pass),
    /// or "extension" (core + extension PRE — reveals counterparty/sender identity).
    #[clap(long, default_value = "default")]
    tier: String,

    /// Target user's Penumbra address (bech32m).
    #[clap(long)]
    sender_address: String,

    /// Orbis node endpoint (e.g. http://127.0.0.1:50051).
    #[clap(long)]
    orbis_endpoint: String,

    /// Orbis ring ID (required with --ring-pk-hex, otherwise fetched via get-latest-ring).
    #[clap(long)]
    ring_id: Option<String>,

    /// Ring public key hex (64 chars). If provided with --ring-id, skips get-latest-ring.
    #[clap(long)]
    ring_pk_hex: Option<String>,

    /// cli-tool binary name.
    #[clap(long, default_value = "cli-tool")]
    cli_tool: String,
}

/// Matches the scan output format from pcli.
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
    #[serde(default)]
    is_spend: bool,
}

/// Output entry for each successfully decrypted transaction.
/// Compatible with `issuer-db update`.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuditEntry {
    height: u64,
    action_index: usize,
    amount: String,
    self_address: String,
    counterparty: String,
    decrypted_via: String,
}

/// Orbis context set up once per run.
struct OrbisContext {
    object_id: String,
    enc_cmt: Element,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Validate tier
    let tier_mode = match args.tier.as_str() {
        "default" | "extension" => args.tier.as_str(),
        other => anyhow::bail!("--tier must be 'default' or 'extension', got '{}'", other),
    };

    // Parse issuer DK
    let dk = parse_fr(&args.dk_hex, "DK")?;
    let dk_pub = Element::GENERATOR * dk;

    // Parse sender address
    let sender_addr: Address = args
        .sender_address
        .parse()
        .context("failed to parse --sender-address as Penumbra address")?;
    let sender_pk_hex = hex::encode(sender_addr.transmission_key().0);
    let b_d_fq = sender_addr
        .diversified_generator()
        .vartime_compress_to_field();
    let b_d_hex = hex::encode(b_d_fq.to_bytes());

    // Set up Orbis CLI
    let cli = cli_tool::CliTool::new(&args.cli_tool, args.orbis_endpoint.clone());

    // Get ring: prefer explicit --ring-pk-hex + --ring-id, fall back to get-latest-ring
    // orbis_ring_pk_hex: original hex from orbis (for cli-tool calls, different serialization)
    // ring_pk: decaf377 Element (for compliance math: ACK, adjusted reader key)
    let (ring_pk, ring_id, orbis_ring_pk_hex) = match (&args.ring_pk_hex, &args.ring_id) {
        (Some(pk_hex), Some(id)) => {
            let bytes = hex::decode(pk_hex).context("invalid --ring-pk-hex")?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("--ring-pk-hex must be 32 bytes"))?;
            let pk = decaf377::Encoding(arr)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("--ring-pk-hex is not a valid curve point"))?;
            (pk, id.clone(), pk_hex.clone())
        }
        _ => cli.get_latest_ring()?,
    };

    // Derive ACK from ring_pk + user's b_d
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ack = ring_pk * d_fr;

    eprintln!(
        "orbis-audit: ring_pk={}, ring_id={}, target={}...",
        &orbis_ring_pk_hex[..16],
        &ring_id,
        &sender_pk_hex[..16],
    );

    // Set up ACP and store dummy secret (use original orbis hex for cli-tool)
    let orbis_ctx = setup_orbis(&cli, &orbis_ring_pk_hex, &ring_id, &b_d_hex)?;

    // Load scan output
    let file = File::open(&args.input).context("failed to open input file")?;
    let reader = BufReader::new(file);
    let scan: ScanOutput = serde_json::from_reader(reader).context("failed to parse scan JSON")?;

    eprintln!(
        "orbis-audit: Processing {} detected transactions",
        scan.detected.len()
    );

    // Connect to Penumbra node
    let channel = connect_to_node(&args.node).await?;

    let mut results: Vec<AuditEntry> = Vec::new();
    let mut attempted = 0u64;
    let mut decrypted = 0u64;
    let mut no_ciphertext = 0u64;
    let mut no_body = 0u64;

    for tx_ref in &scan.detected {
        if tx_ref.is_flagged {
            continue;
        }

        attempted += 1;

        let transactions = fetch_transactions(channel.clone(), tx_ref.height).await?;

        let mut found_action = false;
        for tx in &transactions {
            if let Some(ref body) = tx.body {
                if tx_ref.action_index < body.actions.len() {
                    found_action = true;
                    let action = &body.actions[tx_ref.action_index];

                    match extract_compliance_data(action) {
                        Some(extracted) => {
                            let ct = extracted.ct;

                            // Core tier PRE via adjusted reader key
                            let xnc_cmt_core = orbis_pre_for_epk(
                                &cli,
                                &orbis_ctx,
                                &dk_pub,
                                &ct.epk_1,
                                &b_d_hex,
                                &orbis_ring_pk_hex,
                            )?;
                            let seed_core = recover_seed(&xnc_cmt_core, &dk, &ack, &ct.c2_core);
                            let core = decrypt_core_with_seed(seed_core, &ct)?;

                            let core_handled = if let Some(c) = &core {
                                let valid = hex::encode(c.self_transmission_key) == sender_pk_hex;
                                if valid {
                                    if tier_mode == "extension" {
                                        let epk_2 = ct.epk_2.unwrap_or(ct.epk_1);
                                        if let Some(c2_ext) = ct.c2_ext {
                                            let xnc_cmt_ext = orbis_pre_for_epk(
                                                &cli,
                                                &orbis_ctx,
                                                &dk_pub,
                                                &epk_2,
                                                &b_d_hex,
                                                &orbis_ring_pk_hex,
                                            )?;
                                            let seed_ext =
                                                recover_seed(&xnc_cmt_ext, &dk, &ack, &c2_ext);
                                            let ext = decrypt_extension_with_seed(seed_ext, &ct)?;
                                            if let Some(e) = &ext {
                                                decrypted += 1;
                                                results.push(AuditEntry {
                                                    height: tx_ref.height,
                                                    action_index: tx_ref.action_index,
                                                    amount: c.amount.value().to_string(),
                                                    self_address: hex::encode(
                                                        c.self_transmission_key,
                                                    ),
                                                    counterparty: hex::encode(
                                                        e.counterparty_transmission_key,
                                                    ),
                                                    decrypted_via: "extension".to_string(),
                                                });
                                            } else {
                                                decrypted += 1;
                                                results.push(core_entry(tx_ref, c));
                                            }
                                        } else {
                                            decrypted += 1;
                                            results.push(core_entry(tx_ref, c));
                                        }
                                    } else {
                                        decrypted += 1;
                                        results.push(core_entry(tx_ref, c));
                                    }
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            // Fallback: try sext tier (user is the sender)
                            if !core_handled && tier_mode == "extension" {
                                if let (Some(c2_sext), Some(epk_3)) = (ct.c2_sext, ct.epk_3) {
                                    let xnc_cmt_sext = orbis_pre_for_epk(
                                        &cli,
                                        &orbis_ctx,
                                        &dk_pub,
                                        &epk_3,
                                        &b_d_hex,
                                        &orbis_ring_pk_hex,
                                    )?;
                                    let seed_sext =
                                        recover_seed(&xnc_cmt_sext, &dk, &ack, &c2_sext);
                                    if let Ok(Some(data)) =
                                        decrypt_spend_ext_with_seed(seed_sext, &ct)
                                    {
                                        decrypted += 1;
                                        results.push(AuditEntry {
                                            height: tx_ref.height,
                                            action_index: tx_ref.action_index,
                                            amount: data.amount.value().to_string(),
                                            self_address: hex::encode(
                                                data.recipient_transmission_key,
                                            ),
                                            counterparty: sender_pk_hex.clone(),
                                            decrypted_via: "extension".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        None => {
                            no_ciphertext += 1;
                        }
                    }
                }
            } else {
                no_body += 1;
            }
        }
        if !found_action {
            eprintln!(
                "orbis-audit: height={} action={}: action not found in {} txs",
                tx_ref.height,
                tx_ref.action_index,
                transactions.len()
            );
        }
    }

    // Write results
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
    if no_body > 0 {
        eprintln!("orbis-audit: {} transactions had no body.", no_body);
    }
    println!("Results saved to: {}", args.output.display());

    Ok(())
}

// ============================================================================
// Orbis helpers
// ============================================================================

/// One-time setup: create ACP policy, store dummy secret, register object.
fn setup_orbis(
    cli: &cli_tool::CliTool,
    ring_pk_hex: &str,
    ring_id: &str,
    derivation_hex: &str,
) -> Result<OrbisContext> {
    eprintln!("orbis-audit: Setting up Orbis ACP...");

    let policy_id = cli.add_policy()?;
    eprintln!("orbis-audit: policy_id={}", policy_id);

    // store_secret returns both object_id and enc_cmt from the same operation
    let (object_id, enc_cmt_hex) =
        cli.store_secret(ring_pk_hex, ring_id, &policy_id, derivation_hex)?;
    eprintln!("orbis-audit: object_id={}", object_id);

    cli.register_object(&policy_id, &object_id)?;
    cli.set_relationship(&policy_id, &object_id)?;
    eprintln!("orbis-audit: ACP configured");

    let enc_cmt_bytes = hex::decode(&enc_cmt_hex).context("invalid enc_cmt hex")?;
    let enc_cmt_arr: [u8; 32] = enc_cmt_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("enc_cmt should be 32 bytes"))?;
    let enc_cmt = decaf377::Encoding(enc_cmt_arr)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid enc_cmt curve point"))?;

    eprintln!(
        "orbis-audit: enc_cmt={}",
        hex::encode(&enc_cmt.vartime_compress().0[..8])
    );

    Ok(OrbisContext { object_id, enc_cmt })
}

/// Perform PRE for a single chain EPK using the adjusted reader key trick.
///
/// `adjusted_reader_pk = pk_issuer + EPK_chain - enc_cmt_orbis`
/// Orbis computes: `xnc_cmt = d * sk_ring * (pk_issuer + EPK_chain)`
fn orbis_pre_for_epk(
    cli: &cli_tool::CliTool,
    ctx: &OrbisContext,
    pk_issuer: &Element,
    epk_chain: &Element,
    derivation_hex: &str,
    ring_pk_hex: &str,
) -> Result<Element> {
    let adjusted_pk = compute_adjusted_reader_pk(pk_issuer, epk_chain, &ctx.enc_cmt);
    let adjusted_pk_hex = hex::encode(adjusted_pk.vartime_compress().0);

    let xnc_hex = cli.pre_xnc_only(
        ring_pk_hex,
        &adjusted_pk_hex,
        &ctx.object_id,
        derivation_hex,
    )?;

    let xnc_bytes = hex::decode(&xnc_hex).context("invalid xnc_cmt hex from PRE")?;
    let xnc_arr: [u8; 32] = xnc_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("xnc_cmt should be 32 bytes"))?;
    let xnc_cmt = decaf377::Encoding(xnc_arr)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid xnc_cmt curve point"))?;

    Ok(xnc_cmt)
}

fn core_entry(
    tx_ref: &DetectedTxRef,
    c: &penumbra_sdk_compliance::scanning::CoreData,
) -> AuditEntry {
    AuditEntry {
        height: tx_ref.height,
        action_index: tx_ref.action_index,
        amount: c.amount.value().to_string(),
        self_address: hex::encode(c.self_transmission_key),
        counterparty: "".to_string(),
        decrypted_via: "core".to_string(),
    }
}

// ============================================================================
// Helpers ported from orbis-sim
// ============================================================================

fn parse_fr(hex_str: &str, label: &str) -> Result<Fr> {
    let bytes = hex::decode(hex_str).context(format!("invalid hex for {}", label))?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "{} must be 32 bytes (64 hex chars), got {}",
            label,
            bytes.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Fr::from_le_bytes_mod_order(&arr))
}

struct ExtractedCompliance {
    ct: ComplianceCiphertext,
    #[allow(dead_code)]
    dleq_bytes: Vec<u8>,
}

fn extract_compliance_data(
    action: &penumbra_sdk_proto::core::transaction::v1::Action,
) -> Option<ExtractedCompliance> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action as ActionEnum;

    let action_inner = action.action.as_ref()?;

    let (cc_bytes, dleq_bytes) = match action_inner {
        ActionEnum::Output(output) => {
            let body = output.body.as_ref()?;
            (&body.compliance_ciphertext, body.dleq_proofs.clone())
        }
        ActionEnum::Spend(spend) => {
            let body = spend.body.as_ref()?;
            (&body.compliance_ciphertext, body.dleq_proof.clone())
        }
        _ => return None,
    };

    if cc_bytes.is_empty() {
        return None;
    }

    match ComplianceCiphertext::from_bytes(cc_bytes) {
        Ok(ct) => Some(ExtractedCompliance { ct, dleq_bytes }),
        Err(e) => {
            eprintln!(
                "orbis-audit: ciphertext deserialization failed ({} bytes): {}",
                cc_bytes.len(),
                e
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
        .context(format!("failed to connect to node at {}", node_url))
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
