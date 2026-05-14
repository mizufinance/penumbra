use anyhow::{anyhow, ensure, Context, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use futures::{Stream, StreamExt};
use penumbra_sdk_proto::{DomainType as _, StateReadProto, StateWriteProto};
use penumbra_sdk_tct as tct;
use std::{
    ops::{Range, RangeFrom},
    pin::Pin,
};
use tct::builder::{block, epoch};
use tct::storage::{AsyncRead as TctAsyncRead, AsyncWrite as TctAsyncWrite, StoredPosition};
use tct::structure::Hash;
use tracing::instrument;

use crate::{
    component::{clock::EpochRead, sct::StateReadExt},
    event, nullifier_tree, state_key, CommitmentSource, NullificationInfo, Nullifier,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ProposalNullifierBatchProfile {
    pub lookup_write_ms: f64,
    pub pending_stage_ms: f64,
}

struct SctNvStorage<'a, S: ?Sized> {
    state: &'a mut S,
}

impl<'a, S: ?Sized> SctNvStorage<'a, S> {
    fn new(state: &'a mut S) -> Self {
        Self { state }
    }
}

fn decode_stored_position(bytes: Option<Vec<u8>>) -> Result<StoredPosition> {
    bytes
        .map(|bytes| bincode::deserialize(&bytes).context("decode SCT stored position"))
        .transpose()
        .map(|position| position.unwrap_or_default())
}

fn decode_forgotten(bytes: Option<Vec<u8>>) -> Result<tct::Forgotten> {
    bytes
        .map(|bytes| bincode::deserialize(&bytes).context("decode SCT forgotten version"))
        .transpose()
        .map(|forgotten| forgotten.unwrap_or_default())
}

fn decode_hash_value(bytes: Vec<u8>) -> Result<Hash> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow!("stored SCT hash must be 32 bytes, got {}", bytes.len())
    })?;
    Hash::from_bytes(bytes).map_err(|_| anyhow!("stored SCT hash is not a field element"))
}

fn decode_commitment_value(bytes: Vec<u8>) -> Result<tct::StateCommitment> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow!(
            "stored SCT commitment must be 32 bytes, got {}",
            bytes.len()
        )
    })?;
    tct::StateCommitment::try_from(bytes)
        .map_err(|_| anyhow!("stored SCT commitment is not a field element"))
}

fn decode_hash_row(key: &[u8], bytes: Vec<u8>) -> Result<(tct::Position, u8, Hash)> {
    let key = std::str::from_utf8(key).context("SCT hash key is not UTF-8")?;
    let suffix = key
        .strip_prefix(state_key::tree::incremental_hash_prefix())
        .unwrap_or(key);
    let (position, height) = suffix
        .split_once('/')
        .ok_or_else(|| anyhow!("SCT hash key missing height: {key}"))?;
    Ok((
        position.parse::<u64>()?.into(),
        height.parse::<u8>()?,
        decode_hash_value(bytes)?,
    ))
}

fn decode_commitment_row(
    key: &[u8],
    bytes: Vec<u8>,
) -> Result<(tct::Position, tct::StateCommitment)> {
    let key = std::str::from_utf8(key).context("SCT commitment key is not UTF-8")?;
    let suffix = key
        .strip_prefix(state_key::tree::incremental_commitment_prefix())
        .unwrap_or(key);
    Ok((
        suffix.parse::<u64>()?.into(),
        decode_commitment_value(bytes)?,
    ))
}

fn prefix_range(prefix: &str) -> RangeFrom<Vec<u8>> {
    let _ = prefix;
    Vec::new()..
}

#[async_trait]
impl<S: StateRead + Send + Sync + ?Sized> TctAsyncRead for SctNvStorage<'_, S> {
    type Error = anyhow::Error;

    type HashesStream<'b>
        = Pin<Box<dyn Stream<Item = Result<(tct::Position, u8, Hash)>> + Send + 'b>>
    where
        Self: 'b;

    type CommitmentsStream<'b>
        = Pin<Box<dyn Stream<Item = Result<(tct::Position, tct::StateCommitment)>> + Send + 'b>>
    where
        Self: 'b;

    async fn position(&mut self) -> Result<StoredPosition> {
        decode_stored_position(
            self.state
                .nonverifiable_get_raw(state_key::tree::incremental_position().as_bytes())
                .await?,
        )
    }

    async fn forgotten(&mut self) -> Result<tct::Forgotten> {
        decode_forgotten(
            self.state
                .nonverifiable_get_raw(state_key::tree::incremental_forgotten().as_bytes())
                .await?,
        )
    }

    async fn hash(&mut self, position: tct::Position, height: u8) -> Result<Option<Hash>> {
        self.state
            .nonverifiable_get_raw(state_key::tree::incremental_hash(position, height).as_bytes())
            .await?
            .map(decode_hash_value)
            .transpose()
    }

    fn hashes(&mut self) -> Self::HashesStream<'_> {
        self.state
            .nonverifiable_range_raw(
                Some(state_key::tree::incremental_hash_prefix().as_bytes()),
                prefix_range(state_key::tree::incremental_hash_prefix()),
            )
            .expect("valid SCT hash storage range")
            .map(|result| result.and_then(|(key, bytes)| decode_hash_row(&key, bytes)))
            .boxed()
    }

    async fn commitment(
        &mut self,
        position: tct::Position,
    ) -> Result<Option<tct::StateCommitment>> {
        self.state
            .nonverifiable_get_raw(state_key::tree::incremental_commitment(position).as_bytes())
            .await?
            .map(decode_commitment_value)
            .transpose()
    }

    fn commitments(&mut self) -> Self::CommitmentsStream<'_> {
        self.state
            .nonverifiable_range_raw(
                Some(state_key::tree::incremental_commitment_prefix().as_bytes()),
                prefix_range(state_key::tree::incremental_commitment_prefix()),
            )
            .expect("valid SCT commitment storage range")
            .map(|result| result.and_then(|(key, bytes)| decode_commitment_row(&key, bytes)))
            .boxed()
    }
}

#[async_trait]
impl<S: StateWrite + Send + Sync + ?Sized> TctAsyncWrite for SctNvStorage<'_, S> {
    async fn add_hash(
        &mut self,
        position: tct::Position,
        height: u8,
        hash: Hash,
        _essential: bool,
    ) -> Result<()> {
        self.state.nonverifiable_put_raw(
            state_key::tree::incremental_hash(position, height).into_bytes(),
            hash.to_bytes().to_vec(),
        );
        Ok(())
    }

    async fn add_commitment(
        &mut self,
        position: tct::Position,
        commitment: tct::StateCommitment,
    ) -> Result<()> {
        let key = state_key::tree::incremental_commitment(position);
        if let Some(existing) = self.state.nonverifiable_get_raw(key.as_bytes()).await? {
            anyhow::ensure!(
                existing == commitment.0.to_bytes().to_vec(),
                "refusing to overwrite SCT commitment at position {}",
                u64::from(position)
            );
            return Ok(());
        }
        self.state
            .nonverifiable_put_raw(key.into_bytes(), commitment.0.to_bytes().to_vec());
        Ok(())
    }

    async fn delete_range(
        &mut self,
        below_height: u8,
        positions: Range<tct::Position>,
    ) -> Result<()> {
        let start = u64::from(positions.start);
        let end = u64::from(positions.end);
        for position in start..end {
            let position = tct::Position::from(position);
            self.state.nonverifiable_delete(
                state_key::tree::incremental_commitment(position).into_bytes(),
            );
            for height in 0..below_height {
                self.state.nonverifiable_delete(
                    state_key::tree::incremental_hash(position, height).into_bytes(),
                );
            }
        }
        Ok(())
    }

    async fn set_position(&mut self, position: StoredPosition) -> Result<()> {
        self.state.nonverifiable_put_raw(
            state_key::tree::incremental_position().as_bytes().to_vec(),
            bincode::serialize(&position)?,
        );
        Ok(())
    }

    async fn set_forgotten(&mut self, forgotten: tct::Forgotten) -> Result<()> {
        self.state.nonverifiable_put_raw(
            state_key::tree::incremental_forgotten().as_bytes().to_vec(),
            bincode::serialize(&forgotten)?,
        );
        Ok(())
    }
}

#[async_trait]
/// Provides read access to the state commitment tree and related data.
pub trait SctRead: StateRead {
    /// Fetch the state commitment tree from nonverifiable storage, preferring the cached tree if
    /// it exists.
    async fn get_sct(&self) -> tct::Tree {
        // If we have a cached tree, use that.
        if let Some(tree) = self.object_get(state_key::cache::cached_state_commitment_tree()) {
            return tree;
        }

        let position = decode_stored_position(
            self.nonverifiable_get_raw(state_key::tree::incremental_position().as_bytes())
                .await
                .expect("able to read SCT stored position"),
        )
        .expect("able to decode SCT stored position");
        let forgotten = decode_forgotten(
            self.nonverifiable_get_raw(state_key::tree::incremental_forgotten().as_bytes())
                .await
                .expect("able to read SCT forgotten version"),
        )
        .expect("able to decode SCT forgotten version");

        let mut commitments = tct::Tree::load(position, forgotten);
        let commitment_stream = self
            .nonverifiable_range_raw(
                Some(state_key::tree::incremental_commitment_prefix().as_bytes()),
                prefix_range(state_key::tree::incremental_commitment_prefix()),
            )
            .expect("valid SCT commitment storage range");
        futures::pin_mut!(commitment_stream);
        while let Some((key, bytes)) = commitment_stream
            .next()
            .await
            .transpose()
            .expect("able to stream SCT commitments")
        {
            let (position, commitment) =
                decode_commitment_row(&key, bytes).expect("able to decode SCT commitment");
            commitments.insert(position, commitment);
        }
        drop(commitment_stream);

        let mut hashes = commitments.load_hashes();
        let hash_stream = self
            .nonverifiable_range_raw(
                Some(state_key::tree::incremental_hash_prefix().as_bytes()),
                prefix_range(state_key::tree::incremental_hash_prefix()),
            )
            .expect("valid SCT hash storage range");
        futures::pin_mut!(hash_stream);
        while let Some((key, bytes)) = hash_stream
            .next()
            .await
            .transpose()
            .expect("able to stream SCT hashes")
        {
            let (position, height, hash) =
                decode_hash_row(&key, bytes).expect("able to decode SCT hash");
            hashes.insert(position, height, hash);
        }
        hashes.finish()
    }

    /// Return the SCT root for the given height, if it exists.
    /// If the height is not found, return `None`.
    async fn get_anchor_by_height(&self, height: u64) -> Result<Option<tct::Root>> {
        self.get(&state_key::tree::anchor_by_height(height)).await
    }

    /// Return metadata on the specified nullifier, if it has been spent.
    async fn spend_info(&self, nullifier: Nullifier) -> Result<Option<NullificationInfo>> {
        nullifier_tree::spend_info(self, nullifier).await
    }

    /// Return the set of nullifiers that have been spent in the current block.
    fn pending_nullifiers(&self) -> im::Vector<Nullifier> {
        self.object_get(state_key::nullifier_set::pending_nullifiers())
            .unwrap_or_default()
    }
}

impl<T: StateRead + ?Sized> SctRead for T {}

#[async_trait]
/// Provides write access to the state commitment tree and related data.
pub trait SctManager: StateWrite {
    /// Write an SCT instance to nonverifiable storage and record
    /// the block and epoch roots in the JMT.
    ///
    /// # Panics
    /// If the epoch has not been set, or if a serialization failure occurs.
    async fn write_sct(
        &mut self,
        height: u64,
        sct: tct::Tree,
        block_root: block::Root,
        epoch_root: Option<epoch::Root>,
    ) {
        let sct_anchor = sct.root();
        let block_timestamp = self
            .get_current_block_timestamp()
            .await
            .map(|t| t.unix_timestamp())
            .unwrap_or(0);

        // Write the anchor as a key, so we can check claimed anchors...
        self.put_proto(state_key::tree::anchor_lookup(sct_anchor), height);
        // ... and as a value, so we can check SCT consistency.
        // TODO: can we move this out to NV storage?
        self.put(state_key::tree::anchor_by_height(height), sct_anchor);

        self.record_proto(event::anchor(height, sct_anchor, block_timestamp));
        self.record_proto(
            event::EventBlockRoot {
                height,
                root: block_root,
                timestamp_seconds: block_timestamp,
            }
            .to_proto(),
        );
        // Only record an epoch root event if we are ending the epoch.
        if let Some(epoch_root) = epoch_root {
            let index = self
                .get_current_epoch()
                .await
                .expect("epoch must be set")
                .index;
            self.record_proto(
                event::EventEpochRoot {
                    index,
                    root: epoch_root,
                    timestamp_seconds: block_timestamp,
                }
                .to_proto(),
            );
        }

        self.prune_sct_history(height).await;
        self.write_sct_cache(sct);
        self.persist_sct_cache().await;
    }

    /// Prune SCT anchors and height-indexed metadata outside the configured retention window.
    async fn prune_sct_history(&mut self, current_height: u64) {
        let retention = self
            .get_sct_params()
            .await
            .expect("SCT parameters must be set")
            .sct_anchor_retention_blocks;
        if let Some(expired_height) = current_height.checked_sub(retention.saturating_add(1)) {
            if let Ok(Some(expired_anchor)) = self.get_anchor_by_height(expired_height).await {
                self.delete(state_key::tree::anchor_by_height(expired_height));
                if self
                    .get_proto::<u64>(&state_key::tree::anchor_lookup(expired_anchor))
                    .await
                    .expect("able to read SCT anchor lookup")
                    == Some(expired_height)
                {
                    self.delete(state_key::tree::anchor_lookup(expired_anchor));
                }
            }

            self.delete(state_key::epoch_manager::epoch_by_height(expired_height));
            self.nonverifiable_delete(
                state_key::block_manager::block_timestamp(expired_height).into_bytes(),
            );
        }
    }

    /// Add a state commitment into the SCT, emitting an event recording its
    /// source, and return the insert position in the tree.
    async fn add_sct_commitment(
        &mut self,
        commitment: tct::StateCommitment,
        source: CommitmentSource,
    ) -> Result<tct::Position> {
        // Record in the SCT
        let mut tree = self.get_sct().await;
        let position = tree.insert(tct::Witness::Forget, commitment)?;
        self.write_sct_cache(tree);

        // Record the commitment source in an event
        self.record_proto(event::commitment(commitment, position, source));

        Ok(position)
    }

    /// Add a state commitment into the SCT at a pre-reserved position, without emitting an
    /// `EventCommitment`. Used by the app-level deferred SCT staging path.
    async fn add_sct_commitment_at_position(
        &mut self,
        commitment: tct::StateCommitment,
        expected_position: tct::Position,
    ) -> Result<()> {
        let mut tree = self.get_sct().await;
        let position = tree.insert(tct::Witness::Forget, commitment)?;
        ensure!(
            position == expected_position,
            "deferred SCT append position drifted: expected {expected_position:?}, got {position:?}"
        );
        self.write_sct_cache(tree);

        Ok(())
    }

    #[instrument(skip(self, source))]
    /// Record a nullifier as spent in the verifiable storage.
    async fn nullify(&mut self, nullifier: Nullifier, source: CommitmentSource) -> Result<()> {
        tracing::debug!("marking as spent");
        self.nullify_all(std::slice::from_ref(&nullifier), source)
            .await
    }

    #[instrument(skip(self, source, nullifiers))]
    /// Record a batch of nullifiers as spent in the verifiable storage.
    async fn nullify_all(
        &mut self,
        nullifiers: &[Nullifier],
        source: CommitmentSource,
    ) -> Result<()> {
        if nullifiers.is_empty() {
            return Ok(());
        }

        tracing::debug!(count = nullifiers.len(), "marking batch as spent");

        let id = source
            .id()
            .expect("nullifiers are only consumed by transactions");
        let spend_height = self.get_block_height().await.expect("block height is set");

        nullifier_tree::insert_batch(
            self,
            nullifiers
                .iter()
                .copied()
                .map(|nullifier| (nullifier, NullificationInfo { id, spend_height })),
        )
        .await?;

        // Record the nullifiers to be inserted into the compact block in one object-store rewrite.
        let mut pending_nullifiers = self.pending_nullifiers();
        pending_nullifiers.extend(nullifiers.iter().copied());
        self.object_put(
            state_key::nullifier_set::pending_nullifiers(),
            pending_nullifiers,
        );

        Ok(())
    }

    #[instrument(skip(self, entries))]
    /// Record a proposal-ordered batch of nullifiers as spent in verifiable storage.
    ///
    /// This method is intentionally blind to same-block conflicts. Proposal-order conflict
    /// resolution must happen before this batch is applied.
    async fn nullify_proposal_batch(
        &mut self,
        entries: &[(Nullifier, CommitmentSource)],
    ) -> Result<ProposalNullifierBatchProfile> {
        if entries.is_empty() {
            return Ok(ProposalNullifierBatchProfile::default());
        }

        tracing::debug!(
            count = entries.len(),
            "marking proposal nullifier batch as spent"
        );

        let spend_height = self.get_block_height().await.expect("block height is set");
        let mut profile = ProposalNullifierBatchProfile::default();

        let lookup_write_start = std::time::Instant::now();
        nullifier_tree::insert_batch(
            self,
            entries.iter().map(|(nullifier, source)| {
                (
                    *nullifier,
                    NullificationInfo {
                        id: source
                            .id()
                            .expect("nullifiers are only consumed by transactions"),
                        spend_height,
                    },
                )
            }),
        )
        .await?;
        profile.lookup_write_ms = lookup_write_start.elapsed().as_secs_f64() * 1000.0;

        let pending_stage_start = std::time::Instant::now();
        let mut pending_nullifiers = self.pending_nullifiers();
        pending_nullifiers.extend(entries.iter().map(|(nullifier, _)| *nullifier));
        self.object_put(
            state_key::nullifier_set::pending_nullifiers(),
            pending_nullifiers,
        );
        profile.pending_stage_ms = pending_stage_start.elapsed().as_secs_f64() * 1000.0;

        Ok(profile)
    }

    /// Seal the current block in the SCT, and produce an epoch root if
    /// we are ending an epoch as well.
    ///
    /// # Panics
    /// This method panic if the block is full, or if a serialization failure occurs.
    async fn end_sct_block(
        &mut self,
        end_epoch: bool,
    ) -> Result<(block::Root, Option<epoch::Root>)> {
        let height = self.get_block_height().await?;

        let mut tree = self.get_sct().await;

        // Close the block in the SCT
        let block_root = tree
            .end_block()
            .expect("ending a block in the state commitment tree can never fail");

        // If the block ends an epoch, also close the epoch in the SCT
        let epoch_root = if end_epoch {
            let epoch_root = tree
                .end_epoch()
                .expect("ending an epoch in the state commitment tree can never fail");
            Some(epoch_root)
        } else {
            None
        };

        self.write_sct(height, tree, block_root, epoch_root).await;

        Ok((block_root, epoch_root))
    }

    // Set the state commitment tree in memory, but without committing to it in the nonverifiable
    // storage (very cheap).
    fn write_sct_cache(&mut self, tree: tct::Tree) {
        self.object_put(state_key::cache::cached_state_commitment_tree(), tree);
    }

    /// Persist the object-store SCT instance to nonverifiable storage.
    /// Note that this doesn't actually persist the SCT to disk, see the
    /// cndiarium documentation for more information.
    ///  
    /// # Panics
    /// This method panics if a serialization failure occurs.
    async fn persist_sct_cache(&mut self) {
        // If the cached tree is dirty, flush it to storage
        if let Some(tree) =
            self.object_get::<tct::Tree>(state_key::cache::cached_state_commitment_tree())
        {
            let mut storage = SctNvStorage::new(self);
            tree.to_async_writer(&mut storage)
                .await
                .expect("able to persist state commitment tree to incremental NV storage");
        }
    }
}

impl<T: StateWrite + ?Sized> SctManager for T {}

#[async_trait]
pub trait VerificationExt: StateRead {
    async fn check_claimed_anchor(&self, anchor: tct::Root) -> Result<()> {
        if anchor.is_empty() {
            return Ok(());
        }

        if let Some(anchor_height) = self
            .get_proto::<u64>(&state_key::tree::anchor_lookup(anchor))
            .await?
        {
            tracing::debug!(?anchor, ?anchor_height, "anchor is valid");
            Ok(())
        } else {
            Err(anyhow!(
                "provided anchor {} is not a valid SCT root",
                anchor
            ))
        }
    }

    async fn check_nullifier_unspent(&self, nullifier: Nullifier) -> Result<()> {
        if let Some(info) = self.spend_info(nullifier).await? {
            anyhow::bail!(
                "nullifier {} was already spent in {:?}",
                nullifier,
                hex::encode(info.id),
            );
        }
        Ok(())
    }
}

impl<T: StateRead + ?Sized> VerificationExt for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        component::{
            clock::{EpochManager, EpochRead},
            sct::StateWriteExt,
        },
        epoch::Epoch,
        params::SctParameters,
    };
    use cnidarium::TempStorage;
    use futures::StreamExt;
    use penumbra_sdk_proto::StateReadProto;
    use std::str::FromStr;

    async fn write_test_anchor(
        state: &mut cnidarium::StateDelta<cnidarium::Snapshot>,
        height: u64,
        tree: &mut tct::Tree,
    ) -> tct::Root {
        state.put_block_timestamp(
            height,
            tendermint::Time::from_str("2026-01-01T00:00:00Z").unwrap(),
        );
        state.put_epoch_by_height(
            height,
            Epoch {
                index: 0,
                start_height: 0,
            },
        );
        let commitment = tct::StateCommitment::try_from([height as u8; 32]).unwrap();
        tree.insert(tct::Witness::Forget, commitment).unwrap();
        let block_root = tree.end_block().unwrap();
        let anchor = tree.root();
        state
            .write_sct(height, tree.clone(), block_root, None)
            .await;
        anchor
    }

    #[tokio::test]
    async fn sct_retention_prunes_only_after_window_boundary() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.put_sct_params(SctParameters {
            epoch_duration: 10,
            sct_anchor_retention_blocks: 2,
        });

        let mut tree = tct::Tree::new();
        let anchor_0 = write_test_anchor(&mut state, 0, &mut tree).await;
        let anchor_1 = write_test_anchor(&mut state, 1, &mut tree).await;
        write_test_anchor(&mut state, 2, &mut tree).await;

        assert_eq!(state.get_anchor_by_height(0).await.unwrap(), Some(anchor_0));
        assert!(state.get_block_timestamp(0).await.is_ok());
        assert!(state.get_epoch_by_height(0).await.is_ok());

        write_test_anchor(&mut state, 3, &mut tree).await;

        assert_eq!(state.get_anchor_by_height(0).await.unwrap(), None);
        assert!(state.get_block_timestamp(0).await.is_err());
        assert!(state.get_epoch_by_height(0).await.is_err());
        assert_eq!(state.get_anchor_by_height(1).await.unwrap(), Some(anchor_1));
        assert!(state.get_block_timestamp(1).await.is_ok());
        assert!(state.get_epoch_by_height(1).await.is_ok());
        assert_eq!(
            state
                .get_proto::<u64>(&state_key::tree::anchor_lookup(anchor_0))
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn sct_incremental_nv_persistence_roundtrips_without_full_blob() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        state.put_sct_params(SctParameters {
            epoch_duration: 10,
            sct_anchor_retention_blocks: 100,
        });
        state.put_block_timestamp(
            1,
            tendermint::Time::from_str("2026-01-01T00:00:00Z").unwrap(),
        );
        state.put_epoch_by_height(
            1,
            Epoch {
                index: 0,
                start_height: 0,
            },
        );

        let mut reference = tct::Tree::new();
        for i in 1..=8u8 {
            reference
                .insert(
                    tct::Witness::Keep,
                    tct::StateCommitment::try_from([i; 32]).unwrap(),
                )
                .unwrap();
        }
        let block_root = reference.end_block().unwrap();
        let expected_root = reference.root();

        state
            .write_sct(1, reference.clone(), block_root, None)
            .await;

        assert!(state
            .nonverifiable_get_raw(state_key::tree::state_commitment_tree().as_bytes())
            .await
            .unwrap()
            .is_none());

        let loaded = state.get_sct().await;
        assert_eq!(loaded.root(), expected_root);

        let mut count = 0usize;
        let stream = state
            .nonverifiable_range_raw(
                Some(state_key::tree::incremental_prefix().as_bytes()),
                Vec::new()..,
            )
            .unwrap();
        futures::pin_mut!(stream);
        while let Some(entry) = stream.next().await {
            entry.unwrap();
            count += 1;
        }
        assert!(count > 0, "incremental SCT storage wrote no keys");
        assert!(
            count < 128,
            "incremental SCT storage wrote too many keys for one small block: {count}"
        );
    }
}
