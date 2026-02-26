use decaf377::{Fq, Fr};
use once_cell::sync::Lazy;
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::penumbra::core::component::compliance::v1 as pb;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_tct::StateCommitment;
use serde::{Deserialize, Serialize};

/// Compliance plaintext layout constants.
/// These define the byte sizes for each field in the compliance plaintext that gets encrypted.
/// The circuit's bit-packing logic MUST match these exact sizes.
pub const AMOUNT_BYTES: usize = 16; // u128 = 16 bytes = 128 bits
pub const ASSET_ID_BYTES: usize = 32; // Fq field element = 32 bytes = 256 bits
pub const GENERATOR_BYTES: usize = 32; // Compressed curve point = 32 bytes = 256 bits
pub const KEY_BYTES: usize = 32; // Compressed curve point = 32 bytes = 256 bits
pub const ADDRESS_BYTES: usize = GENERATOR_BYTES + KEY_BYTES; // One address = 64 bytes
pub const TOTAL_PLAINTEXT_BYTES: usize =
    AMOUNT_BYTES + ASSET_ID_BYTES + ADDRESS_BYTES + ADDRESS_BYTES; // 176 bytes (self + counterparty)

/// Compliance ciphertext wire format constants.
///
/// **Spend format (192 bytes):** EPK_1(32) + c2_core(32) + detection(32) + core(96)
///
/// **Output format (512 bytes):** EPK_1(32) + EPK_2(32) + EPK_3(32)
///   + c2_core(32) + c2_ext(32) + c2_sext(32) + detection(32) + core(96) + ext(96) + sext(96)
pub const EPK_BYTES: usize = 32;
pub const C2_BYTES: usize = 32;
pub const DETECTION_TAG_BYTES: usize = 64; // 2 Fq elements: asset_id+flag, salt
pub const ENCRYPTED_TIER_BYTES: usize = 96; // 3 Fq elements per tier

/// Spend ciphertext: 1 EPK + 1 c2 + detection + core.
pub const SPEND_WIRE_BYTES: usize =
    EPK_BYTES + C2_BYTES + DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES; // 224 bytes
pub const SPEND_CIPHERTEXT_FQS: usize = (DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES) / 32; // 5

/// Output ciphertext: 3 EPKs + 3 c2s + detection + 3 tiers.
pub const OUTPUT_WIRE_BYTES: usize =
    EPK_BYTES * 3 + C2_BYTES * 3 + DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES * 3; // 544 bytes
pub const OUTPUT_CIPHERTEXT_FQS: usize = (DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES * 3) / 32; // 11

// Compile-time consistency checks.
const _: () = {
    assert!(SPEND_WIRE_BYTES == 224, "SPEND_WIRE_BYTES must be 224");
    assert!(OUTPUT_WIRE_BYTES == 544, "OUTPUT_WIRE_BYTES must be 544");
    assert!(SPEND_CIPHERTEXT_FQS == 5, "SPEND_CIPHERTEXT_FQS must be 5");
    assert!(
        OUTPUT_CIPHERTEXT_FQS == 11,
        "OUTPUT_CIPHERTEXT_FQS must be 11"
    );
};

/// A single DLEQ proof: (challenge, response).
///
/// Proves EPK = r×G and S = r×ACK use the same r, bound to metadata M.
/// Challenge c is the truncated Poseidon output (high bits zeroed via `fq_to_challenge_scalar`).
/// Stored as Fq for circuit compatibility; high 4 bits of byte 31 are always zero.
#[derive(Clone, Debug)]
pub struct DleqProof {
    pub c: Fq, // Fiat-Shamir challenge (truncated, high bits zero)
    pub s: Fr, // Response: k + c_truncated × r
}

impl DleqProof {
    /// Serialize to 64 bytes: c (32 LE) || s (32 LE).
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&self.c.to_bytes());
        bytes[32..].copy_from_slice(&self.s.to_bytes());
        bytes
    }

    /// Deserialize from 64 bytes.
    pub fn from_bytes(bytes: &[u8; 64]) -> Self {
        let c = Fq::from_le_bytes_mod_order(&bytes[..32]);
        let s = Fr::from_le_bytes_mod_order(&bytes[32..]);
        Self { c, s }
    }
}

/// The domain separator used to generate compliance leaf commitments.
pub(crate) static COMPLIANCE_LEAF_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"penumbra.compliance.leaf").as_bytes())
});

/// A compliance leaf in the public on-chain registry for regulated assets.
///
/// Contains address, asset_id, and derivation scalar `d`.
/// `d = SHA256("elgamal-derivation-v1\0\0" || b_d_fq_bytes)` — matches Orbis derivation.
/// ACK = d × ring_pk, computed in-circuit from the leaf's `d` value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::ComplianceLeaf", into = "pb::ComplianceLeaf")]
pub struct ComplianceLeaf {
    /// The registered address for compliance.
    pub address: Address,
    /// The asset ID this compliance leaf applies to.
    pub asset_id: asset::Id,
    /// Derivation scalar: d = SHA256_derive(b_d_fq). Verified at registration.
    pub d: Fq,
}

impl ComplianceLeaf {
    /// Create a ComplianceLeaf.
    pub fn new(address: Address, asset_id: asset::Id, d: Fq) -> Self {
        Self {
            address,
            asset_id,
            d,
        }
    }

    /// Create the Poseidon commitment: hash_4(domain_sep, (g_d, pk_d, asset_id, d)).
    pub fn commit(&self) -> StateCommitment {
        let diversified_generator = self
            .address
            .diversified_generator()
            .vartime_compress_to_field();
        let transmission_key_s = Fq::from_bytes_checked(&self.address.transmission_key().0)
            .expect("transmission key is valid");
        let asset_id_field = self.asset_id.0;

        let commit = poseidon377::hash_4(
            &COMPLIANCE_LEAF_DOMAIN_SEP,
            (
                diversified_generator,
                transmission_key_s,
                asset_id_field,
                self.d,
            ),
        );

        StateCommitment(commit)
    }

    /// Export to JSON for off-chain sharing.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Import from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl DomainType for ComplianceLeaf {
    type Proto = pb::ComplianceLeaf;
}

impl TryFrom<pb::ComplianceLeaf> for ComplianceLeaf {
    type Error = anyhow::Error;

    fn try_from(value: pb::ComplianceLeaf) -> Result<Self, Self::Error> {
        let d = if value.d.is_empty() {
            Fq::from(0u64)
        } else {
            let bytes: [u8; 32] = value
                .d
                .try_into()
                .map_err(|_| anyhow::anyhow!("d must be 32 bytes"))?;
            Fq::from_bytes_checked(&bytes)
                .map_err(|_| anyhow::anyhow!("invalid d field element"))?
        };
        Ok(ComplianceLeaf {
            address: value
                .address
                .ok_or_else(|| anyhow::anyhow!("missing address"))?
                .try_into()?,
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            d,
        })
    }
}

impl From<ComplianceLeaf> for pb::ComplianceLeaf {
    fn from(value: ComplianceLeaf) -> pb::ComplianceLeaf {
        pb::ComplianceLeaf {
            address: Some(value.address.into()),
            asset_id: Some(value.asset_id.into()),
            d: value.d.to_bytes().to_vec(),
        }
    }
}

/// Per-asset issuer parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetParams {
    /// Issuer's detection key public (curve point).
    pub dk_pub: decaf377::Element,
    /// Amount threshold for flagging (u128 to cover full amount range).
    pub threshold: u128,
    /// IBC channels allowed for this asset. Empty = IBC blocked entirely.
    pub allowed_channels: Vec<String>,
}

/// Orbis ring binding data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingData {
    /// Orbis DKG ring identifier.
    pub ring_id: String,
    /// Aggregate ring public key (sk_ring × G).
    pub ring_pk: decaf377::Element,
    /// SourceHub policy ID.
    pub policy_id: String,
    /// ACP permission name.
    pub permission: String,
    /// ACP resource type.
    pub resource: String,
}

/// Asset-specific compliance policy stored on-chain.
///
/// Contains issuer parameters (detection key, threshold, channel whitelist)
/// and Orbis ring binding (ring_pk, policy identifiers).
/// This is state-only data — NOT included in the IMT Merkle commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetPolicy {
    pub params: AssetParams,
    pub ring: RingData,
}

impl AssetPolicy {
    /// Create a new asset policy.
    pub fn new(
        dk_pub: decaf377::Element,
        threshold: u128,
        allowed_channels: Vec<String>,
        ring_id: String,
        ring_pk: decaf377::Element,
        policy_id: String,
        permission: String,
        resource: String,
    ) -> Self {
        Self {
            params: AssetParams {
                dk_pub,
                threshold,
                allowed_channels,
            },
            ring: RingData {
                ring_id,
                ring_pk,
                policy_id,
                permission,
                resource,
            },
        }
    }

    /// Create a simple policy with just dk_pub, threshold, and ring_pk.
    /// Uses empty strings for ring_id, policy_id, permission, resource.
    pub fn simple(dk_pub: decaf377::Element, threshold: u128, ring_pk: decaf377::Element) -> Self {
        Self::new(
            dk_pub,
            threshold,
            vec![],
            String::new(),
            ring_pk,
            String::new(),
            String::new(),
            String::new(),
        )
    }

    /// Create a default policy for unregulated assets.
    ///
    /// Uses identity element for dk_pub/ring_pk and u128::MAX for threshold.
    pub fn default_unregulated() -> Self {
        Self {
            params: AssetParams {
                dk_pub: decaf377::Element::default(),
                threshold: u128::MAX,
                allowed_channels: vec![],
            },
            ring: RingData {
                ring_id: String::new(),
                ring_pk: decaf377::Element::default(),
                policy_id: String::new(),
                permission: String::new(),
                resource: String::new(),
            },
        }
    }

    /// Convenience accessors for backwards compatibility.
    pub fn dk_pub(&self) -> &decaf377::Element {
        &self.params.dk_pub
    }

    pub fn threshold(&self) -> u128 {
        self.params.threshold
    }

    pub fn ring_pk(&self) -> &decaf377::Element {
        &self.ring.ring_pk
    }

    /// Serialize to bytes for storage.
    ///
    /// Format: [dk_pub: 32] [threshold: 16] [ring_pk: 32]
    ///         [channel_count: 2] [for each: len: 1, utf8 bytes]
    ///         [ring_id_len: 2] [ring_id bytes]
    ///         [policy_id_len: 2] [policy_id bytes]
    ///         [permission_len: 2] [permission bytes]
    ///         [resource_len: 2] [resource bytes]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128);
        // AssetParams
        bytes.extend_from_slice(&self.params.dk_pub.vartime_compress().0);
        bytes.extend_from_slice(&self.params.threshold.to_le_bytes());
        // RingData - ring_pk
        bytes.extend_from_slice(&self.ring.ring_pk.vartime_compress().0);
        // Channels
        let count = self.params.allowed_channels.len() as u16;
        bytes.extend_from_slice(&count.to_le_bytes());
        for channel in &self.params.allowed_channels {
            let len = channel.len() as u8;
            bytes.push(len);
            bytes.extend_from_slice(channel.as_bytes());
        }
        // String fields
        fn write_string(bytes: &mut Vec<u8>, s: &str) {
            let len = s.len() as u16;
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(s.as_bytes());
        }
        write_string(&mut bytes, &self.ring.ring_id);
        write_string(&mut bytes, &self.ring.policy_id);
        write_string(&mut bytes, &self.ring.permission);
        write_string(&mut bytes, &self.ring.resource);
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 80 {
            // 32 (dk_pub) + 16 (threshold) + 32 (ring_pk)
            anyhow::bail!(
                "invalid AssetPolicy length: expected >= 80 bytes, got {}",
                bytes.len()
            );
        }
        let dk_pub_bytes: [u8; 32] = bytes[0..32].try_into()?;
        let dk_pub = decaf377::Encoding(dk_pub_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?;
        let threshold = u128::from_le_bytes(bytes[32..48].try_into()?);
        let ring_pk_bytes: [u8; 32] = bytes[48..80].try_into()?;
        let ring_pk = decaf377::Encoding(ring_pk_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?;

        let mut offset = 80;

        // Parse channels
        let allowed_channels = if offset + 2 <= bytes.len() {
            let count = u16::from_le_bytes(bytes[offset..offset + 2].try_into()?) as usize;
            offset += 2;
            let mut channels = Vec::with_capacity(count);
            for _ in 0..count {
                if offset >= bytes.len() {
                    anyhow::bail!("truncated allowed_channels data");
                }
                let len = bytes[offset] as usize;
                offset += 1;
                if offset + len > bytes.len() {
                    anyhow::bail!("truncated channel string");
                }
                let channel = std::str::from_utf8(&bytes[offset..offset + len])
                    .map_err(|_| anyhow::anyhow!("invalid UTF-8 in channel name"))?;
                channels.push(channel.to_string());
                offset += len;
            }
            channels
        } else {
            vec![]
        };

        // Parse string fields
        fn read_string(bytes: &[u8], offset: &mut usize) -> anyhow::Result<String> {
            if *offset + 2 > bytes.len() {
                return Ok(String::new());
            }
            let len = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into()?) as usize;
            *offset += 2;
            if *offset + len > bytes.len() {
                anyhow::bail!("truncated string field");
            }
            let s = std::str::from_utf8(&bytes[*offset..*offset + len])
                .map_err(|_| anyhow::anyhow!("invalid UTF-8 in string field"))?;
            *offset += len;
            Ok(s.to_string())
        }

        let ring_id = read_string(bytes, &mut offset)?;
        let policy_id = read_string(bytes, &mut offset)?;
        let permission = read_string(bytes, &mut offset)?;
        let resource = read_string(bytes, &mut offset)?;

        Ok(Self {
            params: AssetParams {
                dk_pub,
                threshold,
                allowed_channels,
            },
            ring: RingData {
                ring_id,
                ring_pk,
                policy_id,
                permission,
                resource,
            },
        })
    }
}

// Proto conversion for AssetPolicy
impl DomainType for AssetPolicy {
    type Proto = pb::AssetPolicy;
}

impl TryFrom<pb::AssetPolicy> for AssetPolicy {
    type Error = anyhow::Error;

    fn try_from(value: pb::AssetPolicy) -> Result<Self, Self::Error> {
        let dk_pub = if value.dk_pub.is_empty() {
            decaf377::Element::default()
        } else {
            let bytes: [u8; 32] = value
                .dk_pub
                .try_into()
                .map_err(|_| anyhow::anyhow!("dk_pub must be 32 bytes"))?;
            decaf377::Encoding(bytes)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?
        };

        let threshold = if value.threshold.is_empty() {
            u128::MAX
        } else {
            let bytes: [u8; 16] = value.threshold.try_into().map_err(|v: Vec<u8>| {
                anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len())
            })?;
            u128::from_le_bytes(bytes)
        };

        let ring_pk = if value.ring_pk.is_empty() {
            decaf377::Element::default()
        } else {
            let bytes: [u8; 32] = value
                .ring_pk
                .try_into()
                .map_err(|_| anyhow::anyhow!("ring_pk must be 32 bytes"))?;
            decaf377::Encoding(bytes)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?
        };

        Ok(AssetPolicy {
            params: AssetParams {
                dk_pub,
                threshold,
                allowed_channels: value.allowed_channels,
            },
            ring: RingData {
                ring_id: value.ring_id,
                ring_pk,
                policy_id: value.policy_id,
                permission: value.permission,
                resource: value.resource,
            },
        })
    }
}

impl From<AssetPolicy> for pb::AssetPolicy {
    fn from(value: AssetPolicy) -> pb::AssetPolicy {
        pb::AssetPolicy {
            dk_pub: value.params.dk_pub.vartime_compress().0.to_vec(),
            threshold: value.params.threshold.to_le_bytes().to_vec(),
            allowed_channels: value.params.allowed_channels,
            ring_id: value.ring.ring_id,
            ring_pk: value.ring.ring_pk.vartime_compress().0.to_vec(),
            policy_id: value.ring.policy_id,
            permission: value.ring.permission,
            resource: value.ring.resource,
        }
    }
}

/// Message to register an asset as regulated or non-regulated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::MsgRegisterAsset", into = "pb::MsgRegisterAsset")]
pub struct MsgRegisterAsset {
    /// The asset ID to register.
    pub asset_id: asset::Id,
    /// Whether this asset is regulated (requires compliance).
    pub is_regulated: bool,
    /// Issuer's detection key public (optional).
    pub dk_pub: Option<decaf377::Element>,
    /// Amount threshold for flagging (optional).
    pub threshold: Option<u128>,
    /// IBC channels allowed for this regulated asset. Empty = IBC blocked.
    pub allowed_channels: Vec<String>,
    /// Orbis ring public key (optional).
    pub ring_pk: Option<decaf377::Element>,
    /// Orbis DKG ring identifier.
    pub ring_id: String,
    /// SourceHub policy ID.
    pub policy_id: String,
    /// ACP permission name.
    pub permission: String,
    /// ACP resource type.
    pub resource: String,
}

impl DomainType for MsgRegisterAsset {
    type Proto = pb::MsgRegisterAsset;
}

impl TryFrom<pb::MsgRegisterAsset> for MsgRegisterAsset {
    type Error = anyhow::Error;

    fn try_from(value: pb::MsgRegisterAsset) -> Result<Self, Self::Error> {
        let dk_pub = if value.dk_pub.is_empty() {
            None
        } else {
            let bytes: [u8; 32] = value
                .dk_pub
                .try_into()
                .map_err(|_| anyhow::anyhow!("dk_pub must be exactly 32 bytes"))?;
            Some(
                decaf377::Encoding(bytes)
                    .vartime_decompress()
                    .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?,
            )
        };

        let threshold = if value.threshold.is_empty() {
            None
        } else {
            let threshold_bytes: [u8; 16] = value.threshold.try_into().map_err(|v: Vec<u8>| {
                anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len())
            })?;
            Some(u128::from_le_bytes(threshold_bytes))
        };

        let ring_pk = if value.ring_pk.is_empty() {
            None
        } else {
            let bytes: [u8; 32] = value
                .ring_pk
                .try_into()
                .map_err(|_| anyhow::anyhow!("ring_pk must be exactly 32 bytes"))?;
            Some(
                decaf377::Encoding(bytes)
                    .vartime_decompress()
                    .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?,
            )
        };

        Ok(MsgRegisterAsset {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            is_regulated: value.is_regulated,
            dk_pub,
            threshold,
            allowed_channels: value.allowed_channels,
            ring_pk,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
        })
    }
}

impl From<MsgRegisterAsset> for pb::MsgRegisterAsset {
    fn from(value: MsgRegisterAsset) -> pb::MsgRegisterAsset {
        pb::MsgRegisterAsset {
            asset_id: Some(value.asset_id.into()),
            is_regulated: value.is_regulated,
            dk_pub: value
                .dk_pub
                .map(|e| e.vartime_compress().0.to_vec())
                .unwrap_or_default(),
            threshold: value
                .threshold
                .map(|t| t.to_le_bytes().to_vec())
                .unwrap_or_default(),
            allowed_channels: value.allowed_channels,
            ring_pk: value
                .ring_pk
                .map(|e| e.vartime_compress().0.to_vec())
                .unwrap_or_default(),
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
        }
    }
}

impl penumbra_sdk_txhash::EffectingData for MsgRegisterAsset {
    fn effect_hash(&self) -> penumbra_sdk_txhash::EffectHash {
        penumbra_sdk_txhash::EffectHash::from_proto_effecting_data::<pb::MsgRegisterAsset>(
            &self.clone().into(),
        )
    }
}

/// Message to register a user's address for a regulated asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::MsgRegisterUser", into = "pb::MsgRegisterUser")]
pub struct MsgRegisterUser {
    /// The compliance leaf containing the user's registration information.
    pub leaf: ComplianceLeaf,
    /// Signature authorizing this registration.
    pub signature: Vec<u8>,
}

impl DomainType for MsgRegisterUser {
    type Proto = pb::MsgRegisterUser;
}

impl TryFrom<pb::MsgRegisterUser> for MsgRegisterUser {
    type Error = anyhow::Error;

    fn try_from(value: pb::MsgRegisterUser) -> Result<Self, Self::Error> {
        Ok(MsgRegisterUser {
            leaf: value
                .leaf
                .ok_or_else(|| anyhow::anyhow!("missing leaf"))?
                .try_into()?,
            signature: value.signature,
        })
    }
}

impl From<MsgRegisterUser> for pb::MsgRegisterUser {
    fn from(value: MsgRegisterUser) -> pb::MsgRegisterUser {
        pb::MsgRegisterUser {
            leaf: Some(value.leaf.into()),
            signature: value.signature,
        }
    }
}

impl penumbra_sdk_txhash::EffectingData for MsgRegisterUser {
    fn effect_hash(&self) -> penumbra_sdk_txhash::EffectHash {
        penumbra_sdk_txhash::EffectHash::from_proto_effecting_data::<pb::MsgRegisterUser>(
            &self.clone().into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compliance_leaf_new() {
        let mut rng = rand::thread_rng();
        let address = Address::dummy(&mut rng);
        let asset_id = asset::Id(decaf377::Fq::from(100u64));
        let d = decaf377::Fq::from(42u64);

        let leaf = ComplianceLeaf::new(address.clone(), asset_id, d);

        assert_eq!(leaf.address, address);
        assert_eq!(leaf.asset_id, asset_id);
        assert_eq!(leaf.d, d);
    }

    #[test]
    fn test_compliance_leaf_different_addresses_different_commits() {
        let mut rng = rand::thread_rng();
        let asset_id = asset::Id(decaf377::Fq::from(100u64));
        let d = decaf377::Fq::from(42u64);

        let address1 = Address::dummy(&mut rng);
        let address2 = Address::dummy(&mut rng);

        let leaf1 = ComplianceLeaf::new(address1, asset_id, d);
        let leaf2 = ComplianceLeaf::new(address2, asset_id, d);

        assert_ne!(
            leaf1.commit(),
            leaf2.commit(),
            "Different addresses must have different commitments"
        );
    }

    #[test]
    fn test_compliance_leaf_proto_roundtrip() {
        let mut rng = rand::thread_rng();
        let wallet = Address::dummy(&mut rng);
        let asset_id = asset::Id(decaf377::Fq::from(999u64));
        let d = decaf377::Fq::from(123u64);

        let original = ComplianceLeaf::new(wallet, asset_id, d);

        let proto: pb::ComplianceLeaf = original.clone().into();
        let recovered: ComplianceLeaf = proto.try_into().expect("should parse");

        assert_eq!(original.address, recovered.address);
        assert_eq!(original.asset_id, recovered.asset_id);
        assert_eq!(original.d, recovered.d);
        assert_eq!(original.commit().0, recovered.commit().0);
    }

    #[test]
    fn test_asset_policy_bytes_roundtrip() {
        let dk = decaf377::Fr::from(42u64);
        let dk_pub = decaf377::Element::GENERATOR * dk;
        let rk = decaf377::Fr::from(999u64);
        let ring_pk = decaf377::Element::GENERATOR * rk;

        let policy = AssetPolicy::new(
            dk_pub,
            1000,
            vec!["channel-0".to_string()],
            "ring-123".to_string(),
            ring_pk,
            "policy-abc".to_string(),
            "reader".to_string(),
            "document".to_string(),
        );

        let bytes = policy.to_bytes();
        let recovered = AssetPolicy::from_bytes(&bytes).unwrap();

        assert_eq!(policy.params.dk_pub, recovered.params.dk_pub);
        assert_eq!(policy.params.threshold, recovered.params.threshold);
        assert_eq!(
            policy.params.allowed_channels,
            recovered.params.allowed_channels
        );
        assert_eq!(policy.ring.ring_id, recovered.ring.ring_id);
        assert_eq!(policy.ring.ring_pk, recovered.ring.ring_pk);
        assert_eq!(policy.ring.policy_id, recovered.ring.policy_id);
        assert_eq!(policy.ring.permission, recovered.ring.permission);
        assert_eq!(policy.ring.resource, recovered.ring.resource);
    }

    #[test]
    fn test_asset_policy_proto_roundtrip() {
        let dk_pub = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);

        let policy = AssetPolicy::new(
            dk_pub,
            500,
            vec!["ch-1".to_string(), "ch-2".to_string()],
            "ring-id".to_string(),
            ring_pk,
            "pol-id".to_string(),
            "perm".to_string(),
            "res".to_string(),
        );

        let proto: pb::AssetPolicy = policy.clone().into();
        let recovered = AssetPolicy::try_from(proto).unwrap();

        assert_eq!(policy, recovered);
    }
}

/// A Merkle path in the Quad Merkle Tree (arity 4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(try_from = "pb::MerklePath", into = "pb::MerklePath")]
pub struct MerklePath {
    /// The layers of the Merkle path, from leaf to root.
    pub layers: Vec<MerklePathLayer>,
}

impl MerklePath {
    /// Create a MerklePath from the output of registry auth_path functions.
    pub fn from_auth_path(auth_path: Vec<[StateCommitment; 3]>) -> Self {
        let layers = auth_path
            .into_iter()
            .map(|siblings_array| {
                let siblings = siblings_array
                    .iter()
                    .map(|commitment| commitment.0.to_bytes().to_vec())
                    .collect();
                MerklePathLayer { siblings }
            })
            .collect();
        MerklePath { layers }
    }
}

impl From<Vec<[StateCommitment; 3]>> for MerklePath {
    fn from(auth_path: Vec<[StateCommitment; 3]>) -> Self {
        Self::from_auth_path(auth_path)
    }
}

impl DomainType for MerklePath {
    type Proto = pb::MerklePath;
}

impl TryFrom<pb::MerklePath> for MerklePath {
    type Error = anyhow::Error;

    fn try_from(value: pb::MerklePath) -> Result<Self, Self::Error> {
        let layers = value
            .layers
            .into_iter()
            .map(|l| l.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(MerklePath { layers })
    }
}

impl From<MerklePath> for pb::MerklePath {
    fn from(value: MerklePath) -> pb::MerklePath {
        pb::MerklePath {
            layers: value.layers.into_iter().map(|l| l.into()).collect(),
        }
    }
}

/// A single layer in the Quad Merkle Tree path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::MerklePathLayer", into = "pb::MerklePathLayer")]
pub struct MerklePathLayer {
    pub siblings: Vec<Vec<u8>>,
}

impl DomainType for MerklePathLayer {
    type Proto = pb::MerklePathLayer;
}

impl TryFrom<pb::MerklePathLayer> for MerklePathLayer {
    type Error = anyhow::Error;

    fn try_from(value: pb::MerklePathLayer) -> Result<Self, Self::Error> {
        Ok(MerklePathLayer {
            siblings: value.siblings,
        })
    }
}

impl From<MerklePathLayer> for pb::MerklePathLayer {
    fn from(value: MerklePathLayer) -> pb::MerklePathLayer {
        pb::MerklePathLayer {
            siblings: value.siblings,
        }
    }
}

/// Compliance ciphertext with tiered encryption.
///
/// Supports two formats:
/// - **Spend** (192 bytes): 1 EPK + c2_core + detection + core
/// - **Output** (512 bytes): 3 EPKs + 3 c2s + detection + core + ext + sext
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplianceCiphertext {
    /// Ephemeral public key EPK_1 = r_1 × G (all actions).
    pub epk_1: decaf377::Element,

    /// Ephemeral public key EPK_2 = r_2 × G (Output only).
    pub epk_2: Option<decaf377::Element>,

    /// Ephemeral public key EPK_3 = r_3 × G (Output only).
    pub epk_3: Option<decaf377::Element>,

    /// Encrypted seed for core tier (ElGamal envelope).
    pub c2_core: Fq,

    /// Encrypted seed for extension tier (Output only).
    pub c2_ext: Option<Fq>,

    /// Encrypted seed for sender-extension tier (Output only).
    pub c2_sext: Option<Fq>,

    /// Encrypted detection tier: [asset_id+flag (32 bytes), salt (32 bytes)].
    pub detection_tag: [u8; DETECTION_TAG_BYTES],

    /// Encrypted core data: amount + self address (96 bytes).
    pub encrypted_core: Vec<u8>,

    /// Encrypted extension: counterparty address for receiver (96 bytes, Output only).
    pub encrypted_ext: Option<Vec<u8>>,

    /// Encrypted sender-extension: counterparty data for sender (96 bytes, Output only).
    pub encrypted_sext: Option<Vec<u8>>,
}

impl ComplianceCiphertext {
    /// Serialize EPK_1 to bytes.
    pub fn epk_1_bytes(&self) -> [u8; 32] {
        self.epk_1.vartime_compress().0
    }

    /// Create a Spend ciphertext (detection + core only, 224 bytes).
    pub fn new_spend(
        epk_1: decaf377::Element,
        c2_core: Fq,
        detection_tag: [u8; DETECTION_TAG_BYTES],
        encrypted_core: Vec<u8>,
    ) -> Self {
        Self {
            epk_1,
            epk_2: None,
            epk_3: None,
            c2_core,
            c2_ext: None,
            c2_sext: None,
            detection_tag,
            encrypted_core,
            encrypted_ext: None,
            encrypted_sext: None,
        }
    }

    /// Create an Output ciphertext (all tiers, 544 bytes).
    pub fn new_output(
        epk_1: decaf377::Element,
        epk_2: decaf377::Element,
        epk_3: decaf377::Element,
        c2_core: Fq,
        c2_ext: Fq,
        c2_sext: Fq,
        detection_tag: [u8; DETECTION_TAG_BYTES],
        encrypted_core: Vec<u8>,
        encrypted_ext: Vec<u8>,
        encrypted_sext: Vec<u8>,
    ) -> Self {
        Self {
            epk_1,
            epk_2: Some(epk_2),
            epk_3: Some(epk_3),
            c2_core,
            c2_ext: Some(c2_ext),
            c2_sext: Some(c2_sext),
            detection_tag,
            encrypted_core,
            encrypted_ext: Some(encrypted_ext),
            encrypted_sext: Some(encrypted_sext),
        }
    }

    /// Whether this is a Spend ciphertext (no extension tiers).
    pub fn is_spend(&self) -> bool {
        self.epk_2.is_none()
    }

    /// Serialize to bytes.
    ///
    /// Spend (192): EPK_1 + c2_core + detection + core
    /// Output (512): EPK_1 + EPK_2 + EPK_3 + c2_core + c2_ext + c2_sext + detection + core + ext + sext
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.epk_1_bytes());
        if let Some(epk_2) = &self.epk_2 {
            bytes.extend_from_slice(&epk_2.vartime_compress().0);
        }
        if let Some(epk_3) = &self.epk_3 {
            bytes.extend_from_slice(&epk_3.vartime_compress().0);
        }
        bytes.extend_from_slice(&self.c2_core.to_bytes());
        if let Some(c2_ext) = &self.c2_ext {
            bytes.extend_from_slice(&c2_ext.to_bytes());
        }
        if let Some(c2_sext) = &self.c2_sext {
            bytes.extend_from_slice(&c2_sext.to_bytes());
        }
        bytes.extend_from_slice(&self.detection_tag);
        bytes.extend_from_slice(&self.encrypted_core);
        if let Some(ext) = &self.encrypted_ext {
            bytes.extend_from_slice(ext);
        }
        if let Some(sext) = &self.encrypted_sext {
            bytes.extend_from_slice(sext);
        }
        bytes
    }

    /// Deserialize from bytes. Accepts Spend (192 bytes) or Output (512 bytes) format.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let is_output = match bytes.len() {
            SPEND_WIRE_BYTES => false,
            OUTPUT_WIRE_BYTES => true,
            n => anyhow::bail!(
                "invalid ciphertext length: expected {} (spend) or {} (output), got {}",
                SPEND_WIRE_BYTES,
                OUTPUT_WIRE_BYTES,
                n
            ),
        };

        let mut offset = 0;

        // EPK_1
        let epk_1_bytes: [u8; EPK_BYTES] = bytes[offset..offset + EPK_BYTES].try_into()?;
        let epk_1 = decaf377::Encoding(epk_1_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("failed to decompress epk_1"))?;
        offset += EPK_BYTES;

        // EPK_2 and EPK_3 (output only)
        let (epk_2, epk_3) = if is_output {
            let epk_2_bytes: [u8; EPK_BYTES] = bytes[offset..offset + EPK_BYTES].try_into()?;
            let epk_2 = decaf377::Encoding(epk_2_bytes)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("failed to decompress epk_2"))?;
            offset += EPK_BYTES;

            let epk_3_bytes: [u8; EPK_BYTES] = bytes[offset..offset + EPK_BYTES].try_into()?;
            let epk_3 = decaf377::Encoding(epk_3_bytes)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("failed to decompress epk_3"))?;
            offset += EPK_BYTES;

            (Some(epk_2), Some(epk_3))
        } else {
            (None, None)
        };

        // c2_core
        let c2_core_bytes: [u8; C2_BYTES] = bytes[offset..offset + C2_BYTES].try_into()?;
        let c2_core = Fq::from_bytes_checked(&c2_core_bytes)
            .map_err(|_| anyhow::anyhow!("invalid c2_core field element"))?;
        offset += C2_BYTES;

        // c2_ext and c2_sext (output only)
        let (c2_ext, c2_sext) = if is_output {
            let ext_bytes: [u8; C2_BYTES] = bytes[offset..offset + C2_BYTES].try_into()?;
            let c2_ext = Fq::from_bytes_checked(&ext_bytes)
                .map_err(|_| anyhow::anyhow!("invalid c2_ext field element"))?;
            offset += C2_BYTES;

            let sext_bytes: [u8; C2_BYTES] = bytes[offset..offset + C2_BYTES].try_into()?;
            let c2_sext = Fq::from_bytes_checked(&sext_bytes)
                .map_err(|_| anyhow::anyhow!("invalid c2_sext field element"))?;
            offset += C2_BYTES;

            (Some(c2_ext), Some(c2_sext))
        } else {
            (None, None)
        };

        let detection_tag: [u8; DETECTION_TAG_BYTES] =
            bytes[offset..offset + DETECTION_TAG_BYTES].try_into()?;
        offset += DETECTION_TAG_BYTES;

        let encrypted_core = bytes[offset..offset + ENCRYPTED_TIER_BYTES].to_vec();
        offset += ENCRYPTED_TIER_BYTES;

        let (encrypted_ext, encrypted_sext) = if is_output {
            let ext = bytes[offset..offset + ENCRYPTED_TIER_BYTES].to_vec();
            offset += ENCRYPTED_TIER_BYTES;
            let sext = bytes[offset..offset + ENCRYPTED_TIER_BYTES].to_vec();
            (Some(ext), Some(sext))
        } else {
            (None, None)
        };

        Ok(Self {
            epk_1,
            epk_2,
            epk_3,
            c2_core,
            c2_ext,
            c2_sext,
            detection_tag,
            encrypted_core,
            encrypted_ext,
            encrypted_sext,
        })
    }

    /// Convert to Output circuit public inputs (11 Fq).
    ///
    /// Returns `(epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, ciphertext_fqs)`
    /// where ciphertext_fqs = [detection:2][core:3][ext:3][sext:3] = 11 Fq.
    pub fn to_output_circuit_public_inputs(
        &self,
    ) -> (
        decaf377::Element,
        decaf377::Element,
        decaf377::Element,
        decaf377::Fq,
        decaf377::Fq,
        decaf377::Fq,
        Vec<decaf377::Fq>,
    ) {
        use decaf377::Fq;

        let epk_2 = self
            .epk_2
            .expect("to_output_circuit_public_inputs called on Spend ciphertext");
        let epk_3 = self
            .epk_3
            .expect("to_output_circuit_public_inputs called on Spend ciphertext");
        let c2_ext = self
            .c2_ext
            .expect("to_output_circuit_public_inputs called on Spend ciphertext");
        let c2_sext = self
            .c2_sext
            .expect("to_output_circuit_public_inputs called on Spend ciphertext");
        let encrypted_ext = self
            .encrypted_ext
            .as_ref()
            .expect("to_output_circuit_public_inputs called on Spend ciphertext");
        let encrypted_sext = self
            .encrypted_sext
            .as_ref()
            .expect("to_output_circuit_public_inputs called on Spend ciphertext");

        let payload_bytes = DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES * 3;
        let mut ciphertext_bytes = Vec::with_capacity(payload_bytes);
        ciphertext_bytes.extend_from_slice(&self.detection_tag);
        ciphertext_bytes.extend_from_slice(&self.encrypted_core);
        ciphertext_bytes.extend_from_slice(encrypted_ext);
        ciphertext_bytes.extend_from_slice(encrypted_sext);

        debug_assert_eq!(ciphertext_bytes.len(), payload_bytes);

        let ciphertext_fqs: Vec<Fq> = ciphertext_bytes
            .chunks_exact(32)
            .map(|chunk| {
                let buf: [u8; 32] = chunk.try_into().expect("chunk should be exactly 32 bytes");
                Fq::from_le_bytes_mod_order(&buf)
            })
            .collect();

        debug_assert_eq!(ciphertext_fqs.len(), OUTPUT_CIPHERTEXT_FQS);

        (
            self.epk_1,
            epk_2,
            epk_3,
            self.c2_core,
            c2_ext,
            c2_sext,
            ciphertext_fqs,
        )
    }

    /// Convert to Spend circuit public inputs (5 Fq).
    ///
    /// Returns `(epk_1, c2_core, ciphertext_fqs)` where ciphertext_fqs
    /// = [detection:2][core:3] = 5 Fq.
    pub fn to_spend_circuit_public_inputs(
        &self,
    ) -> (decaf377::Element, decaf377::Fq, Vec<decaf377::Fq>) {
        use decaf377::Fq;

        let mut ciphertext_bytes = Vec::with_capacity(128);
        ciphertext_bytes.extend_from_slice(&self.detection_tag);
        ciphertext_bytes.extend_from_slice(&self.encrypted_core);

        let ciphertext_fqs: Vec<Fq> = ciphertext_bytes
            .chunks_exact(32)
            .map(|chunk| {
                let buf: [u8; 32] = chunk.try_into().expect("chunk should be exactly 32 bytes");
                Fq::from_le_bytes_mod_order(&buf)
            })
            .collect();

        debug_assert_eq!(ciphertext_fqs.len(), SPEND_CIPHERTEXT_FQS);

        (self.epk_1, self.c2_core, ciphertext_fqs)
    }
}

/// Complete compliance payload containing both sender and receiver ciphertexts.
#[derive(Clone, Debug)]
pub struct CompliancePayload {
    pub sender_compliance: ComplianceCiphertext,
    pub receiver_compliance: ComplianceCiphertext,
}

impl CompliancePayload {
    pub fn new(
        sender_compliance: ComplianceCiphertext,
        receiver_compliance: ComplianceCiphertext,
    ) -> Self {
        Self {
            sender_compliance,
            receiver_compliance,
        }
    }
}
