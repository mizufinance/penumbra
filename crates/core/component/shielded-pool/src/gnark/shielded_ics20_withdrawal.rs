use anyhow::Result;
use ark_serialize::CanonicalSerialize;
use decaf377::Fq;

use crate::{
    gnark::{
        shielded_ics20_withdrawal_witness::ShieldedIcs20WithdrawalWitnessV12,
        transfer_proof_result::parse_binary_proof_result,
    },
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic,
    },
    ShieldedIcs20WithdrawalFamilyId,
};

pub fn encode_shielded_ics20_withdrawal_witness_v12(
    public: &ShieldedIcs20WithdrawalProofPublic,
    private: &ShieldedIcs20WithdrawalProofPrivate,
) -> Result<Vec<u8>> {
    ShieldedIcs20WithdrawalWitnessV12::from_public_private(public, private)?.encode()
}

pub fn decode_shielded_ics20_withdrawal_witness_v12(
    bytes: &[u8],
) -> Result<ShieldedIcs20WithdrawalWitnessV12> {
    ShieldedIcs20WithdrawalWitnessV12::decode(bytes)
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
            shieldd_sdk_proto::shieldd::core::component::shielded_pool::v1::ZkShieldedIcs20WithdrawalProof {
                inner: proof_bytes,
            },
        )?;
    Ok((claimed_hash, proof))
}

#[cfg(test)]
mod tests {
    use super::{
        decode_shielded_ics20_withdrawal_witness_v12, encode_shielded_ics20_withdrawal_witness_v12,
    };
    use crate::{
        gnark::ShieldedIcs20WithdrawalWitnessV12, test_proof_helpers::proof_test_helpers,
        ShieldedIcs20WithdrawalFamilyId,
    };
    use decaf377::{Fq, Fr};
    use shieldd_sdk_asset::Balance;
    use shieldd_sdk_tct::StateCommitment;

    #[test]
    fn shielded_ics20_withdrawal_witness_v12_roundtrip() {
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        let encoded = encode_shielded_ics20_withdrawal_witness_v12(&public, &private)
            .expect("encode shielded ICS-20 withdrawal witness");
        let decoded = decode_shielded_ics20_withdrawal_witness_v12(&encoded)
            .expect("decode shielded ICS-20 withdrawal witness");
        let expected = ShieldedIcs20WithdrawalWitnessV12::from_public_private(&public, &private)
            .expect("build shielded ICS-20 withdrawal witness");
        assert_eq!(decoded, expected);

        let leaf = &decoded.asset_indexed_leaf;
        let recomposed = StateCommitment(poseidon377::hash_5(
            &shieldd_sdk_compliance::IMT_LEAF_DOMAIN_SEP,
            (
                Fq::from_bytes_checked(&leaf.value).expect("canonical leaf value"),
                Fq::from(leaf.next_index),
                Fq::from_bytes_checked(&leaf.next_value).expect("canonical next value"),
                Fq::from_bytes_checked(&leaf.params_hash).expect("canonical params hash"),
                Fq::from_bytes_checked(&leaf.ring_hash).expect("canonical ring hash"),
            ),
        ));
        assert_eq!(
            recomposed,
            private.asset_indexed_leaf.commit(),
            "compact leaf view must recompose the canonical native commitment"
        );
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_v12_rejects_unsupported_version() {
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        let mut encoded = encode_shielded_ics20_withdrawal_witness_v12(&public, &private)
            .expect("encode shielded ICS-20 withdrawal witness");
        encoded[4..8].copy_from_slice(&8u32.to_le_bytes());

        decode_shielded_ics20_withdrawal_witness_v12(&encoded)
            .expect_err("decoder must reject unsupported version 8");
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_v12_rejects_non_canonical_boolean_flags() {
        const HEADER_BYTES: usize = 20;
        const TOP_FIELDS_THROUGH_NK: usize = 6 * 32 + 4 * 32 + 32 + 3 * 32 + 2 * 32;
        const MERKLE_PATH_BYTES: usize = 4 + 16 * (4 + 3 * 32);
        const COMMITTED_INDEXED_LEAF_BYTES: usize = 32 + 8 + 3 * 32;
        const IS_REGULATED_OFFSET: usize = HEADER_BYTES
            + TOP_FIELDS_THROUGH_NK
            + MERKLE_PATH_BYTES
            + 8
            + COMMITTED_INDEXED_LEAF_BYTES;
        const SLIM_REQUIRED_SPEND_BYTES: usize = 3 * 32 + 8 + 4 + 24 * 3 * 32 + 32 + 64 + 1;
        const ROUTING_PRIVATE_BYTES: usize = 2 + 8 + 32;
        const OPTIONAL_IS_DUMMY_OFFSET: usize = IS_REGULATED_OFFSET
            + 1
            + ROUTING_PRIVATE_BYTES
            + MERKLE_PATH_BYTES
            + 8
            + 2 * 32
            + 2 * SLIM_REQUIRED_SPEND_BYTES;

        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        for offset in [IS_REGULATED_OFFSET, OPTIONAL_IS_DUMMY_OFFSET] {
            let mut encoded = encode_shielded_ics20_withdrawal_witness_v12(&public, &private)
                .expect("encode shielded ICS-20 withdrawal witness");
            encoded[offset] = 2;
            decode_shielded_ics20_withdrawal_witness_v12(&encoded)
                .expect_err("V12 decoder must reject non-canonical boolean flags");
        }
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_v12_rejects_unbalanced_amounts() {
        let (mut public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        public.outbound_amount += Fq::from(1u64);

        ShieldedIcs20WithdrawalWitnessV12::from_public_private(&public, &private)
            .expect_err("withdrawal witness must reject non-conserving withdrawal amounts");
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_v12_rejects_non_blinding_balance_commitment() {
        let (mut public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        public.balance_commitment = Balance::default().commit(Fr::from(999u64));

        ShieldedIcs20WithdrawalWitnessV12::from_public_private(&public, &private)
            .expect_err("withdrawal witness must reject a non-blinding-only balance commitment");
    }
}

#[cfg(any(unix, windows))]
mod native {
    use super::*;
    use crate::gnark::transport::{BundledArtifacts, GnarkClient, GnarkFamilyConfig};
    use anyhow::bail;
    const SHIELDED_ICS20_WITHDRAWAL_LIB_BASENAME: &str =
        "libshieldd_gnark_shielded_ics20_withdrawal";
    const SHIELDED_ICS20_WITHDRAWAL_ENV_ARTIFACT_DIR: &str =
        "SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR";
    const SHIELDED_ICS20_WITHDRAWAL_ENV_LIB: &str = "SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB";
    const SHIELDED_ICS20_WITHDRAWAL_ENV_DAEMON: &str =
        "SHIELDD_GNARK_SHIELDED_ICS20_WITHDRAWAL_DAEMON";

    const SHIELDED_ICS20_WITHDRAWAL_INIT_SYMBOL: &[u8] =
        b"shieldd_gnark_shielded_ics20_withdrawal_init";
    const SHIELDED_ICS20_WITHDRAWAL_INIT_FROM_BYTES_SYMBOL: &[u8] =
        b"shieldd_gnark_shielded_ics20_withdrawal_init_from_bytes";
    const SHIELDED_ICS20_WITHDRAWAL_PROVE_SYMBOL: &[u8] =
        b"shieldd_gnark_shielded_ics20_withdrawal_prove";
    const SHIELDED_ICS20_WITHDRAWAL_FREE_SYMBOL: &[u8] =
        b"shieldd_gnark_shielded_ics20_withdrawal_free";
    const SHIELDED_ICS20_WITHDRAWAL_SHUTDOWN_SYMBOL: &[u8] =
        b"shieldd_gnark_shielded_ics20_withdrawal_shutdown";

    static SHIELDED_ICS20_WITHDRAWAL_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
        family: "shielded_ics20_withdrawal",
        lib_basename: SHIELDED_ICS20_WITHDRAWAL_LIB_BASENAME,
        bundled_library:
            shieldd_sdk_proof_params::GNARK_SHIELDED_ICS20_WITHDRAWAL_BUNDLED_LIBRARY_PATH,
        env_artifact_dir: SHIELDED_ICS20_WITHDRAWAL_ENV_ARTIFACT_DIR,
        env_lib: SHIELDED_ICS20_WITHDRAWAL_ENV_LIB,
        env_daemon: SHIELDED_ICS20_WITHDRAWAL_ENV_DAEMON,
        init_symbol: SHIELDED_ICS20_WITHDRAWAL_INIT_SYMBOL,
        init_from_bytes_symbol: SHIELDED_ICS20_WITHDRAWAL_INIT_FROM_BYTES_SYMBOL,
        prove_symbol: SHIELDED_ICS20_WITHDRAWAL_PROVE_SYMBOL,
        free_symbol: SHIELDED_ICS20_WITHDRAWAL_FREE_SYMBOL,
        shutdown_symbol: SHIELDED_ICS20_WITHDRAWAL_SHUTDOWN_SYMBOL,
    };

    pub(crate) fn shielded_ics20_withdrawal_family_config(
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

    pub(crate) struct GnarkShieldedIcs20WithdrawalClient {
        family_id: ShieldedIcs20WithdrawalFamilyId,
        inner: GnarkClient,
    }

    impl GnarkShieldedIcs20WithdrawalClient {
        pub(crate) fn load(family_id: ShieldedIcs20WithdrawalFamilyId) -> Result<Self> {
            Ok(Self {
                family_id,
                inner: GnarkClient::load(
                    shielded_ics20_withdrawal_family_config(family_id),
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
            public: &ShieldedIcs20WithdrawalProofPublic,
            private: &ShieldedIcs20WithdrawalProofPrivate,
        ) -> Result<ShieldedIcs20WithdrawalProof> {
            let witness_model =
                ShieldedIcs20WithdrawalWitnessV12::from_public_private(public, private)?;
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
                translate_shielded_ics20_withdrawal_proof_result(&payload, self.family_id)?;
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
pub(crate) use native::shielded_ics20_withdrawal_family_config;
#[cfg(any(unix, windows))]
pub(crate) use native::GnarkShieldedIcs20WithdrawalClient;
