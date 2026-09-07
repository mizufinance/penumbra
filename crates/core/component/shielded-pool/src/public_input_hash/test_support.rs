use ark_r1cs_std::alloc::AllocVar;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::{r1cs::FqVar, Fq};

use super::*;

pub fn hash_statement_fields_var(
    cs: ConstraintSystemRef<Fq>,
    domain: &Fq,
    pad_0: Fq,
    pad_1: Fq,
    fields: &[FqVar],
    expected_len: usize,
) -> Result<FqVar, SynthesisError> {
    if fields.len() != expected_len {
        return Err(SynthesisError::Unsatisfiable);
    }

    let domain_var = FqVar::new_constant(cs.clone(), *domain)?;
    let pad_0_var = FqVar::new_constant(cs.clone(), pad_0)?;
    let pad_1_var = FqVar::new_constant(cs.clone(), pad_1)?;
    let mut first = [
        pad_0_var.clone(),
        pad_1_var.clone(),
        pad_0_var.clone(),
        pad_1_var.clone(),
        pad_0_var.clone(),
        pad_1_var.clone(),
        pad_0_var.clone(),
    ];
    for (i, value) in fields.iter().take(7).enumerate() {
        first[i] = value.clone();
    }

    let mut h = poseidon377::r1cs::hash_7(
        cs.clone(),
        &domain_var,
        (
            first[0].clone(),
            first[1].clone(),
            first[2].clone(),
            first[3].clone(),
            first[4].clone(),
            first[5].clone(),
            first[6].clone(),
        ),
    )?;
    let mut idx = usize::min(7, fields.len());

    while idx + 6 <= fields.len() {
        h = poseidon377::r1cs::hash_7(
            cs.clone(),
            &domain_var,
            (
                h,
                fields[idx].clone(),
                fields[idx + 1].clone(),
                fields[idx + 2].clone(),
                fields[idx + 3].clone(),
                fields[idx + 4].clone(),
                fields[idx + 5].clone(),
            ),
        )?;
        idx += 6;
    }

    if idx < fields.len() {
        let mut tail = [
            pad_0_var.clone(),
            pad_1_var.clone(),
            pad_0_var.clone(),
            pad_1_var.clone(),
            pad_0_var,
            pad_1_var,
        ];
        for (i, value) in fields[idx..].iter().enumerate() {
            tail[i] = value.clone();
        }
        h = poseidon377::r1cs::hash_7(
            cs,
            &domain_var,
            (
                h,
                tail[0].clone(),
                tail[1].clone(),
                tail[2].clone(),
                tail[3].clone(),
                tail[4].clone(),
                tail[5].clone(),
            ),
        )?;
    }

    Ok(h)
}
pub fn note_reshape_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    family_id: NoteReshapeFamilyId,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    hash_statement_fields_var(
        cs,
        &note_reshape_statement_hash_constant(family_id, "statement"),
        note_reshape_statement_hash_constant(family_id, "pad0"),
        note_reshape_statement_hash_constant(family_id, "pad1"),
        fields,
        note_reshape_statement_field_count(family_id.input_count(), family_id.output_count()),
    )
}

pub fn transfer_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    let domain = transfer_statement_hash_constant("statement");
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
