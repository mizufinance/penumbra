use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use ark_groth16::PreparedVerifyingKey;
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Fq};

#[cfg(any(unix, windows))]
use crate::gnark::transport::{auto_lib_path, load_bundled_transport, load_library_transport};

use crate::{
    consolidate::{ConsolidateProof, ConsolidateProofPrivate, ConsolidateProofPublic},
    gnark::{
        note_reshape_witness::ConsolidateWitnessV1,
        transfer_proof_result::parse_binary_proof_result,
        transport::{
            load_daemon_transport, load_from_env_paths, prove_with_transport, shutdown_transport,
            GnarkFamilyConfig, GnarkTransport,
        },
    },
    ConsolidateFamilyId,
};

const CONSOLIDATE_LIB_BASENAME: &str = "libpenumbra_gnark_consolidate";
const CONSOLIDATE_ENV_ARTIFACT_DIR: &str = "PENUMBRA_GNARK_CONSOLIDATE_ARTIFACT_DIR";
const CONSOLIDATE_ENV_LIB: &str = "PENUMBRA_GNARK_CONSOLIDATE_LIB";
const CONSOLIDATE_ENV_DAEMON: &str = "PENUMBRA_GNARK_CONSOLIDATE_DAEMON";

const CONSOLIDATE_INIT_SYMBOL: &[u8] = b"penumbra_gnark_consolidate_init";
const CONSOLIDATE_INIT_FROM_BYTES_SYMBOL: &[u8] = b"penumbra_gnark_consolidate_init_from_bytes";
const CONSOLIDATE_PROVE_SYMBOL: &[u8] = b"penumbra_gnark_consolidate_prove";
const CONSOLIDATE_FREE_SYMBOL: &[u8] = b"penumbra_gnark_consolidate_free";
const CONSOLIDATE_SHUTDOWN_SYMBOL: &[u8] = b"penumbra_gnark_consolidate_shutdown";

static CONSOLIDATE_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "consolidate2x1",
    env_artifact_dir: CONSOLIDATE_ENV_ARTIFACT_DIR,
    env_lib: CONSOLIDATE_ENV_LIB,
    env_daemon: CONSOLIDATE_ENV_DAEMON,
    init_symbol: CONSOLIDATE_INIT_SYMBOL,
    init_from_bytes_symbol: CONSOLIDATE_INIT_FROM_BYTES_SYMBOL,
    prove_symbol: CONSOLIDATE_PROVE_SYMBOL,
    free_symbol: CONSOLIDATE_FREE_SYMBOL,
    shutdown_symbol: CONSOLIDATE_SHUTDOWN_SYMBOL,
};

static CONSOLIDATE_FAMILY_CONFIG_2X1: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "consolidate2x1",
    ..CONSOLIDATE_FAMILY_CONFIG
};

static CONSOLIDATE_FAMILY_CONFIG_4X1: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "consolidate4x1",
    ..CONSOLIDATE_FAMILY_CONFIG
};

static CONSOLIDATE_FAMILY_CONFIG_8X1: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "consolidate8x1",
    ..CONSOLIDATE_FAMILY_CONFIG
};

fn consolidate_family_config(family_id: ConsolidateFamilyId) -> &'static GnarkFamilyConfig {
    match family_id {
        ConsolidateFamilyId::TwoByOne => &CONSOLIDATE_FAMILY_CONFIG_2X1,
        ConsolidateFamilyId::FourByOne => &CONSOLIDATE_FAMILY_CONFIG_4X1,
        ConsolidateFamilyId::EightByOne => &CONSOLIDATE_FAMILY_CONFIG_8X1,
        _ => panic!("unknown consolidate family id {}", family_id.get()),
    }
}

pub fn encode_consolidate_witness_v1(
    public: &ConsolidateProofPublic,
    private: &ConsolidateProofPrivate,
) -> Result<Vec<u8>> {
    ConsolidateWitnessV1::from_public_private(public, private)?.encode()
}

pub fn decode_consolidate_witness_v1(bytes: &[u8]) -> Result<ConsolidateWitnessV1> {
    ConsolidateWitnessV1::decode(bytes)
}

pub struct GnarkConsolidateClient {
    family_id: ConsolidateFamilyId,
    transport: GnarkTransport,
    verifying_key: PreparedVerifyingKey<Bls12_377>,
}

enum ConsolidateTransportSource<'a> {
    #[cfg(any(unix, windows))]
    Library {
        lib_path: &'a Path,
        artifact_dir: &'a Path,
    },
    Daemon {
        binary: &'a Path,
        artifact_dir: &'a Path,
    },
    #[cfg(any(unix, windows))]
    Bundled {
        lib_path: &'a Path,
        pk_bytes: &'a [u8],
        pvk: PreparedVerifyingKey<Bls12_377>,
        metadata: &'a [u8],
    },
}

// SAFETY: `GnarkConsolidateClient` is only shared through immutable references. The daemon
// transport serializes mutable process I/O through its internal `Mutex<GnarkDaemonProcess>`,
// and the library transport stores only an owned library handle, immutable function pointers,
// and an opaque prover handle created during initialization. Calls into the native transport
// take `&self`, do not expose borrowed internal state, and rely on the gnark transport API to
// treat the handle as a thread-safe proving context for concurrent read-only use.
unsafe impl Send for GnarkConsolidateClient {}
// SAFETY: See the `Send` impl above; the client contains no Rust-side unsynchronized mutable
// aliasing, and daemon access is protected by a mutex.
unsafe impl Sync for GnarkConsolidateClient {}

impl GnarkConsolidateClient {
    fn load_transport(
        family_id: ConsolidateFamilyId,
        source: ConsolidateTransportSource<'_>,
    ) -> Result<Self> {
        let config = consolidate_family_config(family_id);
        let (transport, verifying_key) = match source {
            #[cfg(any(unix, windows))]
            ConsolidateTransportSource::Library {
                lib_path,
                artifact_dir,
            } => load_library_transport(lib_path, artifact_dir, config)?,
            ConsolidateTransportSource::Daemon {
                binary,
                artifact_dir,
            } => load_daemon_transport(binary, artifact_dir, config)?,
            #[cfg(any(unix, windows))]
            ConsolidateTransportSource::Bundled {
                lib_path,
                pk_bytes,
                pvk,
                metadata,
            } => (
                load_bundled_transport(lib_path, pk_bytes, &pvk, metadata, config)?,
                pvk,
            ),
        };
        Ok(Self {
            family_id,
            transport,
            verifying_key,
        })
    }

    pub fn from_env(family_id: ConsolidateFamilyId) -> Result<Self> {
        let config = consolidate_family_config(family_id);
        let (artifact_dir, lib_path, daemon_path) = load_from_env_paths(config)?;
        match (lib_path, daemon_path) {
            (Some(lib_path), None) => {
                #[cfg(any(unix, windows))]
                {
                    Self::load_transport(
                        family_id,
                        ConsolidateTransportSource::Library {
                            lib_path: &lib_path,
                            artifact_dir: &artifact_dir,
                        },
                    )
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let _ = (&lib_path, &artifact_dir, family_id);
                    bail!("gnark library transport is not supported on this platform")
                }
            }
            (None, Some(daemon_path)) => Self::load_transport(
                family_id,
                ConsolidateTransportSource::Daemon {
                    binary: &daemon_path,
                    artifact_dir: &artifact_dir,
                },
            ),
            (Some(_), Some(_)) => bail!(
                "{} and {} are mutually exclusive",
                config.env_lib,
                config.env_daemon
            ),
            (None, None) => bail!(
                "expected {} or {} to be set",
                config.env_lib,
                config.env_daemon
            ),
        }
    }

    pub fn from_bundled(
        lib_path: &Path,
        pk_bytes: &[u8],
        pvk: PreparedVerifyingKey<Bls12_377>,
        metadata: &[u8],
        family_id: ConsolidateFamilyId,
    ) -> Result<Self> {
        #[cfg(any(unix, windows))]
        {
            Self::load_transport(
                family_id,
                ConsolidateTransportSource::Bundled {
                    lib_path,
                    pk_bytes,
                    pvk,
                    metadata,
                },
            )
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (lib_path, pk_bytes, pvk, metadata, family_id);
            bail!("gnark bundled library loading is not supported on this platform")
        }
    }

    pub fn bundled_lib_path() -> Option<PathBuf> {
        penumbra_sdk_proof_params::GNARK_CONSOLIDATE_BUNDLED_LIBRARY_PATH.map(PathBuf::from)
    }

    #[cfg(any(unix, windows))]
    pub fn auto_lib_path() -> Option<PathBuf> {
        auto_lib_path(CONSOLIDATE_LIB_BASENAME)
    }

    pub fn env_override_configured() -> bool {
        std::env::var_os(CONSOLIDATE_ENV_LIB).is_some()
            || std::env::var_os(CONSOLIDATE_ENV_DAEMON).is_some()
            || std::env::var_os(CONSOLIDATE_ENV_ARTIFACT_DIR).is_some()
    }

    pub fn prove(
        &self,
        public: &ConsolidateProofPublic,
        private: &ConsolidateProofPrivate,
    ) -> Result<ConsolidateProof> {
        let witness_model = ConsolidateWitnessV1::from_public_private(public, private)?;
        let expected_hash = Fq::from_le_bytes_mod_order(&witness_model.claimed_statement_hash);
        let witness = witness_model.encode()?;
        let payload = prove_with_transport(&self.transport, &witness, self.family_id.label())?;
        let (claimed_hash, proof) = translate_consolidate_proof_result(&payload, self.family_id)?;
        if claimed_hash != expected_hash {
            bail!(
                "gnark {} proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}",
                self.family_id.label()
            );
        }
        proof.verify_with_prepared_vk(public, &self.verifying_key)?;
        Ok(proof)
    }
}

impl Drop for GnarkConsolidateClient {
    fn drop(&mut self) {
        shutdown_transport(&mut self.transport);
    }
}

pub fn translate_consolidate_proof_result(
    payload: &[u8],
    family_id: ConsolidateFamilyId,
) -> Result<(Fq, ConsolidateProof)> {
    let (claimed_hash, proof) = parse_binary_proof_result(payload, b"PCPR", family_id.label())?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof = ConsolidateProof::try_from(
        penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1::ZkConsolidateProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{decode_consolidate_witness_v1, encode_consolidate_witness_v1};
    use crate::{
        gnark::ConsolidateWitnessV1, test_proof_helpers::proof_test_helpers, ConsolidateFamilyId,
    };

    #[test]
    fn consolidate_witness_v1_roundtrip() {
        for family_id in ConsolidateFamilyId::ALL {
            let (public, private) =
                proof_test_helpers::build_consolidate_roundtrip_inputs(family_id);
            let encoded = encode_consolidate_witness_v1(&public, &private)
                .expect("encode consolidate witness");
            let decoded =
                decode_consolidate_witness_v1(&encoded).expect("decode consolidate witness");
            let expected = ConsolidateWitnessV1::from_public_private(&public, &private)
                .expect("build consolidate witness");
            assert_eq!(decoded, expected, "family {}", family_id.label());
        }
    }

    #[test]
    fn consolidate_witness_v1_rejects_bad_magic() {
        let (public, private) =
            proof_test_helpers::build_consolidate_roundtrip_inputs(ConsolidateFamilyId::TwoByOne);
        let mut encoded =
            encode_consolidate_witness_v1(&public, &private).expect("encode consolidate witness");
        encoded[0] = b'X';
        assert!(decode_consolidate_witness_v1(&encoded).is_err());
    }

    #[test]
    fn consolidate_witness_v1_rejects_bad_version() {
        let (public, private) =
            proof_test_helpers::build_consolidate_roundtrip_inputs(ConsolidateFamilyId::TwoByOne);
        let mut encoded =
            encode_consolidate_witness_v1(&public, &private).expect("encode consolidate witness");
        encoded[4..8].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode_consolidate_witness_v1(&encoded).is_err());
    }

    #[test]
    fn consolidate_witness_v1_rejects_bad_length() {
        let (public, private) =
            proof_test_helpers::build_consolidate_roundtrip_inputs(ConsolidateFamilyId::TwoByOne);
        let mut encoded =
            encode_consolidate_witness_v1(&public, &private).expect("encode consolidate witness");
        let wrong_len = (encoded.len() as u32).saturating_sub(1);
        encoded[8..12].copy_from_slice(&wrong_len.to_le_bytes());
        assert!(decode_consolidate_witness_v1(&encoded).is_err());
    }
}
