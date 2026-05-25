use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::BigInteger;
use ark_ff::PrimeField;
use decaf377::{Element, Encoding, Fq, Fr, ZETA};
use poseidon_parameters::v1::{Alpha, MatrixOperations};
use serde::Serialize;

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
    issuer_detection_domain: String,
    dleq_metadata_domain: String,
    imt_leaf_domain: String,
    imt_params_domain: String,
    imt_ring_domain: String,
    hash7_domain: String,
    hash7_inputs: Vec<String>,
    hash7_output: String,
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
struct DleqFixture {
    challenge_keep_bits: usize,
    metadata_hash: String,
    wrong_metadata_hash: String,
    r: String,
    ack_x: String,
    ack_y: String,
    epk_x: String,
    epk_y: String,
    dleq_c: String,
    dleq_s: String,
}

#[derive(Serialize)]
struct Vectors {
    decaf377_companion_curve: CurveVectors,
    poseidon377: PoseidonVectors,
    decaf377_compress_vectors: Vec<DecafCompressVector>,
    decaf377_encode_vectors: Vec<DecafEncodeVector>,
    dleq_fixture: DleqFixture,
}

fn blake2b_fq(label: &[u8]) -> Fq {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes())
}

fn personalized_blake2b_fq(personal: &[u8; 16]) -> Fq {
    let hash = blake2b_simd::Params::default().personal(personal).hash(b"");
    Fq::from_le_bytes_mod_order(hash.as_bytes())
}

fn fq_to_challenge_scalar(fq: Fq) -> Fr {
    let mut bytes = fq.into_bigint().to_bytes_le();
    let keep_bits = Fr::MODULUS_BIT_SIZE - 1;
    let keep_bytes = (keep_bits as usize + 7) / 8;
    let spare_bits = keep_bytes * 8 - keep_bits as usize;
    bytes[keep_bytes - 1] &= 0xFF >> spare_bits;
    Fr::from_le_bytes_mod_order(&bytes)
}

fn compute_metadata_hash(
    policy_id_hash: Fq,
    resource_hash: Fq,
    permission_hash: Fq,
    tier: Fq,
    target_timestamp: Fq,
    salt: Fq,
) -> Fq {
    let domain = blake2b_fq(b"penumbra.compliance.dleq_metadata");
    poseidon377::hash_6(
        &domain,
        (
            policy_id_hash,
            resource_hash,
            permission_hash,
            tier,
            target_timestamp,
            salt,
        ),
    )
}

fn compute_dleq_native(r: Fr, k: Fr, ack: &Element, epk: &Element, metadata_hash: Fq) -> (Fq, Fr) {
    let s_point = *ack * r;
    let r_point = Element::GENERATOR * k;
    let rp_point = *ack * k;

    let domain = Fq::from_le_bytes_mod_order(b"elgamal-encrypt-proof-v1");
    let c_fq_full = poseidon377::hash_7(
        &domain,
        (
            metadata_hash,
            Element::GENERATOR.vartime_compress_to_field(),
            ack.vartime_compress_to_field(),
            epk.vartime_compress_to_field(),
            s_point.vartime_compress_to_field(),
            r_point.vartime_compress_to_field(),
            rp_point.vartime_compress_to_field(),
        ),
    );
    let c_truncated = fq_to_challenge_scalar(c_fq_full);
    let s = k + c_truncated * r;
    (Fq::from_le_bytes_mod_order(&c_truncated.to_bytes()), s)
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
    let spend_domain = blake2b_fq(b"penumbra.shielded_pool.spend.public_input_hash.v1");
    let spend_pad_0 = blake2b_fq(b"penumbra.shielded_pool.spend.public_input_hash.pad0");
    let spend_pad_1 = blake2b_fq(b"penumbra.shielded_pool.spend.public_input_hash.pad1");
    let hash7_domain = blake2b_fq(b"penumbra.gnark.prototype.poseidon377.hash7.v1");
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

    let r = Fr::from(101u64);
    let ring_sk = Fr::from(201u64);
    let ring_pk = Element::GENERATOR * ring_sk;
    let d_fr = Fr::from(301u64);
    let ack = ring_pk * d_fr;
    let epk = Element::GENERATOR * r;
    let k = Fr::from(401u64);
    let target_timestamp = Fq::from(1_700_000_000u64);
    let salt = Fq::from(501u64);
    let metadata_hash = compute_metadata_hash(
        Fq::from(1u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        target_timestamp,
        salt,
    );
    let wrong_metadata_hash = compute_metadata_hash(
        Fq::from(99u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        target_timestamp,
        Fq::from(777u64),
    );
    let (dleq_c, dleq_s) = compute_dleq_native(r, k, &ack, &epk, metadata_hash);
    let ack_affine = ack.into_affine();
    let (ack_x, ack_y) = ack_affine.xy().expect("ack should have coordinates");
    let epk_affine = epk.into_affine();
    let (epk_x, epk_y) = epk_affine.xy().expect("epk should have coordinates");

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
            note_commit_domain: blake2b_fq(b"penumbra.notecommit").to_string(),
            nullifier_domain: blake2b_fq(b"penumbra.nullifier").to_string(),
            value_generator_domain: blake2b_fq(b"penumbra.value.generator").to_string(),
            ivk_domain: Fq::from_le_bytes_mod_order(b"penumbra.derive.ivk").to_string(),
            tct_domain: blake2b_fq(b"penumbra.tct").to_string(),
            sender_leaf_domain: Fq::from_le_bytes_mod_order(
                blake2b_simd::blake2b(b"penumbra.leaf_binding.sender").as_bytes(),
            )
            .to_string(),
            compliance_leaf_domain: blake2b_fq(b"penumbra.compliance.leaf").to_string(),
            issuer_detection_domain: blake2b_fq(b"penumbra.compliance.issuer_detection")
                .to_string(),
            dleq_metadata_domain: blake2b_fq(b"penumbra.compliance.dleq_metadata").to_string(),
            imt_leaf_domain: personalized_blake2b_fq(b"pen.imt.leaf____").to_string(),
            imt_params_domain: personalized_blake2b_fq(b"pen.imt.params__").to_string(),
            imt_ring_domain: personalized_blake2b_fq(b"pen.imt.ring____").to_string(),
            hash7_domain: hash7_domain.to_string(),
            hash7_inputs: hash7_inputs.iter().map(ToString::to_string).collect(),
            hash7_output: hash7_output.to_string(),
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
        dleq_fixture: DleqFixture {
            challenge_keep_bits: (Fr::MODULUS_BIT_SIZE as usize) - 1,
            metadata_hash: metadata_hash.to_string(),
            wrong_metadata_hash: wrong_metadata_hash.to_string(),
            r: r.to_string(),
            ack_x: ack_x.to_string(),
            ack_y: ack_y.to_string(),
            epk_x: epk_x.to_string(),
            epk_y: epk_y.to_string(),
            dleq_c: dleq_c.to_string(),
            dleq_s: Fq::from_le_bytes_mod_order(&dleq_s.to_bytes()).to_string(),
        },
    };

    serde_json::to_writer_pretty(std::io::stdout(), &vectors).expect("serialize vectors");
    println!();
}
