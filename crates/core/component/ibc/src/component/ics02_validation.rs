use crate::client_types::AnyClientState;
use crate::IBC_PROOF_SPECS;
use ibc_proto::google::protobuf::Any;
use ibc_types::lightclients::tendermint::TrustThreshold;

// validate the parameters of an AnyClientState, verifying that it is a valid Penumbra client
// state.
pub fn validate_penumbra_sdk_client_state(
    client_state: Any,
    chain_id: &str,
    current_height: u64,
) -> anyhow::Result<()> {
    let any_client_state = AnyClientState::try_from(client_state)?;

    if any_client_state.is_frozen() {
        anyhow::bail!("invalid client state: frozen");
    }

    // NOTE: Chain ID validation is actually not standardized yet. see
    // https://github.com/informalsystems/ibc-rs/pull/304#discussion_r503917283
    let expected_chain_id = ibc_types::core::connection::ChainId::from_string(chain_id);
    if expected_chain_id != any_client_state.chain_id() {
        anyhow::bail!("invalid client state: chain id does not match");
    }

    // check that the revision number is the same as our chain ID's version
    if any_client_state.latest_height().revision_number() != expected_chain_id.version() {
        anyhow::bail!("invalid client state: revision number does not match");
    }

    // check that the latest height isn't gte the current block height
    if any_client_state.latest_height().revision_height() >= current_height {
        anyhow::bail!(
            "invalid client state: latest height is greater than or equal to the current block height"
        );
    }

    // check client proof specs match penumbra proof specs
    let client_proof_specs = any_client_state.proof_specs();
    if IBC_PROOF_SPECS.clone() != client_proof_specs {
        // allow legacy proof specs without prehash_key_before_comparison
        let mut spec_with_prehash_key = client_proof_specs.clone();
        if spec_with_prehash_key.len() >= 2 {
            spec_with_prehash_key[0].prehash_key_before_comparison = true;
            spec_with_prehash_key[1].prehash_key_before_comparison = true;
        }
        if IBC_PROOF_SPECS.clone() != spec_with_prehash_key {
            anyhow::bail!("invalid client state: proof specs do not match");
        }
    }

    // Tendermint-specific trust threshold validation (Penumbra is a Tendermint chain)
    if let Some(trust_threshold) = any_client_state.trust_threshold() {
        validate_trust_threshold(trust_threshold)?;
    }

    // For Tendermint clients, check unbonding > trusting period
    if let AnyClientState::Tendermint(ref tm) = any_client_state {
        if tm.unbonding_period < tm.trusting_period {
            anyhow::bail!("invalid client state: unbonding period is less than trusting period");
        }
    }

    Ok(())
}

// Check that the trust threshold is:
//
// a) non-zero
// b) greater or equal to 1/3
// c) strictly less than 1
fn validate_trust_threshold(trust_threshold: TrustThreshold) -> anyhow::Result<()> {
    if trust_threshold.denominator() == 0 {
        anyhow::bail!("trust threshold denominator cannot be zero");
    }

    if trust_threshold.numerator() * 3 < trust_threshold.denominator() {
        anyhow::bail!("trust threshold must be greater than 1/3");
    }

    if trust_threshold.numerator() > trust_threshold.denominator() {
        anyhow::bail!("trust threshold must be less than or equal to 1");
    }

    Ok(())
}
