use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use cometindex::{
    async_trait,
    index::{EventBatch, EventBatchContext},
    sqlx, AppView, ContextualizedEvent, PgTransaction,
};
use penumbra_sdk_app::genesis::Content;
use penumbra_sdk_asset::{asset, BASE_ASSET_ID};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::event::ProtoEvent;
use penumbra_sdk_proto::penumbra::core::component::fee::v1 as pb_fee;
use sqlx::{Postgres, Transaction};

use crate::parsing::parse_content;

mod unstaked_supply {
    use anyhow::Result;
    use cometindex::PgTransaction;

    pub async fn init_db(dbtx: &mut PgTransaction<'_>) -> Result<()> {
        sqlx::query(
            r#"
        CREATE TABLE IF NOT EXISTS supply_total_unstaked (
            height BIGINT PRIMARY KEY,
            um BIGINT NOT NULL,
            fees BIGINT NOT NULL
        );
        "#,
        )
        .execute(dbtx.as_mut())
        .await?;
        Ok(())
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Supply {
        pub um: u64,
        pub fees: u64,
    }

    async fn get_supply(dbtx: &mut PgTransaction<'_>, height: u64) -> Result<Option<Supply>> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT um, fees FROM supply_total_unstaked WHERE height <= $1 ORDER BY height DESC LIMIT 1",
        )
        .bind(i64::try_from(height)?)
        .fetch_optional(dbtx.as_mut())
        .await?;
        match row {
            None => Ok(None),
            Some((um, fees)) => Ok(Some(Supply {
                um: um.try_into()?,
                fees: fees.try_into()?,
            })),
        }
    }

    async fn set_supply(dbtx: &mut PgTransaction<'_>, height: u64, supply: Supply) -> Result<()> {
        sqlx::query(
            r#"
        INSERT INTO
            supply_total_unstaked
        VALUES ($1, $2, $3)
        ON CONFLICT (height)
        DO UPDATE SET
            um = excluded.um,
            fees = excluded.fees
        "#,
        )
        .bind(i64::try_from(height)?)
        .bind(i64::try_from(supply.um)?)
        .bind(i64::try_from(supply.fees)?)
        .execute(dbtx.as_mut())
        .await?;
        Ok(())
    }

    pub async fn modify(
        dbtx: &mut PgTransaction<'_>,
        height: u64,
        f: impl FnOnce(Option<Supply>) -> Result<Supply>,
    ) -> Result<()> {
        let supply = get_supply(dbtx, height).await?;
        let new_supply = f(supply)?;
        set_supply(dbtx, height, new_supply).await
    }
}

#[derive(Clone, Debug)]
enum Event {
    BlockFees {
        height: u64,
        total: penumbra_sdk_fee::Fee,
    },
}

impl Event {
    const NAMES: [&'static str; 1] = ["penumbra.core.component.fee.v1.EventBlockFees"];

    async fn index<'d>(&self, dbtx: &mut Transaction<'d, Postgres>) -> anyhow::Result<()> {
        match self {
            Event::BlockFees { height, total } => {
                if total.asset_id() != *BASE_ASSET_ID {
                    return Ok(());
                }
                let amount = u64::try_from(total.amount().value())?;
                if amount == 0 {
                    return Ok(());
                }
                unstaked_supply::modify(dbtx, *height, |current| {
                    let current = current.unwrap_or_default();
                    Ok(unstaked_supply::Supply {
                        um: current.um - amount,
                        fees: current.fees + amount,
                        ..current
                    })
                })
                .await
            }
        }
    }
}

impl TryFrom<ContextualizedEvent<'_>> for Event {
    type Error = anyhow::Error;

    fn try_from(event: ContextualizedEvent<'_>) -> Result<Self, Self::Error> {
        match event.event.kind.as_str() {
            x if x == Event::NAMES[0] => {
                let pe = pb_fee::EventBlockFees::from_event(event.as_ref())?;
                let total = pe
                    .swapped_fee_total
                    .ok_or(anyhow!("EventBlockFees missing swapped_fee_total"))?
                    .try_into()?;
                Ok(Self::BlockFees {
                    height: event.block_height,
                    total,
                })
            }
            x => Err(anyhow!(format!("unrecognized event kind: {x}"))),
        }
    }
}

async fn add_genesis_native_token_allocation_supply<'a>(
    dbtx: &mut PgTransaction<'a>,
    content: &Content,
) -> Result<()> {
    fn content_mints(content: &Content) -> BTreeMap<asset::Id, Amount> {
        let allocation_mints = content
            .shielded_pool_content
            .allocations
            .iter()
            .map(|allocation| {
                let value = allocation.value();
                (value.asset_id, value.amount)
            });

        let mut out = BTreeMap::new();
        for (id, amount) in allocation_mints {
            out.entry(id).and_modify(|x| *x += amount).or_insert(amount);
        }
        out
    }

    let mints = content_mints(content);

    let unstaked_mint = u64::try_from(
        mints
            .get(&*BASE_ASSET_ID)
            .copied()
            .unwrap_or_default()
            .value(),
    )?;
    unstaked_supply::modify(dbtx, 0, |_| {
        Ok(unstaked_supply::Supply {
            um: unstaked_mint,
            fees: 0,
        })
    })
    .await?;

    Ok(())
}

#[derive(Debug)]
pub struct Component {}

impl Component {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AppView for Component {
    fn name(&self) -> String {
        "supply".to_string()
    }

    async fn init_chain(
        &self,
        dbtx: &mut PgTransaction,
        app_state: &serde_json::Value,
    ) -> Result<(), anyhow::Error> {
        unstaked_supply::init_db(dbtx).await?;
        add_genesis_native_token_allocation_supply(dbtx, &parse_content(app_state.clone())?)
            .await?;
        Ok(())
    }

    async fn index_batch(
        &self,
        dbtx: &mut PgTransaction,
        batch: EventBatch,
        _ctx: EventBatchContext,
    ) -> Result<(), anyhow::Error> {
        for event in batch.events() {
            let e = match Event::try_from(event) {
                Ok(e) => e,
                Err(_) => continue,
            };
            e.index(dbtx).await?;
        }
        Ok(())
    }
}
