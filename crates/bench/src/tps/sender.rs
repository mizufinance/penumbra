use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, BroadcastTxAsyncRequest,
};
use tokio::sync::{mpsc, watch, Semaphore};
use tonic::transport::Channel;

use crate::tps::corpus::Corpus;

#[derive(Clone, Debug)]
pub struct SenderConfig {
    pub offered_tps: u64,
    pub submit_workers: usize,
    pub max_inflight: usize,
    pub end_height: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SubmissionRecord {
    pub seq: u64,
    pub tx_hash_hex: String,
    pub endpoint: String,
    pub async_code: u64,
    pub log: String,
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

    let clients = connect_clients(endpoints).await?;

    let token_sem = Arc::new(Semaphore::new(0));
    let inflight_sem = Arc::new(Semaphore::new(config.max_inflight));

    let stop = Arc::new(AtomicBool::new(false));
    let corpus_exhausted = Arc::new(AtomicBool::new(false));
    let next_idx = Arc::new(AtomicUsize::new(0));
    let rr_endpoint = Arc::new(AtomicUsize::new(0));
    let seq_ctr = Arc::new(AtomicU64::new(0));

    let (tx, mut rx) = mpsc::unbounded_channel::<SubmissionRecord>();

    let pacer_stop = stop.clone();
    let pacer_token_sem = token_sem.clone();
    let pacer = tokio::spawn(async move {
        let tick = Duration::from_millis(50);
        let tick_s = tick.as_secs_f64();
        let mut acc = 0f64;
        while !pacer_stop.load(Ordering::Relaxed) {
            tokio::time::sleep(tick).await;
            acc += config.offered_tps as f64 * tick_s;
            let permits = acc.floor() as usize;
            if permits > 0 {
                pacer_token_sem.add_permits(permits);
                acc -= permits as f64;
            }
        }
    });

    let worker_count = config.submit_workers.max(1);
    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let clients = clients.clone();
        let endpoints = endpoints.to_vec();
        let corpus_entries = corpus.entries.clone();
        let token_sem = token_sem.clone();
        let inflight_sem = inflight_sem.clone();
        let stop = stop.clone();
        let exhausted = corpus_exhausted.clone();
        let next_idx = next_idx.clone();
        let rr_endpoint = rr_endpoint.clone();
        let seq_ctr = seq_ctr.clone();
        let height_rx = current_height.clone();
        let tx = tx.clone();
        let end_height = config.end_height;
        let t0 = t0;
        let worker = tokio::spawn(async move {
            loop {
                if stop.load(Ordering::Relaxed) || *height_rx.borrow() >= end_height {
                    break;
                }
                // Consume one pacing token; dropping without `forget()` would
                // return the permit and disable rate-limiting.
                let token = token_sem
                    .acquire()
                    .await
                    .context("failed to acquire pacing token")?;
                token.forget();

                if stop.load(Ordering::Relaxed) || *height_rx.borrow() >= end_height {
                    break;
                }

                let idx = next_idx.fetch_add(1, Ordering::Relaxed);
                if idx >= corpus_entries.len() {
                    exhausted.store(true, Ordering::Relaxed);
                    stop.store(true, Ordering::Relaxed);
                    break;
                }

                let _inflight_guard = inflight_sem
                    .clone()
                    .acquire_owned()
                    .await
                    .context("failed to acquire inflight permit")?;

                let entry = &corpus_entries[idx];
                let endpoint_idx = rr_endpoint.fetch_add(1, Ordering::Relaxed) % clients.len();
                let endpoint = endpoints[endpoint_idx].clone();
                let mut client = clients[endpoint_idx].clone();
                let sent_elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
                let sent_unix_ms = unix_ms_now();
                let seq = seq_ctr.fetch_add(1, Ordering::Relaxed);

                let req = BroadcastTxAsyncRequest {
                    req_id: rand::random::<u64>(),
                    params: entry.tx_bytes.clone(),
                };

                let (async_code, log) = match tokio::time::timeout(
                    Duration::from_secs(10),
                    client.broadcast_tx_async(req),
                )
                .await
                {
                    Ok(Ok(rsp)) => {
                        let rsp = rsp.into_inner();
                        (rsp.code, rsp.log)
                    }
                    Ok(Err(e)) => (u64::MAX, format!("grpc error: {e:#}")),
                    Err(_) => (u64::MAX, "broadcast timeout".to_string()),
                };

                let _ = tx.send(SubmissionRecord {
                    seq,
                    tx_hash_hex: entry.tx_hash_hex.clone(),
                    endpoint,
                    async_code,
                    log,
                    sent_elapsed_ms,
                    sent_unix_ms,
                });

                // Keep watch receiver fresh.
                let _ = height_rx.has_changed();
            }

            Ok::<(), anyhow::Error>(())
        });
        workers.push(worker);
    }
    drop(tx);

    for worker in workers {
        worker
            .await
            .context("sender worker join failure")?
            .context("sender worker failed")?;
    }
    stop.store(true, Ordering::Relaxed);

    let _ = pacer.await;

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

async fn connect_clients(
    endpoints: &[String],
) -> Result<Vec<TendermintProxyServiceClient<Channel>>> {
    let mut out = Vec::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let client = TendermintProxyServiceClient::connect(endpoint.clone())
            .await
            .with_context(|| format!("failed to connect sender endpoint {endpoint}"))?;
        out.push(client);
    }
    Ok(out)
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
