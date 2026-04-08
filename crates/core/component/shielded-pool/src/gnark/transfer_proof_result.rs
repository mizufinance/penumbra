use anyhow::{bail, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::PrimeField;
use ark_groth16::Proof;
use decaf377::{Bls12_377, Fq};

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

const TRANSFER_PROOF_RESULT_MAGIC: &[u8; 4] = b"PTPR";
const TRANSFER_PROOF_RESULT_VERSION: u32 = 1;

fn parse_g1_base_be(bytes: &[u8]) -> ProofG1Base {
    ProofG1Base::from_be_bytes_mod_order(bytes)
}

pub(crate) fn parse_transfer_binary_proof_result(
    payload: &[u8],
    label: &str,
) -> Result<(Fq, Proof<Bls12_377>)> {
    const G1_BYTES: usize = 48;
    const CLAIMED_HASH_BYTES: usize = 32;
    const HEADER_LEN: usize = 4 + 4 + 4 + 4 + 8;
    const EXPECTED_LEN: usize = HEADER_LEN + CLAIMED_HASH_BYTES + (2 + 4 + 2) * G1_BYTES;

    if payload.len() != EXPECTED_LEN {
        bail!(
            "unexpected gnark {label} proof result length: got {}, want {}",
            payload.len(),
            EXPECTED_LEN
        );
    }
    if &payload[0..4] != TRANSFER_PROOF_RESULT_MAGIC {
        bail!("invalid gnark {label} proof result magic");
    }
    let version = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    if version != TRANSFER_PROOF_RESULT_VERSION {
        bail!("unsupported gnark {label} proof result version {version}");
    }
    let total_len = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    if total_len != payload.len() {
        bail!(
            "gnark {label} proof result length mismatch: header={total_len}, actual={}",
            payload.len()
        );
    }
    let status = u32::from_le_bytes(payload[12..16].try_into().unwrap());
    if status != 0 {
        bail!("gnark {label} proof result returned nonzero status {status}");
    }

    let claimed_hash =
        Fq::from_le_bytes_mod_order(&payload[HEADER_LEN..HEADER_LEN + CLAIMED_HASH_BYTES]);
    let mut offset = HEADER_LEN + CLAIMED_HASH_BYTES;
    let next = |offset: &mut usize| {
        let start = *offset;
        *offset += G1_BYTES;
        &payload[start..*offset]
    };

    let a_x = parse_g1_base_be(next(&mut offset));
    let a_y = parse_g1_base_be(next(&mut offset));
    let b_x_a0 = parse_g1_base_be(next(&mut offset));
    let b_x_a1 = parse_g1_base_be(next(&mut offset));
    let b_y_a0 = parse_g1_base_be(next(&mut offset));
    let b_y_a1 = parse_g1_base_be(next(&mut offset));
    let c_x = parse_g1_base_be(next(&mut offset));
    let c_y = parse_g1_base_be(next(&mut offset));

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
