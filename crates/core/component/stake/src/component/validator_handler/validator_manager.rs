use {
    crate::{
        component::{
            metrics,
            stake::ConsensusIndexWrite,
            validator_handler::{ValidatorDataRead, ValidatorDataWrite},
            StateReadExt as _, StateWriteExt as _,
        },
        event, state_key,
        validator::{
            self,
            State::{self, *},
            Validator,
        },
        IdentityKey, Uptime,
    },
    anyhow::{ensure, Result},
    async_trait::async_trait,
    cnidarium::StateWrite,
    penumbra_sdk_num::Amount,
    penumbra_sdk_proto::{DomainType as _, StateWriteProto},
    penumbra_sdk_sct::component::{
        clock::{EpochManager, EpochRead},
        StateReadExt as _,
    },
    tendermint::abci::types::Misbehavior,
    tracing::instrument,
};

#[async_trait]
pub trait ValidatorManager: StateWrite {
    async fn set_validator_state(
        &mut self,
        identity_key: &IdentityKey,
        new_state: validator::State,
    ) -> Result<(State, State)> {
        let old_state = self
            .get_validator_state(identity_key)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("validator state not found for validator {}", identity_key)
            })?;

        self.set_validator_state_inner(identity_key, old_state, new_state)
            .await
    }

    #[instrument(skip(self), name = "set_validator_state")]
    async fn set_validator_state_inner(
        &mut self,
        identity_key: &IdentityKey,
        old_state: validator::State,
        new_state: validator::State,
    ) -> Result<(State, State)> {
        let validator_state_path = state_key::validators::state::by_id(identity_key);
        let current_height = self.get_block_height().await?;

        if let (Active, Defined | Disabled | Jailed | Tombstoned) = (old_state, new_state) {
            self.set_end_epoch_flag();
        }

        match (old_state, new_state) {
            (Defined | Disabled | Jailed, Inactive) => {
                self.add_consensus_set_index(identity_key);
            }
            (Inactive | Jailed | Defined, Disabled) => {}
            (Inactive | Jailed | Disabled, Defined) => {}
            (Inactive, Active) => {
                self.set_validator_uptime(
                    identity_key,
                    Uptime::new(
                        self.get_block_height().await?,
                        self.signed_blocks_window_len().await? as usize,
                    ),
                );
            }
            (Active, Inactive | Defined | Disabled) => {}
            (Active, Jailed) => {}
            (Defined | Disabled | Inactive | Active | Jailed, Tombstoned) => {}

            (Tombstoned, Tombstoned) => {}
            (Defined, Defined) => {}
            (Inactive, Inactive) => {}
            (Active, Active) => {}
            (Jailed, Jailed) => {}
            (Disabled, Disabled) => {}

            (Disabled | Defined | Jailed, Active) => {
                anyhow::bail!(
                    "only inactive validators can become active (identity={}, old_state={:?}, new_state={:?})",
                    identity_key,
                    old_state,
                    new_state
                )
            }
            (Disabled | Defined | Inactive, Jailed) => {
                anyhow::bail!(
                    "only active validators can get jailed (identity={}, old_state={:?}, new_state={:?})",
                    identity_key,
                    old_state,
                    new_state
                )
            }
            (Tombstoned, Defined | Disabled | Inactive | Active | Jailed) => {
                anyhow::bail!(
                    "tombstoning is permanent, identity_key={}, new_state={:?}",
                    identity_key,
                    new_state
                )
            }
        }

        if new_state == Disabled {
            self.set_last_disabled_height(identity_key, current_height)
        }

        self.put(validator_state_path, new_state);
        self.record_proto(
            event::EventValidatorStateChange {
                identity_key: *identity_key,
                state: new_state,
            }
            .to_proto(),
        );

        Ok((old_state, new_state))
    }

    #[instrument(skip(self))]
    async fn try_precursor_transition(
        &mut self,
        validator_id: &IdentityKey,
        previous_state: validator::State,
        voting_power: Amount,
    ) -> Option<State> {
        if !matches!(previous_state, Defined | Inactive | Active) {
            return None;
        }
        tracing::debug!(
            %validator_id,
            ?previous_state,
            ?voting_power,
            "validator precursor transitions depend only on validator state"
        );
        Some(previous_state)
    }

    #[instrument(skip(self))]
    async fn add_genesis_validator(&mut self, validator: Validator) -> Result<()> {
        let power = crate::params::equal_validator_voting_power();

        self.add_validator_inner(validator.clone(), validator::State::Active, power)
            .await?;
        self.add_consensus_set_index(&validator.identity_key);
        self.set_validator_uptime(
            &validator.identity_key,
            Uptime::new(0, self.signed_blocks_window_len().await? as usize),
        );

        Ok(())
    }

    async fn add_validator(&mut self, validator: Validator) -> Result<()> {
        let initial_power = if validator.enabled {
            crate::params::equal_validator_voting_power()
        } else {
            Amount::zero()
        };

        self.add_validator_inner(validator.clone(), validator::State::Defined, initial_power)
            .await?;

        if validator.enabled {
            self.set_validator_state(&validator.identity_key, validator::State::Inactive)
                .await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn add_validator_inner(
        &mut self,
        validator: Validator,
        initial_state: validator::State,
        initial_voting_power: Amount,
    ) -> Result<()> {
        tracing::debug!("adding validator");
        if !matches!(initial_state, State::Defined | State::Active) {
            anyhow::bail!(
                "validator (identity_key={}) cannot have initial_state={:?}",
                validator.identity_key,
                initial_state
            )
        }
        let validator_identity = validator.identity_key;

        self.put(
            state_key::validators::definitions::by_id(&validator_identity),
            validator.clone(),
        );
        self.register_consensus_key(&validator_identity, &validator.consensus_key);
        self.record_proto(
            event::EventValidatorDefinitionUpload {
                validator: validator.clone(),
            }
            .to_proto(),
        );
        self.set_initial_validator_state(&validator_identity, initial_state)?;
        self.set_validator_power(&validator_identity, initial_voting_power)?;

        metrics::gauge!(metrics::MISSED_BLOCKS, "identity_key" => validator_identity.to_string())
            .increment(0.0);

        Ok(())
    }

    #[tracing::instrument(skip(self, validator), fields(id = ?validator.identity_key))]
    async fn update_validator_definition(&mut self, validator: Validator) -> Result<()> {
        let id = &validator.identity_key;
        let current_state = self
            .get_validator_state(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("updated validator has no recorded state"))?;

        match (current_state, validator.enabled) {
            (Active | Inactive | Jailed | Defined | Disabled, false) => {
                self.set_validator_state(id, Disabled).await?;
            }
            (Disabled, true) => {
                let last_disabled_height = self.get_last_disabled_height(id).await;
                if let Some(last_disabled) = last_disabled_height {
                    let current_height = self.get_block_height().await?;
                    let epoch_duration = self.get_sct_params().await?.epoch_duration;
                    let allowed_enabled_height = last_disabled.saturating_add(epoch_duration);
                    let wait_duration = current_height.saturating_sub(allowed_enabled_height);
                    ensure!(
                        current_height >= allowed_enabled_height,
                        "validator has been disabled too recently (last_disabled={}, current_height={}, epoch_duration={}), wait {} blocks",
                        last_disabled,
                        current_height,
                        epoch_duration,
                        wait_duration
                    );
                }
                self.set_validator_state(id, Inactive).await?;
            }
            (Jailed, true) => {
                self.set_validator_state(id, Inactive).await?;
            }
            (Active | Inactive, true) => {}
            (Defined, true) => {
                self.set_validator_state(id, Inactive).await?;
            }
            (Tombstoned, _) => {}
        }

        self.register_consensus_key(&validator.identity_key, &validator.consensus_key);
        self.put(
            state_key::validators::definitions::by_id(id),
            validator.clone(),
        );
        self.record_proto(event::EventValidatorDefinitionUpload { validator }.to_proto());

        Ok(())
    }

    #[instrument(skip(self))]
    async fn process_validator_pool_state(
        &mut self,
        _validator_identity: &IdentityKey,
        _from_height: u64,
    ) -> Result<()> {
        Ok(())
    }

    async fn process_evidence(&mut self, evidence: &Misbehavior) -> Result<()> {
        let validator = self
            .get_validator_definition_by_cometbft_address(&evidence.validator.address)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "attempted to tombstone unknown validator with evidence={:?}",
                    evidence
                )
            })?;

        let (old_state, new_state) = self
            .set_validator_state(&validator.identity_key, validator::State::Tombstoned)
            .await?;

        if let (Inactive | Jailed | Active, Tombstoned) = (old_state, new_state) {
            let current_height = self.get_block_height().await?;
            self.record_proto(
                event::EventTombstoneValidator::from_evidence(
                    current_height,
                    validator.identity_key,
                    evidence,
                )
                .to_proto(),
            );
        }

        Ok(())
    }
}

impl<T: StateWrite + ?Sized> ValidatorManager for T {}
