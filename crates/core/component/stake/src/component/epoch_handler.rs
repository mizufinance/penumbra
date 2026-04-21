use crate::{
    component::{
        stake::{ConsensusIndexRead, ConsensusIndexWrite, ConsensusUpdateWrite},
        validator_handler::{ValidatorDataRead, ValidatorDataWrite, ValidatorManager},
        StateReadExt as _,
    },
    params::equal_validator_voting_power,
    state_key, validator, CurrentConsensusKeys, IdentityKey,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use futures::{StreamExt, TryStreamExt};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use penumbra_sdk_sct::epoch::Epoch;
use std::collections::{BTreeMap, BTreeSet};
use tendermint::{validator::Update, PublicKey};
use tokio::task::JoinSet;
use tracing::instrument;

#[async_trait]
pub trait EpochHandler: crate::component::StateWriteExt + ConsensusIndexRead {
    #[instrument(skip(self, epoch_to_end), fields(index = epoch_to_end.index))]
    async fn end_epoch(&mut self, epoch_to_end: Epoch) -> Result<()> {
        let validators_to_process = self
            .consensus_set_stream()?
            .try_collect::<BTreeSet<_>>()
            .await?;

        for validator_identity in &validators_to_process {
            self.process_validator(validator_identity, epoch_to_end)
                .await
                .map_err(|e| {
                    tracing::error!(
                        ?e,
                        ?validator_identity,
                        "failed to process validator end-epoch"
                    );
                    e
                })?;
        }

        self.set_active_and_inactive_validators().await?;
        Ok(())
    }

    async fn process_validator(
        &mut self,
        validator_identity: &IdentityKey,
        epoch_to_end: Epoch,
    ) -> Result<()> {
        let validator = self.get_validator_definition(validator_identity).await?.ok_or_else(|| {
            anyhow::anyhow!(
                "validator (identity={}) is in consensus index but its definition was not found in the JMT",
                validator_identity
            )
        })?;

        let validator_state = self
            .get_validator_state(&validator.identity_key)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "validator (identity={}) is in consensus index but its state was not found in the JMT",
                    &validator.identity_key
                )
            })?;

        let effective_state = self
            .try_precursor_transition(
                validator_identity,
                validator_state,
                self.get_validator_power(&validator.identity_key)
                    .await?
                    .unwrap_or_default(),
            )
            .await
            .unwrap_or(validator_state);

        let voting_power = if matches!(
            effective_state,
            validator::State::Active | validator::State::Inactive
        ) {
            equal_validator_voting_power()
        } else {
            Amount::zero()
        };
        self.set_validator_power(&validator.identity_key, voting_power)?;

        tracing::debug!(
            validator_identity = %validator.identity_key,
            voting_power = ?voting_power,
            final_state = ?effective_state,
            epoch = epoch_to_end.index,
            "validator end-epoch processed"
        );

        self.process_validator_pool_state(&validator.identity_key, epoch_to_end.start_height)
            .await?;

        if !self.belongs_in_index(&validator.identity_key).await {
            self.remove_consensus_set_index(&validator.identity_key);
        }

        Ok(())
    }

    async fn set_active_and_inactive_validators(&mut self) -> Result<()> {
        let mut indexed_validators = Vec::new();

        let mut validator_identity_stream = self.consensus_set_stream()?;
        while let Some(identity_key) = validator_identity_stream.next().await {
            let identity_key = identity_key?;
            let state = self
                .get_validator_state(&identity_key)
                .await?
                .context("should be able to fetch validator state")?;
            if matches!(state, validator::State::Active | validator::State::Inactive) {
                indexed_validators.push(identity_key);
            }
        }

        indexed_validators.sort();

        let limit = self.get_stake_params().await?.active_validator_limit as usize;
        let active = indexed_validators.iter().take(limit);
        let inactive = indexed_validators.iter().skip(limit);

        for v in active {
            self.set_validator_state(v, validator::State::Active)
                .await?;
        }
        for v in inactive {
            self.set_validator_state(v, validator::State::Inactive)
                .await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn build_cometbft_validator_updates(&mut self) -> Result<()> {
        let current_consensus_keys: CurrentConsensusKeys = self
            .get(state_key::consensus_update::consensus_keys())
            .await?
            .expect("current consensus keys must be present");
        let current_consensus_keys = current_consensus_keys
            .consensus_keys
            .into_iter()
            .collect::<BTreeSet<_>>();

        let mut voting_power_by_consensus_key = BTreeMap::<PublicKey, Amount>::new();
        let mut js: JoinSet<std::prelude::v1::Result<(PublicKey, Amount), anyhow::Error>> =
            JoinSet::new();
        let mut validator_identity_stream = self.consensus_set_stream()?;
        while let Some(identity_key) = validator_identity_stream.next().await {
            let identity_key = identity_key?;
            let state = self.get_validator_state(&identity_key);
            let consensus_key = self.fetch_validator_consensus_key(&identity_key);
            js.spawn(async move {
                let state = state
                    .await?
                    .expect("every known validator must have a recorded state");
                let effective_power = if matches!(state, validator::State::Active) {
                    equal_validator_voting_power()
                } else {
                    Amount::zero()
                };

                let consensus_key = consensus_key
                    .await?
                    .expect("every known validator must have a recorded consensus key");

                anyhow::Ok((consensus_key, effective_power))
            });
        }

        while let Some(pair) = js.join_next().await.transpose()? {
            let (consensus_key, effective_power) = pair?;
            voting_power_by_consensus_key.insert(consensus_key, effective_power);
        }

        voting_power_by_consensus_key.retain(|consensus_key, voting_power| {
            *voting_power > Amount::zero() || current_consensus_keys.contains(consensus_key)
        });

        for ck in current_consensus_keys.iter() {
            voting_power_by_consensus_key
                .entry(*ck)
                .or_insert(Amount::zero());
        }

        let tendermint_validator_updates = voting_power_by_consensus_key
            .iter()
            .map(|(consensus_key, power)| {
                Ok(Update {
                    pub_key: *consensus_key,
                    power: ((*power).value() as u64).try_into()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.put_cometbft_validator_updates(tendermint_validator_updates);

        let updated_consensus_keys = CurrentConsensusKeys {
            consensus_keys: voting_power_by_consensus_key
                .iter()
                .filter_map(|(consensus_key, power)| {
                    if *power != Amount::zero() {
                        Some(*consensus_key)
                    } else {
                        None
                    }
                })
                .collect(),
        };
        tracing::debug!(?updated_consensus_keys);
        self.put(
            state_key::consensus_update::consensus_keys().to_owned(),
            updated_consensus_keys,
        );

        Ok(())
    }
}

impl<T: StateWrite + ConsensusIndexRead + ?Sized> EpochHandler for T {}
