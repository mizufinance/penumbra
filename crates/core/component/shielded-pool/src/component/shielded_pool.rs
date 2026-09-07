use std::sync::Arc;

use crate::params::ShieldedPoolParameters;
use crate::{discovery, genesis, state_key};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use cnidarium_component::Component;
use futures::StreamExt as _;
use shieldd_sdk_proto::StateReadProto as _;
use shieldd_sdk_proto::StateWriteProto as _;
use shieldd_sdk_sct::component::tree::{SctManager as _, SctRead as _, MAX_NULLIFIERS_PER_BLOCK};
use shieldd_sdk_sct::CommitmentSource;
use shieldd_sdk_sct::Nullifier;
use tendermint::v0_37::abci;
use tracing::instrument;

use super::{AssetRegistry, NoteManager};

pub struct ShieldedPool {}

const GENESIS_SCT_BLOCK_CAPACITY: usize = u16::MAX as usize + 1;
/// Volume entries cannot exceed half the combined proof-bound nullifier budget:
/// each ordinary Transfer also carries at least one spend nullifier.
const MAX_VOLUME_NULLIFIER_DELETIONS_PER_BLOCK: usize = MAX_NULLIFIERS_PER_BLOCK / 2;

#[async_trait]
impl Component for ShieldedPool {
    type AppState = genesis::Content;

    #[instrument(name = "shielded_pool", skip(state, app_state))]
    async fn init_chain<S: StateWrite>(mut state: S, app_state: Option<&Self::AppState>) {
        match app_state {
            None => { /* Checkpoint -- no-op */ }
            Some(genesis) => {
                // TODO(erwan): the handling of those parameters is a bit weird.
                // rationalize it before merging
                state.put_shielded_pool_params(genesis.shielded_pool_params.clone());
                state.put_current_discovery_parameters(
                    genesis.shielded_pool_params.discovery_params.clone(),
                );
                state.put_previous_discovery_parameters(
                    genesis.shielded_pool_params.discovery_params.clone(),
                );
                let mut allocations_in_current_sct_block = 0usize;

                // Register a denom for each asset in the genesis state
                for allocation in &genesis.allocations {
                    tracing::debug!(?allocation, "processing allocation");
                    assert_ne!(
                        allocation.raw_amount,
                        0u128.into(),
                        "Genesis allocations contain empty note",
                    );
                    // `InitChain` can mint more notes than fit in a single SCT block. Because no
                    // ABCI `end_block` runs during genesis, we have to roll the frontier forward
                    // here to keep large synthetic allocation sets buildable.
                    if allocations_in_current_sct_block == GENESIS_SCT_BLOCK_CAPACITY {
                        let mut tree = state.get_sct().await;
                        tree.end_block()
                            .expect("ending a genesis SCT block should never fail");
                        state.write_sct_cache(tree);
                        allocations_in_current_sct_block = 0;
                    }

                    state.register_denom(&allocation.denom()).await;
                    state
                        .mint_note(
                            allocation.value(),
                            &allocation.address,
                            CommitmentSource::Genesis,
                        )
                        .await
                        .expect("able to mint note for genesis allocation");
                    allocations_in_current_sct_block += 1;
                }
            }
        }
    }

    #[instrument(name = "shielded_pool", skip(state, begin_block))]
    async fn begin_block<S: StateWrite + 'static>(
        state: &mut Arc<S>,
        begin_block: &abci::request::BeginBlock,
    ) {
        let now = u64::try_from(begin_block.header.time.unix_timestamp())
            .expect("consensus timestamps must be after the Unix epoch");
        Arc::get_mut(state)
            .expect("the state should not be shared")
            .prune_volume_nullifiers(now)
            .await
            .expect("daily volume nullifier pruning must succeed");
    }

    #[instrument(name = "shielded_pool", skip_all)]
    async fn end_block<S: StateWrite + 'static>(
        state: &mut Arc<S>,
        end_block: &abci::request::EndBlock,
    ) {
        let height: u64 = end_block
            .height
            .try_into()
            .expect("height should not be negative");
        let state = Arc::get_mut(state).expect("the state should not be shared");
        let configured = state
            .get_shielded_pool_params()
            .await
            .expect("should be able to read state");
        let current = state
            .get_current_discovery_parameters()
            .await
            .expect("should be able to read state");
        if configured.discovery_params.regulated_precision != current.regulated_precision
            || configured.discovery_params.unregulated_precision != current.unregulated_precision
        {
            let new = discovery::Parameters::new(
                configured.discovery_params.regulated_precision,
                configured.discovery_params.unregulated_precision,
                height,
            )
            .expect("validated discovery parameters remain ordered");
            state.put_previous_discovery_parameters(current);
            state.put_current_discovery_parameters(new);
        }
    }

    async fn end_epoch<S: StateWrite + 'static>(mut _state: &mut Arc<S>) -> Result<()> {
        Ok(())
    }
}
/// Extension trait providing read access to shielded pool data.
#[async_trait]
pub trait StateReadExt: StateRead {
    async fn get_current_discovery_parameters(&self) -> Result<discovery::Parameters> {
        self.get(discovery::state_key::parameters::current())
            .await?
            .ok_or_else(|| anyhow!("missing current discovery parameters"))
    }

    /// Gets the previously active discovery parameters.
    async fn get_previous_discovery_parameters(&self) -> Result<discovery::Parameters> {
        self.get(discovery::state_key::parameters::previous())
            .await?
            .ok_or_else(|| anyhow!("missing previous discovery parameters"))
    }

    async fn get_shielded_pool_params(&self) -> Result<ShieldedPoolParameters> {
        self.get(state_key::shielded_pool_params())
            .await?
            .ok_or_else(|| anyhow!("Missing ShieldedPoolParameters"))
    }

    async fn host_withdrawals_enabled(&self) -> Result<bool> {
        Ok(self
            .get_raw(state_key::host_withdrawals_enabled())
            .await?
            .is_some())
    }

    async fn volume_nullifier_exists(&self, day_start: u64, nullifier: Nullifier) -> Result<bool> {
        Ok(self
            .get_raw(&state_key::volume_nullifiers::by_day_and_nullifier(
                day_start, nullifier,
            ))
            .await?
            .is_some())
    }

    async fn check_volume_nullifier_unspent(
        &self,
        day_start: u64,
        nullifier: Nullifier,
    ) -> Result<()> {
        anyhow::ensure!(
            !self.volume_nullifier_exists(day_start, nullifier).await?,
            "daily volume nullifier {nullifier} is already spent for UTC day {day_start}"
        );
        Ok(())
    }
}

impl<T: StateRead + ?Sized> StateReadExt for T {}

/// Extension trait providing write access to shielded pool data.
#[async_trait]
pub trait StateWriteExt: StateWrite + StateReadExt {
    fn put_shielded_pool_params(&mut self, params: ShieldedPoolParameters) {
        self.put(crate::state_key::shielded_pool_params().into(), params)
    }

    fn put_host_withdrawals_enabled(&mut self, enabled: bool) {
        if enabled {
            self.put_raw(crate::state_key::host_withdrawals_enabled().into(), vec![1])
        } else {
            self.delete(crate::state_key::host_withdrawals_enabled().into())
        }
    }

    fn put_current_discovery_parameters(&mut self, params: discovery::Parameters) {
        self.put(discovery::state_key::parameters::current().into(), params)
    }

    fn put_previous_discovery_parameters(&mut self, params: discovery::Parameters) {
        self.put(discovery::state_key::parameters::previous().into(), params)
    }

    async fn record_volume_nullifier(
        &mut self,
        day_start: u64,
        nullifier: Nullifier,
    ) -> Result<()> {
        self.check_volume_nullifier_unspent(day_start, nullifier)
            .await?;
        self.put_raw(
            state_key::volume_nullifiers::by_day_and_nullifier(day_start, nullifier).into(),
            vec![1],
        );
        self.put_raw(
            state_key::volume_nullifiers::day_marker(day_start).into(),
            vec![1],
        );
        Ok(())
    }

    async fn prune_volume_nullifiers(&mut self, now: u64) -> Result<()> {
        self.prune_volume_nullifiers_with_limit(now, MAX_VOLUME_NULLIFIER_DELETIONS_PER_BLOCK)
            .await?;
        Ok(())
    }

    /// Deletes at most `limit` expired entry or marker keys and returns the count.
    async fn prune_volume_nullifiers_with_limit(
        &mut self,
        now: u64,
        limit: usize,
    ) -> Result<usize> {
        if limit == 0 {
            return Ok(0);
        }
        let marker_prefix = state_key::volume_nullifiers::day_marker_prefix();
        let marker_keys = self
            .prefix_keys(marker_prefix)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let mut deleted = 0usize;
        for marker_key in marker_keys {
            if deleted == limit {
                break;
            }
            let day_start: u64 = marker_key
                .strip_prefix(marker_prefix)
                .ok_or_else(|| anyhow!("invalid volume nullifier day marker"))?
                .parse()?;
            if now <= day_start.saturating_add(crate::VOLUME_ACCUMULATOR_RETENTION_SECS) {
                continue;
            }
            let remaining = limit - deleted;
            let keys = self
                .prefix_keys(&state_key::volume_nullifiers::day_prefix(day_start))
                .take(remaining.saturating_add(1))
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            let has_more = keys.len() > remaining;
            for key in keys.into_iter().take(remaining) {
                self.delete(key.into());
                deleted += 1;
            }
            if !has_more && deleted < limit {
                self.delete(marker_key.into());
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::{StateDelta, TempStorage};
    use decaf377::Fq;

    #[tokio::test]
    async fn volume_nullifiers_are_exclusive_and_pruned_after_the_buffer() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        let day_start = 86_400u64;
        let nullifier = Nullifier(Fq::from(9u64));

        state.record_volume_nullifier(day_start, nullifier).await?;
        assert!(state.volume_nullifier_exists(day_start, nullifier).await?);
        assert!(state
            .record_volume_nullifier(day_start, nullifier)
            .await
            .is_err());

        state
            .prune_volume_nullifiers(
                day_start.saturating_add(crate::VOLUME_ACCUMULATOR_RETENTION_SECS),
            )
            .await?;
        assert!(state.volume_nullifier_exists(day_start, nullifier).await?);

        state
            .prune_volume_nullifiers(
                day_start
                    .saturating_add(crate::VOLUME_ACCUMULATOR_RETENTION_SECS)
                    .saturating_add(1),
            )
            .await?;
        assert!(!state.volume_nullifier_exists(day_start, nullifier).await?);
        Ok(())
    }

    #[tokio::test]
    async fn volume_nullifier_pruning_is_bounded_and_resumes_from_the_marker() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        let day_start = 86_400u64;
        let expired = day_start
            .saturating_add(crate::VOLUME_ACCUMULATOR_RETENTION_SECS)
            .saturating_add(1);
        let nullifiers = [
            Nullifier(Fq::from(1u64)),
            Nullifier(Fq::from(2u64)),
            Nullifier(Fq::from(3u64)),
        ];
        for nullifier in nullifiers {
            state.record_volume_nullifier(day_start, nullifier).await?;
        }

        assert_eq!(
            state.prune_volume_nullifiers_with_limit(expired, 2).await?,
            2
        );
        let mut remaining = 0;
        for nullifier in nullifiers {
            remaining += usize::from(state.volume_nullifier_exists(day_start, nullifier).await?);
        }
        assert_eq!(remaining, 1);
        assert_eq!(
            state.prune_volume_nullifiers_with_limit(expired, 2).await?,
            2
        );
        for nullifier in nullifiers {
            assert!(!state.volume_nullifier_exists(day_start, nullifier).await?);
        }
        assert_eq!(
            state.prune_volume_nullifiers_with_limit(expired, 2).await?,
            0
        );
        Ok(())
    }
}
