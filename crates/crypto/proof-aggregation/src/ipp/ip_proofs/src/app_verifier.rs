//! Control state for the application aggregate verifier.
//!
//! Rich proof and statement values stay at the application edge. This module
//! owns every acceptance-relevant scalar comparison and tagged reduction.

/// Wire version authenticated by the shipping aggregate statement.
pub const APP_VERIFY_PROTOCOL_VERSION: u32 = 3;

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppVerifyFamilyCode {
    pub proof_family_id: u32,
    pub note_reshape_family_id: u32,
    pub shielded_withdrawal_family_id: u32,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppVerifyExpectedCall {
    pub segment_index: usize,
    pub family_index: usize,
    pub family: AppVerifyFamilyCode,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppVerifyCallId {
    pub order_index: usize,
    pub segment_index: usize,
    pub family_index: usize,
    pub family: AppVerifyFamilyCode,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppVerifySegmentRange {
    pub segment_index: usize,
    pub start: usize,
    pub end: usize,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppVerifyCallResult {
    pub id: AppVerifyCallId,
    pub accepted: bool,
}

/// Family and count facts checked against one application-planned verifier call.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppVerifyShippingCall {
    pub id: AppVerifyCallId,
    pub bundle_family: AppVerifyFamilyCode,
    pub expected_real_count: usize,
    pub bundle_real_count: u32,
    pub expected_padded_count: usize,
    pub bundle_padded_count: u32,
}

/// Exact wrapper bytes retained after successful shipping preflight.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyShippingWrapperProjection {
    pub statement_digest: Vec<u8>,
    pub wrapped_proof_bytes: Vec<u8>,
    pub inner_proof_bytes: Vec<u8>,
}

/// Authenticated input to one verifier call. The strict proof decoder consumes
/// `inner_proof_bytes` only after preflight checks.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyShippingInput {
    pub call: AppVerifyShippingCall,
    pub protocol_version: u32,
    pub family: AppVerifyFamilyCode,
    pub srs_id: Vec<u8>,
    pub serialized_vk: Vec<u8>,
    pub vk_digest: Vec<u8>,
    pub real_count: u32,
    pub padded_count: u32,
    pub public_input_arity: u32,
    pub padded_public_inputs: Vec<Vec<Vec<u8>>>,
    pub canonical_statement_bytes: Vec<u8>,
    pub statement_digest: Vec<u8>,
    pub wrapped_proof_bytes: Vec<u8>,
    pub inner_proof_bytes: Vec<u8>,
    pub challenge_context: Vec<u8>,
}

/// Accepted/rejected backend result paired with the exact authenticated input.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyShippingResult {
    pub input: AppVerifyShippingInput,
    pub result: AppVerifyCallResult,
}

/// Tagged result and observation produced by the backend verifier.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct AppVerifyShippingBackendResult<Execution> {
    execution: Execution,
    result: AppVerifyCallResult,
}

/// Application result paired with the backend execution that supplied it.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct AppVerifyShippingExecutedResult<Execution> {
    shipping_result: AppVerifyShippingResult,
    backend_result: AppVerifyShippingBackendResult<Execution>,
}

/// A backend call paired with the exact field rows and shipping input produced
/// by one successful pure constructor execution.
///
/// This generic record does not validate the backend call.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyShippingPreflight<BackendCall, Fields> {
    pub backend_call: BackendCall,
    pub padded_public_input_fields: Fields,
    pub shipping_input: AppVerifyShippingInput,
}

/// Allocation-free projection of the statement rows consumed by shipping
/// preflight. Generic carriers keep Arkworks values outside this crate while
/// retaining exact source identity in extraction.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyShippingRowsProjection<Fields, Serialized> {
    pub real_count: u32,
    pub padded_count: u32,
    pub public_input_arity: u32,
    pub fields: Fields,
    pub serialized: Serialized,
}

/// Exact pairing of Arkworks source rows with their completed row
/// serialization. Serialization semantics remain an explicit external
/// boundary; this record prevents the caller from substituting either side.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyStatementRowBytesProjection<Rows, Serialized> {
    pub source_rows: Rows,
    pub serialized_rows: Serialized,
}

/// Result of the production repeat-final row preparation core.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyPreparedRows<T> {
    pub real_count: usize,
    pub padded_count: usize,
    pub padded_public_inputs: Vec<T>,
}

/// Accepted serialized-row provenance beside the exact shipping preflight.
///
/// Field values remain opaque. The core operates only on their
/// concrete serialization and retains the caller field rows together with
/// the exact repeat-final serialized result. `BindingExecution` is an opaque
/// call-scoped record retained without interpretation for the separate
/// statement-hash and wrapper-decoder refinement.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct AppVerifyAcceptedPreflightStatementProvenance<BackendCall, Field, BindingExecution> {
    pub binding_execution: BindingExecution,
    pub source_field_rows: Vec<Vec<Field>>,
    pub prepared_serialized_rows: AppVerifyPreparedRows<Vec<Vec<u8>>>,
    pub preflight: AppVerifyShippingPreflight<BackendCall, Vec<Vec<Field>>>,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppVerifyShippingInputError {
    CallIdentityMismatch,
    CallPaddingMismatch,
    StatementFamilyMismatch,
    StatementRealCountMismatch,
    StatementPaddedCountMismatch,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppVerifyShippingStatementPreflightError {
    RowPadding(AppVerifyRowPaddingError),
    SourceSerializationCountMismatch,
    StatementSerializationCountMismatch,
    StatementRealCountMismatch,
    StatementPaddedCountMismatch,
    SourceFieldArityMismatch,
    SourceSerializedArityMismatch,
    StatementFieldArityMismatch,
    StatementSerializedArityMismatch,
    StatementSerializedRowsMismatch,
    ShippingInput(AppVerifyShippingInputError),
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppVerifyRowPaddingError {
    TargetSmallerThanInput,
    EmptyRowsForNonzeroTarget,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppVerifyPreflightError {
    BadVersion,
    EmptyProofSet,
    BadSrsLength,
    SrsMismatch,
    SegmentCoverageOverflow,
    SegmentCoverageMismatch,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppVerifyPlanError {
    FamilyCountMismatch,
    FamilyMismatch,
    RealCountOverflow,
    RealCountMismatch,
    PaddedCountOverflow,
    PaddedCountMismatch,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppVerifyReductionError {
    OutcomeCountMismatch,
    OutcomeIdentityMismatch,
}

/// One joined verifier result retaining its first-order observation and opaque
/// execution payload.
///
/// The three identifiers come from the planner, authenticated input, and
/// retained backend result respectively. The accepted projection validates
/// them before returning the observation and execution unchanged.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyPlannerIndexedExecutedRecord<Observation, Execution> {
    pub planner_id: AppVerifyCallId,
    pub authenticated_id: AppVerifyCallId,
    pub executed_id: AppVerifyCallId,
    pub accepted: bool,
    pub observation: Observation,
    pub executed: Execution,
}

/// Structurally valid planner-ordered join results and their rejected calls.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppVerifyAcceptedJoinProjection<Observation, Execution> {
    pub records: Vec<AppVerifyPlannerIndexedExecutedRecord<Observation, Execution>>,
    pub rejected_calls: Vec<AppVerifyCallId>,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppVerifyAcceptedJoinProjectionError {
    OutcomeCountMismatch { expected: usize, actual: usize },
    FullIdentityMismatch { position: usize },
    OutcomeOrderMismatch { position: usize },
}

/// Validate the scalar statement projection against one planned call.
///
/// This is the production check that binds the routed statement family and
/// counts to the exact call accepted by the application reducer.
#[doc(hidden)]
pub fn app_verify_shipping_projection_core(
    call: AppVerifyShippingCall,
    family: AppVerifyFamilyCode,
    real_count: u32,
    padded_count: u32,
) -> Result<(), AppVerifyShippingInputError> {
    match app_verify_plan_identity_core(
        call.id,
        call.bundle_family,
        call.expected_real_count,
        call.bundle_real_count,
    ) {
        Ok(_) => {}
        Err(_) => return Err(AppVerifyShippingInputError::CallIdentityMismatch),
    }
    match app_verify_plan_padding_core(
        call.id,
        call.expected_padded_count,
        call.bundle_padded_count,
    ) {
        Ok(_) => {}
        Err(_) => return Err(AppVerifyShippingInputError::CallPaddingMismatch),
    }
    if !app_verify_family_code_matches(family, call.bundle_family) {
        return Err(AppVerifyShippingInputError::StatementFamilyMismatch);
    }
    if real_count != call.bundle_real_count {
        return Err(AppVerifyShippingInputError::StatementRealCountMismatch);
    }
    if padded_count != call.bundle_padded_count {
        return Err(AppVerifyShippingInputError::StatementPaddedCountMismatch);
    }
    Ok(())
}

/// Pure projection used after successful shipping preflight.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn app_verify_shipping_input_from_parts(
    call: AppVerifyShippingCall,
    protocol_version: u32,
    family: AppVerifyFamilyCode,
    srs_id: Vec<u8>,
    serialized_vk: Vec<u8>,
    vk_digest: Vec<u8>,
    real_count: u32,
    padded_count: u32,
    public_input_arity: u32,
    padded_public_inputs: Vec<Vec<Vec<u8>>>,
    canonical_statement_bytes: Vec<u8>,
    wrapper: AppVerifyShippingWrapperProjection,
    challenge_context: Vec<u8>,
) -> Result<AppVerifyShippingInput, AppVerifyShippingInputError> {
    match app_verify_shipping_projection_core(call, family, real_count, padded_count) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    Ok(AppVerifyShippingInput {
        call,
        protocol_version,
        family,
        srs_id,
        serialized_vk,
        vk_digest,
        real_count,
        padded_count,
        public_input_arity,
        padded_public_inputs,
        canonical_statement_bytes,
        statement_digest: wrapper.statement_digest,
        wrapped_proof_bytes: wrapper.wrapped_proof_bytes,
        inner_proof_bytes: wrapper.inner_proof_bytes,
        challenge_context,
    })
}

/// Construct the shipping input and retain it beside the supplied backend
/// call and source field rows. Counts and serialized rows come from the same
/// row projection. Production invokes this only after backend preflight
/// succeeds.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn app_verify_shipping_preflight_core<BackendCall, Fields>(
    backend_call: BackendCall,
    rows: AppVerifyShippingRowsProjection<Fields, Vec<Vec<Vec<u8>>>>,
    call: AppVerifyShippingCall,
    protocol_version: u32,
    family: AppVerifyFamilyCode,
    srs_id: Vec<u8>,
    serialized_vk: Vec<u8>,
    vk_digest: Vec<u8>,
    canonical_statement_bytes: Vec<u8>,
    wrapper: AppVerifyShippingWrapperProjection,
    challenge_context: Vec<u8>,
) -> Result<AppVerifyShippingPreflight<BackendCall, Fields>, AppVerifyShippingInputError> {
    let real_count = rows.real_count;
    let padded_count = rows.padded_count;
    let public_input_arity = rows.public_input_arity;
    let padded_public_input_fields = rows.fields;
    let padded_public_inputs = rows.serialized;
    let shipping_input = match app_verify_shipping_input_from_parts(
        call,
        protocol_version,
        family,
        srs_id,
        serialized_vk,
        vk_digest,
        real_count,
        padded_count,
        public_input_arity,
        padded_public_inputs,
        canonical_statement_bytes,
        wrapper,
        challenge_context,
    ) {
        Ok(input) => input,
        Err(error) => return Err(error),
    };
    Ok(AppVerifyShippingPreflight {
        backend_call,
        padded_public_input_fields,
        shipping_input,
    })
}

/// Internal constructor used only after a semantic execution has produced the
/// complete call result. Shipping code must not construct this from a bare
/// acceptance bit.
pub(crate) fn app_verify_shipping_backend_result_from_parts<Execution>(
    execution: Execution,
    result: AppVerifyCallResult,
) -> AppVerifyShippingBackendResult<Execution> {
    AppVerifyShippingBackendResult { execution, result }
}

/// Move the exact execution and tagged result across an opaque runtime
/// ownership boundary without reconstructing either value.
pub(crate) fn app_verify_shipping_backend_result_into_parts<Execution>(
    backend: AppVerifyShippingBackendResult<Execution>,
) -> (Execution, AppVerifyCallResult) {
    (backend.execution, backend.result)
}

/// Construct the application result from the exact retained backend result.
///
/// Identity is checked again at this boundary, so an unrelated retained
/// execution fails closed instead of being attached to the supplied input.
#[doc(hidden)]
pub fn app_verify_shipping_result_from_backend_result<Execution>(
    input: AppVerifyShippingInput,
    backend_result: AppVerifyShippingBackendResult<Execution>,
) -> Result<AppVerifyShippingExecutedResult<Execution>, AppVerifyShippingInputError> {
    if !app_verify_call_id_matches(backend_result.result.id, input.call.id) {
        return Err(AppVerifyShippingInputError::CallIdentityMismatch);
    }
    let shipping_result = AppVerifyShippingResult {
        input,
        result: backend_result.result,
    };
    Ok(AppVerifyShippingExecutedResult {
        shipping_result,
        backend_result,
    })
}

/// Borrow the application result while retaining its exact backend execution.
#[doc(hidden)]
pub fn app_verify_shipping_executed_result_result<Execution>(
    executed: &AppVerifyShippingExecutedResult<Execution>,
) -> &AppVerifyShippingResult {
    &executed.shipping_result
}

/// Consume the application result and its exact retained backend execution.
#[doc(hidden)]
pub fn app_verify_shipping_executed_result_into_parts<Execution>(
    executed: AppVerifyShippingExecutedResult<Execution>,
) -> (
    AppVerifyShippingResult,
    AppVerifyShippingBackendResult<Execution>,
) {
    (executed.shipping_result, executed.backend_result)
}

/// Exact successful projection from the shipping preflight carrier into the
/// backend call, retained field rows, and authenticated byte-level input
/// consumed by verification.
#[doc(hidden)]
pub fn app_verify_shipping_into_parts_core<BackendCall, Fields>(
    preflight: AppVerifyShippingPreflight<BackendCall, Fields>,
) -> (BackendCall, Fields, AppVerifyShippingInput) {
    (
        preflight.backend_call,
        preflight.padded_public_input_fields,
        preflight.shipping_input,
    )
}

/// Repeat the final caller-order row up to an exact target length.
#[doc(hidden)]
pub fn app_verify_repeat_final_rows_core<T: Clone>(
    mut rows: Vec<T>,
    target: usize,
) -> Result<Vec<T>, AppVerifyRowPaddingError> {
    if target < rows.len() {
        return Err(AppVerifyRowPaddingError::TargetSmallerThanInput);
    }
    if rows.is_empty() {
        return if target == 0 {
            Ok(rows)
        } else {
            Err(AppVerifyRowPaddingError::EmptyRowsForNonzeroTarget)
        };
    }

    let last_index = match rows.len().checked_sub(1) {
        Some(index) => index,
        None => return Err(AppVerifyRowPaddingError::EmptyRowsForNonzeroTarget),
    };
    let last = match rows.get(last_index) {
        Some(last) => last.clone(),
        None => return Err(AppVerifyRowPaddingError::EmptyRowsForNonzeroTarget),
    };
    while rows.len() < target {
        rows.push(last.clone());
    }
    Ok(rows)
}

/// Production-used row preparation around the extracted repeat-final core.
/// The input argument remains visible to extraction, while the result retains
/// its exact real count, target count, and padded output.
#[doc(hidden)]
pub fn app_verify_prepare_public_input_rows_core<T: Clone>(
    public_inputs: Vec<T>,
    padded_count: usize,
) -> Result<AppVerifyPreparedRows<T>, AppVerifyRowPaddingError> {
    let real_count = public_inputs.len();
    let padded_public_inputs = match app_verify_repeat_final_rows_core(public_inputs, padded_count)
    {
        Ok(rows) => rows,
        Err(error) => return Err(error),
    };
    Ok(AppVerifyPreparedRows {
        real_count,
        padded_count,
        padded_public_inputs,
    })
}

fn app_verify_field_rows_have_arity<Field>(rows: &[Vec<Field>], expected: usize) -> bool {
    let mut index = 0usize;
    while index < rows.len() {
        if rows[index].len() != expected {
            return false;
        }
        index += 1;
    }
    true
}

fn app_verify_serialized_rows_have_arity(rows: &[Vec<Vec<u8>>], expected: usize) -> bool {
    let mut index = 0usize;
    while index < rows.len() {
        if rows[index].len() != expected {
            return false;
        }
        index += 1;
    }
    true
}

/// Structural equality for serialized rows using only concrete byte-vector
/// equality. This avoids treating an opaque field `PartialEq` result as a
/// protocol claim.
fn app_verify_serialized_row_equal(left: &[Vec<u8>], right: &[Vec<u8>]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut field_index = 0usize;
    while field_index < left.len() {
        if left[field_index] != right[field_index] {
            return false;
        }
        field_index += 1;
    }
    true
}

fn app_verify_serialized_rows_equal(left: &[Vec<Vec<u8>>], right: &[Vec<Vec<u8>>]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut row_index = 0usize;
    while row_index < left.len() {
        if !app_verify_serialized_row_equal(&left[row_index], &right[row_index]) {
            return false;
        }
        row_index += 1;
    }
    true
}

fn app_verify_prepare_shipping_statement_rows_core<Field>(
    source_field_rows: &[Vec<Field>],
    source_serialized_rows: Vec<Vec<Vec<u8>>>,
    statement_rows: &AppVerifyShippingRowsProjection<Vec<Vec<Field>>, Vec<Vec<Vec<u8>>>>,
) -> Result<AppVerifyPreparedRows<Vec<Vec<u8>>>, AppVerifyShippingStatementPreflightError> {
    let source_field_count = source_field_rows.len();
    let source_serialized_count = source_serialized_rows.len();
    let statement_field_count = statement_rows.fields.len();
    let statement_serialized_count = statement_rows.serialized.len();
    if source_field_count != source_serialized_count {
        return Err(AppVerifyShippingStatementPreflightError::SourceSerializationCountMismatch);
    }
    if statement_field_count != statement_serialized_count {
        return Err(AppVerifyShippingStatementPreflightError::StatementSerializationCountMismatch);
    }
    if source_field_count != statement_rows.real_count as usize {
        return Err(AppVerifyShippingStatementPreflightError::StatementRealCountMismatch);
    }
    if statement_field_count != statement_rows.padded_count as usize {
        return Err(AppVerifyShippingStatementPreflightError::StatementPaddedCountMismatch);
    }
    let expected_arity = statement_rows.public_input_arity as usize;
    if !app_verify_field_rows_have_arity(source_field_rows, expected_arity) {
        return Err(AppVerifyShippingStatementPreflightError::SourceFieldArityMismatch);
    }
    if !app_verify_serialized_rows_have_arity(&source_serialized_rows, expected_arity) {
        return Err(AppVerifyShippingStatementPreflightError::SourceSerializedArityMismatch);
    }
    if !app_verify_field_rows_have_arity(&statement_rows.fields, expected_arity) {
        return Err(AppVerifyShippingStatementPreflightError::StatementFieldArityMismatch);
    }
    if !app_verify_serialized_rows_have_arity(&statement_rows.serialized, expected_arity) {
        return Err(AppVerifyShippingStatementPreflightError::StatementSerializedArityMismatch);
    }

    let prepared = match app_verify_prepare_public_input_rows_core(
        source_serialized_rows,
        statement_rows.padded_count as usize,
    ) {
        Ok(prepared) => prepared,
        Err(error) => return Err(AppVerifyShippingStatementPreflightError::RowPadding(error)),
    };
    if prepared.real_count != statement_rows.real_count as usize {
        return Err(AppVerifyShippingStatementPreflightError::StatementRealCountMismatch);
    }
    if prepared.padded_count != statement_rows.padded_count as usize {
        return Err(AppVerifyShippingStatementPreflightError::StatementPaddedCountMismatch);
    }
    if !app_verify_serialized_rows_equal(&prepared.padded_public_inputs, &statement_rows.serialized)
    {
        return Err(AppVerifyShippingStatementPreflightError::StatementSerializedRowsMismatch);
    }
    Ok(prepared)
}

/// Pad serialized caller-order rows, bind them to authenticated statement
/// serialization, and construct the exact shipping preflight used by
/// production.
///
/// Arkworks field serialization remains outside this core. The
/// successful carrier retains its opaque field inputs, concrete byte
/// projection, prepared serialization, authenticated statement fields, and
/// shipping output.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn app_verify_shipping_statement_preflight_core<BackendCall, Field, BindingExecution>(
    backend_call: BackendCall,
    binding_execution: BindingExecution,
    source_rows: AppVerifyStatementRowBytesProjection<Vec<Vec<Field>>, Vec<Vec<Vec<u8>>>>,
    statement_rows: AppVerifyShippingRowsProjection<Vec<Vec<Field>>, Vec<Vec<Vec<u8>>>>,
    call: AppVerifyShippingCall,
    protocol_version: u32,
    family: AppVerifyFamilyCode,
    srs_id: Vec<u8>,
    serialized_vk: Vec<u8>,
    vk_digest: Vec<u8>,
    canonical_statement_bytes: Vec<u8>,
    wrapper: AppVerifyShippingWrapperProjection,
    challenge_context: Vec<u8>,
) -> Result<
    AppVerifyAcceptedPreflightStatementProvenance<BackendCall, Field, BindingExecution>,
    AppVerifyShippingStatementPreflightError,
> {
    let AppVerifyStatementRowBytesProjection {
        source_rows: source_field_rows,
        serialized_rows: source_serialized_rows,
    } = source_rows;
    let prepared = match app_verify_prepare_shipping_statement_rows_core(
        &source_field_rows,
        source_serialized_rows,
        &statement_rows,
    ) {
        Ok(prepared) => prepared,
        Err(error) => return Err(error),
    };

    let rows = crate::app_verifier::AppVerifyShippingRowsProjection {
        real_count: statement_rows.real_count,
        padded_count: statement_rows.padded_count,
        public_input_arity: statement_rows.public_input_arity,
        fields: statement_rows.fields,
        serialized: statement_rows.serialized,
    };
    let preflight = match app_verify_shipping_preflight_core(
        backend_call,
        rows,
        call,
        protocol_version,
        family,
        srs_id,
        serialized_vk,
        vk_digest,
        canonical_statement_bytes,
        wrapper,
        challenge_context,
    ) {
        Ok(preflight) => preflight,
        Err(error) => {
            return Err(AppVerifyShippingStatementPreflightError::ShippingInput(
                error,
            ))
        }
    };
    Ok(AppVerifyAcceptedPreflightStatementProvenance {
        binding_execution,
        source_field_rows,
        prepared_serialized_rows: prepared,
        preflight,
    })
}

#[doc(hidden)]
pub fn app_verify_preflight_core(
    expected_version: u32,
    bundle_version: u32,
    total_proof_count: usize,
    expected_srs_id: Vec<u8>,
    bundle_srs_id: Vec<u8>,
    artifact_count: usize,
    counts_provided: bool,
    segment_tx_counts: Vec<usize>,
) -> Result<Vec<AppVerifySegmentRange>, AppVerifyPreflightError> {
    if bundle_version != expected_version {
        return Err(AppVerifyPreflightError::BadVersion);
    }
    if total_proof_count == 0 {
        return Err(AppVerifyPreflightError::EmptyProofSet);
    }
    if bundle_srs_id.len() != 32 {
        return Err(AppVerifyPreflightError::BadSrsLength);
    }
    if bundle_srs_id != expected_srs_id {
        return Err(AppVerifyPreflightError::SrsMismatch);
    }
    if !counts_provided {
        let mut ranges = Vec::with_capacity(1);
        ranges.push(AppVerifySegmentRange {
            segment_index: 0,
            start: 0,
            end: artifact_count,
        });
        return Ok(ranges);
    }

    let mut ranges = Vec::with_capacity(segment_tx_counts.len());
    let mut start = 0usize;
    let mut segment_index = 0usize;
    let mut overflowed = false;
    while segment_index < segment_tx_counts.len() && !overflowed {
        let count = segment_tx_counts[segment_index];
        match start.checked_add(count) {
            Some(end) => {
                ranges.push(AppVerifySegmentRange {
                    segment_index,
                    start,
                    end,
                });
                start = end;
                segment_index += 1;
            }
            None => overflowed = true,
        }
    }
    if overflowed {
        return Err(AppVerifyPreflightError::SegmentCoverageOverflow);
    }
    if start != artifact_count {
        return Err(AppVerifyPreflightError::SegmentCoverageMismatch);
    }
    Ok(ranges)
}

#[doc(hidden)]
pub fn app_verify_family_count_core(
    expected_family_count: usize,
    bundle_family_count: usize,
) -> Result<(), AppVerifyPlanError> {
    if bundle_family_count != expected_family_count {
        return Err(AppVerifyPlanError::FamilyCountMismatch);
    }
    Ok(())
}

#[doc(hidden)]
pub fn app_verify_plan_ids_core(expected: Vec<AppVerifyExpectedCall>) -> Vec<AppVerifyCallId> {
    let mut ids = Vec::with_capacity(expected.len());
    let mut order_index = 0usize;
    while order_index < expected.len() {
        let item = expected[order_index];
        ids.push(AppVerifyCallId {
            order_index,
            segment_index: item.segment_index,
            family_index: item.family_index,
            family: item.family,
        });
        order_index += 1;
    }
    ids
}

fn app_verify_family_code_matches(left: AppVerifyFamilyCode, right: AppVerifyFamilyCode) -> bool {
    left.proof_family_id == right.proof_family_id
        && left.note_reshape_family_id == right.note_reshape_family_id
        && left.shielded_withdrawal_family_id == right.shielded_withdrawal_family_id
}

fn app_verify_call_id_matches(left: AppVerifyCallId, right: AppVerifyCallId) -> bool {
    left.order_index == right.order_index
        && left.segment_index == right.segment_index
        && left.family_index == right.family_index
        && app_verify_family_code_matches(left.family, right.family)
}

#[doc(hidden)]
pub fn app_verify_plan_identity_core(
    id: AppVerifyCallId,
    bundle_family: AppVerifyFamilyCode,
    expected_real_count: usize,
    bundle_real_count: u32,
) -> Result<AppVerifyCallId, AppVerifyPlanError> {
    if !app_verify_family_code_matches(bundle_family, id.family) {
        return Err(AppVerifyPlanError::FamilyMismatch);
    }
    if expected_real_count > u32::MAX as usize {
        return Err(AppVerifyPlanError::RealCountOverflow);
    }
    if bundle_real_count != expected_real_count as u32 {
        return Err(AppVerifyPlanError::RealCountMismatch);
    }
    Ok(id)
}

#[doc(hidden)]
pub fn app_verify_plan_padding_core(
    id: AppVerifyCallId,
    expected_padded_count: usize,
    bundle_padded_count: u32,
) -> Result<AppVerifyCallId, AppVerifyPlanError> {
    if expected_padded_count > u32::MAX as usize {
        return Err(AppVerifyPlanError::PaddedCountOverflow);
    }
    if bundle_padded_count != expected_padded_count as u32 {
        return Err(AppVerifyPlanError::PaddedCountMismatch);
    }
    Ok(id)
}

/// Validate the complete synchronous projection of joined verifier records.
///
/// Failure order is count, retained full identity, then planner order.
/// Structurally valid projection returns the owned records without sorting or
/// replacing their opaque execution payloads and lists every rejected call.
#[doc(hidden)]
pub fn app_verify_accepted_join_projection_core<Observation, Execution>(
    expected_call_ids: Vec<AppVerifyCallId>,
    records: Vec<AppVerifyPlannerIndexedExecutedRecord<Observation, Execution>>,
) -> Result<
    AppVerifyAcceptedJoinProjection<Observation, Execution>,
    AppVerifyAcceptedJoinProjectionError,
> {
    if records.len() != expected_call_ids.len() {
        return Err(AppVerifyAcceptedJoinProjectionError::OutcomeCountMismatch {
            expected: expected_call_ids.len(),
            actual: records.len(),
        });
    }

    let mut position = 0usize;
    let mut identities_match = true;
    while position < records.len() && identities_match {
        let record = &records[position];
        if !app_verify_call_id_matches(record.authenticated_id, record.planner_id)
            || !app_verify_call_id_matches(record.executed_id, record.planner_id)
        {
            identities_match = false;
        } else {
            position += 1;
        }
    }
    if !identities_match {
        return Err(AppVerifyAcceptedJoinProjectionError::FullIdentityMismatch { position });
    }

    position = 0usize;
    let mut order_matches = true;
    while position < expected_call_ids.len() && order_matches {
        if !app_verify_call_id_matches(records[position].planner_id, expected_call_ids[position]) {
            order_matches = false;
        } else {
            position += 1;
        }
    }
    if !order_matches {
        return Err(AppVerifyAcceptedJoinProjectionError::OutcomeOrderMismatch { position });
    }

    let mut rejected_calls = Vec::new();
    position = 0usize;
    while position < records.len() {
        if !records[position].accepted {
            rejected_calls.push(records[position].planner_id);
        }
        position += 1;
    }
    Ok(AppVerifyAcceptedJoinProjection {
        records,
        rejected_calls,
    })
}

/// Accept exactly a structurally projected join with no retained rejections.
///
/// The async caller invokes this only after the existing reducer acceptance
/// path, preserving the reducer's rejection and error ordering.
#[doc(hidden)]
pub fn app_verify_join_acceptance_core(rejected_calls: Vec<AppVerifyCallId>) -> bool {
    rejected_calls.is_empty()
}

#[doc(hidden)]
pub fn app_verify_reduce_core(
    expected_call_ids: Vec<AppVerifyCallId>,
    results: Vec<AppVerifyCallResult>,
) -> Result<Vec<AppVerifyCallId>, AppVerifyReductionError> {
    if results.len() != expected_call_ids.len() {
        return Err(AppVerifyReductionError::OutcomeCountMismatch);
    }

    let mut accepted = vec![None; expected_call_ids.len()];
    for result in results {
        let index = result.id.order_index;
        let Some(expected) = expected_call_ids.get(index) else {
            return Err(AppVerifyReductionError::OutcomeIdentityMismatch);
        };
        if *expected != result.id || accepted[index].replace(result.accepted).is_some() {
            return Err(AppVerifyReductionError::OutcomeIdentityMismatch);
        }
    }
    expected_call_ids
        .into_iter()
        .zip(accepted)
        .filter_map(|(id, accepted)| match accepted {
            Some(true) => None,
            Some(false) => Some(Ok(id)),
            None => Some(Err(AppVerifyReductionError::OutcomeIdentityMismatch)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(tag: u32) -> AppVerifyFamilyCode {
        AppVerifyFamilyCode {
            proof_family_id: tag,
            note_reshape_family_id: 0,
            shielded_withdrawal_family_id: 0,
        }
    }

    fn call(segment_index: usize, family_index: usize) -> AppVerifyExpectedCall {
        AppVerifyExpectedCall {
            segment_index,
            family_index,
            family: family((family_index + 1) as u32),
        }
    }

    #[test]
    fn preflight_checks_every_header_before_segment_planning() {
        let srs = vec![7u8; 32];
        assert_eq!(
            app_verify_preflight_core(2, 1, 0, srs.clone(), Vec::new(), 3, true, vec![2]),
            Err(AppVerifyPreflightError::BadVersion)
        );
        assert_eq!(
            app_verify_preflight_core(2, 2, 0, srs.clone(), srs.clone(), 3, true, vec![2]),
            Err(AppVerifyPreflightError::EmptyProofSet)
        );
        assert_eq!(
            app_verify_preflight_core(2, 2, 1, srs.clone(), vec![7; 31], 3, true, vec![2]),
            Err(AppVerifyPreflightError::BadSrsLength)
        );
        assert_eq!(
            app_verify_preflight_core(2, 2, 1, srs.clone(), vec![8; 32], 3, true, vec![2]),
            Err(AppVerifyPreflightError::SrsMismatch)
        );
        assert_eq!(
            app_verify_preflight_core(
                2,
                2,
                1,
                srs.clone(),
                srs.clone(),
                usize::MAX,
                true,
                vec![usize::MAX, 1],
            ),
            Err(AppVerifyPreflightError::SegmentCoverageOverflow)
        );
        assert_eq!(
            app_verify_preflight_core(2, 2, 1, srs.clone(), srs, 3, true, vec![2]),
            Err(AppVerifyPreflightError::SegmentCoverageMismatch)
        );

        let srs = vec![7u8; 32];
        assert_eq!(
            app_verify_preflight_core(2, 2, 1, srs.clone(), srs.clone(), 5, false, Vec::new()),
            Ok(vec![AppVerifySegmentRange {
                segment_index: 0,
                start: 0,
                end: 5,
            }])
        );
        assert_eq!(
            app_verify_preflight_core(2, 2, 1, srs.clone(), srs, 5, true, vec![2, 0, 3]),
            Ok(vec![
                AppVerifySegmentRange {
                    segment_index: 0,
                    start: 0,
                    end: 2,
                },
                AppVerifySegmentRange {
                    segment_index: 1,
                    start: 2,
                    end: 2,
                },
                AppVerifySegmentRange {
                    segment_index: 2,
                    start: 2,
                    end: 5,
                },
            ])
        );
    }

    #[test]
    fn ids_are_exact_and_reduction_is_permutation_invariant() {
        let expected = app_verify_plan_ids_core(vec![call(0, 0), call(1, 0), call(1, 1)]);
        assert_eq!(expected.len(), 3);
        assert_eq!(expected[0].order_index, 0);
        assert_eq!(expected[1].order_index, 1);
        assert_eq!(expected[2].order_index, 2);

        let outcomes = vec![
            AppVerifyCallResult {
                id: expected[2],
                accepted: false,
            },
            AppVerifyCallResult {
                id: expected[0],
                accepted: true,
            },
            AppVerifyCallResult {
                id: expected[1],
                accepted: true,
            },
        ];
        assert_eq!(
            app_verify_reduce_core(expected.clone(), outcomes.clone()),
            Ok(vec![expected[2]])
        );
    }

    #[test]
    fn accepted_join_projection_is_exact_and_fail_closed() {
        let expected = vec![
            AppVerifyCallId {
                order_index: 0,
                segment_index: 4,
                family_index: 1,
                family: family(1),
            },
            AppVerifyCallId {
                order_index: 1,
                segment_index: 7,
                family_index: 2,
                family: family(2),
            },
        ];
        let records = expected
            .iter()
            .copied()
            .zip(["first-execution", "second-execution"])
            .map(|(id, executed)| AppVerifyPlannerIndexedExecutedRecord {
                planner_id: id,
                authenticated_id: id,
                executed_id: id,
                accepted: true,
                observation: id.order_index,
                executed,
            })
            .collect::<Vec<_>>();

        let projected = app_verify_accepted_join_projection_core(expected.clone(), records.clone())
            .expect("exact accepted joins must project");
        assert_eq!(
            projected,
            AppVerifyAcceptedJoinProjection {
                records: records.clone(),
                rejected_calls: Vec::new(),
            },
            "the core must return records unchanged"
        );
        assert!(
            app_verify_join_acceptance_core(projected.rejected_calls.clone()),
            "an exact join with no rejected calls must accept"
        );

        let mut count_precedes_other_failures = records[..1].to_vec();
        count_precedes_other_failures[0].executed_id.segment_index += 1;
        count_precedes_other_failures[0].accepted = false;
        assert_eq!(
            app_verify_accepted_join_projection_core(
                expected.clone(),
                count_precedes_other_failures,
            ),
            Err(AppVerifyAcceptedJoinProjectionError::OutcomeCountMismatch {
                expected: 2,
                actual: 1,
            })
        );

        let full_id_mutations: [fn(&mut AppVerifyCallId); 6] = [
            |id: &mut AppVerifyCallId| id.order_index += 1,
            |id: &mut AppVerifyCallId| id.segment_index += 1,
            |id: &mut AppVerifyCallId| id.family_index += 1,
            |id: &mut AppVerifyCallId| id.family.proof_family_id += 1,
            |id: &mut AppVerifyCallId| id.family.note_reshape_family_id += 1,
            |id: &mut AppVerifyCallId| id.family.shielded_withdrawal_family_id += 1,
        ];
        for mutate in full_id_mutations {
            let mut bad_full_id = records.clone();
            mutate(&mut bad_full_id[0].executed_id);
            bad_full_id[0].accepted = false;
            assert_eq!(
                app_verify_accepted_join_projection_core(expected.clone(), bad_full_id),
                Err(AppVerifyAcceptedJoinProjectionError::FullIdentityMismatch { position: 0 })
            );
        }

        let mut bad_authenticated_id = records.clone();
        bad_authenticated_id[1].authenticated_id.segment_index += 1;
        assert_eq!(
            app_verify_accepted_join_projection_core(expected.clone(), bad_authenticated_id),
            Err(AppVerifyAcceptedJoinProjectionError::FullIdentityMismatch { position: 1 })
        );

        let mut reordered_and_rejected = vec![records[1].clone(), records[0].clone()];
        reordered_and_rejected[0].accepted = false;
        assert_eq!(
            app_verify_accepted_join_projection_core(expected.clone(), reordered_and_rejected,),
            Err(AppVerifyAcceptedJoinProjectionError::OutcomeOrderMismatch { position: 0 })
        );

        let mut rejected = records;
        rejected[1].accepted = false;
        let expected_rejected_records = rejected.clone();
        let rejected_projection =
            app_verify_accepted_join_projection_core(expected.clone(), rejected)
                .expect("a structurally exact rejected join must project");
        assert_eq!(
            rejected_projection,
            AppVerifyAcceptedJoinProjection {
                records: expected_rejected_records,
                rejected_calls: vec![expected[1]],
            }
        );
        assert!(
            !app_verify_join_acceptance_core(rejected_projection.rejected_calls),
            "a projected join retaining a rejected call must fail closed"
        );
    }

    #[test]
    fn plan_compares_concrete_family_and_count_values_in_failure_order() {
        let transfer = AppVerifyFamilyCode {
            proof_family_id: 1,
            note_reshape_family_id: 0,
            shielded_withdrawal_family_id: 0,
        };
        let reshape = AppVerifyFamilyCode {
            proof_family_id: 2,
            note_reshape_family_id: 1,
            shielded_withdrawal_family_id: 0,
        };
        let id = AppVerifyCallId {
            order_index: 3,
            segment_index: 1,
            family_index: 2,
            family: transfer,
        };
        assert_eq!(
            app_verify_family_count_core(2, 1),
            Err(AppVerifyPlanError::FamilyCountMismatch)
        );
        assert_eq!(
            app_verify_plan_identity_core(id, reshape, 4, 5),
            Err(AppVerifyPlanError::FamilyMismatch)
        );
        assert_eq!(
            app_verify_plan_identity_core(id, transfer, 4, 5),
            Err(AppVerifyPlanError::RealCountMismatch)
        );
        assert_eq!(app_verify_plan_identity_core(id, transfer, 4, 4), Ok(id));
        assert_eq!(
            app_verify_plan_identity_core(id, transfer, u32::MAX as usize + 1, 4),
            Err(AppVerifyPlanError::RealCountOverflow)
        );
        assert_eq!(
            app_verify_plan_padding_core(id, 8, 4),
            Err(AppVerifyPlanError::PaddedCountMismatch)
        );
        assert_eq!(
            app_verify_plan_padding_core(id, u32::MAX as usize + 1, 4),
            Err(AppVerifyPlanError::PaddedCountOverflow)
        );
        assert_eq!(app_verify_plan_padding_core(id, 8, 8), Ok(id));
    }

    #[test]
    fn shipping_projection_accepts_exact_input_and_preserves_failure_order() {
        let call = crate::app_verifier::AppVerifyShippingCall {
            id: AppVerifyCallId {
                order_index: 4,
                segment_index: 2,
                family_index: 1,
                family: family(7),
            },
            bundle_family: family(7),
            expected_real_count: 3,
            bundle_real_count: 3,
            expected_padded_count: 4,
            bundle_padded_count: 4,
        };

        assert_eq!(
            app_verify_shipping_projection_core(call, family(7), 3, 4),
            Ok(())
        );

        let bad_identity = AppVerifyShippingCall {
            bundle_family: family(9),
            expected_padded_count: 5,
            bundle_padded_count: 8,
            ..call
        };
        assert_eq!(
            app_verify_shipping_projection_core(bad_identity, family(8), 2, 8),
            Err(AppVerifyShippingInputError::CallIdentityMismatch)
        );

        let bad_padding = AppVerifyShippingCall {
            expected_padded_count: 5,
            bundle_padded_count: 8,
            ..call
        };
        assert_eq!(
            app_verify_shipping_projection_core(bad_padding, family(8), 2, 8),
            Err(AppVerifyShippingInputError::CallPaddingMismatch)
        );
        assert_eq!(
            app_verify_shipping_projection_core(call, family(8), 2, 8),
            Err(AppVerifyShippingInputError::StatementFamilyMismatch)
        );
        assert_eq!(
            app_verify_shipping_projection_core(call, family(7), 2, 8),
            Err(AppVerifyShippingInputError::StatementRealCountMismatch)
        );
        assert_eq!(
            app_verify_shipping_projection_core(call, family(7), 3, 8),
            Err(AppVerifyShippingInputError::StatementPaddedCountMismatch)
        );
    }

    #[test]
    fn shipping_preflight_projection_cores_preserve_exact_parts() {
        let fields = vec![vec![1u8], vec![2u8]];
        let serialized = vec![vec![vec![0x11]], vec![vec![0x22]]];
        let rows = crate::app_verifier::AppVerifyShippingRowsProjection {
            real_count: 2,
            padded_count: 2,
            public_input_arity: 1,
            fields: fields.as_slice(),
            serialized: serialized.as_slice(),
        };
        assert_eq!(rows.real_count, 2);
        assert_eq!(rows.padded_count, 2);
        assert_eq!(rows.public_input_arity, 1);
        assert_eq!(rows.fields, fields.as_slice());
        assert_eq!(rows.serialized, serialized.as_slice());

        let encoded = crate::app_verifier::AppVerifyStatementRowBytesProjection {
            source_rows: fields.as_slice(),
            serialized_rows: serialized.clone(),
        };
        assert_eq!(encoded.source_rows, fields.as_slice());
        assert_eq!(encoded.serialized_rows, serialized);

        let prepared =
            app_verify_prepare_public_input_rows_core(fields.clone(), 4).expect("valid padding");
        assert_eq!(prepared.real_count, 2);
        assert_eq!(prepared.padded_count, 4);
        assert_eq!(&prepared.padded_public_inputs[..2], fields.as_slice());
        assert_eq!(prepared.padded_public_inputs[2], fields[1]);
        assert_eq!(prepared.padded_public_inputs[3], fields[1]);
    }

    #[test]
    fn shipping_statement_preflight_retains_exact_padding_provenance() {
        let call = crate::app_verifier::AppVerifyShippingCall {
            id: AppVerifyCallId {
                order_index: 4,
                segment_index: 2,
                family_index: 1,
                family: family(7),
            },
            bundle_family: family(7),
            expected_real_count: 2,
            bundle_real_count: 2,
            expected_padded_count: 4,
            bundle_padded_count: 4,
        };
        let source_fields = vec![vec![0xb1, 0xb2], vec![0xc1, 0xc2]];
        let statement_fields = vec![
            source_fields[0].clone(),
            source_fields[1].clone(),
            source_fields[1].clone(),
            source_fields[1].clone(),
        ];
        let source_serialized_rows =
            vec![vec![vec![0x41], vec![0x42]], vec![vec![0x51], vec![0x52]]];
        let statement_serialized_rows = vec![
            source_serialized_rows[0].clone(),
            source_serialized_rows[1].clone(),
            vec![vec![0x51], vec![0x52]],
            vec![vec![0x51], vec![0x52]],
        ];
        let provenance = app_verify_shipping_statement_preflight_core(
            vec![0xa1, 0xa2],
            vec![0xd1, 0xd2],
            crate::app_verifier::AppVerifyStatementRowBytesProjection {
                source_rows: source_fields.clone(),
                serialized_rows: source_serialized_rows.clone(),
            },
            crate::app_verifier::AppVerifyShippingRowsProjection {
                real_count: 2,
                padded_count: 4,
                public_input_arity: 2,
                fields: statement_fields.clone(),
                serialized: statement_serialized_rows.clone(),
            },
            call,
            2,
            family(7),
            vec![0x11; 32],
            vec![0x22, 0x23],
            vec![0x33; 32],
            vec![0x61, 0x62],
            crate::app_verifier::AppVerifyShippingWrapperProjection {
                statement_digest: vec![0x71; 32],
                wrapped_proof_bytes: vec![0x81, 0x82, 0x83],
                inner_proof_bytes: vec![0x82, 0x83],
            },
            vec![0x91; 32],
        )
        .expect("matching statement provenance");

        assert_eq!(provenance.binding_execution, vec![0xd1, 0xd2]);
        assert_eq!(provenance.source_field_rows, source_fields);
        assert_eq!(
            &provenance.prepared_serialized_rows.padded_public_inputs
                [..source_serialized_rows.len()],
            source_serialized_rows.as_slice()
        );
        assert_eq!(provenance.prepared_serialized_rows.real_count, 2);
        assert_eq!(provenance.prepared_serialized_rows.padded_count, 4);
        assert_eq!(
            provenance.prepared_serialized_rows.padded_public_inputs,
            statement_serialized_rows
        );
        assert_eq!(
            provenance.preflight.padded_public_input_fields,
            statement_fields
        );
        assert_eq!(
            provenance.preflight.shipping_input.padded_public_inputs,
            provenance.prepared_serialized_rows.padded_public_inputs
        );

        let source_count_mismatch = app_verify_shipping_statement_preflight_core(
            (),
            (),
            crate::app_verifier::AppVerifyStatementRowBytesProjection {
                source_rows: vec![vec![0xb1, 0xb2], vec![0xc1, 0xc2]],
                serialized_rows: vec![vec![vec![0x41], vec![0x42]]],
            },
            crate::app_verifier::AppVerifyShippingRowsProjection {
                real_count: 2,
                padded_count: 4,
                public_input_arity: 2,
                fields: vec![
                    vec![0xb1, 0xb2],
                    vec![0xc1, 0xc2],
                    vec![0xc1, 0xc2],
                    vec![0xc1, 0xc2],
                ],
                serialized: vec![
                    vec![vec![0x41], vec![0x42]],
                    vec![vec![0x51], vec![0x52]],
                    vec![vec![0x51], vec![0x52]],
                    vec![vec![0x51], vec![0x52]],
                ],
            },
            call,
            2,
            family(7),
            vec![0x11; 32],
            vec![0x22, 0x23],
            vec![0x33; 32],
            vec![0x61, 0x62],
            crate::app_verifier::AppVerifyShippingWrapperProjection {
                statement_digest: vec![0x71; 32],
                wrapped_proof_bytes: vec![0x81, 0x82, 0x83],
                inner_proof_bytes: vec![0x82, 0x83],
            },
            vec![0x91; 32],
        );
        assert!(matches!(
            source_count_mismatch,
            Err(AppVerifyShippingStatementPreflightError::SourceSerializationCountMismatch)
        ));

        let serialized_mismatch = app_verify_shipping_statement_preflight_core(
            (),
            (),
            crate::app_verifier::AppVerifyStatementRowBytesProjection {
                source_rows: vec![vec![0xb1, 0xb2], vec![0xc1, 0xc2]],
                serialized_rows: vec![vec![vec![0x41], vec![0x42]], vec![vec![0x51], vec![0x52]]],
            },
            crate::app_verifier::AppVerifyShippingRowsProjection {
                real_count: 2,
                padded_count: 4,
                public_input_arity: 2,
                fields: vec![
                    vec![0xb1, 0xb2],
                    vec![0xc1, 0xc2],
                    vec![0xc1, 0xc2],
                    vec![0xc1, 0xc2],
                ],
                serialized: vec![
                    vec![vec![0x41], vec![0x42]],
                    vec![vec![0x51], vec![0x52]],
                    vec![vec![0x51], vec![0x52]],
                    vec![vec![0x51], vec![0x53]],
                ],
            },
            call,
            2,
            family(7),
            vec![0x11; 32],
            vec![0x22, 0x23],
            vec![0x33; 32],
            vec![0x61, 0x62],
            crate::app_verifier::AppVerifyShippingWrapperProjection {
                statement_digest: vec![0x71; 32],
                wrapped_proof_bytes: vec![0x81, 0x82, 0x83],
                inner_proof_bytes: vec![0x82, 0x83],
            },
            vec![0x91; 32],
        );
        assert!(matches!(
            serialized_mismatch,
            Err(AppVerifyShippingStatementPreflightError::StatementSerializedRowsMismatch)
        ));
    }

    #[test]
    fn shipping_input_and_result_preserve_every_authenticated_field() {
        let call = crate::app_verifier::AppVerifyShippingCall {
            id: AppVerifyCallId {
                order_index: 4,
                segment_index: 2,
                family_index: 1,
                family: family(7),
            },
            bundle_family: family(7),
            expected_real_count: 3,
            bundle_real_count: 3,
            expected_padded_count: 4,
            bundle_padded_count: 4,
        };
        let row_fields = vec![vec![0xb1, 0xb2], vec![0xc1, 0xc2]];
        let row_bytes = vec![vec![vec![0x41], vec![0x42]], vec![vec![0x51], vec![0x52]]];
        let preflight = app_verify_shipping_preflight_core(
            vec![0xa1, 0xa2],
            crate::app_verifier::AppVerifyShippingRowsProjection {
                real_count: 3,
                padded_count: 4,
                public_input_arity: 2,
                fields: row_fields.clone(),
                serialized: row_bytes.clone(),
            },
            call,
            2,
            family(7),
            vec![0x11; 32],
            vec![0x22, 0x23],
            vec![0x33; 32],
            vec![0x61, 0x62],
            crate::app_verifier::AppVerifyShippingWrapperProjection {
                statement_digest: vec![0x71; 32],
                wrapped_proof_bytes: vec![0x81, 0x82, 0x83],
                inner_proof_bytes: vec![0x82, 0x83],
            },
            vec![0x91; 32],
        )
        .expect("matching shipping input");
        let input = preflight.shipping_input.clone();
        assert_eq!(preflight.backend_call, vec![0xa1, 0xa2]);
        assert_eq!(preflight.padded_public_input_fields, row_fields);
        assert_eq!(input.call, call);
        assert_eq!(input.protocol_version, 2);
        assert_eq!(input.family, family(7));
        assert_eq!(input.srs_id, vec![0x11; 32]);
        assert_eq!(input.serialized_vk, vec![0x22, 0x23]);
        assert_eq!(input.vk_digest, vec![0x33; 32]);
        assert_eq!(input.real_count, 3);
        assert_eq!(input.padded_count, 4);
        assert_eq!(input.public_input_arity, 2);
        assert_eq!(input.padded_public_inputs, row_bytes);
        assert_eq!(input.canonical_statement_bytes, vec![0x61, 0x62]);
        assert_eq!(input.statement_digest, vec![0x71; 32]);
        assert_eq!(input.wrapped_proof_bytes, vec![0x81, 0x82, 0x83]);
        assert_eq!(input.inner_proof_bytes, vec![0x82, 0x83]);
        assert_eq!(input.challenge_context, vec![0x91; 32]);

        let backend_result = app_verify_shipping_backend_result_from_parts(
            vec![0xd1, 0xd2],
            AppVerifyCallResult {
                id: call.id,
                accepted: true,
            },
        );
        let executed =
            app_verify_shipping_result_from_backend_result(input.clone(), backend_result)
                .expect("matching retained backend result");
        let executed_result = app_verify_shipping_executed_result_result(&executed);
        assert_eq!(executed_result.input, input);
        assert_eq!(executed_result.result.id, call.id);
        assert!(executed_result.result.accepted);
        let (_, retained) = app_verify_shipping_executed_result_into_parts(executed);
        assert_eq!(
            retained.result,
            AppVerifyCallResult {
                id: call.id,
                accepted: true,
            }
        );

        let mut wrong_id = call.id;
        wrong_id.order_index += 1;
        let mismatched = app_verify_shipping_backend_result_from_parts(
            vec![0xe1],
            AppVerifyCallResult {
                id: wrong_id,
                accepted: true,
            },
        );
        assert_eq!(
            app_verify_shipping_result_from_backend_result(input.clone(), mismatched)
                .expect_err("unrelated retained backend result must reject"),
            AppVerifyShippingInputError::CallIdentityMismatch
        );

        let parts = app_verify_shipping_into_parts_core(preflight);
        assert_eq!(parts, (vec![0xa1, 0xa2], row_fields, input));
    }

    #[test]
    fn repeat_final_rows_preserves_prefix_and_fills_suffix() {
        let rows = vec![vec![1u8], vec![2u8], vec![3u8]];
        let padded = app_verify_repeat_final_rows_core(rows.clone(), 8).expect("valid padding");

        assert_eq!(padded.len(), 8);
        assert_eq!(&padded[..rows.len()], rows.as_slice());
        assert!(padded[rows.len()..].iter().all(|row| row == &rows[2]));
    }

    #[test]
    fn repeat_final_rows_accepts_only_the_empty_zero_target() {
        assert_eq!(
            app_verify_repeat_final_rows_core::<Vec<u8>>(Vec::new(), 0),
            Ok(Vec::new())
        );
        assert_eq!(
            app_verify_repeat_final_rows_core::<Vec<u8>>(Vec::new(), 1),
            Err(AppVerifyRowPaddingError::EmptyRowsForNonzeroTarget)
        );
    }

    #[test]
    fn repeat_final_rows_rejects_a_smaller_target() {
        assert_eq!(
            app_verify_repeat_final_rows_core(vec![1u8, 2, 3], 2),
            Err(AppVerifyRowPaddingError::TargetSmallerThanInput)
        );
    }

    #[test]
    fn shipping_input_rejects_cross_record_substitution() {
        let call = crate::app_verifier::AppVerifyShippingCall {
            id: AppVerifyCallId {
                order_index: 4,
                segment_index: 2,
                family_index: 1,
                family: family(7),
            },
            bundle_family: family(7),
            expected_real_count: 3,
            bundle_real_count: 3,
            expected_padded_count: 4,
            bundle_padded_count: 4,
        };
        let build = |call, statement_family, real_count, padded_count| {
            app_verify_shipping_input_from_parts(
                call,
                2,
                statement_family,
                vec![0x11; 32],
                vec![0x22],
                vec![0x33; 32],
                real_count,
                padded_count,
                1,
                vec![vec![vec![0x41]]; 4],
                vec![0x61],
                crate::app_verifier::AppVerifyShippingWrapperProjection {
                    statement_digest: vec![0x71; 32],
                    wrapped_proof_bytes: vec![0x81, 0x82],
                    inner_proof_bytes: vec![0x82],
                },
                vec![0x91; 32],
            )
        };

        assert_eq!(
            build(call, family(8), 3, 4),
            Err(AppVerifyShippingInputError::StatementFamilyMismatch)
        );
        assert_eq!(
            build(call, family(7), 2, 4),
            Err(AppVerifyShippingInputError::StatementRealCountMismatch)
        );
        assert_eq!(
            build(call, family(7), 3, 8),
            Err(AppVerifyShippingInputError::StatementPaddedCountMismatch)
        );

        let wrong_call_count = AppVerifyShippingCall {
            bundle_real_count: 2,
            ..call
        };
        assert_eq!(
            build(wrong_call_count, family(7), 2, 4),
            Err(AppVerifyShippingInputError::CallIdentityMismatch)
        );
        let wrong_call_padding = AppVerifyShippingCall {
            bundle_padded_count: 8,
            ..call
        };
        assert_eq!(
            build(wrong_call_padding, family(7), 3, 8),
            Err(AppVerifyShippingInputError::CallPaddingMismatch)
        );
    }

    #[test]
    fn reduction_rejects_missing_duplicate_and_unexpected_tags() {
        let expected = app_verify_plan_ids_core(vec![call(0, 0), call(1, 0)]);
        assert_eq!(
            app_verify_reduce_core(
                expected.clone(),
                vec![AppVerifyCallResult {
                    id: expected[0],
                    accepted: true,
                }],
            ),
            Err(AppVerifyReductionError::OutcomeCountMismatch)
        );
        assert_eq!(
            app_verify_reduce_core(
                expected.clone(),
                vec![
                    AppVerifyCallResult {
                        id: expected[0],
                        accepted: true,
                    },
                    AppVerifyCallResult {
                        id: expected[0],
                        accepted: true,
                    },
                ],
            ),
            Err(AppVerifyReductionError::OutcomeIdentityMismatch)
        );
        let unexpected = AppVerifyCallId {
            order_index: 1,
            segment_index: 9,
            family_index: 0,
            family: family(9),
        };
        assert_eq!(
            app_verify_reduce_core(
                expected.clone(),
                vec![
                    AppVerifyCallResult {
                        id: expected[0],
                        accepted: true,
                    },
                    AppVerifyCallResult {
                        id: unexpected,
                        accepted: true,
                    },
                ],
            ),
            Err(AppVerifyReductionError::OutcomeIdentityMismatch)
        );
    }

    #[test]
    fn reduction_checks_full_identity_before_accepting_indexed_results() {
        let expected = app_verify_plan_ids_core(vec![call(0, 0), call(1, 0)]);
        for field in 0..4 {
            let mut wrong = expected[1];
            match field {
                0 => wrong.order_index = usize::MAX,
                1 => wrong.segment_index += 1,
                2 => wrong.family_index += 1,
                _ => wrong.family = family(99),
            }
            assert_eq!(
                app_verify_reduce_core(
                    expected.clone(),
                    vec![
                        AppVerifyCallResult {
                            id: wrong,
                            accepted: true
                        },
                        AppVerifyCallResult {
                            id: expected[0],
                            accepted: true
                        },
                    ]
                ),
                Err(AppVerifyReductionError::OutcomeIdentityMismatch)
            );
        }
    }
}
