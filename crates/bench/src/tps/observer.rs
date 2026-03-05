use std::convert::TryFrom;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetBlockByHeightRequest,
    GetStatusRequest,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tonic::transport::Channel;

use crate::tps::corpus::tx_hash_hex;

#[derive(Clone, Debug)]
pub struct HeightPlan {
    pub start_height: u64,
    pub warmup_end_height: u64,
    pub end_height: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockRecord {
    pub height: u64,
    pub tx_count: usize,
    pub observed_elapsed_ms: f64,
    pub block_time_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitRecord {
    pub tx_hash_hex: String,
    pub height: u64,
    pub observed_elapsed_ms: f64,
    pub block_time_unix_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ObservationOutput {
    pub plan: HeightPlan,
    pub steady_start_elapsed_ms: f64,
    pub steady_end_elapsed_ms: f64,
    pub blocks: Vec<BlockRecord>,
    pub commits: Vec<CommitRecord>,
}

pub async fn plan_heights(
    observer_endpoint: &str,
    warmup_blocks: u64,
    steady_blocks: u64,
) -> Result<HeightPlan> {
    let mut client = connect(observer_endpoint).await?;
    let status = client
        .get_status(GetStatusRequest {})
        .await
        .context("GetStatus failed while planning heights")?
        .into_inner();
    let start_height = status
        .sync_info
        .map(|s| s.latest_block_height)
        .ok_or_else(|| anyhow::anyhow!("GetStatus response missing sync_info"))?;
    let warmup_end_height = start_height.saturating_add(warmup_blocks);
    let end_height = warmup_end_height.saturating_add(steady_blocks);
    anyhow::ensure!(steady_blocks > 0, "steady_blocks must be > 0");
    Ok(HeightPlan {
        start_height,
        warmup_end_height,
        end_height,
    })
}

pub async fn observe_until_end(
    observer_endpoint: &str,
    plan: HeightPlan,
    t0: Instant,
    height_tx: watch::Sender<u64>,
) -> Result<ObservationOutput> {
    let mut client = connect(observer_endpoint).await?;
    let mut blocks = Vec::new();
    let mut commits = Vec::new();
    let mut last_seen = plan.start_height;
    let mut steady_start_elapsed_ms = if plan.warmup_end_height == plan.start_height {
        t0.elapsed().as_secs_f64() * 1000.0
    } else {
        f64::NAN
    };
    let mut steady_end_elapsed_ms = f64::NAN;

    while last_seen < plan.end_height {
        let latest_height = match tokio::time::timeout(
            Duration::from_secs(3),
            client.get_status(GetStatusRequest {}),
        )
        .await
        {
            Ok(Ok(rsp)) => rsp
                .into_inner()
                .sync_info
                .map(|s| s.latest_block_height)
                .ok_or_else(|| anyhow::anyhow!("GetStatus response missing sync_info"))?,
            Ok(Err(e)) => {
                tracing::warn!(error = ?e, "observer get_status failed; retrying");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            Err(_) => {
                tracing::warn!("observer get_status timeout; retrying");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        let _ = height_tx.send(latest_height);

        let target = latest_height.min(plan.end_height);
        if target <= last_seen {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        }

        for h in (last_seen + 1)..=target {
            let h_i64 =
                i64::try_from(h).context("block height exceeded i64 range for gRPC request")?;
            let rsp = match tokio::time::timeout(
                Duration::from_secs(3),
                client.get_block_by_height(GetBlockByHeightRequest { height: h_i64 }),
            )
            .await
            {
                Ok(Ok(rsp)) => rsp.into_inner(),
                Ok(Err(e)) => {
                    tracing::warn!(height = h, error = ?e, "observer get_block_by_height failed");
                    break;
                }
                Err(_) => {
                    tracing::warn!(height = h, "observer get_block_by_height timeout");
                    break;
                }
            };

            let observed_elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let block = rsp
                .block
                .ok_or_else(|| anyhow::anyhow!("GetBlockByHeight missing block for height {h}"))?;
            let block_time_unix_ms =
                block
                    .header
                    .as_ref()
                    .and_then(|hdr| hdr.time.as_ref())
                    .map(|ts| {
                        ts.seconds
                            .saturating_mul(1_000)
                            .saturating_add((ts.nanos as i64) / 1_000_000)
                    });
            let txs: Vec<Vec<u8>> = block.data.map(|d| d.txs).unwrap_or_default();
            let tx_count = txs.len();

            for tx in txs {
                commits.push(CommitRecord {
                    tx_hash_hex: tx_hash_hex(&tx),
                    height: h,
                    observed_elapsed_ms,
                    block_time_unix_ms,
                });
            }

            blocks.push(BlockRecord {
                height: h,
                tx_count,
                observed_elapsed_ms,
                block_time_unix_ms,
            });

            if h == plan.warmup_end_height && !steady_start_elapsed_ms.is_finite() {
                steady_start_elapsed_ms = observed_elapsed_ms;
            }
            if h == plan.end_height {
                steady_end_elapsed_ms = observed_elapsed_ms;
            }
            last_seen = h;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if !steady_start_elapsed_ms.is_finite() {
        steady_start_elapsed_ms = 0.0;
    }
    if !steady_end_elapsed_ms.is_finite() {
        steady_end_elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    Ok(ObservationOutput {
        plan,
        steady_start_elapsed_ms,
        steady_end_elapsed_ms,
        blocks,
        commits,
    })
}

async fn connect(endpoint: &str) -> Result<TendermintProxyServiceClient<Channel>> {
    TendermintProxyServiceClient::connect(endpoint.to_string())
        .await
        .with_context(|| format!("failed to connect observer endpoint {endpoint}"))
}
