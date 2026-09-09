use ark_ff::ToConstraintField;
use decaf377::Fq;
use shieldd_sdk_compliance::{
    TRANSFER_CORE_CIPHERTEXT_FQS, TRANSFER_DETECTION_FQS, TRANSFER_EXT_CIPHERTEXT_FQS,
};
use shieldd_sdk_proof_params::statement_hash::hash_statement_fields;

use crate::{
    note_reshape::NoteReshapeProofPublic,
    shielded_ics20_withdrawal::ShieldedIcs20WithdrawalProofPublic,
    transfer::{TransferProofPublic, TransferSpendPublic},
    transfer::{TRANSFER_PROOF_LABEL, TRANSFER_STATEMENT_FIELD_COUNT},
    NoteReshapeFamilyId, NoteSeizureProofPublic, NOTE_SEIZURE_PROOF_LABEL,
    NOTE_SEIZURE_STATEMENT_FIELD_COUNT,
};

pub const NOTE_RESHAPE_STATEMENT_BASE_FIELDS: usize = 7;
pub const NOTE_RESHAPE_STATEMENT_FIELDS_PER_INPUT: usize = 3;
pub const NOTE_RESHAPE_STATEMENT_FIELDS_PER_OUTPUT: usize = 2;
pub const TRANSFER_STATEMENT_BASE_FIELDS: usize = 43;
pub const TRANSFER_STATEMENT_FIELDS_PER_INPUT: usize = 3;
pub const TRANSFER_STATEMENT_FIELDS_PER_OUTPUT: usize = 2;
pub const SHIELDED_ICS20_WITHDRAWAL_STATEMENT_BASE_FIELDS: usize = 25;
pub const SHIELDED_ICS20_WITHDRAWAL_STATEMENT_FIELDS_PER_INPUT: usize = 3;

pub const fn note_reshape_statement_field_count(n_in: usize, n_out: usize) -> usize {
    NOTE_RESHAPE_STATEMENT_BASE_FIELDS
        + NOTE_RESHAPE_STATEMENT_FIELDS_PER_INPUT * n_in
        + NOTE_RESHAPE_STATEMENT_FIELDS_PER_OUTPUT * n_out
}

pub const fn transfer_statement_field_count(n_in: usize, n_out: usize) -> usize {
    TRANSFER_STATEMENT_BASE_FIELDS
        + TRANSFER_STATEMENT_FIELDS_PER_INPUT * n_in
        + TRANSFER_STATEMENT_FIELDS_PER_OUTPUT * n_out
}

pub const fn shielded_ics20_withdrawal_statement_field_count(n_in: usize) -> usize {
    SHIELDED_ICS20_WITHDRAWAL_STATEMENT_BASE_FIELDS
        + SHIELDED_ICS20_WITHDRAWAL_STATEMENT_FIELDS_PER_INPUT * n_in
}

fn note_reshape_statement_hash_constant(family_id: NoteReshapeFamilyId, suffix: &str) -> Fq {
    let label = format!(
        "shieldd.shielded_pool.{}.public_input_hash.{suffix}",
        family_id.label()
    );
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}
fn transfer_statement_hash_constant(suffix: &str) -> Fq {
    let label = format!("shieldd.shielded_pool.{TRANSFER_PROOF_LABEL}.public_input_hash.{suffix}");
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}
fn shielded_ics20_withdrawal_statement_hash_constant(suffix: &str) -> Fq {
    let label =
        format!("shieldd.shielded_pool.shielded_ics20_withdrawal.public_input_hash.{suffix}");
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}
fn note_seizure_statement_hash_constant(suffix: &str) -> Fq {
    let label =
        format!("shieldd.shielded_pool.{NOTE_SEIZURE_PROOF_LABEL}.public_input_hash.{suffix}");
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}

#[derive(Debug, thiserror::Error)]
pub enum StatementHashError {
    #[error("invalid field length: expected {expected}, got {got}")]
    InvalidFieldLength { expected: usize, got: usize },
    #[error("failed to decompress randomized spend key")]
    DecompressRk(decaf377::EncodingError),
    #[error("failed converting {field} to constraint field elements")]
    FieldEncoding { field: String },
    #[error("invalid ciphertext field length for {label}: expected {expected}, got {got}")]
    InvalidCiphertextLength {
        label: String,
        expected: usize,
        got: usize,
    },
    #[error("invalid note seizure authorization: {0}")]
    InvalidNoteSeizureAuthorization(String),
}

fn transfer_rk_element(
    spend: &TransferSpendPublic,
) -> Result<decaf377::Element, StatementHashError> {
    decaf377::Encoding(spend.rk.to_bytes())
        .vartime_decompress()
        .map_err(StatementHashError::DecompressRk)
}

fn transfer_field_encoding_error(field: &str) -> StatementHashError {
    StatementHashError::FieldEncoding {
        field: field.to_owned(),
    }
}

fn note_reshape_field_encoding_error(field: &str) -> StatementHashError {
    StatementHashError::FieldEncoding {
        field: field.to_owned(),
    }
}

fn note_reshape_rk_element(
    rk: decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>,
) -> Result<decaf377::Element, StatementHashError> {
    decaf377::Encoding(rk.to_bytes())
        .vartime_decompress()
        .map_err(StatementHashError::DecompressRk)
}

trait NoteReshapeInputPublic {
    fn nullifier(&self) -> shieldd_sdk_sct::Nullifier;
    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>;
    fn history_required(&self) -> bool;
}

trait NoteReshapeOutputPublic {
    fn note_commitment(&self) -> shieldd_sdk_tct::StateCommitment;
    fn recovery_commitment(&self) -> crate::RecoveryCommitment;
}

impl NoteReshapeInputPublic for crate::NoteReshapeInputPublic {
    fn nullifier(&self) -> shieldd_sdk_sct::Nullifier {
        self.nullifier
    }

    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth> {
        self.rk
    }

    fn history_required(&self) -> bool {
        self.history_required
    }
}

impl NoteReshapeOutputPublic for crate::NoteReshapeOutputPublic {
    fn note_commitment(&self) -> shieldd_sdk_tct::StateCommitment {
        self.note_commitment
    }

    fn recovery_commitment(&self) -> crate::RecoveryCommitment {
        self.recovery_commitment
    }
}

impl NoteReshapeInputPublic
    for crate::shielded_ics20_withdrawal::ShieldedIcs20WithdrawalInputPublic
{
    fn nullifier(&self) -> shieldd_sdk_sct::Nullifier {
        self.nullifier
    }

    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth> {
        self.rk
    }

    fn history_required(&self) -> bool {
        self.history_required
    }
}

impl NoteReshapeOutputPublic
    for crate::shielded_ics20_withdrawal::ShieldedIcs20WithdrawalChangePublic
{
    fn note_commitment(&self) -> shieldd_sdk_tct::StateCommitment {
        self.note_commitment
    }

    fn recovery_commitment(&self) -> crate::RecoveryCommitment {
        self.recovery_commitment
    }
}

fn note_reshape_statement_fields_inner<I, O>(
    anchor: shieldd_sdk_tct::Root,
    balance_commitment: shieldd_sdk_asset::balance::Commitment,
    inputs: &[I],
    outputs: &[O],
    recent_position_floor: u64,
    expected: usize,
    field_encoding_error: fn(&str) -> StatementHashError,
) -> Result<Vec<Fq>, StatementHashError>
where
    I: NoteReshapeInputPublic,
    O: NoteReshapeOutputPublic,
{
    let mut fields = Vec::with_capacity(expected);
    fields.extend(
        Fq::from(anchor)
            .to_field_elements()
            .ok_or_else(|| field_encoding_error("anchor"))?,
    );
    for (index, output) in outputs.iter().enumerate() {
        fields.extend(
            output
                .note_commitment()
                .0
                .to_field_elements()
                .ok_or_else(|| field_encoding_error(&format!("note_commitment_{index}")))?,
        );
        fields.push(output.recovery_commitment().0);
    }
    fields.push(balance_commitment.0.vartime_compress_to_field());
    fields.push(Fq::from(recent_position_floor));
    for (index, input) in inputs.iter().enumerate() {
        fields.extend(
            input
                .nullifier()
                .0
                .to_field_elements()
                .ok_or_else(|| field_encoding_error(&format!("nullifier_{index}")))?,
        );
        fields.push(note_reshape_rk_element(input.rk())?.vartime_compress_to_field());
        fields.push(Fq::from(input.history_required()));
    }

    if fields.len() != expected {
        return Err(StatementHashError::InvalidFieldLength {
            expected,
            got: fields.len(),
        });
    }

    Ok(fields)
}

pub fn note_reshape_statement_fields(
    public: &NoteReshapeProofPublic,
) -> Result<Vec<Fq>, StatementHashError> {
    public
        .validate_shape()
        .map_err(|e| note_reshape_field_encoding_error(&e.to_string()))?;

    let expected = note_reshape_statement_field_count(
        public.family_id.input_count(),
        public.family_id.output_count(),
    );
    let mut fields = note_reshape_statement_fields_inner(
        public.anchor,
        public.balance_commitment,
        &public.inputs,
        &public.outputs,
        public.recent_position_floor,
        expected - 4,
        note_reshape_field_encoding_error,
    )?;
    let routing_offset = 2 + 2 * public.outputs.len();
    fields.splice(
        routing_offset..routing_offset,
        [
            public.asset_anchor.0,
            public.compliance_anchor.0,
            Fq::from(public.routing_tag.value),
            public.routing_parameter_set_id,
        ],
    );
    if fields.len() != expected {
        return Err(StatementHashError::InvalidFieldLength {
            expected,
            got: fields.len(),
        });
    }
    Ok(fields)
}

pub fn transfer_statement_fields(
    public: &TransferProofPublic,
) -> Result<Vec<Fq>, StatementHashError> {
    use StatementHashError::{InvalidCiphertextLength, InvalidFieldLength};

    public
        .validate_shape()
        .map_err(|e| transfer_field_encoding_error(&e.to_string()))?;

    let compliance = &public.compliance;
    for (label, ciphertext, expected) in [
        (
            "detection_ciphertext",
            compliance.detection_ciphertext.len(),
            TRANSFER_DETECTION_FQS,
        ),
        (
            "sender_core_ciphertext",
            compliance.sender_core.ciphertext.len(),
            TRANSFER_CORE_CIPHERTEXT_FQS,
        ),
        (
            "sender_ext_ciphertext",
            compliance.sender_ext.ciphertext.len(),
            TRANSFER_EXT_CIPHERTEXT_FQS,
        ),
        (
            "output_core_ciphertext",
            compliance.output_core.ciphertext.len(),
            TRANSFER_CORE_CIPHERTEXT_FQS,
        ),
        (
            "output_ext_ciphertext",
            compliance.output_ext.ciphertext.len(),
            TRANSFER_EXT_CIPHERTEXT_FQS,
        ),
    ] {
        if ciphertext != expected {
            return Err(InvalidCiphertextLength {
                label: label.to_owned(),
                expected,
                got: ciphertext,
            });
        }
    }

    let mut fields = Vec::with_capacity(TRANSFER_STATEMENT_FIELD_COUNT);
    fields.extend(
        Fq::from(public.anchor)
            .to_field_elements()
            .ok_or_else(|| transfer_field_encoding_error("anchor"))?,
    );
    for (index, output) in public.outputs.iter().enumerate() {
        fields.extend(
            output
                .note_commitment
                .0
                .to_field_elements()
                .ok_or_else(|| {
                    transfer_field_encoding_error(&format!("note_commitment_{index}"))
                })?,
        );
        fields.push(output.recovery_commitment.0);
    }
    fields.push(public.balance_commitment.0.vartime_compress_to_field());
    fields.extend(public.routing.tags.map(|tag| Fq::from(tag.value)));
    fields.push(public.routing_parameter_set_id);
    fields.push(Fq::from(public.recent_position_floor));
    fields.push(public.volume_accumulator.nullifier.0);
    fields.push(public.volume_accumulator.commitment.0);
    fields.push(Fq::from(public.volume_accumulator.day_start));
    fields.push(public.proof_context.as_field());
    for (index, spend) in public.inputs.iter().enumerate() {
        fields.extend(
            spend
                .nullifier
                .0
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("nullifier_{index}")))?,
        );
        fields.push(transfer_rk_element(spend)?.vartime_compress_to_field());
        fields.push(Fq::from(spend.history_required));
    }
    fields.extend(
        public
            .asset_anchor
            .0
            .to_field_elements()
            .ok_or_else(|| transfer_field_encoding_error("asset_anchor"))?,
    );
    fields.extend(
        public
            .compliance_anchor
            .0
            .to_field_elements()
            .ok_or_else(|| transfer_field_encoding_error("compliance_anchor"))?,
    );
    fields.extend(compliance.detection_ciphertext.iter().copied());
    for (label, tier) in [
        ("sender_core", &compliance.sender_core),
        ("sender_ext", &compliance.sender_ext),
        ("output_core", &compliance.output_core),
        ("output_ext", &compliance.output_ext),
    ] {
        fields.push(tier.epk.vartime_compress_to_field());
        fields.extend(
            tier.c2
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("{label}_c2")))?,
        );
        fields.extend(tier.ciphertext.iter().copied());
    }
    fields.extend(
        public
            .target_timestamp
            .to_field_elements()
            .ok_or_else(|| transfer_field_encoding_error("target_timestamp"))?,
    );
    fields.push(compliance.sender_core_key_confirmation);
    fields.push(compliance.output_core_key_confirmation);
    let metadata = &compliance.metadata;
    metadata
        .validate()
        .map_err(|e| transfer_field_encoding_error(&format!("transfer_metadata: {e}")))?;
    if public.target_timestamp != Fq::from(metadata.target_timestamp) {
        return Err(transfer_field_encoding_error(
            "transfer_metadata_target_timestamp",
        ));
    }
    for (label, value) in [
        ("transfer_ring_id_hash", metadata.ring_id_hash()),
        ("transfer_policy_id_hash", metadata.policy_id_hash()),
        ("transfer_resource_hash", metadata.resource_hash()),
        ("transfer_permission_hash", metadata.permission_hash()),
        ("transfer_sender_core_salt", metadata.sender_core_salt()),
        ("transfer_sender_ext_salt", metadata.sender_ext_salt()),
        ("transfer_output_core_salt", metadata.output_core_salt()),
        ("transfer_output_ext_salt", metadata.output_ext_salt()),
    ] {
        let value = value.map_err(|e| transfer_field_encoding_error(&format!("{label}: {e}")))?;
        fields.extend(
            value
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(label))?,
        );
    }

    let expected = TRANSFER_STATEMENT_FIELD_COUNT;
    if fields.len() != expected {
        return Err(InvalidFieldLength {
            expected,
            got: fields.len(),
        });
    }

    Ok(fields)
}

pub fn shielded_ics20_withdrawal_statement_fields(
    public: &ShieldedIcs20WithdrawalProofPublic,
) -> Result<Vec<Fq>, StatementHashError> {
    public
        .validate_shape()
        .map_err(|e| StatementHashError::FieldEncoding {
            field: e.to_string(),
        })?;

    let expected = shielded_ics20_withdrawal_statement_field_count(public.family_id.input_count());
    let mut fields = note_reshape_statement_fields_inner(
        public.anchor,
        public.balance_commitment,
        &public.inputs,
        std::slice::from_ref(&public.change_output),
        public.recent_position_floor,
        5 + 3 * public.inputs.len(),
        |field| StatementHashError::FieldEncoding {
            field: field.to_owned(),
        },
    )?;
    fields.extend(public.asset_anchor.0.to_field_elements().ok_or_else(|| {
        StatementHashError::FieldEncoding {
            field: "asset_anchor".to_owned(),
        }
    })?);
    fields.extend(
        public
            .compliance_anchor
            .0
            .to_field_elements()
            .ok_or_else(|| StatementHashError::FieldEncoding {
                field: "compliance_anchor".to_owned(),
            })?,
    );
    fields.extend(public.target_timestamp.to_field_elements().ok_or_else(|| {
        StatementHashError::FieldEncoding {
            field: "target_timestamp".to_owned(),
        }
    })?);
    fields.extend(
        public
            .outbound_asset_id
            .to_field_elements()
            .ok_or_else(|| StatementHashError::FieldEncoding {
                field: "outbound_asset_id".to_owned(),
            })?,
    );
    fields.extend(public.outbound_amount.to_field_elements().ok_or_else(|| {
        StatementHashError::FieldEncoding {
            field: "outbound_amount".to_owned(),
        }
    })?);
    fields.extend(public.withdrawal_effect_hash_limbs);
    fields.push(Fq::from(public.routing_tag.value));
    fields.push(public.routing_parameter_set_id);
    fields.push(public.volume_accumulator.nullifier.0);
    fields.push(public.volume_accumulator.commitment.0);
    fields.push(Fq::from(public.volume_accumulator.day_start));
    fields.push(
        public
            .withdrawal_compliance_ciphertext
            .epk
            .vartime_compress_to_field(),
    );
    fields.push(public.withdrawal_compliance_ciphertext.c2);
    fields.push(public.withdrawal_compliance_ciphertext.key_confirmation);
    for word in public
        .withdrawal_compliance_ciphertext
        .encrypted_sender_address
        .chunks_exact(32)
    {
        fields.push(
            Fq::from_bytes_checked(word.try_into().expect("32-byte ciphertext word"))
                .expect("typed withdrawal ciphertext contains canonical fields"),
        );
    }

    if fields.len() != expected {
        return Err(StatementHashError::InvalidFieldLength {
            expected,
            got: fields.len(),
        });
    }

    Ok(fields)
}

pub fn note_seizure_statement_fields(
    public: &NoteSeizureProofPublic,
) -> Result<Vec<Fq>, StatementHashError> {
    let authorization_commitment = public
        .authorization
        .commitment()
        .map_err(|error| StatementHashError::InvalidNoteSeizureAuthorization(error.to_string()))?;
    let capsule = &public.recovery_capsule;
    let fields = vec![
        Fq::from(public.anchor),
        public.authorization.note_commitment.0,
        public.authorization.nullifier.0,
        Fq::from(public.history_required),
        Fq::from(public.recent_position_floor),
        public
            .authorization
            .address
            .diversified_generator()
            .vartime_compress_to_field(),
        *public.authorization.address.transmission_key_s(),
        public.authorization.asset_id.0,
        Fq::from(public.authorization.amount),
        capsule.commitment().0,
        capsule.epk.vartime_compress_to_field(),
        capsule.c2,
        capsule.salt,
        capsule.key_confirmation,
        capsule.encrypted_amount,
        capsule.encrypted_note_blinding,
        public.recovery_seed,
        public.rnk_commitment,
        authorization_commitment,
    ];
    debug_assert_eq!(fields.len(), NOTE_SEIZURE_STATEMENT_FIELD_COUNT);
    Ok(fields)
}

pub fn note_reshape_statement_hash(
    family_id: NoteReshapeFamilyId,
    fields: &[Fq],
) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &note_reshape_statement_hash_constant(family_id, "statement"),
        note_reshape_statement_hash_constant(family_id, "pad0"),
        note_reshape_statement_hash_constant(family_id, "pad1"),
        fields,
        note_reshape_statement_field_count(family_id.input_count(), family_id.output_count()),
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn transfer_statement_hash(fields: &[Fq]) -> Result<Fq, StatementHashError> {
    let domain = transfer_statement_hash_constant("statement");
    let pad_0 = transfer_statement_hash_constant("pad0");
    let pad_1 = transfer_statement_hash_constant("pad1");
    hash_statement_fields(
        &domain,
        pad_0,
        pad_1,
        fields,
        TRANSFER_STATEMENT_FIELD_COUNT,
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn shielded_ics20_withdrawal_statement_hash(fields: &[Fq]) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &shielded_ics20_withdrawal_statement_hash_constant("statement"),
        shielded_ics20_withdrawal_statement_hash_constant("pad0"),
        shielded_ics20_withdrawal_statement_hash_constant("pad1"),
        fields,
        shielded_ics20_withdrawal_statement_field_count(2),
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn note_seizure_statement_hash(fields: &[Fq]) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &note_seizure_statement_hash_constant("statement"),
        note_seizure_statement_hash_constant("pad0"),
        note_seizure_statement_hash_constant("pad1"),
        fields,
        NOTE_SEIZURE_STATEMENT_FIELD_COUNT,
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn note_reshape_statement_hash_from_public(
    public: &NoteReshapeProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = note_reshape_statement_fields(public)?;
    note_reshape_statement_hash(public.family_id, &fields)
}

pub fn transfer_statement_hash_from_public(
    public: &TransferProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = transfer_statement_fields(public)?;
    transfer_statement_hash(&fields)
}

pub fn shielded_ics20_withdrawal_statement_hash_from_public(
    public: &ShieldedIcs20WithdrawalProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = shielded_ics20_withdrawal_statement_fields(public)?;
    shielded_ics20_withdrawal_statement_hash(&fields)
}

pub fn note_seizure_statement_hash_from_public(
    public: &NoteSeizureProofPublic,
) -> Result<Fq, StatementHashError> {
    note_seizure_statement_hash(&note_seizure_statement_fields(public)?)
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests {
    use super::test_support::*;

    #[test]
    fn canonical_element_projection_matches_reference() {
        for i in 0..64 {
            let element = decaf377::Element::GENERATOR * decaf377::Fr::from(i as u64);
            assert_eq!(
                element.to_field_elements().unwrap(),
                vec![element.vartime_compress_to_field()]
            );
        }
    }

    use super::*;
    use crate::{
        test_proof_helpers::proof_test_helpers, transfer_input_count, transfer_output_count,
    };
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget};
    use ark_relations::r1cs::ConstraintSystem;
    use decaf377::r1cs::FqVar;
    use decaf377::Fq;

    fn go_fixture_statement_hash(path: &str) -> (NoteReshapeFamilyId, Fq) {
        let bytes = std::fs::read(path).expect("read Go note reshape fixture");
        let witness = crate::gnark::decode_note_reshape_witness(&bytes)
            .expect("decode Go note reshape fixture");
        let mut fields = Vec::with_capacity(note_reshape_statement_field_count(
            witness.n_in as usize,
            witness.n_out as usize,
        ));
        fields.push(Fq::from_le_bytes_mod_order(&witness.anchor));
        for output in &witness.outputs {
            fields.push(Fq::from_le_bytes_mod_order(&output.note_commitment));
            fields.push(Fq::from_le_bytes_mod_order(&output.recovery_commitment));
        }
        fields.push(Fq::from_le_bytes_mod_order(
            &crate::gnark::point_affine_compress_to_field_bytes(&witness.balance_commitment_affine),
        ));
        fields.push(Fq::from_le_bytes_mod_order(&witness.asset_anchor));
        fields.push(Fq::from_le_bytes_mod_order(&witness.compliance_anchor));
        fields.push(Fq::from_le_bytes_mod_order(&witness.routing_tag));
        fields.push(Fq::from_le_bytes_mod_order(
            &witness.routing_parameter_set_id,
        ));
        fields.push(Fq::from_le_bytes_mod_order(&witness.recent_position_floor));
        for spend in &witness.spends {
            fields.push(Fq::from_le_bytes_mod_order(&spend.nullifier));
            fields.push(Fq::from_le_bytes_mod_order(
                &crate::gnark::point_affine_compress_to_field_bytes(&spend.rk_affine),
            ));
            fields.push(Fq::from(spend.history_required));
        }
        let hash = note_reshape_statement_hash(witness.family_id, &fields)
            .expect("hash Go note reshape statement fields");
        (witness.family_id, hash)
    }

    #[test]
    fn note_reshape_statement_hash_native_matches_r1cs() {
        for family_id in NoteReshapeFamilyId::ALL {
            let fields = (0..note_reshape_statement_field_count(
                family_id.input_count(),
                family_id.output_count(),
            ))
                .map(|i| Fq::from((i as u64) + 1))
                .collect::<Vec<_>>();
            let native = note_reshape_statement_hash(family_id, &fields)
                .expect("native hash should succeed");

            let cs = ConstraintSystem::<Fq>::new_ref();
            let vars = fields
                .iter()
                .map(|f| FqVar::new_witness(cs.clone(), || Ok(*f)).expect("witness allocation"))
                .collect::<Vec<_>>();
            let var_hash = note_reshape_statement_hash_var(cs.clone(), family_id, &vars)
                .expect("r1cs hash should work");
            let constrained_native = FqVar::new_witness(cs.clone(), || Ok(native))
                .expect("native witness allocation should work");
            var_hash
                .enforce_equal(&constrained_native)
                .expect("hashes must be equal");
            assert!(cs.is_satisfied().expect("cs should evaluate"));
        }
    }

    #[test]
    fn note_reshape_statement_hash_matches_go_fixtures_for_all_families() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("tools/gnark/internal/testfixtures/vectors");
        for (label, filename) in [
            ("note_reshape1x8", "note_reshape1x8_witness.bin"),
            ("note_reshape8x1", "note_reshape8x1_witness.bin"),
        ] {
            let (family_id, hash) = go_fixture_statement_hash(
                root.join(filename).to_str().expect("fixture path is UTF-8"),
            );
            assert_eq!(family_id.label(), label);
            let bytes = std::fs::read(root.join(filename)).expect("read Go note reshape fixture");
            let witness = crate::gnark::decode_note_reshape_witness(&bytes)
                .expect("decode Go note reshape fixture");
            assert_eq!(
                hash,
                Fq::from_le_bytes_mod_order(&witness.claimed_statement_hash),
                "Rust/Go statement hash mismatch for {label}"
            );
        }
    }

    #[test]
    fn note_reshape_wrong_family_domain_changes_the_statement_hash() {
        let family_id = NoteReshapeFamilyId::EightByOne;
        let fields = (0..note_reshape_statement_field_count(
            family_id.input_count(),
            family_id.output_count(),
        ))
            .map(|index| Fq::from((index + 1) as u64))
            .collect::<Vec<_>>();
        let correct =
            note_reshape_statement_hash(family_id, &fields).expect("correct family statement hash");
        let wrong = hash_statement_fields(
            &note_reshape_statement_hash_constant(NoteReshapeFamilyId::OneByEight, "alternate"),
            note_reshape_statement_hash_constant(NoteReshapeFamilyId::OneByEight, "pad0"),
            note_reshape_statement_hash_constant(NoteReshapeFamilyId::OneByEight, "pad1"),
            &fields,
            fields.len(),
            |expected, got| StatementHashError::InvalidFieldLength { expected, got },
        )
        .expect("wrong family domain hash should still be well-formed");
        assert_ne!(correct, wrong, "family domain must bind the statement");
    }

    #[test]
    fn note_reshape_wrong_statement_preimage_changes_the_hash_for_all_families() {
        for family_id in NoteReshapeFamilyId::ALL {
            let (public, _) = proof_test_helpers::build_note_reshape_roundtrip_inputs(family_id);
            let fields = note_reshape_statement_fields(&public).expect("statement fields");
            let correct =
                note_reshape_statement_hash(family_id, &fields).expect("correct statement hash");
            let mut mutated = fields;
            mutated[0] += Fq::from(1u64);
            let wrong =
                note_reshape_statement_hash(family_id, &mutated).expect("mutated statement hash");
            assert_ne!(
                correct,
                wrong,
                "{} preimage mutation was ignored",
                family_id.label()
            );
        }
    }

    #[test]
    fn note_reshape_statement_field_count_matches_family_shape() {
        for family_id in NoteReshapeFamilyId::ALL {
            let core = NOTE_RESHAPE_STATEMENT_BASE_FIELDS
                + NOTE_RESHAPE_STATEMENT_FIELDS_PER_INPUT * family_id.input_count()
                + NOTE_RESHAPE_STATEMENT_FIELDS_PER_OUTPUT * family_id.output_count();
            assert_eq!(
                note_reshape_statement_field_count(
                    family_id.input_count(),
                    family_id.output_count()
                ),
                core,
                "{} statement field count mismatch",
                family_id.label()
            );
            let (public, _) = proof_test_helpers::build_note_reshape_roundtrip_inputs(family_id);
            assert_eq!(
                note_reshape_statement_fields(&public)
                    .expect("statement fields")
                    .len(),
                core,
                "{} statement preimage length mismatch",
                family_id.label()
            );
        }
    }

    #[test]
    fn transfer_statement_hash_native_matches_r1cs() {
        let fields =
            (0..transfer_statement_field_count(transfer_input_count(), transfer_output_count()))
                .map(|i| Fq::from((i as u64) + 1))
                .collect::<Vec<_>>();
        let native = transfer_statement_hash(&fields).expect("native hash should succeed");

        let cs = ConstraintSystem::<Fq>::new_ref();
        let vars = fields
            .iter()
            .map(|f| FqVar::new_witness(cs.clone(), || Ok(*f)).expect("witness allocation"))
            .collect::<Vec<_>>();
        let var_hash =
            transfer_statement_hash_var(cs.clone(), &vars).expect("r1cs hash should work");
        let constrained_native = FqVar::new_witness(cs.clone(), || Ok(native))
            .expect("native witness allocation should work");
        var_hash
            .enforce_equal(&constrained_native)
            .expect("hashes must be equal");
        assert!(cs.is_satisfied().expect("cs should evaluate"));
    }

    #[test]
    fn transfer_statement_binds_core_confirmation_and_private_metadata() {
        let (public, _) = proof_test_helpers::build_transfer_roundtrip_inputs(true);
        let fields = transfer_statement_fields(&public).expect("transfer statement fields");
        assert_eq!(fields.len(), TRANSFER_STATEMENT_FIELD_COUNT);

        let metadata = &public.compliance.metadata;
        let expected_metadata = [
            metadata.ring_id_hash().unwrap(),
            metadata.policy_id_hash().unwrap(),
            metadata.resource_hash().unwrap(),
            metadata.permission_hash().unwrap(),
            metadata.sender_core_salt().unwrap(),
            metadata.sender_ext_salt().unwrap(),
            metadata.output_core_salt().unwrap(),
            metadata.output_ext_salt().unwrap(),
        ];
        let metadata_start = fields.len() - expected_metadata.len();
        assert_eq!(fields[metadata_start - 3], public.target_timestamp);
        assert_eq!(
            fields[metadata_start - 2],
            public.compliance.sender_core_key_confirmation
        );
        assert_eq!(
            fields[metadata_start - 1],
            public.compliance.output_core_key_confirmation
        );
        assert_eq!(&fields[metadata_start..], expected_metadata.as_slice());

        let hash = transfer_statement_hash(&fields).expect("transfer hash");
        let alternate_domain_hash = hash_statement_fields(
            &transfer_statement_hash_constant("alternate"),
            transfer_statement_hash_constant("pad0"),
            transfer_statement_hash_constant("pad1"),
            &fields,
            TRANSFER_STATEMENT_FIELD_COUNT,
            |expected, got| StatementHashError::InvalidFieldLength { expected, got },
        )
        .expect("alternate domain hash");
        assert_ne!(hash, alternate_domain_hash);
    }

    #[test]
    fn transfer_statement_rejects_metadata_timestamp_drift() {
        let (mut public, _) = proof_test_helpers::build_transfer_roundtrip_inputs(true);
        public.compliance.metadata.target_timestamp += 1;
        assert!(
            transfer_statement_fields(&public).is_err(),
            "metadata timestamp must equal the action timestamp"
        );
    }

    #[test]
    fn transfer_statement_hash_commits_to_every_factored_metadata_field() {
        let (public, _) = proof_test_helpers::build_transfer_roundtrip_inputs(true);
        let original = public.statement_hash().expect("baseline transfer hash");

        fn increment(bytes: &mut [u8; 32]) {
            let value = Fq::from_bytes_checked(bytes).expect("fixture field is canonical");
            *bytes = (value + Fq::from(1u64)).to_bytes();
        }
        let mutations: [fn(&mut shieldd_sdk_compliance::TransferComplianceMetadata); 8] = [
            |m| increment(&mut m.ring_id_hash_bytes),
            |m| increment(&mut m.policy_id_hash_bytes),
            |m| increment(&mut m.resource_hash_bytes),
            |m| increment(&mut m.permission_hash_bytes),
            |m| increment(&mut m.sender_core_salt_bytes),
            |m| increment(&mut m.sender_ext_salt_bytes),
            |m| increment(&mut m.output_core_salt_bytes),
            |m| increment(&mut m.output_ext_salt_bytes),
        ];
        for mutate in mutations {
            let mut changed = public.clone();
            mutate(&mut changed.compliance.metadata);
            assert_ne!(
                changed.statement_hash().expect("mutated transfer hash"),
                original
            );
        }
    }
}
