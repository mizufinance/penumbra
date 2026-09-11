use anyhow::{ensure, Result};
use ark_ff::{BigInteger, PrimeField};
use decaf377::{Element, Fr};
use once_cell::sync::Lazy;
use rand_core::{CryptoRng, RngCore};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DleqProof {
    pub commitment_g: Element,
    pub commitment_h: Element,
    pub response: Fr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuerDhEvidence {
    pub version: u32,
    pub asset_id: [u8; 32],
    pub ciphertext_epk: Element,
    pub issuer_dk_pub: Element,
    pub shared_point: Element,
    pub proof: DleqProof,
}

static ISSUER_DLEQ_DOMAIN: Lazy<decaf377::Fq> =
    Lazy::new(|| decaf377::Fq::from_le_bytes_mod_order(b"shieldd.issuer.dh_evidence.dleq.v1\0"));

impl IssuerDhEvidence {
    /// Produce verifiable decryption access for one ciphertext without exporting DK.
    pub fn prove(
        mut rng: impl RngCore + CryptoRng,
        dk: &crate::DetectionKey,
        asset_id: [u8; 32],
        ciphertext_epk: Element,
    ) -> Result<Self> {
        decaf377::Fq::from_bytes_checked(&asset_id)
            .map_err(|_| anyhow::anyhow!("issuer evidence asset ID is not canonical"))?;
        ensure_nonidentity("issuer ciphertext_epk", ciphertext_epk)?;
        ensure_nonidentity("issuer_dk_pub", dk.public_key())?;
        let nonce = loop {
            let nonce = Fr::rand(&mut rng);
            if nonce != Fr::from(0u64) {
                break nonce;
            }
        };
        let mut evidence = Self {
            version: 1,
            asset_id,
            ciphertext_epk,
            issuer_dk_pub: dk.public_key(),
            shared_point: ciphertext_epk * dk.0,
            proof: DleqProof {
                commitment_g: Element::GENERATOR * nonce,
                commitment_h: ciphertext_epk * nonce,
                response: Fr::from(0u64),
            },
        };
        evidence.proof.response = nonce + issuer_challenge(&evidence) * dk.0;
        Ok(evidence)
    }

    /// Expected asset, registered issuer key and EPK must come from accepted chain data.
    pub fn verify_for(
        &self,
        asset_id: [u8; 32],
        issuer_dk_pub: Element,
        ciphertext_epk: Element,
    ) -> Result<Element> {
        ensure!(
            self.asset_id == asset_id,
            "issuer disclosure asset mismatch"
        );
        ensure!(
            self.issuer_dk_pub == issuer_dk_pub,
            "issuer disclosure key mismatch"
        );
        ensure!(
            self.ciphertext_epk == ciphertext_epk,
            "issuer disclosure ciphertext mismatch"
        );
        self.verify()
    }

    pub fn verify(&self) -> Result<Element> {
        ensure!(self.version == 1, "unsupported issuer DH evidence version");
        decaf377::Fq::from_bytes_checked(&self.asset_id)
            .map_err(|_| anyhow::anyhow!("issuer evidence asset ID is not canonical"))?;
        ensure_nonidentity("issuer ciphertext_epk", self.ciphertext_epk)?;
        ensure_nonidentity("issuer_dk_pub", self.issuer_dk_pub)?;
        ensure_nonidentity("issuer shared point", self.shared_point)?;
        ensure_nonidentity("issuer DLEQ generator commitment", self.proof.commitment_g)?;
        ensure_nonidentity("issuer DLEQ EPK commitment", self.proof.commitment_h)?;
        verify_dleq(
            Element::GENERATOR,
            self.ciphertext_epk,
            self.issuer_dk_pub,
            self.shared_point,
            &self.proof,
            issuer_challenge(self),
        )?;
        Ok(self.shared_point)
    }
}

/// Verifies equations only; callers must validate points and bind the full
/// statement and proof commitments into a domain-separated challenge.
pub fn verify_dleq(
    base_g: Element,
    base_h: Element,
    point_g: Element,
    point_h: Element,
    proof: &DleqProof,
    challenge: Fr,
) -> Result<()> {
    ensure!(
        base_g * proof.response == proof.commitment_g + point_g * challenge,
        "invalid DLEQ generator equation"
    );
    ensure!(
        base_h * proof.response == proof.commitment_h + point_h * challenge,
        "invalid DLEQ second-base equation"
    );
    Ok(())
}

fn ensure_nonidentity(label: &str, point: Element) -> Result<()> {
    ensure!(!point.is_identity(), "{label} must not be identity");
    Ok(())
}

fn issuer_challenge(evidence: &IssuerDhEvidence) -> Fr {
    let asset_id = decaf377::Fq::from_bytes_checked(&evidence.asset_id)
        .expect("issuer evidence verification checked the asset ID encoding");
    let challenge = poseidon377::hash_7(
        &ISSUER_DLEQ_DOMAIN,
        (
            asset_id,
            Element::GENERATOR.vartime_compress_to_field(),
            evidence.issuer_dk_pub.vartime_compress_to_field(),
            evidence.ciphertext_epk.vartime_compress_to_field(),
            evidence.shared_point.vartime_compress_to_field(),
            evidence.proof.commitment_g.vartime_compress_to_field(),
            evidence.proof.commitment_h.vartime_compress_to_field(),
        ),
    );
    fq_to_challenge_scalar(challenge)
}

pub fn fq_to_challenge_scalar(challenge: decaf377::Fq) -> Fr {
    let mut bytes = challenge.into_bigint().to_bytes_le();
    bytes.resize(32, 0);
    let keep_bits = (Fr::MODULUS_BIT_SIZE - 1) as usize;
    let keep_bytes = keep_bits.div_ceil(8);
    let spare_bits = keep_bytes * 8 - keep_bits;
    bytes[keep_bytes - 1] &= 0xff >> spare_bits;
    Fr::from_le_bytes_mod_order(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer_proof(secret: Fr, epk: Element, asset_id: [u8; 32]) -> IssuerDhEvidence {
        let nonce = Fr::from(17u64);
        let mut evidence = IssuerDhEvidence {
            version: 1,
            asset_id,
            ciphertext_epk: epk,
            issuer_dk_pub: Element::GENERATOR * secret,
            shared_point: epk * secret,
            proof: DleqProof {
                commitment_g: Element::GENERATOR * nonce,
                commitment_h: epk * nonce,
                response: Fr::from(0u64),
            },
        };
        evidence.proof.response = nonce + issuer_challenge(&evidence) * secret;
        evidence
    }

    #[test]
    fn issuer_evidence_binds_asset_epk_and_shared_point() {
        let evidence = issuer_proof(
            Fr::from(5u64),
            Element::GENERATOR * Fr::from(7u64),
            decaf377::Fq::from(11u64).to_bytes(),
        );
        assert_eq!(evidence.verify().unwrap(), evidence.shared_point);

        let mut wrong_asset = evidence.clone();
        wrong_asset.asset_id = decaf377::Fq::from(12u64).to_bytes();
        assert!(wrong_asset.verify().is_err());

        let mut wrong_epk = evidence.clone();
        wrong_epk.ciphertext_epk += Element::GENERATOR;
        assert!(wrong_epk.verify().is_err());

        let mut wrong_shared = evidence;
        wrong_shared.shared_point += Element::GENERATOR;
        assert!(wrong_shared.verify().is_err());
    }

    #[test]
    fn issuer_disclosure_uses_fresh_proof_and_pinned_chain_values() {
        let dk = crate::DetectionKey::new(Fr::from(5u64));
        let epk = Element::GENERATOR * Fr::from(7u64);
        let asset = decaf377::Fq::from(11u64).to_bytes();
        let first = IssuerDhEvidence::prove(rand_core::OsRng, &dk, asset, epk).unwrap();
        let second = IssuerDhEvidence::prove(rand_core::OsRng, &dk, asset, epk).unwrap();
        assert_ne!(first.proof.commitment_g, second.proof.commitment_g);
        assert_eq!(
            first.verify_for(asset, dk.public_key(), epk).unwrap(),
            epk * dk.0
        );
        assert!(first.verify_for(asset, Element::GENERATOR, epk).is_err());
        assert!(first
            .verify_for(asset, dk.public_key(), Element::GENERATOR)
            .is_err());
        assert!(first
            .verify_for(decaf377::Fq::from(12u64).to_bytes(), dk.public_key(), epk)
            .is_err());
        assert!(IssuerDhEvidence::prove(rand_core::OsRng, &dk, asset, Element::default()).is_err());
    }
}
