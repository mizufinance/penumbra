//! Hybrid KEM/DEM encryption for compliance data, compatible with Orbis PRE.
//!
//! Random seeds are encrypted in ElGamal envelopes (C2 fields) and key a Poseidon
//! stream cipher. Three tiers: detection (issuer-only, always), core (amount + self
//! address), extension (counterparty), sender-extension (sender's copy).
//!
//! Unflagged transactions encrypt core/ext/sext to per-tier ACKs derived from ring_pk.
//! Flagged transactions encrypt all tiers to issuer DK_pub.
//!
//! ## Abbreviations
//! ss = shared secret, ct = ciphertext, pt = plaintext, esk = ephemeral secret key,
//! epk = ephemeral public key, fq = field element (Fq), dk = detection key

use anyhow::Context;
use ark_ff::Zero;
use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use rand_core::{CryptoRng, RngCore};

use sha2::{Digest, Sha256};

use crate::issuer_keys::DETECTION_TIER_BYTES;
use crate::structs::{ComplianceCiphertext, DleqProof};

/// Domain separator for SHA256 derivation — matches Orbis `DERIVATION_DOMAIN` exactly.
const DERIVATION_DOMAIN: &[u8; 23] = b"elgamal-derivation-v1\0\0";

/// Derive the compliance scalar `d` from the diversified basepoint field element.
///
/// `d = Fr::from_le_bytes_mod_order(SHA256(DERIVATION_DOMAIN || b_d_fq.to_bytes()))`
///
/// This MUST match Orbis's `derive_capability_scalar()` so PRE math cancels correctly.
/// The result is stored as Fq in the compliance leaf (Fr fits losslessly in Fq for decaf377).
pub fn derive_compliance_scalar(b_d_fq: Fq) -> Fq {
    let mut hasher = Sha256::new();
    hasher.update(DERIVATION_DOMAIN);
    hasher.update(b_d_fq.to_bytes());
    let hash = hasher.finalize();
    // Reduce mod r first (matching Orbis's Fr::from_le_bytes_mod_order), then embed into Fq.
    // r < q for decaf377, so this conversion is lossless.
    let fr = Fr::from_le_bytes_mod_order(&hash);
    Fq::from_le_bytes_mod_order(&fr.to_bytes())
}

/// Domain separator for Poseidon stream cipher seed derivation.
pub static COMPLIANCE_STREAM_CIPHER_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.poseidon_stream").as_bytes(),
    )
});

/// The "black hole" compliance key for unregulated assets.
///
/// For unregulated assets, compliance data is encrypted to this key, making it
/// effectively unrecoverable since no one knows the discrete log.
/// This is a NUMS point derived from a domain separator.
pub static BLACK_HOLE_ACK: Lazy<Element> = Lazy::new(|| {
    let hash = blake2b_simd::blake2b(b"penumbra.compliance.black_hole_ack");
    let scalar = Fr::from_le_bytes_mod_order(hash.as_bytes());
    Element::GENERATOR * scalar
});

/// Decrypted compliance data.
#[derive(Clone, Debug)]
pub struct DecryptedComplianceData {
    pub asset_id: asset::Id,
    pub amount: Amount,
    pub self_diversified_generator: Element,
    pub self_transmission_key: [u8; 32],
    pub counterparty_diversified_generator: Element,
    pub counterparty_transmission_key: [u8; 32],
}

/// Domain separator for issuer detection tier encryption.
pub static ISSUER_DETECTION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.issuer_detection").as_bytes(),
    )
});

/// Domain separator for DLEQ metadata hash: M = Poseidon_6(domain, fields...).
pub static DLEQ_METADATA_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.dleq_metadata").as_bytes(),
    )
});

/// Domain separator for DLEQ Fiat-Shamir challenge: c = Poseidon_6(domain, points...).
pub static DLEQ_CHALLENGE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.dleq_challenge").as_bytes(),
    )
});

/// Orbis-compatible domain separator for encryption proof challenge hash.
/// Must match Orbis `ENCRYPT_PROOF_DOMAIN` exactly (commit 4b61fa4).
pub const ENCRYPT_PROOF_DOMAIN: &[u8; 24] = b"elgamal-encrypt-proof-v1";

/// Truncate a Poseidon Fq output to a challenge scalar.
///
/// Masks the top bits so the result is in [0, 2^{MODULUS_BIT_SIZE-1}),
/// strictly less than Fr modulus. Avoids in-circuit modular reduction.
/// Must match Orbis `fq_to_challenge_scalar` (commit 4b61fa4).
pub fn fq_to_challenge_scalar(fq: Fq) -> Fr {
    use ark_ff::{BigInteger, PrimeField};
    let mut bytes = fq.into_bigint().to_bytes_le();
    let keep_bits = Fr::MODULUS_BIT_SIZE - 1;
    let keep_bytes = (keep_bits as usize + 7) / 8;
    let spare_bits = keep_bytes * 8 - keep_bits as usize;
    bytes[keep_bytes - 1] &= 0xFF >> spare_bits;
    Fr::from_le_bytes_mod_order(&bytes)
}

/// Compute the salted metadata hash: M = Poseidon_6(domain, (policy_id_hash, resource_hash, permission_hash, tier, target_timestamp, salt)).
pub fn compute_metadata_hash(
    policy_id_hash: Fq,
    resource_hash: Fq,
    permission_hash: Fq,
    tier: Fq,
    target_timestamp: Fq,
    salt: Fq,
) -> Fq {
    poseidon377::hash_6(
        &DLEQ_METADATA_DOMAIN,
        (
            policy_id_hash,
            resource_hash,
            permission_hash,
            tier,
            target_timestamp,
            salt,
        ),
    )
}

/// Compute a single DLEQ proof natively (prover side).
///
/// Proves that EPK = r×G and S = r×ACK share the same scalar r, bound to metadata M.
/// Uses Orbis-compatible hash_7 with ENCRYPT_PROOF_DOMAIN and truncated challenge.
/// Returns (c, s) where c is the truncated Poseidon output (high bits zeroed).
pub fn compute_dleq_native(
    r: Fr,
    k: Fr,
    ack: &Element,
    epk: &Element,
    metadata_hash: Fq,
) -> DleqProof {
    let s_point = *ack * r; // S = r × ACK
    let r_point = Element::GENERATOR * k; // R = k × G
    let rp_point = *ack * k; // R' = k × ACK

    // Compress points to Fq for hashing (matches Orbis point_to_fq)
    let g_fq = Element::GENERATOR.vartime_compress_to_field();
    let ack_fq = ack.vartime_compress_to_field();
    let epk_fq = epk.vartime_compress_to_field();
    let s_fq = s_point.vartime_compress_to_field();
    let r_fq = r_point.vartime_compress_to_field();
    let rp_fq = rp_point.vartime_compress_to_field();

    // Fiat-Shamir challenge via hash_7 (Orbis-compatible ordering: M, G, ACK, EPK, S, R, R')
    let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
    let c_fq_full = poseidon377::hash_7(
        &domain,
        (metadata_hash, g_fq, ack_fq, epk_fq, s_fq, r_fq, rp_fq),
    );

    // Truncate to 252 bits (matches Orbis fq_to_challenge_scalar)
    let c_truncated = fq_to_challenge_scalar(c_fq_full);
    let s = k + c_truncated * r;

    // Store c as Fq with high bits zero
    DleqProof {
        c: Fq::from_le_bytes_mod_order(&c_truncated.to_bytes()),
        s,
    }
}

/// Verify a DLEQ proof given only public inputs (no secret key needed).
///
/// S = r × ACK is provided by the prover alongside the proof.
pub fn verify_dleq_native(
    ack: &Element,
    epk: &Element,
    s_point: &Element,
    dleq_c: &Fq,
    dleq_s: &Fr,
    metadata_hash: Fq,
) -> anyhow::Result<()> {
    let c_fr = Fr::from_le_bytes_mod_order(&dleq_c.to_bytes());

    // Reconstruct R and R' from the DLEQ response
    let r_rec = Element::GENERATOR * *dleq_s - *epk * c_fr;
    let rp_rec = *ack * *dleq_s - *s_point * c_fr;

    // Recompute challenge via hash_7 with Orbis-compatible ordering
    let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
    let g_fq = Element::GENERATOR.vartime_compress_to_field();
    let c_check = poseidon377::hash_7(
        &domain,
        (
            metadata_hash,
            g_fq,
            ack.vartime_compress_to_field(),
            epk.vartime_compress_to_field(),
            s_point.vartime_compress_to_field(),
            r_rec.vartime_compress_to_field(),
            rp_rec.vartime_compress_to_field(),
        ),
    );
    let c_check_trunc = Fq::from_le_bytes_mod_order(&fq_to_challenge_scalar(c_check).to_bytes());

    if c_check_trunc != *dleq_c {
        return Err(anyhow::anyhow!(
            "DLEQ verification failed: challenge mismatch"
        ));
    }
    Ok(())
}

/// Compute DLEQ proof for a Spend action (1 tier: core, tier=1).
pub fn compute_spend_dleq(r_s: Fr, k: Fr, ack: &Element, metadata_hash: Fq) -> DleqProof {
    let epk = Element::GENERATOR * r_s;
    compute_dleq_native(r_s, k, ack, &epk, metadata_hash)
}

/// Compute DLEQ proofs for an Output action (3 tiers: core=1, ext=2, sext=3).
///
/// Returns (core_proof, ext_proof, sext_proof).
pub fn compute_output_dleqs(
    r_1: Fr,
    r_2: Fr,
    r_3: Fr,
    k_1: Fr,
    k_2: Fr,
    k_3: Fr,
    ack_receiver: &Element,
    ack_sender: &Element,
    metadata_hash: Fq,
) -> (DleqProof, DleqProof, DleqProof) {
    let epk_1 = Element::GENERATOR * r_1;
    let epk_2 = Element::GENERATOR * r_2;
    let epk_3 = Element::GENERATOR * r_3;

    let core_proof = compute_dleq_native(r_1, k_1, ack_receiver, &epk_1, metadata_hash);
    let ext_proof = compute_dleq_native(r_2, k_2, ack_receiver, &epk_2, metadata_hash);
    let sext_proof = compute_dleq_native(r_3, k_3, ack_sender, &epk_3, metadata_hash);

    (core_proof, ext_proof, sext_proof)
}

/// Encryption result for a Spend action.
#[derive(Clone, Debug)]
pub struct SpendEncryptionResult {
    pub ciphertext: ComplianceCiphertext,
    /// Ephemeral secret r_s (circuit witness).
    pub r_s: Fr,
    /// r_s × DK_pub (for issuer detection).
    pub issuer_shared_secret: Element,
}

/// Encryption result for an Output action.
#[derive(Clone, Debug)]
pub struct OutputEncryptionResult {
    pub ciphertext: ComplianceCiphertext,
    /// Ephemeral secrets (circuit witnesses).
    pub r_1: Fr,
    pub r_2: Fr,
    pub r_3: Fr,
    /// r_1 × DK_pub (for issuer detection).
    pub issuer_shared_secret: Element,
}

/// Encrypt compliance details for a Spend action (detection + core).
///
/// EPK_1 = r_s × G. Detection via r_s × DK_pub.
/// Core C2: seed + (r_s × ACK_core).compress(). Flagged: seed + (r_s × DK_pub).compress().
pub fn encrypt_spend(
    mut rng: impl RngCore + CryptoRng,
    ack_core: &Element,
    dk_pub: &Element,
    self_address: &Address,
    asset_id: asset::Id,
    amount: Amount,
    is_flagged: bool,
    salt: Fq,
) -> anyhow::Result<SpendEncryptionResult> {
    let r_s = Fr::rand(&mut rng);
    let epk_1 = Element::GENERATOR * r_s;
    let ss_issuer = *dk_pub * r_s;

    let seed_core = Fq::rand(&mut rng);

    let c2_core = if is_flagged {
        seed_core + ss_issuer.vartime_compress_to_field()
    } else {
        let ss_core = *ack_core * r_s;
        seed_core + ss_core.vartime_compress_to_field()
    };

    let detection_tag = compute_detection_tier(&ss_issuer, &epk_1, &asset_id, is_flagged, salt);

    let encrypted_core =
        encrypt_tier_bytes(&amount_and_address_bytes(&amount, self_address), seed_core);

    Ok(SpendEncryptionResult {
        ciphertext: ComplianceCiphertext::new_spend(epk_1, c2_core, detection_tag, encrypted_core),
        r_s,
        issuer_shared_secret: ss_issuer,
    })
}

/// Encrypt compliance details for an Output action (detection + core + ext + sext).
///
/// Three independent r_1, r_2, r_3. Detection via r_1 × DK_pub.
/// Core/ext use `ack_receiver`, sext uses `ack_sender`. Flagged: all use r_i × DK_pub.
pub fn encrypt_output(
    mut rng: impl RngCore + CryptoRng,
    ack_receiver: &Element,
    ack_sender: &Element,
    dk_pub: &Element,
    self_address: &Address,
    counterparty_address: &Address,
    asset_id: asset::Id,
    amount: Amount,
    is_flagged: bool,
    salt: Fq,
) -> anyhow::Result<OutputEncryptionResult> {
    let r_1 = Fr::rand(&mut rng);
    let r_2 = Fr::rand(&mut rng);
    let r_3 = Fr::rand(&mut rng);

    let epk_1 = Element::GENERATOR * r_1;
    let epk_2 = Element::GENERATOR * r_2;
    let epk_3 = Element::GENERATOR * r_3;

    let ss_issuer = *dk_pub * r_1;

    let seed_core = Fq::rand(&mut rng);
    let seed_ext = Fq::rand(&mut rng);
    let seed_sext = Fq::rand(&mut rng);

    let (c2_core, c2_ext, c2_sext) = if is_flagged {
        let ss_1 = ss_issuer.vartime_compress_to_field();
        let ss_2 = (*dk_pub * r_2).vartime_compress_to_field();
        let ss_3 = (*dk_pub * r_3).vartime_compress_to_field();
        (seed_core + ss_1, seed_ext + ss_2, seed_sext + ss_3)
    } else {
        let ss_core = (*ack_receiver * r_1).vartime_compress_to_field();
        let ss_ext_v = (*ack_receiver * r_2).vartime_compress_to_field();
        let ss_sext_v = (*ack_sender * r_3).vartime_compress_to_field();
        (
            seed_core + ss_core,
            seed_ext + ss_ext_v,
            seed_sext + ss_sext_v,
        )
    };

    let detection_tag = compute_detection_tier(&ss_issuer, &epk_1, &asset_id, is_flagged, salt);

    // Core: amount + self address (80 bytes → 3 Fq)
    let encrypted_core =
        encrypt_tier_bytes(&amount_and_address_bytes(&amount, self_address), seed_core);

    // Extension: counterparty address (64 bytes → 3 Fq)
    let encrypted_ext = encrypt_tier_bytes(&address_bytes(counterparty_address), seed_ext);

    // Sender-extension: amount + self address (80 bytes → 3 Fq)
    let encrypted_sext =
        encrypt_tier_bytes(&amount_and_address_bytes(&amount, self_address), seed_sext);

    Ok(OutputEncryptionResult {
        ciphertext: ComplianceCiphertext::new_output(
            epk_1,
            epk_2,
            epk_3,
            c2_core,
            c2_ext,
            c2_sext,
            detection_tag,
            encrypted_core,
            encrypted_ext,
            encrypted_sext,
        ),
        r_1,
        r_2,
        r_3,
        issuer_shared_secret: ss_issuer,
    })
}

/// Compute the detection tier for a compliance ciphertext.
///
/// Derives the Poseidon seed from `ss_issuer` and `epk_1`, then encrypts
/// the detection plaintext (asset_id with optional flag bit) and salt.
/// Returns [asset_id+flag (32 bytes), salt (32 bytes)] = 64 bytes.
fn compute_detection_tier(
    ss_issuer: &Element,
    epk_1: &Element,
    asset_id: &asset::Id,
    is_flagged: bool,
    salt: Fq,
) -> [u8; DETECTION_TIER_BYTES] {
    let epk_1_fq = epk_1.vartime_compress_to_field();
    let seed_detection = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss_issuer.vartime_compress_to_field(), epk_1_fq),
    );
    // ct[0]: asset_id + flag + keystream_0
    let pt_fq = crate::issuer_keys::detection_plaintext_fq(asset_id, is_flagged);
    let keystream_0 = poseidon377::hash_2(&seed_detection, (Fq::zero(), seed_detection));
    let ct_0 = (pt_fq + keystream_0).to_bytes();

    // ct[1]: salt + keystream_1
    let keystream_1 = poseidon377::hash_2(&seed_detection, (Fq::from(1u64), seed_detection));
    let ct_1 = (salt + keystream_1).to_bytes();

    let mut result = [0u8; DETECTION_TIER_BYTES];
    result[..32].copy_from_slice(&ct_0);
    result[32..].copy_from_slice(&ct_1);
    result
}

/// Serialize amount + address as 80 bytes for Poseidon encryption.
fn amount_and_address_bytes(amount: &Amount, address: &Address) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(80);
    bytes.extend_from_slice(&amount.to_le_bytes());
    bytes.extend_from_slice(&address.diversified_generator().vartime_compress().0);
    bytes.extend_from_slice(&address.transmission_key().0);
    bytes
}

/// Serialize address as 64 bytes for Poseidon encryption.
fn address_bytes(address: &Address) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&address.diversified_generator().vartime_compress().0);
    bytes.extend_from_slice(&address.transmission_key().0);
    bytes
}

/// Encrypt a byte slice using Poseidon stream cipher with the given seed.
pub fn encrypt_tier_bytes(plaintext: &[u8], seed: Fq) -> Vec<u8> {
    let mut encrypted = Vec::new();
    for (i, chunk) in plaintext.chunks(31).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let plaintext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed, (counter, seed));
        let ciphertext_fq = plaintext_fq + keystream;
        encrypted.extend_from_slice(&ciphertext_fq.to_bytes());
    }
    encrypted
}

/// Decrypt the 32-byte detection tier using issuer's DK.
///
/// Computes ss = dk × epk_1, then verifies the detection tag against expected_asset_id.
pub fn decrypt_detection_tier(
    dk: &Fr,
    epk_1: &Element,
    detection_ciphertext: &[u8; DETECTION_TIER_BYTES],
    expected_asset_id: &asset::Id,
) -> anyhow::Result<(asset::Id, bool, Fq)> {
    let ss = *epk_1 * *dk;

    let epk_1_fq = epk_1.vartime_compress_to_field();
    let seed = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss.vartime_compress_to_field(), epk_1_fq),
    );

    // Decrypt slot 0: asset_id + flag
    let ct_fq = Fq::from_le_bytes_mod_order(&detection_ciphertext[..32]);
    let keystream_0 = poseidon377::hash_2(&seed, (Fq::zero(), seed));
    let pt_fq = ct_fq - keystream_0;

    // Decrypt slot 1: salt
    let ct_salt = Fq::from_le_bytes_mod_order(&detection_ciphertext[32..64]);
    let keystream_1 = poseidon377::hash_2(&seed, (Fq::from(1u64), seed));
    let salt = ct_salt - keystream_1;

    if pt_fq == expected_asset_id.0 {
        Ok((*expected_asset_id, false, salt))
    } else if pt_fq == expected_asset_id.0 + *crate::issuer_keys::FLAG_SENTINEL {
        Ok((*expected_asset_id, true, salt))
    } else {
        anyhow::bail!("detection tier does not match expected asset")
    }
}

/// Decrypt compliance data using pre-computed shared secrets.
///
/// For Spend ciphertexts, `ss_ext` should be None.
/// For flagged Spend: ss_detection = ss_core = dk × epk_1.
/// For flagged Output: ss_detection = ss_core = dk × epk_1, ss_ext = dk × epk_2.
/// For Orbis path: shared secrets are derived from re-encryption commitments.
pub fn decrypt(
    ss_detection: &Element,
    ss_core: &Element,
    ss_ext: Option<&Element>,
    epk_1: &Element,
    ciphertext: &ComplianceCiphertext,
    expected_asset_id: &asset::Id,
) -> anyhow::Result<DecryptedComplianceData> {
    let epk_1_fq = epk_1.vartime_compress_to_field();
    let seed_detection = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss_detection.vartime_compress_to_field(), epk_1_fq),
    );

    let detection_keystream = poseidon377::hash_2(&seed_detection, (Fq::zero(), seed_detection));
    let ct_fq = Fq::from_le_bytes_mod_order(&ciphertext.detection_tag[..32]);
    let pt_fq = ct_fq - detection_keystream;

    let asset_id = if pt_fq == expected_asset_id.0
        || pt_fq == expected_asset_id.0 + *crate::issuer_keys::FLAG_SENTINEL
    {
        *expected_asset_id
    } else {
        anyhow::bail!("compliance decryption failed: detection tier does not match expected asset")
    };

    // Decrypt core
    let seed_core = ciphertext.c2_core - ss_core.vartime_compress_to_field();
    let core_plaintext_bytes = decrypt_tier_bytes(&ciphertext.encrypted_core, seed_core, 80);

    if core_plaintext_bytes.len() < 80 {
        anyhow::bail!(
            "core plaintext too short: expected 80 bytes, got {}",
            core_plaintext_bytes.len()
        );
    }

    let amount_bytes: [u8; 16] = core_plaintext_bytes[0..16].try_into().context("amount")?;
    let amount = Amount::from_le_bytes(amount_bytes);
    let self_div_gen_bytes: [u8; 32] =
        core_plaintext_bytes[16..48].try_into().context("self gd")?;
    let self_div_gen = decaf377::Encoding(self_div_gen_bytes)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid self diversified generator"))?;
    let self_trans_key_bytes: [u8; 32] =
        core_plaintext_bytes[48..80].try_into().context("self pk")?;

    // Decrypt ext if present (Output ciphertext)
    let (counterparty_div_gen, counterparty_trans_key_bytes) =
        if let (Some(c2_ext), Some(encrypted_ext), Some(ss_ext)) =
            (&ciphertext.c2_ext, &ciphertext.encrypted_ext, ss_ext)
        {
            let seed_ext = *c2_ext - ss_ext.vartime_compress_to_field();
            let ext_bytes = decrypt_tier_bytes(encrypted_ext, seed_ext, 64);
            if ext_bytes.len() < 64 {
                anyhow::bail!("ext plaintext too short");
            }
            let gd_bytes: [u8; 32] = ext_bytes[0..32].try_into().context("counterparty gd")?;
            let gd = decaf377::Encoding(gd_bytes)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("invalid counterparty diversified generator"))?;
            let pk_bytes: [u8; 32] = ext_bytes[32..64].try_into().context("counterparty pk")?;
            (gd, pk_bytes)
        } else {
            (Element::default(), [0u8; 32])
        };

    Ok(DecryptedComplianceData {
        asset_id,
        amount,
        self_diversified_generator: self_div_gen,
        self_transmission_key: self_trans_key_bytes,
        counterparty_diversified_generator: counterparty_div_gen,
        counterparty_transmission_key: counterparty_trans_key_bytes,
    })
}

/// Decrypt a flagged Spend ciphertext using issuer's detection key.
pub fn decrypt_flagged_spend(
    dk: &Fr,
    ciphertext: &ComplianceCiphertext,
    expected_asset_id: &asset::Id,
) -> anyhow::Result<DecryptedComplianceData> {
    let ss = ciphertext.epk_1 * *dk;
    decrypt(
        &ss,
        &ss,
        None,
        &ciphertext.epk_1,
        ciphertext,
        expected_asset_id,
    )
}

/// Decrypt a flagged Output ciphertext using issuer's detection key.
pub fn decrypt_flagged_output(
    dk: &Fr,
    ciphertext: &ComplianceCiphertext,
    expected_asset_id: &asset::Id,
) -> anyhow::Result<DecryptedComplianceData> {
    let ss_1 = ciphertext.epk_1 * *dk;
    let epk_2 = ciphertext
        .epk_2
        .ok_or_else(|| anyhow::anyhow!("not an output ciphertext"))?;
    let ss_2 = epk_2 * *dk;
    decrypt(
        &ss_1,
        &ss_1,
        Some(&ss_2),
        &ciphertext.epk_1,
        ciphertext,
        expected_asset_id,
    )
}

/// Decrypt an encrypted tier using Poseidon stream cipher.
pub fn decrypt_tier_bytes(encrypted: &[u8], seed: Fq, expected_plaintext_len: usize) -> Vec<u8> {
    let mut plaintext_bytes = Vec::new();
    for (i, chunk) in encrypted.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed, (counter, seed));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(expected_plaintext_len - plaintext_bytes.len());
        plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }
    plaintext_bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issuer_keys::DetectionKey;
    use crate::structs::{OUTPUT_WIRE_BYTES, SPEND_WIRE_BYTES};
    use rand_core::OsRng;

    fn make_ring_keys(rng: &mut (impl RngCore + rand_core::CryptoRng)) -> (Fr, Element) {
        let sk_ring = Fr::rand(rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        (sk_ring, ring_pk)
    }

    /// Derive the single ACK for a user from ring_pk and their diversified basepoint.
    fn derive_ack(ring_pk: &Element, b_d_fq: Fq) -> Element {
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    #[test]
    fn test_encrypt_decrypt_spend_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (_, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset_id,
            amount,
            true,
            Fq::zero(),
        )
        .expect("encryption should succeed");

        assert_eq!(result.ciphertext.to_bytes().len(), SPEND_WIRE_BYTES);

        let decrypted = decrypt_flagged_spend(dk.inner(), &result.ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert_eq!(decrypted.asset_id, asset_id);
        assert_eq!(decrypted.amount, amount);
    }

    #[test]
    fn test_encrypt_decrypt_output_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (_, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let receiver_b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_b_d_fq = counterparty_address
            .diversified_generator()
            .vartime_compress_to_field();

        let ack_receiver = derive_ack(&ring_pk, receiver_b_d_fq);
        let ack_sender = derive_ack(&ring_pk, sender_b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack_receiver,
            &ack_sender,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            true,
            Fq::zero(),
        )
        .expect("encryption should succeed");

        assert_eq!(result.ciphertext.to_bytes().len(), OUTPUT_WIRE_BYTES);

        let decrypted = decrypt_flagged_output(dk.inner(), &result.ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert_eq!(decrypted.asset_id, asset_id);
        assert_eq!(decrypted.amount, amount);
    }

    #[test]
    fn test_encrypt_decrypt_spend_non_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (sk_ring, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset_id,
            amount,
            false,
            Fq::zero(),
        )
        .expect("encryption should succeed");

        // Simulate Orbis decryption: effective_sk = d_fr * sk_ring
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let effective_sk = d_fr * sk_ring;
        let ss_core = result.ciphertext.epk_1 * effective_sk;
        let ss_detection = result.ciphertext.epk_1 * *dk.inner();

        let decrypted = decrypt(
            &ss_detection,
            &ss_core,
            None,
            &result.ciphertext.epk_1,
            &result.ciphertext,
            &asset_id,
        )
        .expect("decryption should succeed");

        assert_eq!(decrypted.asset_id, asset_id);
        assert_eq!(decrypted.amount, amount);
    }

    #[test]
    fn test_encrypt_decrypt_output_non_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (sk_ring, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let receiver_b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_b_d_fq = counterparty_address
            .diversified_generator()
            .vartime_compress_to_field();

        let ack_receiver = derive_ack(&ring_pk, receiver_b_d_fq);
        let ack_sender = derive_ack(&ring_pk, sender_b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack_receiver,
            &ack_sender,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            false,
            Fq::zero(),
        )
        .expect("encryption should succeed");

        // Simulate Orbis decryption path (single derivation scalar)
        let d_receiver = derive_compliance_scalar(receiver_b_d_fq);
        let d_receiver_fr = Fr::from_le_bytes_mod_order(&d_receiver.to_bytes());

        let ss_core = result.ciphertext.epk_1 * (d_receiver_fr * sk_ring);
        let ss_ext = result.ciphertext.epk_2.unwrap() * (d_receiver_fr * sk_ring);
        let ss_detection = result.ciphertext.epk_1 * *dk.inner();

        let decrypted = decrypt(
            &ss_detection,
            &ss_core,
            Some(&ss_ext),
            &result.ciphertext.epk_1,
            &result.ciphertext,
            &asset_id,
        )
        .expect("decryption should succeed");

        assert_eq!(decrypted.asset_id, asset_id);
        assert_eq!(decrypted.amount, amount);
    }

    #[test]
    fn test_detection_tier_roundtrip() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let asset_id = asset::Id(Fq::from(12345u64));

        let (_, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        // Not flagged
        {
            let result = encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &self_address,
                asset_id,
                Amount::from(100u128),
                false,
                Fq::zero(),
            )
            .unwrap();

            let (decrypted_asset, is_flagged, _salt) = decrypt_detection_tier(
                dk.inner(),
                &result.ciphertext.epk_1,
                &result.ciphertext.detection_tag,
                &asset_id,
            )
            .unwrap();

            assert_eq!(decrypted_asset, asset_id);
            assert!(!is_flagged);
        }

        // Flagged
        {
            let result = encrypt_spend(
                &mut rng,
                &ack,
                &dk_pub,
                &self_address,
                asset_id,
                Amount::from(100u128),
                true,
                Fq::zero(),
            )
            .unwrap();

            let (decrypted_asset, is_flagged, _salt) = decrypt_detection_tier(
                dk.inner(),
                &result.ciphertext.epk_1,
                &result.ciphertext.detection_tag,
                &asset_id,
            )
            .unwrap();

            assert_eq!(decrypted_asset, asset_id);
            assert!(is_flagged);
        }
    }

    #[test]
    fn test_wrong_dk_cannot_decrypt() {
        let mut rng = OsRng;
        let dk1 = DetectionKey::demo();
        let dk2 = DetectionKey::from_seed(&[1u8; 32]);
        let dk_pub = dk1.public_key();
        let asset_id = asset::Id(Fq::from(42u64));

        let (_, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset_id,
            Amount::from(100u128),
            true,
            Fq::zero(),
        )
        .unwrap();

        let detection_result = decrypt_detection_tier(
            dk2.inner(),
            &result.ciphertext.epk_1,
            &result.ciphertext.detection_tag,
            &asset_id,
        );
        assert!(detection_result.is_err());
    }

    #[test]
    fn test_ciphertext_sizes() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (_, ring_pk) = make_ring_keys(&mut rng);
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let cp_b_d_fq = counterparty_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack_receiver = derive_ack(&ring_pk, b_d_fq);
        let ack_sender = derive_ack(&ring_pk, cp_b_d_fq);

        let spend_result = encrypt_spend(
            &mut rng,
            &ack_receiver,
            &dk_pub,
            &self_address,
            asset_id,
            amount,
            false,
            Fq::zero(),
        )
        .unwrap();
        assert_eq!(spend_result.ciphertext.to_bytes().len(), SPEND_WIRE_BYTES);

        let output_result = encrypt_output(
            &mut rng,
            &ack_receiver,
            &ack_sender,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            false,
            Fq::zero(),
        )
        .unwrap();
        assert_eq!(output_result.ciphertext.to_bytes().len(), OUTPUT_WIRE_BYTES);
    }

    #[test]
    fn test_dleq_native_roundtrip() {
        let mut rng = OsRng;
        let (_, ring_pk) = make_ring_keys(&mut rng);
        let address = Address::dummy(&mut rng);
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let salt = Fq::rand(&mut rng);

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            salt,
        );

        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        // Verify with reconstruction: R_rec = s*G - c_fr*EPK, R'_rec = s*ACK - c_fr*S
        // c is stored as truncated Fq; use directly as Fr for scalar arithmetic
        let c_fr = Fr::from_le_bytes_mod_order(&proof.c.to_bytes());
        let s_point = ack * r;
        let r_rec = Element::GENERATOR * proof.s - epk * c_fr;
        let rp_rec = ack * proof.s - s_point * c_fr;

        // Recompute challenge from reconstructed points (Orbis-compatible)
        let g_fq = Element::GENERATOR.vartime_compress_to_field();
        let ack_fq = ack.vartime_compress_to_field();
        let epk_fq = epk.vartime_compress_to_field();
        let s_fq = s_point.vartime_compress_to_field();
        let r_fq = r_rec.vartime_compress_to_field();
        let rp_fq = rp_rec.vartime_compress_to_field();

        let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
        let c_check_fq = poseidon377::hash_7(
            &domain,
            (metadata_hash, g_fq, ack_fq, epk_fq, s_fq, r_fq, rp_fq),
        );
        let c_check_truncated =
            Fq::from_le_bytes_mod_order(&fq_to_challenge_scalar(c_check_fq).to_bytes());

        assert_eq!(
            proof.c, c_check_truncated,
            "DLEQ challenge should match after reconstruction"
        );
    }

    #[test]
    fn test_dleq_c_truncated() {
        use ark_ff::{BigInteger, PrimeField};

        // Verify that DleqProof.c has high bits zeroed (truncated challenge)
        let mut rng = OsRng;
        let (_, ring_pk) = make_ring_keys(&mut rng);
        let address = Address::dummy(&mut rng);
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        // c should be a valid Fq with high bits zero
        let c_bytes = proof.c.into_bigint().to_bytes_le();
        let keep_bits = Fr::MODULUS_BIT_SIZE - 1; // 250
        let keep_bytes = (keep_bits as usize + 7) / 8; // 32
        let spare_bits = keep_bytes * 8 - keep_bits as usize; // 6
        assert_eq!(
            c_bytes[keep_bytes - 1] & (0xFF << (8 - spare_bits)),
            0,
            "high bits of c should be zeroed (truncated)"
        );
    }

    /// End-to-end: compute metadata hash → DLEQ proof → native verify roundtrip.
    #[test]
    fn test_metadata_to_dleq_native_roundtrip() {
        let mut rng = OsRng;
        let (_, ring_pk) = make_ring_keys(&mut rng);
        let address = Address::dummy(&mut rng);
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;

        // Compute metadata hash (unchanged from before DLEQ refactor)
        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        // Generate DLEQ proof
        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        // Verify: reconstruct R and R', recompute challenge
        let s_point = ack * r;
        let c_fr = Fr::from_le_bytes_mod_order(&proof.c.to_bytes());
        let r_rec = Element::GENERATOR * proof.s - epk * c_fr;
        let rp_rec = ack * proof.s - s_point * c_fr;

        let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
        let g_fq = Element::GENERATOR.vartime_compress_to_field();
        let c_check = poseidon377::hash_7(
            &domain,
            (
                metadata_hash,
                g_fq,
                ack.vartime_compress_to_field(),
                epk.vartime_compress_to_field(),
                s_point.vartime_compress_to_field(),
                r_rec.vartime_compress_to_field(),
                rp_rec.vartime_compress_to_field(),
            ),
        );
        let c_check_trunc =
            Fq::from_le_bytes_mod_order(&fq_to_challenge_scalar(c_check).to_bytes());
        assert_eq!(proof.c, c_check_trunc, "metadata → DLEQ → verify roundtrip");
    }

    #[test]
    fn test_metadata_hash_consistency() {
        let h1 = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(100u64),
            Fq::from(42u64),
        );
        let h2 = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(100u64),
            Fq::from(42u64),
        );
        assert_eq!(h1, h2, "same inputs should produce same hash");

        // Different salt should produce different hash
        let h3 = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(100u64),
            Fq::from(43u64),
        );
        assert_ne!(h1, h3, "different salt should produce different hash");

        // Different tier should produce different hash
        let h4 = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(2u64),
            Fq::from(100u64),
            Fq::from(42u64),
        );
        assert_ne!(h1, h4, "different tier should produce different hash");
    }

    /// Deterministic cross-verification fixture: fixed inputs → DLEQ proof → native verify.
    #[test]
    fn test_dleq_cross_verify_fixture() {
        let r = Fr::from(42u64);
        let ring_sk = Fr::from(99u64);
        let ring_pk = Element::GENERATOR * ring_sk;
        let d_fr = Fr::from(7u64);
        let ack = ring_pk * d_fr;
        let epk = Element::GENERATOR * r;
        let k = Fr::from(123u64);

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::from(55u64),
        );

        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        // Verify: same deterministic inputs must produce same proof every time
        let proof2 = compute_dleq_native(r, k, &ack, &epk, metadata_hash);
        assert_eq!(proof.c, proof2.c, "deterministic c");
        assert_eq!(proof.s, proof2.s, "deterministic s");

        // Native verification
        let s_point = ack * r;
        let c_fr = Fr::from_le_bytes_mod_order(&proof.c.to_bytes());
        let r_rec = Element::GENERATOR * proof.s - epk * c_fr;
        let rp_rec = ack * proof.s - s_point * c_fr;

        let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
        let g_fq = Element::GENERATOR.vartime_compress_to_field();
        let c_check = poseidon377::hash_7(
            &domain,
            (
                metadata_hash,
                g_fq,
                ack.vartime_compress_to_field(),
                epk.vartime_compress_to_field(),
                s_point.vartime_compress_to_field(),
                r_rec.vartime_compress_to_field(),
                rp_rec.vartime_compress_to_field(),
            ),
        );
        let c_check_trunc =
            Fq::from_le_bytes_mod_order(&fq_to_challenge_scalar(c_check).to_bytes());
        assert_eq!(proof.c, c_check_trunc, "cross-verify fixture");
    }

    #[test]
    fn test_detection_tier_salt_roundtrip() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (_, ring_pk) = make_ring_keys(&mut rng);
        let address = Address::dummy(&mut rng);
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let asset_id = asset::Id(Fq::from(42u64));
        let salt = Fq::rand(&mut rng);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &address,
            asset_id,
            Amount::from(100u128),
            false,
            salt,
        )
        .unwrap();

        // Decrypt detection tier and verify salt roundtrips
        let epk_1 = Element::GENERATOR * result.r_s;
        let (decrypted_asset_id, _flagged, decrypted_salt) = decrypt_detection_tier(
            dk.inner(),
            &epk_1,
            &result.ciphertext.detection_tag,
            &asset_id,
        )
        .expect("can decrypt detection tier");
        assert_eq!(decrypted_asset_id, asset_id);
        assert_eq!(
            decrypted_salt, salt,
            "salt should roundtrip through detection tier"
        );
    }

    #[test]
    fn test_fq_to_challenge_scalar_high_bits_zeroed() {
        use ark_ff::{BigInteger, PrimeField};

        // Test with multiple random Fq values
        let mut rng = OsRng;
        for _ in 0..100 {
            let fq = Fq::rand(&mut rng);
            let fr = fq_to_challenge_scalar(fq);

            // Convert back to bytes and check high bits are zero
            let fr_bytes = fr.into_bigint().to_bytes_le();
            let keep_bits = Fr::MODULUS_BIT_SIZE - 1; // 250
            let keep_bytes = (keep_bits as usize + 7) / 8; // 32
            let spare_bits = keep_bytes * 8 - keep_bits as usize; // 6

            // Top spare_bits of byte[keep_bytes-1] must be zero
            assert_eq!(
                fr_bytes[keep_bytes - 1] & (0xFF << (8 - spare_bits)),
                0,
                "high bits should be zeroed after truncation"
            );
        }
    }

    #[test]
    fn test_point_encoding_equivalence() {
        use ark_serialize::CanonicalSerialize;

        // Verify that vartime_compress_to_field() matches Orbis point_to_fq()
        // (serialize_compressed + from_le_bytes_mod_order)
        let mut rng = OsRng;
        for _ in 0..100 {
            let scalar = Fr::rand(&mut rng);
            let point = Element::GENERATOR * scalar;

            // Penumbra method
            let fq_penumbra = point.vartime_compress_to_field();

            // Orbis method: serialize_compressed → from_le_bytes_mod_order
            let mut bytes = Vec::with_capacity(32);
            point
                .serialize_compressed(&mut bytes)
                .expect("compression should succeed");
            let fq_orbis = Fq::from_le_bytes_mod_order(&bytes);

            assert_eq!(
                fq_penumbra, fq_orbis,
                "Penumbra and Orbis point→Fq encoding must match"
            );
        }
    }

    #[test]
    fn test_hash7_domain_and_ordering() {
        // Determinism test: hash_7 with known inputs produces stable output.
        // This locks the input ordering to match Orbis (metadata first, then G).
        let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);

        let metadata = Fq::from(42u64);
        let g_fq = Element::GENERATOR.vartime_compress_to_field();
        let ack_fq = Fq::from(1u64);
        let epk_fq = Fq::from(2u64);
        let s_fq = Fq::from(3u64);
        let r_fq = Fq::from(4u64);
        let rp_fq = Fq::from(5u64);

        let h1 = poseidon377::hash_7(&domain, (metadata, g_fq, ack_fq, epk_fq, s_fq, r_fq, rp_fq));
        let h2 = poseidon377::hash_7(&domain, (metadata, g_fq, ack_fq, epk_fq, s_fq, r_fq, rp_fq));
        assert_eq!(h1, h2, "hash_7 must be deterministic");

        // Different ordering produces different hash
        let h3 = poseidon377::hash_7(&domain, (g_fq, metadata, ack_fq, epk_fq, s_fq, r_fq, rp_fq));
        assert_ne!(
            h1, h3,
            "different input ordering must produce different hash"
        );
    }

    /// Verify all 3 tiers of an output ciphertext are independently decryptable
    /// with distinct ACKs: core/ext use ack_receiver, sext uses ack_sender.
    #[test]
    fn test_output_three_tier_ack_isolation() {
        use crate::scanning::{decrypt_extension, decrypt_spend_ext};

        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let (sk_ring, ring_pk) = make_ring_keys(&mut rng);
        let receiver_address = Address::dummy(&mut rng);
        let sender_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let receiver_b_d_fq = receiver_address
            .diversified_generator()
            .vartime_compress_to_field();
        let sender_b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();

        let ack_receiver = derive_ack(&ring_pk, receiver_b_d_fq);
        let ack_sender = derive_ack(&ring_pk, sender_b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack_receiver,
            &ack_sender,
            &dk_pub,
            &receiver_address,
            &sender_address,
            asset_id,
            amount,
            false,
            Fq::zero(),
        )
        .expect("encryption should succeed");

        let ct = &result.ciphertext;
        assert_eq!(ct.to_bytes().len(), OUTPUT_WIRE_BYTES);

        // Derive effective secret keys for each party
        let d_receiver = derive_compliance_scalar(receiver_b_d_fq);
        let d_receiver_fr = Fr::from_le_bytes_mod_order(&d_receiver.to_bytes());
        let d_sender = derive_compliance_scalar(sender_b_d_fq);
        let d_sender_fr = Fr::from_le_bytes_mod_order(&d_sender.to_bytes());

        // Core tier: epk_1 × (d_receiver × sk_ring)
        let ss_core = ct.epk_1 * (d_receiver_fr * sk_ring);
        let seed_core = ct.c2_core - ss_core.vartime_compress_to_field();
        let core_pt = decrypt_tier_bytes(&ct.encrypted_core, seed_core, 80);
        let decrypted_amount = Amount::from_le_bytes(core_pt[0..16].try_into().unwrap());
        assert_eq!(
            decrypted_amount, amount,
            "core tier should decrypt with ack_receiver"
        );

        // Ext tier: epk_2 × (d_receiver × sk_ring)
        let ss_ext = ct.epk_2.unwrap() * (d_receiver_fr * sk_ring);
        let ext_data = decrypt_extension(&ss_ext, ct)
            .expect("ext decryption should succeed")
            .expect("ext data should be present");
        assert_eq!(
            ext_data.counterparty_diversified_generator,
            *sender_address.diversified_generator(),
            "ext tier should contain sender address"
        );

        // Sext tier: epk_3 × (d_sender × sk_ring) — uses ack_sender
        let ss_sext = ct.epk_3.unwrap() * (d_sender_fr * sk_ring);
        let sext_data = decrypt_spend_ext(&ss_sext, ct)
            .expect("sext decryption should succeed")
            .expect("sext data should be present");
        assert_eq!(sext_data.amount, amount, "sext tier should contain amount");
        assert_eq!(
            sext_data.recipient_diversified_generator,
            *receiver_address.diversified_generator(),
            "sext tier should contain receiver address"
        );

        // Cross-ACK isolation: ack_receiver key cannot decrypt sext tier
        let ss_sext_wrong = ct.epk_3.unwrap() * (d_receiver_fr * sk_ring);
        let wrong_sext = decrypt_spend_ext(&ss_sext_wrong, ct);
        // Should either fail or return wrong data
        match wrong_sext {
            Ok(Some(data)) => {
                assert_ne!(
                    data.amount, amount,
                    "wrong ACK should not recover correct sext data"
                );
            }
            _ => {} // Error or None is also acceptable
        }
    }
}
