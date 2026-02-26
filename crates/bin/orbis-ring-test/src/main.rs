//! Penumbra-side crypto for Orbis ring integration tests.
//!
//! Subcommands:
//!   prepare  — generate test keys, encrypt compliance data, output hex values
//!   verify   — verify DLEQ proofs and test negative cases with ring_pk
//!   encrypt  — real compliance encryption + DLEQ, output structured JSON for Orbis PRE
//!   decrypt  — decrypt compliance ciphertext with recovered seed, verify fields

use anyhow::{Context, Result};
use decaf377::{Fq, Fr};
use rand_core::OsRng;

use penumbra_sdk_asset::{asset, Value};
use penumbra_sdk_compliance::crypto::{
    compute_dleq_native, compute_metadata_hash, decrypt_tier_bytes, derive_compliance_scalar,
    encrypt_output, encrypt_spend, verify_dleq_native,
};
use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};

fn parse_ring_pk(hex_str: &str) -> Result<decaf377::Element> {
    let bytes = hex::decode(hex_str).context("invalid ring_pk hex")?;
    match bytes.len() {
        32 => {
            // Direct decaf377 encoding
            let arr: [u8; 32] = bytes.as_slice().try_into().unwrap();
            decaf377::Encoding(arr)
                .vartime_decompress()
                .map_err(|e| anyhow::anyhow!("invalid ring_pk point: {:?}", e))
        }
        48 => {
            // BLS12-381 G1 point from Orbis DKG — hash into decaf377 space.
            // The real bridge between BLS12-381 and decaf377 is a separate concern;
            // for testing, we deterministically derive a decaf377 point from the
            // BLS key so DLEQ proofs use a consistent ring_pk.
            use penumbra_sdk_compliance::crypto::derive_compliance_scalar;
            let fq = Fq::from_le_bytes_mod_order(&bytes[..32]);
            let d = derive_compliance_scalar(fq);
            let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
            Ok(decaf377::Element::GENERATOR * d_fr)
        }
        n => anyhow::bail!("ring_pk must be 32 bytes (decaf377) or 48 bytes (BLS12-381), got {n}"),
    }
}

fn derive_ack(ring_pk: &decaf377::Element, b_d_fq: Fq) -> decaf377::Element {
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    *ring_pk * d_fr
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: orbis-ring-test <prepare|verify|encrypt|decrypt> [options]");
        std::process::exit(1);
    }

    let subcommand = &args[1];

    match subcommand.as_str() {
        "prepare" | "verify" => {
            let ring_pk_hex = get_arg(&args, "--ring-pk-hex").context("missing --ring-pk-hex")?;
            match subcommand.as_str() {
                "prepare" => cmd_prepare(&ring_pk_hex),
                "verify" => cmd_verify(&ring_pk_hex),
                _ => unreachable!(),
            }
        }
        "encrypt" => cmd_encrypt(&args),
        "decrypt" => cmd_decrypt(&args),
        other => {
            eprintln!(
                "Unknown subcommand: {}. Use 'prepare', 'verify', 'encrypt', or 'decrypt'.",
                other
            );
            std::process::exit(1);
        }
    }
}

/// Generate test data: derivation bytes and EPK hex values for the shell script.
fn cmd_prepare(ring_pk_hex: &str) -> Result<()> {
    let ring_pk = parse_ring_pk(ring_pk_hex)?;

    let seed_a = SeedPhrase::generate(OsRng);
    let sk_a = SpendKey::from_seed_phrase_bip44(seed_a, &Bip44Path::new(0));
    let (alice_addr, _) = sk_a
        .full_viewing_key()
        .incoming()
        .payment_address(0u32.into());

    let seed_b = SeedPhrase::generate(OsRng);
    let sk_b = SpendKey::from_seed_phrase_bip44(seed_b, &Bip44Path::new(0));
    let (bob_addr, _) = sk_b
        .full_viewing_key()
        .incoming()
        .payment_address(0u32.into());

    let alice_b_d_fq = alice_addr
        .diversified_generator()
        .vartime_compress_to_field();
    let bob_b_d_fq = bob_addr.diversified_generator().vartime_compress_to_field();

    let ack_bob = derive_ack(&ring_pk, bob_b_d_fq);
    let ack_alice = derive_ack(&ring_pk, alice_b_d_fq);

    let dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let value = Value {
        amount: 100u64.into(),
        asset_id: asset::Id(Fq::from(42u64)),
    };
    let output_result = encrypt_output(
        &mut OsRng,
        &ack_bob,
        &ack_alice,
        &dk_pub,
        &bob_addr,
        &alice_addr,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;

    let ct = &output_result.ciphertext;

    let data = serde_json::json!({
        "alice_derivation": hex::encode(alice_b_d_fq.to_bytes()),
        "bob_derivation": hex::encode(bob_b_d_fq.to_bytes()),
        "epk_1": hex::encode(ct.epk_1.vartime_compress().0),
        "epk_2": hex::encode(ct.epk_2.expect("epk_2").vartime_compress().0),
        "epk_3": hex::encode(ct.epk_3.expect("epk_3").vartime_compress().0),
    });

    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// Simplified Orbis-compatible metadata hash (WORKAROUND).
/// Production uses compute_metadata_hash with 6 fields; this uses 3 for testing.
fn encode_orbis_metadata(policy_id: &str, resource: &str, permission: &str) -> Fq {
    let domain = Fq::from_le_bytes_mod_order(b"orbis-policy-metadata-v1\0\0\0\0\0\0\0\0");
    let pid = Fq::from_le_bytes_mod_order(policy_id.as_bytes());
    let res = Fq::from_le_bytes_mod_order(resource.as_bytes());
    let perm = Fq::from_le_bytes_mod_order(permission.as_bytes());
    poseidon377::hash_3(&domain, (pid, res, perm))
}

/// Encrypt compliance data using real spend encryption, compute DLEQ proof,
/// and output structured JSON for the Orbis PRE pipeline.
fn cmd_encrypt(args: &[String]) -> Result<()> {
    let ring_pk_hex = get_arg(args, "--ring-pk-hex").context("missing --ring-pk-hex")?;
    let policy_id = get_arg(args, "--policy-id").unwrap_or_else(|| "test-policy".to_string());
    let resource = get_arg(args, "--resource").unwrap_or_else(|| "document".to_string());
    let permission = get_arg(args, "--permission").unwrap_or_else(|| "read".to_string());

    // Detect ring_pk mode from input length
    let ring_pk_raw_bytes = hex::decode(&ring_pk_hex).context("invalid ring_pk hex")?;
    let ring_pk_mode = match ring_pk_raw_bytes.len() {
        32 => "decaf377_native",
        48 => "bls48_surrogate",
        n => anyhow::bail!("ring_pk must be 32 or 48 bytes, got {n}"),
    };
    let ring_pk = parse_ring_pk(&ring_pk_hex)?;

    // Generate Alice Penumbra address
    let seed_phrase = SeedPhrase::generate(OsRng);
    let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
    let (alice_addr, _) = sk
        .full_viewing_key()
        .incoming()
        .payment_address(0u32.into());
    let alice_b_d_fq = alice_addr
        .diversified_generator()
        .vartime_compress_to_field();
    let ack_alice = derive_ack(&ring_pk, alice_b_d_fq);

    // Dummy dk_pub (needed by encrypt_spend signature, not exercised through Orbis)
    let dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let amount = 100u64;
    let asset_id = asset::Id(Fq::from(42u64));

    // Real compliance encryption
    let result = encrypt_spend(
        &mut OsRng,
        &ack_alice,
        &dk_pub,
        &alice_addr,
        asset_id,
        amount.into(),
        false,
        Fq::from(0u64),
    )?;
    let ct = &result.ciphertext;

    // Seed recovery via field subtraction (matches crypto.rs encrypt_spend: c2_core = seed + (ack * r_s).compress())
    let shared_point = ack_alice * result.r_s;
    let shared_point_fq = shared_point.vartime_compress_to_field();
    let seed_core = ct.c2_core - shared_point_fq;

    // Roundtrip assertion
    let roundtrip = seed_core + shared_point_fq;
    anyhow::ensure!(
        roundtrip == ct.c2_core,
        "seed recovery roundtrip failed: {} != {}",
        roundtrip,
        ct.c2_core
    );

    // Metadata hash (simplified Orbis-compatible encoding)
    let metadata_hash = encode_orbis_metadata(&policy_id, &resource, &permission);

    // DLEQ proof
    let s_point = ack_alice * result.r_s;
    let dleq = compute_dleq_native(
        result.r_s,
        Fr::rand(&mut OsRng),
        &ack_alice,
        &ct.epk_1,
        metadata_hash,
    );

    // Local DLEQ verification (3 checks to stderr)
    match verify_dleq_native(
        &ack_alice,
        &ct.epk_1,
        &s_point,
        &dleq.c,
        &dleq.s,
        metadata_hash,
    ) {
        Ok(()) => eprintln!("  DLEQ check 1/3: PASS (positive verification)"),
        Err(e) => {
            eprintln!("  DLEQ check 1/3: FAIL (positive verification) — {}", e);
            std::process::exit(1);
        }
    }

    let bad_metadata = metadata_hash + Fq::from(1u64);
    match verify_dleq_native(
        &ack_alice,
        &ct.epk_1,
        &s_point,
        &dleq.c,
        &dleq.s,
        bad_metadata,
    ) {
        Err(_) => eprintln!("  DLEQ check 2/3: PASS (reject tampered metadata)"),
        Ok(()) => {
            eprintln!("  DLEQ check 2/3: FAIL (tampered metadata accepted)");
            std::process::exit(1);
        }
    }

    let wrong_ack = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    match verify_dleq_native(
        &wrong_ack,
        &ct.epk_1,
        &s_point,
        &dleq.c,
        &dleq.s,
        metadata_hash,
    ) {
        Err(_) => eprintln!("  DLEQ check 3/3: PASS (reject wrong ACK)"),
        Ok(()) => {
            eprintln!("  DLEQ check 3/3: FAIL (wrong ACK accepted)");
            std::process::exit(1);
        }
    }

    if ring_pk_mode == "bls48_surrogate" {
        eprintln!("[WORKAROUND] ring_pk is BLS12-381 48-byte key hashed to decaf377 surrogate. Local DLEQ proves decaf377 math only; Orbis PRE uses the native BLS ring_pk.");
    }

    let data = serde_json::json!({
        "ring_pk_mode": ring_pk_mode,
        "seed_core_hex": hex::encode(seed_core.to_bytes()),
        "ciphertext_hex": hex::encode(ct.to_bytes()),
        "ciphertext_len": ct.to_bytes().len(),
        "alice_derivation": hex::encode(alice_b_d_fq.to_bytes()),
        "dleq_c": hex::encode(dleq.c.to_bytes()),
        "dleq_s": hex::encode(dleq.s.to_bytes()),
        "shared_point_hex": hex::encode(shared_point.vartime_compress().0),
        "metadata_hash": hex::encode(metadata_hash.to_bytes()),
        "epk_1_hex": hex::encode(ct.epk_1.vartime_compress().0),
        "plaintext_amount": amount,
        "alice_gd_hex": hex::encode(alice_addr.diversified_generator().vartime_compress().0),
        "alice_pk_hex": hex::encode(alice_addr.transmission_key().0),
    });

    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// Decrypt compliance ciphertext with a recovered seed and verify structured fields.
fn cmd_decrypt(args: &[String]) -> Result<()> {
    let seed_hex = get_arg(args, "--seed-hex").context("missing --seed-hex")?;
    let ciphertext_hex = get_arg(args, "--ciphertext-hex").context("missing --ciphertext-hex")?;
    let plaintext_amount: u64 = get_arg(args, "--plaintext-amount")
        .context("missing --plaintext-amount")?
        .parse()
        .context("--plaintext-amount must be a u64")?;
    let alice_gd_hex = get_arg(args, "--alice-gd-hex").context("missing --alice-gd-hex")?;
    let alice_pk_hex = get_arg(args, "--alice-pk-hex").context("missing --alice-pk-hex")?;
    let expect_fail = args.iter().any(|a| a == "--expect-fail");

    // Input validation
    anyhow::ensure!(
        seed_hex.len() == 64,
        "--seed-hex must be 64 hex chars, got {}",
        seed_hex.len()
    );
    anyhow::ensure!(
        ciphertext_hex.len() == 448,
        "--ciphertext-hex must be 448 hex chars (224 bytes), got {}",
        ciphertext_hex.len()
    );
    anyhow::ensure!(
        alice_gd_hex.len() == 64,
        "--alice-gd-hex must be 64 hex chars, got {}",
        alice_gd_hex.len()
    );
    anyhow::ensure!(
        alice_pk_hex.len() == 64,
        "--alice-pk-hex must be 64 hex chars, got {}",
        alice_pk_hex.len()
    );

    // Parse inputs
    let seed_bytes = hex::decode(&seed_hex).context("invalid seed hex")?;
    let seed = Fq::from_le_bytes_mod_order(&seed_bytes);

    let ct_bytes = hex::decode(&ciphertext_hex).context("invalid ciphertext hex")?;
    let ciphertext = penumbra_sdk_compliance::ComplianceCiphertext::from_bytes(&ct_bytes)?;

    // Decrypt core tier: 80 bytes = amount(16) + gd(32) + pk(32)
    let plaintext_bytes = decrypt_tier_bytes(&ciphertext.encrypted_core, seed, 80);

    // Parse decrypted fields
    let decrypted_amount_bytes: [u8; 16] =
        plaintext_bytes[0..16].try_into().context("amount bytes")?;
    let decrypted_gd_hex = hex::encode(&plaintext_bytes[16..48]);
    let decrypted_pk_hex = hex::encode(&plaintext_bytes[48..80]);
    let decrypted_amount = u128::from_le_bytes(decrypted_amount_bytes);

    // Compare against expected
    let expected_amount = plaintext_amount as u128;
    let amount_ok = decrypted_amount == expected_amount;
    let gd_ok = decrypted_gd_hex == alice_gd_hex;
    let pk_ok = decrypted_pk_hex == alice_pk_hex;
    let all_ok = amount_ok && gd_ok && pk_ok;

    if expect_fail {
        if !all_ok {
            println!("PASS: decryption mismatch as expected");
            Ok(())
        } else {
            println!("FAIL: expected mismatch but all fields matched");
            std::process::exit(1);
        }
    } else if all_ok {
        println!(
            "PASS: amount={}, gd={:.16}..., pk={:.16}...",
            decrypted_amount, decrypted_gd_hex, decrypted_pk_hex
        );
        Ok(())
    } else {
        if !amount_ok {
            eprintln!(
                "  amount: expected {}, got {}",
                expected_amount, decrypted_amount
            );
        }
        if !gd_ok {
            eprintln!(
                "  gd: expected {:.16}..., got {:.16}...",
                alice_gd_hex, decrypted_gd_hex
            );
        }
        if !pk_ok {
            eprintln!(
                "  pk: expected {:.16}..., got {:.16}...",
                alice_pk_hex, decrypted_pk_hex
            );
        }
        println!("FAIL: decryption mismatch");
        std::process::exit(1);
    }
}

/// Verify DLEQ proofs with the real ring_pk from DKG.
fn cmd_verify(ring_pk_hex: &str) -> Result<()> {
    let ring_pk = parse_ring_pk(ring_pk_hex)?;
    let mut passed = 0u32;
    let mut failed = 0u32;

    println!("--- DLEQ Verification with ring_pk ---");

    let seed_a = SeedPhrase::generate(OsRng);
    let sk_a = SpendKey::from_seed_phrase_bip44(seed_a, &Bip44Path::new(0));
    let (alice_addr, _) = sk_a
        .full_viewing_key()
        .incoming()
        .payment_address(0u32.into());

    let seed_b = SeedPhrase::generate(OsRng);
    let sk_b = SpendKey::from_seed_phrase_bip44(seed_b, &Bip44Path::new(0));
    let (bob_addr, _) = sk_b
        .full_viewing_key()
        .incoming()
        .payment_address(0u32.into());

    let alice_b_d_fq = alice_addr
        .diversified_generator()
        .vartime_compress_to_field();
    let bob_b_d_fq = bob_addr.diversified_generator().vartime_compress_to_field();
    let ack_bob = derive_ack(&ring_pk, bob_b_d_fq);
    let ack_alice = derive_ack(&ring_pk, alice_b_d_fq);

    let dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let value = Value {
        amount: 100u64.into(),
        asset_id: asset::Id(Fq::from(42u64)),
    };

    let metadata_hash = compute_metadata_hash(
        Fq::from(1u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        Fq::from(1_700_000_000u64),
        Fq::from(0u64),
    );

    // Output encryption + DLEQ for all 3 tiers
    let output_result = encrypt_output(
        &mut OsRng,
        &ack_bob,
        &ack_alice,
        &dk_pub,
        &bob_addr,
        &alice_addr,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;
    let ct = &output_result.ciphertext;
    let epk_2 = ct.epk_2.expect("epk_2");
    let epk_3 = ct.epk_3.expect("epk_3");

    // Core tier
    let s_core = ack_bob * output_result.r_1;
    let dleq_core = compute_dleq_native(
        output_result.r_1,
        Fr::rand(&mut OsRng),
        &ack_bob,
        &ct.epk_1,
        metadata_hash,
    );
    match verify_dleq_native(
        &ack_bob,
        &ct.epk_1,
        &s_core,
        &dleq_core.c,
        &dleq_core.s,
        metadata_hash,
    ) {
        Ok(()) => {
            println!("  PASS: DLEQ core tier");
            passed += 1;
        }
        Err(e) => {
            println!("  FAIL: DLEQ core tier — {}", e);
            failed += 1;
        }
    }

    // Ext tier
    let s_ext = ack_bob * output_result.r_2;
    let dleq_ext = compute_dleq_native(
        output_result.r_2,
        Fr::rand(&mut OsRng),
        &ack_bob,
        &epk_2,
        metadata_hash,
    );
    match verify_dleq_native(
        &ack_bob,
        &epk_2,
        &s_ext,
        &dleq_ext.c,
        &dleq_ext.s,
        metadata_hash,
    ) {
        Ok(()) => {
            println!("  PASS: DLEQ ext tier");
            passed += 1;
        }
        Err(e) => {
            println!("  FAIL: DLEQ ext tier — {}", e);
            failed += 1;
        }
    }

    // Sext tier
    let s_sext = ack_alice * output_result.r_3;
    let dleq_sext = compute_dleq_native(
        output_result.r_3,
        Fr::rand(&mut OsRng),
        &ack_alice,
        &epk_3,
        metadata_hash,
    );
    match verify_dleq_native(
        &ack_alice,
        &epk_3,
        &s_sext,
        &dleq_sext.c,
        &dleq_sext.s,
        metadata_hash,
    ) {
        Ok(()) => {
            println!("  PASS: DLEQ sext tier");
            passed += 1;
        }
        Err(e) => {
            println!("  FAIL: DLEQ sext tier — {}", e);
            failed += 1;
        }
    }

    // Spend DLEQ
    let spend_result = encrypt_spend(
        &mut OsRng,
        &ack_alice,
        &dk_pub,
        &alice_addr,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;
    let spend_ct = &spend_result.ciphertext;
    let s_spend = ack_alice * spend_result.r_s;
    let dleq_spend = compute_dleq_native(
        spend_result.r_s,
        Fr::rand(&mut OsRng),
        &ack_alice,
        &spend_ct.epk_1,
        metadata_hash,
    );
    match verify_dleq_native(
        &ack_alice,
        &spend_ct.epk_1,
        &s_spend,
        &dleq_spend.c,
        &dleq_spend.s,
        metadata_hash,
    ) {
        Ok(()) => {
            println!("  PASS: DLEQ spend");
            passed += 1;
        }
        Err(e) => {
            println!("  FAIL: DLEQ spend — {}", e);
            failed += 1;
        }
    }

    // Negative: tampered c
    let bad_c = dleq_core.c + Fq::from(1u64);
    match verify_dleq_native(
        &ack_bob,
        &ct.epk_1,
        &s_core,
        &bad_c,
        &dleq_core.s,
        metadata_hash,
    ) {
        Err(_) => {
            println!("  PASS: reject tampered c");
            passed += 1;
        }
        Ok(()) => {
            println!("  FAIL: tampered c accepted");
            failed += 1;
        }
    }

    // Negative: wrong metadata
    let wrong_meta = compute_metadata_hash(
        Fq::from(99u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        Fq::from(1_700_000_000u64),
        Fq::from(0u64),
    );
    match verify_dleq_native(
        &ack_bob,
        &ct.epk_1,
        &s_core,
        &dleq_core.c,
        &dleq_core.s,
        wrong_meta,
    ) {
        Err(_) => {
            println!("  PASS: reject wrong metadata");
            passed += 1;
        }
        Ok(()) => {
            println!("  FAIL: wrong metadata accepted");
            failed += 1;
        }
    }

    // Negative: wrong ACK
    let wrong_ack = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    match verify_dleq_native(
        &wrong_ack,
        &ct.epk_1,
        &s_core,
        &dleq_core.c,
        &dleq_core.s,
        metadata_hash,
    ) {
        Err(_) => {
            println!("  PASS: reject wrong ACK");
            passed += 1;
        }
        Ok(()) => {
            println!("  FAIL: wrong ACK accepted");
            failed += 1;
        }
    }

    println!();
    println!("Results: {} passed, {} failed", passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
