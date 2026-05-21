use std::str::FromStr;

use anyhow::{Context, Result};
use async_trait::async_trait;
use ibc_types::core::client::ClientId;

use crate::app::StateReadExt;
use crate::{action_handler::AppActionHandler, params::change::ParameterChangeExt as _};
use cnidarium::StateWrite;
use penumbra_sdk_governance::{
    component::{StateReadExt as _, StateWriteExt as _},
    event,
    proposal::{Proposal, ProposalPayload},
    proposal_state::State as ProposalState,
    ProposalSubmit, ProposalSubmitBody,
};
use penumbra_sdk_ibc::component::ClientStateReadExt;
use penumbra_sdk_proto::{DomainType, Message as _, StateWriteProto as _};
use penumbra_sdk_sct::component::clock::EpochRead;
use penumbra_sdk_sct::component::tree::SctRead;

// IMPORTANT: these length limits are enforced by consensus! Changing them will change which
// transactions are accepted by the network, and so they *cannot* be changed without a network
// upgrade!

// This is enough room to print "Proposal #999,999: $TITLE" in 99 characters (and the
// proposal title itself in 80), a decent line width for a modern terminal, as well as a
// reasonable length for other interfaces.
pub const PROPOSAL_TITLE_LIMIT: usize = 80; // ⚠️ DON'T CHANGE THIS (see above)!

// Limit the size of a description to 10,000 characters (a reasonable limit borrowed from
// the Cosmos SDK).
pub const PROPOSAL_DESCRIPTION_LIMIT: usize = 10_000; // ⚠️ DON'T CHANGE THIS (see above)!

#[async_trait]
impl AppActionHandler for ProposalSubmit {
    type CheckStatelessContext = ();
    async fn check_stateless(&self, _context: ()) -> Result<()> {
        let ProposalSubmitBody {
            proposal,
            proposer: _,
            governance_key,
        } = &self.body;
        let Proposal {
            id: _, // we can't check the ID statelessly because it's defined by state
            title,
            description,
            payload,
        } = proposal;

        if title.len() > PROPOSAL_TITLE_LIMIT {
            anyhow::bail!("proposal title must fit within {PROPOSAL_TITLE_LIMIT} characters");
        }

        if description.len() > PROPOSAL_DESCRIPTION_LIMIT {
            anyhow::bail!(
                "proposal description must fit within {PROPOSAL_DESCRIPTION_LIMIT} characters"
            );
        }

        use penumbra_sdk_governance::ProposalPayload::*;
        match payload {
            Signaling { commit: _ } => { /* all signaling proposals are valid */ }
            Emergency { halt_chain: _ } => { /* all emergency proposals are valid */ }
            ParameterChange(_change) => { /* no stateless checks -- see check-and-execute below */ }
            UpgradePlan { .. } => {}
            FreezeIbcClient { client_id } => {
                let _ = &ClientId::from_str(client_id)
                    .context("can't decode client id from IBC proposal")?;
            }
            UnfreezeIbcClient { client_id } => {
                let _ = &ClientId::from_str(client_id)
                    .context("can't decode client id from IBC proposal")?;
            }
            UpdateAssetIbcPolicy(_) => {}
        }

        governance_key
            .0
            .verify(&self.body.to_proto().encode_to_vec(), &self.auth_sig)
            .context("proposal submission signature failed to verify")?;

        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        // These checks all formerly happened in the `check_historical` method,
        // if profiling shows that they cause a bottleneck we could (CAREFULLY)
        // move some of them back.

        let ProposalSubmitBody {
            proposal,
            proposer,
            governance_key,
        } = &self.body;

        state.check_validator_is_active(proposer).await?;
        state
            .check_governance_key_matches_validator(proposer, governance_key)
            .await?;

        // Check that the proposal ID is the correct next proposal ID
        let next_proposal_id = state.next_proposal_id().await?;
        if proposal.id != next_proposal_id {
            anyhow::bail!(
                "submitted proposal ID {} does not match expected proposal ID {}",
                proposal.id,
                next_proposal_id,
            );
        }

        match &proposal.payload {
            ProposalPayload::Signaling { .. } => { /* no stateful checks for signaling */ }
            ProposalPayload::Emergency { .. } => { /* no stateful checks for emergency */ }
            ProposalPayload::ParameterChange(change) => {
                // Check that the parameter change is valid and could be applied to the current
                // parameters. This doesn't guarantee that it will be valid when/if it passes but
                // ensures that clearly malformed proposals are rejected upfront.
                let current_parameters = state.get_app_params().await?;
                change
                    .apply_changes(current_parameters)
                    .context("proposed parameter changes do not apply to current parameters")?;
            }
            ProposalPayload::UpgradePlan { .. } => {
                // TODO(erwan): no stateful checks for upgrade plan.
            }
            ProposalPayload::FreezeIbcClient { client_id } => {
                // Check that the client ID is valid and that there is a corresponding
                // client state. If the client state is already frozen, then freezing it
                // is a no-op.
                let client_id = &ClientId::from_str(client_id)
                    .map_err(|e| tonic::Status::aborted(format!("invalid client id: {e}")))?;
                let _ = state.get_client_state(client_id).await?;
            }
            ProposalPayload::UnfreezeIbcClient { client_id } => {
                // Check that the client ID is valid and that there is a corresponding
                // client state. If the client state is not frozen, then unfreezing it
                // is a no-op.
                let client_id = &ClientId::from_str(client_id)
                    .map_err(|e| tonic::Status::aborted(format!("invalid client id: {e}")))?;
                let _ = state.get_client_state(client_id).await?;
            }
            ProposalPayload::UpdateAssetIbcPolicy(update) => {
                use penumbra_sdk_compliance::ComplianceRegistryRead as _;
                state
                    .get_asset_policy(update.asset_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("asset is not regulated"))?;
            }
        }

        // (end of former check_stateful checks)

        // Store the contents of the proposal and generate a fresh proposal id for it
        let proposal_id = state
            .new_proposal(proposal)
            .await
            .context("can create proposal")?;

        state.put_proposal_submitter(proposal_id, *proposer);

        // Set the proposal state to voting (votes start immediately)
        state.put_proposal_state(proposal_id, ProposalState::Voting);

        // Determine what block it is currently, and calculate when the proposal should start voting
        // (now!) and finish voting (later...), then write that into the state
        let governance_params = state
            .get_governance_params()
            .await
            .context("can get chain params")?;
        let current_block = state
            .get_block_height()
            .await
            .context("can get block height")?;
        let voting_end = current_block + governance_params.proposal_voting_blocks;
        state.put_proposal_voting_start(proposal_id, current_block);
        state.put_proposal_voting_end(proposal_id, voting_end);

        // Compute the effective starting TCT position for the proposal, by rounding the current
        // position down to the start of the block.
        let Some(sct_position) = state.get_sct().await.position() else {
            anyhow::bail!("state commitment tree is full");
        };
        // All proposals start are considered to start at the beginning of the block, because this
        // means there are no ordering games to be played within the block in which a proposal begins:
        let proposal_start_position = (sct_position.epoch(), sct_position.block(), 0).into();
        state.put_proposal_voting_start_position(proposal_id, proposal_start_position);

        // Since there was a proposal submitted, ensure we track this so that clients can retain
        // state needed to vote as delegators
        state.mark_proposal_started();

        state.record_proto(event::proposal_submit(self, current_block, voting_end));

        tracing::debug!(proposal = %proposal_id, "created proposal");

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use penumbra_sdk_governance::{
        change::ParameterChange, Proposal, ProposalPayload, ProposalSubmit, ProposalSubmitBody,
    };
    use penumbra_sdk_proto::{DomainType, Message};
    use penumbra_sdk_validator::{GovernanceKey, IdentityKey};
    use rand_core::OsRng;

    use crate::action_handler::AppActionHandler;

    fn test_proposal_submit(payload: ProposalPayload) -> ProposalSubmit {
        let sk = SigningKey::<SpendAuth>::new(OsRng);
        let vk = VerificationKey::from(&sk);
        let body = ProposalSubmitBody {
            proposal: Proposal {
                id: 0,
                title: "parameter change".to_owned(),
                description: "parameter change remains supported".to_owned(),
                payload,
            },
            proposer: IdentityKey(vk.into()),
            governance_key: GovernanceKey(sk.into()),
        };
        let auth_sig = sk.sign(OsRng, &body.to_proto().encode_to_vec());
        ProposalSubmit { body, auth_sig }
    }

    #[tokio::test]
    async fn parameter_change_proposals_remain_enabled_statelessly() {
        test_proposal_submit(ProposalPayload::ParameterChange(ParameterChange {
            changes: vec![],
            preconditions: vec![],
        }))
        .check_stateless(())
        .await
        .expect("parameter change proposals should remain enabled");
    }
}
