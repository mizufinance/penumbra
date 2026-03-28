use anyhow::{Context, Result};
use ark_ff::Zero;
use async_trait::async_trait;
use cnidarium::StateWrite;
use decaf377::Fr;
use penumbra_sdk_compliance::RegulatedAssetCheck;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_proof_params::DELEGATOR_VOTE_PROOF_VERIFICATION_KEY;
use penumbra_sdk_proto::StateWriteProto as _;
use penumbra_sdk_txhash::TransactionContext;

use crate::{
    event, DelegatorVote, DelegatorVoteBody, DelegatorVoteProofPublic,
    {component::StateWriteExt, StateReadExt},
};
use cnidarium_component::ActionHandler;

pub fn delegator_vote_check_stateless_and_extract(
    action: &DelegatorVote,
    context: &TransactionContext,
) -> Result<BatchItem> {
    let public = DelegatorVoteProofPublic {
        anchor: context.anchor,
        balance_commitment: action.body.value.commit(Fr::zero()),
        nullifier: action.body.nullifier,
        rk: action.body.rk,
        start_position: action.body.start_position,
    };
    action
        .proof
        .to_batch_item(public)
        .map_err(|e| anyhow::anyhow!(e))
}

#[async_trait]
impl ActionHandler for DelegatorVote {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        // 1. Check spend auth signature using provided spend auth key.
        self.body
            .rk
            .verify(context.effect_hash.as_ref(), &self.auth_sig)
            .context("delegator vote auth signature failed to verify")?;

        // 2. Verify the proof against the provided anchor and start position:
        let item = delegator_vote_check_stateless_and_extract(self, &context)?;
        batch::batch_verify(
            &DELEGATOR_VOTE_PROOF_VERIFICATION_KEY,
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("a delegator vote proof did not verify: {e}"))?;

        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        let DelegatorVote {
            body:
                DelegatorVoteBody {
                    proposal,
                    vote,
                    start_position,
                    value,
                    unbonded_amount,
                    nullifier,
                    rk: _, // We already used this to check the auth sig in stateless verification
                },
            auth_sig: _, // We already checked this in stateless verification
            proof: _,    // We already checked this in stateless verification
        } = self;

        // Block regulated delegation tokens from being used to vote
        state
            .ensure_not_regulated(value.asset_id, "DelegatorVote")
            .await?;

        state.check_proposal_votable(*proposal).await?;
        state
            .check_proposal_started_at_position(*proposal, *start_position)
            .await?;
        state
            .check_nullifier_unspent_before_start_block_height(*proposal, nullifier)
            .await?;
        state
            .check_nullifier_unvoted_for_proposal(*proposal, nullifier)
            .await?;
        state
            .check_unbonded_amount_correct_exchange_for_proposal(*proposal, value, unbonded_amount)
            .await?;

        state
            .mark_nullifier_voted_on_proposal(*proposal, nullifier)
            .await;
        let identity_key = state.validator_by_delegation_asset(value.asset_id).await?;
        state
            .cast_delegator_vote(*proposal, identity_key, *vote, nullifier, *unbonded_amount)
            .await?;

        state.record_proto(event::delegator_vote(self, &identity_key));

        Ok(())
    }
}
