use anyhow::{Context, Result};
use penumbra_sdk_transaction::{Action, Transaction};
use penumbra_sdk_txhash::AuthorizingData;

fn note_creating_output_count(tx: &Transaction) -> usize {
    let action_outputs = tx
        .actions()
        .map(|action| match action {
            Action::Transfer(transfer) => transfer
                .body
                .outputs
                .iter()
                .filter(|output| !output.is_dummy())
                .count(),
            Action::Consolidate(consolidate) => consolidate.body.outputs.len(),
            Action::Split(split) => split.body.outputs.len(),
            Action::ShieldedIcs20Withdrawal(_) => 1,
            _ => 0,
        })
        .sum::<usize>();

    let fee_outputs = tx
        .transaction_body()
        .fee_funding
        .map(|fee_funding| {
            fee_funding
                .transfer
                .body
                .outputs
                .iter()
                .filter(|output| !output.is_dummy())
                .count()
        })
        .unwrap_or_default();

    action_outputs + fee_outputs
}

#[tracing::instrument(skip(tx))]
pub(crate) fn valid_binding_signature(tx: &Transaction) -> Result<()> {
    let auth_hash = tx.auth_hash();

    tracing::debug!(bvk = ?tx.binding_verification_key(), ?auth_hash);

    // Check binding signature.
    tx.binding_verification_key()
        .verify(auth_hash.as_bytes(), tx.binding_sig())
        .context("binding signature failed to verify")
}

pub fn num_clues_equal_to_num_outputs(tx: &Transaction) -> anyhow::Result<()> {
    let num_note_creating_actions = note_creating_output_count(tx);
    if tx
        .transaction_body()
        .detection_data
        .unwrap_or_default()
        .fmd_clues
        .len()
        != num_note_creating_actions
    {
        Err(anyhow::anyhow!(
            "consensus rule violated: must have equal number of outputs and FMD clues"
        ))
    } else {
        Ok(())
    }
}

#[allow(clippy::if_same_then_else)]
pub fn check_memo_exists_if_outputs_absent_if_not(tx: &Transaction) -> anyhow::Result<()> {
    let num_outputs = note_creating_output_count(tx);
    if num_outputs > 0 && tx.transaction_body().memo.is_none() {
        Err(anyhow::anyhow!(
            "consensus rule violated: must have memo if outputs present"
        ))
    } else if num_outputs > 0 && tx.transaction_body().memo.is_some() {
        Ok(())
    } else if num_outputs == 0 && tx.transaction_body().memo.is_none() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "consensus rule violated: cannot have memo if no outputs present"
        ))
    }
}

pub fn check_non_empty_transaction(tx: &Transaction) -> anyhow::Result<()> {
    let num_actions = tx.actions().count();
    if num_actions > 0 {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "consensus rule violated: transaction must have more than 0 actions"
        ))
    }
}
