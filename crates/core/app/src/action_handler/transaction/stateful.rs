use anyhow::{ensure, Result};
use cnidarium::StateRead;
use penumbra_sdk_sct::component::tree::VerificationExt;
use penumbra_sdk_shielded_pool::fmd;
use penumbra_sdk_transaction::{Transaction, TransactionParameters};

use super::HistoricalCheckContext;

pub fn tx_parameters_historical_check_with_context(
    transaction: &Transaction,
    context: &HistoricalCheckContext,
) -> Result<()> {
    let TransactionParameters {
        chain_id,
        expiry_height,
        fee: _,
    } = transaction.transaction_parameters();

    ensure!(
        chain_id == context.chain_id,
        "transaction chain ID '{}' must match the current chain ID '{}'",
        chain_id,
        context.chain_id
    );

    if expiry_height != 0 {
        ensure!(
            expiry_height >= context.block_height,
            "transaction expiry height '{}' must be greater than or equal to the current block height '{}'",
            expiry_height,
            context.block_height
        );
    }

    Ok(())
}

pub fn fmd_parameters_valid_with_context(
    transaction: &Transaction,
    context: &HistoricalCheckContext,
) -> Result<()> {
    fmd_precision_within_grace_period(
        transaction,
        context.fmd_meta_params,
        context.previous_fmd_parameters.clone(),
        context.current_fmd_parameters.clone(),
        context.block_height,
    )
}

#[tracing::instrument(
    skip_all,
    fields(
        current_fmd.precision_bits = current_fmd_parameters.precision.bits(),
        previous_fmd.precision_bits = previous_fmd_parameters.precision.bits(),
        previous_fmd.as_of_block_height = previous_fmd_parameters.as_of_block_height,
        block_height,
    )
)]
pub fn fmd_precision_within_grace_period(
    tx: &Transaction,
    meta_params: fmd::MetaParameters,
    previous_fmd_parameters: fmd::Parameters,
    current_fmd_parameters: fmd::Parameters,
    block_height: u64,
) -> anyhow::Result<()> {
    for clue in tx
        .transaction_body()
        .detection_data
        .unwrap_or_default()
        .fmd_clues
    {
        // Clue must be using the current `fmd::Parameters`, or be within
        // `fmd_grace_period_blocks` of the previous `fmd::Parameters`.
        let clue_precision = clue.precision()?;
        let using_current_precision = clue_precision == current_fmd_parameters.precision;
        let using_previous_precision = clue_precision == previous_fmd_parameters.precision;
        let within_grace_period = block_height
            < previous_fmd_parameters.as_of_block_height + meta_params.fmd_grace_period_blocks;
        if using_current_precision || (using_previous_precision && within_grace_period) {
            continue;
        } else {
            tracing::error!(
                %clue_precision,
                %using_current_precision,
                %using_previous_precision,
                %within_grace_period,
                "invalid clue precision"
            );
            anyhow::bail!("consensus rule violated: invalid clue precision");
        }
    }
    Ok(())
}

pub async fn claimed_anchor_is_valid<S: StateRead>(
    state: S,
    transaction: &Transaction,
) -> Result<()> {
    state.check_claimed_anchor(transaction.anchor).await
}
