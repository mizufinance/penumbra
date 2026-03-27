use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use penumbra_sdk_proto::{
    penumbra::util::node::v1::{node_service_client::NodeServiceClient, SubmitTxRequest},
    util::tendermint_proxy::v1::{
        tendermint_proxy_service_client::TendermintProxyServiceClient, BroadcastTxAsyncRequest,
        BroadcastTxSyncRequest,
    },
};
use tokio::sync::{mpsc, watch, Semaphore};
use tokio::task::JoinSet;
use tonic::transport::Channel;

use crate::tps::config::{BurstProfile, EndpointKind};
use crate::tps::corpus::Corpus;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubmitMode {
    Async,
    Sync,
}

impl Default for SubmitMode {
    fn default() -> Self {
        Self::Async
    }
}

impl SubmitMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Async => "async",
            Self::Sync => "sync",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SenderConfig {
    pub offered_tps: u64,
    pub submit_workers: usize,
    pub max_inflight: usize,
    pub end_height: u64,
    pub submit_mode: SubmitMode,
    pub endpoint_kind: EndpointKind,
    pub pacer_tick_ms: u64,
    pub disable_pacer: bool,
    pub burst_profile: Option<BurstProfile>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SubmissionRecord {
    pub seq: u64,
    pub tx_hash_hex: String,
    pub endpoint: String,
    pub submit_mode: String,
    pub response_code: u64,
    pub response_log: String,
    pub async_code: u64,
    pub log: String,
    pub pacing_wait_ms: f64,
    pub inflight_wait_ms: f64,
    pub rpc_wait_ms: f64,
    pub sent_elapsed_ms: f64,
    pub sent_unix_ms: u64,
}

#[derive(Clone, Debug)]
pub struct SenderOutput {
    pub submissions: Vec<SubmissionRecord>,
    pub corpus_exhausted: bool,
}

pub async fn run_sender(
    endpoints: &[String],
    corpus: &Corpus,
    config: SenderConfig,
    t0: Instant,
    current_height: watch::Receiver<u64>,
) -> Result<SenderOutput> {
    anyhow::ensure!(
        !endpoints.is_empty(),
        "run_sender requires at least one pd endpoint"
    );
    anyhow::ensure!(
        !corpus.entries.is_empty(),
        "run_sender requires a non-empty corpus"
    );
    anyhow::ensure!(config.offered_tps > 0, "offered_tps must be > 0");
    anyhow::ensure!(config.max_inflight > 0, "max_inflight must be > 0");
    anyhow::ensure!(
        config.disable_pacer || config.pacer_tick_ms > 0,
        "pacer_tick_ms must be > 0 unless pacing is disabled"
    );
    if let Some(burst_profile) = &config.burst_profile {
        anyhow::ensure!(
            burst_profile.burst_tx_count > 0 && burst_profile.burst_duration_ms > 0,
            "burst_profile must have positive tx count and duration"
        );
    }

    let token_sem = Arc::new(Semaphore::new(0));
    let inflight_sem = Arc::new(Semaphore::new(config.max_inflight));

    let stop = Arc::new(AtomicBool::new(false));
    let corpus_exhausted = Arc::new(AtomicBool::new(false));
    let seq_ctr = Arc::new(AtomicU64::new(0));
    let next_client_idx = Arc::new(AtomicUsize::new(0));

    let (tx, mut rx) = mpsc::unbounded_channel::<SubmissionRecord>();

    let pacer_stop = stop.clone();
    let pacer_token_sem = token_sem.clone();
    let pacer = if config.disable_pacer {
        pacer_token_sem.add_permits(corpus.entries.len().max(config.max_inflight));
        None
    } else {
        Some(tokio::spawn(async move {
            let tick = Duration::from_millis(config.pacer_tick_ms);
            let tick_s = tick.as_secs_f64();
            let mut acc = 0f64;
            let burst_started_at = Instant::now();
            while !pacer_stop.load(Ordering::Relaxed) {
                tokio::time::sleep(tick).await;
                let current_offered_tps = if let Some(burst_profile) = &config.burst_profile {
                    let burst_duration = Duration::from_millis(burst_profile.burst_duration_ms);
                    if burst_started_at.elapsed() < burst_duration {
                        burst_profile.burst_tx_count as f64 / burst_duration.as_secs_f64()
                    } else {
                        config.offered_tps as f64
                    }
                } else {
                    config.offered_tps as f64
                };
                acc += current_offered_tps * tick_s;
                let permits = acc.floor() as usize;
                if permits > 0 {
                    pacer_token_sem.add_permits(permits);
                    acc -= permits as f64;
                }
            }
        }))
    };

    let client_pool_size = config.submit_workers.max(1);
    let clients =
        connect_clients_for_workers(endpoints, client_pool_size, &config.endpoint_kind).await?;
    let corpus_entries = corpus.entries.clone();
    let mut in_flight = JoinSet::new();
    let mut next_idx = 0usize;

    loop {
        while in_flight.len() < config.max_inflight {
            if stop.load(Ordering::Relaxed) || *current_height.borrow() >= config.end_height {
                break;
            }

            let pacing_wait_start = Instant::now();
            if !config.disable_pacer {
                let token = token_sem
                    .acquire()
                    .await
                    .context("failed to acquire pacing token")?;
                token.forget();
            }
            let pacing_wait_ms = pacing_wait_start.elapsed().as_secs_f64() * 1000.0;

            if stop.load(Ordering::Relaxed) || *current_height.borrow() >= config.end_height {
                break;
            }

            if next_idx >= corpus_entries.len() {
                corpus_exhausted.store(true, Ordering::Relaxed);
                stop.store(true, Ordering::Relaxed);
                break;
            }

            let inflight_wait_start = Instant::now();
            let inflight_guard = inflight_sem
                .clone()
                .acquire_owned()
                .await
                .context("failed to acquire inflight permit")?;
            let inflight_wait_ms = inflight_wait_start.elapsed().as_secs_f64() * 1000.0;

            let entry = corpus_entries[next_idx].clone();
            next_idx += 1;
            let sent_elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let sent_unix_ms = unix_ms_now();
            let seq = seq_ctr.fetch_add(1, Ordering::Relaxed);
            let client_idx = next_client_idx.fetch_add(1, Ordering::Relaxed) % clients.len();
            let mut client = clients[client_idx].clone();
            let endpoint = endpoints[client_idx % endpoints.len()].clone();
            let tx = tx.clone();
            let submit_mode = config.submit_mode.clone();

            in_flight.spawn(async move {
                let _inflight_guard = inflight_guard;
                let rpc_wait_start = Instant::now();
                let (response_code, response_log) = match tokio::time::timeout(
                    Duration::from_secs(10),
                    client.submit(submit_mode.clone(), entry.tx_bytes.clone()),
                )
                .await
                {
                    Ok(Ok(outcome)) => outcome,
                    Ok(Err(e)) => (u64::MAX, format!("grpc error: {e:#}")),
                    Err(_) => (u64::MAX, "broadcast timeout".to_string()),
                };
                let rpc_wait_ms = rpc_wait_start.elapsed().as_secs_f64() * 1000.0;

                let _ = tx.send(SubmissionRecord {
                    seq,
                    tx_hash_hex: entry.tx_hash_hex.clone(),
                    endpoint,
                    submit_mode: submit_mode.as_str().to_string(),
                    response_code,
                    response_log: response_log.clone(),
                    async_code: response_code,
                    log: response_log,
                    pacing_wait_ms,
                    inflight_wait_ms,
                    rpc_wait_ms,
                    sent_elapsed_ms,
                    sent_unix_ms,
                });

                Ok::<(), anyhow::Error>(())
            });
        }

        if in_flight.is_empty() {
            break;
        }

        in_flight
            .join_next()
            .await
            .context("sender submission join failure")??
            .context("sender submission failed")?;
    }
    stop.store(true, Ordering::Relaxed);
    drop(tx);

    if let Some(pacer) = pacer {
        let _ = pacer.await;
    }

    let mut submissions = Vec::new();
    while let Some(item) = rx.recv().await {
        submissions.push(item);
    }
    submissions.sort_by(|a, b| a.seq.cmp(&b.seq));

    // Consume latest height updates to avoid unused warning for parameter.
    let _ = current_height.has_changed();

    Ok(SenderOutput {
        submissions,
        corpus_exhausted: corpus_exhausted.load(Ordering::Relaxed),
    })
}

async fn connect_clients_for_workers(
    endpoints: &[String],
    worker_count: usize,
    endpoint_kind: &EndpointKind,
) -> Result<Vec<SubmitClient>> {
    let mut out = Vec::with_capacity(worker_count);
    for worker_idx in 0..worker_count {
        let endpoint = &endpoints[worker_idx % endpoints.len()];
        let client = SubmitClient::connect(endpoint, endpoint_kind).await?;
        out.push(client);
    }
    Ok(out)
}

#[derive(Clone)]
enum SubmitClient {
    TendermintProxy(TendermintProxyServiceClient<Channel>),
    NodeService(NodeServiceClient<Channel>),
}

impl SubmitClient {
    async fn connect(endpoint: &str, endpoint_kind: &EndpointKind) -> Result<Self> {
        match endpoint_kind {
            EndpointKind::TendermintProxy => Ok(Self::TendermintProxy(
                TendermintProxyServiceClient::connect(endpoint.to_string())
                    .await
                    .with_context(|| {
                        format!("failed to connect sender worker endpoint {endpoint}")
                    })?,
            )),
            EndpointKind::NodeService => Ok(Self::NodeService(
                NodeServiceClient::connect(endpoint.to_string())
                    .await
                    .with_context(|| {
                        format!("failed to connect sender worker endpoint {endpoint}")
                    })?,
            )),
        }
    }

    async fn submit(&mut self, mode: SubmitMode, tx_bytes: Vec<u8>) -> Result<(u64, String)> {
        match self {
            SubmitClient::TendermintProxy(client) => match mode {
                SubmitMode::Async => {
                    let rsp = client
                        .broadcast_tx_async(BroadcastTxAsyncRequest {
                            req_id: rand::random::<u64>(),
                            params: tx_bytes,
                        })
                        .await?
                        .into_inner();
                    Ok((rsp.code, rsp.log))
                }
                SubmitMode::Sync => {
                    let rsp = client
                        .broadcast_tx_sync(BroadcastTxSyncRequest {
                            req_id: rand::random::<u64>(),
                            params: tx_bytes,
                        })
                        .await?
                        .into_inner();
                    Ok((rsp.code, rsp.log))
                }
            },
            SubmitClient::NodeService(client) => {
                let rsp = client
                    .submit_tx(SubmitTxRequest { tx: tx_bytes })
                    .await?
                    .into_inner();
                Ok((rsp.code as u64, rsp.log))
            }
        }
    }
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_simulation_is_within_two_percent() {
        let issued = simulate_bucket(200, Duration::from_millis(50), Duration::from_secs(10));
        let expected = 2000.0;
        let error_pct = ((issued as f64 - expected).abs() / expected) * 100.0;
        assert!(
            error_pct <= 2.0,
            "token bucket drift too high: {error_pct:.2}%"
        );
    }

    #[test]
    fn round_robin_distribution_is_balanced() {
        let endpoints = 4usize;
        let sends = 1000usize;
        let mut counts = vec![0usize; endpoints];
        for i in 0..sends {
            counts[i % endpoints] += 1;
        }
        let min = *counts.iter().min().expect("min");
        let max = *counts.iter().max().expect("max");
        assert!(max - min <= 1, "distribution should be near-even");
    }

    fn simulate_bucket(rate_per_sec: u64, tick: Duration, total: Duration) -> usize {
        let mut acc = 0f64;
        let mut emitted = 0usize;
        let mut elapsed = Duration::ZERO;
        while elapsed < total {
            elapsed += tick;
            acc += rate_per_sec as f64 * tick.as_secs_f64();
            let permits = acc.floor() as usize;
            if permits > 0 {
                emitted += permits;
                acc -= permits as f64;
            }
        }
        emitted
    }
}
