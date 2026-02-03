//! User Compliance Key Hierarchy
//!
//! This module implements a hierarchical compliance key system that allows an issuer
//! (e.g., Orbis) to scan all transactions for a given date using a single daily key,
//! while maintaining wallet unlinkability.
//!
//! ## Key Naming Convention
//!
//! **User Compliance Keys:**
//! - `UCK`: User Compliance Key (Scalar, Secret, held by Orbis, per-user)
//! - `ACK`: Address Compliance Key = `UCK * B_d` (Point, Public, stored in Registry)
//! - `DCK`: Daily Compliance Key (Point for encryption, Scalar for decryption)
//!
//! **Issuer/Asset Keys (defined in compliance crate):**
//! - `MCK`: Master Compliance Key (Scalar, Issuer's master secret)
//! - `DK`: Detection Key (Scalar, per-asset, for detection + flagged decryption)
//!
//! ## Key Types
//!
//! User daily keys have two types for tiered encryption:
//! - `Core`: For amount + self address (shared with auditors)
//! - `Extension`: For counterparty address (full access only)
//!
//! Detection is handled separately by the issuer's DetectionKey (DK) defined in
//! the compliance crate. The detection tier is always encrypted to the issuer's
//! DK_pub, not to user daily keys.
//!
//! ## Mathematical Design
//!
//! **Variables:**
//! - `uck`: User Compliance Key (Scalar, Secret, held by Orbis)
//! - `t`: Date/Epoch (Public)
//! - `T`: Daily Tweak = `Hash(t)` (Scalar, Public)
//! - `d`: Wallet Diversifier (Public)
//! - `B_d`: Diversified Generator for `d` (Point, Public)
//! - `ACK`: Address Compliance Key = `uck * B_d` (Point, Public, stored in Registry)
//!
//! **Derivations:**
//! 1. **Daily Compliance Key (Secret):** `dck_t = uck + T`
//!    - Used by Orbis to decrypt transactions for date `t`
//! 2. **Daily Address Key (Public):** `DCK_pub = ACK + (T * B_d)`
//!    - Derived by Sender to encrypt to a specific wallet on a specific date
//!    - Proof: `DCK_pub = (uck * B_d) + (T * B_d) = (uck + T) * B_d = dck_t * B_d`

use ark_ff::Zero;
use decaf377::{Fq, Fr};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use penumbra_sdk_proto::penumbra::core::component::compliance::v1 as pb;
use penumbra_sdk_proto::DomainType;

use super::Diversifier;

pub const CVK_LEN_BYTES: usize = 32;
pub const SECONDS_PER_DAY: u64 = 86400;

/// Key type for tiered compliance encryption.
///
/// Each ciphertext part is encrypted with a different key type, enabling selective disclosure:
/// - Core: For amount + self address (shared with auditors)
/// - Extension: For counterparty address (full access only)
///
/// Note: Detection is handled by the issuer's DetectionKey (DK), not user daily keys.
/// The detection tier is always encrypted to the issuer's DK_pub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyType {
    /// Core key - used to encrypt/decrypt the core data (amount + self address).
    /// Shared with auditors who need transaction details.
    Core,
    /// Extension key - used to encrypt/decrypt the extension data (counterparty address).
    /// Only available with full UCK access.
    Extension,
}

/// Domain separator for core key derivation.
static CORE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.keytype.core").as_bytes(),
    )
});

/// Domain separator for extension key derivation.
static EXTENSION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.keytype.extension").as_bytes(),
    )
});

/// Converts a Unix timestamp (seconds) to a day index.
#[inline]
#[must_use]
pub const fn day_index(timestamp: u64) -> u64 {
    timestamp / SECONDS_PER_DAY
}

/// Derives the PUBLIC daily tweak scalar from a key type and date.
///
/// Computes `T = Hash(domain_keytype, date)` where:
/// - domain_keytype is the domain separator for the specific key type
/// - The hash is mapped to a scalar
///
/// This is used for deriving daily PUBLIC keys: `PK_day = ACK + T * B_d`
/// This tweak is PUBLIC - anyone can compute it from the date and key type.
pub fn derive_daily_tweak(key_type: KeyType, date: u64) -> Fr {
    let domain = match key_type {
        KeyType::Core => &*CORE_DOMAIN,
        KeyType::Extension => &*EXTENSION_DOMAIN,
    };

    let date_fq = Fq::from(date);
    // Use hash_2 with (date, 0) to match circuit implementation
    let tweak_fq = poseidon377::hash_2(domain, (date_fq, Fq::zero()));

    // Map Fq to Fr by taking bytes and reducing mod Fr's order
    let tweak_bytes = tweak_fq.to_bytes();
    Fr::from_le_bytes_mod_order(&tweak_bytes)
}

/// User Compliance Key (Secret).
///
/// Per-user secret scalar held by Orbis (derived from ring master).
/// Can derive Address Compliance Keys (ACK) and Daily Compliance Keys (DCK).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserComplianceKey(pub Fr);

impl UserComplianceKey {
    /// Create a new user compliance key from a scalar.
    pub fn new(scalar: Fr) -> Self {
        Self(scalar)
    }

    /// Generate a deterministic demo UCK for testing purposes.
    ///
    /// # WARNING
    /// This is ONLY for demo/testing purposes. In production, the UCK must be
    /// derived by Orbis from the ring master key.
    ///
    /// The demo UCK uses a fixed, predictable value that anyone can derive.
    /// This is insecure for real use but allows testing the compliance system
    /// without requiring actual key derivation infrastructure.
    ///
    /// # Security Notice
    /// - DO NOT use this in production
    /// - Real UCKs must be derived by Orbis
    /// - This predictable key provides NO security
    #[cfg(any(test, feature = "demo"))]
    pub fn demo() -> Self {
        // Fixed demo value - everyone can derive this
        Self::new(Fr::from(12345u64))
    }

    /// Derive a user-specific UCK from a spend key seed.
    ///
    /// This derives a deterministic UCK unique to each user based on their
    /// spend key seed bytes. This is used for demo purposes to show per-user
    /// compliance key isolation.
    ///
    /// # Arguments
    /// * `spend_seed` - The 32-byte spend key seed
    ///
    /// # Security Notice
    /// In production, UCKs would be derived by Orbis from the ring master.
    /// This derivation is for demo/testing to show per-user key isolation.
    #[cfg(any(test, feature = "demo"))]
    pub fn from_spend_seed(spend_seed: &[u8; 32]) -> Self {
        // Use a domain-separated hash to derive the UCK scalar
        // Note: blake2b personal must be <= 16 bytes
        let personal = b"penumbra_uck_der";
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(personal)
            .hash(spend_seed);
        let scalar = Fr::from_le_bytes_mod_order(hash.as_bytes());
        Self::new(scalar)
    }

    /// Derive the daily compliance key for a given key type and date.
    ///
    /// Computes: `dck_t = uck + Hash(key_type, t)`
    ///
    /// Different key types produce different daily keys for selective disclosure:
    /// - Core: For amount + self address
    /// - Extension: For counterparty address
    ///
    /// Note: Detection is handled by the issuer's DetectionKey (DK), not user daily keys.
    ///
    /// # SECURITY WARNING: Key Sharing
    ///
    /// The daily keys are additively derived: `dck = uck + T_keytype`.
    /// This means if you share `dck_core`, an attacker who knows the public
    /// tweaks `T_core` and `T_extension` can compute:
    /// `dck_extension = dck_core - T_core + T_extension`
    ///
    /// The tiered encryption still provides value because:
    /// - Without UCK, no keys can be derived
    /// - The issuer controls WHO gets which tier at the SERVICE level
    /// - The separate ciphertext parts enable future key isolation schemes
    pub fn derive_daily_key(&self, key_type: KeyType, date: u64) -> DailyComplianceKey {
        let tweak = derive_daily_tweak(key_type, date);
        DailyComplianceKey::new(self.0 + tweak, key_type)
    }

    /// Derive both daily keys (core and extension) for a given date.
    ///
    /// Returns a DailyKeySet containing core and extension keys.
    /// This is a convenience method for full access scenarios.
    ///
    /// Note: Detection is handled by the issuer's DetectionKey, not user daily keys.
    pub fn derive_daily_keys(&self, date: u64) -> DailyKeySet {
        DailyKeySet {
            core: self.derive_daily_key(KeyType::Core, date),
            extension: self.derive_daily_key(KeyType::Extension, date),
        }
    }

    /// Derive the address compliance key for a given diversifier.
    ///
    /// Computes: `ACK = uck * B_d`
    ///
    /// This is the public key stored in the compliance registry for an address.
    pub fn derive_address_key(&self, diversifier: &Diversifier) -> AddressComplianceKey {
        let diversified_generator = diversifier.diversified_generator();
        AddressComplianceKey(diversified_generator * self.0)
    }

    /// Access the inner scalar (use with caution - this is secret material).
    pub fn inner(&self) -> &Fr {
        &self.0
    }

    /// Serialize the UCK to bytes (for display/export).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

/// Daily Compliance Key (Secret).
///
/// Derived from the User Compliance Key for a specific key type and date.
/// Each key type can only decrypt its corresponding ciphertext part:
/// - Detection key: can decrypt detection_tag (asset_id)
/// - Core key: can decrypt encrypted_core (amount + self address)
/// - Extension key: can decrypt encrypted_extension (counterparty address)
///
/// This key is provided by Orbis to auditors for a specific date,
/// allowing them to scan transactions without access to the full UCK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DailyComplianceKey {
    scalar: Fr,
    key_type: KeyType,
}

impl DailyComplianceKey {
    /// Create a new daily compliance key from a scalar and key type.
    pub fn new(scalar: Fr, key_type: KeyType) -> Self {
        Self { scalar, key_type }
    }

    /// Get the key type of this daily key.
    pub fn key_type(&self) -> KeyType {
        self.key_type
    }

    /// Access the inner scalar (use with caution - this is secret material).
    pub fn inner(&self) -> &Fr {
        &self.scalar
    }

    /// Serialize the daily key to bytes (for export/sharing).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.scalar.to_bytes()
    }

    /// Deserialize a daily key from bytes with a specified key type.
    ///
    /// Note: The key type must be provided externally as it's not encoded in the bytes.
    /// This is intentional - when Orbis provides keys, they specify the type.
    pub fn from_bytes(bytes: &[u8; 32], key_type: KeyType) -> Self {
        let fr = Fr::from_le_bytes_mod_order(bytes);
        Self::new(fr, key_type)
    }
}

/// A set of daily keys (core and extension) for a given date.
///
/// This is a convenience struct for scenarios requiring full access.
/// For selective disclosure, individual DailyComplianceKey instances should be shared.
///
/// Note: Detection is handled by the issuer's DetectionKey, not user daily keys.
#[derive(Clone, Copy, Debug)]
pub struct DailyKeySet {
    pub core: DailyComplianceKey,
    pub extension: DailyComplianceKey,
}

impl DailyComplianceKey {
    /// Derive the daily public key for a specific wallet.
    ///
    /// Computes: `PK_day = dck_t * B_d = (uck + T) * B_d`
    ///
    /// This should match the result of `AddressComplianceKey::derive_daily_public_key`
    /// when called with the same key type.
    pub fn derive_public_key(&self, diversifier: &Diversifier) -> decaf377::Element {
        let diversified_generator = diversifier.diversified_generator();
        diversified_generator * self.scalar
    }
}

/// Address Compliance Key (Public).
///
/// This is the public compliance key for a specific address, derived as `ACK = msk * B_d`
/// where B_d is the diversified generator from the address. It is stored in the compliance
/// registry and can be used to derive daily public keys without knowledge of the master secret.
///
/// Note: Previously called `WalletComplianceKey`, but renamed to clarify that each key
/// is specific to a single address (via its diversifier), not a whole wallet.
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

    /// Derive the daily public key for this address on a given key type and date.
    ///
    /// Computes: `PK_day = ACK + (T_keytype * B_d)`
    ///
    /// This is the crucial method that allows encryption without knowing `msk`.
    /// It uses the public `ACK` and computes the key-type and date-specific offset.
    ///
    /// # Proof of Correctness
    /// ```text
    /// PK_day = ACK + (T_keytype * B_d)
    ///        = (msk * B_d) + (T_keytype * B_d)
    ///        = (msk + T_keytype) * B_d
    ///        = dmk_keytype * B_d
    /// ```
    pub fn derive_daily_public_key(
        &self,
        key_type: KeyType,
        date: u64,
        diversifier: &Diversifier,
    ) -> decaf377::Element {
        let tweak = derive_daily_tweak(key_type, date);
        let diversified_generator = diversifier.diversified_generator();

        // PK_day = ACK + (T * B_d)
        self.0 + (diversified_generator * tweak)
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
    use rand_core::OsRng;

    #[test]
    fn test_daily_key_derivation_consistency() {
        let mut rng = OsRng;

        // Create a user compliance key
        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);

        // Pick a date and diversifier
        let date = 19000u64; // Some day index
        let diversifier = Diversifier([1u8; 16]);

        // Test both key types (Core and Extension)
        for key_type in [KeyType::Core, KeyType::Extension] {
            // Derive via two paths:
            // Path 1: UCK -> DCK -> Public Key
            let dck = uck.derive_daily_key(key_type, date);
            let pk_via_dck = dck.derive_public_key(&diversifier);

            // Path 2: UCK -> ACK -> Daily Public Key
            let ack = uck.derive_address_key(&diversifier);
            let pk_via_ack = ack.derive_daily_public_key(key_type, date, &diversifier);

            // They must match!
            assert_eq!(
                pk_via_dck, pk_via_ack,
                "Daily public key derivation must be consistent between Orbis and sender paths for {:?}",
                key_type
            );
        }
    }

    #[test]
    fn test_address_key_serialization() {
        let mut rng = OsRng;

        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);
        let diversifier = Diversifier([2u8; 16]);

        let ack = uck.derive_address_key(&diversifier);

        // Round-trip through bytes
        let bytes = ack.to_bytes();
        let recovered = AddressComplianceKey::from_bytes(bytes).unwrap();

        assert_eq!(
            ack, recovered,
            "Serialization round-trip must preserve address compliance key"
        );
    }

    #[test]
    fn test_different_dates_produce_different_keys() {
        let mut rng = OsRng;

        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);
        let diversifier = Diversifier([3u8; 16]);
        let ack = uck.derive_address_key(&diversifier);

        let date1 = 19000u64;
        let date2 = 19001u64;

        // Test with Core key type (same behavior for all key types)
        let pk1 = ack.derive_daily_public_key(KeyType::Core, date1, &diversifier);
        let pk2 = ack.derive_daily_public_key(KeyType::Core, date2, &diversifier);

        assert_ne!(
            pk1, pk2,
            "Different dates must produce different public keys"
        );
    }

    #[test]
    fn test_different_key_types_produce_different_keys() {
        let mut rng = OsRng;

        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);
        let diversifier = Diversifier([4u8; 16]);
        let ack = uck.derive_address_key(&diversifier);
        let date = 19000u64;

        let pk_core = ack.derive_daily_public_key(KeyType::Core, date, &diversifier);
        let pk_extension = ack.derive_daily_public_key(KeyType::Extension, date, &diversifier);

        assert_ne!(pk_core, pk_extension, "Core and Extension keys must differ");
    }

    #[test]
    fn test_different_diversifiers_produce_different_address_keys() {
        let mut rng = OsRng;

        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);

        let div1 = Diversifier([1u8; 16]);
        let div2 = Diversifier([2u8; 16]);

        let ack1 = uck.derive_address_key(&div1);
        let ack2 = uck.derive_address_key(&div2);

        assert_ne!(
            ack1, ack2,
            "Different diversifiers must produce different address compliance keys"
        );
    }

    #[test]
    fn test_demo_uck_is_deterministic() {
        // Demo UCK should always produce the same value
        let demo1 = UserComplianceKey::demo();
        let demo2 = UserComplianceKey::demo();

        assert_eq!(demo1, demo2, "Demo UCK must be deterministic");
        assert_eq!(
            demo1.0,
            Fr::from(12345u64),
            "Demo UCK must use expected fixed value"
        );
    }

    #[test]
    fn test_uck_from_spend_seed() {
        // Test that UCK derivation from spend seed works and is deterministic
        let seed1 = [1u8; 32];
        let seed2 = [2u8; 32];

        let uck1a = UserComplianceKey::from_spend_seed(&seed1);
        let uck1b = UserComplianceKey::from_spend_seed(&seed1);
        let uck2 = UserComplianceKey::from_spend_seed(&seed2);

        // Same seed produces same UCK
        assert_eq!(uck1a, uck1b, "Same seed must produce same UCK");

        // Different seeds produce different UCKs
        assert_ne!(uck1a, uck2, "Different seeds must produce different UCKs");

        // UCK from seed should differ from demo UCK
        assert_ne!(
            uck1a,
            UserComplianceKey::demo(),
            "Seed-derived UCK should differ from demo"
        );
    }

    #[test]
    fn test_demo_uck_derives_consistent_ack() {
        let demo_uck = UserComplianceKey::demo();
        let diversifier = Diversifier([42u8; 16]);

        // Derive ACK twice - should be identical
        let ack1 = demo_uck.derive_address_key(&diversifier);
        let ack2 = demo_uck.derive_address_key(&diversifier);

        assert_eq!(ack1, ack2, "ACK derivation must be deterministic");
    }

    // Note: Detection tests are in the compliance crate (issuer_keys.rs)
    // since detection is now handled by the issuer's DetectionKey, not user daily keys.
}
