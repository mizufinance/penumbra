use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use ibc_types::core::client::ClientId;
use penumbra_sdk_sct::component::clock::EpochRead;

use crate::client_types::AnyClientState;
use crate::component::{ConsensusStateWriteExt, HostInterface};

use super::client::{
    ClientStatus, StateReadExt as ClientStateReadExt, StateWriteExt as ClientStateWriteExt,
};

/// Extension trait for IBC client recovery operations.
///
/// This trait provides privileged operations for recovering frozen/expired IBC clients
/// by substituting them with active clients. This is typically used during chain upgrades
/// or emergency recovery scenarios.
#[async_trait]
pub trait ClientRecoveryExt: StateWrite + ConsensusStateWriteExt {
    /// Validate a client recovery operation
    async fn validate_recover_client<HI: HostInterface>(
        &self,
        subject_client_id: &ClientId,
        substitute_client_id: &ClientId,
    ) -> Result<()> {
        tracing::debug!(
            %subject_client_id,
            %substitute_client_id,
            "validating ibc client recovery"
        );

        // 1. Check that the clients are well-formed (regex validation)
        validate_client_id_format(subject_client_id)?;
        validate_client_id_format(substitute_client_id)?;

        // Needed for status checks
        let local_chain_current_time = self
            .get_current_block_timestamp()
            .await
            .context("failed to get current block timestamp")?;

        // 2. Check that the clients are found
        let subject_client_state = self
            .get_client_state(subject_client_id)
            .await
            .context("subject client not found")?;

        let substitute_client_state = self
            .get_client_state(substitute_client_id)
            .await
            .context("substitute client not found")?;

        // 3. Check that the subject client is NOT Active
        let subject_status = self
            .get_client_status(subject_client_id, local_chain_current_time)
            .await;
        ensure!(
            subject_status != ClientStatus::Active,
            "subject client must not be Active, found: {}",
            subject_status
        );

        // 4. Check that the substitute client IS Active
        let substitute_status = self
            .get_client_status(substitute_client_id, local_chain_current_time)
            .await;
        ensure!(
            substitute_status == ClientStatus::Active,
            "substitute client must be Active, found: {}",
            substitute_status
        );

        // 5. Check that all client parameters must match except
        // for the frozen height, latest height, trust period, and proof specs
        check_field_consistency(&subject_client_state, &substitute_client_state)?;

        // 6. Check that the substitute client height is greater than subject's latest height
        let subject_height = subject_client_state
            .latest_height()
            .context("unable to get subject client latest height")?;
        let substitute_height = substitute_client_state
            .latest_height()
            .context("unable to get substitute client latest height")?;
        ensure!(
            substitute_height > subject_height,
            "substitute client height ({}) must be greater than subject client height ({})",
            substitute_height,
            subject_height
        );

        // 7. Perform the recovery: copy substitute client state to subject client
        tracing::debug!("overwriting client state");

        Ok(())
    }
    /// Recover a frozen or expired client by substituting it with an active client.
    ///
    /// This operation will:
    /// 1. Validate both client IDs are well-formed
    /// 2. Verify both clients exist
    /// 3. Check that the subject client is NOT Active
    /// 4. Check that the substitute client IS Active
    /// 5. Verify client parameters match.
    /// 6. Verify substitute client has greater height
    /// 7. Copy the substitute client's state over the subject client
    async fn recover_client<HI: HostInterface>(
        &mut self,
        subject_client_id: &ClientId,
        substitute_client_id: &ClientId,
    ) -> Result<()> {
        tracing::debug!(
            %subject_client_id,
            %substitute_client_id,
            "starting ibc client recovery"
        );
        self.validate_recover_client::<HI>(&subject_client_id, &substitute_client_id)
            .await?;

        let substitute_client_state = self
            .get_client_state(substitute_client_id)
            .await
            .context("substitute client not found")?;

        let substitute_latest_height = substitute_client_state
            .latest_height()
            .context("unable to get substitute client latest height")?;

        let substitute_consensus_state = self
            .get_verified_consensus_state(&substitute_latest_height, &substitute_client_id)
            .await?;

        // smooth brain: we write the substitute - into -> the subject.
        self.put_verified_consensus_state::<HI>(
            substitute_latest_height,
            subject_client_id.clone(),
            substitute_consensus_state,
        )
        .await?;

        self.put_client(&subject_client_id, substitute_client_state);

        tracing::info!(
            subject = %subject_client_id,
            substitute = %substitute_client_id,
            "client recovery completed successfully"
        );

        Ok(())
    }
}

impl<T: StateWrite + ConsensusStateWriteExt> ClientRecoveryExt for T {}

/// Validate that a client ID matches a known format.
/// Accepts: 07-tendermint-<NUM> or bankd-<NUM>
pub fn validate_client_id_format(client_id: &ClientId) -> Result<()> {
    use regex::Regex;

    let client_id_str = client_id.as_str();

    // Match: 07-tendermint-<digits> OR bankd-<digits>
    let re = Regex::new(r"^(07-tendermint|bankd)-\d+$").expect("valid regex");

    ensure!(
        re.is_match(client_id_str),
        "invalid client ID format: '{}'. Expected format: 07-tendermint-<NUM> or bankd-<NUM>",
        client_id_str
    );

    // Check for leading zeros in the number part
    let num_part = client_id_str.rsplit('-').next().expect("split has parts");
    ensure!(
        !(num_part.len() > 1 && num_part.starts_with('0')),
        "invalid client ID: '{}'. Number part cannot have leading zeros",
        client_id_str
    );

    Ok(())
}

/// Check that the fields of two client states are coherent.
///
/// For Tendermint clients, immutable fields (chain_id, trust_level, unbonding_period,
/// max_clock_drift, upgrade_path, allow_update) must match.
/// For bankd clients, chain_id must match.
/// Mismatched client types are rejected.
pub fn check_field_consistency(
    subject: &AnyClientState,
    substitute: &AnyClientState,
) -> Result<()> {
    match (subject, substitute) {
        (AnyClientState::Tendermint(s), AnyClientState::Tendermint(sub)) => {
            ensure!(
                s.chain_id == sub.chain_id,
                "chain IDs must match: subject has '{}', substitute has '{}'",
                s.chain_id,
                sub.chain_id
            );
            ensure!(
                s.trust_level == sub.trust_level,
                "trust levels must match: subject has '{:?}', substitute has '{:?}'",
                s.trust_level,
                sub.trust_level
            );
            ensure!(
                s.unbonding_period == sub.unbonding_period,
                "unbonding periods must match: subject has '{:?}', substitute has '{:?}'",
                s.unbonding_period,
                sub.unbonding_period
            );
            ensure!(
                s.max_clock_drift == sub.max_clock_drift,
                "max clock drifts must match: subject has '{:?}', substitute has '{:?}'",
                s.max_clock_drift,
                sub.max_clock_drift
            );
            ensure!(
                s.upgrade_path == sub.upgrade_path,
                "upgrade paths must match: subject has '{:?}', substitute has '{:?}'",
                s.upgrade_path,
                sub.upgrade_path
            );
            ensure!(
                s.allow_update == sub.allow_update,
                "allow_update flags must match: subject has '{:?}', substitute has '{:?}'",
                s.allow_update,
                sub.allow_update
            );
            Ok(())
        }
        (AnyClientState::Bankd(s), AnyClientState::Bankd(sub)) => {
            ensure!(
                s.chain_id == sub.chain_id,
                "chain IDs must match: subject has '{}', substitute has '{}'",
                s.chain_id,
                sub.chain_id
            );
            Ok(())
        }
        _ => {
            anyhow::bail!("client types must match for recovery (subject and substitute are different types)");
        }
    }
}
