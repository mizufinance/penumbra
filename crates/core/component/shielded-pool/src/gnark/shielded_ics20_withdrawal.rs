use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use ark_groth16::PreparedVerifyingKey;
use ark_serialize::CanonicalSerialize;
use decaf377::{Bls12_377, Fq};

#[cfg(any(unix, windows))]
use crate::gnark::transport::{auto_lib_path, load_bundled_transport, load_library_transport};

use crate::{
    gnark::{
        shielded_ics20_withdrawal_witness::ShieldedIcs20WithdrawalWitnessV1,
        transfer_proof_result::parse_binary_proof_result,
        transport::{
            load_daemon_transport, load_from_env_paths, prove_with_transport, shutdown_transport,
            GnarkFamilyConfig, GnarkTransport,
        },
    },
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic,
    },
    ShieldedIcs20WithdrawalFamilyId,
};

const SHIELDED_ICS20_WITHDRAWAL_LIB_BASENAME: &str = "libpenumbra_gnark_shielded_ics20_withdrawal";
const SHIELDED_ICS20_WITHDRAWAL_ENV_ARTIFACT_DIR: &str =
    "PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR";
const SHIELDED_ICS20_WITHDRAWAL_ENV_LIB: &str = "PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB";
const SHIELDED_ICS20_WITHDRAWAL_ENV_DAEMON: &str =
    "PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_DAEMON";

const SHIELDED_ICS20_WITHDRAWAL_INIT_SYMBOL: &[u8] =
    b"penumbra_gnark_shielded_ics20_withdrawal_init";
const SHIELDED_ICS20_WITHDRAWAL_INIT_FROM_BYTES_SYMBOL: &[u8] =
    b"penumbra_gnark_shielded_ics20_withdrawal_init_from_bytes";
const SHIELDED_ICS20_WITHDRAWAL_PROVE_SYMBOL: &[u8] =
    b"penumbra_gnark_shielded_ics20_withdrawal_prove";
const SHIELDED_ICS20_WITHDRAWAL_FREE_SYMBOL: &[u8] =
    b"penumbra_gnark_shielded_ics20_withdrawal_free";
const SHIELDED_ICS20_WITHDRAWAL_SHUTDOWN_SYMBOL: &[u8] =
    b"penumbra_gnark_shielded_ics20_withdrawal_shutdown";

static SHIELDED_ICS20_WITHDRAWAL_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
    family: "shielded_ics20_withdrawal",
    env_artifact_dir: SHIELDED_ICS20_WITHDRAWAL_ENV_ARTIFACT_DIR,
    env_lib: SHIELDED_ICS20_WITHDRAWAL_ENV_LIB,
    env_daemon: SHIELDED_ICS20_WITHDRAWAL_ENV_DAEMON,
    init_symbol: SHIELDED_ICS20_WITHDRAWAL_INIT_SYMBOL,
    init_from_bytes_symbol: SHIELDED_ICS20_WITHDRAWAL_INIT_FROM_BYTES_SYMBOL,
    prove_symbol: SHIELDED_ICS20_WITHDRAWAL_PROVE_SYMBOL,
    free_symbol: SHIELDED_ICS20_WITHDRAWAL_FREE_SYMBOL,
    shutdown_symbol: SHIELDED_ICS20_WITHDRAWAL_SHUTDOWN_SYMBOL,
};

fn shielded_ics20_withdrawal_family_config(
    family_id: ShieldedIcs20WithdrawalFamilyId,
) -> &'static GnarkFamilyConfig {
    match family_id {
        ShieldedIcs20WithdrawalFamilyId::Canonical => &SHIELDED_ICS20_WITHDRAWAL_FAMILY_CONFIG,
        _ => panic!(
            "unknown shielded ICS-20 withdrawal family id {}",
            family_id.get()
        ),
    }
}

pub fn encode_shielded_ics20_withdrawal_witness_v1(
    public: &ShieldedIcs20WithdrawalProofPublic,
    private: &ShieldedIcs20WithdrawalProofPrivate,
) -> Result<Vec<u8>> {
    ShieldedIcs20WithdrawalWitnessV1::from_public_private(public, private)?.encode()
}

pub fn decode_shielded_ics20_withdrawal_witness_v1(
    bytes: &[u8],
) -> Result<ShieldedIcs20WithdrawalWitnessV1> {
    ShieldedIcs20WithdrawalWitnessV1::decode(bytes)
}

pub struct GnarkShieldedIcs20WithdrawalClient {
    family_id: ShieldedIcs20WithdrawalFamilyId,
    transport: GnarkTransport,
    verifying_key: PreparedVerifyingKey<Bls12_377>,
}

enum ShieldedIcs20WithdrawalTransportSource<'a> {
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

unsafe impl Send for GnarkShieldedIcs20WithdrawalClient {}
unsafe impl Sync for GnarkShieldedIcs20WithdrawalClient {}

impl GnarkShieldedIcs20WithdrawalClient {
    fn load_transport(
        family_id: ShieldedIcs20WithdrawalFamilyId,
        source: ShieldedIcs20WithdrawalTransportSource<'_>,
    ) -> Result<Self> {
        let config = shielded_ics20_withdrawal_family_config(family_id);
        let (transport, verifying_key) = match source {
            #[cfg(any(unix, windows))]
            ShieldedIcs20WithdrawalTransportSource::Library {
                lib_path,
                artifact_dir,
            } => load_library_transport(lib_path, artifact_dir, config)?,
            ShieldedIcs20WithdrawalTransportSource::Daemon {
                binary,
                artifact_dir,
            } => load_daemon_transport(binary, artifact_dir, config)?,
            #[cfg(any(unix, windows))]
            ShieldedIcs20WithdrawalTransportSource::Bundled {
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

    pub fn from_env(family_id: ShieldedIcs20WithdrawalFamilyId) -> Result<Self> {
        let config = shielded_ics20_withdrawal_family_config(family_id);
        let (artifact_dir, lib_path, daemon_path) = load_from_env_paths(config)?;
        match (lib_path, daemon_path) {
            (Some(lib_path), None) => {
                #[cfg(any(unix, windows))]
                {
                    Self::load_transport(
                        family_id,
                        ShieldedIcs20WithdrawalTransportSource::Library {
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
                ShieldedIcs20WithdrawalTransportSource::Daemon {
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
        family_id: ShieldedIcs20WithdrawalFamilyId,
    ) -> Result<Self> {
        #[cfg(any(unix, windows))]
        {
            Self::load_transport(
                family_id,
                ShieldedIcs20WithdrawalTransportSource::Bundled {
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
        penumbra_sdk_proof_params::GNARK_SHIELDED_ICS20_WITHDRAWAL_BUNDLED_LIBRARY_PATH
            .map(PathBuf::from)
    }

    #[cfg(any(unix, windows))]
    pub fn auto_lib_path() -> Option<PathBuf> {
        auto_lib_path(SHIELDED_ICS20_WITHDRAWAL_LIB_BASENAME)
    }

    pub fn env_override_configured() -> bool {
        std::env::var_os(SHIELDED_ICS20_WITHDRAWAL_ENV_LIB).is_some()
            || std::env::var_os(SHIELDED_ICS20_WITHDRAWAL_ENV_DAEMON).is_some()
            || std::env::var_os(SHIELDED_ICS20_WITHDRAWAL_ENV_ARTIFACT_DIR).is_some()
    }

    pub fn bundled_transport_available(family_id: ShieldedIcs20WithdrawalFamilyId) -> bool {
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
        public: &ShieldedIcs20WithdrawalProofPublic,
        private: &ShieldedIcs20WithdrawalProofPrivate,
    ) -> Result<ShieldedIcs20WithdrawalProof> {
        let witness_model = ShieldedIcs20WithdrawalWitnessV1::from_public_private(public, private)?;
        let expected_hash = Fq::from_le_bytes_mod_order(&witness_model.claimed_statement_hash);
        let witness = witness_model.encode()?;
        let payload = prove_with_transport(&self.transport, &witness, self.family_id.label())?;
        let (claimed_hash, proof) =
            translate_shielded_ics20_withdrawal_proof_result(&payload, self.family_id)?;
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

impl Drop for GnarkShieldedIcs20WithdrawalClient {
    fn drop(&mut self) {
        shutdown_transport(&mut self.transport);
    }
}

pub fn translate_shielded_ics20_withdrawal_proof_result(
    payload: &[u8],
    family_id: ShieldedIcs20WithdrawalFamilyId,
) -> Result<(Fq, ShieldedIcs20WithdrawalProof)> {
    let (claimed_hash, proof) = parse_binary_proof_result(payload, b"PIPR", family_id.label())?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof =
        ShieldedIcs20WithdrawalProof::try_from(
            penumbra_sdk_proto::penumbra::core::component::shielded_pool::v1::ZkShieldedIcs20WithdrawalProof {
                inner: proof_bytes,
            },
        )?;
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{
        decode_shielded_ics20_withdrawal_witness_v1, encode_shielded_ics20_withdrawal_witness_v1,
    };
    use crate::{
        gnark::ShieldedIcs20WithdrawalWitnessV1, test_proof_helpers::proof_test_helpers,
        ShieldedIcs20WithdrawalFamilyId,
    };

    #[test]
    fn shielded_ics20_withdrawal_witness_v1_roundtrip() {
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        let encoded = encode_shielded_ics20_withdrawal_witness_v1(&public, &private)
            .expect("encode shielded ICS-20 withdrawal witness");
        let decoded = decode_shielded_ics20_withdrawal_witness_v1(&encoded)
            .expect("decode shielded ICS-20 withdrawal witness");
        let expected = ShieldedIcs20WithdrawalWitnessV1::from_public_private(&public, &private)
            .expect("build shielded ICS-20 withdrawal witness");
        assert_eq!(decoded, expected);
    }
}
