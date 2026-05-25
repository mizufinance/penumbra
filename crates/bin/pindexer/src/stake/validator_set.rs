use anyhow::{anyhow, Result};
use cometindex::{
    async_trait,
    index::{EventBatch, EventBatchContext},
    sqlx, AppView, ContextualizedEvent, PgTransaction,
};

use penumbra_sdk_app::genesis::Content;
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{core::component::validator::v1 as pb, event::ProtoEvent};
use penumbra_sdk_validator::{
    validator::{self, Validator},
    IdentityKey,
};

use crate::parsing::parse_content;

#[derive(Debug)]
pub struct ValidatorSet {}

impl ValidatorSet {
    async fn index_event(
        &self,
        dbtx: &mut PgTransaction<'_>,
        event: ContextualizedEvent<'_>,
    ) -> Result<(), anyhow::Error> {
        match event.event.kind.as_str() {
            "penumbra.core.component.validator.v1.EventValidatorDefinitionUpload" => {
                let pe = pb::EventValidatorDefinitionUpload::from_event(event.as_ref())?;
                let val = Validator::try_from(
                    pe.validator
                        .ok_or_else(|| anyhow!("missing validator in event"))?,
                )?;

                handle_upload(dbtx, val).await?;
            }
            "penumbra.core.component.validator.v1.EventValidatorVotingPowerChange" => {
                let pe = pb::EventValidatorVotingPowerChange::from_event(event.as_ref())?;
                let ik = IdentityKey::try_from(
                    pe.identity_key
                        .ok_or_else(|| anyhow!("missing ik in event"))?,
                )?;
                let voting_power = Amount::try_from(
                    pe.voting_power
                        .ok_or_else(|| anyhow!("missing amount in event"))?,
                )?;
                handle_voting_power_change(dbtx, ik, voting_power).await?;
            }
            "penumbra.core.component.validator.v1.EventValidatorStateChange" => {
                let pe = pb::EventValidatorStateChange::from_event(event.as_ref())?;
                let ik = IdentityKey::try_from(
                    pe.identity_key
                        .ok_or_else(|| anyhow!("missing ik in event"))?,
                )?;
                let state = validator::State::try_from(
                    pe.state.ok_or_else(|| anyhow!("missing state in event"))?,
                )?;
                handle_validator_state_change(dbtx, ik, state).await?;
            }
            _ => {}
        }

        Ok(())
    }
}

#[async_trait]
impl AppView for ValidatorSet {
    async fn init_chain(
        &self,
        dbtx: &mut PgTransaction,
        app_state: &serde_json::Value,
    ) -> Result<(), anyhow::Error> {
        sqlx::query(
            // table name is module path + struct name
            // note: protobuf data is encoded as protojson for ease of consumers
            // hence TEXT fields
            "CREATE TABLE stake_validator_set (
                id SERIAL PRIMARY KEY,
                ik TEXT NOT NULL,
                name TEXT NOT NULL,
                definition TEXT NOT NULL,
                voting_power BIGINT NOT NULL,
                validator_state TEXT NOT NULL
            );",
        )
        .execute(dbtx.as_mut())
        .await?;

        sqlx::query("CREATE UNIQUE INDEX idx_stake_validator_set_ik ON stake_validator_set(ik);")
            .execute(dbtx.as_mut())
            .await?;

        add_genesis_validators(dbtx, &parse_content(app_state.clone())?).await?;
        Ok(())
    }

    fn name(&self) -> String {
        "stake/validator_set".to_string()
    }

    async fn index_batch(
        &self,
        dbtx: &mut PgTransaction,
        batch: EventBatch,
        _ctx: EventBatchContext,
    ) -> Result<(), anyhow::Error> {
        for event in batch.events() {
            self.index_event(dbtx, event).await?;
        }
        Ok(())
    }
}

async fn add_genesis_validators<'a>(dbtx: &mut PgTransaction<'a>, content: &Content) -> Result<()> {
    for val in &content.validator_content.validators {
        // FIXME: this shouldn't be a proto type but now that has been propagated
        // all through the rest of the code for no reason
        let val = Validator::try_from(val.clone())?;
        let voting_power = penumbra_sdk_validator::params::equal_validator_voting_power();

        // insert sql
        sqlx::query(
            "INSERT INTO stake_validator_set (
                ik, name, definition, voting_power, validator_state
            )
            VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(val.identity_key.to_string())
        .bind(val.name.clone())
        .bind(serde_json::to_string(&val).expect("can serialize"))
        .bind(voting_power.value() as i64)
        .bind(serde_json::to_string(&validator::State::Active).unwrap()) // see add_genesis_validator
        .execute(dbtx.as_mut())
        .await?;
    }

    Ok(())
}

async fn handle_upload<'a>(dbtx: &mut PgTransaction<'a>, val: Validator) -> Result<()> {
    // First, check if the validator already exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM stake_validator_set WHERE ik = $1)")
            .bind(&val.identity_key.to_string())
            .fetch_one(dbtx.as_mut())
            .await?;

    if exists {
        // Update existing validator, leaving all the other data like state, VP etc unchanged
        sqlx::query(
            "UPDATE stake_validator_set SET
                name = $2,
                definition = $3
            WHERE ik = $1",
        )
        .bind(val.identity_key.to_string())
        .bind(val.name.clone())
        .bind(serde_json::to_string(&val).expect("can serialize"))
        .execute(dbtx.as_mut())
        .await?;
    } else {
        // Insert new validator
        sqlx::query(
            "INSERT INTO stake_validator_set (
                ik, name, definition, voting_power, validator_state
            )
            VALUES ($1, $2, $3, 0, $4)",
        )
        .bind(val.identity_key.to_string())
        .bind(val.name.clone())
        .bind(serde_json::to_string(&val).expect("can serialize"))
        .bind(serde_json::to_string(&validator::State::Defined).expect("can serialize")) // ValidatorManager::add_validator
        .execute(dbtx.as_mut())
        .await?;
    }

    Ok(())
}

async fn handle_voting_power_change<'a>(
    dbtx: &mut PgTransaction<'a>,
    ik: IdentityKey,
    voting_power: Amount,
) -> Result<()> {
    // Update the validator's voting power and reset queued delegations/undelegations
    let rows_affected = sqlx::query(
        "UPDATE stake_validator_set 
        SET 
            voting_power = $2
        WHERE ik = $1",
    )
    .bind(ik.to_string())
    .bind(voting_power.value() as i64)
    .execute(dbtx.as_mut())
    .await?
    .rows_affected();

    // Check if the update was successful
    if rows_affected == 0 {
        anyhow::bail!("No validator found with the given identity key");
    }

    Ok(())
}

async fn handle_validator_state_change<'a>(
    dbtx: &mut PgTransaction<'a>,
    ik: IdentityKey,
    state: validator::State,
) -> Result<()> {
    // Update the validator's state
    let rows_affected = sqlx::query(
        "UPDATE stake_validator_set 
        SET 
            validator_state = $2
        WHERE ik = $1",
    )
    .bind(ik.to_string())
    .bind(serde_json::to_string(&state).expect("can serialize"))
    .execute(dbtx.as_mut())
    .await?
    .rows_affected();

    // Check if the update was successful
    if rows_affected == 0 {
        anyhow::bail!("No validator found with the given identity key");
    }

    Ok(())
}
