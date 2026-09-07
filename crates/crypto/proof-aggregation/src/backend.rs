use std::fmt;

use std::sync::LazyLock;
#[cfg(test)]
use std::time::Instant;

use anyhow::{ensure, Result};

use ark_ec::{
    pairing::{Pairing, PairingOutput},
    AffineRepr, CurveGroup,
};

use ark_ff::{BigInt, CyclotomicMultSubgroup, Field, PrimeField, Zero};
use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::app_verifier::{
    app_verify_shipping_result_from_backend_result, AppVerifyShippingBackendResult,
    AppVerifyShippingResult,
};

use ark_ip_proofs::applications::groth16_aggregation::validate_decoded_aggregate_proof;
use ark_ip_proofs::applications::groth16_aggregation::{
    aggregate_proofs_with_real_count, shipping_verifier_executed_result_into_parts,
    validate_aggregate_proof, verify_validated_aggregate_proof,
    verify_validated_aggregate_proof_shipping, AggregateProof, ShippingVerifierExecutionCarrier,
    ShippingVerifierObservation, ValidatedAggregateProof,
};
#[cfg(test)]
use ark_ip_proofs::applications::groth16_aggregation::{
    aggregate_proofs_with_trace_and_real_count, verify_validated_aggregate_proof_with_trace,
};
use ark_ip_proofs::challenge::ChallengeContext;
#[cfg(test)]
use ark_ip_proofs::challenge::ChallengeTraceSink;
use ark_serialize::CanonicalSerialize;

use ark_serialize::SerializationError;

use decaf377::Fp;
use decaf377::{Bls12_377, Fq};
use digest::Digest;
use shieldd_sdk_proof_params::batch::BatchItem;
use shieldd_sdk_shielded_pool::NoteReshapeFamilyId;

use crate::strict_deserialize::deserialize_compressed_strict_with;
use crate::{
    aggregate_proof_wrapper::{
        encode_wrapped_aggregate_proof, encode_wrapped_torus_aggregate_proof,
        AggregateProofBytesError, AGGREGATE_PROOF_TORUS_WRAPPER_DOMAIN,
        AGGREGATE_PROOF_WRAPPER_DOMAIN, MAX_AGGREGATE_PROOF_BYTES,
    },
    app_verifier::{app_verify_shipping_into_parts_core, AppVerifyShippingCall},
    preflight::{
        preflight_aggregate_verify, preflight_shipping_aggregate_verify, AggregatePreflightInput,
        ShippingStatementConstructionProvenance, VerifiedAggregateBackendCall,
    },
    srs::DevSrs,
    statement::{AggregateStatement, AggregateStatementError},
    torus::{deserialize_torus_aggregate_proof, serialize_torus_aggregate_proof},
    transcript::{
        NoteReshapeTranscriptDigest, ShieldedIcs20WithdrawalTranscriptDigest,
        TransferTranscriptDigest,
    },
    transfer_family_dispatch::{aggregate_transfer_real_count, verify_transfer_aggregate},
    ProofFamilyId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AggregateVerifyError {
    BadVersion(String),
    BadCount(String),
    BadPadding(String),
    RowArityMismatch(String),
    StatementDigestMismatch,
    OversizeBytes(String),
    MalformedProofBytes(String),
    BackendRejected(String),
}

impl fmt::Display for AggregateVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadVersion(err) => write!(f, "bad aggregate version: {err}"),
            Self::BadCount(err) => write!(f, "bad aggregate count: {err}"),
            Self::BadPadding(err) => write!(f, "bad aggregate padding: {err}"),
            Self::RowArityMismatch(err) => write!(f, "aggregate row arity mismatch: {err}"),
            Self::StatementDigestMismatch => write!(f, "aggregate statement digest mismatch"),
            Self::OversizeBytes(err) => write!(f, "oversized aggregate proof bytes: {err}"),
            Self::MalformedProofBytes(err) => write!(f, "malformed aggregate proof bytes: {err}"),
            Self::BackendRejected(err) => write!(f, "SnarkPack backend rejected aggregate: {err}"),
        }
    }
}

impl std::error::Error for AggregateVerifyError {}

impl From<AggregateStatementError> for AggregateVerifyError {
    fn from(value: AggregateStatementError) -> Self {
        match value {
            AggregateStatementError::BadVersion { .. } => Self::BadVersion(value.to_string()),
            AggregateStatementError::BadCount { .. } => Self::BadCount(value.to_string()),
            AggregateStatementError::BadPadding { .. } => Self::BadPadding(value.to_string()),
            AggregateStatementError::RowArityMismatch { .. } => {
                Self::RowArityMismatch(value.to_string())
            }
            AggregateStatementError::OversizeBytes { .. } => Self::OversizeBytes(value.to_string()),
            AggregateStatementError::EncodingFailed(_) => {
                Self::MalformedProofBytes(value.to_string())
            }
        }
    }
}

impl From<AggregateProofBytesError> for AggregateVerifyError {
    fn from(value: AggregateProofBytesError) -> Self {
        match value {
            AggregateProofBytesError::BadVersion => Self::BadVersion(value.to_string()),
            AggregateProofBytesError::StatementDigestMismatch => Self::StatementDigestMismatch,
            AggregateProofBytesError::OversizeBytes { .. } => {
                Self::OversizeBytes(value.to_string())
            }
            AggregateProofBytesError::MalformedProofBytes => {
                Self::MalformedProofBytes(value.to_string())
            }
        }
    }
}

type ShippingAggregateBackendResult =
    AppVerifyShippingBackendResult<ShippingVerifierExecutionCarrier<Bls12_377>>;

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct ShippingAggregateVerification {
    shipping_result: AppVerifyShippingResult,
    execution: ShippingVerifierExecutionCarrier<Bls12_377>,
    _statement_construction: ShippingStatementConstructionProvenance,
}

impl ShippingAggregateVerification {
    #[doc(hidden)]
    pub fn shipping_result(&self) -> &ark_ip_proofs::app_verifier::AppVerifyShippingResult {
        &self.shipping_result
    }

    #[doc(hidden)]
    pub fn shipping_observation(&self) -> ShippingVerifierObservation {
        self.execution.shipping_observation().clone()
    }

    /// Convert an accepted aggregate result into exact per-statement capabilities.
    ///
    /// The reconstructed canonical statement pins the deployed verification key,
    /// family, SRS identity, counts, order, and public inputs before capabilities
    /// are minted. Rejected or mismatched results fail closed.
    pub fn verified_statement_capabilities(
        &self,
        family_id: ProofFamilyId,
        key: shieldd_sdk_proof_params::DeployedProofKey,
        items: &[BatchItem],
    ) -> anyhow::Result<Vec<shieldd_sdk_proof_params::batch::VerifiedBatchItem>> {
        anyhow::ensure!(
            self.shipping_result.result.accepted,
            "cannot mint proof capabilities from a rejected aggregate"
        );
        let srs_id: [u8; 32] = self
            .shipping_result
            .input
            .srs_id
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("authenticated aggregate SRS id is not 32 bytes"))?;
        let prepared = crate::prepare_verify_inputs(
            items,
            self.shipping_result.input.call.expected_padded_count,
        )?;
        let statement = AggregateStatement::new(
            self.shipping_result.input.protocol_version,
            family_id,
            srs_id,
            key.bundled_pvk(),
            self.shipping_result.input.real_count,
            &prepared.padded_public_inputs,
        )?;
        anyhow::ensure!(
            statement.canonical_bytes() == self.shipping_result.input.canonical_statement_bytes,
            "aggregate capability statement does not match authenticated verifier input"
        );
        anyhow::ensure!(
            statement.statement_digest().as_slice()
                == self.shipping_result.input.statement_digest.as_slice(),
            "aggregate capability statement digest mismatch"
        );

        Ok(items
            .iter()
            .cloned()
            .map(|item| {
                // SAFETY: the accepted shipping result authenticates the exact
                // reconstructed key/family/SRS/count/order/public-input statement.
                unsafe {
                    shieldd_sdk_proof_params::batch::VerifiedBatchItem::from_verified_statement(
                        key,
                        std::sync::Arc::new(item),
                    )
                }
            })
            .collect())
    }

    #[cfg(test)]
    pub(crate) fn statement_construction(&self) -> &ShippingStatementConstructionProvenance {
        &self._statement_construction
    }
}

pub struct SnarkpackBackend;

pub(crate) fn validate_bls12_377_gt_fast(
    value: &PairingOutput<Bls12_377>,
) -> Result<(), SerializationError> {
    const BLS_X: &[u64] = &[0x8508_c000_0000_0001];

    if value.0.is_zero() {
        return Err(SerializationError::InvalidData);
    }

    // x^(q^4-q^2+1)=1 iff x^(q^4)*x=x^(q^2). This establishes
    // cyclotomic membership before cyclotomic exponentiation is used.
    let frobenius_2 = value.0.frobenius_map(2);
    let frobenius_4 = frobenius_2.frobenius_map(2);
    if frobenius_4 * value.0 != frobenius_2 {
        return Err(SerializationError::InvalidData);
    }

    // On that subgroup, q ≡ BLS_X (mod r), and the concrete cofactors are
    // coprime; together these conditions are equivalent to x^r=1.
    if value.0.frobenius_map(1) != value.0.cyclotomic_exp(BLS_X) {
        return Err(SerializationError::InvalidData);
    }
    Ok(())
}

pub(crate) fn validate_bls12_377_g1_fast(
    value: &<Bls12_377 as Pairing>::G1,
) -> Result<(), SerializationError> {
    const X_SQUARED: &[u64] = &[0x0a11_8000_0000_0001, 0x4522_17cc_9000_0001];

    let affine = value.into_affine();
    if !affine.is_on_curve() {
        return Err(SerializationError::InvalidData);
    }

    // The concrete relation phi(P) = [-x^2]P has no false
    // positives. If it does not hold, retain Arkworks' scalar check as the
    // exact fallback; valid points normally remain on the fast path.
    static BETA: LazyLock<Fp> = LazyLock::new(|| {
        Fp::from_bigint(BigInt([
            0xffff_ffff_ffff_ffff,
            0xd1e9_4577_9fff_ffff,
            0x5906_4ee8_22fb_5bff,
            0xb888_2a75_cc9b_c8e3,
            0xbc87_56ba_8f8c_524e,
            0x01ae_3a46_17c5_10ea,
        ]))
        .expect("the pinned G1 endomorphism coefficient is canonical")
    });
    let mut phi = affine;
    phi.x *= *BETA;
    if phi != (-affine.mul_bigint(X_SQUARED)).into_affine()
        && !affine.is_in_correct_subgroup_assuming_on_curve()
    {
        return Err(SerializationError::InvalidData);
    }
    Ok(())
}

pub(crate) fn validate_bls12_377_g2_fast(
    value: &<Bls12_377 as Pairing>::G2,
) -> Result<(), SerializationError> {
    const BLS_X: &[u64] = &[0x8508_c000_0000_0001];
    static PSI_X: LazyLock<Fp> = LazyLock::new(|| {
        Fp::from_bigint(BigInt([
            0x8508_c000_0000_0002,
            0x4522_17cc_9000_0000,
            0xc5ed_1347_970d_ec00,
            0x619a_af7d_3459_4aab,
            0x09b3_af05_dd14_f6ec,
            0x0000_0000_0000_0000,
        ]))
        .expect("the pinned G2 psi-x coefficient is canonical")
    });
    static PSI_Y: LazyLock<Fp> = LazyLock::new(|| {
        Fp::from_bigint(BigInt([
            0x6e76_d5ec_f139_1c63,
            0x9958_8459_bff2_7d8e,
            0xbce6_49cf_436b_0f62,
            0x4003_98f5_0ad1_dec1,
            0xc0c5_34db_1a79_beb1,
            0x0168_0a40_7965_37ca,
        ]))
        .expect("the pinned G2 psi-y coefficient is canonical")
    });

    let affine = value.into_affine();
    if !affine.is_on_curve() {
        return Err(SerializationError::InvalidData);
    }

    // The concrete relation psi(P) = [x]P has no false
    // positives. If it does not hold, retain Arkworks' scalar check as the
    // exact fallback. This is Arkworks' untwist-Frobenius-twist map.
    let mut psi = affine;
    psi.x.frobenius_map_in_place(1);
    psi.y.frobenius_map_in_place(1);
    psi.x.mul_assign_by_basefield(&PSI_X);
    psi.y.mul_assign_by_basefield(&PSI_Y);
    if psi != affine.mul_bigint(BLS_X).into_affine()
        && !affine.is_in_correct_subgroup_assuming_on_curve()
    {
        return Err(SerializationError::InvalidData);
    }
    Ok(())
}

pub(crate) fn deserialize_aggregate_proof<D: Digest + Send + Sync>(
    aggregate_proof_bytes: &[u8],
) -> Result<ValidatedAggregateProof<Bls12_377, D>, AggregateVerifyError> {
    let proof = deserialize_compressed_strict_with::<AggregateProof<Bls12_377, D>>(
        aggregate_proof_bytes,
        |proof| {
            validate_decoded_aggregate_proof(
                proof,
                validate_bls12_377_g1_fast,
                validate_bls12_377_g2_fast,
                validate_bls12_377_gt_fast,
            )
        },
    )
    .map_err(|err| AggregateVerifyError::MalformedProofBytes(err.to_string()))?;

    validate_aggregate_proof(&proof)
        .map_err(|err| AggregateVerifyError::MalformedProofBytes(err.to_string()))
}

impl SnarkpackBackend {
    pub fn aggregate_family_torus(
        statement: &AggregateStatement,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>> {
        let family_id = statement.family_id();
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        let inner = match family_id {
            ProofFamilyId::Transfer => {
                aggregate_with_digest_torus_real_count::<TransferTranscriptDigest>(
                    statement.challenge_context(),
                    items,
                    statement.real_count() as usize,
                    srs,
                )
            }
            ProofFamilyId::NoteReshape(family_id) => match family_id {
                NoteReshapeFamilyId::OneByEight => aggregate_with_digest_torus_real_count::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
                >(
                    statement.challenge_context(),
                    items,
                    statement.real_count() as usize,
                    srs,
                ),
                NoteReshapeFamilyId::EightByOne => aggregate_with_digest_torus_real_count::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
                >(
                    statement.challenge_context(),
                    items,
                    statement.real_count() as usize,
                    srs,
                ),
                other => Err(anyhow::anyhow!(
                    "unknown note reshape aggregate family {}",
                    other.get()
                )),
            },
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                aggregate_with_digest_torus_real_count::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    statement.challenge_context(),
                    items,
                    statement.real_count() as usize,
                    srs,
                )
            }
        }?;
        let wrapped = encode_wrapped_torus_aggregate_proof(statement.statement_digest(), &inner)?;
        ensure!(
            wrapped.len() <= MAX_AGGREGATE_PROOF_BYTES,
            "wrapped aggregate proof bytes {} exceed cap {}",
            wrapped.len(),
            MAX_AGGREGATE_PROOF_BYTES
        );
        Ok(wrapped)
    }

    pub fn verify_family_aggregate_torus(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &DevSrs,
    ) -> Result<(), AggregateVerifyError> {
        let preflight_frame = torus_preflight_frame(aggregate_proof_bytes);
        let call = preflight_aggregate_verify(AggregatePreflightInput {
            statement,
            pvk,
            aggregate_proof_bytes: &preflight_frame,
            srs,
        })?;
        let accepted = match call.family_id() {
            ProofFamilyId::Transfer => verify_with_digest_torus::<TransferTranscriptDigest>(
                call.challenge_context(),
                call.pvk(),
                call.inner_proof_bytes(),
                call.padded_public_inputs(),
                call.srs(),
            )?,
            ProofFamilyId::NoteReshape(family_id) => match family_id {
                NoteReshapeFamilyId::OneByEight => verify_with_digest_torus::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
                >(
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?,
                NoteReshapeFamilyId::EightByOne => verify_with_digest_torus::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
                >(
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?,
                other => {
                    return Err(AggregateVerifyError::BadVersion(format!(
                        "unknown note reshape aggregate family {}",
                        other.get()
                    )))
                }
            },
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                verify_with_digest_torus::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
            }
        };
        if !accepted {
            return Err(AggregateVerifyError::BackendRejected(format!(
                "{:?}",
                statement.family_id()
            )));
        }
        Ok(())
    }

    fn aggregate_transfer_family(
        challenge_context: &ChallengeContext,
        items: &[BatchItem],
        real_count: usize,
        srs: &DevSrs,
    ) -> Result<Vec<u8>> {
        aggregate_transfer_real_count(challenge_context, items, real_count, srs)
    }

    fn verify_transfer_family_aggregate(
        challenge_context: &ChallengeContext,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<bool, AggregateVerifyError> {
        verify_transfer_aggregate(
            challenge_context,
            pvk,
            aggregate_proof_bytes,
            padded_public_inputs,
            srs,
        )
    }

    fn aggregate_note_reshape_family(
        family_id: NoteReshapeFamilyId,
        challenge_context: &ChallengeContext,
        items: &[BatchItem],
        real_count: usize,
        srs: &DevSrs,
    ) -> Result<Vec<u8>> {
        match family_id {
            NoteReshapeFamilyId::OneByEight => {
                aggregate_with_digest_real_count::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
                >(challenge_context, items, real_count, srs)
            }
            NoteReshapeFamilyId::EightByOne => {
                aggregate_with_digest_real_count::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
                >(challenge_context, items, real_count, srs)
            }
            other => Err(anyhow::anyhow!(
                "unknown note reshape aggregate family {}",
                other.get()
            )),
        }
    }

    fn verify_note_reshape_family_aggregate(
        challenge_context: &ChallengeContext,
        family_id: NoteReshapeFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<bool, AggregateVerifyError> {
        match family_id {
            NoteReshapeFamilyId::OneByEight => verify_with_digest::<
                NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            NoteReshapeFamilyId::EightByOne => verify_with_digest::<
                NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            other => Err(AggregateVerifyError::BadVersion(format!(
                "unknown note reshape aggregate family {}",
                other.get()
            ))),
        }
    }

    pub(crate) fn verify_shipping_family_aggregate(
        application_call: AppVerifyShippingCall,
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &DevSrs,
    ) -> Result<ShippingAggregateVerification, AggregateVerifyError> {
        let call_id = application_call.id;
        let provenance = preflight_shipping_aggregate_verify(
            application_call,
            AggregatePreflightInput {
                statement,
                pvk,
                aggregate_proof_bytes,
                srs,
            },
        )?;
        let statement_construction = ShippingStatementConstructionProvenance {
            binding_execution: provenance.binding_execution,
            source_field_rows: provenance.source_field_rows,
            prepared_serialized_rows: provenance.prepared_serialized_rows,
        };
        let (backend_call, padded_public_input_fields, input) =
            app_verify_shipping_into_parts_core(provenance.preflight);
        let backend_result = Self::verify_preflighted_shipping_family_aggregate(
            call_id,
            backend_call,
            &padded_public_input_fields,
        )?;
        let executed = app_verify_shipping_result_from_backend_result(input, backend_result)
            .map_err(|error| {
                AggregateVerifyError::BackendRejected(format!(
                    "shipping backend result identity mismatch: {error:?}"
                ))
            })?;
        let (shipping_result, execution) = shipping_verifier_executed_result_into_parts(executed);
        Ok(ShippingAggregateVerification {
            shipping_result,
            execution,
            _statement_construction: statement_construction,
        })
    }

    fn verify_preflighted_shipping_family_aggregate(
        call_id: ark_ip_proofs::app_verifier::AppVerifyCallId,
        call: VerifiedAggregateBackendCall<'_>,
        padded_public_input_fields: &[Vec<Fq>],
    ) -> Result<ShippingAggregateBackendResult, AggregateVerifyError> {
        match call.family_id() {
            ProofFamilyId::Transfer => verify_with_digest_shipping::<TransferTranscriptDigest>(
                call_id,
                call.challenge_context(),
                call.pvk(),
                call.inner_proof_bytes(),
                padded_public_input_fields,
                call.srs(),
            ),
            ProofFamilyId::NoteReshape(family_id) => match family_id {
                NoteReshapeFamilyId::OneByEight => verify_with_digest_shipping::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
                >(
                    call_id,
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    padded_public_input_fields,
                    call.srs(),
                ),
                NoteReshapeFamilyId::EightByOne => verify_with_digest_shipping::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
                >(
                    call_id,
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    padded_public_input_fields,
                    call.srs(),
                ),
                other => Err(AggregateVerifyError::BadVersion(format!(
                    "unknown note reshape aggregate family {}",
                    other.get()
                ))),
            },
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                verify_with_digest_shipping::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    call_id,
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    padded_public_input_fields,
                    call.srs(),
                )
            }
        }
    }
}

impl SnarkpackBackend {
    pub fn aggregate_family(
        statement: &AggregateStatement,
        _pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>> {
        let family_id = statement.family_id();
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        let inner_proof_bytes = match family_id {
            ProofFamilyId::Transfer => Self::aggregate_transfer_family(
                statement.challenge_context(),
                items,
                statement.real_count() as usize,
                srs,
            ),
            ProofFamilyId::NoteReshape(family_id) => Self::aggregate_note_reshape_family(
                family_id,
                statement.challenge_context(),
                items,
                statement.real_count() as usize,
                srs,
            ),
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                aggregate_with_digest_real_count::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    statement.challenge_context(),
                    items,
                    statement.real_count() as usize,
                    srs,
                )
            }
        }?;

        let wrapped =
            encode_wrapped_aggregate_proof(statement.statement_digest(), &inner_proof_bytes)?;
        ensure!(
            wrapped.len() <= MAX_AGGREGATE_PROOF_BYTES,
            "wrapped aggregate proof bytes {} exceed cap {}",
            wrapped.len(),
            MAX_AGGREGATE_PROOF_BYTES
        );
        Ok(wrapped)
    }

    pub fn verify_family_aggregate(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &DevSrs,
    ) -> Result<(), AggregateVerifyError> {
        let call = preflight_aggregate_verify(AggregatePreflightInput {
            statement,
            pvk,
            aggregate_proof_bytes,
            srs,
        })?;
        let family_id = call.family_id();

        let accepted = match family_id {
            ProofFamilyId::Transfer => Self::verify_transfer_family_aggregate(
                call.challenge_context(),
                call.pvk(),
                call.inner_proof_bytes(),
                call.padded_public_inputs(),
                call.srs(),
            )?,
            ProofFamilyId::NoteReshape(family_id) => Self::verify_note_reshape_family_aggregate(
                call.challenge_context(),
                family_id,
                call.pvk(),
                call.inner_proof_bytes(),
                call.padded_public_inputs(),
                call.srs(),
            )?,
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                verify_with_digest::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
            }
        };

        if !accepted {
            return Err(AggregateVerifyError::BackendRejected(format!(
                "{family_id:?}"
            )));
        }
        Ok(())
    }
}

fn collect_proofs(items: &[BatchItem]) -> Vec<ark_groth16::Proof<Bls12_377>> {
    items.iter().map(|item| item.proof.clone()).collect()
}

fn torus_preflight_frame(bytes: &[u8]) -> Vec<u8> {
    let retained_len = bytes.len().min(MAX_AGGREGATE_PROOF_BYTES.saturating_add(1));
    let mut translated = bytes[..retained_len].to_vec();

    if translated.starts_with(AGGREGATE_PROOF_TORUS_WRAPPER_DOMAIN) {
        if AGGREGATE_PROOF_TORUS_WRAPPER_DOMAIN.len() != AGGREGATE_PROOF_WRAPPER_DOMAIN.len() {
            translated.clear();
            return translated;
        }
        translated[..AGGREGATE_PROOF_WRAPPER_DOMAIN.len()]
            .copy_from_slice(AGGREGATE_PROOF_WRAPPER_DOMAIN);
    } else if translated.starts_with(AGGREGATE_PROOF_WRAPPER_DOMAIN) {
        translated[0] = 0;
    }

    translated
}

pub(crate) fn aggregate_with_digest_real_count<D: Digest + Send + Sync>(
    challenge_context: &ChallengeContext,
    items: &[BatchItem],
    real_count: usize,
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    // One dedicated worker prevents concurrent family jobs from stealing each other's work.
    let aggregate = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_scoped(
            |thread| thread.run(),
            |pool| {
                pool.install(|| {
                    aggregate_proofs_with_real_count::<Bls12_377, D>(
                        challenge_context,
                        &inner_product_srs,
                        &collect_proofs(items),
                        real_count,
                    )
                    .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))
                })
            },
        )
        .map_err(|e| anyhow::anyhow!("rayon pool build error: {e}"))??;
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn aggregate_with_digest_torus_real_count<D: Digest + Send + Sync>(
    challenge_context: &ChallengeContext,
    items: &[BatchItem],
    real_count: usize,
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    // One dedicated worker prevents concurrent family jobs from stealing each other's work.
    let aggregate = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_scoped(
            |thread| thread.run(),
            |pool| {
                pool.install(|| {
                    aggregate_proofs_with_real_count::<Bls12_377, D>(
                        challenge_context,
                        &inner_product_srs,
                        &collect_proofs(items),
                        real_count,
                    )
                    .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))
                })
            },
        )
        .map_err(|e| anyhow::anyhow!("rayon pool build error: {e}"))??;
    serialize_torus_aggregate_proof(&aggregate)
        .map_err(|e| anyhow::anyhow!("SnarkPack torus serialization failed: {e}"))
}

#[cfg(test)]
pub(crate) fn aggregate_with_digest_with_trace_and_real_count<D, S>(
    challenge_context: &ChallengeContext,
    trace: &mut S,
    items: &[BatchItem],
    real_count: usize,
    srs: &DevSrs,
) -> Result<Vec<u8>>
where
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    let aggregate = aggregate_proofs_with_trace_and_real_count::<Bls12_377, D, S>(
        challenge_context,
        trace,
        &inner_product_srs,
        &collect_proofs(items),
        real_count,
    )
    .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))?;
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn verify_with_digest<D: Digest + Send + Sync>(
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool, AggregateVerifyError> {
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    verify_validated_aggregate_proof::<Bls12_377, D>(
        challenge_context,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        pvk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))
}

pub(crate) fn verify_with_digest_torus<D: Digest + Send + Sync>(
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool, AggregateVerifyError> {
    let aggregate = deserialize_torus_aggregate_proof::<D>(aggregate_proof_bytes)
        .map_err(|e| AggregateVerifyError::MalformedProofBytes(e.to_string()))?;
    verify_validated_aggregate_proof::<Bls12_377, D>(
        challenge_context,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        pvk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))
}

#[cfg(test)]
pub(crate) fn verify_with_digest_with_trace<D, S>(
    challenge_context: &ChallengeContext,
    trace: &mut S,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool, AggregateVerifyError>
where
    D: Digest + Send + Sync,
    S: ChallengeTraceSink,
{
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    verify_validated_aggregate_proof_with_trace::<Bls12_377, D, S>(
        challenge_context,
        trace,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        pvk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))
}

fn verify_with_digest_shipping<D: Digest + Send + Sync>(
    call_id: ark_ip_proofs::app_verifier::AppVerifyCallId,
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<ShippingAggregateBackendResult, AggregateVerifyError> {
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;

    let verification = verify_validated_aggregate_proof_shipping::<Bls12_377, D>(
        call_id,
        challenge_context,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        pvk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))?;
    Ok(verification)
}

#[cfg(test)]
mod tests {
    use ark_ec::{
        pairing::{Pairing, PairingOutput},
        AffineRepr, CurveGroup, PrimeGroup,
    };
    use ark_ff::{Field, PrimeField, UniformRand, Zero};
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
    use ark_ip_proofs::challenge::{ChallengeTraceEntry, VecChallengeTraceSink};
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use ark_serialize::CanonicalDeserialize;

    use ark_serialize::Valid;
    use ark_snark::SNARK;
    use decaf377::{Fp, Fq};
    use proptest::prelude::*;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
    use shieldd_sdk_proof_params::batch;
    use shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId;

    use crate::transcript::TransferTranscriptDigest;
    use crate::{
        aggregate_family, aggregate_family_torus, decode_wrapped_aggregate_proof,
        decode_wrapped_torus_aggregate_proof, encode_wrapped_aggregate_proof,
        encode_wrapped_torus_aggregate_proof, pad_items_to_power_of_two, srs_id,
        statement::aggregate_verification_key_digest, verify_family_aggregate,
        verify_family_aggregate_torus, AggregateStatement, AggregateVerifyError,
        AGGREGATE_PROTOCOL_VERSION,
    };

    use super::*;

    #[test]
    fn fast_bls12_377_gt_validation_matches_arkworks_on_structured_inputs() {
        type TargetField = <Bls12_377 as Pairing>::TargetField;

        let identity = PairingOutput::<Bls12_377>::zero();
        assert!(identity.check().is_ok());
        assert!(validate_bls12_377_gt_fast(&identity).is_ok());

        let mut rng = ChaCha20Rng::seed_from_u64(0x6774_7661_7374);
        for _ in 0..32 {
            let pairing = Bls12_377::pairing(
                <Bls12_377 as Pairing>::G1::rand(&mut rng),
                <Bls12_377 as Pairing>::G2::rand(&mut rng),
            );
            assert!(pairing.check().is_ok());
            assert!(validate_bls12_377_gt_fast(&pairing).is_ok());

            let raw = PairingOutput::<Bls12_377>(TargetField::rand(&mut rng));
            assert_eq!(
                validate_bls12_377_gt_fast(&raw).is_ok(),
                raw.check().is_ok()
            );

            // The easy part of the final exponentiation maps an arbitrary
            // nonzero field value into the full cyclotomic subgroup, including
            // cofactor torsion that both predicates must reject.
            let sample = TargetField::rand(&mut rng);
            if let Some(inverse) = sample.inverse() {
                let easy = sample.frobenius_map(6) * inverse;
                let cyclotomic = PairingOutput::<Bls12_377>(easy.frobenius_map(2) * easy);
                assert_eq!(
                    validate_bls12_377_gt_fast(&cyclotomic).is_ok(),
                    cyclotomic.check().is_ok()
                );
            }
        }
    }

    #[test]
    fn bls12_377_fr_from_random_bytes_documents_shipping_reduction_boundary() {
        type Fr = <Bls12_377 as Pairing>::ScalarField;

        let mut digest = [0u8; 64];
        digest[0] = 1;
        digest[31] = 0xe0;
        digest[32] = 2;
        digest[63] = 0xa5;

        let actual =
            Fr::from_random_bytes(&digest).expect("the shipping decaf377 scalar decoder is total");
        let two_to_256 = Fr::from(2u64).pow([256u64]);
        let expected = Fr::from_le_bytes_mod_order(&digest[..32])
            + Fr::from_le_bytes_mod_order(&digest[32..]) * two_to_256;
        assert_eq!(
            actual, expected,
            "the shipping decoder reduces the complete 64-byte digest"
        );

        let mut high_bits_cleared = digest;
        high_bits_cleared[31] &= 0x1f;
        assert_ne!(
            Fr::from_random_bytes(&digest),
            Fr::from_random_bytes(&high_bits_cleared),
            "decaf377 does not mask the high three bits of byte 31"
        );

        let mut suffix_cleared = digest;
        suffix_cleared[32..].fill(0);
        assert_ne!(
            Fr::from_random_bytes(&digest),
            Fr::from_random_bytes(&suffix_cleared),
            "bytes 32 through 63 are consumed by the shipping decoder"
        );

        let modulus = Fr::MODULUS;
        let mut modulus_digest = [0u8; 64];
        for (index, limb) in modulus.as_ref().iter().enumerate() {
            modulus_digest[index * 8..(index + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        assert_eq!(
            Fr::from_random_bytes(&modulus_digest),
            Some(Fr::zero()),
            "the modulus is reduced to zero instead of being rejected"
        );
        assert!(
            Fr::from_random_bytes(&[u8::MAX; 64]).is_some(),
            "every 64-byte Blake2b digest is accepted by the shipping decoder"
        );
    }

    /// The optimized PPE expression in `verify_ppe` — `e(α,β)^{r_sum}` reused as a
    /// GT exponentiation plus two pairings against the prepared `-γ`/`-δ` line
    /// tables (from `PreparedVerifyingKey`) — computes the identical target-group
    /// element as the original three-pairing form, for random inputs. This is the
    /// algebraic equivalence guarding that category-1 optimization (the raw `vk`
    /// three-pairing form is retained here as an independent equation reference).
    #[test]
    fn ppe_optimized_matches_baseline_gt_value() {
        use ark_inner_products::{cfg_multi_pairing, cfg_multi_pairing_g1_affine_g2_prepared};
        type P = Bls12_377;
        type G1 = <P as Pairing>::G1;
        type G2 = <P as Pairing>::G2;
        type G2Prepared = <P as Pairing>::G2Prepared;

        let mut rng = ChaCha20Rng::seed_from_u64(2024);
        for _ in 0..32 {
            let alpha = G1::rand(&mut rng);
            let beta = G2::rand(&mut rng);
            let gamma = G2::rand(&mut rng);
            let delta = G2::rand(&mut rng);
            let g_ic = G1::rand(&mut rng);
            let agg_c = G1::rand(&mut rng);
            let r_sum = <P as Pairing>::ScalarField::rand(&mut rng);

            // Baseline: three pairings over the raw vk points.
            let baseline =
                cfg_multi_pairing::<P>(&[alpha * r_sum, g_ic, agg_c], &[beta, gamma, delta])
                    .unwrap();

            // Optimized: e(α,β)^{r_sum} as a GT exponentiation, plus two pairings
            // against the prepared -γ / -δ tables (stands in for pvk's precomputes).
            let alpha_g1_beta_g2 = <P as Pairing>::pairing(alpha, beta).0;
            let alpha_beta_rsum = PairingOutput::<P>(alpha_g1_beta_g2) * r_sum;
            let neg_gamma_pc = G2Prepared::from((-gamma).into_affine());
            let neg_delta_pc = G2Prepared::from((-delta).into_affine());
            let g1_affine = G1::normalize_batch(&[-g_ic, -agg_c]);
            let folded = cfg_multi_pairing_g1_affine_g2_prepared::<P>(
                &g1_affine,
                &[neg_gamma_pc, neg_delta_pc],
            )
            .unwrap();

            assert_eq!(baseline, alpha_beta_rsum + folded);
        }
    }

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

    fn sample_items() -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
        sample_items_with_count(7, 3)
    }

    fn sample_items_with_count(
        seed: u64,
        count: usize,
    ) -> (PreparedVerifyingKey<Bls12_377>, Vec<BatchItem>) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let setup_circuit = SquareCircuit {
            x: Some(Fq::from(1u64)),
        };
        let pk =
            Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                setup_circuit,
                &mut rng,
            )
            .expect("setup should succeed");
        let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();

        let items = (0..count)
            .map(|_| {
                let x = Fq::rand(&mut rng);
                let circuit = SquareCircuit { x: Some(x) };
                let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                    .expect("proof generation should succeed");

                BatchItem {
                    proof,
                    public_inputs: vec![x * x],
                }
            })
            .collect();

        (pvk, items)
    }

    fn parity_families() -> Vec<ProofFamilyId> {
        let mut families = vec![ProofFamilyId::Transfer];
        families.extend(
            NoteReshapeFamilyId::ALL
                .into_iter()
                .map(ProofFamilyId::NoteReshape),
        );
        families.push(ProofFamilyId::ShieldedIcs20Withdrawal(
            ShieldedIcs20WithdrawalFamilyId::Canonical,
        ));
        families
    }

    fn padded_public_inputs(items: &[BatchItem]) -> Vec<Vec<Fq>> {
        items
            .iter()
            .map(|item| item.public_inputs.clone())
            .collect()
    }

    fn statement_for_items(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        real_count: usize,
        padded_items: &[BatchItem],
        srs: &DevSrs,
    ) -> AggregateStatement {
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            family_id,
            srs_id(srs),
            pvk,
            real_count as u32,
            &padded_public_inputs(padded_items),
        )
        .expect("statement should build")
    }

    fn statement_for_public_inputs(
        family_id: ProofFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        real_count: usize,
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> AggregateStatement {
        AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            family_id,
            srs_id(srs),
            pvk,
            real_count as u32,
            padded_public_inputs,
        )
        .expect("statement should build")
    }

    fn shipping_call(
        family_id: ProofFamilyId,
        real_count: usize,
        padded_count: usize,
    ) -> AppVerifyShippingCall {
        let family = crate::app_verify_family_code(family_id);
        ark_ip_proofs::app_verifier::AppVerifyShippingCall {
            id: crate::AppVerifyCallId {
                order_index: 0,
                segment_index: 0,
                family_index: 0,
                family,
            },
            bundle_family: family,
            expected_real_count: real_count,
            bundle_real_count: real_count as u32,
            expected_padded_count: padded_count,
            bundle_padded_count: padded_count as u32,
        }
    }

    type BackendG1 = <Bls12_377 as Pairing>::G1;
    type BackendG2 = <Bls12_377 as Pairing>::G2;
    type BackendG1Affine = <Bls12_377 as Pairing>::G1Affine;
    type BackendG2Affine = <Bls12_377 as Pairing>::G2Affine;
    type BackendScalar = <Bls12_377 as Pairing>::ScalarField;

    fn compressed_bytes<T: CanonicalSerialize>(value: &T) -> Vec<u8> {
        let mut bytes = Vec::new();
        value
            .serialize_compressed(&mut bytes)
            .expect("compressed serialization should succeed");
        bytes
    }

    fn assert_compressed_round_trip<T>(value: &T)
    where
        T: CanonicalDeserialize + CanonicalSerialize + Eq + std::fmt::Debug,
    {
        let bytes = compressed_bytes(value);
        let decoded =
            T::deserialize_compressed(&bytes[..]).expect("compressed round trip should decode");
        assert_eq!(&decoded, value);
    }

    fn sequential_msm<G>(bases: &[G], scalars: &[G::ScalarField]) -> G
    where
        G: CurveGroup,
    {
        bases
            .iter()
            .zip(scalars)
            .fold(G::zero(), |mut acc, (base, scalar)| {
                let mut term = *base;
                term *= *scalar;
                acc += term;
                acc
            })
    }

    fn arkworks_msm<G>(bases: &[G], scalars: &[G::ScalarField]) -> G
    where
        G: CurveGroup,
    {
        let affine_bases = G::normalize_batch(bases);
        G::msm(&affine_bases, scalars).expect("MSM inputs have matching lengths")
    }

    fn non_subgroup_g1(rng: &mut ChaCha20Rng) -> BackendG1Affine {
        for _ in 0..10_000 {
            let x = <BackendG1Affine as AffineRepr>::BaseField::rand(rng);
            for greatest in [false, true] {
                if let Some(point) = BackendG1Affine::get_point_from_x_unchecked(x, greatest) {
                    if point.is_on_curve() && !point.is_in_correct_subgroup_assuming_on_curve() {
                        return point;
                    }
                }
            }
        }
        panic!("could not sample a G1 non-subgroup point");
    }

    fn non_subgroup_g2(rng: &mut ChaCha20Rng) -> BackendG2Affine {
        for _ in 0..10_000 {
            let x = <BackendG2Affine as AffineRepr>::BaseField::rand(rng);
            for greatest in [false, true] {
                if let Some(point) = BackendG2Affine::get_point_from_x_unchecked(x, greatest) {
                    if point.is_on_curve() && !point.is_in_correct_subgroup_assuming_on_curve() {
                        return point;
                    }
                }
            }
        }
        panic!("could not sample a G2 non-subgroup point");
    }

    #[test]
    fn fast_bls12_377_g1_g2_validation_matches_arkworks() {
        let mut rng = ChaCha20Rng::seed_from_u64(0xe10_377);

        for (label, point) in [
            BackendG1::zero(),
            BackendG1::generator(),
            BackendG1::rand(&mut rng),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                validate_bls12_377_g1_fast(&point).is_ok(),
                point.check().is_ok(),
                "G1 structured sample {label}"
            );
        }
        for point in [
            BackendG2::zero(),
            BackendG2::generator(),
            BackendG2::rand(&mut rng),
        ] {
            assert_eq!(
                validate_bls12_377_g2_fast(&point).is_ok(),
                point.check().is_ok()
            );
        }

        for _ in 0..32 {
            let g1 = non_subgroup_g1(&mut rng).into_group();
            assert!(!validate_bls12_377_g1_fast(&g1).is_ok());
            assert!(!g1.check().is_ok());

            let g2 = non_subgroup_g2(&mut rng).into_group();
            assert!(!validate_bls12_377_g2_fast(&g2).is_ok());
            assert!(!g2.check().is_ok());
        }

        let mut off_curve_g1 = BackendG1Affine::new_unchecked(
            <BackendG1Affine as AffineRepr>::BaseField::rand(&mut rng),
            <BackendG1Affine as AffineRepr>::BaseField::rand(&mut rng),
        );
        while off_curve_g1.is_on_curve() {
            off_curve_g1.y = <BackendG1Affine as AffineRepr>::BaseField::rand(&mut rng);
        }
        let off_curve_g1 = off_curve_g1.into_group();
        assert!(!validate_bls12_377_g1_fast(&off_curve_g1).is_ok());
        assert!(!off_curve_g1.check().is_ok());

        let mut off_curve_g2 = BackendG2Affine::new_unchecked(
            <BackendG2Affine as AffineRepr>::BaseField::rand(&mut rng),
            <BackendG2Affine as AffineRepr>::BaseField::rand(&mut rng),
        );
        while off_curve_g2.is_on_curve() {
            off_curve_g2.y = <BackendG2Affine as AffineRepr>::BaseField::rand(&mut rng);
        }
        let off_curve_g2 = off_curve_g2.into_group();
        assert!(!validate_bls12_377_g2_fast(&off_curve_g2).is_ok());
        assert!(!off_curve_g2.check().is_ok());
    }

    #[derive(Clone, Copy)]
    enum ExpectedRejection {
        MalformedProofBytes,
        OversizeBytes,
        StatementDigestMismatch,
    }

    struct RejectionCase {
        name: &'static str,
        statement: AggregateStatement,
        aggregate_bytes: Vec<u8>,
        expected: ExpectedRejection,
    }

    #[derive(Clone, Copy, Debug)]
    struct TimingStats {
        p50_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
    }

    fn assert_expected_rejection(err: &AggregateVerifyError, expected: ExpectedRejection) {
        match expected {
            ExpectedRejection::MalformedProofBytes => {
                assert!(
                    matches!(err, AggregateVerifyError::MalformedProofBytes(_)),
                    "{err}"
                )
            }
            ExpectedRejection::OversizeBytes => {
                assert!(
                    matches!(err, AggregateVerifyError::OversizeBytes(_)),
                    "{err}"
                )
            }
            ExpectedRejection::StatementDigestMismatch => {
                assert_eq!(err, &AggregateVerifyError::StatementDigestMismatch)
            }
        }
    }

    fn percentile(values: &[f64], pct: f64) -> f64 {
        let mut sorted = values.to_vec();
        sorted.sort_by(|left, right| {
            left.partial_cmp(right)
                .expect("timing measurements are finite")
        });
        let rank = ((sorted.len() as f64 - 1.0) * pct).ceil() as usize;
        sorted[rank.min(sorted.len() - 1)]
    }

    fn timing_stats(values: &[f64]) -> TimingStats {
        assert!(!values.is_empty());
        TimingStats {
            p50_ms: percentile(values, 0.50),
            p95_ms: percentile(values, 0.95),
            p99_ms: percentile(values, 0.99),
        }
    }

    fn groth16_oracle_agreement_for_counts(counts: &[usize]) {
        let (pvk, items) = sample_items();
        let base_item = items.first().expect("at least one sample item").clone();
        let srs = DevSrs::default();

        for family_id in parity_families() {
            for count in counts {
                let repeated = vec![base_item.clone(); *count];
                for item in &repeated {
                    assert!(
                        Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                            &pvk,
                            &item.public_inputs,
                            &item.proof,
                        )
                        .expect("single Groth16 oracle should run"),
                        "single Groth16 oracle should accept valid item"
                    );
                }

                let batch_accepts = batch::batch_verify(&pvk, &repeated).is_ok();
                let statement = statement_for_items(family_id, &pvk, *count, &repeated, &srs);
                let aggregate = aggregate_family(&statement, &pvk, &repeated, &srs)
                    .expect("aggregation should succeed");
                let snarkpack_accepts =
                    verify_family_aggregate(&statement, &pvk, &aggregate, &srs).is_ok();
                assert_eq!(
                    snarkpack_accepts, batch_accepts,
                    "SnarkPack and batch oracle disagree for {family_id:?} count={count}"
                );

                if *count <= 64 {
                    let mut invalid_items = repeated.clone();
                    for item in &mut invalid_items {
                        item.public_inputs[0] += Fq::from(1u64);
                        assert!(
                            !Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                                &pvk,
                                &item.public_inputs,
                                &item.proof,
                            )
                            .expect("single Groth16 oracle should run"),
                            "single Groth16 oracle should reject mutated public input"
                        );
                    }
                    assert!(
                        batch::batch_verify(&pvk, &invalid_items).is_err(),
                        "batch oracle should reject invalid items"
                    );
                    let invalid_statement =
                        statement_for_items(family_id, &pvk, *count, &invalid_items, &srs);
                    let invalid_aggregate =
                        aggregate_family(&invalid_statement, &pvk, &invalid_items, &srs)
                            .expect("aggregation over invalid proof records should serialize");
                    assert!(
                        verify_family_aggregate(&invalid_statement, &pvk, &invalid_aggregate, &srs)
                            .is_err(),
                        "SnarkPack should reject aggregate built over individually invalid items"
                    );
                }
            }
        }
    }

    #[test]
    fn arkworks_pairing_identity_and_generator_consistency() {
        let generator_pairing = Bls12_377::pairing(BackendG1::generator(), BackendG2::generator());
        assert_ne!(generator_pairing, PairingOutput::<Bls12_377>::zero());
        assert_eq!(
            Bls12_377::pairing(BackendG1::zero(), BackendG2::generator()),
            PairingOutput::<Bls12_377>::zero()
        );
        assert_eq!(
            Bls12_377::pairing(BackendG1::generator(), BackendG2::zero()),
            PairingOutput::<Bls12_377>::zero()
        );
    }

    #[test]
    fn arkworks_msm_boundary_zero_scalar_identity_and_random_parity() {
        let mut rng = ChaCha20Rng::seed_from_u64(0x5eed);
        let scalars: Vec<BackendScalar> = (0..16).map(|_| BackendScalar::rand(&mut rng)).collect();
        let zero_scalars = vec![BackendScalar::zero(); scalars.len()];

        let g1_bases: Vec<BackendG1> = (0..scalars.len())
            .map(|_| BackendG1::rand(&mut rng))
            .collect();
        let g1_identities = vec![BackendG1::zero(); scalars.len()];
        assert_eq!(arkworks_msm(&g1_bases, &zero_scalars), BackendG1::zero());
        assert_eq!(arkworks_msm(&g1_identities, &scalars), BackendG1::zero());
        assert_eq!(
            arkworks_msm(&g1_bases, &scalars),
            sequential_msm(&g1_bases, &scalars)
        );

        let g2_bases: Vec<BackendG2> = (0..scalars.len())
            .map(|_| BackendG2::rand(&mut rng))
            .collect();
        let g2_identities = vec![BackendG2::zero(); scalars.len()];
        assert_eq!(arkworks_msm(&g2_bases, &zero_scalars), BackendG2::zero());
        assert_eq!(arkworks_msm(&g2_identities, &scalars), BackendG2::zero());
        assert_eq!(
            arkworks_msm(&g2_bases, &scalars),
            sequential_msm(&g2_bases, &scalars)
        );
    }

    #[test]
    fn arkworks_g1_g2_compressed_round_trip_and_identity() {
        let mut rng = ChaCha20Rng::seed_from_u64(0xdecaf);

        for point in [
            BackendG1::zero().into_affine(),
            BackendG1::generator().into_affine(),
            BackendG1::rand(&mut rng).into_affine(),
        ] {
            assert_compressed_round_trip(&point);
        }

        for point in [
            BackendG2::zero().into_affine(),
            BackendG2::generator().into_affine(),
            BackendG2::rand(&mut rng).into_affine(),
        ] {
            assert_compressed_round_trip(&point);
        }
    }

    #[test]
    fn arkworks_g1_g2_malformed_compressed_bytes_reject() {
        let g1_bytes = compressed_bytes(&BackendG1::generator().into_affine());
        let g2_bytes = compressed_bytes(&BackendG2::generator().into_affine());

        assert!(BackendG1Affine::deserialize_compressed(&g1_bytes[..g1_bytes.len() - 1]).is_err());
        assert!(BackendG2Affine::deserialize_compressed(&g2_bytes[..g2_bytes.len() - 1]).is_err());
        assert!(BackendG1Affine::deserialize_compressed(&vec![0xff; g1_bytes.len()][..]).is_err());
        assert!(BackendG2Affine::deserialize_compressed(&vec![0xff; g2_bytes.len()][..]).is_err());
    }

    #[test]
    fn arkworks_g1_g2_subgroup_and_torsion_rejection() {
        let mut rng = ChaCha20Rng::seed_from_u64(0x51a7e);

        let g1_torsion = non_subgroup_g1(&mut rng);
        let g1_bytes = compressed_bytes(&g1_torsion);
        assert!(
            BackendG1Affine::deserialize_compressed(&g1_bytes[..]).is_err(),
            "checked G1 deserialization must reject non-subgroup points"
        );

        let g2_torsion = non_subgroup_g2(&mut rng);
        let g2_bytes = compressed_bytes(&g2_torsion);
        assert!(
            BackendG2Affine::deserialize_compressed(&g2_bytes[..]).is_err(),
            "checked G2 deserialization must reject non-subgroup points"
        );
    }

    #[test]
    fn decaf377_vk_digest_round_trips_after_serialization() {
        let (pvk, _) = sample_items();
        let original_digest =
            aggregate_verification_key_digest(&pvk).expect("VK digest should build");
        let bytes = compressed_bytes(&pvk);
        let decoded = PreparedVerifyingKey::<Bls12_377>::deserialize_compressed(&bytes[..])
            .expect("prepared verifying key should round-trip");

        let decoded_digest =
            aggregate_verification_key_digest(&decoded).expect("decoded VK digest should build");
        assert_eq!(decoded_digest, original_digest);
    }

    #[test]
    fn snarkpack_backend_accepts_valid_aggregate() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");

        verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
            .expect("aggregate verification should succeed");
    }

    #[test]
    fn torus_preflight_translation_is_bounded_and_disjoint() {
        let digest = [0x42; 32];
        let inner = [1, 2, 3, 4];
        let torus = encode_wrapped_torus_aggregate_proof(digest, &inner).unwrap();
        let translated = torus_preflight_frame(&torus);
        assert!(translated.starts_with(AGGREGATE_PROOF_WRAPPER_DOMAIN));
        assert_eq!(
            &translated[AGGREGATE_PROOF_WRAPPER_DOMAIN.len()..],
            &torus[AGGREGATE_PROOF_TORUS_WRAPPER_DOMAIN.len()..]
        );

        let v1 = encode_wrapped_aggregate_proof(digest, &inner).unwrap();
        assert!(!torus_preflight_frame(&v1).starts_with(AGGREGATE_PROOF_WRAPPER_DOMAIN));

        let oversized = vec![0; MAX_AGGREGATE_PROOF_BYTES + 2];
        assert_eq!(
            torus_preflight_frame(&oversized).len(),
            MAX_AGGREGATE_PROOF_BYTES + 1
        );
    }

    #[test]
    fn torus_round_trips_real_aggregate_and_halves_target_wire() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let statement = statement_for_items(
            ProofFamilyId::Transfer,
            &pvk,
            items.len(),
            &padded_items,
            &srs,
        );

        let standard = aggregate_family(&statement, &pvk, &padded_items, &srs).unwrap();
        let torus = aggregate_family_torus(&statement, &padded_items, &srs).unwrap();

        verify_family_aggregate_torus(&statement, &pvk, &torus, &srs).unwrap();
        assert_eq!(standard.len(), 12_385);
        assert_eq!(torus.len(), 6_625);
        assert_eq!(standard.len() - torus.len(), 20 * 288);

        assert!(matches!(
            verify_family_aggregate_torus(&statement, &pvk, &standard, &srs),
            Err(AggregateVerifyError::BadVersion(_))
        ));
        assert!(matches!(
            verify_family_aggregate(&statement, &pvk, &torus, &srs),
            Err(AggregateVerifyError::BadVersion(_))
        ));

        let inner =
            decode_wrapped_torus_aggregate_proof(&torus, statement.statement_digest(), None)
                .unwrap();
        let inner_offset = torus.len() - inner.len();
        let replacement = compressed_bytes(&Fp::from(1u64));
        assert_eq!(replacement.len(), 48);
        let mut non_subgroup_coordinate = torus.clone();
        assert_ne!(
            &non_subgroup_coordinate[inner_offset..inner_offset + replacement.len()],
            replacement.as_slice()
        );
        non_subgroup_coordinate[inner_offset..inner_offset + replacement.len()]
            .copy_from_slice(&replacement);
        assert!(matches!(
            verify_family_aggregate_torus(&statement, &pvk, &non_subgroup_coordinate, &srs,),
            Err(AggregateVerifyError::MalformedProofBytes(_))
        ));
    }

    #[test]
    fn shipping_backend_result_materializes_the_exact_authenticated_input() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let wrapped = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let inner = decode_wrapped_aggregate_proof(&wrapped, statement.statement_digest(), None)
            .expect("wrapper should decode");
        let mut serialized_vk = Vec::new();
        pvk.vk
            .serialize_compressed(&mut serialized_vk)
            .expect("VK should serialize");
        let application_call = shipping_call(family_id, items.len(), padded_items.len());
        let verified = SnarkpackBackend::verify_shipping_family_aggregate(
            application_call,
            &statement,
            &pvk,
            &wrapped,
            &srs,
        )
        .expect("shipping aggregate should verify");

        assert!(verified.shipping_result().result.accepted);
        let observation = verified.shipping_observation();
        assert_eq!(observation.call_id(), application_call.id);
        assert!(observation.accepted());
        assert_eq!(
            observation.challenge_context(),
            statement.challenge_context()
        );
        let input = &verified.shipping_result().input;
        assert_eq!(input.family, crate::app_verify_family_code(family_id));
        assert_eq!(input.protocol_version, AGGREGATE_PROTOCOL_VERSION);
        assert_eq!(input.srs_id, statement.srs_id());
        assert_eq!(input.serialized_vk, serialized_vk);
        assert_eq!(input.vk_digest, statement.vk_digest());
        assert_eq!(input.real_count, statement.real_count());
        assert_eq!(input.padded_count, statement.padded_count());
        let statement_rows = statement.shipping_rows();
        assert_eq!(input.public_input_arity, statement_rows.public_input_arity);
        assert_eq!(
            input.padded_public_inputs,
            statement_rows.serialized.to_nested_bytes()
        );
        assert_eq!(input.canonical_statement_bytes, statement.canonical_bytes());
        assert_eq!(input.statement_digest, statement.statement_digest());
        assert_eq!(input.wrapped_proof_bytes, wrapped);
        assert_eq!(input.inner_proof_bytes, inner);
        assert_eq!(
            input.challenge_context,
            statement.challenge_context().as_bytes()
        );

        let construction = verified.statement_construction();
        let binding = &construction.binding_execution;
        assert_eq!(binding.statement, statement.identity());
        assert_eq!(
            binding.wrapper.expected_statement_digest,
            input.statement_digest
        );
        assert_eq!(
            binding.wrapper.wrapped_proof_bytes,
            input.wrapped_proof_bytes
        );
        assert_eq!(binding.wrapper.inner_proof_bytes, input.inner_proof_bytes);
        assert_eq!(construction.source_field_rows, padded_public_inputs(&items));
        assert_eq!(
            construction.prepared_serialized_rows.padded_public_inputs,
            statement.shipping_rows().serialized.to_nested_bytes()
        );
    }

    #[test]
    fn shipping_and_ordinary_routes_match_all_registered_families() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        for family_id in parity_families() {
            let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
            let wrapped = aggregate_family(&statement, &pvk, &padded_items, &srs)
                .expect("aggregation should succeed");
            SnarkpackBackend::verify_family_aggregate(&statement, &pvk, &wrapped, &srs)
                .expect("ordinary verification should run");
            let shipping = SnarkpackBackend::verify_shipping_family_aggregate(
                shipping_call(family_id, items.len(), padded_items.len()),
                &statement,
                &pvk,
                &wrapped,
                &srs,
            )
            .expect("shipping verification should run");

            assert_eq!(
                shipping.shipping_result().result.accepted,
                true,
                "shipping and ordinary digest routes diverged for {family_id:?}"
            );
            assert!(
                shipping.shipping_result().result.accepted,
                "valid aggregate must be accepted for {family_id:?}"
            );
        }
    }

    #[test]
    fn shipping_preflight_rejects_cross_call_substitution_after_wire_checks() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let wrapped = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let wrong_family =
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical);

        let error = SnarkpackBackend::verify_shipping_family_aggregate(
            shipping_call(wrong_family, items.len(), padded_items.len()),
            &statement,
            &pvk,
            &wrapped,
            &srs,
        )
        .expect_err("an unrelated application call must not materialize");
        assert_eq!(error, AggregateVerifyError::StatementDigestMismatch);

        let oversized = vec![0u8; MAX_AGGREGATE_PROOF_BYTES + 1];
        let error = SnarkpackBackend::verify_shipping_family_aggregate(
            shipping_call(wrong_family, items.len(), padded_items.len()),
            &statement,
            &pvk,
            &oversized,
            &srs,
        )
        .expect_err("wire-size rejection must retain its earlier failure position");
        assert!(matches!(error, AggregateVerifyError::OversizeBytes(_)));
    }

    #[test]
    fn strict_boundary_rejects_nested_component_aliases_and_malformed_bytes() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let statement = statement_for_items(
            ProofFamilyId::Transfer,
            &pvk,
            items.len(),
            &padded_items,
            &srs,
        );
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let inner = decode_wrapped_aggregate_proof(
            &aggregate,
            statement.statement_digest(),
            Some(MAX_AGGREGATE_PROOF_BYTES),
        )
        .expect("aggregate wrapper should decode");

        let decoded = deserialize_aggregate_proof::<TransferTranscriptDigest>(&inner)
            .expect("valid aggregate should decode through the reached boundary");
        fn assert_validated_boundary<D: Digest + Send + Sync>(
            _: &ValidatedAggregateProof<Bls12_377, D>,
        ) {
        }
        assert_validated_boundary(&decoded);

        let reject = |mutated_inner: Vec<u8>, label: &str| {
            let mutated =
                encode_wrapped_aggregate_proof(statement.statement_digest(), &mutated_inner)
                    .expect("mutated aggregate should fit wrapper");
            let err = verify_family_aggregate(&statement, &pvk, &mutated, &srs).expect_err(label);
            assert!(
                matches!(err, AggregateVerifyError::MalformedProofBytes(_)),
                "{label}: unexpected error {err:?}"
            );
        };

        let gt_len = compressed_bytes(&PairingOutput::<Bls12_377>::zero()).len();
        let g2_len = compressed_bytes(&BackendG2::zero().into_affine()).len();
        let rounds_offset = 4 * gt_len + g1_compressed_len();
        let rounds = u64::from_le_bytes(
            inner[rounds_offset..rounds_offset + 8]
                .try_into()
                .expect("round count frame should be eight bytes"),
        ) as usize;
        let final_g2_offset = rounds_offset + 8 + rounds * 4736;
        let first_ab_identity_len_offset = rounds_offset + 8 + 2 * gt_len;

        // Identity commitments are wire-level vectors, but the verifier
        // accepts only the singleton shape emitted by the prover. Both of
        // these encodings are canonically decodable and therefore exercise
        // the post-decode shape gate rather than the strict byte decoder.
        let mut zero_identity = inner.to_vec();
        zero_identity[first_ab_identity_len_offset..first_ab_identity_len_offset + 8]
            .copy_from_slice(&0u64.to_le_bytes());
        zero_identity
            .drain(first_ab_identity_len_offset + 8..first_ab_identity_len_offset + 8 + gt_len);
        assert!(
            AggregateProof::<Bls12_377, TransferTranscriptDigest>::deserialize_compressed(
                &zero_identity[..]
            )
            .is_ok(),
            "zero-length identity output remains canonically decodable"
        );
        let mut malformed_trace = VecChallengeTraceSink::default();
        let shape_error = verify_with_digest_with_trace::<TransferTranscriptDigest, _>(
            statement.challenge_context(),
            &mut malformed_trace,
            &pvk,
            &zero_identity,
            statement.padded_public_inputs(),
            &srs,
        )
        .expect_err("shape validation must reject before verification");
        assert!(matches!(
            shape_error,
            AggregateVerifyError::MalformedProofBytes(_)
        ));
        assert!(
            malformed_trace.entries().is_empty(),
            "shape validation must run before challenge hashing"
        );
        reject(
            zero_identity,
            "zero-length identity output should be malformed proof bytes",
        );

        let mut multi_identity = inner.to_vec();
        multi_identity[first_ab_identity_len_offset..first_ab_identity_len_offset + 8]
            .copy_from_slice(&2u64.to_le_bytes());
        let identity_element = multi_identity
            [first_ab_identity_len_offset + 8..first_ab_identity_len_offset + 8 + gt_len]
            .to_vec();
        multi_identity.splice(
            first_ab_identity_len_offset + 8 + gt_len..first_ab_identity_len_offset + 8 + gt_len,
            identity_element,
        );
        assert!(
            AggregateProof::<Bls12_377, TransferTranscriptDigest>::deserialize_compressed(
                &multi_identity[..]
            )
            .is_ok(),
            "multi-element identity output remains canonically decodable"
        );
        reject(
            multi_identity,
            "multi-element identity output should be malformed proof bytes",
        );

        // A compressed flag mutation in the nested agg_c G1 component.
        let mut malformed_flags = inner.to_vec();
        malformed_flags[4 * gt_len + g1_compressed_len() - 1] |= 0xc0;
        reject(malformed_flags, "nested malformed G1 flags should reject");

        // The first GT lane is uncompressed Fq12. Replacing one 48-byte Fq
        // component with 0xff bytes is a noncanonical field encoding.
        let mut noncanonical_gt = inner.to_vec();
        noncanonical_gt[..48].fill(0xff);
        reject(
            noncanonical_gt,
            "nested noncanonical GT field should reject",
        );

        // The final commitment-key G2 is nested after the round vector. An
        // infinity alias is accepted by arkworks' permissive decoder but not
        // by the strict wrapper's exact re-serialization check.
        let mut g2_infinity_alias = inner.to_vec();
        let g2_zero = compressed_bytes(&BackendG2::zero().into_affine());
        let mut g2_alias = g2_zero;
        g2_alias[0] |= 1;
        g2_infinity_alias[final_g2_offset..final_g2_offset + g2_len].copy_from_slice(&g2_alias);
        assert!(
            AggregateProof::<Bls12_377, TransferTranscriptDigest>::deserialize_compressed(
                &g2_infinity_alias[..]
            )
            .is_ok()
        );
        reject(g2_infinity_alias, "nested G2 infinity alias should reject");

        // The first four fields are pairing outputs. The following `agg_c` is
        // a compressed G1 nested inside AggregateProof; retain its x-coordinate
        // and set only the infinity flag (the compressed flag is already set).
        let agg_c_offset = 4 * compressed_bytes(&PairingOutput::<Bls12_377>::zero()).len();
        let mut mutated_inner = inner.to_vec();
        mutated_inner[agg_c_offset + g1_compressed_len() - 1] |= 0x40;
        assert!(deserialize_aggregate_proof::<TransferTranscriptDigest>(&mutated_inner).is_err());
        reject(mutated_inner, "nested G1 infinity alias should reject");

        let mut non_subgroup = inner.to_vec();
        let torsion = non_subgroup_g1(&mut ChaCha20Rng::seed_from_u64(0x14));
        non_subgroup[4 * gt_len..4 * gt_len + g1_compressed_len()]
            .copy_from_slice(&compressed_bytes(&torsion));
        reject(non_subgroup, "nested non-subgroup G1 should reject");

        let mut non_subgroup = inner.to_vec();
        let torsion = non_subgroup_g2(&mut ChaCha20Rng::seed_from_u64(0x15));
        non_subgroup[final_g2_offset..final_g2_offset + g2_len]
            .copy_from_slice(&compressed_bytes(&torsion));
        reject(non_subgroup, "nested non-subgroup G2 should reject");

        let mut trailing = inner.to_vec();
        trailing.push(0);
        reject(trailing, "aggregate trailing bytes should reject");
    }

    fn g1_compressed_len() -> usize {
        compressed_bytes(&BackendG1::zero().into_affine()).len()
    }
    #[test]
    fn snarkpack_backend_accepts_valid_shielded_ics20_withdrawal_aggregate() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id =
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical);
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");

        verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
            .expect("shielded ICS-20 withdrawal aggregate verification should succeed");
    }

    #[test]
    fn snarkpack_backend_rejects_malformed_aggregate_bytes() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let mut aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        aggregate.truncate(aggregate.len() / 2);

        let err = verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
            .expect_err("malformed aggregate bytes should be rejected");

        assert!(matches!(err, AggregateVerifyError::MalformedProofBytes(_)));
    }

    #[test]
    fn malformed_aggregate_proof_oversize_rejected_before_deserialization() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let oversized = vec![0u8; MAX_AGGREGATE_PROOF_BYTES + 1024];

        let err = verify_family_aggregate(&statement, &pvk, &oversized, &srs)
            .expect_err("oversized aggregate proof should reject before deserialization");

        assert!(matches!(err, AggregateVerifyError::OversizeBytes(_)));
    }

    #[test]
    fn snarkpack_backend_rejects_mutated_public_inputs() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let mut padded_public_inputs = padded_public_inputs(&padded_items);
        padded_public_inputs[0][0] += Fq::from(1u64);
        let mutated_statement =
            statement_for_public_inputs(family_id, &pvk, items.len(), &padded_public_inputs, &srs);

        let err = verify_family_aggregate(&mutated_statement, &pvk, &aggregate, &srs)
            .expect_err("mutated public inputs should be rejected");

        assert_eq!(err, AggregateVerifyError::StatementDigestMismatch);
    }

    #[test]
    fn snarkpack_backend_rejects_wrong_family_id() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let wrong_statement = statement_for_items(
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
            &pvk,
            items.len(),
            &padded_items,
            &srs,
        );

        let err = verify_family_aggregate(&wrong_statement, &pvk, &aggregate, &srs)
            .expect_err("family transcript mismatch should be rejected");

        assert_eq!(err, AggregateVerifyError::StatementDigestMismatch);
    }

    #[test]
    fn statement_rejects_mutated_srs_id() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let padded_inputs = padded_public_inputs(&padded_items);
        let mut wrong_srs_id = srs_id(&srs);
        wrong_srs_id[0] ^= 0x01;
        let wrong_statement = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            family_id,
            wrong_srs_id,
            &pvk,
            items.len() as u32,
            &padded_inputs,
        )
        .expect("wrong-SRS statement should still be structurally valid");

        let err = verify_family_aggregate(&wrong_statement, &pvk, &aggregate, &srs)
            .expect_err("SRS id mutation should reject before backend verification");

        assert_eq!(err, AggregateVerifyError::StatementDigestMismatch);
    }

    #[test]
    fn statement_mismatch_rejects_vk_digest_mutation_before_backend() {
        let (pvk, items) = sample_items();
        let (wrong_pvk, _) = sample_items_with_count(99, 1);
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let wrong_statement =
            statement_for_items(family_id, &wrong_pvk, items.len(), &padded_items, &srs);

        let err = verify_family_aggregate(&wrong_statement, &pvk, &aggregate, &srs)
            .expect_err("VK digest mutation should reject before backend verification");

        assert_eq!(err, AggregateVerifyError::StatementDigestMismatch);
    }

    #[test]
    fn shipping_preflight_preserves_srs_wrapper_and_vk_failure_order() {
        let (pvk, items) = sample_items();
        let (wrong_pvk, _) = sample_items_with_count(99, 1);
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let wrapped = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");
        let oversized = vec![0u8; MAX_AGGREGATE_PROOF_BYTES + 1];

        let mut wrong_srs_id = srs_id(&srs);
        wrong_srs_id[0] ^= 0x01;
        let wrong_srs_statement = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            family_id,
            wrong_srs_id,
            &pvk,
            items.len() as u32,
            &padded_public_inputs(&padded_items),
        )
        .expect("wrong-SRS statement should remain structurally valid");

        let srs_before_wrapper = match preflight_aggregate_verify(AggregatePreflightInput {
            statement: &wrong_srs_statement,
            pvk: &wrong_pvk,
            aggregate_proof_bytes: &oversized,
            srs: &srs,
        }) {
            Ok(_) => panic!("SRS mismatch must reject before wrapper and VK work"),
            Err(error) => error,
        };
        assert_eq!(
            srs_before_wrapper,
            AggregateVerifyError::StatementDigestMismatch
        );

        let wrapper_before_vk = match preflight_aggregate_verify(AggregatePreflightInput {
            statement: &statement,
            pvk: &wrong_pvk,
            aggregate_proof_bytes: &oversized,
            srs: &srs,
        }) {
            Ok(_) => panic!("oversize wrapper must reject before VK serialization and equality"),
            Err(error) => error,
        };
        assert!(matches!(
            wrapper_before_vk,
            AggregateVerifyError::OversizeBytes(_)
        ));

        let vk_after_wrapper = match preflight_aggregate_verify(AggregatePreflightInput {
            statement: &statement,
            pvk: &wrong_pvk,
            aggregate_proof_bytes: &wrapped,
            srs: &srs,
        }) {
            Ok(_) => panic!("wrong VK must reject after a valid wrapper decode"),
            Err(error) => error,
        };
        assert_eq!(
            vk_after_wrapper,
            AggregateVerifyError::StatementDigestMismatch
        );
    }

    #[test]
    #[ignore = "release-mode CI gate; run with `just snarkpack-dos-gate`"]
    fn snarkpack_dos_gate_valid_and_adversarial_paths_hold_thresholds() {
        const SAMPLES: usize = 5;
        const VALID_P99_MAX_MS: f64 = 1_500.0;
        const MIXED_P99_MAX_MS: f64 = 1_500.0;
        const CHEAP_REJECT_P99_MAX_MS: f64 = 25.0;
        const CHEAP_REJECT_VALID_RATIO: f64 = 0.50;

        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");

        for _ in 0..2 {
            verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
                .expect("warmup valid aggregate verification should succeed");
        }

        let mut mutated_inputs = padded_public_inputs(&padded_items);
        mutated_inputs[0][0] += Fq::from(1u64);
        let wrong_public_input_statement =
            statement_for_public_inputs(family_id, &pvk, items.len(), &mutated_inputs, &srs);
        let wrong_family_statement = statement_for_items(
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
            &pvk,
            items.len(),
            &padded_items,
            &srs,
        );
        let malformed_len = aggregate.len().min(8);
        let rejection_cases = vec![
            RejectionCase {
                name: "malformed-wrapper",
                statement: statement.clone(),
                aggregate_bytes: aggregate[..malformed_len].to_vec(),
                expected: ExpectedRejection::MalformedProofBytes,
            },
            RejectionCase {
                name: "wrong-family",
                statement: wrong_family_statement,
                aggregate_bytes: aggregate.clone(),
                expected: ExpectedRejection::StatementDigestMismatch,
            },
            RejectionCase {
                name: "wrong-public-input",
                statement: wrong_public_input_statement,
                aggregate_bytes: aggregate.clone(),
                expected: ExpectedRejection::StatementDigestMismatch,
            },
            RejectionCase {
                name: "oversized-wrapper",
                statement: statement.clone(),
                aggregate_bytes: vec![0u8; MAX_AGGREGATE_PROOF_BYTES + 1],
                expected: ExpectedRejection::OversizeBytes,
            },
        ];

        for case in &rejection_cases {
            let err = match preflight_aggregate_verify(AggregatePreflightInput {
                statement: &case.statement,
                pvk: &pvk,
                aggregate_proof_bytes: &case.aggregate_bytes,
                srs: &srs,
            }) {
                Ok(_) => panic!("adversarial input must fail before backend verification"),
                Err(err) => err,
            };
            assert_expected_rejection(&err, case.expected);
        }

        let mut valid_samples = Vec::with_capacity(SAMPLES);
        let mut mixed_samples = Vec::with_capacity(SAMPLES * (rejection_cases.len() + 1));
        for _ in 0..SAMPLES {
            let started = Instant::now();
            verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
                .expect("valid aggregate verification should succeed");
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            valid_samples.push(elapsed_ms);
            mixed_samples.push(elapsed_ms);
        }

        let valid_stats = timing_stats(&valid_samples);
        assert!(
            valid_stats.p99_ms <= VALID_P99_MAX_MS,
            "valid p99 {:.3}ms exceeds {:.3}ms",
            valid_stats.p99_ms,
            VALID_P99_MAX_MS
        );

        for case in &rejection_cases {
            let mut samples = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let started = Instant::now();
                let err =
                    verify_family_aggregate(&case.statement, &pvk, &case.aggregate_bytes, &srs)
                        .expect_err("adversarial aggregate should reject");
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                assert_expected_rejection(&err, case.expected);
                samples.push(elapsed_ms);
                mixed_samples.push(elapsed_ms);
            }

            let reject_stats = timing_stats(&samples);
            assert!(
                reject_stats.p99_ms <= CHEAP_REJECT_P99_MAX_MS,
                "{name} reject p99 {p99:.3}ms exceeds {limit:.3}ms",
                name = case.name,
                p99 = reject_stats.p99_ms,
                limit = CHEAP_REJECT_P99_MAX_MS
            );
            assert!(
                reject_stats.p99_ms <= valid_stats.p50_ms * CHEAP_REJECT_VALID_RATIO,
                "{name} reject p99 {reject:.3}ms is not cheaply below valid p50 {valid:.3}ms",
                name = case.name,
                reject = reject_stats.p99_ms,
                valid = valid_stats.p50_ms
            );
            println!(
                "snarkpack-dos-gate {name}: p50={p50:.3}ms p95={p95:.3}ms p99={p99:.3}ms",
                name = case.name,
                p50 = reject_stats.p50_ms,
                p95 = reject_stats.p95_ms,
                p99 = reject_stats.p99_ms
            );
        }

        let mixed_stats = timing_stats(&mixed_samples);
        assert!(
            mixed_stats.p99_ms <= MIXED_P99_MAX_MS,
            "mixed workload p99 {:.3}ms exceeds {:.3}ms",
            mixed_stats.p99_ms,
            MIXED_P99_MAX_MS
        );

        println!(
            "snarkpack-dos-gate valid: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
            valid_stats.p50_ms, valid_stats.p95_ms, valid_stats.p99_ms
        );
        println!(
            "snarkpack-dos-gate mixed: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
            mixed_stats.p50_ms, mixed_stats.p95_ms, mixed_stats.p99_ms
        );
    }

    #[test]
    fn prover_verifier_acceptance_parity() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);

        let mut prover_trace = VecChallengeTraceSink::default();
        let inner_aggregate =
            aggregate_with_digest_with_trace_and_real_count::<TransferTranscriptDigest, _>(
                statement.challenge_context(),
                &mut prover_trace,
                &padded_items,
                statement.real_count() as usize,
                &srs,
            )
            .expect("aggregation should succeed");
        let mut verifier_trace = VecChallengeTraceSink::default();
        let accepted = verify_with_digest_with_trace::<TransferTranscriptDigest, _>(
            statement.challenge_context(),
            &mut verifier_trace,
            &pvk,
            &inner_aggregate,
            statement.padded_public_inputs(),
            &srs,
        )
        .expect("verification should run");

        assert!(
            accepted,
            "aggregate verification should accept the traced aggregate"
        );
        assert!(
            !prover_trace.entries().is_empty(),
            "challenge trace should not be empty"
        );
        assert_eq!(prover_trace.entries(), verifier_trace.entries());
    }

    #[test]
    fn snarkpack_matches_single_and_batch_groth16_oracles() {
        groth16_oracle_agreement_for_counts(&[1, 2, 4, 8]);
    }

    #[test]
    #[ignore]
    fn snarkpack_matches_single_and_batch_groth16_oracles_slow() {
        groth16_oracle_agreement_for_counts(&[64, 256, 1024]);
    }

    #[test]
    fn aggregation_is_deterministic_for_fixed_inputs() {
        let (pvk, items) = sample_items_with_count(31, 8);
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let statement = statement_for_items(
            ProofFamilyId::Transfer,
            &pvk,
            items.len(),
            &padded_items,
            &srs,
        );

        let first = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("first aggregation should succeed");
        let second = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("second aggregation should succeed");

        assert_eq!(
            first, second,
            "fixed aggregate inputs must produce stable bytes"
        );
    }

    // Stage 9 byte-trace lock: the committed aggregate-proof bytes for a fixed
    // vector set must not change unless `AGGREGATE_PROTOCOL_VERSION` is bumped.
    // An optimization that alters these bytes is a protocol change, not a
    // refactor, and must take the version-bump path (see docs/snarkpack/verification.md).

    const BYTE_BASELINE_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/aggregate_bytes_baseline.txt"
    );
    const TRACE_BASELINE_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/shieldd_byte_trace_baseline.txt"
    );

    fn baseline_vectors() -> Vec<(ProofFamilyId, usize, u64)> {
        let mut vectors = Vec::new();
        for family_id in parity_families() {
            // Keep each family's seed independent so byte drift remains local.
            let seed_base = match family_id {
                ProofFamilyId::Transfer => 9_000,
                ProofFamilyId::NoteReshape(family_id) => 9_000 + u64::from(family_id.get()) * 100,
                ProofFamilyId::ShieldedIcs20Withdrawal(_) => 9_500,
            };
            for count in [1usize, 2, 4, 8] {
                let seed = seed_base + count as u64;
                vectors.push((family_id, count, seed));
            }
        }
        vectors
    }

    fn family_token(family_id: ProofFamilyId) -> String {
        format!("{family_id:?}")
            .chars()
            .map(|c| if c.is_whitespace() { '_' } else { c })
            .collect()
    }

    fn aggregate_bytes_for_vector(family_id: ProofFamilyId, count: usize, seed: u64) -> Vec<u8> {
        let (pvk, items) = sample_items_with_count(seed, count);
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        aggregate_family(&statement, &pvk, &padded_items, &srs).expect("aggregation should succeed")
    }

    fn render_byte_baseline() -> String {
        let mut out = String::new();
        out.push_str("# SnarkPack aggregate-proof byte baseline (Stage 9 byte-trace lock).\n");
        out.push_str("# Regenerate: cargo test -p shieldd-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored\n");
        out.push_str("# Row: <index> <family> count=<n> seed=<n> <hex_aggregate_bytes>\n");
        out.push_str(&format!("version {AGGREGATE_PROTOCOL_VERSION}\n"));
        for (index, (family_id, count, seed)) in baseline_vectors().into_iter().enumerate() {
            let bytes = aggregate_bytes_for_vector(family_id, count, seed);
            out.push_str(&format!(
                "{index} {} count={count} seed={seed} {}\n",
                family_token(family_id),
                hex::encode(&bytes),
            ));
        }
        out
    }

    fn challenge_trace_for_digest<D>(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        padded_items: &[BatchItem],
        srs: &DevSrs,
        expected_inner: &[u8],
    ) -> Vec<ChallengeTraceEntry>
    where
        D: Digest + Send + Sync,
    {
        let mut prover_trace = VecChallengeTraceSink::default();
        let traced_inner = aggregate_with_digest_with_trace_and_real_count::<D, _>(
            statement.challenge_context(),
            &mut prover_trace,
            padded_items,
            statement.real_count() as usize,
            srs,
        )
        .expect("traced aggregation should succeed");
        assert_eq!(
            traced_inner, expected_inner,
            "traced core must produce the inner proof wrapped by production"
        );

        let mut verifier_trace = VecChallengeTraceSink::default();
        let accepted = verify_with_digest_with_trace::<D, _>(
            statement.challenge_context(),
            &mut verifier_trace,
            pvk,
            &traced_inner,
            statement.padded_public_inputs(),
            srs,
        )
        .expect("traced verification should run");
        assert!(accepted, "traced verification should accept");
        assert_eq!(
            prover_trace.entries(),
            verifier_trace.entries(),
            "prover and verifier must consume identical ordered transcript bytes"
        );
        prover_trace.into_entries()
    }

    fn challenge_trace_for_vector(
        family_id: ProofFamilyId,
        count: usize,
        seed: u64,
    ) -> Vec<ChallengeTraceEntry> {
        let (pvk, items) = sample_items_with_count(seed, count);
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let wrapped = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("production aggregation should succeed");
        let inner = decode_wrapped_aggregate_proof(&wrapped, statement.statement_digest(), None)
            .expect("production wrapper should decode");

        match family_id {
            ProofFamilyId::Transfer => challenge_trace_for_digest::<TransferTranscriptDigest>(
                &statement,
                &pvk,
                &padded_items,
                &srs,
                inner,
            ),
            ProofFamilyId::NoteReshape(NoteReshapeFamilyId::OneByEight) => {
                challenge_trace_for_digest::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
                >(&statement, &pvk, &padded_items, &srs, inner)
            }
            ProofFamilyId::NoteReshape(NoteReshapeFamilyId::EightByOne) => {
                challenge_trace_for_digest::<
                    NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
                >(&statement, &pvk, &padded_items, &srs, inner)
            }
            ProofFamilyId::NoteReshape(other) => {
                panic!("unregistered note reshape baseline family {}", other.get())
            }
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                challenge_trace_for_digest::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    &statement,
                    &pvk,
                    &padded_items,
                    &srs,
                    inner,
                )
            }
        }
    }

    fn render_trace_baseline() -> String {
        let mut out = String::new();
        out.push_str("# SnarkPack prover/verifier challenge-byte trace baseline.\n");
        out.push_str("# Regenerate: cargo test -p shieldd-sdk-proof-aggregation regenerate_shieldd_byte_trace_baseline -- --ignored\n");
        out.push_str(
            "# Row: <vector>.<entry> stage=<hex> nonce=<u64> preimage=<hex> digest=<hex>\n",
        );
        out.push_str(&format!("version {AGGREGATE_PROTOCOL_VERSION}\n"));
        for (vector, (family_id, count, seed)) in baseline_vectors().into_iter().enumerate() {
            let entries = challenge_trace_for_vector(family_id, count, seed);
            out.push_str(&format!(
                "vector {vector} {} count={count} seed={seed} entries={}\n",
                family_token(family_id),
                entries.len(),
            ));
            for (entry_index, entry) in entries.into_iter().enumerate() {
                out.push_str(&format!(
                    "{vector}.{entry_index} stage={} nonce={} preimage={} digest={}\n",
                    hex::encode(entry.stage_label),
                    entry.nonce,
                    hex::encode(entry.preimage),
                    hex::encode(entry.digest),
                ));
            }
        }
        out
    }

    fn committed_baseline_version(contents: &str) -> Option<u32> {
        contents
            .lines()
            .find_map(|line| line.strip_prefix("version "))
            .and_then(|v| v.trim().parse().ok())
    }

    fn assert_aggregate_bytes_match_committed_baseline() {
        let committed = std::fs::read_to_string(BYTE_BASELINE_PATH).unwrap_or_else(|e| {
            panic!("missing aggregate byte baseline at {BYTE_BASELINE_PATH}: {e}; regenerate with `cargo test -p shieldd-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored`")
        });

        // The baseline must identify the same consensus aggregation protocol.
        let committed_version = committed_baseline_version(&committed);
        assert_eq!(
            committed_version,
            Some(AGGREGATE_PROTOCOL_VERSION),
            "byte baseline version tag does not match AGGREGATE_PROTOCOL_VERSION ({AGGREGATE_PROTOCOL_VERSION}); if the byte change is intentional, bump the version and regenerate via `cargo test -p shieldd-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored`"
        );

        let current = render_byte_baseline();
        assert_eq!(
            committed, current,
            "aggregate-proof bytes drifted from the committed baseline. An optimization must preserve bytes or follow the versioning procedure in docs/snarkpack/verification.md#x3--optimization-byte-lock."
        );
    }

    #[test]
    #[ignore = "writes the committed byte baseline; run intentionally after a sanctioned byte change"]
    fn regenerate_aggregate_byte_baseline() {
        let rendered = render_byte_baseline();
        std::fs::write(BYTE_BASELINE_PATH, rendered).expect("write byte baseline");
    }

    fn assert_shieldd_byte_trace_matches_committed_baseline() {
        let committed = std::fs::read_to_string(TRACE_BASELINE_PATH).unwrap_or_else(|e| {
            panic!("missing challenge trace baseline at {TRACE_BASELINE_PATH}: {e}; regenerate with `cargo test -p shieldd-sdk-proof-aggregation regenerate_shieldd_byte_trace_baseline -- --ignored`")
        });
        assert_eq!(
            committed_baseline_version(&committed),
            Some(AGGREGATE_PROTOCOL_VERSION),
            "trace baseline version must match AGGREGATE_PROTOCOL_VERSION"
        );
        assert_eq!(
            committed,
            render_trace_baseline(),
            "challenge transcript bytes drifted from the committed baseline; follow docs/snarkpack/verification.md#x3--optimization-byte-lock"
        );
    }

    #[test]
    fn aggregate_bytes_and_transcript_match_committed_baselines() {
        assert_aggregate_bytes_match_committed_baseline();
        assert_shieldd_byte_trace_matches_committed_baseline();
    }

    #[test]
    #[ignore = "writes the committed challenge trace baseline after a sanctioned protocol change"]
    fn regenerate_shieldd_byte_trace_baseline() {
        let rendered = render_trace_baseline();
        std::fs::write(TRACE_BASELINE_PATH, rendered).expect("write trace baseline");
    }

    #[test]
    fn preflight_rejects_oversize_before_inner_deserialization() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let statement = statement_for_items(
            ProofFamilyId::Transfer,
            &pvk,
            items.len(),
            &padded_items,
            &srs,
        );
        let oversized = vec![0u8; MAX_AGGREGATE_PROOF_BYTES + 1];

        let result = preflight_aggregate_verify(AggregatePreflightInput {
            statement: &statement,
            pvk: &pvk,
            aggregate_proof_bytes: &oversized,
            srs: &srs,
        });
        let err = match result {
            Ok(_) => panic!("oversize preflight input must reject"),
            Err(err) => err,
        };

        assert!(matches!(err, AggregateVerifyError::OversizeBytes(_)));
    }

    #[test]
    #[ignore]
    fn aggregate_proof_size_report() {
        let (pvk, items) = sample_items();
        let base_item = items.first().expect("at least one sample item").clone();
        let srs = DevSrs::default();
        let mut observed_max = 0usize;

        for family_id in parity_families() {
            for count in [1usize, 2, 4, 8, 64] {
                let repeated = vec![base_item.clone(); count];
                let statement = statement_for_items(family_id, &pvk, count, &repeated, &srs);
                let aggregate = aggregate_family(&statement, &pvk, &repeated, &srs)
                    .expect("aggregation should succeed");
                observed_max = observed_max.max(aggregate.len());
                println!(
                    "snarkpack_size family={family_id:?} count={count} wrapped_bytes={}",
                    aggregate.len()
                );
            }
        }

        println!("snarkpack_size observed_max_wrapped_bytes={observed_max}");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(8))]

        #[test]
        fn snarkpack_property_matches_batch_oracle(
            count in 1usize..=8,
            seed in any::<u64>(),
            family_index in 0usize..parity_families().len(),
            mutate_proof in any::<bool>(),
        ) {
            let (pvk, items) = sample_items_with_count(seed, count);
            let srs = DevSrs::default();
            let padded_items = pad_items_to_power_of_two(&items, srs.max_padded_count as usize)
                .expect("padding should succeed");
            let family_id = parity_families()[family_index];

            prop_assert!(
                batch::batch_verify(&pvk, &padded_items).is_ok(),
                "batch verify should accept padded valid proofs"
            );

            let statement = statement_for_items(family_id, &pvk, count, &padded_items, &srs);
            let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
                .expect("aggregation should succeed");
            let padded_public_inputs = padded_public_inputs(&padded_items);

            prop_assert!(
                verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
                .is_ok(),
                "SnarkPack should accept the same valid padded batch"
            );

            let mut mutated_items = padded_items.clone();
            let mut mutated_public_inputs = padded_public_inputs;
            if mutate_proof {
                mutated_items[0].proof.c = Default::default();
            } else {
                mutated_items[0].public_inputs[0] += Fq::from(1u64);
                mutated_public_inputs[0][0] += Fq::from(1u64);
            }

            prop_assert!(
                batch::batch_verify(&pvk, &mutated_items).is_err(),
                "batch verify should reject the mutated batch"
            );

            let snarkpack_result = if mutate_proof {
                let mutated_aggregate = aggregate_family(&statement, &pvk, &mutated_items, &srs)
                    .expect("mutated proof aggregation should still serialize");
                verify_family_aggregate(&statement, &pvk, &mutated_aggregate, &srs)
            } else {
                let mutated_statement = statement_for_public_inputs(
                    family_id,
                    &pvk,
                    count,
                    &mutated_public_inputs,
                    &srs,
                );
                verify_family_aggregate(&mutated_statement, &pvk, &aggregate, &srs)
            };

            prop_assert!(
                snarkpack_result.is_err(),
                "SnarkPack should reject the same mutated batch"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]

        #[test]
        fn preflight_aggregate_verify_do_not_panic(
            bytes in prop::collection::vec(any::<u8>(), 0usize..=4096),
            seed in any::<u64>(),
        ) {
            let (pvk, items) = sample_items_with_count(seed, 2);
            let srs = DevSrs::default();
            let padded_items = pad_items_to_power_of_two(&items, srs.max_padded_count as usize)
                .expect("padding should succeed");
            let statement = statement_for_items(
                ProofFamilyId::Transfer,
                &pvk,
                items.len(),
                &padded_items,
                &srs,
            );

            let _ = preflight_aggregate_verify(AggregatePreflightInput {
                statement: &statement,
                pvk: &pvk,
                aggregate_proof_bytes: &bytes,
                srs: &srs,
            });
        }

        #[test]
        fn deserialize_aggregate_proof_do_not_panic(
            bytes in prop::collection::vec(any::<u8>(), 0usize..=4096),
        ) {
            let _ = deserialize_aggregate_proof::<TransferTranscriptDigest>(&bytes);
        }
    }
}
