use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, bail, Context, Result};
use decaf377::{Element, Encoding, Fq, Fr};
use hkdf::Hkdf;
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

use crate::{indexed_tree::string_to_fq, TransferComplianceMetadata};

const ENCRYPT_PROOF_DOMAIN: &[u8] = b"elgamal-encrypt-proof";
const AAD_DOMAIN: &[u8] = b"elgamal-aad\0";
const DERIVATION_DOMAIN: &[u8] = b"elgamal-derivation-v1\0\0";
const POLICY_METADATA_DOMAIN: &[u8] = b"orbis-policy-metadata";
const TRANSFER_SEED_BINDING_DOMAIN: &[u8] = b"shieldd-transfer-seed-binding";
const HKDF_INFO: &[u8] = b"elgamal-aes-key";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PocOrbisAccess {
    Subject,
    Investigation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PocOrbisTier {
    SenderCore,
    SenderExt,
    OutputCore,
    OutputExt,
}

impl PocOrbisTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::SenderCore => "sender_core",
            Self::SenderExt => "sender_ext",
            Self::OutputCore => "output_core",
            Self::OutputExt => "output_ext",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "sender_core" => Some(Self::SenderCore),
            "sender_ext" => Some(Self::SenderExt),
            "output_core" => Some(Self::OutputCore),
            "output_ext" => Some(Self::OutputExt),
            _ => None,
        }
    }

    /// Whether this tier is the canonical package for its disclosed field.
    ///
    /// Subject packages keep both core tiers because their derivation paths
    /// identify opposite sides of a transfer. Investigation packages use the
    /// Orbis ring key directly, so both core tiers disclose the same amount and
    /// `sender_core` is the canonical representation.
    pub fn is_canonical_for_access(self, access: PocOrbisAccess) -> bool {
        !matches!(
            (access, self),
            (PocOrbisAccess::Investigation, Self::OutputCore)
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocOrbisSecretEnvelope {
    pub enc_cmt: Vec<u8>,
    pub encrypted_data: Vec<u8>,
    pub nonce: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocOrbisAuditPackage {
    pub access: PocOrbisAccess,
    pub derivation: Option<Vec<u8>>,
    pub ring_id: String,
    pub policy_id: String,
    pub resource: String,
    pub permission: String,
    pub tier_label: String,
    pub timestamp: u64,
    pub salt: String,
    pub encrypted_document: Vec<u8>,
    pub enc_cmt: Vec<u8>,
    pub shared_point: Vec<u8>,
    pub orbis_challenge: Vec<u8>,
    pub orbis_response: Vec<u8>,
    pub effective_pk: Vec<u8>,
    pub metadata_hash: Vec<u8>,
    pub transfer_epk: Vec<u8>,
    pub transfer_c2: Vec<u8>,
    pub transfer_seed_binding: Vec<u8>,
    pub tier_ciphertext: Vec<u8>,
}

/// The decryption projection of an Orbis audit package after PRE succeeds.
///
/// The complete `PocOrbisAuditPackage` must remain available for a later inline
/// Orbis request. After PRE returns the re-encrypted seed, this smaller form is
/// sufficient for tier decryption and keeps those helpers narrowly typed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocOrbisStoredAuditPackage {
    pub access: PocOrbisAccess,
    pub derivation: Option<Vec<u8>>,
    pub ring_id: String,
    pub policy_id: String,
    pub resource: String,
    pub permission: String,
    pub tier_label: String,
    pub timestamp: u64,
    pub salt: String,
    pub effective_pk: Vec<u8>,
    pub transfer_epk: Vec<u8>,
    pub transfer_c2: Vec<u8>,
    pub transfer_seed_binding: Vec<u8>,
    pub tier_ciphertext: Vec<u8>,
}

impl From<&PocOrbisAuditPackage> for PocOrbisStoredAuditPackage {
    fn from(package: &PocOrbisAuditPackage) -> Self {
        Self {
            access: package.access,
            derivation: package.derivation.clone(),
            ring_id: package.ring_id.clone(),
            policy_id: package.policy_id.clone(),
            resource: package.resource.clone(),
            permission: package.permission.clone(),
            tier_label: package.tier_label.clone(),
            timestamp: package.timestamp,
            salt: package.salt.clone(),
            effective_pk: package.effective_pk.clone(),
            transfer_epk: package.transfer_epk.clone(),
            transfer_c2: package.transfer_c2.clone(),
            transfer_seed_binding: package.transfer_seed_binding.clone(),
            tier_ciphertext: package.tier_ciphertext.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocOrbisTierBundle {
    pub sender_core: PocOrbisAuditPackage,
    pub sender_ext: PocOrbisAuditPackage,
    pub output_core: PocOrbisAuditPackage,
    pub output_ext: PocOrbisAuditPackage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocOrbisAuditBundle {
    pub subject: PocOrbisTierBundle,
    pub investigation: PocOrbisTierBundle,
}

#[allow(clippy::too_many_arguments)]
pub fn build_poc_orbis_audit_package(
    rng: &mut (impl RngCore + CryptoRng),
    ring_pk: &Element,
    seed: Fq,
    derivation: Option<Vec<u8>>,
    metadata: &TransferComplianceMetadata,
    ring_id: &str,
    policy_id: &str,
    resource: &str,
    permission: &str,
    tier: PocOrbisTier,
    transfer_epk: &Element,
    transfer_c2: Fq,
    tier_ciphertext: &[u8],
) -> Result<PocOrbisAuditPackage> {
    metadata.validate()?;
    anyhow::ensure!(
        string_to_fq(ring_id) == metadata.ring_id_hash()?,
        "ring_id mismatch"
    );
    anyhow::ensure!(
        string_to_fq(policy_id) == metadata.policy_id_hash()?,
        "policy_id mismatch"
    );
    anyhow::ensure!(
        string_to_fq(resource) == metadata.resource_hash()?,
        "resource mismatch"
    );
    anyhow::ensure!(
        string_to_fq(permission) == metadata.permission_hash()?,
        "permission mismatch"
    );

    let access = if derivation.is_some() {
        PocOrbisAccess::Subject
    } else {
        PocOrbisAccess::Investigation
    };
    let effective_pk = match derivation.as_deref() {
        Some(path) => {
            let scalar = derive_capability_scalar(path);
            anyhow::ensure!(scalar != Fr::from(0u64), "derivation produced zero scalar");
            *ring_pk * scalar
        }
        None => *ring_pk,
    };
    anyhow::ensure!(
        effective_pk != Element::default(),
        "effective key is identity"
    );

    let r = sample_nonzero_scalar(rng);
    let enc_cmt = Element::GENERATOR * r;
    let shared_point = effective_pk * r;
    let salt = match tier {
        PocOrbisTier::SenderCore => metadata.sender_core_salt()?,
        PocOrbisTier::SenderExt => metadata.sender_ext_salt()?,
        PocOrbisTier::OutputCore => metadata.output_core_salt()?,
        PocOrbisTier::OutputExt => metadata.output_ext_salt()?,
    };
    let salt_hex = hex::encode(salt.to_bytes());
    let metadata_hash = encode_orbis_policy_metadata(
        policy_id,
        resource,
        permission,
        tier.label(),
        metadata.target_timestamp,
        &salt_hex,
    );
    let (challenge, response) = generate_encryption_proof(
        rng,
        r,
        &effective_pk,
        &enc_cmt,
        &shared_point,
        &metadata_hash,
    )?;
    let envelope = encrypt_seed(rng, &seed.to_bytes(), &enc_cmt, &shared_point)?;
    let transfer_seed_binding = bind_transfer_seed(transfer_epk, transfer_c2, seed);

    Ok(PocOrbisAuditPackage {
        access,
        derivation,
        ring_id: ring_id.to_owned(),
        policy_id: policy_id.to_owned(),
        resource: resource.to_owned(),
        permission: permission.to_owned(),
        tier_label: tier.label().to_owned(),
        timestamp: metadata.target_timestamp,
        salt: salt_hex,
        encrypted_document: serde_json::to_vec(&envelope)
            .context("serialize Orbis encrypted seed envelope")?,
        enc_cmt: enc_cmt.vartime_compress().0.to_vec(),
        shared_point: shared_point.vartime_compress().0.to_vec(),
        orbis_challenge: challenge.to_bytes().to_vec(),
        orbis_response: response.to_bytes().to_vec(),
        effective_pk: effective_pk.vartime_compress().0.to_vec(),
        metadata_hash,
        transfer_epk: transfer_epk.vartime_compress().0.to_vec(),
        transfer_c2: transfer_c2.to_bytes().to_vec(),
        transfer_seed_binding: transfer_seed_binding.to_vec(),
        tier_ciphertext: tier_ciphertext.to_vec(),
    })
}

fn sample_nonzero_scalar(rng: &mut (impl RngCore + CryptoRng)) -> Fr {
    loop {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let scalar = Fr::from_le_bytes_mod_order(&bytes);
        if scalar != Fr::from(0u64) {
            return scalar;
        }
    }
}

fn encrypt_seed(
    rng: &mut (impl RngCore + CryptoRng),
    seed: &[u8; 32],
    enc_cmt: &Element,
    shared_point: &Element,
) -> Result<PocOrbisSecretEnvelope> {
    let key = derive_key_from_point(shared_point)?;
    let cipher = Aes256Gcm::new(&key.into());
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut nonce);
    let enc_cmt_bytes = enc_cmt.vartime_compress().0;
    let shared_bytes = shared_point.vartime_compress().0;
    let encrypted_data = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: seed,
                aad: &build_aad(&enc_cmt_bytes, &shared_bytes),
            },
        )
        .map_err(|_| anyhow!("Orbis seed encryption failed"))?;
    Ok(PocOrbisSecretEnvelope {
        enc_cmt: enc_cmt_bytes.to_vec(),
        encrypted_data,
        nonce: nonce.to_vec(),
    })
}

fn derive_capability_scalar(derivation: &[u8]) -> Fr {
    let mut hasher = Sha512::new();
    hasher.update(DERIVATION_DOMAIN);
    hasher.update(derivation);
    Fr::from_le_bytes_mod_order(&hasher.finalize())
}

fn derive_key_from_point(point: &Element) -> Result<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(None, &point.vartime_compress().0);
    let mut key = [0u8; 32];
    hkdf.expand(HKDF_INFO, &mut key)
        .map_err(|_| anyhow!("HKDF expansion failed"))?;
    Ok(key)
}

fn build_aad(enc_cmt: &[u8], shared_point: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(AAD_DOMAIN);
    hasher.update(enc_cmt);
    hasher.update(shared_point);
    hasher.finalize().into()
}

fn encode_orbis_policy_metadata(
    policy_id: &str,
    resource: &str,
    permission: &str,
    tier: &str,
    timestamp: u64,
    salt: &str,
) -> Vec<u8> {
    let domain = Fq::from_le_bytes_mod_order(POLICY_METADATA_DOMAIN);
    let timestamp = timestamp.to_le_bytes();
    let fields: [&[u8]; 6] = [
        policy_id.as_bytes(),
        resource.as_bytes(),
        permission.as_bytes(),
        tier.as_bytes(),
        &timestamp,
        salt.as_bytes(),
    ];
    let mut inputs = Vec::new();
    for field in fields {
        inputs.push(Fq::from(field.len() as u64));
        inputs.extend(field.chunks(31).map(Fq::from_le_bytes_mod_order));
    }
    let mut state = domain;
    for pair in inputs.chunks(2) {
        state = if pair.len() == 2 {
            poseidon377::hash_2(&state, (pair[0], pair[1]))
        } else {
            poseidon377::hash_1(&state, pair[0])
        };
    }
    state.to_bytes().to_vec()
}

fn generate_encryption_proof(
    rng: &mut (impl RngCore + CryptoRng),
    r: Fr,
    effective_pk: &Element,
    enc_cmt: &Element,
    shared_point: &Element,
    metadata: &[u8],
) -> Result<(Fr, Fr)> {
    let metadata: [u8; 32] = metadata
        .try_into()
        .map_err(|_| anyhow!("Orbis proof metadata must be 32 bytes"))?;
    let k = sample_nonzero_scalar(rng);
    let r1 = Element::GENERATOR * k;
    let r2 = *effective_pk * k;
    let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
    let hash = poseidon377::hash_7(
        &domain,
        (
            Fq::from_le_bytes_mod_order(&metadata),
            Element::GENERATOR.vartime_compress_to_field(),
            effective_pk.vartime_compress_to_field(),
            enc_cmt.vartime_compress_to_field(),
            shared_point.vartime_compress_to_field(),
            r1.vartime_compress_to_field(),
            r2.vartime_compress_to_field(),
        ),
    );
    let challenge = fq_to_challenge_scalar(hash);
    Ok((challenge, k + challenge * r))
}

fn fq_to_challenge_scalar(fq: Fq) -> Fr {
    let mut bytes = fq.to_bytes();
    let keep_bits = Fr::MODULUS_BIT_SIZE as usize - 1;
    let keep_bytes = keep_bits.div_ceil(8);
    let spare_bits = keep_bytes * 8 - keep_bits;
    bytes[keep_bytes - 1] &= 0xff >> spare_bits;
    Fr::from_le_bytes_mod_order(&bytes)
}

pub fn parse_element(bytes: &[u8], label: &str) -> Result<Element> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("{label} must be 32 bytes"))?;
    Encoding(bytes)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid {label}"))
}

pub fn decrypt_reencrypted_seed(
    package: &PocOrbisStoredAuditPackage,
    reader_sk: &Fr,
    xnc_cmt: &Element,
    secret: &PocOrbisSecretEnvelope,
) -> Result<Fq> {
    let effective_pk = parse_element(&package.effective_pk, "effective_pk")?;
    let shared_point = *xnc_cmt - effective_pk * *reader_sk;
    let key = derive_key_from_point(&shared_point)?;
    let cipher = Aes256Gcm::new(&key.into());
    let nonce: [u8; 12] = secret
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("Orbis re-encrypted seed nonce must be 12 bytes"))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &secret.encrypted_data,
                aad: &build_aad(&secret.enc_cmt, &shared_point.vartime_compress().0),
            },
        )
        .map_err(|_| anyhow!("failed to decrypt Orbis re-encrypted seed"))?;
    let seed: [u8; 32] = plaintext
        .try_into()
        .map_err(|_| anyhow!("decrypted seed must be 32 bytes"))?;
    Fq::from_bytes_checked(&seed).map_err(|_| anyhow!("invalid decrypted seed"))
}

pub fn validate_decrypted_seed(package: &PocOrbisStoredAuditPackage, seed: Fq) -> Result<()> {
    let c2: [u8; 32] = package
        .transfer_c2
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("transfer_c2 must be 32 bytes"))?;
    let binding: [u8; 32] = package
        .transfer_seed_binding
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("transfer_seed_binding must be 32 bytes"))?;
    let c2 = Fq::from_bytes_checked(&c2).map_err(|_| anyhow!("invalid transfer_c2"))?;
    let transfer_epk = parse_element(&package.transfer_epk, "transfer_epk")?;
    if binding != bind_transfer_seed(&transfer_epk, c2, seed) {
        bail!("decrypted seed does not match the Shieldd transfer c2");
    }
    Ok(())
}

fn bind_transfer_seed(transfer_epk: &Element, transfer_c2: Fq, seed: Fq) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(TRANSFER_SEED_BINDING_DOMAIN);
    hasher.update(transfer_epk.vartime_compress().0);
    hasher.update(transfer_c2.to_bytes());
    hasher.update(seed.to_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    fn package(derivation: Option<Vec<u8>>) -> PocOrbisAuditPackage {
        let metadata = TransferComplianceMetadata::from_identifiers(
            "ring-1",
            "policy-1",
            "document",
            "read",
            1_700_000_000,
            Fq::from(11u64),
            Fq::from(12u64),
            Fq::from(13u64),
            Fq::from(14u64),
        );
        let seed = Fq::from(99u64);
        let transfer_shared = Fq::from(41u64);
        build_poc_orbis_audit_package(
            &mut OsRng,
            &(Element::GENERATOR * Fr::from(7u64)),
            seed,
            derivation,
            &metadata,
            "ring-1",
            "policy-1",
            "document",
            "read",
            PocOrbisTier::SenderCore,
            &(Element::GENERATOR * Fr::from(17u64)),
            seed + transfer_shared,
            &[7u8; 32],
        )
        .expect("package should build")
    }

    #[test]
    fn subject_and_investigation_packages_use_distinct_access_paths() {
        let subject = package(Some(vec![1, 2, 3]));
        let investigation = package(None);
        assert_eq!(subject.access, PocOrbisAccess::Subject);
        assert_eq!(subject.derivation, Some(vec![1, 2, 3]));
        assert_eq!(investigation.access, PocOrbisAccess::Investigation);
        assert_eq!(investigation.derivation, None);
        assert_ne!(subject.effective_pk, investigation.effective_pk);
    }

    #[test]
    fn package_binds_the_transfer_seed_without_serializing_it() {
        let package = package(None);
        assert_eq!(
            hex::encode(&package.transfer_epk),
            "bc5ffbb7f13ea8eeb0f0364910fb6331945996a1d2389d6ac48ded74b3e2a503"
        );
        assert_eq!(
            hex::encode(&package.transfer_seed_binding),
            "46e6f2ab617ac6f2eb5ff80bdf121f2f1aedf4e1b77f2211e2ddb9569187c8ea"
        );
        let mut stored = PocOrbisStoredAuditPackage::from(&package);
        let seed = Fq::from(99u64);
        validate_decrypted_seed(&stored, seed).expect("transfer binding should validate");
        assert!(!package
            .encrypted_document
            .windows(32)
            .any(|window| window == seed.to_bytes()));
        stored.transfer_seed_binding[0] ^= 1;
        assert!(validate_decrypted_seed(&stored, seed).is_err());
    }

    #[test]
    fn stored_package_omits_local_seed_recovery_material() {
        let package = package(None);
        let stored = PocOrbisStoredAuditPackage::from(&package);
        let serialized = serde_json::to_value(stored).expect("stored package should serialize");

        for pre_input_field in [
            "encrypted_document",
            "enc_cmt",
            "shared_point",
            "orbis_challenge",
            "orbis_response",
            "metadata_hash",
        ] {
            assert!(
                serialized.get(pre_input_field).is_none(),
                "decryption projection leaked {pre_input_field}"
            );
        }
    }

    #[test]
    fn malformed_orbis_nonce_returns_an_error() {
        let package = PocOrbisStoredAuditPackage::from(&package(None));
        let secret = PocOrbisSecretEnvelope {
            enc_cmt: vec![0; 32],
            encrypted_data: vec![],
            nonce: vec![0; 11],
        };

        let error =
            decrypt_reencrypted_seed(&package, &Fr::from(1u64), &Element::GENERATOR, &secret)
                .expect_err("an invalid nonce length must not panic");
        assert!(error.to_string().contains("nonce must be 12 bytes"));
    }

    #[test]
    fn unrestricted_amount_has_one_canonical_tier() {
        assert!(PocOrbisTier::SenderCore.is_canonical_for_access(PocOrbisAccess::Investigation));
        assert!(!PocOrbisTier::OutputCore.is_canonical_for_access(PocOrbisAccess::Investigation));
        assert!(PocOrbisTier::SenderCore.is_canonical_for_access(PocOrbisAccess::Subject));
        assert!(PocOrbisTier::OutputCore.is_canonical_for_access(PocOrbisAccess::Subject));
        assert_eq!(
            PocOrbisTier::from_label("sender_core"),
            Some(PocOrbisTier::SenderCore)
        );
        assert_eq!(PocOrbisTier::from_label("unknown"), None);
    }
}
