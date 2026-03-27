use anyhow::{ensure, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use penumbra_sdk_asset::STAKING_TOKEN_ASSET_ID;
use penumbra_sdk_compliance::RegulatedAssetCheck;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_proof_params::CONVERT_PROOF_VERIFICATION_KEY;
use penumbra_sdk_sct::component::clock::EpochRead;

use crate::component::validator_handler::ValidatorDataRead;
use crate::component::SlashingData;
use crate::undelegate_claim::UndelegateClaimProofPublic;
use crate::UndelegateClaim;
use crate::{component::action_handler::ActionHandler, UnbondingToken};

pub fn undelegate_claim_check_stateless_and_extract(action: &UndelegateClaim) -> Result<BatchItem> {
    let unbonding_id = UnbondingToken::new(
        action.body.validator_identity,
        action.body.unbonding_start_height,
    )
    .id();
    action.proof.to_batch_item(UndelegateClaimProofPublic {
        balance_commitment: action.body.balance_commitment,
        unbonding_id,
        penalty: action.body.penalty,
    })
}

#[async_trait]
impl ActionHandler for UndelegateClaim {
    type CheckStatelessContext = ();
    async fn check_stateless(&self, _context: ()) -> Result<()> {
        let item = undelegate_claim_check_stateless_and_extract(self)?;
        batch::batch_verify(&CONVERT_PROOF_VERIFICATION_KEY, std::slice::from_ref(&item))
            .map_err(|e| anyhow::anyhow!("undelegate claim proof did not verify: {e}"))?;

        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, state: S) -> Result<()> {
        // Defensive check - STAKING_TOKEN should always be unregulated
        state
            .ensure_not_regulated(*STAKING_TOKEN_ASSET_ID, "UndelegateClaim")
            .await?;

        // These checks all formerly happened in the `check_historical` method,
        // if profiling shows that they cause a bottleneck we could (CAREFULLY)
        // move some of them back.

        let current_height = state.get_block_height().await?;
        let unbonding_start_height = self.body.unbonding_start_height;
        ensure!(
            current_height >= unbonding_start_height,
            "the unbonding start height must be less than or equal to the current height"
        );

        // Compute the unbonding height for the claim, and check that it is less than or equal to the current height.
        // If the pool is `Unbonded` or unbonding at an already elapsed height, we default to the current height.
        let allowed_unbonding_height = state
            .compute_unbonding_height(&self.body.validator_identity, unbonding_start_height)
            .await?
            .unwrap_or(current_height);

        let wait_blocks = allowed_unbonding_height.saturating_sub(current_height);

        ensure!(
            current_height >= allowed_unbonding_height,
            "cannot claim unbonding tokens before height {} (currently at {}, wait {} blocks)",
            allowed_unbonding_height,
            current_height,
            wait_blocks
        );

        let unbonding_epoch_start = state
            .get_epoch_by_height(self.body.unbonding_start_height)
            .await?;
        let unbonding_epoch_end = state.get_epoch_by_height(allowed_unbonding_height).await?;

        // This should never happen, but if it did we want to make sure that it wouldn't
        // crash the mempool.
        ensure!(
            unbonding_epoch_end.index >= unbonding_epoch_start.index,
            "unbonding epoch end must be greater than or equal to unbonding epoch start"
        );

        // Compute the penalty for the epoch range [unbonding_epoch_start, unbonding_epoch_end], and check
        // that it matches the penalty in the claim.
        let expected_penalty = state
            .compounded_penalty_over_range(
                &self.body.validator_identity,
                unbonding_epoch_start.index,
                unbonding_epoch_end.index,
            )
            .await?;

        ensure!(
            self.body.penalty == expected_penalty,
            "penalty (kept_rate: {}) does not match expected penalty (kept_rate: {})",
            self.body.penalty.kept_rate(),
            expected_penalty.kept_rate(),
        );

        /* ---------- execution ----------- */
        // No state changes here - this action just converts one token to another

        Ok(())
    }
}
