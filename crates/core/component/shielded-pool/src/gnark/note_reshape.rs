use anyhow::Result;
use ark_serialize::CanonicalSerialize;
use decaf377::Fq;

use crate::{
    gnark::{
        note_reshape_witness::NoteReshapeWitness, transfer_proof_result::parse_binary_proof_result,
    },
    note_reshape::{NoteReshapeProof, NoteReshapeProofPrivate, NoteReshapeProofPublic},
    NoteReshapeFamilyId,
};

pub fn encode_note_reshape_witness(
    public: &NoteReshapeProofPublic,
    private: &NoteReshapeProofPrivate,
) -> Result<Vec<u8>> {
    NoteReshapeWitness::from_public_private(public, private)?.encode()
}

pub fn decode_note_reshape_witness(bytes: &[u8]) -> Result<NoteReshapeWitness> {
    NoteReshapeWitness::decode(bytes)
}

pub fn translate_note_reshape_proof_result(
    payload: &[u8],
    family_id: NoteReshapeFamilyId,
) -> Result<(Fq, NoteReshapeProof)> {
    let (claimed_hash, proof) = parse_binary_proof_result(payload, b"PNRP", family_id.label())?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof = NoteReshapeProof::try_from(
        shieldd_sdk_proto::shieldd::core::component::shielded_pool::v1::ZkNoteReshapeProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{decode_note_reshape_witness, encode_note_reshape_witness};
    use crate::{
        gnark::NoteReshapeWitness, test_proof_helpers::proof_test_helpers, NoteReshapeFamilyId,
    };

    #[test]
    fn note_reshape_witness_roundtrip() {
        for family_id in NoteReshapeFamilyId::ALL {
            let (public, private) =
                proof_test_helpers::build_note_reshape_roundtrip_inputs(family_id);
            let encoded = encode_note_reshape_witness(&public, &private)
                .expect("encode note_reshape witness");
            let decoded =
                decode_note_reshape_witness(&encoded).expect("decode note_reshape witness");
            let expected = NoteReshapeWitness::from_public_private(&public, &private)
                .expect("build note_reshape witness");
            assert_eq!(decoded, expected, "family {}", family_id.label());
        }
    }

    fn corrupt() -> Vec<u8> {
        let (public, private) = proof_test_helpers::build_note_reshape_roundtrip_inputs(
            NoteReshapeFamilyId::EightByOne,
        );
        encode_note_reshape_witness(&public, &private).expect("encode note_reshape witness")
    }

    #[test]
    fn note_reshape_witness_rejects_bad_magic() {
        let mut encoded = corrupt();
        encoded[0] = b'X';
        assert!(decode_note_reshape_witness(&encoded).is_err());
    }

    #[test]
    fn note_reshape_witness_rejects_bad_length() {
        let mut encoded = corrupt();
        let wrong_len = (encoded.len() as u32).saturating_sub(1);
        encoded[4..8].copy_from_slice(&wrong_len.to_le_bytes());
        assert!(decode_note_reshape_witness(&encoded).is_err());
    }
}

#[cfg(any(unix, windows))]
mod native {
    use super::*;
    use crate::gnark::transport::{BundledArtifacts, GnarkClient, GnarkFamilyConfig};
    use anyhow::bail;
    const NOTE_RESHAPE_LIB_BASENAME: &str = "libshieldd_gnark_note_reshape";
    const NOTE_RESHAPE_ENV_ARTIFACT_DIR: &str = "SHIELDD_GNARK_NOTE_RESHAPE_ARTIFACT_DIR";
    const NOTE_RESHAPE_ENV_LIB: &str = "SHIELDD_GNARK_NOTE_RESHAPE_LIB";
    const NOTE_RESHAPE_ENV_DAEMON: &str = "SHIELDD_GNARK_NOTE_RESHAPE_DAEMON";

    const NOTE_RESHAPE_INIT_SYMBOL: &[u8] = b"shieldd_gnark_note_reshape_init";
    const NOTE_RESHAPE_INIT_FROM_BYTES_SYMBOL: &[u8] =
        b"shieldd_gnark_note_reshape_init_from_bytes";
    const NOTE_RESHAPE_PROVE_SYMBOL: &[u8] = b"shieldd_gnark_note_reshape_prove";
    const NOTE_RESHAPE_FREE_SYMBOL: &[u8] = b"shieldd_gnark_note_reshape_free";
    const NOTE_RESHAPE_SHUTDOWN_SYMBOL: &[u8] = b"shieldd_gnark_note_reshape_shutdown";

    static NOTE_RESHAPE_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
        family: "note_reshape8x1",
        lib_basename: NOTE_RESHAPE_LIB_BASENAME,
        bundled_library: shieldd_sdk_proof_params::GNARK_NOTE_RESHAPE_BUNDLED_LIBRARY_PATH,
        env_artifact_dir: NOTE_RESHAPE_ENV_ARTIFACT_DIR,
        env_lib: NOTE_RESHAPE_ENV_LIB,
        env_daemon: NOTE_RESHAPE_ENV_DAEMON,
        init_symbol: NOTE_RESHAPE_INIT_SYMBOL,
        init_from_bytes_symbol: NOTE_RESHAPE_INIT_FROM_BYTES_SYMBOL,
        prove_symbol: NOTE_RESHAPE_PROVE_SYMBOL,
        free_symbol: NOTE_RESHAPE_FREE_SYMBOL,
        shutdown_symbol: NOTE_RESHAPE_SHUTDOWN_SYMBOL,
    };

    static NOTE_RESHAPE_FAMILY_CONFIG_1X8: GnarkFamilyConfig = GnarkFamilyConfig {
        family: "note_reshape1x8",
        ..NOTE_RESHAPE_FAMILY_CONFIG
    };

    pub(crate) fn note_reshape_family_config(
        family_id: NoteReshapeFamilyId,
    ) -> &'static GnarkFamilyConfig {
        match family_id {
            NoteReshapeFamilyId::OneByEight => &NOTE_RESHAPE_FAMILY_CONFIG_1X8,
            NoteReshapeFamilyId::EightByOne => &NOTE_RESHAPE_FAMILY_CONFIG,
            _ => panic!("unknown note_reshape family id {}", family_id.get()),
        }
    }

    pub(crate) struct GnarkNoteReshapeClient {
        family_id: NoteReshapeFamilyId,
        inner: GnarkClient,
    }

    impl GnarkNoteReshapeClient {
        pub(crate) fn load(family_id: NoteReshapeFamilyId) -> Result<Self> {
            Ok(Self {
                family_id,
                inner: GnarkClient::load(
                    note_reshape_family_config(family_id),
                    BundledArtifacts {
                        proving_key: family_id.proving_key_bytes(),
                        verifying_key: family_id.verifying_key_json_bytes(),
                        metadata: family_id.circuit_metadata_bytes(),
                    },
                )?,
            })
        }

        pub fn prove(
            &self,
            public: &NoteReshapeProofPublic,
            private: &NoteReshapeProofPrivate,
        ) -> Result<NoteReshapeProof> {
            let witness_model = NoteReshapeWitness::from_public_private(public, private)?;
            let expected_hash = Fq::from_bytes_checked(&witness_model.claimed_statement_hash)
                .map_err(|_| {
                    anyhow::anyhow!(
                        "{} witness statement hash is non-canonical",
                        self.family_id.label()
                    )
                })?;
            let witness = witness_model.encode()?;
            let payload = self.inner.prove(&witness)?;
            let (claimed_hash, proof) =
                translate_note_reshape_proof_result(&payload, self.family_id)?;
            if claimed_hash != expected_hash {
                bail!(
                "gnark {} proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}",
                self.family_id.label()
            );
            }
            proof.verify_with_prepared_vk(public, &self.inner.verifying_key)?;
            Ok(proof)
        }
    }
}

#[cfg(all(any(unix, windows), any(test, feature = "benchmark-helpers")))]
pub(crate) use native::note_reshape_family_config;
#[cfg(any(unix, windows))]
pub(crate) use native::GnarkNoteReshapeClient;
