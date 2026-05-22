//! Penumbra-side helpers for Orbis PRE interoperability.
//!
//! This module intentionally does not model Orbis as an in-process service.
//! Penumbra owns the deterministic math needed to:
//! - verify the canonical Penumbra transfer-tier DLEQ material,
//! - recover transfer seeds from Orbis re-encryption commitments when needed for
//!   Penumbra-side validation.

use anyhow::Result;
use decaf377::{Element, Fq, Fr};

use crate::crypto::{derive_compliance_scalar, verify_dleq_native};

fn ack_from_subject_derivation(
    ring_pk: &Element,
    subject_derivation_bytes: &[u8],
) -> (Element, Fr) {
    let subject_derivation = Fq::from_le_bytes_mod_order(subject_derivation_bytes);
    let d = derive_compliance_scalar(subject_derivation);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    (*ring_pk * d_fr, d_fr)
}

/// Compute the ring public key from a ring secret.
pub fn compute_ring_pk(sk_ring: &Fr) -> Element {
    Element::GENERATOR * *sk_ring
}

/// Compute the Orbis re-encryption commitment for one tier.
pub fn compute_reencrypt_commitment(
    sk_ring: &Fr,
    epk_tier: &Element,
    pk_issuer: &Element,
    subject_derivation_bytes: &[u8],
) -> Element {
    let (_, d_fr) =
        ack_from_subject_derivation(&compute_ring_pk(sk_ring), subject_derivation_bytes);
    (*pk_issuer + *epk_tier) * (d_fr * *sk_ring)
}

/// Verify the canonical Penumbra transfer-tier DLEQ for one PRE request.
pub fn verify_reencrypt_proof(
    ack_tier: &Element,
    epk_tier: &Element,
    shared_point: &Element,
    dleq_c: &Fq,
    dleq_s: &Fr,
    metadata_hash: Fq,
) -> Result<()> {
    verify_dleq_native(
        ack_tier,
        epk_tier,
        shared_point,
        dleq_c,
        dleq_s,
        metadata_hash,
    )
}

/// Verify the transfer-tier proof material and, on success, compute `xnc_cmt`.
pub fn verify_and_compute_reencrypt_commitment(
    sk_ring: &Fr,
    epk_tier: &Element,
    pk_issuer: &Element,
    subject_derivation_bytes: &[u8],
    dleq_c: &Fq,
    dleq_s: &Fr,
    metadata_hash: Fq,
) -> Result<Element> {
    let ring_pk = compute_ring_pk(sk_ring);
    let (ack_tier, d_fr) = ack_from_subject_derivation(&ring_pk, subject_derivation_bytes);
    let shared_point = *epk_tier * (d_fr * *sk_ring);

    verify_reencrypt_proof(
        &ack_tier,
        epk_tier,
        &shared_point,
        dleq_c,
        dleq_s,
        metadata_hash,
    )?;

    Ok((*pk_issuer + *epk_tier) * (d_fr * *sk_ring))
}

/// Recover the encryption seed from an Orbis re-encryption commitment.
pub fn recover_seed(xnc_cmt: &Element, sk_issuer: &Fr, ack_tier: &Element, c2: &Fq) -> Fq {
    let p = *xnc_cmt - (*ack_tier * *sk_issuer);
    let shared_fq = p.vartime_compress_to_field();
    *c2 - shared_fq
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Encoding;
    use hex::FromHex;
    use rand_core::OsRng;
    use serde::Deserialize;

    use crate::crypto::{compute_dleq_native, compute_metadata_hash, derive_compliance_scalar};

    #[derive(Deserialize)]
    struct OrbisDecaf377Fixture {
        dkg_pk_hex: String,
        enc_cmt_hex: String,
        shared_point_hex: String,
        challenge_hex: String,
        response_hex: String,
        metadata_hex: String,
    }

    fn decode_element(hex_value: &str) -> Element {
        let bytes = <[u8; 32]>::from_hex(hex_value).expect("valid element hex");
        Encoding(bytes)
            .vartime_decompress()
            .expect("valid compressed element")
    }

    fn decode_fq(hex_value: &str) -> Fq {
        let bytes = <[u8; 32]>::from_hex(hex_value).expect("valid fq hex");
        Fq::from_le_bytes_mod_order(&bytes)
    }

    fn decode_fr(hex_value: &str) -> Fr {
        let bytes = <[u8; 32]>::from_hex(hex_value).expect("valid fr hex");
        Fr::from_le_bytes_mod_order(&bytes)
    }

    fn load_fixture() -> OrbisDecaf377Fixture {
        serde_json::from_str(include_str!("../testdata/orbis_decaf377_dleq_fixture.json"))
            .expect("fixture should deserialize")
    }

    fn derive_ack(ring_pk: &Element, subject_derivation: Fq) -> Element {
        let d = derive_compliance_scalar(subject_derivation);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    #[test]
    fn test_reencrypt_commitment_roundtrip() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = compute_ring_pk(&sk_ring);

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let subject_derivation = Fq::from(42u64);
        let subject_derivation_bytes = subject_derivation.to_bytes();
        let ack = derive_ack(&ring_pk, subject_derivation);

        let r = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let seed = Fq::rand(&mut rng);
        let c2 = seed + (ack * r).vartime_compress_to_field();

        let xnc_cmt =
            compute_reencrypt_commitment(&sk_ring, &epk, &pk_issuer, &subject_derivation_bytes);
        let recovered = recover_seed(&xnc_cmt, &sk_issuer, &ack, &c2);

        assert_eq!(seed, recovered, "issuer should recover the original seed");
    }

    #[test]
    fn test_tier_isolation_via_distinct_epks() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = compute_ring_pk(&sk_ring);

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let subject_derivation = Fq::from(42u64);
        let subject_derivation_bytes = subject_derivation.to_bytes();
        let ack = derive_ack(&ring_pk, subject_derivation);

        let r_1 = Fr::rand(&mut rng);
        let r_2 = Fr::rand(&mut rng);
        let epk_1 = Element::GENERATOR * r_1;
        let epk_2 = Element::GENERATOR * r_2;

        let seed = Fq::rand(&mut rng);
        let c2 = seed + (ack * r_1).vartime_compress_to_field();

        let xnc_1 =
            compute_reencrypt_commitment(&sk_ring, &epk_1, &pk_issuer, &subject_derivation_bytes);
        let recovered = recover_seed(&xnc_1, &sk_issuer, &ack, &c2);
        assert_eq!(seed, recovered, "matching EPK should recover the seed");

        let xnc_2 =
            compute_reencrypt_commitment(&sk_ring, &epk_2, &pk_issuer, &subject_derivation_bytes);
        let recovered_wrong = recover_seed(&xnc_2, &sk_issuer, &ack, &c2);
        assert_ne!(
            seed, recovered_wrong,
            "wrong EPK should not recover the seed"
        );
    }

    #[test]
    fn test_verify_and_compute_reencrypt_commitment_valid() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = compute_ring_pk(&sk_ring);

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let subject_derivation = Fq::from(42u64);
        let subject_derivation_bytes = subject_derivation.to_bytes();
        let ack = derive_ack(&ring_pk, subject_derivation);

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let seed = Fq::rand(&mut rng);
        let c2 = seed + (ack * r).vartime_compress_to_field();

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );
        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        let xnc_cmt = verify_and_compute_reencrypt_commitment(
            &sk_ring,
            &epk,
            &pk_issuer,
            &subject_derivation_bytes,
            &proof.c,
            &proof.s,
            metadata_hash,
        )
        .expect("DLEQ verification should succeed");

        let recovered = recover_seed(&xnc_cmt, &sk_issuer, &ack, &c2);
        assert_eq!(seed, recovered, "verified PRE should recover the seed");
    }

    #[test]
    fn test_verify_and_compute_reencrypt_commitment_rejects_tampered_challenge() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = compute_ring_pk(&sk_ring);
        let pk_issuer = Element::GENERATOR * Fr::rand(&mut rng);
        let subject_derivation = Fq::from(42u64);
        let subject_derivation_bytes = subject_derivation.to_bytes();
        let ack = derive_ack(&ring_pk, subject_derivation);

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
        let bad_c = proof.c + Fq::from(1u64);

        let result = verify_and_compute_reencrypt_commitment(
            &sk_ring,
            &epk,
            &pk_issuer,
            &subject_derivation_bytes,
            &bad_c,
            &proof.s,
            metadata_hash,
        );
        assert!(
            result.is_err(),
            "tampered challenge should fail verification"
        );
    }

    #[test]
    fn test_verify_and_compute_reencrypt_commitment_rejects_wrong_metadata() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = compute_ring_pk(&sk_ring);
        let pk_issuer = Element::GENERATOR * Fr::rand(&mut rng);
        let subject_derivation = Fq::from(42u64);
        let subject_derivation_bytes = subject_derivation.to_bytes();
        let ack = derive_ack(&ring_pk, subject_derivation);

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
        let wrong_metadata = compute_metadata_hash(
            Fq::from(99u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        let result = verify_and_compute_reencrypt_commitment(
            &sk_ring,
            &epk,
            &pk_issuer,
            &subject_derivation_bytes,
            &proof.c,
            &proof.s,
            wrong_metadata,
        );
        assert!(result.is_err(), "wrong metadata should fail verification");
    }

    #[test]
    fn test_different_subject_derivations_produce_different_results() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = compute_ring_pk(&sk_ring);

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let subject_derivation_1 = Fq::from(42u64);
        let subject_derivation_2 = Fq::from(43u64);

        let ack_1 = derive_ack(&ring_pk, subject_derivation_1);
        let ack_2 = derive_ack(&ring_pk, subject_derivation_2);
        assert_ne!(
            ack_1, ack_2,
            "different subject derivations should change the ACK"
        );

        let r = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let seed = Fq::rand(&mut rng);
        let c2 = seed + (ack_1 * r).vartime_compress_to_field();

        let xnc_1 = compute_reencrypt_commitment(
            &sk_ring,
            &epk,
            &pk_issuer,
            &subject_derivation_1.to_bytes(),
        );
        let recovered_1 = recover_seed(&xnc_1, &sk_issuer, &ack_1, &c2);
        assert_eq!(seed, recovered_1);

        let xnc_2 = compute_reencrypt_commitment(
            &sk_ring,
            &epk,
            &pk_issuer,
            &subject_derivation_2.to_bytes(),
        );
        let recovered_2 = recover_seed(&xnc_2, &sk_issuer, &ack_2, &c2);
        assert_ne!(
            seed, recovered_2,
            "wrong subject derivation should not decrypt"
        );
    }

    #[test]
    fn test_real_orbis_decaf377_fixture_verifies() {
        let fixture = load_fixture();
        let dkg_pk = decode_element(&fixture.dkg_pk_hex);
        let enc_cmt = decode_element(&fixture.enc_cmt_hex);
        let shared_point = decode_element(&fixture.shared_point_hex);
        let challenge = decode_fq(&fixture.challenge_hex);
        let response = decode_fr(&fixture.response_hex);
        let metadata = decode_fq(&fixture.metadata_hex);

        verify_reencrypt_proof(
            &dkg_pk,
            &enc_cmt,
            &shared_point,
            &challenge,
            &response,
            metadata,
        )
        .expect("fixture should verify");
    }

    #[test]
    fn test_real_orbis_decaf377_fixture_rejects_wrong_metadata() {
        let fixture = load_fixture();
        let dkg_pk = decode_element(&fixture.dkg_pk_hex);
        let enc_cmt = decode_element(&fixture.enc_cmt_hex);
        let shared_point = decode_element(&fixture.shared_point_hex);
        let challenge = decode_fq(&fixture.challenge_hex);
        let response = decode_fr(&fixture.response_hex);
        let metadata = decode_fq(&fixture.metadata_hex);

        let result = verify_reencrypt_proof(
            &dkg_pk,
            &enc_cmt,
            &shared_point,
            &challenge,
            &response,
            metadata + Fq::from(1u64),
        );
        assert!(
            result.is_err(),
            "wrong metadata should fail real Orbis proof verification"
        );
    }

    #[test]
    fn test_real_orbis_decaf377_fixture_rejects_tampered_response() {
        let fixture = load_fixture();
        let dkg_pk = decode_element(&fixture.dkg_pk_hex);
        let enc_cmt = decode_element(&fixture.enc_cmt_hex);
        let shared_point = decode_element(&fixture.shared_point_hex);
        let challenge = decode_fq(&fixture.challenge_hex);
        let response = decode_fr(&fixture.response_hex);
        let metadata = decode_fq(&fixture.metadata_hex);

        let result = verify_reencrypt_proof(
            &dkg_pk,
            &enc_cmt,
            &shared_point,
            &challenge,
            &(response + Fr::from(1u64)),
            metadata,
        );
        assert!(
            result.is_err(),
            "tampered response should fail real Orbis proof verification"
        );
    }
}
