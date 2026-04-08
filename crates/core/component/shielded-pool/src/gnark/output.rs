use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use ark_ec::{pairing::Pairing, AffineRepr};
use ark_ff::PrimeField;
use ark_groth16::{PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalSerialize;
use decaf377::Encoding;
use decaf377::{Bls12_377, Fq};

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
    output::{OutputProof, OutputProofPrivate, OutputProofPublic},
    public_input_hash::{output_statement_fields, output_statement_hash_from_public},
};

const OUTPUT_WITNESS_V1_MAGIC: &[u8; 4] = b"POWG";
const OUTPUT_WITNESS_V1_VERSION: u32 = 1;
const OUTPUT_PROOF_RESULT_MAGIC: &[u8; 4] = b"POPR";
const OUTPUT_PROOF_RESULT_VERSION: u32 = 1;

type ProofG1 = <Bls12_377 as Pairing>::G1Affine;
type ProofG2 = <Bls12_377 as Pairing>::G2Affine;
type ProofG1Base = <ProofG1 as AffineRepr>::BaseField;
type ProofG2Base = <ProofG2 as AffineRepr>::BaseField;

const OUTPUT_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "output",
    lib_basename: "libpenumbra_gnark_output",
    env_artifact_dir: "PENUMBRA_GNARK_OUTPUT_ARTIFACT_DIR",
    env_lib: "PENUMBRA_GNARK_OUTPUT_LIB",
    env_daemon: "PENUMBRA_GNARK_OUTPUT_DAEMON",
    init_symbol: b"penumbra_gnark_output_init",
    init_from_bytes_symbol: b"penumbra_gnark_output_init_from_bytes",
    prove_symbol: b"penumbra_gnark_output_prove",
    free_symbol: b"penumbra_gnark_output_free",
    shutdown_symbol: b"penumbra_gnark_output_shutdown",
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputWitnessV1 {
    pub note_commitment: [u8; 32],
    pub balance_commitment: [u8; 32],
    pub epk_1: [u8; 32],
    pub epk_2: [u8; 32],
    pub epk_3: [u8; 32],
    pub c2_core: [u8; 32],
    pub c2_ext: [u8; 32],
    pub c2_sext: [u8; 32],
    pub compliance_ciphertext: Vec<[u8; 32]>,
    pub target_timestamp: [u8; 32],
    pub dleq_c_1: [u8; 32],
    pub dleq_s_1: [u8; 32],
    pub dleq_c_2: [u8; 32],
    pub dleq_s_2: [u8; 32],
    pub dleq_c_3: [u8; 32],
    pub dleq_s_3: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub counterparty_leaf_hash: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub statement_fields: Vec<[u8; 32]>,
    pub note_blinding: [u8; 32],
    pub note_amount: [u8; 32],
    pub note_asset_id: [u8; 32],
    pub diversified_generator: [u8; 32],
    pub transmission_key: [u8; 32],
    pub clue_key: [u8; 32],
    pub note_bytes: [u8; 160],
    pub balance_blinding: [u8; 32],
    pub asset_path: MerklePathBinary,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeafBinary,
    pub is_regulated: bool,
    pub compliance_path: MerklePathBinary,
    pub compliance_position: u64,
    pub user_leaf: ComplianceLeafBinary,
    pub compliance_ephemeral_secret: [u8; 32],
    pub r_2: [u8; 32],
    pub r_3: [u8; 32],
    pub counterparty_leaf: ComplianceLeafBinary,
    pub tx_blinding_nonce: [u8; 32],
    pub is_flagged: bool,
    pub salt: [u8; 32],
    pub balance_commitment_affine: PointAffineBytes,
    pub epk_1_affine: PointAffineBytes,
    pub epk_2_affine: PointAffineBytes,
    pub epk_3_affine: PointAffineBytes,
    pub note_diversified_generator_affine: PointAffineBytes,
    pub note_transmission_key_affine: PointAffineBytes,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub user_diversified_generator_affine: PointAffineBytes,
    pub user_transmission_key_affine: PointAffineBytes,
    pub counterparty_diversified_generator_affine: PointAffineBytes,
    pub counterparty_transmission_key_affine: PointAffineBytes,
}

impl OutputWitnessV1 {
    pub fn from_public_private(
        public: &OutputProofPublic,
        private: &OutputProofPrivate,
    ) -> Result<Self> {
        let claimed_statement_hash = output_statement_hash_from_public(public)
            .map_err(|e| anyhow!("compute output statement hash: {e}"))?;
        let statement_fields = output_statement_fields(public)
            .map_err(|e| anyhow!("compute output statement fields: {e}"))?;

        Ok(Self {
            note_commitment: public.note_commitment.0.to_bytes(),
            balance_commitment: public.balance_commitment.to_bytes(),
            epk_1: Encoding::from(public.epk_1).0,
            epk_2: Encoding::from(public.epk_2).0,
            epk_3: Encoding::from(public.epk_3).0,
            c2_core: public.c2_core.to_bytes(),
            c2_ext: public.c2_ext.to_bytes(),
            c2_sext: public.c2_sext.to_bytes(),
            compliance_ciphertext: public
                .compliance_ciphertext
                .iter()
                .map(|value| value.to_bytes())
                .collect(),
            target_timestamp: public.target_timestamp.to_bytes(),
            dleq_c_1: public.dleq_c_1.to_bytes(),
            dleq_s_1: public.dleq_s_1.to_bytes(),
            dleq_c_2: public.dleq_c_2.to_bytes(),
            dleq_s_2: public.dleq_s_2.to_bytes(),
            dleq_c_3: public.dleq_c_3.to_bytes(),
            dleq_s_3: public.dleq_s_3.to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            counterparty_leaf_hash: public.counterparty_leaf_hash.0.to_bytes(),
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
            note_bytes: private.note.to_bytes(),
            balance_blinding: private.balance_blinding.to_bytes(),
            asset_path: merkle_path_from_typed(&private.asset_path)?,
            asset_position: private.asset_position,
            asset_indexed_leaf: indexed_leaf_from_typed(&private.asset_indexed_leaf),
            is_regulated: private.is_regulated,
            compliance_path: merkle_path_from_typed(&private.compliance_path)?,
            compliance_position: private.compliance_position,
            user_leaf: compliance_leaf_from_typed(&private.user_leaf)?,
            compliance_ephemeral_secret: private.compliance_ephemeral_secret.to_bytes(),
            r_2: private.r_2.to_bytes(),
            r_3: private.r_3.to_bytes(),
            counterparty_leaf: compliance_leaf_from_typed(&private.counterparty_leaf)?,
            tx_blinding_nonce: private.tx_blinding_nonce.to_bytes(),
            is_flagged: private.is_flagged,
            salt: private.salt.to_bytes(),
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            epk_1_affine: point_affine_bytes(public.epk_1)?,
            epk_2_affine: point_affine_bytes(public.epk_2)?,
            epk_3_affine: point_affine_bytes(public.epk_3)?,
            note_diversified_generator_affine: point_affine_bytes(
                private.note.diversified_generator(),
            )?,
            note_transmission_key_affine: point_affine_bytes(
                Encoding(private.note.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress output transmission key: {e:?}"))?,
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
                    .map_err(|e| anyhow!("decompress output user transmission key: {e:?}"))?,
            )?,
            counterparty_diversified_generator_affine: point_affine_bytes(
                *private.counterparty_leaf.address.diversified_generator(),
            )?,
            counterparty_transmission_key_affine: point_affine_bytes(
                Encoding(private.counterparty_leaf.address.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| {
                        anyhow!("decompress output counterparty transmission key: {e:?}")
                    })?,
            )?,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, OUTPUT_WITNESS_V1_MAGIC);
        put_u32(&mut buf, OUTPUT_WITNESS_V1_VERSION);
        put_u32(&mut buf, 0);

        put_bytes(&mut buf, &self.note_commitment);
        put_bytes(&mut buf, &self.balance_commitment);
        put_bytes(&mut buf, &self.epk_1);
        put_bytes(&mut buf, &self.epk_2);
        put_bytes(&mut buf, &self.epk_3);
        put_bytes(&mut buf, &self.c2_core);
        put_bytes(&mut buf, &self.c2_ext);
        put_bytes(&mut buf, &self.c2_sext);
        encode_vec_32(&mut buf, &self.compliance_ciphertext)?;
        put_bytes(&mut buf, &self.target_timestamp);
        put_bytes(&mut buf, &self.dleq_c_1);
        put_bytes(&mut buf, &self.dleq_s_1);
        put_bytes(&mut buf, &self.dleq_c_2);
        put_bytes(&mut buf, &self.dleq_s_2);
        put_bytes(&mut buf, &self.dleq_c_3);
        put_bytes(&mut buf, &self.dleq_s_3);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.counterparty_leaf_hash);
        put_bytes(&mut buf, &self.claimed_statement_hash);
        encode_vec_32(&mut buf, &self.statement_fields)?;
        put_bytes(&mut buf, &self.note_blinding);
        put_bytes(&mut buf, &self.note_amount);
        put_bytes(&mut buf, &self.note_asset_id);
        put_bytes(&mut buf, &self.diversified_generator);
        put_bytes(&mut buf, &self.transmission_key);
        put_bytes(&mut buf, &self.clue_key);
        put_bytes(&mut buf, &self.note_bytes);
        put_bytes(&mut buf, &self.balance_blinding);
        encode_merkle_path(&mut buf, &self.asset_path)?;
        put_u64(&mut buf, self.asset_position);
        encode_indexed_leaf(&mut buf, &self.asset_indexed_leaf);
        put_u8(&mut buf, u8::from(self.is_regulated));
        encode_merkle_path(&mut buf, &self.compliance_path)?;
        put_u64(&mut buf, self.compliance_position);
        encode_compliance_leaf(&mut buf, &self.user_leaf);
        put_bytes(&mut buf, &self.compliance_ephemeral_secret);
        put_bytes(&mut buf, &self.r_2);
        put_bytes(&mut buf, &self.r_3);
        encode_compliance_leaf(&mut buf, &self.counterparty_leaf);
        put_bytes(&mut buf, &self.tx_blinding_nonce);
        put_u8(&mut buf, u8::from(self.is_flagged));
        put_bytes(&mut buf, &self.salt);
        encode_point_affine(&mut buf, &self.balance_commitment_affine);
        encode_point_affine(&mut buf, &self.epk_1_affine);
        encode_point_affine(&mut buf, &self.epk_2_affine);
        encode_point_affine(&mut buf, &self.epk_3_affine);
        encode_point_affine(&mut buf, &self.note_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.note_transmission_key_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
        encode_point_affine(&mut buf, &self.user_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.user_transmission_key_affine);
        encode_point_affine(&mut buf, &self.counterparty_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.counterparty_transmission_key_affine);

        let total_len = u32::try_from(buf.len()).context("encoded output witness exceeds u32")?;
        buf[8..12].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *OUTPUT_WITNESS_V1_MAGIC {
            return Err(anyhow!("invalid OutputWitnessV1 magic"));
        }
        let version = cursor.read_u32()?;
        if version != OUTPUT_WITNESS_V1_VERSION {
            return Err(anyhow!("unsupported OutputWitnessV1 version {version}"));
        }
        let total_len = cursor.read_u32()? as usize;
        if total_len != bytes.len() {
            return Err(anyhow!(
                "OutputWitnessV1 length mismatch: header={total_len}, actual={}",
                bytes.len()
            ));
        }

        let witness = Self {
            note_commitment: cursor.read_fixed::<32>()?,
            balance_commitment: cursor.read_fixed::<32>()?,
            epk_1: cursor.read_fixed::<32>()?,
            epk_2: cursor.read_fixed::<32>()?,
            epk_3: cursor.read_fixed::<32>()?,
            c2_core: cursor.read_fixed::<32>()?,
            c2_ext: cursor.read_fixed::<32>()?,
            c2_sext: cursor.read_fixed::<32>()?,
            compliance_ciphertext: cursor.read_vec_32()?,
            target_timestamp: cursor.read_fixed::<32>()?,
            dleq_c_1: cursor.read_fixed::<32>()?,
            dleq_s_1: cursor.read_fixed::<32>()?,
            dleq_c_2: cursor.read_fixed::<32>()?,
            dleq_s_2: cursor.read_fixed::<32>()?,
            dleq_c_3: cursor.read_fixed::<32>()?,
            dleq_s_3: cursor.read_fixed::<32>()?,
            asset_anchor: cursor.read_fixed::<32>()?,
            compliance_anchor: cursor.read_fixed::<32>()?,
            counterparty_leaf_hash: cursor.read_fixed::<32>()?,
            claimed_statement_hash: cursor.read_fixed::<32>()?,
            statement_fields: cursor.read_vec_32()?,
            note_blinding: cursor.read_fixed::<32>()?,
            note_amount: cursor.read_fixed::<32>()?,
            note_asset_id: cursor.read_fixed::<32>()?,
            diversified_generator: cursor.read_fixed::<32>()?,
            transmission_key: cursor.read_fixed::<32>()?,
            clue_key: cursor.read_fixed::<32>()?,
            note_bytes: cursor.read_fixed::<160>()?,
            balance_blinding: cursor.read_fixed::<32>()?,
            asset_path: cursor.read_merkle_path()?,
            asset_position: cursor.read_u64()?,
            asset_indexed_leaf: decode_indexed_leaf(&mut cursor)?,
            is_regulated: cursor.read_u8()? != 0,
            compliance_path: cursor.read_merkle_path()?,
            compliance_position: cursor.read_u64()?,
            user_leaf: decode_compliance_leaf(&mut cursor)?,
            compliance_ephemeral_secret: cursor.read_fixed::<32>()?,
            r_2: cursor.read_fixed::<32>()?,
            r_3: cursor.read_fixed::<32>()?,
            counterparty_leaf: decode_compliance_leaf(&mut cursor)?,
            tx_blinding_nonce: cursor.read_fixed::<32>()?,
            is_flagged: cursor.read_u8()? != 0,
            salt: cursor.read_fixed::<32>()?,
            balance_commitment_affine: cursor.read_point_affine()?,
            epk_1_affine: cursor.read_point_affine()?,
            epk_2_affine: cursor.read_point_affine()?,
            epk_3_affine: cursor.read_point_affine()?,
            note_diversified_generator_affine: cursor.read_point_affine()?,
            note_transmission_key_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_dk_pub_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_ring_pk_affine: cursor.read_point_affine()?,
            user_diversified_generator_affine: cursor.read_point_affine()?,
            user_transmission_key_affine: cursor.read_point_affine()?,
            counterparty_diversified_generator_affine: cursor.read_point_affine()?,
            counterparty_transmission_key_affine: cursor.read_point_affine()?,
        };

        cursor.finish("OutputWitnessV1")?;
        Ok(witness)
    }
}

pub fn encode_output_witness_v1(
    public: &OutputProofPublic,
    private: &OutputProofPrivate,
) -> Result<Vec<u8>> {
    OutputWitnessV1::from_public_private(public, private)?.encode()
}

pub fn decode_output_witness_v1(bytes: &[u8]) -> Result<OutputWitnessV1> {
    OutputWitnessV1::decode(bytes)
}

pub struct GnarkOutputClient {
    transport: GnarkTransport,
    pvk: PreparedVerifyingKey<Bls12_377>,
}

// Safety: GnarkOutputTransport::Library holds raw fn pointers and a Library handle,
// both of which are safe to send across threads. The Go library is called with a
// per-context handle so concurrent calls on distinct handles are safe.
unsafe impl Send for GnarkOutputClient {}
unsafe impl Sync for GnarkOutputClient {}

impl GnarkOutputClient {
    #[cfg(any(unix, windows))]
    pub fn load(lib_path: &Path, artifact_dir: &Path) -> Result<Self> {
        Self::load_library(lib_path, artifact_dir)
    }

    #[cfg(any(unix, windows))]
    pub fn load_library(lib_path: &Path, artifact_dir: &Path) -> Result<Self> {
        let (transport, pvk) =
            load_library_transport(lib_path, artifact_dir, &OUTPUT_FAMILY_CONFIG)?;
        Ok(Self { transport, pvk })
    }

    pub fn load_daemon(binary: &Path, artifact_dir: &Path) -> Result<Self> {
        let (transport, pvk) = load_daemon_transport(binary, artifact_dir, &OUTPUT_FAMILY_CONFIG)?;
        Ok(Self { transport, pvk })
    }

    pub fn prove(
        &self,
        public: &OutputProofPublic,
        private: &OutputProofPrivate,
    ) -> Result<OutputProof> {
        let witness_model = OutputWitnessV1::from_public_private(public, private)?;
        let expected_hash = Fq::from_le_bytes_mod_order(&witness_model.claimed_statement_hash);
        let witness = witness_model.encode()?;
        let payload = prove_with_transport(&self.transport, &witness, OUTPUT_FAMILY_CONFIG.family)?;
        let (claimed_hash, output_proof) = translate_output_proof_result(&payload)?;
        if claimed_hash != expected_hash {
            bail!(
                "gnark output proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}"
            );
        }
        output_proof.verify(&self.pvk, public.clone())?;
        Ok(output_proof)
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
            &OUTPUT_FAMILY_CONFIG,
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
        penumbra_sdk_proof_params::GNARK_OUTPUT_BUNDLED_LIBRARY_PATH.map(PathBuf::from)
    }

    /// Searches for `libpenumbra_gnark_output.{so,dylib,dll}` in order:
    /// 1. Beside the current executable (production install).
    /// 2. `tools/gnark/` under any ancestor of the executable (workspace dev/test builds).
    #[cfg(any(unix, windows))]
    pub fn auto_lib_path() -> Option<PathBuf> {
        auto_lib_path(OUTPUT_FAMILY_CONFIG.lib_basename)
    }

    pub fn from_env() -> Result<Self> {
        let (artifact_dir, lib_path, daemon_path) = load_from_env_paths(&OUTPUT_FAMILY_CONFIG)?;
        match (lib_path, daemon_path) {
            (Some(_), Some(_)) => bail!(
                "both {} and {} are set",
                OUTPUT_FAMILY_CONFIG.env_lib,
                OUTPUT_FAMILY_CONFIG.env_daemon
            ),
            #[cfg(any(unix, windows))]
            (Some(lib_path), None) => Self::load_library(&lib_path, &artifact_dir),
            #[cfg(not(any(unix, windows)))]
            (Some(_), None) => bail!("gnark library loading is not supported on this platform"),
            (None, Some(daemon_path)) => Self::load_daemon(&daemon_path, &artifact_dir),
            (None, None) => bail!(
                "set {} or {}",
                OUTPUT_FAMILY_CONFIG.env_lib,
                OUTPUT_FAMILY_CONFIG.env_daemon
            ),
        }
    }
}

impl Drop for GnarkOutputClient {
    fn drop(&mut self) {
        shutdown_transport(&mut self.transport);
    }
}

pub fn translate_output_proof_result(payload: &[u8]) -> Result<(Fq, OutputProof)> {
    let (claimed_hash, proof) = parse_output_binary_proof_result(payload)?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let output_proof = OutputProof::try_from(
        penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1::ZkOutputProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, output_proof))
}

fn parse_g1_base_be(bytes: &[u8]) -> ProofG1Base {
    ProofG1Base::from_be_bytes_mod_order(bytes)
}

fn parse_output_binary_proof_result(bytes: &[u8]) -> Result<(Fq, Proof<Bls12_377>)> {
    const G1_BYTES: usize = 48;
    const CLAIMED_HASH_BYTES: usize = 32;
    const HEADER_LEN: usize = 4 + 4 + 4 + 4 + 8;
    const EXPECTED_LEN: usize = HEADER_LEN + CLAIMED_HASH_BYTES + (2 + 4 + 2) * G1_BYTES;

    if bytes.len() != EXPECTED_LEN {
        bail!(
            "unexpected gnark output proof result length: got {}, want {}",
            bytes.len(),
            EXPECTED_LEN
        );
    }
    if &bytes[0..4] != OUTPUT_PROOF_RESULT_MAGIC {
        bail!("invalid gnark output proof result magic");
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != OUTPUT_PROOF_RESULT_VERSION {
        bail!("unsupported gnark output proof result version {version}");
    }
    let total_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if total_len != bytes.len() {
        bail!(
            "gnark output proof result length mismatch: header={total_len}, actual={}",
            bytes.len()
        );
    }
    let status = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if status != 0 {
        bail!("gnark output proof result returned nonzero status {status}");
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
        bail!("gnark output proof A is invalid");
    }
    if !proof.b.is_on_curve() || !proof.b.is_in_correct_subgroup_assuming_on_curve() {
        bail!("gnark output proof B is invalid");
    }
    if !proof.c.is_on_curve() || !proof.c.is_in_correct_subgroup_assuming_on_curve() {
        bail!("gnark output proof C is invalid");
    }
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{decode_output_witness_v1, encode_output_witness_v1, OutputWitnessV1};
    use crate::{
        output::{OutputProofPrivate, OutputProofPublic},
        test_proof_helpers::proof_test_helpers::{
            generate_test_data, CircuitType, REGULATED_ASSET_ID,
        },
    };
    use decaf377::{Fq, Fr};
    use penumbra_sdk_asset::Balance;

    fn valid_output_inputs() -> (OutputProofPublic, OutputProofPrivate) {
        let mut rng = rand::thread_rng();
        let test_data =
            generate_test_data(&mut rng, REGULATED_ASSET_ID, 42, true, CircuitType::Output);

        let note_commitment = test_data.note.commit();
        let balance_commitment =
            (-Balance::from(test_data.value)).commit(test_data.balance_blinding);
        let tx_blinding_nonce = Fr::from(0u64);
        let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(
            test_data.counterparty_leaf.commit(),
            tx_blinding_nonce,
        );

        let public = OutputProofPublic {
            balance_commitment,
            note_commitment,
            epk_1: test_data.epk_1,
            epk_2: test_data.epk_2.expect("output test requires epk_2"),
            epk_3: test_data.epk_3.expect("output test requires epk_3"),
            c2_core: test_data.c2_core,
            c2_ext: test_data.c2_ext.expect("output test requires c2_ext"),
            c2_sext: test_data.c2_sext.expect("output test requires c2_sext"),
            compliance_ciphertext: test_data.compliance_ciphertext,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c_1: test_data.dleq_c,
            dleq_s_1: test_data.dleq_s,
            dleq_c_2: test_data.dleq_c_2,
            dleq_s_2: test_data.dleq_s_2,
            dleq_c_3: test_data.dleq_c_3,
            dleq_s_3: test_data.dleq_s_3,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            counterparty_leaf_hash,
        };
        let private = OutputProofPrivate {
            note: test_data.note,
            balance_blinding: test_data.balance_blinding,
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated: true,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            r_2: test_data.r_2.expect("output test requires r_2"),
            r_3: test_data.r_3.expect("output test requires r_3"),
            counterparty_leaf: test_data.counterparty_leaf,
            tx_blinding_nonce,
            is_flagged: false,
            salt: test_data.salt,
        };

        (public, private)
    }

    #[test]
    fn output_witness_v1_roundtrip() {
        let (public, private) = valid_output_inputs();
        let encoded = encode_output_witness_v1(&public, &private).expect("encode output witness");
        let decoded = decode_output_witness_v1(&encoded).expect("decode output witness");
        let expected =
            OutputWitnessV1::from_public_private(&public, &private).expect("build witness");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn output_witness_v1_rejects_bad_magic() {
        let (public, private) = valid_output_inputs();
        let mut encoded =
            encode_output_witness_v1(&public, &private).expect("encode output witness");
        encoded[0] = b'X';
        assert!(decode_output_witness_v1(&encoded).is_err());
    }

    #[test]
    fn output_witness_v1_rejects_bad_version() {
        let (public, private) = valid_output_inputs();
        let mut encoded =
            encode_output_witness_v1(&public, &private).expect("encode output witness");
        encoded[4..8].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode_output_witness_v1(&encoded).is_err());
    }

    #[test]
    fn output_witness_v1_rejects_bad_length() {
        let (public, private) = valid_output_inputs();
        let mut encoded =
            encode_output_witness_v1(&public, &private).expect("encode output witness");
        let wrong_len = (encoded.len() as u32).saturating_sub(1);
        encoded[8..12].copy_from_slice(&wrong_len.to_le_bytes());
        assert!(decode_output_witness_v1(&encoded).is_err());
    }

    /// Write `output_witness_v1.bin` to the gnark vectors directory.
    /// Run with: cargo test -p penumbra-sdk-shielded-pool -- --ignored bless_output_witness_v1
    #[test]
    #[ignore = "bless: regenerate output_witness_v1.bin for gnark parity tests"]
    fn bless_output_witness_v1() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0000_0042_4f5554); // "BOUT"
        let test_data =
            generate_test_data(&mut rng, REGULATED_ASSET_ID, 42, true, CircuitType::Output);

        let note_commitment = test_data.note.commit();
        let balance_commitment = (-penumbra_sdk_asset::Balance::from(test_data.value))
            .commit(test_data.balance_blinding);
        let tx_blinding_nonce = Fr::from(0u64);
        let counterparty_leaf_hash = penumbra_sdk_compliance::blind_sender_leaf(
            test_data.counterparty_leaf.commit(),
            tx_blinding_nonce,
        );

        let public = OutputProofPublic {
            balance_commitment,
            note_commitment,
            epk_1: test_data.epk_1,
            epk_2: test_data.epk_2.expect("output bless requires epk_2"),
            epk_3: test_data.epk_3.expect("output bless requires epk_3"),
            c2_core: test_data.c2_core,
            c2_ext: test_data.c2_ext.expect("output bless requires c2_ext"),
            c2_sext: test_data.c2_sext.expect("output bless requires c2_sext"),
            compliance_ciphertext: test_data.compliance_ciphertext,
            asset_anchor: test_data.asset_anchor,
            compliance_anchor: test_data.compliance_anchor,
            target_timestamp: Fq::from(test_data.target_timestamp),
            dleq_c_1: test_data.dleq_c,
            dleq_s_1: test_data.dleq_s,
            dleq_c_2: test_data.dleq_c_2,
            dleq_s_2: test_data.dleq_s_2,
            dleq_c_3: test_data.dleq_c_3,
            dleq_s_3: test_data.dleq_s_3,
            counterparty_leaf_hash,
        };
        let private = OutputProofPrivate {
            note: test_data.note,
            balance_blinding: test_data.balance_blinding,
            asset_path: test_data.asset_path,
            asset_position: test_data.asset_position,
            asset_indexed_leaf: test_data.asset_indexed_leaf,
            is_regulated: true,
            compliance_path: test_data.compliance_path,
            compliance_position: test_data.compliance_position,
            user_leaf: test_data.user_leaf,
            compliance_ephemeral_secret: test_data.ephemeral_secret,
            r_2: test_data.r_2.expect("output bless requires r_2"),
            r_3: test_data.r_3.expect("output bless requires r_3"),
            counterparty_leaf: test_data.counterparty_leaf,
            tx_blinding_nonce,
            is_flagged: false,
            salt: test_data.salt,
        };

        let encoded = encode_output_witness_v1(&public, &private).expect("encode output witness");
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../tools/gnark/internal/primitives/vectors/output_witness_v1.bin");
        std::fs::write(&path, &encoded)
            .unwrap_or_else(|e| panic!("write output_witness_v1.bin to {path:?}: {e}"));
        eprintln!("wrote {} bytes to {path:?}", encoded.len());
    }
}
