use std::{
    ffi::{c_char, c_void, CString},
    fmt::Write as _,
    fs::File,
    io::BufReader,
    path::Path,
    ptr, slice,
    str::FromStr,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use ark_ec::{pairing::Pairing, AffineRepr, CurveGroup};
use ark_ff::PrimeField;
use ark_groth16::{prepare_verifying_key, PreparedVerifyingKey, Proof, VerifyingKey};
use ark_serialize::CanonicalSerialize;
use decaf377::Fr;
use decaf377::{Bls12_377, Encoding, Fq};
use hex::encode as hex_encode;
use libloading::Library;
use penumbra_sdk_compliance::{ComplianceLeaf, IndexedLeaf, MerklePath};
use penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1 as shielded_pool_pb;
use penumbra_sdk_shielded_pool::public_input_hash::{
    spend_statement_fields, spend_statement_hash_from_public,
};
use penumbra_sdk_shielded_pool::{SpendProof, SpendProofPrivate, SpendProofPublic};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const SPEND_WITNESS_V1_MAGIC: &[u8; 4] = b"PSWG";
const SPEND_WITNESS_V1_VERSION: u32 = 1;

#[repr(C)]
struct PenumbraGnarkInitResult {
    handle: u64,
    init_ms: f64,
    err_ptr: *mut c_void,
    err_len: usize,
}

#[repr(C)]
struct PenumbraGnarkBytesResult {
    ptr: *mut c_void,
    len: usize,
    status: u32,
}

type PenumbraGnarkSpendInit =
    unsafe extern "C" fn(*const c_char, usize, *mut PenumbraGnarkInitResult);
type PenumbraGnarkSpendProve =
    unsafe extern "C" fn(u64, *const c_void, usize, *mut PenumbraGnarkBytesResult);
type PenumbraGnarkSpendFree = unsafe extern "C" fn(*mut c_void, usize);
type PenumbraGnarkSpendShutdown = unsafe extern "C" fn(u64);

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

#[derive(Deserialize)]
struct G1PointJson {
    x: String,
    y: String,
}

#[derive(Deserialize)]
struct Fq2Json {
    a0: String,
    a1: String,
}

#[derive(Deserialize)]
struct G2PointJson {
    x: Fq2Json,
    y: Fq2Json,
}

#[derive(Deserialize)]
struct VerifyingKeyJson {
    alpha_g1: G1PointJson,
    beta_g2: G2PointJson,
    gamma_g2: G2PointJson,
    delta_g2: G2PointJson,
    gamma_abc_g1: Vec<G1PointJson>,
}

pub struct SpendWitnessDebugBundle {
    pub payload: Vec<u8>,
    pub raw_dump: String,
    pub payload_sha256_hex: String,
    pub claimed_statement_hash: String,
    pub statement_fields: Vec<String>,
}

pub struct GnarkSpendClient {
    _library: Library,
    prove: PenumbraGnarkSpendProve,
    free: PenumbraGnarkSpendFree,
    shutdown: PenumbraGnarkSpendShutdown,
    handle: u64,
    pvk: PreparedVerifyingKey<Bls12_377>,
    lib_load_ms: f64,
    init_ms: f64,
}

pub struct GnarkSpendProofCall {
    pub payload: Vec<u8>,
}

impl GnarkSpendClient {
    pub fn load(lib_path: &Path, artifact_dir: &Path) -> Result<Self> {
        let lib_start = Instant::now();
        let library = unsafe { Library::new(lib_path) }
            .with_context(|| format!("load gnark library {}", lib_path.display()))?;
        let lib_load_ms = lib_start.elapsed().as_secs_f64() * 1000.0;

        let (init, prove, free, shutdown) = unsafe {
            let init: PenumbraGnarkSpendInit = *library.get(b"penumbra_gnark_spend_init")?;
            let prove: PenumbraGnarkSpendProve = *library.get(b"penumbra_gnark_spend_prove")?;
            let free: PenumbraGnarkSpendFree = *library.get(b"penumbra_gnark_spend_free")?;
            let shutdown: PenumbraGnarkSpendShutdown =
                *library.get(b"penumbra_gnark_spend_shutdown")?;
            (init, prove, free, shutdown)
        };

        let mut init_result = PenumbraGnarkInitResult {
            handle: 0,
            init_ms: 0.0,
            err_ptr: ptr::null_mut(),
            err_len: 0,
        };
        let artifact_dir_c = CString::new(artifact_dir.to_string_lossy().as_bytes().to_vec())
            .context("artifact dir path contains interior NUL byte, cannot pass to gnark init")?;
        unsafe {
            init(
                artifact_dir_c.as_ptr(),
                artifact_dir_c.as_bytes().len(),
                &mut init_result,
            );
        }
        if !init_result.err_ptr.is_null() {
            let err_bytes = take_returned_bytes(init_result.err_ptr, init_result.err_len);
            unsafe { free(init_result.err_ptr, init_result.err_len) };
            return Err(anyhow!(
                "gnark init failed: {}",
                String::from_utf8_lossy(&err_bytes)
            ));
        }

        let pvk = {
            let vk_json: VerifyingKeyJson = serde_json::from_reader(BufReader::new(File::open(
                artifact_dir.join("verifying_key.json"),
            )?))?;
            prepare_verifying_key(&artifacts_to_vk(&vk_json)?)
        };

        Ok(Self {
            _library: library,
            prove,
            free,
            shutdown,
            handle: init_result.handle,
            pvk,
            lib_load_ms,
            init_ms: init_result.init_ms,
        })
    }

    pub fn lib_load_ms(&self) -> f64 {
        self.lib_load_ms
    }

    pub fn init_ms(&self) -> f64 {
        self.init_ms
    }

    pub fn prove_raw(&self, witness: &[u8]) -> Result<GnarkSpendProofCall> {
        let mut prove_result = PenumbraGnarkBytesResult {
            ptr: ptr::null_mut(),
            len: 0,
            status: 0,
        };
        unsafe {
            (self.prove)(
                self.handle,
                witness.as_ptr() as *const c_void,
                witness.len(),
                &mut prove_result,
            );
        }

        let payload = take_returned_bytes(prove_result.ptr, prove_result.len);
        if !prove_result.ptr.is_null() {
            unsafe { (self.free)(prove_result.ptr, prove_result.len) };
        }
        if prove_result.status != 0 {
            return Err(anyhow!(
                "gnark prove failed: {}",
                String::from_utf8_lossy(&payload)
            ));
        }

        Ok(GnarkSpendProofCall { payload })
    }

    pub fn verify(&self, proof: &SpendProof, public: SpendProofPublic) -> Result<()> {
        proof.verify(&self.pvk, public)?;
        Ok(())
    }
}

impl Drop for GnarkSpendClient {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe { (self.shutdown)(self.handle) };
            self.handle = 0;
        }
    }
}

pub fn encode_spend_witness_v1(
    public: &SpendProofPublic,
    private: &SpendProofPrivate,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    put_bytes(&mut buf, SPEND_WITNESS_V1_MAGIC);
    put_u32(&mut buf, SPEND_WITNESS_V1_VERSION);
    put_u32(&mut buf, 0);

    put_bytes(&mut buf, &Fq::from(public.anchor).to_bytes());
    put_bytes(&mut buf, &public.balance_commitment.to_bytes());
    put_bytes(&mut buf, &public.nullifier.0.to_bytes());
    put_bytes(&mut buf, &public.rk.to_bytes());
    put_bytes(&mut buf, &public.asset_anchor.0.to_bytes());
    put_bytes(&mut buf, &public.compliance_anchor.0.to_bytes());
    put_bytes(&mut buf, &<[u8; 32]>::from(Encoding::from(public.epk)));
    put_bytes(&mut buf, &public.c2_core.to_bytes());
    put_u32(
        &mut buf,
        u32::try_from(public.compliance_ciphertext.len())
            .context("compliance ciphertext length exceeds u32")?,
    );
    for value in &public.compliance_ciphertext {
        put_bytes(&mut buf, &value.to_bytes());
    }
    put_bytes(&mut buf, &public.target_timestamp.to_bytes());
    put_bytes(&mut buf, &public.dleq_c.to_bytes());
    put_bytes(&mut buf, &public.dleq_s.to_bytes());
    put_bytes(&mut buf, &public.sender_leaf_hash.0.to_bytes());

    let claimed_statement_hash = spend_statement_hash_from_public(public)
        .map_err(|e| anyhow!("compute claimed statement hash: {e}"))?;
    put_bytes(&mut buf, &claimed_statement_hash.to_bytes());

    let statement_fields =
        spend_statement_fields(public).map_err(|e| anyhow!("compute statement fields: {e}"))?;
    put_u32(
        &mut buf,
        u32::try_from(statement_fields.len()).context("statement fields length exceeds u32")?,
    );
    for value in &statement_fields {
        put_bytes(&mut buf, &value.to_bytes());
    }

    put_bytes(&mut buf, &private.note.note_blinding().to_bytes());
    put_bytes(&mut buf, &Fq::from(private.note.value().amount).to_bytes());
    put_bytes(&mut buf, &private.note.asset_id().0.to_bytes());
    put_bytes(
        &mut buf,
        &<[u8; 32]>::from(Encoding::from(private.note.diversified_generator())),
    );
    put_bytes(&mut buf, &private.note.transmission_key().0);
    put_bytes(
        &mut buf,
        &Fq::from_le_bytes_mod_order(&private.note.clue_key().0).to_bytes(),
    );

    let note_bytes = private.note.to_bytes();
    if note_bytes.len() != 160 {
        return Err(anyhow!("expected 160 note bytes, got {}", note_bytes.len()));
    }
    put_bytes(&mut buf, &note_bytes);
    put_bytes(
        &mut buf,
        &private.state_commitment_proof.commitment().0.to_bytes(),
    );
    put_u64(
        &mut buf,
        u64::from(private.state_commitment_proof.position()),
    );
    let auth_path = private.state_commitment_proof.auth_path();
    put_u32(
        &mut buf,
        u32::try_from(auth_path.len()).context("state auth path length exceeds u32")?,
    );
    for siblings in auth_path {
        for sibling in siblings.iter() {
            put_bytes(&mut buf, &Fq::from(*sibling).to_bytes());
        }
    }
    put_bytes(&mut buf, &private.v_blinding.to_bytes());
    put_bytes(&mut buf, &private.spend_auth_randomizer.to_bytes());
    put_bytes(&mut buf, &private.ak.to_bytes());
    put_bytes(&mut buf, &private.nk.0.to_bytes());
    encode_merkle_path(&mut buf, &private.asset_path)?;
    put_u64(&mut buf, private.asset_position);
    encode_indexed_leaf(&mut buf, &private.asset_indexed_leaf);
    put_u8(&mut buf, u8::from(private.is_regulated));
    encode_merkle_path(&mut buf, &private.compliance_path)?;
    put_u64(&mut buf, private.compliance_position);
    encode_user_leaf(&mut buf, &private.user_leaf);
    put_bytes(&mut buf, &private.compliance_ephemeral_secret.to_bytes());
    put_bytes(&mut buf, &private.tx_blinding_nonce.to_bytes());
    put_u8(&mut buf, u8::from(private.is_flagged));
    put_bytes(&mut buf, &private.salt.to_bytes());

    put_affine_point_bytes(&mut buf, public.balance_commitment.0)?;
    put_affine_point_bytes(
        &mut buf,
        Encoding(public.rk.to_bytes())
            .vartime_decompress()
            .map_err(|e| anyhow!("decompress rk: {e:?}"))?,
    )?;
    put_affine_point_bytes(&mut buf, public.epk)?;
    put_affine_point_bytes(&mut buf, private.note.diversified_generator())?;
    put_affine_point_bytes(
        &mut buf,
        Encoding(private.note.transmission_key().0)
            .vartime_decompress()
            .map_err(|e| anyhow!("decompress transmission key: {e:?}"))?,
    )?;
    put_affine_point_bytes(
        &mut buf,
        Encoding(private.ak.to_bytes())
            .vartime_decompress()
            .map_err(|e| anyhow!("decompress ak: {e:?}"))?,
    )?;
    put_affine_point_bytes(&mut buf, private.asset_indexed_leaf.params.dk_pub)?;
    put_affine_point_bytes(&mut buf, private.asset_indexed_leaf.ring.ring_pk)?;
    put_affine_point_bytes(&mut buf, *private.user_leaf.address.diversified_generator())?;
    put_affine_point_bytes(
        &mut buf,
        Encoding(private.user_leaf.address.transmission_key().0)
            .vartime_decompress()
            .map_err(|e| anyhow!("decompress user transmission key: {e:?}"))?,
    )?;

    let total_len = u32::try_from(buf.len()).context("encoded witness length exceeds u32")?;
    buf[8..12].copy_from_slice(&total_len.to_le_bytes());
    Ok(buf)
}

pub fn encode_spend_witness_v1_debug(
    public: &SpendProofPublic,
    private: &SpendProofPrivate,
) -> Result<SpendWitnessDebugBundle> {
    let payload = encode_spend_witness_v1(public, private)?;
    let claimed_statement_hash = spend_statement_hash_from_public(public)
        .map_err(|e| anyhow!("compute claimed statement hash: {e}"))?;
    let statement_fields =
        spend_statement_fields(public).map_err(|e| anyhow!("compute statement fields: {e}"))?;

    Ok(SpendWitnessDebugBundle {
        payload_sha256_hex: hex_encode(Sha256::digest(&payload)),
        raw_dump: spend_witness_raw_dump(
            public,
            private,
            &payload,
            &claimed_statement_hash,
            &statement_fields,
        )?,
        claimed_statement_hash: claimed_statement_hash.to_string(),
        statement_fields: statement_fields.iter().map(ToString::to_string).collect(),
        payload,
    })
}

pub fn translate_spend_proof_result(payload: &[u8]) -> Result<(Fq, SpendProof)> {
    let (claimed_hash, proof) = parse_binary_proof_result(payload)?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let spend_proof = SpendProof::try_from(shielded_pool_pb::ZkSpendProof { inner: proof_bytes })?;
    Ok((claimed_hash, spend_proof))
}

fn encode_merkle_path(buf: &mut Vec<u8>, path: &MerklePath) -> Result<()> {
    put_u32(
        buf,
        u32::try_from(path.layers.len()).context("merkle layer count exceeds u32")?,
    );
    for layer in &path.layers {
        put_u32(
            buf,
            u32::try_from(layer.siblings.len()).context("merkle sibling count exceeds u32")?,
        );
        for sibling in &layer.siblings {
            if sibling.len() != 32 {
                return Err(anyhow!(
                    "expected 32-byte merkle sibling, got {}",
                    sibling.len()
                ));
            }
            put_bytes(buf, sibling);
        }
    }
    Ok(())
}

fn encode_indexed_leaf(buf: &mut Vec<u8>, leaf: &IndexedLeaf) {
    put_bytes(buf, &leaf.value.to_bytes());
    put_u64(buf, leaf.next_index);
    put_bytes(buf, &leaf.next_value.to_bytes());
    put_bytes(buf, &leaf.params.dk_pub.vartime_compress().0);
    put_u128(buf, leaf.params.threshold);
    put_bytes(buf, &leaf.params.channels_hash.to_bytes());
    put_bytes(buf, &leaf.ring.ring_pk.vartime_compress().0);
    put_bytes(buf, &leaf.ring.ring_id_hash.to_bytes());
    put_bytes(buf, &leaf.ring.policy_id_hash.to_bytes());
    put_bytes(buf, &leaf.ring.permission_hash.to_bytes());
    put_bytes(buf, &leaf.ring.resource_hash.to_bytes());
}

fn encode_user_leaf(buf: &mut Vec<u8>, leaf: &ComplianceLeaf) {
    put_bytes(buf, &leaf.address.to_vec());
    put_bytes(buf, &leaf.asset_id.0.to_bytes());
    put_bytes(buf, &leaf.d.to_bytes());
}

fn put_affine_point_bytes(buf: &mut Vec<u8>, point: decaf377::Element) -> Result<()> {
    let affine = point.into_affine();
    let (x, y) = affine
        .xy()
        .ok_or_else(|| anyhow!("decaf point is identity, affine coordinates unavailable"))?;
    put_bytes(buf, &x.to_bytes());
    put_bytes(buf, &y.to_bytes());
    Ok(())
}

fn put_u8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

fn put_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn put_u128(buf: &mut Vec<u8>, value: u128) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn artifacts_to_vk(vk: &VerifyingKeyJson) -> Result<VerifyingKey<Bls12_377>> {
    Ok(VerifyingKey {
        alpha_g1: parse_g1(&vk.alpha_g1)?,
        beta_g2: parse_g2(&vk.beta_g2)?,
        gamma_g2: parse_g2(&vk.gamma_g2)?,
        delta_g2: parse_g2(&vk.delta_g2)?,
        gamma_abc_g1: vk
            .gamma_abc_g1
            .iter()
            .map(parse_g1)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn parse_g1(point: &G1PointJson) -> Result<ProofG1> {
    let x = ProofG1Base::from_str(&point.x).map_err(|_| anyhow!("invalid G1 x"))?;
    let y = ProofG1Base::from_str(&point.y).map_err(|_| anyhow!("invalid G1 y"))?;
    let point = ProofG1::new_unchecked(x, y);
    if !point.is_on_curve() {
        return Err(anyhow!("G1 point is not on curve"));
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        return Err(anyhow!("G1 point is not in the correct subgroup"));
    }
    Ok(point)
}

fn parse_g2(point: &G2PointJson) -> Result<ProofG2> {
    let x_a0 = ProofG1Base::from_str(&point.x.a0).map_err(|_| anyhow!("invalid G2 x.a0"))?;
    let x_a1 = ProofG1Base::from_str(&point.x.a1).map_err(|_| anyhow!("invalid G2 x.a1"))?;
    let y_a0 = ProofG1Base::from_str(&point.y.a0).map_err(|_| anyhow!("invalid G2 y.a0"))?;
    let y_a1 = ProofG1Base::from_str(&point.y.a1).map_err(|_| anyhow!("invalid G2 y.a1"))?;
    let point = ProofG2::new_unchecked(ProofG2Base::new(x_a0, x_a1), ProofG2Base::new(y_a0, y_a1));
    if !point.is_on_curve() {
        return Err(anyhow!("G2 point is not on curve"));
    }
    if !point.is_in_correct_subgroup_assuming_on_curve() {
        return Err(anyhow!("G2 point is not in the correct subgroup"));
    }
    Ok(point)
}

fn take_returned_bytes(ptr: *mut c_void, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(ptr as *const u8, len) }.to_vec()
}

fn parse_g1_base_be(bytes: &[u8]) -> ProofG1Base {
    ProofG1Base::from_be_bytes_mod_order(bytes)
}

fn parse_binary_proof_result(bytes: &[u8]) -> Result<(Fq, Proof<Bls12_377>)> {
    const MAGIC: &[u8; 4] = b"PSPR";
    const VERSION: u32 = 1;
    const G1_BYTES: usize = 48;
    const CLAIMED_HASH_BYTES: usize = 32;
    const HEADER_LEN: usize = 4 + 4 + 4 + 4 + 8;
    const EXPECTED_LEN: usize = HEADER_LEN + CLAIMED_HASH_BYTES + (2 + 4 + 2) * G1_BYTES;

    if bytes.len() != EXPECTED_LEN {
        return Err(anyhow!(
            "unexpected proof result length: got {}, want {}",
            bytes.len(),
            EXPECTED_LEN
        ));
    }
    if &bytes[0..4] != MAGIC {
        return Err(anyhow!("invalid proof result magic"));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != VERSION {
        return Err(anyhow!("unsupported proof result version {}", version));
    }
    let total_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if total_len != bytes.len() {
        return Err(anyhow!(
            "proof result length mismatch: header={}, actual={}",
            total_len,
            bytes.len()
        ));
    }
    let status = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if status != 0 {
        return Err(anyhow!("proof result returned nonzero status {}", status));
    }

    let claimed_hash =
        Fq::from_le_bytes_mod_order(&bytes[HEADER_LEN..HEADER_LEN + CLAIMED_HASH_BYTES]);
    let mut offset = HEADER_LEN + CLAIMED_HASH_BYTES;
    let next = |offset: &mut usize| {
        let start = *offset;
        *offset += G1_BYTES;
        &bytes[start..*offset]
    };

    let a_x = parse_g1_base_be(next(&mut offset));
    let a_y = parse_g1_base_be(next(&mut offset));
    let b_x_a0 = parse_g1_base_be(next(&mut offset));
    let b_x_a1 = parse_g1_base_be(next(&mut offset));
    let b_y_a0 = parse_g1_base_be(next(&mut offset));
    let b_y_a1 = parse_g1_base_be(next(&mut offset));
    let c_x = parse_g1_base_be(next(&mut offset));
    let c_y = parse_g1_base_be(next(&mut offset));

    let proof = Proof::<Bls12_377> {
        a: ProofG1::new_unchecked(a_x, a_y),
        b: ProofG2::new_unchecked(
            ProofG2Base::new(b_x_a0, b_x_a1),
            ProofG2Base::new(b_y_a0, b_y_a1),
        ),
        c: ProofG1::new_unchecked(c_x, c_y),
    };
    if !proof.a.is_on_curve() || !proof.a.is_in_correct_subgroup_assuming_on_curve() {
        return Err(anyhow!("proof A is invalid"));
    }
    if !proof.b.is_on_curve() || !proof.b.is_in_correct_subgroup_assuming_on_curve() {
        return Err(anyhow!("proof B is invalid"));
    }
    if !proof.c.is_on_curve() || !proof.c.is_in_correct_subgroup_assuming_on_curve() {
        return Err(anyhow!("proof C is invalid"));
    }

    Ok((claimed_hash, proof))
}

fn spend_witness_raw_dump(
    public: &SpendProofPublic,
    private: &SpendProofPrivate,
    payload: &[u8],
    claimed_statement_hash: &Fq,
    statement_fields: &[Fq],
) -> Result<String> {
    let mut out = String::new();

    writeln!(
        &mut out,
        "header.magic={}",
        String::from_utf8_lossy(SPEND_WITNESS_V1_MAGIC)
    )?;
    writeln!(&mut out, "header.version={}", SPEND_WITNESS_V1_VERSION)?;
    writeln!(&mut out, "header.total_length={}", payload.len())?;
    writeln!(
        &mut out,
        "payload.sha256={}",
        hex_encode(Sha256::digest(payload))
    )?;

    append_fq_line(&mut out, "public.anchor", &Fq::from(public.anchor))?;
    append_decaf_point_line(
        &mut out,
        "public.balance_commitment",
        &public.balance_commitment.to_bytes(),
        public.balance_commitment.0,
    )?;
    append_fq_line(&mut out, "public.nullifier", &public.nullifier.0)?;
    append_decaf_point_line(
        &mut out,
        "public.rk",
        &public.rk.to_bytes(),
        decompress_encoding(public.rk.to_bytes(), "rk")?,
    )?;
    append_fq_line(&mut out, "public.asset_anchor", &public.asset_anchor.0)?;
    append_fq_line(
        &mut out,
        "public.compliance_anchor",
        &public.compliance_anchor.0,
    )?;
    append_decaf_point_line(
        &mut out,
        "public.epk",
        &<[u8; 32]>::from(Encoding::from(public.epk)),
        public.epk,
    )?;
    append_fq_line(&mut out, "public.c2_core", &public.c2_core)?;
    writeln!(
        &mut out,
        "public.compliance_ciphertext.len={}",
        public.compliance_ciphertext.len()
    )?;
    for (index, value) in public.compliance_ciphertext.iter().enumerate() {
        append_fq_line(
            &mut out,
            &format!("public.compliance_ciphertext[{index}]"),
            value,
        )?;
    }
    append_fq_line(
        &mut out,
        "public.target_timestamp",
        &public.target_timestamp,
    )?;
    append_fq_line(&mut out, "public.dleq_c", &public.dleq_c)?;
    append_fq_line(&mut out, "public.dleq_s", &public.dleq_s)?;
    append_fq_line(
        &mut out,
        "public.sender_leaf_hash",
        &public.sender_leaf_hash.0,
    )?;
    append_fq_line(
        &mut out,
        "public.claimed_statement_hash",
        claimed_statement_hash,
    )?;
    writeln!(
        &mut out,
        "public.statement_fields.len={}",
        statement_fields.len()
    )?;
    for (index, value) in statement_fields.iter().enumerate() {
        append_fq_line(
            &mut out,
            &format!("public.statement_fields[{index}]"),
            value,
        )?;
    }

    append_fq_line(
        &mut out,
        "private.note_blinding",
        &private.note.note_blinding(),
    )?;
    append_fq_line(
        &mut out,
        "private.note_amount",
        &Fq::from(private.note.value().amount),
    )?;
    append_fq_line(
        &mut out,
        "private.note_asset_id",
        &private.note.asset_id().0,
    )?;
    append_decaf_point_line(
        &mut out,
        "private.note.diversified_generator",
        &<[u8; 32]>::from(Encoding::from(private.note.diversified_generator())),
        private.note.diversified_generator(),
    )?;
    append_decaf_point_line(
        &mut out,
        "private.note.transmission_key",
        &private.note.transmission_key().0,
        decompress_encoding(private.note.transmission_key().0, "transmission key")?,
    )?;
    append_fq_line(
        &mut out,
        "private.note.clue_key",
        &Fq::from_le_bytes_mod_order(&private.note.clue_key().0),
    )?;
    writeln!(
        &mut out,
        "private.note.note_bytes.hex={}",
        hex_encode(private.note.to_bytes())
    )?;
    append_fq_line(
        &mut out,
        "private.state_commitment.commitment",
        &private.state_commitment_proof.commitment().0,
    )?;
    writeln!(
        &mut out,
        "private.state_commitment.position={}",
        u64::from(private.state_commitment_proof.position())
    )?;
    writeln!(
        &mut out,
        "private.state_commitment.auth_path.len={}",
        private.state_commitment_proof.auth_path().len()
    )?;
    for (layer_index, siblings) in private
        .state_commitment_proof
        .auth_path()
        .iter()
        .enumerate()
    {
        for (sibling_index, sibling) in siblings.iter().enumerate() {
            append_fq_line(
                &mut out,
                &format!("private.state_commitment.auth_path[{layer_index}][{sibling_index}]"),
                &Fq::from(*sibling),
            )?;
        }
    }
    append_fr_line(&mut out, "private.v_blinding", &private.v_blinding)?;
    append_fr_line(
        &mut out,
        "private.spend_auth_randomizer",
        &private.spend_auth_randomizer,
    )?;
    append_decaf_point_line(
        &mut out,
        "private.ak",
        &private.ak.to_bytes(),
        decompress_encoding(private.ak.to_bytes(), "ak")?,
    )?;
    append_fq_line(&mut out, "private.nk", &private.nk.0)?;

    append_merkle_path_lines(&mut out, "private.asset_path", &private.asset_path)?;
    writeln!(
        &mut out,
        "private.asset_position={}",
        private.asset_position
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.value",
        &private.asset_indexed_leaf.value,
    )?;
    writeln!(
        &mut out,
        "private.asset_indexed_leaf.next_index={}",
        private.asset_indexed_leaf.next_index
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.next_value",
        &private.asset_indexed_leaf.next_value,
    )?;
    append_decaf_point_line(
        &mut out,
        "private.asset_indexed_leaf.dk_pub",
        &private
            .asset_indexed_leaf
            .params
            .dk_pub
            .vartime_compress()
            .0,
        private.asset_indexed_leaf.params.dk_pub,
    )?;
    append_le_bytes_line(
        &mut out,
        "private.asset_indexed_leaf.threshold",
        &private.asset_indexed_leaf.params.threshold.to_le_bytes(),
        &private.asset_indexed_leaf.params.threshold.to_string(),
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.channels_hash",
        &private.asset_indexed_leaf.params.channels_hash,
    )?;
    append_decaf_point_line(
        &mut out,
        "private.asset_indexed_leaf.ring_pk",
        &private.asset_indexed_leaf.ring.ring_pk.vartime_compress().0,
        private.asset_indexed_leaf.ring.ring_pk,
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.ring_id_hash",
        &private.asset_indexed_leaf.ring.ring_id_hash,
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.policy_id_hash",
        &private.asset_indexed_leaf.ring.policy_id_hash,
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.permission_hash",
        &private.asset_indexed_leaf.ring.permission_hash,
    )?;
    append_fq_line(
        &mut out,
        "private.asset_indexed_leaf.resource_hash",
        &private.asset_indexed_leaf.ring.resource_hash,
    )?;
    writeln!(
        &mut out,
        "private.is_regulated={}",
        u8::from(private.is_regulated)
    )?;

    append_merkle_path_lines(
        &mut out,
        "private.compliance_path",
        &private.compliance_path,
    )?;
    writeln!(
        &mut out,
        "private.compliance_position={}",
        private.compliance_position
    )?;
    writeln!(
        &mut out,
        "private.user_leaf.address.hex={}",
        hex_encode(private.user_leaf.address.to_vec())
    )?;
    append_fq_line(
        &mut out,
        "private.user_leaf.asset_id",
        &private.user_leaf.asset_id.0,
    )?;
    append_fq_line(&mut out, "private.user_leaf.d", &private.user_leaf.d)?;
    append_decaf_point_affine_line(
        &mut out,
        "private.user_leaf.diversified_generator",
        *private.user_leaf.address.diversified_generator(),
    )?;
    append_decaf_point_affine_line(
        &mut out,
        "private.user_leaf.transmission_key",
        decompress_encoding(
            private.user_leaf.address.transmission_key().0,
            "user transmission key",
        )?,
    )?;
    append_fr_line(
        &mut out,
        "private.compliance_ephemeral_secret",
        &private.compliance_ephemeral_secret,
    )?;
    append_fr_line(
        &mut out,
        "private.tx_blinding_nonce",
        &private.tx_blinding_nonce,
    )?;
    writeln!(
        &mut out,
        "private.is_flagged={}",
        u8::from(private.is_flagged)
    )?;
    append_fq_line(&mut out, "private.salt", &private.salt)?;

    Ok(out)
}

fn append_fq_line(out: &mut String, key: &str, value: &Fq) -> Result<()> {
    append_le_bytes_line(out, key, &value.to_bytes(), &value.to_string())
}

fn append_fr_line(out: &mut String, key: &str, value: &Fr) -> Result<()> {
    append_le_bytes_line(out, key, &value.to_bytes(), &value.to_string())
}

fn append_le_bytes_line(out: &mut String, key: &str, bytes: &[u8], dec_value: &str) -> Result<()> {
    writeln!(out, "{key}.le_hex={}", hex_encode(bytes))?;
    writeln!(out, "{key}.dec={dec_value}")?;
    Ok(())
}

fn append_decaf_point_line(
    out: &mut String,
    key: &str,
    encoding_bytes: &[u8],
    point: decaf377::Element,
) -> Result<()> {
    writeln!(out, "{key}.encoding_hex={}", hex_encode(encoding_bytes))?;
    let affine = point.into_affine();
    let (x, y) = affine
        .xy()
        .ok_or_else(|| anyhow!("point {key} is identity, affine coordinates unavailable"))?;
    append_fq_line(out, &format!("{key}.x"), &x)?;
    append_fq_line(out, &format!("{key}.y"), &y)?;
    Ok(())
}

fn append_decaf_point_affine_line(
    out: &mut String,
    key: &str,
    point: decaf377::Element,
) -> Result<()> {
    let affine = point.into_affine();
    let (x, y) = affine
        .xy()
        .ok_or_else(|| anyhow!("point {key} is identity, affine coordinates unavailable"))?;
    append_fq_line(out, &format!("{key}.x"), &x)?;
    append_fq_line(out, &format!("{key}.y"), &y)?;
    Ok(())
}

fn append_merkle_path_lines(out: &mut String, key: &str, path: &MerklePath) -> Result<()> {
    writeln!(out, "{key}.layers={}", path.layers.len())?;
    for (layer_index, layer) in path.layers.iter().enumerate() {
        writeln!(
            out,
            "{key}[{layer_index}].siblings={}",
            layer.siblings.len()
        )?;
        for (sibling_index, sibling) in layer.siblings.iter().enumerate() {
            writeln!(
                out,
                "{key}[{layer_index}][{sibling_index}].hex={}",
                hex_encode(sibling)
            )?;
        }
    }
    Ok(())
}

fn decompress_encoding(bytes: [u8; 32], label: &str) -> Result<decaf377::Element> {
    Encoding(bytes)
        .vartime_decompress()
        .map_err(|e| anyhow!("decompress {label}: {e:?}"))
}
