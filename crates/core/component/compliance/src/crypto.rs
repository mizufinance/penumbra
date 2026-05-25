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
use penumbra_sdk_num::Amount;

use sha2::{Digest, Sha256};

use crate::issuer_keys::DETECTION_TIER_BYTES;
use crate::structs::{ComplianceCiphertext, DleqProof};

#[cfg(test)]
use penumbra_sdk_keys::Address;

/// Domain separator for SHA256 derivation — matches Orbis `DERIVATION_DOMAIN` exactly.
const DERIVATION_DOMAIN: &[u8; 23] = b"elgamal-derivation-v1\0\0";

/// Derive the compliance scalar `d` from canonical slot derivation material.
///
/// `d = Fr::from_le_bytes_mod_order(SHA256(DERIVATION_DOMAIN || slot_derivation.to_bytes()))`
///
/// This MUST match Orbis's `derive_capability_scalar()` so PRE math cancels correctly.
/// The result is stored as Fq in the compliance leaf (Fr fits losslessly in Fq for decaf377).
pub fn derive_compliance_scalar(slot_derivation: Fq) -> Fq {
    let mut hasher = Sha256::new();
    hasher.update(DERIVATION_DOMAIN);
    hasher.update(slot_derivation.to_bytes());
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

fn derive_unregulated_sink_point(domain_sep: &[u8]) -> Element {
    let point_domain = Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(domain_sep).as_bytes());
    Element::encode_to_curve(&point_domain)
}

/// Trapdoorless issuer detection sink for unregulated assets.
///
/// This preserves a uniform transfer ciphertext shape without requiring a
/// real issuer detection key for unregulated assets.
pub static UNREGULATED_SINK_DK_PUB: Lazy<Element> =
    Lazy::new(|| derive_unregulated_sink_point(b"penumbra.compliance.unregulated.dk-pub.v1"));

/// Trapdoorless ring/ACK sink for unregulated assets.
///
/// This preserves uniform ACK-derived encryption routing without reusing the
/// detection sink point or requiring any Orbis-managed ring for unregulated assets.
pub static UNREGULATED_SINK_RING_PK: Lazy<Element> =
    Lazy::new(|| derive_unregulated_sink_point(b"penumbra.compliance.unregulated.ring-pk.v1"));

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
) -> anyhow::Result<(asset::Id, bool, Fq, u32, u32)> {
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

    let slot_id_from_fq = |value: Fq, field: &str| -> anyhow::Result<u32> {
        let bytes = value.to_bytes();
        anyhow::ensure!(
            bytes[4..].iter().all(|byte| *byte == 0),
            "{field} is not a canonical u32 slot id"
        );
        Ok(u32::from_le_bytes(bytes[..4].try_into()?))
    };

    let ct_sender_slot = Fq::from_le_bytes_mod_order(&detection_ciphertext[64..96]);
    let keystream_2 = poseidon377::hash_2(&seed, (Fq::from(2u64), seed));
    let sender_slot_id = slot_id_from_fq(ct_sender_slot - keystream_2, "sender_slot_id")?;

    let ct_receiver_slot = Fq::from_le_bytes_mod_order(&detection_ciphertext[96..128]);
    let keystream_3 = poseidon377::hash_2(&seed, (Fq::from(3u64), seed));
    let receiver_slot_id = slot_id_from_fq(ct_receiver_slot - keystream_3, "receiver_slot_id")?;

    if pt_fq == expected_asset_id.0 {
        Ok((
            *expected_asset_id,
            false,
            salt,
            sender_slot_id,
            receiver_slot_id,
        ))
    } else if pt_fq == expected_asset_id.0 + *crate::issuer_keys::FLAG_SENTINEL {
        Ok((
            *expected_asset_id,
            true,
            salt,
            sender_slot_id,
            receiver_slot_id,
        ))
    } else {
        anyhow::bail!("detection tier does not match expected asset")
    }
}

/// Decrypt compliance data using pre-computed shared secrets.
///
/// For transfer-input ciphertexts, `ss_ext` should be None.
/// For flagged transfer-input ciphertexts: ss_detection = ss_core = dk × epk_1.
/// For flagged transfer-output ciphertexts: ss_detection = ss_core = dk × epk_1, ss_ext = dk × epk_2.
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

    // Decrypt ext if present (transfer-output ciphertext)
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
    use rand_core::OsRng;

    fn make_ring_keys(rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng)) -> (Fr, Element) {
        let sk_ring = Fr::rand(rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        (sk_ring, ring_pk)
    }

    fn derive_ack(ring_pk: &Element, b_d_fq: Fq) -> Element {
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
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
    fn test_verify_dleq_native_valid() {
        let mut rng = OsRng;
        let (_, ring_pk) = make_ring_keys(&mut rng);
        let address = Address::dummy(&mut rng);
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let s_point = ack * r;

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        verify_dleq_native(&ack, &epk, &s_point, &proof.c, &proof.s, metadata_hash)
            .expect("valid DLEQ proof should verify");
    }

    #[test]
    fn test_verify_dleq_native_wrong_metadata() {
        let mut rng = OsRng;
        let (_, ring_pk) = make_ring_keys(&mut rng);
        let address = Address::dummy(&mut rng);
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let s_point = ack * r;

        let original_timestamp = Fq::from(1_700_000_000u64);
        let wrong_timestamp = Fq::from(1_600_000_000u64);

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            original_timestamp,
            Fq::from(42u64),
        );

        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        // Verify with wrong timestamp → different metadata → should fail
        let wrong_metadata = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            wrong_timestamp,
            Fq::from(42u64),
        );

        let result = verify_dleq_native(&ack, &epk, &s_point, &proof.c, &proof.s, wrong_metadata);
        assert!(
            result.is_err(),
            "DLEQ verification with wrong timestamp/metadata should fail"
        );
    }

    #[test]
    fn test_verify_dleq_native_wrong_ack() {
        let mut rng = OsRng;
        let (_, ring_pk) = make_ring_keys(&mut rng);
        let addr1 = Address::dummy(&mut rng);
        let addr2 = Address::dummy(&mut rng);
        let b_d_fq1 = addr1.diversified_generator().vartime_compress_to_field();
        let b_d_fq2 = addr2.diversified_generator().vartime_compress_to_field();
        let ack_correct = derive_ack(&ring_pk, b_d_fq1);
        let ack_wrong = derive_ack(&ring_pk, b_d_fq2);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let s_point = ack_correct * r;

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        let proof = compute_dleq_native(r, k, &ack_correct, &epk, metadata_hash);

        // Verify with wrong ACK should fail
        let result = verify_dleq_native(
            &ack_wrong,
            &epk,
            &s_point,
            &proof.c,
            &proof.s,
            metadata_hash,
        );
        assert!(
            result.is_err(),
            "DLEQ verification with wrong ACK should fail"
        );
    }

    #[test]
    fn test_derive_compliance_scalar_deterministic() {
        let fq1 = Fq::from(12345u64);
        let fq2 = Fq::from(12345u64);
        let fq3 = Fq::from(99999u64);

        assert_eq!(
            derive_compliance_scalar(fq1),
            derive_compliance_scalar(fq2),
            "same input must produce same scalar"
        );
        assert_ne!(
            derive_compliance_scalar(fq1),
            derive_compliance_scalar(fq3),
            "different inputs must produce different scalars"
        );
    }

    #[test]
    fn test_unregulated_sink_keys_are_stable_and_non_identity() {
        assert_ne!(
            *UNREGULATED_SINK_DK_PUB,
            Element::default(),
            "UNREGULATED_SINK_DK_PUB must not be the identity element"
        );
        assert_ne!(
            *UNREGULATED_SINK_RING_PK,
            Element::default(),
            "UNREGULATED_SINK_RING_PK must not be the identity element"
        );
        assert_ne!(
            *UNREGULATED_SINK_DK_PUB, *UNREGULATED_SINK_RING_PK,
            "unregulated sink keys must stay role-separated"
        );
        assert_ne!(
            *UNREGULATED_SINK_DK_PUB,
            Element::GENERATOR,
            "UNREGULATED_SINK_DK_PUB must not collapse to the generator"
        );
        assert_ne!(
            *UNREGULATED_SINK_RING_PK,
            Element::GENERATOR,
            "UNREGULATED_SINK_RING_PK must not collapse to the generator"
        );
    }

    #[test]
    fn test_unregulated_sink_keys_are_hash_to_curve_points() {
        let legacy_dk_hash = blake2b_simd::blake2b(b"penumbra.compliance.unregulated.dk-pub.v1");
        let legacy_ring_hash = blake2b_simd::blake2b(b"penumbra.compliance.unregulated.ring-pk.v1");
        let legacy_dk_scalar = Fr::from_le_bytes_mod_order(legacy_dk_hash.as_bytes());
        let legacy_ring_scalar = Fr::from_le_bytes_mod_order(legacy_ring_hash.as_bytes());

        assert_ne!(
            *UNREGULATED_SINK_DK_PUB,
            Element::GENERATOR * legacy_dk_scalar,
            "UNREGULATED_SINK_DK_PUB must not be a public-scalar multiple of G"
        );
        assert_ne!(
            *UNREGULATED_SINK_RING_PK,
            Element::GENERATOR * legacy_ring_scalar,
            "UNREGULATED_SINK_RING_PK must not be a public-scalar multiple of G"
        );
    }

    #[test]
    fn test_metadata_hash_timestamp_sensitivity() {
        // Adjacent timestamps produce different hashes — this is how
        // the DLEQ proof binds to a specific time window.
        let salt = Fq::from(42u64);
        let base = (
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
        );

        let t1 = Fq::from(1_700_000_000u64);
        let t2 = Fq::from(1_700_000_001u64); // 1 second later
        let t_zero = Fq::zero();

        let h1 = compute_metadata_hash(base.0, base.1, base.2, base.3, t1, salt);
        let h2 = compute_metadata_hash(base.0, base.1, base.2, base.3, t2, salt);
        let h_zero = compute_metadata_hash(base.0, base.1, base.2, base.3, t_zero, salt);

        assert_ne!(h1, h2, "1-second difference must change hash");
        assert_ne!(h1, h_zero, "non-zero vs zero timestamp must differ");
        // Zero timestamp is valid (used for unregulated assets)
        let _ = h_zero;
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
}
