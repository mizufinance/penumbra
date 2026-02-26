//! Address Compliance Key
//!
//! Wrapper around a curve point representing a compliance key bound to a specific address.

use serde::{Deserialize, Serialize};

use penumbra_sdk_proto::penumbra::core::component::compliance::v1 as pb;
use penumbra_sdk_proto::DomainType;

pub const CVK_LEN_BYTES: usize = 32;

/// Address Compliance Key (Public).
///
/// Wrapper around a curve point representing a compliance key bound to a specific address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::ComplianceViewingKey",
    into = "pb::ComplianceViewingKey"
)]
pub struct AddressComplianceKey(pub decaf377::Element);

impl AddressComplianceKey {
    /// Create a new address compliance key from a curve point.
    pub fn new(point: decaf377::Element) -> Self {
        Self(point)
    }

    /// Convert to bytes (compressed curve point encoding).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.vartime_compress().0
    }

    /// Try to create from bytes (decompressed curve point).
    pub fn from_bytes(bytes: [u8; 32]) -> anyhow::Result<Self> {
        let point = decaf377::Encoding(bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid address compliance key bytes"))?;
        Ok(Self(point))
    }

    /// Access the inner curve point.
    pub fn inner(&self) -> &decaf377::Element {
        &self.0
    }
}

// Protobuf serialization for AddressComplianceKey
impl DomainType for AddressComplianceKey {
    type Proto = pb::ComplianceViewingKey;
}

impl TryFrom<pb::ComplianceViewingKey> for AddressComplianceKey {
    type Error = anyhow::Error;

    fn try_from(value: pb::ComplianceViewingKey) -> Result<Self, Self::Error> {
        let bytes: [u8; 32] = value
            .inner
            .try_into()
            .map_err(|_| anyhow::anyhow!("expected 32 byte array"))?;
        AddressComplianceKey::from_bytes(bytes)
    }
}

impl From<AddressComplianceKey> for pb::ComplianceViewingKey {
    fn from(value: AddressComplianceKey) -> pb::ComplianceViewingKey {
        pb::ComplianceViewingKey {
            inner: value.to_bytes().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_key_serialization() {
        let point = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ack = AddressComplianceKey::new(point);

        let bytes = ack.to_bytes();
        let recovered = AddressComplianceKey::from_bytes(bytes).unwrap();

        assert_eq!(ack, recovered, "Serialization round-trip must preserve key");
    }
}
