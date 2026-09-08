use anyhow::Result;
use ark_serialize::CanonicalSerialize;
use decaf377::Fq;

use crate::{
    gnark::{
        shielded_ics20_withdrawal_witness::ShieldedIcs20WithdrawalWitness,
        transfer_proof_result::parse_binary_proof_result,
    },
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic,
    },
    ShieldedIcs20WithdrawalFamilyId,
};

pub fn encode_shielded_ics20_withdrawal_witness(
    public: &ShieldedIcs20WithdrawalProofPublic,
    private: &ShieldedIcs20WithdrawalProofPrivate,
) -> Result<Vec<u8>> {
    ShieldedIcs20WithdrawalWitness::from_public_private(public, private)?.encode()
}

pub fn decode_shielded_ics20_withdrawal_witness(
    bytes: &[u8],
) -> Result<ShieldedIcs20WithdrawalWitness> {
    ShieldedIcs20WithdrawalWitness::decode(bytes)
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
        decode_shielded_ics20_withdrawal_witness, encode_shielded_ics20_withdrawal_witness,
    };
    use crate::{
        gnark::ShieldedIcs20WithdrawalWitness, test_proof_helpers::proof_test_helpers,
        ShieldedIcs20WithdrawalFamilyId,
    };
    use decaf377::{Fq, Fr};
    use shieldd_sdk_asset::Balance;

    #[test]
    fn shielded_ics20_withdrawal_witness_roundtrip() {
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        let encoded = encode_shielded_ics20_withdrawal_witness(&public, &private)
            .expect("encode shielded ICS-20 withdrawal witness");
        let decoded = decode_shielded_ics20_withdrawal_witness(&encoded)
            .expect("decode shielded ICS-20 withdrawal witness");
        let expected = ShieldedIcs20WithdrawalWitness::from_public_private(&public, &private)
            .expect("build shielded ICS-20 withdrawal witness");
        assert_eq!(decoded, expected);

        let leaf = &decoded.asset_indexed_leaf;
        let parts = private.asset_indexed_leaf.commitment_parts();
        let recomposed = shieldd_sdk_tct::StateCommitment(poseidon377::hash_5(
            &shieldd_sdk_compliance::IMT_LEAF_DOMAIN_SEP,
            (
                Fq::from_bytes_checked(&leaf.value).expect("canonical leaf value"),
                Fq::from(leaf.next_index),
                Fq::from_bytes_checked(&leaf.next_value).expect("canonical next value"),
                parts.params_hash,
                parts.ring_hash,
            ),
        ));
        assert_eq!(
            recomposed,
            private.asset_indexed_leaf.commit(),
            "compact leaf view must recompose the canonical native commitment"
        );
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_rejects_non_canonical_boolean_flags() {
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        let witness = ShieldedIcs20WithdrawalWitness::from_public_private(&public, &private)
            .expect("build shielded ICS-20 withdrawal witness");
        let encoded = witness
            .encode()
            .expect("encode shielded ICS-20 withdrawal witness");
        let changed_byte_offset = |altered: &ShieldedIcs20WithdrawalWitness| {
            let alternate = altered
                .encode()
                .expect("encode witness with one boolean toggled");
            let offsets = encoded
                .iter()
                .zip(alternate.iter())
                .enumerate()
                .filter_map(|(offset, (left, right))| (left != right).then_some(offset))
                .collect::<Vec<_>>();
            assert_eq!(offsets.len(), 1, "one toggled boolean must change one byte");
            offsets[0]
        };

        let mut alternate_regulation = witness.clone();
        alternate_regulation.is_regulated = !alternate_regulation.is_regulated;
        let mut alternate_dummy = witness.clone();
        alternate_dummy.optional_spend.is_dummy = !alternate_dummy.optional_spend.is_dummy;

        for offset in [
            changed_byte_offset(&alternate_regulation),
            changed_byte_offset(&alternate_dummy),
        ] {
            let mut encoded = encoded.clone();
            encoded[offset] = 2;
            decode_shielded_ics20_withdrawal_witness(&encoded)
                .expect_err(" decoder must reject non-canonical boolean flags");
        }
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_rejects_unbalanced_amounts() {
        let (mut public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        public.outbound_amount += Fq::from(1u64);

        ShieldedIcs20WithdrawalWitness::from_public_private(&public, &private)
            .expect_err("withdrawal witness must reject non-conserving withdrawal amounts");
    }

    #[test]
    fn shielded_ics20_withdrawal_witness_rejects_non_blinding_balance_commitment() {
        let (mut public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs(
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        public.balance_commitment = Balance::default().commit(Fr::from(999u64));

        ShieldedIcs20WithdrawalWitness::from_public_private(&public, &private)
            .expect_err("withdrawal witness must reject a non-blinding-only balance commitment");
    }
}

#[cfg(any(unix, windows))]
mod native {
    use super::*;
    use crate::gnark::transport::{BundledArtifacts, GnarkClient, GnarkFamilyConfig};
    use anyhow::bail;
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

    static CONFIGS: std::sync::LazyLock<
        std::collections::BTreeMap<
            ShieldedIcs20WithdrawalFamilyId,
            Result<super::super::transport::ResolvedGnarkConfig, String>,
        >,
    > = std::sync::LazyLock::new(|| {
        ShieldedIcs20WithdrawalFamilyId::ALL
            .into_iter()
            .map(|id| {
                (
                    id,
                    shielded_ics20_withdrawal_family_config(id)
                        .resolve()
                        .map_err(|error| error.to_string()),
                )
            })
            .collect()
    });

    pub(crate) fn resolved_configuration(
        family_id: ShieldedIcs20WithdrawalFamilyId,
    ) -> Result<&'static super::super::transport::ResolvedGnarkConfig> {
        CONFIGS
            .get(&family_id)
            .expect("known proof family")
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    impl GnarkShieldedIcs20WithdrawalClient {
        pub(crate) fn load(family_id: ShieldedIcs20WithdrawalFamilyId) -> Result<Self> {
            let config = resolved_configuration(family_id)?;
            Ok(Self {
                family_id,
                inner: GnarkClient::load(
                    config,
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
                ShieldedIcs20WithdrawalWitness::from_public_private(public, private)?;
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

#[cfg(any(unix, windows))]
pub(super) use native::resolved_configuration;
