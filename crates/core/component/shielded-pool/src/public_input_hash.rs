use ark_ff::ToConstraintField;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::{r1cs::FqVar, Fq};
use once_cell::sync::Lazy;
use penumbra_sdk_compliance::structs::{OUTPUT_CIPHERTEXT_FQS, SPEND_CIPHERTEXT_FQS};
use penumbra_sdk_proof_params::statement_hash::{hash_statement_fields, hash_statement_fields_var};

use crate::{output::OutputProofPublic, spend::SpendProofPublic};

pub const SPEND_STATEMENT_FIELD_COUNT: usize = 17;
pub const OUTPUT_STATEMENT_FIELD_COUNT: usize = 29;

pub static SPEND_STATEMENT_HASH_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.shielded_pool.spend.public_input_hash.v1").as_bytes(),
    )
});
pub static OUTPUT_STATEMENT_HASH_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.shielded_pool.output.public_input_hash.v1").as_bytes(),
    )
});
pub static SPEND_STATEMENT_PAD_0: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.shielded_pool.spend.public_input_hash.pad0").as_bytes(),
    )
});
pub static SPEND_STATEMENT_PAD_1: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.shielded_pool.spend.public_input_hash.pad1").as_bytes(),
    )
});
pub static OUTPUT_STATEMENT_PAD_0: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.shielded_pool.output.public_input_hash.pad0").as_bytes(),
    )
});
pub static OUTPUT_STATEMENT_PAD_1: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.shielded_pool.output.public_input_hash.pad1").as_bytes(),
    )
});

#[derive(Debug, thiserror::Error)]
pub enum StatementHashError {
    #[error("invalid field length: expected {expected}, got {got}")]
    InvalidFieldLength { expected: usize, got: usize },
    #[error("failed to decompress randomized spend key")]
    DecompressRk(decaf377::EncodingError),
    #[error("failed converting {field} to constraint field elements")]
    FieldEncoding { field: &'static str },
    #[error("invalid ciphertext field length: expected {expected}, got {got}")]
    InvalidCiphertextLength { expected: usize, got: usize },
}

pub fn spend_statement_fields(public: &SpendProofPublic) -> Result<Vec<Fq>, StatementHashError> {
    use StatementHashError::FieldEncoding;

    if public.compliance_ciphertext.len() != SPEND_CIPHERTEXT_FQS {
        return Err(StatementHashError::InvalidCiphertextLength {
            expected: SPEND_CIPHERTEXT_FQS,
            got: public.compliance_ciphertext.len(),
        });
    }

    let rk_element = decaf377::Encoding(public.rk.to_bytes())
        .vartime_decompress()
        .map_err(StatementHashError::DecompressRk)?;

    macro_rules! to_field_elements {
        ($fe:expr, $name:expr) => {
            $fe.to_field_elements()
                .ok_or(FieldEncoding { field: $name })?
        };
    }

    let mut fields = [
        to_field_elements!(Fq::from(public.anchor), "anchor"),
        to_field_elements!(public.balance_commitment.0, "balance_commitment"),
        to_field_elements!(public.nullifier.0, "nullifier"),
        to_field_elements!(rk_element, "rk"),
        to_field_elements!(public.asset_anchor.0, "asset_anchor"),
        to_field_elements!(public.compliance_anchor.0, "compliance_anchor"),
        to_field_elements!(public.epk, "epk"),
        to_field_elements!(public.c2_core, "c2_core"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    fields.extend(public.compliance_ciphertext.iter().copied());
    fields.push(public.target_timestamp);
    fields.push(public.dleq_c);
    fields.push(public.dleq_s);
    fields.extend(to_field_elements!(
        public.sender_leaf_hash.0,
        "sender_leaf_hash"
    ));

    if fields.len() != SPEND_STATEMENT_FIELD_COUNT {
        return Err(StatementHashError::InvalidFieldLength {
            expected: SPEND_STATEMENT_FIELD_COUNT,
            got: fields.len(),
        });
    }

    Ok(fields)
}

pub fn output_statement_fields(public: &OutputProofPublic) -> Result<Vec<Fq>, StatementHashError> {
    use StatementHashError::FieldEncoding;

    if public.compliance_ciphertext.len() != OUTPUT_CIPHERTEXT_FQS {
        return Err(StatementHashError::InvalidCiphertextLength {
            expected: OUTPUT_CIPHERTEXT_FQS,
            got: public.compliance_ciphertext.len(),
        });
    }

    macro_rules! to_field_elements {
        ($fe:expr, $name:expr) => {
            $fe.to_field_elements()
                .ok_or(FieldEncoding { field: $name })?
        };
    }

    let mut fields = Vec::with_capacity(OUTPUT_STATEMENT_FIELD_COUNT);
    fields.extend(to_field_elements!(
        public.note_commitment.0,
        "note_commitment"
    ));
    fields.extend(to_field_elements!(
        public.balance_commitment.0,
        "balance_commitment"
    ));
    fields.extend(to_field_elements!(public.asset_anchor.0, "asset_anchor"));
    fields.extend(to_field_elements!(
        public.compliance_anchor.0,
        "compliance_anchor"
    ));
    fields.extend(to_field_elements!(public.epk_1, "epk_1"));
    fields.extend(to_field_elements!(public.epk_2, "epk_2"));
    fields.extend(to_field_elements!(public.epk_3, "epk_3"));
    fields.extend(to_field_elements!(public.c2_core, "c2_core"));
    fields.extend(to_field_elements!(public.c2_ext, "c2_ext"));
    fields.extend(to_field_elements!(public.c2_sext, "c2_sext"));
    fields.extend(public.compliance_ciphertext.iter().copied());
    fields.extend(to_field_elements!(
        public.target_timestamp,
        "target_timestamp"
    ));
    fields.extend(to_field_elements!(public.dleq_c_1, "dleq_c_1"));
    fields.extend(to_field_elements!(public.dleq_s_1, "dleq_s_1"));
    fields.extend(to_field_elements!(public.dleq_c_2, "dleq_c_2"));
    fields.extend(to_field_elements!(public.dleq_s_2, "dleq_s_2"));
    fields.extend(to_field_elements!(public.dleq_c_3, "dleq_c_3"));
    fields.extend(to_field_elements!(public.dleq_s_3, "dleq_s_3"));
    fields.extend(to_field_elements!(
        public.counterparty_leaf_hash.0,
        "counterparty_leaf_hash"
    ));

    if fields.len() != OUTPUT_STATEMENT_FIELD_COUNT {
        return Err(StatementHashError::InvalidFieldLength {
            expected: OUTPUT_STATEMENT_FIELD_COUNT,
            got: fields.len(),
        });
    }

    Ok(fields)
}

pub fn spend_statement_hash(fields: &[Fq]) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &SPEND_STATEMENT_HASH_DOMAIN,
        *SPEND_STATEMENT_PAD_0,
        *SPEND_STATEMENT_PAD_1,
        fields,
        SPEND_STATEMENT_FIELD_COUNT,
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn output_statement_hash(fields: &[Fq]) -> Result<Fq, StatementHashError> {
    hash_statement_fields(
        &OUTPUT_STATEMENT_HASH_DOMAIN,
        *OUTPUT_STATEMENT_PAD_0,
        *OUTPUT_STATEMENT_PAD_1,
        fields,
        OUTPUT_STATEMENT_FIELD_COUNT,
        |expected, got| StatementHashError::InvalidFieldLength { expected, got },
    )
}

pub fn spend_statement_hash_from_public(
    public: &SpendProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = spend_statement_fields(public)?;
    spend_statement_hash(&fields)
}

pub fn output_statement_hash_from_public(
    public: &OutputProofPublic,
) -> Result<Fq, StatementHashError> {
    let fields = output_statement_fields(public)?;
    output_statement_hash(&fields)
}

pub fn spend_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    hash_statement_fields_var(
        cs,
        &SPEND_STATEMENT_HASH_DOMAIN,
        *SPEND_STATEMENT_PAD_0,
        *SPEND_STATEMENT_PAD_1,
        fields,
        SPEND_STATEMENT_FIELD_COUNT,
    )
}

pub fn output_statement_hash_var(
    cs: ConstraintSystemRef<Fq>,
    fields: &[FqVar],
) -> Result<FqVar, SynthesisError> {
    hash_statement_fields_var(
        cs,
        &OUTPUT_STATEMENT_HASH_DOMAIN,
        *OUTPUT_STATEMENT_PAD_0,
        *OUTPUT_STATEMENT_PAD_1,
        fields,
        OUTPUT_STATEMENT_FIELD_COUNT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget};
    use ark_relations::r1cs::ConstraintSystem;
    use decaf377::Fq;
    use penumbra_sdk_proof_params::DummyWitness;
    use std::iter;

    use crate::{output::OutputCircuit, spend::SpendCircuit};

    #[test]
    fn spend_statement_hash_native_matches_r1cs() {
        let fields = (0..SPEND_STATEMENT_FIELD_COUNT)
            .map(|i| Fq::from((i as u64) + 1))
            .collect::<Vec<_>>();
        let native = spend_statement_hash(&fields).expect("native hash should succeed");

        let cs = ConstraintSystem::<Fq>::new_ref();
        let vars = fields
            .iter()
            .map(|f| FqVar::new_witness(cs.clone(), || Ok(*f)).expect("witness allocation"))
            .collect::<Vec<_>>();
        let var_hash = spend_statement_hash_var(cs.clone(), &vars).expect("r1cs hash should work");
        let constrained_native = FqVar::new_witness(cs.clone(), || Ok(native))
            .expect("native witness allocation should work");
        var_hash
            .enforce_equal(&constrained_native)
            .expect("hashes must be equal");
        assert!(cs.is_satisfied().expect("cs should evaluate"));
    }

    #[test]
    fn output_statement_hash_native_matches_r1cs() {
        let fields = (0..OUTPUT_STATEMENT_FIELD_COUNT)
            .map(|i| Fq::from((i as u64) + 1))
            .collect::<Vec<_>>();
        let native = output_statement_hash(&fields).expect("native hash should succeed");

        let cs = ConstraintSystem::<Fq>::new_ref();
        let vars = fields
            .iter()
            .map(|f| FqVar::new_witness(cs.clone(), || Ok(*f)).expect("witness allocation"))
            .collect::<Vec<_>>();
        let var_hash = output_statement_hash_var(cs.clone(), &vars).expect("r1cs hash should work");
        let constrained_native = FqVar::new_witness(cs.clone(), || Ok(native))
            .expect("native witness allocation should work");
        var_hash
            .enforce_equal(&constrained_native)
            .expect("hashes must be equal");
        assert!(cs.is_satisfied().expect("cs should evaluate"));
    }

    #[test]
    fn spend_statement_fields_match_historical_flatten_order() {
        let circuit = SpendCircuit::with_dummy_witness();
        let (public, _, _) = circuit.into_parts();

        let rk_element = decaf377::Encoding(public.rk.to_bytes())
            .vartime_decompress()
            .expect("dummy rk should decompress");

        let expected = iter::empty()
            .chain(
                Fq::from(public.anchor)
                    .to_field_elements()
                    .expect("anchor fields"),
            )
            .chain(
                public
                    .balance_commitment
                    .0
                    .to_field_elements()
                    .expect("balance fields"),
            )
            .chain(
                public
                    .nullifier
                    .0
                    .to_field_elements()
                    .expect("nullifier fields"),
            )
            .chain(rk_element.to_field_elements().expect("rk fields"))
            .chain(
                public
                    .asset_anchor
                    .0
                    .to_field_elements()
                    .expect("asset anchor fields"),
            )
            .chain(
                public
                    .compliance_anchor
                    .0
                    .to_field_elements()
                    .expect("compliance anchor fields"),
            )
            .chain(public.epk.to_field_elements().expect("epk fields"))
            .chain(public.c2_core.to_field_elements().expect("c2 fields"))
            .chain(public.compliance_ciphertext.iter().copied())
            .chain(
                public
                    .target_timestamp
                    .to_field_elements()
                    .expect("timestamp fields"),
            )
            .chain(public.dleq_c.to_field_elements().expect("dleq c fields"))
            .chain(public.dleq_s.to_field_elements().expect("dleq s fields"))
            .chain(
                public
                    .sender_leaf_hash
                    .0
                    .to_field_elements()
                    .expect("sender leaf fields"),
            )
            .collect::<Vec<_>>();

        let got = spend_statement_fields(&public).expect("field extraction should succeed");
        assert_eq!(got, expected);
        assert_eq!(got.len(), SPEND_STATEMENT_FIELD_COUNT);
    }

    #[test]
    fn output_statement_fields_match_historical_flatten_order() {
        let circuit = OutputCircuit::with_dummy_witness();
        let (public, _, _) = circuit.into_parts();

        let expected = iter::empty()
            .chain(
                public
                    .note_commitment
                    .0
                    .to_field_elements()
                    .expect("note commitment fields"),
            )
            .chain(
                public
                    .balance_commitment
                    .0
                    .to_field_elements()
                    .expect("balance fields"),
            )
            .chain(
                public
                    .asset_anchor
                    .0
                    .to_field_elements()
                    .expect("asset anchor fields"),
            )
            .chain(
                public
                    .compliance_anchor
                    .0
                    .to_field_elements()
                    .expect("compliance anchor fields"),
            )
            .chain(public.epk_1.to_field_elements().expect("epk1 fields"))
            .chain(public.epk_2.to_field_elements().expect("epk2 fields"))
            .chain(public.epk_3.to_field_elements().expect("epk3 fields"))
            .chain(public.c2_core.to_field_elements().expect("c2 core fields"))
            .chain(public.c2_ext.to_field_elements().expect("c2 ext fields"))
            .chain(public.c2_sext.to_field_elements().expect("c2 sext fields"))
            .chain(public.compliance_ciphertext.iter().copied())
            .chain(
                public
                    .target_timestamp
                    .to_field_elements()
                    .expect("timestamp fields"),
            )
            .chain(public.dleq_c_1.to_field_elements().expect("dleq c1 fields"))
            .chain(public.dleq_s_1.to_field_elements().expect("dleq s1 fields"))
            .chain(public.dleq_c_2.to_field_elements().expect("dleq c2 fields"))
            .chain(public.dleq_s_2.to_field_elements().expect("dleq s2 fields"))
            .chain(public.dleq_c_3.to_field_elements().expect("dleq c3 fields"))
            .chain(public.dleq_s_3.to_field_elements().expect("dleq s3 fields"))
            .chain(
                public
                    .counterparty_leaf_hash
                    .0
                    .to_field_elements()
                    .expect("counterparty leaf fields"),
            )
            .collect::<Vec<_>>();

        let got = output_statement_fields(&public).expect("field extraction should succeed");
        assert_eq!(got, expected);
        assert_eq!(got.len(), OUTPUT_STATEMENT_FIELD_COUNT);
    }
}
