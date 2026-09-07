use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::PrimeField;
use decaf377::{Element, Encoding, Fq, Fr, ZETA};
use poseidon_parameters::v1::{Alpha, MatrixOperations};
use serde::Serialize;
use sha2::{Digest, Sha512};

#[derive(Serialize)]
struct CurveVectors {
    a: String,
    d: String,
    a_minus_d: String,
    order: String,
    zeta: String,
    generator_x: String,
    generator_y: String,
    generator_encoding_hex: String,
    generator_compress_to_field: String,
    value_blinding_generator_input: String,
    value_blinding_generator_x: String,
    value_blinding_generator_y: String,
    value_blinding_generator_encoding_hex: String,
    value_blinding_generator_compress_to_field: String,
}

#[derive(Serialize)]
struct PoseidonRateVectors {
    alpha: u32,
    full_rounds: usize,
    partial_rounds: usize,
    width: usize,
    rate: usize,
    mds: Vec<String>,
    arc: Vec<String>,
}

#[derive(Serialize)]
struct PoseidonVectors {
    spend_domain: String,
    spend_pad_0: String,
    spend_pad_1: String,
    note_commit_domain: String,
    nullifier_domain: String,
    value_generator_domain: String,
    ivk_domain: String,
    tct_domain: String,
    sender_leaf_domain: String,
    compliance_leaf_domain: String,
    compliance_nullifier_derivation_domain: String,
    issuer_detection_domain: String,
    imt_leaf_domain: String,
    imt_params_domain: String,
    imt_ring_domain: String,
    hash7_domain: String,
    hash7_inputs: Vec<String>,
    hash7_output: String,
    // Composite-derivation goldens: the nullifier (Poseidon hash_3 over the
    // nullifier domain) and the note commitment (hash_5 over the note-commit
    // domain), pinning RATE_3 / RATE_6 output correctness across Rust and gnark.
    nullifier_inputs: Vec<String>,
    nullifier_output: String,
    note_commit_inputs: Vec<String>,
    note_commit_output: String,
    rate_1: PoseidonRateVectors,
    rate_2: PoseidonRateVectors,
    rate_3: PoseidonRateVectors,
    rate_4: PoseidonRateVectors,
    rate_5: PoseidonRateVectors,
    rate_6: PoseidonRateVectors,
    rate_7: PoseidonRateVectors,
}

#[derive(Serialize)]
struct DecafCompressVector {
    scalar: String,
    x: String,
    y: String,
    compress_to_field: String,
    encoding_hex: String,
}

#[derive(Serialize)]
struct DecafEncodeVector {
    input: String,
    x: String,
    y: String,
    compress_to_field: String,
    encoding_hex: String,
}

#[derive(Serialize)]
struct OrbisDerivationVector {
    derivation_hex: String,
    scalar: String,
}

/// NoteReshape statement-hash seam fixture (H3 / Phase C).
///
/// Seeded retained-family public statements in the exact production role order,
/// plus reference statement hashes over their family-specific domains. The Go
/// seam test assembles the same fields and checks the production gadget.
#[derive(Serialize)]
struct NoteReshapeStatementFixture {
    label: String,
    domain: String,
    field_roles: Vec<String>,
    fields: Vec<String>,
    statement_hash: String,
}

#[derive(Serialize)]
struct Vectors {
    decaf377_companion_curve: CurveVectors,
    poseidon377: PoseidonVectors,
    decaf377_compress_vectors: Vec<DecafCompressVector>,
    decaf377_encode_vectors: Vec<DecafEncodeVector>,
    orbis_derivation: OrbisDerivationVector,
    note_reshape_statements: Vec<NoteReshapeStatementFixture>,
}

fn blake2b_fq(label: &[u8]) -> Fq {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes())
}

fn personalized_blake2b_fq(personal: &[u8; 16]) -> Fq {
    let hash = blake2b_simd::Params::default().personal(personal).hash(b"");
    Fq::from_le_bytes_mod_order(hash.as_bytes())
}

fn note_reshape_statement_fixture(
    label: &str,
    n_in: usize,
    n_out: usize,
) -> NoteReshapeStatementFixture {
    let domain_label = format!("shieldd.shielded_pool.{label}.public_input_hash.statement");
    let pad_0_label = format!("shieldd.shielded_pool.{label}.public_input_hash.pad0");
    let pad_1_label = format!("shieldd.shielded_pool.{label}.public_input_hash.pad1");
    let domain = blake2b_fq(domain_label.as_bytes());
    let pad_0 = blake2b_fq(pad_0_label.as_bytes());
    let pad_1 = blake2b_fq(pad_1_label.as_bytes());

    let mut field_roles = vec!["anchor".to_string()];
    for index in 0..n_out {
        field_roles.push(format!("output_note_commitment_{index}"));
        field_roles.push(format!("output_recovery_commitment_{index}"));
    }
    field_roles.push("balance_commitment_fq".to_string());
    field_roles.push("asset_anchor".to_string());
    field_roles.push("compliance_anchor".to_string());
    field_roles.push("routing_tag".to_string());
    field_roles.push("routing_parameter_set_id".to_string());
    field_roles.push("recent_position_floor".to_string());
    for index in 0..n_in {
        field_roles.push(format!("nullifier_{index}"));
        field_roles.push(format!("rk_{index}"));
        field_roles.push(format!("history_required_{index}"));
    }

    let fields = (0..field_roles.len())
        .map(|index| Fq::from(70_001u64 + index as u64))
        .collect::<Vec<_>>();
    let mut first = [pad_0, pad_1, pad_0, pad_1, pad_0, pad_1, pad_0];
    for (index, field) in fields.iter().take(7).enumerate() {
        first[index] = *field;
    }
    let mut hash = poseidon377::hash_7(
        &domain,
        (
            first[0], first[1], first[2], first[3], first[4], first[5], first[6],
        ),
    );
    let mut index = usize::min(7, fields.len());
    while index + 6 <= fields.len() {
        hash = poseidon377::hash_7(
            &domain,
            (
                hash,
                fields[index],
                fields[index + 1],
                fields[index + 2],
                fields[index + 3],
                fields[index + 4],
                fields[index + 5],
            ),
        );
        index += 6;
    }
    if index < fields.len() {
        let mut tail = [pad_0, pad_1, pad_0, pad_1, pad_0, pad_1];
        for (offset, field) in fields[index..].iter().enumerate() {
            tail[offset] = *field;
        }
        hash = poseidon377::hash_7(
            &domain,
            (hash, tail[0], tail[1], tail[2], tail[3], tail[4], tail[5]),
        );
    }

    NoteReshapeStatementFixture {
        label: label.to_string(),
        domain: domain.to_string(),
        field_roles,
        fields: fields.iter().map(ToString::to_string).collect(),
        statement_hash: hash.to_string(),
    }
}

fn poseidon_rate_vectors<
    const WIDTH: usize,
    const RATE: usize,
    const MDS_SIZE: usize,
    const RATE_SQUARED: usize,
    const ROUND_CONSTS: usize,
>(
    params: poseidon377::PoseidonParameters<
        WIDTH,
        RATE,
        MDS_SIZE,
        RATE_SQUARED,
        39,
        WIDTH,
        ROUND_CONSTS,
        31,
    >,
) -> PoseidonRateVectors {
    PoseidonRateVectors {
        alpha: match params.alpha {
            Alpha::Exponent(exp) => exp,
            Alpha::Inverse => panic!("unexpected inverse alpha for poseidon377"),
        },
        full_rounds: params.rounds.full(),
        partial_rounds: params.rounds.partial(),
        width: WIDTH,
        rate: RATE,
        mds: params
            .mds
            .elements()
            .iter()
            .map(ToString::to_string)
            .collect(),
        arc: params
            .arc
            .elements()
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

fn main() {
    let spend_domain = blake2b_fq(b"shieldd.shielded_pool.spend.public_input_hash.statement");
    let spend_pad_0 = blake2b_fq(b"shieldd.shielded_pool.spend.public_input_hash.pad0");
    let spend_pad_1 = blake2b_fq(b"shieldd.shielded_pool.spend.public_input_hash.pad1");
    let hash7_domain = blake2b_fq(b"shieldd.gnark.prototype.poseidon377.hash7");
    let hash7_inputs = [1u64, 2, 3, 4, 5, 6, 7].map(Fq::from);
    let hash7_output = poseidon377::hash_7(
        &hash7_domain,
        (
            hash7_inputs[0],
            hash7_inputs[1],
            hash7_inputs[2],
            hash7_inputs[3],
            hash7_inputs[4],
            hash7_inputs[5],
            hash7_inputs[6],
        ),
    );
    // Nullifier = Poseidon hash_3(nullifier_domain, [nk, stateCommitment, position]).
    let nullifier_domain = blake2b_fq(b"shieldd.nullifier");
    let nullifier_inputs = [11u64, 22, 3].map(Fq::from);
    let nullifier_output = poseidon377::hash_3(
        &nullifier_domain,
        (
            nullifier_inputs[0],
            nullifier_inputs[1],
            nullifier_inputs[2],
        ),
    );
    // Note commitment binds the recovery capsule commitment alongside the note facts.
    let note_commit_domain = blake2b_fq(b"shieldd.notecommit");
    let note_commit_inputs = [101u64, 202, 303, 404, 505, 606].map(Fq::from);
    let note_commit_output = poseidon377::hash_6(
        &note_commit_domain,
        (
            note_commit_inputs[0],
            note_commit_inputs[1],
            note_commit_inputs[2],
            note_commit_inputs[3],
            note_commit_inputs[4],
            note_commit_inputs[5],
        ),
    );
    let note_reshape_statements = [("note_reshape8x1", 8, 1), ("note_reshape1x8", 1, 8)]
        .into_iter()
        .map(|(label, n_in, n_out)| note_reshape_statement_fixture(label, n_in, n_out))
        .collect();
    let generator_encoding = Encoding::from(Element::GENERATOR);
    let generator_encoding_bytes: [u8; 32] = generator_encoding.into();
    let generator_affine = Element::GENERATOR.into_affine();
    let (generator_x, generator_y) = generator_affine
        .xy()
        .expect("generator should have affine coordinates");
    let decaf_scalars = [1u64, 2, 3, 5, 7, 42];
    let decaf377_compress_vectors = decaf_scalars
        .into_iter()
        .map(|scalar| {
            let scalar_fr = Fr::from(scalar);
            let element = Element::GENERATOR * scalar_fr;
            let affine = element.into_affine();
            let (x, y) = affine.xy().expect("affine point should have coordinates");
            let encoding = Encoding::from(element);
            let encoding_bytes: [u8; 32] = encoding.into();

            DecafCompressVector {
                scalar: scalar_fr.to_string(),
                x: x.to_string(),
                y: y.to_string(),
                compress_to_field: element.vartime_compress_to_field().to_string(),
                encoding_hex: hex::encode(encoding_bytes),
            }
        })
        .collect();
    let decaf377_encode_vectors = [1u64, 2, 42]
        .into_iter()
        .map(|input| {
            let input_fq = Fq::from(input);
            let element = Element::encode_to_curve(&input_fq);
            let affine = element.into_affine();
            let (x, y) = affine.xy().expect("affine point should have coordinates");
            let encoding = Encoding::from(element);
            let encoding_bytes: [u8; 32] = encoding.into();

            DecafEncodeVector {
                input: input_fq.to_string(),
                x: x.to_string(),
                y: y.to_string(),
                compress_to_field: element.vartime_compress_to_field().to_string(),
                encoding_hex: hex::encode(encoding_bytes),
            }
        })
        .collect();
    let value_blinding_generator_input = blake2b_fq(b"decaf377-rdsa-binding");
    let value_blinding_generator = Element::encode_to_curve(&value_blinding_generator_input);
    let value_blinding_generator_affine = value_blinding_generator.into_affine();
    let (value_blinding_generator_x, value_blinding_generator_y) = value_blinding_generator_affine
        .xy()
        .expect("value blinding generator should have affine coordinates");
    let value_blinding_generator_encoding = Encoding::from(value_blinding_generator);
    let value_blinding_generator_encoding_bytes: [u8; 32] =
        value_blinding_generator_encoding.into();
    let derivation: Vec<u8> = (0u8..48).collect();
    let mut derivation_hasher = Sha512::new();
    derivation_hasher.update(b"elgamal-derivation-v1\0\0");
    derivation_hasher.update(&derivation);
    let derivation_scalar = Fr::from_le_bytes_mod_order(&derivation_hasher.finalize());

    let vectors = Vectors {
        decaf377_companion_curve: CurveVectors {
            a: "-1".to_string(),
            d: Fq::from(3021u64).to_string(),
            a_minus_d: (-(Fq::ONE) - Fq::from(3021u64)).to_string(),
            order: Fr::MODULUS.to_string(),
            zeta: ZETA.to_string(),
            generator_x: generator_x.to_string(),
            generator_y: generator_y.to_string(),
            generator_encoding_hex: hex::encode(generator_encoding_bytes),
            generator_compress_to_field: Element::GENERATOR.vartime_compress_to_field().to_string(),
            value_blinding_generator_input: value_blinding_generator_input.to_string(),
            value_blinding_generator_x: value_blinding_generator_x.to_string(),
            value_blinding_generator_y: value_blinding_generator_y.to_string(),
            value_blinding_generator_encoding_hex: hex::encode(
                value_blinding_generator_encoding_bytes,
            ),
            value_blinding_generator_compress_to_field: value_blinding_generator
                .vartime_compress_to_field()
                .to_string(),
        },
        poseidon377: PoseidonVectors {
            spend_domain: spend_domain.to_string(),
            spend_pad_0: spend_pad_0.to_string(),
            spend_pad_1: spend_pad_1.to_string(),
            note_commit_domain: note_commit_domain.to_string(),
            nullifier_domain: blake2b_fq(b"shieldd.nullifier").to_string(),
            value_generator_domain: blake2b_fq(b"shieldd.value.generator").to_string(),
            ivk_domain: Fq::from_le_bytes_mod_order(b"shieldd.derive.ivk").to_string(),
            tct_domain: blake2b_fq(b"shieldd.tct").to_string(),
            sender_leaf_domain: Fq::from_le_bytes_mod_order(
                blake2b_simd::blake2b(b"shieldd.leaf_binding.sender").as_bytes(),
            )
            .to_string(),
            compliance_leaf_domain: blake2b_fq(b"shieldd.compliance.leaf").to_string(),
            compliance_nullifier_derivation_domain: blake2b_fq(
                b"shieldd.compliance.nullifier_derivation",
            )
            .to_string(),
            issuer_detection_domain: blake2b_fq(b"shieldd.compliance.issuer_detection").to_string(),
            imt_leaf_domain: personalized_blake2b_fq(b"pen.imt.leaf____").to_string(),
            imt_params_domain: personalized_blake2b_fq(b"pen.imt.params2_").to_string(),
            imt_ring_domain: personalized_blake2b_fq(b"pen.imt.ring____").to_string(),
            hash7_domain: hash7_domain.to_string(),
            hash7_inputs: hash7_inputs.iter().map(ToString::to_string).collect(),
            hash7_output: hash7_output.to_string(),
            nullifier_inputs: nullifier_inputs.iter().map(ToString::to_string).collect(),
            nullifier_output: nullifier_output.to_string(),
            note_commit_inputs: note_commit_inputs.iter().map(ToString::to_string).collect(),
            note_commit_output: note_commit_output.to_string(),
            rate_1: poseidon_rate_vectors(poseidon377::RATE_1_PARAMS.clone()),
            rate_2: poseidon_rate_vectors(poseidon377::RATE_2_PARAMS.clone()),
            rate_3: poseidon_rate_vectors(poseidon377::RATE_3_PARAMS.clone()),
            rate_4: poseidon_rate_vectors(poseidon377::RATE_4_PARAMS.clone()),
            rate_5: poseidon_rate_vectors(poseidon377::RATE_5_PARAMS.clone()),
            rate_6: poseidon_rate_vectors(poseidon377::RATE_6_PARAMS.clone()),
            rate_7: poseidon_rate_vectors(poseidon377::RATE_7_PARAMS.clone()),
        },
        decaf377_compress_vectors,
        decaf377_encode_vectors,
        orbis_derivation: OrbisDerivationVector {
            derivation_hex: hex::encode(derivation),
            scalar: derivation_scalar.to_string(),
        },
        note_reshape_statements,
    };

    serde_json::to_writer_pretty(std::io::stdout(), &vectors).expect("serialize vectors");
    println!();
}
