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

use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::Address;

use sha2::{Digest, Sha512};

use crate::issuer_keys::{detection_flag_from_fq, DETECTION_TIER_BYTES};
/// Domain separator for SHA-512 derivation — matches Orbis `DERIVATION_DOMAIN` exactly.
const DERIVATION_DOMAIN: &[u8] = b"elgamal-derivation-v1\0\0";

/// Canonical ordinary-Orbis derivation path for one full Shieldd address.
pub fn compliance_derivation(address: &Address) -> Vec<u8> {
    address.to_vec()
}

/// Derive the compliance scalar `d` from the full canonical address.
///
/// `d = Fr::from_le_bytes_mod_order(SHA512(DERIVATION_DOMAIN || address.to_vec()))`
///
/// This MUST match Orbis's `derive_capability_scalar()` so PRE math cancels correctly.
/// Orbis uses a 64-byte SHA-512 digest reduced mod `Fr` (wide reduction, negligible
/// bias). The result is stored as Fq in the compliance leaf (Fr fits losslessly in Fq).
pub fn derive_compliance_scalar(address: &Address) -> Fq {
    let fr = capability_scalar(&compliance_derivation(address));
    Fq::from_le_bytes_mod_order(&fr.to_bytes())
}

fn capability_scalar(derivation: &[u8]) -> Fr {
    let mut hasher = Sha512::new();
    hasher.update(DERIVATION_DOMAIN);
    hasher.update(derivation);
    let hash = hasher.finalize();
    Fr::from_le_bytes_mod_order(&hash)
}

/// Domain separator for Poseidon stream cipher seed derivation.
pub static COMPLIANCE_STREAM_CIPHER_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.compliance.poseidon_stream").as_bytes(),
    )
});

/// Domain separator for non-indexing transfer core key confirmation.
pub static TRANSFER_KEY_CONFIRMATION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.transfer.compliance.key_confirmation.v1").as_bytes(),
    )
});

pub fn transfer_key_confirmation(seed: Fq, epk_fq: Fq, tier_salt: Fq) -> Fq {
    poseidon377::hash_3(&TRANSFER_KEY_CONFIRMATION_DOMAIN, (seed, epk_fq, tier_salt))
}

pub fn compliance_stream_block(seed: Fq, counter: u64) -> Fq {
    poseidon377::hash_2(&COMPLIANCE_STREAM_CIPHER_DOMAIN, (seed, Fq::from(counter)))
}

fn derive_unregulated_sink_point(domain_sep: &[u8]) -> Element {
    let point_domain = Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(domain_sep).as_bytes());
    Element::encode_to_curve(&point_domain)
}

/// Trapdoorless issuer detection sink for unregulated assets.
///
/// This preserves a uniform transfer ciphertext shape without requiring a
/// real issuer detection key for unregulated assets.
pub static UNREGULATED_SINK_DK_PUB: Lazy<Element> =
    Lazy::new(|| derive_unregulated_sink_point(b"shieldd.compliance.unregulated.dk-pub"));

/// Trapdoorless ring/ACK sink for unregulated assets.
///
/// This preserves uniform ACK-derived encryption routing without reusing the
/// detection sink point or requiring any Orbis-managed ring for unregulated assets.
pub static UNREGULATED_SINK_RING_PK: Lazy<Element> =
    Lazy::new(|| derive_unregulated_sink_point(b"shieldd.compliance.unregulated.ring-pk"));

/// Domain separator for issuer detection tier encryption.
pub static ISSUER_DETECTION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.compliance.issuer_detection").as_bytes(),
    )
});

/// Encrypt a byte slice using Poseidon stream cipher with the given seed.
pub fn encrypt_tier_bytes(plaintext: &[u8], seed: Fq) -> Vec<u8> {
    let mut encrypted = Vec::new();
    for (i, chunk) in plaintext.chunks(31).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let plaintext_fq = Fq::from_le_bytes_mod_order(&buf);
        let keystream = compliance_stream_block(seed, i as u64);
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

    // Decrypt slot 0: exact asset ID.
    let ct_fq = Fq::from_le_bytes_mod_order(&detection_ciphertext[..32]);
    let keystream_0 = compliance_stream_block(seed, 0);
    let decrypted_asset_id = ct_fq - keystream_0;
    anyhow::ensure!(
        decrypted_asset_id == expected_asset_id.0,
        "detection tier does not match expected asset"
    );

    // Decrypt slot 1: salt
    let ct_salt = Fq::from_le_bytes_mod_order(&detection_ciphertext[32..64]);
    let keystream_1 = compliance_stream_block(seed, 1);
    let salt = ct_salt - keystream_1;

    let ct_flag = Fq::from_le_bytes_mod_order(&detection_ciphertext[64..96]);
    let keystream_2 = compliance_stream_block(seed, 2);
    let is_flagged = detection_flag_from_fq(ct_flag - keystream_2)?;

    let ct_reserved = Fq::from_le_bytes_mod_order(&detection_ciphertext[96..128]);
    let keystream_3 = compliance_stream_block(seed, 3);
    anyhow::ensure!(
        ct_reserved - keystream_3 == Fq::from(0u64),
        "detection reserved word is nonzero"
    );

    Ok((*expected_asset_id, is_flagged, salt))
}

/// Decrypt an encrypted tier using Poseidon stream cipher.
pub fn decrypt_tier_bytes(encrypted: &[u8], seed: Fq, expected_plaintext_len: usize) -> Vec<u8> {
    let mut plaintext_bytes = Vec::new();
    for (i, chunk) in encrypted.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let keystream = compliance_stream_block(seed, i as u64);
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

    #[test]
    fn test_point_encoding_equivalence() {
        use ark_serialize::CanonicalSerialize;

        // Verify that vartime_compress_to_field() matches Orbis point_to_fq()
        // (serialize_compressed + from_le_bytes_mod_order)
        let mut rng = OsRng;
        for _ in 0..100 {
            let scalar = Fr::rand(&mut rng);
            let point = Element::GENERATOR * scalar;

            // Shieldd method
            let fq_shieldd = point.vartime_compress_to_field();

            // Orbis method: serialize_compressed → from_le_bytes_mod_order
            let mut bytes = Vec::with_capacity(32);
            point
                .serialize_compressed(&mut bytes)
                .expect("compression should succeed");
            let fq_orbis = Fq::from_le_bytes_mod_order(&bytes);

            assert_eq!(
                fq_shieldd, fq_orbis,
                "Shieldd and Orbis point→Fq encoding must match"
            );
        }
    }

    #[test]
    fn test_derive_compliance_scalar_deterministic() {
        let address1 = &shieldd_sdk_keys::test_keys::ADDRESS_0;
        let address2 = &shieldd_sdk_keys::test_keys::ADDRESS_1;

        assert_eq!(
            derive_compliance_scalar(address1),
            derive_compliance_scalar(address1),
            "same input must produce same scalar"
        );
        assert_ne!(
            derive_compliance_scalar(address1),
            derive_compliance_scalar(address2),
            "different inputs must produce different scalars"
        );
        assert_eq!(compliance_derivation(address1), address1.to_vec());
        assert_eq!(compliance_derivation(address1).len(), 48);
    }

    #[test]
    fn orbis_capability_derivation_matches_cross_language_vector() {
        let derivation: Vec<u8> = (0u8..48).collect();
        assert_eq!(
            capability_scalar(&derivation).to_string(),
            "1193025715605820042638296979565000936182988212520272662409467010519644752199"
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
        let dk_hash = blake2b_simd::blake2b(b"shieldd.compliance.unregulated.dk-pub");
        let ring_hash = blake2b_simd::blake2b(b"shieldd.compliance.unregulated.ring-pk");
        let dk_scalar = Fr::from_le_bytes_mod_order(dk_hash.as_bytes());
        let ring_scalar = Fr::from_le_bytes_mod_order(ring_hash.as_bytes());

        assert_ne!(
            *UNREGULATED_SINK_DK_PUB,
            Element::GENERATOR * dk_scalar,
            "UNREGULATED_SINK_DK_PUB must not be a public-scalar multiple of G"
        );
        assert_ne!(
            *UNREGULATED_SINK_RING_PK,
            Element::GENERATOR * ring_scalar,
            "UNREGULATED_SINK_RING_PK must not be a public-scalar multiple of G"
        );
    }
}
