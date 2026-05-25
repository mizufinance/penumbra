pub mod address;

use crate::params::ValidatorParameters;
use crate::validator::Validator;
use crate::{state_key, CurrentConsensusKeys, IdentityKey};
use anyhow::Context;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use cnidarium_component::Component;
use futures::{StreamExt, TryStreamExt};
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use penumbra_sdk_sct::component::clock::EpochRead;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use tap::{TapFallible, TapOptional};
use tendermint::v0_37::abci;
use tendermint::validator::Update;
use tendermint::PublicKey;
use tracing::{error, instrument};

use crate::component::epoch_handler::EpochHandler;
use crate::component::validator_handler::{
    ValidatorDataRead, ValidatorManager, ValidatorUptimeTracker,
};

#[cfg(test)]
mod tests;

pub struct Staking {}

#[async_trait]
impl Component for Staking {
    type AppState = (
        crate::genesis::Content,
        penumbra_sdk_shielded_pool::genesis::Content,
    );

    #[instrument(name = "validator", skip(state, app_state))]
    async fn init_chain<S: StateWrite>(mut state: S, app_state: Option<&Self::AppState>) {
        match app_state {
            None => {}
            Some((validator_genesis, _sp_genesis)) => {
                state.put_stake_params(validator_genesis.validator_params.clone());

                for validator in validator_genesis.validators.iter() {
                    let validator = Validator::try_from(validator.clone())
                        .expect("should be able to parse genesis validator");
                    state
                        .add_genesis_validator(validator)
                        .await
                        .expect("should be able to add genesis validator to state");
                }

                state.put(
                    state_key::consensus_update::consensus_keys().to_owned(),
                    CurrentConsensusKeys::default(),
                );
            }
        }

        state
            .build_cometbft_validator_updates()
            .await
            .expect("should be able to build initial tendermint validator updates");
    }

    #[instrument(name = "validator", skip(state, begin_block))]
    async fn begin_block<S: StateWrite + 'static>(
        state: &mut Arc<S>,
        begin_block: &abci::request::BeginBlock,
    ) {
        let state = Arc::get_mut(state).expect("state should be unique");
        for evidence in begin_block.byzantine_validators.iter() {
            let _ = state.process_evidence(evidence).await.map_err(|e| {
                tracing::warn!(?e, "failed to process byzantine misbehavior evidence")
            });
        }

        state
            .track_uptime(&begin_block.last_commit_info)
            .await
            .expect("should be able to track uptime");
    }

    #[instrument(name = "validator", skip(state, end_block))]
    async fn end_block<S: StateWrite + 'static>(
        state: &mut Arc<S>,
        end_block: &abci::request::EndBlock,
    ) {
        let _ = (state, end_block);
    }

    #[instrument(name = "validator", skip(state))]
    async fn end_epoch<S: StateWrite + 'static>(state: &mut Arc<S>) -> anyhow::Result<()> {
        let state = Arc::get_mut(state).context("state should be unique")?;
        let epoch_ending = state
            .get_current_epoch()
            .await
            .context("should be able to get current epoch during end_epoch")?;
        state
            .end_epoch(epoch_ending)
            .await
            .context("should be able to write end_epoch")?;
        state
            .build_cometbft_validator_updates()
            .await
            .context("should be able to build tendermint validator updates")?;
        Ok(())
    }
}

pub trait ConsensusUpdateRead: StateRead {
    fn cometbft_validator_updates(&self) -> Option<Vec<Update>> {
        self.object_get(state_key::internal::cometbft_validator_updates())
            .unwrap_or(None)
    }
}

impl<T: StateRead + ?Sized> ConsensusUpdateRead for T {}

pub(crate) trait ConsensusUpdateWrite: StateWrite {
    fn put_cometbft_validator_updates(&mut self, updates: Vec<Update>) {
        self.object_put(
            state_key::internal::cometbft_validator_updates(),
            Some(updates),
        )
    }
}

impl<T: StateWrite + ?Sized> ConsensusUpdateWrite for T {}

#[async_trait]
pub trait StateReadExt: StateRead {
    #[instrument(skip(self), level = "trace")]
    async fn get_stake_params(&self) -> Result<ValidatorParameters> {
        self.get(state_key::parameters::key())
            .await
            .tap_err(|err| error!(?err, "could not deserialize validator parameters"))
            .expect("no deserialization error should happen")
            .tap_none(|| error!("could not find validator parameters"))
            .ok_or_else(|| anyhow!("Missing ValidatorParameters"))
    }

    #[instrument(skip(self), level = "trace")]
    async fn signed_blocks_window_len(&self) -> Result<u64> {
        self.get_stake_params()
            .await
            .map(|p| p.signed_blocks_window_len)
    }

    #[instrument(skip(self), level = "trace")]
    async fn missed_blocks_maximum(&self) -> Result<u64> {
        self.get_stake_params()
            .await
            .map(|p| p.missed_blocks_maximum)
    }
}

impl<T: StateRead + ?Sized> StateReadExt for T {}

#[async_trait]
pub trait StateWriteExt: StateWrite {
    fn put_stake_params(&mut self, params: ValidatorParameters) {
        self.put(state_key::parameters::key().into(), params)
    }

    #[instrument(skip_all)]
    fn register_consensus_key(&mut self, identity_key: &IdentityKey, consensus_key: &PublicKey) {
        let address = self::address::validator_address(consensus_key);
        self.put(
            state_key::validators::lookup_by::cometbft_address(&address),
            consensus_key.clone(),
        );
        self.put(
            state_key::validators::lookup_by::consensus_key(consensus_key),
            identity_key.clone(),
        );
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}

#[async_trait]
pub trait ConsensusIndexRead: StateRead {
    fn consensus_set_stream(
        &self,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<IdentityKey>> + Send + 'static>>> {
        Ok(self
            .nonverifiable_prefix_raw(
                state_key::validators::consensus_set_index::prefix().as_bytes(),
            )
            .map(|res| {
                res.map(|(_, raw_identity_key)| {
                    let str_identity_key = std::str::from_utf8(raw_identity_key.as_slice())
                        .expect("state keys should only have valid identity keys");
                    IdentityKey::from_str(str_identity_key)
                        .expect("state keys should only have valid identity keys")
                })
            })
            .boxed())
    }

    async fn get_consensus_set(&self) -> anyhow::Result<Vec<IdentityKey>> {
        self.consensus_set_stream()?.try_collect().await
    }

    #[instrument(level = "error", skip(self))]
    async fn belongs_in_index(&self, validator_id: &IdentityKey) -> bool {
        let Some(state) = self
            .get_validator_state(validator_id)
            .await
            .expect("no deserialization error")
        else {
            tracing::error!("validator state was not found");
            return false;
        };

        matches!(
            state,
            crate::validator::State::Active | crate::validator::State::Inactive
        )
    }
}

impl<T: StateRead + ?Sized> ConsensusIndexRead for T {}

#[async_trait]
pub trait ConsensusIndexWrite: StateWrite {
    fn add_consensus_set_index(&mut self, identity_key: &IdentityKey) {
        self.nonverifiable_put_raw(
            state_key::validators::consensus_set_index::by_id(identity_key)
                .as_bytes()
                .to_vec(),
            identity_key.to_string().as_bytes().to_vec(),
        );
    }

    fn remove_consensus_set_index(&mut self, identity_key: &IdentityKey) {
        self.nonverifiable_delete(
            state_key::validators::consensus_set_index::by_id(identity_key)
                .as_bytes()
                .to_vec(),
        );
    }
}

impl<T: StateWrite + ?Sized> ConsensusIndexWrite for T {}
