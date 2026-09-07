use decaf377::Fq;

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
