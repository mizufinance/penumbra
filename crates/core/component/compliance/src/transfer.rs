use anyhow::{anyhow, Context, Result};
use decaf377::{Element, Fq, Fr};
use penumbra_sdk_asset::Value;
use penumbra_sdk_keys::Address;
use rand_core::{CryptoRng, RngCore};

use crate::{
    crypto::{
        compute_dleq_native, compute_metadata_hash, encrypt_tier_bytes, ISSUER_DETECTION_DOMAIN,
    },
    issuer_keys::detection_plaintext_fq,
    structs::{DleqProof, C2_BYTES, DETECTION_TAG_BYTES, EPK_BYTES, FQ_BYTES},
};

pub const TRANSFER_DETECTION_FQS: usize = 2;
pub const TRANSFER_CORE_CIPHERTEXT_FQS: usize = 1;
pub const TRANSFER_EXT_CIPHERTEXT_FQS: usize = 3;
pub const TRANSFER_CIPHERTEXT_FQS: usize = TRANSFER_DETECTION_FQS
    + TRANSFER_CORE_CIPHERTEXT_FQS
    + TRANSFER_EXT_CIPHERTEXT_FQS
    + TRANSFER_CORE_CIPHERTEXT_FQS
    + TRANSFER_EXT_CIPHERTEXT_FQS;
pub const TRANSFER_WIRE_BYTES: usize = EPK_BYTES * 4
    + C2_BYTES * 4
    + DETECTION_TAG_BYTES
    + FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS
    + FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS
    + FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS
    + FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS;
pub const TRANSFER_DLEQ_BYTES: usize = FQ_BYTES * 8;

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
    pub detection_ciphertext: [Fq; TRANSFER_DETECTION_FQS],
    pub sender_core_ciphertext: [Fq; TRANSFER_CORE_CIPHERTEXT_FQS],
    pub sender_ext_ciphertext: [Fq; TRANSFER_EXT_CIPHERTEXT_FQS],
    pub output_core_ciphertext: [Fq; TRANSFER_CORE_CIPHERTEXT_FQS],
    pub output_ext_ciphertext: [Fq; TRANSFER_EXT_CIPHERTEXT_FQS],
}

#[derive(Clone, Debug)]
pub struct TransferComplianceDleqProofs {
    pub sender_core: DleqProof,
    pub sender_ext: DleqProof,
    pub output_core: DleqProof,
    pub output_ext: DleqProof,
}

#[derive(Clone, Debug)]
pub struct TransferEncryptionResult {
    pub ciphertext: TransferComplianceCiphertext,
    pub sender_r_core: Fr,
    pub sender_r_ext: Fr,
    pub output_r_core: Fr,
    pub output_r_ext: Fr,
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
        let read_fixed = |offset: &mut usize, len: usize| -> Result<Vec<u8>> {
            let value = bytes[*offset..*offset + len].to_vec();
            *offset += len;
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

        let detection_tag: [u8; DETECTION_TAG_BYTES] =
            read_fixed(&mut offset, DETECTION_TAG_BYTES)?
                .try_into()
                .map_err(|_| anyhow!("invalid transfer detection tag length"))?;
        let encrypted_sender_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] =
            read_fixed(&mut offset, FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS)?
                .try_into()
                .map_err(|_| anyhow!("invalid sender_core ciphertext length"))?;
        let encrypted_sender_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] =
            read_fixed(&mut offset, FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS)?
                .try_into()
                .map_err(|_| anyhow!("invalid sender_ext ciphertext length"))?;
        let encrypted_output_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] =
            read_fixed(&mut offset, FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS)?
                .try_into()
                .map_err(|_| anyhow!("invalid output_core ciphertext length"))?;
        let encrypted_output_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] =
            read_fixed(&mut offset, FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS)?
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
            detection_ciphertext: decode_fqs(&self.detection_tag),
            sender_core_ciphertext: decode_fqs(&self.encrypted_sender_core),
            sender_ext_ciphertext: decode_fqs(&self.encrypted_sender_ext),
            output_core_ciphertext: decode_fqs(&self.encrypted_output_core),
            output_ext_ciphertext: decode_fqs(&self.encrypted_output_ext),
        }
    }
}

impl TransferComplianceDleqProofs {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(TRANSFER_DLEQ_BYTES);
        bytes.extend_from_slice(&self.sender_core.to_bytes());
        bytes.extend_from_slice(&self.sender_ext.to_bytes());
        bytes.extend_from_slice(&self.output_core.to_bytes());
        bytes.extend_from_slice(&self.output_ext.to_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != TRANSFER_DLEQ_BYTES {
            anyhow::bail!(
                "transfer compliance DLEQ bundle must be {TRANSFER_DLEQ_BYTES} bytes, got {}",
                bytes.len()
            );
        }
        let parse = |offset: usize| -> DleqProof {
            let raw: [u8; 64] = bytes[offset..offset + 64]
                .try_into()
                .expect("transfer DLEQ proof must be 64 bytes");
            DleqProof::from_bytes(&raw)
        };
        Ok(Self {
            sender_core: parse(0),
            sender_ext: parse(64),
            output_core: parse(128),
            output_ext: parse(192),
        })
    }
}

pub fn derive_transfer_salt(root: Fr, label: &[u8]) -> Fq {
    let domain = Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.transfer.compliance.salt").as_bytes(),
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
) -> Result<TransferEncryptionResult> {
    let sender_r_core = Fr::rand(&mut rng);
    let sender_r_ext = Fr::rand(&mut rng);
    let output_r_core = Fr::rand(&mut rng);
    let output_r_ext = Fr::rand(&mut rng);

    let sender_core_epk = Element::GENERATOR * sender_r_core;
    let sender_ext_epk = Element::GENERATOR * sender_r_ext;
    let output_core_epk = Element::GENERATOR * output_r_core;
    let output_ext_epk = Element::GENERATOR * output_r_ext;

    let sender_core_seed = Fq::rand(&mut rng);
    let sender_ext_seed = Fq::rand(&mut rng);
    let output_core_seed = Fq::rand(&mut rng);
    let output_ext_seed = Fq::rand(&mut rng);

    let sender_core_shared = if is_flagged {
        *dk_pub * sender_r_core
    } else {
        *ack_sender * sender_r_core
    };
    let sender_ext_shared = if is_flagged {
        *dk_pub * sender_r_ext
    } else {
        *ack_sender * sender_r_ext
    };
    let output_core_shared = if is_flagged {
        *dk_pub * output_r_core
    } else {
        *ack_receiver * output_r_core
    };
    let output_ext_shared = if is_flagged {
        *dk_pub * output_r_ext
    } else {
        *ack_receiver * output_r_ext
    };

    let sender_core_c2 = sender_core_seed + sender_core_shared.vartime_compress_to_field();
    let sender_ext_c2 = sender_ext_seed + sender_ext_shared.vartime_compress_to_field();
    let output_core_c2 = output_core_seed + output_core_shared.vartime_compress_to_field();
    let output_ext_c2 = output_ext_seed + output_ext_shared.vartime_compress_to_field();

    let ss_detection = *dk_pub * sender_r_core;
    let sender_core_epk_fq = sender_core_epk.vartime_compress_to_field();
    let seed_detection = poseidon377::hash_2(
        &ISSUER_DETECTION_DOMAIN,
        (ss_detection.vartime_compress_to_field(), sender_core_epk_fq),
    );
    let detection_0 = detection_plaintext_fq(&receiver_value.asset_id, is_flagged)
        + poseidon377::hash_2(&seed_detection, (Fq::from(0u64), seed_detection));
    let detection_1 =
        detection_salt + poseidon377::hash_2(&seed_detection, (Fq::from(1u64), seed_detection));
    let mut detection_tag = [0u8; DETECTION_TAG_BYTES];
    detection_tag[..32].copy_from_slice(&detection_0.to_bytes());
    detection_tag[32..].copy_from_slice(&detection_1.to_bytes());

    let amount_bytes = receiver_value.amount.to_le_bytes();
    let encrypted_sender_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&amount_bytes, sender_core_seed)
            .try_into()
            .map_err(|_| anyhow!("sender_core ciphertext must be one Fq"))?;
    let encrypted_sender_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&address_bytes(receiver_address), sender_ext_seed)
            .try_into()
            .map_err(|_| anyhow!("sender_ext ciphertext must be three Fqs"))?;
    let encrypted_output_core: [u8; FQ_BYTES * TRANSFER_CORE_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&amount_bytes, output_core_seed)
            .try_into()
            .map_err(|_| anyhow!("output_core ciphertext must be one Fq"))?;
    let encrypted_output_ext: [u8; FQ_BYTES * TRANSFER_EXT_CIPHERTEXT_FQS] =
        encrypt_tier_bytes(&address_bytes(sender_address), output_ext_seed)
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
            detection_tag,
            encrypted_sender_core,
            encrypted_sender_ext,
            encrypted_output_core,
            encrypted_output_ext,
        },
        sender_r_core,
        sender_r_ext,
        output_r_core,
        output_r_ext,
    })
}

pub fn compute_transfer_dleqs(
    sender_r_core: Fr,
    sender_r_ext: Fr,
    output_r_core: Fr,
    output_r_ext: Fr,
    sender_k_core: Fr,
    sender_k_ext: Fr,
    output_k_core: Fr,
    output_k_ext: Fr,
    ack_sender: &Element,
    ack_receiver: &Element,
    policy_id_hash: Fq,
    resource_hash: Fq,
    permission_hash: Fq,
    sender_core_salt: Fq,
    sender_ext_salt: Fq,
    output_core_salt: Fq,
    output_ext_salt: Fq,
    target_timestamp: u64,
) -> TransferComplianceDleqProofs {
    let sender_core_epk = Element::GENERATOR * sender_r_core;
    let sender_ext_epk = Element::GENERATOR * sender_r_ext;
    let output_core_epk = Element::GENERATOR * output_r_core;
    let output_ext_epk = Element::GENERATOR * output_r_ext;

    let sender_core_metadata = compute_metadata_hash(
        policy_id_hash,
        resource_hash,
        permission_hash,
        Fq::from(1u64),
        Fq::from(target_timestamp),
        sender_core_salt,
    );
    let sender_ext_metadata = compute_metadata_hash(
        policy_id_hash,
        resource_hash,
        permission_hash,
        Fq::from(2u64),
        Fq::from(target_timestamp),
        sender_ext_salt,
    );
    let output_core_metadata = compute_metadata_hash(
        policy_id_hash,
        resource_hash,
        permission_hash,
        Fq::from(3u64),
        Fq::from(target_timestamp),
        output_core_salt,
    );
    let output_ext_metadata = compute_metadata_hash(
        policy_id_hash,
        resource_hash,
        permission_hash,
        Fq::from(4u64),
        Fq::from(target_timestamp),
        output_ext_salt,
    );

    TransferComplianceDleqProofs {
        sender_core: compute_dleq_native(
            sender_r_core,
            sender_k_core,
            ack_sender,
            &sender_core_epk,
            sender_core_metadata,
        ),
        sender_ext: compute_dleq_native(
            sender_r_ext,
            sender_k_ext,
            ack_sender,
            &sender_ext_epk,
            sender_ext_metadata,
        ),
        output_core: compute_dleq_native(
            output_r_core,
            output_k_core,
            ack_receiver,
            &output_core_epk,
            output_core_metadata,
        ),
        output_ext: compute_dleq_native(
            output_r_ext,
            output_k_ext,
            ack_receiver,
            &output_ext_epk,
            output_ext_metadata,
        ),
    }
}

fn address_bytes(address: &Address) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&address.diversified_generator().vartime_compress().0);
    bytes.extend_from_slice(&address.transmission_key().0);
    bytes
}
