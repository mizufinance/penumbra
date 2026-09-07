use std::{
    fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Instant,
};

use anyhow::{anyhow, ensure, Context, Result};
use ark_ec::pairing::Pairing;
use ark_ff::Zero;
use ark_ip_proofs::{
    applications::groth16_aggregation::setup_inner_product,
    tipa::{VerifierSRS as InnerProductVerifierSrs, SRS as InnerProductSrs},
};
use ark_serialize::CanonicalSerialize;
use blake2::Blake2b;
use decaf377::Bls12_377;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::strict_deserialize::deserialize_compressed_strict;

pub const DEFAULT_MAX_PADDED_PROOF_COUNT: u32 = 32_768;
pub const DEV_SRS_CURVE_ID: &str = "bls12-377";
pub const DEV_SRS_BACKEND_ID: &str = "ripp-snarkpack";
const DEFAULT_DEV_SRS_ID_PREFIX: &[u8] =
    b"shieldd.proof_aggregation.srs:backend=ripp-snarkpack:curve=bls12-377:max_padded_count=32768";
pub const PRODUCTION_SRS_ARTIFACT_DIR_ENV: &str = "SHIELDD_SNARKPACK_SRS_ARTIFACT_DIR";
const PRODUCTION_SRS_ID_DOMAIN: &[u8] = b"shieldd.proof_aggregation.registered_srs";
const MAX_PRODUCTION_SRS_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_DEV_SRS_ID: [u8; 32] = [
    0x44, 0x41, 0x81, 0x48, 0x72, 0x67, 0x60, 0xbd, 0x81, 0x39, 0xbb, 0xf7, 0xb4, 0x94, 0x7a, 0x8d,
    0x45, 0xb2, 0x82, 0x54, 0x56, 0xfe, 0x1d, 0x58, 0x0d, 0x97, 0xbf, 0x7b, 0xe9, 0xb3, 0xb4, 0xc4,
];
// Public deterministic fixture seed. The resulting alpha/beta values are
// reproducible, so this setup cannot support a KZG soundness claim.
const DEV_SRS_SEED: [u8; 32] = [0x50; 32];

#[derive(Clone, Copy)]
struct ProductionSrsRegistryEntry {
    id: [u8; 32],
    artifact_sha256: [u8; 32],
    artifact_filename: &'static str,
    max_padded_count: u32,
}

// Intentionally empty until an identified ceremony owner supplies a verified
// BLS12-377 transcript and canonical artifact. Adding an entry requires pinning
// both hashes below; runtime configuration may select a path, never new bytes.
const ACTIVE_PRODUCTION_SRS_ID: Option<[u8; 32]> = None;
const PRODUCTION_SRS_REGISTRY: &[ProductionSrsRegistryEntry] = &[];

struct RegisteredSrsMaterial {
    id: [u8; 32],
    inner_product_srs: InnerProductSrs<Bls12_377>,
    verifier_srs: InnerProductVerifierSrs<Bls12_377>,
}

#[derive(Clone)]
enum SrsSource {
    Development,
    Registered(Arc<RegisteredSrsMaterial>),
}

/// SnarkPack SRS handle.
///
/// `Default` is only the deterministic development fixture and is not an
/// adversarially sound trusted setup. Production handles come only from the
/// digest-pinned registry loader.
#[derive(Clone)]
pub struct DevSrs {
    pub max_padded_count: u32,
    source: SrsSource,
}

impl fmt::Debug for DevSrs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DevSrs")
            .field("max_padded_count", &self.max_padded_count)
            .field("registered", &self.is_registered())
            .field("srs_id", &hex::encode(srs_id(self)))
            .finish()
    }
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
            source: SrsSource::Development,
        }
    }
}

impl DevSrs {
    /// True only when canonical bytes matched a compile-time registry digest.
    ///
    /// Registration authenticates the selected bytes. Unknown-trapdoor setup
    /// still requires separate ceremony-transcript evidence.
    pub fn is_registered(&self) -> bool {
        matches!(&self.source, SrsSource::Registered(_))
    }

    pub fn ensure_supported_count(&self, item_count: usize) -> Result<()> {
        ensure!(
            dev_srs_supports_count(self.max_padded_count, item_count),
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

        let inner_product_srs = self.full_inner_product_srs()?;
        let required_power_count = (2 * item_count) - 1;
        ensure!(
            required_power_count <= inner_product_srs.g_alpha_powers.len(),
            "requested proof count {} exceeds available G1 powers {}",
            item_count,
            inner_product_srs.g_alpha_powers.len()
        );
        ensure!(
            required_power_count <= inner_product_srs.h_beta_powers.len(),
            "requested proof count {} exceeds available G2 powers {}",
            item_count,
            inner_product_srs.h_beta_powers.len()
        );

        Ok(InnerProductSrs {
            g_alpha_powers: inner_product_srs.g_alpha_powers[..required_power_count].to_vec(),
            h_beta_powers: inner_product_srs.h_beta_powers[..required_power_count].to_vec(),
            g_beta: inner_product_srs.g_beta,
            h_alpha: inner_product_srs.h_alpha,
        })
    }

    pub(crate) fn verifier_srs(&self) -> Result<&InnerProductVerifierSrs<Bls12_377>> {
        match &self.source {
            SrsSource::Development => Ok(&self.generated()?.verifier_srs),
            SrsSource::Registered(material) => Ok(&material.verifier_srs),
        }
    }

    fn full_inner_product_srs(&self) -> Result<&InnerProductSrs<Bls12_377>> {
        match &self.source {
            SrsSource::Development => Ok(&self.generated()?.inner_product_srs),
            SrsSource::Registered(material) => Ok(&material.inner_product_srs),
        }
    }

    fn generation_ms(&self) -> Result<u128> {
        match &self.source {
            SrsSource::Development => Ok(self.generated()?.generation_ms),
            SrsSource::Registered(_) => Ok(0),
        }
    }

    fn generated(&self) -> Result<&'static GeneratedDevSrs> {
        ensure!(
            matches!(&self.source, SrsSource::Development),
            "registered SRS cannot use the deterministic development generator"
        );
        ensure!(
            self.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT,
            "only the default dev SRS size {} is currently supported, got {}",
            DEFAULT_MAX_PADDED_PROOF_COUNT,
            self.max_padded_count
        );

        Ok(DEFAULT_DEV_SRS.get_or_init(generate_default_dev_srs))
    }
}

pub fn dev_srs_supports_count(max_padded_count: u32, item_count: usize) -> bool {
    item_count <= max_padded_count as usize
}

struct GeneratedDevSrs {
    inner_product_srs: InnerProductSrs<Bls12_377>,
    verifier_srs: InnerProductVerifierSrs<Bls12_377>,
    generation_ms: u128,
}

static DEFAULT_DEV_SRS: OnceLock<GeneratedDevSrs> = OnceLock::new();
static ACTIVE_PRODUCTION_SRS: OnceLock<Result<DevSrs, String>> = OnceLock::new();

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

/// Load the sole production SRS selected by the compile-time registry.
///
/// This fails until a ceremony artifact, its digest, and its identifier are
/// reviewed and pinned in `PRODUCTION_SRS_REGISTRY`.
pub fn load_active_production_srs() -> Result<DevSrs> {
    let entry = active_production_srs_entry()?;
    cached_active_production_srs(entry)
}

/// Load the active production SRS only when the requested wire identifier is
/// exactly the identifier pinned by the compile-time registry.
pub fn load_production_srs_for_id(requested_id: &[u8]) -> Result<DevSrs> {
    let requested_id: [u8; 32] = requested_id.try_into().map_err(|_| {
        anyhow!(
            "production SnarkPack SRS id must be 32 bytes, got {}",
            requested_id.len()
        )
    })?;
    let entry = active_production_srs_entry()?;
    ensure!(
        requested_id == entry.id,
        "production SnarkPack SRS id {} is not the active registered id {}",
        hex::encode(requested_id),
        hex::encode(entry.id)
    );
    cached_active_production_srs(entry)
}

fn active_production_srs_entry() -> Result<&'static ProductionSrsRegistryEntry> {
    let active_id = ACTIVE_PRODUCTION_SRS_ID.ok_or_else(|| {
        anyhow!(
            "no production SnarkPack SRS is registered; an identified ceremony owner must \
             supply a verified BLS12-377 setup transcript and canonical artifact digest"
        )
    })?;
    let matching = PRODUCTION_SRS_REGISTRY
        .iter()
        .filter(|entry| entry.id == active_id)
        .collect::<Vec<_>>();
    ensure!(
        matching.len() == 1,
        "active production SnarkPack SRS id {} has {} registry entries, expected exactly one",
        hex::encode(active_id),
        matching.len()
    );
    let entry = matching[0];
    ensure!(
        entry.id != DEFAULT_DEV_SRS_ID,
        "production SnarkPack registry must not select the development SRS id"
    );
    ensure!(
        entry.id != [0; 32] && entry.artifact_sha256 != [0; 32],
        "production SnarkPack registry ids and artifact hashes must be non-zero"
    );
    ensure!(
        entry.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT,
        "production SnarkPack SRS max padded count is {}, expected {}",
        entry.max_padded_count,
        DEFAULT_MAX_PADDED_PROOF_COUNT
    );
    let artifact_filename = Path::new(entry.artifact_filename);
    ensure!(
        artifact_filename.file_name() == Some(artifact_filename.as_os_str()),
        "production SnarkPack SRS artifact filename must be one relative path component"
    );
    Ok(entry)
}

fn cached_active_production_srs(entry: &'static ProductionSrsRegistryEntry) -> Result<DevSrs> {
    let loaded = ACTIVE_PRODUCTION_SRS
        .get_or_init(|| load_production_srs_entry(entry).map_err(|error| format!("{error:#}")));
    loaded
        .as_ref()
        .map(Clone::clone)
        .map_err(|error| anyhow!("{error}"))
}

fn load_production_srs_entry(entry: &ProductionSrsRegistryEntry) -> Result<DevSrs> {
    let artifact_path = production_srs_artifact_path(entry)?;
    let artifact = fs::File::open(&artifact_path)
        .with_context(|| format!("open production SnarkPack SRS {}", artifact_path.display()))?;
    let artifact_metadata = artifact.metadata().with_context(|| {
        format!(
            "stat opened production SnarkPack SRS {}",
            artifact_path.display()
        )
    })?;
    ensure!(
        artifact_metadata.is_file(),
        "production SnarkPack SRS {} is not a regular file",
        artifact_path.display()
    );
    let bytes = read_bounded_srs_artifact(artifact, MAX_PRODUCTION_SRS_ARTIFACT_BYTES)
        .with_context(|| format!("read production SnarkPack SRS {}", artifact_path.display()))?;
    decode_production_srs_entry(entry, &bytes)
        .with_context(|| format!("load production SnarkPack SRS {}", artifact_path.display()))
}

fn read_bounded_srs_artifact(reader: impl Read, max_bytes: u64) -> Result<Vec<u8>> {
    let read_limit = max_bytes
        .checked_add(1)
        .context("production SnarkPack SRS byte limit overflow")?;
    let mut bytes = Vec::new();
    reader
        .take(read_limit)
        .read_to_end(&mut bytes)
        .context("read bounded production SnarkPack SRS bytes")?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= max_bytes,
        "production SnarkPack SRS has {} bytes, expected 1..={}",
        bytes.len(),
        max_bytes
    );
    Ok(bytes)
}

fn production_srs_artifact_path(entry: &ProductionSrsRegistryEntry) -> Result<PathBuf> {
    let root = match std::env::var_os(PRODUCTION_SRS_ARTIFACT_DIR_ENV) {
        Some(root) => {
            ensure!(
                !root.is_empty(),
                "{PRODUCTION_SRS_ARTIFACT_DIR_ENV} must not be empty"
            );
            let root = PathBuf::from(root);
            ensure!(
                root.is_absolute(),
                "{PRODUCTION_SRS_ARTIFACT_DIR_ENV} must be an absolute path"
            );
            root
        }
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("artifacts"),
    };
    Ok(root.join(entry.artifact_filename))
}

fn decode_production_srs_entry(entry: &ProductionSrsRegistryEntry, bytes: &[u8]) -> Result<DevSrs> {
    let artifact_sha256: [u8; 32] = Sha256::digest(bytes).into();
    ensure!(
        artifact_sha256 == entry.artifact_sha256,
        "production SnarkPack SRS artifact hash mismatch: expected {}, got {}",
        hex::encode(entry.artifact_sha256),
        hex::encode(artifact_sha256)
    );
    validate_production_srs_serialized_shape(bytes, entry.max_padded_count)
        .context("strictly decode canonical production SnarkPack SRS")?;
    let inner_product_srs = deserialize_compressed_strict::<InnerProductSrs<Bls12_377>>(bytes)
        .context("strictly decode canonical production SnarkPack SRS")?;
    validate_production_srs_shape(&inner_product_srs, entry.max_padded_count)?;

    let computed_id = registered_srs_id(entry.max_padded_count, bytes);
    ensure!(
        computed_id == entry.id,
        "production SnarkPack SRS identifier mismatch: expected {}, got {}",
        hex::encode(entry.id),
        hex::encode(computed_id)
    );
    let verifier_srs = inner_product_srs.get_verifier_key();
    Ok(DevSrs {
        max_padded_count: entry.max_padded_count,
        source: SrsSource::Registered(Arc::new(RegisteredSrsMaterial {
            id: entry.id,
            inner_product_srs,
            verifier_srs,
        })),
    })
}

fn validate_production_srs_serialized_shape(bytes: &[u8], max_padded_count: u32) -> Result<()> {
    ensure!(
        max_padded_count > 0 && max_padded_count.is_power_of_two(),
        "production SnarkPack SRS max padded count must be a non-zero power of two"
    );
    let expected_power_count = (max_padded_count as usize)
        .checked_mul(2)
        .and_then(|count| count.checked_sub(1))
        .context("production SnarkPack SRS power count overflow")?;
    let expected_power_count_u64 =
        u64::try_from(expected_power_count).context("production SnarkPack SRS count overflow")?;

    let g1_size = <Bls12_377 as Pairing>::G1::zero().compressed_size();
    let g2_size = <Bls12_377 as Pairing>::G2::zero().compressed_size();
    let length_prefix_size = std::mem::size_of::<u64>();
    let second_length_offset = length_prefix_size
        .checked_add(
            expected_power_count
                .checked_mul(g1_size)
                .context("production SnarkPack SRS G1 byte count overflow")?,
        )
        .context("production SnarkPack SRS G1 layout overflow")?;
    let expected_bytes = second_length_offset
        .checked_add(length_prefix_size)
        .and_then(|size| {
            expected_power_count
                .checked_mul(g2_size)
                .and_then(|g2_bytes| size.checked_add(g2_bytes))
        })
        .and_then(|size| size.checked_add(g1_size))
        .and_then(|size| size.checked_add(g2_size))
        .context("production SnarkPack SRS serialized size overflow")?;

    let read_length = |offset: usize, lane: &str| -> Result<u64> {
        let end = offset
            .checked_add(length_prefix_size)
            .context("production SnarkPack SRS length offset overflow")?;
        let encoded = bytes.get(offset..end).ok_or_else(|| {
            anyhow!("production SnarkPack SRS is missing the {lane} vector length at byte {offset}")
        })?;
        Ok(u64::from_le_bytes(
            encoded
                .try_into()
                .expect("u64 length prefix slice has exact size"),
        ))
    };

    let g1_count = read_length(0, "G1")?;
    ensure!(
        g1_count == expected_power_count_u64,
        "production SnarkPack SRS declares {g1_count} G1 powers, expected {expected_power_count}"
    );
    let g2_count = read_length(second_length_offset, "G2")?;
    ensure!(
        g2_count == expected_power_count_u64,
        "production SnarkPack SRS declares {g2_count} G2 powers, expected {expected_power_count}"
    );
    ensure!(
        bytes.len() == expected_bytes,
        "production SnarkPack SRS has {} serialized bytes, expected {}",
        bytes.len(),
        expected_bytes
    );
    Ok(())
}

fn validate_production_srs_shape(
    srs: &InnerProductSrs<Bls12_377>,
    max_padded_count: u32,
) -> Result<()> {
    ensure!(
        max_padded_count > 0 && max_padded_count.is_power_of_two(),
        "production SnarkPack SRS max padded count must be a non-zero power of two"
    );
    let expected_power_count = (max_padded_count as usize)
        .checked_mul(2)
        .and_then(|count| count.checked_sub(1))
        .context("production SnarkPack SRS power count overflow")?;
    ensure!(
        srs.g_alpha_powers.len() == expected_power_count,
        "production SnarkPack SRS has {} G1 powers, expected {}",
        srs.g_alpha_powers.len(),
        expected_power_count
    );
    ensure!(
        srs.h_beta_powers.len() == expected_power_count,
        "production SnarkPack SRS has {} G2 powers, expected {}",
        srs.h_beta_powers.len(),
        expected_power_count
    );
    ensure!(
        srs.g_alpha_powers.iter().all(|point| !point.is_zero()),
        "production SnarkPack SRS contains a zero G1 power"
    );
    ensure!(
        srs.h_beta_powers.iter().all(|point| !point.is_zero()),
        "production SnarkPack SRS contains a zero G2 power"
    );
    ensure!(
        !srs.g_beta.is_zero(),
        "production SnarkPack SRS g_beta is zero"
    );
    ensure!(
        !srs.h_alpha.is_zero(),
        "production SnarkPack SRS h_alpha is zero"
    );
    Ok(())
}

fn registered_srs_id(max_padded_count: u32, serialized_srs: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PRODUCTION_SRS_ID_DOMAIN);
    hasher.update(DEV_SRS_BACKEND_ID.as_bytes());
    hasher.update(DEV_SRS_CURVE_ID.as_bytes());
    hasher.update(max_padded_count.to_le_bytes());
    hasher.update((serialized_srs.len() as u64).to_le_bytes());
    hasher.update(serialized_srs);
    hasher.finalize().into()
}

pub fn srs_id(srs: &DevSrs) -> [u8; 32] {
    if let SrsSource::Registered(material) = &srs.source {
        return material.id;
    }
    if srs.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT {
        return DEFAULT_DEV_SRS_ID;
    }

    compute_srs_id(srs)
}

fn compute_srs_id(srs: &DevSrs) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if srs.max_padded_count == DEFAULT_MAX_PADDED_PROOF_COUNT {
        let serialized = srs
            .serialized_inner_product_srs_compressed()
            .expect("serializing cached dev SRS must succeed");
        hasher.update(default_dev_srs_id_preimage(&serialized));
    } else {
        hasher.update(
            format!(
                "shieldd.proof_aggregation.srs:backend={DEV_SRS_BACKEND_ID}:curve={DEV_SRS_CURVE_ID}:max_padded_count={}",
                srs.max_padded_count
            )
            .as_bytes(),
        );
    }
    hasher.finalize().into()
}

pub fn default_dev_srs_id_preimage(serialized_srs: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DEFAULT_DEV_SRS_ID_PREFIX.len() + serialized_srs.len());
    bytes.extend_from_slice(DEFAULT_DEV_SRS_ID_PREFIX);
    bytes.extend_from_slice(serialized_srs);
    bytes
}

pub fn srs_report(srs: &DevSrs) -> Result<DevSrsReport> {
    let inner_product_srs = srs.full_inner_product_srs()?;
    let compressed = srs.serialized_inner_product_srs_compressed()?;
    let uncompressed = srs.serialized_inner_product_srs_uncompressed()?;

    let started = Instant::now();
    // This reparses Shieldd-generated bytes for a report, not an untrusted load.
    let decoded = deserialize_compressed_strict::<InnerProductSrs<Bls12_377>>(&compressed)?;
    let load_ms = started.elapsed().as_millis();
    ensure!(
        decoded.g_alpha_powers.len() == inner_product_srs.g_alpha_powers.len()
            && decoded.h_beta_powers.len() == inner_product_srs.h_beta_powers.len(),
        "deserialized SRS dimensions do not match loaded SRS"
    );

    Ok(DevSrsReport {
        generation_ms: srs.generation_ms()?,
        load_ms,
        compressed_bytes: compressed.len(),
        uncompressed_bytes: uncompressed.len(),
    })
}

impl DevSrs {
    pub fn serialized_inner_product_srs_compressed(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.full_inner_product_srs()?
            .serialize_compressed(&mut bytes)?;
        Ok(bytes)
    }

    pub fn serialized_inner_product_srs_uncompressed(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.full_inner_product_srs()?
            .serialize_uncompressed(&mut bytes)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use sha2::{Digest as _, Sha256};

    use super::{
        compute_srs_id, decode_production_srs_entry, load_active_production_srs,
        load_production_srs_for_id, read_bounded_srs_artifact, srs_id, DevSrs,
        ProductionSrsRegistryEntry, DEFAULT_DEV_SRS_ID,
    };
    use super::{
        DEFAULT_DEV_SRS_ID_PREFIX, DEFAULT_MAX_PADDED_PROOF_COUNT, DEV_SRS_BACKEND_ID,
        DEV_SRS_CURVE_ID,
    };

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
            source: super::SrsSource::Development,
        });
        let b = srs_id(&DevSrs {
            max_padded_count: 16_384,
            source: super::SrsSource::Development,
        });
        assert_ne!(a, b);
    }

    #[test]
    fn development_srs_is_never_registered() {
        assert!(!DevSrs::default().is_registered());
    }

    #[test]
    fn production_registry_fails_closed_until_ceremony_entry_is_pinned() {
        let active_error =
            load_active_production_srs().expect_err("empty production registry must reject");
        assert!(active_error
            .to_string()
            .contains("no production SnarkPack SRS is registered"));

        let selected_error = load_production_srs_for_id(&DEFAULT_DEV_SRS_ID)
            .expect_err("development SRS id must not load from production registry");
        assert!(selected_error
            .to_string()
            .contains("no production SnarkPack SRS is registered"));
    }

    #[test]
    fn production_registry_rejects_noncanonical_identifier_length() {
        let error = load_production_srs_for_id(&[0u8; 31])
            .expect_err("noncanonical SRS id length must reject");
        assert!(error
            .to_string()
            .contains("SRS id must be 32 bytes, got 31"));
    }

    #[test]
    fn production_artifact_hash_is_checked_before_decoding() {
        let entry = ProductionSrsRegistryEntry {
            id: [1; 32],
            artifact_sha256: [2; 32],
            artifact_filename: "never-read.srs",
            max_padded_count: DEFAULT_MAX_PADDED_PROOF_COUNT,
        };
        let error = decode_production_srs_entry(&entry, b"not an SRS")
            .expect_err("wrong artifact digest must reject");
        assert!(error.to_string().contains("artifact hash mismatch"));
    }

    #[test]
    fn matching_digest_still_requires_strict_canonical_srs_decoding() {
        let bytes = b"not an SRS";
        let entry = ProductionSrsRegistryEntry {
            id: [1; 32],
            artifact_sha256: Sha256::digest(bytes).into(),
            artifact_filename: "never-read.srs",
            max_padded_count: DEFAULT_MAX_PADDED_PROOF_COUNT,
        };
        let error = decode_production_srs_entry(&entry, bytes)
            .expect_err("malformed artifact must reject after matching its digest");
        assert!(error
            .to_string()
            .contains("strictly decode canonical production SnarkPack SRS"));
    }

    #[test]
    fn production_artifact_read_is_bounded_by_actual_bytes() {
        assert_eq!(
            read_bounded_srs_artifact(Cursor::new([1, 2, 3, 4]), 4).expect("exact bound must load"),
            vec![1, 2, 3, 4]
        );
        let oversized = read_bounded_srs_artifact(Cursor::new([1, 2, 3, 4, 5]), 4)
            .expect_err("one byte over the bound must reject");
        assert!(oversized.to_string().contains("has 5 bytes"));
        let empty =
            read_bounded_srs_artifact(Cursor::new([]), 4).expect_err("empty artifact must reject");
        assert!(empty.to_string().contains("has 0 bytes"));
    }

    #[test]
    fn default_srs_id_prefix_matches_declared_fields() {
        let expected = format!(
            "shieldd.proof_aggregation.srs:backend={DEV_SRS_BACKEND_ID}:curve={DEV_SRS_CURVE_ID}:max_padded_count={DEFAULT_MAX_PADDED_PROOF_COUNT}"
        );

        assert_eq!(DEFAULT_DEV_SRS_ID_PREFIX, expected.as_bytes());
    }

    #[test]
    #[ignore = "serializes the full default SRS; run only when SRS generation changes"]
    fn checked_in_default_srs_id_matches_generated_srs() {
        assert_eq!(DEFAULT_DEV_SRS_ID, compute_srs_id(&DevSrs::default()));
    }
}
