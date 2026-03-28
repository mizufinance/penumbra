use ark_r1cs_std::alloc::AllocVar;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::{r1cs::FqVar, Fq};

pub fn hash_statement_fields<E>(
    domain: &Fq,
    pad_0: Fq,
    pad_1: Fq,
    fields: &[Fq],
    expected_len: usize,
    invalid_len: impl FnOnce(usize, usize) -> E,
) -> Result<Fq, E> {
    if fields.len() != expected_len {
        return Err(invalid_len(expected_len, fields.len()));
    }

    let mut first = [pad_0, pad_1, pad_0, pad_1, pad_0, pad_1, pad_0];
    for (i, value) in fields.iter().take(7).enumerate() {
        first[i] = *value;
    }

    let mut h = poseidon377::hash_7(
        domain,
        (
            first[0], first[1], first[2], first[3], first[4], first[5], first[6],
        ),
    );
    let mut idx = usize::min(7, fields.len());

    while idx + 6 <= fields.len() {
        h = poseidon377::hash_7(
            domain,
            (
                h,
                fields[idx],
                fields[idx + 1],
                fields[idx + 2],
                fields[idx + 3],
                fields[idx + 4],
                fields[idx + 5],
            ),
        );
        idx += 6;
    }

    if idx < fields.len() {
        let mut tail = [pad_0, pad_1, pad_0, pad_1, pad_0, pad_1];
        for (i, value) in fields[idx..].iter().enumerate() {
            tail[i] = *value;
        }
        h = poseidon377::hash_7(
            domain,
            (h, tail[0], tail[1], tail[2], tail[3], tail[4], tail[5]),
        );
    }

    Ok(h)
}

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
