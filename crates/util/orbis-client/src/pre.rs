use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, bail, Context, Result};
use decaf377::{Element, Encoding, Fq, Fr};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::types::{PreparedSecret, SecretEnvelope};

const ENCRYPT_PROOF_DOMAIN: &[u8; 24] = b"elgamal-encrypt-proof-v1";
const AAD_DOMAIN: &[u8; 15] = b"elgamal-aad-v1\0";
const DERIVATION_DOMAIN: &[u8; 23] = b"elgamal-derivation-v1\0\0";
const POLICY_METADATA_DOMAIN: &[u8] = b"orbis-policy-metadata-v1";
const HKDF_INFO: &[u8] = b"elgamal-aes-key-v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EncryptionProof {
    shared_point: Vec<u8>,
    challenge: Vec<u8>,
    response: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    derived_pk: Option<Vec<u8>>,
}

pub(crate) fn prepare_secret(
    ring_pk_hex: &str,
    derivation_hex: &str,
    policy_id: &str,
    resource: &str,
    permission: &str,
    tier: Option<&str>,
    timestamp: Option<u64>,
    salt: Option<&str>,
) -> Result<PreparedSecret> {
    let ring_pk_bytes = hex::decode(ring_pk_hex).context("invalid ring_pk hex")?;
    let ring_pk_arr: [u8; 32] = ring_pk_bytes
        .try_into()
        .map_err(|_| anyhow!("ring_pk should be 32 bytes"))?;
    let ring_pk = Encoding(ring_pk_arr)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid ring_pk encoding"))?;

    let derivation = hex::decode(derivation_hex).context("invalid derivation hex")?;
    let metadata = encode_metadata(policy_id, resource, permission, tier, timestamp, salt);
    let dummy_secret = [0u8; 32];

    let (enc_cmt, encrypted_secret, proof) =
        encrypt_secret(&ring_pk, &dummy_secret, Some(&derivation), Some(&metadata))?;

    Ok(PreparedSecret {
        encrypted_document: serde_json::to_vec(&encrypted_secret)
            .context("failed to serialize encrypted secret")?,
        enc_cmt: enc_cmt.vartime_compress().0.to_vec(),
        shared_point: proof.shared_point,
        challenge: proof.challenge,
        response: proof.response,
        metadata,
        derived_pk: proof.derived_pk,
    })
}

fn encrypt_secret(
    dkg_pk: &Element,
    data: &[u8],
    derivation: Option<&[u8]>,
    metadata: Option<&[u8]>,
) -> Result<(Element, SecretEnvelope, EncryptionProof)> {
    if *dkg_pk == Element::default() {
        bail!("invalid dkg_pk: cannot be the identity element");
    }

    let mut rng = OsRng;
    let r = loop {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let candidate = Fr::from_le_bytes_mod_order(&bytes);
        if candidate != Fr::from(0u64) {
            break candidate;
        }
    };
    let enc_cmt = Element::GENERATOR * r;

    let (effective_pk, derived_pk) = if let Some(deriv_bytes) = derivation {
        let d = derive_capability_scalar(deriv_bytes);
        if d == Fr::from(0u64) {
            bail!("derivation produced zero scalar");
        }
        let derived_pk = *dkg_pk * d;
        if derived_pk == Element::default() {
            bail!("derived public key is the identity element");
        }
        (derived_pk, Some(derived_pk.vartime_compress().0.to_vec()))
    } else {
        (*dkg_pk, None)
    };

    let shared_point = effective_pk * r;
    let (challenge, response) =
        generate_encryption_proof(&r, &effective_pk, &enc_cmt, &shared_point, metadata)?;

    let shared_point_bytes = shared_point.vartime_compress().0.to_vec();
    let proof = EncryptionProof {
        shared_point: shared_point_bytes.clone(),
        challenge: challenge.to_bytes().to_vec(),
        response: response.to_bytes().to_vec(),
        derived_pk,
    };

    let aes_key = derive_key_from_point(&shared_point)?;
    let cipher = Aes256Gcm::new(&aes_key.into());

    let mut nonce_bytes = [0u8; 12];
    rng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let enc_cmt_bytes = enc_cmt.vartime_compress().0.to_vec();
    let aad = build_aad(&enc_cmt_bytes, &shared_point_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: data,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("encryption failed"))?;

    Ok((
        enc_cmt,
        SecretEnvelope {
            enc_cmt: enc_cmt_bytes,
            encrypted_data: ciphertext,
            nonce: nonce_bytes.to_vec(),
        },
        proof,
    ))
}

fn derive_capability_scalar(derivation: &[u8]) -> Fr {
    let mut hasher = Sha256::new();
    hasher.update(DERIVATION_DOMAIN);
    hasher.update(derivation);
    let hash = hasher.finalize();
    Fr::from_le_bytes_mod_order(&hash)
}

fn encode_metadata(
    policy_id: &str,
    resource: &str,
    permission: &str,
    tier: Option<&str>,
    timestamp: Option<u64>,
    salt: Option<&str>,
) -> Vec<u8> {
    let domain = Fq::from_le_bytes_mod_order(POLICY_METADATA_DOMAIN);
    let ts_le = timestamp.map(|t| t.to_le_bytes());
    let ts_bytes: &[u8] = ts_le.as_ref().map_or(&[], |b| b.as_slice());

    let mut inputs = Vec::new();
    for field in &[
        policy_id.as_bytes(),
        resource.as_bytes(),
        permission.as_bytes(),
        tier.unwrap_or("").as_bytes(),
        ts_bytes,
        salt.unwrap_or("").as_bytes(),
    ] {
        inputs.push(Fq::from(field.len() as u64));
        for chunk in field.chunks(31) {
            inputs.push(Fq::from_le_bytes_mod_order(chunk));
        }
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

fn derive_key_from_point(point: &Element) -> Result<[u8; 32]> {
    let point_bytes = point.vartime_compress().0;
    let hkdf = Hkdf::<Sha256>::new(None, &point_bytes);
    let mut key = [0u8; 32];
    hkdf.expand(HKDF_INFO, &mut key)
        .map_err(|_| anyhow!("HKDF expansion failed"))?;
    Ok(key)
}

fn build_aad(enc_cmt_bytes: &[u8], shared_point_bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(AAD_DOMAIN);
    hasher.update(enc_cmt_bytes);
    hasher.update(shared_point_bytes);
    hasher.finalize().into()
}

fn generate_encryption_proof(
    r: &Fr,
    dkg_pk: &Element,
    enc_cmt: &Element,
    shared_point: &Element,
    metadata: Option<&[u8]>,
) -> Result<(Fr, Fr)> {
    let metadata_arr: Option<[u8; 32]> = metadata
        .map(|m| {
            m.try_into()
                .map_err(|_| anyhow!("metadata must be exactly 32 bytes"))
        })
        .transpose()?;

    let mut rng = OsRng;
    let mut bytes = [0u8; 64];
    rng.fill_bytes(&mut bytes);
    let k = Fr::from_le_bytes_mod_order(&bytes);

    let r1 = Element::GENERATOR * k;
    let r2 = *dkg_pk * k;
    let challenge = hash_encryption_proof_points(
        &Element::GENERATOR,
        dkg_pk,
        enc_cmt,
        shared_point,
        &r1,
        &r2,
        metadata_arr.as_ref(),
    );

    Ok((challenge, k + (challenge * *r)))
}

fn hash_encryption_proof_points(
    g: &Element,
    dkg_pk: &Element,
    enc_cmt: &Element,
    shared_point: &Element,
    r1: &Element,
    r2: &Element,
    metadata: Option<&[u8; 32]>,
) -> Fr {
    let metadata_fq = metadata
        .map(|bytes| Fq::from_le_bytes_mod_order(bytes))
        .unwrap_or_else(|| Fq::from(0u64));
    let domain = Fq::from_le_bytes_mod_order(ENCRYPT_PROOF_DOMAIN);
    let hash = poseidon377::hash_7(
        &domain,
        (
            metadata_fq,
            g.vartime_compress_to_field(),
            dkg_pk.vartime_compress_to_field(),
            enc_cmt.vartime_compress_to_field(),
            shared_point.vartime_compress_to_field(),
            r1.vartime_compress_to_field(),
            r2.vartime_compress_to_field(),
        ),
    );
    fq_to_challenge_scalar(hash)
}

fn fq_to_challenge_scalar(fq: Fq) -> Fr {
    let mut bytes = fq.to_bytes();
    let keep_bits = Fr::MODULUS_BIT_SIZE as usize - 1;
    let keep_bytes = keep_bits.div_ceil(8);
    let spare_bits = keep_bytes * 8 - keep_bits;
    bytes[keep_bytes - 1] &= 0xff >> spare_bits;
    Fr::from_le_bytes_mod_order(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use penumbra_sdk_compliance::verify_dleq_native;

    #[test]
    fn prepared_secret_roundtrips_local_proof() {
        let ring_sk = Fr::from(11u64);
        let ring_pk_hex = hex::encode((Element::GENERATOR * ring_sk).vartime_compress().0);
        let derivation_hex = hex::encode([7u8; 32]);

        let prepared = prepare_secret(
            &ring_pk_hex,
            &derivation_hex,
            "policy-id",
            "document",
            "read",
            None,
            None,
            None,
        )
        .expect("prepared secret should build");

        let enc_cmt = Encoding(prepared.enc_cmt.clone().try_into().expect("32-byte point"))
            .vartime_decompress()
            .expect("enc_cmt should decode");
        let shared_point = Encoding(
            prepared
                .shared_point
                .clone()
                .try_into()
                .expect("32-byte point"),
        )
        .vartime_decompress()
        .expect("shared point should decode");
        let challenge = Fq::from_le_bytes_mod_order(&prepared.challenge);
        let response = Fr::from_le_bytes_mod_order(&prepared.response);
        let derived_pk = Encoding(
            prepared
                .derived_pk
                .clone()
                .expect("derived_pk should be present")
                .try_into()
                .expect("32-byte point"),
        )
        .vartime_decompress()
        .expect("derived_pk should decode");
        let metadata_hash = Fq::from_le_bytes_mod_order(&prepared.metadata);

        verify_dleq_native(
            &derived_pk,
            &enc_cmt,
            &shared_point,
            &challenge,
            &response,
            metadata_hash,
        )
        .expect("local proof should verify against Penumbra verifier");

        let secret: SecretEnvelope = serde_json::from_slice(&prepared.encrypted_document)
            .expect("secret should deserialize");
        let key = derive_key_from_point(&shared_point).expect("AES key should derive");
        let cipher = Aes256Gcm::new(&key.into());
        let aad = build_aad(&secret.enc_cmt, &prepared.shared_point);
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&secret.nonce),
                Payload {
                    msg: &secret.encrypted_data,
                    aad: &aad,
                },
            )
            .expect("ciphertext should decrypt");
        assert_eq!(plaintext, vec![0u8; 32]);
    }
}
