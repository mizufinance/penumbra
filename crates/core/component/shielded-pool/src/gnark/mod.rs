mod artifacts;
mod binary;
mod consolidate;
mod note_reshape_witness;
mod note_reshape_witness_binary;
pub mod runtime;
mod shielded_ics20_withdrawal;
mod shielded_ics20_withdrawal_witness;
mod shielded_ics20_withdrawal_witness_binary;
mod split;
mod transfer;
mod transfer_proof_result;
mod transfer_witness;
mod transfer_witness_binary;
mod transport;
mod typed;

pub use artifacts::GnarkArtifactMetadata;
pub use consolidate::{
    decode_consolidate_witness_v1, encode_consolidate_witness_v1,
    translate_consolidate_proof_result, GnarkConsolidateClient,
};
pub use note_reshape_witness::{ConsolidateWitnessV1, SplitWitnessV1};
pub use shielded_ics20_withdrawal::{
    decode_shielded_ics20_withdrawal_witness_v1, encode_shielded_ics20_withdrawal_witness_v1,
    translate_shielded_ics20_withdrawal_proof_result, GnarkShieldedIcs20WithdrawalClient,
};
pub use shielded_ics20_withdrawal_witness::ShieldedIcs20WithdrawalWitnessV1;
pub use split::{
    decode_split_witness_v1, encode_split_witness_v1, translate_split_proof_result,
    GnarkSplitClient,
};
pub use transfer::{
    decode_transfer_witness_v1, encode_transfer_witness_v1, translate_transfer_proof_result,
    GnarkTransferClient,
};
pub use transfer_witness::TransferWitnessV1;
pub use typed::{ComplianceLeafBinary, IndexedLeafBinary, MerklePathBinary, PointAffineBytes};

#[cfg(all(test, any(unix, windows)))]
mod repo_local_demo_library_tests {
    use std::path::PathBuf;

    use libloading::Library;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../")
            .canonicalize()
            .expect("repo root should resolve")
    }

    fn shared_lib_ext() -> &'static str {
        if cfg!(target_os = "macos") {
            "dylib"
        } else if cfg!(target_os = "linux") {
            "so"
        } else {
            "dll"
        }
    }

    #[test]
    fn repo_local_demo_gnark_libraries_are_loadable() {
        let gnark_dir = repo_root().join("tools/gnark");
        let cases = [
            (
                "transfer",
                "artifacts/transfer",
                b"penumbra_gnark_transfer_init" as &[u8],
            ),
            (
                "split",
                "artifacts/split1x4",
                b"penumbra_gnark_split_init" as &[u8],
            ),
            (
                "consolidate",
                "artifacts/consolidate2x1",
                b"penumbra_gnark_consolidate_init" as &[u8],
            ),
            (
                "shielded_ics20_withdrawal",
                "artifacts/shielded_ics20_withdrawal",
                b"penumbra_gnark_shielded_ics20_withdrawal_init" as &[u8],
            ),
        ];

        let available_cases = cases
            .into_iter()
            .map(|(family, artifact_dir, init_symbol)| {
                (
                    family,
                    artifact_dir,
                    init_symbol,
                    gnark_dir.join(format!("libpenumbra_gnark_{family}.{}", shared_lib_ext())),
                )
            })
            .filter(|(_, _, _, lib_path)| lib_path.exists())
            .collect::<Vec<_>>();

        if available_cases.is_empty() {
            eprintln!(
                "skipping repo-local demo gnark library smoke test; no local shared libraries found in {}",
                gnark_dir.display()
            );
            return;
        }

        for (family, artifact_dir, init_symbol, lib_path) in available_cases {
            let metadata_path = gnark_dir.join(artifact_dir).join("circuit_metadata.json");
            assert!(
                metadata_path.exists(),
                "expected repo-local demo gnark metadata for {family} at {}",
                metadata_path.display()
            );

            let library = unsafe { Library::new(&lib_path) }.unwrap_or_else(|error| {
                panic!(
                    "repo-local demo gnark library for {family} failed to load from {}: {error}",
                    lib_path.display()
                )
            });

            unsafe {
                let _: libloading::Symbol<'_, unsafe extern "C" fn()> =
                    library.get(init_symbol).unwrap_or_else(|error| {
                        panic!(
                            "repo-local demo gnark library for {family} is missing init symbol {:?}: {error}",
                            std::str::from_utf8(init_symbol).expect("symbol should be utf8")
                        )
                    });
            }
        }
    }
}
