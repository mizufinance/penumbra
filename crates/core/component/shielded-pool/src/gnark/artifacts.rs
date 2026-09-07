use std::{fs, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::PrimeField;
use ark_groth16::{prepare_verifying_key, PreparedVerifyingKey, VerifyingKey};
use decaf377::Bls12_377;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct G1PointJson {
    pub x: String,
    pub y: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fq2Json {
    pub a0: String,
    pub a1: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct G2PointJson {
    pub x: Fq2Json,
    pub y: Fq2Json,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyingKeyJson {
    pub alpha_g1: G1PointJson,
    pub beta_g2: G2PointJson,
    pub gamma_g2: G2PointJson,
    pub delta_g2: G2PointJson,
    pub gamma_abc_g1: Vec<G1PointJson>,
}

const CIRCUIT_METADATA_SCHEMA: &str = "shieldd.gnark.circuit_metadata";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GnarkArtifactMetadata {
    pub schema: String,
    pub curve: String,
    pub circuit: String,
    pub proving_key_size_bytes: i64,
    pub verifying_key_size_bytes: i64,
    pub nb_constraints: i32,
    pub nb_public_variables: i32,
    pub nb_secret_variables: i32,
    pub sr1cs_sha256_hex: String,
    pub setup_provenance_sha256_hex: String,
    pub proving_key_sha256_hex: String,
    pub verifying_key_binary_sha256_hex: String,
    pub verifying_key_json_sha256_hex: String,
}

pub(crate) fn load_artifact_metadata(artifact_dir: &Path) -> Result<GnarkArtifactMetadata> {
    let bytes = fs::read(artifact_dir.join("circuit_metadata.json"))
        .context("read circuit_metadata.json")?;
    load_artifact_metadata_bytes(&bytes, "circuit_metadata.json")
}

pub(crate) fn load_artifact_metadata_bytes(
    bytes: &[u8],
    label: &str,
) -> Result<GnarkArtifactMetadata> {
    decode_canonical_json(bytes, label)
}

pub(crate) fn validate_artifact_metadata(
    metadata: &GnarkArtifactMetadata,
    family: &str,
) -> Result<()> {
    if metadata.schema != CIRCUIT_METADATA_SCHEMA {
        bail!(
            "unsupported gnark {family} circuit metadata schema {}",
            metadata.schema
        );
    }
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
    if metadata.proving_key_size_bytes <= 0 || metadata.verifying_key_size_bytes <= 0 {
        bail!("gnark {family} circuit metadata is missing key sizes");
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
            bail!("gnark {family} circuit metadata has invalid {label} hash");
        }
    }
    Ok(())
}

pub(crate) fn validate_artifact_hashes(
    artifact_dir: &Path,
    metadata: &GnarkArtifactMetadata,
    family: &str,
) -> Result<()> {
    validate_repository_hashes(artifact_dir, metadata, family)?;
    let proving_key = fs::read(artifact_dir.join("proving_key.bin"))?;
    if i64::try_from(proving_key.len()).ok() != Some(metadata.proving_key_size_bytes) {
        bail!(
            "gnark {family} proving key size mismatch: expected {}, got {}",
            metadata.proving_key_size_bytes,
            proving_key.len()
        );
    }
    let actual = sha256_hex(&proving_key);
    if actual != metadata.proving_key_sha256_hex {
        bail!(
            "gnark {family} proving key hash mismatch: expected {}, got {actual}",
            metadata.proving_key_sha256_hex
        );
    }
    Ok(())
}

pub(crate) fn validate_repository_hashes(
    artifact_dir: &Path,
    metadata: &GnarkArtifactMetadata,
    family: &str,
) -> Result<()> {
    let actual = sha256_hex(&fs::read(artifact_dir.join("setup_provenance.json"))?);
    if actual != metadata.setup_provenance_sha256_hex {
        bail!(
            "gnark {family} setup provenance hash mismatch: expected {}, got {actual}",
            metadata.setup_provenance_sha256_hex
        );
    }
    let verifying_key = fs::read(artifact_dir.join("verifying_key.bin"))?;
    if i64::try_from(verifying_key.len()).ok() != Some(metadata.verifying_key_size_bytes) {
        bail!(
            "gnark {family} verifying key size mismatch: expected {}, got {}",
            metadata.verifying_key_size_bytes,
            verifying_key.len()
        );
    }
    let actual = sha256_hex(&verifying_key);
    if actual != metadata.verifying_key_binary_sha256_hex {
        bail!(
            "gnark {family} binary verifying key hash mismatch: expected {}, got {actual}",
            metadata.verifying_key_binary_sha256_hex
        );
    }
    let actual = sha256_hex(&fs::read(artifact_dir.join("verifying_key.json"))?);
    if actual != metadata.verifying_key_json_sha256_hex {
        bail!(
            "gnark {family} JSON verifying key hash mismatch: expected {}, got {actual}",
            metadata.verifying_key_json_sha256_hex
        );
    }
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_canonical_json<T>(bytes: &[u8], label: &str) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let value: T = serde_json::from_slice(bytes).with_context(|| format!("decode {label}"))?;
    let mut canonical =
        serde_json::to_vec_pretty(&value).with_context(|| format!("re-encode {label}"))?;
    canonical.push(b'\n');
    if bytes != canonical {
        bail!("{label} is not canonical two-space-indented JSON");
    }
    Ok(value)
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
    family: &str,
) -> Result<PreparedVerifyingKey<Bls12_377>> {
    let bytes =
        fs::read(artifact_dir.join("verifying_key.json")).context("read verifying_key.json")?;
    load_prepared_vk_bytes(&bytes, family)
}

pub(crate) fn load_prepared_vk_bytes(
    bytes: &[u8],
    family: &str,
) -> Result<PreparedVerifyingKey<Bls12_377>> {
    let vk_json: VerifyingKeyJson =
        decode_canonical_json(bytes, &format!("gnark {family} verifying_key.json"))?;
    Ok(prepare_verifying_key(&verifying_key_from_json(&vk_json)?))
}

fn parse_g1(point: &G1PointJson) -> Result<ProofG1> {
    let x: ProofG1Base = parse_canonical_field(&point.x, "G1 x")?;
    let y: ProofG1Base = parse_canonical_field(&point.y, "G1 y")?;
    let point = ProofG1::new_unchecked(x, y);
    if !point.is_on_curve() {
        bail!("G1 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G1 point is not in the correct subgroup");
    }
    Ok(point)
}

#[cfg(test)]
mod statement_parity_tests {
    use super::*;
    use ark_ff::BigInteger;
    use std::path::PathBuf;

    /// Every deployed family exposes only `ClaimedStatementHash` publicly.
    const EXPECTED_PUBLIC_INPUT_ARITY: usize = 1;

    /// All committed gnark families (one artifact dir each under tools/gnark/artifacts).
    const FAMILIES: &[&str] = &[
        "note_reshape1x8",
        "note_reshape8x1",
        "shielded_ics20_withdrawal",
        "transfer",
    ];

    fn artifacts_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../tools/gnark/artifacts")
            .canonicalize()
            .expect("tools/gnark/artifacts should resolve")
    }

    #[test]
    fn family_vks_expose_single_statement_hash_public_input() {
        let root = artifacts_root();
        for family in FAMILIES {
            let family_dir = root.join(family);
            let vk_path = family_dir.join("verifying_key.json");
            let vk_json: VerifyingKeyJson = serde_json::from_slice(
                &fs::read(&vk_path).unwrap_or_else(|e| panic!("read {vk_path:?}: {e}")),
            )
            .unwrap_or_else(|e| panic!("decode {vk_path:?}: {e}"));

            // Parse into ark VK so we validate the same shape the runtime consumes.
            let vk = verifying_key_from_json(&vk_json)
                .unwrap_or_else(|e| panic!("parse VK for {family}: {e}"));

            let gamma_abc_len = vk.gamma_abc_g1.len();
            let arity = gamma_abc_len
                .checked_sub(1)
                .unwrap_or_else(|| panic!("family {family} VK has empty gamma_abc_g1"));

            assert_eq!(
                arity, EXPECTED_PUBLIC_INPUT_ARITY,
                "family {family} VK public-input arity {arity} != {EXPECTED_PUBLIC_INPUT_ARITY} \
                 (gamma_abc_g1 len {gamma_abc_len}); statement-parity broken — the circuit's \
                 public-input surface no longer matches the single ClaimedStatementHash wire the \
                 aggregation layer assumes"
            );

            // Cross-file parity: the two committed artifacts must agree on the
            // public surface. gnark's `nb_public_variables` counts the implicit
            // constant `1` wire plus each declared public input, which is exactly
            // `gamma_abc_g1.len()` (Groth16 lays out `[gamma_abc_0, ..inputs..]`).
            // A partial regen (new VK, stale metadata, or vice versa) drifts these.
            let metadata = load_artifact_metadata(&family_dir)
                .unwrap_or_else(|e| panic!("load metadata for {family}: {e}"));
            assert_eq!(
                metadata.nb_public_variables as usize, gamma_abc_len,
                "family {family} circuit_metadata.nb_public_variables {} disagrees with VK \
                 gamma_abc_g1 len {gamma_abc_len}; the VK and metadata artifacts have drifted",
                metadata.nb_public_variables
            );
        }
    }

    /// Every deployed key encoding has a required, unambiguous metadata pin.
    #[test]
    fn committed_family_artifacts_carry_verified_hashes() {
        let root = artifacts_root();
        for family in FAMILIES {
            let family_dir = root.join(family);
            let metadata = load_artifact_metadata(&family_dir)
                .unwrap_or_else(|e| panic!("load metadata for {family}: {e}"));

            // The strict metadata decoder requires all pins. The repository
            // validator checks setup and verification material; materialized
            // proving artifacts are checked before bundled-prover builds.
            validate_repository_hashes(&family_dir, &metadata, family)
                .unwrap_or_else(|e| panic!("hash validation for {family}: {e}"));

            // Also parse the committed VK end-to-end (on-curve + subgroup checks
            // in load_prepared_vk / parse_g1 / parse_g2) so a malformed committed
            // key is caught here rather than at first proof verification.
            load_prepared_vk(&family_dir, family)
                .unwrap_or_else(|e| panic!("prepared VK load for {family}: {e}"));
        }
    }

    #[test]
    fn runtime_rejects_noncanonical_metadata_and_vk_json() {
        let family = "transfer";
        let family_dir = artifacts_root().join(family);

        let metadata_bytes =
            fs::read(family_dir.join("circuit_metadata.json")).expect("read metadata");
        load_artifact_metadata_bytes(&metadata_bytes, "test metadata")
            .expect("canonical metadata loads");
        let metadata_value: serde_json::Value =
            serde_json::from_slice(&metadata_bytes).expect("parse metadata value");
        let compact_metadata =
            serde_json::to_vec(&metadata_value).expect("compact metadata encoding");
        assert!(
            load_artifact_metadata_bytes(&compact_metadata, "test metadata").is_err(),
            "compact metadata alias must fail"
        );

        let vk_bytes = fs::read(family_dir.join("verifying_key.json")).expect("read VK JSON");
        load_prepared_vk_bytes(&vk_bytes, family).expect("canonical VK JSON loads");
        let vk_value: serde_json::Value =
            serde_json::from_slice(&vk_bytes).expect("parse VK value");
        let compact_vk = serde_json::to_vec(&vk_value).expect("compact VK encoding");
        assert!(
            load_prepared_vk_bytes(&compact_vk, family).is_err(),
            "compact VK JSON alias must fail"
        );

        let mut vk_json: VerifyingKeyJson =
            serde_json::from_slice(&vk_bytes).expect("decode canonical VK JSON");
        let canonical_x = vk_json
            .alpha_g1
            .x
            .parse::<ProofG1Base>()
            .expect("decode canonical G1 x");
        let mut alias = canonical_x.into_bigint();
        assert!(
            !alias.add_with_carry(&ProofG1Base::MODULUS),
            "x + modulus fits the base-field integer width"
        );
        vk_json.alpha_g1.x = alias.to_string();
        let mut modular_alias = serde_json::to_vec_pretty(&vk_json).expect("encode modular alias");
        modular_alias.push(b'\n');
        assert!(
            load_prepared_vk_bytes(&modular_alias, family).is_err(),
            "x + modulus must not decode to the same verifying key"
        );

        vk_json.alpha_g1.x = "-1".to_owned();
        let mut negative_alias =
            serde_json::to_vec_pretty(&vk_json).expect("encode negative coordinate");
        negative_alias.push(b'\n');
        assert!(
            load_prepared_vk_bytes(&negative_alias, family).is_err(),
            "negative field coordinates must fail"
        );
    }
}

fn parse_g2(point: &G2PointJson) -> Result<ProofG2> {
    let x_a0 = parse_canonical_field(&point.x.a0, "G2 x.a0")?;
    let x_a1 = parse_canonical_field(&point.x.a1, "G2 x.a1")?;
    let y_a0 = parse_canonical_field(&point.y.a0, "G2 y.a0")?;
    let y_a1 = parse_canonical_field(&point.y.a1, "G2 y.a1")?;
    let point = ProofG2::new_unchecked(ProofG2Base::new(x_a0, x_a1), ProofG2Base::new(y_a0, y_a1));
    if !point.is_on_curve() {
        bail!("G2 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G2 point is not in the correct subgroup");
    }
    Ok(point)
}

fn parse_canonical_field<F: PrimeField>(value: &str, label: &str) -> Result<F> {
    let parsed = F::from_str(value).map_err(|_| anyhow!("invalid {label}"))?;
    if parsed.to_string() != value {
        bail!("{label} is not a canonical field decimal");
    }
    Ok(parsed)
}
