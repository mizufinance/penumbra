use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{ensure, Result};
use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::applications::groth16_aggregation::{
    aggregate_proofs, aggregate_proofs_profiled, verify_aggregate_proof,
    verify_aggregate_proof_profiled, AggregateProof, AggregateProofBuildProfile,
    AggregateProofVerificationProfile,
};
use ark_ip_proofs::applications::groth16_aggregation::{
    aggregate_proofs_with_trace, verify_aggregate_proof_with_trace,
};
use ark_ip_proofs::challenge::{
    challenge_context_preimage, ChallengeContext, ChallengeTraceSink, VecChallengeTraceSink,
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::{Bls12_377, Fq};
use digest::Digest;
use penumbra_sdk_proof_aggregation_trace_schema::{
    TraceComparisonLevel, TraceEvent, TraceEventKind,
};
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_shielded_pool::{ConsolidateFamilyId, SplitFamilyId};

use crate::{
    aggregate_proof_wrapper::{
        encode_wrapped_aggregate_proof, AggregateProofBytesError, MAX_AGGREGATE_PROOF_BYTES,
    },
    preflight::{
        preflight_aggregate_verify, AggregatePreflightInput, VerifiedAggregateBackendCall,
    },
    srs::DevSrs,
    statement::{AggregateStatement, AggregateStatementError},
    transcript::{
        ConsolidateTranscriptDigest, ShieldedIcs20WithdrawalTranscriptDigest,
        SplitTranscriptDigest, TransferTranscriptDigest,
    },
    transfer_family_dispatch::{
        aggregate_transfer, aggregate_transfer_profiled, verify_transfer_aggregate,
        verify_transfer_aggregate_profiled_status,
    },
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateTraceReport {
    pub accepted: bool,
    pub trace: Vec<TraceEvent>,
}

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

#[derive(Clone, Debug)]
pub struct AggregateVerificationProfile {
    pub deserialize_ms: f64,
    pub challenge_ms: f64,
    pub tipa_ab_ms: f64,
    pub tipa_c_ms: f64,
    pub public_input_fold_ms: f64,
    pub ppe_ms: f64,
    pub core_total_ms: f64,
    pub total_ms: f64,
    pub accepted: bool,
}

impl Default for AggregateVerificationProfile {
    fn default() -> Self {
        Self {
            deserialize_ms: 0.0,
            challenge_ms: 0.0,
            tipa_ab_ms: 0.0,
            tipa_c_ms: 0.0,
            public_input_fold_ms: 0.0,
            ppe_ms: 0.0,
            core_total_ms: 0.0,
            total_ms: 0.0,
            accepted: true,
        }
    }
}

impl AggregateVerificationProfile {
    pub fn merge(&mut self, other: &Self) {
        self.deserialize_ms += other.deserialize_ms;
        self.challenge_ms += other.challenge_ms;
        self.tipa_ab_ms += other.tipa_ab_ms;
        self.tipa_c_ms += other.tipa_c_ms;
        self.public_input_fold_ms += other.public_input_fold_ms;
        self.ppe_ms += other.ppe_ms;
        self.core_total_ms += other.core_total_ms;
        self.total_ms += other.total_ms;
        self.accepted &= other.accepted;
    }
}

#[derive(Clone, Debug, Default)]
pub struct AggregateBuildBackendProfile {
    pub collect_proofs_ms: f64,
    pub backend_aggregate_ms: f64,
    pub backend_point_extract_ms: f64,
    pub backend_prepared_srs_ms: f64,
    pub backend_commitment_key_extract_ms: f64,
    pub backend_commitment_ms: f64,
    pub backend_com_a_ms: f64,
    pub backend_com_b_ms: f64,
    pub backend_com_c_ms: f64,
    pub backend_pairing_normalize_batch_ms: f64,
    pub backend_pairing_prepare_ms: f64,
    pub backend_pairing_miller_loop_ms: f64,
    pub backend_pairing_final_exponentiation_ms: f64,
    pub backend_randomizer_ms: f64,
    pub backend_structured_scalar_ms: f64,
    pub backend_weighted_a_ms: f64,
    pub backend_ip_ab_ms: f64,
    pub backend_agg_c_ms: f64,
    pub backend_ck_1_r_ms: f64,
    pub backend_consistency_check_ms: f64,
    pub backend_tipa_ab_ms: f64,
    pub backend_tipa_c_ms: f64,
    pub backend_tipa_ab_gipa_ms: f64,
    pub backend_tipa_ab_gipa_commit_l_ms: f64,
    pub backend_tipa_ab_gipa_commit_r_ms: f64,
    pub backend_tipa_ab_gipa_challenge_ms: f64,
    pub backend_tipa_ab_gipa_rescale_m1_ms: f64,
    pub backend_tipa_ab_gipa_rescale_m2_ms: f64,
    pub backend_tipa_ab_gipa_rescale_ck1_ms: f64,
    pub backend_tipa_ab_gipa_rescale_ck2_ms: f64,
    pub backend_tipa_ab_transcript_inverse_ms: f64,
    pub backend_tipa_ab_kzg_challenge_ms: f64,
    pub backend_tipa_ab_kzg_coefficient_build_ms: f64,
    pub backend_tipa_ab_kzg_eval_quotient_ms: f64,
    pub backend_tipa_ab_kzg_opening_msm_ms: f64,
    pub backend_tipa_ab_kzg_opening_ck_a_ms: f64,
    pub backend_tipa_ab_kzg_opening_ck_b_ms: f64,
    pub backend_tipa_c_gipa_ms: f64,
    pub backend_tipa_c_gipa_commit_l_ms: f64,
    pub backend_tipa_c_gipa_commit_r_ms: f64,
    pub backend_tipa_c_gipa_challenge_ms: f64,
    pub backend_tipa_c_gipa_rescale_m1_ms: f64,
    pub backend_tipa_c_gipa_rescale_m2_ms: f64,
    pub backend_tipa_c_gipa_rescale_ck1_ms: f64,
    pub backend_tipa_c_gipa_rescale_ck2_ms: f64,
    pub backend_tipa_c_transcript_inverse_ms: f64,
    pub backend_tipa_c_kzg_challenge_ms: f64,
    pub backend_tipa_c_kzg_coefficient_build_ms: f64,
    pub backend_tipa_c_kzg_eval_quotient_ms: f64,
    pub backend_tipa_c_kzg_opening_msm_ms: f64,
    pub backend_tipa_c_kzg_opening_ck_a_ms: f64,
    pub serialize_ms: f64,
    pub total_ms: f64,
}

pub trait AggregationBackend {
    type Srs;

    fn aggregate_family(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &Self::Srs,
    ) -> Result<Vec<u8>>;

    fn verify_family_aggregate(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &Self::Srs,
    ) -> Result<(), AggregateVerifyError>;
}

pub struct SnarkpackBackend;

/// Per-invocation rayon thread count. 1 = one dedicated thread per GIPA task (default).
/// 0 = use the global pool (all available threads shared across concurrent tasks).
static RAYON_THREADS_PER_BATCH: AtomicUsize = AtomicUsize::new(1);

/// Set the rayon thread count used per `aggregate_with_digest_profiled` call.
/// 1 is the production default (one dedicated thread per GIPA task, no cross-task stealing).
/// 0 falls back to the global pool.
pub fn set_rayon_threads_per_batch_for_bench(n: usize) {
    RAYON_THREADS_PER_BATCH.store(n, Ordering::Relaxed);
}

pub(crate) fn deserialize_aggregate_proof<D: Digest>(
    aggregate_proof_bytes: &[u8],
) -> Result<AggregateProof<Bls12_377, D>, AggregateVerifyError> {
    AggregateProof::<Bls12_377, D>::deserialize_compressed(&aggregate_proof_bytes[..])
        .map_err(|err| AggregateVerifyError::MalformedProofBytes(err.to_string()))
}

impl SnarkpackBackend {
    fn verify_transfer_family_aggregate_profiled_status(
        challenge_context: &ChallengeContext,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
        verify_transfer_aggregate_profiled_status(
            challenge_context,
            pvk,
            aggregate_proof_bytes,
            padded_public_inputs,
            srs,
        )
    }

    fn aggregate_transfer_family(
        challenge_context: &ChallengeContext,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>> {
        aggregate_transfer(challenge_context, items, srs)
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

    fn aggregate_transfer_family_profiled(
        items: &[BatchItem],
        srs: &DevSrs,
        challenge_context: &ChallengeContext,
    ) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
        aggregate_transfer_profiled(items, srs, challenge_context)
    }

    fn aggregate_transfer_family_with_trace<S>(
        challenge_context: &ChallengeContext,
        trace: &mut S,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>>
    where
        S: ChallengeTraceSink,
    {
        aggregate_with_digest_with_trace::<TransferTranscriptDigest, S>(
            challenge_context,
            trace,
            items,
            srs,
        )
    }

    fn aggregate_split_family_with_trace<S>(
        family_id: SplitFamilyId,
        challenge_context: &ChallengeContext,
        trace: &mut S,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>>
    where
        S: ChallengeTraceSink,
    {
        match family_id {
            SplitFamilyId::OneByFour => aggregate_with_digest_with_trace::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByFour.get() }>,
                S,
            >(challenge_context, trace, items, srs),
            SplitFamilyId::OneByEight => aggregate_with_digest_with_trace::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByEight.get() }>,
                S,
            >(challenge_context, trace, items, srs),
            other => Err(anyhow::anyhow!(
                "unknown split aggregate family {}",
                other.get()
            )),
        }
    }

    fn aggregate_consolidate_family_with_trace<S>(
        family_id: ConsolidateFamilyId,
        challenge_context: &ChallengeContext,
        trace: &mut S,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<Vec<u8>>
    where
        S: ChallengeTraceSink,
    {
        match family_id {
            ConsolidateFamilyId::TwoByOne => aggregate_with_digest_with_trace::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::TwoByOne.get() }>,
                S,
            >(challenge_context, trace, items, srs),
            ConsolidateFamilyId::FourByOne => aggregate_with_digest_with_trace::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::FourByOne.get() }>,
                S,
            >(challenge_context, trace, items, srs),
            ConsolidateFamilyId::EightByOne => aggregate_with_digest_with_trace::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::EightByOne.get() }>,
                S,
            >(challenge_context, trace, items, srs),
            other => Err(anyhow::anyhow!(
                "unknown consolidate aggregate family {}",
                other.get()
            )),
        }
    }

    fn aggregate_split_family_profiled(
        family_id: SplitFamilyId,
        items: &[BatchItem],
        srs: &DevSrs,
        challenge_context: &ChallengeContext,
    ) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
        match family_id {
            SplitFamilyId::OneByFour => aggregate_with_digest_profiled::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByFour.get() }>,
            >(items, srs, challenge_context),
            SplitFamilyId::OneByEight => aggregate_with_digest_profiled::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByEight.get() }>,
            >(items, srs, challenge_context),
            other => Err(anyhow::anyhow!(
                "unknown split aggregate family {}",
                other.get()
            )),
        }
    }

    fn aggregate_consolidate_family_profiled(
        family_id: ConsolidateFamilyId,
        items: &[BatchItem],
        srs: &DevSrs,
        challenge_context: &ChallengeContext,
    ) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
        match family_id {
            ConsolidateFamilyId::TwoByOne => aggregate_with_digest_profiled::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::TwoByOne.get() }>,
            >(items, srs, challenge_context),
            ConsolidateFamilyId::FourByOne => aggregate_with_digest_profiled::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::FourByOne.get() }>,
            >(items, srs, challenge_context),
            ConsolidateFamilyId::EightByOne => aggregate_with_digest_profiled::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::EightByOne.get() }>,
            >(items, srs, challenge_context),
            other => Err(anyhow::anyhow!(
                "unknown consolidate aggregate family {}",
                other.get()
            )),
        }
    }

    fn verify_split_family_aggregate_profiled_status(
        challenge_context: &ChallengeContext,
        family_id: SplitFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
        match family_id {
            SplitFamilyId::OneByFour => verify_with_digest_profiled::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByFour.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            SplitFamilyId::OneByEight => verify_with_digest_profiled::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByEight.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            other => Err(AggregateVerifyError::BadVersion(format!(
                "unknown split aggregate family {}",
                other.get()
            ))),
        }
    }

    fn verify_split_family_aggregate_with_trace<S>(
        challenge_context: &ChallengeContext,
        trace: &mut S,
        family_id: SplitFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<bool, AggregateVerifyError>
    where
        S: ChallengeTraceSink,
    {
        match family_id {
            SplitFamilyId::OneByFour => verify_with_digest_with_trace::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByFour.get() }>,
                S,
            >(
                challenge_context,
                trace,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            SplitFamilyId::OneByEight => verify_with_digest_with_trace::<
                SplitTranscriptDigest<{ SplitFamilyId::OneByEight.get() }>,
                S,
            >(
                challenge_context,
                trace,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            other => Err(AggregateVerifyError::BadVersion(format!(
                "unknown split aggregate family {}",
                other.get()
            ))),
        }
    }

    fn verify_consolidate_family_aggregate_profiled_status(
        challenge_context: &ChallengeContext,
        family_id: ConsolidateFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
        match family_id {
            ConsolidateFamilyId::TwoByOne => verify_with_digest_profiled::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::TwoByOne.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ConsolidateFamilyId::FourByOne => verify_with_digest_profiled::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::FourByOne.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ConsolidateFamilyId::EightByOne => verify_with_digest_profiled::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::EightByOne.get() }>,
            >(
                challenge_context,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            other => Err(AggregateVerifyError::BadVersion(format!(
                "unknown consolidate aggregate family {}",
                other.get()
            ))),
        }
    }

    fn verify_consolidate_family_aggregate_with_trace<S>(
        challenge_context: &ChallengeContext,
        trace: &mut S,
        family_id: ConsolidateFamilyId,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        padded_public_inputs: &[Vec<Fq>],
        srs: &DevSrs,
    ) -> Result<bool, AggregateVerifyError>
    where
        S: ChallengeTraceSink,
    {
        match family_id {
            ConsolidateFamilyId::TwoByOne => verify_with_digest_with_trace::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::TwoByOne.get() }>,
                S,
            >(
                challenge_context,
                trace,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ConsolidateFamilyId::FourByOne => verify_with_digest_with_trace::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::FourByOne.get() }>,
                S,
            >(
                challenge_context,
                trace,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            ConsolidateFamilyId::EightByOne => verify_with_digest_with_trace::<
                ConsolidateTranscriptDigest<{ ConsolidateFamilyId::EightByOne.get() }>,
                S,
            >(
                challenge_context,
                trace,
                pvk,
                aggregate_proof_bytes,
                padded_public_inputs,
                srs,
            ),
            other => Err(AggregateVerifyError::BadVersion(format!(
                "unknown consolidate aggregate family {}",
                other.get()
            ))),
        }
    }

    pub fn verify_family_aggregate_profiled_status(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
        let call = preflight_aggregate_verify(AggregatePreflightInput {
            statement,
            pvk,
            aggregate_proof_bytes,
            srs,
        })?;
        Self::verify_preflighted_family_aggregate_profiled_status(call)
    }

    fn verify_preflighted_family_aggregate_profiled_status(
        call: VerifiedAggregateBackendCall<'_>,
    ) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
        match call.family_id() {
            ProofFamilyId::Transfer => Self::verify_transfer_family_aggregate_profiled_status(
                call.challenge_context(),
                call.pvk(),
                call.inner_proof_bytes(),
                call.padded_public_inputs(),
                call.srs(),
            ),
            ProofFamilyId::Consolidate(family_id) => {
                Self::verify_consolidate_family_aggregate_profiled_status(
                    call.challenge_context(),
                    family_id,
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )
            }
            ProofFamilyId::Split(family_id) => Self::verify_split_family_aggregate_profiled_status(
                call.challenge_context(),
                family_id,
                call.pvk(),
                call.inner_proof_bytes(),
                call.padded_public_inputs(),
                call.srs(),
            ),
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                verify_with_digest_profiled::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    call.challenge_context(),
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )
            }
        }
    }

    pub fn verify_family_aggregate_profiled(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &DevSrs,
    ) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
        let profile = Self::verify_family_aggregate_profiled_status(
            statement,
            pvk,
            aggregate_proof_bytes,
            srs,
        )?;

        if !profile.accepted {
            return Err(AggregateVerifyError::BackendRejected(format!(
                "{:?}",
                statement.family_id()
            )));
        }
        Ok(profile)
    }

    pub fn aggregate_family_with_trace(
        statement: &AggregateStatement,
        _pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<(Vec<u8>, Vec<TraceEvent>)> {
        let family_id = statement.family_id();
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        let mut challenge_trace = VecChallengeTraceSink::default();
        let inner_proof_bytes = match family_id {
            ProofFamilyId::Transfer => Self::aggregate_transfer_family_with_trace(
                statement.challenge_context(),
                &mut challenge_trace,
                items,
                srs,
            ),
            ProofFamilyId::Consolidate(family_id) => Self::aggregate_consolidate_family_with_trace(
                family_id,
                statement.challenge_context(),
                &mut challenge_trace,
                items,
                srs,
            ),
            ProofFamilyId::Split(family_id) => Self::aggregate_split_family_with_trace(
                family_id,
                statement.challenge_context(),
                &mut challenge_trace,
                items,
                srs,
            ),
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                aggregate_with_digest_with_trace::<ShieldedIcs20WithdrawalTranscriptDigest, _>(
                    statement.challenge_context(),
                    &mut challenge_trace,
                    items,
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

        Ok((
            wrapped,
            complete_snarkpack_trace(statement, challenge_trace.into_entries()),
        ))
    }

    pub fn verify_family_aggregate_with_trace(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &DevSrs,
    ) -> Result<AggregateTraceReport, AggregateVerifyError> {
        let call = preflight_aggregate_verify(AggregatePreflightInput {
            statement,
            pvk,
            aggregate_proof_bytes,
            srs,
        })?;
        let family_id = call.family_id();
        let mut challenge_trace = VecChallengeTraceSink::default();

        let accepted = match family_id {
            ProofFamilyId::Transfer => {
                verify_with_digest_with_trace::<TransferTranscriptDigest, _>(
                    call.challenge_context(),
                    &mut challenge_trace,
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
            }
            ProofFamilyId::Consolidate(family_id) => {
                Self::verify_consolidate_family_aggregate_with_trace(
                    call.challenge_context(),
                    &mut challenge_trace,
                    family_id,
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
            }
            ProofFamilyId::Split(family_id) => Self::verify_split_family_aggregate_with_trace(
                call.challenge_context(),
                &mut challenge_trace,
                family_id,
                call.pvk(),
                call.inner_proof_bytes(),
                call.padded_public_inputs(),
                call.srs(),
            )?,
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                verify_with_digest_with_trace::<ShieldedIcs20WithdrawalTranscriptDigest, _>(
                    call.challenge_context(),
                    &mut challenge_trace,
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
            }
        };

        Ok(AggregateTraceReport {
            accepted,
            trace: complete_snarkpack_trace(statement, challenge_trace.into_entries()),
        })
    }
}

impl AggregationBackend for SnarkpackBackend {
    type Srs = DevSrs;

    fn aggregate_family(
        statement: &AggregateStatement,
        _pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &Self::Srs,
    ) -> Result<Vec<u8>> {
        let family_id = statement.family_id();
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        let inner_proof_bytes = match family_id {
            ProofFamilyId::Transfer => {
                Self::aggregate_transfer_family(statement.challenge_context(), items, srs)
            }
            ProofFamilyId::Consolidate(family_id) => Self::aggregate_consolidate_family_profiled(
                family_id,
                items,
                srs,
                statement.challenge_context(),
            )
            .map(|(bytes, _)| bytes),
            ProofFamilyId::Split(family_id) => Self::aggregate_split_family_profiled(
                family_id,
                items,
                srs,
                statement.challenge_context(),
            )
            .map(|(bytes, _)| bytes),
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                aggregate_with_digest::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    statement.challenge_context(),
                    items,
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

    fn verify_family_aggregate(
        statement: &AggregateStatement,
        pvk: &PreparedVerifyingKey<Bls12_377>,
        aggregate_proof_bytes: &[u8],
        srs: &Self::Srs,
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
            ProofFamilyId::Consolidate(family_id) => {
                Self::verify_consolidate_family_aggregate_profiled_status(
                    call.challenge_context(),
                    family_id,
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
                .accepted
            }
            ProofFamilyId::Split(family_id) => {
                Self::verify_split_family_aggregate_profiled_status(
                    call.challenge_context(),
                    family_id,
                    call.pvk(),
                    call.inner_proof_bytes(),
                    call.padded_public_inputs(),
                    call.srs(),
                )?
                .accepted
            }
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

impl SnarkpackBackend {
    pub fn aggregate_family_profiled(
        statement: &AggregateStatement,
        _pvk: &PreparedVerifyingKey<Bls12_377>,
        items: &[BatchItem],
        srs: &DevSrs,
    ) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
        let family_id = statement.family_id();
        srs.ensure_supported_count(items.len())?;
        ensure!(
            !items.is_empty(),
            "cannot build an aggregate proof for empty family {:?}",
            family_id
        );

        let (bytes, profile) = match family_id {
            ProofFamilyId::Transfer => {
                Self::aggregate_transfer_family_profiled(items, srs, statement.challenge_context())
            }
            ProofFamilyId::Consolidate(family_id) => Self::aggregate_consolidate_family_profiled(
                family_id,
                items,
                srs,
                statement.challenge_context(),
            ),
            ProofFamilyId::Split(family_id) => Self::aggregate_split_family_profiled(
                family_id,
                items,
                srs,
                statement.challenge_context(),
            ),
            ProofFamilyId::ShieldedIcs20Withdrawal(_) => {
                aggregate_with_digest_profiled::<ShieldedIcs20WithdrawalTranscriptDigest>(
                    items,
                    srs,
                    statement.challenge_context(),
                )
            }
        }?;

        let wrapped = encode_wrapped_aggregate_proof(statement.statement_digest(), &bytes)?;
        ensure!(
            wrapped.len() <= MAX_AGGREGATE_PROOF_BYTES,
            "wrapped aggregate proof bytes {} exceed cap {}",
            wrapped.len(),
            MAX_AGGREGATE_PROOF_BYTES
        );

        Ok((wrapped, profile))
    }
}

pub(crate) fn aggregate_with_digest<D: Digest>(
    challenge_context: &ChallengeContext,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    let aggregate = aggregate_proofs::<Bls12_377, D>(
        challenge_context,
        &inner_product_srs,
        &collect_proofs(items),
    )
    .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))?;
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn aggregate_with_digest_with_trace<D, S>(
    challenge_context: &ChallengeContext,
    trace: &mut S,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>>
where
    D: Digest,
    S: ChallengeTraceSink,
{
    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    let aggregate = aggregate_proofs_with_trace::<Bls12_377, D, S>(
        challenge_context,
        trace,
        &inner_product_srs,
        &collect_proofs(items),
    )
    .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))?;
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn aggregate_with_digest_profiled<D: Digest>(
    items: &[BatchItem],
    srs: &DevSrs,
    challenge_context: &ChallengeContext,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    let rayon_threads = RAYON_THREADS_PER_BATCH.load(Ordering::Relaxed);
    if rayon_threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(rayon_threads)
            .build_scoped(
                |thread| thread.run(),
                |pool| {
                    pool.install(|| {
                        aggregate_with_digest_profiled_core::<D>(challenge_context, items, srs)
                    })
                },
            )
            .map_err(|e| anyhow::anyhow!("rayon pool build error: {e}"))?
    } else {
        aggregate_with_digest_profiled_core::<D>(challenge_context, items, srs)
    }
}

fn aggregate_with_digest_profiled_core<D: Digest>(
    challenge_context: &ChallengeContext,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    let mut profile = AggregateBuildBackendProfile::default();
    let total_start = Instant::now();

    let collect_start = Instant::now();
    let proofs = collect_proofs(items);
    profile.collect_proofs_ms = collect_start.elapsed().as_secs_f64() * 1000.0;

    let inner_product_srs = srs.inner_product_srs_for_count(items.len())?;
    let backend_start = Instant::now();
    let (aggregate, core_profile) =
        aggregate_proofs_profiled::<Bls12_377, D>(challenge_context, &inner_product_srs, &proofs)
            .map_err(|e| anyhow::anyhow!("SnarkPack aggregation failed: {e}"))?;
    profile.backend_aggregate_ms = backend_start.elapsed().as_secs_f64() * 1000.0;
    apply_core_build_profile(&mut profile, &core_profile);

    let serialize_start = Instant::now();
    let mut bytes = Vec::new();
    aggregate.serialize_compressed(&mut bytes)?;
    profile.serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;
    profile.total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

    Ok((bytes, profile))
}

fn apply_core_build_profile(
    profile: &mut AggregateBuildBackendProfile,
    core_profile: &AggregateProofBuildProfile,
) {
    profile.backend_point_extract_ms = core_profile.point_extract_ms;
    profile.backend_prepared_srs_ms = core_profile.prepared_srs_ms;
    profile.backend_commitment_key_extract_ms = core_profile.commitment_key_extract_ms;
    profile.backend_commitment_ms = core_profile.commitment_ms;
    profile.backend_com_a_ms = core_profile.com_a_ms;
    profile.backend_com_b_ms = core_profile.com_b_ms;
    profile.backend_com_c_ms = core_profile.com_c_ms;
    profile.backend_pairing_normalize_batch_ms = core_profile.pairing_normalize_batch_ms;
    profile.backend_pairing_prepare_ms = core_profile.pairing_prepare_ms;
    profile.backend_pairing_miller_loop_ms = core_profile.pairing_miller_loop_ms;
    profile.backend_pairing_final_exponentiation_ms = core_profile.pairing_final_exponentiation_ms;
    profile.backend_randomizer_ms = core_profile.randomizer_ms;
    profile.backend_structured_scalar_ms = core_profile.structured_scalar_ms;
    profile.backend_weighted_a_ms = core_profile.weighted_a_ms;
    profile.backend_ip_ab_ms = core_profile.ip_ab_ms;
    profile.backend_agg_c_ms = core_profile.agg_c_ms;
    profile.backend_ck_1_r_ms = core_profile.ck_1_r_ms;
    profile.backend_consistency_check_ms = core_profile.consistency_check_ms;
    profile.backend_tipa_ab_ms = core_profile.tipa_ab_ms;
    profile.backend_tipa_c_ms = core_profile.tipa_c_ms;
    profile.backend_tipa_ab_gipa_ms = core_profile.tipa_ab_gipa_ms;
    profile.backend_tipa_ab_gipa_commit_l_ms = core_profile.tipa_ab_gipa_commit_l_ms;
    profile.backend_tipa_ab_gipa_commit_r_ms = core_profile.tipa_ab_gipa_commit_r_ms;
    profile.backend_tipa_ab_gipa_challenge_ms = core_profile.tipa_ab_gipa_challenge_ms;
    profile.backend_tipa_ab_gipa_rescale_m1_ms = core_profile.tipa_ab_gipa_rescale_m1_ms;
    profile.backend_tipa_ab_gipa_rescale_m2_ms = core_profile.tipa_ab_gipa_rescale_m2_ms;
    profile.backend_tipa_ab_gipa_rescale_ck1_ms = core_profile.tipa_ab_gipa_rescale_ck1_ms;
    profile.backend_tipa_ab_gipa_rescale_ck2_ms = core_profile.tipa_ab_gipa_rescale_ck2_ms;
    profile.backend_tipa_ab_transcript_inverse_ms = core_profile.tipa_ab_transcript_inverse_ms;
    profile.backend_tipa_ab_kzg_challenge_ms = core_profile.tipa_ab_kzg_challenge_ms;
    profile.backend_tipa_ab_kzg_coefficient_build_ms =
        core_profile.tipa_ab_kzg_coefficient_build_ms;
    profile.backend_tipa_ab_kzg_eval_quotient_ms = core_profile.tipa_ab_kzg_eval_quotient_ms;
    profile.backend_tipa_ab_kzg_opening_msm_ms = core_profile.tipa_ab_kzg_opening_msm_ms;
    profile.backend_tipa_ab_kzg_opening_ck_a_ms = core_profile.tipa_ab_kzg_opening_ck_a_ms;
    profile.backend_tipa_ab_kzg_opening_ck_b_ms = core_profile.tipa_ab_kzg_opening_ck_b_ms;
    profile.backend_tipa_c_gipa_ms = core_profile.tipa_c_gipa_ms;
    profile.backend_tipa_c_gipa_commit_l_ms = core_profile.tipa_c_gipa_commit_l_ms;
    profile.backend_tipa_c_gipa_commit_r_ms = core_profile.tipa_c_gipa_commit_r_ms;
    profile.backend_tipa_c_gipa_challenge_ms = core_profile.tipa_c_gipa_challenge_ms;
    profile.backend_tipa_c_gipa_rescale_m1_ms = core_profile.tipa_c_gipa_rescale_m1_ms;
    profile.backend_tipa_c_gipa_rescale_m2_ms = core_profile.tipa_c_gipa_rescale_m2_ms;
    profile.backend_tipa_c_gipa_rescale_ck1_ms = core_profile.tipa_c_gipa_rescale_ck1_ms;
    profile.backend_tipa_c_gipa_rescale_ck2_ms = core_profile.tipa_c_gipa_rescale_ck2_ms;
    profile.backend_tipa_c_transcript_inverse_ms = core_profile.tipa_c_transcript_inverse_ms;
    profile.backend_tipa_c_kzg_challenge_ms = core_profile.tipa_c_kzg_challenge_ms;
    profile.backend_tipa_c_kzg_coefficient_build_ms = core_profile.tipa_c_kzg_coefficient_build_ms;
    profile.backend_tipa_c_kzg_eval_quotient_ms = core_profile.tipa_c_kzg_eval_quotient_ms;
    profile.backend_tipa_c_kzg_opening_msm_ms = core_profile.tipa_c_kzg_opening_msm_ms;
    profile.backend_tipa_c_kzg_opening_ck_a_ms = core_profile.tipa_c_kzg_opening_ck_a_ms;
}

pub(crate) fn verify_with_digest<D: Digest>(
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool, AggregateVerifyError> {
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    verify_aggregate_proof::<Bls12_377, D>(
        challenge_context,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        &pvk.vk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))
}

pub(crate) fn verify_with_digest_with_trace<D, S>(
    challenge_context: &ChallengeContext,
    trace: &mut S,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool, AggregateVerifyError>
where
    D: Digest,
    S: ChallengeTraceSink,
{
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    verify_aggregate_proof_with_trace::<Bls12_377, D, S>(
        challenge_context,
        trace,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        &pvk.vk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))
}

fn complete_snarkpack_trace(
    statement: &AggregateStatement,
    challenge_entries: Vec<TraceEvent>,
) -> Vec<TraceEvent> {
    let mut trace = Vec::with_capacity(1 + 10 + challenge_entries.len());
    trace.push(trace_context_event(statement));
    push_static_abstract_trace(&mut trace);
    trace.extend(challenge_entries);
    trace
}

fn trace_context_event(statement: &AggregateStatement) -> TraceEvent {
    TraceEvent {
        spec_row_id: "fs.context-constructor",
        primary_level: TraceComparisonLevel::PenumbraByte,
        event_kind: TraceEventKind::ChallengeContext,
        stage_label: "statement",
        nonce: None,
        round_index: None,
        byte_payload: challenge_context_preimage(statement.statement_digest()),
        abstract_payload: None,
        filecoin_bug_class: None,
    }
}

fn push_static_abstract_trace(trace: &mut Vec<TraceEvent>) {
    push_abstract_trace(trace, "gipa.input-relation");
    push_abstract_trace(trace, "gipa.round-folding");
    push_abstract_trace(trace, "gipa.verifier-folding");
    push_abstract_trace(trace, "tipa.srs");
    push_abstract_trace(trace, "tipa.ab.gipa");
    push_abstract_trace(trace, "tipa.ab.kzg-equations");
    push_abstract_trace(trace, "ssm.power-sequence");
    push_abstract_trace(trace, "ssm.base-equation");
    push_abstract_trace(trace, "groth16.folded-inputs");
    push_abstract_trace(trace, "groth16.ppe");
}

fn push_abstract_trace(trace: &mut Vec<TraceEvent>, spec_row_id: &'static str) {
    trace.push(TraceEvent {
        spec_row_id,
        primary_level: TraceComparisonLevel::AbstractTrace,
        event_kind: TraceEventKind::EquationRole,
        stage_label: spec_row_id,
        nonce: None,
        round_index: None,
        byte_payload: Vec::new(),
        abstract_payload: Some(spec_row_id),
        filecoin_bug_class: None,
    });
}

pub(crate) fn verify_with_digest_profiled<D: Digest>(
    challenge_context: &ChallengeContext,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile, AggregateVerifyError> {
    let started = Instant::now();

    let deserialize_started = Instant::now();
    let aggregate = deserialize_aggregate_proof::<D>(aggregate_proof_bytes)?;
    let deserialize_ms = deserialize_started.elapsed().as_secs_f64() * 1000.0;

    let core_profile = verify_aggregate_proof_profiled::<Bls12_377, D>(
        challenge_context,
        srs.verifier_srs()
            .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?,
        &pvk.vk,
        padded_public_inputs,
        &aggregate,
    )
    .map_err(|e| AggregateVerifyError::BackendRejected(e.to_string()))?;

    Ok(profile_with_deserialize(
        core_profile,
        deserialize_ms,
        started.elapsed().as_secs_f64() * 1000.0,
    ))
}

fn profile_with_deserialize(
    core_profile: AggregateProofVerificationProfile,
    deserialize_ms: f64,
    total_ms: f64,
) -> AggregateVerificationProfile {
    AggregateVerificationProfile {
        deserialize_ms,
        challenge_ms: core_profile.challenge_ms,
        tipa_ab_ms: core_profile.tipa_ab_ms,
        tipa_c_ms: core_profile.tipa_c_ms,
        public_input_fold_ms: core_profile.public_input_fold_ms,
        ppe_ms: core_profile.ppe_ms,
        core_total_ms: core_profile.core_total_ms,
        total_ms,
        accepted: core_profile.accepted,
    }
}

#[cfg(test)]
mod tests {
    use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
    use ark_ip_proofs::challenge::VecChallengeTraceSink;
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use ark_snark::SNARK;
    use decaf377::Fq;
    use penumbra_sdk_proof_params::batch;
    use penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId;
    use proptest::prelude::*;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    use crate::transcript::TransferTranscriptDigest;
    use crate::{
        aggregate_family, aggregate_family_profiled, pad_items_to_power_of_two, srs_id,
        verify_family_aggregate, verify_family_aggregate_profiled, AggregateStatement,
        AggregateVerifyError, AGGREGATE_PROTOCOL_VERSION,
    };

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

    fn parity_families() -> [ProofFamilyId; 4] {
        [
            ProofFamilyId::Transfer,
            ProofFamilyId::Consolidate(penumbra_sdk_shielded_pool::CONSOLIDATE_FAMILY_SPECS[0].id),
            ProofFamilyId::Split(penumbra_sdk_shielded_pool::SPLIT_FAMILY_SPECS[0].id),
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
        ]
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

    fn snarkpack_matches_legacy_batch_for_counts(counts: &[usize]) {
        let (pvk, items) = sample_items();
        let base_item = items.first().expect("at least one sample item").clone();
        let srs = DevSrs::default();

        for family_id in parity_families() {
            for count in counts {
                let repeated = vec![base_item.clone(); *count];
                batch::batch_verify(&pvk, &repeated)
                    .expect("legacy batch verify should accept repeated valid proofs");

                let statement = statement_for_items(family_id, &pvk, *count, &repeated, &srs);
                let aggregate = aggregate_family(&statement, &pvk, &repeated, &srs)
                    .expect("aggregation should succeed");

                verify_family_aggregate(&statement, &pvk, &aggregate, &srs)
                    .expect("SnarkPack verify should match legacy batch verify");
            }
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
    fn snarkpack_profile_accepts_valid_aggregate() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let aggregate = aggregate_family(&statement, &pvk, &padded_items, &srs)
            .expect("aggregation should succeed");

        let profile = verify_family_aggregate_profiled(&statement, &pvk, &aggregate, &srs)
            .expect("aggregate verification should succeed");

        assert!(profile.accepted, "profiled verification should accept");
        assert!(profile.total_ms >= profile.deserialize_ms);
        assert!(profile.challenge_ms >= 0.0);
        assert!(profile.core_total_ms >= profile.tipa_ab_ms);
        assert!(profile.core_total_ms >= profile.tipa_c_ms);
        assert!(profile.public_input_fold_ms >= 0.0);
        assert!(profile.ppe_ms >= 0.0);
    }

    #[test]
    fn prover_verifier_challenge_streams_match() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");
        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);

        let mut prover_trace = VecChallengeTraceSink::default();
        let inner_aggregate = aggregate_with_digest_with_trace::<TransferTranscriptDigest, _>(
            statement.challenge_context(),
            &mut prover_trace,
            &padded_items,
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
    fn snarkpack_build_profile_exposes_tipa_subbuckets() {
        let (pvk, items) = sample_items();
        let srs = DevSrs::default();
        let padded_items =
            pad_items_to_power_of_two(&items, srs.max_padded_count as usize).expect("padding");

        let family_id = ProofFamilyId::Transfer;
        let statement = statement_for_items(family_id, &pvk, items.len(), &padded_items, &srs);
        let (_aggregate, profile) =
            aggregate_family_profiled(&statement, &pvk, &padded_items, &srs)
                .expect("profiled aggregation should succeed");

        assert!(profile.backend_tipa_ab_ms >= profile.backend_tipa_ab_gipa_ms);
        assert!(profile.backend_tipa_c_ms >= profile.backend_tipa_c_gipa_ms);
        assert!(profile.backend_tipa_ab_gipa_ms > 0.0);
        assert!(profile.backend_tipa_c_gipa_ms > 0.0);
        assert!(profile.backend_prepared_srs_ms >= 0.0);
        assert!(profile.backend_commitment_key_extract_ms >= 0.0);
        assert!(profile.backend_com_a_ms >= 0.0);
        assert!(profile.backend_com_b_ms >= 0.0);
        assert!(profile.backend_com_c_ms >= 0.0);
        assert!(profile.backend_pairing_normalize_batch_ms >= 0.0);
        assert!(profile.backend_pairing_prepare_ms >= 0.0);
        assert!(profile.backend_pairing_miller_loop_ms >= 0.0);
        assert!(profile.backend_pairing_final_exponentiation_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_coefficient_build_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_eval_quotient_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_opening_msm_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_opening_ck_a_ms >= 0.0);
        assert!(profile.backend_tipa_ab_kzg_opening_ck_b_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_coefficient_build_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_eval_quotient_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_opening_msm_ms >= 0.0);
        assert!(profile.backend_tipa_c_kzg_opening_ck_a_ms >= 0.0);

        // Subtotals use only direct children of the tipa_ab/tipa_c spans.
        // kzg_coefficient_build_ms, kzg_eval_quotient_ms, kzg_opening_msm_ms are
        // accumulated sums of ck_a and ck_b sub-operations — already contained
        // within kzg_opening_ck_a_ms and kzg_opening_ck_b_ms — so they are not
        // included here to avoid double-counting.
        let tipa_ab_subtotal = profile.backend_tipa_ab_gipa_ms
            + profile.backend_tipa_ab_transcript_inverse_ms
            + profile.backend_tipa_ab_kzg_challenge_ms
            + profile.backend_tipa_ab_kzg_opening_ck_a_ms
            + profile.backend_tipa_ab_kzg_opening_ck_b_ms;
        let tipa_c_subtotal = profile.backend_tipa_c_gipa_ms
            + profile.backend_tipa_c_transcript_inverse_ms
            + profile.backend_tipa_c_kzg_challenge_ms
            + profile.backend_tipa_c_kzg_opening_ck_a_ms;

        assert!(profile.backend_tipa_ab_ms + 5.0 >= tipa_ab_subtotal);
        assert!(profile.backend_tipa_c_ms + 5.0 >= tipa_c_subtotal);
        // kzg sub-ops are bounded by their ck wrapper spans
        assert!(
            profile.backend_tipa_ab_kzg_opening_ck_a_ms
                + profile.backend_tipa_ab_kzg_opening_ck_b_ms
                + 1.0
                >= profile.backend_tipa_ab_kzg_coefficient_build_ms
                    + profile.backend_tipa_ab_kzg_eval_quotient_ms
                    + profile.backend_tipa_ab_kzg_opening_msm_ms
        );
        assert!(
            profile.backend_tipa_ab_gipa_rescale_ck1_ms
                + profile.backend_tipa_ab_gipa_rescale_ck2_ms
                > 0.0
        );
    }

    #[test]
    fn snarkpack_matches_legacy_batch_across_families_and_counts() {
        snarkpack_matches_legacy_batch_for_counts(&[1, 2, 4, 8]);
    }

    #[test]
    #[ignore]
    fn snarkpack_matches_legacy_batch_across_families_and_counts_slow() {
        snarkpack_matches_legacy_batch_for_counts(&[64, 256, 1024]);
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
    // refactor, and must take the version-bump path (see docs/snarkpack/security.md).

    const BYTE_BASELINE_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/aggregate_bytes_baseline.txt"
    );

    fn baseline_vectors() -> Vec<(ProofFamilyId, usize, u64)> {
        let mut vectors = Vec::new();
        for (family_index, family_id) in parity_families().into_iter().enumerate() {
            for count in [1usize, 2, 4, 8] {
                let seed = 9_000 + (family_index as u64) * 100 + count as u64;
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
        out.push_str("# Regenerate: cargo test -p penumbra-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored\n");
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

    fn committed_baseline_version(contents: &str) -> Option<u32> {
        contents
            .lines()
            .find_map(|line| line.strip_prefix("version "))
            .and_then(|v| v.trim().parse().ok())
    }

    #[test]
    fn aggregate_bytes_match_committed_baseline() {
        let committed = std::fs::read_to_string(BYTE_BASELINE_PATH).unwrap_or_else(|e| {
            panic!("missing aggregate byte baseline at {BYTE_BASELINE_PATH}: {e}; regenerate with `cargo test -p penumbra-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored`")
        });

        // Version-drift guard: a committed baseline from a different protocol
        // version is the mechanical fork between "preserve bytes" and "version".
        let committed_version = committed_baseline_version(&committed);
        assert_eq!(
            committed_version,
            Some(AGGREGATE_PROTOCOL_VERSION),
            "byte baseline version tag does not match AGGREGATE_PROTOCOL_VERSION ({AGGREGATE_PROTOCOL_VERSION}); if the byte change is intentional, bump the version and regenerate via `cargo test -p penumbra-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored`"
        );

        let current = render_byte_baseline();
        assert_eq!(
            committed, current,
            "aggregate-proof bytes drifted from the committed baseline. An optimization must preserve bytes or take the protocol-version path: bump AGGREGATE_PROTOCOL_VERSION, regenerate via `cargo test -p penumbra-sdk-proof-aggregation regenerate_aggregate_byte_baseline -- --ignored`, and add an adaptation-register row."
        );
    }

    #[test]
    #[ignore = "writes the committed byte baseline; run intentionally after a sanctioned byte change"]
    fn regenerate_aggregate_byte_baseline() {
        let rendered = render_byte_baseline();
        std::fs::write(BYTE_BASELINE_PATH, rendered).expect("write byte baseline");
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
        fn snarkpack_property_matches_legacy_batch_oracle(
            count in 1usize..=8,
            seed in any::<u64>(),
            family_index in 0usize..4,
            mutate_proof in any::<bool>(),
        ) {
            let (pvk, items) = sample_items_with_count(seed, count);
            let srs = DevSrs::default();
            let padded_items = pad_items_to_power_of_two(&items, srs.max_padded_count as usize)
                .expect("padding should succeed");
            let family_id = parity_families()[family_index];

            prop_assert!(
                batch::batch_verify(&pvk, &padded_items).is_ok(),
                "legacy batch verify should accept padded valid proofs"
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
                "legacy batch verify should reject the mutated batch"
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
