use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use decaf377::{Element, Fq, Fr};
use penumbra_sdk_compliance::derive_compliance_scalar;
use penumbra_sdk_compliance::structs::ComplianceCiphertext;
use penumbra_sdk_compliance::{
    decrypt_core_via_orbis, decrypt_extension_via_orbis, decrypt_spend_ext_via_orbis,
    OrbisReencryptor, SimulatedOrbis,
};
use penumbra_sdk_keys::Address;
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use url::Url;

/// Simulated Orbis Proxy Re-Encryption for compliance audits.
///
/// Simulates the Orbis MPC ring for a single user audit. In production,
/// sk_ring is threshold-shared across ring nodes and never reconstructed.
///
/// Modes:
///   --derive-ring-pk    Print ring_pk hex for a given sk_ring and exit.
///   (default)           Process detected transactions via PRE.
#[derive(Parser, Debug)]
#[clap(name = "orbis-sim", about = "Simulate Orbis PRE for compliance audits")]
struct Args {
    /// Ring secret key (64 hex chars = 32 bytes).
    /// In production, this is threshold-shared in Orbis MPC — never held by one party.
    #[clap(long)]
    sk_ring_hex: String,

    /// Derive ring_pk from sk_ring and print it (hex). No other args needed.
    #[clap(long)]
    derive_ring_pk: bool,

    /// Path to detected transactions JSON (from `pcli tx compliance scan`).
    #[clap(long)]
    input: Option<PathBuf>,

    /// Issuer's private detection key (64 hex chars = 32 bytes).
    #[clap(long)]
    dk_hex: Option<String>,

    /// The URL of the pd gRPC endpoint.
    #[clap(long, env = "PENUMBRA_NODE_PD_URL")]
    node: Option<Url>,

    /// Output file for audit results (JSON format).
    #[clap(long, default_value = "/tmp/alice-audit.json")]
    output: PathBuf,

    /// Disclosure tier: "default" (core PRE + sender_ciphertext in one pass),
    /// or "extension" (core + extension PRE — reveals counterparty/sender identity).
    #[clap(long, default_value = "default")]
    tier: String,

    /// Target user's Penumbra address (bech32m). Orbis looks up the user's
    /// diversified basepoint (b_d) from this address at re-encryption time.
    #[clap(long)]
    sender_address: Option<String>,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuditEntry {
    height: u64,
    action_index: usize,
    amount: String,
    self_address: String,
    counterparty: String,
    decrypted_via: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Parse ring secret key (always required)
    let sk_ring = parse_fr(&args.sk_ring_hex, "sk_ring")?;
    let orbis = SimulatedOrbis::new(sk_ring);
    let ring_pk = orbis.ring_pk();

    // --derive-ring-pk mode: print ring_pk and exit
    if args.derive_ring_pk {
        println!("{}", hex::encode(ring_pk.vartime_compress().0));
        return Ok(());
    }

    // Validate required args for audit mode
    let input = args
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--input is required for audit mode"))?;
    let dk_hex = args
        .dk_hex
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--dk-hex is required for audit mode"))?;
    let node = args
        .node
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--node is required for audit mode"))?;
    let sender_address_str = args
        .sender_address
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--sender-address is required for audit mode"))?;

    // Validate tier
    let tier_mode = match args.tier.as_str() {
        "default" | "extension" => args.tier.as_str(),
        other => anyhow::bail!("--tier must be 'default' or 'extension', got '{}'", other),
    };

    // Parse sender address → derive b_d_fq (in production, Orbis looks this up from registration)
    let sender_addr: Address = sender_address_str
        .parse()
        .context("failed to parse --sender-address as Penumbra address")?;
    let sender_pk_hex = hex::encode(sender_addr.transmission_key().0);
    let b_d_fq = sender_addr
        .diversified_generator()
        .vartime_compress_to_field();
    let b_d_bytes = b_d_fq.to_bytes();

    eprintln!(
        "orbis-sim: ring_pk={}, target={}...",
        hex::encode(&ring_pk.vartime_compress().0[..8]),
        &sender_pk_hex[..16],
    );

    // Derive ACK from ring_pk + user's b_d
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ack = ring_pk * d_fr;

    // Parse issuer DK
    let dk = parse_fr(dk_hex, "DK")?;
    let dk_pub = Element::GENERATOR * dk;

    // Load scan output
    let file = File::open(input).context("failed to open input file")?;
    let reader = BufReader::new(file);
    let scan: ScanOutput = serde_json::from_reader(reader).context("failed to parse scan JSON")?;

    eprintln!(
        "orbis-sim: Processing {} detected transactions",
        scan.detected.len()
    );

    // Connect to node
    let channel = connect_to_node(node).await?;

    let mut results: Vec<AuditEntry> = Vec::new();
    let mut attempted = 0u64;
    let mut decrypted = 0u64;
    let mut no_ciphertext = 0u64;
    let mut no_body = 0u64;

    for tx_ref in &scan.detected {
        // Skip flagged txs (already decrypted by issuer directly)
        if tx_ref.is_flagged {
            continue;
        }

        attempted += 1;

        // Fetch the transaction from chain
        let transactions = fetch_transactions(channel.clone(), tx_ref.height).await?;

        let mut found_action = false;
        for tx in &transactions {
            if let Some(ref body) = tx.body {
                if tx_ref.action_index < body.actions.len() {
                    found_action = true;
                    let action = &body.actions[tx_ref.action_index];

                    // Extract compliance ciphertext + DLEQ proofs
                    match extract_compliance_data(action) {
                        Some(extracted) => {
                            let ct = extracted.ct;
                            let dleq_bytes = extracted.dleq_bytes;

                            // Parse DLEQ proof for core tier (first 64 bytes)
                            let dleq_core = parse_dleq_proof(&dleq_bytes);

                            // Attempt DLEQ-verified PRE, fall back to plain PRE
                            // Metadata hash is not available from protobuf — use zero placeholder.
                            // Full metadata verification requires on-chain policy lookup (deferred).
                            let metadata_hash = Fq::from(0u64);
                            let xnc_cmt_core = if let Some((c, s)) = dleq_core {
                                match orbis.verify_and_reencrypt(
                                    &ct.epk_1,
                                    &dk_pub,
                                    &b_d_bytes,
                                    &c,
                                    &s,
                                    metadata_hash,
                                ) {
                                    Ok(xnc) => xnc,
                                    Err(e) => {
                                        eprintln!("orbis-sim: DLEQ verification failed for height={} action={}: {}",
                                            tx_ref.height, tx_ref.action_index, e);
                                        orbis.reencrypt(&ct.epk_1, &dk_pub, &b_d_bytes)
                                    }
                                }
                            } else {
                                orbis.reencrypt(&ct.epk_1, &dk_pub, &b_d_bytes)
                            };
                            let core = decrypt_core_via_orbis(&xnc_cmt_core, &dk, &ack, &ct)?;

                            // Check if core PRE produced a valid result for this user
                            let core_handled = if let Some(c) = &core {
                                // Core has no auth tag (~50% false positive on garbage).
                                // Verify the decrypted self_transmission_key matches our target user.
                                let valid = hex::encode(c.self_transmission_key) == sender_pk_hex;
                                if valid {
                                    if tier_mode == "extension" {
                                        let epk_2 = ct.epk_2.unwrap_or(ct.epk_1);
                                        // Parse DLEQ for ext tier (bytes 64..128 for Output)
                                        let dleq_ext =
                                            parse_dleq_proof(dleq_bytes.get(64..).unwrap_or(&[]));
                                        let xnc_cmt_ext = if let Some((c, s)) = dleq_ext {
                                            orbis
                                                .verify_and_reencrypt(
                                                    &epk_2,
                                                    &dk_pub,
                                                    &b_d_bytes,
                                                    &c,
                                                    &s,
                                                    metadata_hash,
                                                )
                                                .unwrap_or_else(|e| {
                                                    eprintln!("orbis-sim: ext DLEQ failed: {}", e);
                                                    orbis.reencrypt(&epk_2, &dk_pub, &b_d_bytes)
                                                })
                                        } else {
                                            orbis.reencrypt(&epk_2, &dk_pub, &b_d_bytes)
                                        };
                                        let ext = decrypt_extension_via_orbis(
                                            &xnc_cmt_ext,
                                            &dk,
                                            &ack,
                                            &ct,
                                        )?;
                                        if let Some(e) = &ext {
                                            decrypted += 1;
                                            results.push(AuditEntry {
                                                height: tx_ref.height,
                                                action_index: tx_ref.action_index,
                                                amount: c.amount.value().to_string(),
                                                self_address: hex::encode(c.self_transmission_key),
                                                counterparty: hex::encode(
                                                    e.counterparty_transmission_key,
                                                ),
                                                decrypted_via: "extension".to_string(),
                                            });
                                        } else {
                                            decrypted += 1;
                                            results.push(AuditEntry {
                                                height: tx_ref.height,
                                                action_index: tx_ref.action_index,
                                                amount: c.amount.value().to_string(),
                                                self_address: hex::encode(c.self_transmission_key),
                                                counterparty: "".to_string(),
                                                decrypted_via: "core".to_string(),
                                            });
                                        }
                                    } else {
                                        decrypted += 1;
                                        results.push(AuditEntry {
                                            height: tx_ref.height,
                                            action_index: tx_ref.action_index,
                                            amount: c.amount.value().to_string(),
                                            self_address: hex::encode(c.self_transmission_key),
                                            counterparty: "".to_string(),
                                            decrypted_via: "core".to_string(),
                                        });
                                    }
                                    true
                                } else {
                                    false // false positive — try sender_ct
                                }
                            } else {
                                false // core returned None
                            };

                            // In extension mode, if core didn't match this user, try spend_ext tier
                            // (user might be the sender — spend_ext is encrypted to sender's ACK_sext)
                            if !core_handled && tier_mode == "extension" {
                                if ct.c2_sext.is_some() {
                                    let epk_3 = ct.epk_3.unwrap_or(ct.epk_1);
                                    // Parse DLEQ for sext tier (bytes 128..192 for Output)
                                    let dleq_sext =
                                        parse_dleq_proof(dleq_bytes.get(128..).unwrap_or(&[]));
                                    let xnc_cmt_sext = if let Some((c, s)) = dleq_sext {
                                        orbis
                                            .verify_and_reencrypt(
                                                &epk_3,
                                                &dk_pub,
                                                &b_d_bytes,
                                                &c,
                                                &s,
                                                metadata_hash,
                                            )
                                            .unwrap_or_else(|e| {
                                                eprintln!("orbis-sim: sext DLEQ failed: {}", e);
                                                orbis.reencrypt(&epk_3, &dk_pub, &b_d_bytes)
                                            })
                                    } else {
                                        orbis.reencrypt(&epk_3, &dk_pub, &b_d_bytes)
                                    };
                                    if let Ok(Some(data)) =
                                        decrypt_spend_ext_via_orbis(&xnc_cmt_sext, &dk, &ack, &ct)
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
                "orbis-sim: height={} action={}: action not found in {} txs",
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
        "orbis-sim: Decrypted {}/{} non-flagged transfers (tier={}, target user's txs).",
        decrypted, attempted, args.tier
    );
    if no_ciphertext > 0 {
        eprintln!(
            "orbis-sim: {} actions had no compliance ciphertext.",
            no_ciphertext
        );
    }
    if no_body > 0 {
        eprintln!("orbis-sim: {} transactions had no body.", no_body);
    }
    println!("Results saved to: {}", args.output.display());

    Ok(())
}

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

/// Extracted compliance data from a protobuf Action.
struct ExtractedCompliance {
    ct: ComplianceCiphertext,
    /// Raw DLEQ proof bytes (64 for Spend, 192 for Output, empty if absent).
    dleq_bytes: Vec<u8>,
}

/// Extract ComplianceCiphertext and DLEQ proof bytes from a protobuf Action.
/// Works for both Output and Spend actions.
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
                "orbis-sim: ciphertext deserialization failed ({} bytes): {}",
                cc_bytes.len(),
                e
            );
            None
        }
    }
}

/// Parse a single DLEQ proof from 64 bytes: c (32 bytes Fq) || s (32 bytes Fr).
fn parse_dleq_proof(bytes: &[u8]) -> Option<(Fq, Fr)> {
    if bytes.len() < 64 {
        return None;
    }
    let c = Fq::from_le_bytes_mod_order(&bytes[..32]);
    let s = Fr::from_le_bytes_mod_order(&bytes[32..64]);
    Some((c, s))
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
