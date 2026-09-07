#[cfg(any(unix, windows))]
mod artifacts;
mod binary;
mod note_reshape;
mod note_reshape_witness;
mod note_reshape_witness_binary;
#[cfg(any(unix, windows))]
pub(crate) mod prover_worker;
#[cfg(any(unix, windows))]
mod runtime;
mod shielded_ics20_withdrawal;
mod shielded_ics20_withdrawal_witness;
mod shielded_ics20_withdrawal_witness_binary;
mod transfer;
mod transfer_proof_result;
mod transfer_witness;
mod transfer_witness_binary;
#[cfg(any(unix, windows))]
mod transport;
mod typed;

pub use note_reshape::{
    decode_note_reshape_witness_v6, encode_note_reshape_witness_v6,
    translate_note_reshape_proof_result,
};
pub use note_reshape_witness::NoteReshapeWitnessV6;
pub use shielded_ics20_withdrawal::{
    decode_shielded_ics20_withdrawal_witness_v12, encode_shielded_ics20_withdrawal_witness_v12,
    translate_shielded_ics20_withdrawal_proof_result,
};
pub use shielded_ics20_withdrawal_witness::ShieldedIcs20WithdrawalWitnessV12;
pub use transfer::{
    decode_transfer_witness_v20, encode_transfer_witness_v20, translate_transfer_proof_result,
};
pub use transfer_witness::TransferWitnessV20;
#[cfg(test)]
pub(crate) use typed::point_affine_compress_to_field_bytes;
pub use typed::{ComplianceLeafBinary, IndexedLeafBinary, MerklePathBinary, PointAffineBytes};

#[cfg(test)]
mod soundness_fixture_tests {
    use std::path::PathBuf;

    use rand::SeedableRng;

    use crate::{
        gnark::{
            encode_note_reshape_witness_v6, encode_shielded_ics20_withdrawal_witness_v12,
            encode_transfer_witness_v20,
        },
        test_proof_helpers::proof_test_helpers,
        NoteReshapeFamilyId, ShieldedIcs20WithdrawalFamilyId,
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
            "transfer_witness_v20.bin",
            encode_transfer_witness_v20(&public, &private).expect("encode transfer witness"),
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
            "transfer_unregulated_witness_v20.bin",
            encode_transfer_witness_v20(&public, &private)
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
            "transfer_flagged_witness_v20.bin",
            encode_transfer_witness_v20(&public, &private)
                .expect("encode flagged transfer witness"),
        );
    }

    fn write_shielded_ics20_withdrawal_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0000_0049_4353_3201);
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng(
                &mut rng,
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                true,
            );
        write_fixture(
            "shielded_ics20_withdrawal_witness_v12.bin",
            encode_shielded_ics20_withdrawal_witness_v12(&public, &private)
                .expect("encode shielded ICS-20 withdrawal witness"),
        );
    }

    fn write_unregulated_shielded_ics20_withdrawal_fixture() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x554e_5245_4757_4438);
        let (public, private) =
            proof_test_helpers::build_shielded_ics20_withdrawal_roundtrip_inputs_with_rng_and_real_spends(
                &mut rng,
                ShieldedIcs20WithdrawalFamilyId::Canonical,
                false,
                1,
            );
        write_fixture(
            "shielded_ics20_withdrawal_unregulated_witness_v12.bin",
            encode_shielded_ics20_withdrawal_witness_v12(&public, &private)
                .expect("encode unregulated optional-dummy withdrawal witness"),
        );
    }

    #[test]
    #[ignore = "debug: refresh Rust-emitted withdrawal gnark soundness fixture"]
    fn bless_shielded_ics20_withdrawal_witness_fixture() {
        write_shielded_ics20_withdrawal_fixture();
        write_unregulated_shielded_ics20_withdrawal_fixture();
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
    #[ignore = "debug: refresh Rust-emitted gnark soundness fixtures"]
    fn bless_soundness_gnark_witness_fixtures() {
        write_transfer_fixture();
        write_flagged_transfer_fixture();

        let mut one_to_many_rng = rand::rngs::StdRng::seed_from_u64(0x0000_0053_3158_3401);
        let (one_to_many_public, one_to_many_private) =
            proof_test_helpers::build_note_reshape_roundtrip_inputs_with_rng(
                &mut one_to_many_rng,
                NoteReshapeFamilyId::OneByEight,
            );
        write_fixture(
            "note_reshape1x8_witness_v6.bin",
            encode_note_reshape_witness_v6(&one_to_many_public, &one_to_many_private)
                .expect("encode note reshape witness"),
        );

        for (family_id, seed, filename) in [(
            NoteReshapeFamilyId::EightByOne,
            0x0000_0043_3858_3101,
            "note_reshape8x1_witness_v6.bin",
        )] {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let (public, private) =
                proof_test_helpers::build_note_reshape_roundtrip_inputs_with_rng(
                    &mut rng, family_id,
                );
            write_fixture(
                filename,
                encode_note_reshape_witness_v6(&public, &private)
                    .expect("encode note reshape witness"),
            );
        }

        write_shielded_ics20_withdrawal_fixture();
    }
}

#[cfg(all(any(unix, windows), any(test, feature = "benchmark-helpers")))]
#[derive(Clone, Copy)]
pub enum ProofTestFamily {
    Transfer,
    NoteReshape(crate::NoteReshapeFamilyId),
    Withdrawal,
}

#[cfg(all(any(unix, windows), any(test, feature = "benchmark-helpers")))]
pub fn require_proof_test_runtime(family: ProofTestFamily) -> anyhow::Result<()> {
    match family {
        ProofTestFamily::Transfer => transfer::TRANSFER_FAMILY_CONFIG
            .require_test_prerequisites(shieldd_sdk_proof_params::transfer_proving_key_bytes()),
        ProofTestFamily::NoteReshape(family) => note_reshape::note_reshape_family_config(family)
            .require_test_prerequisites(family.proving_key_bytes()),
        ProofTestFamily::Withdrawal => {
            let family = crate::ShieldedIcs20WithdrawalFamilyId::Canonical;
            shielded_ics20_withdrawal::shielded_ics20_withdrawal_family_config(family)
                .require_test_prerequisites(family.proving_key_bytes())
        }
    }
}

#[cfg(any(unix, windows))]
pub(crate) use transfer::GnarkTransferClient;

#[cfg(any(unix, windows))]
pub(crate) use note_reshape::GnarkNoteReshapeClient;

#[cfg(any(unix, windows))]
pub(crate) use shielded_ics20_withdrawal::GnarkShieldedIcs20WithdrawalClient;
