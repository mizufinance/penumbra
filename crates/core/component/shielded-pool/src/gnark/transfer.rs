use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use ark_groth16::PreparedVerifyingKey;
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Fq};

#[cfg(any(unix, windows))]
use crate::gnark::transport::{auto_lib_path, load_bundled_transport, load_library_transport};
use crate::{
    gnark::{
        transfer_proof_result::parse_transfer_binary_proof_result,
        transfer_witness::TransferWitnessV1,
        transport::{
            load_daemon_transport, load_from_env_paths, prove_with_transport, shutdown_transport,
            GnarkFamilyConfig, GnarkTransport,
        },
    },
    transfer::{TransferProof, TransferProofPrivate, TransferProofPublic},
    TransferFamilyId,
};

const TRANSFER_LIB_BASENAME: &str = "libpenumbra_gnark_transfer";
const TRANSFER_ENV_ARTIFACT_DIR: &str = "PENUMBRA_GNARK_TRANSFER_ARTIFACT_DIR";
const TRANSFER_ENV_LIB: &str = "PENUMBRA_GNARK_TRANSFER_LIB";
const TRANSFER_ENV_DAEMON: &str = "PENUMBRA_GNARK_TRANSFER_DAEMON";

const TRANSFER_INIT_SYMBOL: &[u8] = b"penumbra_gnark_transfer_init";
const TRANSFER_INIT_FROM_BYTES_SYMBOL: &[u8] = b"penumbra_gnark_transfer_init_from_bytes";
const TRANSFER_PROVE_SYMBOL: &[u8] = b"penumbra_gnark_transfer_prove";
const TRANSFER_FREE_SYMBOL: &[u8] = b"penumbra_gnark_transfer_free";
const TRANSFER_SHUTDOWN_SYMBOL: &[u8] = b"penumbra_gnark_transfer_shutdown";

static TRANSFER_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "transfer1x1",
    lib_basename: TRANSFER_LIB_BASENAME,
    env_artifact_dir: TRANSFER_ENV_ARTIFACT_DIR,
    env_lib: TRANSFER_ENV_LIB,
    env_daemon: TRANSFER_ENV_DAEMON,
    init_symbol: TRANSFER_INIT_SYMBOL,
    init_from_bytes_symbol: TRANSFER_INIT_FROM_BYTES_SYMBOL,
    prove_symbol: TRANSFER_PROVE_SYMBOL,
    free_symbol: TRANSFER_FREE_SYMBOL,
    shutdown_symbol: TRANSFER_SHUTDOWN_SYMBOL,
};

static TRANSFER_FAMILY_CONFIG_1X1: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "transfer1x1",
    ..TRANSFER_FAMILY_CONFIG
};

static TRANSFER_FAMILY_CONFIG_1X2: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "transfer1x2",
    ..TRANSFER_FAMILY_CONFIG
};

static TRANSFER_FAMILY_CONFIG_2X1: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "transfer2x1",
    ..TRANSFER_FAMILY_CONFIG
};

static TRANSFER_FAMILY_CONFIG_2X2: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "transfer2x2",
    ..TRANSFER_FAMILY_CONFIG
};

fn transfer_family_config(family_id: TransferFamilyId) -> &'static GnarkFamilyConfig {
    match family_id {
        TransferFamilyId::OneByOne => &TRANSFER_FAMILY_CONFIG_1X1,
        TransferFamilyId::OneByTwo => &TRANSFER_FAMILY_CONFIG_1X2,
        TransferFamilyId::TwoByOne => &TRANSFER_FAMILY_CONFIG_2X1,
        TransferFamilyId::TwoByTwo => &TRANSFER_FAMILY_CONFIG_2X2,
        _ => panic!("unknown transfer family id {}", family_id.get()),
    }
}

pub fn encode_transfer_witness_v1(
    public: &TransferProofPublic,
    private: &TransferProofPrivate,
) -> Result<Vec<u8>> {
    TransferWitnessV1::from_public_private(public, private)?.encode()
}

pub fn decode_transfer_witness_v1(bytes: &[u8]) -> Result<TransferWitnessV1> {
    TransferWitnessV1::decode(bytes)
}

pub struct GnarkTransferClient {
    family_id: TransferFamilyId,
    transport: GnarkTransport,
}

enum TransferTransportSource<'a> {
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

// SAFETY: `GnarkTransferClient` is only shared through immutable references. The daemon
// transport serializes mutable process I/O through its internal `Mutex<GnarkDaemonProcess>`,
// and the library transport stores only an owned library handle, immutable function pointers,
// and an opaque prover handle created during initialization. Calls into the native transport
// take `&self`, do not expose borrowed internal state, and rely on the gnark transport API to
// treat the handle as a thread-safe proving context for concurrent read-only use.
unsafe impl Send for GnarkTransferClient {}
// SAFETY: See the `Send` impl above; the client contains no Rust-side unsynchronized mutable
// aliasing, and daemon access is protected by a mutex.
unsafe impl Sync for GnarkTransferClient {}

impl GnarkTransferClient {
    fn load_transport(
        family_id: TransferFamilyId,
        source: TransferTransportSource<'_>,
    ) -> Result<Self> {
        let config = transfer_family_config(family_id);
        let transport = match source {
            #[cfg(any(unix, windows))]
            TransferTransportSource::Library {
                lib_path,
                artifact_dir,
            } => {
                let (transport, _pvk) = load_library_transport(lib_path, artifact_dir, &config)?;
                transport
            }
            TransferTransportSource::Daemon {
                binary,
                artifact_dir,
            } => {
                let (transport, _pvk) = load_daemon_transport(binary, artifact_dir, &config)?;
                transport
            }
            #[cfg(any(unix, windows))]
            TransferTransportSource::Bundled {
                lib_path,
                pk_bytes,
                pvk,
                metadata,
            } => load_bundled_transport(lib_path, pk_bytes, &pvk, metadata, &config)?,
        };
        Ok(Self {
            family_id,
            transport,
        })
    }

    pub fn from_env(family_id: TransferFamilyId) -> Result<Self> {
        let config = transfer_family_config(family_id);
        let (artifact_dir, lib_path, daemon_path) = load_from_env_paths(config)?;
        match (lib_path, daemon_path) {
            (Some(lib_path), None) => {
                #[cfg(any(unix, windows))]
                {
                    Self::load_transport(
                        family_id,
                        TransferTransportSource::Library {
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
                TransferTransportSource::Daemon {
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
        family_id: TransferFamilyId,
    ) -> Result<Self> {
        #[cfg(any(unix, windows))]
        {
            Self::load_transport(
                family_id,
                TransferTransportSource::Bundled {
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
        penumbra_sdk_proof_params::GNARK_TRANSFER_BUNDLED_LIBRARY_PATH.map(PathBuf::from)
    }

    #[cfg(any(unix, windows))]
    pub fn auto_lib_path() -> Option<PathBuf> {
        auto_lib_path(TRANSFER_LIB_BASENAME)
    }

    pub fn env_override_configured() -> bool {
        std::env::var_os(TRANSFER_ENV_LIB).is_some()
            || std::env::var_os(TRANSFER_ENV_DAEMON).is_some()
            || std::env::var_os(TRANSFER_ENV_ARTIFACT_DIR).is_some()
    }

    pub fn bundled_transport_available(family_id: TransferFamilyId) -> bool {
        let lib_path = Self::bundled_lib_path().or_else(|| {
            #[cfg(any(unix, windows))]
            {
                Self::auto_lib_path()
            }
            #[cfg(not(any(unix, windows)))]
            {
                None
            }
        });
        lib_path.is_some() && !family_id.proving_key_bytes().is_empty()
    }

    pub fn prove(
        &self,
        public: &TransferProofPublic,
        private: &TransferProofPrivate,
    ) -> Result<TransferProof> {
        let witness_model = TransferWitnessV1::from_public_private(public, private)?;
        let expected_hash = Fq::from_le_bytes_mod_order(&witness_model.claimed_statement_hash);
        let witness = witness_model.encode()?;
        let payload = prove_with_transport(&self.transport, &witness, self.family_id.label())?;
        let (claimed_hash, proof) = translate_transfer_proof_result(&payload, self.family_id)?;
        if claimed_hash != expected_hash {
            bail!(
                "gnark {} proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}",
                self.family_id.label()
            );
        }
        proof.verify(public)?;
        Ok(proof)
    }
}

impl Drop for GnarkTransferClient {
    fn drop(&mut self) {
        shutdown_transport(&mut self.transport);
    }
}

pub fn translate_transfer_proof_result(
    payload: &[u8],
    family_id: TransferFamilyId,
) -> Result<(Fq, TransferProof)> {
    let (claimed_hash, proof) = parse_transfer_binary_proof_result(payload, family_id.label())?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof = TransferProof::try_from(
        penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1::ZkTransferProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, proof))
}
