use std::{sync::OnceLock, time::Instant};

use anyhow::{ensure, Result};
use ark_ip_proofs::{
    applications::groth16_aggregation::setup_inner_product,
    tipa::{VerifierSRS as InnerProductVerifierSrs, SRS as InnerProductSrs},
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use blake2::Blake2b;
use decaf377::Bls12_377;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub const DEFAULT_MAX_PADDED_PROOF_COUNT: u32 = 32_768;
pub const DEV_SRS_VERSION: u32 = 1;
pub const DEV_SRS_CURVE_ID: &str = "bls12-377";
pub const DEV_SRS_BACKEND_ID: &str = "ripp-snarkpack";
const DEV_SRS_SEED: [u8; 32] = [0x50; 32];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevSrs {
    pub max_padded_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevSrsReport {
    pub generation_ms: u128,
    pub load_ms: u128,
    pub compressed_bytes: usize,
    pub uncompressed_bytes: usize,
}

impl Default for DevSrs {
    fn default() -> Self {
        Self {
            max_padded_count: DEFAULT_MAX_PADDED_PROOF_COUNT,
        }
    }
}

impl DevSrs {
    pub fn ensure_supported_count(&self, item_count: usize) -> Result<()> {
        ensure!(
            item_count <= self.max_padded_count as usize,
            "proof count {} exceeds SRS max {}",
            item_count,
            self.max_padded_count
        );
        Ok(())
    }

    pub(crate) fn inner_product_srs_for_count(
        &self,
        item_count: usize,
    ) -> Result<InnerProductSrs<Bls12_377>> {
        self.ensure_supported_count(item_count)?;
        ensure!(item_count > 0, "proof count must be non-zero");

        let generated = self.generated()?;
        let required_power_count = (2 * item_count) - 1;
        ensure!(
            required_power_count <= generated.inner_product_srs.g_alpha_powers.len(),
            "requested proof count {} exceeds available G1 powers {}",
            item_count,
            generated.inner_product_srs.g_alpha_powers.len()
        );
        ensure!(
            required_power_count <= generated.inner_product_srs.h_beta_powers.len(),
            "requested proof count {} exceeds available G2 powers {}",
            item_count,
            generated.inner_product_srs.h_beta_powers.len()
        );

        Ok(InnerProductSrs {
            g_alpha_powers: generated.inner_product_srs.g_alpha_powers[..required_power_count]
                .to_vec(),
            h_beta_powers: generated.inner_product_srs.h_beta_powers[..required_power_count]
                .to_vec(),
            g_beta: generated.inner_product_srs.g_beta,
            h_alpha: generated.inner_product_srs.h_alpha,
        })
    }

    pub(crate) fn verifier_srs(&self) -> Result<&'static InnerProductVerifierSrs<Bls12_377>> {
        Ok(&self.generated()?.verifier_srs)
    }

    fn generated(&self) -> Result<&'static GeneratedDevSrs> {
        ensure!(
            self.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT,
            "only the default dev SRS size {} is currently supported, got {}",
            DEFAULT_MAX_PADDED_PROOF_COUNT,
            self.max_padded_count
        );

        Ok(DEFAULT_DEV_SRS.get_or_init(generate_default_dev_srs))
    }
}

struct GeneratedDevSrs {
    inner_product_srs: InnerProductSrs<Bls12_377>,
    verifier_srs: InnerProductVerifierSrs<Bls12_377>,
    generation_ms: u128,
}

static DEFAULT_DEV_SRS: OnceLock<GeneratedDevSrs> = OnceLock::new();
static DEFAULT_DEV_SRS_ID: OnceLock<[u8; 32]> = OnceLock::new();

fn generate_default_dev_srs() -> GeneratedDevSrs {
    let started = Instant::now();
    let mut rng = ChaCha20Rng::from_seed(DEV_SRS_SEED);
    let inner_product_srs = setup_inner_product::<Bls12_377, Blake2b, _>(
        &mut rng,
        DEFAULT_MAX_PADDED_PROOF_COUNT as usize,
    )
    .expect("deterministic dev SnarkPack SRS generation must succeed");
    let verifier_srs = inner_product_srs.get_verifier_key();

    GeneratedDevSrs {
        inner_product_srs,
        verifier_srs,
        generation_ms: started.elapsed().as_millis(),
    }
}

pub fn srs_id(srs: &DevSrs) -> [u8; 32] {
    if srs.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT {
        return *DEFAULT_DEV_SRS_ID.get_or_init(|| compute_srs_id(srs));
    }

    compute_srs_id(srs)
}

fn compute_srs_id(srs: &DevSrs) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "penumbra.proof_aggregation.srs.v{DEV_SRS_VERSION}:backend={DEV_SRS_BACKEND_ID}:curve={DEV_SRS_CURVE_ID}:max_padded_count={}",
            srs.max_padded_count
        )
        .as_bytes(),
    );
    if srs.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT {
        let serialized = srs
            .serialized_inner_product_srs_compressed()
            .expect("serializing cached dev SRS must succeed");
        hasher.update(&serialized);
    }
    hasher.finalize().into()
}

pub fn srs_report(srs: &DevSrs) -> Result<DevSrsReport> {
    let generated = srs.generated()?;
    let compressed = srs.serialized_inner_product_srs_compressed()?;
    let uncompressed = srs.serialized_inner_product_srs_uncompressed()?;

    let started = Instant::now();
    let decoded = InnerProductSrs::<Bls12_377>::deserialize_compressed(&compressed[..])?;
    let load_ms = started.elapsed().as_millis();
    ensure!(
        decoded.g_alpha_powers.len() == generated.inner_product_srs.g_alpha_powers.len()
            && decoded.h_beta_powers.len() == generated.inner_product_srs.h_beta_powers.len(),
        "deserialized SRS dimensions do not match generated dev SRS"
    );

    Ok(DevSrsReport {
        generation_ms: generated.generation_ms,
        load_ms,
        compressed_bytes: compressed.len(),
        uncompressed_bytes: uncompressed.len(),
    })
}

impl DevSrs {
    pub fn serialized_inner_product_srs_compressed(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.generated()?
            .inner_product_srs
            .serialize_compressed(&mut bytes)?;
        Ok(bytes)
    }

    pub fn serialized_inner_product_srs_uncompressed(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.generated()?
            .inner_product_srs
            .serialize_uncompressed(&mut bytes)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{srs_id, DevSrs};

    #[test]
    fn srs_id_is_stable() {
        let a = srs_id(&DevSrs::default());
        let b = srs_id(&DevSrs::default());
        assert_eq!(a, b);
    }

    #[test]
    fn srs_id_changes_with_parameters() {
        let a = srs_id(&DevSrs {
            max_padded_count: 32_768,
        });
        let b = srs_id(&DevSrs {
            max_padded_count: 16_384,
        });
        assert_ne!(a, b);
    }
}
