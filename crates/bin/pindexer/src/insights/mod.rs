use anyhow::Result;
use ethnum::I256;
use std::collections::BTreeMap;

use crate::parsing::parse_content;
use cometindex::{
    async_trait,
    index::{EventBatch, EventBatchContext},
    AppView, PgTransaction,
};
use penumbra_sdk_app::genesis::Content;
use penumbra_sdk_asset::{asset, BASE_ASSET_ID};
use penumbra_sdk_fee::event::EventBlockFees;
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::event::EventDomainType;
use penumbra_sdk_shielded_pool::event::{
    EventInboundFungibleTokenTransfer, EventOutboundFungibleTokenRefund,
    EventOutboundFungibleTokenTransfer,
};

#[derive(Default, Debug, Clone, Copy)]
struct Supply {
    total: u64,
}

async fn modify_supply(
    dbtx: &mut PgTransaction<'_>,
    height: u64,
    f: Box<dyn FnOnce(Supply) -> anyhow::Result<Supply> + Send + 'static>,
) -> anyhow::Result<()> {
    let supply: Supply = {
        let row: Option<i64> =
            sqlx::query_scalar("SELECT total FROM insights_supply ORDER BY HEIGHT DESC LIMIT 1")
                .fetch_optional(dbtx.as_mut())
                .await?;
        row.map(|total| {
            anyhow::Result::<_>::Ok(Supply {
                total: total.try_into()?,
            })
        })
        .transpose()?
        .unwrap_or_default()
    };
    let supply = f(supply)?;
    sqlx::query(
        r#"
        INSERT INTO
            insights_supply(height, total)
            VALUES ($1, $2)
        ON CONFLICT (height) DO UPDATE SET
        total = excluded.total
    "#,
    )
    .bind(i64::try_from(height)?)
    .bind(i64::try_from(supply.total)?)
    .execute(dbtx.as_mut())
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DepositorExisted {
    Yes,
    No,
}

async fn register_depositor(
    dbtx: &mut PgTransaction<'_>,
    asset_id: asset::Id,
    address: &str,
) -> anyhow::Result<DepositorExisted> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM _insights_shielded_pool_depositors WHERE asset_id = $1 AND address = $2)",
    )
    .bind(asset_id.to_bytes())
    .bind(address)
    .fetch_one(dbtx.as_mut())
    .await?;
    if exists {
        return Ok(DepositorExisted::Yes);
    }
    sqlx::query("INSERT INTO _insights_shielded_pool_depositors VALUES ($1, $2)")
        .bind(asset_id.to_bytes())
        .bind(address)
        .execute(dbtx.as_mut())
        .await?;
    Ok(DepositorExisted::No)
}

async fn asset_flow(
    dbtx: &mut PgTransaction<'_>,
    asset_id: asset::Id,
    height: u64,
    flow: I256,
    refund: bool,
    depositor_existed: DepositorExisted,
) -> anyhow::Result<()> {
    let asset_pool: Option<(String, String, i32)> = sqlx::query_as("SELECT total_value, current_value, unique_depositors FROM insights_shielded_pool WHERE asset_id = $1 ORDER BY height DESC LIMIT 1").bind(asset_id.to_bytes()).fetch_optional(dbtx.as_mut()).await?;
    let mut asset_pool = asset_pool
        .map(|(t, c, u)| {
            anyhow::Result::<(I256, I256, i32)>::Ok((
                I256::from_str_radix(&t, 10)?,
                I256::from_str_radix(&c, 10)?,
                u,
            ))
        })
        .transpose()?
        .unwrap_or((I256::ZERO, I256::ZERO, 0i32));
    asset_pool.0 += if refund {
        I256::ZERO
    } else {
        flow.max(I256::ZERO)
    };
    asset_pool.1 += flow;
    asset_pool.2 += match depositor_existed {
        DepositorExisted::Yes => 0,
        DepositorExisted::No => 1,
    };
    sqlx::query(
        r#"
        INSERT INTO insights_shielded_pool
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (asset_id, height) DO UPDATE SET
        total_value = excluded.total_value,
        current_value = excluded.current_value,
        unique_depositors = excluded.unique_depositors
    "#,
    )
    .bind(asset_id.to_bytes())
    .bind(i64::try_from(height)?)
    .bind(asset_pool.0.to_string())
    .bind(asset_pool.1.to_string())
    .bind(asset_pool.2)
    .execute(dbtx.as_mut())
    .await?;
    Ok(())
}

#[derive(Debug)]
pub struct Component;

impl Component {
    pub fn new() -> Self {
        Self
    }
}

/// Add the initial native token supply.
async fn add_genesis_native_token_allocation_supply<'a>(
    dbtx: &mut PgTransaction<'a>,
    content: &Content,
) -> anyhow::Result<()> {
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

    let total = u64::try_from(
        mints
            .get(&*BASE_ASSET_ID)
            .copied()
            .unwrap_or_default()
            .value(),
    )?;

    modify_supply(dbtx, 0, Box::new(move |_| Ok(Supply { total }))).await?;

    Ok(())
}

impl Component {
    async fn index_event(
        &self,
        dbtx: &mut PgTransaction<'_>,
        event: cometindex::ContextualizedEvent<'_>,
    ) -> Result<(), anyhow::Error> {
        let height = event.block_height;
        if let Ok(e) = EventBlockFees::try_from_event(&event.event) {
            let value = e.swapped_fee_total.value();
            if value.asset_id == *BASE_ASSET_ID {
                let amount = u64::try_from(value.amount.value())?;
                // We consider the tip to be destroyed too, matching the current logic
                // DRAGON: if this changes, this code should use the base fee only.
                modify_supply(
                    dbtx,
                    height,
                    Box::new(move |supply| {
                        Ok(Supply {
                            total: supply.total - amount,
                            ..supply
                        })
                    }),
                )
                .await?;
            }
        } else if let Ok(e) = EventInboundFungibleTokenTransfer::try_from_event(&event.event) {
            if e.value.asset_id != *BASE_ASSET_ID {
                let existed = register_depositor(dbtx, e.value.asset_id, &e.sender).await?;
                let flow = I256::try_from(e.value.amount.value())?;
                asset_flow(dbtx, e.value.asset_id, height, flow, false, existed).await?;
            }
        } else if let Ok(e) = EventOutboundFungibleTokenTransfer::try_from_event(&event.event) {
            if e.value.asset_id != *BASE_ASSET_ID {
                let flow = I256::try_from(e.value.amount.value())?;
                // For outbound transfers, never increment unique count
                asset_flow(
                    dbtx,
                    e.value.asset_id,
                    height,
                    -flow,
                    false,
                    DepositorExisted::No,
                )
                .await?;
            }
        } else if let Ok(e) = EventOutboundFungibleTokenRefund::try_from_event(&event.event) {
            if e.value.asset_id != *BASE_ASSET_ID {
                let flow = I256::try_from(e.value.amount.value())?;
                // For outbound transfers, never increment unique count.
                asset_flow(
                    dbtx,
                    e.value.asset_id,
                    height,
                    flow,
                    true,
                    DepositorExisted::No,
                )
                .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl AppView for Component {
    async fn init_chain(
        &self,
        dbtx: &mut PgTransaction,
        app_state: &serde_json::Value,
    ) -> Result<(), anyhow::Error> {
        for statement in include_str!("schema.sql").split(";") {
            sqlx::query(statement).execute(dbtx.as_mut()).await?;
        }

        // decode the initial supply from the genesis
        // initial app state is not recomputed from events, because events are not emitted in init_chain.
        // instead, the indexer directly parses the genesis.
        add_genesis_native_token_allocation_supply(dbtx, &parse_content(app_state.clone())?)
            .await?;
        Ok(())
    }

    fn name(&self) -> String {
        "insights".to_string()
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

    async fn reset(&self, dbtx: &mut PgTransaction) -> Result<(), anyhow::Error> {
        for statement in include_str!("reset.sql").split(";") {
            sqlx::query(statement).execute(dbtx.as_mut()).await?;
        }
        Ok(())
    }
}
