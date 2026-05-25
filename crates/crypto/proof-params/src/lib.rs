#![deny(clippy::unwrap_used)]
#![allow(clippy::redundant_static_lifetimes)]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]

use anyhow::{bail, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_groth16::{PreparedVerifyingKey, VerifyingKey};
use decaf377::Bls12_377;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::{fs, path::Path, str::FromStr};

/// The length of our Groth16 proofs in bytes.
pub const GROTH16_PROOF_LENGTH_BYTES: usize = 192;

pub mod batch;
pub mod statement_hash;
mod traits;

pub use traits::{
    generate_constraint_matrices, generate_prepared_test_parameters, generate_test_parameters,
    DummyWitness, ProvingKeyExt, VerifyingKeyExt,
};

include!(concat!(env!("OUT_DIR"), "/gnark_bundled.rs"));

include!("gen/gnark/transfer_registry.rs");
include!("gen/gnark/consolidate_registry.rs");
include!("gen/gnark/split_registry.rs");
include!("gen/gnark/shielded_ics20_withdrawal_registry.rs");

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

#[derive(Clone, Debug, Deserialize)]
struct G1PointJson {
    x: String,
    y: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Fq2Json {
    a0: String,
    a1: String,
}

#[derive(Clone, Debug, Deserialize)]
struct G2PointJson {
    x: Fq2Json,
    y: Fq2Json,
}

#[derive(Clone, Debug, Deserialize)]
struct VerifyingKeyJson {
    alpha_g1: G1PointJson,
    beta_g2: G2PointJson,
    gamma_g2: G2PointJson,
    delta_g2: G2PointJson,
    gamma_abc_g1: Vec<G1PointJson>,
}

#[derive(Clone, Debug, Deserialize)]
struct CircuitMetadataJson {
    curve: String,
    circuit: String,
    #[serde(default)]
    nb_constraints: Option<i32>,
    #[serde(default)]
    nb_public_variables: Option<i32>,
    #[serde(default)]
    nb_secret_variables: Option<i32>,
}

fn load_verifying_key_json_bytes(bytes: &[u8]) -> Result<VerifyingKey<Bls12_377>> {
    let vk_json: VerifyingKeyJson = serde_json::from_slice(bytes)?;
    verifying_key_from_json(&vk_json)
}

fn load_verifying_key_json_artifact(
    artifact_dir: &Path,
    expected_circuit: &str,
) -> Result<VerifyingKey<Bls12_377>> {
    let metadata_path = artifact_dir.join("circuit_metadata.json");
    let metadata: CircuitMetadataJson = serde_json::from_slice(&fs::read(&metadata_path)?)?;
    if metadata.curve != "bls12-377" {
        bail!("artifact curve {} does not match bls12-377", metadata.curve);
    }
    if metadata.circuit != expected_circuit {
        bail!(
            "artifact circuit {} does not match expected {}",
            metadata.circuit,
            expected_circuit
        );
    }
    let _ = (
        metadata.nb_constraints,
        metadata.nb_public_variables,
        metadata.nb_secret_variables,
    );

    let vk_path = artifact_dir.join("verifying_key.json");
    let vk_json: VerifyingKeyJson = serde_json::from_slice(&fs::read(&vk_path)?)?;
    verifying_key_from_json(&vk_json)
}

fn verifying_key_from_json(vk: &VerifyingKeyJson) -> Result<VerifyingKey<Bls12_377>> {
    Ok(VerifyingKey {
        alpha_g1: parse_g1(&vk.alpha_g1)?,
        beta_g2: parse_g2(&vk.beta_g2)?,
        gamma_g2: parse_g2(&vk.gamma_g2)?,
        delta_g2: parse_g2(&vk.delta_g2)?,
        gamma_abc_g1: vk
            .gamma_abc_g1
            .iter()
            .map(parse_g1)
            .collect::<Result<_>>()?,
    })
}

fn parse_g1(point: &G1PointJson) -> Result<ProofG1> {
    let x = ProofG1Base::from_str(&point.x).map_err(|_| anyhow::anyhow!("invalid G1 x"))?;
    let y = ProofG1Base::from_str(&point.y).map_err(|_| anyhow::anyhow!("invalid G1 y"))?;
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
    let x_a0 =
        ProofG1Base::from_str(&point.x.a0).map_err(|_| anyhow::anyhow!("invalid G2 x.a0"))?;
    let x_a1 =
        ProofG1Base::from_str(&point.x.a1).map_err(|_| anyhow::anyhow!("invalid G2 x.a1"))?;
    let y_a0 =
        ProofG1Base::from_str(&point.y.a0).map_err(|_| anyhow::anyhow!("invalid G2 y.a0"))?;
    let y_a1 =
        ProofG1Base::from_str(&point.y.a1).map_err(|_| anyhow::anyhow!("invalid G2 y.a1"))?;
    let point = ProofG2::new_unchecked(ProofG2Base::new(x_a0, x_a1), ProofG2Base::new(y_a0, y_a1));
    if !point.is_on_curve() {
        bail!("G2 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G2 point is not in the correct subgroup");
    }
    Ok(point)
}
