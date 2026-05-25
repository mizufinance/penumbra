use anyhow::Result;
use async_trait::async_trait;
use cnidarium::StateWrite;
use penumbra_sdk_asset::Value;
use penumbra_sdk_keys::Address;
use penumbra_sdk_sct::component::tree::SctManager;
use penumbra_sdk_sct::CommitmentSource;
use penumbra_sdk_tct as tct;
use tct::StateCommitment;
use tracing::instrument;

use crate::state_key;
use crate::{Note, NotePayload, Rseed};

#[cfg(feature = "benchmark-helpers")]
use penumbra_sdk_ibc::benchmarking::{record_inbound_stage, InboundStage};
#[cfg(feature = "benchmark-helpers")]
use std::time::Instant;

/// Manages the addition of new notes to the chain state.
#[async_trait]
pub trait NoteManager: StateWrite {
    /// Mint a new (public) note into the shielded pool.
    ///
    /// Most notes in the shielded pool are created by client transactions.
    /// This method allows the chain to inject new value into the shielded pool
    /// on its own.
    #[instrument(skip(self, value, address, source))]
    async fn mint_note(
        &mut self,
        value: Value,
        address: &Address,
        source: CommitmentSource,
    ) -> Result<()> {
        #[cfg(feature = "benchmark-helpers")]
        let mint_note_start = Instant::now();

        tracing::debug!(?value, ?address, "minting tokens");

        // These notes are public, so we don't need a blinding factor for
        // privacy, but since the note commitments are determined by the note
        // contents, we need to have unique (deterministic) blinding factors for
        // each note, so they cannot collide.
        //
        // Hashing the current SCT root would be sufficient, since it will
        // change every time we insert a new note.  But computing the SCT root
        // is very slow, so instead we hash the current position.
        #[cfg(feature = "benchmark-helpers")]
        let sct_append_start = Instant::now();
        let source_for_append = source.clone();
        let (position, note_payload) = self
            .add_sct_commitment_from_position(source_for_append, |position| {
                #[cfg(feature = "benchmark-helpers")]
                let note_build_start = Instant::now();
                let note_payload = build_position_derived_mint_payload(value, address, position)?;
                #[cfg(feature = "benchmark-helpers")]
                record_inbound_stage(InboundStage::MintNoteBuild, note_build_start.elapsed());

                Ok((note_payload.note_commitment, note_payload))
            })
            .await?;
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::MintNoteSctAppend, sct_append_start.elapsed());

        #[cfg(feature = "benchmark-helpers")]
        let pending_payload_start = Instant::now();
        let mut payloads = self.pending_note_payloads();
        payloads.push_back((position, note_payload, source));
        self.object_put(state_key::pending_notes(), payloads);
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::MintNotePendingPayload,
            pending_payload_start.elapsed(),
        );

        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::MintNoteTotal, mint_note_start.elapsed());

        Ok(())
    }

    #[instrument(skip(self, note_payload, source), fields(commitment = ?note_payload.note_commitment))]
    async fn add_note_payload(&mut self, note_payload: NotePayload, source: CommitmentSource) {
        tracing::debug!(source = ?source);

        // Action handlers emit semantic note-created/nullifier-spent events.
        // NoteManager only stages SCT and compact-block state.
        #[cfg(feature = "benchmark-helpers")]
        let sct_insert_start = Instant::now();
        let position = self.add_sct_commitment(note_payload.note_commitment, source.clone())
            .await
            // TODO: why? can't we exceed the number of state commitments in a block?
            .expect("inserting into the state commitment tree should not fail because we should budget commitments per block (currently unimplemented)");
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::MintNoteSctAppend, sct_insert_start.elapsed());

        // Queue the payload for compact-block emission after SCT insertion.
        #[cfg(feature = "benchmark-helpers")]
        let pending_payload_start = Instant::now();
        let mut payloads = self.pending_note_payloads();
        payloads.push_back((position, note_payload, source));
        self.object_put(state_key::pending_notes(), payloads);
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::MintNotePendingPayload,
            pending_payload_start.elapsed(),
        );
    }

    #[instrument(skip(self, note_commitment))]
    async fn add_rolled_up_payload(
        &mut self,
        note_commitment: StateCommitment,
        source: CommitmentSource,
    ) {
        tracing::debug!(?note_commitment);

        // Rolled-up payloads are synchronization artifacts only; semantic events
        // are emitted by the action handlers that created them.
        let position = self.add_sct_commitment(note_commitment, source)
            .await
            // TODO: why? can't we exceed the number of state commitments in a block?
            .expect("inserting into the state commitment tree should not fail because we should budget commitments per block (currently unimplemented)");

        // Queue the rolled-up commitment for compact-block emission.
        let mut payloads = self.pending_rolled_up_payloads();
        payloads.push_back((position, note_commitment));
        self.object_put(state_key::pending_rolled_up_payloads(), payloads);
    }

    fn pending_note_payloads(&self) -> im::Vector<(tct::Position, NotePayload, CommitmentSource)> {
        self.object_get(state_key::pending_notes())
            .unwrap_or_default()
    }

    fn pending_rolled_up_payloads(&self) -> im::Vector<(tct::Position, StateCommitment)> {
        self.object_get(state_key::pending_rolled_up_payloads())
            .unwrap_or_default()
    }
}

impl<T: StateWrite + ?Sized> NoteManager for T {}

pub fn build_position_derived_mint_payload(
    value: Value,
    address: &Address,
    position: tct::Position,
) -> Result<NotePayload> {
    let note = Note::from_parts(address.clone(), value, mint_rseed(position)?)?;
    Ok(note.payload())
}

fn mint_rseed(position: tct::Position) -> Result<Rseed> {
    let position_u64: u64 = position.into();
    let rseed_bytes: [u8; 32] = blake2b_simd::Params::default()
        .personal(b"PenumbraMint")
        .to_state()
        .update(&position_u64.to_le_bytes())
        .finalize()
        .as_bytes()[0..32]
        .try_into()?;
    Ok(Rseed(rseed_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::{StateDelta, TempStorage};
    use penumbra_sdk_asset::{Value, BASE_ASSET_ID};
    use penumbra_sdk_keys::test_keys;
    use penumbra_sdk_num::Amount;
    use std::ops::Deref as _;

    #[tokio::test]
    async fn mint_note_stages_position_derived_payloads() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        let address = test_keys::ADDRESS_0.deref().clone();
        let value = Value {
            amount: Amount::from(1u64),
            asset_id: *BASE_ASSET_ID,
        };

        state
            .mint_note(value, &address, CommitmentSource::Genesis)
            .await?;
        state
            .mint_note(value, &address, CommitmentSource::Genesis)
            .await?;

        let payloads = state.pending_note_payloads();
        assert_eq!(payloads.len(), 2);
        let first_position = payloads[0].0;
        let second_position = payloads[1].0;
        assert_eq!(u64::from(second_position), u64::from(first_position) + 1);
        assert_ne!(payloads[0].1.note_commitment, payloads[1].1.note_commitment);

        for (position, payload, source) in payloads {
            let expected_note = Note::from_parts(address.clone(), value, mint_rseed(position)?)?;
            let expected_payload = expected_note.payload();
            assert_eq!(payload.note_commitment, expected_payload.note_commitment);
            assert_eq!(payload.ephemeral_key.0, expected_payload.ephemeral_key.0);
            assert_eq!(payload.encrypted_note.0, expected_payload.encrypted_note.0);
            assert_eq!(source, CommitmentSource::Genesis);
        }

        Ok(())
    }

    #[tokio::test]
    async fn position_derived_mint_matches_immediate_mint_payload() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        let address = test_keys::ADDRESS_0.deref().clone();
        let value = Value {
            amount: Amount::from(1u64),
            asset_id: *BASE_ASSET_ID,
        };

        state
            .mint_note(value, &address, CommitmentSource::Genesis)
            .await?;
        let payloads = state.pending_note_payloads();
        let (position, immediate_payload, _) = &payloads[0];

        let rebuilt_payload = build_position_derived_mint_payload(value, &address, *position)?;

        assert_eq!(
            immediate_payload.note_commitment,
            rebuilt_payload.note_commitment
        );
        assert_eq!(
            immediate_payload.ephemeral_key.0,
            rebuilt_payload.ephemeral_key.0
        );
        assert_eq!(
            immediate_payload.encrypted_note.0,
            rebuilt_payload.encrypted_note.0
        );

        Ok(())
    }
}
