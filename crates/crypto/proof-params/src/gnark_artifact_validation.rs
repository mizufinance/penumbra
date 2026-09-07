use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) const CIRCUIT_METADATA_SCHEMA: &str = "shieldd.gnark.circuit_metadata";
const CONSTRAINT_MANIFEST_SCHEMA: &str = "shieldd.gnark.constraint_manifest";
const SETUP_PROVENANCE_SCHEMA: &str = "shieldd.gnark.setup_provenance";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FamilyKind {
    Transfer,
    NoteReshape,
    ShieldedIcs20Withdrawal,
    NoteSeizure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputPadding {
    Fixed,
    SyntheticPrivate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputPadding {
    Fixed,
    ZeroNote,
}

/// A closed build-time identity for one deployed proof family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeployedFamily {
    pub kind: FamilyKind,
    pub id: Option<u32>,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
    pub input_padding: InputPadding,
    pub output_padding: OutputPadding,
    pub min_real_inputs: usize,
    pub max_real_inputs: usize,
    pub min_real_outputs: usize,
    pub max_real_outputs: usize,
}

pub(crate) const DEPLOYED_FAMILIES: [DeployedFamily; 5] = [
    DeployedFamily {
        kind: FamilyKind::Transfer,
        id: None,
        label: "transfer",
        artifact_name: "transfer",
        n_in: 2,
        n_out: 2,
        input_padding: InputPadding::Fixed,
        output_padding: OutputPadding::Fixed,
        min_real_inputs: 2,
        max_real_inputs: 2,
        min_real_outputs: 2,
        max_real_outputs: 2,
    },
    DeployedFamily {
        kind: FamilyKind::NoteReshape,
        id: Some(2),
        label: "note_reshape1x8",
        artifact_name: "note_reshape1x8",
        n_in: 1,
        n_out: 8,
        input_padding: InputPadding::Fixed,
        output_padding: OutputPadding::ZeroNote,
        min_real_inputs: 1,
        max_real_inputs: 1,
        min_real_outputs: 2,
        max_real_outputs: 8,
    },
    DeployedFamily {
        kind: FamilyKind::NoteReshape,
        id: Some(3),
        label: "note_reshape8x1",
        artifact_name: "note_reshape8x1",
        n_in: 8,
        n_out: 1,
        input_padding: InputPadding::SyntheticPrivate,
        output_padding: OutputPadding::Fixed,
        min_real_inputs: 2,
        max_real_inputs: 8,
        min_real_outputs: 1,
        max_real_outputs: 1,
    },
    DeployedFamily {
        kind: FamilyKind::ShieldedIcs20Withdrawal,
        id: Some(1),
        label: "shielded_ics20_withdrawal",
        artifact_name: "shielded_ics20_withdrawal",
        n_in: 2,
        n_out: 1,
        input_padding: InputPadding::Fixed,
        output_padding: OutputPadding::Fixed,
        min_real_inputs: 2,
        max_real_inputs: 2,
        min_real_outputs: 1,
        max_real_outputs: 1,
    },
    DeployedFamily {
        kind: FamilyKind::NoteSeizure,
        id: None,
        label: "note_seizure",
        artifact_name: "note_seizure",
        n_in: 1,
        n_out: 0,
        input_padding: InputPadding::Fixed,
        output_padding: OutputPadding::Fixed,
        min_real_inputs: 1,
        max_real_inputs: 1,
        min_real_outputs: 0,
        max_real_outputs: 0,
    },
];

pub(crate) fn validate_deployed_family_roster(actual: &[DeployedFamily]) -> Result<()> {
    if actual != DEPLOYED_FAMILIES {
        bail!(
            "generated proof-family roster does not match the exact deployed families:\nexpected: {:#?}\nactual: {actual:#?}",
            DEPLOYED_FAMILIES
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct G1PointJson {
    pub x: String,
    pub y: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fq2Json {
    pub a0: String,
    pub a1: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct G2PointJson {
    pub x: Fq2Json,
    pub y: Fq2Json,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifyingKeyJson {
    pub alpha_g1: G1PointJson,
    pub beta_g2: G2PointJson,
    pub gamma_g2: G2PointJson,
    pub delta_g2: G2PointJson,
    pub gamma_abc_g1: Vec<G1PointJson>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CircuitMetadataJson {
    pub schema: String,
    pub curve: String,
    pub circuit: String,
    pub proving_key_size_bytes: u64,
    pub verifying_key_size_bytes: u64,
    pub nb_constraints: u64,
    pub nb_public_variables: u64,
    pub nb_secret_variables: u64,
    pub sr1cs_sha256_hex: String,
    pub setup_provenance_sha256_hex: String,
    pub proving_key_sha256_hex: String,
    pub verifying_key_binary_sha256_hex: String,
    pub verifying_key_json_sha256_hex: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GenerationSelfTestJson {
    proof_case: String,
    witness_sha256_hex: String,
    proved_and_verified_in_process: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetupProvenanceJson {
    schema: String,
    curve: String,
    circuit: String,
    mode: String,
    sr1cs_sha256_hex: String,
    proving_key_sha256_hex: String,
    verifying_key_binary_sha256_hex: String,
    verifying_key_json_sha256_hex: String,
    generation_self_tests: Vec<GenerationSelfTestJson>,
    setup_transcript: String,
    toxic_waste_erasure: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstraintManifestJson {
    schema: String,
    circuit: String,
    shape: CircuitShapeJson,
    nb_constraints: u64,
    nb_public_variables: u64,
    nb_secret_variables: u64,
    nb_internal_variables: u64,
    sr1cs_sha256_hex: String,
    witness_wires: serde_json::Value,
    semantic_bindings: serde_json::Value,
    segments: serde_json::Value,
    breakdown: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CircuitShapeJson {
    n_in: usize,
    n_out: usize,
}

pub(crate) fn decode_canonical_json<T>(bytes: &[u8], label: &str) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let value: T =
        serde_json::from_slice(bytes).with_context(|| format!("decode canonical {label}"))?;
    let canonical = canonical_json_bytes(&value)?;
    if bytes != canonical {
        bail!("{label} is not canonical two-space-indented JSON");
    }
    Ok(value)
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut canonical = serde_json::to_vec_pretty(value)?;
    canonical.push(b'\n');
    Ok(canonical)
}

pub(crate) fn validate_circuit_metadata(
    metadata: &CircuitMetadataJson,
    expected_circuit: &str,
) -> Result<()> {
    if metadata.schema != CIRCUIT_METADATA_SCHEMA {
        bail!("unsupported circuit metadata schema {}", metadata.schema);
    }
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
    if metadata.proving_key_size_bytes == 0
        || metadata.verifying_key_size_bytes == 0
        || metadata.nb_constraints == 0
        || metadata.nb_secret_variables == 0
    {
        bail!("artifact metadata has a zero size or circuit dimension");
    }
    if metadata.nb_public_variables != 2 {
        bail!(
            "{} metadata has {} public variables; deployed statement-hash circuits require 2",
            expected_circuit,
            metadata.nb_public_variables
        );
    }
    for (label, value) in [
        ("SR1CS", &metadata.sr1cs_sha256_hex),
        ("setup provenance", &metadata.setup_provenance_sha256_hex),
        ("proving key", &metadata.proving_key_sha256_hex),
        (
            "binary verifying key",
            &metadata.verifying_key_binary_sha256_hex,
        ),
        (
            "JSON verifying key",
            &metadata.verifying_key_json_sha256_hex,
        ),
    ] {
        if !is_lower_sha256(value) {
            bail!("artifact metadata has invalid {label} hash");
        }
    }
    Ok(())
}

/// Validate the metadata and verification material kept in Git.
pub(crate) fn validate_repository_artifacts(
    artifact_root: &Path,
    family: DeployedFamily,
) -> Result<()> {
    let dir = artifact_root.join(family.artifact_name);
    require_directory(&dir)?;
    validate_artifact_directory_roster(artifact_root, family, false)?;

    let metadata_path = dir.join("circuit_metadata.json");
    let metadata_bytes = read_regular_file(&metadata_path)?;
    let metadata: CircuitMetadataJson = decode_canonical_json(
        &metadata_bytes,
        &format!("{} circuit_metadata.json", family.label),
    )?;
    validate_circuit_metadata(&metadata, family.label)?;

    let manifest_path = dir.join(format!("{}-manifest.json", family.artifact_name));
    let manifest_bytes = read_regular_file(&manifest_path)?;
    let manifest: ConstraintManifestJson = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("decode {}", manifest_path.display()))?;
    validate_manifest(&manifest, family, &metadata)?;

    let setup_path = dir.join("setup_provenance.json");
    let setup_bytes = read_regular_file(&setup_path)?;
    validate_bytes_hash(
        &setup_path,
        "setup provenance",
        &setup_bytes,
        &metadata.setup_provenance_sha256_hex,
    )?;
    let setup: SetupProvenanceJson = decode_canonical_json(
        &setup_bytes,
        &format!("{} setup_provenance.json", family.label),
    )?;
    validate_setup_provenance(&setup, family.label, &metadata)?;

    validate_hash(
        &dir.join("verifying_key.bin"),
        "binary verifying key",
        &metadata.verifying_key_binary_sha256_hex,
        Some(metadata.verifying_key_size_bytes),
    )?;

    let vk_json_path = dir.join("verifying_key.json");
    let vk_json_bytes = read_regular_file(&vk_json_path)?;
    validate_bytes_hash(
        &vk_json_path,
        "JSON verifying key",
        &vk_json_bytes,
        &metadata.verifying_key_json_sha256_hex,
    )?;
    let vk: VerifyingKeyJson = decode_canonical_json(
        &vk_json_bytes,
        &format!("{} verifying_key.json", family.label),
    )?;
    validate_verifying_key_json(&vk, metadata.nb_public_variables, family.label)?;

    Ok(())
}

/// Validate the materialized SR1CS and proving key against committed metadata.
#[cfg(test)]
pub(crate) fn validate_materialized_artifacts(
    artifact_root: &Path,
    family: DeployedFamily,
) -> Result<()> {
    validate_repository_artifacts(artifact_root, family)?;
    validate_artifact_directory_roster(artifact_root, family, true)?;
    let dir = artifact_root.join(family.artifact_name);
    let metadata_path = dir.join("circuit_metadata.json");
    let metadata: CircuitMetadataJson = decode_canonical_json(
        &read_regular_file(&metadata_path)?,
        &format!("{} circuit_metadata.json", family.label),
    )?;
    validate_hash(
        &dir.join(format!("{}.sr1cs", family.artifact_name)),
        "SR1CS",
        &metadata.sr1cs_sha256_hex,
        None,
    )?;
    validate_hash(
        &dir.join("proving_key.bin"),
        "proving key",
        &metadata.proving_key_sha256_hex,
        Some(metadata.proving_key_size_bytes),
    )?;
    Ok(())
}

/// Validate the materialized proving key needed by runtime builds.
pub(crate) fn validate_materialized_proving_key(
    artifact_root: &Path,
    family: DeployedFamily,
) -> Result<()> {
    validate_repository_artifacts(artifact_root, family)?;
    validate_artifact_directory_roster(artifact_root, family, true)?;
    let dir = artifact_root.join(family.artifact_name);
    let metadata_path = dir.join("circuit_metadata.json");
    let metadata: CircuitMetadataJson = decode_canonical_json(
        &read_regular_file(&metadata_path)?,
        &format!("{} circuit_metadata.json", family.label),
    )?;
    validate_hash(
        &dir.join("proving_key.bin"),
        "proving key",
        &metadata.proving_key_sha256_hex,
        Some(metadata.proving_key_size_bytes),
    )?;
    Ok(())
}

#[cfg(test)]
fn validate_family_artifacts(artifact_root: &Path, family: DeployedFamily) -> Result<()> {
    validate_materialized_artifacts(artifact_root, family)
}

pub(crate) fn repository_artifact_paths(
    artifact_root: &Path,
    family: DeployedFamily,
) -> [PathBuf; 5] {
    let dir = artifact_root.join(family.artifact_name);
    [
        dir.join("circuit_metadata.json"),
        dir.join(format!("{}-manifest.json", family.artifact_name)),
        dir.join("setup_provenance.json"),
        dir.join("verifying_key.bin"),
        dir.join("verifying_key.json"),
    ]
}

pub(crate) fn artifact_paths(artifact_root: &Path, family: DeployedFamily) -> [PathBuf; 7] {
    let dir = artifact_root.join(family.artifact_name);
    [
        dir.join("circuit_metadata.json"),
        dir.join(format!("{}-manifest.json", family.artifact_name)),
        dir.join(format!("{}.sr1cs", family.artifact_name)),
        dir.join("setup_provenance.json"),
        dir.join("proving_key.bin"),
        dir.join("verifying_key.bin"),
        dir.join("verifying_key.json"),
    ]
}

fn validate_artifact_directory_roster(
    artifact_root: &Path,
    family: DeployedFamily,
    require_materialized: bool,
) -> Result<()> {
    let dir = artifact_root.join(family.artifact_name);
    let repository = repository_artifact_paths(artifact_root, family)
        .into_iter()
        .map(|path| {
            path.file_name()
                .expect("artifact paths always have file names")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    let materialized = artifact_paths(artifact_root, family)
        .into_iter()
        .map(|path| {
            path.file_name()
                .expect("artifact paths always have file names")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    let actual = fs::read_dir(&dir)
        .with_context(|| format!("enumerate artifact directory {}", dir.display()))?
        .map(|entry| {
            entry
                .with_context(|| format!("enumerate artifact directory {}", dir.display()))
                .map(|entry| entry.file_name())
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let matches = if require_materialized {
        actual == materialized
    } else {
        actual == repository || actual == materialized
    };
    if !matches {
        bail!(
            "{} artifact directory roster mismatch: expected {:?}, got {actual:?}",
            family.label,
            if require_materialized {
                &materialized
            } else {
                &repository
            },
        );
    }
    Ok(())
}

fn validate_manifest(
    manifest: &ConstraintManifestJson,
    family: DeployedFamily,
    metadata: &CircuitMetadataJson,
) -> Result<()> {
    if manifest.schema != CONSTRAINT_MANIFEST_SCHEMA {
        bail!(
            "{} has unsupported constraint manifest schema {}",
            family.label,
            manifest.schema
        );
    }
    if manifest.circuit != family.label {
        bail!(
            "{} constraint manifest circuit {} does not match its deployed identity",
            family.label,
            manifest.circuit
        );
    }
    if (manifest.shape.n_in, manifest.shape.n_out) != (family.n_in, family.n_out) {
        bail!(
            "{} constraint manifest shape {}x{} does not match deployed shape {}x{}",
            family.label,
            manifest.shape.n_in,
            manifest.shape.n_out,
            family.n_in,
            family.n_out
        );
    }
    for (label, actual, expected) in [
        (
            "constraint count",
            manifest.nb_constraints,
            metadata.nb_constraints,
        ),
        (
            "public-variable count",
            manifest.nb_public_variables,
            metadata.nb_public_variables,
        ),
        (
            "secret-variable count",
            manifest.nb_secret_variables,
            metadata.nb_secret_variables,
        ),
    ] {
        if actual != expected {
            bail!(
                "{} manifest {label} {actual} does not match metadata {expected}",
                family.label
            );
        }
    }
    if manifest.nb_internal_variables == 0 {
        bail!("{} manifest has no internal variables", family.label);
    }
    if manifest.sr1cs_sha256_hex != metadata.sr1cs_sha256_hex {
        bail!(
            "{} manifest SR1CS hash does not match circuit metadata",
            family.label
        );
    }
    if !manifest.witness_wires.is_array()
        || !manifest.semantic_bindings.is_array()
        || !manifest.segments.is_array()
        || !manifest.breakdown.is_object()
    {
        bail!(
            "{} constraint manifest has invalid section types",
            family.label
        );
    }
    Ok(())
}

fn validate_setup_provenance(
    setup: &SetupProvenanceJson,
    expected_circuit: &str,
    metadata: &CircuitMetadataJson,
) -> Result<()> {
    if setup.schema != SETUP_PROVENANCE_SCHEMA {
        bail!(
            "{expected_circuit} has unsupported setup provenance schema {}",
            setup.schema
        );
    }
    if setup.curve != "bls12-377"
        || setup.circuit != expected_circuit
        || setup.mode != "fresh_setup"
    {
        bail!("{expected_circuit} setup provenance identity or mode mismatch");
    }
    for (label, actual, expected) in [
        ("SR1CS", &setup.sr1cs_sha256_hex, &metadata.sr1cs_sha256_hex),
        (
            "proving key",
            &setup.proving_key_sha256_hex,
            &metadata.proving_key_sha256_hex,
        ),
        (
            "binary verifying key",
            &setup.verifying_key_binary_sha256_hex,
            &metadata.verifying_key_binary_sha256_hex,
        ),
        (
            "JSON verifying key",
            &setup.verifying_key_json_sha256_hex,
            &metadata.verifying_key_json_sha256_hex,
        ),
    ] {
        if actual != expected {
            bail!("{expected_circuit} setup provenance {label} hash mismatch");
        }
    }
    if setup.generation_self_tests.is_empty() {
        bail!("{expected_circuit} setup provenance has no generation self-test");
    }
    let mut proof_cases = BTreeSet::new();
    let mut witness_hashes = BTreeSet::new();
    for test in &setup.generation_self_tests {
        if test.proof_case.is_empty()
            || !is_lower_sha256(&test.witness_sha256_hex)
            || !test.proved_and_verified_in_process
        {
            bail!(
                "{expected_circuit} setup provenance has an invalid generation self-test receipt"
            );
        }
        if !proof_cases.insert(&test.proof_case) {
            bail!(
                "{expected_circuit} setup provenance repeats generation self-test case {}",
                test.proof_case
            );
        }
        if !witness_hashes.insert(&test.witness_sha256_hex) {
            bail!(
                "{expected_circuit} setup provenance repeats a generation self-test witness hash"
            );
        }
    }
    if setup.setup_transcript != "not_recorded"
        || setup.toxic_waste_erasure != "not_mechanically_verified"
    {
        bail!("{expected_circuit} setup provenance has an unexpected setup-assumption status");
    }
    Ok(())
}

fn validate_verifying_key_json(
    vk: &VerifyingKeyJson,
    nb_public_variables: u64,
    circuit: &str,
) -> Result<()> {
    if vk.gamma_abc_g1.len() as u64 != nb_public_variables {
        bail!(
            "{circuit} JSON verifying key has {} gamma_abc points, expected {nb_public_variables}",
            vk.gamma_abc_g1.len()
        );
    }
    let mut coordinates = vec![
        &vk.alpha_g1.x,
        &vk.alpha_g1.y,
        &vk.beta_g2.x.a0,
        &vk.beta_g2.x.a1,
        &vk.beta_g2.y.a0,
        &vk.beta_g2.y.a1,
        &vk.gamma_g2.x.a0,
        &vk.gamma_g2.x.a1,
        &vk.gamma_g2.y.a0,
        &vk.gamma_g2.y.a1,
        &vk.delta_g2.x.a0,
        &vk.delta_g2.x.a1,
        &vk.delta_g2.y.a0,
        &vk.delta_g2.y.a1,
    ];
    for point in &vk.gamma_abc_g1 {
        coordinates.extend([&point.x, &point.y]);
    }
    if coordinates
        .into_iter()
        .any(|coordinate| !is_canonical_decimal(coordinate))
    {
        bail!("{circuit} JSON verifying key has a noncanonical field decimal");
    }
    Ok(())
}

fn is_canonical_decimal(value: &str) -> bool {
    value == "0"
        || value
            .as_bytes()
            .first()
            .is_some_and(|first| (b'1'..=b'9').contains(first))
            && value.as_bytes()[1..]
                .iter()
                .all(|byte| byte.is_ascii_digit())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect artifact directory {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "artifact directory is not a direct directory: {}",
            path.display()
        );
    }
    Ok(())
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>> {
    require_regular_file(path)?;
    fs::read(path).with_context(|| format!("read artifact {}", path.display()))
}

fn require_regular_file(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect artifact {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("artifact is not a direct regular file: {}", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            bail!(
                "artifact has {} hard-link aliases: {}",
                metadata.nlink(),
                path.display()
            );
        }
    }
    Ok(metadata)
}

fn validate_hash(
    path: &Path,
    label: &str,
    expected_hash: &str,
    expected_size: Option<u64>,
) -> Result<()> {
    let metadata = require_regular_file(path)?;
    if let Some(expected_size) = expected_size {
        if metadata.len() != expected_size {
            bail!(
                "{} size mismatch for {}: expected {}, got {}",
                label,
                path.display(),
                expected_size,
                metadata.len()
            );
        }
    }
    let mut file =
        fs::File::open(path).with_context(|| format!("open artifact {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("hash artifact {}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual_hash = hex::encode(hasher.finalize());
    if actual_hash != expected_hash {
        bail!(
            "{label} hash mismatch for {}: expected {expected_hash}, got {actual_hash}",
            path.display()
        );
    }
    Ok(())
}

fn validate_bytes_hash(path: &Path, label: &str, bytes: &[u8], expected_hash: &str) -> Result<()> {
    let actual_hash = hex::encode(Sha256::digest(bytes));
    if actual_hash != expected_hash {
        bail!(
            "{label} hash mismatch for {}: expected {expected_hash}, got {actual_hash}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempFixture {
        root: PathBuf,
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn hash(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
        canonical_json_bytes(value).expect("fixture JSON encodes")
    }

    fn synthetic_fixture() -> (TempFixture, DeployedFamily) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "shieldd-proof-params-artifact-test-{}-{nonce}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let family = DEPLOYED_FAMILIES[0];
        let dir = root.join(family.artifact_name);
        fs::create_dir_all(&dir).expect("create fixture");

        let sr1cs = b"synthetic sr1cs\n";
        let pk = b"synthetic proving key";
        let vk_bin = b"synthetic binary verifying key";
        let vk = VerifyingKeyJson {
            alpha_g1: G1PointJson {
                x: "1".into(),
                y: "2".into(),
            },
            beta_g2: G2PointJson {
                x: Fq2Json {
                    a0: "3".into(),
                    a1: "4".into(),
                },
                y: Fq2Json {
                    a0: "5".into(),
                    a1: "6".into(),
                },
            },
            gamma_g2: G2PointJson {
                x: Fq2Json {
                    a0: "7".into(),
                    a1: "8".into(),
                },
                y: Fq2Json {
                    a0: "9".into(),
                    a1: "10".into(),
                },
            },
            delta_g2: G2PointJson {
                x: Fq2Json {
                    a0: "11".into(),
                    a1: "12".into(),
                },
                y: Fq2Json {
                    a0: "13".into(),
                    a1: "14".into(),
                },
            },
            gamma_abc_g1: vec![
                G1PointJson {
                    x: "15".into(),
                    y: "16".into(),
                },
                G1PointJson {
                    x: "17".into(),
                    y: "18".into(),
                },
            ],
        };
        let vk_json = canonical(&vk);
        let setup = SetupProvenanceJson {
            schema: SETUP_PROVENANCE_SCHEMA.into(),
            curve: "bls12-377".into(),
            circuit: family.label.into(),
            mode: "fresh_setup".into(),
            sr1cs_sha256_hex: hash(sr1cs),
            proving_key_sha256_hex: hash(pk),
            verifying_key_binary_sha256_hex: hash(vk_bin),
            verifying_key_json_sha256_hex: hash(&vk_json),
            generation_self_tests: vec![GenerationSelfTestJson {
                proof_case: "synthetic".into(),
                witness_sha256_hex: hash(b"witness"),
                proved_and_verified_in_process: true,
            }],
            setup_transcript: "not_recorded".into(),
            toxic_waste_erasure: "not_mechanically_verified".into(),
        };
        let setup_json = canonical(&setup);
        let metadata = CircuitMetadataJson {
            schema: CIRCUIT_METADATA_SCHEMA.into(),
            curve: "bls12-377".into(),
            circuit: family.label.into(),
            proving_key_size_bytes: pk.len() as u64,
            verifying_key_size_bytes: vk_bin.len() as u64,
            nb_constraints: 10,
            nb_public_variables: 2,
            nb_secret_variables: 3,
            sr1cs_sha256_hex: hash(sr1cs),
            setup_provenance_sha256_hex: hash(&setup_json),
            proving_key_sha256_hex: hash(pk),
            verifying_key_binary_sha256_hex: hash(vk_bin),
            verifying_key_json_sha256_hex: hash(&vk_json),
        };
        let manifest = serde_json::json!({
            "schema": CONSTRAINT_MANIFEST_SCHEMA,
            "circuit": family.label,
            "shape": {"n_in": family.n_in, "n_out": family.n_out},
            "nb_constraints": metadata.nb_constraints,
            "nb_public_variables": metadata.nb_public_variables,
            "nb_secret_variables": metadata.nb_secret_variables,
            "nb_internal_variables": 4,
            "sr1cs_sha256_hex": metadata.sr1cs_sha256_hex,
            "witness_wires": [],
            "semantic_bindings": [],
            "segments": [],
            "breakdown": {}
        });

        fs::write(dir.join("circuit_metadata.json"), canonical(&metadata)).expect("write metadata");
        fs::write(
            dir.join(format!("{}-manifest.json", family.artifact_name)),
            serde_json::to_vec_pretty(&manifest).expect("encode manifest"),
        )
        .expect("write manifest");
        fs::write(dir.join(format!("{}.sr1cs", family.artifact_name)), sr1cs).expect("write SR1CS");
        fs::write(dir.join("setup_provenance.json"), setup_json).expect("write setup");
        fs::write(dir.join("proving_key.bin"), pk).expect("write PK");
        fs::write(dir.join("verifying_key.bin"), vk_bin).expect("write binary VK");
        fs::write(dir.join("verifying_key.json"), vk_json).expect("write JSON VK");

        (TempFixture { root }, family)
    }

    fn rewrite_metadata(
        fixture: &TempFixture,
        family: DeployedFamily,
        mutate: impl FnOnce(&mut CircuitMetadataJson),
    ) {
        let path = fixture
            .root
            .join(family.artifact_name)
            .join("circuit_metadata.json");
        let mut metadata: CircuitMetadataJson =
            serde_json::from_slice(&fs::read(&path).expect("read metadata"))
                .expect("decode metadata");
        mutate(&mut metadata);
        fs::write(path, canonical(&metadata)).expect("rewrite metadata");
    }

    fn rewrite_setup_and_pin(
        fixture: &TempFixture,
        family: DeployedFamily,
        mutate: impl FnOnce(&mut SetupProvenanceJson),
    ) {
        let path = fixture
            .root
            .join(family.artifact_name)
            .join("setup_provenance.json");
        let mut setup: SetupProvenanceJson =
            serde_json::from_slice(&fs::read(&path).expect("read setup provenance"))
                .expect("decode setup provenance");
        mutate(&mut setup);
        let bytes = canonical(&setup);
        fs::write(path, &bytes).expect("rewrite setup provenance");
        rewrite_metadata(fixture, family, |metadata| {
            metadata.setup_provenance_sha256_hex = hash(&bytes)
        });
    }

    #[test]
    fn exact_roster_and_all_repository_artifacts_validate() {
        validate_deployed_family_roster(&DEPLOYED_FAMILIES).expect("exact roster validates");
        let artifact_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/gnark/artifacts");
        for family in DEPLOYED_FAMILIES {
            validate_repository_artifacts(&artifact_root, family)
                .unwrap_or_else(|error| panic!("{} artifacts failed: {error:#}", family.label));
        }
    }

    #[test]
    fn roster_rejects_family_id_and_shape_mutations() {
        let mut roster = DEPLOYED_FAMILIES;
        roster[1].id = Some(99);
        assert!(validate_deployed_family_roster(&roster).is_err());

        let mut roster = DEPLOYED_FAMILIES;
        roster[2].n_out = 7;
        assert!(validate_deployed_family_roster(&roster).is_err());
    }

    #[test]
    fn metadata_rejects_schema_circuit_and_manifest_shape_mutations() {
        for mutation in 0..3 {
            let (fixture, family) = synthetic_fixture();
            if mutation == 0 {
                rewrite_metadata(&fixture, family, |metadata| {
                    metadata.schema = "shieldd.gnark.wrong_metadata".into()
                });
            } else if mutation == 1 {
                rewrite_metadata(&fixture, family, |metadata| {
                    metadata.circuit = "other".into()
                });
            } else {
                let path = fixture
                    .root
                    .join(family.artifact_name)
                    .join(format!("{}-manifest.json", family.artifact_name));
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).expect("read manifest"))
                        .expect("decode manifest");
                manifest["shape"]["n_in"] = serde_json::json!(99);
                fs::write(
                    path,
                    serde_json::to_vec_pretty(&manifest).expect("encode manifest"),
                )
                .expect("rewrite manifest");
            }
            assert!(
                validate_family_artifacts(&fixture.root, family).is_err(),
                "mutation {mutation} must fail"
            );
        }
    }

    #[test]
    fn every_metadata_pinned_artifact_hash_is_enforced() {
        let mutations = [
            "transfer.sr1cs",
            "setup_provenance.json",
            "proving_key.bin",
            "verifying_key.bin",
            "verifying_key.json",
        ];
        for relative in mutations {
            let (fixture, family) = synthetic_fixture();
            let path = fixture.root.join(family.artifact_name).join(relative);
            let mut bytes = fs::read(&path).expect("read pinned artifact");
            bytes.push(b'!');
            fs::write(&path, bytes).expect("mutate pinned artifact");
            assert!(
                validate_family_artifacts(&fixture.root, family).is_err(),
                "{relative} mutation must fail"
            );
        }

        let (fixture, family) = synthetic_fixture();
        rewrite_metadata(&fixture, family, |metadata| {
            metadata.sr1cs_sha256_hex = "00".repeat(32)
        });
        assert!(validate_family_artifacts(&fixture.root, family).is_err());
    }

    #[test]
    fn runtime_validation_requires_only_the_proving_key() {
        let (fixture, family) = synthetic_fixture();
        let sr1cs = fixture
            .root
            .join(family.artifact_name)
            .join(format!("{}.sr1cs", family.artifact_name));
        fs::write(sr1cs, b"git lfs pointer placeholder").expect("replace SR1CS");
        validate_materialized_proving_key(&fixture.root, family)
            .expect("runtime validation ignores unmaterialized constraints");

        let proving_key = fixture
            .root
            .join(family.artifact_name)
            .join("proving_key.bin");
        fs::write(proving_key, b"corrupted proving key").expect("replace proving key");
        assert!(validate_materialized_proving_key(&fixture.root, family).is_err());
    }

    #[test]
    fn setup_provenance_rejects_status_and_duplicate_self_test_mutations() {
        let (fixture, family) = synthetic_fixture();
        rewrite_setup_and_pin(&fixture, family, |setup| {
            setup.setup_transcript = "unreviewed".into()
        });
        assert!(validate_family_artifacts(&fixture.root, family).is_err());

        let (fixture, family) = synthetic_fixture();
        rewrite_setup_and_pin(&fixture, family, |setup| {
            setup
                .generation_self_tests
                .push(setup.generation_self_tests[0].clone())
        });
        assert!(validate_family_artifacts(&fixture.root, family).is_err());
    }

    #[test]
    fn artifact_directory_rejects_extra_entries_and_hardlink_aliases() {
        let (fixture, family) = synthetic_fixture();
        fs::write(
            fixture.root.join(family.artifact_name).join("unexpected"),
            b"extra",
        )
        .expect("write extra artifact");
        assert!(validate_family_artifacts(&fixture.root, family).is_err());

        #[cfg(unix)]
        {
            let (fixture, family) = synthetic_fixture();
            fs::hard_link(
                fixture
                    .root
                    .join(family.artifact_name)
                    .join("proving_key.bin"),
                fixture.root.join("proving-key-alias"),
            )
            .expect("create hard-link alias");
            assert!(validate_family_artifacts(&fixture.root, family).is_err());
        }
    }
}
