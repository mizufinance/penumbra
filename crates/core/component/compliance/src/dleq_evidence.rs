use anyhow::{ensure, Result};
use ark_ff::{BigInteger, PrimeField};
use decaf377::{Element, Fr};
use once_cell::sync::Lazy;

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
}
