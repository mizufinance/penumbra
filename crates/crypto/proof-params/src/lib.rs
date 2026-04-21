#![deny(clippy::unwrap_used)]
#![allow(clippy::redundant_static_lifetimes)]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use anyhow::{bail, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_groth16::{PreparedVerifyingKey, ProvingKey, VerifyingKey};
use ark_serialize::CanonicalDeserialize;
use decaf377::Bls12_377;
use once_cell::sync::{Lazy, OnceCell};
use serde::Deserialize;
use std::{fs, ops::Deref, path::Path, str::FromStr};

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

/// A wrapper around a proving key that can be lazily loaded.
///
/// One instance of this struct is created for each proving key.
///
/// The behavior of those instances is controlled by the `bundled-proving-keys`
/// feature. When the feature is enabled, the proving key data is bundled into
/// the binary at compile time, and the proving key is loaded from the bundled
/// data on first use.  When the feature is not enabled, the proving key must be
/// loaded using `try_load` prior to its first use.
///
/// The `bundled-proving-keys` feature needs access to proving keys at build
/// time.  When pulling the crate as a dependency, these may not be available.
/// To address this, the `download-proving-keys` feature will download them from
/// the network at build time. All proving keys are checked against hardcoded hashes
/// to ensure they have not been tampered with.
#[derive(Debug, Default)]
pub struct LazyProvingKey {
    pk_id: &'static str,
    inner: OnceCell<ProvingKey<Bls12_377>>,
}

impl LazyProvingKey {
    // Not making this pub means only the statically defined proving keys can exist.
    fn new(pk_id: &'static str) -> Self {
        LazyProvingKey {
            pk_id,
            inner: OnceCell::new(),
        }
    }

    /// Attempt to load the proving key from the given bytes.
    ///
    /// The provided bytes are validated against a hardcoded hash of the expected proving key,
    /// so passing the wrong proving key will fail.
    ///
    /// If the proving key is already loaded, this method is a no-op.
    pub fn try_load(&self, bytes: &[u8]) -> Result<&ProvingKey<Bls12_377>> {
        self.inner.get_or_try_init(|| {
            let pk = ProvingKey::deserialize_uncompressed_unchecked(bytes)?;

            let pk_id = pk.debug_id();
            if pk_id != self.pk_id {
                bail!(
                    "proving key ID mismatch: expected {}, loaded {}",
                    self.pk_id,
                    pk_id
                );
            }

            Ok(pk)
        })
    }

    /// Attempt to load the proving key from the given bytes.
    ///
    /// This method bypasses the validation checks against the hardcoded
    /// hash of the expected proving key.
    pub fn try_load_unchecked(&self, bytes: &[u8]) -> Result<&ProvingKey<Bls12_377>> {
        self.inner.get_or_try_init(|| {
            let pk = ProvingKey::deserialize_uncompressed_unchecked(bytes)?;

            Ok(pk)
        })
    }
}

impl Deref for LazyProvingKey {
    type Target = ProvingKey<Bls12_377>;

    fn deref(&self) -> &Self::Target {
        self.inner.get().expect("Proving key cannot be loaded!")
    }
}

// Note: Conditionally load the proving key objects if the
// bundled-proving-keys is present.

include!("gen/gnark/transfer_registry.rs");
include!("gen/gnark/consolidate_registry.rs");
include!("gen/gnark/split_registry.rs");
include!("gen/gnark/shielded_ics20_withdrawal_registry.rs");

/// Proving key for the nullifier derivation proof.
pub static NULLIFIER_DERIVATION_PROOF_PROVING_KEY: Lazy<LazyProvingKey> = Lazy::new(|| {
    let nullifier_proving_key = LazyProvingKey::new(nullifier_derivation::PROVING_KEY_ID);

    #[cfg(feature = "bundled-proving-keys")]
    nullifier_proving_key
        .try_load(include_bytes!("gen/nullifier_derivation_pk.bin"))
        .expect("bundled proving key is valid");

    nullifier_proving_key
});

/// Verification key for the nullifier derivation proof.
pub static NULLIFIER_DERIVATION_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> =
    Lazy::new(|| nullifier_derivation_verification_parameters().into());

pub mod nullifier_derivation {
    include!("gen/nullifier_derivation_id.rs");
}

// Note: Here we are using `CanonicalDeserialize::deserialize_uncompressed_unchecked` as the
// parameters are being loaded from a trusted source (our source code).

fn nullifier_derivation_verification_parameters() -> VerifyingKey<Bls12_377> {
    let vk_params = include_bytes!("gen/nullifier_derivation_vk.param");
    VerifyingKey::deserialize_uncompressed_unchecked(&vk_params[..])
        .expect("can deserialize VerifyingKey")
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_keys_smoke_load() {
        let _ = &*NULLIFIER_DERIVATION_PROOF_PROVING_KEY;
        let _ = &*NULLIFIER_DERIVATION_PROOF_VERIFICATION_KEY;
    }
}
