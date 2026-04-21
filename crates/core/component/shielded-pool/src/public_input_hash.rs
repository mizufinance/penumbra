use ark_ff::ToConstraintField;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::{r1cs::FqVar, Fq};
use penumbra_sdk_compliance::{
    TRANSFER_CORE_CIPHERTEXT_FQS, TRANSFER_DETECTION_FQS, TRANSFER_EXT_CIPHERTEXT_FQS,
};
use penumbra_sdk_proof_params::statement_hash::{hash_statement_fields, hash_statement_fields_var};

use crate::{
    consolidate::ConsolidateProofPublic,
    shielded_ics20_withdrawal::ShieldedIcs20WithdrawalProofPublic,
    split::SplitProofPublic,
    transfer::{TransferProofPublic, TransferSpendPublic},
    transfer::{TRANSFER_PROOF_LABEL, TRANSFER_STATEMENT_FIELD_COUNT},
    ConsolidateFamilyId, SplitFamilyId,
};

pub const CONSOLIDATE_STATEMENT_BASE_FIELDS: usize = 2;
pub const CONSOLIDATE_STATEMENT_FIELDS_PER_INPUT: usize = 2;
pub const CONSOLIDATE_STATEMENT_FIELDS_PER_OUTPUT: usize = 1;
pub const SPLIT_STATEMENT_BASE_FIELDS: usize = 2;
pub const SPLIT_STATEMENT_FIELDS_PER_INPUT: usize = 2;
pub const SPLIT_STATEMENT_FIELDS_PER_OUTPUT: usize = 1;
pub const TRANSFER_STATEMENT_BASE_FIELDS: usize = 31;
pub const TRANSFER_STATEMENT_FIELDS_PER_INPUT: usize = 2;
pub const TRANSFER_STATEMENT_FIELDS_PER_OUTPUT: usize = 1;
pub const SHIELDED_ICS20_WITHDRAWAL_STATEMENT_BASE_FIELDS: usize = 10;
pub const SHIELDED_ICS20_WITHDRAWAL_STATEMENT_FIELDS_PER_INPUT: usize = 2;

pub const fn consolidate_statement_field_count(n_in: usize, n_out: usize) -> usize {
    CONSOLIDATE_STATEMENT_BASE_FIELDS
        + CONSOLIDATE_STATEMENT_FIELDS_PER_INPUT * n_in
        + CONSOLIDATE_STATEMENT_FIELDS_PER_OUTPUT * n_out
}

pub const fn split_statement_field_count(n_in: usize, n_out: usize) -> usize {
    SPLIT_STATEMENT_BASE_FIELDS
        + SPLIT_STATEMENT_FIELDS_PER_INPUT * n_in
        + SPLIT_STATEMENT_FIELDS_PER_OUTPUT * n_out
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

fn consolidate_statement_hash_constant(family_id: ConsolidateFamilyId, suffix: &str) -> Fq {
    let label = format!(
        "penumbra.shielded_pool.{}.public_input_hash.{suffix}",
        family_id.label()
    );
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}
fn split_statement_hash_constant(family_id: SplitFamilyId, suffix: &str) -> Fq {
    let label = format!(
        "penumbra.shielded_pool.{}.public_input_hash.{suffix}",
        family_id.label()
    );
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}
fn transfer_statement_hash_constant(suffix: &str) -> Fq {
    let label = format!("penumbra.shielded_pool.{TRANSFER_PROOF_LABEL}.public_input_hash.{suffix}");
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label.as_bytes()).as_bytes())
}
fn shielded_ics20_withdrawal_statement_hash_constant(suffix: &str) -> Fq {
    let label =
        format!("penumbra.shielded_pool.shielded_ics20_withdrawal.public_input_hash.{suffix}");
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

fn consolidate_field_encoding_error(field: &str) -> StatementHashError {
    StatementHashError::FieldEncoding {
        field: field.to_owned(),
    }
}

fn split_field_encoding_error(field: &str) -> StatementHashError {
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
    fn nullifier(&self) -> penumbra_sdk_sct::Nullifier;
    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>;
}

trait NoteReshapeOutputPublic {
    fn note_commitment(&self) -> penumbra_sdk_tct::StateCommitment;
}

impl NoteReshapeInputPublic for crate::ConsolidateInputPublic {
    fn nullifier(&self) -> penumbra_sdk_sct::Nullifier {
        self.nullifier
    }

    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth> {
        self.rk
    }
}

impl NoteReshapeOutputPublic for crate::ConsolidateOutputPublic {
    fn note_commitment(&self) -> penumbra_sdk_tct::StateCommitment {
        self.note_commitment
    }
}

impl NoteReshapeInputPublic for crate::SplitInputPublic {
    fn nullifier(&self) -> penumbra_sdk_sct::Nullifier {
        self.nullifier
    }

    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth> {
        self.rk
    }
}

impl NoteReshapeOutputPublic for crate::SplitOutputPublic {
    fn note_commitment(&self) -> penumbra_sdk_tct::StateCommitment {
        self.note_commitment
    }
}

impl NoteReshapeInputPublic
    for crate::shielded_ics20_withdrawal::ShieldedIcs20WithdrawalInputPublic
{
    fn nullifier(&self) -> penumbra_sdk_sct::Nullifier {
        self.nullifier
    }

    fn rk(&self) -> decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth> {
        self.rk
    }
}

impl NoteReshapeOutputPublic
    for crate::shielded_ics20_withdrawal::ShieldedIcs20WithdrawalChangePublic
{
    fn note_commitment(&self) -> penumbra_sdk_tct::StateCommitment {
        self.note_commitment
    }
}

fn note_reshape_statement_fields<I, O>(
    anchor: penumbra_sdk_tct::Root,
    balance_commitment: penumbra_sdk_asset::balance::Commitment,
    inputs: &[I],
    outputs: &[O],
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
    }
    fields.extend(
        balance_commitment
            .0
            .to_field_elements()
            .ok_or_else(|| field_encoding_error("balance_commitment"))?,
    );
    for (index, input) in inputs.iter().enumerate() {
        fields.extend(
            input
                .nullifier()
                .0
                .to_field_elements()
                .ok_or_else(|| field_encoding_error(&format!("nullifier_{index}")))?,
        );
        fields.extend(
            note_reshape_rk_element(input.rk())?
                .to_field_elements()
                .ok_or_else(|| field_encoding_error(&format!("rk_{index}")))?,
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

pub fn consolidate_statement_fields(
    public: &ConsolidateProofPublic,
) -> Result<Vec<Fq>, StatementHashError> {
    public
        .validate_shape()
        .map_err(|e| consolidate_field_encoding_error(&e.to_string()))?;

    let expected = consolidate_statement_field_count(
        public.family_id.input_count(),
        public.family_id.output_count(),
    );
    note_reshape_statement_fields(
        public.anchor,
        public.balance_commitment,
        &public.inputs,
        &public.outputs,
        expected,
        consolidate_field_encoding_error,
    )
}

pub fn split_statement_fields(public: &SplitProofPublic) -> Result<Vec<Fq>, StatementHashError> {
    public
        .validate_shape()
        .map_err(|e| split_field_encoding_error(&e.to_string()))?;

    let expected = split_statement_field_count(
        public.family_id.input_count(),
        public.family_id.output_count(),
    );
    note_reshape_statement_fields(
        public.anchor,
        public.balance_commitment,
        &public.inputs,
        &public.outputs,
        expected,
        split_field_encoding_error,
    )
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
    }
    fields.extend(
        public
            .balance_commitment
            .0
            .to_field_elements()
            .ok_or_else(|| transfer_field_encoding_error("balance_commitment"))?,
    );
    for (index, spend) in public.inputs.iter().enumerate() {
        fields.extend(
            spend
                .nullifier
                .0
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("nullifier_{index}")))?,
        );
        fields.extend(
            transfer_rk_element(spend)?
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("rk_{index}")))?,
        );
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
        fields.extend(
            tier.epk
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("{label}_epk")))?,
        );
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
    for (label, dleq) in [
        ("transfer_sender_core_dleq", &compliance.sender_core_dleq),
        ("transfer_sender_ext_dleq", &compliance.sender_ext_dleq),
        ("transfer_output_core_dleq", &compliance.output_core_dleq),
        ("transfer_output_ext_dleq", &compliance.output_ext_dleq),
    ] {
        fields.extend(
            dleq.c
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("{label}_c")))?,
        );
        fields.extend(
            dleq.s
                .to_field_elements()
                .ok_or_else(|| transfer_field_encoding_error(&format!("{label}_s")))?,
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
    let mut fields = note_reshape_statement_fields(
        public.anchor,
        public.balance_commitment,
        &public.inputs,
        std::slice::from_ref(&public.change_output),
        2 + 1 + 2 * public.inputs.len(),
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
    fields.extend([
        public.withdrawal_effect_hash_lo,
        public.withdrawal_effect_hash_hi,
    ]);

    if fields.len() != expected {
        return Err(StatementHashError::InvalidFieldLength {
            expected,
            got: fields.len(),
        });
    }

    Ok(fields)
}

pub fn consolidate_statement_hash(
    family_id: ConsolidateFamilyId,
    fields: &[Fq],
) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &consolidate_statement_hash_constant(family_id, "v1"),
        consolidate_statement_hash_constant(family_id, "pad0"),
        consolidate_statement_hash_constant(family_id, "pad1"),
        fields,
        consolidate_statement_field_count(family_id.input_count(), family_id.output_count()),
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn split_statement_hash(
    family_id: SplitFamilyId,
    fields: &[Fq],
) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &split_statement_hash_constant(family_id, "v1"),
        split_statement_hash_constant(family_id, "pad0"),
        split_statement_hash_constant(family_id, "pad1"),
        fields,
        split_statement_field_count(family_id.input_count(), family_id.output_count()),
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn transfer_statement_hash(fields: &[Fq]) -> Result<Fq, StatementHashError> {
    let domain = transfer_statement_hash_constant("v1");
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
        &shielded_ics20_withdrawal_statement_hash_constant("v1"),
        shielded_ics20_withdrawal_statement_hash_constant("pad0"),
        shielded_ics20_withdrawal_statement_hash_constant("pad1"),
        fields,
        shielded_ics20_withdrawal_statement_field_count(2),
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn consolidate_statement_hash_from_public(
    public: &ConsolidateProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = consolidate_statement_fields(public)?;
    consolidate_statement_hash(public.family_id, &fields)
}

pub fn split_statement_hash_from_public(
    public: &SplitProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = split_statement_fields(public)?;
    split_statement_hash(public.family_id, &fields)
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

pub fn consolidate_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    family_id: ConsolidateFamilyId,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    hash_statement_fields_var(
        cs,
        &consolidate_statement_hash_constant(family_id, "v1"),
        consolidate_statement_hash_constant(family_id, "pad0"),
        consolidate_statement_hash_constant(family_id, "pad1"),
        fields,
        consolidate_statement_field_count(family_id.input_count(), family_id.output_count()),
    )
}

pub fn split_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    family_id: SplitFamilyId,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    hash_statement_fields_var(
        cs,
        &split_statement_hash_constant(family_id, "v1"),
        split_statement_hash_constant(family_id, "pad0"),
        split_statement_hash_constant(family_id, "pad1"),
        fields,
        split_statement_field_count(family_id.input_count(), family_id.output_count()),
    )
}

pub fn transfer_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    let domain = transfer_statement_hash_constant("v1");
    let pad_0 = transfer_statement_hash_constant("pad0");
    let pad_1 = transfer_statement_hash_constant("pad1");
    hash_statement_fields_var(
        cs,
        &domain,
        pad_0,
        pad_1,
        fields,
        TRANSFER_STATEMENT_FIELD_COUNT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{transfer_input_count, transfer_output_count};
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget};
    use ark_relations::r1cs::ConstraintSystem;
    use decaf377::Fq;

    #[test]
    fn consolidate_statement_hash_native_matches_r1cs() {
        for family_id in ConsolidateFamilyId::ALL {
            let fields = (0..consolidate_statement_field_count(
                family_id.input_count(),
                family_id.output_count(),
            ))
                .map(|i| Fq::from((i as u64) + 1))
                .collect::<Vec<_>>();
            let native =
                consolidate_statement_hash(family_id, &fields).expect("native hash should succeed");

            let cs = ConstraintSystem::<Fq>::new_ref();
            let vars = fields
                .iter()
                .map(|f| FqVar::new_witness(cs.clone(), || Ok(*f)).expect("witness allocation"))
                .collect::<Vec<_>>();
            let var_hash = consolidate_statement_hash_var(cs.clone(), family_id, &vars)
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
    fn split_statement_hash_native_matches_r1cs() {
        for family_id in SplitFamilyId::ALL {
            let fields =
                (0..split_statement_field_count(family_id.input_count(), family_id.output_count()))
                    .map(|i| Fq::from((i as u64) + 1))
                    .collect::<Vec<_>>();
            let native =
                split_statement_hash(family_id, &fields).expect("native hash should succeed");

            let cs = ConstraintSystem::<Fq>::new_ref();
            let vars = fields
                .iter()
                .map(|f| FqVar::new_witness(cs.clone(), || Ok(*f)).expect("witness allocation"))
                .collect::<Vec<_>>();
            let var_hash = split_statement_hash_var(cs.clone(), family_id, &vars)
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
}
