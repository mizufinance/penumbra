use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use ibc_types::core::client::ClientId;
use once_cell::sync::Lazy;
use penumbra_sdk_sct::component::clock::EpochRead;
use regex::Regex;

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
static CLIENT_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(07-tendermint|bankd)-\d+$").expect("valid regex"));

pub fn validate_client_id_format(client_id: &ClientId) -> Result<()> {
    let client_id_str = client_id.as_str();

    ensure!(
        CLIENT_ID_RE.is_match(client_id_str),
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
            anyhow::bail!(
                "client types must match for recovery (subject and substitute are different types)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use ibc_types::DomainType as _;
    use std::str::FromStr;

    #[test]
    fn validate_tendermint_client_id() {
        let id = ClientId::from_str("07-tendermint-0").expect("valid client id");
        validate_client_id_format(&id).expect("should accept 07-tendermint-0");
    }

    #[test]
    fn validate_tendermint_client_id_large_number() {
        let id = ClientId::from_str("07-tendermint-999").expect("valid client id");
        validate_client_id_format(&id).expect("should accept 07-tendermint-999");
    }

    #[test]
    fn validate_bankd_client_id() {
        // ClientId requires minimum 9 chars, so "bankd-100" is the shortest valid bankd ID
        let id = ClientId::from_str("bankd-100").expect("valid client id");
        validate_client_id_format(&id).expect("should accept bankd-100");
    }

    #[test]
    fn validate_bankd_client_id_large_number() {
        let id = ClientId::from_str("bankd-9999").expect("valid client id");
        validate_client_id_format(&id).expect("should accept bankd-9999");
    }

    #[test]
    fn reject_unknown_client_type() {
        let id = ClientId::from_str("08-wasm-0").expect("valid client id");
        let err = validate_client_id_format(&id).unwrap_err();
        assert!(err.to_string().contains("invalid client ID format"));
    }

    #[test]
    fn reject_leading_zeros() {
        let id = ClientId::from_str("07-tendermint-01").expect("valid client id");
        let err = validate_client_id_format(&id).unwrap_err();
        assert!(err.to_string().contains("leading zeros"));
    }

    #[test]
    fn reject_bankd_leading_zeros() {
        let id = ClientId::from_str("bankd-007").expect("valid client id");
        let err = validate_client_id_format(&id).unwrap_err();
        assert!(err.to_string().contains("leading zeros"));
    }

    #[test]
    fn check_field_consistency_bankd_same_chain() {
        use crate::client_types::{AnyClientState, BankdClientState};

        let a = AnyClientState::Bankd(BankdClientState {
            chain_id: "bankd-testnet-1".to_string(),
            latest_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: 10,
            }),
            frozen_height: None,
            proof_specs: vec![],
        });
        let b = AnyClientState::Bankd(BankdClientState {
            chain_id: "bankd-testnet-1".to_string(),
            latest_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: 20,
            }),
            frozen_height: None,
            proof_specs: vec![],
        });
        check_field_consistency(&a, &b).expect("same chain_id should pass");
    }

    #[test]
    fn check_field_consistency_bankd_different_chain_rejected() {
        use crate::client_types::{AnyClientState, BankdClientState};

        let a = AnyClientState::Bankd(BankdClientState {
            chain_id: "bankd-testnet-1".to_string(),
            latest_height: None,
            frozen_height: None,
            proof_specs: vec![],
        });
        let b = AnyClientState::Bankd(BankdClientState {
            chain_id: "bankd-mainnet-1".to_string(),
            latest_height: None,
            frozen_height: None,
            proof_specs: vec![],
        });
        let err = check_field_consistency(&a, &b).unwrap_err();
        assert!(err.to_string().contains("chain IDs must match"));
    }

    #[test]
    fn check_field_consistency_mixed_types_rejected() {
        use crate::client_types::{AnyClientState, BankdClientState};

        let bankd = AnyClientState::Bankd(BankdClientState {
            chain_id: "test".to_string(),
            latest_height: None,
            frozen_height: None,
            proof_specs: vec![],
        });

        // Build a Tendermint client state from the fixture
        let raw = base64::prelude::BASE64_STANDARD
            .decode(include_str!("test/create_client.msg").replace('\n', ""))
            .expect("valid base64");
        let msg =
            ibc_types::core::client::msgs::MsgCreateClient::decode(raw.as_slice()).expect("valid");
        let tm_cs = ibc_types::lightclients::tendermint::client_state::ClientState::try_from(
            msg.client_state,
        )
        .expect("valid");
        let tm = AnyClientState::Tendermint(tm_cs);

        let err = check_field_consistency(&bankd, &tm).unwrap_err();
        assert!(err.to_string().contains("client types must match"));
    }
}
