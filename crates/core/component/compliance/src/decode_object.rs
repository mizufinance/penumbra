use anyhow::{anyhow, ensure, Result};
use decaf377::Fq;
use serde::{Deserialize, Serialize};

use crate::indexed_tree::string_to_fq;

/// Canonical circuit-bound metadata for one fixed-shape transfer.
///
/// Tier identity is structural: the four salts and ciphertexts always appear
/// in sender-core, sender-extension, output-core, output-extension order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferComplianceMetadata {
    pub ring_id_hash_bytes: [u8; 32],
    pub policy_id_hash_bytes: [u8; 32],
    pub resource_hash_bytes: [u8; 32],
    pub permission_hash_bytes: [u8; 32],
    pub target_timestamp: u64,
    pub audit_epoch: u64,
    pub sender_core_salt_bytes: [u8; 32],
    pub sender_ext_salt_bytes: [u8; 32],
    pub output_core_salt_bytes: [u8; 32],
    pub output_ext_salt_bytes: [u8; 32],
}

pub const TRANSFER_COMPLIANCE_METADATA_BYTES: usize = 8 * 32 + 16;

impl TransferComplianceMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ring_id_hash: Fq,
        policy_id_hash: Fq,
        resource_hash: Fq,
        permission_hash: Fq,
        target_timestamp: u64,
        audit_epoch: u64,
        sender_core_salt: Fq,
        sender_ext_salt: Fq,
        output_core_salt: Fq,
        output_ext_salt: Fq,
    ) -> Self {
        Self {
            ring_id_hash_bytes: ring_id_hash.to_bytes(),
            policy_id_hash_bytes: policy_id_hash.to_bytes(),
            resource_hash_bytes: resource_hash.to_bytes(),
            permission_hash_bytes: permission_hash.to_bytes(),
            target_timestamp,
            audit_epoch,
            sender_core_salt_bytes: sender_core_salt.to_bytes(),
            sender_ext_salt_bytes: sender_ext_salt.to_bytes(),
            output_core_salt_bytes: output_core_salt.to_bytes(),
            output_ext_salt_bytes: output_ext_salt.to_bytes(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_identifiers(
        ring_id: &str,
        policy_id: &str,
        resource: &str,
        permission: &str,
        target_timestamp: u64,
        audit_epoch: u64,
        sender_core_salt: Fq,
        sender_ext_salt: Fq,
        output_core_salt: Fq,
        output_ext_salt: Fq,
    ) -> Self {
        Self::new(
            string_to_fq(ring_id),
            string_to_fq(policy_id),
            string_to_fq(resource),
            string_to_fq(permission),
            target_timestamp,
            audit_epoch,
            sender_core_salt,
            sender_ext_salt,
            output_core_salt,
            output_ext_salt,
        )
    }

    pub fn validate(&self) -> Result<()> {
        self.ring_id_hash()?;
        self.policy_id_hash()?;
        self.resource_hash()?;
        self.permission_hash()?;
        for salt in self.salts() {
            parse_fq(*salt, "transfer compliance salt")?;
        }
        ensure!(
            self.target_timestamp != 0,
            "target_timestamp must be non-zero"
        );
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(TRANSFER_COMPLIANCE_METADATA_BYTES);
        out.extend_from_slice(&self.ring_id_hash_bytes);
        out.extend_from_slice(&self.policy_id_hash_bytes);
        out.extend_from_slice(&self.resource_hash_bytes);
        out.extend_from_slice(&self.permission_hash_bytes);
        out.extend_from_slice(&self.target_timestamp.to_le_bytes());
        out.extend_from_slice(&self.audit_epoch.to_le_bytes());
        for salt in self.salts() {
            out.extend_from_slice(salt);
        }
        debug_assert_eq!(out.len(), TRANSFER_COMPLIANCE_METADATA_BYTES);
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() == TRANSFER_COMPLIANCE_METADATA_BYTES,
            "transfer compliance metadata must be {TRANSFER_COMPLIANCE_METADATA_BYTES} bytes, got {}",
            bytes.len()
        );
        let mut reader = MetadataReader::new(bytes);
        let metadata = Self {
            ring_id_hash_bytes: reader.read_array::<32>()?,
            policy_id_hash_bytes: reader.read_array::<32>()?,
            resource_hash_bytes: reader.read_array::<32>()?,
            permission_hash_bytes: reader.read_array::<32>()?,
            target_timestamp: reader.read_u64()?,
            audit_epoch: reader.read_u64()?,
            sender_core_salt_bytes: reader.read_array::<32>()?,
            sender_ext_salt_bytes: reader.read_array::<32>()?,
            output_core_salt_bytes: reader.read_array::<32>()?,
            output_ext_salt_bytes: reader.read_array::<32>()?,
        };
        reader.finish()?;
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn ring_id_hash(&self) -> Result<Fq> {
        parse_fq(self.ring_id_hash_bytes, "ring_id_hash_bytes")
    }

    pub fn policy_id_hash(&self) -> Result<Fq> {
        parse_fq(self.policy_id_hash_bytes, "policy_id_hash_bytes")
    }

    pub fn resource_hash(&self) -> Result<Fq> {
        parse_fq(self.resource_hash_bytes, "resource_hash_bytes")
    }

    pub fn permission_hash(&self) -> Result<Fq> {
        parse_fq(self.permission_hash_bytes, "permission_hash_bytes")
    }

    pub fn sender_core_salt(&self) -> Result<Fq> {
        parse_fq(self.sender_core_salt_bytes, "sender_core_salt_bytes")
    }

    pub fn sender_ext_salt(&self) -> Result<Fq> {
        parse_fq(self.sender_ext_salt_bytes, "sender_ext_salt_bytes")
    }

    pub fn output_core_salt(&self) -> Result<Fq> {
        parse_fq(self.output_core_salt_bytes, "output_core_salt_bytes")
    }

    pub fn output_ext_salt(&self) -> Result<Fq> {
        parse_fq(self.output_ext_salt_bytes, "output_ext_salt_bytes")
    }

    fn salts(&self) -> [&[u8; 32]; 4] {
        [
            &self.sender_core_salt_bytes,
            &self.sender_ext_salt_bytes,
            &self.output_core_salt_bytes,
            &self.output_ext_salt_bytes,
        ]
    }
}

fn parse_fq(bytes: [u8; 32], label: &str) -> Result<Fq> {
    Fq::from_bytes_checked(&bytes).map_err(|_| anyhow!("invalid {label}"))
}

struct MetadataReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> MetadataReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or_else(|| anyhow!("metadata offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| anyhow!("truncated transfer compliance metadata"))?
            .try_into()
            .expect("slice length is checked");
        self.offset = end;
        Ok(value)
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_array::<8>()?))
    }

    fn finish(self) -> Result<()> {
        ensure!(
            self.offset == self.bytes.len(),
            "trailing bytes in transfer compliance metadata"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> TransferComplianceMetadata {
        TransferComplianceMetadata::from_identifiers(
            "ring-id",
            "policy-id",
            "document",
            "read",
            1_700_000_000,
            1,
            Fq::from(11u64),
            Fq::from(12u64),
            Fq::from(13u64),
            Fq::from(14u64),
        )
    }

    #[test]
    fn transfer_metadata_has_one_canonical_fixed_encoding() {
        let metadata = metadata();
        let encoded = metadata.to_bytes().expect("metadata should encode");
        assert_eq!(encoded.len(), TRANSFER_COMPLIANCE_METADATA_BYTES);
        assert_eq!(
            TransferComplianceMetadata::from_bytes(&encoded).expect("metadata should decode"),
            metadata
        );

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(TransferComplianceMetadata::from_bytes(&trailing).is_err());
        assert!(TransferComplianceMetadata::from_bytes(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn transfer_metadata_does_not_publish_subject_derivations() {
        let encoded = metadata().to_bytes().expect("metadata should encode");
        for derivation in [Fq::from(41u64), Fq::from(42u64)] {
            assert!(
                !encoded
                    .windows(32)
                    .any(|window| window == derivation.to_bytes()),
                "subject derivations must not be serialized with transfer metadata"
            );
        }
    }

    #[test]
    fn transfer_metadata_wire_is_exactly_the_factored_record() {
        let metadata = metadata();
        let mut expected = Vec::with_capacity(TRANSFER_COMPLIANCE_METADATA_BYTES);
        expected.extend_from_slice(&metadata.ring_id_hash_bytes);
        expected.extend_from_slice(&metadata.policy_id_hash_bytes);
        expected.extend_from_slice(&metadata.resource_hash_bytes);
        expected.extend_from_slice(&metadata.permission_hash_bytes);
        expected.extend_from_slice(&metadata.target_timestamp.to_le_bytes());
        expected.extend_from_slice(&metadata.audit_epoch.to_le_bytes());
        expected.extend_from_slice(&metadata.sender_core_salt_bytes);
        expected.extend_from_slice(&metadata.sender_ext_salt_bytes);
        expected.extend_from_slice(&metadata.output_core_salt_bytes);
        expected.extend_from_slice(&metadata.output_ext_salt_bytes);

        assert_eq!(expected.len(), TRANSFER_COMPLIANCE_METADATA_BYTES);
        assert_eq!(
            metadata.to_bytes().expect("metadata should encode"),
            expected
        );
    }

    #[test]
    fn transfer_metadata_rejects_zero_timestamp_and_noncanonical_fields() {
        let mut zero_timestamp = metadata();
        zero_timestamp.target_timestamp = 0;
        assert!(zero_timestamp.to_bytes().is_err());

        let mut encoded = metadata().to_bytes().expect("metadata should encode");
        encoded[..32].fill(0xff);
        assert!(TransferComplianceMetadata::from_bytes(&encoded).is_err());
    }
}
