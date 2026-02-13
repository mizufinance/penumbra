//! BLS12-381 threshold signature verification for bankd finalization certificates.
//!
//! Bankd uses Kora (commonware) for consensus finalization. Finalization certificates
//! are BLS12-381 threshold signatures over a consensus digest. This module provides
//! verification of those signatures using the `blst` crate directly.
//!
//! # Signature Scheme (MinSig variant)
//!
//! - Public keys: G2 (96 bytes compressed)
//! - Signatures: G1 (48 bytes compressed)
//! - DST: `BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_` (Proof of Possession scheme)
//! - Message format: `union_unique(namespace, payload)` where union_unique prepends a
//!   varint-encoded length prefix to the namespace before concatenating with the payload.

use anyhow::{ensure, Result};

/// Kora's simplex signing namespace. Must match exactly.
pub const SIMPLEX_NAMESPACE: &[u8] = b"_COMMONWARE_KORA_SIMPLEX";

/// BLS12-381 DST for message signing with MinSig variant (signatures in G1).
///
/// This matches commonware-cryptography's `G1_MESSAGE` constant, which is used for
/// the Proof of Possession (POP) scheme per draft-irtf-cfrg-bls-signature-05 section 4.2.
pub const BLS_DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

/// Verify a BLS12-381 MinSig threshold signature.
///
/// This verifies a single aggregated threshold signature against the group public key,
/// as produced by Kora's simplex finalization.
///
/// # Arguments
///
/// * `group_public_key` - 96-byte compressed G2Affine (the DKG group key)
/// * `message` - the signed message bytes (before namespace prepending)
/// * `signature` - 48-byte compressed G1Affine
///
/// # Returns
///
/// `Ok(true)` if the signature is valid, `Ok(false)` if the pairing check fails,
/// or `Err` if the public key or signature bytes are malformed.
pub fn verify_bls_threshold_signature(
    group_public_key: &[u8; 96],
    message: &[u8],
    signature: &[u8; 48],
) -> Result<bool> {
    let pk = blst::min_sig::PublicKey::from_bytes(group_public_key)
        .map_err(|e| anyhow::anyhow!("invalid BLS12-381 public key: {:?}", e))?;

    let sig = blst::min_sig::Signature::from_bytes(signature)
        .map_err(|e| anyhow::anyhow!("invalid BLS12-381 signature: {:?}", e))?;

    // Construct the full signed payload: union_unique(SIMPLEX_NAMESPACE, message)
    let payload = union_unique(SIMPLEX_NAMESPACE, message);

    // Verify: hash_msg=true, dst=BLS_DST, aug=empty, pk, pk_validate=true
    let result = sig.verify(true, &payload, BLS_DST, &[], &pk, true);
    Ok(result == blst::BLST_ERROR::BLST_SUCCESS)
}

/// Verify a BLS12-381 MinSig signature with an explicit namespace and DST.
///
/// Lower-level function for cases where the caller provides the namespace
/// and DST directly (e.g., for testing or non-simplex contexts).
pub fn verify_bls_signature_with_namespace(
    group_public_key: &[u8; 96],
    namespace: &[u8],
    message: &[u8],
    signature: &[u8; 48],
    dst: &[u8],
) -> Result<bool> {
    let pk = blst::min_sig::PublicKey::from_bytes(group_public_key)
        .map_err(|e| anyhow::anyhow!("invalid BLS12-381 public key: {:?}", e))?;

    let sig = blst::min_sig::Signature::from_bytes(signature)
        .map_err(|e| anyhow::anyhow!("invalid BLS12-381 signature: {:?}", e))?;

    let payload = union_unique(namespace, message);

    let result = sig.verify(true, &payload, dst, &[], &pk, true);
    Ok(result == blst::BLST_ERROR::BLST_SUCCESS)
}

/// Replicates commonware's `union_unique(namespace, msg)` encoding.
///
/// Format: `varint(namespace.len()) || namespace || msg`
///
/// The varint uses protobuf-style encoding (7 data bits per byte, MSB continuation).
/// This matches `commonware_codec::varint::UInt<u32>::write()`.
///
/// This function is `pub(crate)` so that `bankd_provider` tests can construct
/// signed payloads using the same encoding.
pub(crate) fn union_unique(namespace: &[u8], msg: &[u8]) -> Vec<u8> {
    let len = namespace.len() as u32;
    let mut buf = Vec::with_capacity(5 + namespace.len() + msg.len());
    write_varint_u32(len, &mut buf);
    buf.extend_from_slice(namespace);
    buf.extend_from_slice(msg);
    buf
}

/// Encode a u32 as a protobuf-style varint (7 data bits per byte, MSB continuation).
fn write_varint_u32(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        if value < 0x80 {
            buf.push(value as u8);
            return;
        }
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
}

/// Validate that raw bytes are a well-formed BLS12-381 G2 compressed point.
pub fn validate_group_public_key(key: &[u8]) -> Result<()> {
    ensure!(
        key.len() == 96,
        "group public key must be 96 bytes, got {}",
        key.len()
    );
    let pk = blst::min_sig::PublicKey::from_bytes(key)
        .map_err(|e| anyhow::anyhow!("invalid BLS12-381 public key: {:?}", e))?;
    pk.validate()
        .map_err(|e| anyhow::anyhow!("BLS12-381 public key failed subgroup check: {:?}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplex_namespace_value() {
        assert_eq!(SIMPLEX_NAMESPACE, b"_COMMONWARE_KORA_SIMPLEX");
        assert_eq!(SIMPLEX_NAMESPACE.len(), 24);
    }

    #[test]
    fn test_bls_dst_value() {
        assert_eq!(BLS_DST, b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_");
    }

    #[test]
    fn test_union_unique_format() {
        // For namespace length 24 (< 128), varint is a single byte 0x18
        let result = union_unique(b"_COMMONWARE_KORA_SIMPLEX", b"hello");
        assert_eq!(result[0], 24); // varint of 24
        assert_eq!(&result[1..25], b"_COMMONWARE_KORA_SIMPLEX");
        assert_eq!(&result[25..], b"hello");
        assert_eq!(result.len(), 1 + 24 + 5);
    }

    #[test]
    fn test_union_unique_large_namespace() {
        // For namespace length 200 (>= 128), varint is two bytes
        let ns = vec![0xAA; 200];
        let result = union_unique(&ns, b"msg");
        // 200 = 0xC8 => varint: [0xC8 | 0x80, 0x01] = [0xC8, 0x01]
        assert_eq!(result[0], 0xC8); // (200 & 0x7F) | 0x80 = 0x48 | 0x80
        assert_eq!(result[1], 0x01); // 200 >> 7 = 1
        assert_eq!(&result[2..202], &ns[..]);
        assert_eq!(&result[202..], b"msg");
    }

    #[test]
    fn test_write_varint_u32() {
        let mut buf = Vec::new();

        // Single byte cases
        write_varint_u32(0, &mut buf);
        assert_eq!(buf, &[0x00]);
        buf.clear();

        write_varint_u32(1, &mut buf);
        assert_eq!(buf, &[0x01]);
        buf.clear();

        write_varint_u32(127, &mut buf);
        assert_eq!(buf, &[0x7F]);
        buf.clear();

        // Two byte cases
        write_varint_u32(128, &mut buf);
        assert_eq!(buf, &[0x80, 0x01]);
        buf.clear();

        write_varint_u32(300, &mut buf);
        assert_eq!(buf, &[0xAC, 0x02]);
        buf.clear();
    }

    #[test]
    fn test_invalid_public_key_bytes() {
        let bad_pk = [0u8; 96];
        let msg = b"hello";
        let sig = [0u8; 48];
        // Zero bytes are not a valid G2 point (point at infinity rejected by blst)
        let result = verify_bls_threshold_signature(&bad_pk, msg, &sig);
        match result {
            Ok(valid) => assert!(!valid),
            Err(_) => {} // also acceptable
        }
    }

    #[test]
    fn test_invalid_signature_bytes() {
        // Generate a valid public key via blst to test invalid signature
        let ikm = [42u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();

        let bad_sig = [0xFFu8; 48]; // not a valid G1 point
        let result = verify_bls_threshold_signature(&pk_bytes, b"msg", &bad_sig);
        match result {
            Ok(valid) => assert!(!valid),
            Err(_) => {} // also acceptable
        }
    }

    #[test]
    fn test_valid_signature_roundtrip() {
        // Generate a keypair, sign a message, and verify
        let ikm = [99u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();

        let message = b"test consensus digest";

        // Sign using the same union_unique + DST that our verification expects
        let payload = union_unique(SIMPLEX_NAMESPACE, message);
        let sig = sk.sign(&payload, BLS_DST, &[]);
        let sig_bytes: [u8; 48] = sig.compress();

        // Verify via our function
        let valid = verify_bls_threshold_signature(&pk_bytes, message, &sig_bytes)
            .expect("verification should not error");
        assert!(valid, "valid signature should verify");
    }

    #[test]
    fn test_tampered_message_rejects() {
        let ikm = [99u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();

        let message = b"original message";
        let payload = union_unique(SIMPLEX_NAMESPACE, message);
        let sig = sk.sign(&payload, BLS_DST, &[]);
        let sig_bytes: [u8; 48] = sig.compress();

        // Verify with tampered message
        let valid = verify_bls_threshold_signature(&pk_bytes, b"tampered message", &sig_bytes)
            .expect("verification should not error");
        assert!(!valid, "tampered message should not verify");
    }

    #[test]
    fn test_wrong_namespace_rejects() {
        let ikm = [99u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();

        let message = b"test message";

        // Sign with a different namespace
        let wrong_payload = union_unique(b"WRONG_NAMESPACE", message);
        let sig = sk.sign(&wrong_payload, BLS_DST, &[]);
        let sig_bytes: [u8; 48] = sig.compress();

        // Verify expects SIMPLEX_NAMESPACE, so this should fail
        let valid = verify_bls_threshold_signature(&pk_bytes, message, &sig_bytes)
            .expect("verification should not error");
        assert!(!valid, "wrong namespace should not verify");
    }

    #[test]
    fn test_wrong_key_rejects() {
        let ikm1 = [99u8; 32];
        let sk1 = blst::min_sig::SecretKey::key_gen(&ikm1, &[]).expect("keygen");

        let ikm2 = [77u8; 32];
        let sk2 = blst::min_sig::SecretKey::key_gen(&ikm2, &[]).expect("keygen");
        let pk2 = sk2.sk_to_pk();
        let pk2_bytes: [u8; 96] = pk2.compress();

        let message = b"test message";
        let payload = union_unique(SIMPLEX_NAMESPACE, message);
        let sig = sk1.sign(&payload, BLS_DST, &[]);
        let sig_bytes: [u8; 48] = sig.compress();

        // Verify with wrong public key
        let valid = verify_bls_threshold_signature(&pk2_bytes, message, &sig_bytes)
            .expect("verification should not error");
        assert!(!valid, "wrong public key should not verify");
    }

    #[test]
    fn test_validate_group_public_key_valid() {
        let ikm = [42u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes = pk.compress();
        validate_group_public_key(&pk_bytes).expect("valid key should pass validation");
    }

    #[test]
    fn test_validate_group_public_key_wrong_length() {
        let result = validate_group_public_key(&[0u8; 32]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("96 bytes"));
    }

    #[test]
    fn test_verify_with_explicit_namespace() {
        let ikm = [99u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();

        let namespace = b"CUSTOM_NS";
        let message = b"custom msg";
        let payload = union_unique(namespace, message);
        let sig = sk.sign(&payload, BLS_DST, &[]);
        let sig_bytes: [u8; 48] = sig.compress();

        let valid =
            verify_bls_signature_with_namespace(&pk_bytes, namespace, message, &sig_bytes, BLS_DST)
                .expect("verification should not error");
        assert!(valid);
    }
}
