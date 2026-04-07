#[cfg(any(unix, windows))]
use crate::TransferFamilyId;
#[cfg(any(unix, windows))]
use anyhow::Result;

#[cfg(any(unix, windows))]
pub(super) fn should_skip_transfer_proof_roundtrip_tests() -> bool {
    if cfg!(debug_assertions) {
        eprintln!(
            "skipping transfer gnark roundtrip tests in debug builds: use `cargo test --release -p penumbra-sdk-shielded-pool --features bundled-proving-keys transfer_proof_roundtrip --lib` for real bundled transfer proving"
        );
        return true;
    }

    match transfer_prover_available_for_tests(TransferFamilyId::OneByOne) {
        Ok(false) => {
            eprintln!(
                "skipping transfer gnark roundtrip tests: no bundled or external gnark transfer prover transport is available"
            );
            true
        }
        Ok(true) => false,
        Err(error) => panic!("transfer gnark prover transport configuration is invalid: {error}"),
    }
}

#[cfg(any(unix, windows))]
fn transfer_prover_available_for_tests(
    family_id: TransferFamilyId,
) -> Result<bool, crate::ProofError> {
    if crate::gnark::GnarkTransferClient::env_override_configured() {
        return crate::gnark::GnarkTransferClient::from_env(family_id)
            .map(|_| true)
            .map_err(|error| crate::ProofError::ProofGenerationFailed(error.to_string()));
    }

    Ok(crate::gnark::GnarkTransferClient::bundled_transport_available(family_id))
}
