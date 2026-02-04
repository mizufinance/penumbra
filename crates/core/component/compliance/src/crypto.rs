//! Cryptographic primitives for compliance encryption/decryption.
//!
//! This module implements ECDH-based encryption for compliance data using:
//! - Elliptic curve Diffie-Hellman for shared secret derivation
//! - Poseidon stream cipher (ZK-friendly, circuit-compatible)
//!
//! ## Encryption
//!
//! The transaction builder encrypts compliance data for BOTH sender and receiver
//! in a single call to `encrypt_compliance_details`. Each party gets their own
//! ciphertext that they can decrypt with their daily keys.
//!
//! For regulated assets with threshold policies:
//! - **Detection tier (32 bytes)**: Always encrypted to issuer's DK_pub
//!   - Contains: `asset_id` with flag packed in high bits (bit 252)
//!   - Fq order is < 2^252, so bit 252 is always 0 for valid asset IDs
//!   - Issuer can scan all transfers of their asset and see if flagged
//! - **Core + Extension**: Encrypted based on flag:
//!   - If NOT flagged: encrypted to user's daily keys (user can decrypt)
//!   - If flagged: encrypted to issuer's DK_pub (issuer can decrypt full details)

use anyhow::Context;
use ark_ff::Zero;
use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::keys::{AddressComplianceKey, KeyType};
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use rand_core::{CryptoRng, RngCore};

use crate::indexed_tree::IndexedLeaf;
use crate::issuer_keys::{DetectionTierPlaintext, DETECTION_TIER_BYTES};
use crate::structs::ComplianceCiphertext;

/// Domain separator for Poseidon stream cipher seed derivation.
pub static COMPLIANCE_STREAM_CIPHER_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.poseidon_stream").as_bytes(),
    )
});

/// The "black hole" compliance key for unregulated assets.
///
/// For unregulated assets, compliance data is encrypted to this key, making it
/// effectively unrecoverable (a "dead letter") since no one knows the discrete log.
///
/// This is a NUMS (Nothing-Up-My-Sleeve) point derived from a domain separator,
/// proving no one knows the discrete log. Since encryption verification is
/// conditional on `is_regulated` in the circuit, this value can be changed
/// without regenerating proving/verifying keys.
pub static BLACK_HOLE_ACK: Lazy<Element> = Lazy::new(|| {
    let hash = blake2b_simd::blake2b(b"penumbra.compliance.black_hole_ack");
    let scalar = Fr::from_le_bytes_mod_order(hash.as_bytes());
    Element::GENERATOR * scalar
});

/// Decrypted compliance data.
#[derive(Clone, Debug)]
pub struct DecryptedComplianceData {
    /// The asset ID being transacted.
    pub asset_id: asset::Id,
    /// The amount being transacted.
    pub amount: Amount,
    /// The "self" diversified generator (the party who can decrypt this ciphertext).
    pub self_diversified_generator: Element,
    /// The "self" transmission key.
    pub self_transmission_key: [u8; 32],
    /// The counterparty diversified generator (the other party in the transaction).
    pub counterparty_diversified_generator: Element,
    /// The counterparty transmission key.
    pub counterparty_transmission_key: [u8; 32],
}

/// Domain separator for issuer detection tier encryption.
pub static ISSUER_DETECTION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.issuer_detection").as_bytes(),
    )
});

/// Result from encryption, containing all data needed for decryption.
#[derive(Clone, Debug)]
pub struct EncryptionResult {
    /// The compliance ciphertext.
    pub ciphertext: ComplianceCiphertext,
    /// The ephemeral secret (for circuit witness).
    pub ephemeral_secret: Fr,
    /// The issuer shared secret (for issuer decryption).
    /// This is r * DK_pub where r is the ephemeral secret.
    pub issuer_shared_secret: Element,
}

/// Encrypt compliance details for a single party.
///
/// This function implements the per-asset threshold system where:
/// - Detection tier (32 bytes) is ALWAYS encrypted to the issuer's DK_pub
/// - Core + Extension are encrypted to user's DCK when NOT flagged
/// - Core + Extension are encrypted to issuer's DK_pub when flagged
///
/// The flag is computed deterministically: `is_flagged = amount >= threshold`
///
/// This allows issuers to:
/// 1. Scan all transfers of their asset (via detection tier)
/// 2. See full details only for flagged transfers (large amounts)
///
/// # Arguments
/// * `rng` - Random number generator
/// * `self_ack` - The "self" party's wallet compliance key
/// * `self_address` - The "self" party's address
/// * `date` - Day index for user key derivation
/// * `asset_id` - The asset being transacted
/// * `amount` - The amount being transacted
/// * `counterparty_address` - The other party
/// * `asset_leaf` - The asset's IMT leaf (contains dk_pub and threshold)
///
/// # Returns
/// An `EncryptionResult` containing ciphertext and shared secrets
pub fn encrypt_compliance_details(
    mut rng: impl RngCore + CryptoRng,
    self_ack: &AddressComplianceKey,
    self_address: &Address,
    date: u64,
    asset_id: asset::Id,
    amount: Amount,
    counterparty_address: &Address,
    asset_leaf: &IndexedLeaf,
) -> anyhow::Result<EncryptionResult> {
    // Extract policy from asset leaf
    let dk_pub = &asset_leaf.policy.dk_pub;
    let threshold = asset_leaf.policy.threshold;

    // Compute is_flagged deterministically from amount vs threshold
    let is_flagged = u128::from(amount) >= threshold;

    // 1. Extract diversified generator and diversifier
    let diversified_generator = self_address.diversified_generator();
    let diversifier = self_address.diversifier();

    // 2. Generate ephemeral secret r (shared for all tiers)
    let ephemeral_secret = Fr::rand(&mut rng);

    // 3. Compute BOTH ephemeral public keys (same r, different base points)
    //
    // ECDH requires both parties to use the same base point. Penumbra uses:
    // - User keys: on diversified curve B_d (per-address privacy)
    // - Issuer keys: on standard generator G (global, stored in asset leaf)
    //
    // Since B_d ≠ G, we need two EPKs:
    // - epk = r × B_d (for user decryption via diversified keys)
    // - epk_g = r × G (for issuer decryption via standard keys)
    //
    // Issuer ECDH:
    //   Encryption: ss = r × DK_pub = r × (dk × G) = r × dk × G
    //   Decryption: ss = dk × epk_g = dk × (r × G) = r × dk × G  ✓ (matches!)
    let epk = diversified_generator * ephemeral_secret;
    let epk_g = Element::GENERATOR * ephemeral_secret;

    // 4. Compute issuer shared secret using epk_g (correct ECDH)
    //    ss_issuer = r × DK_pub = r × dk × G
    //    Issuer computes: dk × epk_g = dk × r × G (same result!)
    let ss_issuer = *dk_pub * ephemeral_secret;

    // 5. Derive user's daily public keys for core+extension (only used if not flagged)
    let pk_core = self_ack.derive_daily_public_key(KeyType::Core, date, &diversifier);
    let pk_extension = self_ack.derive_daily_public_key(KeyType::Extension, date, &diversifier);

    // 6. Compute shared secrets for core+extension based on flag
    let (ss_core, ss_extension) = if is_flagged {
        // Flagged: encrypt to issuer's DK_pub using same shared secret computation
        (ss_issuer, ss_issuer)
    } else {
        // Not flagged: encrypt to user's daily keys
        let ss_core = pk_core * ephemeral_secret;
        let ss_extension = pk_extension * ephemeral_secret;
        (ss_core, ss_extension)
    };

    // 7. Derive Poseidon seeds
    let epk_fq = epk.vartime_compress_to_field();
    let seed_issuer = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss_issuer.vartime_compress_to_field(), epk_fq),
    );
    let seed_core = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss_core.vartime_compress_to_field(), epk_fq),
    );
    let seed_extension = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss_extension.vartime_compress_to_field(), epk_fq),
    );

    // 8. Build detection tier plaintext (32 bytes: asset_id with flag in high bits)
    let detection_plaintext = DetectionTierPlaintext::new(asset_id, is_flagged);
    let detection_bytes = detection_plaintext.to_bytes();

    // 9. Encrypt detection tier (single Fq element)
    let pt_fq = Fq::from_le_bytes_mod_order(&detection_bytes);
    let keystream = poseidon377::hash_2(&seed_issuer, (Fq::zero(), seed_issuer));
    let ct_fq = pt_fq + keystream;
    let detection_tag: [u8; 32] = ct_fq.to_bytes();

    // 10. Encrypt core data (amount + self address)
    let mut core_bytes = Vec::with_capacity(80);
    core_bytes.extend_from_slice(&amount.to_le_bytes());
    core_bytes.extend_from_slice(&self_address.diversified_generator().vartime_compress().0);
    core_bytes.extend_from_slice(&self_address.transmission_key().0);

    let mut encrypted_core = Vec::new();
    for (i, chunk) in core_bytes.chunks(31).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let plaintext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_core, (counter, seed_core));
        let ciphertext_fq = plaintext_fq + keystream;
        encrypted_core.extend_from_slice(&ciphertext_fq.to_bytes());
    }

    // 11. Encrypt extension data (counterparty address)
    let mut extension_bytes = Vec::with_capacity(64);
    extension_bytes.extend_from_slice(
        &counterparty_address
            .diversified_generator()
            .vartime_compress()
            .0,
    );
    extension_bytes.extend_from_slice(&counterparty_address.transmission_key().0);

    let mut encrypted_extension = Vec::new();
    for (i, chunk) in extension_bytes.chunks(31).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let plaintext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_extension, (counter, seed_extension));
        let ciphertext_fq = plaintext_fq + keystream;
        encrypted_extension.extend_from_slice(&ciphertext_fq.to_bytes());
    }

    Ok(EncryptionResult {
        ciphertext: ComplianceCiphertext::new(
            epk,
            epk_g,
            detection_tag,
            encrypted_core,
            encrypted_extension,
        ),
        ephemeral_secret,
        issuer_shared_secret: ss_issuer,
    })
}

/// Decrypt the 32-byte detection tier using issuer's DK.
///
/// This allows the issuer to:
/// 1. Identify that this ciphertext involves their asset
/// 2. Determine if the transfer was flagged (flag is packed in bit 252)
///
/// # Arguments
/// * `dk` - Issuer's detection key (scalar)
/// * `epk` - User's ephemeral public key (r * B_d) - used for seed derivation
/// * `epk_g` - Issuer's ephemeral public key (r * G) - used for ECDH shared secret
/// * `detection_ciphertext` - The 32-byte detection tier ciphertext
///
/// # Returns
/// Tuple of (asset_id, is_flagged) if decryption succeeds
pub fn decrypt_detection_tier_with_dk(
    dk: &Fr,
    epk: &Element,
    epk_g: &Element,
    detection_ciphertext: &[u8; DETECTION_TIER_BYTES],
) -> anyhow::Result<(asset::Id, bool)> {
    // Compute shared secret using epk_g (standard generator):
    // ss = dk × epk_g = dk × (r × G) = r × dk × G
    // This matches encryption's: r × DK_pub = r × (dk × G)
    let ss = *epk_g * *dk;

    // Derive seed (uses epk for consistency with encryption's seed derivation)
    let epk_fq = epk.vartime_compress_to_field();
    let seed = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss.vartime_compress_to_field(), epk_fq),
    );

    // Decrypt the single Fq element (asset_id with flag in high bits)
    let ct_fq = Fq::from_le_bytes_mod_order(detection_ciphertext);
    let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
    let plaintext_fq = ct_fq - keystream;

    // Convert to bytes and extract using DetectionTierPlaintext
    let plaintext_bytes = plaintext_fq.to_bytes();
    let detection_plaintext = DetectionTierPlaintext::from_bytes(&plaintext_bytes)?;

    Ok((detection_plaintext.asset_id, detection_plaintext.is_flagged))
}

/// Decrypt core+extension using issuer's DK (for flagged transfers).
///
/// When a transfer is flagged, the issuer can decrypt the full details
/// using their DK instead of the user's daily key.
///
/// # Arguments
/// * `dk` - Issuer's detection key (scalar)
/// * `ciphertext` - The full compliance ciphertext (contains both epk and epk_g)
///
/// # Returns
/// Decrypted compliance data
pub fn decrypt_compliance_details_with_dk(
    dk: &Fr,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<DecryptedComplianceData> {
    // Compute shared secret using epk_g (standard generator):
    // ss = dk × epk_g = dk × (r × G) = r × dk × G
    let ss = ciphertext.epk_g * *dk;

    // Decrypt detection tier using ISSUER_DETECTION_DOMAIN
    let epk_fq = ciphertext.epk.vartime_compress_to_field();
    let seed_detection = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss.vartime_compress_to_field(), epk_fq),
    );

    // Decrypt detection_tag (asset_id with flag in high bits)
    let detection_ciphertext_fq = Fq::from_le_bytes_mod_order(&ciphertext.detection_tag);
    let detection_keystream = poseidon377::hash_2(&seed_detection, (Fq::zero(), seed_detection));
    let detection_plaintext_fq = detection_ciphertext_fq - detection_keystream;

    // Extract asset_id and flag from detection tier
    let detection_bytes = detection_plaintext_fq.to_bytes();
    let detection_plaintext = DetectionTierPlaintext::from_bytes(&detection_bytes)
        .context("failed to parse detection tier")?;
    let asset_id = detection_plaintext.asset_id;

    // Decrypt core and extension tiers using COMPLIANCE_STREAM_CIPHER_DOMAIN
    let seed_core = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss.vartime_compress_to_field(), epk_fq),
    );
    let seed_extension = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss.vartime_compress_to_field(), epk_fq),
    );

    // Decrypt core data (amount + self address) - 3 Fq elements (80 bytes plaintext)
    let mut core_plaintext_bytes = Vec::new();
    for (i, chunk) in ciphertext.encrypted_core.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_core, (counter, seed_core));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(80 - core_plaintext_bytes.len());
        core_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    // Decrypt extension data (counterparty address) - 3 Fq elements (64 bytes plaintext)
    let mut extension_plaintext_bytes = Vec::new();
    for (i, chunk) in ciphertext.encrypted_extension.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_extension, (counter, seed_extension));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(64 - extension_plaintext_bytes.len());
        extension_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    // Parse core plaintext: amount (16) || self_div_gen (32) || self_trans_key (32)
    if core_plaintext_bytes.len() < 80 {
        anyhow::bail!(
            "core plaintext too short: expected 80 bytes, got {}",
            core_plaintext_bytes.len()
        );
    }

    let amount_bytes: [u8; 16] = core_plaintext_bytes[0..16]
        .try_into()
        .context("failed to extract amount")?;
    let amount = Amount::from_le_bytes(amount_bytes);

    let self_div_gen_bytes: [u8; 32] = core_plaintext_bytes[16..48]
        .try_into()
        .context("failed to extract self diversified generator")?;
    let self_div_gen = decaf377::Encoding(self_div_gen_bytes)
        .vartime_decompress()
        .map_err(|_| {
            anyhow::anyhow!("compliance decryption failed: invalid self diversified generator")
        })?;

    let self_trans_key_bytes: [u8; 32] = core_plaintext_bytes[48..80]
        .try_into()
        .context("failed to extract self transmission key")?;

    // Parse extension plaintext: counterparty_div_gen (32) || counterparty_trans_key (32)
    if extension_plaintext_bytes.len() < 64 {
        anyhow::bail!(
            "extension plaintext too short: expected 64 bytes, got {}",
            extension_plaintext_bytes.len()
        );
    }

    let counterparty_div_gen_bytes: [u8; 32] = extension_plaintext_bytes[0..32]
        .try_into()
        .context("failed to extract counterparty diversified generator")?;
    let counterparty_div_gen = decaf377::Encoding(counterparty_div_gen_bytes)
        .vartime_decompress()
        .map_err(|_| {
            anyhow::anyhow!(
                "compliance decryption failed: invalid counterparty diversified generator"
            )
        })?;

    let counterparty_trans_key_bytes: [u8; 32] = extension_plaintext_bytes[32..64]
        .try_into()
        .context("failed to extract counterparty transmission key")?;

    Ok(DecryptedComplianceData {
        asset_id,
        amount,
        self_diversified_generator: self_div_gen,
        self_transmission_key: self_trans_key_bytes,
        counterparty_diversified_generator: counterparty_div_gen,
        counterparty_transmission_key: counterparty_trans_key_bytes,
    })
}

/// Decrypt compliance details using Poseidon stream cipher with tiered keys.
///
/// This function reverses the encryption process to recover the original data.
/// Each ciphertext part requires its corresponding shared secret:
/// - detection_tag: decrypted with ss_detection
/// - encrypted_core: decrypted with ss_core
/// - encrypted_extension: decrypted with ss_extension
///
/// # Arguments
/// * `ss_detection` - Shared secret for detection key: `dmk_detection * EPK`
/// * `ss_core` - Shared secret for core key: `dmk_core * EPK`
/// * `ss_extension` - Shared secret for extension key: `dmk_extension * EPK`
/// * `epk` - The ephemeral public key from the ciphertext
/// * `ciphertext` - The compliance ciphertext to decrypt
///
/// # Returns
/// The decrypted compliance data, or an error if decryption fails
pub fn decrypt_with_shared_secrets(
    ss_detection: &Element,
    ss_core: &Element,
    ss_extension: &Element,
    epk: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<DecryptedComplianceData> {
    // 1. Derive seeds for each key type
    let epk_fq = epk.vartime_compress_to_field();

    let seed_detection = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss_detection.vartime_compress_to_field(), epk_fq),
    );
    let seed_core = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss_core.vartime_compress_to_field(), epk_fq),
    );
    let seed_extension = poseidon377::hash_2(
        &COMPLIANCE_STREAM_CIPHER_DOMAIN,
        (ss_extension.vartime_compress_to_field(), epk_fq),
    );

    // 2. Decrypt detection_tag (asset_id + flag) - 1 Fq element
    let detection_ciphertext_fq = Fq::from_le_bytes_mod_order(&ciphertext.detection_tag);
    let detection_keystream =
        poseidon377::hash_2(&seed_detection, (Fq::from(0u64), seed_detection));
    let detection_plaintext_fq = detection_ciphertext_fq - detection_keystream;

    // Parse detection tier using DetectionTierPlaintext (handles flag bit extraction)
    let detection_bytes = detection_plaintext_fq.to_bytes();
    let detection_plaintext = DetectionTierPlaintext::from_bytes(&detection_bytes)
        .map_err(|e| anyhow::anyhow!("compliance decryption failed: {}", e))?;
    let asset_id = detection_plaintext.asset_id;

    // 3. Decrypt core data (amount + self address) - 3 Fq elements (80 bytes plaintext)
    let mut core_plaintext_bytes = Vec::new();
    for (i, chunk) in ciphertext.encrypted_core.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_core, (counter, seed_core));
        let plaintext_fq = ciphertext_fq - keystream;
        // Take 31 bytes from each Fq (to match 31-byte chunk encoding)
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(80 - core_plaintext_bytes.len());
        core_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    // 4. Decrypt extension data (counterparty address) - 3 Fq elements (64 bytes plaintext)
    let mut extension_plaintext_bytes = Vec::new();
    for (i, chunk) in ciphertext.encrypted_extension.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_extension, (counter, seed_extension));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(64 - extension_plaintext_bytes.len());
        extension_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    // 5. Parse core plaintext: amount (16) || self_div_gen (32) || self_trans_key (32)
    if core_plaintext_bytes.len() < 80 {
        anyhow::bail!(
            "core plaintext too short: expected 80 bytes, got {}",
            core_plaintext_bytes.len()
        );
    }

    let amount_bytes: [u8; 16] = core_plaintext_bytes[0..16]
        .try_into()
        .context("failed to extract amount")?;
    let amount = Amount::from_le_bytes(amount_bytes);

    let self_div_gen_bytes: [u8; 32] = core_plaintext_bytes[16..48]
        .try_into()
        .context("failed to extract self diversified generator")?;
    let self_div_gen = decaf377::Encoding(self_div_gen_bytes)
        .vartime_decompress()
        .map_err(|_| {
            anyhow::anyhow!(
                "compliance decryption failed: invalid self diversified generator \
                 (not a valid curve point, likely wrong decryption key or corrupted data)"
            )
        })?;

    let self_trans_key_bytes: [u8; 32] = core_plaintext_bytes[48..80]
        .try_into()
        .context("failed to extract self transmission key")?;

    // 6. Parse extension plaintext: counterparty_div_gen (32) || counterparty_trans_key (32)
    if extension_plaintext_bytes.len() < 64 {
        anyhow::bail!(
            "extension plaintext too short: expected 64 bytes, got {}",
            extension_plaintext_bytes.len()
        );
    }

    let counterparty_div_gen_bytes: [u8; 32] = extension_plaintext_bytes[0..32]
        .try_into()
        .context("failed to extract counterparty diversified generator")?;
    let counterparty_div_gen = decaf377::Encoding(counterparty_div_gen_bytes)
        .vartime_decompress()
        .map_err(|_| {
            anyhow::anyhow!(
                "compliance decryption failed: invalid counterparty diversified generator \
                 (not a valid curve point, likely wrong decryption key or corrupted data)"
            )
        })?;

    let counterparty_trans_key_bytes: [u8; 32] = extension_plaintext_bytes[32..64]
        .try_into()
        .context("failed to extract counterparty transmission key")?;

    // 7. Return the decrypted data
    Ok(DecryptedComplianceData {
        asset_id,
        amount,
        self_diversified_generator: self_div_gen,
        self_transmission_key: self_trans_key_bytes,
        counterparty_diversified_generator: counterparty_div_gen,
        counterparty_transmission_key: counterparty_trans_key_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexed_tree::FQ_MAX;
    use crate::structs::AssetPolicy;
    use penumbra_sdk_keys::keys::{Diversifier, UserComplianceKey};
    use rand_core::OsRng;

    /// Helper to create an IndexedLeaf for tests.
    fn make_test_leaf(dk_pub: Element, threshold: u128) -> IndexedLeaf {
        IndexedLeaf {
            value: Fq::from(42u64),
            next_index: 0,
            next_value: *FQ_MAX,
            policy: AssetPolicy::new(dk_pub, threshold),
        }
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        use crate::issuer_keys::DetectionKey;

        let mut rng = OsRng;

        // Setup: Create a master key and derive a wallet key
        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);

        let diversifier = Diversifier([1u8; 16]);
        let address_key = uck.derive_address_key(&diversifier);

        // Create a test address using the SAME diversifier (crucial for ECDH to work)
        let random_scalar = Fr::rand(&mut rng);
        let random_point = decaf377::Element::GENERATOR * random_scalar;
        let pk_d = decaf377_ka::Public(random_point.vartime_compress().0);

        let mut ck_d_bytes = [0u8; 32];
        rng.fill_bytes(&mut ck_d_bytes);
        let ck_d = decaf377_fmd::ClueKey(ck_d_bytes);

        let recipient_address =
            Address::from_components(diversifier, pk_d, ck_d).expect("valid address components");

        // Encryption parameters
        let date = 19000u64;
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);
        let counterparty_address = Address::dummy(&mut rng);

        // Setup issuer detection key
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        // Create asset leaf with low threshold so amount is flagged
        let asset_leaf = make_test_leaf(dk_pub, 500); // threshold=500, amount=1000 => flagged

        // Encrypt (flagged because amount >= threshold)
        let result = encrypt_compliance_details(
            &mut rng,
            &address_key,
            &recipient_address,
            date,
            asset_id,
            amount,
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Issuer decrypts using their detection key (flagged = full access)
        let decrypted = decrypt_compliance_details_with_dk(dk.inner(), &result.ciphertext)
            .expect("decryption should succeed");

        // Verify
        assert_eq!(decrypted.asset_id, asset_id, "asset_id should match");
        assert_eq!(decrypted.amount, amount, "amount should match");
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        use crate::issuer_keys::DetectionKey;

        let mut rng = OsRng;

        // Setup: Create a master key and derive a wallet key
        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);

        let diversifier = Diversifier([1u8; 16]);
        let address_key = uck.derive_address_key(&diversifier);

        let recipient_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);

        // Setup issuer
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        // Create asset leaf with high threshold (not flagged)
        let asset_leaf = make_test_leaf(dk_pub, u128::MAX);

        // Encrypt
        let date = 19000u64;
        let result = encrypt_compliance_details(
            &mut rng,
            &address_key,
            &recipient_address,
            date,
            asset::Id(Fq::from(42u64)),
            Amount::from(1000u128),
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Try to decrypt with a DIFFERENT user compliance key
        let wrong_uck_scalar = Fr::rand(&mut rng);
        let wrong_uck = UserComplianceKey::new(wrong_uck_scalar);
        let wrong_daily_keys = wrong_uck.derive_daily_keys(date);
        let wrong_ss_core = result.ciphertext.epk * wrong_daily_keys.core.inner();
        let wrong_ss_extension = result.ciphertext.epk * wrong_daily_keys.extension.inner();

        // Use correct detection (issuer), but wrong core/extension
        let ss_detection = &result.issuer_shared_secret;

        // Decryption should fail (wrong shared secret will produce garbage plaintext)
        let decryption_result = decrypt_with_shared_secrets(
            ss_detection,
            &wrong_ss_core,
            &wrong_ss_extension,
            &result.ciphertext.epk,
            &result.ciphertext,
        );
        assert!(
            decryption_result.is_err(),
            "decryption with wrong key should fail"
        );
    }

    #[test]
    fn test_decrypt_with_wrong_date_fails() {
        use crate::issuer_keys::DetectionKey;

        let mut rng = OsRng;

        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);

        let diversifier = Diversifier([1u8; 16]);
        let address_key = uck.derive_address_key(&diversifier);

        let recipient_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);

        // Setup issuer
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        // Create asset leaf with high threshold (not flagged)
        let asset_leaf = make_test_leaf(dk_pub, u128::MAX);

        // Encrypt for date 19000
        let date = 19000u64;
        let result = encrypt_compliance_details(
            &mut rng,
            &address_key,
            &recipient_address,
            date,
            asset::Id(Fq::from(42u64)),
            Amount::from(1000u128),
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Try to decrypt with a DIFFERENT date (wrong core/extension keys)
        let wrong_date = 19001u64;
        let wrong_daily_keys = uck.derive_daily_keys(wrong_date);
        let wrong_ss_core = result.ciphertext.epk * wrong_daily_keys.core.inner();
        let wrong_ss_extension = result.ciphertext.epk * wrong_daily_keys.extension.inner();

        // Use correct detection (issuer), but wrong date for core/extension
        let ss_detection = &result.issuer_shared_secret;

        // Decryption with wrong date should either fail OR return garbage data
        let decryption_result = decrypt_with_shared_secrets(
            ss_detection,
            &wrong_ss_core,
            &wrong_ss_extension,
            &result.ciphertext.epk,
            &result.ciphertext,
        );
        match decryption_result {
            Err(_) => {
                // Expected: decryption failed (random bytes didn't form valid field elements)
            }
            Ok(decrypted) => {
                // Also valid: decryption succeeded but produced garbage data
                assert!(
                    decrypted.asset_id != asset::Id(Fq::from(42u64))
                        || decrypted.amount != Amount::from(1000u128),
                    "decryption with wrong date should not produce correct data"
                );
            }
        }
    }

    // ========== Threshold Encryption Tests ==========

    #[test]
    fn test_issuer_encryption_not_flagged() {
        use crate::issuer_keys::DetectionKey;

        let mut rng = OsRng;

        // Setup user
        let uck = UserComplianceKey::new(Fr::rand(&mut rng));
        let diversifier = Diversifier([1u8; 16]);
        let address_key = uck.derive_address_key(&diversifier);

        // Create test addresses
        let random_scalar = Fr::rand(&mut rng);
        let random_point = decaf377::Element::GENERATOR * random_scalar;
        let pk_d = decaf377_ka::Public(random_point.vartime_compress().0);
        let mut ck_d_bytes = [0u8; 32];
        rng.fill_bytes(&mut ck_d_bytes);
        let ck_d = decaf377_fmd::ClueKey(ck_d_bytes);
        let self_address =
            Address::from_components(diversifier, pk_d, ck_d).expect("valid address");

        let counterparty_address = Address::dummy(&mut rng);

        // Setup issuer
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        // Encrypt (not flagged: amount < threshold)
        let date = 19000u64;
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(5000u128);

        // Threshold is 10000, amount is 5000 => NOT flagged
        let asset_leaf = make_test_leaf(dk_pub, 10000);

        let result = encrypt_compliance_details(
            &mut rng,
            &address_key,
            &self_address,
            date,
            asset_id,
            amount,
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // User should be able to decrypt core+extension (not flagged)
        let daily_keys = uck.derive_daily_keys(date);
        let ss_core = result.ciphertext.epk * daily_keys.core.inner();
        let ss_extension = result.ciphertext.epk * daily_keys.extension.inner();

        // Detection is issuer-only, use issuer's shared secret
        let ss_detection = &result.issuer_shared_secret;

        let decrypted = decrypt_with_shared_secrets(
            ss_detection,
            &ss_core,
            &ss_extension,
            &result.ciphertext.epk,
            &result.ciphertext,
        )
        .expect("user decryption should succeed for non-flagged TX");

        assert_eq!(decrypted.amount, amount);
        assert_eq!(decrypted.asset_id, asset_id);
    }

    #[test]
    fn test_issuer_encryption_flagged() {
        use crate::issuer_keys::DetectionKey;

        let mut rng = OsRng;

        // Setup user
        let uck = UserComplianceKey::new(Fr::rand(&mut rng));
        let diversifier = Diversifier([1u8; 16]);
        let address_key = uck.derive_address_key(&diversifier);

        // Create test addresses
        let random_scalar = Fr::rand(&mut rng);
        let random_point = decaf377::Element::GENERATOR * random_scalar;
        let pk_d = decaf377_ka::Public(random_point.vartime_compress().0);
        let mut ck_d_bytes = [0u8; 32];
        rng.fill_bytes(&mut ck_d_bytes);
        let ck_d = decaf377_fmd::ClueKey(ck_d_bytes);
        let self_address =
            Address::from_components(diversifier, pk_d, ck_d).expect("valid address");

        let counterparty_address = Address::dummy(&mut rng);

        // Setup issuer
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        // Encrypt (flagged: amount >= threshold)
        let date = 19000u64;
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(100000u128); // Large amount

        // Threshold is 50000, amount is 100000 => FLAGGED
        let asset_leaf = make_test_leaf(dk_pub, 50000);

        let result = encrypt_compliance_details(
            &mut rng,
            &address_key,
            &self_address,
            date,
            asset_id,
            amount,
            &counterparty_address,
            &asset_leaf,
        )
        .expect("encryption should succeed");

        // Issuer should be able to decrypt core+extension (flagged)
        // Note: We use the issuer_shared_secret from the encryption result
        // because standard ECDH (dk * epk) doesn't work when epk uses diversified generator
        // and dk_pub uses the standard generator G.
        let ss_issuer = &result.issuer_shared_secret;

        let decrypted = decrypt_with_shared_secrets(
            ss_issuer,
            ss_issuer,
            ss_issuer,
            &result.ciphertext.epk,
            &result.ciphertext,
        )
        .expect("issuer decryption should succeed for flagged TX");

        assert_eq!(decrypted.amount, amount);
        assert_eq!(decrypted.asset_id, asset_id);

        // User should NOT be able to decrypt core/extension (encrypted to issuer's key)
        let daily_keys = uck.derive_daily_keys(date);
        let ss_core = result.ciphertext.epk * daily_keys.core.inner();
        let ss_extension = result.ciphertext.epk * daily_keys.extension.inner();

        // Even with correct detection, user core/extension keys won't work on flagged TX
        let user_result = decrypt_with_shared_secrets(
            ss_issuer, // Use issuer detection (detection is issuer-only)
            &ss_core,
            &ss_extension,
            &result.ciphertext.epk,
            &result.ciphertext,
        );

        // User decryption should fail or return garbage (core/extension encrypted to issuer)
        match user_result {
            Err(_) => {
                // Expected: decryption failed
            }
            Ok(decrypted) => {
                // Also valid: decryption succeeded but produced garbage
                assert_ne!(
                    decrypted.amount, amount,
                    "user should not be able to decrypt flagged TX"
                );
            }
        }
    }

    #[test]
    fn test_detection_tier_roundtrip() {
        use crate::issuer_keys::{DetectionKey, DetectionTierPlaintext};

        let mut rng = OsRng;

        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ephemeral_secret = Fr::rand(&mut rng);
        let epk = decaf377::Element::GENERATOR * ephemeral_secret;

        let asset_id = asset::Id(Fq::from(12345u64));

        // Test not flagged
        {
            let plaintext = DetectionTierPlaintext::new(asset_id, false);
            let plaintext_bytes = plaintext.to_bytes();

            // Compute shared secret and seed
            let ss = dk_pub * ephemeral_secret;
            let epk_fq = epk.vartime_compress_to_field();
            let seed = poseidon377::hash_2(
                &super::ISSUER_DETECTION_DOMAIN,
                (ss.vartime_compress_to_field(), epk_fq),
            );

            // Encrypt (single Fq element)
            let pt_fq = Fq::from_le_bytes_mod_order(&plaintext_bytes);
            let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
            let ct_fq = pt_fq + keystream;
            let ciphertext = ct_fq.to_bytes();

            // Decrypt
            // In this test, epk = epk_g since we used GENERATOR for both
            let (decrypted_asset, is_flagged) =
                decrypt_detection_tier_with_dk(dk.inner(), &epk, &epk, &ciphertext)
                    .expect("decryption should succeed");

            assert_eq!(decrypted_asset, asset_id);
            assert!(!is_flagged);
        }

        // Test flagged
        {
            let plaintext = DetectionTierPlaintext::new(asset_id, true);
            let plaintext_bytes = plaintext.to_bytes();

            let ss = dk_pub * ephemeral_secret;
            let epk_fq = epk.vartime_compress_to_field();
            let seed = poseidon377::hash_2(
                &super::ISSUER_DETECTION_DOMAIN,
                (ss.vartime_compress_to_field(), epk_fq),
            );

            // Encrypt (single Fq element)
            let pt_fq = Fq::from_le_bytes_mod_order(&plaintext_bytes);
            let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
            let ct_fq = pt_fq + keystream;
            let ciphertext = ct_fq.to_bytes();

            // In this test, epk = epk_g since we used GENERATOR for both
            let (decrypted_asset, is_flagged) =
                decrypt_detection_tier_with_dk(dk.inner(), &epk, &epk, &ciphertext)
                    .expect("decryption should succeed");

            assert_eq!(decrypted_asset, asset_id);
            assert!(is_flagged);
        }
    }

    #[test]
    fn test_wrong_dk_cannot_decrypt_detection_tier() {
        use crate::issuer_keys::{DetectionKey, DetectionTierPlaintext};

        let mut rng = OsRng;

        let dk1 = DetectionKey::demo();
        let dk2 = DetectionKey::from_seed(&[1u8; 32]); // Different key
        let dk1_pub = dk1.public_key();

        let ephemeral_secret = Fr::rand(&mut rng);
        let epk = decaf377::Element::GENERATOR * ephemeral_secret;

        let asset_id = asset::Id(Fq::from(12345u64));
        let plaintext = DetectionTierPlaintext::new(asset_id, false);
        let plaintext_bytes = plaintext.to_bytes();

        // Encrypt with dk1's public key (single Fq element)
        let ss = dk1_pub * ephemeral_secret;
        let epk_fq = epk.vartime_compress_to_field();
        let seed = poseidon377::hash_2(
            &super::ISSUER_DETECTION_DOMAIN,
            (ss.vartime_compress_to_field(), epk_fq),
        );

        let pt_fq = Fq::from_le_bytes_mod_order(&plaintext_bytes);
        let keystream = poseidon377::hash_2(&seed, (Fq::zero(), seed));
        let ct_fq = pt_fq + keystream;
        let ciphertext = ct_fq.to_bytes();

        // Try to decrypt with dk2 - should produce garbage
        // In this test, epk = epk_g since we used GENERATOR for both
        let result = decrypt_detection_tier_with_dk(dk2.inner(), &epk, &epk, &ciphertext);

        // Decryption might succeed but produce wrong data, or fail on validation
        match result {
            Err(_) => {
                // Expected: validation failed
            }
            Ok((wrong_asset, _)) => {
                assert_ne!(
                    wrong_asset, asset_id,
                    "wrong DK should not produce correct asset_id"
                );
            }
        }
    }
}
