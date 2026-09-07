//! Fixed-shape sender ciphertext for regulated host and ICS-20 withdrawals.

use anyhow::{anyhow, ensure, Context, Result};
use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use rand_core::{CryptoRng, RngCore};
use shieldd_sdk_keys::Address;

use crate::{
    crypto::compliance_stream_block, dleq_evidence::IssuerDhEvidence, scanning::AddressData,
    ComplianceLeaf, IndexedLeaf, FQ_BYTES,
};

pub const WITHDRAWAL_ADDRESS_BYTES: usize = 64;
pub const WITHDRAWAL_ADDRESS_CIPHERTEXT_FQS: usize = 3;
pub const WITHDRAWAL_COMPLIANCE_WIRE_BYTES: usize =
    FQ_BYTES * (3 + WITHDRAWAL_ADDRESS_CIPHERTEXT_FQS);

pub static WITHDRAWAL_KEY_CONFIRMATION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.withdrawal.compliance.key_confirmation.v1").as_bytes(),
    )
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawalComplianceCiphertext {
    pub epk: Element,
    pub c2: Fq,
    pub key_confirmation: Fq,
    pub encrypted_sender_address: [u8; FQ_BYTES * WITHDRAWAL_ADDRESS_CIPHERTEXT_FQS],
}

#[derive(Clone, Debug)]
pub struct WithdrawalEncryptionResult {
    pub ciphertext: WithdrawalComplianceCiphertext,
    pub seed: Fq,
    pub r: Fr,
}

impl WithdrawalComplianceCiphertext {
    pub fn to_bytes(&self) -> [u8; WITHDRAWAL_COMPLIANCE_WIRE_BYTES] {
        let mut bytes = [0u8; WITHDRAWAL_COMPLIANCE_WIRE_BYTES];
        bytes[..32].copy_from_slice(&self.epk.vartime_compress().0);
        bytes[32..64].copy_from_slice(&self.c2.to_bytes());
        bytes[64..96].copy_from_slice(&self.key_confirmation.to_bytes());
        bytes[96..].copy_from_slice(&self.encrypted_sender_address);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() == WITHDRAWAL_COMPLIANCE_WIRE_BYTES,
            "withdrawal compliance ciphertext must be {WITHDRAWAL_COMPLIANCE_WIRE_BYTES} bytes, got {}",
            bytes.len()
        );
        let epk = decaf377::Encoding(bytes[..32].try_into()?)
            .vartime_decompress()
            .map_err(|_| anyhow!("invalid withdrawal compliance EPK"))?;
        ensure!(
            !epk.is_identity(),
            "withdrawal compliance EPK must not be identity"
        );
        let c2 = read_fq(&bytes[32..64], "c2")?;
        let key_confirmation = read_fq(&bytes[64..96], "key_confirmation")?;
        let encrypted_sender_address: [u8; FQ_BYTES * WITHDRAWAL_ADDRESS_CIPHERTEXT_FQS] =
            bytes[96..].try_into()?;
        for (index, word) in encrypted_sender_address.chunks_exact(FQ_BYTES).enumerate() {
            read_fq(word, &format!("sender address word {index}"))?;
        }
        Ok(Self {
            epk,
            c2,
            key_confirmation,
            encrypted_sender_address,
        })
    }

    /// Classify an issuer- or PRE-authorized candidate shared point.
    pub fn decrypt_sender_if_key_matches(
        &self,
        shared_point: Element,
    ) -> Result<Option<AddressData>> {
        ensure!(
            !shared_point.is_identity(),
            "withdrawal shared point must not be identity"
        );
        let seed = self.c2 - shared_point.vartime_compress_to_field();
        if withdrawal_key_confirmation(seed, self.epk.vartime_compress_to_field())
            != self.key_confirmation
        {
            return Ok(None);
        }

        let mut plaintext = Vec::with_capacity(WITHDRAWAL_ADDRESS_BYTES);
        for (counter, word) in self
            .encrypted_sender_address
            .chunks_exact(FQ_BYTES)
            .enumerate()
        {
            let ciphertext = read_fq(word, "encrypted sender address")?;
            let decoded = (ciphertext - compliance_stream_block(seed, counter as u64)).to_bytes();
            let take = (WITHDRAWAL_ADDRESS_BYTES - plaintext.len()).min(31);
            plaintext.extend_from_slice(&decoded[..take]);
        }
        ensure!(
            plaintext.len() == WITHDRAWAL_ADDRESS_BYTES,
            "withdrawal sender address plaintext has the wrong length"
        );
        let diversified_generator_bytes: [u8; 32] = plaintext[..32]
            .try_into()
            .context("withdrawal sender diversified generator")?;
        let diversified_generator = decaf377::Encoding(diversified_generator_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow!("invalid withdrawal sender diversified generator"))?;
        ensure!(
            !diversified_generator.is_identity(),
            "withdrawal sender diversified generator must not be identity"
        );
        Ok(Some(AddressData {
            diversified_generator,
            transmission_key: plaintext[32..64]
                .try_into()
                .context("withdrawal sender transmission key")?,
        }))
    }
}

pub fn encrypt_withdrawal(
    mut rng: impl RngCore + CryptoRng,
    encryption_key: Element,
    sender_address: &Address,
) -> Result<WithdrawalEncryptionResult> {
    let r = sample_nonzero_scalar(&mut rng);
    let seed = Fq::rand(&mut rng);
    encrypt_withdrawal_with_material(encryption_key, sender_address, seed, r)
}

/// Deterministically derive material when the action body and witness are built
/// in separate passes from the same private plan.
pub fn derive_withdrawal_encryption_material(root: Fr) -> (Fq, Fr) {
    let seed_hash = blake2b_simd::blake2b(
        &[
            b"shieldd.withdrawal.compliance.seed.v1\0".as_slice(),
            &root.to_bytes(),
        ]
        .concat(),
    );
    let seed = Fq::from_le_bytes_mod_order(seed_hash.as_bytes());
    let mut counter = 0u32;
    let r = loop {
        let hash = blake2b_simd::blake2b(
            &[
                b"shieldd.withdrawal.compliance.randomizer.v1\0".as_slice(),
                &root.to_bytes(),
                &counter.to_be_bytes(),
            ]
            .concat(),
        );
        let candidate = Fr::from_le_bytes_mod_order(hash.as_bytes());
        if candidate != Fr::from(0u64) {
            break candidate;
        }
        counter += 1;
    };
    (seed, r)
}

pub fn encrypt_withdrawal_with_material(
    encryption_key: Element,
    sender_address: &Address,
    seed: Fq,
    r: Fr,
) -> Result<WithdrawalEncryptionResult> {
    ensure!(
        !encryption_key.is_identity(),
        "withdrawal compliance encryption key must not be identity"
    );
    ensure!(
        r != Fr::from(0u64),
        "withdrawal compliance randomizer must be nonzero"
    );
    let epk = Element::GENERATOR * r;
    let shared = encryption_key * r;
    let c2 = seed + shared.vartime_compress_to_field();
    let key_confirmation = withdrawal_key_confirmation(seed, epk.vartime_compress_to_field());

    let plaintext = address_components(sender_address);
    let mut encrypted_sender_address = [0u8; FQ_BYTES * WITHDRAWAL_ADDRESS_CIPHERTEXT_FQS];
    for (counter, (chunk, output)) in plaintext
        .chunks(31)
        .zip(encrypted_sender_address.chunks_exact_mut(FQ_BYTES))
        .enumerate()
    {
        let mut padded = [0u8; 32];
        padded[..chunk.len()].copy_from_slice(chunk);
        let ciphertext =
            Fq::from_le_bytes_mod_order(&padded) + compliance_stream_block(seed, counter as u64);
        output.copy_from_slice(&ciphertext.to_bytes());
    }

    Ok(WithdrawalEncryptionResult {
        ciphertext: WithdrawalComplianceCiphertext {
            epk,
            c2,
            key_confirmation,
            encrypted_sender_address,
        },
        seed,
        r,
    })
}

pub fn withdrawal_key_confirmation(seed: Fq, epk_fq: Fq) -> Fq {
    poseidon377::hash_2(&WITHDRAWAL_KEY_CONFIRMATION_DOMAIN, (seed, epk_fq))
}

pub fn withdrawal_encryption_key(
    is_regulated: bool,
    is_flagged: bool,
    sender_leaf: &ComplianceLeaf,
    asset_leaf: &IndexedLeaf,
) -> Result<(Element, bool)> {
    if is_regulated {
        ensure!(
            sender_leaf.asset_id.0 == asset_leaf.value,
            "withdrawal sender and asset registry leaves disagree on regulated asset"
        );
    }
    ensure!(
        !is_flagged || is_regulated,
        "unregulated withdrawal cannot be flagged"
    );
    let key = if is_flagged {
        asset_leaf.params.dk_pub
    } else {
        sender_leaf.capk
    };
    ensure!(
        !key.is_identity(),
        "withdrawal compliance selected an identity encryption key"
    );
    Ok((key, is_flagged))
}

/// Verify issuer DH evidence for this exact asset and ciphertext before opening it.
pub fn classify_withdrawal_with_issuer(
    ciphertext: &WithdrawalComplianceCiphertext,
    evidence: &IssuerDhEvidence,
    expected_asset_id: [u8; 32],
) -> Result<AddressData> {
    ensure!(
        evidence.asset_id == expected_asset_id,
        "withdrawal issuer evidence is for a different asset"
    );
    ensure!(
        evidence.ciphertext_epk == ciphertext.epk,
        "withdrawal issuer evidence is for a different ciphertext EPK"
    );
    ciphertext
        .decrypt_sender_if_key_matches(evidence.verify()?)?
        .ok_or_else(|| anyhow!("withdrawal issuer evidence did not open the accepted ciphertext"))
}

pub fn address_components(address: &Address) -> [u8; WITHDRAWAL_ADDRESS_BYTES] {
    let mut bytes = [0u8; WITHDRAWAL_ADDRESS_BYTES];
    bytes[..32].copy_from_slice(&address.diversified_generator().vartime_compress().0);
    bytes[32..].copy_from_slice(&address.transmission_key().0);
    bytes
}

fn sample_nonzero_scalar(rng: &mut (impl RngCore + CryptoRng)) -> Fr {
    loop {
        let scalar = Fr::rand(&mut *rng);
        if scalar != Fr::from(0u64) {
            return scalar;
        }
    }
}

fn read_fq(bytes: &[u8], label: &str) -> Result<Fq> {
    let bytes: [u8; FQ_BYTES] = bytes
        .try_into()
        .with_context(|| format!("withdrawal compliance {label} must be {FQ_BYTES} bytes"))?;
    Fq::from_bytes_checked(&bytes)
        .map_err(|_| anyhow!("withdrawal compliance {label} is not canonical"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::make_address;
    use rand_core::OsRng;

    #[test]
    fn withdrawal_ciphertext_round_trips_and_classifies_wrong_keys() {
        let secret = Fr::from(17u64);
        let encryption_key = Element::GENERATOR * secret;
        let sender = make_address(73);
        let encrypted = encrypt_withdrawal(&mut OsRng, encryption_key, &sender).unwrap();
        let encoded = encrypted.ciphertext.to_bytes();
        let decoded = WithdrawalComplianceCiphertext::from_bytes(&encoded).unwrap();

        let opened = decoded
            .decrypt_sender_if_key_matches(decoded.epk * secret)
            .unwrap()
            .unwrap();
        assert_eq!(
            opened.diversified_generator,
            *sender.diversified_generator()
        );
        assert_eq!(opened.transmission_key, sender.transmission_key().0);
        assert!(decoded
            .decrypt_sender_if_key_matches(decoded.epk * Fr::from(19u64))
            .unwrap()
            .is_none());
    }

    #[test]
    fn withdrawal_ciphertext_rejects_noncanonical_fields_and_identity_epk() {
        let sender = make_address(74);
        let encrypted = encrypt_withdrawal(&mut OsRng, Element::GENERATOR, &sender).unwrap();
        let mut identity = encrypted.ciphertext.to_bytes();
        identity[..32].copy_from_slice(&Element::IDENTITY.vartime_compress().0);
        assert!(WithdrawalComplianceCiphertext::from_bytes(&identity).is_err());

        let mut noncanonical = encrypted.ciphertext.to_bytes();
        noncanonical[32..64].fill(0xff);
        assert!(WithdrawalComplianceCiphertext::from_bytes(&noncanonical).is_err());
    }
}
