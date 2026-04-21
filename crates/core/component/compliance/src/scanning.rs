//! Transfer-only flagged compliance decryption helpers.

use anyhow::{ensure, Context};
use decaf377::{Element, Fr};
use penumbra_sdk_asset::asset;
use penumbra_sdk_num::Amount;

use crate::crypto::{decrypt_detection_tier, decrypt_tier_bytes};
use crate::transfer::TransferComplianceCiphertext;

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

fn decrypt_amount_with_seed(seed: decaf377::Fq, encrypted: &[u8]) -> anyhow::Result<Amount> {
    let plaintext = decrypt_tier_bytes(encrypted, seed, 16);
    let amount_bytes: [u8; 16] = plaintext[..16]
        .try_into()
        .context("transfer amount plaintext must be 16 bytes")?;
    Ok(Amount::from_le_bytes(amount_bytes))
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
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    let (_, is_flagged, _) = decrypt_detection_tier(
        dk_secret,
        &ciphertext.sender_core_epk,
        &ciphertext.detection_tag,
        &asset_id,
    )?;
    if !is_flagged {
        return Ok(None);
    }

    let sender_core_seed = ciphertext.sender_core_c2
        - (ciphertext.sender_core_epk * *dk_secret).vartime_compress_to_field();
    let sender_ext_seed = ciphertext.sender_ext_c2
        - (ciphertext.sender_ext_epk * *dk_secret).vartime_compress_to_field();
    let output_core_seed = ciphertext.output_core_c2
        - (ciphertext.output_core_epk * *dk_secret).vartime_compress_to_field();
    let output_ext_seed = ciphertext.output_ext_c2
        - (ciphertext.output_ext_epk * *dk_secret).vartime_compress_to_field();

    let sender_amount =
        decrypt_amount_with_seed(sender_core_seed, &ciphertext.encrypted_sender_core)?;
    let receiver_amount =
        decrypt_amount_with_seed(output_core_seed, &ciphertext.encrypted_output_core)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_compliance_scalar;
    use crate::issuer_keys::DetectionKey;
    use crate::test_helpers::make_address;
    use crate::transfer::encrypt_transfer;
    use penumbra_sdk_asset::Value;
    use rand_core::OsRng;

    fn derive_ack(
        ring_pk: &decaf377::Element,
        address: &penumbra_sdk_keys::Address,
    ) -> decaf377::Element {
        let b_d_fq = address.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = decaf377::Fr::from_le_bytes_mod_order(&d.to_bytes());
        *ring_pk * d_fr
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
        )
        .unwrap()
        .ciphertext;

        let decrypted = decrypt_full_flagged(dk.inner(), &ciphertext, asset_id)
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
        )
        .unwrap()
        .ciphertext;

        assert!(decrypt_full_flagged(dk.inner(), &ciphertext, asset_id)
            .unwrap()
            .is_none());
    }
}
