use std::str::FromStr;

use anyhow::{Context, Result};
use async_trait::async_trait;
use ibc_types::core::client::ClientId;

use crate::app::StateReadExt;
use crate::{action_handler::AppActionHandler, params::change::ParameterChangeExt as _};
use cnidarium::StateWrite;
use penumbra_sdk_asset::STAKING_TOKEN_DENOM;
use penumbra_sdk_governance::{
    component::{StateReadExt as _, StateWriteExt as _},
    event,
    proposal::{Proposal, ProposalPayload},
    proposal_state::State as ProposalState,
    ProposalNft, ProposalSubmit, VotingReceiptToken,
};
use penumbra_sdk_ibc::component::ClientStateReadExt;
use penumbra_sdk_proto::StateWriteProto as _;
use penumbra_sdk_sct::component::clock::EpochRead;
use penumbra_sdk_sct::component::tree::SctRead;
use penumbra_sdk_shielded_pool::component::AssetRegistry;

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
        let ProposalSubmit {
            proposal,
            deposit_amount: _, // we don't check the deposit amount because it's defined by state
        } = self;
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
            CommunityPoolSpend {
                transaction_plan: _,
            } => {
                anyhow::bail!(
                    "proposal payload disabled in lightweight transfer-only phase: CommunityPoolSpend"
                );
            }
            UpgradePlan { .. } => {}
            FreezeIbcClient { client_id } => {
                let _ = &ClientId::from_str(client_id)
                    .context("can't decode client id from IBC proposal")?;
            }
            UnfreezeIbcClient { client_id } => {
                let _ = &ClientId::from_str(client_id)
                    .context("can't decode client id from IBC proposal")?;
            }
        }

        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        // These checks all formerly happened in the `check_historical` method,
        // if profiling shows that they cause a bottleneck we could (CAREFULLY)
        // move some of them back.

        let ProposalSubmit {
            deposit_amount,
            proposal, // statelessly verified
        } = self;

        // Check that the deposit amount agrees with the parameters
        let governance_parameters = state.get_governance_params().await?;
        if *deposit_amount != governance_parameters.proposal_deposit_amount {
            anyhow::bail!(
                "submitted proposal deposit of {}{} does not match required proposal deposit of {}{}",
                deposit_amount,
                *STAKING_TOKEN_DENOM,
                governance_parameters.proposal_deposit_amount,
                *STAKING_TOKEN_DENOM,
            );
        }

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
            ProposalPayload::CommunityPoolSpend {
                transaction_plan: _,
            } => {
                anyhow::bail!(
                    "proposal payload disabled in lightweight transfer-only phase: CommunityPoolSpend"
                );
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
        }

        // (end of former check_stateful checks)

        let ProposalSubmit {
            proposal,
            deposit_amount,
        } = self;

        // Store the contents of the proposal and generate a fresh proposal id for it
        let proposal_id = state
            .new_proposal(proposal)
            .await
            .context("can create proposal")?;

        // Set the deposit amount for the proposal
        state.put_deposit_amount(proposal_id, *deposit_amount);

        // Register the denom for the voting proposal NFT
        state
            .register_denom(&ProposalNft::deposit(proposal_id).denom())
            .await;

        // Register the denom for the vote receipt tokens
        state
            .register_denom(&VotingReceiptToken::new(proposal_id).denom())
            .await;

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
    use penumbra_sdk_governance::{
        change::ParameterChange, Proposal, ProposalPayload, ProposalSubmit,
    };

    use crate::action_handler::AppActionHandler;

    #[tokio::test]
    async fn parameter_change_proposals_remain_enabled_statelessly() {
        ProposalSubmit {
            proposal: Proposal {
                id: 0,
                title: "parameter change".to_owned(),
                description: "kept enabled in lightweight mode".to_owned(),
                payload: ProposalPayload::ParameterChange(ParameterChange {
                    changes: vec![],
                    preconditions: vec![],
                }),
            },
            deposit_amount: 0u32.into(),
        }
        .check_stateless(())
        .await
        .expect("parameter change proposals should remain enabled");
    }

    #[tokio::test]
    async fn community_pool_spend_proposals_are_disabled_statelessly() {
        let err = ProposalSubmit {
            proposal: Proposal {
                id: 0,
                title: "community pool spend".to_owned(),
                description: "disabled in lightweight mode".to_owned(),
                payload: ProposalPayload::CommunityPoolSpend {
                    transaction_plan: vec![],
                },
            },
            deposit_amount: 0u32.into(),
        }
        .check_stateless(())
        .await
        .expect_err("community pool spend proposals should be disabled");

        assert_eq!(
            err.to_string(),
            "proposal payload disabled in lightweight transfer-only phase: CommunityPoolSpend"
        );
    }
}
