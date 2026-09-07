use anyhow::{bail, Context, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::Proof;
use decaf377::{Bls12_377, Fq};

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

fn parse_g1_base_be(bytes: &[u8], label: &str) -> Result<ProofG1Base> {
    let modulus = ProofG1Base::MODULUS.to_bytes_be();
    if bytes.len() != modulus.len() || bytes >= modulus.as_slice() {
        bail!("non-canonical gnark proof {label} coordinate");
    }
    Ok(ProofG1Base::from_be_bytes_mod_order(bytes))
}

fn parse_claimed_hash_le(bytes: &[u8], label: &str) -> Result<Fq> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .context("gnark proof result claimed hash must be 32 bytes")?;
    Fq::from_bytes_checked(&bytes)
        .map_err(|_| anyhow::anyhow!("non-canonical gnark {label} claimed statement hash"))
}

pub(crate) fn parse_binary_proof_result(
    payload: &[u8],
    magic: &[u8; 4],
    label: &str,
) -> Result<(Fq, Proof<Bls12_377>)> {
    const G1_BYTES: usize = 48;
    const CLAIMED_HASH_BYTES: usize = 32;
    const HEADER_LEN: usize = 4 + 4 + 4 + 8;
    const EXPECTED_LEN: usize = HEADER_LEN + CLAIMED_HASH_BYTES + (2 + 4 + 2) * G1_BYTES;

    if payload.len() != EXPECTED_LEN {
        bail!(
            "unexpected gnark {label} proof result length: got {}, want {}",
            payload.len(),
            EXPECTED_LEN
        );
    }
    if &payload[0..4] != magic {
        bail!("invalid gnark {label} proof result magic");
    }
    let total_len = u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize;
    if total_len != payload.len() {
        bail!(
            "gnark {label} proof result length mismatch: header={total_len}, actual={}",
            payload.len()
        );
    }
    let status = u32::from_le_bytes(payload[8..12].try_into().unwrap());
    if status != 0 {
        bail!("gnark {label} proof result returned nonzero status {status}");
    }

    let claimed_hash =
        parse_claimed_hash_le(&payload[HEADER_LEN..HEADER_LEN + CLAIMED_HASH_BYTES], label)?;
    let mut offset = HEADER_LEN + CLAIMED_HASH_BYTES;
    let next = |offset: &mut usize| {
        let start = *offset;
        *offset += G1_BYTES;
        &payload[start..*offset]
    };

    let a_x = parse_g1_base_be(next(&mut offset), "a.x")?;
    let a_y = parse_g1_base_be(next(&mut offset), "a.y")?;
    let b_x_a0 = parse_g1_base_be(next(&mut offset), "b.x.a0")?;
    let b_x_a1 = parse_g1_base_be(next(&mut offset), "b.x.a1")?;
    let b_y_a0 = parse_g1_base_be(next(&mut offset), "b.y.a0")?;
    let b_y_a1 = parse_g1_base_be(next(&mut offset), "b.y.a1")?;
    let c_x = parse_g1_base_be(next(&mut offset), "c.x")?;
    let c_y = parse_g1_base_be(next(&mut offset), "c.y")?;

    let a = ProofG1::new_unchecked(a_x, a_y);
    let b = ProofG2::new_unchecked(
        ProofG2Base::new(b_x_a0, b_x_a1),
        ProofG2Base::new(b_y_a0, b_y_a1),
    );
    let c = ProofG1::new_unchecked(c_x, c_y);
    for (point_label, point) in [("a", a), ("c", c)] {
        if !point.is_on_curve() {
            bail!("gnark {label} proof {point_label} is not on curve");
        }
        if !point.is_in_correct_subgroup_assuming_on_curve() {
            bail!("gnark {label} proof {point_label} is not in the correct subgroup");
        }
    }
    if !b.is_on_curve() {
        bail!("gnark {label} proof b is not on curve");
    }
    if !b.is_in_correct_subgroup_assuming_on_curve() {
        bail!("gnark {label} proof b is not in the correct subgroup");
    }

    Ok((claimed_hash, Proof { a, b, c }))
}

pub(crate) fn parse_transfer_binary_proof_result(
    payload: &[u8],
    label: &str,
) -> Result<(Fq, Proof<Bls12_377>)> {
    parse_binary_proof_result(payload, b"PTPR", label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_one_le(mut bytes: [u8; 32]) -> [u8; 32] {
        for byte in &mut bytes {
            let (next, carry) = byte.overflowing_add(1);
            *byte = next;
            if !carry {
                break;
            }
        }
        bytes
    }

    #[test]
    fn claimed_hash_parser_rejects_modulus_alias() {
        let mut modulus = (-Fq::from(1u64)).to_bytes();
        modulus = add_one_le(modulus);
        let alias_of_one = add_one_le(modulus);

        assert_eq!(
            parse_claimed_hash_le(&Fq::from(1u64).to_bytes(), "test")
                .expect("canonical statement hash"),
            Fq::from(1u64)
        );
        for bytes in [modulus, alias_of_one] {
            let err = parse_claimed_hash_le(&bytes, "test")
                .expect_err("statement-hash alias should fail");
            assert!(
                err.to_string().contains("non-canonical"),
                "unexpected error: {err:#}"
            );
        }
    }

    #[test]
    fn proof_coordinate_parser_rejects_base_field_modulus() {
        let modulus = ProofG1Base::MODULUS.to_bytes_be();
        let err = parse_g1_base_be(&modulus, "test").expect_err("coordinate modulus should fail");
        assert!(
            err.to_string().contains("non-canonical"),
            "unexpected error: {err:#}"
        );
    }
}
