use std::collections::BTreeMap;

use shieldd_sdk_compact_block::{CompactBlock, StatePayload};
use shieldd_sdk_compliance::effective_nullifier_key;
use shieldd_sdk_fee::GasPrices;
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_sct::{nullifier_generation::NullifierWindow, Nullifier};
use shieldd_sdk_shielded_pool::{
    discovery, Note, NotePayload, VolumeAccumulatorPayload, VolumeAccumulatorState,
};
use shieldd_sdk_tct::{self as tct, StateCommitment};
use tracing::Instrument;

use crate::{storage::ComplianceBlockPlan, SpendableNoteRecord, Storage};

const SCT_BLOCK_CAPACITY: usize = u16::MAX as usize + 1;

/// Contains the results of scanning a single block.
#[derive(Debug, Clone)]
pub struct FilteredBlock {
    pub new_notes: BTreeMap<StateCommitment, SpendableNoteRecord>,
    pub spent_nullifiers: Vec<Nullifier>,
    pub height: u64,
    pub discovery_parameters: Option<discovery::Parameters>,
    pub app_parameters_updated: bool,
    pub gas_prices: Option<GasPrices>,
    pub nullifier_window: Option<NullifierWindow>,
    pub volume_accumulators: Vec<RecoveredVolumeAccumulator>,
}

#[derive(Debug, Clone)]
pub struct RecoveredVolumeAccumulator {
    pub payload: VolumeAccumulatorPayload,
    pub state: VolumeAccumulatorState,
    pub position: tct::Position,
}

#[tracing::instrument(skip_all, fields(height = %height))]
pub async fn scan_block(
    fvk: &FullViewingKey,
    state_commitment_tree: &mut tct::Tree,
    CompactBlock {
        height,
        state_payloads,
        nullifiers,
        block_root,
        epoch_root,
        discovery_parameters,
        app_parameters_updated,
        gas_prices,
        nullifier_window,
        // TODO: do we need this, or is there a bug in scan_block?
        // proposal_started,
        ..
    }: CompactBlock,
    storage: &Storage,
    compliance_plan: Option<&ComplianceBlockPlan>,
) -> anyhow::Result<FilteredBlock> {
    // Trial-decrypt a note with our own specific viewing key
    let trial_decrypt_note = |note_payload: NotePayload| -> tokio::task::JoinHandle<Option<Note>> {
        // TODO: change fvk to Arc<FVK> in Worker and pass to scan_block as Arc
        // need this so the task is 'static and not dependent on key lifetime
        let fvk2 = fvk.clone();
        tokio::spawn(
            async move { note_payload.trial_decrypt(&fvk2) }.instrument(tracing::Span::current()),
        )
    };
    // Nullifiers we've found in this block
    let spent_nullifiers: Vec<Nullifier> = nullifiers;

    // Trial-decrypt the notes in this block, keeping track of the ones that were meant for us
    let mut note_decryptions = Vec::new();
    let mut unknown_commitments = Vec::new();
    let mut volume_advice = BTreeMap::new();

    for payload in state_payloads.iter() {
        match payload {
            StatePayload::Note { note, .. } => {
                note_decryptions.push(trial_decrypt_note((**note).clone()));
            }
            StatePayload::RolledUp { commitment, .. } => unknown_commitments.push(*commitment),
            StatePayload::VolumeAccumulator { payload, .. } => {
                match payload.trial_decrypt(fvk.outgoing()) {
                    Some((state, true)) => {
                        volume_advice.insert(payload.commitment, ((**payload).clone(), state));
                    }
                    _ => unknown_commitments.push(payload.commitment),
                }
            }
        }
    }
    // Having started trial decryption in the background, ask the Storage for scanning advice:
    let mut note_advice = storage.scan_advice(unknown_commitments).await?;
    for decryption in note_decryptions {
        if let Some(note) = decryption
            .await
            .expect("able to join tokio note decryption handle")
        {
            note_advice.insert(note.commit(), note);
        }
    }
    // Newly detected spendable notes.
    let mut new_notes = BTreeMap::new();
    let mut volume_accumulators = Vec::new();

    if note_advice.is_empty() && volume_advice.is_empty() {
        // If there are no notes we care about in this block, just insert the block root into the
        // tree instead of processing each commitment individually
        state_commitment_tree
            .insert_block(block_root)
            .expect("inserting a block root must succeed");
    } else {
        // If we found at least one note for us in this block, we have to explicitly construct the
        // whole block in the SCT by inserting each commitment one at a time
        tracing::debug!("found at least one relevant SCT entry, reconstructing block subtree");
        let mut commitments_in_reconstructed_block = 0usize;

        for payload in state_payloads.into_iter() {
            if commitments_in_reconstructed_block == SCT_BLOCK_CAPACITY {
                state_commitment_tree
                    .end_block()
                    .expect("ending a reconstructed SCT block must succeed");
                commitments_in_reconstructed_block = 0;
            }

            // We need to insert each commitment, so use a match statement to ensure we
            // exhaustively cover all possible cases.
            if let Some((volume_payload, state)) = volume_advice.remove(payload.commitment()) {
                let position = state_commitment_tree
                    .insert(tct::Witness::Keep, *payload.commitment())
                    .expect("inserting a volume accumulator commitment must succeed");
                volume_accumulators.push(RecoveredVolumeAccumulator {
                    payload: volume_payload,
                    state,
                    position,
                });
            } else {
                match note_advice.get(payload.commitment()) {
                    Some(note) => {
                        // Keep track of this commitment for later witnessing
                        let position = state_commitment_tree
                            .insert(tct::Witness::Keep, *payload.commitment())
                            .expect("inserting a commitment must succeed");

                        let source = payload.source().clone();
                        let policy = match compliance_plan
                            .and_then(|plan| plan.asset_policy(note.asset_id()))
                        {
                            Some(policy) => Some(policy.clone()),
                            None => storage.get_asset_policy(&note.asset_id()).await?,
                        };
                        let nk = match policy {
                            Some(policy) => {
                                let pending_leaf = compliance_plan
                                    .and_then(|plan| {
                                        plan.user_leaf(&note.address(), note.asset_id())
                                    })
                                    .cloned();
                                let rnk_dh_pk = match pending_leaf {
                                    Some(leaf) => leaf.rnk_dh_pk,
                                    None => {
                                        let leaf = storage
                                            .get_compliance_leaf_data(
                                                &note.address(),
                                                &note.asset_id(),
                                            )
                                            .await?
                                            .ok_or_else(|| {
                                                anyhow::anyhow!(
                                                    "regulated note is missing its compliance leaf"
                                                )
                                            })?;
                                        decaf377::Encoding(leaf.rnk_dh_pk)
                                            .vartime_decompress()
                                            .map_err(|_| {
                                                anyhow::anyhow!("stored rnk_dh_pk is invalid")
                                            })?
                                    }
                                };
                                effective_nullifier_key(
                                    *fvk.nullifier_key(),
                                    fvk.incoming(),
                                    &note.address(),
                                    note.asset_id(),
                                    policy.ring.ring_pk,
                                    rnk_dh_pk,
                                    true,
                                )?
                            }
                            None => *fvk.nullifier_key(),
                        };
                        let nullifier = Nullifier::derive(&nk, position, payload.commitment());
                        let address_index =
                            fvk.incoming().index_for_diversifier(note.diversifier());

                        new_notes.insert(
                            *payload.commitment(),
                            SpendableNoteRecord {
                                note_commitment: *payload.commitment(),
                                height_spent: None,
                                height_created: height,
                                note: note.clone(),
                                address_index,
                                nullifier,
                                position,
                                source,
                                return_address: None,
                            },
                        );
                    }
                    None => {
                        // Don't remember this commitment; it wasn't ours
                        state_commitment_tree
                            .insert(tct::Witness::Forget, *payload.commitment())
                            .expect("inserting a commitment must succeed");
                    }
                }
            }

            commitments_in_reconstructed_block += 1;
        }

        if commitments_in_reconstructed_block > 0 {
            state_commitment_tree
                .end_block()
                .expect("ending the block must succeed");
        }
    }

    // If we've also reached the end of the epoch, end the epoch in the commitment tree
    let is_epoch_end = epoch_root.is_some();
    if is_epoch_end {
        tracing::debug!(?height, "end of epoch");
        state_commitment_tree
            .end_epoch()
            .expect("ending the epoch must succeed");
    }

    // Print the TCT root for debugging
    #[cfg(feature = "sct-divergence-check")]
    tracing::debug!(tct_root = %state_commitment_tree.root(), "tct root");

    // Filter nullifiers to remove any without matching note note_commitments
    // This is a very important optimization to avoid unnecessary query load on the storage backend
    // -- it results in 100x+ slower sync times if we don't do this!
    let filtered_nullifiers = storage.filter_nullifiers(spent_nullifiers).await?;

    // Construct filtered block
    let result = FilteredBlock {
        new_notes,
        spent_nullifiers: filtered_nullifiers,
        height,
        discovery_parameters,
        app_parameters_updated,
        gas_prices,
        nullifier_window,
        volume_accumulators,
    };

    Ok(result)
}
