#[cfg(any(unix, windows))]
use anyhow::bail;
use anyhow::Result;
use ark_serialize::CanonicalSerialize;
use decaf377::Fq;

use crate::{
    gnark::{
        note_seizure_witness::NoteSeizureWitness, transfer_proof_result::parse_binary_proof_result,
    },
    NoteSeizureProof, NoteSeizureProofPrivate, NoteSeizureProofPublic,
};

pub fn encode_note_seizure_witness(
    public: &NoteSeizureProofPublic,
    private: &NoteSeizureProofPrivate,
) -> Result<Vec<u8>> {
    NoteSeizureWitness::from_public_private(public, private)?.encode()
}

pub fn decode_note_seizure_witness(bytes: &[u8]) -> Result<NoteSeizureWitness> {
    NoteSeizureWitness::decode(bytes)
}

pub fn translate_note_seizure_proof_result(payload: &[u8]) -> Result<(Fq, NoteSeizureProof)> {
    let (claimed_hash, proof) = parse_binary_proof_result(payload, b"PNSP", "note seizure")?;
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes)?;
    let proof = NoteSeizureProof::try_from(
        shieldd_sdk_proto::shieldd::core::component::shielded_pool::v1::ZkNoteSeizureProof {
            inner: proof_bytes,
        },
    )?;
    Ok((claimed_hash, proof))
}

#[cfg(any(unix, windows))]
mod native {
    use super::*;
    use crate::gnark::transport::{GnarkClient, GnarkFamilyConfig};
    const NOTE_SEIZURE_ENV_ARTIFACT_DIR: &str = "SHIELDD_GNARK_NOTE_SEIZURE_ARTIFACT_DIR";
    const NOTE_SEIZURE_ENV_LIB: &str = "SHIELDD_GNARK_NOTE_SEIZURE_LIB";
    const NOTE_SEIZURE_ENV_DAEMON: &str = "SHIELDD_GNARK_NOTE_SEIZURE_DAEMON";

    pub(crate) static NOTE_SEIZURE_FAMILY_CONFIG: GnarkFamilyConfig = GnarkFamilyConfig {
        family: "note_seizure",
        bundled_library: None,
        env_artifact_dir: NOTE_SEIZURE_ENV_ARTIFACT_DIR,
        env_lib: NOTE_SEIZURE_ENV_LIB,
        env_daemon: NOTE_SEIZURE_ENV_DAEMON,
        init_symbol: b"",
        init_from_bytes_symbol: b"",
        prove_symbol: b"",
        free_symbol: b"",
        shutdown_symbol: b"",
    };

    /// Offline note-seizure prover backed by the shared gnark prover daemon.
    pub struct GnarkNoteSeizureClient {
        config: &'static crate::gnark::transport::ResolvedGnarkConfig,
        inner: std::sync::OnceLock<Result<GnarkClient, String>>,
    }

    static CONFIG: std::sync::LazyLock<
        Result<crate::gnark::transport::ResolvedGnarkConfig, String>,
    > = std::sync::LazyLock::new(|| {
        NOTE_SEIZURE_FAMILY_CONFIG
            .resolve()
            .map_err(|e| e.to_string())
    });

    pub(crate) fn resolved_configuration(
    ) -> Result<&'static crate::gnark::transport::ResolvedGnarkConfig> {
        CONFIG.as_ref().map_err(|e| anyhow::anyhow!("{e}"))
    }

    impl GnarkNoteSeizureClient {
        pub fn new() -> Result<Self> {
            Ok(Self {
                config: resolved_configuration()?,
                inner: std::sync::OnceLock::new(),
            })
        }

        pub fn prove(
            &self,
            public: &NoteSeizureProofPublic,
            private: &NoteSeizureProofPrivate,
        ) -> Result<NoteSeizureProof> {
            let inner = self
                .inner
                .get_or_init(|| GnarkClient::load_external(self.config).map_err(|e| e.to_string()))
                .as_ref()
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let witness_model = NoteSeizureWitness::from_public_private(public, private)?;
            let expected_hash = Fq::from_bytes_checked(&witness_model.claimed_statement_hash)
                .map_err(|_| {
                    anyhow::anyhow!("note seizure witness statement hash is non-canonical")
                })?;
            let witness = witness_model.encode()?;
            let payload = inner.prove(&witness)?;
            let (claimed_hash, proof) = translate_note_seizure_proof_result(&payload)?;
            if claimed_hash != expected_hash {
                bail!(
                "gnark note seizure proof returned wrong statement hash: expected {expected_hash}, got {claimed_hash}"
            );
            }
            proof.verify_with_prepared_vk(public, &inner.verifying_key)?;
            Ok(proof)
        }
    }
}
#[cfg(any(unix, windows))]
pub use native::GnarkNoteSeizureClient;
#[cfg(all(any(unix, windows), any(test, feature = "benchmark-helpers")))]
pub(crate) use native::NOTE_SEIZURE_FAMILY_CONFIG;

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use decaf377::Fr;
    use shieldd_sdk_asset::{asset, Value};
    use shieldd_sdk_keys::{keys::NullifierKey, test_keys};
    use shieldd_sdk_num::Amount;
    use shieldd_sdk_sct::Nullifier;
    use shieldd_sdk_tct as tct;

    use super::*;
    use crate::{HostTransfer, HostWithdrawal, HostWithdrawalDestination, RecoveryCapsule, Rseed};

    fn proof_inputs() -> (NoteSeizureProofPublic, NoteSeizureProofPrivate) {
        let address = test_keys::ADDRESS_0.clone();
        let asset_id = asset::Id(Fq::from(11u64));
        let amount = Amount::from(42u64);
        let capk = decaf377::Element::GENERATOR * Fr::from(19u64);
        let rseed = Rseed([17; 32]);
        let note_blinding = rseed.derive_note_blinding();
        let (recovery_capsule, opening) =
            RecoveryCapsule::encrypt(amount, note_blinding, capk, rseed)
                .expect("encrypt recovery capsule");
        let note_commitment = crate::note::commitment_from_address(
            address.clone(),
            Value { amount, asset_id },
            note_blinding,
            recovery_capsule.commitment(),
        )
        .expect("commit note");

        let mut tree = tct::Tree::new();
        tree.insert(tct::Witness::Keep, note_commitment)
            .expect("insert note commitment");
        let state_commitment_proof = tree
            .witness(note_commitment)
            .expect("witness note commitment");
        let rnk = Fq::from(23u64);
        let nullifier = Nullifier::derive(
            &NullifierKey(rnk),
            state_commitment_proof.position(),
            &note_commitment,
        );
        let authorization = crate::NoteSeizureAuthorizationBody {
            chain_id: "shieldd-test".to_owned(),
            note_commitment,
            nullifier,
            address,
            asset_id,
            amount,
            freeze_generation: 2,
            frozen_since_height: 10,
            withdrawal: HostWithdrawal {
                value: Value { amount, asset_id },
                destination: HostWithdrawalDestination::Transfer(HostTransfer {
                    recipient: "bank1seizureauthority".to_owned(),
                }),
            },
            expiry_height: 20,
        };

        (
            NoteSeizureProofPublic {
                authorization,
                anchor: state_commitment_proof.root(),
                history_required: false,
                recent_position_floor: 0,
                recovery_capsule,
                recovery_seed: opening.seed,
                rnk_commitment: shieldd_sdk_compliance::compliance_nullifier_key_commitment(rnk),
            },
            NoteSeizureProofPrivate {
                note_blinding,
                state_commitment_proof,
                rnk,
            },
        )
    }

    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_daemon_proof_note_seizure_roundtrip() {
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::NoteSeizure)
            .expect("proof test prerequisites must be present");

        let (public, private) = proof_inputs();
        let client = GnarkNoteSeizureClient::new().expect("start note seizure prover");
        let proof = client
            .prove(&public, &private)
            .expect("prove and verify note seizure through the daemon transport");

        proof
            .verify(&public)
            .expect("verify note seizure with the compiled consensus key");

        let mut changed_public = public;
        changed_public.authorization.expiry_height += 1;
        proof
            .verify(&changed_public)
            .expect_err("proof must not verify for a changed authorization statement");
    }
}

#[cfg(any(unix, windows))]
pub(super) use native::resolved_configuration;
