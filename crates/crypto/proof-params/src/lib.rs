#![deny(clippy::unwrap_used)]
#![allow(clippy::redundant_static_lifetimes)]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]

use anyhow::{bail, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::PrimeField;
use ark_groth16::{PreparedVerifyingKey, VerifyingKey};
use decaf377::Bls12_377;
use gnark_artifact_validation::{
    decode_canonical_json, G1PointJson, G2PointJson, VerifyingKeyJson,
};
use once_cell::sync::Lazy;

/// The length of our Groth16 proofs in bytes.
pub const GROTH16_PROOF_LENGTH_BYTES: usize = 192;

pub mod batch;
#[allow(dead_code)]
mod gnark_artifact_validation;
pub mod historical;
pub mod statement_hash;

include!(concat!(env!("OUT_DIR"), "/gnark_bundled.rs"));

include!("gen/gnark/transfer_registry.rs");
include!("gen/gnark/note_reshape_registry.rs");
include!("gen/gnark/shielded_ics20_withdrawal_registry.rs");
include!("gen/gnark/note_seizure_registry.rs");

/// Closed identity for every proof key deployed by consensus.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeployedProofKey {
    Transfer,
    NoteReshapeOneByEight,
    NoteReshapeEightByOne,
    ShieldedIcs20WithdrawalCanonical,
    NoteSeizure,
}

impl DeployedProofKey {
    pub const ALL: [Self; 5] = [
        Self::Transfer,
        Self::NoteReshapeOneByEight,
        Self::NoteReshapeEightByOne,
        Self::ShieldedIcs20WithdrawalCanonical,
        Self::NoteSeizure,
    ];

    /// Resolve this closed identity to its bundled prepared verification key.
    pub fn bundled_pvk(self) -> &'static PreparedVerifyingKey<Bls12_377> {
        match self {
            Self::Transfer => transfer_proof_verification_key(),
            Self::NoteReshapeOneByEight => note_reshape_proof_verification_key(2),
            Self::NoteReshapeEightByOne => note_reshape_proof_verification_key(3),
            Self::ShieldedIcs20WithdrawalCanonical => {
                shielded_ics20_withdrawal_proof_verification_key(1)
            }
            Self::NoteSeizure => note_seizure_proof_verification_key(),
        }
    }
}

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

fn load_verifying_key_json_bytes(bytes: &[u8]) -> Result<VerifyingKey<Bls12_377>> {
    let vk_json: VerifyingKeyJson = decode_canonical_json(bytes, "verifying_key.json")?;
    verifying_key_from_json(&vk_json)
}

fn verifying_key_from_json(vk: &VerifyingKeyJson) -> Result<VerifyingKey<Bls12_377>> {
    Ok(VerifyingKey {
        alpha_g1: parse_g1(&vk.alpha_g1)?,
        beta_g2: parse_g2(&vk.beta_g2)?,
        gamma_g2: parse_g2(&vk.gamma_g2)?,
        delta_g2: parse_g2(&vk.delta_g2)?,
        gamma_abc_g1: vk
            .gamma_abc_g1
            .iter()
            .map(parse_g1)
            .collect::<Result<_>>()?,
    })
}

fn parse_g1(point: &G1PointJson) -> Result<ProofG1> {
    let x: ProofG1Base = parse_canonical_field(&point.x, "G1 x")?;
    let y: ProofG1Base = parse_canonical_field(&point.y, "G1 y")?;
    let point = ProofG1::new_unchecked(x, y);
    if !point.is_on_curve() {
        bail!("G1 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G1 point is not in the correct subgroup");
    }
    Ok(point)
}

fn parse_g2(point: &G2PointJson) -> Result<ProofG2> {
    let x_a0 = parse_canonical_field(&point.x.a0, "G2 x.a0")?;
    let x_a1 = parse_canonical_field(&point.x.a1, "G2 x.a1")?;
    let y_a0 = parse_canonical_field(&point.y.a0, "G2 y.a0")?;
    let y_a1 = parse_canonical_field(&point.y.a1, "G2 y.a1")?;
    let point = ProofG2::new_unchecked(ProofG2Base::new(x_a0, x_a1), ProofG2Base::new(y_a0, y_a1));
    if !point.is_on_curve() {
        bail!("G2 point is not on curve");
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        bail!("G2 point is not in the correct subgroup");
    }
    Ok(point)
}

fn parse_canonical_field<F: PrimeField>(value: &str, label: &str) -> Result<F> {
    let parsed = F::from_str(value).map_err(|_| anyhow::anyhow!("invalid {label}"))?;
    if parsed.to_string() != value {
        bail!("{label} is not a canonical field decimal: encoded {value}, decoded {parsed}");
    }
    Ok(parsed)
}

#[cfg(test)]
mod gnark_artifact_tests {
    use super::*;
    use ark_ff::BigInteger;
    use gnark_artifact_validation::{validate_circuit_metadata, CircuitMetadataJson};

    const TRANSFER_METADATA: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/transfer/circuit_metadata.json"
    ));

    fn assert_bundled_verification_key(bytes: &[u8], actual: &PreparedVerifyingKey<Bls12_377>) {
        let expected =
            load_verifying_key_json_bytes(bytes).expect("bundled verifying key must decode");
        assert_eq!(
            actual.vk, expected,
            "consensus verifier must use the exact build-bundled key"
        );
    }

    #[test]
    fn consensus_verifying_keys_are_exact_bundled_keys() {
        assert_bundled_verification_key(
            transfer_verifying_key_json_bytes(),
            transfer_proof_verification_key(),
        );
        for family_id in [2, 3] {
            assert_bundled_verification_key(
                note_reshape_verifying_key_json_bytes(family_id),
                note_reshape_proof_verification_key(family_id),
            );
        }
        assert_bundled_verification_key(
            shielded_ics20_withdrawal_verifying_key_json_bytes(1),
            shielded_ics20_withdrawal_proof_verification_key(1),
        );
        assert_bundled_verification_key(
            note_seizure_verifying_key_json_bytes(),
            note_seizure_proof_verification_key(),
        );
    }

    #[test]
    fn deployed_proof_key_registry_is_exhaustive_and_pairwise_distinct() {
        assert_eq!(
            DeployedProofKey::ALL.len(),
            2 + GENERATED_NOTE_RESHAPE_PROOF_FAMILIES.len()
                + GENERATED_SHIELDED_ICS20_WITHDRAWAL_PROOF_FAMILIES.len(),
            "closed deployed-key registry must track every generated proof family"
        );

        let mut generated = vec![(
            DeployedProofKey::Transfer,
            transfer_proof_verification_key(),
        )];
        generated.extend(GENERATED_NOTE_RESHAPE_PROOF_FAMILIES.iter().map(|family| {
            let key = match family.id {
                2 => DeployedProofKey::NoteReshapeOneByEight,
                3 => DeployedProofKey::NoteReshapeEightByOne,
                unknown => panic!("generated note reshape family {unknown} has no deployed key"),
            };
            (key, &**family.verification_key)
        }));
        generated.extend(
            GENERATED_SHIELDED_ICS20_WITHDRAWAL_PROOF_FAMILIES
                .iter()
                .map(|family| {
                    let key = match family.id {
                        1 => DeployedProofKey::ShieldedIcs20WithdrawalCanonical,
                        unknown => panic!(
                            "generated shielded ICS-20 withdrawal family {unknown} has no deployed key"
                        ),
                    };
                    (key, &**family.verification_key)
                }),
        );
        generated.push((
            DeployedProofKey::NoteSeizure,
            note_seizure_proof_verification_key(),
        ));

        for deployed_key in DeployedProofKey::ALL {
            let matches = generated
                .iter()
                .filter(|(generated_key, _)| *generated_key == deployed_key)
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "{deployed_key:?} must map to exactly one generated verifier"
            );
            assert_eq!(deployed_key.bundled_pvk(), matches[0].1);
        }
        for (index, (_, left)) in generated.iter().enumerate() {
            for (_, right) in generated.iter().skip(index + 1) {
                assert_ne!(*left, *right, "deployed proof keys must be distinct");
            }
        }
    }

    #[test]
    fn proof_params_rejects_noncanonical_metadata_and_vk_json() {
        let metadata: CircuitMetadataJson =
            decode_canonical_json(TRANSFER_METADATA, "transfer metadata")
                .expect("canonical metadata loads");
        validate_circuit_metadata(&metadata, "transfer").expect("metadata validates");
        let metadata_value: serde_json::Value =
            serde_json::from_slice(TRANSFER_METADATA).expect("parse metadata");
        let compact_metadata =
            serde_json::to_vec(&metadata_value).expect("encode compact metadata");
        assert!(
            decode_canonical_json::<CircuitMetadataJson>(&compact_metadata, "transfer metadata")
                .is_err(),
            "compact metadata alias must fail"
        );

        let vk_bytes = transfer_verifying_key_json_bytes();
        load_verifying_key_json_bytes(vk_bytes).expect("canonical VK JSON loads");
        let vk_value: serde_json::Value = serde_json::from_slice(vk_bytes).expect("parse VK JSON");
        let compact_vk = serde_json::to_vec(&vk_value).expect("encode compact VK JSON");
        assert!(
            load_verifying_key_json_bytes(&compact_vk).is_err(),
            "compact VK JSON alias must fail"
        );

        let mut vk_json: VerifyingKeyJson =
            serde_json::from_slice(vk_bytes).expect("decode canonical VK JSON");
        let canonical_x = vk_json
            .alpha_g1
            .x
            .parse::<ProofG1Base>()
            .expect("decode canonical G1 x");
        let mut alias = canonical_x.into_bigint();
        assert!(
            !alias.add_with_carry(&ProofG1Base::MODULUS),
            "x + modulus fits the base-field integer width"
        );
        vk_json.alpha_g1.x = alias.to_string();
        let mut modular_alias = serde_json::to_vec_pretty(&vk_json).expect("encode modular alias");
        modular_alias.push(b'\n');
        assert!(
            load_verifying_key_json_bytes(&modular_alias).is_err(),
            "x + modulus must not decode to the same verifying key"
        );

        vk_json.alpha_g1.x = "-1".to_owned();
        let mut negative_alias =
            serde_json::to_vec_pretty(&vk_json).expect("encode negative coordinate");
        negative_alias.push(b'\n');
        assert!(
            load_verifying_key_json_bytes(&negative_alias).is_err(),
            "negative field coordinates must fail"
        );
    }
}
