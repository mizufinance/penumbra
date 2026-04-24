use anyhow::{anyhow, bail, Result};
use decaf377::{Element, Encoding, Fq, Fr};
use serde::{Deserialize, Serialize};

use crate::{
    crypto::{compute_metadata_hash, derive_compliance_scalar, verify_dleq_native},
    indexed_tree::string_to_fq,
    structs::DleqProof,
};

/// Canonical Penumbra transfer-tier labels for DLEQ metadata binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u64)]
#[serde(rename_all = "snake_case")]
pub enum TransferTierKind {
    SenderCore = 1,
    SenderExt = 2,
    OutputCore = 3,
    OutputExt = 4,
}

impl TransferTierKind {
    pub fn as_u64(self) -> u64 {
        self as u64
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SenderCore => "sender_core",
            Self::SenderExt => "sender_ext",
            Self::OutputCore => "output_core",
            Self::OutputExt => "output_ext",
        }
    }
}

/// Penumbra-owned canonical transfer metadata statement.
///
/// This is the metadata that the transfer DLEQ binds to the ACK/EPK relation.
/// It is distinct from the current Orbis anchor-object metadata used by
/// `store_secret`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferTierMetadataStatement {
    /// Canonical 32-byte little-endian encoding of the subject's `B_d` field element.
    pub subject_b_d_bytes: [u8; 32],
    pub ring_id_hash_bytes: [u8; 32],
    pub policy_id_hash_bytes: [u8; 32],
    pub resource_hash_bytes: [u8; 32],
    pub permission_hash_bytes: [u8; 32],
    pub tier: TransferTierKind,
    pub target_timestamp: u64,
    pub salt_bytes: [u8; 32],
}

impl TransferTierMetadataStatement {
    pub fn new(
        subject_b_d: Fq,
        ring_id_hash: Fq,
        policy_id_hash: Fq,
        resource_hash: Fq,
        permission_hash: Fq,
        tier: TransferTierKind,
        target_timestamp: u64,
        salt: Fq,
    ) -> Self {
        Self {
            subject_b_d_bytes: subject_b_d.to_bytes(),
            ring_id_hash_bytes: ring_id_hash.to_bytes(),
            policy_id_hash_bytes: policy_id_hash.to_bytes(),
            resource_hash_bytes: resource_hash.to_bytes(),
            permission_hash_bytes: permission_hash.to_bytes(),
            tier,
            target_timestamp,
            salt_bytes: salt.to_bytes(),
        }
    }

    pub fn from_identifiers(
        subject_b_d: Fq,
        ring_id: &str,
        policy_id: &str,
        resource: &str,
        permission: &str,
        tier: TransferTierKind,
        target_timestamp: u64,
        salt: Fq,
    ) -> Self {
        Self::new(
            subject_b_d,
            string_to_fq(ring_id),
            string_to_fq(policy_id),
            string_to_fq(resource),
            string_to_fq(permission),
            tier,
            target_timestamp,
            salt,
        )
    }

    pub fn validate_shape(&self) -> Result<()> {
        self.subject_b_d()?;
        self.ring_id_hash()?;
        self.policy_id_hash()?;
        self.resource_hash()?;
        self.permission_hash()?;
        self.salt()?;

        if self.target_timestamp == 0 {
            bail!("target_timestamp must be non-zero");
        }

        Ok(())
    }

    pub fn metadata_hash(&self) -> Result<Fq> {
        self.validate_shape()?;
        Ok(compute_metadata_hash(
            self.policy_id_hash()?,
            self.resource_hash()?,
            self.permission_hash()?,
            Fq::from(self.tier.as_u64()),
            Fq::from(self.target_timestamp),
            self.salt()?,
        ))
    }

    pub fn subject_ack(&self, ring_pk: &Element) -> Result<Element> {
        let d = derive_compliance_scalar(self.subject_b_d()?);
        let d_fr = Fr::from_bytes_checked(&d.to_bytes())
            .map_err(|_| anyhow!("derived compliance scalar is not a canonical Fr"))?;
        Ok(*ring_pk * d_fr)
    }

    pub fn subject_b_d(&self) -> Result<Fq> {
        parse_fq(self.subject_b_d_bytes, "subject_b_d_bytes")
    }

    pub fn policy_id_hash(&self) -> Result<Fq> {
        parse_fq(self.policy_id_hash_bytes, "policy_id_hash_bytes")
    }

    pub fn ring_id_hash(&self) -> Result<Fq> {
        parse_fq(self.ring_id_hash_bytes, "ring_id_hash_bytes")
    }

    pub fn resource_hash(&self) -> Result<Fq> {
        parse_fq(self.resource_hash_bytes, "resource_hash_bytes")
    }

    pub fn permission_hash(&self) -> Result<Fq> {
        parse_fq(self.permission_hash_bytes, "permission_hash_bytes")
    }

    pub fn salt(&self) -> Result<Fq> {
        parse_fq(self.salt_bytes, "salt_bytes")
    }
}

/// Penumbra-owned public decode contract for one logical transfer tier.
///
/// This object is public-data-only and contains no seed material. It models the
/// long-term object Penumbra wants Orbis to authorize over, while remaining
/// explicit that current live PRE operates on a separate Orbis-compatible
/// encrypted-seed object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTransferTierDecodeObject {
    pub statement: TransferTierMetadataStatement,
    /// Public tier EPK from the transfer ciphertext.
    pub epk_bytes: [u8; 32],
    /// Public tier C2 from the transfer ciphertext.
    pub c2_bytes: [u8; 32],
    /// Public `S = r x ACK` point needed to validate the transfer DLEQ offline.
    pub shared_point_bytes: [u8; 32],
    pub dleq_challenge_bytes: [u8; 32],
    pub dleq_response_bytes: [u8; 32],
}

impl PublicTransferTierDecodeObject {
    pub fn new(
        statement: TransferTierMetadataStatement,
        epk: &Element,
        c2: Fq,
        shared_point: &Element,
        proof: &DleqProof,
    ) -> Self {
        Self {
            statement,
            epk_bytes: epk.vartime_compress().0,
            c2_bytes: c2.to_bytes(),
            shared_point_bytes: shared_point.vartime_compress().0,
            dleq_challenge_bytes: proof.c.to_bytes(),
            dleq_response_bytes: proof.s.to_bytes(),
        }
    }

    pub fn validate_shape(&self) -> Result<()> {
        self.statement.validate_shape()?;
        self.epk()?;
        self.c2()?;
        self.shared_point()?;
        self.dleq_proof()?;
        Ok(())
    }

    /// Validate the canonical Penumbra metadata binding for this tier object.
    ///
    /// This checks the transfer DLEQ only. It does not prove that `c2` encrypts
    /// a valid seed; that remains a Penumbra zk-circuit responsibility.
    pub fn validate(&self, ring_pk: &Element) -> Result<()> {
        self.validate_shape()?;
        let metadata_hash = self.statement.metadata_hash()?;
        let ack = self.statement.subject_ack(ring_pk)?;
        let epk = self.epk()?;
        let shared_point = self.shared_point()?;
        let proof = self.dleq_proof()?;
        verify_dleq_native(&ack, &epk, &shared_point, &proof.c, &proof.s, metadata_hash)
    }

    pub fn epk(&self) -> Result<Element> {
        parse_element(self.epk_bytes, "epk_bytes")
    }

    pub fn c2(&self) -> Result<Fq> {
        parse_fq(self.c2_bytes, "c2_bytes")
    }

    pub fn shared_point(&self) -> Result<Element> {
        parse_element(self.shared_point_bytes, "shared_point_bytes")
    }

    pub fn dleq_proof(&self) -> Result<DleqProof> {
        let c = parse_fq(self.dleq_challenge_bytes, "dleq_challenge_bytes")?;
        let s = Fr::from_bytes_checked(&self.dleq_response_bytes)
            .map_err(|_| anyhow!("invalid dleq_response_bytes"))?;
        Ok(DleqProof { c, s })
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Into::into)
    }
}

fn parse_fq(bytes: [u8; 32], label: &str) -> Result<Fq> {
    Fq::from_bytes_checked(&bytes).map_err(|_| anyhow!("invalid {label}"))
}

fn parse_element(bytes: [u8; 32], label: &str) -> Result<Element> {
    Encoding(bytes)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid {label}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    use crate::crypto::compute_dleq_native;

    fn valid_decode_object() -> (PublicTransferTierDecodeObject, Element) {
        let mut rng = OsRng;
        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;

        let subject_b_d = Fq::from(42u64);
        let statement = TransferTierMetadataStatement::from_identifiers(
            subject_b_d,
            "ring-id",
            "policy-id",
            "document",
            "read",
            TransferTierKind::OutputCore,
            1_700_000_000,
            Fq::from(777u64),
        );
        let ack = statement.subject_ack(&ring_pk).expect("ack should derive");

        let r = Fr::rand(&mut rng);
        let k = Fr::rand(&mut rng);
        let epk = Element::GENERATOR * r;
        let shared_point = ack * r;
        let c2 = Fq::from(1234u64);
        let metadata_hash = statement
            .metadata_hash()
            .expect("metadata hash should compute");
        let proof = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

        (
            PublicTransferTierDecodeObject::new(statement, &epk, c2, &shared_point, &proof),
            ring_pk,
        )
    }

    #[test]
    fn transfer_tier_decode_object_roundtrips_json() {
        let (object, ring_pk) = valid_decode_object();
        let json = object.to_json().expect("object should serialize");
        let decoded =
            PublicTransferTierDecodeObject::from_json(&json).expect("object should deserialize");

        assert_eq!(decoded, object);
        decoded
            .validate(&ring_pk)
            .expect("roundtripped object should stay valid");
    }

    #[test]
    fn transfer_tier_decode_object_rejects_wrong_tier() {
        let (mut object, ring_pk) = valid_decode_object();
        object.statement.tier = TransferTierKind::OutputExt;

        let error = object
            .validate(&ring_pk)
            .expect_err("tampered tier should fail binding validation");
        assert!(error.to_string().contains("challenge mismatch"));
    }

    #[test]
    fn transfer_tier_decode_object_rejects_wrong_timestamp() {
        let (mut object, ring_pk) = valid_decode_object();
        object.statement.target_timestamp += 1;

        let error = object
            .validate(&ring_pk)
            .expect_err("tampered timestamp should fail binding validation");
        assert!(error.to_string().contains("challenge mismatch"));
    }

    #[test]
    fn transfer_tier_decode_object_rejects_wrong_subject_linkage() {
        let (mut object, ring_pk) = valid_decode_object();
        object.statement.subject_b_d_bytes = Fq::from(999u64).to_bytes();

        let error = object
            .validate(&ring_pk)
            .expect_err("tampered subject linkage should fail binding validation");
        assert!(error.to_string().contains("challenge mismatch"));
    }

    #[test]
    fn transfer_tier_decode_object_rejects_tampered_dleq() {
        let (mut object, ring_pk) = valid_decode_object();
        object.dleq_challenge_bytes = Fq::from(5u64).to_bytes();

        let error = object
            .validate(&ring_pk)
            .expect_err("tampered challenge should fail binding validation");
        assert!(error.to_string().contains("challenge mismatch"));
    }

    #[test]
    fn transfer_tier_decode_object_rejects_invalid_point_encoding() {
        let (mut object, _) = valid_decode_object();
        object.epk_bytes = [0xff; 32];

        let error = object
            .validate_shape()
            .expect_err("malformed point encoding should fail shape validation");
        assert!(error.to_string().contains("epk_bytes"));
    }
}
