use std::fmt;

use ark_groth16::PreparedVerifyingKey;
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proto::core::transaction::v1 as pb;
use sha2::{Digest as _, Sha256};

use crate::ProofFamilyId;

pub const AGGREGATE_STATEMENT_VERSION: u32 = 1;

const STATEMENT_DIGEST_DOMAIN: &[u8] = b"penumbra.snarkpack.statement_digest.v1\0";
const CHALLENGE_CONTEXT_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge_context.v1\0";
const VK_DIGEST_DOMAIN: &[u8] = b"penumbra.snarkpack.vk_digest.v1\0";
const PADDING_RULE_DOMAIN: &[u8] = b"penumbra.snarkpack.padding.repeat-final-row.v1\0";

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
        len: usize,
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
            Self::OversizeBytes { field, len } => {
                write!(
                    f,
                    "aggregate statement field {field} is too large: {len} bytes"
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
    pub proof_family_id: u32,
    pub consolidate_family_id: u32,
    pub split_family_id: u32,
    pub shielded_ics20_withdrawal_family_id: u32,
    pub srs_id: [u8; 32],
    pub vk_digest: [u8; 32],
    pub real_count: u32,
    pub padded_count: u32,
    pub public_input_arity: u32,
    pub padded_public_inputs: Vec<Vec<Vec<u8>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateStatement {
    input: StatementEncodingInput,
    padded_public_inputs: Vec<Vec<Fq>>,
    canonical_bytes: Vec<u8>,
    statement_digest: [u8; 32],
    challenge_context: [u8; 32],
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
        if version != AGGREGATE_STATEMENT_VERSION {
            return Err(AggregateStatementError::BadVersion { version });
        }

        let padded_count = u32::try_from(padded_public_inputs.len()).map_err(|_| {
            AggregateStatementError::OversizeBytes {
                field: "padded_public_inputs.len",
                len: padded_public_inputs.len(),
            }
        })?;
        validate_counts(real_count, padded_count, padded_public_inputs)?;

        let expected_arity = pvk.vk.gamma_abc_g1.len().checked_sub(1).ok_or(
            AggregateStatementError::RowArityMismatch {
                index: 0,
                expected: 1,
                got: 0,
            },
        )?;
        validate_row_arity(padded_public_inputs, expected_arity)?;

        let family = family_encoding(family_id);
        let input = StatementEncodingInput {
            version,
            proof_family_id: family.proof_family_id,
            consolidate_family_id: family.consolidate_family_id,
            split_family_id: family.split_family_id,
            shielded_ics20_withdrawal_family_id: family.shielded_ics20_withdrawal_family_id,
            srs_id,
            vk_digest: aggregate_verification_key_digest(pvk)?,
            real_count,
            padded_count,
            public_input_arity: u32::try_from(expected_arity).map_err(|_| {
                AggregateStatementError::OversizeBytes {
                    field: "public_input_arity",
                    len: expected_arity,
                }
            })?,
            padded_public_inputs: field_rows_to_bytes(padded_public_inputs)?,
        };
        let canonical_bytes = encode_statement(&input)?;
        let statement_digest = statement_digest_from_canonical(&canonical_bytes);
        let challenge_context = challenge_context_from_statement_digest(statement_digest);

        Ok(Self {
            input,
            padded_public_inputs: padded_public_inputs.to_vec(),
            canonical_bytes,
            statement_digest,
            challenge_context,
        })
    }

    pub fn family_id(&self) -> ProofFamilyId {
        let input = &self.input;
        ProofFamilyId::try_from_proto_fields(
            input.proof_family_id as i32,
            input.consolidate_family_id,
            input.split_family_id,
            input.shielded_ics20_withdrawal_family_id,
        )
        .expect("aggregate statement stores a valid proof family id")
    }

    pub fn real_count(&self) -> u32 {
        self.input.real_count
    }

    pub fn padded_count(&self) -> u32 {
        self.input.padded_count
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

    pub fn challenge_context(&self) -> [u8; 32] {
        self.challenge_context
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
    append_bytes_field(&mut bytes, PADDING_RULE_DOMAIN)?;
    append_u32_field(&mut bytes, input.version);
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
    for row in &input.padded_public_inputs {
        append_len(&mut bytes, row.len(), "row_arity")?;
        for field in row {
            append_bytes_field(&mut bytes, field)?;
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
) -> Result<[u8; 32], AggregateStatementError> {
    Ok(challenge_context_from_statement_digest(statement_digest(
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

pub fn validate_row_arity(
    rows: &[Vec<Fq>],
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

fn statement_digest_from_canonical(canonical_bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(STATEMENT_DIGEST_DOMAIN);
    hasher.update(canonical_bytes);
    hasher.finalize().into()
}

fn challenge_context_from_statement_digest(statement_digest: [u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CHALLENGE_CONTEXT_DOMAIN);
    hasher.update(statement_digest);
    hasher.finalize().into()
}

fn field_rows_to_bytes(rows: &[Vec<Fq>]) -> Result<Vec<Vec<Vec<u8>>>, AggregateStatementError> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|field| {
                    let mut bytes = Vec::new();
                    field
                        .serialize_compressed(&mut bytes)
                        .map_err(|err| AggregateStatementError::EncodingFailed(err.to_string()))?;
                    Ok(bytes)
                })
                .collect()
        })
        .collect()
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
        len: field.len(),
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
    let len =
        u32::try_from(len).map_err(|_| AggregateStatementError::OversizeBytes { field, len })?;
    bytes.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
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
            AGGREGATE_STATEMENT_VERSION,
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
        assert_ne!(statement.statement_digest(), statement.challenge_context());
        assert_eq!(statement.padded_public_inputs(), rows.as_slice());
    }

    #[test]
    fn statement_rejects_bad_counts() {
        let pvk = sample_pvk();
        let rows = vec![vec![Fq::from(1u64)]];

        let err = AggregateStatement::new(
            AGGREGATE_STATEMENT_VERSION,
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
    fn statement_rejects_bad_padding() {
        let pvk = sample_pvk();
        let rows = vec![
            vec![Fq::from(1u64)],
            vec![Fq::from(2u64)],
            vec![Fq::from(3u64)],
        ];

        let err = AggregateStatement::new(
            AGGREGATE_STATEMENT_VERSION,
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
            AGGREGATE_STATEMENT_VERSION,
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
            AGGREGATE_STATEMENT_VERSION,
            ProofFamilyId::Transfer,
            [1u8; 32],
            &pvk,
            1,
            &rows,
        )
        .expect("statement should build");
        let changed = AggregateStatement::new(
            AGGREGATE_STATEMENT_VERSION,
            ProofFamilyId::Transfer,
            [1u8; 32],
            &pvk,
            1,
            &changed_rows,
        )
        .expect("changed statement should build");

        assert_ne!(original.canonical_bytes(), changed.canonical_bytes());
        assert_ne!(original.statement_digest(), changed.statement_digest());
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
                version: AGGREGATE_STATEMENT_VERSION,
                proof_family_id: 1,
                consolidate_family_id: 0,
                split_family_id: 0,
                shielded_ics20_withdrawal_family_id: 0,
                srs_id: [1u8; 32],
                vk_digest: [2u8; 32],
                real_count,
                padded_count,
                public_input_arity: expected_arity as u32,
                padded_public_inputs: primitive_rows,
            };
            let encoded = encode_statement(&input).expect("bounded statement encoding succeeds");
            prop_assert!(!encoded.is_empty());
        }
    }
}
