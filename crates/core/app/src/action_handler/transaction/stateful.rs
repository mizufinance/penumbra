use anyhow::{ensure, Result};
use cnidarium::StateRead;
use shieldd_sdk_sct::component::tree::VerificationExt;
use shieldd_sdk_shielded_pool::discovery;
use shieldd_sdk_transaction::{Action, Transaction, TransactionParameters};

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

pub fn nullifier_window_valid_with_context(
    transaction: &Transaction,
    context: &HistoricalCheckContext,
) -> Result<()> {
    if transaction.spent_nullifier_count() == 0 {
        ensure!(
            transaction.transaction_body.nullifier_window.is_none(),
            "spend-free transaction must not carry a nullifier window"
        );
        return Ok(());
    }
    ensure!(
        transaction.transaction_body.nullifier_window == Some(context.nullifier_window),
        "transaction nullifier window does not match the current consensus window"
    );
    Ok(())
}

pub fn discovery_parameters_valid_with_context(
    transaction: &Transaction,
    context: &HistoricalCheckContext,
) -> Result<()> {
    routing_parameters_within_grace_period(
        transaction,
        context.discovery_grace_period_blocks,
        context.previous_discovery_parameters.clone(),
        context.current_discovery_parameters.clone(),
        context.block_height,
    )
}

#[tracing::instrument(
    skip_all,
    fields(
        current_discovery.regulated_precision_bits = current_discovery_parameters.regulated_precision.bits(),
        current_discovery.unregulated_precision_bits = current_discovery_parameters.unregulated_precision.bits(),
        previous_discovery.regulated_precision_bits = previous_discovery_parameters.regulated_precision.bits(),
        previous_discovery.unregulated_precision_bits = previous_discovery_parameters.unregulated_precision.bits(),
        current_discovery.as_of_height = current_discovery_parameters.as_of_height,
        block_height,
    )
)]
pub fn routing_parameters_within_grace_period(
    tx: &Transaction,
    grace_period_blocks: u64,
    previous_discovery_parameters: discovery::Parameters,
    current_discovery_parameters: discovery::Parameters,
    block_height: u64,
) -> anyhow::Result<()> {
    let mut parameter_set_ids = Vec::new();
    for action in tx.actions() {
        match action {
            Action::Transfer(transfer) => {
                parameter_set_ids.push(transfer.body.routing_parameter_set_id);
            }
            Action::NoteReshape(reshape) => {
                parameter_set_ids.push(reshape.body.routing_parameter_set_id);
            }

            Action::ShieldedHostWithdrawal(withdrawal) => {
                parameter_set_ids.push(withdrawal.body.routing_parameter_set_id);
            }
            _ => {}
        }
    }
    let transaction_body = tx.transaction_body();
    if let Some(fee_funding) = &transaction_body.fee_funding {
        parameter_set_ids.push(fee_funding.transfer.body.routing_parameter_set_id);
    }

    for parameter_set_id in parameter_set_ids {
        if current_discovery_parameters.accepted_at(
            &previous_discovery_parameters,
            grace_period_blocks,
            block_height,
            parameter_set_id,
        ) {
            continue;
        } else {
            let using_current_parameters = parameter_set_id == current_discovery_parameters.id();
            let using_previous_parameters = parameter_set_id == previous_discovery_parameters.id();
            let within_grace_period = block_height
                < current_discovery_parameters
                    .as_of_height
                    .saturating_add(grace_period_blocks);
            tracing::error!(
                parameter_set_id = %hex::encode(parameter_set_id.to_bytes()),
                %using_current_parameters,
                %using_previous_parameters,
                %within_grace_period,
                "invalid routing parameter set"
            );
            anyhow::bail!("consensus rule violated: invalid routing parameter set");
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
