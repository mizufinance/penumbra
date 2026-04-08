use std::{fs, path::Path, str::FromStr};

use anyhow::{anyhow, bail, Context, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_groth16::{prepare_verifying_key, PreparedVerifyingKey, VerifyingKey};
use decaf377::Bls12_377;
use penumbra_sdk_proof_params::VerifyingKeyExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

#[derive(Clone, Debug, Deserialize)]
pub struct G1PointJson {
    pub x: String,
    pub y: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Fq2Json {
    pub a0: String,
    pub a1: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct G2PointJson {
    pub x: Fq2Json,
    pub y: Fq2Json,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VerifyingKeyJson {
    pub alpha_g1: G1PointJson,
    pub beta_g2: G2PointJson,
    pub gamma_g2: G2PointJson,
    pub delta_g2: G2PointJson,
    pub gamma_abc_g1: Vec<G1PointJson>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GnarkArtifactMetadata {
    pub curve: String,
    pub circuit: String,
    pub compile_ms: f64,
    pub setup_ms: f64,
    pub proving_key_size_bytes: i64,
    pub verifying_key_size_bytes: i64,
    pub nb_constraints: i32,
    pub nb_public_variables: i32,
    pub nb_secret_variables: i32,
    #[serde(default)]
    pub verifying_key_sha256_hex: Option<String>,
    #[serde(default)]
    pub proving_key_sha256_hex: Option<String>,
    #[serde(default)]
    pub verifying_key_id: Option<String>,
}

pub(crate) fn load_artifact_metadata(artifact_dir: &Path) -> Result<GnarkArtifactMetadata> {
    serde_json::from_slice(
        &fs::read(artifact_dir.join("circuit_metadata.json"))
            .context("read circuit_metadata.json")?,
    )
    .context("decode circuit_metadata.json")
}

pub(crate) fn validate_artifact_metadata(
    metadata: &GnarkArtifactMetadata,
    family: &str,
) -> Result<()> {
    if metadata.curve != "bls12-377" {
        bail!("unsupported gnark {family} curve {}", metadata.curve);
    }
    if metadata.circuit != family {
        bail!(
            "unsupported gnark {family} circuit {}, expected {family}",
            metadata.circuit
        );
    }
    if metadata.nb_constraints <= 0
        || metadata.nb_public_variables <= 0
        || metadata.nb_secret_variables <= 0
    {
        bail!("gnark {family} circuit metadata is missing constraint shape");
    }
    Ok(())
}

pub(crate) fn validate_artifact_hashes(
    artifact_dir: &Path,
    metadata: &GnarkArtifactMetadata,
    family: &str,
) -> Result<()> {
    if let Some(expected) = &metadata.proving_key_sha256_hex {
        let actual = sha256_hex(&fs::read(artifact_dir.join("proving_key.bin"))?);
        if &actual != expected {
            bail!("gnark {family} proving key hash mismatch: expected {expected}, got {actual}");
        }
    }
    if let Some(expected) = &metadata.verifying_key_sha256_hex {
        let actual = sha256_hex(&fs::read(artifact_dir.join("verifying_key.json"))?);
        if &actual != expected {
            bail!("gnark {family} verifying key hash mismatch: expected {expected}, got {actual}");
        }
    }
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(crate) fn verifying_key_from_json(vk: &VerifyingKeyJson) -> Result<VerifyingKey<Bls12_377>> {
    Ok(VerifyingKey {
        alpha_g1: parse_g1(&vk.alpha_g1)?,
        beta_g2: parse_g2(&vk.beta_g2)?,
        gamma_g2: parse_g2(&vk.gamma_g2)?,
        delta_g2: parse_g2(&vk.delta_g2)?,
        gamma_abc_g1: vk
            .gamma_abc_g1
            .iter()
            .map(parse_g1)
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(crate) fn load_prepared_vk(
    artifact_dir: &Path,
    metadata: &GnarkArtifactMetadata,
    family: &str,
) -> Result<PreparedVerifyingKey<Bls12_377>> {
    let vk_json: VerifyingKeyJson = serde_json::from_slice(
        &fs::read(artifact_dir.join("verifying_key.json")).context("read verifying_key.json")?,
    )
    .context("decode verifying_key.json")?;
    let pvk = prepare_verifying_key(&verifying_key_from_json(&vk_json)?);
    if let Some(expected_id) = &metadata.verifying_key_id {
        let actual_id = pvk.debug_id();
        if &actual_id != expected_id {
            bail!(
                "gnark {family} verifying key id mismatch: expected {expected_id}, got {actual_id}"
            );
        }
    }
    Ok(pvk)
}

fn parse_g1(point: &G1PointJson) -> Result<ProofG1> {
    let x = ProofG1Base::from_str(&point.x).map_err(|_| anyhow!("invalid G1 x"))?;
    let y = ProofG1Base::from_str(&point.y).map_err(|_| anyhow!("invalid G1 y"))?;
    let point = ProofG1::new_unchecked(x, y);
    if !point.is_on_curve() {
        bail!("G1 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G1 point is not in the correct subgroup");
    }
    Ok(point)
}

fn parse_g2(point: &G2PointJson) -> Result<ProofG2> {
    let x_a0 = ProofG1Base::from_str(&point.x.a0).map_err(|_| anyhow!("invalid G2 x.a0"))?;
    let x_a1 = ProofG1Base::from_str(&point.x.a1).map_err(|_| anyhow!("invalid G2 x.a1"))?;
    let y_a0 = ProofG1Base::from_str(&point.y.a0).map_err(|_| anyhow!("invalid G2 y.a0"))?;
    let y_a1 = ProofG1Base::from_str(&point.y.a1).map_err(|_| anyhow!("invalid G2 y.a1"))?;
    let point = ProofG2::new_unchecked(ProofG2Base::new(x_a0, x_a1), ProofG2Base::new(y_a0, y_a1));
    if !point.is_on_curve() {
        bail!("G2 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G2 point is not in the correct subgroup");
    }
    Ok(point)
}
