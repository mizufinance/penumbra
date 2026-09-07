//! Issuer Compliance Key Hierarchy
//!
//! This module implements the issuer-side key hierarchy for per-asset compliance.
//!
//! - `MCK`: Master Compliance Key (Orbis secret, per-issuer, for future signature verification)
//! - `DK`: Detection Key (per-asset, generated and held by the issuer for scanning and decryption)
//!
//! DK is standalone (not derived from MCK or any Orbis key). The issuer registers dk_pub on-chain.

use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use shieldd_sdk_asset::asset;

use crate::crypto::compliance_stream_block;

/// Domain separator for detection tier encryption seed derivation.
/// Must match ISSUER_DETECTION_DOMAIN in crypto.rs for encryption/decryption compatibility.
static DETECTION_TIER_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.compliance.issuer_detection").as_bytes(),
    )
});

/// Fixed detection tier: asset ID, salt, flagged bit, and reserved zero padding.
pub const DETECTION_TIER_BYTES: usize = 128;

pub(crate) fn detection_flag_plaintext(is_flagged: bool) -> Fq {
    Fq::from(u64::from(is_flagged))
}

pub(crate) fn detection_flag_from_fq(value: Fq) -> anyhow::Result<bool> {
    anyhow::ensure!(
        value == Fq::from(0u64) || value == Fq::from(1u64),
        "detection flag is not boolean"
    );
    Ok(value == Fq::from(1u64))
}

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
        let personal = b"shieldd_mck_der";
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

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let scalar = Fr::from_le_bytes_mod_order(bytes);
        Self::new(scalar)
    }
}

/// Detection Key (Per-Asset Secret, Held by Issuer).
///
/// Per-asset secret key generated and held by the issuer. Used for:
/// - Scanning: Decrypting the detection tier to identify transfers of this asset
/// - Flagged decryption: Decrypting core+extension data for flagged transactions
///
/// **Important**: DK is standalone (not derived from MCK or any Orbis key).
/// The issuer registers dk_pub on-chain; the private scalar never leaves the issuer.
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
        let personal = b"shieldd_dk_demo";
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
        let personal = b"shieldd_dk_seed";
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

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let scalar = Fr::from_le_bytes_mod_order(bytes);
        Self::new(scalar)
    }

    /// Try to decrypt the detection tier of a compliance ciphertext.
    ///
    /// Decrypts via Fq subtraction, requires the exact expected asset ID, and
    /// decodes the canonical flagged bit, and requires zero reserved padding.
    ///
    /// Returns `Ok((asset_id, is_flagged, salt))`
    /// if the decrypted value matches expected_asset_id,
    /// or `Err(_)` if decryption doesn't match (wrong key or wrong asset).
    pub fn try_decrypt_detection(
        &self,
        epk: &Element,
        detection_ciphertext: &[u8; DETECTION_TIER_BYTES],
        expected_asset_id: &asset::Id,
    ) -> anyhow::Result<(asset::Id, bool, Fq)> {
        // 1. Compute the shared secret from the serialized detection EPK.
        let shared_secret = *epk * self.0;

        // 2. Derive Poseidon stream cipher seed
        let shared_secret_fq = shared_secret.vartime_compress_to_field();
        let epk_fq = epk.vartime_compress_to_field();
        let seed = poseidon377::hash_2(&*DETECTION_TIER_DOMAIN, (shared_secret_fq, epk_fq));

        // 3. Decrypt via Fq subtraction: pt = ct - keystream
        // Detection tier layout:
        // [asset_id, salt, is_flagged, reserved_zero]
        let ct_fq = Fq::from_le_bytes_mod_order(&detection_ciphertext[..32]);
        let keystream = compliance_stream_block(seed, 0);
        let decrypted_asset_id = ct_fq - keystream;
        anyhow::ensure!(
            decrypted_asset_id == expected_asset_id.0,
            "detection tier does not match expected asset"
        );

        // 3b. Decrypt salt (second Fq element, counter=1)
        let ct_salt_fq = Fq::from_le_bytes_mod_order(&detection_ciphertext[32..64]);
        let keystream_salt = compliance_stream_block(seed, 1);
        let salt = ct_salt_fq - keystream_salt;

        let ct_flag = Fq::from_le_bytes_mod_order(&detection_ciphertext[64..96]);
        let is_flagged = detection_flag_from_fq(ct_flag - compliance_stream_block(seed, 2))?;
        let ct_reserved = Fq::from_le_bytes_mod_order(&detection_ciphertext[96..128]);
        anyhow::ensure!(
            ct_reserved - compliance_stream_block(seed, 3) == Fq::from(0u64),
            "detection reserved word is nonzero"
        );

        Ok((*expected_asset_id, is_flagged, salt))
    }

    /// Encrypt a detection tier for tests of issuer-side decoding.
    #[cfg(test)]
    fn encrypt_to_public<R: rand_core::RngCore + rand_core::CryptoRng>(
        &self,
        rng: &mut R,
        asset_id: &asset::Id,
        is_flagged: bool,
    ) -> ([u8; DETECTION_TIER_BYTES], Element) {
        Self::encrypt_to_dk_pub(rng, &self.public_key(), asset_id, is_flagged)
    }

    /// Encrypt detection tier to a specific public key (for encryption without holding DK).
    ///
    /// Uses the exact asset, salt, Boolean flag, and zero-reserved plaintext.
    #[cfg(test)]
    fn encrypt_to_dk_pub<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
        dk_pub: &Element,
        asset_id: &asset::Id,
        is_flagged: bool,
    ) -> ([u8; DETECTION_TIER_BYTES], Element) {
        let ephemeral_secret = Fr::rand(rng);
        let epk = Element::GENERATOR * ephemeral_secret;

        // Compute shared secret: S = r * DK_pub
        let shared_secret = *dk_pub * ephemeral_secret;

        // Derive seed
        let shared_secret_fq = shared_secret.vartime_compress_to_field();
        let epk_fq = epk.vartime_compress_to_field();
        let seed = poseidon377::hash_2(&*DETECTION_TIER_DOMAIN, (shared_secret_fq, epk_fq));

        let mut detection_bytes = [0u8; DETECTION_TIER_BYTES];
        let plaintext = [
            asset_id.0,
            Fq::from(0u64),
            detection_flag_plaintext(is_flagged),
            Fq::from(0u64),
        ];
        for (counter, (word, chunk)) in plaintext
            .into_iter()
            .zip(detection_bytes.chunks_exact_mut(32))
            .enumerate()
        {
            let keystream = compliance_stream_block(seed, counter as u64);
            chunk.copy_from_slice(&(word + keystream).to_bytes());
        }
        (detection_bytes, epk)
    }
}

/// Detection Key Public (Point).
///
/// The public component of the detection key, stored in the asset leaf.
/// This is what senders encrypt the detection tier to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectionKeyPublic(pub Element);

impl DetectionKeyPublic {
    pub fn new(point: Element) -> Self {
        Self(point)
    }

    pub fn from_dk(dk: &DetectionKey) -> Self {
        Self(dk.public_key())
    }

    pub fn inner(&self) -> &Element {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.vartime_compress().0
    }

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
    pub fn new(point: Element) -> Self {
        Self(point)
    }

    pub fn from_mck(mck: &MasterComplianceKey) -> Self {
        Self(mck.public_key())
    }

    pub fn inner(&self) -> &Element {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.vartime_compress().0
    }

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

        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, false);

        let (decrypted_asset, decrypted_flag, _salt) = dk
            .try_decrypt_detection(&epk, &ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id);
        assert!(!decrypted_flag);
    }

    #[test]
    fn test_detection_tier_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let asset_id = asset::Id(Fq::from(99999u64));

        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, true);

        let (decrypted_asset, decrypted_flag, _salt) = dk
            .try_decrypt_detection(&epk, &ciphertext, &asset_id)
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

        let (ciphertext, epk) =
            DetectionKey::encrypt_to_dk_pub(&mut rng, &dk_pub, &asset_id, false);

        let (decrypted_asset, decrypted_flag, _salt) = dk
            .try_decrypt_detection(&epk, &ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id);
        assert!(!decrypted_flag);
    }

    #[test]
    fn test_wrong_dk_cannot_decrypt() {
        let mut rng = OsRng;
        let dk1 = DetectionKey::demo();
        let dk2 = DetectionKey::from_seed(&[1u8; 32]);
        let asset_id = asset::Id(Fq::from(77777u64));

        let (ciphertext, epk) = dk1.encrypt_to_public(&mut rng, &asset_id, false);

        // Wrong DK → pt_fq won't match expected asset_id → Err
        let result = dk2.try_decrypt_detection(&epk, &ciphertext, &asset_id);
        assert!(
            result.is_err(),
            "wrong DK should fail to match expected asset"
        );
    }

    #[test]
    fn detection_flag_is_boolean_and_canonical() {
        assert!(!detection_flag_from_fq(detection_flag_plaintext(false)).unwrap());
        assert!(detection_flag_from_fq(detection_flag_plaintext(true)).unwrap());
        assert!(detection_flag_from_fq(Fq::from(2u64)).is_err());
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
        let mck = MasterComplianceKey::demo();
        let dk = DetectionKey::demo();
        assert_ne!(mck.0, dk.0);
        assert_ne!(mck.public_key(), dk.public_key());
    }

    #[test]
    fn test_flag_roundtrip_variety_of_asset_ids() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let asset_ids = [
            asset::Id(Fq::from(0u64)),
            asset::Id(Fq::from(1u64)),
            asset::Id(Fq::from(u64::MAX)),
            asset::Id(Fq::from(12345678901234567890u128)),
        ];

        for asset_id in asset_ids {
            for is_flagged in [false, true] {
                let (ct, epk) = dk.encrypt_to_public(&mut rng, &asset_id, is_flagged);
                let (dec_id, dec_flag, _salt) = dk
                    .try_decrypt_detection(&epk, &ct, &asset_id)
                    .expect("decryption should succeed");

                assert_eq!(dec_id, asset_id, "Asset ID mismatch");
                assert_eq!(
                    dec_flag, is_flagged,
                    "Flag mismatch for asset {:?}",
                    asset_id
                );
            }
        }
    }

    #[test]
    fn test_flag_survives_encrypt_decrypt_realistic_asset_id() {
        // Regression: the flag is independent of every asset-ID bit.
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let mut asset_bytes = [0u8; 32];
        asset_bytes[0] = 0x42;
        asset_bytes[31] = 0x05;
        let asset_id = asset::Id(Fq::from_le_bytes_mod_order(&asset_bytes));

        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, true);
        let (decrypted_asset, decrypted_flag, _salt) = dk
            .try_decrypt_detection(&epk, &ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id, "asset ID should match");
        assert!(decrypted_flag, "flag should survive for realistic asset ID");
    }

    #[test]
    fn test_no_false_positive_flag_for_asset_with_high_byte() {
        // Regression: high asset-ID bits cannot alias the separately packed flag.
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let mut asset_bytes = [0u8; 32];
        asset_bytes[0] = 0x01;
        asset_bytes[31] = 0x11;
        let asset_id = asset::Id(Fq::from_le_bytes_mod_order(&asset_bytes));

        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, false);
        let (_, decrypted_flag, _salt) = dk
            .try_decrypt_detection(&epk, &ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert!(
            !decrypted_flag,
            "unflagged TX should not be detected as flagged"
        );
    }

    #[test]
    fn high_bit_asset_alias_is_rejected() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let asset_id = asset::Id(Fq::from(41u64));
        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, true);

        let mut sentinel_bytes = [0u8; 32];
        sentinel_bytes[31] = 1 << 5;
        let alias = asset::Id(asset_id.0 + Fq::from_le_bytes_mod_order(&sentinel_bytes));

        assert!(
            dk.try_decrypt_detection(&epk, &ciphertext, &alias).is_err(),
            "ciphertext word 0 must bind the exact asset independently of the flag"
        );
    }
}
