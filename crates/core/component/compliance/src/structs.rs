use decaf377::{Fq, Fr};
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
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
/// **Transfer-input format (288 bytes):** EPK_1(32) + c2_core(32) + detection(128) + core(96)
///
/// **Transfer-output format (544 bytes):** EPK_1(32) + EPK_2(32) + EPK_3(32)
///   + c2_core(32) + c2_ext(32) + c2_sext(32) + detection(128) + core(96) + ext(96) + sext(96)
pub const EPK_BYTES: usize = 32;
pub const C2_BYTES: usize = 32;
pub const DETECTION_TAG_BYTES: usize = 128; // 4 Fq elements: asset_id+flag, salt, sender slot, receiver slot
pub const ENCRYPTED_TIER_BYTES: usize = 96; // 3 Fq elements per tier

/// Transfer-input ciphertext: 1 EPK + 1 c2 + detection + core.
pub const TRANSFER_INPUT_WIRE_BYTES: usize =
    EPK_BYTES + C2_BYTES + DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES; // 288 bytes
pub const TRANSFER_INPUT_CIPHERTEXT_FQS: usize = (DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES) / 32; // 7

/// Transfer-output ciphertext: 3 EPKs + 3 c2s + detection + 3 tiers.
pub const TRANSFER_OUTPUT_WIRE_BYTES: usize =
    EPK_BYTES * 3 + C2_BYTES * 3 + DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES * 3; // 608 bytes
pub const TRANSFER_OUTPUT_CIPHERTEXT_FQS: usize =
    (DETECTION_TAG_BYTES + ENCRYPTED_TIER_BYTES * 3) / 32; // 13

pub const DEFAULT_COMPLIANCE_SLOT_COUNT: u32 = 10;

const ASSET_REGISTRATION_GRANT_DOMAIN: &[u8] = b"penumbra.compliance.asset_registration_grant.v1";
const USER_REGISTRATION_GRANT_DOMAIN: &[u8] = b"penumbra.compliance.user_registration_grant.v1";

fn grant_signing_bytes(domain: &[u8], body_bytes: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(domain.len() + 1 + body_bytes.len());
    bytes.extend_from_slice(domain);
    bytes.push(0);
    bytes.extend_from_slice(&body_bytes);
    bytes
}

/// DLEQ proof wire format: (c, s) per tier. Transfer-input has 1 tier, transfer-output has 3.
pub const FQ_BYTES: usize = 32;
pub const TRANSFER_INPUT_DLEQ_BYTES: usize = FQ_BYTES * 2; // 64 bytes: c || s
pub const TRANSFER_OUTPUT_DLEQ_BYTES: usize = FQ_BYTES * 6; // 192 bytes: c1||s1||c2||s2||c3||s3

// Compile-time consistency checks.
const _: () = {
    assert!(
        TRANSFER_INPUT_WIRE_BYTES == 288,
        "TRANSFER_INPUT_WIRE_BYTES must be 288"
    );
    assert!(
        TRANSFER_OUTPUT_WIRE_BYTES == 608,
        "TRANSFER_OUTPUT_WIRE_BYTES must be 608"
    );
    assert!(
        TRANSFER_INPUT_DLEQ_BYTES == 64,
        "TRANSFER_INPUT_DLEQ_BYTES must be 64"
    );
    assert!(
        TRANSFER_OUTPUT_DLEQ_BYTES == 192,
        "TRANSFER_OUTPUT_DLEQ_BYTES must be 192"
    );
    assert!(
        TRANSFER_INPUT_CIPHERTEXT_FQS == 7,
        "TRANSFER_INPUT_CIPHERTEXT_FQS must be 7"
    );
    assert!(
        TRANSFER_OUTPUT_CIPHERTEXT_FQS == 13,
        "TRANSFER_OUTPUT_CIPHERTEXT_FQS must be 13"
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
/// Contains address, asset_id, slot metadata, and derivation scalar `d`.
/// `d = SHA256("elgamal-derivation-v1\0\0" || slot_derivation)` — matches Orbis derivation.
/// ACK = d × ring_pk, computed in-circuit from the leaf's `d` value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::ComplianceLeaf", into = "pb::ComplianceLeaf")]
pub struct ComplianceLeaf {
    /// The registered address for compliance.
    pub address: Address,
    /// The asset ID this compliance leaf applies to.
    pub asset_id: asset::Id,
    /// ACP-authorized slot for this asset.
    pub slot_id: u32,
    /// Canonical random slot derivation material.
    pub slot_derivation: Fq,
    /// Derivation scalar: d = SHA256_derive(slot_derivation). Verified at registration.
    pub d: Fq,
}

impl ComplianceLeaf {
    /// Create a slot-0 ComplianceLeaf from ACP-authorized slot derivation material.
    pub fn new(address: Address, asset_id: asset::Id, slot_derivation: Fq) -> Self {
        Self::with_slot(address, asset_id, 0, slot_derivation)
    }

    /// Create a ComplianceLeaf from ACP-authorized slot derivation material.
    pub fn with_slot(
        address: Address,
        asset_id: asset::Id,
        slot_id: u32,
        slot_derivation: Fq,
    ) -> Self {
        let d = crate::derive_compliance_scalar(slot_derivation);
        Self {
            address,
            asset_id,
            slot_id,
            slot_derivation,
            d,
        }
    }

    /// Create the explicit synthetic leaf used only for unregulated asset proofs.
    pub fn synthetic_unregulated(address: Address, asset_id: asset::Id) -> Self {
        let slot_derivation = address.diversified_generator().vartime_compress_to_field();
        Self::with_slot(address, asset_id, 0, slot_derivation)
    }

    /// Create a test-only leaf with explicitly supplied d.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_unchecked(
        address: Address,
        asset_id: asset::Id,
        slot_id: u32,
        slot_derivation: Fq,
        d: Fq,
    ) -> Self {
        Self {
            address,
            asset_id,
            slot_id,
            slot_derivation,
            d,
        }
    }

    pub fn validate_derivation(&self) -> anyhow::Result<()> {
        let expected = crate::derive_compliance_scalar(self.slot_derivation);
        anyhow::ensure!(
            self.d == expected,
            "compliance leaf d does not match slot_derivation"
        );
        Ok(())
    }

    /// Create the Poseidon commitment.
    pub fn commit(&self) -> StateCommitment {
        let diversified_generator = self
            .address
            .diversified_generator()
            .vartime_compress_to_field();
        let transmission_key_s = Fq::from_bytes_checked(&self.address.transmission_key().0)
            .expect("transmission key is valid");
        let asset_id_field = self.asset_id.0;

        let commit = poseidon377::hash_6(
            &COMPLIANCE_LEAF_DOMAIN_SEP,
            (
                diversified_generator,
                transmission_key_s,
                asset_id_field,
                Fq::from(self.slot_id),
                self.slot_derivation,
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
        if value.d.is_empty() {
            anyhow::bail!("missing d");
        }
        let bytes: [u8; 32] = value
            .d
            .try_into()
            .map_err(|_| anyhow::anyhow!("d must be 32 bytes"))?;
        let d = Fq::from_bytes_checked(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid d field element"))?;
        if value.slot_derivation.is_empty() {
            anyhow::bail!("missing slot_derivation");
        }
        let slot_derivation_bytes: [u8; 32] =
            value.slot_derivation.try_into().map_err(|v: Vec<u8>| {
                anyhow::anyhow!("slot_derivation must be 32 bytes, got {}", v.len())
            })?;
        let slot_derivation = Fq::from_bytes_checked(&slot_derivation_bytes)
            .map_err(|_| anyhow::anyhow!("invalid slot_derivation field element"))?;
        let leaf = ComplianceLeaf {
            address: value
                .address
                .ok_or_else(|| anyhow::anyhow!("missing address"))?
                .try_into()?,
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            slot_id: value.slot_id,
            slot_derivation,
            d,
        };
        leaf.validate_derivation()?;
        Ok(leaf)
    }
}

impl From<ComplianceLeaf> for pb::ComplianceLeaf {
    fn from(value: ComplianceLeaf) -> pb::ComplianceLeaf {
        pb::ComplianceLeaf {
            address: Some(value.address.into()),
            asset_id: Some(value.asset_id.into()),
            d: value.d.to_bytes().to_vec(),
            slot_id: value.slot_id,
            slot_derivation: value.slot_derivation.to_bytes().to_vec(),
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
    /// Number of ACP-authorized compliance slots for this asset.
    pub slot_count: u32,
    /// Direct IBC routes allowed for this asset. Empty = IBC blocked.
    pub allowed_ibc_routes: Vec<IbcRoute>,
    /// External origin for regulated voucher assets.
    pub ibc_origin: Option<IbcAssetOrigin>,
}

/// A direct ICS-20 route bound to local committed IBC identifiers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "pb::IbcRoute", into = "pb::IbcRoute")]
pub struct IbcRoute {
    pub local_port: String,
    pub local_channel: String,
    pub connection_id: String,
    pub counterparty_port: String,
    pub counterparty_channel: String,
}

impl IbcRoute {
    pub fn transfer(
        local_channel: impl Into<String>,
        connection_id: impl Into<String>,
        counterparty_channel: impl Into<String>,
    ) -> Self {
        Self {
            local_port: "transfer".to_string(),
            local_channel: local_channel.into(),
            connection_id: connection_id.into(),
            counterparty_port: "transfer".to_string(),
            counterparty_channel: counterparty_channel.into(),
        }
    }

    pub fn canonical_key(&self) -> String {
        [
            self.local_port.as_str(),
            self.local_channel.as_str(),
            self.connection_id.as_str(),
            self.counterparty_port.as_str(),
            self.counterparty_channel.as_str(),
        ]
        .join("\0")
    }
}

impl DomainType for IbcRoute {
    type Proto = pb::IbcRoute;
}

impl TryFrom<pb::IbcRoute> for IbcRoute {
    type Error = anyhow::Error;

    fn try_from(value: pb::IbcRoute) -> Result<Self, Self::Error> {
        anyhow::ensure!(!value.local_port.is_empty(), "missing local_port");
        anyhow::ensure!(!value.local_channel.is_empty(), "missing local_channel");
        anyhow::ensure!(!value.connection_id.is_empty(), "missing connection_id");
        anyhow::ensure!(
            !value.counterparty_port.is_empty(),
            "missing counterparty_port"
        );
        anyhow::ensure!(
            !value.counterparty_channel.is_empty(),
            "missing counterparty_channel"
        );
        Ok(Self {
            local_port: value.local_port,
            local_channel: value.local_channel,
            connection_id: value.connection_id,
            counterparty_port: value.counterparty_port,
            counterparty_channel: value.counterparty_channel,
        })
    }
}

impl From<IbcRoute> for pb::IbcRoute {
    fn from(value: IbcRoute) -> Self {
        Self {
            local_port: value.local_port,
            local_channel: value.local_channel,
            connection_id: value.connection_id,
            counterparty_port: value.counterparty_port,
            counterparty_channel: value.counterparty_channel,
        }
    }
}

/// External origin for a regulated ICS-20 voucher asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::IbcAssetOrigin", into = "pb::IbcAssetOrigin")]
pub struct IbcAssetOrigin {
    pub route: IbcRoute,
    pub base_denom: String,
}

impl IbcAssetOrigin {
    pub fn canonical_key(&self) -> String {
        format!("{}\0{}", self.route.canonical_key(), self.base_denom)
    }
}

impl DomainType for IbcAssetOrigin {
    type Proto = pb::IbcAssetOrigin;
}

impl TryFrom<pb::IbcAssetOrigin> for IbcAssetOrigin {
    type Error = anyhow::Error;

    fn try_from(value: pb::IbcAssetOrigin) -> Result<Self, Self::Error> {
        anyhow::ensure!(!value.base_denom.is_empty(), "missing base_denom");
        Ok(Self {
            route: value
                .route
                .ok_or_else(|| anyhow::anyhow!("missing origin route"))?
                .try_into()?,
            base_denom: value.base_denom,
        })
    }
}

impl From<IbcAssetOrigin> for pb::IbcAssetOrigin {
    fn from(value: IbcAssetOrigin) -> Self {
        Self {
            route: Some(value.route.into()),
            base_denom: value.base_denom,
        }
    }
}

fn canonical_routes(mut routes: Vec<IbcRoute>) -> Vec<IbcRoute> {
    routes.sort();
    routes.dedup();
    routes
}

pub fn canonical_route_policy_string(
    origin: &Option<IbcAssetOrigin>,
    routes: &[IbcRoute],
) -> String {
    let mut sorted = routes.to_vec();
    sorted.sort();
    let route_part = sorted
        .iter()
        .map(IbcRoute::canonical_key)
        .collect::<Vec<_>>()
        .join("\0\0");
    let origin_part = origin
        .as_ref()
        .map(IbcAssetOrigin::canonical_key)
        .unwrap_or_default();
    format!("origin:{origin_part}\0routes:{route_part}")
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
/// Contains issuer parameters (detection key, threshold, IBC route policy)
/// and Orbis ring binding (ring_pk, policy identifiers).
/// This is state-only data — NOT included in the IMT Merkle commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetPolicy {
    pub params: AssetParams,
    pub ring: RingData,
    pub registration_authority_vk: Option<VerificationKey<SpendAuth>>,
}

const ASSET_POLICY_STORAGE_MAGIC: &[u8; 4] = b"AP2\0";

impl AssetPolicy {
    /// Create a new asset policy.
    pub fn new(
        dk_pub: decaf377::Element,
        threshold: u128,
        slot_count: u32,
        allowed_ibc_routes: Vec<IbcRoute>,
        ibc_origin: Option<IbcAssetOrigin>,
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
                slot_count,
                allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
                ibc_origin,
            },
            ring: RingData {
                ring_id,
                ring_pk,
                policy_id,
                permission,
                resource,
            },
            registration_authority_vk: None,
        }
    }

    pub fn with_registration_authority(mut self, vk: VerificationKey<SpendAuth>) -> Self {
        self.registration_authority_vk = Some(vk);
        self
    }

    pub fn replace_allowed_ibc_routes(&mut self, routes: Vec<IbcRoute>) {
        self.params.allowed_ibc_routes = canonical_routes(routes);
    }

    pub fn permits_ibc_route(&self, route: &IbcRoute) -> bool {
        self.params.allowed_ibc_routes.binary_search(route).is_ok()
    }

    /// Create a simple policy with just dk_pub, threshold, and ring_pk.
    /// Uses empty strings for ring_id, policy_id, permission, resource.
    pub fn simple(dk_pub: decaf377::Element, threshold: u128, ring_pk: decaf377::Element) -> Self {
        Self::new(
            dk_pub,
            threshold,
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![],
            None,
            String::new(),
            ring_pk,
            String::new(),
            String::new(),
            String::new(),
        )
    }

    /// Create a default policy for unregulated assets.
    ///
    /// Uses protocol sink keys for dk_pub/ring_pk and u128::MAX for threshold.
    pub fn default_unregulated() -> Self {
        Self {
            params: AssetParams {
                dk_pub: *crate::crypto::UNREGULATED_SINK_DK_PUB,
                threshold: u128::MAX,
                slot_count: 0,
                allowed_ibc_routes: vec![],
                ibc_origin: None,
            },
            ring: RingData {
                ring_id: String::new(),
                ring_pk: *crate::crypto::UNREGULATED_SINK_RING_PK,
                policy_id: String::new(),
                permission: String::new(),
                resource: String::new(),
            },
            registration_authority_vk: None,
        }
    }

    /// Serialize to bytes for storage.
    ///
    /// Format starts with `AP2\0`, so older channel-only rows fail closed.
    ///         [ring_id_len: 2] [ring_id bytes]
    ///         [policy_id_len: 2] [policy_id bytes]
    ///         [permission_len: 2] [permission bytes]
    ///         [resource_len: 2] [resource bytes]
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(ASSET_POLICY_STORAGE_MAGIC);
        // AssetParams
        bytes.extend_from_slice(&self.params.dk_pub.vartime_compress().0);
        bytes.extend_from_slice(&self.params.threshold.to_le_bytes());
        bytes.extend_from_slice(&self.params.slot_count.to_le_bytes());
        // RingData - ring_pk
        bytes.extend_from_slice(&self.ring.ring_pk.vartime_compress().0);
        fn write_string(bytes: &mut Vec<u8>, s: &str, field: &str) -> anyhow::Result<()> {
            let len = u16::try_from(s.len())
                .map_err(|_| anyhow::anyhow!("{field} too long: {} bytes", s.len()))?;
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(s.as_bytes());
            Ok(())
        }
        fn write_route(bytes: &mut Vec<u8>, route: &IbcRoute) -> anyhow::Result<()> {
            write_string(bytes, &route.local_port, "local_port")?;
            write_string(bytes, &route.local_channel, "local_channel")?;
            write_string(bytes, &route.connection_id, "connection_id")?;
            write_string(bytes, &route.counterparty_port, "counterparty_port")?;
            write_string(bytes, &route.counterparty_channel, "counterparty_channel")
        }
        let count = u16::try_from(self.params.allowed_ibc_routes.len()).map_err(|_| {
            anyhow::anyhow!(
                "too many allowed IBC routes: {}",
                self.params.allowed_ibc_routes.len()
            )
        })?;
        bytes.extend_from_slice(&count.to_le_bytes());
        for route in &self.params.allowed_ibc_routes {
            write_route(&mut bytes, route)?;
        }
        match &self.params.ibc_origin {
            Some(origin) => {
                bytes.push(1);
                write_route(&mut bytes, &origin.route)?;
                write_string(&mut bytes, &origin.base_denom, "base_denom")?;
            }
            None => bytes.push(0),
        }
        // String fields
        write_string(&mut bytes, &self.ring.ring_id, "ring_id")?;
        write_string(&mut bytes, &self.ring.policy_id, "policy_id")?;
        write_string(&mut bytes, &self.ring.permission, "permission")?;
        write_string(&mut bytes, &self.ring.resource, "resource")?;
        if let Some(vk) = &self.registration_authority_vk {
            bytes.push(1);
            bytes.extend_from_slice(&vk.to_bytes());
        } else {
            bytes.push(0);
        }
        Ok(bytes)
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < ASSET_POLICY_STORAGE_MAGIC.len() + 84 {
            anyhow::bail!(
                "invalid AssetPolicy length: expected AP2 header and policy body, got {}",
                bytes.len()
            );
        }
        anyhow::ensure!(
            &bytes[..ASSET_POLICY_STORAGE_MAGIC.len()] == ASSET_POLICY_STORAGE_MAGIC,
            "unsupported AssetPolicy storage encoding"
        );
        let mut offset = ASSET_POLICY_STORAGE_MAGIC.len();
        let dk_pub_bytes: [u8; 32] = bytes[offset..offset + 32].try_into()?;
        offset += 32;
        let dk_pub = decaf377::Encoding(dk_pub_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?;
        let threshold = u128::from_le_bytes(bytes[offset..offset + 16].try_into()?);
        offset += 16;
        let slot_count = u32::from_le_bytes(bytes[offset..offset + 4].try_into()?);
        offset += 4;
        let ring_pk_bytes: [u8; 32] = bytes[offset..offset + 32].try_into()?;
        offset += 32;
        let ring_pk = decaf377::Encoding(ring_pk_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?;

        fn read_string(bytes: &[u8], offset: &mut usize, field: &str) -> anyhow::Result<String> {
            if *offset + 2 > bytes.len() {
                anyhow::bail!("missing {field}");
            }
            let len = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into()?) as usize;
            *offset += 2;
            if *offset + len > bytes.len() {
                anyhow::bail!("truncated {field}");
            }
            let s = std::str::from_utf8(&bytes[*offset..*offset + len])
                .map_err(|_| anyhow::anyhow!("invalid UTF-8 in {field}"))?;
            *offset += len;
            Ok(s.to_string())
        }
        fn read_route(bytes: &[u8], offset: &mut usize) -> anyhow::Result<IbcRoute> {
            Ok(IbcRoute {
                local_port: read_string(bytes, offset, "local_port")?,
                local_channel: read_string(bytes, offset, "local_channel")?,
                connection_id: read_string(bytes, offset, "connection_id")?,
                counterparty_port: read_string(bytes, offset, "counterparty_port")?,
                counterparty_channel: read_string(bytes, offset, "counterparty_channel")?,
            })
        }

        if offset + 2 > bytes.len() {
            anyhow::bail!("missing allowed_ibc_routes count");
        }
        let count = u16::from_le_bytes(bytes[offset..offset + 2].try_into()?) as usize;
        offset += 2;
        let mut allowed_ibc_routes = Vec::with_capacity(count);
        for _ in 0..count {
            allowed_ibc_routes.push(read_route(bytes, &mut offset)?);
        }
        allowed_ibc_routes = canonical_routes(allowed_ibc_routes);

        if offset >= bytes.len() {
            anyhow::bail!("missing ibc_origin flag");
        }
        let has_origin = bytes[offset];
        offset += 1;
        let ibc_origin = match has_origin {
            0 => None,
            1 => Some(IbcAssetOrigin {
                route: read_route(bytes, &mut offset)?,
                base_denom: read_string(bytes, &mut offset, "base_denom")?,
            }),
            _ => anyhow::bail!("invalid ibc_origin flag: {has_origin}"),
        };

        let ring_id = read_string(bytes, &mut offset, "ring_id")?;
        let policy_id = read_string(bytes, &mut offset, "policy_id")?;
        let permission = read_string(bytes, &mut offset, "permission")?;
        let resource = read_string(bytes, &mut offset, "resource")?;
        if offset >= bytes.len() {
            anyhow::bail!("missing registration_authority_vk flag");
        }
        let has_vk = bytes[offset];
        offset += 1;
        let registration_authority_vk = if has_vk == 0 {
            None
        } else if has_vk == 1 {
            if offset + 32 > bytes.len() {
                anyhow::bail!("truncated registration_authority_vk");
            }
            let vk = VerificationKey::<SpendAuth>::try_from(&bytes[offset..offset + 32])
                .map_err(|_| anyhow::anyhow!("invalid registration_authority_vk"))?;
            offset += 32;
            Some(vk)
        } else {
            anyhow::bail!("invalid registration_authority_vk flag: {has_vk}");
        };
        if offset != bytes.len() {
            anyhow::bail!("trailing bytes after AssetPolicy");
        }

        Ok(Self {
            params: AssetParams {
                dk_pub,
                threshold,
                slot_count,
                allowed_ibc_routes,
                ibc_origin,
            },
            ring: RingData {
                ring_id,
                ring_pk,
                policy_id,
                permission,
                resource,
            },
            registration_authority_vk,
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
        if value.dk_pub.is_empty() {
            anyhow::bail!("missing dk_pub");
        }
        let bytes: [u8; 32] = value
            .dk_pub
            .try_into()
            .map_err(|_| anyhow::anyhow!("dk_pub must be 32 bytes"))?;
        let dk_pub = decaf377::Encoding(bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding"))?;

        if value.threshold.is_empty() {
            anyhow::bail!("missing threshold");
        }
        let bytes: [u8; 16] = value
            .threshold
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len()))?;
        let threshold = u128::from_le_bytes(bytes);

        if value.ring_pk.is_empty() {
            anyhow::bail!("missing ring_pk");
        }
        let bytes: [u8; 32] = value
            .ring_pk
            .try_into()
            .map_err(|_| anyhow::anyhow!("ring_pk must be 32 bytes"))?;
        let ring_pk = decaf377::Encoding(bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?;
        let registration_authority_vk = value
            .registration_authority_vk
            .map(TryInto::try_into)
            .transpose()
            .map_err(|_| anyhow::anyhow!("invalid registration_authority_vk"))?;
        let allowed_ibc_routes = value
            .allowed_ibc_routes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let ibc_origin = value.ibc_origin.map(TryInto::try_into).transpose()?;

        Ok(AssetPolicy {
            params: AssetParams {
                dk_pub,
                threshold,
                slot_count: value.slot_count,
                allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
                ibc_origin,
            },
            ring: RingData {
                ring_id: value.ring_id,
                ring_pk,
                policy_id: value.policy_id,
                permission: value.permission,
                resource: value.resource,
            },
            registration_authority_vk,
        })
    }
}

impl From<AssetPolicy> for pb::AssetPolicy {
    fn from(value: AssetPolicy) -> pb::AssetPolicy {
        pb::AssetPolicy {
            dk_pub: value.params.dk_pub.vartime_compress().0.to_vec(),
            threshold: value.params.threshold.to_le_bytes().to_vec(),
            slot_count: value.params.slot_count,
            allowed_ibc_routes: value
                .params
                .allowed_ibc_routes
                .into_iter()
                .map(Into::into)
                .collect(),
            ring_id: value.ring.ring_id,
            ring_pk: value.ring.ring_pk.vartime_compress().0.to_vec(),
            policy_id: value.ring.policy_id,
            permission: value.ring.permission,
            resource: value.ring.resource,
            registration_authority_vk: value.registration_authority_vk.map(Into::into),
            ibc_origin: value.params.ibc_origin.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::AssetRegistrationGrantBody",
    into = "pb::AssetRegistrationGrantBody"
)]
pub struct AssetRegistrationGrantBody {
    pub asset_id: asset::Id,
    pub is_regulated: bool,
    pub dk_pub: Option<decaf377::Element>,
    pub threshold: Option<u128>,
    pub slot_count: u32,
    pub allowed_ibc_routes: Vec<IbcRoute>,
    pub ibc_origin: Option<IbcAssetOrigin>,
    pub ring_pk: Option<decaf377::Element>,
    pub ring_id: String,
    pub policy_id: String,
    pub permission: String,
    pub resource: String,
    pub registration_authority_vk: Option<VerificationKey<SpendAuth>>,
    pub valid_until_unix: u64,
}

impl DomainType for AssetRegistrationGrantBody {
    type Proto = pb::AssetRegistrationGrantBody;
}

impl AssetRegistrationGrantBody {
    pub fn signing_bytes(&self) -> Vec<u8> {
        grant_signing_bytes(ASSET_REGISTRATION_GRANT_DOMAIN, self.encode_to_vec())
    }
}

impl TryFrom<pb::AssetRegistrationGrantBody> for AssetRegistrationGrantBody {
    type Error = anyhow::Error;

    fn try_from(value: pb::AssetRegistrationGrantBody) -> Result<Self, Self::Error> {
        let dk_pub = decode_optional_element(value.dk_pub, "dk_pub")?;
        let threshold = if value.threshold.is_empty() {
            None
        } else {
            let bytes: [u8; 16] = value.threshold.try_into().map_err(|v: Vec<u8>| {
                anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len())
            })?;
            Some(u128::from_le_bytes(bytes))
        };
        let ring_pk = decode_optional_element(value.ring_pk, "ring_pk")?;
        let registration_authority_vk = value
            .registration_authority_vk
            .map(TryInto::try_into)
            .transpose()
            .map_err(|_| anyhow::anyhow!("invalid registration_authority_vk"))?;
        let allowed_ibc_routes = value
            .allowed_ibc_routes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let ibc_origin = value.ibc_origin.map(TryInto::try_into).transpose()?;

        Ok(Self {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            is_regulated: value.is_regulated,
            dk_pub,
            threshold,
            slot_count: value.slot_count,
            allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
            ibc_origin,
            ring_pk,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            registration_authority_vk,
            valid_until_unix: value.valid_until_unix,
        })
    }
}

impl From<AssetRegistrationGrantBody> for pb::AssetRegistrationGrantBody {
    fn from(value: AssetRegistrationGrantBody) -> Self {
        Self {
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
            slot_count: value.slot_count,
            allowed_ibc_routes: value
                .allowed_ibc_routes
                .into_iter()
                .map(Into::into)
                .collect(),
            ring_pk: value
                .ring_pk
                .map(|e| e.vartime_compress().0.to_vec())
                .unwrap_or_default(),
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            registration_authority_vk: value.registration_authority_vk.map(Into::into),
            valid_until_unix: value.valid_until_unix,
            ibc_origin: value.ibc_origin.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::AssetRegistrationGrant",
    into = "pb::AssetRegistrationGrant"
)]
pub struct AssetRegistrationGrant {
    pub body: AssetRegistrationGrantBody,
    pub registrar_vk: VerificationKey<SpendAuth>,
    pub signature: Signature<SpendAuth>,
}

impl DomainType for AssetRegistrationGrant {
    type Proto = pb::AssetRegistrationGrant;
}

impl AssetRegistrationGrant {
    pub fn verify(&self) -> anyhow::Result<()> {
        self.registrar_vk
            .verify(&self.body.signing_bytes(), &self.signature)
            .map_err(|_| anyhow::anyhow!("asset registration grant signature failed to verify"))
    }
}

impl TryFrom<pb::AssetRegistrationGrant> for AssetRegistrationGrant {
    type Error = anyhow::Error;

    fn try_from(value: pb::AssetRegistrationGrant) -> Result<Self, Self::Error> {
        Ok(Self {
            body: value
                .body
                .ok_or_else(|| anyhow::anyhow!("missing asset registration grant body"))?
                .try_into()?,
            registrar_vk: value
                .registrar_vk
                .ok_or_else(|| anyhow::anyhow!("missing asset registration registrar_vk"))?
                .try_into()?,
            signature: value
                .signature
                .ok_or_else(|| anyhow::anyhow!("missing asset registration grant signature"))?
                .try_into()?,
        })
    }
}

impl From<AssetRegistrationGrant> for pb::AssetRegistrationGrant {
    fn from(value: AssetRegistrationGrant) -> Self {
        Self {
            body: Some(value.body.into()),
            registrar_vk: Some(value.registrar_vk.into()),
            signature: Some(value.signature.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::UserRegistrationGrantBody",
    into = "pb::UserRegistrationGrantBody"
)]
pub struct UserRegistrationGrantBody {
    pub leaf: ComplianceLeaf,
    pub policy_id: String,
    pub valid_until_unix: u64,
    pub nonce: Vec<u8>,
}

impl DomainType for UserRegistrationGrantBody {
    type Proto = pb::UserRegistrationGrantBody;
}

impl UserRegistrationGrantBody {
    pub fn signing_bytes(&self) -> Vec<u8> {
        grant_signing_bytes(USER_REGISTRATION_GRANT_DOMAIN, self.encode_to_vec())
    }
}

impl TryFrom<pb::UserRegistrationGrantBody> for UserRegistrationGrantBody {
    type Error = anyhow::Error;

    fn try_from(value: pb::UserRegistrationGrantBody) -> Result<Self, Self::Error> {
        Ok(Self {
            leaf: value
                .leaf
                .ok_or_else(|| anyhow::anyhow!("missing user registration grant leaf"))?
                .try_into()?,
            policy_id: value.policy_id,
            valid_until_unix: value.valid_until_unix,
            nonce: value.nonce,
        })
    }
}

impl From<UserRegistrationGrantBody> for pb::UserRegistrationGrantBody {
    fn from(value: UserRegistrationGrantBody) -> Self {
        Self {
            leaf: Some(value.leaf.into()),
            policy_id: value.policy_id,
            valid_until_unix: value.valid_until_unix,
            nonce: value.nonce,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::UserRegistrationGrant",
    into = "pb::UserRegistrationGrant"
)]
pub struct UserRegistrationGrant {
    pub body: UserRegistrationGrantBody,
    pub signature: Signature<SpendAuth>,
}

impl DomainType for UserRegistrationGrant {
    type Proto = pb::UserRegistrationGrant;
}

impl UserRegistrationGrant {
    pub fn verify(&self, vk: &VerificationKey<SpendAuth>) -> anyhow::Result<()> {
        vk.verify(&self.body.signing_bytes(), &self.signature)
            .map_err(|_| anyhow::anyhow!("user registration grant signature failed to verify"))
    }
}

impl TryFrom<pb::UserRegistrationGrant> for UserRegistrationGrant {
    type Error = anyhow::Error;

    fn try_from(value: pb::UserRegistrationGrant) -> Result<Self, Self::Error> {
        Ok(Self {
            body: value
                .body
                .ok_or_else(|| anyhow::anyhow!("missing user registration grant body"))?
                .try_into()?,
            signature: value
                .signature
                .ok_or_else(|| anyhow::anyhow!("missing user registration grant signature"))?
                .try_into()?,
        })
    }
}

impl From<UserRegistrationGrant> for pb::UserRegistrationGrant {
    fn from(value: UserRegistrationGrant) -> Self {
        Self {
            body: Some(value.body.into()),
            signature: Some(value.signature.into()),
        }
    }
}

fn decode_optional_element(
    bytes: Vec<u8>,
    field: &'static str,
) -> anyhow::Result<Option<decaf377::Element>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("{field} must be exactly 32 bytes"))?;
    Ok(Some(
        decaf377::Encoding(bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid {field} encoding"))?,
    ))
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
    /// Number of ACP-authorized compliance slots for this asset.
    pub slot_count: u32,
    /// Direct IBC routes allowed for this regulated asset. Empty = IBC blocked.
    pub allowed_ibc_routes: Vec<IbcRoute>,
    /// External IBC origin for regulated voucher assets.
    pub ibc_origin: Option<IbcAssetOrigin>,
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
    /// Immutable authority key that signs user registration grants for this asset.
    pub registration_authority_vk: Option<VerificationKey<SpendAuth>>,
    /// Registrar authorization for this asset registration.
    pub asset_registration_grant: Option<AssetRegistrationGrant>,
}

impl DomainType for MsgRegisterAsset {
    type Proto = pb::MsgRegisterAsset;
}

impl TryFrom<pb::MsgRegisterAsset> for MsgRegisterAsset {
    type Error = anyhow::Error;

    fn try_from(value: pb::MsgRegisterAsset) -> Result<Self, Self::Error> {
        let dk_pub = decode_optional_element(value.dk_pub, "dk_pub")?;

        let threshold = if value.threshold.is_empty() {
            None
        } else {
            let threshold_bytes: [u8; 16] = value.threshold.try_into().map_err(|v: Vec<u8>| {
                anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len())
            })?;
            Some(u128::from_le_bytes(threshold_bytes))
        };

        let ring_pk = decode_optional_element(value.ring_pk, "ring_pk")?;
        let registration_authority_vk = value
            .registration_authority_vk
            .map(TryInto::try_into)
            .transpose()
            .map_err(|_| anyhow::anyhow!("invalid registration_authority_vk"))?;
        let asset_registration_grant = value
            .asset_registration_grant
            .map(TryInto::try_into)
            .transpose()?;
        let allowed_ibc_routes = value
            .allowed_ibc_routes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let ibc_origin = value.ibc_origin.map(TryInto::try_into).transpose()?;

        Ok(MsgRegisterAsset {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            is_regulated: value.is_regulated,
            dk_pub,
            threshold,
            slot_count: value.slot_count,
            allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
            ibc_origin,
            ring_pk,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            registration_authority_vk,
            asset_registration_grant,
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
            slot_count: value.slot_count,
            allowed_ibc_routes: value
                .allowed_ibc_routes
                .into_iter()
                .map(Into::into)
                .collect(),
            ring_pk: value
                .ring_pk
                .map(|e| e.vartime_compress().0.to_vec())
                .unwrap_or_default(),
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            registration_authority_vk: value.registration_authority_vk.map(Into::into),
            asset_registration_grant: value.asset_registration_grant.map(Into::into),
            ibc_origin: value.ibc_origin.map(Into::into),
        }
    }
}

impl MsgRegisterAsset {
    pub fn registration_grant_body(&self, valid_until_unix: u64) -> AssetRegistrationGrantBody {
        AssetRegistrationGrantBody {
            asset_id: self.asset_id,
            is_regulated: self.is_regulated,
            dk_pub: self.dk_pub,
            threshold: self.threshold,
            slot_count: self.slot_count,
            allowed_ibc_routes: self.allowed_ibc_routes.clone(),
            ibc_origin: self.ibc_origin.clone(),
            ring_pk: self.ring_pk,
            ring_id: self.ring_id.clone(),
            policy_id: self.policy_id.clone(),
            permission: self.permission.clone(),
            resource: self.resource.clone(),
            registration_authority_vk: self.registration_authority_vk,
            valid_until_unix,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::UpdateAssetIbcPolicy",
    into = "pb::UpdateAssetIbcPolicy"
)]
pub struct UpdateAssetIbcPolicy {
    pub asset_id: asset::Id,
    pub expected_route_policy_hash: [u8; 32],
    pub allowed_ibc_routes: Vec<IbcRoute>,
}

impl DomainType for UpdateAssetIbcPolicy {
    type Proto = pb::UpdateAssetIbcPolicy;
}

impl TryFrom<pb::UpdateAssetIbcPolicy> for UpdateAssetIbcPolicy {
    type Error = anyhow::Error;

    fn try_from(value: pb::UpdateAssetIbcPolicy) -> Result<Self, Self::Error> {
        let expected_route_policy_hash =
            value
                .expected_route_policy_hash
                .try_into()
                .map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "expected_route_policy_hash must be 32 bytes, got {}",
                        v.len()
                    )
                })?;
        let allowed_ibc_routes = value
            .allowed_ibc_routes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            expected_route_policy_hash,
            allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
        })
    }
}

impl From<UpdateAssetIbcPolicy> for pb::UpdateAssetIbcPolicy {
    fn from(value: UpdateAssetIbcPolicy) -> Self {
        Self {
            asset_id: Some(value.asset_id.into()),
            expected_route_policy_hash: value.expected_route_policy_hash.to_vec(),
            allowed_ibc_routes: value
                .allowed_ibc_routes
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

/// Message to register a user's address for a regulated asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::MsgRegisterUser", into = "pb::MsgRegisterUser")]
pub struct MsgRegisterUser {
    /// The compliance leaf containing the user's registration information.
    pub leaf: ComplianceLeaf,
    /// Grant authorizing this registration.
    pub grant: Option<UserRegistrationGrant>,
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
            grant: value.grant.map(TryInto::try_into).transpose()?,
        })
    }
}

impl From<MsgRegisterUser> for pb::MsgRegisterUser {
    fn from(value: MsgRegisterUser) -> pb::MsgRegisterUser {
        pb::MsgRegisterUser {
            leaf: Some(value.leaf.into()),
            grant: value.grant.map(Into::into),
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
        let slot_derivation = decaf377::Fq::from(42u64);

        let leaf = ComplianceLeaf::with_slot(address.clone(), asset_id, 3, slot_derivation);

        assert_eq!(leaf.address, address);
        assert_eq!(leaf.asset_id, asset_id);
        assert_eq!(leaf.slot_id, 3);
        assert_eq!(leaf.slot_derivation, slot_derivation);
        assert_eq!(leaf.d, crate::derive_compliance_scalar(slot_derivation));
    }

    #[test]
    fn test_compliance_leaf_different_addresses_different_commits() {
        let mut rng = rand::thread_rng();
        let asset_id = asset::Id(decaf377::Fq::from(100u64));
        let slot_derivation = decaf377::Fq::from(42u64);

        let address1 = Address::dummy(&mut rng);
        let address2 = Address::dummy(&mut rng);

        let leaf1 = ComplianceLeaf::with_slot(address1, asset_id, 0, slot_derivation);
        let leaf2 = ComplianceLeaf::with_slot(address2, asset_id, 0, slot_derivation);

        assert_ne!(
            leaf1.commit(),
            leaf2.commit(),
            "Different addresses must have different commitments"
        );
    }

    #[test]
    fn test_same_slot_reuse_same_d_and_ack() {
        let mut rng = rand::thread_rng();
        let asset_id = asset::Id(decaf377::Fq::from(100u64));
        let slot_derivation = decaf377::Fq::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(77u64);

        let leaf1 =
            ComplianceLeaf::with_slot(Address::dummy(&mut rng), asset_id, 2, slot_derivation);
        let leaf2 =
            ComplianceLeaf::with_slot(Address::dummy(&mut rng), asset_id, 2, slot_derivation);
        let ack1 = ring_pk * decaf377::Fr::from_le_bytes_mod_order(&leaf1.d.to_bytes());
        let ack2 = ring_pk * decaf377::Fr::from_le_bytes_mod_order(&leaf2.d.to_bytes());

        assert_eq!(leaf1.d, leaf2.d);
        assert_eq!(ack1, ack2);
    }

    #[test]
    fn test_same_address_different_slots_different_d_and_ack() {
        let mut rng = rand::thread_rng();
        let address = Address::dummy(&mut rng);
        let asset_id = asset::Id(decaf377::Fq::from(100u64));
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(77u64);

        let leaf1 =
            ComplianceLeaf::with_slot(address.clone(), asset_id, 0, decaf377::Fq::from(42u64));
        let leaf2 = ComplianceLeaf::with_slot(address, asset_id, 1, decaf377::Fq::from(43u64));
        let ack1 = ring_pk * decaf377::Fr::from_le_bytes_mod_order(&leaf1.d.to_bytes());
        let ack2 = ring_pk * decaf377::Fr::from_le_bytes_mod_order(&leaf2.d.to_bytes());

        assert_ne!(leaf1.d, leaf2.d);
        assert_ne!(ack1, ack2);
        assert_ne!(leaf1.commit(), leaf2.commit());
    }

    #[test]
    fn test_compliance_leaf_proto_roundtrip() {
        let mut rng = rand::thread_rng();
        let wallet = Address::dummy(&mut rng);
        let asset_id = asset::Id(decaf377::Fq::from(999u64));
        let slot_derivation = decaf377::Fq::from(123u64);

        let original = ComplianceLeaf::with_slot(wallet, asset_id, 4, slot_derivation);

        let proto: pb::ComplianceLeaf = original.clone().into();
        let recovered: ComplianceLeaf = proto.try_into().expect("should parse");

        assert_eq!(original.address, recovered.address);
        assert_eq!(original.asset_id, recovered.asset_id);
        assert_eq!(original.slot_id, recovered.slot_id);
        assert_eq!(original.slot_derivation, recovered.slot_derivation);
        assert_eq!(original.d, recovered.d);
        assert_eq!(original.commit().0, recovered.commit().0);
    }

    #[test]
    fn test_compliance_leaf_proto_rejects_missing_d() {
        let mut rng = rand::thread_rng();
        let proto = pb::ComplianceLeaf {
            address: Some(Address::dummy(&mut rng).into()),
            asset_id: Some(asset::Id(decaf377::Fq::from(999u64)).into()),
            d: vec![],
            slot_id: 0,
            slot_derivation: decaf377::Fq::from(123u64).to_bytes().to_vec(),
        };

        let err = ComplianceLeaf::try_from(proto).expect_err("missing d should fail");

        assert!(
            err.to_string().contains("missing d"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_compliance_leaf_proto_rejects_missing_slot_derivation() {
        let mut rng = rand::thread_rng();
        let slot_derivation = decaf377::Fq::from(123u64);
        let proto = pb::ComplianceLeaf {
            address: Some(Address::dummy(&mut rng).into()),
            asset_id: Some(asset::Id(decaf377::Fq::from(999u64)).into()),
            d: crate::derive_compliance_scalar(slot_derivation)
                .to_bytes()
                .to_vec(),
            slot_id: 0,
            slot_derivation: vec![],
        };

        let err = ComplianceLeaf::try_from(proto).expect_err("missing slot derivation should fail");

        assert!(
            err.to_string().contains("missing slot_derivation"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_compliance_leaf_proto_rejects_mismatched_d() {
        let mut rng = rand::thread_rng();
        let proto = pb::ComplianceLeaf {
            address: Some(Address::dummy(&mut rng).into()),
            asset_id: Some(asset::Id(decaf377::Fq::from(999u64)).into()),
            d: decaf377::Fq::from(456u64).to_bytes().to_vec(),
            slot_id: 0,
            slot_derivation: decaf377::Fq::from(123u64).to_bytes().to_vec(),
        };

        let err = ComplianceLeaf::try_from(proto).expect_err("mismatched d should fail");

        assert!(
            err.to_string().contains("does not match slot_derivation"),
            "unexpected error: {err:#}"
        );
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
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![IbcRoute::transfer("channel-0", "connection-0", "channel-7")],
            Some(IbcAssetOrigin {
                route: IbcRoute::transfer("channel-0", "connection-0", "channel-7"),
                base_denom: "uusd".to_string(),
            }),
            "ring-123".to_string(),
            ring_pk,
            "policy-abc".to_string(),
            "reader".to_string(),
            "document".to_string(),
        );

        let bytes = policy.to_bytes().unwrap();
        let recovered = AssetPolicy::from_bytes(&bytes).unwrap();

        assert_eq!(policy.params.dk_pub, recovered.params.dk_pub);
        assert_eq!(policy.params.threshold, recovered.params.threshold);
        assert_eq!(
            policy.params.allowed_ibc_routes,
            recovered.params.allowed_ibc_routes
        );
        assert_eq!(policy.params.ibc_origin, recovered.params.ibc_origin);
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
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![
                IbcRoute::transfer("channel-1", "connection-0", "channel-7"),
                IbcRoute::transfer("channel-2", "connection-1", "channel-8"),
            ],
            None,
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

    #[test]
    fn test_asset_policy_to_bytes_rejects_overlong_route_field() {
        let dk_pub = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);
        let policy = AssetPolicy::new(
            dk_pub,
            500,
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![IbcRoute::transfer(
                "x".repeat(usize::from(u16::MAX) + 1),
                "connection-0",
                "channel-7",
            )],
            None,
            "ring-id".to_string(),
            ring_pk,
            "pol-id".to_string(),
            "perm".to_string(),
            "res".to_string(),
        );

        let err = policy.to_bytes().expect_err("overlong route should fail");

        assert!(
            err.to_string().contains("local_channel too long"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_asset_policy_to_bytes_rejects_overlong_string() {
        let dk_pub = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);
        let policy = AssetPolicy::new(
            dk_pub,
            500,
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![],
            None,
            "r".repeat(usize::from(u16::MAX) + 1),
            ring_pk,
            "pol-id".to_string(),
            "perm".to_string(),
            "res".to_string(),
        );

        let err = policy.to_bytes().expect_err("overlong ring_id should fail");

        assert!(
            err.to_string().contains("ring_id too long"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_asset_policy_from_bytes_rejects_missing_storage_fields() {
        let dk_pub = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);
        let policy = AssetPolicy::new(
            dk_pub,
            500,
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![],
            None,
            "ring-id".to_string(),
            ring_pk,
            "pol-id".to_string(),
            "perm".to_string(),
            "res".to_string(),
        );
        let bytes = policy.to_bytes().unwrap();

        let err = AssetPolicy::from_bytes(&bytes[..80]).expect_err("truncated policy should fail");

        assert!(
            err.to_string().contains("invalid AssetPolicy length")
                || err.to_string().contains("missing allowed_ibc_routes count"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_asset_policy_proto_rejects_missing_required_fields() {
        let dk_pub = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);
        let mut proto: pb::AssetPolicy = AssetPolicy::new(
            dk_pub,
            500,
            DEFAULT_COMPLIANCE_SLOT_COUNT,
            vec![],
            None,
            "ring-id".to_string(),
            ring_pk,
            "pol-id".to_string(),
            "perm".to_string(),
            "res".to_string(),
        )
        .into();

        proto.dk_pub.clear();
        let err = AssetPolicy::try_from(proto.clone()).expect_err("missing dk_pub should fail");
        assert!(
            err.to_string().contains("missing dk_pub"),
            "unexpected error: {err:#}"
        );

        proto.dk_pub = dk_pub.vartime_compress().0.to_vec();
        proto.threshold.clear();
        let err = AssetPolicy::try_from(proto.clone()).expect_err("missing threshold should fail");
        assert!(
            err.to_string().contains("missing threshold"),
            "unexpected error: {err:#}"
        );

        proto.threshold = 500u128.to_le_bytes().to_vec();
        proto.ring_pk.clear();
        let err = AssetPolicy::try_from(proto).expect_err("missing ring_pk should fail");
        assert!(
            err.to_string().contains("missing ring_pk"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_default_unregulated_policy_uses_sink_keys() {
        let policy = AssetPolicy::default_unregulated();

        assert_eq!(
            policy.params.dk_pub,
            *crate::crypto::UNREGULATED_SINK_DK_PUB
        );
        assert_eq!(
            policy.ring.ring_pk,
            *crate::crypto::UNREGULATED_SINK_RING_PK
        );
        assert_eq!(policy.params.threshold, u128::MAX);
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
/// - **Transfer-input** (288 bytes): 1 EPK + c2_core + detection + core
/// - **Transfer-output** (544 bytes): 3 EPKs + 3 c2s + detection + core + ext + sext
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplianceCiphertext {
    /// Ephemeral public key EPK_1 = r_1 × G (all actions).
    pub epk_1: decaf377::Element,

    /// Ephemeral public key EPK_2 = r_2 × G (transfer-output only).
    pub epk_2: Option<decaf377::Element>,

    /// Ephemeral public key EPK_3 = r_3 × G (transfer-output only).
    pub epk_3: Option<decaf377::Element>,

    /// Encrypted seed for core tier (ElGamal envelope).
    pub c2_core: Fq,

    /// Encrypted seed for extension tier (transfer-output only).
    pub c2_ext: Option<Fq>,

    /// Encrypted seed for sender-extension tier (transfer-output only).
    pub c2_sext: Option<Fq>,

    /// Encrypted detection tier: [asset_id+flag (32 bytes), salt (32 bytes)].
    pub detection_tag: [u8; DETECTION_TAG_BYTES],

    /// Encrypted core data: amount + self address (96 bytes).
    pub encrypted_core: Vec<u8>,

    /// Encrypted extension: counterparty address for receiver (96 bytes, transfer-output only).
    pub encrypted_ext: Option<Vec<u8>>,

    /// Encrypted sender-extension: counterparty data for sender (96 bytes, transfer-output only).
    pub encrypted_sext: Option<Vec<u8>>,
}

impl ComplianceCiphertext {
    /// Serialize EPK_1 to bytes.
    pub fn epk_1_bytes(&self) -> [u8; 32] {
        self.epk_1.vartime_compress().0
    }

    /// Create a transfer-input ciphertext (detection + core only, 288 bytes).
    pub fn new_transfer_input(
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

    /// Create a transfer-output ciphertext (all tiers, 544 bytes).
    pub fn new_transfer_output(
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

    /// Whether this is a transfer-input ciphertext (no extension tiers).
    pub fn is_transfer_input(&self) -> bool {
        self.epk_2.is_none()
    }

    /// Serialize to bytes.
    ///
    /// Transfer-input (224): EPK_1 + c2_core + detection + core
    /// Transfer-output (544): EPK_1 + EPK_2 + EPK_3 + c2_core + c2_ext + c2_sext + detection + core + ext + sext
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

    /// Deserialize from bytes. Accepts transfer-input (288 bytes) or transfer-output (544 bytes) format.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let is_output = match bytes.len() {
            TRANSFER_INPUT_WIRE_BYTES => false,
            TRANSFER_OUTPUT_WIRE_BYTES => true,
            n => anyhow::bail!(
                "invalid ciphertext length: expected {} (transfer input) or {} (transfer output), got {}",
                TRANSFER_INPUT_WIRE_BYTES,
                TRANSFER_OUTPUT_WIRE_BYTES,
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

        // EPK_2 and EPK_3 (transfer-output only)
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

        // c2_ext and c2_sext (transfer-output only)
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

    /// Convert to transfer-output circuit public inputs (11 Fq).
    ///
    /// Returns `(epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, ciphertext_fqs)`
    /// where ciphertext_fqs = [detection:2][core:3][ext:3][sext:3] = 11 Fq.
    pub fn to_transfer_output_circuit_public_inputs(
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
            .expect("to_transfer_output_circuit_public_inputs called on transfer-input ciphertext");
        let epk_3 = self
            .epk_3
            .expect("to_transfer_output_circuit_public_inputs called on transfer-input ciphertext");
        let c2_ext = self
            .c2_ext
            .expect("to_transfer_output_circuit_public_inputs called on transfer-input ciphertext");
        let c2_sext = self
            .c2_sext
            .expect("to_transfer_output_circuit_public_inputs called on transfer-input ciphertext");
        let encrypted_ext = self
            .encrypted_ext
            .as_ref()
            .expect("to_transfer_output_circuit_public_inputs called on transfer-input ciphertext");
        let encrypted_sext = self
            .encrypted_sext
            .as_ref()
            .expect("to_transfer_output_circuit_public_inputs called on transfer-input ciphertext");

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

        debug_assert_eq!(ciphertext_fqs.len(), TRANSFER_OUTPUT_CIPHERTEXT_FQS);

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

    /// Convert to transfer-input circuit public inputs (5 Fq).
    ///
    /// Returns `(epk_1, c2_core, ciphertext_fqs)` where ciphertext_fqs
    /// = [detection:2][core:3] = 5 Fq.
    pub fn to_transfer_input_circuit_public_inputs(
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

        debug_assert_eq!(ciphertext_fqs.len(), TRANSFER_INPUT_CIPHERTEXT_FQS);

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
