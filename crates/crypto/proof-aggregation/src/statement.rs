use std::fmt;

use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::challenge::ChallengeContext;
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proto::core::transaction::v1 as pb;
use sha2::{Digest as _, Sha256};

use crate::{padding::PADDING_RULE_DOMAIN, ProofFamilyId, DEV_SRS_BACKEND_ID, DEV_SRS_CURVE_ID};

pub const AGGREGATE_PROTOCOL_VERSION: u32 = 1;

const STATEMENT_DIGEST_DOMAIN: &[u8] = b"penumbra.snarkpack.statement_digest.v1\0";
const VK_DIGEST_DOMAIN: &[u8] = b"penumbra.snarkpack.vk_digest.v1\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AggregateStatementError {
    BadVersion {
        version: u32,
    },
    BadCount {
        real_count: u32,
        padded_count: u32,
    },
    BadPadding {
        padded_count: u32,
        row_count: usize,
    },
    RowArityMismatch {
        index: usize,
        expected: usize,
        got: usize,
    },
    OversizeBytes {
        field: &'static str,
        max: usize,
        got: usize,
    },
    EncodingFailed(String),
}

impl fmt::Display for AggregateStatementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadVersion { version } => {
                write!(f, "unsupported aggregate statement version {version}")
            }
            Self::BadCount {
                real_count,
                padded_count,
            } => write!(
                f,
                "invalid aggregate statement counts: real={real_count}, padded={padded_count}"
            ),
            Self::BadPadding {
                padded_count,
                row_count,
            } => write!(
                f,
                "invalid aggregate statement padding: padded={padded_count}, rows={row_count}"
            ),
            Self::RowArityMismatch {
                index,
                expected,
                got,
            } => write!(
                f,
                "aggregate statement row {index}: expected {expected} public inputs, got {got}"
            ),
            Self::OversizeBytes { field, max, got } => {
                write!(
                    f,
                    "aggregate statement field {field} is too large: got {got} bytes, max {max}"
                )
            }
            Self::EncodingFailed(err) => write!(f, "aggregate statement encoding failed: {err}"),
        }
    }
}

impl std::error::Error for AggregateStatementError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatementEncodingInput {
    pub version: u32,
    pub curve_id: Vec<u8>,
    pub backend_id: Vec<u8>,
    pub proof_family_id: u32,
    pub consolidate_family_id: u32,
    pub split_family_id: u32,
    pub shielded_ics20_withdrawal_family_id: u32,
    pub srs_id: [u8; 32],
    pub vk_digest: [u8; 32],
    pub real_count: u32,
    pub padded_count: u32,
    pub public_input_arity: u32,
    pub padded_public_inputs: StatementPaddedRows,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatementFieldBytes {
    bytes: Vec<u8>,
}

impl StatementFieldBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatementPublicInputRow {
    fields: Vec<StatementFieldBytes>,
}

impl StatementPublicInputRow {
    pub fn new(fields: Vec<StatementFieldBytes>) -> Self {
        Self { fields }
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &StatementFieldBytes> {
        self.fields.iter()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatementPaddedRows {
    rows: Vec<StatementPublicInputRow>,
}

impl StatementPaddedRows {
    pub fn new(rows: Vec<StatementPublicInputRow>) -> Self {
        Self { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &StatementPublicInputRow> {
        self.rows.iter()
    }
}

impl From<Vec<Vec<Vec<u8>>>> for StatementPaddedRows {
    fn from(rows: Vec<Vec<Vec<u8>>>) -> Self {
        Self::new(
            rows.into_iter()
                .map(|row| {
                    StatementPublicInputRow::new(
                        row.into_iter().map(StatementFieldBytes::new).collect(),
                    )
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateStatement {
    family_id: ProofFamilyId,
    srs_id: [u8; 32],
    vk_digest: [u8; 32],
    real_count: u32,
    padded_count: u32,
    padded_public_inputs: Vec<Vec<Fq>>,
    canonical_bytes: Vec<u8>,
    statement_digest: [u8; 32],
    challenge_context: ChallengeContext,
}

impl AggregateStatement {
    pub fn new(
        version: u32,
        family_id: ProofFamilyId,
        srs_id: [u8; 32],
        pvk: &PreparedVerifyingKey<Bls12_377>,
        real_count: u32,
        padded_public_inputs: &[Vec<Fq>],
    ) -> Result<Self, AggregateStatementError> {
        if version != AGGREGATE_PROTOCOL_VERSION {
            return Err(AggregateStatementError::BadVersion { version });
        }

        let padded_count = u32::try_from(padded_public_inputs.len()).map_err(|_| {
            AggregateStatementError::OversizeBytes {
                field: "padded_public_inputs.len",
                max: u32::MAX as usize,
                got: padded_public_inputs.len(),
            }
        })?;
        validate_counts(real_count, padded_count, padded_public_inputs)?;
        validate_repeat_final_padding(real_count, padded_public_inputs)?;

        let expected_arity = pvk.vk.gamma_abc_g1.len().checked_sub(1).ok_or(
            AggregateStatementError::RowArityMismatch {
                index: 0,
                expected: 1,
                got: 0,
            },
        )?;
        validate_row_arity(padded_public_inputs, expected_arity)?;

        let family = family_encoding(family_id);
        let vk_digest = aggregate_verification_key_digest(pvk)?;
        let input = StatementEncodingInput {
            version,
            curve_id: DEV_SRS_CURVE_ID.as_bytes().to_vec(),
            backend_id: DEV_SRS_BACKEND_ID.as_bytes().to_vec(),
            proof_family_id: family.proof_family_id,
            consolidate_family_id: family.consolidate_family_id,
            split_family_id: family.split_family_id,
            shielded_ics20_withdrawal_family_id: family.shielded_ics20_withdrawal_family_id,
            srs_id,
            vk_digest,
            real_count,
            padded_count,
            public_input_arity: u32::try_from(expected_arity).map_err(|_| {
                AggregateStatementError::OversizeBytes {
                    field: "public_input_arity",
                    max: u32::MAX as usize,
                    got: expected_arity,
                }
            })?,
            padded_public_inputs: field_rows_to_bytes(padded_public_inputs)?,
        };
        let canonical_bytes = encode_statement(&input)?;
        let statement_digest = statement_digest_from_canonical(&canonical_bytes);
        let challenge_context = ChallengeContext::from_statement_digest(statement_digest);

        Ok(Self {
            family_id,
            srs_id,
            vk_digest,
            real_count,
            padded_count,
            padded_public_inputs: padded_public_inputs.to_vec(),
            canonical_bytes,
            statement_digest,
            challenge_context,
        })
    }

    pub fn family_id(&self) -> ProofFamilyId {
        self.family_id
    }

    pub fn srs_id(&self) -> [u8; 32] {
        self.srs_id
    }

    pub fn vk_digest(&self) -> [u8; 32] {
        self.vk_digest
    }

    pub fn real_count(&self) -> u32 {
        self.real_count
    }

    pub fn padded_count(&self) -> u32 {
        self.padded_count
    }

    pub fn padded_public_inputs(&self) -> &[Vec<Fq>] {
        &self.padded_public_inputs
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub fn statement_digest(&self) -> [u8; 32] {
        self.statement_digest
    }

    pub fn challenge_context(&self) -> &ChallengeContext {
        &self.challenge_context
    }
}

pub fn aggregate_verification_key_digest(
    pvk: &PreparedVerifyingKey<Bls12_377>,
) -> Result<[u8; 32], AggregateStatementError> {
    let mut vk_bytes = Vec::new();
    pvk.vk
        .serialize_compressed(&mut vk_bytes)
        .map_err(|err| AggregateStatementError::EncodingFailed(err.to_string()))?;

    let mut hasher = Sha256::new();
    hasher.update(VK_DIGEST_DOMAIN);
    append_field(&mut hasher, &vk_bytes)?;
    Ok(hasher.finalize().into())
}

pub fn encode_statement(
    input: &StatementEncodingInput,
) -> Result<Vec<u8>, AggregateStatementError> {
    let mut bytes = Vec::new();
    // Every byte field is length-prefixed, including fixed-width digests.
    append_u32_field(&mut bytes, input.version);
    append_bytes_field(&mut bytes, &input.curve_id)?;
    append_bytes_field(&mut bytes, &input.backend_id)?;
    append_bytes_field(&mut bytes, PADDING_RULE_DOMAIN)?;
    append_u32_field(&mut bytes, input.proof_family_id);
    append_u32_field(&mut bytes, input.consolidate_family_id);
    append_u32_field(&mut bytes, input.split_family_id);
    append_u32_field(&mut bytes, input.shielded_ics20_withdrawal_family_id);
    append_bytes_field(&mut bytes, &input.srs_id)?;
    append_bytes_field(&mut bytes, &input.vk_digest)?;
    append_u32_field(&mut bytes, input.real_count);
    append_u32_field(&mut bytes, input.padded_count);
    append_u32_field(&mut bytes, input.public_input_arity);
    append_len(&mut bytes, input.padded_public_inputs.len(), "row_count")?;
    for row in input.padded_public_inputs.iter() {
        append_len(&mut bytes, row.len(), "row_arity")?;
        for field in row.iter() {
            append_bytes_field(&mut bytes, field.as_bytes())?;
        }
    }
    Ok(bytes)
}

pub fn statement_digest(
    input: &StatementEncodingInput,
) -> Result<[u8; 32], AggregateStatementError> {
    Ok(statement_digest_from_canonical(&encode_statement(input)?))
}

pub fn challenge_context(
    input: &StatementEncodingInput,
) -> Result<ChallengeContext, AggregateStatementError> {
    Ok(ChallengeContext::from_statement_digest(statement_digest(
        input,
    )?))
}

pub fn validate_counts<T>(
    real_count: u32,
    padded_count: u32,
    rows: &[T],
) -> Result<(), AggregateStatementError> {
    if real_count == 0 || real_count > padded_count {
        return Err(AggregateStatementError::BadCount {
            real_count,
            padded_count,
        });
    }
    if padded_count == 0
        || !padded_count.is_power_of_two()
        || usize::try_from(padded_count).ok() != Some(rows.len())
    {
        return Err(AggregateStatementError::BadPadding {
            padded_count,
            row_count: rows.len(),
        });
    }
    Ok(())
}

pub fn validate_row_arity<T>(
    rows: &[Vec<T>],
    expected: usize,
) -> Result<(), AggregateStatementError> {
    for (index, row) in rows.iter().enumerate() {
        if row.len() != expected {
            return Err(AggregateStatementError::RowArityMismatch {
                index,
                expected,
                got: row.len(),
            });
        }
    }
    Ok(())
}

pub fn validate_repeat_final_padding<T: Eq>(
    real_count: u32,
    rows: &[Vec<T>],
) -> Result<(), AggregateStatementError> {
    let padded_count =
        u32::try_from(rows.len()).map_err(|_| AggregateStatementError::OversizeBytes {
            field: "padded_public_inputs.len",
            max: u32::MAX as usize,
            got: rows.len(),
        })?;
    let real_count_usize =
        usize::try_from(real_count).map_err(|_| AggregateStatementError::BadCount {
            real_count,
            padded_count,
        })?;
    if real_count_usize == 0 || real_count_usize > rows.len() {
        return Err(AggregateStatementError::BadCount {
            real_count,
            padded_count,
        });
    }
    let final_real = &rows[real_count_usize - 1];
    for row in &rows[real_count_usize..] {
        if row != final_real {
            return Err(AggregateStatementError::BadPadding {
                padded_count,
                row_count: rows.len(),
            });
        }
    }
    Ok(())
}

fn statement_digest_from_canonical(canonical_bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(STATEMENT_DIGEST_DOMAIN);
    hasher.update(canonical_bytes);
    hasher.finalize().into()
}

fn field_rows_to_bytes(rows: &[Vec<Fq>]) -> Result<StatementPaddedRows, AggregateStatementError> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|field| {
                    let mut bytes = Vec::new();
                    field
                        .serialize_compressed(&mut bytes)
                        .map_err(|err| AggregateStatementError::EncodingFailed(err.to_string()))?;
                    Ok(StatementFieldBytes::new(bytes))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(StatementPublicInputRow::new)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(StatementPaddedRows::new)
}

#[derive(Clone, Copy)]
struct FamilyEncoding {
    proof_family_id: u32,
    consolidate_family_id: u32,
    split_family_id: u32,
    shielded_ics20_withdrawal_family_id: u32,
}

fn family_encoding(family_id: ProofFamilyId) -> FamilyEncoding {
    FamilyEncoding {
        proof_family_id: pb::ProofFamilyId::from(family_id) as u32,
        consolidate_family_id: family_id.consolidate_family_id(),
        split_family_id: family_id.split_family_id(),
        shielded_ics20_withdrawal_family_id: family_id.shielded_ics20_withdrawal_family_id(),
    }
}

fn append_u32_field(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn append_bytes_field(bytes: &mut Vec<u8>, field: &[u8]) -> Result<(), AggregateStatementError> {
    append_len(bytes, field.len(), "bytes_field")?;
    bytes.extend_from_slice(field);
    Ok(())
}

fn append_field(hasher: &mut Sha256, field: &[u8]) -> Result<(), AggregateStatementError> {
    let len = u32::try_from(field.len()).map_err(|_| AggregateStatementError::OversizeBytes {
        field: "hash_field",
        max: u32::MAX as usize,
        got: field.len(),
    })?;
    hasher.update(len.to_le_bytes());
    hasher.update(field);
    Ok(())
}

fn append_len(
    bytes: &mut Vec<u8>,
    len: usize,
    field: &'static str,
) -> Result<(), AggregateStatementError> {
    let len = u32::try_from(len).map_err(|_| AggregateStatementError::OversizeBytes {
        field,
        max: u32::MAX as usize,
        got: len,
    })?;
    bytes.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
    use ark_ip_proofs::challenge::challenge_preimage;
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use decaf377::{Bls12_377, Fq};
    use proptest::prelude::*;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    use crate::{srs_id, DevSrs, ProofFamilyId};

    use super::*;

    #[derive(Clone)]
    struct SquareCircuit {
        x: Option<Fq>,
    }

    impl ConstraintSynthesizer<Fq> for SquareCircuit {
        fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> Result<(), SynthesisError> {
            let x = FpVar::new_witness(cs.clone(), || {
                self.x.ok_or(SynthesisError::AssignmentMissing)
            })?;
            let x_sq = &x * &x;
            let public = FpVar::new_input(cs, || {
                let x = self.x.ok_or(SynthesisError::AssignmentMissing)?;
                Ok(x * x)
            })?;

            x_sq.enforce_equal(&public)?;
            Ok(())
        }
    }

    fn sample_pvk() -> PreparedVerifyingKey<Bls12_377> {
        let mut rng = ChaCha20Rng::seed_from_u64(11);
        let pk =
            Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                SquareCircuit {
                    x: Some(Fq::from(1u64)),
                },
                &mut rng,
            )
            .expect("setup should succeed");
        pk.vk.into()
    }

    #[test]
    fn statement_accepts_valid_padded_inputs() {
        let pvk = sample_pvk();
        let srs = DevSrs::default();
        let rows = vec![vec![Fq::from(1u64)], vec![Fq::from(4u64)]];

        let statement = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            srs_id(&srs),
            &pvk,
            2,
            &rows,
        )
        .expect("statement should build");

        assert_eq!(statement.real_count(), 2);
        assert_eq!(statement.padded_count(), 2);
        assert_eq!(statement.family_id(), ProofFamilyId::Transfer);
        assert_ne!(
            &statement.statement_digest(),
            statement.challenge_context().as_bytes()
        );
        assert_eq!(statement.padded_public_inputs(), rows.as_slice());
    }

    #[test]
    fn statement_rejects_bad_counts() {
        let pvk = sample_pvk();
        let rows = vec![vec![Fq::from(1u64)]];

        let err = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [0u8; 32],
            &pvk,
            0,
            &rows,
        )
        .expect_err("zero real count should reject");

        assert!(matches!(err, AggregateStatementError::BadCount { .. }));
    }

    #[test]
    fn statement_validation_helpers_cover_count_cases() {
        let one_row = vec![vec![Fq::from(1u64)]];
        let two_rows = vec![vec![Fq::from(1u64)], vec![Fq::from(2u64)]];

        assert!(matches!(
            validate_counts(0, 1, &one_row),
            Err(AggregateStatementError::BadCount { .. })
        ));
        assert!(matches!(
            validate_counts(2, 1, &one_row),
            Err(AggregateStatementError::BadCount { .. })
        ));
        assert!(matches!(
            validate_counts(1, 0, &one_row),
            Err(AggregateStatementError::BadCount { .. })
        ));
        assert!(matches!(
            validate_counts(1, 3, &one_row),
            Err(AggregateStatementError::BadPadding { .. })
        ));
        assert!(matches!(
            validate_counts(1, 2, &one_row),
            Err(AggregateStatementError::BadPadding { .. })
        ));
        assert!(validate_counts(1, 2, &two_rows).is_ok());
        assert!(validate_row_arity(&two_rows, 1).is_ok());
        assert!(matches!(
            validate_row_arity(&two_rows, 2),
            Err(AggregateStatementError::RowArityMismatch { .. })
        ));
    }

    #[test]
    fn statement_rejects_noncanonical_repeat_final_padding() {
        let pvk = sample_pvk();
        let rows = vec![
            vec![Fq::from(1u64)],
            vec![Fq::from(2u64)],
            vec![Fq::from(3u64)],
            vec![Fq::from(2u64)],
        ];

        let err = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [0u8; 32],
            &pvk,
            2,
            &rows,
        )
        .expect_err("padding suffix must repeat the final real row");

        assert!(matches!(err, AggregateStatementError::BadPadding { .. }));
    }

    #[test]
    fn statement_accepts_canonical_repeat_final_padding() {
        let pvk = sample_pvk();
        let rows = vec![
            vec![Fq::from(1u64)],
            vec![Fq::from(2u64)],
            vec![Fq::from(2u64)],
            vec![Fq::from(2u64)],
        ];

        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [0u8; 32],
            &pvk,
            2,
            &rows,
        )
        .expect("canonical repeat-final padding should build");
    }

    #[test]
    fn statement_rejects_bad_padding() {
        let pvk = sample_pvk();
        let rows = vec![
            vec![Fq::from(1u64)],
            vec![Fq::from(2u64)],
            vec![Fq::from(3u64)],
        ];

        let err = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [0u8; 32],
            &pvk,
            3,
            &rows,
        )
        .expect_err("non power of two padding should reject");

        assert!(matches!(err, AggregateStatementError::BadPadding { .. }));
    }

    #[test]
    fn statement_rejects_row_arity_mismatch() {
        let pvk = sample_pvk();
        let rows = vec![vec![Fq::from(1u64), Fq::from(2u64)]];

        let err = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [0u8; 32],
            &pvk,
            1,
            &rows,
        )
        .expect_err("wrong public input arity should reject");

        assert!(matches!(
            err,
            AggregateStatementError::RowArityMismatch {
                expected: 1,
                got: 2,
                ..
            }
        ));
    }

    #[test]
    fn statement_digest_binds_inputs() {
        let pvk = sample_pvk();
        let rows = vec![vec![Fq::from(1u64)], vec![Fq::from(1u64)]];
        let mut changed_rows = rows.clone();
        changed_rows[1][0] += Fq::from(1u64);

        let original = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [1u8; 32],
            &pvk,
            2,
            &rows,
        )
        .expect("statement should build");
        let changed = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [1u8; 32],
            &pvk,
            2,
            &changed_rows,
        )
        .expect("changed statement should build");

        assert_ne!(original.canonical_bytes(), changed.canonical_bytes());
        assert_ne!(original.statement_digest(), changed.statement_digest());
    }

    #[test]
    fn statement_digest_binds_real_count_even_with_repeated_rows() {
        let pvk = sample_pvk();
        let rows = vec![
            vec![Fq::from(7u64)],
            vec![Fq::from(7u64)],
            vec![Fq::from(7u64)],
            vec![Fq::from(7u64)],
        ];
        let one_real = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [1u8; 32],
            &pvk,
            1,
            &rows,
        )
        .expect("statement should build");
        let three_real = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            [1u8; 32],
            &pvk,
            3,
            &rows,
        )
        .expect("statement should build");

        assert_ne!(one_real.canonical_bytes(), three_real.canonical_bytes());
        assert_ne!(one_real.statement_digest(), three_real.statement_digest());
    }

    #[test]
    fn statement_encoding_binds_all_top_level_fields() {
        let base = StatementEncodingInput {
            version: AGGREGATE_PROTOCOL_VERSION,
            curve_id: b"curve".to_vec(),
            backend_id: b"backend".to_vec(),
            proof_family_id: 1,
            consolidate_family_id: 2,
            split_family_id: 3,
            shielded_ics20_withdrawal_family_id: 4,
            srs_id: [0x11; 32],
            vk_digest: [0x22; 32],
            real_count: 1,
            padded_count: 1,
            public_input_arity: 1,
            padded_public_inputs: vec![vec![vec![0xaa]]].into(),
        };
        let base_encoded = encode_statement(&base).expect("base encodes");

        let mutations = [
            StatementEncodingInput {
                version: 2,
                ..base.clone()
            },
            StatementEncodingInput {
                curve_id: b"other-curve".to_vec(),
                ..base.clone()
            },
            StatementEncodingInput {
                backend_id: b"other-backend".to_vec(),
                ..base.clone()
            },
            StatementEncodingInput {
                proof_family_id: 9,
                ..base.clone()
            },
            StatementEncodingInput {
                consolidate_family_id: 9,
                ..base.clone()
            },
            StatementEncodingInput {
                split_family_id: 9,
                ..base.clone()
            },
            StatementEncodingInput {
                shielded_ics20_withdrawal_family_id: 9,
                ..base.clone()
            },
            StatementEncodingInput {
                srs_id: [0x33; 32],
                ..base.clone()
            },
            StatementEncodingInput {
                vk_digest: [0x44; 32],
                ..base.clone()
            },
            StatementEncodingInput {
                real_count: 2,
                ..base.clone()
            },
            StatementEncodingInput {
                padded_count: 2,
                ..base.clone()
            },
            StatementEncodingInput {
                public_input_arity: 2,
                ..base.clone()
            },
            StatementEncodingInput {
                padded_public_inputs: vec![vec![vec![0xbb]]].into(),
                ..base.clone()
            },
        ];

        for mutated in mutations {
            assert_ne!(
                base_encoded,
                encode_statement(&mutated).expect("mutation encodes")
            );
        }
    }

    #[test]
    fn statement_canonical_encoding_layout() {
        let input = StatementEncodingInput {
            version: AGGREGATE_PROTOCOL_VERSION,
            curve_id: b"curve-x".to_vec(),
            backend_id: b"backend-y".to_vec(),
            proof_family_id: 1,
            consolidate_family_id: 2,
            split_family_id: 3,
            shielded_ics20_withdrawal_family_id: 4,
            srs_id: [0x11; 32],
            vk_digest: [0x22; 32],
            real_count: 1,
            padded_count: 2,
            public_input_arity: 1,
            padded_public_inputs: vec![vec![vec![0xaa, 0xbb]], vec![vec![0xcc]]].into(),
        };

        let encoded = encode_statement(&input).expect("encoding succeeds");
        let mut expected = Vec::new();
        expected.extend_from_slice(&4u32.to_le_bytes());
        expected.extend_from_slice(&AGGREGATE_PROTOCOL_VERSION.to_le_bytes());
        expected.extend_from_slice(&7u32.to_le_bytes());
        expected.extend_from_slice(b"curve-x");
        expected.extend_from_slice(&9u32.to_le_bytes());
        expected.extend_from_slice(b"backend-y");
        expected.extend_from_slice(&(PADDING_RULE_DOMAIN.len() as u32).to_le_bytes());
        expected.extend_from_slice(PADDING_RULE_DOMAIN);
        for value in [1u32, 2, 3, 4] {
            expected.extend_from_slice(&4u32.to_le_bytes());
            expected.extend_from_slice(&value.to_le_bytes());
        }
        expected.extend_from_slice(&32u32.to_le_bytes());
        expected.extend_from_slice(&[0x11; 32]);
        expected.extend_from_slice(&32u32.to_le_bytes());
        expected.extend_from_slice(&[0x22; 32]);
        for value in [1u32, 2, 1] {
            expected.extend_from_slice(&4u32.to_le_bytes());
            expected.extend_from_slice(&value.to_le_bytes());
        }
        expected.extend_from_slice(&2u32.to_le_bytes());
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&2u32.to_le_bytes());
        expected.extend_from_slice(&[0xaa, 0xbb]);
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&[0xcc]);

        assert_eq!(encoded, expected);
    }

    #[test]
    fn statement_encoding_length_prefixes_top_level_byte_fields() {
        let left = StatementEncodingInput {
            version: AGGREGATE_PROTOCOL_VERSION,
            curve_id: b"a".to_vec(),
            backend_id: b"bc".to_vec(),
            proof_family_id: 1,
            consolidate_family_id: 2,
            split_family_id: 3,
            shielded_ics20_withdrawal_family_id: 4,
            srs_id: [0x11; 32],
            vk_digest: [0x22; 32],
            real_count: 1,
            padded_count: 1,
            public_input_arity: 1,
            padded_public_inputs: vec![vec![vec![0xaa]]].into(),
        };
        let right = StatementEncodingInput {
            curve_id: b"ab".to_vec(),
            backend_id: b"c".to_vec(),
            ..left.clone()
        };

        assert_ne!(
            encode_statement(&left).expect("left encodes"),
            encode_statement(&right).expect("right encodes")
        );
    }

    #[test]
    fn statement_encoding_length_prefixes_public_input_fields() {
        let left = StatementEncodingInput {
            version: AGGREGATE_PROTOCOL_VERSION,
            curve_id: b"curve".to_vec(),
            backend_id: b"backend".to_vec(),
            proof_family_id: 1,
            consolidate_family_id: 2,
            split_family_id: 3,
            shielded_ics20_withdrawal_family_id: 4,
            srs_id: [0x11; 32],
            vk_digest: [0x22; 32],
            real_count: 1,
            padded_count: 1,
            public_input_arity: 2,
            padded_public_inputs: vec![vec![vec![1], vec![2, 3]]].into(),
        };
        let right = StatementEncodingInput {
            padded_public_inputs: vec![vec![vec![1, 2], vec![3]]].into(),
            ..left.clone()
        };

        assert_ne!(
            encode_statement(&left).expect("left encodes"),
            encode_statement(&right).expect("right encodes")
        );
    }

    #[test]
    fn statement_encoding_length_prefixes_public_input_rows() {
        let left = StatementEncodingInput {
            version: AGGREGATE_PROTOCOL_VERSION,
            curve_id: b"curve".to_vec(),
            backend_id: b"backend".to_vec(),
            proof_family_id: 1,
            consolidate_family_id: 2,
            split_family_id: 3,
            shielded_ics20_withdrawal_family_id: 4,
            srs_id: [0x11; 32],
            vk_digest: [0x22; 32],
            real_count: 1,
            padded_count: 2,
            public_input_arity: 1,
            padded_public_inputs: vec![vec![vec![1]], vec![vec![2]]].into(),
        };
        let right = StatementEncodingInput {
            padded_count: 1,
            public_input_arity: 2,
            padded_public_inputs: vec![vec![vec![1], vec![2]]].into(),
            ..left.clone()
        };

        assert_ne!(
            encode_statement(&left).expect("left encodes"),
            encode_statement(&right).expect("right encodes")
        );
    }

    #[test]
    fn challenge_preimage_layout_golden() {
        let context = ChallengeContext::from_statement_digest([9u8; 32]);
        let stage = b"stage.alpha";
        let nonce = 42u64;
        let messages = [0xaa, 0xbb, 0xcc];

        let preimage = challenge_preimage(&context, stage, nonce, &messages);
        let mut expected = Vec::new();
        expected.extend_from_slice(b"penumbra.snarkpack.challenge.v1\0");
        expected.extend_from_slice(&(stage.len() as u32).to_le_bytes());
        expected.extend_from_slice(stage);
        expected.extend_from_slice(context.as_bytes());
        expected.extend_from_slice(&nonce.to_le_bytes());
        expected.extend_from_slice(&messages);

        assert_eq!(preimage, expected);
    }

    #[test]
    fn challenge_preimage_changes_on_stage_context_nonce_or_messages() {
        let context = ChallengeContext::from_statement_digest([9u8; 32]);
        let other_context = ChallengeContext::from_statement_digest([10u8; 32]);
        let base = challenge_preimage(&context, b"stage.alpha", 42, &[0xaa, 0xbb]);

        assert_ne!(
            base,
            challenge_preimage(&context, b"stage.beta", 42, &[0xaa, 0xbb])
        );
        assert_ne!(
            base,
            challenge_preimage(&other_context, b"stage.alpha", 42, &[0xaa, 0xbb])
        );
        assert_ne!(
            base,
            challenge_preimage(&context, b"stage.alpha", 43, &[0xaa, 0xbb])
        );
        assert_ne!(
            base,
            challenge_preimage(&context, b"stage.alpha", 42, &[0xaa, 0xbc])
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn statement_fuzz_constructor_helpers_and_encoder_do_not_panic(
            real_count in 0u32..=10,
            expected_arity in 0usize..=4,
            rows in prop::collection::vec(
                prop::collection::vec(0u64..=16, 0usize..=4),
                0usize..=8,
            ),
        ) {
            let fq_rows = rows
                .iter()
                .map(|row| row.iter().copied().map(Fq::from).collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let padded_count = u32::try_from(fq_rows.len()).expect("bounded row count");
            let _ = validate_counts(real_count, padded_count, &fq_rows);
            let _ = validate_row_arity(&fq_rows, expected_arity);

            let primitive_rows = rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|value| value.to_le_bytes().to_vec())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let input = StatementEncodingInput {
                version: AGGREGATE_PROTOCOL_VERSION,
                curve_id: DEV_SRS_CURVE_ID.as_bytes().to_vec(),
                backend_id: DEV_SRS_BACKEND_ID.as_bytes().to_vec(),
                proof_family_id: 1,
                consolidate_family_id: 0,
                split_family_id: 0,
                shielded_ics20_withdrawal_family_id: 0,
                srs_id: [1u8; 32],
                vk_digest: [2u8; 32],
                real_count,
                padded_count,
                public_input_arity: expected_arity as u32,
                padded_public_inputs: primitive_rows.into(),
            };
            let encoded = encode_statement(&input).expect("bounded statement encoding succeeds");
            prop_assert!(!encoded.is_empty());
        }
    }
}
