//! Issuer Compliance Key Hierarchy
//!
//! This module implements the issuer-side key hierarchy for per-asset compliance.
//!
//! - `MCK`: Master Compliance Key (Orbis secret, per-issuer, for future signature verification)
//! - `DK`: Detection Key (per-asset, generated and held by the issuer for scanning and decryption)
//!
//! DK is standalone (not derived from MCK or any Orbis key). The issuer registers dk_pub on-chain.

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

/// Size of the detection tier in bytes (asset_id+flag, salt, sender slot, receiver slot).
pub const DETECTION_TIER_BYTES: usize = 128;

/// Sentinel Fq value for flagged transactions: `Fq::from(2^253)`.
/// Since the Fq modulus is ~2^252.6, this wraps to `2^253 - r` in the field.
/// This matches the R1CS circuit's flag_bit_value exactly (r1cs.rs:1122-1129).
pub static FLAG_SENTINEL: Lazy<Fq> = Lazy::new(|| {
    use ark_ff::{BigInteger, BigInteger256};
    let mut big = BigInteger256::from(1u64);
    for _ in 0..253 {
        big.mul2();
    }
    Fq::from(big)
});

/// Build the detection tier plaintext as an Fq element.
/// If flagged, adds FLAG_SENTINEL to the asset_id. Both native and circuit use this.
pub fn detection_plaintext_fq(asset_id: &asset::Id, is_flagged: bool) -> Fq {
    if is_flagged {
        asset_id.0 + *FLAG_SENTINEL
    } else {
        asset_id.0
    }
}

fn slot_id_from_fq(value: Fq, field: &str) -> anyhow::Result<u32> {
    let bytes = value.to_bytes();
    anyhow::ensure!(
        bytes[4..].iter().all(|byte| *byte == 0),
        "{field} is not a canonical u32 slot id"
    );
    Ok(u32::from_le_bytes(bytes[..4].try_into()?))
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

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let scalar = Fr::from_le_bytes_mod_order(bytes);
        Self::new(scalar)
    }

    /// Try to decrypt the detection tier of a compliance ciphertext.
    ///
    /// Decrypts via Fq subtraction, then compares the result against the expected
    /// asset_id (with and without FLAG_SENTINEL) to determine the flag.
    /// Also decrypts the salt (second Fq element) for DLEQ metadata binding.
    ///
    /// Returns `Ok((asset_id, is_flagged, salt, sender_slot_id, receiver_slot_id))`
    /// if the decrypted value matches expected_asset_id,
    /// or `Err(_)` if decryption doesn't match (wrong key or wrong asset).
    pub fn try_decrypt_detection(
        &self,
        epk: &Element,
        epk_orbis: &Element,
        detection_ciphertext: &[u8; DETECTION_TIER_BYTES],
        expected_asset_id: &asset::Id,
    ) -> anyhow::Result<(asset::Id, bool, Fq, u32, u32)> {
        // 1. Compute shared secret using standard curve EPK: S = dk * epk_orbis
        let shared_secret = *epk_orbis * self.0;

        // 2. Derive Poseidon stream cipher seed
        let shared_secret_fq = shared_secret.vartime_compress_to_field();
        let epk_fq = epk.vartime_compress_to_field();
        let seed = poseidon377::hash_2(&*DETECTION_TIER_DOMAIN, (shared_secret_fq, epk_fq));

        // 3. Decrypt via Fq subtraction: pt = ct - keystream
        // Detection tier layout: [asset_id+flag, salt, sender_slot_id, receiver_slot_id]
        let ct_fq = Fq::from_le_bytes_mod_order(&detection_ciphertext[..32]);
        let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
        let pt_fq = ct_fq - keystream;

        // 3b. Decrypt salt (second Fq element, counter=1)
        let ct_salt_fq = Fq::from_le_bytes_mod_order(&detection_ciphertext[32..64]);
        let keystream_salt = poseidon377::hash_2(&seed, (Fq::from(1u64), seed));
        let salt = ct_salt_fq - keystream_salt;

        let ct_sender_slot = Fq::from_le_bytes_mod_order(&detection_ciphertext[64..96]);
        let keystream_sender_slot = poseidon377::hash_2(&seed, (Fq::from(2u64), seed));
        let sender_slot_id =
            slot_id_from_fq(ct_sender_slot - keystream_sender_slot, "sender_slot_id")?;

        let ct_receiver_slot = Fq::from_le_bytes_mod_order(&detection_ciphertext[96..128]);
        let keystream_receiver_slot = poseidon377::hash_2(&seed, (Fq::from(3u64), seed));
        let receiver_slot_id = slot_id_from_fq(
            ct_receiver_slot - keystream_receiver_slot,
            "receiver_slot_id",
        )?;

        // 4. Compare against expected asset_id to determine flag
        if pt_fq == expected_asset_id.0 {
            Ok((
                *expected_asset_id,
                false,
                salt,
                sender_slot_id,
                receiver_slot_id,
            ))
        } else if pt_fq == expected_asset_id.0 + *FLAG_SENTINEL {
            Ok((
                *expected_asset_id,
                true,
                salt,
                sender_slot_id,
                receiver_slot_id,
            ))
        } else {
            anyhow::bail!("detection tier does not match expected asset")
        }
    }

    /// Encrypt a detection tier plaintext to this detection key's public key.
    ///
    /// Uses Fq addition (matching the R1CS circuit):
    /// `ct = (asset_id + flag_sentinel) + keystream`
    pub fn encrypt_to_public<R: rand_core::RngCore + rand_core::CryptoRng>(
        &self,
        rng: &mut R,
        asset_id: &asset::Id,
        is_flagged: bool,
    ) -> ([u8; DETECTION_TIER_BYTES], Element) {
        Self::encrypt_to_dk_pub(rng, &self.public_key(), asset_id, is_flagged)
    }

    /// Encrypt detection tier to a specific public key (for encryption without holding DK).
    ///
    /// Uses Fq addition: `ct = (asset_id + flag_sentinel) + keystream`
    pub fn encrypt_to_dk_pub<R: rand_core::RngCore + rand_core::CryptoRng>(
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

        // Encrypt: ct = pt + keystream (Fq addition, matches R1CS circuit)
        let pt_fq = detection_plaintext_fq(asset_id, is_flagged);
        let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
        let ct_fq = pt_fq + keystream;

        let mut detection_bytes = [0u8; DETECTION_TIER_BYTES];
        detection_bytes[..32].copy_from_slice(&ct_fq.to_bytes());
        for (counter, chunk) in (1u64..=3).zip(detection_bytes[32..].chunks_exact_mut(32)) {
            let keystream = poseidon377::hash_2(&seed, (Fq::from(counter), seed));
            chunk.copy_from_slice(&keystream.to_bytes());
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

        let (decrypted_asset, decrypted_flag, _salt, _sender_slot_id, _receiver_slot_id) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext, &asset_id)
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

        let (decrypted_asset, decrypted_flag, _salt, _sender_slot_id, _receiver_slot_id) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext, &asset_id)
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

        let (decrypted_asset, decrypted_flag, _salt, _sender_slot_id, _receiver_slot_id) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext, &asset_id)
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
        let result = dk2.try_decrypt_detection(&epk, &epk, &ciphertext, &asset_id);
        assert!(
            result.is_err(),
            "wrong DK should fail to match expected asset"
        );
    }

    #[test]
    fn test_sentinel_fq_differs_from_zero() {
        // FLAG_SENTINEL should be a non-trivial Fq constant
        assert_ne!(*FLAG_SENTINEL, Fq::zero());

        // Flagged plaintext should differ from unflagged
        let asset_id = asset::Id(Fq::from(11111u64));
        let pt_unflagged = detection_plaintext_fq(&asset_id, false);
        let pt_flagged = detection_plaintext_fq(&asset_id, true);
        assert_ne!(pt_unflagged, pt_flagged);

        // The difference should be exactly FLAG_SENTINEL
        assert_eq!(pt_flagged - pt_unflagged, *FLAG_SENTINEL);
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
                let (dec_id, dec_flag, _salt, _sender_slot_id, _receiver_slot_id) = dk
                    .try_decrypt_detection(&epk, &epk, &ct, &asset_id)
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
        // Regression: real asset IDs with high bytes must work with Fq sentinel.
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let mut asset_bytes = [0u8; 32];
        asset_bytes[0] = 0x42;
        asset_bytes[31] = 0x05;
        let asset_id = asset::Id(Fq::from_le_bytes_mod_order(&asset_bytes));

        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, true);
        let (decrypted_asset, decrypted_flag, _salt, _sender_slot_id, _receiver_slot_id) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert_eq!(decrypted_asset, asset_id, "asset ID should match");
        assert!(decrypted_flag, "flag should survive for realistic asset ID");
    }

    #[test]
    fn test_no_false_positive_flag_for_asset_with_high_byte() {
        // Regression: asset IDs with byte 31 = 0x11 must not produce false positives.
        let mut rng = OsRng;
        let dk = DetectionKey::demo();

        let mut asset_bytes = [0u8; 32];
        asset_bytes[0] = 0x01;
        asset_bytes[31] = 0x11;
        let asset_id = asset::Id(Fq::from_le_bytes_mod_order(&asset_bytes));

        let (ciphertext, epk) = dk.encrypt_to_public(&mut rng, &asset_id, false);
        let (_, decrypted_flag, _salt, _sender_slot_id, _receiver_slot_id) = dk
            .try_decrypt_detection(&epk, &epk, &ciphertext, &asset_id)
            .expect("decryption should succeed");

        assert!(
            !decrypted_flag,
            "unflagged TX should not be detected as flagged"
        );
    }
}
