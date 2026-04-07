use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::PrimeField;
use ark_groth16::{PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Encoding, Fq};

#[cfg(any(unix, windows))]
use crate::gnark::transport::{auto_lib_path, load_bundled_transport, load_library_transport};
use crate::{
    gnark::{
        binary::{encode_vec_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor},
        transport::{
            load_daemon_transport, load_from_env_paths, prove_with_transport, shutdown_transport,
            GnarkFamilyConfig, GnarkTransport,
        },
        typed::{
            compliance_leaf_from_typed, decode_compliance_leaf, decode_indexed_leaf,
            encode_compliance_leaf, encode_indexed_leaf, encode_merkle_path, encode_point_affine,
            indexed_leaf_from_typed, merkle_path_from_typed, point_affine_bytes,
            point_affine_bytes_with_fallback, ComplianceLeafBinary, IndexedLeafBinary,
            MerklePathBinary, PointAffineBytes,
        },
    },
    public_input_hash::{spend_statement_fields, spend_statement_hash_from_public},
    SpendProof, SpendProofPrivate, SpendProofPublic,
};

const SPEND_WITNESS_V1_MAGIC: &[u8; 4] = b"PSWG";
const SPEND_WITNESS_V1_VERSION: u32 = 1;
const SPEND_PROOF_RESULT_MAGIC: &[u8; 4] = b"PSPR";
const SPEND_PROOF_RESULT_VERSION: u32 = 1;

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

const SPEND_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "spend",
    lib_basename: "libpenumbra_gnark_spend",
    env_artifact_dir: "PENUMBRA_GNARK_SPEND_ARTIFACT_DIR",
    env_lib: "PENUMBRA_GNARK_SPEND_LIB",
    env_daemon: "PENUMBRA_GNARK_SPEND_DAEMON",
    init_symbol: b"penumbra_gnark_spend_init",
    init_from_bytes_symbol: b"penumbra_gnark_spend_init_from_bytes",
    prove_symbol: b"penumbra_gnark_spend_prove",
    free_symbol: b"penumbra_gnark_spend_free",
    shutdown_symbol: b"penumbra_gnark_spend_shutdown",
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendWitnessV1 {
    pub anchor: [u8; 32],
    pub balance_commitment: [u8; 32],
    pub nullifier: [u8; 32],
    pub rk: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub epk: [u8; 32],
    pub c2_core: [u8; 32],
    pub compliance_ciphertext: Vec<[u8; 32]>,
    pub target_timestamp: [u8; 32],
    pub dleq_c: [u8; 32],
    pub dleq_s: [u8; 32],
    pub sender_leaf_hash: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub statement_fields: Vec<[u8; 32]>,
    pub note_blinding: [u8; 32],
    pub note_amount: [u8; 32],
    pub note_asset_id: [u8; 32],
    pub diversified_generator: [u8; 32],
    pub transmission_key: [u8; 32],
    pub clue_key: [u8; 32],
    pub note_bytes: [u8; 160],
    pub state_commitment_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub v_blinding: [u8; 32],
    pub spend_auth_randomizer: [u8; 32],
    pub ak: [u8; 32],
    pub nk: [u8; 32],
    pub asset_path: MerklePathBinary,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeafBinary,
    pub is_regulated: bool,
    pub compliance_path: MerklePathBinary,
    pub compliance_position: u64,
    pub user_leaf: ComplianceLeafBinary,
    pub compliance_ephemeral_secret: [u8; 32],
    pub tx_blinding_nonce: [u8; 32],
    pub is_flagged: bool,
    pub salt: [u8; 32],
    pub balance_commitment_affine: PointAffineBytes,
    pub rk_affine: PointAffineBytes,
    pub epk_affine: PointAffineBytes,
    pub diversified_generator_affine: PointAffineBytes,
    pub transmission_key_affine: PointAffineBytes,
    pub ak_affine: PointAffineBytes,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub user_diversified_generator_affine: PointAffineBytes,
    pub user_transmission_key_affine: PointAffineBytes,
}

impl SpendWitnessV1 {
    pub fn from_public_private(
        public: &SpendProofPublic,
        private: &SpendProofPrivate,
    ) -> Result<Self> {
        let claimed_statement_hash = spend_statement_hash_from_public(public)
            .map_err(|e| anyhow!("compute spend statement hash: {e}"))?;
        let statement_fields = spend_statement_fields(public)
            .map_err(|e| anyhow!("compute spend statement fields: {e}"))?;
        let note_bytes = private.note.to_bytes();
        let auth_path = private
            .state_commitment_proof
            .auth_path()
            .iter()
            .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
            .collect::<Vec<_>>();

        Ok(Self {
            anchor: Fq::from(public.anchor).to_bytes(),
            balance_commitment: public.balance_commitment.to_bytes(),
            nullifier: public.nullifier.0.to_bytes(),
            rk: public.rk.to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            epk: Encoding::from(public.epk).0,
            c2_core: public.c2_core.to_bytes(),
            compliance_ciphertext: public
                .compliance_ciphertext
                .iter()
                .map(|value| value.to_bytes())
                .collect(),
            target_timestamp: public.target_timestamp.to_bytes(),
            dleq_c: public.dleq_c.to_bytes(),
            dleq_s: public.dleq_s.to_bytes(),
            sender_leaf_hash: public.sender_leaf_hash.0.to_bytes(),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            statement_fields: statement_fields
                .iter()
                .map(|value| value.to_bytes())
                .collect(),
            note_blinding: private.note.note_blinding().to_bytes(),
            note_amount: Fq::from(private.note.value().amount).to_bytes(),
            note_asset_id: private.note.asset_id().0.to_bytes(),
            diversified_generator: Encoding::from(private.note.diversified_generator()).0,
            transmission_key: private.note.transmission_key().0,
            clue_key: Fq::from_le_bytes_mod_order(&private.note.clue_key().0).to_bytes(),
            note_bytes,
            state_commitment_commitment: private.state_commitment_proof.commitment().0.to_bytes(),
            state_commitment_position: u64::from(private.state_commitment_proof.position()),
            state_commitment_auth_path: auth_path,
            v_blinding: private.v_blinding.to_bytes(),
            spend_auth_randomizer: private.spend_auth_randomizer.to_bytes(),
            ak: private.ak.to_bytes(),
            nk: private.nk.0.to_bytes(),
            asset_path: merkle_path_from_typed(&private.asset_path)?,
            asset_position: private.asset_position,
            asset_indexed_leaf: indexed_leaf_from_typed(&private.asset_indexed_leaf),
            is_regulated: private.is_regulated,
            compliance_path: merkle_path_from_typed(&private.compliance_path)?,
            compliance_position: private.compliance_position,
            user_leaf: compliance_leaf_from_typed(&private.user_leaf)?,
            compliance_ephemeral_secret: private.compliance_ephemeral_secret.to_bytes(),
            tx_blinding_nonce: private.tx_blinding_nonce.to_bytes(),
            is_flagged: private.is_flagged,
            salt: private.salt.to_bytes(),
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            rk_affine: point_affine_bytes(
                Encoding(public.rk.to_bytes())
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress rk: {e:?}"))?,
            )?,
            epk_affine: point_affine_bytes(public.epk)?,
            diversified_generator_affine: point_affine_bytes(private.note.diversified_generator())?,
            transmission_key_affine: point_affine_bytes(
                Encoding(private.note.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress transmission key: {e:?}"))?,
            )?,
            ak_affine: point_affine_bytes(
                Encoding(private.ak.to_bytes())
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress ak: {e:?}"))?,
            )?,
            asset_indexed_leaf_dk_pub_affine: point_affine_bytes_with_fallback(
                private.asset_indexed_leaf.params.dk_pub,
                decaf377::Element::GENERATOR,
            )?,
            asset_indexed_leaf_ring_pk_affine: point_affine_bytes_with_fallback(
                private.asset_indexed_leaf.ring.ring_pk,
                decaf377::Element::GENERATOR,
            )?,
            user_diversified_generator_affine: point_affine_bytes(
                *private.user_leaf.address.diversified_generator(),
            )?,
            user_transmission_key_affine: point_affine_bytes(
                Encoding(private.user_leaf.address.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress user transmission key: {e:?}"))?,
            )?,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, SPEND_WITNESS_V1_MAGIC);
        put_u32(&mut buf, SPEND_WITNESS_V1_VERSION);
        put_u32(&mut buf, 0);

        put_bytes(&mut buf, &self.anchor);
        put_bytes(&mut buf, &self.balance_commitment);
        put_bytes(&mut buf, &self.nullifier);
        put_bytes(&mut buf, &self.rk);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.epk);
        put_bytes(&mut buf, &self.c2_core);
        encode_vec_32(&mut buf, &self.compliance_ciphertext)?;
        put_bytes(&mut buf, &self.target_timestamp);
        put_bytes(&mut buf, &self.dleq_c);
        put_bytes(&mut buf, &self.dleq_s);
        put_bytes(&mut buf, &self.sender_leaf_hash);
        put_bytes(&mut buf, &self.claimed_statement_hash);
        encode_vec_32(&mut buf, &self.statement_fields)?;
        put_bytes(&mut buf, &self.note_blinding);
        put_bytes(&mut buf, &self.note_amount);
        put_bytes(&mut buf, &self.note_asset_id);
        put_bytes(&mut buf, &self.diversified_generator);
        put_bytes(&mut buf, &self.transmission_key);
        put_bytes(&mut buf, &self.clue_key);
        put_bytes(&mut buf, &self.note_bytes);
        put_bytes(&mut buf, &self.state_commitment_commitment);
        put_u64(&mut buf, self.state_commitment_position);
        put_u32(
            &mut buf,
            u32::try_from(self.state_commitment_auth_path.len())
                .context("state commitment path length exceeds u32")?,
        );
        for siblings in &self.state_commitment_auth_path {
            for sibling in siblings {
                put_bytes(&mut buf, sibling);
            }
        }
        put_bytes(&mut buf, &self.v_blinding);
        put_bytes(&mut buf, &self.spend_auth_randomizer);
        put_bytes(&mut buf, &self.ak);
        put_bytes(&mut buf, &self.nk);
        encode_merkle_path(&mut buf, &self.asset_path)?;
        put_u64(&mut buf, self.asset_position);
        encode_indexed_leaf(&mut buf, &self.asset_indexed_leaf);
        put_u8(&mut buf, u8::from(self.is_regulated));
        encode_merkle_path(&mut buf, &self.compliance_path)?;
        put_u64(&mut buf, self.compliance_position);
        encode_compliance_leaf(&mut buf, &self.user_leaf);
        put_bytes(&mut buf, &self.compliance_ephemeral_secret);
        put_bytes(&mut buf, &self.tx_blinding_nonce);
        put_u8(&mut buf, u8::from(self.is_flagged));
        put_bytes(&mut buf, &self.salt);
        encode_point_affine(&mut buf, &self.balance_commitment_affine);
        encode_point_affine(&mut buf, &self.rk_affine);
        encode_point_affine(&mut buf, &self.epk_affine);
        encode_point_affine(&mut buf, &self.diversified_generator_affine);
        encode_point_affine(&mut buf, &self.transmission_key_affine);
        encode_point_affine(&mut buf, &self.ak_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
        encode_point_affine(&mut buf, &self.user_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.user_transmission_key_affine);

        let total_len = u32::try_from(buf.len()).context("encoded spend witness exceeds u32")?;
        buf[8..12].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *SPEND_WITNESS_V1_MAGIC {
            bail!("invalid SpendWitnessV1 magic");
        }
        let version = cursor.read_u32()?;
        if version != SPEND_WITNESS_V1_VERSION {
            bail!("unsupported SpendWitnessV1 version {version}");
        }
        let total_len = cursor.read_u32()? as usize;
        if total_len != bytes.len() {
            bail!(
                "SpendWitnessV1 length mismatch: header={total_len}, actual={}",
                bytes.len()
            );
        }

        let witness = Self {
            anchor: cursor.read_fixed::<32>()?,
            balance_commitment: cursor.read_fixed::<32>()?,
            nullifier: cursor.read_fixed::<32>()?,
            rk: cursor.read_fixed::<32>()?,
            asset_anchor: cursor.read_fixed::<32>()?,
            compliance_anchor: cursor.read_fixed::<32>()?,
            epk: cursor.read_fixed::<32>()?,
            c2_core: cursor.read_fixed::<32>()?,
            compliance_ciphertext: cursor.read_vec_32()?,
            target_timestamp: cursor.read_fixed::<32>()?,
            dleq_c: cursor.read_fixed::<32>()?,
            dleq_s: cursor.read_fixed::<32>()?,
            sender_leaf_hash: cursor.read_fixed::<32>()?,
            claimed_statement_hash: cursor.read_fixed::<32>()?,
            statement_fields: cursor.read_vec_32()?,
            note_blinding: cursor.read_fixed::<32>()?,
            note_amount: cursor.read_fixed::<32>()?,
            note_asset_id: cursor.read_fixed::<32>()?,
            diversified_generator: cursor.read_fixed::<32>()?,
            transmission_key: cursor.read_fixed::<32>()?,
            clue_key: cursor.read_fixed::<32>()?,
            note_bytes: cursor.read_fixed::<160>()?,
            state_commitment_commitment: cursor.read_fixed::<32>()?,
            state_commitment_position: cursor.read_u64()?,
            state_commitment_auth_path: {
                let len = cursor.read_u32()? as usize;
                let mut out = Vec::with_capacity(len);
                for _ in 0..len {
                    out.push([
                        cursor.read_fixed::<32>()?,
                        cursor.read_fixed::<32>()?,
                        cursor.read_fixed::<32>()?,
                    ]);
                }
                out
            },
            v_blinding: cursor.read_fixed::<32>()?,
            spend_auth_randomizer: cursor.read_fixed::<32>()?,
            ak: cursor.read_fixed::<32>()?,
            nk: cursor.read_fixed::<32>()?,
            asset_path: cursor.read_merkle_path()?,
            asset_position: cursor.read_u64()?,
            asset_indexed_leaf: decode_indexed_leaf(&mut cursor)?,
            is_regulated: cursor.read_u8()? != 0,
            compliance_path: cursor.read_merkle_path()?,
            compliance_position: cursor.read_u64()?,
            user_leaf: decode_compliance_leaf(&mut cursor)?,
            compliance_ephemeral_secret: cursor.read_fixed::<32>()?,
            tx_blinding_nonce: cursor.read_fixed::<32>()?,
            is_flagged: cursor.read_u8()? != 0,
            salt: cursor.read_fixed::<32>()?,
            balance_commitment_affine: cursor.read_point_affine()?,
            rk_affine: cursor.read_point_affine()?,
            epk_affine: cursor.read_point_affine()?,
            diversified_generator_affine: cursor.read_point_affine()?,
            transmission_key_affine: cursor.read_point_affine()?,
            ak_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_dk_pub_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_ring_pk_affine: cursor.read_point_affine()?,
            user_diversified_generator_affine: cursor.read_point_affine()?,
            user_transmission_key_affine: cursor.read_point_affine()?,
        };

        cursor.finish("SpendWitnessV1")?;
        Ok(witness)
    }
}

pub struct GnarkSpendClient {
    transport: GnarkTransport,
    pvk: PreparedVerifyingKey<Bls12_377>,
}

// Safety: GnarkSpendTransport::Library holds raw fn pointers and a Library handle,
// both of which are safe to send across threads. The Go library is called with a
// per-context handle so concurrent calls on distinct handles are safe.
unsafe impl Send for GnarkSpendClient {}
unsafe impl Sync for GnarkSpendClient {}

impl GnarkSpendClient {
    #[cfg(any(unix, windows))]
    pub fn load(lib_path: &Path, artifact_dir: &Path) -> Result<Self> {
        Self::load_library(lib_path, artifact_dir)
    }

    #[cfg(any(unix, windows))]
    pub fn load_library(lib_path: &Path, artifact_dir: &Path) -> Result<Self> {
        let (transport, pvk) =
            load_library_transport(lib_path, artifact_dir, &SPEND_FAMILY_CONFIG)?;
        Ok(Self { transport, pvk })
    }

    pub fn load_daemon(binary: &Path, artifact_dir: &Path) -> Result<Self> {
        let (transport, pvk) = load_daemon_transport(binary, artifact_dir, &SPEND_FAMILY_CONFIG)?;
        Ok(Self { transport, pvk })
    }

    pub fn prove(
        &self,
        public: &SpendProofPublic,
        private: &SpendProofPrivate,
    ) -> Result<SpendProof> {
        let witness_model = SpendWitnessV1::from_public_private(public, private)?;
        let expected_hash = Fq::from_le_bytes_mod_order(&witness_model.claimed_statement_hash);
        let witness = witness_model.encode()?;
        let payload = prove_with_transport(&self.transport, &witness, SPEND_FAMILY_CONFIG.family)?;
        let (claimed_hash, spend_proof) = translate_spend_proof_result(&payload)?;
        if claimed_hash != expected_hash {
            bail!(
                "gnark spend proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}"
            );
        }
        spend_proof.verify(&self.pvk, public.clone())?;
        Ok(spend_proof)
    }

    /// Load from bundled PK bytes and a pre-constructed VK, without needing an artifact directory.
    #[cfg(any(unix, windows))]
    pub fn from_bundled(
        lib_path: &Path,
        pk_bytes: &[u8],
        pvk: PreparedVerifyingKey<Bls12_377>,
        metadata_json: &[u8],
    ) -> Result<Self> {
        let transport = load_bundled_transport(
            lib_path,
            pk_bytes,
            &pvk,
            metadata_json,
            &SPEND_FAMILY_CONFIG,
        )?;
        Ok(Self { transport, pvk })
    }

    #[cfg(not(any(unix, windows)))]
    pub fn from_bundled(
        _lib_path: &Path,
        _pk_bytes: &[u8],
        _pvk: PreparedVerifyingKey<Bls12_377>,
        _metadata_json: &[u8],
    ) -> Result<Self> {
        bail!("gnark bundled library loading is not supported on this platform")
    }

    pub fn bundled_lib_path() -> Option<PathBuf> {
        penumbra_sdk_proof_params::GNARK_SPEND_BUNDLED_LIBRARY_PATH.map(PathBuf::from)
    }

    /// Searches for `libpenumbra_gnark_spend.{so,dylib,dll}` in order:
    /// 1. Beside the current executable (production install).
    /// 2. `tools/gnark/` under any ancestor of the executable (workspace dev/test builds).
    #[cfg(any(unix, windows))]
    pub fn auto_lib_path() -> Option<PathBuf> {
        auto_lib_path(SPEND_FAMILY_CONFIG.lib_basename)
    }

    pub fn from_env() -> Result<Self> {
        let (artifact_dir, lib_path, daemon_path) = load_from_env_paths(&SPEND_FAMILY_CONFIG)?;
        match (lib_path, daemon_path) {
            (Some(_), Some(_)) => bail!(
                "both {} and {} are set",
                SPEND_FAMILY_CONFIG.env_lib,
                SPEND_FAMILY_CONFIG.env_daemon
            ),
            #[cfg(any(unix, windows))]
            (Some(lib_path), None) => Self::load_library(&lib_path, &artifact_dir),
            #[cfg(not(any(unix, windows)))]
            (Some(_), None) => bail!("gnark library loading is not supported on this platform"),
            (None, Some(daemon_path)) => Self::load_daemon(&daemon_path, &artifact_dir),
            (None, None) => bail!(
                "set {} or {}",
                SPEND_FAMILY_CONFIG.env_lib,
                SPEND_FAMILY_CONFIG.env_daemon
            ),
        }
    }
}

impl Drop for GnarkSpendClient {
    fn drop(&mut self) {
        shutdown_transport(&mut self.transport);
    }
}

pub fn encode_spend_witness_v1(
    public: &SpendProofPublic,
    private: &SpendProofPrivate,
) -> Result<Vec<u8>> {
    SpendWitnessV1::from_public_private(public, private)?.encode()
}

pub fn decode_spend_witness_v1(bytes: &[u8]) -> Result<SpendWitnessV1> {
    SpendWitnessV1::decode(bytes)
}

pub fn translate_spend_proof_result(payload: &[u8]) -> Result<(Fq, SpendProof)> {
    let (claimed_hash, proof) = parse_spend_binary_proof_result(payload)?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let spend_proof = SpendProof::try_from(
        penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1::ZkSpendProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, spend_proof))
}

fn parse_g1_base_be(bytes: &[u8]) -> ProofG1Base {
    ProofG1Base::from_be_bytes_mod_order(bytes)
}

fn parse_spend_binary_proof_result(bytes: &[u8]) -> Result<(Fq, Proof<Bls12_377>)> {
    const G1_BYTES: usize = 48;
    const CLAIMED_HASH_BYTES: usize = 32;
    const HEADER_LEN: usize = 4 + 4 + 4 + 4 + 8;
    const EXPECTED_LEN: usize = HEADER_LEN + CLAIMED_HASH_BYTES + (2 + 4 + 2) * G1_BYTES;

    if bytes.len() != EXPECTED_LEN {
        bail!(
            "unexpected gnark spend proof result length: got {}, want {}",
            bytes.len(),
            EXPECTED_LEN
        );
    }
    if &bytes[0..4] != SPEND_PROOF_RESULT_MAGIC {
        bail!("invalid gnark spend proof result magic");
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != SPEND_PROOF_RESULT_VERSION {
        bail!("unsupported gnark spend proof result version {version}");
    }
    let total_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if total_len != bytes.len() {
        bail!(
            "gnark spend proof result length mismatch: header={total_len}, actual={}",
            bytes.len()
        );
    }
    let status = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if status != 0 {
        bail!("gnark spend proof result returned nonzero status {status}");
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
        bail!("gnark spend proof A is invalid");
    }
    if !proof.b.is_on_curve() || !proof.b.is_in_correct_subgroup_assuming_on_curve() {
        bail!("gnark spend proof B is invalid");
    }
    if !proof.c.is_on_curve() || !proof.c.is_in_correct_subgroup_assuming_on_curve() {
        bail!("gnark spend proof C is invalid");
    }
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{decode_spend_witness_v1, encode_spend_witness_v1, SpendWitnessV1};
    use crate::{
        test_proof_helpers::proof_test_helpers::{
            generate_test_data, CircuitType, REGULATED_ASSET_ID,
        },
        SpendProofPrivate, SpendProofPublic,
    };
    use decaf377::{Fq, Fr};
    use penumbra_sdk_asset::Balance;
    use penumbra_sdk_sct::Nullifier;
    use penumbra_sdk_tct as tct;

    fn valid_spend_inputs() -> (SpendProofPublic, SpendProofPrivate) {
        let mut rng = rand::thread_rng();
        let test_data =
            generate_test_data(&mut rng, REGULATED_ASSET_ID, 42, true, CircuitType::Spend);

        let mut sct = tct::Tree::new();
        let note_commitment = test_data.note.commit();
        sct.insert(tct::Witness::Keep, note_commitment).unwrap();
        let anchor = sct.root();
        let state_commitment_proof = sct.witness(note_commitment).unwrap();

        let balance_commitment = Balance::from(test_data.value).commit(test_data.balance_blinding);
        let nullifier = Nullifier::derive(
            test_data.fvk.nullifier_key(),
            state_commitment_proof.position(),
            &note_commitment,
        );
        let randomizer = Fr::from(7u64);
        let rk = test_data
            .fvk
            .spend_verification_key()
            .randomize(&randomizer);
        let tx_blinding_nonce = Fr::from(0u64);
        let sender_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(
            test_data.user_leaf.commit(),
            tx_blinding_nonce,
        );

        let public = SpendProofPublic {
            anchor,
            balance_commitment,
            nullifier,
            rk,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            epk: test_data.epk_1,
            c2_core: test_data.c2_core,
            compliance_ciphertext: test_data.compliance_ciphertext,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c: test_data.dleq_c,
            dleq_s: test_data.dleq_s,
            sender_leaf_hash,
        };
        let private = SpendProofPrivate {
            state_commitment_proof,
            note: test_data.note,
            v_blinding: test_data.balance_blinding,
            spend_auth_randomizer: randomizer,
            ak: *test_data.fvk.spend_verification_key(),
            nk: *test_data.fvk.nullifier_key(),
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated: true,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            tx_blinding_nonce,
            is_flagged: false,
            salt: test_data.salt,
        };

        (public, private)
    }

    #[test]
    fn spend_witness_v1_roundtrip() {
        let (public, private) = valid_spend_inputs();
        let encoded = encode_spend_witness_v1(&public, &private).expect("encode spend witness");
        let decoded = decode_spend_witness_v1(&encoded).expect("decode spend witness");
        let expected =
            SpendWitnessV1::from_public_private(&public, &private).expect("build witness");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn spend_witness_v1_rejects_bad_magic() {
        let (public, private) = valid_spend_inputs();
        let mut encoded = encode_spend_witness_v1(&public, &private).expect("encode spend witness");
        encoded[0] = b'X';
        assert!(decode_spend_witness_v1(&encoded).is_err());
    }

    #[test]
    fn spend_witness_v1_rejects_bad_version() {
        let (public, private) = valid_spend_inputs();
        let mut encoded = encode_spend_witness_v1(&public, &private).expect("encode spend witness");
        encoded[4..8].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode_spend_witness_v1(&encoded).is_err());
    }

    #[test]
    fn spend_witness_v1_rejects_bad_length() {
        let (public, private) = valid_spend_inputs();
        let mut encoded = encode_spend_witness_v1(&public, &private).expect("encode spend witness");
        let wrong_len = (encoded.len() as u32).saturating_sub(1);
        encoded[8..12].copy_from_slice(&wrong_len.to_le_bytes());
        assert!(decode_spend_witness_v1(&encoded).is_err());
    }
}
