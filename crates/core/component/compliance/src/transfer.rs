use anyhow::{anyhow, Context, Result};
use decaf377::{Element, Fq, Fr};
use rand_core::{CryptoRng, RngCore};
use shieldd_sdk_asset::Value;
use shieldd_sdk_keys::Address;

use crate::{
    crypto::{
        compliance_stream_block, encrypt_tier_bytes, transfer_key_confirmation,
        ISSUER_DETECTION_DOMAIN,
    },
    issuer_keys::detection_flag_plaintext,
    structs::{C2_BYTES, DETECTION_TAG_BYTES, EPK_BYTES, FQ_BYTES},
};

pub const TRANSFER_DETECTION_FQS: usize = 4;
pub const TRANSFER_CORE_CIPHERTEXT_FQS: usize = 1;
pub const TRANSFER_EXT_CIPHERTEXT_FQS: usize = 3;
pub const TRANSFER_CIPHERTEXT_FQS: usize = TRANSFER_DETECTION_FQS
    + TRANSFER_CORE_CIPHERTEXT_FQS
    + TRANSFER_EXT_CIPHERTEXT_FQS
    + TRANSFER_CORE_CIPHERTEXT_FQS
    + TRANSFER_EXT_CIPHERTEXT_FQS;
pub const TRANSFER_WIRE_BYTES: usize = EPK_BYTES * 4
    + C2_BYTES * 4
    + FQ_BYTES * 2
    + DETECTION_TAG_BYTES
    + FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS
    + FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS
    + FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS
    + FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS;

#[derive(Clone, Debug)]
pub struct TransferComplianceCiphertext {
    pub sender_core_epk: Element,
    pub sender_ext_epk: Element,
    pub output_core_epk: Element,
    pub output_ext_epk: Element,
    pub sender_core_c2: Fq,
    pub sender_ext_c2: Fq,
    pub output_core_c2: Fq,
    pub output_ext_c2: Fq,
    pub sender_core_key_confirmation: Fq,
    pub output_core_key_confirmation: Fq,
    pub detection_tag: [u8; DETECTION_TAG_BYTES],
    pub encrypted_sender_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS],
    pub encrypted_sender_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS],
    pub encrypted_output_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS],
    pub encrypted_output_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS],
}

#[derive(Clone, Debug)]
pub struct TransferCompliancePublicInputs {
    pub sender_core_epk: Element,
    pub sender_ext_epk: Element,
    pub output_core_epk: Element,
    pub output_ext_epk: Element,
    pub sender_core_c2: Fq,
    pub sender_ext_c2: Fq,
    pub output_core_c2: Fq,
    pub output_ext_c2: Fq,
    pub sender_core_key_confirmation: Fq,
    pub output_core_key_confirmation: Fq,
    pub detection_ciphertext: [Fq; TRANSFER_DETECTION_FQS],
    pub sender_core_ciphertext: [Fq; TRANSFER_CORE_CIPHERTEXT_FQS],
    pub sender_ext_ciphertext: [Fq; TRANSFER_EXT_CIPHERTEXT_FQS],
    pub output_core_ciphertext: [Fq; TRANSFER_CORE_CIPHERTEXT_FQS],
    pub output_ext_ciphertext: [Fq; TRANSFER_EXT_CIPHERTEXT_FQS],
}

#[derive(Clone, Debug)]
pub struct TierSecretMaterial {
    pub seed: Fq,
    pub r: Fr,
}

#[derive(Clone, Debug)]
pub struct PartyTierMaterial {
    pub core: TierSecretMaterial,
    pub ext: TierSecretMaterial,
}

#[derive(Clone, Debug)]
pub struct TransferEncryptionResult {
    pub ciphertext: TransferComplianceCiphertext,
    pub sender: PartyTierMaterial,
    pub output: PartyTierMaterial,
}

fn sample_nonzero_scalar(rng: &mut (impl RngCore + CryptoRng)) -> Fr {
    loop {
        let scalar = Fr::rand(&mut *rng);
        if scalar != Fr::from(0u64) {
            return scalar;
        }
    }
}

impl TransferComplianceCiphertext {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(TRANSFER_WIRE_BYTES);
        bytes.extend_from_slice(&self.sender_core_epk.vartime_compress().0);
        bytes.extend_from_slice(&self.sender_ext_epk.vartime_compress().0);
        bytes.extend_from_slice(&self.output_core_epk.vartime_compress().0);
        bytes.extend_from_slice(&self.output_ext_epk.vartime_compress().0);
        bytes.extend_from_slice(&self.sender_core_c2.to_bytes());
        bytes.extend_from_slice(&self.sender_ext_c2.to_bytes());
        bytes.extend_from_slice(&self.output_core_c2.to_bytes());
        bytes.extend_from_slice(&self.output_ext_c2.to_bytes());
        bytes.extend_from_slice(&self.sender_core_key_confirmation.to_bytes());
        bytes.extend_from_slice(&self.output_core_key_confirmation.to_bytes());
        bytes.extend_from_slice(&self.detection_tag);
        bytes.extend_from_slice(&self.encrypted_sender_core);
        bytes.extend_from_slice(&self.encrypted_sender_ext);
        bytes.extend_from_slice(&self.encrypted_output_core);
        bytes.extend_from_slice(&self.encrypted_output_ext);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != TRANSFER_WIRE_BYTES {
            anyhow::bail!(
                "transfer compliance ciphertext must be {TRANSFER_WIRE_BYTES} bytes, got {}",
                bytes.len()
            );
        }

        let mut offset = 0usize;
        let read_point = |offset: &mut usize| -> Result<Element> {
            let compressed: [u8; 32] = bytes[*offset..*offset + 32]
                .try_into()
                .context("read transfer compliance EPK")?;
            *offset += 32;
            decaf377::Encoding(compressed)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress transfer compliance EPK: {e:?}"))
        };
        let read_fq = |offset: &mut usize| -> Result<Fq> {
            let raw: [u8; 32] = bytes[*offset..*offset + 32]
                .try_into()
                .context("read transfer compliance Fq")?;
            *offset += 32;
            Fq::from_bytes_checked(&raw).map_err(|_| anyhow!("invalid transfer compliance Fq"))
        };
        let read_fq_words =
            |offset: &mut usize, word_count: usize, label: &str| -> Result<Vec<u8>> {
                let len = word_count
                    .checked_mul(FQ_BYTES)
                    .ok_or_else(|| anyhow!("{label} length overflow"))?;
                let value = bytes[*offset..*offset + len].to_vec();
                *offset += len;
                for (index, chunk) in value.chunks_exact(FQ_BYTES).enumerate() {
                    let raw: [u8; FQ_BYTES] = chunk
                        .try_into()
                        .expect("chunks_exact yields one field element");
                    Fq::from_bytes_checked(&raw)
                        .map_err(|_| anyhow!("invalid canonical {label} field element {index}"))?;
                }
                Ok(value)
            };
        let sender_core_epk = read_point(&mut offset)?;
        let sender_ext_epk = read_point(&mut offset)?;
        let output_core_epk = read_point(&mut offset)?;
        let output_ext_epk = read_point(&mut offset)?;
        let sender_core_c2 = read_fq(&mut offset)?;
        let sender_ext_c2 = read_fq(&mut offset)?;
        let output_core_c2 = read_fq(&mut offset)?;
        let output_ext_c2 = read_fq(&mut offset)?;
        let sender_core_key_confirmation = read_fq(&mut offset)?;
        let output_core_key_confirmation = read_fq(&mut offset)?;

        let detection_tag: [u8; DETECTION_TAG_BYTES] = read_fq_words(
            &mut offset,
            TRANSFER_DETECTION_FQS,
            "transfer detection ciphertext",
        )?
        .try_into()
        .map_err(|_| anyhow!("invalid transfer detection tag length"))?;
        let encrypted_sender_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] = read_fq_words(
            &mut offset,
            TRANSFER_CORE_CIPHERTEXT_FQS,
            "sender_core ciphertext",
        )?
        .try_into()
        .map_err(|_| anyhow!("invalid sender_core ciphertext length"))?;
        let encrypted_sender_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] = read_fq_words(
            &mut offset,
            TRANSFER_EXT_CIPHERTEXT_FQS,
            "sender_ext ciphertext",
        )?
        .try_into()
        .map_err(|_| anyhow!("invalid sender_ext ciphertext length"))?;
        let encrypted_output_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] = read_fq_words(
            &mut offset,
            TRANSFER_CORE_CIPHERTEXT_FQS,
            "output_core ciphertext",
        )?
        .try_into()
        .map_err(|_| anyhow!("invalid output_core ciphertext length"))?;
        let encrypted_output_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] = read_fq_words(
            &mut offset,
            TRANSFER_EXT_CIPHERTEXT_FQS,
            "output_ext ciphertext",
        )?
        .try_into()
        .map_err(|_| anyhow!("invalid output_ext ciphertext length"))?;

        Ok(Self {
            sender_core_epk,
            sender_ext_epk,
            output_core_epk,
            output_ext_epk,
            sender_core_c2,
            sender_ext_c2,
            output_core_c2,
            output_ext_c2,
            sender_core_key_confirmation,
            output_core_key_confirmation,
            detection_tag,
            encrypted_sender_core,
            encrypted_sender_ext,
            encrypted_output_core,
            encrypted_output_ext,
        })
    }

    pub fn to_transfer_circuit_public_inputs(&self) -> TransferCompliancePublicInputs {
        fn decode_fqs<const N: usize>(bytes: &[u8]) -> [Fq; N] {
            std::array::from_fn(|index| {
                let start = index * 32;
                let raw: [u8; 32] = bytes[start..start + 32]
                    .try_into()
                    .expect("transfer ciphertext chunk must be 32 bytes");
                Fq::from_le_bytes_mod_order(&raw)
            })
        }

        TransferCompliancePublicInputs {
            sender_core_epk: self.sender_core_epk,
            sender_ext_epk: self.sender_ext_epk,
            output_core_epk: self.output_core_epk,
            output_ext_epk: self.output_ext_epk,
            sender_core_c2: self.sender_core_c2,
            sender_ext_c2: self.sender_ext_c2,
            output_core_c2: self.output_core_c2,
            output_ext_c2: self.output_ext_c2,
            sender_core_key_confirmation: self.sender_core_key_confirmation,
            output_core_key_confirmation: self.output_core_key_confirmation,
            detection_ciphertext: decode_fqs(&self.detection_tag),
            sender_core_ciphertext: decode_fqs(&self.encrypted_sender_core),
            sender_ext_ciphertext: decode_fqs(&self.encrypted_sender_ext),
            output_core_ciphertext: decode_fqs(&self.encrypted_output_core),
            output_ext_ciphertext: decode_fqs(&self.encrypted_output_ext),
        }
    }
}

pub fn derive_transfer_salt(root: Fr, label: &[u8]) -> Fq {
    let domain = Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.transfer.compliance.salt").as_bytes(),
    );
    let label_fq = Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes());
    poseidon377::hash_2(
        &domain,
        (Fq::from_le_bytes_mod_order(&root.to_bytes()), label_fq),
    )
}

pub fn encrypt_transfer(
    mut rng: impl RngCore + CryptoRng,
    ack_sender: &Element,
    ack_receiver: &Element,
    dk_pub: &Element,
    receiver_address: &Address,
    sender_address: &Address,
    receiver_value: Value,
    is_flagged: bool,
    detection_salt: Fq,
    sender_core_salt: Fq,
    output_core_salt: Fq,
) -> Result<TransferEncryptionResult> {
    let sender = PartyTierMaterial {
        core: TierSecretMaterial {
            seed: Fq::rand(&mut rng),
            r: sample_nonzero_scalar(&mut rng),
        },
        ext: TierSecretMaterial {
            seed: Fq::rand(&mut rng),
            r: sample_nonzero_scalar(&mut rng),
        },
    };
    let output = PartyTierMaterial {
        core: TierSecretMaterial {
            seed: Fq::rand(&mut rng),
            r: sample_nonzero_scalar(&mut rng),
        },
        ext: TierSecretMaterial {
            seed: Fq::rand(&mut rng),
            r: sample_nonzero_scalar(&mut rng),
        },
    };

    let sender_core_epk = Element::GENERATOR * sender.core.r;
    let sender_ext_epk = Element::GENERATOR * sender.ext.r;
    let output_core_epk = Element::GENERATOR * output.core.r;
    let output_ext_epk = Element::GENERATOR * output.ext.r;

    let sender_core_shared = if is_flagged {
        *dk_pub * sender.core.r
    } else {
        *ack_sender * sender.core.r
    };
    let sender_ext_shared = if is_flagged {
        *dk_pub * sender.ext.r
    } else {
        *ack_sender * sender.ext.r
    };
    let output_core_shared = if is_flagged {
        *dk_pub * output.core.r
    } else {
        *ack_receiver * output.core.r
    };
    let output_ext_shared = if is_flagged {
        *dk_pub * output.ext.r
    } else {
        *ack_receiver * output.ext.r
    };

    let sender_core_c2 = sender.core.seed + sender_core_shared.vartime_compress_to_field();
    let sender_ext_c2 = sender.ext.seed + sender_ext_shared.vartime_compress_to_field();
    let output_core_c2 = output.core.seed + output_core_shared.vartime_compress_to_field();
    let output_ext_c2 = output.ext.seed + output_ext_shared.vartime_compress_to_field();
    let sender_core_key_confirmation = transfer_key_confirmation(
        sender.core.seed,
        sender_core_epk.vartime_compress_to_field(),
        sender_core_salt,
    );
    let output_core_key_confirmation = transfer_key_confirmation(
        output.core.seed,
        output_core_epk.vartime_compress_to_field(),
        output_core_salt,
    );

    let ss_detection = *dk_pub * sender.core.r;
    let sender_core_epk_fq = sender_core_epk.vartime_compress_to_field();
    let seed_detection = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss_detection.vartime_compress_to_field(), sender_core_epk_fq),
    );
    let detection_0 = receiver_value.asset_id.0 + compliance_stream_block(seed_detection, 0);
    let detection_1 = detection_salt + compliance_stream_block(seed_detection, 1);
    let detection_2 =
        detection_flag_plaintext(is_flagged) + compliance_stream_block(seed_detection, 2);
    let detection_3 = compliance_stream_block(seed_detection, 3);
    let mut detection_tag = [0u8; DETECTION_TAG_BYTES];
    detection_tag[..32].copy_from_slice(&detection_0.to_bytes());
    detection_tag[32..64].copy_from_slice(&detection_1.to_bytes());
    detection_tag[64..96].copy_from_slice(&detection_2.to_bytes());
    detection_tag[96..128].copy_from_slice(&detection_3.to_bytes());

    let amount_bytes = receiver_value.amount.to_le_bytes();
    let encrypted_sender_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&amount_bytes, sender.core.seed)
            .try_into()
            .map_err(|_| anyhow!("sender_core ciphertext must be one Fq"))?;
    let encrypted_sender_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&address_bytes(receiver_address), sender.ext.seed)
            .try_into()
            .map_err(|_| anyhow!("sender_ext ciphertext must be three Fqs"))?;
    let encrypted_output_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&amount_bytes, output.core.seed)
            .try_into()
            .map_err(|_| anyhow!("output_core ciphertext must be one Fq"))?;
    let encrypted_output_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&address_bytes(sender_address), output.ext.seed)
            .try_into()
            .map_err(|_| anyhow!("output_ext ciphertext must be three Fqs"))?;

    Ok(TransferEncryptionResult {
        ciphertext: TransferComplianceCiphertext {
            sender_core_epk,
            sender_ext_epk,
            output_core_epk,
            output_ext_epk,
            sender_core_c2,
            sender_ext_c2,
            output_core_c2,
            output_ext_c2,
            sender_core_key_confirmation,
            output_core_key_confirmation,
            detection_tag,
            encrypted_sender_core,
            encrypted_sender_ext,
            encrypted_output_core,
            encrypted_output_ext,
        },
        sender,
        output,
    })
}

fn address_bytes(address: &Address) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&address.diversified_generator().vartime_compress().0);
    bytes.extend_from_slice(&address.transmission_key().0);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::Error;

    struct ZeroThenOneRng {
        word_calls: usize,
        fill_calls: usize,
    }

    impl RngCore for ZeroThenOneRng {
        fn next_u32(&mut self) -> u32 {
            let value = if self.word_calls < 8 { 0 } else { 1 };
            self.word_calls += 1;
            value
        }

        fn next_u64(&mut self) -> u64 {
            let value = if self.word_calls < 4 { 0 } else { 1 };
            self.word_calls += 1;
            value
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(0);
            if self.fill_calls > 0 {
                dest[0] = 1;
            }
            self.fill_calls += 1;
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    impl CryptoRng for ZeroThenOneRng {}

    #[test]
    fn transfer_nonce_sampling_rejects_zero() {
        let mut rng = ZeroThenOneRng {
            word_calls: 0,
            fill_calls: 0,
        };

        assert_ne!(sample_nonzero_scalar(&mut rng), Fr::from(0u64));
        assert!(
            rng.fill_calls > 1 || rng.word_calls > 4,
            "the sampler must retry after the first all-zero scalar"
        );
    }

    fn canonical_ciphertext() -> TransferComplianceCiphertext {
        TransferComplianceCiphertext {
            sender_core_epk: Element::GENERATOR,
            sender_ext_epk: Element::GENERATOR,
            output_core_epk: Element::GENERATOR,
            output_ext_epk: Element::GENERATOR,
            sender_core_c2: Fq::from(0u64),
            sender_ext_c2: Fq::from(0u64),
            output_core_c2: Fq::from(0u64),
            output_ext_c2: Fq::from(0u64),
            sender_core_key_confirmation: Fq::from(0u64),
            output_core_key_confirmation: Fq::from(0u64),
            detection_tag: [0; DETECTION_TAG_BYTES],
            encrypted_sender_core: [0; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS],
            encrypted_sender_ext: [0; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS],
            encrypted_output_core: [0; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS],
            encrypted_output_ext: [0; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS],
        }
    }

    #[test]
    fn transfer_ciphertext_rejects_noncanonical_field_words() {
        let canonical = canonical_ciphertext().to_bytes();
        TransferComplianceCiphertext::from_bytes(&canonical)
            .expect("canonical transfer ciphertext must decode");

        let ciphertext_offset = 4 * EPK_BYTES + 4 * C2_BYTES;
        for word in 0..(2 + TRANSFER_CIPHERTEXT_FQS) {
            let mut noncanonical = canonical.clone();
            let start = ciphertext_offset + word * FQ_BYTES;
            noncanonical[start..start + FQ_BYTES].fill(0xff);
            assert!(
                TransferComplianceCiphertext::from_bytes(&noncanonical).is_err(),
                "noncanonical transfer ciphertext word {word} must be rejected"
            );
        }
    }

    #[test]
    fn transfer_ciphertext_roundtrip_preserves_key_confirmations() {
        let mut ciphertext = canonical_ciphertext();
        ciphertext.sender_core_key_confirmation = Fq::from(41u64);
        ciphertext.output_core_key_confirmation = Fq::from(42u64);

        let encoded = ciphertext.to_bytes();
        assert_eq!(encoded.len(), TRANSFER_WIRE_BYTES);
        let decoded = TransferComplianceCiphertext::from_bytes(&encoded).unwrap();
        assert_eq!(decoded.sender_core_key_confirmation, Fq::from(41u64));
        assert_eq!(decoded.output_core_key_confirmation, Fq::from(42u64));
    }
}
