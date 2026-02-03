//! Issuer Compliance Key Hierarchy
//!
//! This module implements the issuer-side key hierarchy for per-asset compliance,
//! enabling threshold-based flagging and issuer detection of flagged transactions.
//!
//! ## Key Naming Convention
//!
//! **Issuer/Asset Keys:**
//! - `MCK`: Master Compliance Key (Scalar, Orbis secret, per-issuer)
//! - `MCK_pub`: MCK * G (Point, stored in asset leaf for future signature verification)
//! - `DK`: Detection Key (Scalar, per-asset, shared with issuer by Orbis)
//! - `DK_pub`: DK * G (Point, stored in asset leaf)
//!
//! ## Design Principles
//!
//! - **DK is standalone**: Not derived from MCK, allowing Orbis to share DK with issuer
//! - **MCK retained by Orbis**: Issuer never sees MCK
//! - **Per-asset isolation**: DK only works for the specific asset's detection tier
//! - **Selective disclosure**: Issuer sees all transfers (detection tier) but only
//!   gets full details (amount, addresses) for flagged transactions
//!
//! ## Detection Tier Structure
//!
//! The detection tier is a single 32-byte Fq element with the flag packed into
//! the high bits. Fq has order < 2^252, so bits 252-255 are always zero for
//! valid field elements. We use bit 252 (0x10 in the high byte) to encode the flag.
//!
//! ```text
//! | Bits       | Content                                    |
//! |------------|--------------------------------------------|
//! | 0-251      | Asset identifier (Fq value)                |
//! | 252        | Flag: 0 = not flagged, 1 = flagged         |
//! | 253-255    | Unused (always 0)                          |
//! ```
//!
//! The detection tier is always encrypted to the issuer's DK_pub, allowing
//! issuers to scan for all transfers of their asset.

use ark_ff::Zero;
use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use penumbra_sdk_asset::asset;

/// Domain separator for detection tier encryption seed derivation.
/// Must match ISSUER_DETECTION_DOMAIN in crypto.rs for encryption/decryption compatibility.
static DETECTION_TIER_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.issuer_detection").as_bytes(),
    )
});

/// Size of the detection tier in bytes (1 Fq element = 32 bytes).
/// The flag is packed into the high bits of the Fq element.
pub const DETECTION_TIER_BYTES: usize = 32;

/// Bit mask for the flag in the high byte (bit 252 in LE representation).
/// Fq order is < 2^252, so this bit is always 0 for valid asset IDs.
pub const FLAG_BIT_MASK: u8 = 0x10;

/// Master Compliance Key (Orbis Secret).
///
/// Per-issuer master secret key held by Orbis. Used for:
/// - Future signature verification of policy updates
/// - Deriving asset-specific keys (if needed)
///
/// Note: MCK is NOT currently used for detection. Detection uses DK directly.
/// MCK_pub is stored in the asset leaf for future signature verification.
/// The issuer never sees MCK - only Orbis holds this secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MasterComplianceKey(pub Fr);

impl MasterComplianceKey {
    /// Create a new master compliance key from a scalar.
    pub fn new(scalar: Fr) -> Self {
        Self(scalar)
    }

    /// Generate a deterministic demo MCK for testing.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn demo() -> Self {
        Self::new(Fr::from(99999u64))
    }

    /// Derive MCK from a seed (for deterministic testing).
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let personal = b"penumbra_mck_der";
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(personal)
            .hash(seed);
        let scalar = Fr::from_le_bytes_mod_order(hash.as_bytes());
        Self::new(scalar)
    }

    /// Derive the public key (MCK_pub = MCK * G).
    ///
    /// This is stored in the asset leaf for future signature verification.
    pub fn public_key(&self) -> Element {
        Element::GENERATOR * self.0
    }

    /// Access the inner scalar (use with caution - this is secret material).
    pub fn inner(&self) -> &Fr {
        &self.0
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let scalar = Fr::from_le_bytes_mod_order(bytes);
        Self::new(scalar)
    }
}

/// Detection Key (Per-Asset Secret, Shared with Issuer).
///
/// Per-asset secret key shared by Orbis with the issuer. Used for:
/// - Scanning: Decrypting the detection tier to identify transfers of this asset
/// - Flagged decryption: Decrypting core+extension data for flagged transactions
///
/// **Important**: DK is standalone (not derived from MCK), allowing Orbis to
/// share DK with issuers without exposing MCK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectionKey(pub Fr);

impl DetectionKey {
    /// Create a new detection key from a scalar.
    pub fn new(scalar: Fr) -> Self {
        Self(scalar)
    }

    /// Generate a deterministic demo DK for testing.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn demo() -> Self {
        Self::new(Fr::from(88888u64))
    }

    /// Generate a demo DK for a specific asset (deterministic).
    ///
    /// This allows different assets to have different DKs in tests.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn demo_for_asset(asset_id: &asset::Id) -> Self {
        let personal = b"penumbra_dk_demo";
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(personal)
            .hash(&asset_id.0.to_bytes());
        let scalar = Fr::from_le_bytes_mod_order(hash.as_bytes());
        Self::new(scalar)
    }

    /// Derive DK from a seed (for deterministic testing).
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let personal = b"penumbra_dk_seed";
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(personal)
            .hash(seed);
        let scalar = Fr::from_le_bytes_mod_order(hash.as_bytes());
        Self::new(scalar)
    }

    /// Derive the public key (DK_pub = DK * G).
    ///
    /// This is stored in the asset leaf for encryption.
    pub fn public_key(&self) -> Element {
        Element::GENERATOR * self.0
    }

    /// Access the inner scalar (use with caution - this is secret material).
    pub fn inner(&self) -> &Fr {
        &self.0
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let scalar = Fr::from_le_bytes_mod_order(bytes);
        Self::new(scalar)
    }

    /// Try to decrypt the detection tier of a compliance ciphertext.
    ///
    /// The detection tier is a single 32-byte Fq element with the flag packed
    /// in bit 252 (the high bits that are unused by valid Fq elements).
    ///
    /// # Arguments
    /// * `epk` - The ephemeral public key on diversified curve (r * B_d, for seed derivation)
    /// * `epk_g` - The ephemeral public key on standard curve (r * G, for shared secret)
    /// * `detection_ciphertext` - The 32-byte detection tier ciphertext
    ///
    /// # Returns
    /// * `Ok((asset_id, is_flagged))` if decryption succeeds
    /// * `Err(_)` if decryption fails
    pub fn try_decrypt_detection(
        &self,
        epk: &Element,
        epk_g: &Element,
        detection_ciphertext: &[u8; DETECTION_TIER_BYTES],
    ) -> anyhow::Result<(asset::Id, bool)> {
        // 1. Compute shared secret using standard curve EPK: S = dk * epk_g
        //    This matches encryption: ss_issuer = r × DK_pub = r × dk × G
        let shared_secret = *epk_g * self.0;

        // 2. Derive Poseidon stream cipher seed using DIVERSIFIED curve EPK
        //    This must match encryption which uses epk (not epk_g) for the seed
        let shared_secret_fq = shared_secret.vartime_compress_to_field();
        let epk_fq = epk.vartime_compress_to_field();
        let seed = poseidon377::hash_2(&*DETECTION_TIER_DOMAIN, (shared_secret_fq, epk_fq));

        // 3. Decrypt the single Fq element (asset_id with flag in high bits)
        let ct_fq = Fq::from_le_bytes_mod_order(detection_ciphertext);
        let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
        let plaintext_fq = ct_fq - keystream;

        // 4. Convert to bytes and extract using DetectionTierPlaintext
        let plaintext_bytes = plaintext_fq.to_bytes();
        let detection_plaintext = DetectionTierPlaintext::from_bytes(&plaintext_bytes)?;

        Ok((detection_plaintext.asset_id, detection_plaintext.is_flagged))
    }

    /// Encrypt data to this detection key's public key.
    ///
    /// This is used to encrypt the detection tier and (for flagged TXs) the core+extension.
    ///
    /// # Arguments
    /// * `rng` - Random number generator
    /// * `plaintext` - The data to encrypt (must be multiple of 32 bytes)
    ///
    /// # Returns
    /// * `(ciphertext, ephemeral_public_key)` - The encrypted data and EPK
    pub fn encrypt_to_public<R: rand_core::RngCore + rand_core::CryptoRng>(
        &self,
        rng: &mut R,
        plaintext: &[u8],
    ) -> ([u8; DETECTION_TIER_BYTES], Element) {
        self.encrypt_to_public_inner(rng, plaintext, None)
    }

    /// Encrypt data to a specific public key (for encryption without holding DK).
    ///
    /// This allows senders to encrypt to the issuer's DK_pub without knowing DK.
    pub fn encrypt_to_dk_pub<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
        dk_pub: &Element,
        plaintext: &[u8],
    ) -> ([u8; DETECTION_TIER_BYTES], Element) {
        // Generate ephemeral secret
        let ephemeral_secret = Fr::rand(rng);
        let epk = Element::GENERATOR * ephemeral_secret;

        // Compute shared secret: S = r * DK_pub
        let shared_secret = *dk_pub * ephemeral_secret;

        // Derive seed
        let shared_secret_fq = shared_secret.vartime_compress_to_field();
        let epk_fq = epk.vartime_compress_to_field();
        let seed = poseidon377::hash_2(&*DETECTION_TIER_DOMAIN, (shared_secret_fq, epk_fq));

        // Encrypt plaintext (single Fq element)
        assert!(
            plaintext.len() == DETECTION_TIER_BYTES,
            "plaintext must be {} bytes, got {}",
            DETECTION_TIER_BYTES,
            plaintext.len()
        );

        let pt_bytes: [u8; 32] = plaintext.try_into().expect("slice is 32 bytes");
        let pt_fq = Fq::from_le_bytes_mod_order(&pt_bytes);
        let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
        let ct_fq = pt_fq + keystream;

        (ct_fq.to_bytes(), epk)
    }

    /// Internal encryption helper.
    fn encrypt_to_public_inner<R: rand_core::RngCore + rand_core::CryptoRng>(
        &self,
        rng: &mut R,
        plaintext: &[u8],
        _diversifier: Option<&[u8; 16]>,
    ) -> ([u8; DETECTION_TIER_BYTES], Element) {
        Self::encrypt_to_dk_pub(rng, &self.public_key(), plaintext)
    }
}

/// Detection tier plaintext structure.
///
/// This is what gets encrypted in the detection tier of compliance ciphertexts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectionTierPlaintext {
    /// The asset identifier.
    pub asset_id: asset::Id,
    /// Whether this transaction is flagged (threshold exceeded, not whitelisted).
    pub is_flagged: bool,
}

impl DetectionTierPlaintext {
    /// Create a new detection tier plaintext.
    pub fn new(asset_id: asset::Id, is_flagged: bool) -> Self {
        Self {
            asset_id,
            is_flagged,
        }
    }

    /// Serialize to bytes (32 bytes: asset_id with flag packed in high bits).
    ///
    /// The flag is stored in bit 252 (the 0x10 bit of the high byte in LE representation).
    /// This is safe because Fq order is < 2^252, so valid asset IDs never use this bit.
    pub fn to_bytes(&self) -> [u8; DETECTION_TIER_BYTES] {
        let mut bytes = self.asset_id.0.to_bytes();

        // Pack flag into bit 252 (high byte, bit 4 in LE)
        if self.is_flagged {
            bytes[31] |= FLAG_BIT_MASK;
        }

        bytes
    }

    /// Deserialize from bytes.
    ///
    /// Extracts the flag from bit 252 and clears it to recover the asset_id.
    pub fn from_bytes(bytes: &[u8; DETECTION_TIER_BYTES]) -> anyhow::Result<Self> {
        // Extract flag from bit 252
        let is_flagged = (bytes[31] & FLAG_BIT_MASK) != 0;

        // Clear the flag bit to recover the original asset_id
        let mut asset_bytes = *bytes;
        asset_bytes[31] &= !FLAG_BIT_MASK;

        let asset_id_fq = Fq::from_le_bytes_mod_order(&asset_bytes);
        let asset_id = asset::Id(asset_id_fq);

        Ok(Self {
            asset_id,
            is_flagged,
        })
    }
}

/// Detection Key Public (Point).
///
/// The public component of the detection key, stored in the asset leaf.
/// This is what senders encrypt the detection tier to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectionKeyPublic(pub Element);

impl DetectionKeyPublic {
    /// Create from an element.
    pub fn new(point: Element) -> Self {
        Self(point)
    }

    /// Create from a DetectionKey.
    pub fn from_dk(dk: &DetectionKey) -> Self {
        Self(dk.public_key())
    }

    /// Access the inner element.
    pub fn inner(&self) -> &Element {
        &self.0
    }

    /// Serialize to bytes (compressed point encoding).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.vartime_compress().0
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> anyhow::Result<Self> {
        let point = decaf377::Encoding(bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid detection key public bytes"))?;
        Ok(Self(point))
    }
}

/// Master Compliance Key Public (Point).
///
/// The public component of the master compliance key, stored in the asset leaf.
/// Used for future signature verification of policy updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MasterComplianceKeyPublic(pub Element);

impl MasterComplianceKeyPublic {
    /// Create from an element.
    pub fn new(point: Element) -> Self {
        Self(point)
    }

    /// Create from a MasterComplianceKey.
    pub fn from_mck(mck: &MasterComplianceKey) -> Self {
        Self(mck.public_key())
    }

    /// Access the inner element.
    pub fn inner(&self) -> &Element {
        &self.0
    }

    /// Serialize to bytes (compressed point encoding).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.vartime_compress().0
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> anyhow::Result<Self> {
        let point = decaf377::Encoding(bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid master compliance key public bytes"))?;
        Ok(Self(point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn test_mck_basic() {
        let mck = MasterComplianceKey::demo();
        let mck_pub = mck.public_key();

        // Verify public key is derived correctly
        assert_eq!(mck_pub, Element::GENERATOR * mck.0);

        // Round-trip through bytes
        let bytes = mck.to_bytes();
        let recovered = MasterComplianceKey::from_bytes(&bytes);
        assert_eq!(mck, recovered);
    }

    #[test]
    fn test_dk_basic() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        // Verify public key is derived correctly
        assert_eq!(dk_pub, Element::GENERATOR * dk.0);

        // Round-trip through bytes
        let bytes = dk.to_bytes();
        let recovered = DetectionKey::from_bytes(&bytes);
        assert_eq!(dk, recovered);
    }

    #[test]
    fn test_dk_per_asset_isolation() {
        let asset1 = asset::Id(Fq::from(100u64));
        let asset2 = asset::Id(Fq::from(200u64));

        let dk1 = DetectionKey::demo_for_asset(&asset1);
        let dk2 = DetectionKey::demo_for_asset(&asset2);

        // Different assets get different DKs
        assert_ne!(dk1, dk2);

        // Same asset gets same DK (deterministic)
        let dk1_again = DetectionKey::demo_for_asset(&asset1);
        assert_eq!(dk1, dk1_again);
    }

    #[test]
    fn test_detection_tier_roundtrip() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let asset_id = asset::Id(Fq::from(12345u64));
        let plaintext = DetectionTierPlaintext::new(asset_id, false);
        let plaintext_bytes = plaintext.to_bytes();

        // Encrypt
        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &plaintext_bytes);

        // Decrypt (standalone encryption uses standard curve, so epk = epk_g)
        let (decrypted_asset, decrypted_flag) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id);
        assert!(!decrypted_flag);
    }

    #[test]
    fn test_detection_tier_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let asset_id = asset::Id(Fq::from(99999u64));
        let plaintext = DetectionTierPlaintext::new(asset_id, true); // flagged
        let plaintext_bytes = plaintext.to_bytes();

        // Encrypt
        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &plaintext_bytes);

        // Decrypt (standalone encryption uses standard curve, so epk = epk_g)
        let (decrypted_asset, decrypted_flag) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id);
        assert!(decrypted_flag);
    }

    #[test]
    fn test_encrypt_to_dk_pub_without_dk() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let asset_id = asset::Id(Fq::from(55555u64));
        let plaintext = DetectionTierPlaintext::new(asset_id, false);
        let plaintext_bytes = plaintext.to_bytes();

        // Encrypt using only the public key (sender doesn't have DK)
        let (ciphertext, epk) =
            DetectionKey::encrypt_to_dk_pub(&mut rng, &dk_pub, &plaintext_bytes);

        // Issuer with DK can decrypt (standalone encryption uses standard curve, so epk = epk_g)
        let (decrypted_asset, decrypted_flag) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id);
        assert!(!decrypted_flag);
    }

    #[test]
    fn test_wrong_dk_cannot_decrypt() {
        let mut rng = OsRng;
        let dk1 = DetectionKey::demo();
        let dk2 = DetectionKey::from_seed(&[1u8; 32]); // Different key

        let asset_id = asset::Id(Fq::from(77777u64));
        let plaintext = DetectionTierPlaintext::new(asset_id, false);
        let plaintext_bytes = plaintext.to_bytes();

        // Encrypt to dk1
        let (ciphertext, epk) = dk1.encrypt_to_public(&mut rng, &plaintext_bytes);

        // Try to decrypt with dk2 - will produce garbage (standalone uses standard curve, so epk = epk_g)
        let result = dk2.try_decrypt_detection(&epk, &epk, &ciphertext);

        // Decryption "succeeds" but produces wrong data
        // (In practice, the garbage would fail asset_id matching)
        if let Ok((wrong_asset, _)) = result {
            assert_ne!(wrong_asset, asset_id);
        }
    }

    #[test]
    fn test_detection_tier_plaintext_serialization() {
        let asset_id = asset::Id(Fq::from(11111u64));

        // Not flagged
        let pt1 = DetectionTierPlaintext::new(asset_id, false);
        let bytes1 = pt1.to_bytes();
        let recovered1 = DetectionTierPlaintext::from_bytes(&bytes1).unwrap();
        assert_eq!(pt1, recovered1);

        // Flagged
        let pt2 = DetectionTierPlaintext::new(asset_id, true);
        let bytes2 = pt2.to_bytes();
        let recovered2 = DetectionTierPlaintext::from_bytes(&bytes2).unwrap();
        assert_eq!(pt2, recovered2);

        // Different flag values produce different bytes
        assert_ne!(bytes1, bytes2);
    }

    #[test]
    fn test_detection_key_public_roundtrip() {
        let dk = DetectionKey::demo();
        let dk_pub = DetectionKeyPublic::from_dk(&dk);

        let bytes = dk_pub.to_bytes();
        let recovered = DetectionKeyPublic::from_bytes(bytes).unwrap();

        assert_eq!(dk_pub, recovered);
    }

    #[test]
    fn test_mck_public_roundtrip() {
        let mck = MasterComplianceKey::demo();
        let mck_pub = MasterComplianceKeyPublic::from_mck(&mck);

        let bytes = mck_pub.to_bytes();
        let recovered = MasterComplianceKeyPublic::from_bytes(bytes).unwrap();

        assert_eq!(mck_pub, recovered);
    }

    #[test]
    fn test_mck_and_dk_are_independent() {
        // Verify MCK and DK are not derived from each other
        let mck = MasterComplianceKey::demo();
        let dk = DetectionKey::demo();

        // They should have different scalars
        assert_ne!(mck.0, dk.0);

        // And different public keys
        assert_ne!(mck.public_key(), dk.public_key());
    }

    #[test]
    fn test_flag_packing_in_high_bits() {
        // Verify that the flag is correctly packed in bit 252
        let asset_id = asset::Id(Fq::from(12345u64));

        // Not flagged - high byte should be 0 (assuming asset_id doesn't use high bits)
        let pt_not_flagged = DetectionTierPlaintext::new(asset_id, false);
        let bytes_not_flagged = pt_not_flagged.to_bytes();
        assert_eq!(bytes_not_flagged[31] & FLAG_BIT_MASK, 0);

        // Flagged - high byte should have FLAG_BIT_MASK set
        let pt_flagged = DetectionTierPlaintext::new(asset_id, true);
        let bytes_flagged = pt_flagged.to_bytes();
        assert_ne!(bytes_flagged[31] & FLAG_BIT_MASK, 0);

        // Asset IDs should be recovered correctly in both cases
        let recovered_not_flagged = DetectionTierPlaintext::from_bytes(&bytes_not_flagged).unwrap();
        let recovered_flagged = DetectionTierPlaintext::from_bytes(&bytes_flagged).unwrap();

        assert_eq!(recovered_not_flagged.asset_id, asset_id);
        assert!(!recovered_not_flagged.is_flagged);
        assert_eq!(recovered_flagged.asset_id, asset_id);
        assert!(recovered_flagged.is_flagged);
    }

    #[test]
    fn test_flag_does_not_affect_asset_id_recovery() {
        // Use a variety of asset IDs to ensure the flag bit doesn't interfere
        let asset_ids = [
            asset::Id(Fq::from(0u64)),
            asset::Id(Fq::from(1u64)),
            asset::Id(Fq::from(u64::MAX)),
            asset::Id(Fq::from(12345678901234567890u128)),
        ];

        for asset_id in asset_ids {
            for is_flagged in [false, true] {
                let pt = DetectionTierPlaintext::new(asset_id, is_flagged);
                let bytes = pt.to_bytes();
                let recovered = DetectionTierPlaintext::from_bytes(&bytes).unwrap();

                assert_eq!(recovered.asset_id, asset_id, "Asset ID mismatch");
                assert_eq!(
                    recovered.is_flagged, is_flagged,
                    "Flag mismatch for asset {:?}",
                    asset_id
                );
            }
        }
    }
}
