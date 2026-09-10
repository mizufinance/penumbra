#[cfg(all(feature = "prover", any(unix, windows)))]
mod artifacts;
mod binary;
mod note_reshape;
mod note_reshape_witness;
mod note_reshape_witness_binary;
mod note_seizure;
mod note_seizure_witness;
mod note_seizure_witness_binary;
#[cfg(all(feature = "prover", any(unix, windows)))]
pub(crate) mod prover_worker;
mod recovery_capsule_witness;
mod recovery_capsule_witness_binary;
#[cfg(all(feature = "prover", any(unix, windows)))]
mod runtime;
mod shielded_withdrawal;
mod shielded_withdrawal_witness;
mod shielded_withdrawal_witness_binary;
mod transfer;
mod transfer_proof_result;
mod transfer_witness;
mod transfer_witness_binary;
#[cfg(all(feature = "prover", any(unix, windows)))]
mod transport;
mod typed;

pub use note_reshape::{
    decode_note_reshape_witness, encode_note_reshape_witness, translate_note_reshape_proof_result,
};
pub use note_reshape_witness::NoteReshapeWitness;
#[cfg(all(feature = "prover", any(unix, windows)))]
pub use note_seizure::GnarkNoteSeizureClient;
pub use note_seizure::{
    decode_note_seizure_witness, encode_note_seizure_witness, translate_note_seizure_proof_result,
};
pub use note_seizure_witness::{NoteSeizureRecoveryWitness, NoteSeizureWitness};
pub use recovery_capsule_witness::RecoveryCapsuleWitness;
pub use shielded_withdrawal::{
    decode_shielded_withdrawal_witness, encode_shielded_withdrawal_witness,
    translate_shielded_withdrawal_proof_result,
};
pub use shielded_withdrawal_witness::ShieldedWithdrawalWitness;
pub use transfer::{
    decode_transfer_witness, encode_transfer_witness, translate_transfer_proof_result,
};
pub use transfer_witness::TransferWitness;
#[cfg(test)]
pub(crate) use typed::point_affine_compress_to_field_bytes;
pub use typed::{ComplianceLeafBinary, IndexedLeafBinary, MerklePathBinary, PointAffineBytes};

#[cfg(test)]
mod soundness_fixture_tests {
    use std::path::PathBuf;

    use rand::SeedableRng;

    use crate::{
        gnark::{
            encode_note_reshape_witness, encode_shielded_withdrawal_witness,
            encode_transfer_witness,
        },
        test_proof_helpers::proof_test_helpers,
        NoteReshapeFamilyId, ShieldedWithdrawalFamilyId,
    };

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../tools/gnark/internal/testfixtures/vectors")
    }

    fn write_fixture(filename: &str, bytes: Vec<u8>) {
        let dir = fixture_dir();
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("create soundness fixture dir {dir:?}: {e}"));
        let path = dir.join(filename);
        std::fs::write(&path, &bytes)
            .unwrap_or_else(|e| panic!("write soundness fixture {path:?}: {e}"));
        eprintln!("wrote {} bytes to {path:?}", bytes.len());
    }

    fn write_transfer_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0000_0054_5832_5832);
        let (public, private) =
            proof_test_helpers::build_transfer_roundtrip_inputs_with_rng(&mut rng, true);
        write_fixture(
            "transfer_witness.bin",
            encode_transfer_witness(&public, &private).expect("encode transfer witness"),
        );
    }

    fn write_unregulated_transfer_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x554e_5245_4758_4631);
        let asset_id = shieldd_sdk_asset::asset::REGISTRY
            .parse_unit("test_usd")
            .id();
        let predecessor_asset_id = asset_id.0 - decaf377::Fq::from(1u64);
        let (public, private) =
            proof_test_helpers::build_transfer_hidden_arity_roundtrip_inputs_for_asset_populated(
                &mut rng,
                asset_id,
                predecessor_asset_id,
                1,
                false,
            );
        write_fixture(
            "transfer_unregulated_witness.bin",
            encode_transfer_witness(&public, &private)
                .expect("encode unregulated transfer witness"),
        );
    }

    fn write_flagged_transfer_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x464c_4147_5631_3601);
        let (public, private) =
            proof_test_helpers::build_transfer_flagged_hidden_arity_roundtrip_inputs_with_rng(
                &mut rng,
            );
        write_fixture(
            "transfer_flagged_witness.bin",
            encode_transfer_witness(&public, &private).expect("encode flagged transfer witness"),
        );
    }

    fn write_accumulating_transfer_fixtures() {
        for (limit, filename) in [
            (100, "transfer_accumulating_witness.bin"),
            (99, "transfer_accumulator_over_limit_witness.bin"),
        ] {
            let mut rng = rand::rngs::StdRng::seed_from_u64(
                0x4143_4355_4d00_0000u64.saturating_add(limit as u64),
            );
            let (public, private) = proof_test_helpers::
                build_transfer_accumulating_hidden_arity_roundtrip_inputs_with_rng(
                    &mut rng, limit,
                );
            write_fixture(
                filename,
                encode_transfer_witness(&public, &private)
                    .expect("encode accumulating transfer witness"),
            );
        }
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x434f_4e54_494e_5545);
        let (public, private) =
            proof_test_helpers::build_transfer_continuing_accumulator_roundtrip_inputs_with_rng(
                &mut rng,
            );
        write_fixture(
            "transfer_accumulator_continuation_witness.bin",
            encode_transfer_witness(&public, &private)
                .expect("encode continuing accumulator transfer witness"),
        );
    }

    fn write_shielded_withdrawal_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0000_0049_4353_3201);
        let (public, private) =
            proof_test_helpers::build_shielded_withdrawal_roundtrip_inputs_with_rng(
                &mut rng,
                ShieldedWithdrawalFamilyId::Canonical,
                true,
            );
        write_fixture(
            "shielded_withdrawal_witness.bin",
            encode_shielded_withdrawal_witness(&public, &private)
                .expect("encode shielded withdrawal witness"),
        );
    }

    fn write_accumulating_shielded_withdrawal_fixtures() {
        for (name, seed, mode) in [
            (
                "shielded_withdrawal_accumulator_origin_witness.bin",
                0x4f52_4947_494e_0001,
                proof_test_helpers::WithdrawalAccumulatorTestMode::Origin,
            ),
            (
                "shielded_withdrawal_accumulator_continuation_witness.bin",
                0x434f_4e54_0000_0001,
                proof_test_helpers::WithdrawalAccumulatorTestMode::Continuation {
                    prior_volume: 25,
                },
            ),
        ] {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let (public, private) =
                proof_test_helpers::build_shielded_withdrawal_roundtrip_inputs_with_rng_and_mode(
                    &mut rng,
                    ShieldedWithdrawalFamilyId::Canonical,
                    true,
                    2,
                    mode,
                );
            write_fixture(
                name,
                encode_shielded_withdrawal_witness(&public, &private)
                    .expect("encode accumulating withdrawal witness"),
            );
        }
    }

    fn write_unregulated_shielded_withdrawal_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x554e_5245_4757_4438);
        let (public, private) =
            proof_test_helpers::build_shielded_withdrawal_roundtrip_inputs_with_rng_and_real_spends(
                &mut rng,
                ShieldedWithdrawalFamilyId::Canonical,
                false,
                1,
            );
        write_fixture(
            "shielded_withdrawal_unregulated_witness.bin",
            encode_shielded_withdrawal_witness(&public, &private)
                .expect("encode unregulated optional-dummy withdrawal witness"),
        );
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted withdrawal gnark soundness fixture"]
    fn bless_shielded_withdrawal_witness_fixture() {
        write_shielded_withdrawal_fixture();
        write_unregulated_shielded_withdrawal_fixture();
        write_accumulating_shielded_withdrawal_fixtures();
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted transfer gnark soundness fixture"]
    fn bless_transfer_witness_fixture() {
        write_transfer_fixture();
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted unregulated transfer gnark soundness fixture"]
    fn bless_unregulated_transfer_witness_fixture() {
        write_unregulated_transfer_fixture();
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted flagged transfer gnark soundness fixture"]
    fn bless_flagged_transfer_witness_fixture() {
        write_flagged_transfer_fixture();
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted accumulator transfer gnark fixtures"]
    fn bless_accumulating_transfer_witness_fixtures() {
        write_accumulating_transfer_fixtures();
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted gnark soundness fixtures"]
    fn bless_soundness_gnark_witness_fixtures() {
        write_transfer_fixture();
        write_flagged_transfer_fixture();
        write_accumulating_transfer_fixtures();

        let mut one_to_many_rng = rand::rngs::StdRng::seed_from_u64(0x0000_0053_3158_3401);
        let (one_to_many_public, one_to_many_private) =
            proof_test_helpers::build_note_reshape_roundtrip_inputs_with_rng(
                &mut one_to_many_rng,
                NoteReshapeFamilyId::OneByEight,
            );
        write_fixture(
            "note_reshape1x8_witness.bin",
            encode_note_reshape_witness(&one_to_many_public, &one_to_many_private)
                .expect("encode note reshape witness"),
        );

        for (family_id, seed, filename) in [(
            NoteReshapeFamilyId::EightByOne,
            0x0000_0043_3858_3101,
            "note_reshape8x1_witness.bin",
        )] {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let (public, private) =
                proof_test_helpers::build_note_reshape_roundtrip_inputs_with_rng(
                    &mut rng, family_id,
                );
            write_fixture(
                filename,
                encode_note_reshape_witness(&public, &private)
                    .expect("encode note reshape witness"),
            );
        }

        write_shielded_withdrawal_fixture();
    }
}

#[cfg(all(
    all(feature = "prover", any(unix, windows)),
    any(test, feature = "benchmark-helpers")
))]
#[derive(Clone, Copy)]
pub enum ProofTestFamily {
    NoteSeizure,
    Transfer,
    NoteReshape(crate::NoteReshapeFamilyId),
    Withdrawal,
}

#[cfg(all(
    all(feature = "prover", any(unix, windows)),
    any(test, feature = "benchmark-helpers")
))]
pub fn require_proof_test_runtime(family: ProofTestFamily) -> anyhow::Result<()> {
    match family {
        ProofTestFamily::NoteSeizure => {
            note_seizure::NOTE_SEIZURE_FAMILY_CONFIG.require_test_prerequisites(&[])
        }
        ProofTestFamily::Transfer => transfer::TRANSFER_FAMILY_CONFIG
            .require_test_prerequisites(shieldd_sdk_proof_params::transfer_proving_key_bytes()),
        ProofTestFamily::NoteReshape(family) => note_reshape::note_reshape_family_config(family)
            .require_test_prerequisites(family.proving_key_bytes()),
        ProofTestFamily::Withdrawal => {
            let family = crate::ShieldedWithdrawalFamilyId::Canonical;
            shielded_withdrawal::shielded_withdrawal_family_config(family)
                .require_test_prerequisites(family.proving_key_bytes())
        }
    }
}

#[cfg(all(feature = "prover", any(unix, windows)))]
pub(crate) use transfer::GnarkTransferClient;

#[cfg(all(feature = "prover", any(unix, windows)))]
pub(crate) use note_reshape::GnarkNoteReshapeClient;

#[cfg(all(feature = "prover", any(unix, windows)))]
pub(crate) use shielded_withdrawal::GnarkShieldedWithdrawalClient;

/// Explicitly selected proving capability; verification does not require one.
#[cfg(all(feature = "prover", any(unix, windows)))]
pub enum ProverCapability {
    Transfer,
    NoteReshape(crate::NoteReshapeFamilyId),
    Withdrawal(crate::ShieldedWithdrawalFamilyId),
    NoteSeizure,
}

/// Resolves process configuration once without loading keys or starting a prover.
#[cfg(all(feature = "prover", any(unix, windows)))]
pub fn initialize_prover(capability: ProverCapability) -> anyhow::Result<()> {
    match capability {
        ProverCapability::Transfer => transfer::resolved_configuration()?,
        ProverCapability::NoteReshape(family) => note_reshape::resolved_configuration(family)?,
        ProverCapability::Withdrawal(family) => {
            shielded_withdrawal::resolved_configuration(family)?
        }
        ProverCapability::NoteSeizure => note_seizure::resolved_configuration()?,
    };
    Ok(())
}
