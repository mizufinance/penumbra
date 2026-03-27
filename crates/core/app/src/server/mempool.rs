use anyhow::Result;
use futures::FutureExt;
use sha2::Digest as _;

use cnidarium::{Snapshot, Storage};

use tendermint::v0_37::abci::{
    request::CheckTx as CheckTxReq, request::CheckTxKind, response::CheckTx as CheckTxRsp,
    MempoolRequest as Request, MempoolResponse as Response,
};
use tokio::sync::mpsc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tower_actor::Message;
use tracing::Instrument;

use std::sync::Arc;
use std::{any::Any, panic::AssertUnwindSafe};

use crate::{
    app::{App, CheckTxSharedContext},
    block_tx_indexing::BlockTxIndexingMode,
    metrics,
    stateless_cache::StatelessCache,
};

const DEFAULT_MAX_IN_FLIGHT_CHECKTX: usize = 8;

fn max_in_flight_checktx() -> usize {
    std::env::var("PENUMBRA_MEMPOOL_CHECKTX_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|parallelism| parallelism.get().max(DEFAULT_MAX_IN_FLIGHT_CHECKTX))
                .unwrap_or(DEFAULT_MAX_IN_FLIGHT_CHECKTX)
        })
}

fn max_in_flight_checktx_heavywork() -> usize {
    std::env::var("PENUMBRA_MEMPOOL_CHECKTX_HEAVYWORK_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|parallelism| std::cmp::max(1, parallelism.get() / 2))
                .unwrap_or(1)
        })
}

/// A mempool service that applies transaction checks against an isolated application fork.
pub struct Mempool {
    queue: mpsc::Receiver<Message<Request, Response, tower::BoxError>>,
    storage: Storage,
    stateless_cache: Arc<StatelessCache>,
}

impl Mempool {
    fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_string()
        } else if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else {
            "non-string panic payload".to_string()
        }
    }

    pub fn new(
        storage: Storage,
        stateless_cache: Arc<StatelessCache>,
        queue: mpsc::Receiver<Message<Request, Response, tower::BoxError>>,
    ) -> Self {
        Self {
            queue,
            storage,
            stateless_cache,
        }
    }

    async fn check_tx_with_state(
        snapshot: Snapshot,
        checktx_shared_context: Option<Arc<CheckTxSharedContext>>,
        stateless_cache: Arc<StatelessCache>,
        req: Request,
    ) -> Result<Response, tower::BoxError> {
        let Request::CheckTx(CheckTxReq {
            tx: tx_bytes, kind, ..
        }) = req;

        let start = tokio::time::Instant::now();
        let kind_str = match kind {
            CheckTxKind::New => "new",
            CheckTxKind::Recheck => "recheck",
        };

        let mut app = App::new(snapshot);
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        if let Some(checktx_shared_context) = checktx_shared_context {
            app.set_checktx_shared_context(checktx_shared_context);
        }

        match app
            .deliver_tx_bytes_v2_profiled(tx_bytes.as_ref(), Some(stateless_cache.as_ref()))
            .await
        {
            Ok((events, profile)) => {
                let elapsed = start.elapsed();
                App::emit_checktx_breakdown(&profile);
                tracing::info!(
                    ?elapsed,
                    execute_ms = profile.execute_ms,
                    check_historical_ms = profile.check_historical_ms,
                    nullifier_lookup_count = profile.execute_nullifier_lookup_count,
                    nullifier_lookup_wall_ms = profile.execute_spend_nullifier_committed_check_ms,
                    "tx accepted"
                );
                metrics::histogram!(
                    metrics::MEMPOOL_CHECKTX_DURATION,
                    "kind" => kind_str,
                    "code" => "0"
                )
                .record(elapsed);
                metrics::counter!(metrics::MEMPOOL_CHECKTX_TOTAL, "kind" => kind_str, "code" => "0").increment(1);
                Ok(Response::CheckTx(CheckTxRsp {
                    events,
                    ..Default::default()
                }))
            }
            Err(e) => {
                let elapsed = start.elapsed();
                tracing::info!(?e, ?elapsed, "tx rejected");
                metrics::histogram!(
                    metrics::MEMPOOL_CHECKTX_DURATION,
                    "kind" => kind_str,
                    "code" => "1"
                )
                .record(elapsed);
                metrics::counter!(metrics::MEMPOOL_CHECKTX_TOTAL, "kind" => kind_str, "code" => "1").increment(1);
                Ok(Response::CheckTx(CheckTxRsp {
                    code: 1.into(),
                    // Use the alternate format specifier to include the chain of error causes.
                    log: format!("{e:#}"),
                    ..Default::default()
                }))
            }
        }
    }

    pub async fn run(mut self) -> Result<(), tower::BoxError> {
        let max_in_flight = max_in_flight_checktx();
        let max_in_flight_heavywork = max_in_flight_checktx_heavywork();
        tracing::info!(
            max_in_flight,
            max_in_flight_heavywork,
            "mempool service started"
        );
        let mut in_flight = JoinSet::new();
        let permits = Arc::new(Semaphore::new(max_in_flight));
        let heavywork_permits = Arc::new(Semaphore::new(max_in_flight_heavywork));
        let storage = self.storage.clone();
        let stateless_cache = self.stateless_cache.clone();
        let mut queue_closed = false;

        let mut snapshot_rx = storage.subscribe();
        let initial_snapshot = snapshot_rx.borrow().clone();
        let initial_ctx = match CheckTxSharedContext::load(&initial_snapshot).await {
            Ok(ctx) => Some(Arc::new(ctx)),
            Err(error) => {
                tracing::warn!(?error, "CheckTxSharedContext unavailable at mempool startup; falling back to legacy CheckTx path until a later snapshot succeeds");
                None
            }
        };
        let (ctx_tx, ctx_rx) = tokio::sync::watch::channel(initial_ctx);

        tokio::spawn(async move {
            loop {
                if snapshot_rx.changed().await.is_err() {
                    break;
                }
                let snapshot = snapshot_rx.borrow_and_update().clone();
                let version = snapshot.version();
                match CheckTxSharedContext::load(&snapshot).await {
                    Ok(ctx) => {
                        let _ = ctx_tx.send(Some(Arc::new(ctx)));
                    }
                    Err(e) => {
                        tracing::warn!(?version, "CheckTxSharedContext::load failed: {e:#}");
                    }
                }
            }
        });

        loop {
            tokio::select! {
                Some(joined) = in_flight.join_next(), if !in_flight.is_empty() => {
                    if let Err(join_error) = joined {
                        tracing::error!(?join_error, "check_tx task terminated unexpectedly after panic handling");
                    }
                }
                message = self.queue.recv(), if !queue_closed => {
                    match message {
                        Some(Message { req, rsp_sender, span }) => {
                            let received_at = tokio::time::Instant::now();
                            let tx_hash: Option<[u8; 32]> = match &req {
                                Request::CheckTx(CheckTxReq { tx, kind, .. }) => {
                                    let _ = kind;
                                    Some(sha2::Sha256::digest(tx.as_ref()).into())
                                }
                            };
                            let tx_size_bytes = match &req {
                                Request::CheckTx(CheckTxReq { tx, .. }) => tx.len(),
                            };
                            let current_checktx_context = ctx_rx.borrow().clone();
                            tracing::info!(
                                parent: &span,
                                tx_size_bytes,
                                "checktx_frontdoor_received"
                            );
                            metrics::gauge!(metrics::MEMPOOL_CHECKTX_PENDING).increment(1.0);
                            let permit_wait_started = tokio::time::Instant::now();
                            let permit = permits
                                .clone()
                                .acquire_owned()
                                .await
                                .expect("mempool semaphore should not be closed");
                            tracing::info!(
                                parent: &span,
                                permit_wait_ms = permit_wait_started.elapsed().as_secs_f64() * 1000.0,
                                "checktx_frontdoor_permit_acquired"
                            );
                            let heavywork_permits = heavywork_permits.clone();
                            let snapshot = storage.latest_snapshot();
                            let checktx_shared_context = current_checktx_context;
                            let stateless_cache = stateless_cache.clone();
                            let stateless_cache_for_check = stateless_cache.clone();
                            in_flight.spawn(async move {
                                let _permit = permit;
                                let heavywork_wait_started = tokio::time::Instant::now();
                                let _heavywork_permit = heavywork_permits
                                    .acquire_owned()
                                    .await
                                    .expect("mempool heavywork semaphore should not be closed");
                                tracing::info!(
                                    heavywork_wait_ms = heavywork_wait_started.elapsed().as_secs_f64() * 1000.0,
                                    frontdoor_elapsed_ms = received_at.elapsed().as_secs_f64() * 1000.0,
                                    "checktx_frontdoor_heavywork_acquired"
                                );
                                metrics::gauge!(metrics::MEMPOOL_CHECKTX_IN_FLIGHT).increment(1.0);
                                let execute_started = tokio::time::Instant::now();
                                let result = AssertUnwindSafe(Self::check_tx_with_state(
                                        snapshot,
                                        checktx_shared_context,
                                        stateless_cache_for_check,
                                        req,
                                    ))
                                    .catch_unwind()
                                    .await;
                                let _ = (tx_hash, stateless_cache);
                                let tx_hash_hex = tx_hash.map(hex::encode);
                                let result = match result {
                                    Ok(result) => result,
                                    Err(panic_payload) => {
                                        let panic_message = Self::panic_payload_message(&*panic_payload);
                                        tracing::error!(
                                            tx_hash = tx_hash_hex.as_deref().unwrap_or("unknown"),
                                            tx_size_bytes,
                                            %panic_message,
                                            "checktx task panicked; rejecting transaction instead of terminating mempool actor"
                                        );
                                        Ok(Response::CheckTx(CheckTxRsp {
                                            code: 1.into(),
                                            log: format!(
                                                "internal CheckTx panic: {panic_message}"
                                            ),
                                            ..Default::default()
                                        }))
                                    }
                                };
                                tracing::info!(
                                    execute_wall_ms = execute_started.elapsed().as_secs_f64() * 1000.0,
                                    frontdoor_total_ms = received_at.elapsed().as_secs_f64() * 1000.0,
                                    response_ok = result.is_ok(),
                                    "checktx_frontdoor_finished"
                                );
                                metrics::gauge!(metrics::MEMPOOL_CHECKTX_IN_FLIGHT).decrement(1.0);
                                metrics::gauge!(metrics::MEMPOOL_CHECKTX_PENDING).decrement(1.0);
                                let _ = rsp_sender.send(result);
                            }.instrument(span));
                        }
                        None => {
                            tracing::warn!("mempool request queue closed");
                            queue_closed = true;
                        }
                    }
                }
                else => break,
            }

            if queue_closed && in_flight.is_empty() {
                tracing::warn!(
                    "mempool service loop exiting after queue closure and draining inflight work"
                );
                break;
            }
        }
        tracing::info!("mempool service stopped");
        Ok(())
    }
}
