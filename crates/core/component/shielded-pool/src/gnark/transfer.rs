use anyhow::Result;
use ark_serialize::CanonicalSerialize;
use decaf377::Fq;

use crate::{
    gnark::{
        transfer_proof_result::parse_transfer_binary_proof_result,
        transfer_witness::TransferWitness,
    },
    transfer::{TransferProof, TransferProofPrivate, TransferProofPublic},
};

pub fn encode_transfer_witness(
    public: &TransferProofPublic,
    private: &TransferProofPrivate,
) -> Result<Vec<u8>> {
    TransferWitness::from_public_private(public, private)?.encode()
}

pub fn decode_transfer_witness(bytes: &[u8]) -> Result<TransferWitness> {
    TransferWitness::decode(bytes)
}

pub fn translate_transfer_proof_result(payload: &[u8]) -> Result<(Fq, TransferProof)> {
    let (claimed_hash, proof) = parse_transfer_binary_proof_result(payload, "transfer")?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof = TransferProof::try_from(
        shieldd_sdk_proto::shieldd::core::component::shielded_pool::v1::ZkTransferProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn transfer_witness_roundtrip() {
        let (public, private) =
            crate::test_proof_helpers::proof_test_helpers::build_transfer_roundtrip_inputs(true);
        let encoded = encode_transfer_witness(&public, &private).expect("encode transfer witness");
        assert_eq!(
            u32::from_le_bytes(encoded[4..8].try_into().unwrap()) as usize,
            encoded.len()
        );
        let decoded = decode_transfer_witness(&encoded).expect("decode transfer witness");
        let expected = TransferWitness::from_public_private(&public, &private)
            .expect("build transfer witness");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn transfer_hidden_arity_witness_roundtrip() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0000_0054_5831_5831);
        let (public, private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_with_rng(&mut rng, true, false);
        assert!(private.optional_input.is_dummy);

        let encoded =
            encode_transfer_witness(&public, &private).expect("encode hidden-arity witness");
        let decoded =
            decode_transfer_witness(&encoded).expect("decode hidden-arity transfer witness");
        let expected = TransferWitness::from_public_private(&public, &private)
            .expect("build hidden-arity transfer witness");
        assert_eq!(decoded, expected);
    }
}

#[cfg(any(unix, windows))]
mod native {
    use super::*;
    use crate::gnark::transport::{BundledArtifacts, GnarkClient, GnarkFamilyConfig};
    use anyhow::bail;
    const TRANSFER_LIB_BASENAME: &str = "libshieldd_gnark_transfer";
    const TRANSFER_ENV_ARTIFACT_DIR: &str = "SHIELDD_GNARK_TRANSFER_ARTIFACT_DIR";
    const TRANSFER_ENV_LIB: &str = "SHIELDD_GNARK_TRANSFER_LIB";
    const TRANSFER_ENV_DAEMON: &str = "SHIELDD_GNARK_TRANSFER_DAEMON";

    const TRANSFER_INIT_SYMBOL: &[u8] = b"shieldd_gnark_transfer_init";
    const TRANSFER_INIT_FROM_BYTES_SYMBOL: &[u8] = b"shieldd_gnark_transfer_init_from_bytes";
    const TRANSFER_PROVE_SYMBOL: &[u8] = b"shieldd_gnark_transfer_prove";
    const TRANSFER_FREE_SYMBOL: &[u8] = b"shieldd_gnark_transfer_free";
    const TRANSFER_SHUTDOWN_SYMBOL: &[u8] = b"shieldd_gnark_transfer_shutdown";

    pub(crate) static TRANSFER_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
        family: "transfer",
        lib_basename: TRANSFER_LIB_BASENAME,
        bundled_library: shieldd_sdk_proof_params::GNARK_TRANSFER_BUNDLED_LIBRARY_PATH,
        env_artifact_dir: TRANSFER_ENV_ARTIFACT_DIR,
        env_lib: TRANSFER_ENV_LIB,
        env_daemon: TRANSFER_ENV_DAEMON,
        init_symbol: TRANSFER_INIT_SYMBOL,
        init_from_bytes_symbol: TRANSFER_INIT_FROM_BYTES_SYMBOL,
        prove_symbol: TRANSFER_PROVE_SYMBOL,
        free_symbol: TRANSFER_FREE_SYMBOL,
        shutdown_symbol: TRANSFER_SHUTDOWN_SYMBOL,
    };

    pub(crate) struct GnarkTransferClient {
        inner: GnarkClient,
    }

    impl GnarkTransferClient {
        pub(crate) fn load() -> Result<Self> {
            Ok(Self {
                inner: GnarkClient::load(
                    &TRANSFER_FAMILY_CONFIG,
                    BundledArtifacts {
                        proving_key: shieldd_sdk_proof_params::transfer_proving_key_bytes(),
                        verifying_key: shieldd_sdk_proof_params::transfer_verifying_key_json_bytes(
                        ),
                        metadata: shieldd_sdk_proof_params::transfer_circuit_metadata(),
                    },
                )?,
            })
        }

        pub fn prove(
            &self,
            public: &TransferProofPublic,
            private: &TransferProofPrivate,
        ) -> Result<TransferProof> {
            let witness_model = TransferWitness::from_public_private(public, private)?;
            let expected_hash = Fq::from_bytes_checked(&witness_model.claimed_statement_hash)
                .map_err(|_| anyhow::anyhow!("transfer witness statement hash is non-canonical"))?;
            let witness = witness_model.encode()?;
            let payload = self.inner.prove(&witness)?;
            let (claimed_hash, proof) = translate_transfer_proof_result(&payload)?;
            if claimed_hash != expected_hash {
                bail!(
                "gnark transfer proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}",
            );
            }
            proof.verify_with_prepared_vk(public, &self.inner.verifying_key)?;
            Ok(proof)
        }
    }
}

#[cfg(any(unix, windows))]
pub(crate) use native::GnarkTransferClient;
#[cfg(all(any(unix, windows), any(test, feature = "benchmark-helpers")))]
pub(crate) use native::TRANSFER_FAMILY_CONFIG;
