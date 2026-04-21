use crate::{
    component::MAX_VOTING_POWER,
    event, state_key,
    validator::{self, State, Validator},
    IdentityKey, Uptime,
};
use anyhow::Result;
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use futures::{Future, FutureExt};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{
    state::future::DomainFuture, DomainType, StateReadProto, StateWriteProto,
};
use std::pin::Pin;
use tendermint::PublicKey;
use tracing::instrument;

#[async_trait]
pub trait ValidatorDataRead: StateRead {
    async fn get_validator_info(
        &self,
        identity_key: &IdentityKey,
    ) -> Result<Option<validator::Info>> {
        let validator = self.get_validator_definition(identity_key).await?;
        let status = self.get_validator_status(identity_key).await?;

        match (validator, status) {
            (Some(validator), Some(status)) => Ok(Some(validator::Info { validator, status })),
            _ => Ok(None),
        }
    }

    fn get_validator_state(
        &self,
        identity_key: &IdentityKey,
    ) -> DomainFuture<validator::State, Self::GetRawFut> {
        self.get(&state_key::validators::state::by_id(identity_key))
    }

    async fn get_validator_status(
        &self,
        identity_key: &IdentityKey,
    ) -> Result<Option<validator::Status>> {
        let state = self.get_validator_state(identity_key).await?;
        let power = self.get_validator_power(identity_key).await?;
        let identity_key = *identity_key;
        match (state, power) {
            (Some(state), Some(voting_power)) => Ok(Some(validator::Status {
                identity_key,
                state,
                voting_power,
            })),
            _ => Ok(None),
        }
    }

    fn get_validator_power(
        &self,
        validator: &IdentityKey,
    ) -> DomainFuture<Amount, Self::GetRawFut> {
        self.get(&state_key::validators::power::by_id(validator))
    }

    async fn get_last_disabled_height(&self, identity_key: &IdentityKey) -> Option<u64> {
        self.nonverifiable_get_raw(
            state_key::validators::last_disabled::by_id(identity_key).as_bytes(),
        )
        .await
        .expect("no deserialization error expected")
        .map(|bytes| u64::from_be_bytes(bytes.try_into().expect("we only write 8 bytes")))
    }

    async fn get_validator_definition(
        &self,
        identity_key: &IdentityKey,
    ) -> Result<Option<Validator>> {
        self.get(&state_key::validators::definitions::by_id(identity_key))
            .await
    }

    fn get_validator_uptime(
        &self,
        identity_key: &IdentityKey,
    ) -> DomainFuture<Uptime, Self::GetRawFut> {
        let key = state_key::validators::uptime::by_id(identity_key);
        self.nonverifiable_get(key.as_bytes())
    }

    async fn lookup_identity_key_by_consensus_key(&self, ck: &PublicKey) -> Option<IdentityKey> {
        self.get(&state_key::validators::lookup_by::consensus_key(ck))
            .await
            .expect("no deserialization error")
    }

    async fn lookup_consensus_key_by_comet_address(&self, address: &[u8; 20]) -> Option<PublicKey> {
        self.get(&state_key::validators::lookup_by::cometbft_address(address))
            .await
            .expect("no deserialization error")
    }

    async fn get_validator_definition_by_consensus_key(
        &self,
        ck: &PublicKey,
    ) -> Result<Option<Validator>> {
        if let Some(identity_key) = self.lookup_identity_key_by_consensus_key(ck).await {
            self.get_validator_definition(&identity_key).await
        } else {
            Ok(None)
        }
    }

    async fn get_validator_definition_by_cometbft_address(
        &self,
        address: &[u8; 20],
    ) -> Result<Option<Validator>> {
        if let Some(consensus_key) = self.lookup_consensus_key_by_comet_address(address).await {
            self.get_validator_definition_by_consensus_key(&consensus_key)
                .await
        } else {
            Ok(None)
        }
    }

    fn fetch_validator_consensus_key(
        &self,
        identity_key: &IdentityKey,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PublicKey>>> + Send + 'static>> {
        use futures::TryFutureExt;
        self.get(&state_key::validators::definitions::by_id(identity_key))
            .map_ok(|opt: Option<Validator>| opt.map(|v: Validator| v.consensus_key))
            .boxed()
    }
}

impl<T: StateRead + ?Sized> ValidatorDataRead for T {}

#[async_trait]
pub(crate) trait ValidatorDataWrite: StateWrite {
    fn set_validator_uptime(&mut self, identity_key: &IdentityKey, uptime: Uptime) {
        self.nonverifiable_put_raw(
            state_key::validators::uptime::by_id(identity_key)
                .as_bytes()
                .to_vec(),
            uptime.encode_to_vec(),
        );
    }

    #[instrument(skip(self))]
    fn set_validator_power(
        &mut self,
        identity_key: &IdentityKey,
        voting_power: Amount,
    ) -> Result<()> {
        tracing::debug!(validator_identity = ?identity_key, ?voting_power, "setting validator power");
        if voting_power.value() > MAX_VOTING_POWER {
            anyhow::bail!("voting power exceeds maximum")
        }
        self.put(
            state_key::validators::power::by_id(identity_key),
            voting_power,
        );
        self.record_proto(
            event::EventValidatorVotingPowerChange {
                identity_key: *identity_key,
                voting_power,
            }
            .to_proto(),
        );

        Ok(())
    }

    #[instrument(skip(self))]
    fn set_initial_validator_state(
        &mut self,
        id: &IdentityKey,
        initial_state: State,
    ) -> Result<()> {
        tracing::debug!(validator_identity = %id, ?initial_state, "setting initial validator state");
        if !matches!(initial_state, State::Active | State::Defined) {
            anyhow::bail!("invalid initial validator state");
        }

        self.put(state_key::validators::state::by_id(id), initial_state);
        self.record_proto(
            event::EventValidatorStateChange {
                identity_key: *id,
                state: initial_state,
            }
            .to_proto(),
        );
        Ok(())
    }

    fn set_last_disabled_height(&mut self, identity_key: &IdentityKey, height: u64) {
        self.nonverifiable_put_raw(
            state_key::validators::last_disabled::by_id(identity_key)
                .as_bytes()
                .to_vec(),
            height.to_be_bytes().to_vec(),
        );
    }
}

impl<T: StateWrite + ?Sized> ValidatorDataWrite for T {}
