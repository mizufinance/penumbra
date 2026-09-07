//! Native verification for nullifier-history Groth16 proofs.

use ark_bw6_761::{Fr as Bw6Scalar, BW6_761};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::{BigInteger, PrimeField, Zero};
use ark_groth16::{
    r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof, VerifyingKey,
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::{Bls12_377, Fq as BlsScalar};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::str::FromStr;

use crate::gnark_artifact_validation::{
    decode_canonical_json, G1PointJson, G2PointJson, VerifyingKeyJson,
};

pub const GENERATION_PROOF_BYTES: usize = 192;
pub const CHUNK_PROOF_BYTES: usize = 480;

const GENERATION_VK_JSON: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/historical_generation_indexed/generation_verifying_key.json"
));
const CHUNK_VK_JSON: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/historical_generation_indexed/chunk_verifying_key.json"
));

static GENERATION_PVK: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    decode_generation_verifying_key(GENERATION_VK_JSON)
        .expect("bundled indexed historical generation key is valid")
});
static CHUNK_PVK: Lazy<ChunkVerificationKey> = Lazy::new(|| {
    decode_chunk_verifying_key(CHUNK_VK_JSON)
        .expect("bundled indexed historical chunk key is valid")
});

pub fn generation_verification_key() -> &'static PreparedVerifyingKey<Bls12_377> {
    &GENERATION_PVK
}

pub fn chunk_verification_key() -> &'static ChunkVerificationKey {
    &CHUNK_PVK
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationClaim {
    pub protocol_version: u32,
    pub nullifier: [u8; 32],
    pub generation_index: u64,
    pub generation_root: [u8; 32],
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    pub start_history_head: [u8; 32],
    pub end_history_head: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkClaim {
    pub protocol_version: u32,
    pub nullifier: [u8; 32],
    pub chunk_index: u64,
    pub start_history_head: [u8; 32],
    pub end_history_head: [u8; 32],
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Bw6VerifyingKeyJson {
    alpha_g1: G1PointJson,
    beta_g2: G1PointJson,
    gamma_g2: G1PointJson,
    delta_g2: G1PointJson,
    gamma_abc_g1: Vec<G1PointJson>,
    commitment_keys: Vec<Bw6CommitmentKeyJson>,
    public_and_commitment_committed: Vec<Vec<usize>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BlsVerifyingKeyJson {
    #[serde(flatten)]
    groth16: VerifyingKeyJson,
    commitment_keys: Vec<serde_json::Value>,
    public_and_commitment_committed: Vec<Vec<usize>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BlsProofJson {
    a: G1PointJson,
    b: G2PointJson,
    c: G1PointJson,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Bw6ProofJson {
    a: G1PointJson,
    b: G1PointJson,
    c: G1PointJson,
    commitment: G1PointJson,
    commitment_pok: G1PointJson,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Bw6CommitmentKeyJson {
    g: G1PointJson,
    g_sigma_neg: G1PointJson,
}

pub struct ChunkVerificationKey {
    pvk: PreparedVerifyingKey<BW6_761>,
    commitment_g: Bw6G2,
    commitment_g_sigma_neg: Bw6G2,
    committed_indexes: Vec<usize>,
}

pub fn decode_generation_verifying_key(
    bytes: &[u8],
) -> anyhow::Result<PreparedVerifyingKey<Bls12_377>> {
    let value: BlsVerifyingKeyJson =
        decode_canonical_json(bytes, "historical BLS verifying_key.json")?;
    anyhow::ensure!(
        value.commitment_keys.is_empty() && value.public_and_commitment_committed.is_empty(),
        "indexed historical BLS key must not use commitment wires"
    );
    let vk = super::verifying_key_from_json(&value.groth16)?;
    anyhow::ensure!(
        vk.gamma_abc_g1.len() == 9,
        "historical BLS key must bind eight public inputs"
    );
    Ok(vk.into())
}

pub fn decode_chunk_verifying_key(bytes: &[u8]) -> anyhow::Result<ChunkVerificationKey> {
    let value: Bw6VerifyingKeyJson = decode_canonical_json(bytes, "BW6 verifying_key.json")?;
    anyhow::ensure!(
        value.commitment_keys.len() == 1 && value.public_and_commitment_committed.len() == 1,
        "indexed historical BW6 key must contain one commitment wire"
    );
    let vk = VerifyingKey {
        alpha_g1: parse_bw6_g1(&value.alpha_g1)?,
        beta_g2: parse_bw6_g2(&value.beta_g2)?,
        gamma_g2: parse_bw6_g2(&value.gamma_g2)?,
        delta_g2: parse_bw6_g2(&value.delta_g2)?,
        gamma_abc_g1: value
            .gamma_abc_g1
            .iter()
            .map(parse_bw6_g1)
            .collect::<anyhow::Result<_>>()?,
    };
    anyhow::ensure!(
        vk.gamma_abc_g1.len() == 7,
        "historical BW6 key must bind five public inputs plus the commitment input"
    );
    let key = &value.commitment_keys[0];
    Ok(ChunkVerificationKey {
        pvk: vk.into(),
        commitment_g: parse_bw6_g2(&key.g)?,
        commitment_g_sigma_neg: parse_bw6_g2(&key.g_sigma_neg)?,
        committed_indexes: value.public_and_commitment_committed[0].clone(),
    })
}

pub fn encode_generation_proof_json(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let value: BlsProofJson = decode_canonical_json(bytes, "historical BLS proof JSON")?;
    encode_proof(Proof::<Bls12_377> {
        a: super::parse_g1(&value.a)?,
        b: super::parse_g2(&value.b)?,
        c: super::parse_g1(&value.c)?,
    })
}

pub fn encode_chunk_proof_json(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let value: Bw6ProofJson = decode_canonical_json(bytes, "historical BW6 proof JSON")?;
    let mut bytes = encode_proof(Proof::<BW6_761> {
        a: parse_bw6_g1(&value.a)?,
        b: parse_bw6_g2(&value.b)?,
        c: parse_bw6_g1(&value.c)?,
    })?;
    parse_bw6_g1(&value.commitment)?.serialize_compressed(&mut bytes)?;
    parse_bw6_g1(&value.commitment_pok)?.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub fn verify_generation(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    claim: GenerationClaim,
    proof_bytes: &[u8],
) -> anyhow::Result<()> {
    let public_inputs = generation_public_inputs(claim)?;
    verify(
        pvk,
        &public_inputs,
        proof_bytes,
        GENERATION_PROOF_BYTES,
        "historical generation",
    )
}

pub fn verify_chunk(
    key: &ChunkVerificationKey,
    claim: ChunkClaim,
    proof_bytes: &[u8],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        proof_bytes.len() == CHUNK_PROOF_BYTES,
        "historical chunk proof must be exactly {CHUNK_PROOF_BYTES} bytes"
    );
    let mut remaining = proof_bytes;
    let proof = Proof::<BW6_761>::deserialize_compressed(&mut remaining)?;
    let commitment = Bw6G1::deserialize_compressed(&mut remaining)?;
    let commitment_pok = Bw6G1::deserialize_compressed(&mut remaining)?;
    anyhow::ensure!(
        remaining.is_empty(),
        "historical chunk proof has trailing bytes"
    );
    let pok_pairing = BW6_761::multi_pairing(
        [commitment, commitment_pok],
        [key.commitment_g_sigma_neg, key.commitment_g],
    );
    anyhow::ensure!(
        pok_pairing.is_zero(),
        "historical chunk commitment proof rejected"
    );
    let public_inputs = chunk_public_inputs(claim)?;
    let commitment_input =
        chunk_commitment_hash(commitment, &public_inputs, &key.committed_indexes)?;
    let mut augmented = public_inputs;
    augmented.push(commitment_input);
    let mut prepared = Groth16::<BW6_761, LibsnarkReduction>::prepare_inputs(&key.pvk, &augmented)?;
    prepared += commitment;
    anyhow::ensure!(
        Groth16::<BW6_761, LibsnarkReduction>::verify_proof_with_prepared_inputs(
            &key.pvk, &proof, &prepared,
        )?,
        "historical chunk proof verification failed"
    );
    Ok(())
}

fn verify<E: Pairing>(
    pvk: &PreparedVerifyingKey<E>,
    public_inputs: &[E::ScalarField],
    proof_bytes: &[u8],
    expected_bytes: usize,
    label: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        proof_bytes.len() == expected_bytes,
        "{label} proof must be exactly {expected_bytes} bytes"
    );
    let mut remaining = proof_bytes;
    let proof = Proof::<E>::deserialize_compressed(&mut remaining)
        .map_err(|error| anyhow::anyhow!("invalid {label} proof encoding: {error}"))?;
    anyhow::ensure!(remaining.is_empty(), "{label} proof has trailing bytes");
    anyhow::ensure!(
        Groth16::<E>::verify_proof(pvk, &proof, public_inputs)?,
        "{label} proof verification failed"
    );
    Ok(())
}

fn encode_proof<E: Pairing>(proof: Proof<E>) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    proof.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

fn chunk_commitment_hash(
    commitment: Bw6G1,
    public_inputs: &[Bw6Scalar],
    committed_indexes: &[usize],
) -> anyhow::Result<Bw6Scalar> {
    let mut input = Vec::new();
    let (x, y) = commitment
        .xy()
        .ok_or_else(|| anyhow::anyhow!("chunk commitment cannot be identity"))?;
    let base_bytes = Bw6Base::MODULUS_BIT_SIZE.div_ceil(8) as usize;
    for coordinate in [x, y] {
        let encoded = coordinate.into_bigint().to_bytes_be();
        input.resize(input.len() + base_bytes - encoded.len(), 0);
        input.extend_from_slice(&encoded);
    }
    let scalar_bytes = Bw6Scalar::MODULUS_BIT_SIZE.div_ceil(8) as usize;
    for index in committed_indexes {
        anyhow::ensure!(
            *index > 0 && *index <= public_inputs.len(),
            "invalid chunk commitment index"
        );
        let encoded = public_inputs[*index - 1].into_bigint().to_bytes_be();
        input.resize(input.len() + scalar_bytes - encoded.len(), 0);
        input.extend_from_slice(&encoded);
    }
    Ok(hash_to_field(&input, b"bsb22-commitment"))
}

fn hash_to_field(message: &[u8], dst: &[u8]) -> Bw6Scalar {
    let output_len = 16 + Bw6Scalar::MODULUS_BIT_SIZE.div_ceil(8) as usize;
    let ell = output_len.div_ceil(32);
    let dst_len = dst.len() as u8;
    let mut initial = Sha256::new();
    initial.update([0u8; 64]);
    initial.update(message);
    initial.update([(output_len >> 8) as u8, output_len as u8, 0]);
    initial.update(dst);
    initial.update([dst_len]);
    let b0 = initial.finalize();

    let mut first = Sha256::new();
    first.update(b0);
    first.update([1]);
    first.update(dst);
    first.update([dst_len]);
    let mut previous = first.finalize();
    let mut uniform = Vec::with_capacity(ell * 32);
    uniform.extend_from_slice(&previous);
    for index in 2..=ell {
        let xor = std::array::from_fn::<_, 32, _>(|i| b0[i] ^ previous[i]);
        let mut next = Sha256::new();
        next.update(xor);
        next.update([index as u8]);
        next.update(dst);
        next.update([dst_len]);
        previous = next.finalize();
        uniform.extend_from_slice(&previous);
    }
    Bw6Scalar::from_be_bytes_mod_order(&uniform[..output_len])
}

fn generation_public_inputs(claim: GenerationClaim) -> anyhow::Result<Vec<BlsScalar>> {
    Ok(vec![
        BlsScalar::from(claim.protocol_version),
        canonical_bls_scalar(claim.nullifier, "historical nullifier")?,
        BlsScalar::from(claim.generation_index),
        canonical_bls_scalar(claim.generation_root, "historical generation root")?,
        BlsScalar::from(claim.generation_start_position),
        BlsScalar::from(claim.generation_end_position),
        canonical_bls_scalar(claim.start_history_head, "historical start head")?,
        canonical_bls_scalar(claim.end_history_head, "historical end head")?,
    ])
}

fn chunk_public_inputs(claim: ChunkClaim) -> anyhow::Result<Vec<Bw6Scalar>> {
    for (bytes, label) in [
        (claim.nullifier, "historical nullifier"),
        (claim.start_history_head, "historical start head"),
        (claim.end_history_head, "historical end head"),
    ] {
        canonical_bls_scalar(bytes, label)?;
    }
    Ok(vec![
        Bw6Scalar::from(claim.protocol_version),
        Bw6Scalar::from_le_bytes_mod_order(&claim.nullifier),
        Bw6Scalar::from(claim.chunk_index),
        Bw6Scalar::from_le_bytes_mod_order(&claim.start_history_head),
        Bw6Scalar::from_le_bytes_mod_order(&claim.end_history_head),
    ])
}

fn canonical_bls_scalar(bytes: [u8; 32], label: &str) -> anyhow::Result<BlsScalar> {
    BlsScalar::from_bytes_checked(&bytes)
        .map_err(|_| anyhow::anyhow!("{label} is not a canonical BLS12-377 scalar"))
}

type Bw6G1 = <BW6_761 as Pairing>::G1Affine;
type Bw6G2 = <BW6_761 as Pairing>::G2Affine;
type Bw6Base = <Bw6G1 as AffineRepr>::BaseField;

fn parse_bw6_g1(point: &G1PointJson) -> anyhow::Result<Bw6G1> {
    let point = Bw6G1::new_unchecked(
        parse_field(&point.x, "BW6 G1 x")?,
        parse_field(&point.y, "BW6 G1 y")?,
    );
    anyhow::ensure!(point.is_on_curve(), "BW6 G1 point is not on curve");
    anyhow::ensure!(
        point.is_in_correct_subgroup_assuming_on_curve(),
        "BW6 G1 point is not in the correct subgroup"
    );
    Ok(point)
}

fn parse_bw6_g2(point: &G1PointJson) -> anyhow::Result<Bw6G2> {
    let point = Bw6G2::new_unchecked(
        parse_field(&point.x, "BW6 G2 x")?,
        parse_field(&point.y, "BW6 G2 y")?,
    );
    anyhow::ensure!(point.is_on_curve(), "BW6 G2 point is not on curve");
    anyhow::ensure!(
        point.is_in_correct_subgroup_assuming_on_curve(),
        "BW6 G2 point is not in the correct subgroup"
    );
    Ok(point)
}

fn parse_field(value: &str, label: &str) -> anyhow::Result<Bw6Base> {
    let parsed = Bw6Base::from_str(value).map_err(|_| anyhow::anyhow!("invalid {label}"))?;
    anyhow::ensure!(parsed.to_string() == value, "{label} is not canonical");
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_serialize::{CanonicalSerialize, Compress};
    use shieldd_sdk_sct::{
        indexed_nullifier_tree::{
            IndexedNullifierLeaf, IndexedNullifierWitness, DEPTH, ZERO_HASHES,
        },
        nullifier_generation::{append_history, empty_history_head},
        Nullifier,
    };

    fn artifact_bytes(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../tools/gnark/artifacts/historical_generation_indexed")
                .join(name),
        )
        .unwrap()
    }

    fn artifact_hex(name: &str) -> [u8; 32] {
        hex::decode(String::from_utf8(artifact_bytes(name)).unwrap().trim())
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[test]
    fn canonical_proof_lengths_match_wire_contract() {
        assert_eq!(
            Proof::<Bls12_377>::default().serialized_size(Compress::Yes),
            GENERATION_PROOF_BYTES
        );
        assert_eq!(
            Proof::<BW6_761>::default().serialized_size(Compress::Yes)
                + 2 * Bw6G1::default().serialized_size(Compress::Yes),
            CHUNK_PROOF_BYTES
        );
    }

    #[test]
    fn artifact_roots_match_the_live_sct_domains() {
        let lower = IndexedNullifierLeaf {
            value: BlsScalar::from(0u64).to_bytes(),
            next_index: 1,
            next_value: BlsScalar::from(7u64).to_bytes(),
            is_lower_sentinel: true,
            is_terminal: false,
        };
        let witness = IndexedNullifierWitness {
            leaf_position: 1,
            leaf: IndexedNullifierLeaf::ordinary(
                Nullifier(BlsScalar::from(7u64)),
                0,
                BlsScalar::from(0u64).to_bytes(),
                true,
            ),
            auth_path: (0..DEPTH)
                .map(|level| {
                    let sibling = ZERO_HASHES[level as usize].to_bytes();
                    if level == 0 {
                        [lower.commitment().unwrap().to_bytes(), sibling, sibling]
                    } else {
                        [sibling, sibling, sibling]
                    }
                })
                .collect(),
        };
        let root = witness.root().unwrap();
        assert_eq!(root, artifact_hex("generation_root.txt"));

        let mut head = empty_history_head();
        assert_eq!(head, artifact_hex("start_history_head.txt"));
        for generation in 0..10u64 {
            head = append_history(
                head,
                generation,
                root,
                generation << 32,
                (generation + 1) << 32,
            )
            .unwrap();
            if generation == 0 {
                assert_eq!(head, artifact_hex("first_end_history_head.txt"));
            }
        }
        assert_eq!(head, artifact_hex("end_history_head.txt"));
    }

    #[test]
    fn generated_sample_proofs_verify() {
        let nullifier = BlsScalar::from(9u64).to_bytes();
        let root = artifact_hex("generation_root.txt");
        let start_head = artifact_hex("start_history_head.txt");
        let first_end_head = artifact_hex("first_end_history_head.txt");
        let end_head = artifact_hex("end_history_head.txt");
        let generation_proof =
            encode_generation_proof_json(&artifact_bytes("sample_generation_proof.json")).unwrap();
        let generation_claim = GenerationClaim {
            protocol_version: 2,
            nullifier,
            generation_index: 0,
            generation_root: root,
            generation_start_position: 0,
            generation_end_position: 1 << 32,
            start_history_head: start_head,
            end_history_head: first_end_head,
        };
        verify_generation(
            generation_verification_key(),
            generation_claim,
            &generation_proof,
        )
        .unwrap();
        for malformed in [
            GenerationClaim {
                protocol_version: 3,
                ..generation_claim
            },
            GenerationClaim {
                nullifier: BlsScalar::from(10u64).to_bytes(),
                ..generation_claim
            },
            GenerationClaim {
                generation_index: 1,
                ..generation_claim
            },
            GenerationClaim {
                generation_root: BlsScalar::from(1u64).to_bytes(),
                ..generation_claim
            },
            GenerationClaim {
                generation_start_position: 1,
                ..generation_claim
            },
            GenerationClaim {
                generation_end_position: (1 << 32) + 1,
                ..generation_claim
            },
            GenerationClaim {
                start_history_head: first_end_head,
                ..generation_claim
            },
            GenerationClaim {
                end_history_head: start_head,
                ..generation_claim
            },
        ] {
            assert!(
                verify_generation(generation_verification_key(), malformed, &generation_proof,)
                    .is_err()
            );
        }

        let chunk_proof =
            encode_chunk_proof_json(&artifact_bytes("sample_chunk_proof.json")).unwrap();
        let chunk_claim = ChunkClaim {
            protocol_version: 2,
            nullifier,
            chunk_index: 0,
            start_history_head: start_head,
            end_history_head: end_head,
        };
        verify_chunk(chunk_verification_key(), chunk_claim, &chunk_proof).unwrap();
        for malformed in [
            ChunkClaim {
                protocol_version: 3,
                ..chunk_claim
            },
            ChunkClaim {
                nullifier: BlsScalar::from(10u64).to_bytes(),
                ..chunk_claim
            },
            ChunkClaim {
                chunk_index: 1,
                ..chunk_claim
            },
            ChunkClaim {
                start_history_head: first_end_head,
                ..chunk_claim
            },
            ChunkClaim {
                end_history_head: first_end_head,
                ..chunk_claim
            },
        ] {
            assert!(verify_chunk(chunk_verification_key(), malformed, &chunk_proof).is_err());
        }
    }

    #[test]
    fn verifier_rejects_noncanonical_public_field_encodings() {
        let invalid = [u8::MAX; 32];
        let valid = BlsScalar::from(1u64).to_bytes();
        let generation = GenerationClaim {
            protocol_version: 2,
            nullifier: valid,
            generation_index: 0,
            generation_root: valid,
            generation_start_position: 0,
            generation_end_position: 1,
            start_history_head: valid,
            end_history_head: valid,
        };
        for claim in [
            GenerationClaim {
                nullifier: invalid,
                ..generation
            },
            GenerationClaim {
                generation_root: invalid,
                ..generation
            },
            GenerationClaim {
                start_history_head: invalid,
                ..generation
            },
            GenerationClaim {
                end_history_head: invalid,
                ..generation
            },
        ] {
            assert!(generation_public_inputs(claim).is_err());
        }

        let chunk = ChunkClaim {
            protocol_version: 2,
            nullifier: valid,
            chunk_index: 0,
            start_history_head: valid,
            end_history_head: invalid,
        };
        assert!(chunk_public_inputs(chunk).is_err());
    }
}
