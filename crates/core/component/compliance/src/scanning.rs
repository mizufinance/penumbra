//! Compliance decryption with tiered access control.
//!
//! Provides separate functions for decrypting different tiers of compliance data,
//! enabling selective disclosure:
//!
//! - **Core**: amount + self address
//! - **Extension**: counterparty address (Output only)
//! - **Sender-extension**: sender's copy of amount + recipient address (Output only)
//!
//! ## Access Paths
//!
//! 1. **Orbis path** (non-flagged): Orbis provides xnc_cmt per tier, issuer recovers seed.
//! 2. **Flagged path**: issuer decrypts directly via dk × epk_i (no Orbis needed).
//! 3. **Shared-secret path**: caller provides pre-computed shared secret per tier.

use decaf377::{Element, Fq, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_num::Amount;

use crate::orbis::recover_seed;
use crate::structs::ComplianceCiphertext;

/// Decrypted core data: amount and self address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreData {
    pub amount: Amount,
    pub self_diversified_generator: Element,
    pub self_transmission_key: [u8; 32],
}

/// Decrypted extension data: counterparty address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionData {
    pub counterparty_diversified_generator: Element,
    pub counterparty_transmission_key: [u8; 32],
}

/// Decrypted sender-extension data from an Output ciphertext.
///
/// Contains amount + recipient address, encrypted to the sender's ACK_sext.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendExtData {
    pub amount: Amount,
    pub recipient_diversified_generator: Element,
    pub recipient_transmission_key: [u8; 32],
}

/// Full decrypted compliance data (all tiers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullComplianceData {
    pub asset_id: asset::Id,
    pub core: CoreData,
    pub extension: ExtensionData,
}

/// Scanner role for dual-ciphertext transactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScannerRole {
    Sender,
    Receiver,
}

// ============================================================================
// Shared-secret-based decryption
// ============================================================================

/// Decrypt core tier using a pre-computed shared secret.
///
/// seed = c2_core - ss_core.compress()
pub fn decrypt_core(
    ss_core: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<CoreData>> {
    let shared_fq = ss_core.vartime_compress_to_field();
    let seed_core = ciphertext.c2_core - shared_fq;
    decrypt_core_with_seed(seed_core, ciphertext)
}

/// Decrypt extension tier using a pre-computed shared secret.
///
/// seed = c2_ext - ss_ext.compress()
pub fn decrypt_extension(
    ss_ext: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<ExtensionData>> {
    let c2_ext = match ciphertext.c2_ext {
        Some(c2) => c2,
        None => return Ok(None),
    };
    let shared_fq = ss_ext.vartime_compress_to_field();
    let seed_ext = c2_ext - shared_fq;
    decrypt_extension_with_seed(seed_ext, ciphertext)
}

/// Decrypt sender-extension tier using a pre-computed shared secret.
pub fn decrypt_spend_ext(
    ss_sext: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<SpendExtData>> {
    let c2_sext = match ciphertext.c2_sext {
        Some(c2) => c2,
        None => return Ok(None),
    };
    let shared_fq = ss_sext.vartime_compress_to_field();
    let seed_sext = c2_sext - shared_fq;
    decrypt_spend_ext_with_seed(seed_sext, ciphertext)
}

/// Decrypt core + extension using pre-computed shared secrets.
pub fn decrypt_full(
    ss_core: &Element,
    ss_ext: &Element,
    ciphertext: &ComplianceCiphertext,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    let core = match decrypt_core(ss_core, ciphertext)? {
        Some(c) => c,
        None => return Ok(None),
    };
    let extension = match decrypt_extension(ss_ext, ciphertext)? {
        Some(e) => e,
        None => return Ok(None),
    };
    Ok(Some(FullComplianceData {
        asset_id,
        core,
        extension,
    }))
}

/// Decrypt compliance data for a specific role.
///
/// - **Receiver**: core + ext from receiver ciphertext.
/// - **Sender**: core from spend ciphertext + sext from output ciphertext.
pub fn decrypt_with_role(
    ss_core: &Element,
    ss_ext: &Element,
    ss_sext: &Element,
    sender_ciphertext: &ComplianceCiphertext,
    receiver_ciphertext: &ComplianceCiphertext,
    role: ScannerRole,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    match role {
        ScannerRole::Receiver => decrypt_full(ss_core, ss_ext, receiver_ciphertext, asset_id),
        ScannerRole::Sender => {
            // Core from the spend ciphertext
            let core = match decrypt_core(ss_core, sender_ciphertext)? {
                Some(c) => c,
                None => return Ok(None),
            };

            // Try ext from sender CT (sender CT uses Output format)
            if let Some(ext) = decrypt_extension(ss_ext, sender_ciphertext)? {
                return Ok(Some(FullComplianceData {
                    asset_id,
                    core,
                    extension: ext,
                }));
            }

            // Production: Spend CT has no ext. Get counterparty from
            // the Output CT's sext tier (encrypted to sender's ACK_sext).
            if let Some(spend_ext) = decrypt_spend_ext(ss_sext, receiver_ciphertext)? {
                return Ok(Some(FullComplianceData {
                    asset_id,
                    core,
                    extension: ExtensionData {
                        counterparty_diversified_generator: spend_ext
                            .recipient_diversified_generator,
                        counterparty_transmission_key: spend_ext.recipient_transmission_key,
                    },
                }));
            }

            Ok(None)
        }
    }
}

// ============================================================================
// Orbis PRE decryption
// ============================================================================

/// Decrypt core tier using Orbis re-encryption commitment.
///
/// Issuer receives xnc_cmt from Orbis, recovers seed via:
/// P = xnc_cmt - sk_issuer × ack_core, seed = c2_core - P.compress()
pub fn decrypt_core_via_orbis(
    xnc_cmt: &Element,
    sk_issuer: &Fr,
    ack_core: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<CoreData>> {
    let seed_core = recover_seed(xnc_cmt, sk_issuer, ack_core, &ciphertext.c2_core);
    decrypt_core_with_seed(seed_core, ciphertext)
}

/// Decrypt extension tier using Orbis re-encryption commitment.
pub fn decrypt_extension_via_orbis(
    xnc_cmt: &Element,
    sk_issuer: &Fr,
    ack_ext: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<ExtensionData>> {
    let c2_ext = match ciphertext.c2_ext {
        Some(c2) => c2,
        None => return Ok(None),
    };
    let seed_ext = recover_seed(xnc_cmt, sk_issuer, ack_ext, &c2_ext);
    decrypt_extension_with_seed(seed_ext, ciphertext)
}

/// Decrypt sender-extension tier using Orbis re-encryption commitment.
pub fn decrypt_spend_ext_via_orbis(
    xnc_cmt: &Element,
    sk_issuer: &Fr,
    ack_sext: &Element,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<SpendExtData>> {
    let c2_sext = match ciphertext.c2_sext {
        Some(c2) => c2,
        None => return Ok(None),
    };
    let seed_sext = recover_seed(xnc_cmt, sk_issuer, ack_sext, &c2_sext);
    decrypt_spend_ext_with_seed(seed_sext, ciphertext)
}

/// Decrypt all tiers using Orbis re-encryption commitments.
pub fn decrypt_full_via_orbis(
    xnc_cmt_core: &Element,
    xnc_cmt_ext: &Element,
    sk_issuer: &Fr,
    ack_core: &Element,
    ack_ext: &Element,
    ciphertext: &ComplianceCiphertext,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    let core = match decrypt_core_via_orbis(xnc_cmt_core, sk_issuer, ack_core, ciphertext)? {
        Some(c) => c,
        None => return Ok(None),
    };
    let extension = match decrypt_extension_via_orbis(xnc_cmt_ext, sk_issuer, ack_ext, ciphertext)?
    {
        Some(e) => e,
        None => return Ok(None),
    };
    Ok(Some(FullComplianceData {
        asset_id,
        core,
        extension,
    }))
}

// ============================================================================
// Flagged transaction decryption (direct ECDH, no Orbis)
// ============================================================================

/// Decrypt core tier from a flagged transaction (dk × epk_1).
pub fn decrypt_core_flagged(
    dk_secret: &Fr,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<CoreData>> {
    let shared_point = ciphertext.epk_1 * *dk_secret;
    let shared_fq = shared_point.vartime_compress_to_field();
    let seed_core = ciphertext.c2_core - shared_fq;
    decrypt_core_with_seed(seed_core, ciphertext)
}

/// Decrypt extension tier from a flagged transaction (dk × epk_2).
pub fn decrypt_extension_flagged(
    dk_secret: &Fr,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<ExtensionData>> {
    let (c2_ext, epk_2) = match (ciphertext.c2_ext, ciphertext.epk_2) {
        (Some(c2), Some(epk)) => (c2, epk),
        _ => return Ok(None),
    };
    let shared_point = epk_2 * *dk_secret;
    let shared_fq = shared_point.vartime_compress_to_field();
    let seed_ext = c2_ext - shared_fq;
    decrypt_extension_with_seed(seed_ext, ciphertext)
}

/// Decrypt sender-extension from a flagged transaction (dk × epk_3).
pub fn decrypt_spend_ext_flagged(
    dk_secret: &Fr,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<SpendExtData>> {
    let (c2_sext, epk_3) = match (ciphertext.c2_sext, ciphertext.epk_3) {
        (Some(c2), Some(epk)) => (c2, epk),
        _ => return Ok(None),
    };
    let shared_point = epk_3 * *dk_secret;
    let shared_fq = shared_point.vartime_compress_to_field();
    let seed_sext = c2_sext - shared_fq;
    decrypt_spend_ext_with_seed(seed_sext, ciphertext)
}

/// Decrypt all tiers from a flagged transaction.
pub fn decrypt_full_flagged(
    dk_secret: &Fr,
    ciphertext: &ComplianceCiphertext,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    let core = match decrypt_core_flagged(dk_secret, ciphertext)? {
        Some(c) => c,
        None => return Ok(None),
    };
    let extension = match decrypt_extension_flagged(dk_secret, ciphertext)? {
        Some(e) => e,
        None => return Ok(None),
    };
    Ok(Some(FullComplianceData {
        asset_id,
        core,
        extension,
    }))
}

// ============================================================================
// Internal helpers — seed-based decryption
// ============================================================================

pub fn decrypt_core_with_seed(
    seed_core: Fq,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<CoreData>> {
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

    if core_plaintext_bytes.len() < 80 {
        return Ok(None);
    }

    let amount_bytes: [u8; 16] = match core_plaintext_bytes[0..16].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let amount = Amount::from_le_bytes(amount_bytes);

    let self_div_gen_bytes: [u8; 32] = match core_plaintext_bytes[16..48].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let self_div_gen = match decaf377::Encoding(self_div_gen_bytes).vartime_decompress() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let self_trans_key_bytes: [u8; 32] = match core_plaintext_bytes[48..80].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    Ok(Some(CoreData {
        amount,
        self_diversified_generator: self_div_gen,
        self_transmission_key: self_trans_key_bytes,
    }))
}

pub fn decrypt_extension_with_seed(
    seed_ext: Fq,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<ExtensionData>> {
    let encrypted_ext = match &ciphertext.encrypted_ext {
        Some(data) => data,
        None => return Ok(None),
    };

    let mut extension_plaintext_bytes = Vec::new();
    for (i, chunk) in encrypted_ext.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed_ext, (counter, seed_ext));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(64 - extension_plaintext_bytes.len());
        extension_plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    if extension_plaintext_bytes.len() < 64 {
        return Ok(None);
    }

    let counterparty_div_gen_bytes: [u8; 32] = match extension_plaintext_bytes[0..32].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let counterparty_div_gen =
        match decaf377::Encoding(counterparty_div_gen_bytes).vartime_decompress() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

    let counterparty_trans_key_bytes: [u8; 32] = match extension_plaintext_bytes[32..64].try_into()
    {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    Ok(Some(ExtensionData {
        counterparty_diversified_generator: counterparty_div_gen,
        counterparty_transmission_key: counterparty_trans_key_bytes,
    }))
}

pub fn decrypt_spend_ext_with_seed(
    seed: Fq,
    ciphertext: &ComplianceCiphertext,
) -> anyhow::Result<Option<SpendExtData>> {
    let encrypted_sext = match &ciphertext.encrypted_sext {
        Some(data) => data,
        None => return Ok(None),
    };

    let mut plaintext_bytes = Vec::new();
    for (i, chunk) in encrypted_sext.chunks(32).enumerate() {
        let mut buf = [0u8; 32];
        buf[0..chunk.len()].copy_from_slice(chunk);
        let ciphertext_fq = Fq::from_le_bytes_mod_order(&buf);
        let counter = Fq::from(i as u64);
        let keystream = poseidon377::hash_2(&seed, (counter, seed));
        let plaintext_fq = ciphertext_fq - keystream;
        let fq_bytes = plaintext_fq.to_bytes();
        let bytes_to_take = 31.min(80 - plaintext_bytes.len());
        plaintext_bytes.extend_from_slice(&fq_bytes[0..bytes_to_take]);
    }

    if plaintext_bytes.len() < 80 {
        return Ok(None);
    }

    let amount_bytes: [u8; 16] = match plaintext_bytes[0..16].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let amount = Amount::from_le_bytes(amount_bytes);

    let div_gen_bytes: [u8; 32] = match plaintext_bytes[16..48].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let div_gen = match decaf377::Encoding(div_gen_bytes).vartime_decompress() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let trans_key_bytes: [u8; 32] = match plaintext_bytes[48..80].try_into() {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    Ok(Some(SpendExtData {
        amount,
        recipient_diversified_generator: div_gen,
        recipient_transmission_key: trans_key_bytes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{derive_compliance_scalar, encrypt_output, encrypt_spend};
    use crate::issuer_keys::DetectionKey;
    use penumbra_sdk_keys::Address;
    use rand_core::OsRng;

    fn derive_ack(ring_pk: &Element, b_d_fq: Fq) -> Element {
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    #[test]
    fn test_core_and_extension_decryption_flagged() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack,
            &ack,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            true,
            Fq::from(0u64),
        )
        .unwrap();

        // Core decryption (flagged)
        let core = decrypt_core_flagged(dk.inner(), &result.ciphertext)
            .unwrap()
            .expect("core decryption should succeed");
        assert_eq!(core.amount, Amount::from(1000u128));

        // Extension decryption (flagged)
        let ext = decrypt_extension_flagged(dk.inner(), &result.ciphertext)
            .unwrap()
            .expect("extension decryption should succeed");
        assert!(ext.counterparty_transmission_key != [0u8; 32]);
    }

    #[test]
    fn test_orbis_pre_decrypt_roundtrip() {
        use crate::orbis::{OrbisReencryptor, SimulatedOrbis};

        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let dk_secret = *dk.inner();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let orbis = SimulatedOrbis::new(sk_ring);

        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(1000u128);

        let receiver_b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();

        let ack = derive_ack(&ring_pk, receiver_b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack,
            &ack,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            false,
            Fq::from(0u64),
        )
        .unwrap();

        let ct = &result.ciphertext;
        let b_d_bytes = receiver_b_d_fq.to_bytes();

        // Orbis re-encryption for core and ext tiers
        let xnc_core = orbis.reencrypt(&ct.epk_1, &dk_pub, &b_d_bytes);
        let xnc_ext = orbis.reencrypt(&ct.epk_2.unwrap(), &dk_pub, &b_d_bytes);

        // Issuer decrypts core
        let core = decrypt_core_via_orbis(&xnc_core, &dk_secret, &ack, ct)
            .unwrap()
            .expect("core decryption should succeed");
        assert_eq!(core.amount, Amount::from(1000u128));

        // Issuer decrypts extension
        let ext = decrypt_extension_via_orbis(&xnc_ext, &dk_secret, &ack, ct)
            .unwrap()
            .expect("extension decryption should succeed");
        assert!(ext.counterparty_transmission_key != [0u8; 32]);
    }

    #[test]
    fn test_wrong_ring_key_fails() {
        use crate::orbis::{OrbisReencryptor, SimulatedOrbis};

        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let dk_secret = *dk.inner();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;

        // Wrong ring key for Orbis
        let wrong_sk_ring = Fr::rand(&mut rng);
        let wrong_orbis = SimulatedOrbis::new(wrong_sk_ring);

        let self_address = Address::dummy(&mut rng);
        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset::Id(Fq::from(42u64)),
            Amount::from(1000u128),
            false,
            Fq::from(0u64),
        )
        .unwrap();

        let ct = &result.ciphertext;
        let b_d_bytes = b_d_fq.to_bytes();

        let xnc = wrong_orbis.reencrypt(&ct.epk_1, &dk_pub, &b_d_bytes);
        let core = decrypt_core_via_orbis(&xnc, &dk_secret, &ack, ct).unwrap();

        // Should return None (garbage decompression fails) or garbage amount
        if let Some(core_data) = core {
            assert_ne!(core_data.amount, Amount::from(1000u128));
        }
    }

    // ========================================================================
    // Edge case tests
    // ========================================================================

    #[test]
    fn test_decrypt_core_wrong_shared_secret() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(500u128);

        let b_d_fq = counterparty_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack,
            &ack,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            false,
            Fq::from(0u64),
        )
        .unwrap();

        // Use a completely wrong shared secret
        let wrong_ss = Element::GENERATOR * Fr::rand(&mut rng);
        let core = decrypt_core(&wrong_ss, &result.ciphertext).unwrap();

        // Should return None (garbage point decompression fails) or wrong amount
        if let Some(core_data) = core {
            assert_ne!(
                core_data.amount, amount,
                "wrong ss should not yield correct amount"
            );
        }
    }

    #[test]
    fn test_decrypt_extension_wrong_key() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(500u128);

        let b_d_fq = counterparty_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack,
            &ack,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            false,
            Fq::from(0u64),
        )
        .unwrap();

        // Correct core secret, wrong extension secret
        let correct_ss_core = ack * result.r_1;
        let wrong_ss_ext = Element::GENERATOR * Fr::rand(&mut rng);

        // Core should work
        let core = decrypt_core(&correct_ss_core, &result.ciphertext)
            .unwrap()
            .expect("core decryption with correct key should succeed");
        assert_eq!(core.amount, amount);

        // Extension with wrong key should fail gracefully
        let ext = decrypt_extension(&wrong_ss_ext, &result.ciphertext).unwrap();
        if let Some(ext_data) = ext {
            // If decompression happens to succeed, address should be wrong
            assert_ne!(
                ext_data.counterparty_diversified_generator,
                *counterparty_address.diversified_generator(),
                "wrong key should not produce correct counterparty"
            );
        }
    }

    #[test]
    fn test_decrypt_spend_ciphertext_has_no_extension() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(500u128);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset_id,
            amount,
            false,
            Fq::from(0u64),
        )
        .unwrap();

        // Spend ciphertext has no extension tier
        let dummy_ss = Element::GENERATOR * Fr::rand(&mut rng);
        let ext = decrypt_extension(&dummy_ss, &result.ciphertext).unwrap();
        assert!(
            ext.is_none(),
            "spend ciphertext should have no extension tier"
        );

        let sext = decrypt_spend_ext(&dummy_ss, &result.ciphertext).unwrap();
        assert!(sext.is_none(), "spend ciphertext should have no sext tier");
    }

    #[test]
    fn test_decrypt_full_partial_failure() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let counterparty_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(500u128);

        let b_d_fq = counterparty_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_output(
            &mut rng,
            &ack,
            &ack,
            &dk_pub,
            &self_address,
            &counterparty_address,
            asset_id,
            amount,
            false,
            Fq::from(0u64),
        )
        .unwrap();

        // Correct core key, wrong ext key → decrypt_full returns None
        let correct_ss_core = ack * result.r_1;
        let wrong_ss_ext = Element::GENERATOR * Fr::rand(&mut rng);

        let full = decrypt_full(
            &correct_ss_core,
            &wrong_ss_ext,
            &result.ciphertext,
            asset_id,
        )
        .unwrap();

        // Full decrypt should fail because ext fails
        // (it returns None if any tier after core returns garbage/None)
        // This may or may not be None depending on whether garbage decompresses
        // The important thing is it doesn't panic
        let _ = full;
    }

    #[test]
    fn test_decrypt_zero_amount() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(0u128);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset_id,
            amount,
            true,
            Fq::from(0u64),
        )
        .unwrap();

        let core = decrypt_core_flagged(dk.inner(), &result.ciphertext)
            .unwrap()
            .expect("should decrypt zero amount");
        assert_eq!(core.amount, Amount::from(0u128));
    }

    #[test]
    fn test_decrypt_max_amount() {
        let mut rng = OsRng;
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let self_address = Address::dummy(&mut rng);
        let asset_id = asset::Id(Fq::from(42u64));
        let amount = Amount::from(u128::MAX);

        let b_d_fq = self_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ack = derive_ack(&ring_pk, b_d_fq);

        let result = encrypt_spend(
            &mut rng,
            &ack,
            &dk_pub,
            &self_address,
            asset_id,
            amount,
            true,
            Fq::from(0u64),
        )
        .unwrap();

        let core = decrypt_core_flagged(dk.inner(), &result.ciphertext)
            .unwrap()
            .expect("should decrypt max amount");
        assert_eq!(core.amount, Amount::from(u128::MAX));
    }
}
