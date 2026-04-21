use std::collections::BTreeMap;

use anyhow::Context;
use rand_core::{CryptoRng, RngCore};
use tracing::instrument;

use penumbra_sdk_keys::keys::AddressIndex;
use penumbra_sdk_proto::view::v1::NotesRequest;
use penumbra_sdk_transaction::TransactionPlan;
use penumbra_sdk_view::{NoteManager, SpendableNoteRecord, TransferPlanningResult, ViewClient};

#[instrument(skip(view, rng))]
pub async fn sweep<V, R>(view: &mut V, mut rng: R) -> anyhow::Result<Vec<TransactionPlan>>
where
    V: ViewClient + Send,
    R: RngCore + CryptoRng,
{
    let mut plans = Vec::new();

    // Sweep dust notes by consolidating them into larger notes.
    plans.extend(sweep_notes(view, &mut rng).await?);

    Ok(plans)
}

#[instrument(skip(view, rng))]
async fn sweep_notes<V, R>(view: &mut V, mut rng: R) -> anyhow::Result<Vec<TransactionPlan>>
where
    V: ViewClient + Send,
    R: RngCore + CryptoRng,
{
    let gas_prices = view.gas_prices().await?;

    let all_notes = view
        .notes(NotesRequest {
            ..Default::default()
        })
        .await?;

    let mut notes_by_addr_and_denom: BTreeMap<AddressIndex, BTreeMap<_, Vec<SpendableNoteRecord>>> =
        BTreeMap::new();

    for record in all_notes {
        notes_by_addr_and_denom
            .entry(record.address_index)
            .or_default()
            .entry(record.note.asset_id())
            .or_default()
            .push(record);
    }

    let mut plans = Vec::new();

    for (index, notes_by_denom) in notes_by_addr_and_denom {
        tracing::info!(?index, "processing address");

        for (asset_id, mut records) in notes_by_denom {
            tracing::debug!(?asset_id, "processing asset");

            // Sort notes by amount, ascending, so the biggest notes are at the end...
            records.sort_by(|a, b| a.note.value().amount.cmp(&b.note.value().amount));
            if records.len() < 2 {
                continue;
            }

            let mut note_manager = NoteManager::new(&mut rng);
            note_manager.set_gas_prices(gas_prices);

            let planning_result = note_manager
                .plan_consolidate(view, index, asset_id, None)
                .await
                .context("can't build sweep transaction")?;

            let plan = match planning_result {
                TransferPlanningResult::Ready { transaction_plan } => transaction_plan,
                TransferPlanningResult::NeedsMaintenance { .. } => continue,
                TransferPlanningResult::InsufficientBalance => continue,
                TransferPlanningResult::UnsupportedIntent { reason } => {
                    tracing::debug!(?asset_id, ?reason, "skipping unsupported sweep intent");
                    continue;
                }
            };

            tracing::debug!(?plan);
            plans.push(plan);
        }
    }

    Ok(plans)
}
