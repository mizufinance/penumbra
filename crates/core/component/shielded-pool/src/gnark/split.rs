use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use ark_groth16::PreparedVerifyingKey;
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Fq};

#[cfg(any(unix, windows))]
use crate::gnark::transport::{auto_lib_path, load_bundled_transport, load_library_transport};

use crate::{
    gnark::{
        note_reshape_witness::SplitWitnessV1,
        transfer_proof_result::parse_binary_proof_result,
        transport::{
            load_daemon_transport, load_from_env_paths, prove_with_transport, shutdown_transport,
            GnarkFamilyConfig, GnarkTransport,
        },
    },
    split::{SplitProofPrivate, SplitProofPublic},
    SplitFamilyId,
};

const SPLIT_LIB_BASENAME: &str = "libpenumbra_gnark_split";
const SPLIT_ENV_ARTIFACT_DIR: &str = "PENUMBRA_GNARK_SPLIT_ARTIFACT_DIR";
const SPLIT_ENV_LIB: &str = "PENUMBRA_GNARK_SPLIT_LIB";
const SPLIT_ENV_DAEMON: &str = "PENUMBRA_GNARK_SPLIT_DAEMON";

const SPLIT_INIT_SYMBOL: &[u8] = b"penumbra_gnark_split_init";
const SPLIT_INIT_FROM_BYTES_SYMBOL: &[u8] = b"penumbra_gnark_split_init_from_bytes";
const SPLIT_PROVE_SYMBOL: &[u8] = b"penumbra_gnark_split_prove";
const SPLIT_FREE_SYMBOL: &[u8] = b"penumbra_gnark_split_free";
const SPLIT_SHUTDOWN_SYMBOL: &[u8] = b"penumbra_gnark_split_shutdown";

static SPLIT_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "split1x4",
    env_artifact_dir: SPLIT_ENV_ARTIFACT_DIR,
    env_lib: SPLIT_ENV_LIB,
    env_daemon: SPLIT_ENV_DAEMON,
    init_symbol: SPLIT_INIT_SYMBOL,
    init_from_bytes_symbol: SPLIT_INIT_FROM_BYTES_SYMBOL,
    prove_symbol: SPLIT_PROVE_SYMBOL,
    free_symbol: SPLIT_FREE_SYMBOL,
    shutdown_symbol: SPLIT_SHUTDOWN_SYMBOL,
};

static SPLIT_FAMILY_CONFIG_1X4: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "split1x4",
    ..SPLIT_FAMILY_CONFIG
};

static SPLIT_FAMILY_CONFIG_1X8: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "split1x8",
    ..SPLIT_FAMILY_CONFIG
};

fn split_family_config(family_id: SplitFamilyId) -> &'static GnarkFamilyConfig {
    match family_id {
        SplitFamilyId::OneByFour => &SPLIT_FAMILY_CONFIG_1X4,
        SplitFamilyId::OneByEight => &SPLIT_FAMILY_CONFIG_1X8,
        _ => panic!("unknown split family id {}", family_id.get()),
    }
}

pub fn encode_split_witness_v1(
    public: &SplitProofPublic,
    private: &SplitProofPrivate,
) -> Result<Vec<u8>> {
    SplitWitnessV1::from_public_private(public, private)?.encode()
}

pub fn decode_split_witness_v1(bytes: &[u8]) -> Result<SplitWitnessV1> {
    SplitWitnessV1::decode(bytes)
}

pub struct GnarkSplitClient {
    family_id: SplitFamilyId,
    transport: GnarkTransport,
    verifying_key: PreparedVerifyingKey<Bls12_377>,
}

enum SplitTransportSource<'a> {
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

// SAFETY: `GnarkSplitClient` is only shared through immutable references. The daemon
// transport serializes mutable process I/O through its internal `Mutex<GnarkDaemonProcess>`,
// and the library transport stores only an owned library handle, immutable function pointers,
// and an opaque prover handle created during initialization. Calls into the native transport
// take `&self`, do not expose borrowed internal state, and rely on the gnark transport API to
// treat the handle as a thread-safe proving context for concurrent read-only use.
unsafe impl Send for GnarkSplitClient {}
// SAFETY: See the `Send` impl above; the client contains no Rust-side unsynchronized mutable
// aliasing, and daemon access is protected by a mutex.
unsafe impl Sync for GnarkSplitClient {}

impl GnarkSplitClient {
    fn load_transport(family_id: SplitFamilyId, source: SplitTransportSource<'_>) -> Result<Self> {
        let config = split_family_config(family_id);
        let (transport, verifying_key) = match source {
            #[cfg(any(unix, windows))]
            SplitTransportSource::Library {
                lib_path,
                artifact_dir,
            } => load_library_transport(lib_path, artifact_dir, config)?,
            SplitTransportSource::Daemon {
                binary,
                artifact_dir,
            } => load_daemon_transport(binary, artifact_dir, config)?,
            #[cfg(any(unix, windows))]
            SplitTransportSource::Bundled {
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

    pub fn from_env(family_id: SplitFamilyId) -> Result<Self> {
        let config = split_family_config(family_id);
        let (artifact_dir, lib_path, daemon_path) = load_from_env_paths(config)?;
        match (lib_path, daemon_path) {
            (Some(lib_path), None) => {
                #[cfg(any(unix, windows))]
                {
                    Self::load_transport(
                        family_id,
                        SplitTransportSource::Library {
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
                SplitTransportSource::Daemon {
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
        family_id: SplitFamilyId,
    ) -> Result<Self> {
        #[cfg(any(unix, windows))]
        {
            Self::load_transport(
                family_id,
                SplitTransportSource::Bundled {
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
        penumbra_sdk_proof_params::GNARK_SPLIT_BUNDLED_LIBRARY_PATH.map(PathBuf::from)
    }

    #[cfg(any(unix, windows))]
    pub fn auto_lib_path() -> Option<PathBuf> {
        auto_lib_path(SPLIT_LIB_BASENAME)
    }

    pub fn env_override_configured() -> bool {
        std::env::var_os(SPLIT_ENV_LIB).is_some()
            || std::env::var_os(SPLIT_ENV_DAEMON).is_some()
            || std::env::var_os(SPLIT_ENV_ARTIFACT_DIR).is_some()
    }

    pub fn prove(
        &self,
        public: &SplitProofPublic,
        private: &SplitProofPrivate,
    ) -> Result<crate::split::SplitProof> {
        let witness_model = SplitWitnessV1::from_public_private(public, private)?;
        let expected_hash = Fq::from_le_bytes_mod_order(&witness_model.claimed_statement_hash);
        let witness = witness_model.encode()?;
        let payload = prove_with_transport(&self.transport, &witness, self.family_id.label())?;
        let (claimed_hash, proof) = translate_split_proof_result(&payload, self.family_id)?;
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

impl Drop for GnarkSplitClient {
    fn drop(&mut self) {
        shutdown_transport(&mut self.transport);
    }
}

pub fn translate_split_proof_result(
    payload: &[u8],
    family_id: SplitFamilyId,
) -> Result<(Fq, crate::split::SplitProof)> {
    let (claimed_hash, proof) = parse_binary_proof_result(payload, b"PLPR", family_id.label())?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof = crate::split::SplitProof::try_from(
        penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1::ZkSplitProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{decode_split_witness_v1, encode_split_witness_v1};
    use crate::{gnark::SplitWitnessV1, test_proof_helpers::proof_test_helpers, SplitFamilyId};

    #[test]
    fn split_witness_v1_roundtrip() {
        for family_id in SplitFamilyId::ALL {
            let (public, private) = proof_test_helpers::build_split_roundtrip_inputs(family_id);
            let encoded = encode_split_witness_v1(&public, &private).expect("encode split witness");
            let decoded = decode_split_witness_v1(&encoded).expect("decode split witness");
            let expected = SplitWitnessV1::from_public_private(&public, &private)
                .expect("build split witness");
            assert_eq!(decoded, expected, "family {}", family_id.label());
        }
    }

    fn corrupt() -> Vec<u8> {
        let (public, private) =
            proof_test_helpers::build_split_roundtrip_inputs(SplitFamilyId::OneByFour);
        encode_split_witness_v1(&public, &private).expect("encode split witness")
    }

    #[test]
    fn split_witness_v1_rejects_bad_magic() {
        let mut encoded = corrupt();
        encoded[0] = b'X';
        assert!(decode_split_witness_v1(&encoded).is_err());
    }

    #[test]
    fn split_witness_v1_rejects_bad_version() {
        let mut encoded = corrupt();
        encoded[4..8].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode_split_witness_v1(&encoded).is_err());
    }

    #[test]
    fn split_witness_v1_rejects_bad_length() {
        let mut encoded = corrupt();
        let wrong_len = (encoded.len() as u32).saturating_sub(1);
        encoded[8..12].copy_from_slice(&wrong_len.to_le_bytes());
        assert!(decode_split_witness_v1(&encoded).is_err());
    }
}
