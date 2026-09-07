//! Flagged compliance decryption helpers.

use anyhow::{ensure, Context};
use decaf377::{Element, Fq, Fr};
use shieldd_sdk_asset::asset;
use shieldd_sdk_num::Amount;

use crate::crypto::{
    compliance_stream_block, decrypt_detection_tier, decrypt_tier_bytes, transfer_key_confirmation,
};
use crate::decode_object::TransferComplianceMetadata;
use crate::transfer::TransferComplianceCiphertext;
use crate::withdrawal::WithdrawalComplianceCiphertext;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressData {
    pub diversified_generator: Element,
    pub transmission_key: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullComplianceData {
    pub asset_id: asset::Id,
    pub amount: Amount,
    pub sender_address: AddressData,
    pub receiver_address: AddressData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawalComplianceData {
    pub asset_id: asset::Id,
    pub sender_address: AddressData,
}

fn decrypt_amount_with_seed(seed: decaf377::Fq, encrypted: &[u8]) -> anyhow::Result<Amount> {
    let encrypted: [u8; 32] = encrypted
        .try_into()
        .context("transfer amount ciphertext must be one field element")?;
    let ciphertext = Fq::from_bytes_checked(&encrypted)
        .map_err(|_| anyhow::anyhow!("transfer amount ciphertext is not canonical"))?;
    let plaintext = (ciphertext - compliance_stream_block(seed, 0)).to_bytes();
    ensure!(
        plaintext[16..].iter().all(|byte| *byte == 0),
        "transfer amount plaintext exceeds 128 bits"
    );
    let amount_bytes: [u8; 16] = plaintext[..16]
        .try_into()
        .expect("amount slice is exactly 16 bytes");
    Ok(Amount::from_le_bytes(amount_bytes))
}

/// Decrypt one core tier only when the candidate shared secret reproduces its
/// non-indexing key confirmation. A mismatch is an authenticated non-match.
pub fn decrypt_core_amount_if_key_matches(
    shared_secret: &Element,
    epk: &Element,
    c2: Fq,
    key_confirmation: Fq,
    tier_salt: Fq,
    encrypted: &[u8],
) -> anyhow::Result<Option<Amount>> {
    let seed = c2 - shared_secret.vartime_compress_to_field();
    let expected = transfer_key_confirmation(seed, epk.vartime_compress_to_field(), tier_salt);
    if expected != key_confirmation {
        return Ok(None);
    }
    decrypt_amount_with_seed(seed, encrypted).map(Some)
}

fn decrypt_address_with_seed(seed: decaf377::Fq, encrypted: &[u8]) -> anyhow::Result<AddressData> {
    let plaintext = decrypt_tier_bytes(encrypted, seed, 64);
    let diversified_generator_bytes: [u8; 32] = plaintext[..32]
        .try_into()
        .context("transfer address diversified generator must be 32 bytes")?;
    let transmission_key: [u8; 32] = plaintext[32..64]
        .try_into()
        .context("transfer address transmission key must be 32 bytes")?;
    let diversified_generator = decaf377::Encoding(diversified_generator_bytes)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid transfer address diversified generator"))?;
    Ok(AddressData {
        diversified_generator,
        transmission_key,
    })
}

/// Fully decrypt a flagged transfer compliance ciphertext with the issuer detection key.
pub fn decrypt_full_flagged(
    dk_secret: &Fr,
    ciphertext: &TransferComplianceCiphertext,
    metadata: &TransferComplianceMetadata,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    metadata.validate()?;
    let (_, is_flagged, _) = decrypt_detection_tier(
        dk_secret,
        &ciphertext.sender_core_epk,
        &ciphertext.detection_tag,
        &asset_id,
    )?;
    if !is_flagged {
        return Ok(None);
    }

    let sender_ext_seed = ciphertext.sender_ext_c2
        - (ciphertext.sender_ext_epk * *dk_secret).vartime_compress_to_field();
    let output_ext_seed = ciphertext.output_ext_c2
        - (ciphertext.output_ext_epk * *dk_secret).vartime_compress_to_field();

    let sender_amount = decrypt_core_amount_if_key_matches(
        &(ciphertext.sender_core_epk * *dk_secret),
        &ciphertext.sender_core_epk,
        ciphertext.sender_core_c2,
        ciphertext.sender_core_key_confirmation,
        metadata.sender_core_salt()?,
        &ciphertext.encrypted_sender_core,
    )?
    .context("issuer key does not match sender core tier")?;
    let receiver_amount = decrypt_core_amount_if_key_matches(
        &(ciphertext.output_core_epk * *dk_secret),
        &ciphertext.output_core_epk,
        ciphertext.output_core_c2,
        ciphertext.output_core_key_confirmation,
        metadata.output_core_salt()?,
        &ciphertext.encrypted_output_core,
    )?
    .context("issuer key does not match output core tier")?;
    ensure!(
        sender_amount == receiver_amount,
        "transfer compliance amount mismatch between sender and receiver tiers"
    );

    let receiver_address =
        decrypt_address_with_seed(sender_ext_seed, &ciphertext.encrypted_sender_ext)?;
    let sender_address =
        decrypt_address_with_seed(output_ext_seed, &ciphertext.encrypted_output_ext)?;

    Ok(Some(FullComplianceData {
        asset_id,
        amount: sender_amount,
        sender_address,
        receiver_address,
    }))
}

/// Decrypt the private sender of a flagged withdrawal; amount and destination are public.
pub fn decrypt_flagged_withdrawal_sender(
    dk_secret: &Fr,
    ciphertext: &WithdrawalComplianceCiphertext,
    asset_id: asset::Id,
) -> anyhow::Result<Option<WithdrawalComplianceData>> {
    Ok(ciphertext
        .decrypt_sender_if_key_matches(ciphertext.epk * *dk_secret)?
        .map(|sender_address| WithdrawalComplianceData {
            asset_id,
            sender_address,
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_compliance_scalar;
    use crate::issuer_keys::DetectionKey;
    use crate::test_helpers::make_address;
    use crate::transfer::encrypt_transfer;
    use rand_core::OsRng;
    use shieldd_sdk_asset::Value;

    fn derive_ack(
        ring_pk: &decaf377::Element,
        address: &shieldd_sdk_keys::Address,
    ) -> decaf377::Element {
        let d = derive_compliance_scalar(address);
        let d_fr = decaf377::Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
    }

    fn metadata(sender_core_salt: Fq, output_core_salt: Fq) -> TransferComplianceMetadata {
        TransferComplianceMetadata::from_identifiers(
            "ring",
            "policy",
            "resource",
            "permission",
            1,
            sender_core_salt,
            Fq::from(12u64),
            output_core_salt,
            Fq::from(14u64),
        )
    }

    #[test]
    fn test_decrypt_full_flagged_transfer() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(31);
        let receiver_address = make_address(32);
        let asset_id = asset::Id(decaf377::Fq::from(4242u64));
        let amount = Amount::from(1_000_000u128);

        let sender_core_salt = decaf377::Fq::from(1u64);
        let output_core_salt = decaf377::Fq::from(2u64);
        let ciphertext = encrypt_transfer(
            &mut OsRng,
            &derive_ack(&ring_pk, &sender_address),
            &derive_ack(&ring_pk, &receiver_address),
            &dk_pub,
            &receiver_address,
            &sender_address,
            Value { amount, asset_id },
            true,
            decaf377::Fq::from(0u64),
            sender_core_salt,
            output_core_salt,
        )
        .unwrap()
        .ciphertext;

        let decrypted = decrypt_full_flagged(
            dk.inner(),
            &ciphertext,
            &metadata(sender_core_salt, output_core_salt),
            asset_id,
        )
        .unwrap()
        .expect("flagged transfer should decrypt");

        assert_eq!(decrypted.asset_id, asset_id);
        assert_eq!(decrypted.amount, amount);
        assert_eq!(
            decrypted.sender_address.transmission_key,
            sender_address.transmission_key().0
        );
        assert_eq!(
            decrypted.receiver_address.transmission_key,
            receiver_address.transmission_key().0
        );
    }

    #[test]
    fn test_decrypt_full_flagged_rejects_unflagged_transfer() {
        let dk = DetectionKey::demo();
        let dk_pub = dk.public_key();
        let ring_pk = decaf377::Element::GENERATOR * decaf377::Fr::rand(&mut OsRng);
        let sender_address = make_address(41);
        let receiver_address = make_address(42);
        let asset_id = asset::Id(decaf377::Fq::from(999u64));

        let sender_core_salt = decaf377::Fq::from(2u64);
        let output_core_salt = decaf377::Fq::from(3u64);
        let ciphertext = encrypt_transfer(
            &mut OsRng,
            &derive_ack(&ring_pk, &sender_address),
            &derive_ack(&ring_pk, &receiver_address),
            &dk_pub,
            &receiver_address,
            &sender_address,
            Value {
                amount: Amount::from(5u128),
                asset_id,
            },
            false,
            decaf377::Fq::from(1u64),
            sender_core_salt,
            output_core_salt,
        )
        .unwrap()
        .ciphertext;

        assert!(decrypt_full_flagged(
            dk.inner(),
            &ciphertext,
            &metadata(sender_core_salt, output_core_salt),
            asset_id,
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn core_key_confirmation_distinguishes_match_from_non_match() {
        let epk = Element::GENERATOR * Fr::from(7u64);
        let shared = Element::GENERATOR * Fr::from(11u64);
        let seed = Fq::from(19u64);
        let salt = Fq::from(23u64);
        let amount = Amount::from(29u128);
        let c2 = seed + shared.vartime_compress_to_field();
        let encrypted = crate::crypto::encrypt_tier_bytes(&amount.to_le_bytes(), seed);
        let confirmation = transfer_key_confirmation(seed, epk.vartime_compress_to_field(), salt);

        assert_eq!(
            decrypt_core_amount_if_key_matches(&shared, &epk, c2, confirmation, salt, &encrypted,)
                .unwrap(),
            Some(amount),
        );
        assert_eq!(
            decrypt_core_amount_if_key_matches(
                &(shared + Element::GENERATOR),
                &epk,
                c2,
                confirmation,
                salt,
                &encrypted,
            )
            .unwrap(),
            None,
        );
    }
}
