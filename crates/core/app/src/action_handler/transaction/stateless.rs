use anyhow::{Context, Result};
use penumbra_sdk_transaction::{Action, Transaction};
use penumbra_sdk_txhash::AuthorizingData;

#[tracing::instrument(skip(tx))]
pub(super) fn valid_binding_signature(tx: &Transaction) -> Result<()> {
    let auth_hash = tx.auth_hash();

    tracing::debug!(bvk = ?tx.binding_verification_key(), ?auth_hash);

    // Check binding signature.
    tx.binding_verification_key()
        .verify(auth_hash.as_bytes(), tx.binding_sig())
        .context("binding signature failed to verify")
}

pub fn num_clues_equal_to_num_outputs(tx: &Transaction) -> anyhow::Result<()> {
    if tx
        .transaction_body()
        .detection_data
        .unwrap_or_default()
        .fmd_clues
        .len()
        != tx.outputs().count()
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
    let num_outputs = tx.outputs().count();
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

/// Validates the cryptographic binding between spend and output actions.
///
/// For compliance, spends and outputs must be bound through their blinded leaf hashes:
/// - Each spend's `counterparty_leaf_hash` must match some output's `receiver_leaf_hash`
/// - Each output's `counterparty_leaf_hash` must match some spend's `sender_leaf_hash`
///
/// This ensures the same `tx_blinding_nonce` was used and that sender/receiver
/// relationships are cryptographically bound without revealing which compliance
/// leaves are transacting.
pub fn validate_spend_output_binding(tx: &Transaction) -> anyhow::Result<()> {
    // Collect spend and output bodies
    let spends: Vec<_> = tx
        .actions()
        .filter_map(|a| {
            if let Action::Spend(s) = a {
                Some(&s.body)
            } else {
                None
            }
        })
        .collect();

    let outputs: Vec<_> = tx
        .actions()
        .filter_map(|a| {
            if let Action::Output(o) = a {
                Some(&o.body)
            } else {
                None
            }
        })
        .collect();

    // If no spends or outputs, nothing to validate
    if spends.is_empty() || outputs.is_empty() {
        return Ok(());
    }

    // For each spend, verify binding with outputs:
    // spend.counterparty_leaf_hash must match some output.receiver_leaf_hash
    for spend in &spends {
        let has_matching_output = outputs
            .iter()
            .any(|output| spend.counterparty_leaf_hash.0 == output.receiver_leaf_hash.0);

        if !has_matching_output {
            anyhow::bail!("spend counterparty_leaf_hash has no matching output receiver_leaf_hash");
        }
    }

    // For each output, verify binding with spends:
    // output.counterparty_leaf_hash must match some spend.sender_leaf_hash
    for output in &outputs {
        let has_matching_spend = spends
            .iter()
            .any(|spend| output.counterparty_leaf_hash.0 == spend.sender_leaf_hash.0);

        if !has_matching_spend {
            anyhow::bail!("output counterparty_leaf_hash has no matching spend sender_leaf_hash");
        }
    }

    Ok(())
}
