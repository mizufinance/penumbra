use decaf377::Fq;
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::{ensure_nonidentity_spend_auth_key, Address};
use shieldd_sdk_proto::shieldd::core::component::compliance::v1 as pb;
use shieldd_sdk_proto::DomainType;
use shieldd_sdk_tct::StateCommitment;

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
pub const DETECTION_TAG_BYTES: usize = 128; // 4 Fq elements: asset, salt, flag, reserved zero
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

const ASSET_REGISTRATION_GRANT_DOMAIN: &[u8] = b"shieldd.compliance.asset_registration_grant.v1";
const USER_REGISTRATION_GRANT_DOMAIN: &[u8] = b"shieldd.compliance.user_registration_grant.v1";

fn grant_signing_bytes(domain: &[u8], body_bytes: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(domain.len() + 1 + body_bytes.len());
    bytes.extend_from_slice(domain);
    bytes.push(0);
    bytes.extend_from_slice(&body_bytes);
    bytes
}

pub const FQ_BYTES: usize = 32;

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
        TRANSFER_INPUT_CIPHERTEXT_FQS == 7,
        "TRANSFER_INPUT_CIPHERTEXT_FQS must be 7"
    );
    assert!(
        TRANSFER_OUTPUT_CIPHERTEXT_FQS == 13,
        "TRANSFER_OUTPUT_CIPHERTEXT_FQS must be 13"
    );
};

/// The domain separator used to generate compliance leaf commitments.
pub(crate) static COMPLIANCE_LEAF_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.compliance.leaf.v5").as_bytes())
});

/// Authorization state for one address and regulated asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserAssetStatus {
    Active,
    Frozen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserAssetStatusAction {
    Freeze,
    Unfreeze,
}

impl UserAssetStatus {
    pub fn as_field(self) -> Fq {
        Fq::from(match self {
            Self::Active => 1u64,
            Self::Frozen => 2u64,
        })
    }

    pub fn validate_transition(self, next: Self) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(
                (self, next),
                (Self::Active, Self::Frozen) | (Self::Frozen, Self::Active)
            ),
            "illegal user asset status transition {self:?} -> {next:?}"
        );
        Ok(())
    }

    pub fn freeze(self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self == Self::Active,
            "only an active user asset can be frozen"
        );
        Ok(Self::Frozen)
    }

    pub fn unfreeze(self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self == Self::Frozen,
            "only a frozen user asset can be unfrozen"
        );
        Ok(Self::Active)
    }
}

impl UserAssetStatusAction {
    pub fn apply(self, current: UserAssetStatus) -> anyhow::Result<UserAssetStatus> {
        match self {
            Self::Freeze => current.freeze(),
            Self::Unfreeze => current.unfreeze(),
        }
    }
}

impl TryFrom<i32> for UserAssetStatus {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match pb::UserAssetStatus::try_from(value) {
            Ok(pb::UserAssetStatus::Active) => Ok(Self::Active),
            Ok(pb::UserAssetStatus::Frozen) => Ok(Self::Frozen),
            Ok(pb::UserAssetStatus::Unspecified) => {
                anyhow::bail!("user asset status must be specified")
            }
            Err(_) => anyhow::bail!("unknown user asset status {value}"),
        }
    }
}

impl From<UserAssetStatus> for pb::UserAssetStatus {
    fn from(value: UserAssetStatus) -> Self {
        match value {
            UserAssetStatus::Active => Self::Active,
            UserAssetStatus::Frozen => Self::Frozen,
        }
    }
}

/// A compliance leaf in the public on-chain registry for regulated assets.
///
/// Contains the full address, asset ID, status, and its ordinary-Orbis derivation scalar `d`.
/// `d = SHA512("elgamal-derivation-v1\0\0" || address.to_vec())` reduced into `Fr`.
/// ACK = d × ring_pk, computed in-circuit from the leaf's `d` value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::ComplianceLeaf", into = "pb::ComplianceLeaf")]
pub struct ComplianceLeaf {
    /// The registered address for compliance.
    pub address: Address,
    /// The asset ID this compliance leaf applies to.
    pub asset_id: asset::Id,
    /// Derivation scalar verified from the full canonical address at registration.
    pub d: Fq,
    /// Current authorization state for this address and asset.
    pub status: UserAssetStatus,
}

fn validate_derivation_scalar(d: Fq, expected: Fq) -> anyhow::Result<()> {
    anyhow::ensure!(
        expected != Fq::from(0u64),
        "compliance leaf derivation scalar must be nonzero"
    );
    anyhow::ensure!(
        d == expected,
        "compliance leaf d does not match the canonical address derivation"
    );
    Ok(())
}

impl ComplianceLeaf {
    /// Create a leaf using the one ordinary-Orbis derivation for this address.
    pub fn new(address: Address, asset_id: asset::Id) -> Self {
        let d = crate::derive_compliance_scalar(&address);
        Self {
            address,
            asset_id,
            d,
            status: UserAssetStatus::Active,
        }
    }

    /// Create the explicit synthetic leaf used only for unregulated asset proofs.
    pub fn synthetic_unregulated(address: Address, asset_id: asset::Id) -> Self {
        Self::new(address, asset_id)
    }

    /// Create a test-only leaf with explicitly supplied d.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_unchecked(address: Address, asset_id: asset::Id, d: Fq) -> Self {
        Self {
            address,
            asset_id,
            d,
            status: UserAssetStatus::Active,
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn with_status_for_test(mut self, status: UserAssetStatus) -> Self {
        self.status = status;
        self
    }

    pub fn validate_derivation(&self) -> anyhow::Result<()> {
        let expected = crate::derive_compliance_scalar(&self.address);
        validate_derivation_scalar(self.d, expected)
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

        let commit = poseidon377::hash_5(
            &COMPLIANCE_LEAF_DOMAIN_SEP,
            (
                diversified_generator,
                transmission_key_s,
                asset_id_field,
                self.d,
                self.status.as_field(),
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
        let address = value
            .address
            .ok_or_else(|| anyhow::anyhow!("missing address"))?
            .try_into()?;
        let leaf = ComplianceLeaf {
            address,
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            d,
            status: value.status.try_into()?,
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
            status: pb::UserAssetStatus::from(value.status) as i32,
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
    /// Vera policy ID.
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

const ASSET_POLICY_STORAGE_MAGIC: &[u8; 4] = b"AP3\0";

impl AssetPolicy {
    /// Create a new asset policy.
    pub fn new(
        dk_pub: decaf377::Element,
        threshold: u128,
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

    pub fn validate_crypto_keys(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.params.dk_pub.is_identity(),
            "asset policy detection key must not be the identity"
        );
        anyhow::ensure!(
            !self.ring.ring_pk.is_identity(),
            "asset policy ring key must not be the identity"
        );
        if let Some(registration_authority_vk) = &self.registration_authority_vk {
            ensure_nonidentity_spend_auth_key(
                registration_authority_vk,
                "compliance registration authority key",
            )?;
        }
        Ok(())
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
    /// Format starts with `AP3\0`; any other prefix fails closed.
    ///         [ring_id_len: 2] [ring_id bytes]
    ///         [policy_id_len: 2] [policy_id bytes]
    ///         [permission_len: 2] [permission bytes]
    ///         [resource_len: 2] [resource bytes]
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        self.validate_crypto_keys()?;
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(ASSET_POLICY_STORAGE_MAGIC);
        // AssetParams
        bytes.extend_from_slice(&self.params.dk_pub.vartime_compress().0);
        bytes.extend_from_slice(&self.params.threshold.to_le_bytes());
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
        if bytes.len() < ASSET_POLICY_STORAGE_MAGIC.len() + 80 {
            anyhow::bail!(
                "invalid AssetPolicy length: expected AP3 header and policy body, got {}",
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

        let policy = Self {
            params: AssetParams {
                dk_pub,
                threshold,
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
        };
        policy.validate_crypto_keys()?;
        Ok(policy)
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

        let policy = AssetPolicy {
            params: AssetParams {
                dk_pub,
                threshold,
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
        };
        policy.validate_crypto_keys()?;
        Ok(policy)
    }
}

impl From<AssetPolicy> for pb::AssetPolicy {
    fn from(value: AssetPolicy) -> pb::AssetPolicy {
        pb::AssetPolicy {
            dk_pub: value.params.dk_pub.vartime_compress().0.to_vec(),
            threshold: value.params.threshold.to_le_bytes().to_vec(),
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

    pub fn validate_authorization_keys(&self) -> anyhow::Result<()> {
        if let Some(registration_authority_vk) = &self.registration_authority_vk {
            ensure_nonidentity_spend_auth_key(
                registration_authority_vk,
                "compliance registration authority key",
            )?;
        }
        Ok(())
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

        let body = Self {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            is_regulated: value.is_regulated,
            dk_pub,
            threshold,
            allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
            ibc_origin,
            ring_pk,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            registration_authority_vk,
            valid_until_unix: value.valid_until_unix,
        };
        body.validate_authorization_keys()?;
        Ok(body)
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
        self.body.validate_authorization_keys()?;
        ensure_nonidentity_spend_auth_key(&self.registrar_vk, "compliance registrar key")?;
        self.registrar_vk
            .verify(&self.body.signing_bytes(), &self.signature)
            .map_err(|_| anyhow::anyhow!("asset registration grant signature failed to verify"))
    }
}

impl TryFrom<pb::AssetRegistrationGrant> for AssetRegistrationGrant {
    type Error = anyhow::Error;

    fn try_from(value: pb::AssetRegistrationGrant) -> Result<Self, Self::Error> {
        let grant = Self {
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
        };
        grant.body.validate_authorization_keys()?;
        ensure_nonidentity_spend_auth_key(&grant.registrar_vk, "compliance registrar key")?;
        Ok(grant)
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
        ensure_nonidentity_spend_auth_key(vk, "compliance registration authority key")?;
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
    /// Direct IBC routes allowed for this regulated asset. Empty = IBC blocked.
    pub allowed_ibc_routes: Vec<IbcRoute>,
    /// External IBC origin for regulated voucher assets.
    pub ibc_origin: Option<IbcAssetOrigin>,
    /// Orbis ring public key (optional).
    pub ring_pk: Option<decaf377::Element>,
    /// Orbis DKG ring identifier.
    pub ring_id: String,
    /// Vera policy ID.
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

        let message = MsgRegisterAsset {
            asset_id: value
                .asset_id
                .ok_or_else(|| anyhow::anyhow!("missing asset_id"))?
                .try_into()?,
            is_regulated: value.is_regulated,
            dk_pub,
            threshold,
            allowed_ibc_routes: canonical_routes(allowed_ibc_routes),
            ibc_origin,
            ring_pk,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            registration_authority_vk,
            asset_registration_grant,
        };
        message.validate_authorization_keys()?;
        Ok(message)
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
    pub fn validate_authorization_keys(&self) -> anyhow::Result<()> {
        if let Some(registration_authority_vk) = &self.registration_authority_vk {
            ensure_nonidentity_spend_auth_key(
                registration_authority_vk,
                "compliance registration authority key",
            )?;
        }
        if let Some(grant) = &self.asset_registration_grant {
            grant.body.validate_authorization_keys()?;
            ensure_nonidentity_spend_auth_key(&grant.registrar_vk, "compliance registrar key")?;
        }
        Ok(())
    }

    pub fn registration_grant_body(&self, valid_until_unix: u64) -> AssetRegistrationGrantBody {
        AssetRegistrationGrantBody {
            asset_id: self.asset_id,
            is_regulated: self.is_regulated,
            dk_pub: self.dk_pub,
            threshold: self.threshold,
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

impl shieldd_sdk_txhash::EffectingData for MsgRegisterAsset {
    fn effect_hash(&self) -> shieldd_sdk_txhash::EffectHash {
        shieldd_sdk_txhash::EffectHash::from_proto_effecting_data::<pb::MsgRegisterAsset>(
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

impl shieldd_sdk_txhash::EffectingData for MsgRegisterUser {
    fn effect_hash(&self) -> shieldd_sdk_txhash::EffectHash {
        shieldd_sdk_txhash::EffectHash::from_proto_effecting_data::<pb::MsgRegisterUser>(
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
        let leaf = ComplianceLeaf::new(address.clone(), asset_id);

        assert_eq!(leaf.address, address);
        assert_eq!(leaf.asset_id, asset_id);
        assert_eq!(leaf.d, crate::derive_compliance_scalar(&address));
    }

    #[test]
    fn test_compliance_leaf_different_addresses_different_commits() {
        let mut rng = rand::thread_rng();
        let asset_id = asset::Id(decaf377::Fq::from(100u64));
        let address1 = Address::dummy(&mut rng);
        let address2 = Address::dummy(&mut rng);

        let leaf1 = ComplianceLeaf::new(address1, asset_id);
        let leaf2 = ComplianceLeaf::new(address2, asset_id);

        assert_ne!(
            leaf1.d, leaf2.d,
            "Different owners use different audit keys"
        );
        assert_ne!(
            leaf1.commit(),
            leaf2.commit(),
            "Different owners must have different commitments"
        );
    }

    #[test]
    fn test_compliance_leaf_proto_roundtrip() {
        let mut rng = rand::thread_rng();
        let wallet = Address::dummy(&mut rng);
        let asset_id = asset::Id(decaf377::Fq::from(999u64));
        let original = ComplianceLeaf::new(wallet, asset_id);

        let proto: pb::ComplianceLeaf = original.clone().into();
        let recovered: ComplianceLeaf = proto.try_into().expect("should parse");

        assert_eq!(original.address, recovered.address);
        assert_eq!(original.asset_id, recovered.asset_id);
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
            status: pb::UserAssetStatus::Active as i32,
        };

        let err = ComplianceLeaf::try_from(proto).expect_err("missing d should fail");

        assert!(
            err.to_string().contains("missing d"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_compliance_leaf_proto_rejects_mismatched_d() {
        let mut rng = rand::thread_rng();
        let address = Address::dummy(&mut rng);
        let proto = pb::ComplianceLeaf {
            address: Some(address.into()),
            asset_id: Some(asset::Id(decaf377::Fq::from(999u64)).into()),
            d: decaf377::Fq::from(456u64).to_bytes().to_vec(),
            status: pb::UserAssetStatus::Active as i32,
        };

        let err = ComplianceLeaf::try_from(proto).expect_err("mismatched d should fail");

        assert!(
            err.to_string().contains("canonical address derivation"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_compliance_leaf_validation_rejects_zero_d() {
        let error = validate_derivation_scalar(decaf377::Fq::from(0u64), decaf377::Fq::from(0u64))
            .expect_err("zero d cannot be a valid registered compliance key");

        assert!(
            error.to_string().contains("must be nonzero"),
            "unexpected error: {error:#}"
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
    fn asset_policy_rejects_identity_crypto_keys() {
        let identity = decaf377::Element::IDENTITY;
        let generator = decaf377::Element::GENERATOR;

        let identity_dk = AssetPolicy::simple(identity, 1, generator);
        assert!(identity_dk.validate_crypto_keys().is_err());
        assert!(identity_dk.to_bytes().is_err());

        let identity_ring = AssetPolicy::simple(generator, 1, identity);
        assert!(identity_ring.validate_crypto_keys().is_err());
        assert!(identity_ring.to_bytes().is_err());

        let valid = AssetPolicy::simple(generator, 1, generator);
        let mut identity_dk_proto: pb::AssetPolicy = valid.clone().into();
        identity_dk_proto.dk_pub = identity.vartime_compress().0.to_vec();
        assert!(AssetPolicy::try_from(identity_dk_proto).is_err());

        let mut identity_ring_proto: pb::AssetPolicy = valid.into();
        identity_ring_proto.ring_pk = identity.vartime_compress().0.to_vec();
        assert!(AssetPolicy::try_from(identity_ring_proto).is_err());
    }

    #[test]
    fn asset_policy_rejects_identity_registration_authority_key() {
        let identity_signing_key =
            decaf377_rdsa::SigningKey::<SpendAuth>::from(decaf377::Fr::from(0u64));
        let identity = VerificationKey::from(&identity_signing_key);
        let policy = AssetPolicy::simple(
            decaf377::Element::GENERATOR,
            1,
            decaf377::Element::GENERATOR,
        )
        .with_registration_authority(identity);

        let error = policy
            .validate_crypto_keys()
            .expect_err("asset policies must reject identity registration authorities");
        assert!(
            error
                .to_string()
                .contains("compliance registration authority key must not be identity"),
            "unexpected rejection reason: {error:#}"
        );

        let proto: pb::AssetPolicy = policy.into();
        assert!(
            AssetPolicy::try_from(proto).is_err(),
            "typed policy decode must reject identity registration authorities"
        );
    }

    #[test]
    fn asset_registration_grant_rejects_identity_registrar_key() {
        let signing_key = decaf377_rdsa::SigningKey::<SpendAuth>::from(decaf377::Fr::from(0u64));
        let registrar_vk = VerificationKey::from(&signing_key);
        let body = AssetRegistrationGrantBody {
            asset_id: asset::Id(decaf377::Fq::from(1u64)),
            is_regulated: false,
            dk_pub: None,
            threshold: None,
            allowed_ibc_routes: Vec::new(),
            ibc_origin: None,
            ring_pk: None,
            ring_id: String::new(),
            policy_id: String::new(),
            permission: String::new(),
            resource: String::new(),
            registration_authority_vk: None,
            valid_until_unix: 1,
        };
        let signature = signing_key.sign_deterministic(&body.signing_bytes());
        let grant = AssetRegistrationGrant {
            body,
            registrar_vk,
            signature,
        };

        let error = grant
            .verify()
            .expect_err("asset registration grants must reject identity registrar keys");
        assert!(
            error
                .to_string()
                .contains("compliance registrar key must not be identity"),
            "unexpected rejection reason: {error:#}"
        );

        let proto: pb::AssetRegistrationGrant = grant.into();
        assert!(
            AssetRegistrationGrant::try_from(proto).is_err(),
            "typed grant decode must reject identity registrar keys"
        );
    }

    #[test]
    fn user_registration_grant_rejects_identity_registration_authority() {
        let signing_key = decaf377_rdsa::SigningKey::<SpendAuth>::from(decaf377::Fr::from(0u64));
        let registration_authority_vk = VerificationKey::from(&signing_key);
        let body = UserRegistrationGrantBody {
            leaf: ComplianceLeaf::new(
                Address::dummy(&mut rand::thread_rng()),
                asset::Id(decaf377::Fq::from(1u64)),
            ),
            policy_id: "policy".to_owned(),
            valid_until_unix: 1,
            nonce: vec![1],
        };
        let signature = signing_key.sign_deterministic(&body.signing_bytes());
        let grant = UserRegistrationGrant { body, signature };

        let error = grant
            .verify(&registration_authority_vk)
            .expect_err("user registration grants must reject identity authorities");
        assert!(
            error
                .to_string()
                .contains("compliance registration authority key must not be identity"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn test_asset_policy_proto_roundtrip() {
        let dk_pub = decaf377::Element::GENERATOR * decaf377::Fr::from(42u64);
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::from(999u64);

        let policy = AssetPolicy::new(
            dk_pub,
            500,
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

    #[test]
    fn merkle_path_default_is_fixed_width_and_canonical() {
        let path = MerklePath::default();
        path.validate().expect("default path is canonical");
        assert_eq!(path.layers.len(), usize::from(crate::tree::DEFAULT_DEPTH));
        assert!(path.layers.iter().all(|layer| {
            layer.siblings.len() == 3 && layer.siblings.iter().all(|sibling| sibling == &[0u8; 32])
        }));

        let proto: pb::MerklePath = path.clone().into();
        assert_eq!(
            MerklePath::try_from(proto).expect("canonical path roundtrip"),
            path
        );
    }

    #[test]
    fn merkle_path_proto_rejects_noncanonical_shape_and_fields() {
        let canonical: pb::MerklePath = MerklePath::default().into();

        let mut short = canonical.clone();
        short.layers.pop();
        MerklePath::try_from(short).expect_err("short path must fail");

        let mut wrong_arity = canonical.clone();
        wrong_arity.layers[0].siblings.pop();
        MerklePath::try_from(wrong_arity).expect_err("wrong sibling arity must fail");

        let mut wrong_length = canonical.clone();
        wrong_length.layers[0].siblings[0].pop();
        MerklePath::try_from(wrong_length).expect_err("short sibling must fail");

        let mut noncanonical = canonical;
        noncanonical.layers[0].siblings[0] = vec![0xff; 32];
        MerklePath::try_from(noncanonical).expect_err("noncanonical sibling must fail");
    }
}

/// A Merkle path in the Quad Merkle Tree (arity 4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::MerklePath", into = "pb::MerklePath")]
pub struct MerklePath {
    /// The layers of the Merkle path, from leaf to root.
    pub layers: Vec<MerklePathLayer>,
}

impl MerklePath {
    /// Fixed-width zero path for a conditionally disabled membership branch.
    pub fn zeroed() -> Self {
        Self {
            layers: (0..crate::tree::DEFAULT_DEPTH)
                .map(|_| MerklePathLayer {
                    siblings: vec![vec![0u8; 32]; 3],
                })
                .collect(),
        }
    }

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

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.layers.len() == usize::from(crate::tree::DEFAULT_DEPTH),
            "Merkle path must have exactly {} layers, got {}",
            crate::tree::DEFAULT_DEPTH,
            self.layers.len()
        );
        for (layer_index, layer) in self.layers.iter().enumerate() {
            anyhow::ensure!(
                layer.siblings.len() == 3,
                "Merkle path layer {layer_index} must have exactly 3 siblings, got {}",
                layer.siblings.len()
            );
            for (sibling_index, sibling) in layer.siblings.iter().enumerate() {
                let bytes: [u8; 32] = sibling.as_slice().try_into().map_err(|_| {
                    anyhow::anyhow!(
                        "Merkle path layer {layer_index} sibling {sibling_index} must be 32 bytes"
                    )
                })?;
                Fq::from_bytes_checked(&bytes).map_err(|_| {
                    anyhow::anyhow!(
                        "Merkle path layer {layer_index} sibling {sibling_index} is not canonical"
                    )
                })?;
            }
        }
        Ok(())
    }
}

impl Default for MerklePath {
    fn default() -> Self {
        Self::zeroed()
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
        let path = MerklePath { layers };
        path.validate()?;
        Ok(path)
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
