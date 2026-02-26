//! Orbis Proxy Re-Encryption (PRE) interface.
//!
//! Provides the interface for Orbis PRE operations on compliance ciphertexts.
//! In production, Orbis is a threshold MPC network holding shares of sk_ring.
//! For testing, a simulated Orbis holds sk_ring directly.
//!
//! ## How PRE Works
//!
//! A single derivation scalar `d = SHA256_derive(b_d_fq)` is stored in each
//! user's compliance leaf. The ACK is `d × ring_pk`. Tier isolation comes from
//! 3 distinct EPKs (r_1, r_2, r_3), not from separate derivation scalars.
//!
//! When a user encrypts: `C2_i = S_i + (r_i × ACK).compress()`
//!
//! Orbis computes: `xnc_cmt = d × sk_ring × (PK_issuer + epk_i)`
//!
//! The issuer recovers: `P = xnc_cmt - SK_issuer × ACK`, then `S_i = C2_i - P.compress()`

use anyhow::{anyhow, Result};
use decaf377::{Element, Fq, Fr};

use crate::crypto::{fq_to_challenge_scalar, ENCRYPT_PROOF_DOMAIN};

/// Trait for Orbis re-encryption operations.
///
/// In production, this makes network calls to the Orbis MPC.
/// For testing, use [`SimulatedOrbis`] which holds sk_ring directly.
pub trait OrbisReencryptor {
    /// Compute the re-encryption commitment for one tier's EPK.
    ///
    /// `b_d_bytes`: the user's diversified basepoint field element bytes.
    /// Derivation uses SHA256 internally (matching Orbis `derive_capability_scalar`).
    fn reencrypt(&self, epk_tier: &Element, pk_issuer: &Element, b_d_bytes: &[u8]) -> Element;

    /// Verify DLEQ proof, then compute the re-encryption commitment.
    ///
    /// Returns `Err` if the DLEQ proof is invalid. On success, returns
    /// the same `xnc_cmt` as [`reencrypt`].
    fn verify_and_reencrypt(
        &self,
        epk_tier: &Element,
        pk_issuer: &Element,
        b_d_bytes: &[u8],
        dleq_c: &Fq,
        dleq_s: &Fr,
        metadata_hash: Fq,
    ) -> Result<Element>;
}

/// Simulated Orbis for testing.
///
/// Holds sk_ring directly (no MPC threshold).
pub struct SimulatedOrbis {
    sk_ring: Fr,
}

impl SimulatedOrbis {
    /// Create a new simulated Orbis with the given ring secret key.
    pub fn new(sk_ring: Fr) -> Self {
        Self { sk_ring }
    }

    /// Get the ring public key.
    pub fn ring_pk(&self) -> Element {
        Element::GENERATOR * self.sk_ring
    }
}

impl OrbisReencryptor for SimulatedOrbis {
    fn reencrypt(&self, epk_tier: &Element, pk_issuer: &Element, b_d_bytes: &[u8]) -> Element {
        let b_d_fq = Fq::from_le_bytes_mod_order(b_d_bytes);
        let d = crate::crypto::derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());

        // xnc_cmt = d × sk_ring × (PK_issuer + epk_tier)
        (*pk_issuer + epk_tier) * (d_fr * self.sk_ring)
    }

    fn verify_and_reencrypt(
        &self,
        epk_tier: &Element,
        pk_issuer: &Element,
        b_d_bytes: &[u8],
        dleq_c: &Fq,
        dleq_s: &Fr,
        metadata_hash: Fq,
    ) -> Result<Element> {
        let b_d_fq = Fq::from_le_bytes_mod_order(b_d_bytes);
        let d = crate::crypto::derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());

        // Orbis can compute ACK and S from sk_ring + d + EPK
        let ring_pk = self.ring_pk();
        let ack = ring_pk * d_fr;
        let s_point = *epk_tier * (d_fr * self.sk_ring); // S = r × ACK = d × sk_ring × EPK

        // Reconstruct R and R' from the DLEQ response
        let c_fr = Fr::from_le_bytes_mod_order(&dleq_c.to_bytes());
        let r_rec = Element::GENERATOR * *dleq_s - *epk_tier * c_fr;
        let rp_rec = ack * *dleq_s - s_point * c_fr;

        // Recompute challenge via hash_7 with Orbis-compatible ordering
        let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
        let g_fq = Element::GENERATOR.vartime_compress_to_field();
        let c_check = poseidon377::hash_7(
            &domain,
            (
                metadata_hash,
                g_fq,
                ack.vartime_compress_to_field(),
                epk_tier.vartime_compress_to_field(),
                s_point.vartime_compress_to_field(),
                r_rec.vartime_compress_to_field(),
                rp_rec.vartime_compress_to_field(),
            ),
        );
        let c_check_trunc =
            Fq::from_le_bytes_mod_order(&fq_to_challenge_scalar(c_check).to_bytes());

        if c_check_trunc != *dleq_c {
            return Err(anyhow!("DLEQ verification failed: challenge mismatch"));
        }

        // Proof valid — proceed with re-encryption
        Ok((*pk_issuer + epk_tier) * (d_fr * self.sk_ring))
    }
}

/// Recover the encryption seed from an Orbis re-encryption commitment.
///
/// Given `xnc_cmt` from Orbis, the issuer computes:
/// ```text
/// P = xnc_cmt - SK_issuer × ACK_tier
/// S = C2 - P.compress()
/// ```
pub fn recover_seed(xnc_cmt: &Element, sk_issuer: &Fr, ack_tier: &Element, c2: &Fq) -> Fq {
    let p = *xnc_cmt - (*ack_tier * *sk_issuer);
    let shared_fq = p.vartime_compress_to_field();
    *c2 - shared_fq
}

/// Compute the adjusted reader public key for bridging Penumbra EPKs into Orbis PRE.
///
/// Orbis computes `xnc_cmt = d * sk_ring * (reader_pk + enc_cmt_orbis)`.
/// To make this equal `d * sk_ring * (pk_issuer + epk_chain)`, set:
/// `reader_pk = pk_issuer + epk_chain - enc_cmt_orbis`.
pub fn compute_adjusted_reader_pk(
    pk_issuer: &Element,
    epk_chain: &Element,
    enc_cmt_orbis: &Element,
) -> Element {
    *pk_issuer + *epk_chain - *enc_cmt_orbis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{compute_dleq_native, compute_metadata_hash, derive_compliance_scalar};
    use rand_core::OsRng;

    /// Derive ACK from b_d_fq using SHA256 (native equivalent of circuit's derive_ack_from_leaf_d).
    fn derive_ack(ring_pk: &Element, b_d_fq: Fq) -> Element {
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    #[test]
    fn test_simulated_orbis_roundtrip() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let orbis = SimulatedOrbis::new(sk_ring);
        let ring_pk = orbis.ring_pk();

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let b_d_fq = Fq::from(42u64);
        let b_d_bytes = b_d_fq.to_bytes();

        // Single ACK from SHA256 derivation
        let ack = derive_ack(&ring_pk, b_d_fq);

        // Encryption: r × G as EPK, C2 = S + (r × ACK).compress()
        let r = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let seed = Fq::rand(&mut rng);
        let c2 = seed + (ack * r).vartime_compress_to_field();

        // Orbis re-encryption
        let xnc_cmt = orbis.reencrypt(&epk, &pk_issuer, &b_d_bytes);

        // Issuer recovery
        let recovered = recover_seed(&xnc_cmt, &sk_issuer, &ack, &c2);
        assert_eq!(seed, recovered, "Issuer should recover the original seed");
    }

    #[test]
    fn test_tier_isolation_via_distinct_epks() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let orbis = SimulatedOrbis::new(sk_ring);
        let ring_pk = orbis.ring_pk();

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let b_d_fq = Fq::from(42u64);
        let b_d_bytes = b_d_fq.to_bytes();

        // Single ACK (same for all tiers)
        let ack = derive_ack(&ring_pk, b_d_fq);

        // Encrypt with distinct EPKs per tier
        let r_1 = Fr::rand(&mut rng);
        let r_2 = Fr::rand(&mut rng);
        let epk_1 = Element::GENERATOR * r_1;
        let epk_2 = Element::GENERATOR * r_2;

        let seed_core = Fq::rand(&mut rng);
        let c2_core = seed_core + (ack * r_1).vartime_compress_to_field();

        // PRE with epk_1 → recovers core seed
        let xnc_1 = orbis.reencrypt(&epk_1, &pk_issuer, &b_d_bytes);
        let recovered_core = recover_seed(&xnc_1, &sk_issuer, &ack, &c2_core);
        assert_eq!(seed_core, recovered_core, "Core PRE should work");

        // PRE with epk_2 → does NOT recover core seed (wrong EPK)
        let xnc_2 = orbis.reencrypt(&epk_2, &pk_issuer, &b_d_bytes);
        let recovered_wrong = recover_seed(&xnc_2, &sk_issuer, &ack, &c2_core);
        assert_ne!(
            seed_core, recovered_wrong,
            "Wrong EPK should not recover core"
        );
    }

    #[test]
    fn test_verify_and_reencrypt_valid() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let orbis = SimulatedOrbis::new(sk_ring);
        let ring_pk = orbis.ring_pk();

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let b_d_fq = Fq::from(42u64);
        let b_d_bytes = b_d_fq.to_bytes();
        let ack = derive_ack(&ring_pk, b_d_fq);

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

        let xnc_cmt = orbis
            .verify_and_reencrypt(
                &epk,
                &pk_issuer,
                &b_d_bytes,
                &proof.c,
                &proof.s,
                metadata_hash,
            )
            .expect("DLEQ verification should succeed");

        let recovered = recover_seed(&xnc_cmt, &sk_issuer, &ack, &c2);
        assert_eq!(seed, recovered, "verify_and_reencrypt should recover seed");
    }

    #[test]
    fn test_verify_and_reencrypt_invalid_c() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let orbis = SimulatedOrbis::new(sk_ring);
        let ring_pk = orbis.ring_pk();

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let b_d_fq = Fq::from(42u64);
        let b_d_bytes = b_d_fq.to_bytes();
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

        // Tamper with c
        let bad_c = proof.c + Fq::from(1u64);
        let result = orbis.verify_and_reencrypt(
            &epk,
            &pk_issuer,
            &b_d_bytes,
            &bad_c,
            &proof.s,
            metadata_hash,
        );
        assert!(result.is_err(), "tampered c should fail verification");
    }

    #[test]
    fn test_verify_and_reencrypt_wrong_metadata() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let orbis = SimulatedOrbis::new(sk_ring);
        let ring_pk = orbis.ring_pk();

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let b_d_fq = Fq::from(42u64);
        let b_d_bytes = b_d_fq.to_bytes();
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

        // Use wrong metadata
        let wrong_metadata = compute_metadata_hash(
            Fq::from(99u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );
        let result = orbis.verify_and_reencrypt(
            &epk,
            &pk_issuer,
            &b_d_bytes,
            &proof.c,
            &proof.s,
            wrong_metadata,
        );
        assert!(result.is_err(), "wrong metadata should fail verification");
    }

    #[test]
    fn test_different_diversifiers_produce_different_results() {
        let mut rng = OsRng;

        let sk_ring = Fr::rand(&mut rng);
        let orbis = SimulatedOrbis::new(sk_ring);
        let ring_pk = orbis.ring_pk();

        let sk_issuer = Fr::rand(&mut rng);
        let pk_issuer = Element::GENERATOR * sk_issuer;

        let b_d_fq_1 = Fq::from(42u64);
        let b_d_fq_2 = Fq::from(43u64);

        let ack_1 = derive_ack(&ring_pk, b_d_fq_1);
        let ack_2 = derive_ack(&ring_pk, b_d_fq_2);
        assert_ne!(ack_1, ack_2, "Different diversifiers → different ACKs");

        // Encrypt with ACK_1
        let r = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let seed = Fq::rand(&mut rng);
        let c2 = seed + (ack_1 * r).vartime_compress_to_field();

        // Re-encrypt with correct b_d
        let xnc_1 = orbis.reencrypt(&epk, &pk_issuer, &b_d_fq_1.to_bytes());
        let recovered_1 = recover_seed(&xnc_1, &sk_issuer, &ack_1, &c2);
        assert_eq!(seed, recovered_1);

        // Re-encrypt with wrong b_d
        let xnc_2 = orbis.reencrypt(&epk, &pk_issuer, &b_d_fq_2.to_bytes());
        let recovered_2 = recover_seed(&xnc_2, &sk_issuer, &ack_2, &c2);
        assert_ne!(seed, recovered_2, "Wrong diversifier should not decrypt");
    }
}
