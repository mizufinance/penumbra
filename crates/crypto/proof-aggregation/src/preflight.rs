use ark_groth16::PreparedVerifyingKey;
use ark_ip_proofs::{
    app_verifier::{
        app_verify_shipping_statement_preflight_core,
        AppVerifyAcceptedPreflightStatementProvenance, AppVerifyPreparedRows,
    },
    challenge::ChallengeContext,
};
use decaf377::{Bls12_377, Fq};

use crate::{
    aggregate_proof_wrapper::{decode_wrapped_aggregate_proof, MAX_AGGREGATE_PROOF_BYTES},
    app_verifier::{app_verify_family_code, AppVerifyShippingCall, APP_VERIFY_PROTOCOL_VERSION},
    backend::AggregateVerifyError,
    srs::{srs_id, DevSrs},
    statement::{
        aggregate_verification_key_bytes, statement_row_bytes_core, AggregateStatement,
        StatementIdentity,
    },
    ProofFamilyId,
};

#[derive(Clone, Debug)]
pub(crate) struct WrapperIdentity {
    pub expected_statement_digest: Vec<u8>,
    pub wrapped_proof_bytes: Vec<u8>,
    pub inner_proof_bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(crate) struct ShippingStatementBinding {
    pub statement: StatementIdentity,
    pub wrapper: WrapperIdentity,
}

/// Owned caller-prefix and repeat-final evidence retained by an accepted
/// shipping verification after its borrowed backend call has been consumed.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug)]
pub(crate) struct ShippingStatementConstructionProvenance {
    pub(crate) binding_execution: ShippingStatementBinding,
    pub(crate) source_field_rows: Vec<Vec<Fq>>,
    pub(crate) prepared_serialized_rows: AppVerifyPreparedRows<Vec<Vec<u8>>>,
}

pub struct AggregatePreflightInput<'a> {
    pub statement: &'a AggregateStatement,
    pub pvk: &'a PreparedVerifyingKey<Bls12_377>,
    pub aggregate_proof_bytes: &'a [u8],
    pub srs: &'a DevSrs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreflightCheapChecks {
    pub supported_count: bool,
    pub non_empty: bool,
    pub srs_matches: bool,
    pub wrapper_matches_statement: bool,
    pub vk_matches_statement: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreflightWorkGate {
    Reject,
    BackendWorkAllowed,
}

pub fn preflight_work_gate(checks: PreflightCheapChecks) -> PreflightWorkGate {
    if checks.supported_count
        && checks.non_empty
        && checks.srs_matches
        && checks.wrapper_matches_statement
        && checks.vk_matches_statement
    {
        PreflightWorkGate::BackendWorkAllowed
    } else {
        PreflightWorkGate::Reject
    }
}

#[derive(Clone, Copy)]
struct VerifiedPreflightChecks;

#[derive(Clone, Copy)]
pub struct VerifiedInnerProofBytes<'a> {
    bytes: &'a [u8],
}

impl<'a> VerifiedInnerProofBytes<'a> {
    pub fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Clone, Copy)]
pub struct VerifiedChallengeContext<'a> {
    context: &'a ChallengeContext,
}

impl<'a> VerifiedChallengeContext<'a> {
    pub fn as_ref(self) -> &'a ChallengeContext {
        self.context
    }
}

#[derive(Clone)]
pub struct VerifiedAggregateBackendCall<'a> {
    family_id: ProofFamilyId,
    pvk: &'a PreparedVerifyingKey<Bls12_377>,
    srs: &'a DevSrs,
    authenticated_srs_id: [u8; 32],
    challenge_context: VerifiedChallengeContext<'a>,
    inner_proof_bytes: VerifiedInnerProofBytes<'a>,
    padded_public_inputs: &'a [Vec<Fq>],
}

impl<'a> VerifiedAggregateBackendCall<'a> {
    pub fn family_id(&self) -> ProofFamilyId {
        self.family_id
    }

    pub fn pvk(&self) -> &'a PreparedVerifyingKey<Bls12_377> {
        self.pvk
    }

    pub fn srs(&self) -> &'a DevSrs {
        self.srs
    }

    pub fn challenge_context(&self) -> &'a ChallengeContext {
        self.challenge_context.as_ref()
    }

    pub fn inner_proof_bytes(&self) -> &'a [u8] {
        self.inner_proof_bytes.as_bytes()
    }

    pub fn padded_public_inputs(&self) -> &'a [Vec<Fq>] {
        self.padded_public_inputs
    }

    fn new(
        _checks: VerifiedPreflightChecks,
        family_id: ProofFamilyId,
        pvk: &'a PreparedVerifyingKey<Bls12_377>,
        srs: &'a DevSrs,
        authenticated_srs_id: [u8; 32],
        challenge_context: &'a ChallengeContext,
        inner_proof_bytes: &'a [u8],
        padded_public_inputs: &'a [Vec<Fq>],
    ) -> Self {
        Self {
            family_id,
            pvk,
            srs,
            authenticated_srs_id,
            challenge_context: VerifiedChallengeContext {
                context: challenge_context,
            },
            inner_proof_bytes: VerifiedInnerProofBytes {
                bytes: inner_proof_bytes,
            },
            padded_public_inputs,
        }
    }
}

pub fn preflight_aggregate_verify<'a>(
    input: AggregatePreflightInput<'a>,
) -> Result<VerifiedAggregateBackendCall<'a>, AggregateVerifyError> {
    let statement = input.statement;
    let family_id = statement.family_id();
    let padded_public_inputs = statement.padded_public_inputs();

    let supported_count = input
        .srs
        .ensure_supported_count(padded_public_inputs.len())
        .map(|_| true)
        .map_err(|err| AggregateVerifyError::BadPadding(err.to_string()))?;
    if padded_public_inputs.is_empty() {
        return Err(AggregateVerifyError::BadCount(format!(
            "cannot verify an empty aggregate for family {family_id:?}"
        )));
    }

    let authenticated_srs_id = srs_id(input.srs);
    if statement.srs_id() != authenticated_srs_id {
        return Err(AggregateVerifyError::StatementDigestMismatch);
    }

    let inner_proof_bytes = decode_wrapped_aggregate_proof(
        input.aggregate_proof_bytes,
        statement.statement_digest(),
        Some(MAX_AGGREGATE_PROOF_BYTES),
    )?;

    let serialized_vk = aggregate_verification_key_bytes(input.pvk)?;
    if statement.serialized_vk() != serialized_vk.as_slice() {
        return Err(AggregateVerifyError::StatementDigestMismatch);
    }

    let checks = require_preflight_checks(PreflightCheapChecks {
        supported_count,
        non_empty: true,
        srs_matches: true,
        wrapper_matches_statement: true,
        vk_matches_statement: true,
    })?;

    Ok(VerifiedAggregateBackendCall::new(
        checks,
        family_id,
        input.pvk,
        input.srs,
        authenticated_srs_id,
        statement.challenge_context(),
        inner_proof_bytes,
        padded_public_inputs,
    ))
}

/// Shipping preflight plus a pure projection of every authenticated byte and
/// scalar field consumed by the v1 protocol.
#[doc(hidden)]
pub(crate) fn preflight_shipping_aggregate_verify<'a>(
    application_call: AppVerifyShippingCall,
    input: AggregatePreflightInput<'a>,
) -> Result<
    AppVerifyAcceptedPreflightStatementProvenance<
        VerifiedAggregateBackendCall<'a>,
        Fq,
        ShippingStatementBinding,
    >,
    AggregateVerifyError,
> {
    let statement = input.statement;
    let wrapped_proof_bytes = input.aggregate_proof_bytes;
    let backend_call = preflight_aggregate_verify(input)?;
    let binding_execution = ShippingStatementBinding {
        statement: statement.identity(),
        wrapper: WrapperIdentity {
            expected_statement_digest: statement.statement_digest().to_vec(),
            wrapped_proof_bytes: wrapped_proof_bytes.to_vec(),
            inner_proof_bytes: backend_call.inner_proof_bytes().to_vec(),
        },
    };
    let source_count = usize::try_from(statement.real_count())
        .map_err(|_| AggregateVerifyError::StatementDigestMismatch)?;
    let source_public_inputs = statement
        .padded_public_inputs()
        .get(..source_count)
        .ok_or(AggregateVerifyError::StatementDigestMismatch)?;
    let source_serialized_rows = statement_row_bytes_core(source_public_inputs)?.to_nested_bytes();
    let source_rows = ark_ip_proofs::app_verifier::AppVerifyStatementRowBytesProjection {
        source_rows: source_public_inputs.to_vec(),
        serialized_rows: source_serialized_rows,
    };
    let statement_rows = statement.shipping_rows();
    let rows = ark_ip_proofs::app_verifier::AppVerifyShippingRowsProjection {
        real_count: statement_rows.real_count,
        padded_count: statement_rows.padded_count,
        public_input_arity: statement_rows.public_input_arity,
        fields: statement_rows.fields.to_vec(),
        serialized: statement_rows.serialized.to_nested_bytes(),
    };
    let authenticated_srs_id = backend_call.authenticated_srs_id;
    let serialized_vk = binding_execution.statement.serialized_vk.clone();
    let vk_digest = binding_execution.statement.vk_digest.clone();
    let canonical_statement = binding_execution.statement.canonical_statement.clone();
    let challenge_context = binding_execution.statement.challenge_context.clone();
    let wrapper = ark_ip_proofs::app_verifier::AppVerifyShippingWrapperProjection {
        statement_digest: binding_execution.wrapper.expected_statement_digest.clone(),
        wrapped_proof_bytes: binding_execution.wrapper.wrapped_proof_bytes.clone(),
        inner_proof_bytes: binding_execution.wrapper.inner_proof_bytes.clone(),
    };
    let projection = app_verify_shipping_statement_preflight_core(
        backend_call,
        binding_execution,
        source_rows,
        rows,
        application_call,
        APP_VERIFY_PROTOCOL_VERSION,
        app_verify_family_code(statement.family_id()),
        authenticated_srs_id.to_vec(),
        serialized_vk,
        vk_digest,
        canonical_statement,
        wrapper,
        challenge_context,
    )
    .map_err(|_| AggregateVerifyError::StatementDigestMismatch)?;
    Ok(projection)
}

fn require_preflight_checks(
    checks: PreflightCheapChecks,
) -> Result<VerifiedPreflightChecks, AggregateVerifyError> {
    match preflight_work_gate(checks) {
        PreflightWorkGate::BackendWorkAllowed => Ok(VerifiedPreflightChecks),
        PreflightWorkGate::Reject => Err(AggregateVerifyError::StatementDigestMismatch),
    }
}
