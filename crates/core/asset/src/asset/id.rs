use crate::Value;
use ark_ff::ToConstraintField;
use ark_serialize::CanonicalDeserialize;
use base64::Engine;
use decaf377::Fq;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{serializers::bech32str, shieldd::core::asset::v1 as pb, DomainType};

/// An identifier for an IBC asset type.
///
/// This is similar to, but different from, the design in [ADR001].  As in
/// ADR001, a denomination trace is hashed to a fixed-size identifier, but
/// unlike ADR001, we hash to a field element rather than a byte string.
///
/// A denomination trace looks like
///
/// - `denom` (native chain A asset)
/// - `transfer/channelToA/denom` (chain B representation of chain A asset)
/// - `transfer/channelToB/transfer/channelToA/denom` (chain C representation of chain B representation of chain A asset)
///
/// ADR001 defines the IBC asset ID as the SHA-256 hash of the denomination
/// trace.  Instead, Shieldd hashes to a field element, so that asset IDs can
/// be more easily used inside of a circuit.
///
/// [ADR001]:
/// https://github.com/cosmos/ibc-go/blob/main/docs/architecture/adr-001-coin-source-tracing.md
#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "pb::AssetId", into = "pb::AssetId")]
pub struct Id(pub Fq);

impl From<Id> for pb::AssetId {
    fn from(id: Id) -> Self {
        pb::AssetId {
            inner: id.0.to_bytes().to_vec(),
            // Never produce a proto encoding with the alt string encoding.
            alt_bech32m: String::new(),
            // Never produce a proto encoding with the alt base denom.
            alt_base_denom: String::new(),
        }
    }
}

impl TryFrom<pb::AssetId> for Id {
    type Error = anyhow::Error;
    fn try_from(value: pb::AssetId) -> Result<Self, Self::Error> {
        if !value.inner.is_empty() {
            if !value.alt_base_denom.is_empty() || !value.alt_bech32m.is_empty() {
                anyhow::bail!(
                    "AssetId proto has both inner and alt_bech32m or alt_base_denom fields set"
                );
            }
            value.inner.as_slice().try_into()
        } else if !value.alt_bech32m.is_empty() {
            value.alt_bech32m.parse()
        } else if !value.alt_base_denom.is_empty() {
            Ok(Self::from_raw_denom(&value.alt_base_denom))
        } else {
            Err(anyhow::anyhow!(
                "AssetId proto has neither inner nor alt_bech32m nor alt_base_denom fields set"
            ))
        }
    }
}

impl DomainType for Id {
    type Proto = pb::AssetId;
}

impl TryFrom<&[u8]> for Id {
    type Error = anyhow::Error;

    fn try_from(slice: &[u8]) -> Result<Id, Self::Error> {
        Ok(Id(Fq::deserialize_compressed(slice)?))
    }
}

impl TryFrom<[u8; 32]> for Id {
    type Error = anyhow::Error;

    fn try_from(bytes: [u8; 32]) -> Result<Id, Self::Error> {
        Ok(Id(Fq::from_bytes_checked(&bytes).expect("convert to bytes")))
    }
}

impl std::fmt::Debug for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&bech32str::encode(
            &self.0.to_bytes(),
            bech32str::asset_id::BECH32_PREFIX,
            bech32str::Bech32m,
        ))
    }
}

impl std::fmt::Display for Id {
    // IMPORTANT: Changing this is state-breaking.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&bech32str::encode(
            &self.0.to_bytes(),
            bech32str::asset_id::BECH32_PREFIX,
            bech32str::Bech32m,
        ))
    }
}

impl std::str::FromStr for Id {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = bech32str::decode(s, bech32str::asset_id::BECH32_PREFIX, bech32str::Bech32m)?;
        pb::AssetId {
            inner,
            alt_bech32m: String::new(),
            alt_base_denom: String::new(),
        }
        .try_into()
    }
}

impl ToConstraintField<Fq> for Id {
    fn to_field_elements(&self) -> Option<Vec<Fq>> {
        let mut elements = Vec::new();
        elements.extend_from_slice(&[self.0]);
        Some(elements)
    }
}

/// The domain separator used to hash asset ids to value generators.
pub static VALUE_GENERATOR_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.value.generator").as_bytes())
});

impl Id {
    /// Compute the value generator for this asset, used for computing balance commitments.
    pub fn value_generator(&self) -> decaf377::Element {
        decaf377::Element::encode_to_curve(&poseidon377::hash_1(
            &VALUE_GENERATOR_DOMAIN_SEP,
            self.0,
        ))
    }

    /// Convert the asset ID to bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Create a value of this denomination.
    pub fn value(&self, amount: Amount) -> Value {
        Value {
            amount,
            asset_id: *self,
        }
    }

    pub(super) fn from_raw_denom(base_denom: &str) -> Self {
        Id(Fq::from_le_bytes_mod_order(
            // XXX choice of hash function?
            blake2b_simd::Params::default()
                .personal(b"Shieldd_AssetID")
                .hash(base_denom.as_bytes())
                .as_bytes(),
        ))
    }

    /// Returns the base64 encoded string of the inner bytes.
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.to_bytes())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use hex;
    use std::str::FromStr;

    #[test]
    fn asset_id_encoding() {
        let id = Id::from_raw_denom("ushieldd");

        let bech32m_id = format!("{id}");

        let id2 = Id::from_str(&bech32m_id).expect("can decode valid asset id");

        use shieldd_sdk_proto::Message;

        let proto = id.encode_to_vec();
        let proto2 = pb::AssetId {
            alt_bech32m: bech32m_id,
            ..Default::default()
        }
        .encode_to_vec();
        let proto3 = pb::AssetId {
            alt_base_denom: "ushieldd".to_owned(),
            ..Default::default()
        }
        .encode_to_vec();

        let id3 = Id::decode(proto.as_ref()).expect("can decode valid asset id");
        let id4 = Id::decode(proto2.as_ref()).expect("can decode valid asset id");
        let id5 = Id::decode(proto3.as_ref()).expect("can decode valid asset id");

        assert_eq!(id2, id);
        assert_eq!(id3, id);
        assert_eq!(id4, id);
        assert_eq!(id5, id);

        let hex_strings = [
            "cc0d3c9eef0c7ff4e225eca85a3094603691d289aeaf428ab0d87319ad93a302", // USDY
            "a7a339f42e671b2db1de226d4483d3e63036661cad1554d75f5f76fe04ec1e00", // SHITMOS
            "29ea9c2f3371f6a487e7e95c247041f4a356f983eb064e5d2b3bcf322ca96a10", // UM
            "76b3e4b10681358c123b381f90638476b7789040e47802de879f0fb3eedc8d0b", // USDC
            "2923a0a87b3a2421f165cc853dbf73a9bdafb5da0d948564b6059cb0217c4407", // OSMO
            "07ef660132a4c3235fab272d43d9b9752a8337b2d108597abffaff5f246d0f0f", // ATOM
            "5314b33eecfd5ca2e99c0b6d1e0ccafe3d2dd581c952d814fb64fdf51f85c411", // TIA
            "516108d0d0bba3f76e1f982d0a7cde118833307b03c0cd4ccb94e882b53c1f0f", // WBTC
            "414e723f74bd987c02ccbc997585ed52b196e2ffe75b3793aa68cc2996626910", // allBTC
            "bf8b035dda339b6cda8f221e79773b0fd871f27a472920f84c4aa2b4f98a700d", // allUSDT
        ];

        for hex in hex_strings {
            let bytes = hex::decode(hex).expect("valid hex string");
            let bytes_array: [u8; 32] = bytes.try_into().expect("hex is 32 bytes");

            let id = Id::try_from(bytes_array).expect("valid asset ID bytes");
            let bech32_str = id.to_string();

            let id_decoded = Id::from_str(&bech32_str).expect("valid bech32 string");
            assert_eq!(id, id_decoded);
        }
    }

    /// Backs ZK-ASSUME-ICS20-SUPPLY-CONSERVATION: the denom-trace -> asset-id
    /// derivation must be injective, or two distinct source denoms would alias
    /// one shielded asset id and share a single `ics20_value_balance` counter,
    /// breaking per-denom supply accounting. `from_raw_denom` is a pure function
    /// of the exact (prefixed) denom string, so distinct multi-hop traces must
    /// yield distinct ids and a prefixed trace must never collide with its base.
    #[test]
    fn denom_trace_to_asset_id_is_injective() {
        // Representative corpus: bases, single-hop and multi-hop ICS-20 traces,
        // and adversarial near-collisions (channel-index boundary, base that
        // itself contains path separators).
        let denoms = [
            "uatom",
            "uosmo",
            "transfer/channel-0/uatom",
            "transfer/channel-1/uatom",
            "transfer/channel-10/uatom",
            "transfer/channel-0/uosmo",
            "transfer/channel-0/transfer/channel-1/uatom",
            "transfer/channel-1/transfer/channel-0/uatom",
            "transfer/channel-0/uatom-with-suffix",
        ];

        let mut seen: std::collections::HashMap<[u8; 32], &str> = std::collections::HashMap::new();
        for denom in denoms {
            // Determinism: the derivation is a pure function of the string.
            assert_eq!(
                Id::from_raw_denom(denom).to_bytes(),
                Id::from_raw_denom(denom).to_bytes(),
                "asset-id derivation must be deterministic for {denom}"
            );
            let id = Id::from_raw_denom(denom).to_bytes();
            if let Some(prev) = seen.insert(id, denom) {
                panic!("denom-trace asset-id collision: {prev:?} and {denom:?} map to the same id");
            }
        }

        // A prefixed trace must not alias its inner base denom.
        assert_ne!(
            Id::from_raw_denom("uatom").to_bytes(),
            Id::from_raw_denom("transfer/channel-0/uatom").to_bytes(),
            "prefixed trace must derive a different asset id than its base denom"
        );
    }
}
