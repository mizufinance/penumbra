//! Integration test binary for Orbis DKG, DLEQ, and PRE against real orbis-nodes.
//!
//! Mirrors the structure of `test_full_orbis_flow` in compliance_full_flow.rs,
//! but connects to real orbis-nodes via `cli-tool` subprocess calls for
//! DKG, store-secret, and PRE operations. Penumbra-side crypto (encryption,
//! DLEQ, detection) is done directly via crates.

use anyhow::{Context, Result};
use clap::Parser;
use decaf377::{Fq, Fr};
use penumbra_sdk_asset::{asset, Value};
use penumbra_sdk_compliance::{
    crypto::{
        compute_dleq_native, compute_metadata_hash, decrypt_detection_tier, encrypt_output,
        encrypt_spend, verify_dleq_native,
    },
    derive_compliance_scalar,
    scanning::{decrypt_core_with_seed, decrypt_extension_with_seed, decrypt_spend_ext_with_seed},
};
use penumbra_sdk_keys::{
    keys::{Bip44Path, SeedPhrase, SpendKey},
    Address,
};
use rand_core::{OsRng, RngCore};

mod cli_tool;

#[derive(Parser)]
#[clap(
    name = "orbis-test",
    about = "Test Orbis DKG, DLEQ, and PRE against real nodes"
)]
struct Args {
    /// Orbis node endpoints (comma-separated)
    #[clap(
        long,
        default_value = "http://127.0.0.1:50051,http://127.0.0.1:50052,http://127.0.0.1:50053"
    )]
    orbis_endpoints: String,

    /// SourceHub RPC endpoint
    #[clap(long, default_value = "http://localhost:26657")]
    sourcehub_rpc: String,

    /// cli-tool binary name
    #[clap(long, default_value = "cli-tool")]
    cli_tool: String,
}

// ============================================================================
// Context structs (mirrors test_full_orbis_flow)
// ============================================================================

struct RingContext {
    ring_pk: decaf377::Element,
    ring_id: String,
    /// Local DKG shares for FROST/PRE tests that can run in-process.
    /// None when using real nodes (shares held by nodes).
    local_shares: Option<LocalShares>,
}

struct LocalShares {
    secret_shares: Vec<crypto::r#trait::PriShare<Fr>>,
    pub_poly: crypto::decaf377::common::PubPoly,
    n: usize,
    t: usize,
}

struct IssuerContext {
    dk_scalar: Fr,
    dk_pub: decaf377::Element,
    _sk_issuer: Fr,
    _pk_issuer: decaf377::Element,
}

struct UserContext {
    address: Address,
    b_d_fq: Fq,
    _d_fr: Fr,
    ack: decaf377::Element,
}

// ============================================================================
// Test results
// ============================================================================

struct TestResults {
    passed: usize,
    failed: usize,
}

impl TestResults {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
        }
    }
    fn pass(&mut self, msg: &str) {
        println!("  \x1b[32mPASS\x1b[0m: {msg}");
        self.passed += 1;
    }
    fn fail(&mut self, msg: &str) {
        println!("  \x1b[31mFAIL\x1b[0m: {msg}");
        self.failed += 1;
    }
}

// ============================================================================
// Builder methods (same as test)
// ============================================================================

fn create_issuer() -> IssuerContext {
    let dk_scalar = Fr::rand(&mut OsRng);
    let dk_pub = decaf377::Element::GENERATOR * dk_scalar;
    let sk_issuer = Fr::rand(&mut OsRng);
    let pk_issuer = decaf377::Element::GENERATOR * sk_issuer;
    IssuerContext {
        dk_scalar,
        dk_pub,
        _sk_issuer: sk_issuer,
        _pk_issuer: pk_issuer,
    }
}

fn create_wallet() -> (Address, SpendKey) {
    let mut seed_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut seed_bytes);
    let seed = SeedPhrase::from_randomness(&seed_bytes);
    let spend_key = SpendKey::from_seed_phrase_bip44(seed, &Bip44Path::new(0));
    let fvk = spend_key.full_viewing_key();
    let address = fvk.payment_address(0u32.into()).0;
    (address, spend_key)
}

fn register_user(ring: &RingContext) -> UserContext {
    let (address, _spend_key) = create_wallet();
    let b_d_fq = address.diversified_generator().vartime_compress_to_field();
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ack = ring.ring_pk * d_fr;
    UserContext {
        address,
        b_d_fq,
        _d_fr: d_fr,
        ack,
    }
}

fn derive_seed(c2: &Fq, ack: &decaf377::Element, r: &Fr) -> Fq {
    let ss = (*ack * *r).vartime_compress_to_field();
    *c2 - ss
}

// ============================================================================
// FROST signing (in-process, requires local DKG shares)
// ============================================================================

fn frost_sign_local(
    shares: &LocalShares,
    msg: &[u8],
) -> Result<crypto::decaf377::sign::SchnorrSignature> {
    use crypto::decaf377::sign::ThresholdDecafSigner;
    use crypto::r#trait::{DistKeyShare, PriShare, ThresholdSigner};

    let signer = ThresholdDecafSigner::new();
    let dist_key_shares: Vec<DistKeyShare<Fr>> = shares
        .secret_shares
        .iter()
        .take(shares.t)
        .map(|s| DistKeyShare {
            pri_share: PriShare { i: s.i, v: s.v },
        })
        .collect();

    let mut commitments = Vec::new();
    let mut states = Vec::new();
    for dks in &dist_key_shares {
        let (commitment, state) = signer.generate_nonces(dks)?;
        commitments.push((dks.pri_share.i, commitment));
        states.push(state);
    }

    let mut sig_shares = Vec::new();
    for (dks, state) in dist_key_shares.iter().zip(states.iter()) {
        let sig_share = signer.sign(dks, msg, &shares.pub_poly, Some(state), &commitments)?;
        signer.verify_share(msg, &shares.pub_poly, &sig_share, &commitments)?;
        sig_shares.push(sig_share);
    }

    let sig = signer.recover(&sig_shares, shares.t, shares.n, msg, &commitments)?;
    sig.ok_or_else(|| anyhow::anyhow!("FROST recovery returned None"))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let endpoints: Vec<String> = args
        .orbis_endpoints
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let cli = cli_tool::CliTool::new(&args.cli_tool, endpoints[0].clone());

    let mut results = TestResults::new();

    println!("=== Orbis Integration Test ===");
    println!("  Endpoints: {}", args.orbis_endpoints);
    println!();

    // ================================================================
    // Phase 1: DKG → Ring
    // ================================================================
    println!("--- Phase 1: DKG ---");

    // Try to get existing ring first
    let ring = match cli.get_latest_ring() {
        Ok((pk, id)) => {
            println!("  Existing ring found, reusing.");
            results.pass("Ring available");
            RingContext {
                ring_pk: pk,
                ring_id: id,
                local_shares: None,
            }
        }
        Err(_) => {
            // Get peer IDs and run DKG
            let peer_ids: Vec<String> = endpoints
                .iter()
                .map(|ep| cli.get_peer_id(ep))
                .collect::<Result<_>>()?;
            println!(
                "  Peers: {}",
                peer_ids
                    .iter()
                    .map(|p| format!("{}...", &p[..16.min(p.len())]))
                    .collect::<Vec<_>>()
                    .join(" ")
            );

            cli.run_dkg(2, &peer_ids)?;
            println!("  Waiting 30s for DKG completion...");
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            let (pk, id) = cli
                .get_latest_ring()
                .context("Could not retrieve ring after DKG")?;
            results.pass("DKG completed");
            RingContext {
                ring_pk: pk,
                ring_id: id,
                local_shares: None,
            }
        }
    };
    println!(
        "  ring_pk: {}...",
        &hex::encode(ring.ring_pk.vartime_compress_to_field().to_bytes())[..16]
    );
    println!("  ring_id: {}", ring.ring_id);

    // Also create a local ring for FROST and in-process tests
    let local_ring = create_local_ring(5, 3)?;
    println!("  (local ring created for FROST/DLEQ tests)");

    // ================================================================
    // Phase 2: Issuer keypair
    // ================================================================
    println!();
    println!("--- Phase 2: Issuer keypair ---");
    let issuer = create_issuer();
    results.pass("Issuer keypair generated");

    // ================================================================
    // Phase 3: Users (with FROST-signed ACK using local ring)
    // ================================================================
    println!();
    println!("--- Phase 3: Register users with FROST-signed ACK ---");
    let alice = register_user(&local_ring);
    let bob = register_user(&local_ring);

    if let Some(ref shares) = local_ring.local_shares {
        let asset_id = asset::Id(Fq::from(10001u64));
        let msg = build_ack_registration_message(
            &alice.address,
            &alice.ack,
            &local_ring.ring_pk,
            asset_id,
        );
        let sig = frost_sign_local(shares, &msg)?;
        verify_ack_signature(&local_ring.ring_pk, &msg, &sig)?;
        results.pass("FROST-signed ACK (Alice)");

        let msg_bob =
            build_ack_registration_message(&bob.address, &bob.ack, &local_ring.ring_pk, asset_id);
        let sig_bob = frost_sign_local(shares, &msg_bob)?;
        verify_ack_signature(&local_ring.ring_pk, &msg_bob, &sig_bob)?;
        results.pass("FROST-signed ACK (Bob)");
    }

    // ================================================================
    // Phase 4: Encrypt output (Alice → Bob)
    // ================================================================
    println!();
    println!("--- Phase 4: Encrypt output ---");
    let value = Value {
        amount: 100u64.into(),
        asset_id: asset::Id(Fq::from(10001u64)),
    };

    let output_result = encrypt_output(
        &mut OsRng,
        &bob.ack,
        &alice.ack,
        &issuer.dk_pub,
        &bob.address,
        &alice.address,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;
    let ct = &output_result.ciphertext;
    results.pass("Output encryption (3 tiers)");

    // ================================================================
    // Phase 5: Derive seeds
    // ================================================================
    let seed_core = derive_seed(&ct.c2_core, &bob.ack, &output_result.r_1);
    let seed_ext = derive_seed(&ct.c2_ext.expect("c2_ext"), &bob.ack, &output_result.r_2);
    let seed_sext = derive_seed(
        &ct.c2_sext.expect("c2_sext"),
        &alice.ack,
        &output_result.r_3,
    );
    results.pass("Seed derivation (core, ext, sext)");

    // ================================================================
    // Phase 6: DLEQ proofs
    // ================================================================
    println!();
    println!("--- Phase 6: DLEQ proofs ---");
    let metadata_hash = compute_metadata_hash(
        Fq::from(1u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        Fq::from(1_700_000_000u64),
        Fq::from(0u64),
    );

    // Core DLEQ
    let s_core = bob.ack * output_result.r_1;
    let dleq_core = compute_dleq_native(
        output_result.r_1,
        Fr::rand(&mut OsRng),
        &bob.ack,
        &ct.epk_1,
        metadata_hash,
    );
    verify_dleq_native(
        &bob.ack,
        &ct.epk_1,
        &s_core,
        &dleq_core.c,
        &dleq_core.s,
        metadata_hash,
    )?;
    results.pass("Core DLEQ");

    // Ext DLEQ
    let epk_2 = ct.epk_2.expect("epk_2");
    let s_ext = bob.ack * output_result.r_2;
    let dleq_ext = compute_dleq_native(
        output_result.r_2,
        Fr::rand(&mut OsRng),
        &bob.ack,
        &epk_2,
        metadata_hash,
    );
    verify_dleq_native(
        &bob.ack,
        &epk_2,
        &s_ext,
        &dleq_ext.c,
        &dleq_ext.s,
        metadata_hash,
    )?;
    results.pass("Ext DLEQ");

    // Sext DLEQ
    let epk_3 = ct.epk_3.expect("epk_3");
    let s_sext = alice.ack * output_result.r_3;
    let dleq_sext = compute_dleq_native(
        output_result.r_3,
        Fr::rand(&mut OsRng),
        &alice.ack,
        &epk_3,
        metadata_hash,
    );
    verify_dleq_native(
        &alice.ack,
        &epk_3,
        &s_sext,
        &dleq_sext.c,
        &dleq_sext.s,
        metadata_hash,
    )?;
    results.pass("Sext DLEQ");

    // ================================================================
    // Phase 7: Orbis PRE pipeline per tier (via real nodes)
    // ================================================================
    println!();
    println!("--- Phase 7: Orbis PRE pipeline ---");

    // Generate reader key via real Orbis
    let (reader_sk, reader_pk) = cli.generate_reader_key()?;
    results.pass("Reader key generated");

    // Core tier
    println!();
    println!("  --- Core tier (Bob's derivation) ---");
    let bob_derivation = hex::encode(bob.b_d_fq.to_bytes());
    let recovered_core = run_pre_pipeline(
        &cli,
        &ring,
        &reader_sk,
        &reader_pk,
        seed_core,
        &bob_derivation,
        "core",
        &mut results,
    )
    .await?;
    if let Some(rc) = recovered_core {
        let core_data = decrypt_core_with_seed(rc, ct)?;
        if let Some(core) = core_data {
            if core.amount == value.amount {
                results.pass("Core tier decrypted (amount matches)");
            } else {
                results.fail(&format!(
                    "Core amount mismatch: {} != {}",
                    core.amount, value.amount
                ));
            }
        } else {
            results.fail("Core tier decryption returned None");
        }
    }

    // Ext tier
    println!();
    println!("  --- Ext tier (Bob's derivation) ---");
    let recovered_ext = run_pre_pipeline(
        &cli,
        &ring,
        &reader_sk,
        &reader_pk,
        seed_ext,
        &bob_derivation,
        "ext",
        &mut results,
    )
    .await?;
    if let Some(re) = recovered_ext {
        let ext_data = decrypt_extension_with_seed(re, ct)?;
        if let Some(ext) = ext_data {
            if ext.counterparty_diversified_generator == *alice.address.diversified_generator() {
                results.pass("Ext tier decrypted (counterparty = Alice)");
            } else {
                results.fail("Ext counterparty mismatch");
            }
        } else {
            results.fail("Ext tier decryption returned None");
        }
    }

    // Sext tier
    println!();
    println!("  --- Sext tier (Alice's derivation) ---");
    let alice_derivation = hex::encode(alice.b_d_fq.to_bytes());
    let recovered_sext = run_pre_pipeline(
        &cli,
        &ring,
        &reader_sk,
        &reader_pk,
        seed_sext,
        &alice_derivation,
        "sext",
        &mut results,
    )
    .await?;
    if recovered_sext.is_some() {
        let sext_data = decrypt_spend_ext_with_seed(recovered_sext.unwrap(), ct)?;
        if sext_data.is_some() {
            results.pass("Sext tier decrypted");
        } else {
            results.fail("Sext tier decryption returned None");
        }
    }

    // ================================================================
    // Phase 8: Detection tier
    // ================================================================
    println!();
    println!("--- Phase 8: Detection tier ---");
    let det = decrypt_detection_tier(
        &issuer.dk_scalar,
        &ct.epk_1,
        &ct.detection_tag,
        &value.asset_id,
    )?;
    let (det_asset_id, det_flag, _) = det;
    if det_asset_id == value.asset_id && !det_flag {
        results.pass("Detection tier (correct asset, not flagged)");
    } else {
        results.fail("Detection tier mismatch");
    }

    // ================================================================
    // Phase 9: Spend path
    // ================================================================
    println!();
    println!("--- Phase 9: Spend path ---");
    let spend_result = encrypt_spend(
        &mut OsRng,
        &alice.ack,
        &issuer.dk_pub,
        &alice.address,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;
    let spend_ct = &spend_result.ciphertext;
    let seed_spend = derive_seed(&spend_ct.c2_core, &alice.ack, &spend_result.r_s);

    let s_spend = alice.ack * spend_result.r_s;
    let dleq_spend = compute_dleq_native(
        spend_result.r_s,
        Fr::rand(&mut OsRng),
        &alice.ack,
        &spend_ct.epk_1,
        metadata_hash,
    );
    verify_dleq_native(
        &alice.ack,
        &spend_ct.epk_1,
        &s_spend,
        &dleq_spend.c,
        &dleq_spend.s,
        metadata_hash,
    )?;
    results.pass("Spend DLEQ");

    let recovered_spend = run_pre_pipeline(
        &cli,
        &ring,
        &reader_sk,
        &reader_pk,
        seed_spend,
        &alice_derivation,
        "core",
        &mut results,
    )
    .await?;
    if let Some(rs) = recovered_spend {
        let spend_core = decrypt_core_with_seed(rs, spend_ct)?;
        if let Some(sc) = spend_core {
            if sc.amount == value.amount {
                results.pass("Spend core decrypted (amount matches)");
            } else {
                results.fail("Spend amount mismatch");
            }
        } else {
            results.fail("Spend core decryption returned None");
        }
    }

    // ================================================================
    // Phase 10: Negative tests
    // ================================================================
    println!();
    println!("--- Phase 10: Negative tests ---");

    // Tampered DLEQ c
    let bad_c = dleq_core.c + Fq::from(1u64);
    if verify_dleq_native(
        &bob.ack,
        &ct.epk_1,
        &s_core,
        &bad_c,
        &dleq_core.s,
        metadata_hash,
    )
    .is_err()
    {
        results.pass("Tampered c rejected");
    } else {
        results.fail("Tampered c should have been rejected");
    }

    // Wrong metadata
    let wrong_metadata = compute_metadata_hash(
        Fq::from(99u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        Fq::from(1_700_000_000u64),
        Fq::from(0u64),
    );
    if verify_dleq_native(
        &bob.ack,
        &ct.epk_1,
        &s_core,
        &dleq_core.c,
        &dleq_core.s,
        wrong_metadata,
    )
    .is_err()
    {
        results.pass("Wrong metadata rejected");
    } else {
        results.fail("Wrong metadata should have been rejected");
    }

    // FROST tampered message (using local ring)
    if let Some(ref shares) = local_ring.local_shares {
        let asset_id = asset::Id(Fq::from(10001u64));
        let msg = build_ack_registration_message(
            &alice.address,
            &alice.ack,
            &local_ring.ring_pk,
            asset_id,
        );
        let sig = frost_sign_local(shares, &msg)?;
        if verify_ack_signature(&local_ring.ring_pk, b"tampered-message", &sig).is_err() {
            results.pass("FROST tampered message rejected");
        } else {
            results.fail("FROST tampered message should have been rejected");
        }
    }

    // Wrong DK detection
    let wrong_dk = Fr::rand(&mut OsRng);
    if let Ok((wrong_asset_id, _, _)) =
        decrypt_detection_tier(&wrong_dk, &ct.epk_1, &ct.detection_tag, &value.asset_id)
    {
        if wrong_asset_id != value.asset_id {
            results.pass("Wrong DK produces wrong asset_id");
        } else {
            results.fail("Wrong DK should produce wrong asset_id");
        }
    } else {
        results.pass("Wrong DK detection failed (expected)");
    }

    // ================================================================
    // Results
    // ================================================================
    println!();
    println!(
        "=== Results: {} passed, {} failed ===",
        results.passed, results.failed
    );
    if results.failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

// ============================================================================
// PRE pipeline for one tier via cli-tool
// ============================================================================

async fn run_pre_pipeline(
    cli: &cli_tool::CliTool,
    ring: &RingContext,
    reader_sk: &str,
    reader_pk: &str,
    seed: Fq,
    derivation_hex: &str,
    tier: &str,
    results: &mut TestResults,
) -> Result<Option<Fq>> {
    let seed_hex = hex::encode(seed.to_bytes());

    // Create ACP policy
    let policy_id = match cli.add_policy() {
        Ok(id) => id,
        Err(e) => {
            results.fail(&format!("{tier}: create policy failed: {e}"));
            return Ok(None);
        }
    };

    // Store seed in Orbis
    let ring_pk_hex = hex::encode(ring.ring_pk.vartime_compress().0);
    let object_id = match cli.store_secret(
        &seed_hex,
        &ring_pk_hex,
        &ring.ring_id,
        &policy_id,
        derivation_hex,
    ) {
        Ok(id) => id,
        Err(e) => {
            results.fail(&format!("{tier}: store secret failed: {e}"));
            return Ok(None);
        }
    };

    // Register object
    if let Err(e) = cli.register_object(&policy_id, &object_id) {
        results.fail(&format!("{tier}: register object failed: {e}"));
        return Ok(None);
    }

    // PRE before grant (must fail)
    match cli.pre(
        &ring_pk_hex,
        reader_pk,
        reader_sk,
        &object_id,
        derivation_hex,
    ) {
        Ok(_) => results.fail(&format!("{tier}: PRE before grant should fail")),
        Err(_) => results.pass(&format!("{tier}: PRE denied before grant")),
    }

    // Grant permission
    if let Err(e) = cli.set_relationship(&policy_id, &object_id) {
        results.fail(&format!("{tier}: set relationship failed: {e}"));
        return Ok(None);
    }

    // PRE after grant (must succeed, with polling)
    let mut recovered_seed = None;
    for attempt in 1..=5 {
        match cli.pre(
            &ring_pk_hex,
            reader_pk,
            reader_sk,
            &object_id,
            derivation_hex,
        ) {
            Ok(seed_hex) => {
                let bytes = hex::decode(&seed_hex)?;
                recovered_seed = Some(Fq::from_le_bytes_mod_order(&bytes));
                break;
            }
            Err(_) if attempt < 5 => {
                println!("     polling ({attempt}/5)...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            Err(e) => {
                results.fail(&format!("{tier}: PRE after grant failed: {e}"));
                return Ok(None);
            }
        }
    }

    match recovered_seed {
        Some(rs) => {
            if rs == seed {
                results.pass(&format!("{tier}: seed round-trip matches"));
            } else {
                results.fail(&format!("{tier}: seed mismatch"));
            }
            Ok(Some(rs))
        }
        None => {
            results.fail(&format!("{tier}: PRE never succeeded"));
            Ok(None)
        }
    }
}

// ============================================================================
// Local DKG (for FROST and DLEQ tests that need secret shares)
// ============================================================================

fn create_local_ring(n: usize, t: usize) -> Result<RingContext> {
    use crypto::decaf377::dkg::DKGNode;
    use crypto::r#trait::Dkg;
    use crypto::test_helper::DKGCoordinator;

    let mut coordinator = DKGCoordinator::new(
        |id, threshold, total_nodes, session_id| DKGNode::new(id, threshold, total_nodes, session_id),
        n,
        t,
    )?;
    let (ring_pk, secret_shares, pub_poly) = coordinator.run_dkg()?;
    let ring_pk_hex = hex::encode(ring_pk.vartime_compress_to_field().to_bytes());
    Ok(RingContext {
        ring_pk,
        ring_id: format!("local-{}", &ring_pk_hex[..8]),
        local_shares: Some(LocalShares {
            secret_shares,
            pub_poly,
            n,
            t,
        }),
    })
}

// ============================================================================
// FROST helpers
// ============================================================================

fn build_ack_registration_message(
    address: &Address,
    ack: &decaf377::Element,
    ring_pk: &decaf377::Element,
    asset_id: asset::Id,
) -> Vec<u8> {
    use ark_serialize::CanonicalSerialize;
    let mut msg = Vec::new();
    msg.extend_from_slice(b"orbis-ack-registration-v1");
    address
        .diversified_generator()
        .serialize_compressed(&mut msg)
        .expect("serialize B_d");
    msg.extend_from_slice(&address.transmission_key().0);
    ack.serialize_compressed(&mut msg).expect("serialize ACK");
    ring_pk
        .serialize_compressed(&mut msg)
        .expect("serialize ring_pk");
    msg.extend_from_slice(&asset_id.0.to_bytes());
    msg
}

fn verify_ack_signature(
    ring_pk: &decaf377::Element,
    msg: &[u8],
    signature: &crypto::decaf377::sign::SchnorrSignature,
) -> Result<()> {
    use crypto::decaf377::sign::ThresholdDecafSigner;
    use crypto::r#trait::ThresholdSigner;
    let verifier = ThresholdDecafSigner::new();
    verifier.verify(ring_pk, msg, signature)?;
    Ok(())
}
